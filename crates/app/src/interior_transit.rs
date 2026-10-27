//! # M18 B2-b — world → interior → world through a door
//!
//! The spine's scene transition (contract:
//! `docs/sintergic/game/programming/spine-contracts-CR-f716455c.md` §4) as the
//! **only** path from the world into an interior and back, plus the door
//! activation that feeds it (bindings already declared in
//! `m2_player::ActionState`: `E` = interact, `G` = bash).
//!
//! ## What this module owns (and what it does NOT)
//!
//! | Owns | Does NOT own |
//! |:---|:---|
//! | `SceneTarget` / `TransitionState` / `SceneTransition` (§4, byte-for-byte) | loading geometry — that is a [`SceneLoader`] supplied by `app` |
//! | the door registry and the reach test | building meshes, collision grids, RDB parsing (bridge/M11) |
//! | the transit state machine (enter → exit, 0 double load) | rendering, input devices (`ButtonInput` stays in `m2_player`) |
//!
//! **Contract fidelity:** the five items of §4 keep their exact signatures.
//! `begin` / `complete` / `fail` are an **R-8 addendum, ratified by
//! M34-SPINE (W34-0, `CR-2d5c8f41`)**: §4 assigns the load to `app` ("la carga
//! es responsabilidad de `app`") but exposes no way to report its outcome, so
//! `app` cannot reach `Done` or `Failed` without them. They are additive (0
//! signature changes) and 0 game rule is added. The negative control of
//! `INTERIOR-TRANSIT-001` shows the state machine is complete with them, and
//! `SPINE-001` control C4 observes the three paths (`begin`, `complete`,
//! `fail`) in the M34 gate probe.
//!
//! **No new game rules:** the reach distance comes from the demo's own
//! `DOOR_REACH_M = 2.5 m` (`m18_window.rs:205`, M28 retail-door reach); bash vs
//! interact only selects WHICH key opened the door — what the bash does to a
//! lock is M28's business, not this module's.


//! ## Where the §4 law types live (I1 F1)
//!
//! `SceneError` / `TransitionState` / `SceneTransition` moved to
//! `daggerfall_game_core::scene` (plan §3.1.1: they were de-facto law trapped
//! in the binary). They are re-exported here, so every consumer keeps its
//! exact path with 0 duplicated definitions — the same pattern this module
//! already uses for `SceneTarget` below. What stays in `app`: the door
//! registry, the [`SceneLoader`] trait and the transit orchestrator, because
//! those wire THIS game's geometry.

/// The scene vocabulary (`LocationRef`, `SceneId`, `SceneTarget`).
pub use daggerfall_nervous::events::{LocationRef, SceneId, SceneTarget};

/// Contract §4 law types, owned by `game-core` since I1 F1.
pub use daggerfall_game_core::scene::{SceneError, SceneTransition, TransitionState};


/// Which key activated the door (`m2_player::ActionState`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoorAction {
    /// `E` — open/interact with the door.
    Interact,
    /// `G` — bash the door (M28 owns what that does to a lock).
    Bash,
}

impl DoorAction {
    /// Maps the action-layer flags to a door activation.
    ///
    /// `Interact` wins when both are pressed in the same frame: the safe
    /// action is the deterministic tie-break, and the bash keeps its own frame
    /// (M28 semantics unchanged).
    pub const fn from_flags(interact: bool, bash: bool) -> Option<Self> {
        if interact {
            Some(Self::Interact)
        } else if bash {
            Some(Self::Bash)
        } else {
            None
        }
    }

    /// Short label for evidence lines.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Interact => "interact(E)",
            Self::Bash => "bash(G)",
        }
    }
}

/// One door of the exterior world that leads to an interior.
#[derive(Debug, Clone, PartialEq)]
pub struct DoorSite {
    /// Door id, unique inside the location.
    pub id: u32,
    /// Building that owns the door.
    pub building: u32,
    /// `.RDB` block the door leads to.
    pub block: u32,
    /// Location of the door.
    pub location: LocationRef,
    /// Door position in the exterior world (m, Bevy frame).
    pub world_position: [f32; 3],
    /// Reach radius of this door (m).
    pub reach_m: f32,
}

impl DoorSite {
    /// Scene target this door loads.
    pub const fn target(&self) -> SceneTarget {
        SceneTarget::Interior {
            location: self.location,
            block: self.block,
            building: self.building,
        }
    }

    /// Planar distance (m) from `point` to the door.
    pub fn distance_m(&self, point: [f32; 3]) -> f32 {
        let dx = self.world_position[0] - point[0];
        let dz = self.world_position[2] - point[2];
        (dx * dx + dz * dz).sqrt()
    }
}

/// Deterministic set of doors of one exterior scene.
#[derive(Debug, Clone, PartialEq)]
pub struct DoorRegistry {
    sites: Vec<DoorSite>,
}

impl DoorRegistry {
    /// Builds the registry: doors sorted by id (ties impossible: ids are
    /// unique). Duplicated ids keep the first one and are dropped, so the
    /// registry is a function of the input set, never of its order.
    pub fn new(sites: Vec<DoorSite>) -> Self {
        let mut sites = sites;
        sites.sort_by_key(|site| site.id);
        sites.dedup_by_key(|site| site.id);
        Self { sites }
    }

    /// Door by id.
    pub fn get(&self, id: u32) -> Option<&DoorSite> {
        self.sites.iter().find(|site| site.id == id)
    }

    /// Number of doors.
    pub fn len(&self) -> usize {
        self.sites.len()
    }

    /// `true` without doors.
    pub fn is_empty(&self) -> bool {
        self.sites.is_empty()
    }

    /// Doors in id order.
    pub fn sites(&self) -> &[DoorSite] {
        &self.sites
    }

    /// Nearest door within its own reach; ties broken by id (ascending).
    ///
    /// Returns the door and the measured distance. `None` means "no door in
    /// reach", which is an explicit failure of the activation, never a silent
    /// no-op.
    pub fn nearest(&self, point: [f32; 3]) -> Option<(f32, &DoorSite)> {
        self.sites
            .iter()
            .map(|site| (site.distance_m(point), site))
            .filter(|(distance, site)| *distance <= site.reach_m)
            .min_by(|(left, a), (right, b)| {
                left.total_cmp(right).then(a.id.cmp(&b.id))
            })
    }

    /// Nearest door to `point` regardless of reach (for diagnostics).
    pub fn closest(&self, point: [f32; 3]) -> Option<(f32, &DoorSite)> {
        self.sites
            .iter()
            .map(|site| (site.distance_m(point), site))
            .min_by(|(left, a), (right, b)| {
                left.total_cmp(right).then(a.id.cmp(&b.id))
            })
    }
}

/// What a [`SceneLoader`] returns after a successful load.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneHandle {
    /// Scene now on screen.
    pub scene: SceneId,
    /// Where the player stands after the load (m, Bevy frame).
    pub position: [f32; 3],
    /// FNV-1a64 of the loaded scene content (determinism evidence).
    pub digest: u64,
}

/// The load itself — supplied by `app` over the SAME loaders the probes use
/// (`bridge::build_retail_interior` for `.RDB`, `bridge::build_shop_mesh` /
/// `world_mesh` for the M4 shop). This module never builds geometry.
pub trait SceneLoader {
    /// Loads the interior of `target` (called with the target `begin` returned).
    fn enter_interior(&mut self, target: SceneTarget) -> Result<SceneHandle, SceneError>;
    /// Leaves the interior and re-enters the exterior world for `target`.
    fn leave_interior(&mut self, target: SceneTarget) -> Result<SceneHandle, SceneError>;
}

/// One completed step of the transit, kept as evidence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransitStep {
    /// Door that produced the step.
    pub door: u32,
    /// Key that activated it.
    pub action: DoorAction,
    /// Scene after the step.
    pub scene: SceneId,
    /// Player position after the step.
    pub position: [f32; 3],
    /// Digest of the loaded scene.
    pub digest: u64,
}

/// Orchestrates world → interior → world (contract §4 `SceneTransition` is the
/// only path; no second loader).
#[derive(Debug, Clone, PartialEq)]
pub struct Transit {
    /// The transition state machine.
    pub transition: SceneTransition,
    /// Scene currently on screen.
    pub scene: SceneId,
    /// Player position in the current scene.
    pub position: [f32; 3],
    /// World position of the last door used (the exit returns here).
    pub door_position: Option<[f32; 3]>,
    /// Doors consumed (one per accepted activation).
    pub doors_consumed: u32,
    /// Interior loads performed by the loader (double-load detector).
    pub loads: u32,
    /// Steps completed, in order.
    pub steps: Vec<TransitStep>,
    exterior: SceneTarget,
}

impl Transit {
    /// Starts in the exterior scene of `exterior`, at `spawn`.
    pub fn new(exterior: SceneTarget, spawn: [f32; 3]) -> Self {
        Self {
            transition: SceneTransition::new(),
            scene: exterior.scene(),
            position: spawn,
            door_position: None,
            doors_consumed: 0,
            loads: 0,
            steps: Vec::new(),
            exterior,
        }
    }

    /// Exterior target the transit returns to.
    pub fn exterior(&self) -> SceneTarget {
        self.exterior
    }

    /// Activates `action` on the nearest door in reach (`E`/`G` path).
    pub fn activate_nearest(
        &mut self,
        action: DoorAction,
        doors: &DoorRegistry,
        loader: &mut impl SceneLoader,
    ) -> Result<SceneTarget, SceneError> {
        let Some((_, site)) = doors.nearest(self.position) else {
            let error = match doors.closest(self.position) {
                Some((distance, site)) => SceneError::NotAdjacent {
                    door: site.id,
                    distance_mm: (distance * 1000.0).round() as u32,
                },
                None => SceneError::UnknownDoor { door: 0 },
            };
            self.transition.fail(error.clone());
            return Err(error);
        };
        self.enter(site.id, action, doors, loader)
    }

    /// Activates ONE door by id (explicit path; used by the negative control).
    pub fn activate_door(
        &mut self,
        id: u32,
        action: DoorAction,
        doors: &DoorRegistry,
        loader: &mut impl SceneLoader,
    ) -> Result<SceneTarget, SceneError> {
        self.enter(id, action, doors, loader)
    }

    /// Shared activation: reach test → request → load → complete.
    fn enter(
        &mut self,
        id: u32,
        action: DoorAction,
        doors: &DoorRegistry,
        loader: &mut impl SceneLoader,
    ) -> Result<SceneTarget, SceneError> {
        if self.scene != SceneId::Exterior {
            let error = SceneError::NotInScene {
                expected: SceneId::Exterior,
                found: self.scene,
            };
            self.transition.fail(error.clone());
            return Err(error);
        }
        let Some(site) = doors.get(id) else {
            let error = SceneError::UnknownDoor { door: id };
            self.transition.fail(error.clone());
            return Err(error);
        };
        let distance = site.distance_m(self.position);
        if distance > site.reach_m {
            let error = SceneError::NotAdjacent {
                door: id,
                distance_mm: (distance * 1000.0).round() as u32,
            };
            self.transition.fail(error.clone());
            return Err(error);
        }
        let target = site.target();
        let position = [site.world_position[0], 0.0, site.world_position[2]];
        if let Err(error) = self.transition.request(target) {
            self.transition.fail(error.clone());
            return Err(error);
        }
        let Some(pending) = self.transition.begin() else {
            let error = SceneError::LoadRejected {
                block: site.block,
                reason: "request did not enqueue a target".to_string(),
            };
            self.transition.fail(error.clone());
            return Err(error);
        };
        match loader.enter_interior(pending) {
            Ok(handle) => {
                self.transition.complete(pending)?;
                self.doors_consumed += 1;
                self.door_position = Some(position);
                self.loads += 1;
                self.scene = handle.scene;
                self.position = handle.position;
                self.steps.push(TransitStep {
                    door: id,
                    action,
                    scene: handle.scene,
                    position: handle.position,
                    digest: handle.digest,
                });
                Ok(pending)
            }
            Err(error) => {
                self.transition.fail(error.clone());
                Err(error)
            }
        }
    }

    /// Leaves the interior and returns to the world at the door used to enter.
    pub fn exit(&mut self, loader: &mut impl SceneLoader) -> Result<SceneTarget, SceneError> {
        if self.scene != SceneId::Interior {
            let error = SceneError::NotInScene {
                expected: SceneId::Interior,
                found: self.scene,
            };
            self.transition.fail(error.clone());
            return Err(error);
        }
        let target = self.exterior;
        if let Err(error) = self.transition.request(target) {
            self.transition.fail(error.clone());
            return Err(error);
        }
        let Some(pending) = self.transition.begin() else {
            let error = SceneError::LoadRejected {
                block: target.location().block,
                reason: "request did not enqueue a target".to_string(),
            };
            self.transition.fail(error.clone());
            return Err(error);
        };
        match loader.leave_interior(pending) {
            Ok(handle) => {
                self.transition.complete(pending)?;
                let door = self
                    .door_position
                    .unwrap_or(handle.position);
                self.loads += 1;
                self.scene = SceneId::Exterior;
                self.position = door;
                self.steps.push(TransitStep {
                    door: u32::MAX,
                    action: DoorAction::Interact,
                    scene: SceneId::Exterior,
                    position: door,
                    digest: handle.digest,
                });
                Ok(pending)
            }
            Err(error) => {
                self.transition.fail(error.clone());
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCATION: LocationRef = LocationRef::new(1, 2, 3);

    fn exterior() -> SceneTarget {
        SceneTarget::Exterior {
            map_pixel: (4, 5),
            location: LOCATION,
        }
    }

    fn door(id: u32, position: [f32; 3]) -> DoorSite {
        DoorSite {
            id,
            building: id + 10,
            block: 7,
            location: LOCATION,
            world_position: position,
            reach_m: 2.5,
        }
    }

    fn registry() -> DoorRegistry {
        DoorRegistry::new(vec![
            door(9, [40.0, 0.0, 0.0]),
            door(3, [0.0, 0.0, 1.0]),
        ])
    }

    /// Loader double: records every call, can refuse.
    struct FakeLoader {
        loads: u32,
        refuse: bool,
        interior_position: [f32; 3],
    }

    impl FakeLoader {
        fn new(interior_position: [f32; 3]) -> Self {
            Self {
                loads: 0,
                refuse: false,
                interior_position,
            }
        }
    }

    impl SceneLoader for FakeLoader {
        fn enter_interior(&mut self, target: SceneTarget) -> Result<SceneHandle, SceneError> {
            let SceneTarget::Interior { block, .. } = target else {
                return Err(SceneError::LoadRejected {
                    block: 0,
                    reason: "not an interior target".to_string(),
                });
            };
            if self.refuse {
                self.loads += 1;
                return Err(SceneError::BlockNotFound { block });
            }
            self.loads += 1;
            Ok(SceneHandle {
                scene: SceneId::Interior,
                position: self.interior_position,
                digest: 0x1111_2222_3333_4444,
            })
        }

        fn leave_interior(&mut self, target: SceneTarget) -> Result<SceneHandle, SceneError> {
            if self.refuse {
                return Err(SceneError::LoadRejected {
                    block: target.location().block,
                    reason: "refused".to_string(),
                });
            }
            Ok(SceneHandle {
                scene: SceneId::Exterior,
                position: [0.0, 0.0, 0.0],
                digest: 0x5555_6666_7777_8888,
            })
        }
    }

    #[test]
    fn request_enqueues_and_poll_delivers_done_exactly_once() {
        let mut transition = SceneTransition::new();
        assert_eq!(*transition.state(), TransitionState::Idle);
        assert_eq!(transition.poll(), None);
        let target = SceneTarget::Interior {
            location: LOCATION,
            block: 7,
            building: 2,
        };
        transition.request(target).unwrap();
        assert_eq!(transition.begin(), Some(target));
        assert_eq!(transition.poll(), Some(TransitionState::Loading(target)));
        transition.complete(target).unwrap();
        assert_eq!(transition.poll(), Some(TransitionState::Done(target)));
        assert_eq!(transition.poll(), None, "Done is a single-shot value");
        assert_eq!(
            *transition.state(),
            TransitionState::Done(target),
            "the state keeps the value after delivery"
        );
    }

    #[test]
    fn request_is_a_one_slot_queue() {
        let mut transition = SceneTransition::new();
        let first = SceneTarget::Menu;
        transition.request(first).unwrap();
        assert_eq!(
            transition.request(exterior()),
            Err(SceneError::Busy(first))
        );
    }

    #[test]
    fn complete_rejects_a_target_that_is_not_in_flight() {
        let mut transition = SceneTransition::new();
        assert!(transition.complete(SceneTarget::Menu).is_err());
        let target = SceneTarget::Interior {
            location: LOCATION,
            block: 7,
            building: 2,
        };
        transition.request(target).unwrap();
        assert!(transition.complete(SceneTarget::Menu).is_err());
        assert!(transition.complete(target).is_ok());
    }

    #[test]
    fn door_action_maps_the_declared_bindings() {
        // `E` = interact, `G` = bash (m2_player::ActionState).
        assert_eq!(DoorAction::from_flags(true, false), Some(DoorAction::Interact));
        assert_eq!(DoorAction::from_flags(false, true), Some(DoorAction::Bash));
        assert_eq!(DoorAction::from_flags(false, false), None);
        // Both in one frame: the safe action wins (deterministic tie-break).
        assert_eq!(DoorAction::from_flags(true, true), Some(DoorAction::Interact));
    }

    #[test]
    fn registry_is_order_independent_and_ties_break_by_id() {
        let forward = DoorRegistry::new(vec![door(1, [0.0, 0.0, 0.0]), door(2, [0.0, 0.0, 0.0])]);
        let backward = DoorRegistry::new(vec![door(2, [0.0, 0.0, 0.0]), door(1, [0.0, 0.0, 0.0])]);
        assert_eq!(forward, backward);
        let (distance, site) = forward.nearest([0.0, 0.0, 0.0]).unwrap();
        assert_eq!(distance, 0.0);
        assert_eq!(site.id, 1, "tie broken by ascending id");
        assert_eq!(forward.len(), 2);
        assert!(!forward.is_empty());
        assert!(DoorRegistry::new(Vec::new()).is_empty());
    }

    #[test]
    fn enter_and_exit_return_to_the_door_position() {
        let doors = registry();
        let spawn = [0.0, 0.0, 0.0];
        let mut transit = Transit::new(exterior(), spawn);
        let mut loader = FakeLoader::new([0.5, 1.6, -3.0]);

        let target = transit
            .activate_nearest(DoorAction::Interact, &doors, &mut loader)
            .unwrap();
        assert_eq!(target.scene(), SceneId::Interior);
        assert_eq!(transit.scene, SceneId::Interior);
        assert_eq!(transit.position, [0.5, 1.6, -3.0]);
        assert_eq!(transit.loads, 1);
        assert_eq!(transit.doors_consumed, 1);
        assert_eq!(transit.door_position, Some([0.0, 0.0, 1.0]));
        assert_eq!(
            transit.transition.poll(),
            Some(TransitionState::Done(target))
        );
        assert_eq!(transit.transition.poll(), None);

        let back = transit.exit(&mut loader).unwrap();
        assert_eq!(back, exterior());
        assert_eq!(transit.scene, SceneId::Exterior);
        assert_eq!(
            transit.position,
            [0.0, 0.0, 1.0],
            "the world resumes AT the door"
        );
        assert_eq!(transit.loads, 2, "one load per transition");
        assert_eq!(transit.steps.len(), 2);
    }

    #[test]
    fn unknown_door_fails_without_mutating_the_scene() {
        let doors = registry();
        let mut transit = Transit::new(exterior(), [0.0, 0.0, 0.0]);
        let mut loader = FakeLoader::new([0.5, 1.6, -3.0]);
        let error = transit
            .activate_door(77, DoorAction::Bash, &doors, &mut loader)
            .unwrap_err();
        assert_eq!(error, SceneError::UnknownDoor { door: 77 });
        assert_eq!(transit.scene, SceneId::Exterior, "0 scene mutation");
        assert_eq!(transit.loads, 0, "0 loads");
        assert_eq!(transit.doors_consumed, 0, "0 doors consumed");
        assert_eq!(transit.position, [0.0, 0.0, 0.0]);
        assert!(matches!(
            transit.transition.state(),
            TransitionState::Failed(SceneError::UnknownDoor { door: 77 })
        ));
    }

    #[test]
    fn door_out_of_reach_fails_without_mutating_the_scene() {
        let doors = registry();
        let mut transit = Transit::new(exterior(), [20.0, 0.0, 0.0]);
        let mut loader = FakeLoader::new([0.5, 1.6, -3.0]);
        let error = transit
            .activate_door(9, DoorAction::Interact, &doors, &mut loader)
            .unwrap_err();
        assert_eq!(
            error,
            SceneError::NotAdjacent {
                door: 9,
                distance_mm: 20_000
            }
        );
        assert_eq!(transit.scene, SceneId::Exterior);
        assert_eq!(transit.loads, 0);
        assert_eq!(transit.doors_consumed, 0);
        assert!(matches!(
            transit.transition.state(),
            TransitionState::Failed(SceneError::NotAdjacent { .. })
        ));
    }

    #[test]
    fn a_second_activation_inside_the_interior_does_not_load_again() {
        let doors = registry();
        let mut transit = Transit::new(exterior(), [0.0, 0.0, 0.0]);
        let mut loader = FakeLoader::new([0.5, 1.6, -3.0]);
        transit
            .activate_nearest(DoorAction::Interact, &doors, &mut loader)
            .unwrap();
        let error = transit
            .activate_door(3, DoorAction::Bash, &doors, &mut loader)
            .unwrap_err();
        assert_eq!(
            error,
            SceneError::NotInScene {
                expected: SceneId::Exterior,
                found: SceneId::Interior
            }
        );
        assert_eq!(transit.loads, 1, "0 double load");
        assert_eq!(transit.doors_consumed, 1, "the door is consumed once");
    }

    #[test]
    fn a_refused_load_leaves_the_scene_and_the_scene_digest_untouched() {
        let doors = registry();
        let mut transit = Transit::new(exterior(), [0.0, 0.0, 0.0]);
        let mut loader = FakeLoader::new([0.5, 1.6, -3.0]);
        loader.refuse = true;
        let error = transit
            .activate_door(3, DoorAction::Interact, &doors, &mut loader)
            .unwrap_err();
        assert_eq!(error, SceneError::BlockNotFound { block: 7 });
        assert_eq!(transit.scene, SceneId::Exterior);
        assert_eq!(transit.steps.len(), 0);
        assert_eq!(transit.doors_consumed, 0);
        assert_eq!(transit.position, [0.0, 0.0, 0.0]);
        assert!(matches!(
            transit.transition.state(),
            TransitionState::Failed(SceneError::BlockNotFound { .. })
        ));
    }

    #[test]
    fn the_digest_trail_is_deterministic_across_runs() {
        let run = || {
            let doors = registry();
            let mut transit = Transit::new(exterior(), [0.0, 0.0, 0.0]);
            let mut loader = FakeLoader::new([0.5, 1.6, -3.0]);
            transit
                .activate_nearest(DoorAction::Interact, &doors, &mut loader)
                .unwrap();
            transit.exit(&mut loader).unwrap();
            transit
        };
        let first = run();
        let second = run();
        assert_eq!(first.steps, second.steps);
        assert_eq!(
            first.steps.iter().map(|step| step.digest).collect::<Vec<_>>(),
            vec![0x1111_2222_3333_4444, 0x5555_6666_7777_8888]
        );
    }
}

//! M18 **FG-01** — the doors that open: exterior → interior → exterior.
//!
//! ## Measured cause this module closes
//!
//! `interior_transit` (and its green probe `INTERIOR-TRANSIT-001`) was a
//! complete state machine with **zero** production callers: `run_windowed_m18`
//! never built a [`DoorWorld`] nor a [`SceneLoader`], the window's only door
//! consumer (`m28_door_bash`) reads `action.bash` (CS-14), and `action.interact`
//! (`E`) had no consumer at all. The machine worked; nobody called it
//! (`hh-1-20261029-fg01-puertas`, task 1).
//!
//! This module is **wiring only** — it invents no geometry, no door and no
//! scene:
//! - doors come from the retail composition (`city.buildings[].doors`; the
//!   capital's `.RMB` sub-records report `0 door(s)` in this data path, so the
//!   one door of the composition is the CS-14 scenario door
//!   `M18_CS14=1` — the same asset `PROBE-M28-CS14` prices);
//! - the interior is the ONE measured block `N0000055.RDB`
//!   (`INTERIOR-TRANSIT-001`, digest `e416fa07…`), assembled through the same
//!   `m11_probe` loader the probe uses.
//!
//! ## Deliberate boundaries (named, not silent)
//!
//! - **`GAP-FG01-01`** — building → `.RDB` mapping: every door loads
//!   [`INTERIOR_BLOCK`]. The real per-building block (BLOCKS.BSA sector/
//!   LocationId → block index) is a separate capability and does **not** block
//!   FG-01's closure (capitán, this cycle).
//! - **`GAP-FG01-02`** — the capital's retail buildings report 0 doors in the
//!   `.RMB` data path (`PROBE-RMB-001`), so the door the player walks to is the
//!   CS-14 scenario door.
//! - The interior mesh is placed [`INTERIOR_OFFSET_M`] away from the city on
//!   +X so both worlds fit one Bevy world without overlapping; the camera's
//!   default far plane (1 000 m) sees only the scene the player is in.

use bevy::prelude::*;
use daggerfall_behavior::locomotion::{LocomotionWorld, StepOutcome as LocomotionStepOutcome};
use daggerfall_nervous::events::{LocationRef, SceneId, SceneTarget};
use daggerfall_parser_bevy_bridge as bridge;

use daggerfall_app::interior_transit::{
    DoorAction, DoorRegistry, DoorSite, SceneError, SceneHandle, SceneLoader, Transit,
};

use crate::m18_demo::{DemoLocomotionWorld, DemoScene, DemoTexture};
use crate::m18_window::{DemoWorld, DOOR_REACH_M};

/// Block every door loads (capitán, this cycle): the one interior measured by
/// `PROBE-INTERIOR-TRANSIT-001`. `GAP-FG01-01` owns the building → block map.
pub const INTERIOR_BLOCK_NAME: &str = "N0000055.RDB";

/// `.RDB` block index of [`INTERIOR_BLOCK_NAME`] (the record's own number, i.e.
/// the `BLOCKS.BSA` directory name). This is the `block` a [`DoorSite`] carries
/// and the index the loader accepts; anything else is `BlockNotFound` (loud).
pub const INTERIOR_BLOCK: u32 = 55;

/// +X translation of the interior stage inside the single Bevy world (m). The
/// city occupies the M13 ring near the origin; 10 km is far beyond the default
/// camera far plane, so neither world is visible from the other.
pub const INTERIOR_OFFSET_M: f32 = 10_000.0;

/// Distance the `--door-spawn` scenario stands the player from the door (m):
/// inside the [`DOOR_REACH_M`] reach, and short enough to be "at the door".
pub const DOOR_STANDOFF_M: f32 = 1.0;

/// Window title value of the exterior scene (`scene=` field).
pub const EXTERIOR_LABEL: &str = "Exterior";

/// Where the player stands after a transition is applied (the locomotion
/// position). `m18_apply_player_action` consumes it on the next frame, so the
/// teleport never fights the step that already ran this frame.
pub type Teleport = [f32; 3];

/// Loaded interior stage: the retail block in the **world frame** (already
/// translated by [`INTERIOR_OFFSET_M`]) plus its collision grid and spawn.
pub struct InteriorStage {
    /// Record name in `BLOCKS.BSA`.
    pub block_name: String,
    /// Mesh of the block, translated to the world frame.
    pub spec: bridge::MeshSpec,
    /// Collision grid of the translated mesh (`DEFAULT_CELL_M`).
    pub grid: bridge::CollisionGrid,
    /// Eye position of the measured free spawn (world frame, m).
    pub eye: [f32; 3],
    /// Exact floor height under the spawn (m).
    pub floor_y: f32,
    /// FNV-1a64 of the **untranslated** retail **mesh-spec** (positions +
    /// indices), i.e. [`mesh_digest`]. This is *not* the block's
    /// canonical-bytes digest that `PROBE-INTERIOR-TRANSIT-001` reports
    /// (`e416fa07…`, `interior_transit_probe.rs`): same assembled interior,
    /// two different metrics. The value the window prints is this one.
    pub digest: u64,
    /// Triangles of the retail mesh.
    pub triangles: usize,
    /// Placed models resolved into the mesh.
    pub models: usize,
    /// Placements that had no resolvable ARCH3D (honest degradation).
    pub unresolved_models: usize,
}

/// FNV-1a64 of a mesh-spec (positions + indices) — the digest the stage reports
/// (not the block's canonical-bytes digest of `PROBE-INTERIOR-TRANSIT-001`).
pub fn mesh_digest(mesh: &bridge::MeshSpec) -> u64 {
    let mut bytes = Vec::with_capacity(mesh.positions.len() * 12 + mesh.indices.len() * 4);
    for position in &mesh.positions {
        for component in position {
            bytes.extend_from_slice(&component.to_le_bytes());
        }
    }
    for index in &mesh.indices {
        bytes.extend_from_slice(&index.to_le_bytes());
    }
    daggerfall_parser_rs::rdi::fnv1a64(&bytes)
}

/// Planar (x/z) distance in metres.
pub fn planar_distance_m(left: [f32; 3], right: [f32; 3]) -> f32 {
    let dx = left[0] - right[0];
    let dz = left[2] - right[2];
    (dx * dx + dz * dz).sqrt()
}

/// Assembles [`INTERIOR_BLOCK_NAME`] with the **same** loader
/// `PROBE-INTERIOR-TRANSIT-001` uses (`m11_probe::RdbSources`), translates the
/// mesh to the world frame and builds its collision grid.
pub fn build_stage() -> Result<InteriorStage, String> {
    let mut sources = crate::m11_probe::load_rdb_sources()?;
    let records = sources.parse_directory();
    let record = records
        .iter()
        .find(|record| record.name.eq_ignore_ascii_case(INTERIOR_BLOCK_NAME))
        .ok_or_else(|| {
            format!(
                "{INTERIOR_BLOCK_NAME} is not in the BLOCKS.BSA directory ({} .RDB records)",
                records.len()
            )
        })?;
    let interior = sources.assemble(record)?;
    if interior.mesh.indices.is_empty() {
        return Err(format!(
            "{INTERIOR_BLOCK_NAME} assembled an empty mesh ({} models, {} unresolved)",
            interior.models.len(),
            interior.unresolved_models
        ));
    }
    let digest = mesh_digest(&interior.mesh);
    let mut spec = interior.mesh.clone();
    for position in &mut spec.positions {
        position[0] += INTERIOR_OFFSET_M;
    }
    let grid = bridge::CollisionGrid::from_mesh_spec(&spec, bridge::DEFAULT_CELL_M);
    let spawn = grid
        .find_spawn(&spec, bridge::EYE_HEIGHT_M, bridge::PLAYER_RADIUS_M)
        .ok_or_else(|| format!("{INTERIOR_BLOCK_NAME} has no free spawn"))?;
    Ok(InteriorStage {
        block_name: interior.block_name,
        triangles: spec.triangle_count(),
        spec,
        grid,
        eye: spawn.eye,
        floor_y: spawn.floor_y,
        digest,
        models: interior.models.len(),
        unresolved_models: interior.unresolved_models,
    })
}

/// The app's [`SceneLoader`] over the retail block: the only thing the
/// [`Transit`] state machine talks to, exactly like
/// `PROBE-INTERIOR-TRANSIT-001` talks to its own loader.
///
/// Deliberately **lazy**: the block is assembled on the first `enter_interior`
/// (`BLOCKS.BSA` directory + one block), never at startup, so the window's boot
/// stays as `MENU-001` measures it.
pub struct DoorLoader {
    /// Stage produced by the last successful `enter_interior` (moved out by
    /// [`DoorWorld::interact`] into [`DoorWorld::stage`]).
    pub stage_output: Option<InteriorStage>,
    /// Exterior position the exit returns to (the measured spawn eye).
    pub exterior_position: [f32; 3],
    /// Exterior content digest (composition digest, evidence only).
    pub exterior_digest: u64,
    /// Last build error, if any (reported by the interact system).
    pub build_error: Option<String>,
    /// Interior loads attempted (evidence: a second load is a defect).
    pub loads: u32,
}

impl DoorLoader {
    /// Fresh loader: exterior spawn + composition digest.
    pub fn new(exterior_position: [f32; 3], exterior_digest: u64) -> Self {
        Self {
            stage_output: None,
            exterior_position,
            exterior_digest,
            build_error: None,
            loads: 0,
        }
    }
}

impl SceneLoader for DoorLoader {
    fn enter_interior(&mut self, target: SceneTarget) -> Result<SceneHandle, SceneError> {
        let SceneTarget::Interior { block, .. } = target else {
            return Err(SceneError::LoadRejected {
                block: 0,
                reason: "door target is not an interior".to_string(),
            });
        };
        if block != INTERIOR_BLOCK {
            return Err(SceneError::BlockNotFound { block });
        }
        self.loads += 1;
        match build_stage() {
            Ok(stage) => {
                let handle = SceneHandle {
                    scene: SceneId::Interior,
                    position: stage.eye,
                    digest: stage.digest,
                };
                self.stage_output = Some(stage);
                self.build_error = None;
                Ok(handle)
            }
            Err(error) => {
                self.build_error = Some(error.clone());
                Err(SceneError::LoadRejected {
                    block,
                    reason: error,
                })
            }
        }
    }

    fn leave_interior(&mut self, target: SceneTarget) -> Result<SceneHandle, SceneError> {
        if target.scene() != SceneId::Exterior {
            return Err(SceneError::LoadRejected {
                block: target.location().block,
                reason: "exit target is not the exterior".to_string(),
            });
        }
        Ok(SceneHandle {
            scene: SceneId::Exterior,
            position: self.exterior_position,
            digest: self.exterior_digest,
        })
    }
}

/// `LocomotionWorld` of the world the player is in: the retail exterior
/// ([`DemoLocomotionWorld`]) or the interior block. One adapter, so
/// `Locomotion::step` never learns which scene it is stepping in.
pub enum ActiveWorld<'a> {
    /// Retail city + M13 terrain (the production adapter, unmodified).
    Exterior(DemoLocomotionWorld<'a>),
    /// Retail interior block (`resolve_step` over its own grid).
    Interior(&'a InteriorStage),
}

impl LocomotionWorld for ActiveWorld<'_> {
    fn ground_height(&self, x: f32, z: f32) -> f32 {
        match self {
            Self::Exterior(world) => world.ground_height(x, z),
            Self::Interior(stage) => stage.floor_y,
        }
    }

    /// Interior semantics are the retail grid's: `resolve_step` slides along
    /// walls, and **outside the block's bounds is blocked** (no silent exit
    /// through the walls).
    fn step(&self, from: [f32; 3], delta: [f32; 3], radius: f32) -> LocomotionStepOutcome {
        match self {
            Self::Exterior(world) => world.step(from, delta, radius),
            Self::Interior(stage) => {
                let outcome = stage
                    .grid
                    .resolve_step(from, [delta[0], 0.0, delta[2]], radius);
                if outcome.moved {
                    LocomotionStepOutcome {
                        pos: [outcome.pos[0], from[1], outcome.pos[2]],
                        moved: true,
                        blocked: outcome.blocked.is_some(),
                    }
                } else {
                    LocomotionStepOutcome {
                        pos: from,
                        moved: false,
                        blocked: true,
                    }
                }
            }
        }
    }

    fn wall_ahead(&self, _eye: [f32; 3], _dir: [f32; 3], _reach: f32) -> bool {
        // Climbing is not modelled in the interior (the same honest `false` the
        // exterior demo adapter reports, §6.5).
        false
    }

    fn submerged(&self, _eye: [f32; 3]) -> bool {
        // A block has no water volume.
        false
    }
}

/// The wired door system: registry + [`Transit`] + loader + the loaded stage.
#[derive(Resource)]
pub struct DoorWorld {
    /// The state machine that was green and unreachable until this cycle.
    pub transit: Transit,
    /// Doors of the composition, in id order.
    pub registry: DoorRegistry,
    /// `Transit`'s loader (the only geometry producer).
    pub loader: DoorLoader,
    /// Scene the player is in.
    pub scene: SceneId,
    /// Loaded interior (built once, reused for every door — `GAP-FG01-01`).
    pub stage: Option<InteriorStage>,
    /// Interior spawn = the exit point of the block (reach-tested).
    pub interior_entry: Option<[f32; 3]>,
    /// Entities spawned for the interior groups + its light.
    pub interior_entities: Vec<Entity>,
    /// Pending teleport for `m18_apply_player_action` (next frame).
    pub pending_teleport: Option<Teleport>,
    /// `scene=` value of the window title (`Exterior` | `Interior(<block>)`).
    pub scene_label: String,
    /// Doors in the composition (0 ⇒ the data path has none: `GAP-FG01-02`).
    pub doors_in_composition: usize,
    /// Last transition error, as printed (evidence; `None` after a success).
    pub last_error: Option<String>,
    /// M44 (`CR-bd972066-59e6-4768-9985-de75f69f6798`): the dungeon this
    /// interior descends into. `None` = not entered yet; the content block is
    /// published exactly once per door world.
    pub dungeon: Option<crate::dungeon_content::DungeonEntry>,
}

impl DoorWorld {
    /// Builds the wired system from the composition: one [`DoorSite`] per
    /// retail door; every site loads [`INTERIOR_BLOCK`].
    pub fn from_scene(scene: &DemoScene, composition_digest: u64) -> Self {
        let exterior = SceneTarget::Exterior {
            map_pixel: (scene.map_pixel[0] as u16, scene.map_pixel[1] as u16),
            location: LocationRef::new(0, 0, INTERIOR_BLOCK),
        };
        let sites = door_sites(scene);
        let doors_in_composition = sites.len();
        Self {
            transit: Transit::new(exterior, scene.spawn.eye),
            registry: DoorRegistry::new(sites),
            loader: DoorLoader::new(scene.spawn.eye, composition_digest),
            scene: SceneId::Exterior,
            stage: None,
            interior_entry: None,
            interior_entities: Vec::new(),
            pending_teleport: None,
            scene_label: EXTERIOR_LABEL.to_string(),
            doors_in_composition,
            last_error: None,
            dungeon: None,
        }
    }

    /// `LocomotionWorld` of the current scene.
    pub fn active_world<'a>(&'a self, scene: &'a DemoScene) -> ActiveWorld<'a> {
        match (self.scene, self.stage.as_ref()) {
            (SceneId::Interior, Some(stage)) => ActiveWorld::Interior(stage),
            _ => ActiveWorld::Exterior(scene.locomotion_world()),
        }
    }

    /// Consumes the pending teleport (the movement system calls this once per
    /// frame, before the step).
    pub fn take_teleport(&mut self) -> Option<Teleport> {
        self.pending_teleport.take()
    }

    /// **NEW GAME (CR-178183d7)** — returns the door world to the exterior of
    /// `scene` at the retail spawn and hands back the entities of the loaded
    /// stage so the caller despawns them.
    ///
    /// The stage is released, not kept: a new game must not carry the interior
    /// of the session that just ended, and keeping it would duplicate the
    /// block on the next door (`spawn_stage` is only ever called from a fresh
    /// `stage`). The block is rebuilt by the lazy [`DoorLoader`] when the
    /// player walks through a door again (`GAP-FG01-01`: one block, same
    /// bytes). `loader.loads` keeps counting those builds — it is evidence,
    /// never reset here.
    ///
    /// Inside the interior the exit goes through the state machine
    /// ([`Transit::exit`], reach-tested against the block entry) so the
    /// transition rules are the same ones `E` uses; a refused exit is printed
    /// and the reset forces the exterior anyway (0 stale scene).
    pub fn reset_to_exterior(&mut self, scene: &DemoScene) -> Vec<Entity> {
        if self.scene == SceneId::Interior {
            if let Some(entry) = self.interior_entry {
                // The exit is reach-tested against the entry point: stand there.
                self.transit.position = entry;
            }
            if let Err(error) = self.interact() {
                println!(
                    "M18 VE: NEW GAME — the interior exit was refused ({error:?}); the reset \
                     forces the exterior anyway (0 stale stage)"
                );
            }
        }
        let exterior = SceneTarget::Exterior {
            map_pixel: (scene.map_pixel[0] as u16, scene.map_pixel[1] as u16),
            location: LocationRef::new(0, 0, INTERIOR_BLOCK),
        };
        self.transit = Transit::new(exterior, scene.spawn.eye);
        self.scene = SceneId::Exterior;
        self.stage = None;
        self.interior_entry = None;
        self.pending_teleport = None;
        self.scene_label = EXTERIOR_LABEL.to_string();
        self.last_error = None;
        // The dungeon descent is per door world: `None` republishes on re-entry.
        self.dungeon = None;
        std::mem::take(&mut self.interior_entities)
    }

    /// `E` path: `Transit::activate_nearest` in the exterior, `Transit::exit`
    /// in the interior (reach-tested against the block's entry point).
    ///
    /// `Ok(Some(scene))` means the scene changed in this call.
    pub fn interact(&mut self) -> Result<Option<SceneId>, SceneError> {
        match self.scene {
            SceneId::Exterior => {
                {
                    let Self {
                        transit,
                        registry,
                        loader,
                        ..
                    } = &mut *self;
                    transit.activate_nearest(DoorAction::Interact, registry, loader)?;
                }
                // The state machine completed the load: its loader produced the
                // destination stage.
                let stage = self.loader.stage_output.take().ok_or_else(|| {
                    SceneError::LoadRejected {
                        block: INTERIOR_BLOCK,
                        reason: "loader reported success without a stage".to_string(),
                    }
                })?;
                self.interior_entry = Some(stage.eye);
                self.pending_teleport = Some(stage.eye);
                self.stage = Some(stage);
                self.scene = SceneId::Interior;
                self.scene_label = format!("Interior({INTERIOR_BLOCK_NAME})");
                Ok(Some(SceneId::Interior))
            }
            SceneId::Interior => {
                let distance = self
                    .interior_entry
                    .map(|entry| planar_distance_m(entry, self.transit.position));
                match distance {
                    Some(meters) if meters <= DOOR_REACH_M => {}
                    other => {
                        let error = SceneError::NotAdjacent {
                            door: u32::MAX,
                            distance_mm: other
                                .map(|meters| (meters * 1000.0).round() as u32)
                                .unwrap_or(0),
                        };
                        self.last_error = Some(format!("{error:?}"));
                        return Err(error);
                    }
                }
                {
                    let Self { transit, loader, .. } = &mut *self;
                    transit.exit(loader)?;
                }
                self.scene = SceneId::Exterior;
                self.scene_label = EXTERIOR_LABEL.to_string();
                let mut eye = self.transit.position;
                eye[1] = self.loader.exterior_position[1];
                self.pending_teleport = Some(eye);
                Ok(Some(SceneId::Exterior))
            }
            SceneId::Menu => Ok(None),
        }
    }
}

/// One [`DoorSite`] per door of the composition, in composition order.
///
/// The retail path (`city.buildings[].doors`) is the only door source; the
/// capital reports 0 of them today (`GAP-FG01-02`), so the set is the CS-14
/// scenario door when `M18_CS14=1`. No door is synthesised here.
pub fn door_sites(scene: &DemoScene) -> Vec<DoorSite> {
    let mut sites = Vec::new();
    for (building, spec) in scene.city.buildings.iter().enumerate() {
        for position in &spec.doors {
            sites.push(DoorSite {
                id: sites.len() as u32,
                building: building as u32,
                block: INTERIOR_BLOCK,
                location: LocationRef::new(0, 0, INTERIOR_BLOCK),
                world_position: *position,
                reach_m: DOOR_REACH_M,
            });
        }
    }
    sites
}

/// Applies `--door-spawn` **on the composition**, before anything consumes it
/// (camera, `Locomotion`, `Transit`): the player starts [`DOOR_STANDOFF_M`]
/// from the first door, facing it.
///
/// Refuses loudly when the composition has no door (0 doors is a measured
/// property of the `.RMB` data path, never a silent fallback).
pub fn apply_door_spawn(scene: &mut DemoScene) -> Result<[f32; 3], String> {
    let sites = door_sites(scene);
    let Some(site) = sites.first() else {
        return Err(format!(
            "no door in the composition ({} buildings, 0 door(s) in the .RMB data path) — \
             the CS-14 scenario door needs M18_CS14=1",
            scene.city.buildings.len()
        ));
    };
    let door = site.world_position;
    let spawn = scene.spawn.eye;
    let (dx, dz) = {
        let dx = spawn[0] - door[0];
        let dz = spawn[2] - door[2];
        let len = (dx * dx + dz * dz).sqrt();
        if len > 1e-3 {
            (dx / len, dz / len)
        } else {
            (0.0, 1.0)
        }
    };
    let eye = [
        door[0] + dx * DOOR_STANDOFF_M,
        spawn[1],
        door[2] + dz * DOOR_STANDOFF_M,
    ];
    scene.spawn.eye = eye;
    // Bevy frame: forward = [-sin(yaw), 0, -cos(yaw)]; facing the door means
    // forward = -offset ⇒ yaw = atan2(offset.x, offset.z).
    scene.spawn.yaw = dx.atan2(dz);
    Ok(door)
}

/// Spawns the interior stage (textured groups + one point light) into the
/// world. Called once per process: the block is built on the first door and
/// reused for every other door (`GAP-FG01-01`).
pub fn spawn_stage(
    stage: &InteriorStage,
    textures: &std::collections::BTreeMap<u32, DemoTexture>,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    images: &mut Assets<Image>,
    materials: &mut Assets<StandardMaterial>,
) -> (Vec<Entity>, usize) {
    let flat_material = bridge::material_from_spec(
        &bridge::MaterialSpec {
            base_color: [0.42, 0.39, 0.35, 1.0],
            ..default()
        },
        materials,
    );
    let mut entities = Vec::new();
    let mut textured = 0usize;
    for group in bridge::split_by_texture(&stage.spec) {
        let mut fallback =
            || (bridge::mesh_from_spec(&group.mesh, meshes), flat_material.clone());
        let (mesh, material) = match textures.get(&group.texture_key) {
            Some(texture) => match bridge::image_from_rgba(
                &texture.rgba,
                u32::from(texture.width),
                u32::from(texture.height),
                images,
            ) {
                Ok(image) => {
                    let uvs = bridge::scale_uvs(
                        &group.mesh.uvs,
                        u32::from(texture.width),
                        u32::from(texture.height),
                    );
                    let spec = bridge::MeshSpec {
                        uvs,
                        ..group.mesh.clone()
                    };
                    let mesh = bridge::mesh_from_spec(&spec, meshes);
                    let material = bridge::material_from_texture(
                        &bridge::MaterialSpec {
                            base_color: [1.0, 1.0, 1.0, 1.0],
                            ..default()
                        },
                        image,
                        materials,
                    );
                    textured += 1;
                    (mesh, material)
                }
                Err(_) => fallback(),
            },
            None => fallback(),
        };
        entities.push(commands.spawn((Mesh3d(mesh), MeshMaterial3d(material))).id());
    }
    // A block has a roof: the sun does not reach it, so the room carries its
    // own light at the spawn (the retail light placements of this path are not
    // wired — a named boundary, not a hidden constant).
    entities.push(
        commands
            .spawn((
                PointLight {
                    intensity: 180_000.0,
                    range: 60.0,
                    shadow_maps_enabled: false,
                    ..default()
                },
                Transform::from_xyz(stage.eye[0], stage.eye[1] + 2.0, stage.eye[2]),
            ))
            .id(),
    );
    (entities, textured)
}

/// **FG-01 startup** — inserts the [`DoorWorld`] built from the composition.
///
/// Runs after the composition resource exists ([`DemoWorld`]); it builds no
/// geometry (the interior is lazy) and fails on nothing: 0 doors is printed as
/// the measured data-path fact, never a silent empty registry.
pub fn m18_door_startup(mut commands: Commands, world: Res<DemoWorld>) {
    let scene = &world.0;
    let doors = DoorWorld::from_scene(scene, mesh_digest(&scene.city.combined));
    println!(
        "FG01 VE: door registry {} door(s) · interior {} (block {}) · reach {:.1} m · \
         exterior digest {:016x}",
        doors.registry.len(),
        INTERIOR_BLOCK_NAME,
        INTERIOR_BLOCK,
        DOOR_REACH_M,
        doors.loader.exterior_digest,
    );
    if doors.registry.is_empty() {
        println!(
            "FG01 VE: 0 doors in the composition (.RMB data path, GAP-FG01-02) — `E` finds no \
             door until a door exists in the composition"
        );
    }
    commands.insert_resource(doors);
}

/// **FG-01 interact** — the `E` path of the player, wired to [`DoorWorld`].
///
/// Runs after [`crate::m18_window::m18_apply_player_action`] (the step has
/// already moved and synced the position) and only in the PLAYING phase. One
/// press: exterior ⇒ interior; interior at the entry point ⇒ exterior. A
/// rejected activation is printed (never a silent no-op).
pub fn m18_door_interact(
    mut commands: Commands,
    action: Res<crate::m2_player::ActionState>,
    world: Res<DemoWorld>,
    mut doors: ResMut<DoorWorld>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !action.interact {
        return;
    }
    let before = doors.scene;
    let changed = match doors.interact() {
        Ok(changed) => {
            doors.last_error = None;
            changed
        }
        Err(error) => {
            println!("FG01 VE: interact rejected — {error:?}");
            doors.last_error = Some(format!("{error:?}"));
            None
        }
    };
    let Some(scene) = changed else {
        return;
    };
    if before == SceneId::Exterior && scene == SceneId::Interior {
        if doors.interior_entities.is_empty() {
            let Some(stage) = doors.stage.as_ref() else {
                return;
            };
            let (entities, textured) = spawn_stage(
                stage,
                &world.0.textures,
                &mut commands,
                &mut meshes,
                &mut images,
                &mut materials,
            );
            println!(
                "FG01 VE: interior {} spawned — {} groups ({textured} textured) · {} triangles · \
                 {} models ({} unresolved) · eye=({:.2},{:.2},{:.2})",
                stage.block_name,
                entities.len() - 1,
                stage.triangles,
                stage.models,
                stage.unresolved_models,
                stage.eye[0],
                stage.eye[1],
                stage.eye[2],
            );
            doors.interior_entities = entities;
        }
        // M44 (`CR-bd972066`): the interior is the way down. The content block
        // is published **before** the `FG01 VE: interact → interior` line — the
        // gate driver kills the process as soon as it reads that line.
        crate::dungeon_content::descend(&mut doors);
        println!(
            "FG01 VE: interact → interior {} · block {} · digest {:016x} · scene={}",
            doors
                .stage
                .as_ref()
                .map(|stage| stage.block_name.as_str())
                .unwrap_or(INTERIOR_BLOCK_NAME),
            INTERIOR_BLOCK,
            doors.stage.as_ref().map(|stage| stage.digest).unwrap_or(0),
            doors.scene_label,
        );
    } else if before == SceneId::Interior && scene == SceneId::Exterior {
        println!(
            "FG01 VE: interact → exterior · door returned · position=({:.2},{:.2},{:.2}) · scene={}",
            doors.transit.position[0],
            doors.transit.position[1],
            doors.transit.position[2],
            doors.scene_label,
        );
    }
}

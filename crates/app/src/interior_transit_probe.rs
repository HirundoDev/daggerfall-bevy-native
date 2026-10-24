//! PROBE-INTERIOR-TRANSIT-001 — world → interior → world through a door
//! (M18 B2-b, REQ-L99-043 · CS-02/CS-03 spine).
//!
//! **What it proves:** the spine's `SceneTransition`
//! (`spine-contracts-CR-f716455c.md` §4) moves the session from the exterior
//! world into a **real** `.RDB` interior and back to the world **at the door**
//! with 0 wall traversals, one door consumption and 0 double loads — and that
//! the load is the SAME one M4/M11 already run, not a second loader:
//!
//! | Leg | Code that does it |
//! |:---|:---|
//! | record → `RetailInterior` | `m11_probe::evaluate_real_interior` → `RdbSources::assemble` → `bridge::build_retail_interior` (`interior_retail.rs:299`) |
//! | world mesh (street + block) | `bridge::retail_world` (`interior_retail.rs:372`) |
//! | collision | `CollisionGrid::from_mesh_spec` (`collision_grid.rs:122`) |
//! | world→door march | `bridge::march_axis` (`interior_probe.rs:80`, the M4/M5 march) |
//! | scene transition | `daggerfall_app::interior_transit::SceneTransition` |
//!
//! **Controls (executed, never declared):** unknown door, door out of reach and
//! a loader that refuses the block — each must leave `Failed` in the transition
//! and 0 mutation in the scene (position, steps, digest). The whole transit is
//! composed TWICE (two assemblies of the same record through the single
//! assembly path) and the per-stage digests must match.
//!
//! **`SPINE-001` scope:** steps 4 and 5 of the smoke (transition to interior,
//! door activation with 0 double load) are exercised here; the remaining 10
//! steps, the `EventBus` transport of `DoorActivated` and the live-window
//! wiring belong to M34-SPINE (see the handoff).

use daggerfall_parser_bevy_bridge as bridge;
use daggerfall_parser_rs::rdi;

use daggerfall_app::interior_transit::{
    DoorAction, DoorRegistry, DoorSite, LocationRef, SceneError, SceneHandle, SceneId, SceneLoader,
    SceneTarget, Transit, TransitionState,
};

/// One measured check of the probe.
struct Check {
    id: &'static str,
    ok: bool,
    detail: String,
}

/// Loader over the interior M11 assembled — the ONLY loader of this probe.
struct RetailLoader {
    /// Block index the loader holds (directory rank of the subject).
    block: u32,
    interior_position: [f32; 3],
    exterior_position: [f32; 3],
    interior_digest: u64,
    exterior_digest: u64,
    calls: u32,
    refused: bool,
}

impl SceneLoader for RetailLoader {
    fn enter_interior(&mut self, target: SceneTarget) -> Result<SceneHandle, SceneError> {
        self.calls += 1;
        let SceneTarget::Interior { block, .. } = target else {
            return Err(SceneError::LoadRejected {
                block: 0,
                reason: "target is not an interior".to_string(),
            });
        };
        if block != self.block {
            return Err(SceneError::BlockNotFound { block });
        }
        if self.refused {
            return Err(SceneError::LoadRejected {
                block,
                reason: "control: loader refuses the block".to_string(),
            });
        }
        Ok(SceneHandle {
            scene: SceneId::Interior,
            position: self.interior_position,
            digest: self.interior_digest,
        })
    }

    fn leave_interior(&mut self, target: SceneTarget) -> Result<SceneHandle, SceneError> {
        self.calls += 1;
        if target.scene() != SceneId::Exterior {
            return Err(SceneError::LoadRejected {
                block: 0,
                reason: "target is not an exterior".to_string(),
            });
        }
        Ok(SceneHandle {
            scene: SceneId::Exterior,
            position: self.exterior_position,
            digest: self.exterior_digest,
        })
    }
}

/// FNV-1a64 of a mesh (positions + indices) — the exterior stage digest.
fn mesh_digest(mesh: &bridge::MeshSpec) -> u64 {
    let mut bytes = Vec::with_capacity(mesh.positions.len() * 4 + mesh.indices.len() * 4);
    for position in &mesh.positions {
        for component in position {
            bytes.extend_from_slice(&component.to_le_bytes());
        }
    }
    for index in &mesh.indices {
        bytes.extend_from_slice(&index.to_le_bytes());
    }
    rdi::fnv1a64(&bytes)
}

/// Door site of the real interior: the probe's own door (the M11 door scan
/// result, `RetailInteriorReport::door`) expressed on the world frame.
fn door_site(
    interior: &bridge::RetailInterior,
    door: (bridge::Side, f32, f32),
    block: u32,
) -> Option<(DoorSite, [f32; 3])> {
    let (side, lateral_from, lateral_to) = door;
    let (bmin, bmax) = interior.mesh_bounds()?;
    let axis = side.axis();
    let lateral_axis = side.lateral_axis();
    let plane = match side {
        bridge::Side::PosX | bridge::Side::PosZ => bmax[axis],
        bridge::Side::NegX | bridge::Side::NegZ => bmin[axis],
    };
    let lateral = 0.5 * (lateral_from + lateral_to);
    let mut position = [0.0f32; 3];
    position[axis] = plane;
    position[lateral_axis] = lateral;
    position[1] = bridge::EYE_HEIGHT_M;
    let site = DoorSite {
        id: 1,
        building: 0,
        block,
        location: LocationRef::new(0, 0, block),
        world_position: position,
        // The M28 retail-door reach (`m18_window.rs:205`) — no new rule.
        reach_m: 2.5,
    };
    // Street start: mid street strip on the outside of the door.
    let mut start = position;
    start[axis] = plane - side.inward_sign() * bridge::STREET_MARGIN_M * 0.5;
    Some((site, start))
}

/// One full transit over an already assembled interior; returns the transit,
/// its checks and the digests of the two stages.
#[allow(clippy::type_complexity)]
fn run_transit(
    interior: &bridge::RetailInterior,
    block: u32,
    world: &bridge::MeshSpec,
    door: (bridge::Side, f32, f32),
) -> Option<(Transit, Vec<Check>, u64, u64)> {
    let (site, street) = door_site(interior, door, block)?;
    let grid = bridge::CollisionGrid::from_mesh_spec(world, bridge::DEFAULT_CELL_M);
    let doors = DoorRegistry::new(vec![site.clone()]);
    let exterior = SceneTarget::Exterior {
        map_pixel: (0, 0),
        location: LocationRef::new(0, 0, block),
    };
    let interior_bounds = interior.mesh_bounds()?;
    // Interior spawn: the M11 spawn rule (inside the block, eye height).
    let interior_position = [
        0.5 * (interior_bounds.0[0] + interior_bounds.1[0]),
        bridge::EYE_HEIGHT_M,
        0.5 * (interior_bounds.0[2] + interior_bounds.1[2]),
    ];
    let mut loader = RetailLoader {
        block,
        interior_position,
        exterior_position: site.world_position,
        interior_digest: rdi::fnv1a64(&interior.canonical_bytes()),
        exterior_digest: mesh_digest(world),
        calls: 0,
        refused: false,
    };
    let mut checks: Vec<Check> = Vec::new();

    // ── 1. The load is the M11/M4 loader ─────────────────────────────────
    let interior_digest = loader.interior_digest;
    checks.push(Check {
        id: "transit_loader_is_m11",
        ok: interior.block_name.ends_with(".RDB")
            && interior.counts().0 >= 1
            && interior.mesh.triangle_count() > 0
            && !interior.canonical_bytes().is_empty()
            && interior.unresolved_models == 0,
        detail: format!(
            "interior {} from `m11_probe::evaluate_real_interior` → `RdbSources::assemble` → \
             `bridge::build_retail_interior` (interior_retail.rs:299) · {} models/{} lights/{} \
             flats · {} triangles · canonical_bytes {} · FNV {:016x} · second loader: 0",
            interior.block_name,
            interior.counts().0,
            interior.counts().1,
            interior.counts().2,
            interior.mesh.triangle_count(),
            interior.canonical_bytes().len(),
            interior_digest,
        ),
    });

    // ── 2. World → door on foot, through the door span (0 walls) ─────────
    let axis = door.0.axis();
    let inward = door.0.inward_sign();
    let plane = site.world_position[axis];
    let walk = bridge::march_axis(world, &grid, street, axis, inward, plane);
    let lateral = site.world_position[door.0.lateral_axis()];
    let inside_span = lateral >= door.1 - 0.5 && lateral <= door.2 + 0.5;
    checks.push(Check {
        id: "world_to_door_walk",
        ok: walk.reached && walk.violations == 0 && inside_span,
        detail: format!(
            "start ({:.2},{:.2},{:.2}) → door on face {} lateral [{:.1},{:.1}] · {} steps of \
             0.25 m · reached={} · traversals={} (0 req) · oracle samples {} · eye=({:.2},{:.2},{:.2})",
            street[0],
            street[1],
            street[2],
            door.0.label(),
            door.1,
            door.2,
            walk.steps,
            walk.reached,
            walk.violations,
            walk.oracle_samples,
            walk.pos[0],
            walk.pos[1],
            walk.pos[2],
        ),
    });

    // ── 3. Door activation → interior (E, then the G binding) ────────────
    let mut transit = Transit::new(exterior, walk.pos);
    let entered = transit.activate_nearest(DoorAction::Interact, &doors, &mut loader);
    let target = match entered {
        Ok(target) => Some(target),
        Err(error) => {
            checks.push(Check {
                id: "world_to_interior",
                ok: false,
                detail: format!("activation failed: {error}"),
            });
            None
        }
    };
    checks.push(Check {
        id: "world_to_interior",
        ok: target.is_some()
            && transit.scene == SceneId::Interior
            && matches!(transit.transition.state(), TransitionState::Done(SceneTarget::Interior { .. })),
        detail: format!(
            "E on door 1 at {:.1} m → request(Interior{{block:{block}}}) → begin → \
             `enter_interior` → complete → state {} · scene={} · position=({:.2},{:.2},{:.2}) · \
             loads={}",
            site.distance_m(walk.pos),
            transit.transition.state().label(),
            transit.scene.label(),
            transit.position[0],
            transit.position[1],
            transit.position[2],
            transit.loads,
        ),
    });

    // ── 4. `poll()` delivers `Done` exactly once ─────────────────────────
    let first = transit.transition.poll();
    let second = transit.transition.poll();
    checks.push(Check {
        id: "poll_done_once",
        ok: matches!(first, Some(TransitionState::Done(SceneTarget::Interior { .. })))
            && second.is_none(),
        detail: format!(
            "first poll = {} · second poll = None (Done is single-shot) · state keeps {}",
            first
                .as_ref()
                .map_or_else(|| "None".to_string(), TransitionState::label),
            transit.transition.state().label(),
        ),
    });

    // ── 5. Door consumed once · 0 double load ────────────────────────────
    let repeat = transit.activate_door(1, DoorAction::Bash, &doors, &mut loader);
    let repeat_ok = matches!(
        repeat,
        Err(SceneError::NotInScene {
            expected: SceneId::Exterior,
            ..
        })
    );
    checks.push(Check {
        id: "door_consumed_once",
        ok: transit.doors_consumed == 1 && transit.loads == 1 && repeat_ok && loader.calls == 1,
        detail: format!(
            "doors_consumed={} · loads={} · loader calls={} · G inside the interior → {} (0 double load)",
            transit.doors_consumed,
            transit.loads,
            loader.calls,
            repeat
                .as_ref()
                .map(|_| "loaded again".to_string())
                .unwrap_or_else(|error| error.to_string()),
        ),
    });

    // ── 6. Exit → world AT the door ──────────────────────────────────────
    let exited = transit.exit(&mut loader);
    let back_at_door = transit
        .door_position
        .map(|door_position| {
            let dx = transit.position[0] - door_position[0];
            let dz = transit.position[2] - door_position[2];
            (dx * dx + dz * dz).sqrt() < 0.001
        })
        .unwrap_or(false);
    checks.push(Check {
        id: "interior_to_world_at_door",
        ok: exited.is_ok()
            && transit.scene == SceneId::Exterior
            && back_at_door
            && transit.loads == 2
            && transit.steps.len() == 2,
        detail: format!(
            "exit → {} · scene={} · position=({:.2},{:.2},{:.2}) == door position ({}) · \
             loads={} (one per transition) · steps={}",
            exited
                .as_ref()
                .map(|target| target.label())
                .unwrap_or_else(|error| error.to_string()),
            transit.scene.label(),
            transit.position[0],
            transit.position[1],
            transit.position[2],
            transit.door_position.is_some(),
            transit.loads,
            transit.steps.len(),
        ),
    });

    // ── 7. Negative: unknown door ⇒ Failed, 0 mutation ───────────────────
    let mut control = Transit::new(exterior, walk.pos);
    let mut control_loader = RetailLoader {
        block,
        interior_position,
        exterior_position: site.world_position,
        interior_digest,
        exterior_digest: mesh_digest(world),
        calls: 0,
        refused: false,
    };
    let unknown = control.activate_door(99, DoorAction::Interact, &doors, &mut control_loader);
    let unknown_ok = matches!(unknown, Err(SceneError::UnknownDoor { door: 99 }))
        && control.scene == SceneId::Exterior
        && control.loads == 0
        && control.doors_consumed == 0
        && control.steps.is_empty()
        && control.position == walk.pos
        && matches!(
            control.transition.state(),
            TransitionState::Failed(SceneError::UnknownDoor { door: 99 })
        );
    checks.push(Check {
        id: "negative_unknown_door",
        ok: unknown_ok,
        detail: format!(
            "door 99 (absent) → {} · state={} · loads={} · doors_consumed={} · steps={} · \
             position mutated={}",
            unknown
                .as_ref()
                .map(|target| target.label())
                .unwrap_or_else(|error| error.to_string()),
            control.transition.state().label(),
            control.loads,
            control.doors_consumed,
            control.steps.len(),
            control.position != walk.pos,
        ),
    });

    // ── 8. Negative: door out of reach ⇒ Failed, 0 mutation ──────────────
    let mut far = Transit::new(exterior, [walk.pos[0] + 20.0, walk.pos[1], walk.pos[2]]);
    let mut far_loader = RetailLoader {
        block,
        interior_position,
        exterior_position: site.world_position,
        interior_digest,
        exterior_digest: mesh_digest(world),
        calls: 0,
        refused: false,
    };
    let out_of_reach = far.activate_nearest(DoorAction::Bash, &doors, &mut far_loader);
    let out_ok = matches!(out_of_reach, Err(SceneError::NotAdjacent { door: 1, .. }))
        && far.scene == SceneId::Exterior
        && far.loads == 0
        && far.doors_consumed == 0
        && matches!(
            far.transition.state(),
            TransitionState::Failed(SceneError::NotAdjacent { .. })
        );
    checks.push(Check {
        id: "negative_not_adjacent",
        ok: out_ok,
        detail: format!(
            "player 20 m away → {} · state={} · loads={} · doors_consumed={} · 0 scene mutation",
            out_of_reach
                .as_ref()
                .map(|target| target.label())
                .unwrap_or_else(|error| error.to_string()),
            far.transition.state().label(),
            far.loads,
            far.doors_consumed,
        ),
    });

    // ── 9. Negative: the loader refuses ⇒ Failed, 0 mutation ─────────────
    let mut refused = Transit::new(exterior, walk.pos);
    let mut refused_loader = RetailLoader {
        block,
        interior_position,
        exterior_position: site.world_position,
        interior_digest,
        exterior_digest: mesh_digest(world),
        calls: 0,
        refused: true,
    };
    let refused_result =
        refused.activate_nearest(DoorAction::Interact, &doors, &mut refused_loader);
    let refused_ok = matches!(refused_result, Err(SceneError::LoadRejected { .. }))
        && refused.scene == SceneId::Exterior
        && refused.loads == 0
        && refused.steps.is_empty()
        && matches!(
            refused.transition.state(),
            TransitionState::Failed(SceneError::LoadRejected { .. })
        );
    checks.push(Check {
        id: "negative_load_refused",
        ok: refused_ok,
        detail: format!(
            "loader refuses block {block} → {} · state={} · scene={} · steps={} · 0 mutation",
            refused_result
                .as_ref()
                .map(|target| target.label())
                .unwrap_or_else(|error| error.to_string()),
            refused.transition.state().label(),
            refused.scene.label(),
            refused.steps.len(),
        ),
    });

    Some((transit, checks, interior_digest, loader.exterior_digest))
}

/// Runs PROBE-INTERIOR-TRANSIT-001 (M18 B2-b).
pub fn run_interior_transit_001() -> i32 {
    println!(
        "PROBE-INTERIOR-TRANSIT-001 - world → interior → world through a door \
         (M18 B2-b, REQ-L99-043, CR-f716455c)"
    );
    let mut eval = match crate::m11_probe::evaluate_real_interior() {
        Ok(eval) => eval,
        Err(reason) => {
            println!("[FAIL] real interior source: {reason}");
            return 1;
        }
    };
    let Some(door) = eval.report.door else {
        println!("[FAIL] the subject has no real door — the transit needs one");
        return 1;
    };
    let world = bridge::retail_world(&eval.subject);
    let grid = bridge::CollisionGrid::from_mesh_spec(&world, bridge::DEFAULT_CELL_M);
    let block = eval.subject_rank as u32;
    println!(
        "[OK] subject: {} (record #{} of the directory) · face {} lateral [{:.1},{:.1}] m · \
         world {} triangles · grid {} solid cells · loader = M11 assembly path",
        eval.subject.block_name,
        eval.subject_rank + 1,
        door.0.label(),
        door.1,
        door.2,
        world.triangle_count(),
        grid.solid_cell_count(),
    );

    let Some((first, checks, interior_digest, exterior_digest)) =
        run_transit(&eval.subject, block, &world, door)
    else {
        println!("[FAIL] transit could not be composed (geometry without bounds)");
        return 1;
    };
    let mut all_ok = true;
    for check in &checks {
        println!(
            "[{}] {} - {}",
            if check.ok { "OK" } else { "FAIL" },
            check.id,
            check.detail
        );
        all_ok &= check.ok;
    }
    for step in &first.steps {
        println!(
            "     step door={} action={} scene={} position=({:.2},{:.2},{:.2}) digest {:016x}",
            step.door,
            step.action.label(),
            step.scene.label(),
            step.position[0],
            step.position[1],
            step.position[2],
            step.digest,
        );
    }

    // ── 10. The transit TWICE: second assembly of the same record ────────
    // `assemble` is the single assembly path, so the second composition is a
    // real independent build of the same bytes (not a cached handle).
    let second = eval
        .sources
        .assemble(&eval.records[eval.subject_rank])
        .map_err(|reason| format!("second assembly: {reason}"));
    let second_ok = match &second {
        Ok(interior) => {
            let second_world = bridge::retail_world(interior);
            match run_transit(interior, block, &second_world, door) {
                Some((transit, checks, digest, exterior)) => {
                    let same_bytes = interior.canonical_bytes() == eval.subject.canonical_bytes();
                    let same_interior = digest == interior_digest;
                    let same_exterior = exterior == exterior_digest;
                    let same_steps = transit.steps == first.steps;
                    let same_checks = checks.iter().all(|check| check.ok);
                    println!(
                        "[{}] transit_deterministic - two assemblies of {} · \
                         canonical_bytes {} == {} · interior FNV {:016x} vs {:016x} · \
                         exterior FNV {:016x} vs {:016x} · steps identical={} · \
                         second run {}/{} checks",
                        if same_bytes && same_interior && same_exterior && same_steps && same_checks {
                            "OK"
                        } else {
                            "FAIL"
                        },
                        eval.subject.block_name,
                        interior.canonical_bytes().len(),
                        eval.subject.canonical_bytes().len(),
                        digest,
                        interior_digest,
                        exterior,
                        exterior_digest,
                        same_steps,
                        checks.iter().filter(|check| check.ok).count(),
                        checks.len(),
                    );
                    same_bytes && same_interior && same_exterior && same_steps && same_checks
                }
                None => {
                    println!("[FAIL] transit_deterministic - second composition has no bounds");
                    false
                }
            }
        }
        Err(reason) => {
            println!("[FAIL] transit_deterministic - {reason}");
            false
        }
    };
    all_ok &= second_ok;

    // ── SPINE-001 scope: steps 4 and 5 are exercised; 6-12 are M34 ───────
    println!(
        "[{}] spine_steps_4_5 - SPINE-001 step 4 (request(Interior) ⇒ Done once, scene=Interior) \
         and step 5 (DoorActivated ⇒ consumed by the transition, 0 double load) exercised on the \
         real record · deferred to M34-SPINE: steps 1-3 (GameSession/heart clock), 6-12 (save \
         sections), the `EventBus` transport of `DoorActivated` and the live-window wiring",
        if all_ok { "OK" } else { "FAIL" },
    );

    let passed = checks.iter().filter(|check| check.ok).count() + usize::from(second_ok);
    let total = checks.len() + 1;
    if all_ok {
        println!(
            "VERDICT: PROBE-INTERIOR-TRANSIT-001 PASS - {passed}/{total} checks · \
             world→interior→world at the door with 0 traversals · door consumed 1 · \
             0 double load · digest ×2 interior {:016x} exterior {:016x} · 3 negatives Failed \
             with 0 mutation",
            interior_digest, exterior_digest,
        );
        0
    } else {
        println!("VERDICT: PROBE-INTERIOR-TRANSIT-001 FAIL - see [FAIL] rows");
        1
    }
}

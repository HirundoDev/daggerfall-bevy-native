//! M18 · **B1 (O2/O4/O9/O8)** — standalone Anti-Screen probes of the parity
//! wave. They live **outside** the `ALL-001` suite (37 M2-M29 entries; 40
//! since M34, spec §11) and share every composition with the window: 0
//! duplicated logic.
//!
//! | Probe | Sub-WP | What it measures |
//! |---|---|---|
//! | `--probe ORIGIN-001` | B1-a (O2) | the M13 terrain sampling pixel is an input: the default composition carries **0 ocean / 0 beach**, an `--origin` (or ring-scanned) **coastal** pixel carries both and builds the two mesh groups, an **interior** pixel carries neither, `--yaw` reaches the spawn, the `Ocean` band is a sea surface that **cannot be walked into** (two-sided control: the same walk advances on the default map), and **block F** puts the EYE on a band (`--spawn band:ocean|beach|land`): ocean/beach/land viewpoints outside the hull, digest ×2, session state untouched, the CS-08 sea-level coherence and the explicit refusal of a band the composition does not carry (never a fallback to `land`) |
//! | `--probe ATTACK-KEY-001` | B1-c (O9) | the `F` key reaches the production combat session through the real `ButtonInput<KeyCode>` state and the single `m2_kinematics::attack_triggered` mapping: a landed hit drops the target's HP **and** changes the HUD digest, a real miss changes neither |
//! | `--probe MENU-001` | B1-d (O8) | the startup phase is `Menu` with **0 world frames** (the gated bundle that owns the game clock never runs) until `Enter`; the visible panel id is `main-menu` in `Menu` and `hud` in `Playing` (read from the composed scene, never from the title); `Enter` switches to `Playing` and the spawn/HUD are the default composition's |
//!
//! Common contract of the three:
//! - **headless** (0 window, 0 GPU, 0 device audio);
//! - **digests FNV-1a 64 ×2** (two compositions / two states) with an explicit
//!   comparison — a difference fails the probe;
//! - an **explicit negative control** printed with its verdict;
//! - `EXIT=0` only if every row is `PASS`; `EXIT=2` when the requested retail
//!   datum is not there (ARENA2 / `WOODS.WLD` / a coastal pixel inside the
//!   documented ring), `EXIT=1` on a measured failure.
//!
//! **Custody (INV-L99-6):** everything is read from the user's ARENA2 at run
//! time; 0 retail bytes are packaged.

use bevy::input::keyboard::KeyCode;
use bevy::input::mouse::MouseButton;
use bevy::prelude::{ButtonInput, IntoScheduleConfigs, Res, ResMut, Resource, Schedule, World};

use crate::m18_demo::{
    band_counts_at, compose_demo_fight, compose_demo_with, compose_demo_with_overrides,
    demo_player_strike, find_coastal_pixel, parse_overrides, read_woods, CoastalPixel, DemoError,
    DemoScene, DemoTerrain, M18Overrides, SpawnBand, BAND_SPAWN_HULL_MARGIN_M,
    PEAK_STAND_MIN_RELIEF_M,
};
use crate::m18_phase::{phase_after_enter, phase_playing, AppPhase};
use daggerfall_behavior::locomotion::LocomotionWorld;
use daggerfall_parser_bevy_bridge::{EYE_HEIGHT_M, PLAYER_RADIUS_M};
use daggerfall_parser_rs::woods::{TerrainBand, HEIGHTMAP_DIM};

/// Exit code of "retail datum missing" (same convention as the rest of the
/// suite: 2 = missing data, 1 = measured failure).
const EXIT_MISSING_DATA: i32 = 2;
/// Exit code of a measured failure.
const EXIT_FAIL: i32 = 1;

/// Composition failure → the probe verdict and the exit code (never a panic).
fn composition_failed(label: &str, err: &DemoError) -> i32 {
    match err {
        DemoError::MissingData(reason) => {
            println!("ARENA2 not accessible: {reason}");
            println!("VERDICT: {label} FAIL - retail data missing");
            EXIT_MISSING_DATA
        }
        DemoError::Invalid(reason) => {
            println!("[FAIL] composition - {reason}");
            println!("VERDICT: {label} FAIL - invalid composition");
            EXIT_FAIL
        }
    }
}

/// Composes the demo twice and prints the determinism row.
///
/// Returns `(scene, digest)` of the first composition plus whether the two
/// digests matched; the caller owns the verdict row.
fn compose_twice(
    origin: Option<[i32; 2]>,
    yaw: Option<f32>,
) -> Result<(DemoScene, u64, bool), DemoError> {
    let first = compose_demo_with(origin, yaw)?;
    let second = compose_demo_with(origin, yaw)?;
    let digest = first.digest();
    Ok((first, digest, digest == second.digest()))
}

/// `(ocean, beach, land)` of the composition's terrain bands.
fn counts(scene: &DemoScene) -> (usize, usize, usize) {
    scene.terrain.band_counts()
}

/// Composes the demo twice with the full override set (band spawn included) and
/// prints whether the two digests match.
fn compose_overrides_twice(
    overrides: &M18Overrides,
) -> Result<(DemoScene, u64, bool), DemoError> {
    let first = compose_demo_with_overrides(overrides)?;
    let second = compose_demo_with_overrides(overrides)?;
    let digest = first.digest();
    Ok((first, digest, digest == second.digest()))
}

/// `(min, max, count)` of the mesh heights over the ocean samples. The sea mesh
/// is the declared plane `OCEAN_ELEVATION − datum` scaled by the datum blend
/// (`0..=1`, 0 at the hull edge), so every ocean sample lies BETWEEN the plane
/// and the city datum (0) — the blend never overshoots the sea level (CS-08).
fn sea_mesh_range(terrain: &DemoTerrain) -> (f32, f32, usize) {
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    let mut count = 0usize;
    for (at, band) in terrain.bands.iter().enumerate() {
        if *band == TerrainBand::Ocean {
            count += 1;
            min = min.min(terrain.heights[at]);
            max = max.max(terrain.heights[at]);
        }
    }
    if count == 0 {
        (0.0, 0.0, 0)
    } else {
        (min, max, count)
    }
}

/// Band labels of the terrain groups (evidence that the mesh groups exist).
fn group_bands(scene: &DemoScene) -> Vec<&'static str> {
    scene
        .terrain
        .groups
        .iter()
        .map(|group| group.band.name())
        .collect()
}

// ── PROBE ORIGIN-001 (B1-a / O2) ───────────────────────────────────────────

/// `--probe ORIGIN-001` — the terrain sampling pixel is actionable and the sea
/// it produces cannot be walked into.
///
/// Blocks:
/// 1. **default**: the capital pixel ⇒ `ocean 0 / beach 0` and digest ×2 equal
///    (the pre-B1 behavior, byte-identical);
/// 2. **coastal**: `--origin <x>,<y>` (or the ring's nearest coastal pixel when
///    the flag is absent) ⇒ `ocean>0 ∧ beach>0`, both bands present in
///    `terrain.groups`, digest ×2 equal;
/// 3. **negative control**: an **interior** pixel (first ring pixel, Chebyshev
///    ≥ 1, with `ocean 0 ∧ beach 0`) ⇒ still `0 ocean / 0 beach` on a pixel
///    that is not the base one (m-1);
/// 4. **yaw**: `--yaw` reaches the spawn (deterministic ×2, digest differs);
/// 5. **CS-08 water**: the coastal composition carries a sea level and water
///    quads; walking 24 m toward the sea is blocked at the shoreline, and the
///    SAME walk on the default composition (0 ocean) advances — the negative
///    control that makes the block non-tautological.
pub fn run_origin_001() -> i32 {
    println!(
        "PROBE-ORIGIN-001 - B1-a origin/map-pixel accionable (M13 sampling pixel: capital vs \
         coastal vs interior)"
    );
    let mut all_ok = true;

    // ── block A · default composition (today's behavior) ───────────────
    let default = match compose_twice(None, None) {
        Ok(triple) => triple,
        Err(err) => return composition_failed("PROBE-ORIGIN-001", &err),
    };
    let (first, digest_default, deterministic_default) = default;
    let (ocean_d, beach_d, land_d) = counts(&first);
    let default_ok = ocean_d == 0 && beach_d == 0 && deterministic_default;
    println!(
        "[{}] default_capital - map-pixel ({},{}) · bands ocean {ocean_d} / beach {beach_d} / \
         land {land_d} · groups {:?} · digest {digest_default:016x} ×2 equal={deterministic_default}",
        if default_ok { "OK" } else { "FAIL" },
        first.map_pixel[0],
        first.map_pixel[1],
        group_bands(&first),
    );
    all_ok &= default_ok;

    // ── block B · coastal origin ───────────────────────────────────────
    let woods = match read_woods() {
        Ok(woods) => woods,
        Err(err) => {
            println!("[FAIL] coastal_ring - WOODS.WLD unavailable ({err})");
            println!("VERDICT: PROBE-ORIGIN-001 FAIL - retail data missing");
            return EXIT_MISSING_DATA;
        }
    };
    let args: Vec<String> = std::env::args().collect();
    let cli_origin = match parse_overrides(&args) {
        Ok(overrides) => overrides.origin,
        Err(err) => {
            println!("[FAIL] origin_override - {err}");
            println!("VERDICT: PROBE-ORIGIN-001 FAIL - unparsable --origin");
            return EXIT_FAIL;
        }
    };
    let coastal = match cli_origin {
        Some(pixel) => {
            let (ocean, beach, land) = band_counts_at(&woods, pixel);
            CoastalPixel {
                pixel,
                ocean,
                beach,
                land,
            }
        }
        None => match find_coastal_pixel(&woods, first.map_pixel) {
            Some(found) => found,
            None => {
                println!(
                    "[FAIL] coastal_ring - no coastal pixel in ring (±{} px around ({},{})): the \
                     B1-a control cannot be measured",
                    crate::m18_demo::COASTAL_RING,
                    first.map_pixel[0],
                    first.map_pixel[1],
                );
                println!("VERDICT: PROBE-ORIGIN-001 FAIL - retail data missing (no coast in ring)");
                return EXIT_MISSING_DATA;
            }
        },
    };
    println!(
        "M18 VE: origin-override px=({},{}) — source {} · Chebyshev {}/{} · full-grid samples \
         ocean {} / beach {} / land {}",
        coastal.pixel[0],
        coastal.pixel[1],
        if cli_origin.is_some() {
            "--origin / DF_M18_ORIGIN"
        } else {
            "ring scan (Chebyshev, then x,y ascending)"
        },
        coastal.chebyshev_from(first.map_pixel),
        crate::m18_demo::COASTAL_RING,
        coastal.ocean,
        coastal.beach,
        coastal.land,
    );
    let coastal_scene = match compose_twice(Some(coastal.pixel), None) {
        Ok(triple) => triple,
        Err(err) => return composition_failed("PROBE-ORIGIN-001", &err),
    };
    let (coastal_scene, digest_coastal, deterministic_coastal) = coastal_scene;
    let (ocean_c, beach_c, land_c) = counts(&coastal_scene);
    let bands = group_bands(&coastal_scene);
    let coastal_ok = ocean_c > 0
        && beach_c > 0
        && coastal_scene.terrain.origin_pixel == coastal.pixel
        && bands.contains(&"ocean")
        && bands.contains(&"beach")
        && deterministic_coastal;
    println!(
        "[{}] coastal_origin - map-pixel ({},{}) · bands ocean {ocean_c} / beach {beach_c} / \
         land {land_c} · groups {:?} · {} triangles · digest {digest_coastal:016x} ×2 \
         equal={deterministic_coastal}",
        if coastal_ok { "OK" } else { "FAIL" },
        coastal_scene.terrain.origin_pixel[0],
        coastal_scene.terrain.origin_pixel[1],
        bands,
        coastal_scene.terrain.triangles,
    );
    all_ok &= coastal_ok;

    // ── block C · negative control: interior pixel ─────────────────────
    let interior = find_interior_pixel(&woods, first.map_pixel);
    match interior {
        Some(pixel) => {
            let interior_scene = match compose_twice(Some(pixel), None) {
                Ok(triple) => triple,
                Err(err) => return composition_failed("PROBE-ORIGIN-001", &err),
            };
            let (interior_scene, digest_interior, deterministic_interior) = interior_scene;
            let (ocean_i, beach_i, land_i) = counts(&interior_scene);
            let chebyshev = (pixel[0] - first.map_pixel[0])
                .abs()
                .max((pixel[1] - first.map_pixel[1]).abs());
            // m-1: the control is a DIFFERENT pixel (Chebyshev ≥ 1) that really
            // re-sampled the terrain — not the base pixel measuring itself.
            let control_ok = ocean_i == 0
                && beach_i == 0
                && deterministic_interior
                && chebyshev >= 1
                && interior_scene.terrain.origin_pixel == pixel;
            println!(
                "[{}] control_negative_interior - map-pixel ({},{}) (Chebyshev {chebyshev}, \
                 independent of the base) · bands ocean {ocean_i} / beach {beach_i} / land \
                 {land_i} · groups {:?} · digest {digest_interior:016x} ×2 \
                 equal={deterministic_interior}",
                if control_ok { "OK" } else { "FAIL" },
                pixel[0],
                pixel[1],
                group_bands(&interior_scene),
            );
            all_ok &= control_ok;
        }
        None => {
            println!(
                "[FAIL] control_negative_interior - no interior pixel in ring (±{} px around \
                 ({},{}))",
                crate::m18_demo::COASTAL_RING,
                first.map_pixel[0],
                first.map_pixel[1]
            );
            all_ok = false;
        }
    }

    // ── block D · yaw override (B1-a, m-5) ─────────────────────────────
    let yaw_target = 1.0f32;
    match compose_twice(None, Some(yaw_target)) {
        Ok((yaw_scene, digest_yaw, deterministic_yaw)) => {
            let yaw_ok = (yaw_scene.spawn.yaw - yaw_target).abs() < 1e-6
                && deterministic_yaw
                && digest_yaw != digest_default;
            println!(
                "[{}] yaw_override - --yaw {yaw_target:.4} rad · spawn yaw {:.4} · eye \
                 ({:.2},{:.2},{:.2}) · digest {digest_yaw:016x} ×2 equal={deterministic_yaw} · \
                 differs from the default digest={}",
                if yaw_ok { "OK" } else { "FAIL" },
                yaw_scene.spawn.yaw,
                yaw_scene.spawn.eye[0],
                yaw_scene.spawn.eye[1],
                yaw_scene.spawn.eye[2],
                digest_yaw != digest_default,
            );
            all_ok &= yaw_ok;
        }
        Err(err) => return composition_failed("PROBE-ORIGIN-001", &err),
    }

    // ── block E · CS-08: sea surface + shoreline block ─────────────────
    // The `Ocean` band is the sea surface (the sampler clamps oceanic samples
    // to `OCEAN_ELEVATION`, `DefaultTerrainSampler.cs:115-118`) and the
    // collision path refuses it. Two-sided control below: the SAME walk on the
    // composition without ocean must advance.
    let surface_ok = coastal_scene.terrain.sea_level_m.is_some()
        && coastal_scene.terrain.water_quads > 0
        && first.terrain.sea_level_m.is_none()
        && first.terrain.water_quads == 0;
    println!(
        "[{}] sea_surface - coastal sea level {:.2} m (OCEAN_ELEVATION − datum) · {} ocean quads \
         (water cells) · default composition: {} · 0 ocean samples",
        if surface_ok { "OK" } else { "FAIL" },
        coastal_scene.terrain.sea_level_m(),
        coastal_scene.terrain.water_quads,
        match first.terrain.sea_level_m {
            Some(level) => format!("sea level {level:.2} m"),
            None => "no water".to_string(),
        },
    );
    all_ok &= surface_ok;

    const WALK_STEPS: usize = 50;
    const WALK_STEP_M: f32 = 0.3;
    let requested = WALK_STEPS as f32 * WALK_STEP_M;
    match coast_pair(&coastal_scene.terrain) {
        Some((from, dir, ocean_at)) => {
            let ocean_distance =
                ((ocean_at[0] - from[0]).powi(2) + (ocean_at[2] - from[2]).powi(2)).sqrt();
            let (blocked_pos, blocked_hits, blocked_travel) =
                advance(&coastal_scene, from, dir, WALK_STEPS, WALK_STEP_M);
            let blocked_band = coastal_scene
                .terrain
                .band_at(blocked_pos[0], blocked_pos[2])
                .name();
            let coast_ok = blocked_hits > 0
                && !coastal_scene
                    .terrain
                    .is_water(blocked_pos[0], blocked_pos[2])
                && blocked_travel < requested * 0.5;
            println!(
                "[{}] water_not_walkable - coastal: start ({:.2},{:.2}) band {} · direction \
                 ({:.0},{:.0}) · {WALK_STEPS} steps × {WALK_STEP_M} m toward the sea (ocean sample \
                 {ocean_distance:.1} m away) · blocked {blocked_hits} steps · travelled \
                 {blocked_travel:.2} m of {requested:.2} m · final ({:.2},{:.2}) band \
                 {blocked_band} — the shoreline holds",
                if coast_ok { "OK" } else { "FAIL" },
                from[0],
                from[2],
                coastal_scene.terrain.band_at(from[0], from[2]).name(),
                dir[0],
                dir[1],
                blocked_pos[0],
                blocked_pos[2],
            );
            all_ok &= coast_ok;

            // Negative control of the block: same start, same direction, same
            // count on the DEFAULT composition (0 ocean samples). If the block
            // were a blanket "terrain does not move", this row fails.
            let (free_pos, free_hits, free_travel) =
                advance(&first, from, dir, WALK_STEPS, WALK_STEP_M);
            let free_band = first.terrain.band_at(free_pos[0], free_pos[2]).name();
            let control_ok = free_hits == 0
                && (free_travel - requested).abs() < 0.05
                && !first.terrain.is_water(free_pos[0], free_pos[2]);
            println!(
                "[{}] control_negative_no_ocean - default composition, same start ({:.2},{:.2}) \
                 and direction ({:.0},{:.0}): blocked {free_hits} steps · travelled {free_travel:.2} \
                 m of {requested:.2} m · final ({:.2},{:.2}) band {free_band} — without the block \
                 the walk advances (the row above is not tautological)",
                if control_ok { "OK" } else { "FAIL" },
                from[0],
                from[2],
                dir[0],
                dir[1],
                free_pos[0],
                free_pos[2],
            );
            all_ok &= control_ok;
        }
        None => {
            println!(
                "[FAIL] water_not_walkable - no walkable ring sample next to an Ocean sample in the \
                 coastal composition"
            );
            all_ok = false;
        }
    }

    // The city hull never moves: the default and the coastal composition share
    // the retail map-pixel, the spawn and the collision grid summary.
    let hull_ok = coastal_scene.map_pixel == first.map_pixel
        && coastal_scene.spawn.eye == first.spawn.eye
        && coastal_scene.grid.solid_cell_count() == first.grid.solid_cell_count();
    println!(
        "[{}] city_frame - map-pixel ({},{}) · spawn=({:.2},{:.2},{:.2}) yaw={:.2} · {} solid \
         cells — identical with the coastal terrain origin",
        if hull_ok { "OK" } else { "FAIL" },
        coastal_scene.map_pixel[0],
        coastal_scene.map_pixel[1],
        coastal_scene.spawn.eye[0],
        coastal_scene.spawn.eye[1],
        coastal_scene.spawn.eye[2],
        coastal_scene.spawn.yaw,
        coastal_scene.grid.solid_cell_count(),
    );
    all_ok &= hull_ok;

    // ── block F · CS-08/09/10: spawn by terrain band ───────────────────
    // The composition can carry the bands, but from the retail spawn it can
    // neither show nor reach them (city floor at y=0 over the band, hull
    // occlusion). `--spawn band:<…>` moves the EYE onto the band; this block
    // measures the resolved viewpoint, its determinism ×2, that the session
    // state does not move with it, and the two-sided CS-08 walk.
    let mut band_spawns: Vec<(&'static str, DemoScene)> = Vec::new();
    for band in [SpawnBand::Ocean, SpawnBand::Beach, SpawnBand::Land] {
        let overrides = M18Overrides {
            origin: Some(coastal.pixel),
            yaw: None,
            spawn: Some(band),
        };
        match compose_overrides_twice(&overrides) {
            Ok((scene, digest_band, deterministic_band)) => {
                let Some(resolved) = scene.spawn_band else {
                    println!(
                        "[FAIL] spawn_band_{} - the override did not record a viewpoint",
                        band.id()
                    );
                    all_ok = false;
                    continue;
                };
                let under_eye = scene.terrain.band_at(resolved.eye[0], resolved.eye[2]);
                let outside_ok = resolved.outside_hull_m + 1e-3 >= BAND_SPAWN_HULL_MARGIN_M;
                let eye_ok = (resolved.eye[1] - (resolved.surface_m + EYE_HEIGHT_M)).abs()
                    <= 1e-3
                    && resolved.eye[1] + 1e-3 >= resolved.surface_m;
                let band_ok = under_eye == band.terrain_band();
                // CS-08/09: the frame is open sea; CS-10: the silhouette rises
                // above the vantage point (never a flat stand).
                let frame_ok = match band {
                    SpawnBand::Ocean | SpawnBand::Beach => resolved.water_samples > 0,
                    SpawnBand::Land => resolved.relief_m >= PEAK_STAND_MIN_RELIEF_M,
                };
                // The session state is the default composition's: hull, grid,
                // NPC placement and HUD do not move with the viewpoint.
                let state_ok = scene.city.digest() == coastal_scene.city.digest()
                    && scene.grid.solid_cell_count()
                        == coastal_scene.grid.solid_cell_count()
                    && scene.terrain.water_quads == coastal_scene.terrain.water_quads
                    && scene.terrain.origin_pixel == coastal_scene.terrain.origin_pixel
                    && scene.m20_hud_digest() == coastal_scene.m20_hud_digest()
                    && scene
                        .npcs
                        .iter()
                        .map(|npc| npc.position)
                        .eq(coastal_scene.npcs.iter().map(|npc| npc.position));
                let visible = digest_band != digest_default;
                let ok = deterministic_band
                    && outside_ok
                    && eye_ok
                    && band_ok
                    && state_ok
                    && visible
                    && frame_ok
                    && resolved.target_distance_m > 1.0;
                println!(
                    "[{}] spawn_band_{} - --spawn band:{} · eye=({:.2},{:.2},{:.2}) · at \
                     ({:.1},{:.1}) m = {:.1} m outside the hull (≥ {:.0} m) · band under the eye \
                     {} · surface {:.2} m = mesh {:.2} m · sea level {} · yaw {:.4} rad \
                     ({:.1}°) toward ({:.1},{:.1}) {:.1} m away · open water {} samples / \
                     relief {:.1} m · open run {:.2} m · digest {digest_band:016x} ×2 \
                     equal={deterministic_band} · differs from the default digest={visible} · \
                     state untouched={state_ok}",
                    if ok { "OK" } else { "FAIL" },
                    band.id(),
                    band.id(),
                    resolved.eye[0],
                    resolved.eye[1],
                    resolved.eye[2],
                    resolved.at[0],
                    resolved.at[1],
                    resolved.outside_hull_m,
                    BAND_SPAWN_HULL_MARGIN_M,
                    under_eye.name(),
                    resolved.surface_m,
                    resolved.ground_m,
                    match resolved.sea_level_m {
                        Some(level) => format!("{level:.2} m"),
                        None => "none".to_string(),
                    },
                    resolved.yaw,
                    resolved.yaw.to_degrees(),
                    resolved.target[0],
                    resolved.target[1],
                    resolved.target_distance_m,
                    resolved.water_samples,
                    resolved.relief_m,
                    resolved.open_run_m,
                );
                all_ok &= ok;
                if band == SpawnBand::Ocean {
                    let (min_ocean, max_ocean, ocean_samples) = sea_mesh_range(&scene.terrain);
                    let level = scene.terrain.sea_level_m();
                    let (lo, hi) = if level >= 0.0 {
                        (0.0, level)
                    } else {
                        (level, 0.0)
                    };
                    let sea_ok = scene.terrain.sea_level_m.is_some()
                        && ocean_samples > 0
                        && min_ocean >= lo - 1e-3
                        && max_ocean <= hi + 1e-3
                        && !scene.locomotion_world().submerged(resolved.eye)
                        && scene.terrain.is_water(resolved.eye[0], resolved.eye[2]);
                    println!(
                        "[{}] sea_level_coherent - declared sea level {level:.2} m \
                         (OCEAN_ELEVATION − datum) · {ocean_samples} ocean mesh samples · mesh \
                         range {min_ocean:.2}..{max_ocean:.2} m inside [{}..{}] (plane, datum) · \
                         eye above the water (not submerged) · the eye cell IS water · {} water \
                         quads",
                        if sea_ok { "OK" } else { "FAIL" },
                        lo,
                        hi,
                        scene.terrain.water_quads,
                    );
                    all_ok &= sea_ok;
                }
                // All three scenes are kept: the CS-08 two-sided control needs a
                // walkable standpoint of the same composition to compare against.
                band_spawns.push((band.id(), scene));
            }
            Err(err) => {
                println!(
                    "[FAIL] spawn_band_{} - --spawn band:{} could not be resolved: {err}",
                    band.id(),
                    band.id()
                );
                all_ok = false;
            }
        }
    }

    // CS-08 · two-sided walk: from the ocean viewpoint every forward step is
    // water (blocked, position unchanged) while the beach viewpoint of the SAME
    // composition advances sideways along the strip. A blanket "the terrain does
    // not move" would fail the second row.
    if let Some((_, ocean_scene)) = band_spawns.iter().find(|(id, _)| *id == "ocean") {
        if let Some(ocean_view) = ocean_scene.spawn_band {
            let dir = [-ocean_view.yaw.sin(), -ocean_view.yaw.cos()];
            let (end, blocked_steps, travelled) =
                advance(ocean_scene, ocean_view.eye, dir, WALK_STEPS, WALK_STEP_M);
            let end_band = ocean_scene.terrain.band_at(end[0], end[2]).name();
            let ok = blocked_steps == WALK_STEPS
                && travelled <= 0.05
                && ocean_scene.terrain.is_water(end[0], end[2]);
            println!(
                "[{}] water_not_walkable_spawn - from the ocean viewpoint ({:.1},{:.1}) along the \
                 yaw ({:.0},{:.0}) · {WALK_STEPS} steps × {WALK_STEP_M} m · blocked \
                 {blocked_steps}/{WALK_STEPS} · travelled {travelled:.2} m · final \
                 ({:.2},{:.2}) band {end_band} — the eye cannot enter the sea",
                if ok { "OK" } else { "FAIL" },
                ocean_view.at[0],
                ocean_view.at[1],
                dir[0],
                dir[1],
                end[0],
                end[2],
            );
            all_ok &= ok;
        }
    } else {
        println!("[FAIL] water_not_walkable_spawn - no ocean viewpoint to walk from");
        all_ok = false;
    }
    if let Some((_, beach_scene)) = band_spawns.iter().find(|(id, _)| *id == "beach") {
        if let Some(beach_view) = beach_scene.spawn_band {
            // The strip must be walkable SOMEWHERE: the 8 compass directions from
            // the beach viewpoint, best travel wins. Otherwise the row above would
            // only be measuring a blanket "the terrain never moves".
            let dirs: [[f32; 2]; 8] = [
                [1.0, 0.0],
                [-1.0, 0.0],
                [0.0, 1.0],
                [0.0, -1.0],
                [1.0, 1.0],
                [1.0, -1.0],
                [-1.0, 1.0],
                [-1.0, -1.0],
            ];
            let mut best = (beach_view.eye, WALK_STEPS, 0.0f32, [1.0f32, 0.0f32]);
            for dir in dirs {
                let len = (dir[0] * dir[0] + dir[1] * dir[1]).sqrt();
                let unit = [dir[0] / len, dir[1] / len];
                let (end, hits, travelled) =
                    advance(beach_scene, beach_view.eye, unit, WALK_STEPS, WALK_STEP_M);
                if travelled > best.2 {
                    best = (end, hits, travelled, unit);
                }
            }
            let (mut end, mut hits, mut travelled, mut dir_used) = best;
            let mut from = beach_view;
            let mut label = "beach viewpoint";
            if travelled <= 1.0 {
                // The sand strip has no walkable step from its CS-09 standpoint
                // (isolated sliver): the control moves to the LAND standpoint of
                // the same composition and the same collision path.
                let land_view = band_spawns
                    .iter()
                    .find(|(id, _)| *id == "land")
                    .and_then(|(_, scene)| scene.spawn_band);
                if let Some(land_view) = land_view {
                    let mut land_best = (land_view.eye, WALK_STEPS, 0.0f32, [1.0f32, 0.0f32]);
                    for dir in dirs {
                        let len = (dir[0] * dir[0] + dir[1] * dir[1]).sqrt();
                        let unit = [dir[0] / len, dir[1] / len];
                        let (end, hits, travelled) =
                            advance(beach_scene, land_view.eye, unit, WALK_STEPS, WALK_STEP_M);
                        if travelled > land_best.2 {
                            land_best = (end, hits, travelled, unit);
                        }
                    }
                    if land_best.2 > travelled {
                        (end, hits, travelled, dir_used) = land_best;
                        from = land_view;
                        label = "land viewpoint (beach strip has no walkable step)";
                    }
                }
            }
            let end_band = beach_scene.terrain.band_at(end[0], end[2]).name();
            // What the viewpoint itself stands on, so a 0 m row is readable: the
            // band of the four axis neighbours at one step and their walkability.
            let probe_of = |dx: f32, dz: f32| {
                let x = from.eye[0] + dx;
                let z = from.eye[2] + dz;
                format!(
                    "({dx:+.1},{dz:+.1})={}{}",
                    beach_scene.terrain.band_at(x, z).name(),
                    if beach_scene.terrain.walkable_at(x, z) {
                        ""
                    } else {
                        "/water"
                    }
                )
            };
            let ok = travelled > 1.0 && !beach_scene.terrain.is_water(end[0], end[2]);
            println!(
                "[{}] control_negative_shoreline_walk - from the {label} ({:.1},{:.1}) the best of \
                 8 directions ({:.2},{:.2}) · {WALK_STEPS} steps × {WALK_STEP_M} m · blocked \
                 {hits}/{WALK_STEPS} · travelled {travelled:.2} m · final ({:.2},{:.2}) band \
                 {end_band} · stand: water {} samples / walkable {} m · one step: {} {} {} {} — the same collision path \
                 advances off the water (row above is not tautological)",
                if ok { "OK" } else { "FAIL" },
                from.at[0],
                from.at[1],
                dir_used[0],
                dir_used[1],
                end[0],
                end[2],
                from.water_samples,
                from.walkable_run_m,
                probe_of(WALK_STEP_M, 0.0),
                probe_of(-WALK_STEP_M, 0.0),
                probe_of(0.0, WALK_STEP_M),
                probe_of(0.0, -WALK_STEP_M),
            );
            all_ok &= ok;
        }
    } else {
        println!("[FAIL] control_negative_shoreline_walk - no beach viewpoint to walk from");
        all_ok = false;
    }

    // Negative control of the bands themselves: the DEFAULT composition carries
    // 0 ocean and 0 beach, so those overrides must be an explicit error naming
    // the band — never a silent fallback to `land` (which the same composition
    // CAN provide: the third row proves the refusal is band-specific).
    for band in [SpawnBand::Ocean, SpawnBand::Beach] {
        let overrides = M18Overrides {
            origin: None,
            yaw: None,
            spawn: Some(band),
        };
        match compose_demo_with_overrides(&overrides) {
            Ok(scene) => {
                println!(
                    "[FAIL] control_negative_band_{}_absent - the default composition (0 {}) built \
                     a viewpoint at ({:.2},{:.2}) instead of refusing it",
                    band.id(),
                    band.id(),
                    scene.spawn.eye[0],
                    scene.spawn.eye[2],
                );
                all_ok = false;
            }
            Err(DemoError::Invalid(reason)) => {
                let explicit = reason.contains(&format!("band:{}", band.id()))
                    && reason.contains(&format!("0 {} samples", band.id()))
                    && reason.contains("refusing to fall back to another band");
                println!(
                    "[{}] control_negative_band_{}_absent - --spawn band:{} on the default \
                     composition (0 {} samples, map-pixel ({},{})) ⇒ explicit refusal: \
                     \"{reason}\"",
                    if explicit { "OK" } else { "FAIL" },
                    band.id(),
                    band.id(),
                    band.id(),
                    first.map_pixel[0],
                    first.map_pixel[1],
                );
                all_ok &= explicit;
            }
            Err(err) => {
                println!(
                    "[FAIL] control_negative_band_{}_absent - refusal is not the documented one: \
                     {err}",
                    band.id()
                );
                all_ok = false;
            }
        }
    }
    // `land` on the default composition: the control that shows the refusal
    // above is about the missing band, not about band spawns being broken.
    match compose_demo_with_overrides(&M18Overrides {
        origin: None,
        yaw: None,
        spawn: Some(SpawnBand::Land),
    }) {
        Ok(scene) => {
            let resolved = scene.spawn_band;
            let ok = resolved.is_some_and(|view| {
                view.band == SpawnBand::Land
                    && scene.terrain.band_at(view.eye[0], view.eye[2]) == TerrainBand::Land
                    && view.outside_hull_m + 1e-3 >= BAND_SPAWN_HULL_MARGIN_M
            });
            println!(
                "[{}] control_band_land_present - --spawn band:land on the DEFAULT composition \
                 resolves ({}) — the refusals above are about the missing band",
                if ok { "OK" } else { "FAIL" },
                resolved.map_or_else(
                    || "no viewpoint".to_string(),
                    |view| format!(
                        "eye=({:.2},{:.2},{:.2}) · {:.1} m outside the hull · band under the eye {}",
                        view.eye[0],
                        view.eye[1],
                        view.eye[2],
                        view.outside_hull_m,
                        scene.terrain.band_at(view.eye[0], view.eye[2]).name()
                    )
                ),
            );
            all_ok &= ok;
        }
        Err(err) => {
            println!("[FAIL] control_band_land_present - land spawn failed: {err}");
            all_ok = false;
        }
    }

    if all_ok {
        println!(
            "VERDICT: PROBE-ORIGIN-001 PASS - default 0/0 bands (digest {digest_default:016x} ×2) · \
             coastal origin ({},{}) ocean {ocean_c} / beach {beach_c} (digest \
             {digest_coastal:016x} ×2) · interior control 0/0 · city frame untouched · band spawn \
             ocean/beach/land resolved (CS-08/09/10) · band absent ⇒ explicit refusal",
            coastal.pixel[0], coastal.pixel[1]
        );
        0
    } else {
        println!("VERDICT: PROBE-ORIGIN-001 FAIL - see [FAIL] rows");
        EXIT_FAIL
    }
}

/// First map pixel of the ring (same deterministic order as
/// [`find_coastal_pixel`]) whose **full** grid carries neither ocean nor beach
/// and that is **not** the base pixel itself.
///
/// This is the negative control of `ORIGIN-001`: an interior pixel must give
/// `0 ocean / 0 beach` and build neither coastal mesh group. The `Chebyshev ≥ 1`
/// guard is the m-1 fix — the base pixel already carries `0/0`, so accepting it
/// (Chebyshev 0) made the control a duplicate of block A instead of an
/// independent measurement.
fn find_interior_pixel(
    woods: &daggerfall_parser_rs::woods::WoodsMap,
    base: [i32; 2],
) -> Option<[i32; 2]> {
    let ring = crate::m18_demo::COASTAL_RING;
    let mut candidates: Vec<([i32; 2], i32)> = Vec::new();
    for dx in -ring..=ring {
        for dy in -ring..=ring {
            candidates.push(([base[0] + dx, base[1] + dy], dx.abs().max(dy.abs())));
        }
    }
    candidates.sort_by(|left, right| {
        left.1
            .cmp(&right.1)
            .then(left.0[0].cmp(&right.0[0]))
            .then(left.0[1].cmp(&right.0[1]))
    });
    candidates.into_iter().find_map(|(pixel, chebyshev)| {
        // Chebyshev 0 IS the base pixel: it cannot be its own control.
        if chebyshev == 0 {
            return None;
        }
        let (ocean, beach, _) = band_counts_at(woods, pixel);
        (ocean == 0 && beach == 0).then_some(pixel)
    })
}

/// Walkable ring sample that has an `Ocean` sample immediately next to it and
/// whose walk direction points **away from the city rectangle**: returns
/// `(start position, unit walk direction, ocean sample position)`.
///
/// The direction is away from the rectangle so the whole walk stays on the M13
/// terrain in BOTH compositions (a walk toward the city would be stopped by the
/// retail grid, which would make the control measure the wall, not the water).
/// Deterministic order: `iz`, then `ix`, then `+x, −x, +z, −z` (0 RNG, 0 clock).
fn coast_pair(terrain: &crate::m18_demo::DemoTerrain) -> Option<([f32; 3], [f32; 2], [f32; 3])> {
    let dim = terrain.dim as i32;
    let spacing = daggerfall_parser_bevy_bridge::SAMPLE_SPACING_M;
    // Distance (in samples, Chebyshev) from the city rectangle `0..=128`.
    let axis_distance = |global: i32| -> i32 {
        let last = HEIGHTMAP_DIM as i32 - 2;
        if global < 0 {
            -global
        } else if global > last {
            global - last
        } else {
            0
        }
    };
    let rect_distance = |x: i32, z: i32| axis_distance(x).max(axis_distance(z));
    let directions: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
    // `WALK_STEPS × WALK_STEP_M` = 15 m ⇒ 3 samples of headroom is enough.
    const HEADROOM: i32 = 3;
    for iz in 1..dim - 1 {
        for ix in 1..dim - 1 {
            let at = iz as usize * terrain.dim + ix as usize;
            if terrain.bands[at] == TerrainBand::Ocean {
                continue;
            }
            let (global_x, global_z) = (terrain.first_sample + ix, terrain.first_sample + iz);
            let distance = rect_distance(global_x, global_z);
            if distance == 0 {
                // Inside the rectangle: the retail grid rules there, not the ring.
                continue;
            }
            for (dx, dz) in directions {
                let (ahead_x, ahead_z) = (ix + dx * HEADROOM, iz + dz * HEADROOM);
                if ahead_x < 1 || ahead_x > dim - 2 || ahead_z < 1 || ahead_z > dim - 2 {
                    continue;
                }
                let neighbour = (iz + dz) as usize * terrain.dim + (ix + dx) as usize;
                if terrain.bands[neighbour] != TerrainBand::Ocean {
                    continue;
                }
                // The sea must be FARTHER from the city than the start: the walk
                // goes outward and therefore never crosses the rectangle.
                if rect_distance(global_x + dx, global_z + dz) <= distance {
                    continue;
                }
                let x = global_x as f32 * spacing;
                let z = global_z as f32 * spacing;
                let ocean_x = (global_x + dx) as f32 * spacing;
                let ocean_z = (global_z + dz) as f32 * spacing;
                return Some((
                    [x, terrain.height_at(x, z), z],
                    [dx as f32, dz as f32],
                    [ocean_x, terrain.height_at(ocean_x, ocean_z), ocean_z],
                ));
            }
        }
    }
    None
}

/// Walks `steps` × `step_m` metres from `from` along `dir` through the
/// production collision primitive ([`DemoScene::collision_step`], the same one
/// `Locomotion::step` calls every frame in the window).
///
/// Returns `(final position, blocked steps, distance travelled in metres)`.
fn advance(
    scene: &DemoScene,
    from: [f32; 3],
    dir: [f32; 2],
    steps: usize,
    step_m: f32,
) -> ([f32; 3], usize, f32) {
    let mut position = from;
    let mut blocked = 0usize;
    for _ in 0..steps {
        let (next, _moved, hit) = scene.collision_step(
            position,
            [dir[0] * step_m, 0.0, dir[1] * step_m],
            PLAYER_RADIUS_M,
        );
        if hit {
            blocked += 1;
        }
        position = next;
    }
    let travelled = ((position[0] - from[0]).powi(2) + (position[2] - from[2]).powi(2)).sqrt();
    (position, blocked, travelled)
}

// ── PROBE ATTACK-KEY-001 (B1-c / O9) ───────────────────────────────────────

/// `--probe ATTACK-KEY-001` — the keyboard attack reaches the engine and the
/// HUD reflects the target's HP.
///
/// The input route is the production one, only the window is missing:
/// `ButtonInput<KeyCode>` (the real Bevy state) → `m2_kinematics::attack_triggered`
/// (the function `m2_player` uses, `KeyF` OR left click) → `ActionState::attack`
/// → [`demo_player_strike`] (the function `m14_melee_sfx` uses) → the production
/// `CombatSession` + the HUD line.
pub fn run_attack_key_001() -> i32 {
    println!(
        "PROBE-ATTACK-KEY-001 - B1-c ataque accionable por teclado (KeyF) + feedback de daño en \
         el HUD"
    );
    let scene = match compose_demo_with(None, None) {
        Ok(scene) => scene,
        Err(err) => return composition_failed("PROBE-ATTACK-KEY-001", &err),
    };
    let mut all_ok = true;

    // ── block A · KeyF pressed N ticks ─────────────────────────────────
    let mut pressed_scene = scene;
    let mut fight = compose_demo_fight(&pressed_scene);
    let target = 0usize;
    let hp_before = fight.session.enemies[target].health();
    let hud_before = pressed_scene.m20_hud_digest();
    let mut keys = ButtonInput::<KeyCode>::default();
    let mouse = ButtonInput::<MouseButton>::default();
    let mut hits = 0usize;
    let mut misses = 0usize;
    let mut hud_mismatch = 0usize;
    // One key press per frame for N frames: `clear()` resets the `just_pressed`
    // set (Bevy semantics), then `press` marks the frame (like holding/tapping
    // F once per tick in the window).
    for tick in 0..20 {
        // One tap per frame: `reset_all()` releases the key and clears the
        // edge sets, so `press` raises `just_pressed` again (a held key would
        // only raise it once — Bevy semantics).
        keys.reset_all();
        keys.press(KeyCode::KeyF);
        let attack = daggerfall_app::m2_kinematics::attack_triggered(
            mouse.just_pressed(MouseButton::Left),
            keys.just_pressed(KeyCode::KeyF),
        );
        if !attack {
            println!("[FAIL] tick {tick} - KeyF did not raise the attack flag");
            all_ok = false;
            break;
        }
        if !fight.session.enemies[target].is_alive() {
            println!("      tick {tick}: target dead — the session refuses further strikes");
            break;
        }
        let Some(report) = demo_player_strike(&mut pressed_scene, &mut fight.session, target)
        else {
            println!("      tick {tick}: no strike (session guard)");
            break;
        };
        if report.strike.attack.hit {
            hits += 1;
            if report.hud_after == report.hud_before {
                hud_mismatch += 1;
            }
        } else {
            misses += 1;
            if report.hud_after != report.hud_before {
                hud_mismatch += 1;
            }
        }
        println!(
            "      tick {tick}: attack=true · hit={} damage={} taken={} · target {} HP {}→{}/{} · \
             HUD {:016x}→{:016x}",
            report.strike.attack.hit,
            report.strike.attack.damage,
            report.damage_taken(),
            report.target_label,
            report.target_health_before,
            report.strike.target_health,
            report.target_max_health,
            report.hud_before,
            report.hud_after,
        );
    }
    let hp_after = fight.session.enemies[target].health();
    let hud_after = pressed_scene.m20_hud_digest();
    let pressed_ok = hits > 0
        && misses > 0
        && hp_after < hp_before
        && hud_after != hud_before
        && hud_mismatch == 0;
    println!(
        "[{}] key_f_attack - {} strikes (hits {hits} / misses {misses}) · target HP {hp_before}→\
         {hp_after} · HUD {hud_before:016x}→{hud_after:016x} · digest changes exactly on a hit \
         (mismatches {hud_mismatch}) · line \"{}\"",
        if pressed_ok { "OK" } else { "FAIL" },
        hits + misses,
        pressed_scene.m20_status_line(),
    );
    all_ok &= pressed_ok;

    // ── block B · negative control: no input ───────────────────────────
    let idle_scene = match compose_demo_with(None, None) {
        Ok(scene) => scene,
        Err(err) => return composition_failed("PROBE-ATTACK-KEY-001", &err),
    };
    let idle_fight = compose_demo_fight(&idle_scene);
    let idle_hp_before = idle_fight.session.enemies[target].health();
    let idle_hud_before = idle_scene.m20_hud_digest();
    let mut idle_keys = ButtonInput::<KeyCode>::default();
    let idle_mouse = ButtonInput::<MouseButton>::default();
    let mut idle_attacks = 0usize;
    for _ in 0..20 {
        idle_keys.reset_all();
        let attack = daggerfall_app::m2_kinematics::attack_triggered(
            idle_mouse.just_pressed(MouseButton::Left),
            idle_keys.just_pressed(KeyCode::KeyF),
        );
        if attack {
            idle_attacks += 1;
        }
    }
    let idle_hp_after = idle_fight.session.enemies[target].health();
    let idle_hud_after = idle_scene.m20_hud_digest();
    let control_ok =
        idle_attacks == 0 && idle_hp_after == idle_hp_before && idle_hud_after == idle_hud_before;
    println!(
        "[{}] control_negative_no_input - 20 ticks without KeyF/click · attacks raised \
         {idle_attacks} · target HP {idle_hp_before}→{idle_hp_after} · HUD \
         {idle_hud_before:016x}→{idle_hud_after:016x}",
        if control_ok { "OK" } else { "FAIL" },
    );
    all_ok &= control_ok;

    // ── block C · negative control: a REAL miss writes nothing (m-2) ───
    // A fresh composition is driven until the first miss; on that strike the
    // HUD digest and the rendered line must both be untouched. The previous
    // version only asserted that a fresh line carries no `TARGET` segment,
    // which held even if a miss wrote the HUD (tautological, 0 discrimination).
    let mut miss_scene = idle_scene;
    let mut miss_fight = compose_demo_fight(&miss_scene);
    let mut first_miss: Option<(usize, u64, u64, String)> = None;
    for tick in 0..20 {
        if !miss_fight.session.enemies[target].is_alive() {
            break;
        }
        let Some(report) = demo_player_strike(&mut miss_scene, &mut miss_fight.session, target)
        else {
            break;
        };
        if !report.strike.attack.hit {
            first_miss = Some((
                tick,
                report.hud_before,
                report.hud_after,
                miss_scene.m20_status_line(),
            ));
            break;
        }
    }
    match first_miss {
        Some((tick, before, after, line)) => {
            let miss_ok = before == after && !line.contains("TARGET");
            println!(
                "[{}] miss_writes_nothing - first real miss at strike {tick} · HUD \
                 {before:016x}→{after:016x} unchanged={} · line \"{line}\" has TARGET segment={}",
                if miss_ok { "OK" } else { "FAIL" },
                before == after,
                line.contains("TARGET"),
            );
            all_ok &= miss_ok;
        }
        None => {
            println!(
                "[FAIL] miss_writes_nothing - no miss in 20 real strikes: the control cannot be \
                 measured"
            );
            all_ok = false;
        }
    }

    if all_ok {
        println!(
            "VERDICT: PROBE-ATTACK-KEY-001 PASS - KeyF → ActionState::attack → player_strike: \
             {hits} hits / {misses} misses · target HP {hp_before}→{hp_after} · HUD \
             {hud_before:016x}→{hud_after:016x} · control (no input) HP/HUD unchanged"
        );
        0
    } else {
        println!("VERDICT: PROBE-ATTACK-KEY-001 FAIL - see [FAIL] rows");
        EXIT_FAIL
    }
}

// ── PROBE MENU-001 (B1-d / O8) ─────────────────────────────────────────────

/// World-frame counter of the probe schedule.
///
/// It counts how many frames the **gated** bundle ran — the same bundle
/// `m18_window` gates with [`phase_playing`], which owns `m20_progression_tick`
/// (the window's game clock: 1 real second = 1 game minute). `0` frames ⇒ that
/// system never ran ⇒ the clock advanced 0 minutes. The probe does **not**
/// re-implement the clock (m-4): it measures the gate the clock lives behind.
#[derive(Resource, Default)]
struct ProbeClock {
    /// Frames the gated bundle ran.
    frames: u64,
}

/// `Enter` handler of the probe schedule — the same pure transition the window
/// uses ([`phase_after_enter`]), gated **by nothing** (it is the way out).
fn probe_phase_keys(keys: Res<ButtonInput<KeyCode>>, mut phase: ResMut<AppPhase>) {
    if keys.just_pressed(KeyCode::Enter) {
        *phase = phase_after_enter(*phase);
    }
}

/// World system of the probe schedule — gated by the REAL
/// [`phase_playing`] condition (the one `m18_window` attaches to its world
/// bundle, the game clock included).
fn probe_world_tick(mut clock: ResMut<ProbeClock>) {
    clock.frames += 1;
}

/// `--probe MENU-001` — the demo opens on the menu and `Enter` enters the world.
///
/// The schedule is headless (`World` + `Schedule`, 0 plugins, 0 window) and
/// consumes the production [`AppPhase`] + [`phase_playing`]: the run condition
/// the probe measures IS the one the window gates its world systems with.
pub fn run_menu_001() -> i32 {
    println!(
        "PROBE-MENU-001 - B1-d fase de arranque Menu → Playing (Enter) · 0 ticks de mundo en el \
         menú"
    );
    let reference = match compose_demo_with(None, None) {
        Ok(scene) => scene,
        Err(err) => return composition_failed("PROBE-MENU-001", &err),
    };
    let spawn = reference.spawn;
    let hud_menu_expected = reference.m20_hud_digest();
    // M-1: the surfaces are read from the REAL composition (panel ids of the
    // scene) through the same selector the window applies — never from the
    // title bar.
    let panel_ids: Vec<&str> = reference
        .panels
        .iter()
        .map(|panel| panel.id.as_str())
        .collect();
    let menu_panel = crate::m18_phase::visible_panel_index(AppPhase::Menu, &panel_ids);
    let playing_panel = crate::m18_phase::visible_panel_index(AppPhase::Playing, &panel_ids);
    let menu_id = menu_panel.map(|index| panel_ids[index]);
    let playing_id = playing_panel.map(|index| panel_ids[index]);
    let mut all_ok = true;

    let mut world = World::new();
    world.insert_resource(AppPhase::Menu);
    world.insert_resource(ButtonInput::<KeyCode>::default());
    world.insert_resource(ProbeClock::default());
    let mut schedule = Schedule::default();
    // `chain` fixes the order inside a frame: the `Enter` handler evaluates
    // before the gated world bundle, so the frame that enters the world is the
    // first frame that ticks. **m-3:** the window declares that same precedence
    // (`m18_phase_keys.before(m2_gather_action).before(m18_apply_player_action)`),
    // so this chain measures an order that exists in the window too.
    schedule.add_systems((probe_phase_keys, probe_world_tick.run_if(phase_playing)).chain());

    // ── block A · startup: Menu surface, 0 world frames ────────────────
    let phase_start = *world.resource::<AppPhase>();
    let clock_start = world.resource::<ProbeClock>();
    let startup_ok = phase_start == AppPhase::Menu
        && clock_start.frames == 0
        && menu_id == Some(crate::m18_phase::MENU_PANEL_ID)
        && !crate::m18_phase::hud_visible(AppPhase::Menu)
        && menu_panel.is_some_and(|index| reference.panels[index].opaque > 0);
    println!(
        "[{}] startup_phase - phase={} · visible panel \"{}\" (index {}) · {} opaque px · HUD \
         hidden={} · world frames {} (the gated bundle owns the game clock)",
        if startup_ok { "OK" } else { "FAIL" },
        phase_start.label(),
        menu_id.unwrap_or("-"),
        menu_panel.map_or(-1i64, |index| index as i64),
        menu_panel.map_or(0, |index| reference.panels[index].opaque),
        !crate::m18_phase::hud_visible(AppPhase::Menu),
        clock_start.frames,
    );
    all_ok &= startup_ok;

    // ── block B · N frames without Enter ⇒ still Menu, still 0 ─────────
    for _ in 0..8 {
        schedule.run(&mut world);
    }
    let phase_menu = *world.resource::<AppPhase>();
    let clock_menu = world.resource::<ProbeClock>();
    let menu_ok = phase_menu == AppPhase::Menu
        && clock_menu.frames == 0
        && menu_id == Some(crate::m18_phase::MENU_PANEL_ID);
    println!(
        "[{}] menu_holds_without_enter - 8 frames · phase={} · visible panel \"{}\" · world frames \
         {} (the gated bundle does not run)",
        if menu_ok { "OK" } else { "FAIL" },
        phase_menu.label(),
        menu_id.unwrap_or("-"),
        clock_menu.frames,
    );
    all_ok &= menu_ok;

    // ── block C · Enter ⇒ Playing, world ticks, HUD surface back ───────
    world
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Enter);
    schedule.run(&mut world);
    world.resource_mut::<ButtonInput<KeyCode>>().clear();
    let phase_entered = *world.resource::<AppPhase>();
    let clock_playing = world.resource::<ProbeClock>();
    let enter_ok = phase_entered == AppPhase::Playing
        && clock_playing.frames == 1
        && playing_id == Some(crate::m18_phase::HUD_PANEL_ID)
        && crate::m18_phase::hud_visible(AppPhase::Playing)
        && playing_panel.is_some_and(|index| reference.panels[index].opaque > 0);
    println!(
        "[{}] enter_enters_world - phase={} · visible panel \"{}\" (index {}) · HUD visible={} · \
         world frames {} (first gated frame)",
        if enter_ok { "OK" } else { "FAIL" },
        phase_entered.label(),
        playing_id.unwrap_or("-"),
        playing_panel.map_or(-1i64, |index| index as i64),
        crate::m18_phase::hud_visible(AppPhase::Playing),
        clock_playing.frames,
    );
    all_ok &= enter_ok;

    for _ in 0..3 {
        schedule.run(&mut world);
    }
    let clock_after = world.resource::<ProbeClock>();
    let ticking_ok = clock_after.frames == 4;
    println!(
        "[{}] world_ticks_while_playing - 3 more frames · phase={} · world frames {}",
        if ticking_ok { "OK" } else { "FAIL" },
        world.resource::<AppPhase>().label(),
        clock_after.frames,
    );
    all_ok &= ticking_ok;

    // ── block D · the spawn is the default composition's ───────────────
    let second = match compose_demo_with(None, None) {
        Ok(scene) => scene,
        Err(err) => return composition_failed("PROBE-MENU-001", &err),
    };
    let hud_playing = second.m20_hud_digest();
    let spawn_ok = second.spawn == spawn && hud_playing == hud_menu_expected;
    println!(
        "[{}] spawn_and_hud_unchanged - spawn=({:.3},{:.3},{:.3}) yaw={:.4} · digest ×2 HUD \
         {hud_menu_expected:016x} / {hud_playing:016x} · phase does not alter the composition",
        if spawn_ok { "OK" } else { "FAIL" },
        spawn.eye[0],
        spawn.eye[1],
        spawn.eye[2],
        spawn.yaw,
    );
    all_ok &= spawn_ok;

    // ── block E · negative control: Enter is one-way ──────────────────
    let before_control = *world.resource::<AppPhase>();
    world
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Enter);
    schedule.run(&mut world);
    world.resource_mut::<ButtonInput<KeyCode>>().clear();
    let after_control = *world.resource::<AppPhase>();
    let control_ok = before_control == AppPhase::Playing
        && after_control == AppPhase::Playing
        && world.resource::<ProbeClock>().frames == 5;
    println!(
        "[{}] control_negative_enter_is_one_way - Enter while PLAYING keeps phase={} and the \
         world running (frames {}) — a session is never reset",
        if control_ok { "OK" } else { "FAIL" },
        after_control.label(),
        world.resource::<ProbeClock>().frames,
    );
    all_ok &= control_ok;

    if all_ok {
        println!(
            "VERDICT: PROBE-MENU-001 PASS - startup MENU with 0 world frames behind the gate that \
             owns the game clock · visible panel \"{}\" (HUD hidden) · 8 frames without Enter stay \
             MENU · Enter ⇒ PLAYING with panel \"{}\" + HUD · spawn=({:.2},{:.2},{:.2}) yaw={:.2} = \
             default composition · HUD {hud_menu_expected:016x} ×2",
            menu_id.unwrap_or("-"),
            playing_id.unwrap_or("-"),
            spawn.eye[0],
            spawn.eye[1],
            spawn.eye[2],
            spawn.yaw
        );
        0
    } else {
        println!("VERDICT: PROBE-MENU-001 FAIL - see [FAIL] rows");
        EXIT_FAIL
    }
}

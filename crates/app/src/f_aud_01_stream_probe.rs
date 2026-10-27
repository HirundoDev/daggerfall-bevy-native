//! STREAM-001 — F-AUD-01 async cell streaming probe (design
//! `f-aud-01-02-streaming-navmesh-CR-96142f2f.md` §2.7/§2.8).
//!
//! Blocks and their landing phase (design §2.8):
//!
//! | Block | What it falsifies | Status |
//! |:---|:---|:---|
//! | P | pure policy invariants (window, hysteresis, digest) | F1 — always compiled |
//! | A | frame repartition: ≤1 commit/frame, ≥2 frames over a 3-cell walk | F2 |
//! | M | measurements: active window under movement, per-tick cost, population seed stability (0 re-seed), §2.5 per-cell cota | F2 |
//! | C/D | real despawn + 0 asset leak (`Assets<Mesh>` baseline) | pending F3 |
//! | E | teleport backpressure/cancellation | pending F4 |
//! | F | parity: window digest ×2 + P-2 regression list | pending F5 |
//!
//! Exit codes: `0` ⟺ every implemented block passes (P + A + M today; the
//! verdict line names the pending blocks instead of faking them), `2` without
//! ARENA2 custody, `1` on any FAIL. Without the `window` feature the ECS
//! blocks are not compiled (`world_stream` is `cfg(feature = "window")` by
//! design §2.8) and the probe exits `2` after the pure policy block.
//!
//! The harness is headless (`MinimalPlugins` + `GameCorePlugin`, 0 GPU,
//! 0 window — I-6): the same plugin the M18 window will compose, driven by a
//! synthetic focus walk.

use daggerfall_parser_bevy_bridge::stream_policy::{
    cell_of_point, desired_cells, plan, plan_digest, CellKey, StreamConfig,
    STREAM_HYSTERESIS_CELLS,
};
use std::collections::BTreeSet;

#[cfg(feature = "window")]
use crate::world_stream::{ArchSpecs, StreamSource, StreamTexture, StreamWindow};
#[cfg(feature = "window")]
use bevy::prelude::{App, Entity};
#[cfg(feature = "window")]
use daggerfall_parser_rs::rmb::MapBlockRecord;
#[cfg(feature = "window")]
use std::collections::BTreeMap;

/// Runs STREAM-001 (dispatch entry of `--probe STREAM-001`).
pub fn run_stream_001() -> i32 {
    println!("STREAM-001 - F-AUD-01 async cell streaming (CR-M37F1F2-20261027)");
    if !run_policy_block() {
        println!("VERDICT: STREAM-001 FAIL - policy block P");
        return 1;
    }
    #[cfg(feature = "window")]
    {
        run_window_blocks()
    }
    #[cfg(not(feature = "window"))]
    {
        println!(
            "[SKIP-NO-WINDOW] blocks A/M need the plugin, which is cfg(feature = \"window\") \
             (design §2.8: the default headless build keeps the module out of the tree)"
        );
        println!(
            "VERDICT: STREAM-001 NO-FEATURE - policy block P green; re-run with --features window"
        );
        2
    }
}

/// Block P — the pure policy invariants (F1 gate), headless in every build.
fn run_policy_block() -> bool {
    let mut ok = true;
    let config = StreamConfig::default();

    let window9 = desired_cells(CellKey { x: 3, z: -2, level: 0 }, config.radius_cells);
    let nine = window9.len() == 9 && window9[0] == (CellKey { x: 2, z: -3, level: 0 });
    ok &= nine;
    println!(
        "[{}] P window - R={} ⇒ {} cells (first {:?})",
        pass(nine),
        config.radius_cells,
        window9.len(),
        window9.first()
    );

    // Hysteresis (B falsifier, pure): after loading a 3×3 window and
    // crossing one cell east, 20 oscillations over the border are silent.
    let mut loaded: BTreeSet<CellKey> = BTreeSet::new();
    let start = CellKey { x: 0, z: 0, level: 0 };
    let first = plan(&config, start, &loaded);
    loaded.extend(first.load.iter().copied());
    let crossed = plan(&config, CellKey { x: 1, z: 0, level: 0 }, &loaded);
    loaded.extend(crossed.load.iter().copied());
    for i in 0..20 {
        let x_m = if i % 2 == 0 {
            102.4 - 0.1
        } else {
            102.4 + 0.1
        };
        let center = cell_of_point(&config, [x_m, 0.0, 0.0]);
        let step = plan(&config, center, &loaded);
        if !step.load.is_empty() || !step.unload.is_empty() {
            ok = false;
            println!(
                "[FAIL] P hysteresis - oscillation {i} at center {center:?}: load {:?} unload {:?}",
                step.load, step.unload
            );
            break;
        }
        loaded.extend(step.load.iter().copied());
    }
    println!(
        "[{}] P hysteresis - 20 border oscillations after stabilization: 0 loads, 0 unloads \
         (H1 strict >, band = R+{} cell)",
        pass(ok),
        STREAM_HYSTERESIS_CELLS
    );

    let digest_a = plan_digest(&first);
    let digest_b = plan_digest(&first);
    let stable = digest_a == digest_b;
    ok &= stable;
    println!(
        "[{}] P plan_digest - FNV-1a 64 {digest_a:016x} ×2 identical={stable}",
        pass(stable)
    );
    ok
}

fn pass(ok: bool) -> &'static str {
    if ok { "OK" } else { "FAIL" }
}

#[cfg(feature = "window")]
fn run_window_blocks() -> i32 {
    use crate::m10_probe;
    use crate::world_stream::{
        ArchSpecs, CellPart, CellRoot, StreamIntent, StreamSource, StreamSourceInner,
        StreamWindow, STALL_FRAMES,
    };
    use bevy::prelude::*;
    use daggerfall_parser_bevy_bridge::npc_spec::{NpcWorld, PopulationRequest};
    use daggerfall_parser_rs::namegen::NameGen;
    use daggerfall_parser_rs::text_rsc::TextRsc;
    use daggerfall_parser_rs::texture::TextureArchive;
    use std::collections::BTreeMap;
    use std::time::Instant;

    // ── sources (Mode A custody; no ARENA2 ⇒ EXIT=2) ─────────────────────
    let (blocks, arch_bytes) = match m10_probe::capital_blocks() {
        Ok(sources) => sources,
        Err(reason) => {
            println!("ARENA2 not accessible: {reason}");
            println!("VERDICT: STREAM-001 NO DATA - ARENA2 not accessible");
            return 2;
        }
    };
    let (width, height) = (blocks.width(), blocks.height());
    println!(
        "[OK] capital sources - {width}×{height} grid, {} .RMB records, ARCH3D.BSA {} bytes",
        blocks.blocks().len(),
        arch_bytes.len()
    );
    let arch = match ArchSpecs::parse(arch_bytes) {
        Ok(arch) => arch,
        Err(reason) => {
            println!("[FAIL] ARCH3D.BSA: {reason}");
            println!("VERDICT: STREAM-001 FAIL - invalid ARCH3D.BSA");
            return 1;
        }
    };
    // D-M37-6 session texture store: decode the retail textures the capital
    // references (PAL.PAL + TEXTURE.*) so the MaterialCache warms real
    // images/materials at Startup (block D measures warm == final). A decode
    // failure degrades to untextured cells with the shared ground material.
    let textures = match build_textures(blocks.blocks(), width, height, &arch) {
        Ok(textures) => {
            println!("[OK] session textures - {} decoded", textures.len());
            textures
        }
        Err(reason) => {
            println!("[WARN] textures unavailable ({reason}) - untextured cells (ground)");
            BTreeMap::new()
        }
    };
    let source = StreamSource(std::sync::Arc::new(StreamSourceInner {
        width_blocks: width,
        height_blocks: height,
        blocks: blocks
            .blocks()
            .iter()
            .map(|(bx, by, record)| ((*bx, *by), record.clone()))
            .collect::<BTreeMap<_, _>>(),
        textures,
        arch,
    }));

    // ── population before the walk (D-M37-7: the streaming must not move
    //    this digest — 0 re-seed) ─────────────────────────────────────────
    let retail_file = |name: &str| -> Result<Vec<u8>, String> {
        let path = crate::mode_a_arena2_path(name)?;
        std::fs::read(&path).map_err(|e| format!("{name} ({e})"))
    };
    let namegen_bytes = match retail_file("NAMEGEN.DAT") {
        Ok(bytes) => bytes,
        Err(reason) => {
            println!("ARENA2 not accessible: {reason}");
            println!("VERDICT: STREAM-001 NO DATA - retail population inputs unreadable");
            return 2;
        }
    };
    let namegen = match NameGen::parse(&namegen_bytes) {
        Ok(namegen) => namegen,
        Err(err) => {
            println!("[FAIL] NAMEGEN.DAT: {err:?}");
            return 1;
        }
    };
    let flats_bytes = match retail_file(crate::m18_demo::PERSON_FLAT_FILE) {
        Ok(bytes) => bytes,
        Err(reason) => {
            println!("ARENA2 not accessible: {reason}");
            return 2;
        }
    };
    let flats = match TextureArchive::parse(&flats_bytes) {
        Ok(flats) => flats,
        Err(err) => {
            println!("[FAIL] person flats: {err:?}");
            return 1;
        }
    };
    let text_bytes = match retail_file("TEXT.RSC") {
        Ok(bytes) => bytes,
        Err(reason) => {
            println!("ARENA2 not accessible: {reason}");
            return 2;
        }
    };
    let text = match TextRsc::parse(&text_bytes) {
        Ok(text) => text,
        Err(err) => {
            println!("[FAIL] TEXT.RSC: {err:?}");
            return 1;
        }
    };
    let npc_world = NpcWorld {
        namegen: &namegen,
        person_flats: &flats,
        text: &text,
        text_bytes: &text_bytes,
    };
    let request = PopulationRequest {
        seed: crate::m18_demo::NPC_SEED,
        region: crate::m18_demo::NPC_REGION,
        subclimate: crate::m18_demo::NPC_SUBCLIMATE,
        count: 50,
    };
    let population_before = match npc_world.populate(request) {
        Ok(population) => population.digest(),
        Err(err) => {
            println!("[FAIL] population: {err:?}");
            return 1;
        }
    };

    // ── headless app: MinimalPlugins + contract + the streaming plugin ───
    let config = StreamConfig::default();
    // Start at the center of grid block (2, 2) — a 2-block margin so the
    // 3×3 window never leaves the 8×8 capital during the walk.
    let start_bx = 2usize;
    let start_by = 2usize;
    let start = CellKey {
        x: start_bx as i32,
        z: (height - 1 - start_by) as i32,
        level: 0,
    };
    let block_m = f64::from(config.cell_m);
    let mut pos_x = start_bx as f64 * block_m + block_m / 2.0;
    let pos_z = (height - start_by - 1) as f64 * block_m + block_m / 2.0;

    let (mut app, focus) = make_app(config, source.clone(), start, pos_x as f32, pos_z as f32);
    // Baseline before any commit: `init_asset::<Mesh>()` left the store empty
    // (warm adds 0 meshes — only images/materials), so this is the leak floor
    // block D drains back to.
    let baseline_meshes = app.world().resource::<Assets<Mesh>>().len();

    // ── synthetic walk: 3 cells east with adaptive pacing (§2.4: the budget
    //    IS the throttle — while the pipeline is saturated with 0 commits
    //    for `CATCHUP_FRAMES`, movement pauses so commits land INSIDE the
    //    moving window; the sample series covers walk + drain uniformly) ──
    // Settle first: commit the initial 3×3 window before moving (this also
    // warms the ARCH3D cache), so the walk **evicts** the cells behind it —
    // the eviction path is measured, not assumed.
    let mut settle_frames = 0usize;
    while settle_frames < 400 {
        let converged_settle = {
            let window = app.world().resource::<StreamWindow>();
            window.in_flight.is_empty()
                && window.ready.is_empty()
                && window.loaded.len() == 9
        };
        if converged_settle {
            break;
        }
        app.update();
        settle_frames += 1;
    }
    println!("[M] settle - initial window committed in {settle_frames} frames");
    let settle_meshes = app.world().resource::<Assets<Mesh>>().len();
    let warm_images = app.world().resource::<Assets<Image>>().len();
    let warm_materials = app.world().resource::<Assets<StandardMaterial>>().len();
    struct FrameSample {
        frame: usize,
        commits: u64,
        despawns: u64,
        loaded: usize,
        in_flight: usize,
        tick_us: u64,
        mesh_bytes: usize,
    }
    let mut samples: Vec<FrameSample> = Vec::new();
    let step_m = 10.24f32;
    const CATCHUP_FRAMES: usize = 40;
    let target_m = 3.0 * block_m; // three cells east
    let mut walked_m = 0.0f64;
    // Walk-phase deltas start from the post-settle counters (the settle
    // commits belong to the startup, not to the moving window).
    let (mut prev_commits, mut prev_despawns) = {
        let window = app.world().resource::<StreamWindow>();
        (window.stats.commits, window.stats.despawns)
    };
    let mut frames_since_commit = 0usize;
    let mut peak_mesh_bytes = 0usize;
    let mut peak_loaded = 0usize;
    let mut peak_in_flight = 0usize;
    let mut max_tick_us = 0u64;
    let mut sum_tick_us = 0u64;
    let mut frame = 0usize;
    const MAX_FRAMES: usize = 900;


    while frame < MAX_FRAMES {
        let (converged, saturated) = {
            let window = app.world().resource::<StreamWindow>();
            (
                walked_m >= target_m
                    && window.in_flight.is_empty()
                    && window.ready.is_empty(),
                window.in_flight.len() >= usize::from(config.max_in_flight),
            )
        };
        if converged {
            break;
        }
        let advance = (!saturated || frames_since_commit < CATCHUP_FRAMES)
            && walked_m < target_m;
        if advance {
            pos_x += f64::from(step_m);
            walked_m += f64::from(step_m);
            app.world_mut()
                .entity_mut(focus)
                .get_mut::<Transform>()
                .expect("focus anchor")
                .translation
                .x = pos_x as f32;
        }

        let tick_started = Instant::now();
        app.update();
        let tick_us = tick_started.elapsed().as_micros() as u64;

        let window = app.world().resource::<StreamWindow>();
        let stats = window.stats;
        let mesh_bytes = window.composed_mesh_bytes();
        if stats.commits > prev_commits {
            frames_since_commit = 0;
        } else {
            frames_since_commit += 1;
        }
        samples.push(FrameSample {
            frame,
            commits: stats.commits - prev_commits,
            despawns: stats.despawns - prev_despawns,
            loaded: window.loaded.len(),
            in_flight: window.in_flight.len(),
            tick_us,
            mesh_bytes,
        });
        prev_commits = stats.commits;
        prev_despawns = stats.despawns;
        peak_mesh_bytes = peak_mesh_bytes.max(mesh_bytes);
        peak_loaded = peak_loaded.max(window.loaded.len());
        peak_in_flight = peak_in_flight.max(window.in_flight.len());
        max_tick_us = max_tick_us.max(tick_us);
        sum_tick_us += tick_us;
        frame += 1;
    }
    let walk_frames = frame;

    let final_window = app.world().resource::<StreamWindow>();
    let stats = final_window.stats;
    let final_center = app.world().resource::<StreamIntent>().center;
    let final_loaded = final_window.loaded.clone();
    let cancelled_total = final_window.stats.cancelled;
    // Fold the cells' own declared identity (cell.key), not the store keys —
    // the window digest must describe the composed payload (§2.6).
    let final_composed: Vec<(CellKey, u64)> = final_window
        .composed
        .values()
        .map(|cell| (cell.key, cell.digest))
        .collect();
    let mut max_buildings_per_cell = 0usize;
    for cell in final_window.composed.values() {
        max_buildings_per_cell = max_buildings_per_cell.max(cell.city.buildings.len());
    }
    let final_mesh_bytes = final_window.composed_mesh_bytes();
    let _ = final_window;

    // Population after the walk: same request ⇒ same digest (0 re-seed).
    let population_after = match npc_world.populate(request) {
        Ok(population) => population.digest(),
        Err(err) => {
            println!("[FAIL] population after walk: {err:?}");
            return 1;
        }
    };
    let population_stable = population_before == population_after;

    // ── frame table (real output — the evidence of block A/M) ────────────
    for s in &samples {
        println!(
            "[M] f={:03} commits={} despawns={} loaded={} inflight={} tick={}us mesh_bytes={}",
            s.frame, s.commits, s.despawns, s.loaded, s.in_flight, s.tick_us, s.mesh_bytes
        );
    }

    // ── block A: frame repartition ───────────────────────────────────────
    let frames_with_commits = samples.iter().filter(|s| s.commits > 0).count();
    let budget_ok = samples.iter().all(|s| s.commits <= u64::from(config.max_commits_per_frame));
    let repartition_ok = budget_ok && frames_with_commits >= 2;
    println!(
        "[{}] A repartition - max_commits/frame={} (budget {}) · frames_with_commits={frames_with_commits} \
         (>= 2) over {} frames",
        pass(repartition_ok),
        samples.iter().map(|s| s.commits).max().unwrap_or(0),
        config.max_commits_per_frame,
        walk_frames
    );

    // ── measurements (block M) ───────────────────────────────────────────
    let backpressure_ok = peak_in_flight <= usize::from(config.max_in_flight);
    println!(
        "[{}] M backpressure - peak in_flight={peak_in_flight} (max_in_flight={})",
        pass(backpressure_ok),
        config.max_in_flight
    );
    let converged_ok = final_window_converged(&final_loaded, final_center, config.radius_cells)
        && stats.stalls == 0;
    println!(
        "[{}] M convergence - walk+drain converged in {walk_frames} frames · loaded={} cells · \
         stalls={} (STALL_FRAMES={STALL_FRAMES})",
        pass(converged_ok),
        final_loaded.len(),
        stats.stalls
    );
    let window_digest = fold_window_digest(&final_loaded, final_composed.iter().copied());
    println!(
        "[M] window - center {final_center:?} · loaded {} · composed {} · commits total {} · \
         despawns {} · discarded {} · cancelled {} · window_digest {window_digest:016x}",
        final_loaded.len(),
        final_composed.len(),
        stats.commits,
        stats.despawns,
        stats.discarded,
        cancelled_total
    );
    let mean_tick_us = sum_tick_us / walk_frames as u64;
    println!(
        "[M] tick cost - mean {mean_tick_us}us · max {max_tick_us}us over {walk_frames} frames \
         (budget {}ms soft)",
        config.commit_budget_ms
    );
    println!(
        "[M] eviction - mesh payload peak {peak_mesh_bytes} B → final {final_mesh_bytes} B · \
         peak window = {peak_loaded} cells (Assets<Mesh> accounted in block D)"
    );
    println!(
        "[M] §2.5 cota [INFERENCE] - max buildings/cell = {max_buildings_per_cell} \
         (design assumes ≤ ~300) · peak window payload = {peak_mesh_bytes} B"
    );
    let population_ok = population_stable && stats.commits > 0 && stats.despawns > 0;
    println!(
        "[{}] M population seed - digest {population_before:016x} before == {population_after:016x} \
         after the walk (commits={}, despawns={} ⇒ the walk really streamed, 0 re-seed)",
        pass(population_ok),
        stats.commits,
        stats.despawns
    );

    // ── block C (F3): real despawn — one `CellRoot` per loaded cell, one
    //    `CellPart`/`Mesh3d` child per texture group; the cells evicted
    //    behind the walk left 0 entities behind ──────────────────────────
    let mut root_keys: BTreeSet<CellKey> = BTreeSet::new();
    let mut cell_parts = 0usize;
    let mut cell_meshes = 0usize;
    for entity in app.world().iter_entities() {
        if let Some(root) = entity.get::<CellRoot>() {
            root_keys.insert(root.0);
        }
        if entity.contains::<CellPart>() {
            cell_parts += 1;
        }
        if entity.contains::<Mesh3d>() {
            cell_meshes += 1;
        }
    }
    let c_roots_ok = root_keys == final_loaded && !root_keys.contains(&start);
    let c_parts_ok = cell_meshes == cell_parts;
    let c_ok = c_roots_ok && c_parts_ok;
    println!(
        "[{}] C despawn - CellRoot entities={} == loaded {} · evicted start cell absent={} · \
         CellPart==Mesh3d ({cell_parts} parts)",
        pass(c_ok),
        root_keys.len(),
        final_loaded.len(),
        !root_keys.contains(&start),
    );

    // ── block D (F3): 0 asset leak — drain the window (teleport the focus
    //    outside the city grid) and confirm `Assets<Mesh>` returns to the
    //    baseline; images/materials never moved under the walk (D-M37-6: the
    //    cell owns only meshes) ───────────────────────────────────────────
    app.world_mut()
        .entity_mut(focus)
        .get_mut::<Transform>()
        .expect("focus anchor")
        .translation
        .x = -1.0e6; // far outside the 8×8 capital ⇒ every cell unloads
    let mut drain_frames = 0usize;
    while drain_frames < 300 {
        app.update();
        let empty = app.world().resource::<StreamWindow>().loaded.is_empty();
        drain_frames += 1;
        if empty {
            break;
        }
    }
    // Let the last despawns settle (deferred asset release).
    for _ in 0..5 {
        app.update();
    }
    let drained_meshes = app.world().resource::<Assets<Mesh>>().len();
    let final_images = app.world().resource::<Assets<Image>>().len();
    let final_materials = app.world().resource::<Assets<StandardMaterial>>().len();
    let d_mesh_ok = drained_meshes == baseline_meshes;
    let d_tex_ok = final_images == warm_images && final_materials == warm_materials;
    let d_ok = d_mesh_ok && d_tex_ok;
    println!(
        "[{}] D asset leak - Assets<Mesh> {settle_meshes} (loaded) → {drained_meshes} (drained) \
         == baseline {baseline_meshes} · images {warm_images}→{final_images} · materials \
         {warm_materials}→{final_materials} (warm==final)",
        pass(d_ok),
    );

    // ── block E (F4): teleport backpressure/cancellation + a real stall ──
    let teleport_ok = teleport_block(config, source.clone(), start, start_bx, start_by, height);
    let stall_ok = stall_block(source.clone(), start, start_bx, start_by, height);
    let e_ok = teleport_ok && stall_ok;

    // ── block F (F5): parity — the window digest is printed for the ×2
    //    cross-process diff; the P-2 anchor measured here is the population
    //    digest (I-4 golden), which M37 must not move (D-M37-7: 0 re-seed) ──
    const POPULATION_GOLDEN: u64 = 0x67728ed2e9587f71;
    let i4_ok = population_before == POPULATION_GOLDEN;
    println!(
        "[{}] F parity/I-4 - population digest {population_before:016x} == golden \
         {POPULATION_GOLDEN:016x} · window_digest {window_digest:016x} (×2 across processes)",
        pass(i4_ok)
    );
    let f_ok = i4_ok;

    let ok = repartition_ok && backpressure_ok && converged_ok && population_ok && c_ok && d_ok
        && e_ok && f_ok;
    if ok {
        println!(
            "VERDICT: STREAM-001 PASS - blocks P+A+M+C+D+E+F · window digest {window_digest:016x} \
             (confirm ×2 in two separate processes)"
        );
        0
    } else {
        println!("VERDICT: STREAM-001 FAIL - see the [FAIL] rows above");
        1
    }
}

/// True when every loaded cell is inside the hysteresis band of the final
/// center (chebyshev ≤ R + 1): the band is the designed extra ring, anything
/// farther would be a stray the unload pass failed to evict.
#[cfg(feature = "window")]
fn final_window_converged(
    loaded: &BTreeSet<CellKey>,
    center: CellKey,
    radius: u8,
) -> bool {
    let band = i32::from(radius) + i32::from(STREAM_HYSTERESIS_CELLS);
    loaded.iter().all(|cell| {
        cell.level == center.level
            && (cell.x - center.x).abs() <= band
            && (cell.z - center.z).abs() <= band
    })
}

/// FNV-1a 64 fold of the loaded keys **and** the composed cell digests, both
/// in key order (design §2.6: order of arrival is irrelevant — the fold
/// follows the key order, never the task completion order).
#[cfg(feature = "window")]
fn fold_window_digest(
    loaded: &BTreeSet<CellKey>,
    composed: impl Iterator<Item = (CellKey, u64)>,
) -> u64 {
    use daggerfall_parser_rs::rdi;
    let mut state = rdi::fnv1a64(b"M37WINDOW1");
    for key in loaded {
        state = rdi::fnv1a64_update(state, &key.x.to_le_bytes());
        state = rdi::fnv1a64_update(state, &key.z.to_le_bytes());
        state = rdi::fnv1a64_update(state, &key.level.to_le_bytes());
    }
    for (key, digest) in composed {
        state = rdi::fnv1a64_update(state, &key.x.to_le_bytes());
        state = rdi::fnv1a64_update(state, &key.z.to_le_bytes());
        state = rdi::fnv1a64_update(state, &key.level.to_le_bytes());
        state = rdi::fnv1a64_update(state, &digest.to_le_bytes());
    }
    state
}

/// Headless app with the asset stores the F3 streaming layer writes into:
/// `MinimalPlugins` + `AssetPlugin` + `init_asset::<Mesh/Image/StandardMaterial>`
/// **before** `GameCorePlugin` (the M18 harness requirement — `plugin.rs`
/// registers none of them; headless precedent `m12_texture_render.rs`). The
/// `MaterialCache` warms on `Startup`, so it exists before the first commit.
/// Returns the app with the focus anchor spawned at `(pos_x, 1.6, pos_z)`.
#[cfg(feature = "window")]
fn make_app(
    config: StreamConfig,
    source: StreamSource,
    start: CellKey,
    pos_x: f32,
    pos_z: f32,
) -> (App, Entity) {
    use crate::world_stream::{StreamFocus, WorldStreamPlugin};
    use bevy::prelude::*;
    use daggerfall_game_core::GameCorePlugin;

    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()));
    app.init_asset::<Mesh>();
    app.init_asset::<Image>();
    app.init_asset::<StandardMaterial>();
    app.add_plugins(GameCorePlugin::default());
    app.add_plugins(WorldStreamPlugin {
        config,
        source,
        start,
    });
    let focus = app
        .world_mut()
        .spawn((StreamFocus, Transform::from_xyz(pos_x, 1.6, pos_z)))
        .id();
    (app, focus)
}

/// Decodes the retail textures the whole capital references (`PAL.PAL` +
/// `TEXTURE.*`) so the streaming source carries a real session store
/// (D-M37-6) and the `MaterialCache` warms images/materials at Startup.
/// Builds the global city once purely to learn the texture keys, then frees
/// it — the streamed cells compose per-cell; this is only the key source.
#[cfg(feature = "window")]
fn build_textures(
    blocks: &[(usize, usize, MapBlockRecord)],
    width: usize,
    height: usize,
    arch: &ArchSpecs,
) -> Result<BTreeMap<u32, StreamTexture>, String> {
    use daggerfall_parser_bevy_bridge::build_retail_city;
    use daggerfall_parser_rs::texture::Palette;

    let dir = crate::mode_a_arena2_dir()?;
    let palette_bytes = std::fs::read(dir.join("PAL.PAL")).map_err(|e| format!("PAL.PAL: {e}"))?;
    let palette = Palette::parse(&palette_bytes).map_err(|e| format!("PAL.PAL parse: {e:?}"))?;
    let textures = {
        let city = build_retail_city(width, height, blocks, &mut |id| arch.resolve(id));
        let (demo, _failures) = crate::m18_demo::decode_city_textures(&city, &dir, &palette)
            .map_err(|e| format!("decode textures: {e:?}"))?;
        demo.into_iter()
            .map(|(key, dt)| {
                (
                    key,
                    StreamTexture {
                        width: u32::from(dt.width),
                        height: u32::from(dt.height),
                        rgba: dt.rgba,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>()
    };
    Ok(textures)
}

/// Block E (F4) part 1 — teleport backpressure/cancellation: after one frame
/// of in-flight work, teleport +10 cells without settling; the in-flight
/// results become out-of-window and are **discarded on arrival** (never
/// committed), input backpressure keeps `in_flight <= max_in_flight` in every
/// frame, and the new window still settles (`stalls == 0`). Returns pass/fail.
#[cfg(feature = "window")]
fn teleport_block(
    config: StreamConfig,
    source: StreamSource,
    start: CellKey,
    start_bx: usize,
    start_by: usize,
    height: usize,
) -> bool {
    use bevy::prelude::*;

    let block_m = f64::from(config.cell_m);
    let pos_x = start_bx as f64 * block_m + block_m / 2.0;
    let pos_z = (height - start_by - 1) as f64 * block_m + block_m / 2.0;
    let (mut app, focus) = make_app(config, source, start, pos_x as f32, pos_z as f32);

    // Frame 1 spawns the initial window's tasks onto the compute pool; they
    // are still in flight (`poll_once` on a fresh task is `Pending`) when the
    // teleport lands, so they are guaranteed to be discarded on arrival.
    app.update();
    let discarded_before = app.world().resource::<StreamWindow>().stats.discarded;
    let mut peak_in_flight = app.world().resource::<StreamWindow>().in_flight.len();

    // Teleport +10 cells east (no settle).
    app.world_mut()
        .entity_mut(focus)
        .get_mut::<Transform>()
        .expect("focus anchor")
        .translation
        .x += (10.0 * block_m) as f32;

    // Run to drain (capped so a broken pipeline cannot hang the probe).
    const TELEPORT_MAX_FRAMES: usize = 600;
    let mut frames = 0usize;
    while frames < TELEPORT_MAX_FRAMES {
        app.update();
        let window = app.world().resource::<StreamWindow>();
        peak_in_flight = peak_in_flight.max(window.in_flight.len());
        let converged = window.in_flight.is_empty() && window.ready.is_empty();
        frames += 1;
        if converged {
            break;
        }
    }
    let window = app.world().resource::<StreamWindow>();
    let discarded = window.stats.discarded - discarded_before;
    let ok = discarded >= 1
        && peak_in_flight <= usize::from(config.max_in_flight)
        && window.stats.stalls == 0;
    println!(
        "[{}] E teleport - +10 cells: discarded={discarded} (>= 1) · peak in_flight={peak_in_flight} \
         (<= {}) · stalls={} · drained in {frames} frames",
        pass(ok),
        config.max_in_flight,
        window.stats.stalls
    );
    ok
}

/// Block E (F4) part 2 — a real stall: `max_in_flight: 0` means no task ever
/// launches, so the pending set never shrinks; after `STALL_FRAMES` the
/// detector must fire `StreamStalled` and bump `stats.stalls` (a silent load
/// failure would violate §2.4). Returns pass/fail.
#[cfg(feature = "window")]
fn stall_block(
    source: StreamSource,
    start: CellKey,
    start_bx: usize,
    start_by: usize,
    height: usize,
) -> bool {
    use crate::world_stream::{StreamStalled, STALL_FRAMES};
    use bevy::ecs::message::Messages;

    let config = StreamConfig {
        max_in_flight: 0,
        ..StreamConfig::default()
    };
    let block_m = f64::from(config.cell_m);
    let pos_x = start_bx as f64 * block_m + block_m / 2.0;
    let pos_z = (height - start_by - 1) as f64 * block_m + block_m / 2.0;
    let (mut app, _focus) = make_app(config, source, start, pos_x as f32, pos_z as f32);

    let mut stalled_events = 0usize;
    let total = STALL_FRAMES as usize + 10;
    for _ in 0..total {
        app.update();
        // Drain the buffered message each frame so the single firing is not
        // lost to the `Messages` double-buffer swap.
        stalled_events += app
            .world_mut()
            .resource_mut::<Messages<StreamStalled>>()
            .drain()
            .count();
    }
    let window = app.world().resource::<StreamWindow>();
    let ok = window.stats.stalls >= 1 && stalled_events >= 1 && window.loaded.is_empty();
    println!(
        "[{}] E stall - max_in_flight=0 over {total} frames: stalls={} · StreamStalled events={} · \
         loaded={}",
        pass(ok),
        window.stats.stalls,
        stalled_events,
        window.loaded.len()
    );
    ok
}

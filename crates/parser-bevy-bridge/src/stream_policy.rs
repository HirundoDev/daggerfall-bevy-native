//! `stream_policy` — pure window/hysteresis policy for M37 cell streaming
//! (F-AUD-01, design `f-aud-01-02-streaming-navmesh-CR-96142f2f.md` §2.2/§2.3).
//!
//! **Std only, 0 Bevy, 0 I/O, 0 floats in the ordering** (I-5): the window is
//! geometry, not ECS. The ECS layer (`crates/app/src/world_stream.rs`, the
//! `window` build) consumes this module verbatim; the policy itself is tested
//! headless with no runtime at all.
//!
//! Three units, one abstract key (D-M37-1/D-M37-2):
//! - `CellKey { x, z, level }` is integer by construction — it is **not**
//!   `memory::chunks::ChunkCoord` (that one is the M7 automap's *visited*
//!   record; the streaming eviction must never write there, R7);
//! - what changes per world is `StreamConfig::cell_m` — **102.4 m** for a
//!   retail `.RMB` block (`city_retail::RETAIL_BLOCK_M`), 51.2 m for a
//!   dungeon `.RDB` cell, 102.4 m for a terrain patch (§2.1);
//! - `level` exists from day 1 so M38 never has to change the key shape; the
//!   city policy here always uses `level = 0` and never evicts other levels
//!   (a multi-level world owns its vertical lifecycle — declared scope).
//!
//! **Hysteresis (D-M37-3), declared deviation H1:** the design §2.2 writes
//! the unload rule as `Chebyshev >= R + HYSTERESIS`, but its own F1 falsifier
//! (stated in §2.2 *and* §2.7-B: "oscillating ±0.1 m over a cell border 10
//! times produces **no** load and **no** unload after the first
//! stabilization") is only satisfiable with a **strict `>`**. With `>=`, a
//! cell at Chebyshev exactly `R + H` from the two alternating centers of a
//! border walk (R=1, H=1 → the ring one cell ahead) would unload on one
//! frame and reload on the next, forever — precisely the thrash the falsifier
//! forbids. The falsifiable acceptance criterion is the operative spec, so
//! the unload band is `Chebyshev > R + H`; the deviation is declared in the
//! lane's A-PUNTO (CR-M37F1F2-20261027) instead of being silently invented.
//!
//! Digest: FNV-1a 64 over a canonical little-endian serialization, reusing
//! the parser's public `rdi::fnv1a64` (same algorithm and reference vectors
//! as `behavior::determinism::fnv1a64` cited by the design; the bridge has
//! no dependency edge to `behavior` — adding one would violate I-1's DAG, so
//! the existing public implementation is consumed instead of duplicated).

use std::collections::BTreeSet;

use crate::city_retail::RETAIL_BLOCK_M;
use daggerfall_parser_rs::rdi;

/// Hysteresis of the unload band in cells (D-M37-3): a loaded cell only
/// unloads when it is farther than `radius_cells + STREAM_HYSTERESIS_CELLS`
/// (Chebyshev, x/z) from the window center. See the module doc (H1) for the
/// strict-`>` rule.
pub const STREAM_HYSTERESIS_CELLS: u8 = 1;

/// Key of a streamed cell: integers only (D-M37-2). `level = 0` in the
/// exterior; a dungeon key carries its quantized level (M38).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CellKey {
    /// Column along the world X axis (grid coordinates for a composed city).
    pub x: i32,
    /// Column along the world Z axis (grid coordinates for a composed city).
    pub z: i32,
    /// Vertical level (`0` in the exterior; M38 quantizes dungeon levels).
    pub level: i16,
}

/// Bounding box of a cell in meters — **debug/evidence only**: the ordering
/// of this module never touches a float (I-5).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellSpan {
    /// The cell this box describes.
    pub key: CellKey,
    /// Minimum corner `(x, y, z)` in meters (y starts at the block floor).
    pub min_m: [f32; 3],
    /// Maximum corner `(x, y, z)` in meters (y extent is debug-only too).
    pub max_m: [f32; 3],
}

/// Window and budget configuration of one streamed world.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StreamConfig {
    /// Window radius in cells: `1` ⇒ a 3×3 window (9 cells).
    pub radius_cells: u8,
    /// Side of one cell in meters (102.4 retail city · 51.2 dungeon ·
    /// 102.4 terrain patch).
    pub cell_m: f32,
    /// Maximum cell commits per frame (soft: at least one if `ready`).
    pub max_commits_per_frame: u8,
    /// Maximum cell unloads per frame (farthest first).
    pub max_unloads_per_frame: u8,
    /// Backpressure: maximum tasks in flight at once.
    pub max_in_flight: u8,
    /// Soft budget in milliseconds per commit pass (at least one commit).
    pub commit_budget_ms: f32,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            radius_cells: 1,
            cell_m: RETAIL_BLOCK_M,
            max_commits_per_frame: 1,
            max_unloads_per_frame: 2,
            max_in_flight: 4,
            commit_budget_ms: 4.0,
        }
    }
}

/// Plan of one frame: loads in load-priority order (nearest first, then by
/// key) and unloads in unload-priority order (farthest first, then by key).
/// Both orders are total and integer, so the plan is deterministic by
/// construction (§2.2, I-5).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StreamPlan {
    /// Cells to load, sorted by `(d2 ascending, CellKey)`.
    pub load: Vec<CellKey>,
    /// Cells to unload, sorted by `(d2 descending, CellKey)`.
    pub unload: Vec<CellKey>,
}

/// Cell that contains the point `p` (x/z by `floor(p / cell_m)`; level `0` —
/// the city policy has a single level; M38 constructs keys with its own
/// level quantization).
///
/// Negative coordinates floor correctly (`-0.1 m` is cell `-1`), and the
/// f64 intermediate keeps the floor stable for large coordinates (an `as`
/// cast saturates instead of wrapping — no UB, same result every run).
pub fn cell_of_point(config: &StreamConfig, p: [f32; 3]) -> CellKey {
    let column = |v: f32| -> i32 {
        let cell = f64::from(v) / f64::from(config.cell_m);
        cell.floor() as i32
    };
    CellKey {
        x: column(p[0]),
        z: column(p[2]),
        level: 0,
    }
}

/// Bounding box of `key` in meters (debug/evidence only — I-5). The y extent
/// `[0, cell_m]` is a display convention, not a world promise.
pub fn span_of(config: &StreamConfig, key: CellKey) -> CellSpan {
    let m = config.cell_m;
    let min_m = [key.x as f32 * m, 0.0, key.z as f32 * m];
    CellSpan {
        key,
        min_m,
        max_m: [min_m[0] + m, m, min_m[2] + m],
    }
}

/// Load window of `center`: every cell with Chebyshev(x/z) `<= radius`, all
/// on `center.level`, in `(x, z)` order — `(2r+1)²` cells, 9 for `r = 1`.
pub fn desired_cells(center: CellKey, radius: u8) -> Vec<CellKey> {
    let r = i32::from(radius);
    let side = 2 * r + 1;
    let mut out = Vec::with_capacity((side * side) as usize);
    for x in (center.x - r)..=(center.x + r) {
        for z in (center.z - r)..=(center.z + r) {
            out.push(CellKey {
                x,
                z,
                level: center.level,
            });
        }
    }
    out
}

/// Squared x/z distance `(dx² + dz²)` in `i64` (no float, no overflow for
/// any reachable key pair). Different levels never compare: callers only use
/// this for same-level cells (the policy keeps windows per level).
fn distance2_xz(center: CellKey, key: CellKey) -> i64 {
    let dx = i64::from(key.x) - i64::from(center.x);
    let dz = i64::from(key.z) - i64::from(center.z);
    dx * dx + dz * dz
}

/// Chebyshev distance (x/z) in cells, `None` when `key` is on another level
/// (out of this window's scope — never loaded, never unloaded here).
fn chebyshev_xz(center: CellKey, key: CellKey) -> Option<i32> {
    if key.level != center.level {
        return None;
    }
    let dx = (i64::from(key.x) - i64::from(center.x)).abs();
    let dz = (i64::from(key.z) - i64::from(center.z)).abs();
    i32::try_from(dx.max(dz)).ok()
}

/// Load priority of `key`: `(d2, CellKey)` — a total integer order (§2.2).
pub fn priority(center: CellKey, key: CellKey) -> (u32, CellKey) {
    let d2 = distance2_xz(center, key);
    (u32::try_from(d2).unwrap_or(u32::MAX), key)
}

/// Plan of one frame against the current `loaded` set (§2.2):
///
/// - `load` = desired but not loaded, nearest first;
/// - `unload` = loaded but farther than `radius + STREAM_HYSTERESIS_CELLS`
///   (strict `>`, see H1 in the module doc), farthest first;
/// - the center cell (the player's own, D-M37-4) is never unloaded, even if
///   a pathological `loaded` set would otherwise mark it;
/// - cells on another `level` are outside this policy's scope: neither
///   loaded (the window keeps `center.level`) nor unloaded.
///
/// `load ∩ unload = ∅` holds by construction (`load ⊆ !loaded`,
/// `unload ⊆ loaded`) and is asserted over a 1000-position seeded walk in
/// the tests below.
pub fn plan(config: &StreamConfig, center: CellKey, loaded: &BTreeSet<CellKey>) -> StreamPlan {
    let unload_above = i32::from(config.radius_cells) + i32::from(STREAM_HYSTERESIS_CELLS);
    let desired: BTreeSet<CellKey> = desired_cells(center, config.radius_cells).into_iter().collect();

    let mut load: Vec<CellKey> = desired.difference(loaded).copied().collect();
    load.sort_by_key(|key| priority(center, *key));

    let mut unload: Vec<CellKey> = loaded
        .iter()
        .copied()
        .filter(|key| *key != center) // D-M37-4: the player's cell is pinned.
        .filter_map(|key| chebyshev_xz(center, key).map(|c| (c, key)))
        .filter(|(c, _)| *c > unload_above) // H1: strict `>` (declared).
        .map(|(_, key)| key)
        .collect();
    unload.sort_by_key(|key| (std::cmp::Reverse(priority(center, *key).0), *key));

    StreamPlan { load, unload }
}

/// FNV-1a 64 of the plan's canonical serialization (marker + both lists in
/// order). Same plan ⇒ same integer; order of arrival is irrelevant because
/// the lists are sorted before they reach this function (§2.6).
pub fn plan_digest(plan: &StreamPlan) -> u64 {
    let mut bytes = Vec::with_capacity(16 + 12 * (plan.load.len() + plan.unload.len()));
    bytes.extend_from_slice(b"M37PLAN1");
    push_keys(&mut bytes, &plan.load);
    push_keys(&mut bytes, &plan.unload);
    rdi::fnv1a64(&bytes)
}

fn push_keys(out: &mut Vec<u8>, keys: &[CellKey]) {
    out.extend_from_slice(&(keys.len() as u32).to_le_bytes());
    for key in keys {
        out.extend_from_slice(&key.x.to_le_bytes());
        out.extend_from_slice(&key.z.to_le_bytes());
        out.extend_from_slice(&key.level.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> StreamConfig {
        StreamConfig::default()
    }

    fn key(x: i32, z: i32) -> CellKey {
        CellKey {
            x,
            z,
            level: 0,
        }
    }

    /// Applies a plan the way the ECS layer does: loads join the set, unloads
    /// leave it.
    fn apply(loaded: &mut BTreeSet<CellKey>, step: &StreamPlan) {
        loaded.extend(step.load.iter().copied());
        for cell in &step.unload {
            loaded.remove(cell);
        }
    }

    #[test]
    fn desired_cells_r1_is_nine_in_order() {
        let got = desired_cells(key(3, -2), 1);
        let want = vec![
            key(2, -3),
            key(2, -2),
            key(2, -1),
            key(3, -3),
            key(3, -2),
            key(3, -1),
            key(4, -3),
            key(4, -2),
            key(4, -1),
        ];
        assert_eq!(got, want, "(x, z) order, 9 cells for R=1");
    }

    #[test]
    fn desired_cells_radius_extremes_and_level() {
        assert_eq!(desired_cells(key(0, 0), 0), vec![key(0, 0)]);
        assert_eq!(desired_cells(key(0, 0), 2).len(), 25);
        let levelled = desired_cells(
            CellKey {
                x: 0,
                z: 0,
                level: 3,
            },
            1,
        );
        assert!(levelled.iter().all(|k| k.level == 3));
    }

    #[test]
    fn cell_of_point_floors_and_handles_negatives() {
        let config = cfg();
        assert_eq!(cell_of_point(&config, [0.0, 0.0, 0.0]), key(0, 0));
        assert_eq!(cell_of_point(&config, [102.3, 5.0, 102.5]), key(0, 1));
        // Exact border belongs to the next cell (floor semantics).
        assert_eq!(cell_of_point(&config, [102.4, 0.0, 0.0]), key(1, 0));
        // Negative world coordinates floor away from zero.
        assert_eq!(cell_of_point(&config, [-0.1, 0.0, -100.0]), key(-1, -1));
    }

    #[test]
    fn span_of_marks_the_block_box() {
        let span = span_of(&cfg(), key(1, -1));
        assert_eq!(span.key, key(1, -1));
        assert_eq!(span.min_m[0], 102.4);
        assert_eq!(span.min_m[2], -102.4);
        assert_eq!(span.max_m[0], 204.8);
        assert_eq!(span.max_m[2], 0.0);
    }

    #[test]
    fn priority_orders_by_distance2_then_key() {
        // (1,1) has d2 = 2, (2,0) has d2 = 4 → nearest first.
        assert!(priority(key(0, 0), key(1, 1)) < priority(key(0, 0), key(2, 0)));
        // Tie on d2 → CellKey order breaks it (total order, no floats).
        assert!(priority(key(0, 0), key(0, 1)) < priority(key(0, 0), key(1, 0)));
    }

    #[test]
    fn plan_on_empty_world_loads_the_full_window() {
        let step = plan(&cfg(), key(0, 0), &BTreeSet::new());
        // Same set as the window (the list itself is priority-ordered).
        let mut got = step.load.clone();
        let mut want = desired_cells(key(0, 0), 1);
        got.sort();
        want.sort();
        assert_eq!(got, want);
        assert_eq!(step.load.len(), 9);
        assert!(step.unload.is_empty());
    }

    /// §2.2 falsifier (B), pure: walking over a cell border loads the new
    /// ring once, and afterwards 20 oscillations of ±0.1 m over the border
    /// produce **zero** loads and **zero** unloads.
    #[test]
    fn hysteresis_border_oscillation_produces_no_churn() {
        let config = cfg();
        let border = RETAIL_BLOCK_M; // 102.4 m: cells (0,·) | (1,·)
        let mut loaded = BTreeSet::new();

        // Stabilization: start at (0,0), load the 3×3 window.
        let first = plan(&config, key(0, 0), &loaded);
        assert_eq!(first.load.len(), 9);
        apply(&mut loaded, &first);

        // Cross the border east: the new ring loads once, nothing unloads
        // (the old ring is inside the hysteresis band).
        let crossed = plan(&config, key(1, 0), &loaded);
        assert_eq!(crossed.load.len(), 3, "only the x=2 ring loads");
        assert!(crossed.unload.is_empty(), "hysteresis keeps the x=-1 ring");
        apply(&mut loaded, &crossed);

        // Oscillate ±0.1 m over the border 20 times: after the first
        // stabilization the plan is empty on every frame.
        let mut pos = border + 0.1;
        for i in 0..20 {
            pos = if pos > border { border - 0.1 } else { border + 0.1 };
            let center = cell_of_point(&config, [pos, 0.0, 0.0]);
            let step = plan(&config, center, &loaded);
            assert!(
                step.load.is_empty() && step.unload.is_empty(),
                "oscillation {i}: center {center:?} produced load {:?} / unload {:?}",
                step.load,
                step.unload
            );
            apply(&mut loaded, &step);
        }
        assert_eq!(loaded.len(), 12, "band = 4×3 cells during the oscillation");
    }

    /// §2.2 falsifier adjacency: `plan()` never emits the same cell in both
    /// lists, over a 1000-position walk with a fixed seed, and the loaded
    /// set never exceeds the hysteresis band.
    #[test]
    fn plan_never_loads_and_unloads_same_cell_over_1000_positions() {
        let config = cfg();
        let mut rng = 0x243F_6A88_85A3_08D3u64; // fixed seed (π digits)
        let mut next = move || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng
        };
        let mut pos = [0.0f32, 0.0, 0.0];
        let mut loaded = BTreeSet::new();
        let max_band = i32::from(config.radius_cells) + i32::from(STREAM_HYSTERESIS_CELLS);

        for _ in 0..1000 {
            let dx = (next() % 301) as i32 - 150;
            let dz = (next() % 301) as i32 - 150;
            pos[0] += dx as f32;
            pos[2] += dz as f32;
            let center = cell_of_point(&config, pos);
            let step = plan(&config, center, &loaded);

            let overlap: Vec<CellKey> = step
                .load
                .iter()
                .filter(|k| step.unload.contains(k))
                .copied()
                .collect();
            assert!(overlap.is_empty(), "load ∩ unload = {overlap:?}");

            apply(&mut loaded, &step);
            for cell in &loaded {
                let c = chebyshev_xz(center, *cell).expect("same level");
                assert!(
                    c <= max_band,
                    "loaded cell {cell:?} escaped the hysteresis band ({c} > {max_band})"
                );
            }
        }
    }

    #[test]
    fn player_cell_is_never_unloaded() {
        let mut loaded = BTreeSet::new();
        // Pathological radius 0 with a far cell: the center stays pinned.
        loaded.insert(key(0, 0));
        loaded.insert(key(5, 5));
        let config = StreamConfig {
            radius_cells: 0,
            ..StreamConfig::default()
        };
        let step = plan(&config, key(0, 0), &loaded);
        assert_eq!(step.unload, vec![key(5, 5)], "far cell unloads, pinned does not");
        assert!(!step.unload.contains(&key(0, 0)));

        // Normal radius: the whole window around the player is pinned even
        // when it is also the only far thing loaded (empty unload list).
        loaded.clear();
        loaded.extend(desired_cells(key(0, 0), 1));
        let step = plan(&cfg(), key(0, 0), &loaded);
        assert!(step.unload.is_empty());
    }

    #[test]
    fn cells_on_other_levels_are_out_of_policy_scope() {
        let mut loaded = BTreeSet::new();
        loaded.insert(CellKey {
            x: 9,
            z: 9,
            level: 1,
        });
        let step = plan(&cfg(), key(0, 0), &loaded);
        assert!(step.unload.is_empty(), "another level is never evicted here");
        assert!(step.load.iter().all(|k| k.level == 0));
    }

    #[test]
    fn unload_is_farthest_first() {
        let config = StreamConfig {
            radius_cells: 1,
            ..StreamConfig::default()
        };
        let mut loaded = BTreeSet::new();
        loaded.extend(desired_cells(key(0, 0), 1));
        loaded.insert(key(0, 3)); // d2 = 9
        loaded.insert(key(0, -4)); // d2 = 16
        let step = plan(&config, key(0, 0), &loaded);
        assert_eq!(step.unload, vec![key(0, -4), key(0, 3)], "farthest first");
    }

    #[test]
    fn plan_digest_is_deterministic_and_sensitive() {
        let config = cfg();
        let mut loaded = BTreeSet::new();
        let a = plan(&config, key(0, 0), &loaded);
        apply(&mut loaded, &a);
        let b = plan(&config, key(1, 0), &loaded);
        assert_eq!(plan_digest(&a), plan_digest(&a), "×2 identical");
        assert_ne!(plan_digest(&a), plan_digest(&b), "different plans differ");
        assert_ne!(plan_digest(&a), plan_digest(&StreamPlan::default()));
    }

    #[test]
    fn fnv1a64_reference_vectors() {
        // Same canonical vectors the rest of the tree pins (rdi::tests).
        assert_eq!(rdi::fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(rdi::fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
    }
}

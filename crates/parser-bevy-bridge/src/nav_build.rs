//! `nav_build` — extract a navigation graph from a dungeon block (M38 N2,
//! F-AUD-02, design `f-aud-01-02-streaming-navmesh` §3.3).
//!
//! **Why it exists:** the pure graph model lives in `behavior::nav_graph`
//! (N1); this module is the bridge-side *extractor* that turns a parsed
//! `.RDB` block into a [`NavBlockGraph`] using the geometry oracle that
//! already exists — the same ARCH3D model resolution as
//! [`crate::interior_retail`] (0 new math, P-1) voxelized by
//! [`crate::collision_grid::CollisionGrid`].
//!
//! **Boundary:** the parser is untouched (§5): `nav_build` consumes
//! `rdb::RdbBlock` and `maps::*` *as they are*. The graph types are pulled
//! from `daggerfall-behavior` — a new `bridge → behavior` edge that does
//! **not** change `cargo tree -p daggerfall-behavior` (behavior keeps 0 deps,
//! I-2).
//!
//! **Determinism (I-5):** node/edge insertion is in fixed lattice/column
//! order with no RNG; the same `.RDB` + same ARCH3D models ⇒ the same graph
//! and the same digest. The measurement harness ([`run_navmesh_measurement`])
//! proves this over real retail blocks ×2.
//!
//! **N3 (seams + portals):** the lattice is anchored at the **block origin**
//! (`cell_origin_m`), so world = origin + local and adjacent blocks share
//! one global 1.6 m lattice (block origins sit on a 51.2 m lattice = 32 ×
//! 1.6 m; z is already negated by `dungeon_grid::block_origin_m`). Action
//! doors become `Door { object_offset }` portals between the two nearest
//! walkable flanks ([`BlockDoor`]); boundaries are stitched by
//! [`stitch_seams`] with the 1.4 m tolerance (D-M38-3) and boundaries
//! without an opening are **counted** (`seams_rejected`, R5) instead of
//! being silently disconnected.
//!
//! **`[INFERENCE]` parameters** (design §7, first to measure in N1–N3):
//! lattice 1.6 m is *measured* (`npc_ai.rs:34`); `LEVEL_STEP_M`, `MAX_LEVELS`
//! and `NPC_RADIUS_M` are inferred and are the first knobs to tune if N3
//! (seams) misbehaves.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use daggerfall_behavior::nav_graph::{
    AiPathFallback, BlockId, DoorPortal, EdgeKind, FOLLOW_SPEED_MPS, NavBlockGraph, NavEdge,
    NavGraph, NavNode, NavPoint, NavStep, NavStats, NavNodeId, PathFollower, PathQuery, PathResult,
    SeamEdge, CELL_METERS, DIAGONAL_COST, DOOR_COST, LEVEL_STEP_M, MAX_EXPANSIONS_DEFAULT,
    MAX_LEVELS, NPC_RADIUS_M, RAMP_COST, SEAM_ALIGN_TOLERANCE_M, SEAM_COST, WALK_COST,
    node_world_m,
};
use daggerfall_parser_rs::rdb::RdbBlock;

use crate::collision_grid::{CellStatus, CollisionGrid};
use crate::dungeon_grid::DungeonGrid;
use crate::interior_retail::RDB_SIDE_M;
use crate::mesh_spec::MeshSpec;

/// One action door fed to the extractor (N3): world position plus the
/// deterministic `.RDB` object offset that becomes the portal identity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockDoor {
    /// Object offset inside the `.RDB` record (`DungeonDoor::object_offset`).
    pub object_offset: usize,
    /// Door centre in world meters, same frame as `DungeonDoor::position_m`.
    pub world_m: [f32; 3],
}

/// Tunables of the extractor; defaults are the `[INFERENCE]` parameters of
/// design §7 (lattice 1.6 m is measured, the rest inferred).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NavBuildConfig {
    /// Lattice cell size, meters (measured `CELL_METERS = 1.6`).
    pub cell_m: f32,
    /// Vertical level quantization step, meters `[INFERENCE]`.
    pub level_step_m: f32,
    /// NPC clearance radius, meters `[INFERENCE]`.
    pub npc_radius_m: f32,
    /// Level clamp before the graph is marked `partial` `[INFERENCE]`.
    pub max_levels: i16,
    /// Vertical sample step while scanning for floors (meters).
    pub floor_sample_m: f32,
}

impl Default for NavBuildConfig {
    fn default() -> Self {
        Self {
            cell_m: CELL_METERS,
            level_step_m: LEVEL_STEP_M,
            npc_radius_m: NPC_RADIUS_M,
            max_levels: MAX_LEVELS,
            floor_sample_m: CELL_METERS / 2.0,
        }
    }
}

/// Block-local position of a lattice node: the lattice is anchored at the
/// block origin (N3), so `world = cell_origin_m + local` and adjacent
/// blocks share one global 1.6 m lattice.
fn node_local(cfg: &NavBuildConfig, cell: [i16; 2], y: f32) -> [f32; 3] {
    [
        (f32::from(cell[0]) + 0.5) * cfg.cell_m,
        y,
        (f32::from(cell[1]) + 0.5) * cfg.cell_m,
    ]
}

/// World position of a lattice node (origin + local).
fn node_world(cell_origin_m: [f32; 3], cfg: &NavBuildConfig, cell: [i16; 2], y: f32) -> [f32; 3] {
    let local = node_local(cfg, cell, y);
    [
        cell_origin_m[0] + local[0],
        cell_origin_m[1] + local[1],
        cell_origin_m[2] + local[2],
    ]
}

/// Extracts a [`NavBlockGraph`] from a block mesh (block-local frame).
///
/// Steps (design §3.3): voxelise → walkable 1.6 m lattice (floor surfaces
/// with NPC clearance) → quantize levels → cardinal/diagonal/ramp edges →
/// door portals ([`BlockDoor`]) → deterministic flood fill
/// (`unreachable_regions`) → stats + digest.
pub fn build_nav_block(
    block_id: BlockId,
    cell_origin_m: [f32; 3],
    mesh: &MeshSpec,
    doors: &[BlockDoor],
    cfg: &NavBuildConfig,
) -> NavBlockGraph {
    let empty = |stats: NavStats| NavBlockGraph {
        block: block_id,
        cell_origin_m,
        nodes: Vec::new(),
        edges: Vec::new(),
        portals: Vec::new(),
        partial: false,
        stats,
        digest: 0,
    };

    let Some((min, max)) = mesh.bounds() else {
        return empty(NavStats::default());
    };
    if mesh.triangle_count() == 0 {
        return empty(NavStats::default());
    }

    let grid = CollisionGrid::from_mesh_spec(mesh, 1.0);

    // Lattice extents: every cell whose centre lies inside the mesh AABB.
    let ix_min = ((min[0] / cfg.cell_m) - 0.5).ceil() as i32;
    let ix_max = ((max[0] / cfg.cell_m) - 0.5).floor() as i32;
    let iz_min = ((min[2] / cfg.cell_m) - 0.5).ceil() as i32;
    let iz_max = ((max[2] / cfg.cell_m) - 0.5).floor() as i32;

    // (level, ix, iz) → node id, for edge wiring.
    let mut index: BTreeMap<(i16, i32, i32), NavNodeId> = BTreeMap::new();
    let mut nodes: Vec<NavNode> = Vec::new();
    let mut over_levels = false;

    for iz in iz_min..=iz_max {
        for ix in ix_min..=ix_max {
            let cell = [ix as i16, iz as i16];
            let local = |y: f32| node_local(cfg, cell, y);
            let cx = local(0.0)[0];
            let cz = local(0.0)[2];

            // Vertical scan: one node per contiguous free run that has a
            // solid floor directly below (or sits on the block's lower bound).
            let mut y = min[1];
            while y < max[1] {
                let here = local(y);
                let below = local(y - cfg.floor_sample_m);
                let is_floor = matches!(grid.classify_point(here), CellStatus::Free)
                    && (matches!(grid.classify_point(below), CellStatus::Solid)
                        || (y - min[1]) < cfg.cell_m);
                if is_floor
                    && grid.is_free([cx, y + cfg.npc_radius_m, cz], cfg.npc_radius_m)
                {
                    let level = (y / cfg.level_step_m).round() as i16;
                    if level >= 0 && level < cfg.max_levels {
                        let key = (level, ix, iz);
                        if let std::collections::btree_map::Entry::Vacant(slot) = index.entry(key) {
                            let id = nodes.len() as NavNodeId;
                            slot.insert(id);
                            nodes.push(NavNode {
                                id,
                                level,
                                cell,
                                y_m: y,
                                walkable_radius_m: cfg.npc_radius_m,
                                flags: 0,
                            });
                        }
                    } else if level >= cfg.max_levels {
                        over_levels = true;
                    }
                    // Skip the rest of this free run (avoid mid-air duplicates).
                    y += cfg.level_step_m;
                    continue;
                }
                y += cfg.floor_sample_m;
            }
        }
    }

    // Edges: 8-neighbourhood on the same level (Walk cardinal / Diagonal),
    // plus vertical Ramp between stacked levels in the same column.
    let mut edges: Vec<Vec<NavEdge>> = vec![Vec::new(); nodes.len()];
    for node in &nodes {
        let from = node.id as usize;
        let from_world = node_local(cfg, node.cell, node.y_m);
        for dz in -1i16..=1 {
            for dx in -1i16..=1 {
                if dx == 0 && dz == 0 {
                    continue;
                }
                let ncell = [node.cell[0] + dx, node.cell[1] + dz];
                let Some(&to) = index.get(&(node.level, ncell[0] as i32, ncell[1] as i32)) else {
                    continue;
                };
                let to_node = &nodes[to as usize];
                let to_world = node_local(cfg, to_node.cell, to_node.y_m);
                let mid = [
                    (from_world[0] + to_world[0]) * 0.5,
                    (from_world[1] + to_world[1]) * 0.5,
                    (from_world[2] + to_world[2]) * 0.5,
                ];
                // Do not cut through a wall between the two cells.
                if matches!(grid.classify_point(mid), CellStatus::Solid) {
                    continue;
                }
                let (cost, kind) = if dx == 0 || dz == 0 {
                    (WALK_COST, EdgeKind::Walk)
                } else {
                    (DIAGONAL_COST, EdgeKind::Diagonal)
                };
                edges[from].push(NavEdge { to, cost, kind });
            }
        }
        // Ramp between stacked levels in the same column (stairs are
        // walkable both ways; N3 correction: the N2 extractor only wired
        // the ascent, which made reachability asymmetric and left real
        // cross-block routes `Unreachable`).
        for other_level in [node.level + 1, node.level - 1] {
            let key = (other_level, node.cell[0] as i32, node.cell[1] as i32);
            if let Some(&to) = index.get(&key) {
                let to_node = &nodes[to as usize];
                if (to_node.y_m - node.y_m).abs() <= cfg.level_step_m * 2.0 {
                    edges[from].push(NavEdge { to, cost: RAMP_COST, kind: EdgeKind::Ramp });
                }
            }
        }
    }

    // Door portals (design §3.2/§3.3 step 5): each action door becomes a
    // `Door { object_offset }` edge between the two nearest walkable flanks
    // *when the wall blocks a direct walk* ("el portal manda; 0 duplicado",
    // §3.4). Flank reach = 2 × SEAM_ALIGN_TOLERANCE_M; the ordering is
    // integer mm² (0 `f32` comparisons, I-5). Doors whose opening already
    // walks freely (or that have < 2 flanks) build no portal — they are
    // counted by the caller (`doors_unflanked`), never faked.
    let mut portals: Vec<DoorPortal> = Vec::new();
    for door in doors {
        let local = [
            door.world_m[0] - cell_origin_m[0],
            door.world_m[1] - cell_origin_m[1],
            door.world_m[2] - cell_origin_m[2],
        ];
        let reach_mm = (f64::from(2.0 * SEAM_ALIGN_TOLERANCE_M) * 1000.0).round() as i64;
        let dy_limit_mm = (f64::from(cfg.level_step_m) * 1000.0).round() as i64;
        let mut flanks: Vec<(i64, NavNodeId)> = Vec::new();
        for node in &nodes {
            let pos = node_local(cfg, node.cell, node.y_m);
            let dx_mm = ((f64::from(pos[0]) - f64::from(local[0])) * 1000.0).round() as i64;
            let dz_mm = ((f64::from(pos[2]) - f64::from(local[2])) * 1000.0).round() as i64;
            let dy_mm = ((f64::from(pos[1]) - f64::from(local[1])) * 1000.0).round() as i64;
            if dx_mm.abs() > reach_mm || dz_mm.abs() > reach_mm || dy_mm.abs() > dy_limit_mm {
                continue;
            }
            flanks.push((dx_mm * dx_mm + dz_mm * dz_mm, node.id));
        }
        flanks.sort_unstable();
        let Some(&(_, a)) = flanks.first() else {
            continue;
        };
        // Second flank: nearest node not already walk-adjacent to `a`
        // (an adjacent pair means the opening already walks; no duplicate).
        let Some(&(_, b)) = flanks
            .iter()
            .skip(1)
            .find(|(_, id)| !edges[a as usize].iter().any(|edge| edge.to == *id))
        else {
            continue;
        };
        let kind = EdgeKind::Door { object_offset: door.object_offset };
        edges[a as usize].push(NavEdge { to: b, cost: DOOR_COST, kind });
        edges[b as usize].push(NavEdge { to: a, cost: DOOR_COST, kind });
        portals.push(DoorPortal { a, b, object_offset: door.object_offset });
    }

    // Deterministic flood fill → unreachable regions (portals count).
    let components = connected_components(&nodes, &edges);
    let unreachable_regions = components.len().saturating_sub(1);

    let edge_count: usize = edges.iter().map(Vec::len).sum();
    let levels: BTreeSet<i16> = nodes.iter().map(|n| n.level).collect();
    let stats = NavStats {
        nodes: nodes.len(),
        edges: edge_count,
        portals: portals.len(),
        levels: levels.len(),
        unreachable_regions,
        seams_rejected: 0,
    };

    let mut graph = NavBlockGraph {
        block: block_id,
        cell_origin_m,
        nodes,
        edges,
        portals,
        partial: over_levels,
        stats,
        digest: 0,
    };
    graph.digest = graph.digest();
    graph
}

/// Deterministic connected components over the block's edges.
fn connected_components(nodes: &[NavNode], edges: &[Vec<NavEdge>]) -> Vec<Vec<NavNodeId>> {
    let mut seen: BTreeSet<NavNodeId> = BTreeSet::new();
    let mut out: Vec<Vec<NavNodeId>> = Vec::new();
    for node in nodes {
        if seen.contains(&node.id) {
            continue;
        }
        let mut component = Vec::new();
        let mut stack = vec![node.id];
        seen.insert(node.id);
        while let Some(current) = stack.pop() {
            component.push(current);
            for edge in &edges[current as usize] {
                if seen.insert(edge.to) {
                    stack.push(edge.to);
                }
            }
        }
        out.push(component);
    }
    out
}

/// Full pipeline: `.RDB` block + ARCH3D resolver → [`NavBlockGraph`]
/// (reuses [`crate::interior_retail::build_retail_interior`] for the mesh —
/// 0 new model math, P-1).
pub fn nav_build_from_rdb(
    block_id: BlockId,
    cell_origin_m: [f32; 3],
    name: &str,
    rdb_block: &RdbBlock,
    resolve_model: &mut dyn FnMut(u32) -> Option<MeshSpec>,
    doors: &[BlockDoor],
    cfg: &NavBuildConfig,
) -> NavBlockGraph {
    let interior = crate::interior_retail::build_retail_interior(name, rdb_block, resolve_model);
    build_nav_block(block_id, cell_origin_m, &interior.mesh, doors, cfg)
}

/// Outcome of one measured block.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockMeasurement {
    /// `.RDB` record name.
    pub name: String,
    /// Nodes in the extracted graph.
    pub nodes: usize,
    /// Directed edges.
    pub edges: usize,
    /// Distinct levels.
    pub levels: usize,
    /// `partial` flag (exceeded `MAX_LEVELS`).
    pub partial: bool,
    /// Action doors fed to the extractor (0 for the standalone sample).
    pub doors: usize,
    /// Door portals actually built (`Door { object_offset }`).
    pub portals: usize,
    /// FNV-1a 64 of the block graph.
    pub digest: u64,
    /// Wall-clock extraction time, milliseconds.
    pub build_ms: f64,
    /// A→B outcome (`Found` with cost/length, or the honest failure).
    pub path: PathSummary,
    /// Diagnosis of the A→B outcome: budget vs topology (N2 `Partial`s).
    pub diagnosis: BudgetDiagnosis,
}

/// Budget ladder used to tell a `Partial` (budget) from `Unreachable`
/// (topology). 256 is the design default `[INFERENCE]`; the ladder shows
/// what real retail routes actually need.
pub const BUDGET_LADDER: [u32; 5] = [256, 1024, 4096, 16_384, 65_536];

/// Diagnosis of the A→B route of one block (assignment N3: presupuesto o
/// topología, measured — never guessed).
#[derive(Debug, Clone, PartialEq)]
pub struct BudgetDiagnosis {
    /// Connected components of the block graph (flood fill).
    pub components: usize,
    /// Regions unreachable from the largest one.
    pub unreachable_regions: usize,
    /// First budget of [`BUDGET_LADDER`] that yields `Found` (`None` = never).
    pub found_at: Option<u32>,
    /// Outcome at the top of the ladder.
    pub final_outcome: &'static str,
}

/// Runs the A→B query up the [`BUDGET_LADDER`] until `Found`.
fn diagnose_budget(graph: &NavBlockGraph) -> BudgetDiagnosis {
    let components = connected_components(&graph.nodes, &graph.edges).len();
    let mut found_at = None;
    let mut final_outcome = "NoGraph";
    if !graph.nodes.is_empty() {
        final_outcome = "Unreachable";
        for budget in BUDGET_LADDER {
            let summary = measure_path_with_budget(graph, budget);
            if summary.outcome == "Found" {
                found_at = Some(budget);
                final_outcome = "Found";
                break;
            }
            final_outcome = summary.outcome;
        }
    }
    BudgetDiagnosis {
        components,
        unreachable_regions: graph.stats.unreachable_regions,
        found_at,
        final_outcome,
    }
}

/// Summary of a measured A→B query.
#[derive(Debug, Clone, PartialEq)]
pub struct PathSummary {
    /// `Found`/`Partial`/`Unreachable`/`NoGraph` tag.
    pub outcome: &'static str,
    /// Number of steps (0 unless a path prefix exists).
    pub steps: usize,
    /// Total integer cost (0 unless a path prefix exists).
    pub cost: u32,
    /// World-space length in meters (0 unless a path prefix exists).
    pub length_m: f32,
    /// path_digest of the steps.
    pub digest: u64,
}

/// Aggregated measurement of the sample + the unique-`.RDB` census + the
/// whole-dungeon assembly (N3).
#[derive(Debug, Clone, PartialEq)]
pub struct NavmeshMeasurement {
    /// Per-block measurements (sample order).
    pub blocks: Vec<BlockMeasurement>,
    /// Distinct `.RDB` names across every retail dungeon (census).
    pub unique_rdb: usize,
    /// Total placed blocks across every retail dungeon (census).
    pub placed_blocks: usize,
    /// `true` when every sample block produced the same digest twice (in-process).
    pub stable: bool,
    /// Highest `levels` count observed across sample + dungeon blocks
    /// (evidence for the `MAX_LEVELS = 8` `[INFERENCE]`).
    pub max_levels_observed: i16,
    /// Blocks flagged `partial` (exceeded `MAX_LEVELS`) across both passes.
    pub partial_blocks: usize,
    /// Whole-dungeon assembly with seams (NAVMESH-001 B).
    pub dungeon: DungeonMeasurement,
    /// `Start → Enter` marker walk of a ramped dungeon (NAVMESH-001 C).
    pub walk: MarkerWalk,
}

/// Loads `MAPS.BSA`/`BLOCKS.BSA`/`ARCH3D.BSA` from an ARENA2 directory and
/// measures the nav extraction over the fixed 5-block sample plus a
/// unique-`.RDB` census across all dungeons. Returns `Err` with the honest
/// reason when ARENA2 is not accessible (never a silent skip).
pub fn run_navmesh_measurement(arena2: &std::path::Path) -> Result<NavmeshMeasurement, String> {
    let read = |file: &str| -> Result<Vec<u8>, String> {
        let path = arena2.join(file);
        std::fs::read(&path).map_err(|e| format!("{file}: {e} ({})", path.display()))
    };

    let maps_data = read("MAPS.BSA")?;
    let blocks_data = read("BLOCKS.BSA")?;
    let arch_data = read("ARCH3D.BSA")?;

    let maps = crate::bsa_source::parse_bsa(&maps_data).map_err(|e| format!("MAPS.BSA: {e}"))?;
    let blocks =
        crate::bsa_source::parse_bsa(&blocks_data).map_err(|e| format!("BLOCKS.BSA: {e}"))?;
    let arch = crate::bsa_source::parse_bsa(&arch_data).map_err(|e| format!("ARCH3D.BSA: {e}"))?;

    // ARCH3D record_id → directory index (for the resolve_model closure).
    let mut arch_index: BTreeMap<u32, usize> = BTreeMap::new();
    for (i, entry) in arch.entries.iter().enumerate() {
        if let crate::bsa_source::Entry::Numbered { record_id, .. } = entry {
            arch_index.insert(*record_id, i);
        }
    }
    // BLOCKS name → directory index (uppercase).
    let mut blocks_index: BTreeMap<String, usize> = BTreeMap::new();
    for (i, entry) in blocks.entries.iter().enumerate() {
        if let crate::bsa_source::Entry::Named { name, .. } = entry {
            blocks_index.insert(name.to_ascii_uppercase(), i);
        }
    }

    let mut resolve_model = |id: u32| -> Option<MeshSpec> {
        let index = *arch_index.get(&id)?;
        crate::bsa_source::mesh_spec_at(&arch, &arch_data, index).ok()
    };

    // Fixed 5-block sample (dungeon_probe.rs:66 start_block names).
    const SAMPLE: [&str; 5] = [
        "N0000006.RDB",
        "N0000050.RDB",
        "W0000011.RDB",
        "M0000001.RDB",
        "M0000000.RDB",
    ];
    let cfg = NavBuildConfig::default();
    let mut block_measurements = Vec::new();
    let mut stable = true;
    let mut max_levels_observed: i16 = 0;
    let mut partial_blocks = 0usize;
    for (block_id, name) in SAMPLE.iter().enumerate() {
        let Some(index) = blocks_index.get(&name.to_ascii_uppercase()) else {
            return Err(format!("{name}: absent from BLOCKS.BSA"));
        };
        let rdb_bytes = crate::bsa_source::entry_data(&blocks, &blocks_data, *index)
            .map_err(|e| format!("{name}: {e}"))?;
        let rdb = daggerfall_parser_rs::rdb::parse(rdb_bytes)
            .map_err(|e| format!("{name}: parse {e}"))?;

        let start = std::time::Instant::now();
        let g1 = nav_build_from_rdb(
            block_id as BlockId,
            [0.0; 3],
            name,
            &rdb,
            &mut resolve_model,
            &[],
            &cfg,
        );
        let build_ms = start.elapsed().as_secs_f64() * 1000.0;
        // Second in-process build (determinism ×2).
        let g2 = nav_build_from_rdb(
            block_id as BlockId,
            [0.0; 3],
            name,
            &rdb,
            &mut resolve_model,
            &[],
            &cfg,
        );
        if g1.digest != g2.digest || g1.nodes != g2.nodes || g1.edges != g2.edges {
            stable = false;
        }
        max_levels_observed = max_levels_observed.max(g1.stats.levels as i16);
        if g1.partial {
            partial_blocks += 1;
        }

        let path = measure_path(&g1);
        let diagnosis = diagnose_budget(&g1);
        block_measurements.push(BlockMeasurement {
            name: name.to_string(),
            nodes: g1.stats.nodes,
            edges: g1.stats.edges,
            levels: g1.stats.levels,
            partial: g1.partial,
            doors: 0,
            portals: g1.stats.portals,
            digest: g1.digest,
            build_ms,
            path,
            diagnosis,
        });
    }

    // Unique-`.RDB` census across every retail dungeon (MAPS only; no ARCH3D).
    let (unique_rdb, placed_blocks) = census_unique_rdb(&maps, &maps_data)?;

    // N3: whole-dungeon assembly with seams + door portals over the
    // canonical 12-block dungeon (dungeon_probe SAMPLE[0]: location 26219).
    let sources = NavmeshSources {
        maps: &maps,
        maps_data: &maps_data,
        blocks: &blocks,
        blocks_data: &blocks_data,
        arch: &arch,
        arch_data: &arch_data,
        blocks_index: &blocks_index,
    };
    let dungeon = measure_dungeon_nav(&sources, 26_219, &cfg)?;
    max_levels_observed = max_levels_observed.max(
        dungeon
            .max_levels_observed,
    );
    partial_blocks += dungeon.partial_blocks;
    if !dungeon.stable {
        stable = false;
    }

    // N4-C: `Start → Enter` walk of a **ramped** dungeon that carries both
    // editor markers and whose markers land in the SAME lattice component.
    // Measured over the retail sample: 26381 (`M0000001.RDB`) carries both
    // markers but Start/Enter fall in different components (`Unreachable`);
    // 26045 resolves to a real `WALK` over a ramped starting block (ramps=1).
    // The canonical 26219 has no `Enter` flat, so it cannot host this gate.
    let walk = measure_marker_walk(&sources, 26_045, &cfg)?;

    Ok(NavmeshMeasurement {
        blocks: block_measurements,
        unique_rdb,
        placed_blocks,
        stable,
        max_levels_observed,
        partial_blocks,
        dungeon,
        walk,
    })
}

/// A→B query over the block graph at the default budget.
fn measure_path(graph: &NavBlockGraph) -> PathSummary {
    measure_path_with_budget(graph, MAX_EXPANSIONS_DEFAULT)
}

/// A→B query over the block graph: start = first node of the largest
/// component, goal = the node farthest from it (BFS over the edges), so the
/// measured path is a long real traversal, not an adjacent pair.
fn measure_path_with_budget(graph: &NavBlockGraph, budget: u32) -> PathSummary {
    if graph.nodes.is_empty() {
        return PathSummary { outcome: "NoGraph", steps: 0, cost: 0, length_m: 0.0, digest: 0 };
    }
    // Wrap the single block into a NavGraph for find_path.
    let mut blocks = BTreeMap::new();
    blocks.insert(graph.block, graph.clone());
    let nav = NavGraph { dungeon_location_id: 0, blocks, seams: Vec::new(), digest: 0 };

    let components = connected_components(&graph.nodes, &graph.edges);
    let largest = components.into_iter().max_by_key(Vec::len).unwrap_or_default();
    let Some(&start_id) = largest.first() else {
        return PathSummary { outcome: "Unreachable", steps: 0, cost: 0, length_m: 0.0, digest: 0 };
    };
    let goal_id = farthest_reachable(graph, start_id);
    let start_node = &graph.nodes[start_id as usize];
    let goal_node = &graph.nodes[goal_id as usize];
    let from = NavPoint { block: graph.block, cell: start_node.cell, level: start_node.level };
    let to = NavPoint { block: graph.block, cell: goal_node.cell, level: goal_node.level };
    let query = PathQuery { from, to, max_expansions: budget };

    match daggerfall_behavior::nav_graph::find_path(&nav, &query) {
        PathResult::Found(steps) => {
            let cost = total_cost(&steps, graph);
            PathSummary {
                outcome: "Found",
                steps: steps.len(),
                cost,
                length_m: path_length_m(&steps, graph),
                digest: daggerfall_behavior::nav_graph::path_digest(&steps),
            }
        }
        PathResult::Partial(steps) => {
            let cost = total_cost(&steps, graph);
            PathSummary {
                outcome: "Partial",
                steps: steps.len(),
                cost,
                length_m: path_length_m(&steps, graph),
                digest: daggerfall_behavior::nav_graph::path_digest(&steps),
            }
        }
        PathResult::Unreachable => PathSummary { outcome: "Unreachable", steps: 0, cost: 0, length_m: 0.0, digest: 0 },
        PathResult::NoGraph { .. } => PathSummary { outcome: "NoGraph", steps: 0, cost: 0, length_m: 0.0, digest: 0 },
    }
}

/// BFS (unweighted hops) from `from_id`; returns the farthest reachable node
/// id (deterministic: ties broken by smallest id).
fn farthest_reachable(graph: &NavBlockGraph, from_id: NavNodeId) -> NavNodeId {
    let mut dist: BTreeMap<NavNodeId, u32> = BTreeMap::new();
    let mut queue = std::collections::VecDeque::new();
    dist.insert(from_id, 0);
    queue.push_back(from_id);
    let mut farthest = from_id;
    let mut best = 0u32;
    while let Some(current) = queue.pop_front() {
        let d = dist[&current];
        for edge in &graph.edges[current as usize] {
            if let std::collections::btree_map::Entry::Vacant(slot) = dist.entry(edge.to) {
                slot.insert(d + 1);
                queue.push_back(edge.to);
                if d + 1 > best || (d + 1 == best && edge.to < farthest) {
                    best = d + 1;
                    farthest = edge.to;
                }
            }
        }
    }
    farthest
}

/// World-space polyline length of a path: lattice cell delta × `CELL_METERS`
/// in x/z plus the node `y_m` delta (the mesh origin cancels between nodes).
fn path_length_m(steps: &[daggerfall_behavior::nav_graph::NavStep], graph: &NavBlockGraph) -> f32 {
    let lookup = |p: &NavPoint| -> Option<(f32, f32, f32)> {
        let node = graph.nodes.iter().find(|n| n.cell == p.cell && n.level == p.level)?;
        Some((f32::from(node.cell[0]), node.y_m, f32::from(node.cell[1])))
    };
    let mut total = 0.0f32;
    for pair in steps.windows(2) {
        if let (Some(a), Some(b)) = (lookup(&pair[0].point), lookup(&pair[1].point)) {
            let dx = (b.0 - a.0) * CELL_METERS;
            let dy = b.1 - a.1;
            let dz = (b.2 - a.2) * CELL_METERS;
            total += (dx * dx + dy * dy + dz * dz).sqrt();
        }
    }
    total
}

/// Sums the integer edge cost along a path (0 for non-adjacent steps).
fn total_cost(steps: &[NavStep], graph: &NavBlockGraph) -> u32 {
    steps
        .windows(2)
        .map(|pair| {
            let a = &pair[0].point;
            let b = &pair[1].point;
            let (Some(an), Some(bn)) = (
                graph.nodes.iter().find(|n| n.cell == a.cell && n.level == a.level),
                graph.nodes.iter().find(|n| n.cell == b.cell && n.level == b.level),
            ) else {
                return 0;
            };
            graph
                .edges
                .get(an.id as usize)
                .and_then(|adj| adj.iter().find(|e| e.to == bn.id).map(|e| u32::from(e.cost)))
                .unwrap_or(0)
        })
        .sum()
}

/// Enumerates every dungeon via MAPS and counts unique `.RDB` names and
/// total placed blocks (closes the `NO-VERIFICADO` precompute multiplier,
/// design §3.3 / R4).
fn census_unique_rdb(
    maps: &crate::bsa_source::BsaArchive,
    maps_data: &[u8],
) -> Result<(usize, usize), String> {
    let mut unique: HashSet<String> = HashSet::new();
    let mut placed = 0usize;

    // Region suffixes = every `MAPNAMES.<nnn>` in the archive.
    let mut suffixes: Vec<String> = Vec::new();
    for entry in maps.entries.iter() {
        if let crate::bsa_source::Entry::Named { name, .. } = entry {
            let upper = name.to_ascii_uppercase();
            if let Some(rest) = upper.strip_prefix("MAPNAMES.") {
                suffixes.push(rest.to_string());
            }
        }
    }
    suffixes.sort();

    for suffix in suffixes {
        let names_key = format!("MAPNAMES.{suffix}");
        let pitem_key = format!("MAPPITEM.{suffix}");
        let ditem_key = format!("MAPDITEM.{suffix}");
        let names_record = named_record(maps, maps_data, &names_key).unwrap_or(&[]);
        let pitem_record = named_record(maps, maps_data, &pitem_key).unwrap_or(&[]);
        let ditem_record = named_record(maps, maps_data, &ditem_key).unwrap_or(&[]);
        if names_record.is_empty() || pitem_record.is_empty() {
            continue;
        }
        let Ok(names) = daggerfall_parser_rs::maps::parse_map_names(names_record) else {
            continue;
        };
        if names.is_empty() {
            continue;
        }
        let Ok(locations) =
            daggerfall_parser_rs::maps::parse_pitem(pitem_record, names.len())
        else {
            continue;
        };
        for location in &locations {
            let Ok(Some(layout)) =
                daggerfall_parser_rs::maps::parse_ditem(ditem_record, location.location_id)
            else {
                continue;
            };
            for block in &layout.blocks {
                placed += 1;
                unique.insert(block.name.to_ascii_uppercase());
            }
        }
    }
    Ok((unique.len(), placed))
}

/// Raw bytes of a named MAPS record (`None` when absent).
fn named_record<'a>(
    archive: &crate::bsa_source::BsaArchive,
    data: &'a [u8],
    key: &str,
) -> Option<&'a [u8]> {
    for (i, entry) in archive.entries.iter().enumerate() {
        if let crate::bsa_source::Entry::Named { name, .. } = entry {
            if name.eq_ignore_ascii_case(key) {
                return crate::bsa_source::entry_data(archive, data, i).ok();
            }
        }
    }
    None
}

/// Seam counters of one assembled dungeon (design §3.4 / R5 — honest, no
/// silent boundaries).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SeamReport {
    /// 4-adjacent boundary pairs considered (grid coordinates).
    pub boundaries: usize,
    /// Walk seams built across aligned openings.
    pub seams: usize,
    /// Seams whose only opening is an action door (`Door { object_offset }`).
    pub door_seams: usize,
    /// Boundaries with neither an aligned opening nor a door: rejected
    /// (the graph keeps two regions and `Unreachable` stays honest).
    pub seams_rejected: usize,
}

/// World position of a node (origin + local, N3 lattice).
fn node_world_pos(graph: &NavBlockGraph, node: &NavNode) -> [f32; 3] {
    node_world(graph.cell_origin_m, &NavBuildConfig::default(), node.cell, node.y_m)
}

/// Node of `graph` nearest to a world position (integer mm² horizontal
/// distance, then |Δy|, then id — 0 `f32` comparisons, I-5).
fn nearest_node(graph: &NavBlockGraph, world_m: [f32; 3]) -> Option<NavNodeId> {
    let cfg = NavBuildConfig::default();
    let mut best: Option<(i64, i64, NavNodeId)> = None;
    for node in &graph.nodes {
        let pos = node_world(graph.cell_origin_m, &cfg, node.cell, node.y_m);
        let dx_mm = ((f64::from(pos[0]) - f64::from(world_m[0])) * 1000.0).round() as i64;
        let dz_mm = ((f64::from(pos[2]) - f64::from(world_m[2])) * 1000.0).round() as i64;
        let dy_mm = ((f64::from(pos[1]) - f64::from(world_m[1])) * 1000.0).round() as i64;
        let key = (dx_mm * dx_mm + dz_mm * dz_mm, dy_mm.abs(), node.id);
        if best.is_none_or(|b| key < b) {
            best = Some(key);
        }
    }
    best.map(|(_, _, id)| id)
}

/// Stitches cross-block seams (design §3.4, D-M38-2/3): for every 4-
/// adjacent boundary pair (grid coordinates) the node pairs on the shared
/// face within `tolerance_m` (lateral + vertical) become `Seam` edges; a
/// boundary blocked on both sides but with an action door on the frontier
/// gets ONE `Door { object_offset }` seam ("el portal manda; 0 duplicado");
/// a boundary with neither is **counted** in `seams_rejected` (R5 — an
/// honest `Unreachable`, never a fake connection). Diagonal adjacency is
/// out of v1 (§3.4) and never stitched here.
pub fn stitch_seams(
    blocks: &BTreeMap<BlockId, NavBlockGraph>,
    grid: &DungeonGrid,
    tolerance_m: f32,
) -> (Vec<SeamEdge>, SeamReport) {
    let mut report = SeamReport::default();
    let mut out: Vec<SeamEdge> = Vec::new();
    let mut by_cell: BTreeMap<(i8, i8), BlockId> = BTreeMap::new();
    for (id, placed) in grid.blocks.iter().enumerate() {
        if blocks.contains_key(&(id as BlockId)) {
            by_cell.insert((placed.x, placed.z), id as BlockId);
        }
    }
    let tol_mm = (f64::from(tolerance_m) * 1000.0).round() as i64;

    for (&(ax, az), &a_id) in &by_cell {
        let a_graph = &blocks[&a_id];
        for (dx, dz) in [(1i8, 0i8), (-1, 0), (0, 1), (0, -1)] {
            let Some(&b_id) = by_cell.get(&(ax.wrapping_add(dx), az.wrapping_add(dz))) else {
                continue;
            };
            if a_id >= b_id {
                continue; // one visit per unordered pair
            }
            report.boundaries += 1;
            let b_graph = &blocks[&b_id];
            let oa = a_graph.cell_origin_m;
            let ob = b_graph.cell_origin_m;
            let axis = if dx != 0 { 0 } else { 2 };
            let sign = if ob[axis] > oa[axis] { 1.0f32 } else { -1.0f32 };
            let boundary = oa[axis] + sign * RDB_SIDE_M;
            let lateral_axis = if axis == 0 { 2 } else { 0 };

            let mut pairs = 0usize;
            for na in &a_graph.nodes {
                let wa = node_world_pos(a_graph, na);
                let sa = ((f64::from(wa[axis]) - f64::from(boundary)) * f64::from(sign) * 1000.0).round() as i64;
                if sa > 0 || -sa > tol_mm {
                    continue; // not on A's face within tolerance
                }
                for nb in &b_graph.nodes {
                    let wb = node_world_pos(b_graph, nb);
                    let sb = ((f64::from(wb[axis]) - f64::from(boundary)) * f64::from(sign) * 1000.0).round() as i64;
                    if sb < 0 || sb > tol_mm {
                        continue; // not on B's face within tolerance
                    }
                    let lateral_mm =
                        ((f64::from(wa[lateral_axis]) - f64::from(wb[lateral_axis])).abs() * 1000.0).round() as i64;
                    let dy_mm =
                        ((f64::from(wa[1]) - f64::from(wb[1])).abs() * 1000.0).round() as i64;
                    if lateral_mm > tol_mm || dy_mm > tol_mm {
                        continue;
                    }
                    out.push(SeamEdge {
                        a: (a_id, na.id),
                        b: (b_id, nb.id),
                        cost: SEAM_COST,
                        via: EdgeKind::Seam { neighbor: b_id },
                    });
                    pairs += 1;
                }
            }
            report.seams += pairs;
            if pairs > 0 {
                continue;
            }

            // No aligned opening: an action door ON the frontier is the
            // only way through (design §3.4). One seam per boundary.
            let mut door_seamed = false;
            for door in &grid.doors {
                let sd = ((f64::from(door.position_m[axis]) - f64::from(boundary)) * f64::from(sign) * 1000.0)
                    .round() as i64;
                if sd.abs() > tol_mm {
                    continue;
                }
                let (Some(a), Some(b)) = (nearest_node(a_graph, door.position_m), nearest_node(b_graph, door.position_m))
                else {
                    continue;
                };
                out.push(SeamEdge {
                    a: (a_id, a),
                    b: (b_id, b),
                    cost: SEAM_COST,
                    via: EdgeKind::Door { object_offset: door.object_offset },
                });
                report.door_seams += 1;
                door_seamed = true;
                break;
            }
            if !door_seamed {
                report.seams_rejected += 1;
            }
        }
    }
    (out, report)
}

/// Assembled whole-dungeon nav: graph + honest counters.
#[derive(Debug, Clone, PartialEq)]
pub struct DungeonNav {
    /// The assembled graph (blocks + seams + digest).
    pub graph: NavGraph,
    /// Seam counters (design §3.4 / R5).
    pub report: SeamReport,
    /// Action doors of the grid.
    pub doors: usize,
    /// Doors whose block built no flanking portal (honest counter).
    pub doors_unflanked: usize,
}

/// Assembles a whole dungeon into a [`NavGraph`] with seams (N3): every
/// parsed block is extracted with its grid doors, then boundaries are
/// stitched by [`stitch_seams`]. `resolve_rdb` returns the `.RDB` bytes of
/// a record name (the harness resolves them through BLOCKS.BSA); missing
/// or unparsed blocks are skipped exactly as `DungeonGrid` recorded them.
pub fn assemble_dungeon_nav(
    dungeon_location_id: u16,
    grid: &DungeonGrid,
    resolve_rdb: &mut dyn FnMut(&str) -> Option<Vec<u8>>,
    resolve_model: &mut dyn FnMut(u32) -> Option<MeshSpec>,
    cfg: &NavBuildConfig,
) -> DungeonNav {
    let mut blocks = BTreeMap::new();
    let mut doors_unflanked = 0usize;
    for (index, placed) in grid.blocks.iter().enumerate() {
        if !placed.parsed {
            continue;
        }
        let Some(bytes) = resolve_rdb(&placed.name) else {
            continue;
        };
        let Ok(rdb) = daggerfall_parser_rs::rdb::parse(&bytes) else {
            continue;
        };
        let doors: Vec<BlockDoor> = grid
            .doors
            .iter()
            .filter(|door| door.block == index)
            .map(|door| BlockDoor { object_offset: door.object_offset, world_m: door.position_m })
            .collect();
        let graph = nav_build_from_rdb(
            index as BlockId,
            placed.origin_m,
            &placed.name,
            &rdb,
            resolve_model,
            &doors,
            cfg,
        );
        let portal_offsets: BTreeSet<usize> =
            graph.portals.iter().map(|portal| portal.object_offset).collect();
        doors_unflanked += doors
            .iter()
            .filter(|door| !portal_offsets.contains(&door.object_offset))
            .count();
        blocks.insert(index as BlockId, graph);
    }
    let (seams, report) = stitch_seams(&blocks, grid, SEAM_ALIGN_TOLERANCE_M);
    let mut graph = NavGraph { dungeon_location_id, blocks, seams, digest: 0 };
    graph.digest = graph.digest();
    DungeonNav { graph, report, doors: grid.doors.len(), doors_unflanked }
}

/// Whole-dungeon assembly measurement (NAVMESH-001 B).
#[derive(Debug, Clone, PartialEq)]
pub struct DungeonMeasurement {
    /// `location_id` of the dungeon.
    pub location_id: u16,
    /// Blocks placed in the grid.
    pub placed: usize,
    /// Blocks with a built graph.
    pub built: usize,
    /// Nodes across the dungeon.
    pub nodes: usize,
    /// Highest `levels` count observed in this dungeon.
    pub max_levels_observed: i16,
    /// Blocks flagged `partial` (exceeded `MAX_LEVELS`).
    pub partial_blocks: usize,
    /// Action doors of the grid.
    pub doors: usize,
    /// Doors that built no portal inside their block.
    pub doors_unflanked: usize,
    /// 4-adjacent boundary pairs considered.
    pub boundaries: usize,
    /// Walk seams built across aligned openings.
    pub seams: usize,
    /// Seams whose only opening is an action door.
    pub door_seams: usize,
    /// Boundaries with no opening at all (R5: honest rejection).
    pub seams_rejected: usize,
    /// FNV-1a 64 of the assembled `NavGraph`.
    pub digest: u64,
    /// `true` when the whole assembly produced the same digest twice.
    pub stable: bool,
    /// Cross-boundary route (gate B).
    pub cross_path: CrossPathSummary,
}

/// Cross-boundary A→B summary (gate B).
#[derive(Debug, Clone, PartialEq)]
pub struct CrossPathSummary {
    /// `Found`/`Partial`/`Unreachable`/`NoGraph`/`NoSeam` tag.
    pub outcome: &'static str,
    /// Steps of the path (0 on failure).
    pub steps: usize,
    /// Steps that cross a block boundary (`Seam`/`Door` kind).
    pub crossings: usize,
    /// Crossing steps tagged `Door { object_offset }`.
    pub door_crossings: usize,
    /// `path_digest` of the steps.
    pub digest: u64,
    /// Blocks visited, in order (`a>b>c`).
    pub blocks: String,
}

/// Every archive handle the dungeon pass needs (BSA custody stays in the
/// harness; grouping keeps [`measure_dungeon_nav`] within argument reason).
#[derive(Debug, Clone, Copy)]
pub struct NavmeshSources<'a> {
    /// `MAPS.BSA` archive.
    pub maps: &'a crate::bsa_source::BsaArchive,
    /// Raw `MAPS.BSA` bytes.
    pub maps_data: &'a [u8],
    /// `BLOCKS.BSA` archive.
    pub blocks: &'a crate::bsa_source::BsaArchive,
    /// Raw `BLOCKS.BSA` bytes.
    pub blocks_data: &'a [u8],
    /// `ARCH3D.BSA` archive.
    pub arch: &'a crate::bsa_source::BsaArchive,
    /// Raw `ARCH3D.BSA` bytes.
    pub arch_data: &'a [u8],
    /// Record name → directory index in `BLOCKS.BSA`.
    pub blocks_index: &'a BTreeMap<String, usize>,
}

/// N3/N4 pass: assembles `location_id` with seams and measures a
/// cross-boundary route. The whole assembly runs ×2 in-process and the
/// digests must match (determinism, N5-D).
pub fn measure_dungeon_nav(
    sources: &NavmeshSources<'_>,
    location_id: u16,
    cfg: &NavBuildConfig,
) -> Result<DungeonMeasurement, String> {
    let NavmeshSources { maps, maps_data, blocks, blocks_data, arch, arch_data, blocks_index } =
        *sources;
    let layout = locate_dungeon(maps, maps_data, location_id)?;

    let mut arch_index: BTreeMap<u32, usize> = BTreeMap::new();
    for (i, entry) in arch.entries.iter().enumerate() {
        if let crate::bsa_source::Entry::Numbered { record_id, .. } = entry {
            arch_index.insert(*record_id, i);
        }
    }
    let mut resolve_model = |id: u32| -> Option<MeshSpec> {
        let index = *arch_index.get(&id)?;
        crate::bsa_source::mesh_spec_at(arch, arch_data, index).ok()
    };
    let mut resolve_rdb = |name: &str| -> Option<Vec<u8>> {
        let index = *blocks_index.get(&name.to_ascii_uppercase())?;
        crate::bsa_source::entry_data(blocks, blocks_data, index)
            .ok()
            .map(<[u8]>::to_vec)
    };

    let grid = {
        let mut closure = |name: &str| -> Option<&[u8]> {
            let index = *blocks_index.get(&name.to_ascii_uppercase())?;
            crate::bsa_source::entry_data(blocks, blocks_data, index).ok()
        };
        crate::dungeon_grid::assemble_dungeon(&layout, &mut closure)
    };

    let nav1 = assemble_dungeon_nav(location_id, &grid, &mut resolve_rdb, &mut resolve_model, cfg);
    let nav2 = assemble_dungeon_nav(location_id, &grid, &mut resolve_rdb, &mut resolve_model, cfg);
    let stable = nav1.graph.digest == nav2.graph.digest && nav1.report == nav2.report;

    let nodes: usize = nav1.graph.blocks.values().map(|b| b.stats.nodes).sum();
    let max_levels_observed = nav1
        .graph
        .blocks
        .values()
        .map(|b| b.stats.levels as i16)
        .max()
        .unwrap_or(0);
    let partial_blocks = nav1.graph.blocks.values().filter(|b| b.partial).count();
    let cross_path = measure_cross_path(&nav1.graph);

    Ok(DungeonMeasurement {
        location_id,
        placed: grid.blocks.len(),
        built: nav1.graph.blocks.len(),
        nodes,
        max_levels_observed,
        partial_blocks,
        doors: nav1.doors,
        doors_unflanked: nav1.doors_unflanked,
        boundaries: nav1.report.boundaries,
        seams: nav1.report.seams,
        door_seams: nav1.report.door_seams,
        seams_rejected: nav1.report.seams_rejected,
        digest: nav1.graph.digest,
        stable,
        cross_path,
    })
}

/// Cross-boundary route over the first seam of the graph (gate B): from the
/// node deepest inside block A to the node deepest inside block B, so the
/// path must actually leave A through the stitched boundary.
fn measure_cross_path(graph: &NavGraph) -> CrossPathSummary {
    let empty = |outcome: &'static str| CrossPathSummary {
        outcome,
        steps: 0,
        crossings: 0,
        door_crossings: 0,
        digest: 0,
        blocks: String::new(),
    };
    let Some(seam) = graph.seams.first() else {
        return empty("NoSeam");
    };
    let Some(a_graph) = graph.blocks.get(&seam.a.0) else {
        return empty("NoGraph");
    };
    let Some(b_graph) = graph.blocks.get(&seam.b.0) else {
        return empty("NoGraph");
    };
    let from_id = farthest_reachable(a_graph, seam.a.1);
    let to_id = farthest_reachable(b_graph, seam.b.1);
    let an = &a_graph.nodes[from_id as usize];
    let bn = &b_graph.nodes[to_id as usize];
    let from = NavPoint { block: seam.a.0, cell: an.cell, level: an.level };
    let to = NavPoint { block: seam.b.0, cell: bn.cell, level: bn.level };
    let query = PathQuery { from, to, max_expansions: BUDGET_LADDER[4] };

    let (outcome, steps) = match daggerfall_behavior::nav_graph::find_path(graph, &query) {
        PathResult::Found(steps) => ("Found", steps),
        PathResult::Partial(steps) => ("Partial", steps),
        PathResult::Unreachable => return empty("Unreachable"),
        PathResult::NoGraph { .. } => return empty("NoGraph"),
    };
    let mut crossings = 0usize;
    let mut door_crossings = 0usize;
    for pair in steps.windows(2) {
        if pair[0].point.block != pair[1].point.block {
            crossings += 1;
            if matches!(pair[1].via, EdgeKind::Door { .. }) {
                door_crossings += 1;
            }
        }
    }
    let mut visited: Vec<BlockId> = Vec::new();
    for step in &steps {
        if visited.last() != Some(&step.point.block) {
            visited.push(step.point.block);
        }
    }
    let blocks_tag = visited.iter().map(|b| b.to_string()).collect::<Vec<_>>().join(">");
    CrossPathSummary {
        outcome,
        steps: steps.len(),
        crossings,
        door_crossings,
        digest: daggerfall_behavior::nav_graph::path_digest(&steps),
        blocks: blocks_tag,
    }
}

/// Outcome of the `Start → Enter` marker walk (N4-C / NAVMESH-001 C).
#[derive(Debug, Clone, PartialEq)]
pub struct MarkerWalk {
    /// Starting block that was walked.
    pub block: BlockId,
    /// `WALK` (a path existed and was followed), `FALLBACK` (no graph ⇒
    /// `AiPathFallback`), or an honest tag (`Unreachable`/`NoGraph`).
    pub outcome: &'static str,
    /// Lattice steps in the path.
    pub steps: usize,
    /// `Ramp` edges traversed (evidence the walk climbed real ramps).
    pub ramps: usize,
    /// Simulated frames walked (60 fps).
    pub frames: u32,
    /// Straight-line distance actually walked (m).
    pub distance_m: f32,
    /// Solid penetrations sampled per frame with `classify_point`
    /// (`collision_grid.rs:215`) — the N4 gate is **0**.
    pub penetrations: usize,
    /// Frames the `resolve_step` clamp deflected (slid or stalled).
    pub blocked: usize,
    /// The follower reached the `Enter` marker.
    pub finished: bool,
    /// `path_digest` of the walked path (0 when no path).
    pub digest: u64,
    /// `AiPathFallback` demo: a query whose block has no graph emits this.
    pub fallback: Option<AiPathFallback>,
}

/// Measures the `Start → Enter` walk of a dungeon's starting block
/// (NAVMESH-001 C, design §3.7): extracts the block's mesh into a
/// [`CollisionGrid`], snaps the two editor markers to their nearest lattice
/// nodes, follows the A\* path with a [`PathFollower`] and clamps every
/// commanded step through `CollisionGrid::resolve_step` (the app owns
/// collision — behavior stays pure). Each frame the agent body (a sphere of
/// `npc_radius` centred one radius over the floor) is sampled with
/// `classify_point`; the gate is **0 solid penetrations**. A companion query
/// to a phantom block proves the `NoGraph ⇒ AiPathFallback` path (design
/// §3.6, escalon 1). Deterministic: same retail bytes ⇒ same walk (I-5).
pub fn measure_marker_walk(
    sources: &NavmeshSources<'_>,
    location_id: u16,
    cfg: &NavBuildConfig,
) -> Result<MarkerWalk, String> {
    let NavmeshSources { maps, maps_data, blocks, blocks_data, arch, arch_data, blocks_index } =
        *sources;
    let layout = locate_dungeon(maps, maps_data, location_id)?;

    let mut arch_index: BTreeMap<u32, usize> = BTreeMap::new();
    for (i, entry) in arch.entries.iter().enumerate() {
        if let crate::bsa_source::Entry::Numbered { record_id, .. } = entry {
            arch_index.insert(*record_id, i);
        }
    }
    let mut resolve_model = |id: u32| -> Option<MeshSpec> {
        let index = *arch_index.get(&id)?;
        crate::bsa_source::mesh_spec_at(arch, arch_data, index).ok()
    };
    let mut resolve_rdb = |name: &str| -> Option<Vec<u8>> {
        let index = *blocks_index.get(&name.to_ascii_uppercase())?;
        crate::bsa_source::entry_data(blocks, blocks_data, index).ok().map(<[u8]>::to_vec)
    };

    let grid = {
        let mut closure = |name: &str| -> Option<&[u8]> {
            let index = *blocks_index.get(&name.to_ascii_uppercase())?;
            crate::bsa_source::entry_data(blocks, blocks_data, index).ok()
        };
        crate::dungeon_grid::assemble_dungeon(&layout, &mut closure)
    };

    let nav = assemble_dungeon_nav(location_id, &grid, &mut resolve_rdb, &mut resolve_model, cfg);

    // Starting block (the only one that carries editor markers, DFU).
    let Some(start_index) = grid.blocks.iter().position(|b| b.is_starting_block) else {
        return Err(format!("location {location_id}: no starting block"));
    };
    let block_id = start_index as BlockId;
    let placed = &grid.blocks[start_index];
    let origin = placed.origin_m;

    // Rebuild the starting block's mesh → the collision oracle for the walk.
    let Some(rdb_bytes) = resolve_rdb(&placed.name) else {
        return Err(format!("{}: absent from BLOCKS.BSA", placed.name));
    };
    let rdb = daggerfall_parser_rs::rdb::parse(&rdb_bytes)
        .map_err(|e| format!("{}: parse {e}", placed.name))?;
    let interior =
        crate::interior_retail::build_retail_interior(&placed.name, &rdb, &mut resolve_model);
    let collision = CollisionGrid::from_mesh_spec(&interior.mesh, 1.0);

    let Some(block_graph) = nav.graph.blocks.get(&block_id) else {
        return Err(format!("location {location_id}: starting block {block_id} has no graph"));
    };

    // Markers (honest when absent — never fabricated).
    let (Some(start_marker), Some(enter_marker)) = (grid.start_marker, grid.enter_marker) else {
        let reason = if grid.start_marker.is_none() { "no Start marker" } else { "no Enter marker" };
        return Err(format!("location {location_id}: {reason}"));
    };

    let (Some(from_id), Some(to_id)) =
        (nearest_node(block_graph, start_marker), nearest_node(block_graph, enter_marker))
    else {
        return Err(format!("location {location_id}: a marker has no lattice node"));
    };
    let from_node = &block_graph.nodes[from_id as usize];
    let to_node = &block_graph.nodes[to_id as usize];
    let from = NavPoint { block: block_id, cell: from_node.cell, level: from_node.level };
    let to = NavPoint { block: block_id, cell: to_node.cell, level: to_node.level };

    // Fallback demo (independent of the walk): a query to a phantom block
    // has no graph ⇒ `AiPathFallback` is emitted, never a fake path.
    let ghost = NavPoint { block: u16::MAX, cell: [0, 0], level: 0 };
    let ghost_result = daggerfall_behavior::nav_graph::find_path(
        &nav.graph,
        &PathQuery { from, to: ghost, max_expansions: MAX_EXPANSIONS_DEFAULT },
    );
    let fallback = AiPathFallback::from_path_result(&ghost_result, from);

    let query = PathQuery { from, to, max_expansions: BUDGET_LADDER[4] };
    let result = daggerfall_behavior::nav_graph::find_path(&nav.graph, &query);

    let steps: &[NavStep] = match &result {
        PathResult::Found(s) | PathResult::Partial(s) => s,
        PathResult::Unreachable | PathResult::NoGraph { .. } => {
            let outcome = match &result {
                PathResult::Unreachable => "Unreachable",
                _ => "NoGraph",
            };
            return Ok(MarkerWalk {
                block: block_id,
                outcome,
                steps: 0,
                ramps: 0,
                frames: 0,
                distance_m: 0.0,
                penetrations: 0,
                blocked: 0,
                finished: false,
                digest: 0,
                fallback,
            });
        }
    };

    let ramps = steps.iter().filter(|s| matches!(s.via, EdgeKind::Ramp)).count();
    let digest = daggerfall_behavior::nav_graph::path_digest(steps);

    let Some(mut follower) = PathFollower::new(&result, &nav.graph, FOLLOW_SPEED_MPS) else {
        return Ok(MarkerWalk {
            block: block_id,
            outcome: "FALLBACK",
            steps: steps.len(),
            ramps,
            frames: 0,
            distance_m: 0.0,
            penetrations: 0,
            blocked: 0,
            finished: false,
            digest,
            fallback,
        });
    };

    // Walk at 60 fps. `pos` tracks the lattice (floor) frame the follower
    // steers in; the collision query is shifted up one radius so the NPC
    // body sphere clears the floor it stands on (constant offset ⇒ the
    // delta is identical in both frames).
    let dt = 1.0 / 60.0;
    let radius = cfg.npc_radius_m;
    let mut pos = node_world_m(block_graph, &from).unwrap_or(start_marker);
    let mut frames = 0u32;
    let mut penetrations = 0usize;
    let mut blocked = 0usize;
    let mut distance_m = 0.0f32;
    let max_frames = 60u32 * 60;
    while !follower.is_finished() && frames < max_frames {
        let commanded = follower.advance(pos, dt);
        let delta =
            [commanded[0] - pos[0], commanded[1] - pos[1], commanded[2] - pos[2]];
        let body_from = [pos[0], pos[1] + radius, pos[2]];
        let from_local =
            [body_from[0] - origin[0], body_from[1] - origin[1], body_from[2] - origin[2]];
        let step = collision.resolve_step(from_local, delta, radius);
        if step.blocked.is_some() {
            blocked += 1;
        }
        if matches!(collision.classify_point(step.pos), CellStatus::Solid) {
            penetrations += 1;
        }
        let resolved =
            [step.pos[0] + origin[0], step.pos[1] + origin[1] - radius, step.pos[2] + origin[2]];
        let dx = resolved[0] - pos[0];
        let dy = resolved[1] - pos[1];
        let dz = resolved[2] - pos[2];
        distance_m += (dx * dx + dy * dy + dz * dz).sqrt();
        pos = resolved;
        frames += 1;
        // A stuck follower reports `Blocked`; stop honestly instead of
        // spinning to the frame cap (design §3.6, escalon 2).
        if follower.fallback().is_some() {
            break;
        }
    }

    Ok(MarkerWalk {
        block: block_id,
        outcome: "WALK",
        steps: steps.len(),
        ramps,
        frames,
        distance_m,
        penetrations,
        blocked,
        finished: follower.is_finished(),
        digest,
        fallback,
    })
}

/// First `MAPDITEM` layout with `location_id` (sorted region suffixes,
/// location order — deterministic).
fn locate_dungeon(
    maps: &crate::bsa_source::BsaArchive,
    maps_data: &[u8],
    location_id: u16,
) -> Result<daggerfall_parser_rs::maps::LocationDungeon, String> {
    let mut suffixes: Vec<String> = Vec::new();
    for entry in maps.entries.iter() {
        if let crate::bsa_source::Entry::Named { name, .. } = entry {
            let upper = name.to_ascii_uppercase();
            if let Some(rest) = upper.strip_prefix("MAPNAMES.") {
                suffixes.push(rest.to_string());
            }
        }
    }
    suffixes.sort();
    for suffix in &suffixes {
        let Some(names_record) = named_record(maps, maps_data, &format!("MAPNAMES.{suffix}")) else {
            continue;
        };
        let Some(pitem_record) = named_record(maps, maps_data, &format!("MAPPITEM.{suffix}")) else {
            continue;
        };
        let Some(ditem_record) = named_record(maps, maps_data, &format!("MAPDITEM.{suffix}")) else {
            continue;
        };
        let Ok(names) = daggerfall_parser_rs::maps::parse_map_names(names_record) else {
            continue;
        };
        if names.is_empty() {
            continue;
        }
        let Ok(locations) = daggerfall_parser_rs::maps::parse_pitem(pitem_record, names.len())
        else {
            continue;
        };
        for location in &locations {
            if location.location_id != location_id {
                continue;
            }
            if let Ok(Some(layout)) =
                daggerfall_parser_rs::maps::parse_ditem(ditem_record, location_id)
            {
                return Ok(layout);
            }
        }
    }
    Err(format!("location {location_id} absent from MAPS.BSA"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A flat floor quad + a raised platform joined by a ramp, as a MeshSpec
    /// (block-local). Enough to exercise the extractor headless (I-6).
    fn two_level_mesh() -> MeshSpec {
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut indices = Vec::new();
        let mut quad = |x0: f32, z0: f32, x1: f32, z1: f32, y: f32| {
            let base = positions.len() as u32;
            positions.extend_from_slice(&[
                [x0, y, z0],
                [x1, y, z0],
                [x1, y, z1],
                [x0, y, z1],
            ]);
            normals.extend_from_slice(&[[0.0, 1.0, 0.0]; 4]);
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        };
        // Ground floor 0..12 m at y=0.
        quad(0.0, -12.0, 12.0, 0.0, 0.0);
        // Raised platform 0..12 x, -12..-6 z at y=3.2 (one level up).
        quad(0.0, -12.0, 12.0, -6.0, 3.2);
        MeshSpec::new(positions, normals, indices)
    }

    #[test]
    fn extraction_produces_nodes_and_is_stable() {
        let cfg = NavBuildConfig::default();
        let mesh = two_level_mesh();
        let a = build_nav_block(0, [0.0; 3], &mesh, &[], &cfg);
        let b = build_nav_block(0, [0.0; 3], &mesh, &[], &cfg);
        assert!(a.stats.nodes > 0, "expected nodes, got 0");
        assert_eq!(a.digest, b.digest, "digest must be stable ×2");
        assert_eq!(a.stats.nodes, b.stats.nodes);
        assert!(a.stats.levels >= 1);
    }

    #[test]
    fn two_level_graph_has_a_walkable_path() {
        let cfg = NavBuildConfig::default();
        let mesh = two_level_mesh();
        let graph = build_nav_block(0, [0.0; 3], &mesh, &[], &cfg);
        let summary = measure_path(&graph);
        // The raised platform is reachable from the ground via the shared edge
        // of the two quads (same lattice columns, adjacent levels).
        assert!(
            summary.outcome == "Found" || summary.outcome == "Partial",
            "expected a path, got {}",
            summary.outcome
        );
        if summary.outcome == "Found" {
            assert!(summary.steps >= 2);
            assert!(summary.cost > 0);
        }
    }

    #[test]
    fn empty_mesh_yields_empty_graph() {
        let cfg = NavBuildConfig::default();
        let doors = [
            BlockDoor { object_offset: 0, world_m: [1.0, 0.0, -1.0] },
            BlockDoor { object_offset: 1, world_m: [2.0, 0.0, -2.0] },
            BlockDoor { object_offset: 2, world_m: [3.0, 0.0, -3.0] },
        ];
        let graph = build_nav_block(
            0,
            [0.0; 3],
            &MeshSpec::new(vec![], vec![], vec![]),
            &doors,
            &cfg,
        );
        assert_eq!(graph.stats.nodes, 0);
        // No geometry ⇒ no flanking nodes ⇒ 0 portals (honest, never faked).
        assert_eq!(graph.stats.portals, 0);
    }

    /// Mesh from `(x0, z0, x1, z1, y)` quads (up-facing, block-local).
    fn quad_mesh(quads: &[[f32; 5]]) -> MeshSpec {
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut indices = Vec::new();
        for q in quads {
            let base = positions.len() as u32;
            positions.extend_from_slice(&[
                [q[0], q[4], q[1]],
                [q[2], q[4], q[1]],
                [q[2], q[4], q[3]],
                [q[0], q[4], q[3]],
            ]);
            normals.extend_from_slice(&[[0.0, 1.0, 0.0]; 4]);
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        MeshSpec::new(positions, normals, indices)
    }

    /// Full-footprint floor (0..51.2 × −51.2..0) at y = 0 plus a ceiling at
    /// y = 3.2: the extractor's vertical scan needs headroom above the floor
    /// (1 m collision grid ⇒ the node sits one free cell over the surface,
    /// same effect already visible in `two_level_mesh`).
    fn full_floor() -> MeshSpec {
        quad_mesh(&[[0.0, -51.2, 51.2, 0.0, 0.0], [0.0, -51.2, 51.2, 0.0, 3.2]])
    }

    fn placed(name: &str, x: i8, z: i8) -> crate::dungeon_grid::PlacedBlock {
        crate::dungeon_grid::PlacedBlock {
            name: name.to_string(),
            x,
            z,
            origin_m: crate::dungeon_grid::block_origin_m(x, z),
            is_starting_block: x == 0 && z == 0,
            parsed: true,
        }
    }

    fn empty_grid(blocks: Vec<crate::dungeon_grid::PlacedBlock>) -> DungeonGrid {
        let min_x = blocks.iter().map(|b| b.x).min().unwrap_or(0);
        let max_x = blocks.iter().map(|b| b.x).max().unwrap_or(0);
        let min_z = blocks.iter().map(|b| b.z).min().unwrap_or(0);
        let max_z = blocks.iter().map(|b| b.z).max().unwrap_or(0);
        DungeonGrid {
            blocks,
            min_x,
            max_x,
            min_z,
            max_z,
            doors: Vec::new(),
            doors_removed: 0,
            start_marker: None,
            enter_marker: None,
            missing_blocks: Vec::new(),
            unparsed_blocks: Vec::new(),
        }
    }

    #[test]
    fn stitch_seams_connects_aligned_faces() {
        let cfg = NavBuildConfig::default();
        let mesh = full_floor();
        let mut blocks = BTreeMap::new();
        blocks.insert(0u16, build_nav_block(0, [0.0; 3], &mesh, &[], &cfg));
        blocks.insert(1u16, build_nav_block(1, [51.2, 0.0, 0.0], &mesh, &[], &cfg));
        assert!(blocks[&0].stats.nodes > 0 && blocks[&1].stats.nodes > 0);

        let grid = empty_grid(vec![placed("A.RDB", 0, 0), placed("B.RDB", 1, 0)]);
        let (seams, report) = stitch_seams(&blocks, &grid, SEAM_ALIGN_TOLERANCE_M);
        assert_eq!(report.boundaries, 1);
        // 32 aligned face pairs (one per lattice row of the 51.2 m face).
        assert_eq!(report.seams, 32);
        assert_eq!(report.seams_rejected, 0);
        assert_eq!(report.door_seams, 0);
        assert_eq!(seams.len(), 32);
        for seam in &seams {
            assert_eq!(seam.a.0, 0);
            assert_eq!(seam.b.0, 1);
            assert_eq!(seam.cost, SEAM_COST);
            assert_eq!(seam.via, EdgeKind::Seam { neighbor: 1 });
            // Face cells: ix 31 on the A side, ix 0 on the B side.
            assert_eq!(blocks[&0].nodes[seam.a.1 as usize].cell[0], 31);
            assert_eq!(blocks[&1].nodes[seam.b.1 as usize].cell[0], 0);
        }
    }

    #[test]
    fn stitch_seams_counts_boundary_without_opening() {
        let cfg = NavBuildConfig::default();
        let mesh = full_floor();
        let mut blocks = BTreeMap::new();
        blocks.insert(0u16, build_nav_block(0, [0.0; 3], &mesh, &[], &cfg));
        // Block 1 parsed but empty (no geometry ⇒ no nodes): the boundary
        // can never be stitched and must be *counted*, not silently dropped.
        let empty_mesh = MeshSpec::new(vec![], vec![], vec![]);
        blocks.insert(1u16, build_nav_block(1, [51.2, 0.0, 0.0], &empty_mesh, &[], &cfg));
        let grid = empty_grid(vec![placed("A.RDB", 0, 0), placed("B.RDB", 1, 0)]);
        let (seams, report) = stitch_seams(&blocks, &grid, SEAM_ALIGN_TOLERANCE_M);
        assert_eq!(report.boundaries, 1);
        assert_eq!(report.seams, 0);
        assert_eq!(report.door_seams, 0);
        assert_eq!(report.seams_rejected, 1, "an opening-less boundary is rejected, loudly");
        assert!(seams.is_empty());
    }

    #[test]
    fn door_portal_bridges_rooms_blocked_by_gap() {
        let cfg = NavBuildConfig::default();
        // Two rooms separated by a 3.2 m gap: no lattice node can exist in
        // the gap (no floor below) so no walk edge crosses; the action door
        // in the gap must produce the only connection.
        let mesh = quad_mesh(&[
            [0.0, -3.2, 6.4, 0.0, 0.0],
            [0.0, -3.2, 6.4, 0.0, 3.2],
            [9.6, -3.2, 16.0, 0.0, 0.0],
            [9.6, -3.2, 16.0, 0.0, 3.2],
        ]);
        let doors = [BlockDoor { object_offset: 4242, world_m: [8.0, 1.0, -1.6] }];
        let graph = build_nav_block(0, [0.0; 3], &mesh, &doors, &cfg);
        assert_eq!(graph.stats.portals, 1, "the door must build exactly one portal");
        assert_eq!(graph.portals[0].object_offset, 4242);
        // The portal is the only cross-gap connection: the route resolves.
        let summary = measure_path(&graph);
        assert_eq!(summary.outcome, "Found", "portal must bridge the rooms");
        let uses_door = graph.edges.iter().flatten().any(|e| matches!(e.kind, EdgeKind::Door { object_offset: 4242 }));
        assert!(uses_door, "a Door edge with the door identity must exist");
    }
}

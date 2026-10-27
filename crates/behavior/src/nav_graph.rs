//! `nav_graph` — dungeon navigation graph + A* pathfinding (M38 / F-AUD-02).
//!
//! **Why it exists:** the dungeon is assembled as a grid of `.RDB` blocks
//! ([`crate::nav_graph` consumers live in the bridge's `nav_build`]) but no
//! agent *navigates* it — there was 0 pathfinding before M38. This module
//! gives the pure graph model and a deterministic A* query so an NPC can ask
//! "how do I walk from A to B" without touching Bevy, I/O or the parser.
//!
//! **Purity (I-2, non-negotiable):** `behavior` has **0 dependencies** and
//! stays that way. Everything here is plain data + integer arithmetic + a
//! local FNV-1a 64 (see the crate-local `determinism` module); the world
//! enters as a
//! `&NavGraph` the bridge built. `cargo tree -p daggerfall-behavior` before
//! and after this module is **byte-identical** (0 new edges to the DAG).
//!
//! **Integer costs (I-5):** every edge cost and the A* ordering are integers;
//! no `f32` ever decides the order (the `y_m`/`cell_origin_m` floats are
//! evidence/geometry only, never compared for ordering). Same graph ⇒ same
//! path ⇒ same digest, with no RNG anywhere in the query.
//!
//! Sources of the constants (DoD §b declaration, design `f-aud-01-02-streaming-navmesh`):
//! - lattice 1.6 m = `CELL_METERS` (`npc_ai.rs:34`) — the graph and the city
//!   navgrid speak the same unit (re-use, no second lattice).
//! - `DOOR_OVERLAP_TOLERANCE_M = 1.4` (`dungeon_grid.rs:62`) reused as the
//!   seam alignment magnitude (D-M38-3).
//! - `LEVEL_STEP_M`, `MAX_LEVELS`, `NPC_RADIUS_M`, `MAX_EXPANSIONS_DEFAULT`
//!   are `[INFERENCE]` (design §7): first parameters to measure in N1–N3.

use std::collections::BTreeMap;
use std::cmp::Reverse;

use crate::determinism::fnv1a64;

/// Lattice cell size in meters (`npc_ai.rs:34` `CELL_METERS`).
pub const CELL_METERS: f32 = 1.6;
/// Vertical distance between quantized levels, meters `[INFERENCE]` (design §7).
pub const LEVEL_STEP_M: f32 = 3.2;
/// Maximum levels per block before the graph is marked `partial` `[INFERENCE]`.
pub const MAX_LEVELS: i16 = 8;
/// Clearance radius for a walkable node, meters `[INFERENCE]` (design §3.3).
pub const NPC_RADIUS_M: f32 = 0.35;
/// Default A* expansion budget per query `[INFERENCE]` (design §3.5).
pub const MAX_EXPANSIONS_DEFAULT: u32 = 256;
/// Seam alignment tolerance, meters `[INFERENCE]` (D-M38-3, reuses 1.4 m).
pub const SEAM_ALIGN_TOLERANCE_M: f32 = 1.4;

/// Axial walk cost (integer, design §3.2).
pub const WALK_COST: u16 = 10;
/// Diagonal walk cost (integer, ≈ √2 × walk).
pub const DIAGONAL_COST: u16 = 14;
/// Ramp/climb cost (integer).
pub const RAMP_COST: u16 = 12;
/// Door traversal cost (integer, on top of the step).
pub const DOOR_COST: u16 = 5;
/// Seam (cross-block) cost (integer).
pub const SEAM_COST: u16 = 5;
/// Vertical level-change cost (integer).
pub const LEVEL_STEP_COST: u16 = 20;

/// Index of a block inside a [`NavGraph`] (matches `DungeonGrid::blocks`).
pub type BlockId = u16;
/// Stable node id **within** one block's graph.
pub type NavNodeId = u32;

/// A world location addressed by block + lattice cell + level (D-M38-1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NavPoint {
    /// Block index.
    pub block: BlockId,
    /// Lattice cell `[x, z]` inside the block.
    pub cell: [i16; 2],
    /// Quantized vertical level.
    pub level: i16,
}

/// Kind of an edge (D-M38-1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EdgeKind {
    /// Cardinal step on the same level.
    Walk,
    /// Diagonal step on the same level.
    Diagonal,
    /// Vertical connection (stairs/ramp) between levels.
    Ramp,
    /// Through an action door; identity is the `.RDB` object offset.
    Door {
        /// Object offset of the door inside the block (deterministic identity).
        object_offset: usize,
    },
    /// Cross-block seam (built in N3).
    Seam {
        /// Neighbor block across the seam.
        neighbor: BlockId,
    },
}

/// One navigable node (D-M38-1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NavNode {
    /// Stable id within the block.
    pub id: NavNodeId,
    /// Quantized vertical level.
    pub level: i16,
    /// Lattice cell `[x, z]`.
    pub cell: [i16; 2],
    /// Surface Y in meters (geometry evidence only; never orders A*).
    pub y_m: f32,
    /// Walkable clearance radius recorded at build time (meters).
    pub walkable_radius_m: f32,
    /// Bit flags (reserved; 0 today).
    pub flags: u8,
}

/// One directed edge out of a node (D-M38-1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NavEdge {
    /// Target node id (same block).
    pub to: NavNodeId,
    /// Integer traversal cost.
    pub cost: u16,
    /// Kind of traversal.
    pub kind: EdgeKind,
}

/// A door portal: the two nodes flanking an action door (design §3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DoorPortal {
    /// Node on one side of the door plane.
    pub a: NavNodeId,
    /// Node on the other side.
    pub b: NavNodeId,
    /// Object offset of the door inside the block.
    pub object_offset: usize,
}

/// Cross-block seam between two nodes (built in N3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeamEdge {
    /// Endpoint in block A.
    pub a: (BlockId, NavNodeId),
    /// Endpoint in block B.
    pub b: (BlockId, NavNodeId),
    /// Integer traversal cost.
    pub cost: u16,
    /// How the seam is crossed: `Seam { neighbor }` for an aligned opening;
    /// `Door { object_offset }` when the boundary wall leaves an opening only
    /// at an action door (design §3.4 — the portal wins, 0 duplicates).
    pub via: EdgeKind,
}

/// Build/telemetry stats of one block graph (honest counters, no silence).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NavStats {
    /// Nodes in the graph.
    pub nodes: usize,
    /// Directed edges in the graph.
    pub edges: usize,
    /// Door portals.
    pub portals: usize,
    /// Distinct levels present.
    pub levels: usize,
    /// Flood-fill regions unreachable from the largest region.
    pub unreachable_regions: usize,
    /// Seams rejected at a boundary (N3; 0 until then).
    pub seams_rejected: usize,
}

/// Navigation graph of ONE `.RDB` block (D-M38-1).
#[derive(Debug, Clone, PartialEq)]
pub struct NavBlockGraph {
    /// Block index (matches `DungeonGrid::blocks`).
    pub block: BlockId,
    /// Block origin in meters (Bevy frame); nodes are relative to it.
    pub cell_origin_m: [f32; 3],
    /// Nodes, ordered by id.
    pub nodes: Vec<NavNode>,
    /// Adjacency: `edges[i]` are the outgoing edges of `nodes[i]`.
    pub edges: Vec<Vec<NavEdge>>,
    /// Door portals of this block.
    pub portals: Vec<DoorPortal>,
    /// `true` when the block exceeded [`MAX_LEVELS`] (degraded, not truncated silently).
    pub partial: bool,
    /// Telemetry counters.
    pub stats: NavStats,
    /// FNV-1a 64 of the canonical graph bytes.
    pub digest: u64,
}

impl NavBlockGraph {
    /// Canonical, deterministic byte rendering (feeds the digest).
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = String::new();
        out.push_str(&format!(
            "block {} origin {:.3},{:.3},{:.3} partial={} nodes={} edges={} portals={} levels={} unreachable={} seams_rejected={}\n",
            self.block,
            self.cell_origin_m[0], self.cell_origin_m[1], self.cell_origin_m[2],
            u8::from(self.partial),
            self.stats.nodes, self.stats.edges, self.stats.portals,
            self.stats.levels, self.stats.unreachable_regions, self.stats.seams_rejected,
        ));
        for node in &self.nodes {
            out.push_str(&format!(
                "n{} cell {},{} level {} y {:.3} r {:.3} flags {}\n",
                node.id, node.cell[0], node.cell[1], node.level,
                node.y_m, node.walkable_radius_m, node.flags,
            ));
        }
        for (i, adjacency) in self.edges.iter().enumerate() {
            for edge in adjacency {
                out.push_str(&format!(
                    "e{} ->{} cost {} kind {}\n",
                    i, edge.to, edge.cost, edge_kind_tag(edge.kind),
                ));
            }
        }
        for portal in &self.portals {
            out.push_str(&format!(
                "p {} {} off {}\n", portal.a, portal.b, portal.object_offset,
            ));
        }
        out.into_bytes()
    }

    /// FNV-1a 64 of [`Self::canonical_bytes`].
    pub fn digest(&self) -> u64 {
        fnv1a64(&self.canonical_bytes())
    }
}

/// Assembled navigation graph of a whole dungeon (D-M38-1).
#[derive(Debug, Clone, PartialEq)]
pub struct NavGraph {
    /// `LocationId` of the dungeon.
    pub dungeon_location_id: u16,
    /// Block graphs keyed by block index (ordered, deterministic).
    pub blocks: BTreeMap<BlockId, NavBlockGraph>,
    /// Cross-block seams (built in N3; empty until then).
    pub seams: Vec<SeamEdge>,
    /// FNV-1a 64 of the canonical dungeon bytes.
    pub digest: u64,
}

impl NavGraph {
    /// Canonical, deterministic byte rendering of the whole dungeon.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = String::new();
        out.push_str(&format!(
            "dungeon {} blocks {} seams {}\n",
            self.dungeon_location_id, self.blocks.len(), self.seams.len(),
        ));
        for (block_id, graph) in &self.blocks {
            out.push_str(&format!("B{} digest {:016x}\n", block_id, graph.digest));
        }
        for seam in &self.seams {
            out.push_str(&format!(
                "S {},{} -> {},{} cost {} kind {}\n",
                seam.a.0, seam.a.1, seam.b.0, seam.b.1, seam.cost,
                edge_kind_tag(seam.via),
            ));
        }
        out.into_bytes()
    }

    /// FNV-1a 64 of [`Self::canonical_bytes`].
    pub fn digest(&self) -> u64 {
        fnv1a64(&self.canonical_bytes())
    }
}

/// Which blocks of a [`NavGraph`] a query may touch (design §4, I-6).
///
/// - [`Self::WholeDungeon`]: the whole dungeon is resident (the regime of
///   N1–N4: M37 streaming is not required for M38 to work).
/// - [`Self::StreamedCells`]: only `loaded` blocks are navigable; queries
///   honour D-M37-8 — no step may point at a block outside the window. The
///   `loaded` set is the filter so this enum stays in `behavior` (0 deps,
///   I-2); the app-side adapter passes `&StreamWindow::loaded` mapped to
///   block ids (integration declared in design §4, after F3 + N3).
#[derive(Debug, Clone, Copy)]
pub enum NavGraphSource<'a> {
    /// Every block of the dungeon is resident.
    WholeDungeon(&'a NavGraph),
    /// Streaming window: only `loaded` blocks are navigable.
    StreamedCells {
        /// The full graph (blocks outside the window are filtered out).
        graph: &'a NavGraph,
        /// Block ids currently resident.
        loaded: &'a std::collections::BTreeSet<BlockId>,
    },
}

impl NavGraphSource<'_> {
    /// The underlying graph.
    pub fn graph(&self) -> &NavGraph {
        match self {
            Self::WholeDungeon(graph) => graph,
            Self::StreamedCells { graph, .. } => graph,
        }
    }

    /// Is `block` navigable under this source?
    pub fn is_block_loaded(&self, block: BlockId) -> bool {
        match self {
            Self::WholeDungeon(_) => true,
            Self::StreamedCells { loaded, .. } => loaded.contains(&block),
        }
    }
}

/// Stable short tag of an edge kind (canonical text).
fn edge_kind_tag(kind: EdgeKind) -> String {
    match kind {
        EdgeKind::Walk => "walk".to_string(),
        EdgeKind::Diagonal => "diag".to_string(),
        EdgeKind::Ramp => "ramp".to_string(),
        EdgeKind::Door { object_offset } => format!("door:{object_offset}"),
        EdgeKind::Seam { neighbor } => format!("seam:{neighbor}"),
    }
}

/// A step of a computed path: the node reached and the edge kind used to
/// reach it (the first step is the start, tagged `Walk` by convention).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NavStep {
    /// The node reached by this step.
    pub point: NavPoint,
    /// Edge kind taken to reach `point` (`Walk` for the start).
    pub via: EdgeKind,
}

/// Result of [`find_path`] (design §3.5 — 4 honest outcomes, 0 fakes).
#[derive(Debug, Clone, PartialEq)]
pub enum PathResult {
    /// Complete, valid path from `from` to `to`.
    Found(Vec<NavStep>),
    /// Valid prefix; the expansion budget ran out (retry next tick).
    Partial(Vec<NavStep>),
    /// Two regions with no connecting portal: no path exists.
    Unreachable,
    /// A referenced block has no graph (absent or unparsed).
    NoGraph {
        /// The block that is missing.
        block: BlockId,
    },
}

/// One path query (design §3.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathQuery {
    /// Start location.
    pub from: NavPoint,
    /// Goal location.
    pub to: NavPoint,
    /// A* expansion budget (`0` ⇒ [`MAX_EXPANSIONS_DEFAULT`]).
    pub max_expansions: u32,
}

/// Internal edge for the flattened A* view: `(from_index, edge, to_index, to_block)`.
struct FlatEdge {
    to: usize,
    cost: u16,
    kind: EdgeKind,
}

/// Flattened node used by A*.
struct FlatNode {
    block: BlockId,
    cell: [i16; 2],
    level: i16,
    edges: Vec<FlatEdge>,
}

/// Effective A* expansion budget (`0` ⇒ [`MAX_EXPANSIONS_DEFAULT`]).
fn budget_of(query: &PathQuery) -> u32 {
    if query.max_expansions == 0 {
        MAX_EXPANSIONS_DEFAULT
    } else {
        query.max_expansions
    }
}

/// Lower bound of the integer cost between two lattice points: diagonal
/// moves first ([`DIAGONAL_COST`] each), then cardinal ([`WALK_COST`]), plus
/// [`RAMP_COST`] per vertical level. Admissible with the real edge costs.
///
/// **N3 correction (design §3.5):** the N1 Manhattan heuristic
/// `(dx + dz) * WALK_COST` is *not* admissible — a diagonal step costs 14,
/// not 20, and a ramp costs 12, not `LEVEL_STEP_COST = 20`. Overestimation
/// breaks A*'s optimality contract and inflates expansions (real N2 routes
/// came back `Partial` at the 256 budget). The lattice deltas stay integer
/// (I-5).
fn h_cost(cell: [i16; 2], level: i16, goal_cell: [i16; 2], goal_level: i16) -> u32 {
    let dx = (i32::from(cell[0]) - i32::from(goal_cell[0])).unsigned_abs();
    let dz = (i32::from(cell[1]) - i32::from(goal_cell[1])).unsigned_abs();
    let (lo, hi) = (dx.min(dz), dx.max(dz));
    let horizontal = lo * u32::from(DIAGONAL_COST) + (hi - lo) * u32::from(WALK_COST);
    let vertical =
        (i32::from(level) - i32::from(goal_level)).unsigned_abs() * u32::from(RAMP_COST);
    horizontal + vertical
}

/// Flattens the graph into one index space, honouring `keep` (design §4:
/// the window filter of [`NavGraphSource`]). Deterministic: BTreeMap order
/// by block id, then node id order. A seam connects both sides only when
/// **both** blocks are kept — no edge may point outside the window.
fn flatten(
    graph: &NavGraph,
    keep: impl Fn(BlockId) -> bool,
) -> (Vec<FlatNode>, BTreeMap<(BlockId, NavNodeId), usize>) {
    let mut flat: Vec<FlatNode> = Vec::new();
    let mut index: BTreeMap<(BlockId, NavNodeId), usize> = BTreeMap::new();
    for (block_id, block_graph) in &graph.blocks {
        if !keep(*block_id) {
            continue;
        }
        for node in &block_graph.nodes {
            index.insert((*block_id, node.id), flat.len());
            flat.push(FlatNode {
                block: *block_id,
                cell: node.cell,
                level: node.level,
                edges: Vec::new(),
            });
        }
    }
    // Intra-block edges.
    for (block_id, block_graph) in &graph.blocks {
        if !keep(*block_id) {
            continue;
        }
        for (i, node) in block_graph.nodes.iter().enumerate() {
            let Some(&from_index) = index.get(&(*block_id, node.id)) else {
                continue;
            };
            for edge in &block_graph.edges[i] {
                if let Some(&to_index) = index.get(&(*block_id, edge.to)) {
                    flat[from_index].edges.push(FlatEdge {
                        to: to_index,
                        cost: edge.cost,
                        kind: edge.kind,
                    });
                }
            }
        }
    }
    // Cross-block seams (N3): the crossing step keeps its `via` kind —
    // `Seam { neighbor }` for an aligned opening, `Door { object_offset }`
    // when the portal is the only opening (design §3.4).
    for seam in &graph.seams {
        if let (Some(&a_index), Some(&b_index)) = (index.get(&seam.a), index.get(&seam.b)) {
            flat[a_index].edges.push(FlatEdge { to: b_index, cost: seam.cost, kind: seam.via });
            let back = match seam.via {
                EdgeKind::Door { object_offset } => EdgeKind::Door { object_offset },
                _ => EdgeKind::Seam { neighbor: seam.a.0 },
            };
            flat[b_index].edges.push(FlatEdge { to: a_index, cost: seam.cost, kind: back });
        }
    }
    (flat, index)
}

/// A* core over a flattened graph: integer costs, admissible [`h_cost`],
/// tie-break by flat index for a total order, **0 RNG** (design §3.5, I-5).
/// Returns `Found`, `Partial` (budget exhausted) or `Unreachable`.
fn run_astar(flat: &[FlatNode], start: usize, goal_index: usize, budget: u32) -> PathResult {
    let heuristic = |index: usize| -> u32 {
        h_cost(
            flat[index].cell,
            flat[index].level,
            flat[goal_index].cell,
            flat[goal_index].level,
        )
    };

    let mut g_score: BTreeMap<usize, u32> = BTreeMap::new();
    let mut parent: BTreeMap<usize, (usize, EdgeKind)> = BTreeMap::new();
    // Open set as a max-heap of Reverse((f, Reverse(g), Reverse(index))): min f,
    // then max g (deeper first), then min index — a total, deterministic order.
    let mut open: std::collections::BinaryHeap<(Reverse<u32>, Reverse<u32>, Reverse<usize>)> =
        std::collections::BinaryHeap::new();

    g_score.insert(start, 0);
    open.push((Reverse(heuristic(start)), Reverse(0), Reverse(start)));

    let mut expansions: u32 = 0;
    let mut best_by_h: (u32, usize) = (heuristic(start), start); // closest-to-goal seen
    let mut closed: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();

    while let Some((Reverse(f), Reverse(g), Reverse(index))) = open.pop() {
        if closed.contains(&index) {
            continue;
        }
        closed.insert(index);

        if index == goal_index {
            return PathResult::Found(reconstruct(flat, &parent, start, goal_index));
        }

        let h = f.saturating_sub(g);
        if h < best_by_h.0 || (h == best_by_h.0 && index < best_by_h.1) {
            best_by_h = (h, index);
        }

        expansions += 1;
        if expansions > budget {
            // Budget exhausted: honest Partial with the prefix to the
            // closest-to-goal node reached so far.
            let prefix = reconstruct(flat, &parent, start, best_by_h.1);
            return PathResult::Partial(prefix);
        }

        for edge in &flat[index].edges {
            if closed.contains(&edge.to) {
                continue;
            }
            let tentative_g = g + u32::from(edge.cost);
            let known = g_score.get(&edge.to).copied().unwrap_or(u32::MAX);
            if tentative_g < known {
                g_score.insert(edge.to, tentative_g);
                parent.insert(edge.to, (index, edge.kind));
                let f = tentative_g + heuristic(edge.to);
                open.push((Reverse(f), Reverse(tentative_g), Reverse(edge.to)));
            }
        }
    }

    PathResult::Unreachable
}

/// A* over a [`NavGraph`] (design §3.5, I-5): integer costs, an admissible
/// diagonal-aware heuristic, tie-break for a total order, **0 RNG**. Four
/// honest outcomes: `Found` / `Partial` / `Unreachable` / `NoGraph`.
pub fn find_path(graph: &NavGraph, query: &PathQuery) -> PathResult {
    let budget = budget_of(query);

    // Missing block ⇒ NoGraph (honest, never a fake path).
    if !graph.blocks.contains_key(&query.from.block) {
        return PathResult::NoGraph { block: query.from.block };
    }
    if !graph.blocks.contains_key(&query.to.block) {
        return PathResult::NoGraph { block: query.to.block };
    }

    let (flat, _) = flatten(graph, |_| true);
    let (Some(start), Some(goal_index)) =
        (resolve_point(&flat, &query.from), resolve_point(&flat, &query.to))
    else {
        // A point with no node at that cell+level is unreachable (not a fake).
        return PathResult::Unreachable;
    };
    run_astar(&flat, start, goal_index, budget)
}

/// Query through a [`NavGraphSource`] (design §4, D-M37-8): the path never
/// touches a block outside the window; a destination outside it yields
/// `Partial` with the prefix up to the window edge (0 navigation toward
/// unloaded geometry).
pub fn find_path_source(source: &NavGraphSource<'_>, query: &PathQuery) -> PathResult {
    let budget = budget_of(query);
    let graph = source.graph();
    if !source.is_block_loaded(query.from.block) {
        return PathResult::NoGraph { block: query.from.block };
    }
    let (flat, key_to_index) = flatten(graph, |block| source.is_block_loaded(block));
    let Some(start) = resolve_point(&flat, &query.from) else {
        return PathResult::Unreachable;
    };
    if source.is_block_loaded(query.to.block) {
        let Some(goal_index) = resolve_point(&flat, &query.to) else {
            return PathResult::Unreachable;
        };
        return run_astar(&flat, start, goal_index, budget);
    }
    // Proxy goal = the window edge that faces the destination block: the
    // loaded endpoint of a seam/door whose other side is `to.block`,
    // nearest (in lattice cells) to the destination. Fallback: the loaded
    // node nearest the destination. Deterministic: min by (h, flat index).
    let mut proxy: Option<(u32, usize)> = None;
    let mut consider = |key: (u32, usize)| {
        if proxy.is_none_or(|best| key < best) {
            proxy = Some(key);
        }
    };
    let mut edge_facing = 0usize;
    for seam in &graph.seams {
        let loaded_end = if seam.b.0 == query.to.block && source.is_block_loaded(seam.a.0) {
            Some(seam.a)
        } else if seam.a.0 == query.to.block && source.is_block_loaded(seam.b.0) {
            Some(seam.b)
        } else {
            None
        };
        let Some(&index) = loaded_end.and_then(|end| key_to_index.get(&end)) else {
            continue;
        };
        edge_facing += 1;
        let h = h_cost(flat[index].cell, flat[index].level, query.to.cell, query.to.level);
        consider((h, index));
    }
    if edge_facing == 0 {
        for (i, node) in flat.iter().enumerate() {
            let h = h_cost(node.cell, node.level, query.to.cell, query.to.level);
            consider((h, i));
        }
    }
    let Some((_, proxy_index)) = proxy else {
        return PathResult::NoGraph { block: query.to.block };
    };
    match run_astar(&flat, start, proxy_index, budget) {
        // Reached the window edge, not the goal: honest Partial (D-M37-8).
        PathResult::Found(steps) => PathResult::Partial(steps),
        other => other,
    }
}

/// Resolves a [`NavPoint`] to a flat index by exact `(cell, level)` match.
fn resolve_point(flat: &[FlatNode], point: &NavPoint) -> Option<usize> {
    // Scan the flat nodes for the matching block+cell+level (deterministic).
    flat
        .iter()
        .position(|node| node.block == point.block && node.cell == point.cell && node.level == point.level)
}

/// Why navigation fell back to locomotion (design §3.6; 0 fake graphs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackReason {
    /// Block absent or unparsed: straight-line `LocomotionWorld` toward the goal.
    NoGraph,
    /// Two regions with no portal: retry with backoff, then escalate to `NoGraph`.
    Unreachable,
    /// The follower could not make progress for [`BLOCKED_TICKS_LIMIT`] ticks.
    Blocked,
}

/// Observable fallback event `AiPathFallback { block, reason }` (design §3.6):
/// the consumer emits it (ECS event / probe line); `behavior` only computes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiPathFallback {
    /// Block the agent is trying to navigate.
    pub block: BlockId,
    /// Why navigation fell back.
    pub reason: FallbackReason,
}

impl AiPathFallback {
    /// Fallback for a failed [`PathResult`]; `None` while the path works
    /// (`Found`/`Partial`).
    pub fn from_path_result(result: &PathResult, from: NavPoint) -> Option<Self> {
        match result {
            PathResult::NoGraph { block } => Some(Self {
                block: *block,
                reason: FallbackReason::NoGraph,
            }),
            PathResult::Unreachable => Some(Self {
                block: from.block,
                reason: FallbackReason::Unreachable,
            }),
            PathResult::Found(_) | PathResult::Partial(_) => None,
        }
    }
}

/// Follow speed of [`PathFollower`], meters/second `[INFERENCE]` (the design
/// §3.7 sketch fixes none; 2.0 m/s ≈ a Daggerfall NPC walk).
pub const FOLLOW_SPEED_MPS: f32 = 2.0;
/// Ticks without progress before [`PathFollower::fallback`] fires
/// `[INFERENCE]` (1 s at 60 fps; analogue of `SEEK_COUNT_RECYCLE_ABOVE`,
/// `npc_ai.rs:57`).
pub const BLOCKED_TICKS_LIMIT: u32 = 60;

/// Pure path consumer (design §3.7): steers toward the current step's
/// waypoint each tick. **0 RNG, 0 Bevy, 0 I/O** (I-2). The *caller* owns
/// collision — the app clamps every commanded step with
/// `CollisionGrid::resolve_step` — the follower only reports honest
/// progress and the [`AiPathFallback`] when stuck.
#[derive(Debug, Clone)]
pub struct PathFollower {
    /// World waypoint + block per step (resolved from the graph at `new`).
    targets: Vec<([f32; 3], BlockId)>,
    index: usize,
    speed_mps: f32,
    blocked_ticks: u32,
    last_distance_m: f32,
    partial: bool,
}

impl PathFollower {
    /// Builds a follower for a `Found`/`Partial` path; `None` for
    /// `Unreachable`/`NoGraph` (the consumer emits the fallback instead —
    /// 0 followers on fake paths).
    pub fn new(path: &PathResult, graph: &NavGraph, speed_mps: f32) -> Option<Self> {
        let (steps, partial) = match path {
            PathResult::Found(steps) => (steps, false),
            PathResult::Partial(steps) => (steps, true),
            PathResult::Unreachable | PathResult::NoGraph { .. } => return None,
        };
        let mut targets = Vec::with_capacity(steps.len());
        for step in steps {
            let block = graph.blocks.get(&step.point.block)?;
            let world = node_world_m(block, &step.point)?;
            targets.push((world, step.point.block));
        }
        Some(Self {
            targets,
            index: 0,
            speed_mps,
            blocked_ticks: 0,
            last_distance_m: f32::INFINITY,
            partial,
        })
    }

    /// Current waypoint in world meters (`None` once finished).
    pub fn target(&self) -> Option<[f32; 3]> {
        self.targets.get(self.index).map(|(world, _)| *world)
    }

    /// Every waypoint has been reached.
    pub fn is_finished(&self) -> bool {
        self.index >= self.targets.len()
    }

    /// The path was a budget prefix (`Partial`), not the full route.
    pub fn is_partial(&self) -> bool {
        self.partial
    }

    /// One tick toward the current waypoint: the commanded position after
    /// `dt` at the follower's speed. The caller feeds the *actual* position
    /// back next tick; consecutive non-decreases of the waypoint distance
    /// count toward [`BLOCKED_TICKS_LIMIT`] (deterministic, 0 RNG).
    pub fn advance(&mut self, pos: [f32; 3], dt: f32) -> [f32; 3] {
        let Some(&(target, _)) = self.targets.get(self.index) else {
            return pos; // finished: hold position
        };
        let delta = [
            target[0] - pos[0],
            target[1] - pos[1],
            target[2] - pos[2],
        ];
        let distance = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
        let step = self.speed_mps * dt;
        if distance <= step {
            self.index += 1;
            self.blocked_ticks = 0;
            self.last_distance_m = f32::INFINITY;
            return target;
        }
        if distance >= self.last_distance_m {
            self.blocked_ticks += 1;
        } else {
            self.blocked_ticks = 0;
        }
        self.last_distance_m = distance;
        let scale = step / distance;
        [
            pos[0] + delta[0] * scale,
            pos[1] + delta[1] * scale,
            pos[2] + delta[2] * scale,
        ]
    }

    /// `Some` once the agent has been stuck for [`BLOCKED_TICKS_LIMIT`]
    /// ticks (the waypoint's block + [`FallbackReason::Blocked`]).
    pub fn fallback(&self) -> Option<AiPathFallback> {
        (self.blocked_ticks >= BLOCKED_TICKS_LIMIT).then(|| {
            let block = self.targets.get(self.index).map(|(_, block)| *block).unwrap_or(0);
            AiPathFallback { block, reason: FallbackReason::Blocked }
        })
    }
}

/// World position of a [`NavPoint`] (block origin + lattice, N3 anchor);
/// `None` when the block graph has no node at that cell+level.
pub fn node_world_m(block: &NavBlockGraph, point: &NavPoint) -> Option<[f32; 3]> {
    let node = block
        .nodes
        .iter()
        .find(|n| n.cell == point.cell && n.level == point.level)?;
    Some([
        block.cell_origin_m[0] + (f32::from(node.cell[0]) + 0.5) * CELL_METERS,
        block.cell_origin_m[1] + node.y_m,
        block.cell_origin_m[2] + (f32::from(node.cell[1]) + 0.5) * CELL_METERS,
    ])
}

/// Walks `parent` back from `to` to `start` and emits [`NavStep`]s.
fn reconstruct(
    flat: &[FlatNode],
    parent: &BTreeMap<usize, (usize, EdgeKind)>,
    start: usize,
    to: usize,
) -> Vec<NavStep> {
    let mut chain: Vec<usize> = Vec::new();
    let mut cursor = to;
    chain.push(cursor);
    while cursor != start {
        match parent.get(&cursor) {
            Some((prev, _)) => {
                cursor = *prev;
                chain.push(cursor);
            }
            None => break,
        }
    }
    chain.reverse();
    chain.iter()
        .map(|index| {
            let node = &flat[*index];
            let via = if *index == start {
                EdgeKind::Walk
            } else {
                parent.get(index).map(|(_, kind)| *kind).unwrap_or(EdgeKind::Walk)
            };
            NavStep {
                point: NavPoint { block: node.block, cell: node.cell, level: node.level },
                via,
            }
        })
        .collect()
}

/// FNV-1a 64 over the canonical encoding of a path (deterministic, no RNG).
pub fn path_digest(steps: &[NavStep]) -> u64 {
    let mut out = String::new();
    for step in steps {
        out.push_str(&format!(
            "{},{},{} {}\n",
            step.point.block, step.point.cell[0], step.point.cell[1], edge_kind_tag(step.via),
        ));
    }
    fnv1a64(out.as_bytes())
}

/// Convenience: total integer cost of a path (0 for empty).
pub fn path_cost(steps: &[NavStep], graph: &NavGraph) -> u32 {
    steps
        .windows(2)
        .map(|pair| edge_cost_between(graph, &pair[0], &pair[1]))
        .sum()
}

/// Cost of the single edge between two consecutive steps (0 if not adjacent).
fn edge_cost_between(graph: &NavGraph, from: &NavStep, to: &NavStep) -> u32 {
    let Some(block_graph) = graph.blocks.get(&from.point.block) else {
        return 0;
    };
    let Some(from_node) = block_graph
        .nodes
        .iter()
        .find(|n| n.cell == from.point.cell && n.level == from.point.level)
    else {
        return 0;
    };
    let Some(to_node) = block_graph
        .nodes
        .iter()
        .find(|n| n.cell == to.point.cell && n.level == to.point.level)
    else {
        return 0;
    };
    block_graph
        .edges
        .get(from_node.id as usize)
        .and_then(|adjacency| {
            adjacency
                .iter()
                .find(|edge| edge.to == to_node.id)
                .map(|edge| u32::from(edge.cost))
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one block graph from explicit nodes/edges and fills stats+digest.
    fn block(
        block_id: BlockId,
        nodes: Vec<NavNode>,
        edges: Vec<Vec<NavEdge>>,
        portals: Vec<DoorPortal>,
    ) -> NavBlockGraph {
        let mut levels = std::collections::BTreeSet::new();
        for n in &nodes {
            levels.insert(n.level);
        }
        let edge_count: usize = edges.iter().map(Vec::len).sum();
        let stats = NavStats {
            nodes: nodes.len(),
            edges: edge_count,
            portals: portals.len(),
            levels: levels.len(),
            unreachable_regions: 0,
            seams_rejected: 0,
        };
        let mut g = NavBlockGraph {
            block: block_id,
            cell_origin_m: [0.0; 3],
            nodes,
            edges,
            portals,
            partial: false,
            stats,
            digest: 0,
        };
        g.digest = g.digest();
        g
    }

    fn node(id: NavNodeId, cell: [i16; 2], level: i16) -> NavNode {
        NavNode {
            id,
            level,
            cell,
            y_m: f32::from(level) * LEVEL_STEP_M,
            walkable_radius_m: NPC_RADIUS_M,
            flags: 0,
        }
    }

    fn walk(to: NavNodeId) -> NavEdge {
        NavEdge { to, cost: WALK_COST, kind: EdgeKind::Walk }
    }

    fn ramp(to: NavNodeId) -> NavEdge {
        NavEdge { to, cost: RAMP_COST, kind: EdgeKind::Ramp }
    }

    /// Level 0 nodes (0,0)-(2,0) in a row; a ramp from (2,0) up to level 1
    /// node (2,0)@L1; level 1 continues to (3,0)@L1. Total 5 nodes.
    fn ramp_graph() -> NavGraph {
        let nodes = vec![
            node(0, [0, 0], 0),
            node(1, [1, 0], 0),
            node(2, [2, 0], 0),
            node(3, [2, 0], 1),
            node(4, [3, 0], 1),
        ];
        let edges = vec![
            vec![walk(1)],
            vec![walk(0), walk(2)],
            vec![walk(1), ramp(3)],
            vec![ramp(2), walk(4)],
            vec![walk(3)],
        ];
        let mut blocks = BTreeMap::new();
        blocks.insert(0u16, block(0, nodes, edges, Vec::new()));
        NavGraph { dungeon_location_id: 1, blocks, seams: Vec::new(), digest: 0 }
    }

    fn query(from: NavPoint, to: NavPoint, budget: u32) -> PathQuery {
        PathQuery { from, to, max_expansions: budget }
    }

    #[test]
    fn two_level_ramp_path_is_found_and_digest_is_stable() {
        let graph = ramp_graph();
        let from = NavPoint { block: 0, cell: [0, 0], level: 0 };
        let to = NavPoint { block: 0, cell: [3, 0], level: 1 };
        let result = find_path(&graph, &query(from, to, MAX_EXPANSIONS_DEFAULT));
        let PathResult::Found(steps) = result else {
            panic!("expected Found, got {result:?}");
        };
        // 5 nodes → 4 steps (start + 4 moves? no: start + 3 intermediate = path visits
        // nodes 0,1,2,3,4 → 5 steps; the ramp edge is used at least once).
        assert_eq!(steps.len(), 5);
        assert_eq!(steps[0].point, from);
        assert_eq!(steps[4].point, to);
        assert!(steps.iter().any(|s| matches!(s.via, EdgeKind::Ramp)));
        // path_digest ×2 identical (determinism, I-5).
        assert_eq!(path_digest(&steps), path_digest(&steps));
        // Total cost: walk(0→1)=10 + walk(1→2)=10 + ramp(2→3)=12 + walk(3→4)=10 = 42.
        assert_eq!(path_cost(&steps, &graph), 42);
    }

    #[test]
    fn disconnected_regions_are_unreachable() {
        let nodes = vec![node(0, [0, 0], 0), node(1, [5, 5], 0)];
        let edges = vec![vec![], vec![]];
        let mut blocks = BTreeMap::new();
        blocks.insert(0u16, block(0, nodes, edges, Vec::new()));
        let graph = NavGraph { dungeon_location_id: 2, blocks, seams: Vec::new(), digest: 0 };
        let from = NavPoint { block: 0, cell: [0, 0], level: 0 };
        let to = NavPoint { block: 0, cell: [5, 5], level: 0 };
        assert_eq!(find_path(&graph, &query(from, to, MAX_EXPANSIONS_DEFAULT)), PathResult::Unreachable);
    }

    #[test]
    fn missing_block_reports_no_graph() {
        let graph = ramp_graph();
        let from = NavPoint { block: 0, cell: [0, 0], level: 0 };
        let to = NavPoint { block: 9, cell: [0, 0], level: 0 };
        assert_eq!(
            find_path(&graph, &query(from, to, MAX_EXPANSIONS_DEFAULT)),
            PathResult::NoGraph { block: 9 }
        );
    }

    #[test]
    fn tiny_budget_returns_partial_prefix() {
        let graph = ramp_graph();
        let from = NavPoint { block: 0, cell: [0, 0], level: 0 };
        let to = NavPoint { block: 0, cell: [3, 0], level: 1 };
        // Budget 1: expands only the start, then yields a valid prefix.
        match find_path(&graph, &query(from, to, 1)) {
            PathResult::Partial(prefix) => {
                assert!(!prefix.is_empty());
                assert_eq!(prefix[0].point, from);
                // Prefix is shorter than the full 5-step path.
                assert!(prefix.len() < 5);
            }
            other => panic!("expected Partial, got {other:?}"),
        }
    }

    #[test]
    fn graph_digest_is_stable_across_builds() {
        assert_eq!(ramp_graph().digest(), ramp_graph().digest());
        // Building the same block twice yields the same block digest.
        let a = ramp_graph().blocks[&0].digest();
        let b = ramp_graph().blocks[&0].digest();
        assert_eq!(a, b);
    }

    /// Two single-node blocks joined by one seam with the given `via` kind.
    fn seam_graph(via: EdgeKind) -> NavGraph {
        let mut blocks = BTreeMap::new();
        blocks.insert(0u16, block(0, vec![node(0, [31, 0], 0)], vec![vec![]], Vec::new()));
        blocks.insert(1u16, block(1, vec![node(0, [0, 0], 0)], vec![vec![]], Vec::new()));
        let seams = vec![SeamEdge { a: (0, 0), b: (1, 0), cost: SEAM_COST, via }];
        NavGraph { dungeon_location_id: 3, blocks, seams, digest: 0 }
    }

    #[test]
    fn seam_crossing_step_is_tagged_seam() {
        let graph = seam_graph(EdgeKind::Seam { neighbor: 1 });
        let from = NavPoint { block: 0, cell: [31, 0], level: 0 };
        let to = NavPoint { block: 1, cell: [0, 0], level: 0 };
        let PathResult::Found(steps) = find_path(&graph, &query(from, to, MAX_EXPANSIONS_DEFAULT))
        else {
            panic!("expected Found across the seam");
        };
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[1].via, EdgeKind::Seam { neighbor: 1 });
    }

    #[test]
    fn door_seam_step_is_tagged_door() {
        // Design §3.4: when the only opening on the boundary is an action
        // door, the crossing step carries `Door { object_offset }`.
        let graph = seam_graph(EdgeKind::Door { object_offset: 77 });
        let from = NavPoint { block: 0, cell: [31, 0], level: 0 };
        let to = NavPoint { block: 1, cell: [0, 0], level: 0 };
        let PathResult::Found(steps) = find_path(&graph, &query(from, to, MAX_EXPANSIONS_DEFAULT))
        else {
            panic!("expected Found through the door seam");
        };
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[1].via, EdgeKind::Door { object_offset: 77 });
    }

    #[test]
    fn streamed_window_never_leaves_loaded_blocks() {
        // 3-block chain 0 — 1 — 2.
        let mut blocks = BTreeMap::new();
        for id in 0u16..3 {
            blocks.insert(id, block(id, vec![node(0, [0, 0], 0)], vec![vec![]], Vec::new()));
        }
        let seams = vec![
            SeamEdge { a: (0, 0), b: (1, 0), cost: SEAM_COST, via: EdgeKind::Seam { neighbor: 1 } },
            SeamEdge { a: (1, 0), b: (2, 0), cost: SEAM_COST, via: EdgeKind::Seam { neighbor: 2 } },
        ];
        let graph = NavGraph { dungeon_location_id: 4, blocks, seams, digest: 0 };
        let from = NavPoint { block: 0, cell: [0, 0], level: 0 };
        let to = NavPoint { block: 2, cell: [0, 0], level: 0 };
        let q = query(from, to, MAX_EXPANSIONS_DEFAULT);

        // Whole dungeon: Found across both seams.
        let whole = NavGraphSource::WholeDungeon(&graph);
        assert!(matches!(find_path_source(&whole, &q), PathResult::Found(_)));

        // Window {0, 1}: destination outside ⇒ Partial stopping at the edge.
        let loaded: std::collections::BTreeSet<BlockId> = [0u16, 1].into_iter().collect();
        let windowed = NavGraphSource::StreamedCells { graph: &graph, loaded: &loaded };
        match find_path_source(&windowed, &q) {
            PathResult::Partial(steps) => {
                assert_eq!(steps.last().expect("prefix").point.block, 1);
            }
            other => panic!("expected Partial at the window edge, got {other:?}"),
        }

        // Full window: the same query reaches block 2.
        let all: std::collections::BTreeSet<BlockId> = [0u16, 1, 2].into_iter().collect();
        let full = NavGraphSource::StreamedCells { graph: &graph, loaded: &all };
        assert!(matches!(find_path_source(&full, &q), PathResult::Found(_)));

        // Start outside the window ⇒ NoGraph (honest, 0 fake steps).
        let only_tail: std::collections::BTreeSet<BlockId> = [1u16, 2].into_iter().collect();
        let tail = NavGraphSource::StreamedCells { graph: &graph, loaded: &only_tail };
        assert_eq!(find_path_source(&tail, &q), PathResult::NoGraph { block: 0 });
    }

    #[test]
    fn whole_dungeon_source_matches_find_path() {
        let graph = ramp_graph();
        let source = NavGraphSource::WholeDungeon(&graph);
        let q = query(
            NavPoint { block: 0, cell: [0, 0], level: 0 },
            NavPoint { block: 0, cell: [3, 0], level: 1 },
            MAX_EXPANSIONS_DEFAULT,
        );
        assert_eq!(find_path(&graph, &q), find_path_source(&source, &q));
    }

    #[test]
    fn follower_walks_the_ramp_path_to_the_end() {
        let graph = ramp_graph();
        let q = query(
            NavPoint { block: 0, cell: [0, 0], level: 0 },
            NavPoint { block: 0, cell: [3, 0], level: 1 },
            MAX_EXPANSIONS_DEFAULT,
        );
        let result = find_path(&graph, &q);
        let digest_before = path_digest(match &result {
            PathResult::Found(steps) | PathResult::Partial(steps) => steps,
            other => panic!("expected a path, got {other:?}"),
        });
        let mut follower =
            PathFollower::new(&result, &graph, FOLLOW_SPEED_MPS).expect("Found builds a follower");
        assert!(!follower.is_partial());

        // 10 simulated seconds at 60 fps; the caller feeds back the exact
        // commanded position (no collision here — that's the app's clamp).
        let dt = 1.0 / 60.0;
        let mut pos = [0.8, 0.0, 0.0];
        let mut ticks = 0u32;
        while !follower.is_finished() && ticks < 600 {
            pos = follower.advance(pos, dt);
            ticks += 1;
        }
        assert!(follower.is_finished(), "the follower must reach the last waypoint");
        assert!(follower.fallback().is_none(), "no fallback on a clean walk");
        let target = &graph.blocks[&0];
        let end = node_world_m(
            target,
            &NavPoint { block: 0, cell: [3, 0], level: 1 },
        )
        .expect("goal node exists");
        assert_eq!(pos, end);
        // The path itself never changed while following (0 RNG, I-5).
        assert_eq!(
            path_digest(match &result {
                PathResult::Found(steps) | PathResult::Partial(steps) => steps,
                _ => unreachable!(),
            }),
            digest_before
        );
    }

    #[test]
    fn blocked_follower_reports_fallback_after_the_limit() {
        let graph = ramp_graph();
        let q = query(
            NavPoint { block: 0, cell: [0, 0], level: 0 },
            NavPoint { block: 0, cell: [3, 0], level: 1 },
            MAX_EXPANSIONS_DEFAULT,
        );
        let result = find_path(&graph, &q);
        let mut follower =
            PathFollower::new(&result, &graph, FOLLOW_SPEED_MPS).expect("follower");
        // The agent never moves (a wall the clamp refuses): the commanded
        // position is computed but the actual position stays put.
        let pos = [0.8, 0.0, 0.0];
        let mut ticks = 0u32;
        while follower.fallback().is_none() && ticks < BLOCKED_TICKS_LIMIT * 4 {
            let _commanded = follower.advance(pos, 1.0 / 60.0);
            ticks += 1;
        }
        let fallback = follower.fallback().expect("stuck follower must report");
        assert_eq!(fallback.reason, FallbackReason::Blocked);
        assert_eq!(fallback.block, 0);
    }

    #[test]
    fn failed_paths_produce_fallback_not_followers() {
        let graph = ramp_graph();
        let from = NavPoint { block: 0, cell: [0, 0], level: 0 };
        let missing = NavPoint { block: 9, cell: [0, 0], level: 0 };
        let no_graph = find_path(&graph, &query(from, missing, MAX_EXPANSIONS_DEFAULT));
        assert!(PathFollower::new(&no_graph, &graph, FOLLOW_SPEED_MPS).is_none());
        assert_eq!(
            AiPathFallback::from_path_result(&no_graph, from),
            Some(AiPathFallback { block: 9, reason: FallbackReason::NoGraph })
        );
        // Disconnected region inside a present block ⇒ Unreachable fallback.
        let mut blocks = BTreeMap::new();
        blocks.insert(
            0u16,
            block(0, vec![node(0, [0, 0], 0), node(1, [9, 9], 0)], vec![vec![], vec![]], Vec::new()),
        );
        let graph = NavGraph { dungeon_location_id: 9, blocks, seams: Vec::new(), digest: 0 };
        let to = NavPoint { block: 0, cell: [9, 9], level: 0 };
        let unreachable = find_path(&graph, &query(from, to, MAX_EXPANSIONS_DEFAULT));
        assert!(PathFollower::new(&unreachable, &graph, FOLLOW_SPEED_MPS).is_none());
        assert_eq!(
            AiPathFallback::from_path_result(&unreachable, from),
            Some(AiPathFallback { block: 0, reason: FallbackReason::Unreachable })
        );
    }
}

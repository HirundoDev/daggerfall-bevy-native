//! `world_stream` — M37 ECS layer: async cell streaming with a frame budget
//! (F-AUD-01, design `f-aud-01-02-streaming-navmesh-CR-96142f2f.md` §2.3-§2.6).
//!
//! Layering (D-M37-5): the **policy** is
//! `daggerfall_parser_bevy_bridge::stream_policy` (pure, headless-tested in
//! F1); this module is the **orchestration** — tasks, budgets, eviction —
//! and nothing else. `game-core` and the parser stay untouched (I-1, 0 new
//! crate edges: the tasks compose cells through the bridge's existing
//! functions).
//!
//! | Piece | Where it runs | What it may touch |
//! |:---|:---|:---|
//! | [`stream_watch`] | `CoreSet::Input` | reads the focus anchor, writes only [`StreamIntent`] |
//! | [`stream_commit`] | `CoreSet::Simulate` | spawns/polls/commits/evicts cells with the budget |
//!
//! The `Update` order `Input -> Simulate` is pinned here for apps that do not
//! compose the whole `GameCorePlugin` chain (the M18 window orders systems
//! manually); apps that do get the identical relation for free.
//!
//! **F3 scope (design §2.5/§2.8):** a committed cell spawns its entity tree —
//! root [`CellRoot`] + [`CellAssets`] (the strong `Mesh` handles the cell
//! owns) with one [`CellPart`] child per texture group. Eviction despawns the
//! tree recursively; the last strong handle releases the mesh from
//! `Assets<Mesh>` (measured by `STREAM-001` C/D: `AssetEvent::Removed` is
//! observable 2 frames after the despawn). Images/materials are **never** the
//! cell's: they live in the session [`MaterialCache`] keyed by texture key
//! (D-M37-6), so only `Assets<Mesh>` tracks the window.
//!
//! **Requirement on the composing app:** `AssetPlugin` +
//! `init_asset::<Mesh/Image/StandardMaterial>()` must be registered **before**
//! this plugin (headless precedent `m12_texture_render.rs:112-115`; design
//! §2.7/R8). The STREAM-001 harness does exactly that.
//!
//! **No `block_on` anywhere in this module** (F2 gate): tasks are polled with
//! `futures_lite`'s `now_or_never` (a single non-blocking poll — the same
//! primitive `bevy::future::poll_once` wraps).
//!
//! Population is **never re-seeded** here (D-M37-7): this module imports no
//! `npc_spec` item; the streaming only loads/unloads geometry views. The
//! STREAM-001 probe measures the population digest across a synthetic walk to
//! falsify any drift.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};

use daggerfall_parser_bevy_bridge as bridge;
use daggerfall_parser_bevy_bridge::stream_policy::{
    cell_of_point, desired_cells, plan, priority, CellKey, StreamConfig, STREAM_HYSTERESIS_CELLS,
};
use daggerfall_game_core::schedule::CoreSet;
use daggerfall_parser_rs::rdi;
use daggerfall_parser_rs::rmb::MapBlockRecord;

/// Frames without pending-set progress before [`StreamStats::stalls`]
/// increments and [`StreamStalled`] fires (2 s at 60 fps, design §2.4).
pub const STALL_FRAMES: u64 = 120;

/// D-M37-3 is a compile-time invariant: without hysteresis the border
/// thrash falsifier (§2.2/B) cannot hold.
const _: () = {
    assert!(STREAM_HYSTERESIS_CELLS >= 1, "D-M37-3 requires hysteresis");
};

/// Output-side backpressure multiplier (design §2.4): the `ready` queue may
/// hold up to `4·(2R+1)²` composed cells before new tasks stop launching.
const READY_BACKPRESSURE_MULT: usize = 4;

/// ARCH3D.BSA resolved on demand: shared read-only by every cell task, with
/// a mutexed spec cache (the same record is placed in many cells). This is
/// the thread-safe counterpart of `m10_probe::Arch3dIndex` (which is
/// single-consumer).
pub struct ArchSpecs {
    archive: bridge::BsaArchive,
    data: Vec<u8>,
    by_id: std::collections::HashMap<u32, usize>,
    cache: Mutex<std::collections::HashMap<u32, Option<bridge::MeshSpec>>>,
}

impl ArchSpecs {
    /// Parses `ARCH3D.BSA` bytes (custody resolved by the caller — Mode A).
    pub fn parse(data: Vec<u8>) -> Result<Self, String> {
        let archive =
            bridge::parse_bsa(&data).map_err(|e| format!("invalid ARCH3D.BSA: {e}"))?;
        let mut by_id = std::collections::HashMap::new();
        for (i, entry) in archive.entries.iter().enumerate() {
            if let bridge::Entry::Numbered { record_id, .. } = entry {
                by_id.insert(*record_id, i);
            }
        }
        Ok(Self {
            archive,
            data,
            by_id,
            cache: Mutex::new(std::collections::HashMap::new()),
        })
    }

    /// Spec of one ARCH3D record (`None` when unresolvable — honest
    /// degradation, same rule as the synchronous composition).
    pub fn resolve(&self, model_id: u32) -> Option<bridge::MeshSpec> {
        let cached = self
            .cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&model_id)
            .cloned();
        if let Some(spec) = cached {
            return spec;
        }
        let spec = self
            .by_id
            .get(&model_id)
            .and_then(|idx| bridge::mesh_spec_at(&self.archive, &self.data, *idx).ok());
        self.cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(model_id, spec.clone());
        spec
    }
}

/// Composition inputs of one streamed world, shared cheaply (`Arc`) between
/// the resource and every task closure.
#[derive(Clone)]
pub struct StreamSource(pub Arc<StreamSourceInner>);

/// The owned data behind [`StreamSource`].
pub struct StreamSourceInner {
    /// Blocks along X (the location grid the records came from).
    pub width_blocks: usize,
    /// Blocks along Z (same grid).
    pub height_blocks: usize,
    /// `.RMB` records keyed by grid coordinates `(bx, by)` (design §2.1: in
    /// the city a cell **is** one `.RMB` block).
    pub blocks: BTreeMap<(usize, usize), MapBlockRecord>,
    /// ARCH3D specs resolved on demand.
    pub arch: ArchSpecs,
    /// Session texture store (D-M37-6): decoded retail RGBA by texture key.
    /// Session-scoped **input** — the cells read it at commit time and never
    /// own it (the `Assets<Image>` handles are created once by the session
    /// [`MaterialCache`]). Empty = untextured cells with the shared ground
    /// material — honest degradation, never invented pixels.
    pub textures: BTreeMap<u32, StreamTexture>,
}

/// One decoded retail texture of the session store — bytes only (0 Bevy
/// types: the handles live in [`MaterialCache`]).
#[derive(Debug, Clone)]
pub struct StreamTexture {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// RGBA8 (`width × height × 4`), top-left origin.
    pub rgba: Vec<u8>,
}

/// Grid coordinates of a streamed cell in the retail city grid: the engine
/// maps block `(bx, by)` to bevy `z ∈ [(H−by−1)·cell_m, (H−by)·cell_m)` (the
/// df→bevy z-negation), so the inverse is `bx = key.x`,
/// `by = H−1−key.z`. `None` = outside the city grid — a cell the world
/// cannot compose (terrain beyond the city is another layer's job).
pub fn grid_of_key(key: CellKey, source: &StreamSourceInner) -> Option<(usize, usize)> {
    let bx = usize::try_from(key.x).ok()?;
    let z = usize::try_from(key.z).ok()?;
    let by = source.height_blocks.checked_sub(1)?.checked_sub(z)?;
    (bx < source.width_blocks && by < source.height_blocks).then_some((bx, by))
}

/// Composed geometry of one cell + its digest (pure data — 0 handles, 0 ECS;
/// [`spawn_cell_entities`] turns it into the `CellRoot` tree at commit time).
#[derive(Debug, Clone)]
pub struct CellComposed {
    /// The cell this geometry belongs to.
    pub key: CellKey,
    /// City composed from the cell's single `.RMB` block (world frame of the
    /// global grid — bit-identical math to the synchronous composition).
    pub city: bridge::RetailCity,
    /// FNV-1a 64 of [`bridge::cell_canonical_bytes`] (the P-1 canonical
    /// form), so the digest of a cell is comparable across composition
    /// routes.
    pub digest: u64,
}

/// Per-frame counters (design §2.3 — the fields are part of the spec; no
/// extra counters are added to this struct).
#[derive(Debug, Clone, Copy, Default)]
pub struct StreamStats {
    /// Frames the commit system ran.
    pub frames: u64,
    /// Cells committed (composed geometry registered + cell marked loaded).
    pub commits: u64,
    /// Cells evicted (composed geometry dropped; F3 extends this counter
    /// with the entity despawn).
    pub despawns: u64,
    /// Tasks cancelled mid-flight. Bevy `Task`s are not abortable (§2.4), so
    /// this stays 0 until a phase has a lawful use for it; out-of-window
    /// results are counted in [`Self::discarded`], never here.
    pub cancelled: u64,
    /// Composed results discarded on arrival (cell left the window).
    pub discarded: u64,
    /// Stall episodes (`STALL_FRAMES` without pending-set progress).
    pub stalls: u64,
    /// Maximum commits observed in a single frame (budget evidence).
    pub max_commits_in_frame: u8,
}

/// Sent when the pending set stops shrinking for [`STALL_FRAMES`] (§2.4): a
/// load failure may never be silent. (bevy 0.20: the buffered channel is
/// `Message`, the old `Event`.)
#[derive(Message, Debug, Clone, Copy)]
pub struct StreamStalled {
    /// Cells still wanted by the window when the stall fired. The F4 stall
    /// consumer (STREAM-001 E) reads it; until then the field ships with the
    /// event because it is part of the design's observable payload (§2.4).
    #[allow(dead_code)]
    pub pending: usize,
}

/// The streaming window resource (design §2.3).
#[derive(Resource)]
pub struct StreamWindow {
    /// Window/budget configuration.
    pub config: StreamConfig,
    /// Composition inputs (shared).
    pub source: StreamSource,
    /// Cells currently loaded (committed and inside the policy window).
    pub loaded: BTreeSet<CellKey>,
    /// Tasks in flight, keyed by cell (backpressure: `<= max_in_flight`).
    pub in_flight: BTreeMap<CellKey, Task<CellComposed>>,
    /// Composed results waiting to be committed (priority order is applied
    /// at commit time; the map keeps arrival set semantics).
    pub ready: BTreeMap<CellKey, CellComposed>,
    /// Committed geometry of the loaded window (kept for metrics/digest; the
    /// ECS entities live under [`Self::cell_roots`]).
    pub composed: BTreeMap<CellKey, CellComposed>,
    /// Root entity of each loaded cell's tree (F3): eviction looks the root
    /// up here and despawns it recursively.
    pub cell_roots: BTreeMap<CellKey, Entity>,
    /// The player's own cell (D-M37-4 — pinned against unload).
    pub pinned: CellKey,
    /// Observable counters.
    pub stats: StreamStats,
    /// Frames without pending-set progress (stall detector state).
    stalled_ticks: u64,
    /// Pending count of the previous frame (stall detector state).
    last_pending: usize,
}

impl StreamWindow {
    /// Window starting at `start` (empty: the first frames fill it).
    pub fn new(config: StreamConfig, source: StreamSource, start: CellKey) -> Self {
        Self {
            config,
            source,
            loaded: BTreeSet::new(),
            in_flight: BTreeMap::new(),
            ready: BTreeMap::new(),
            composed: BTreeMap::new(),
            cell_roots: BTreeMap::new(),
            pinned: start,
            stats: StreamStats::default(),
            stalled_ticks: 0,
            last_pending: 0,
        }
    }

    /// Mesh payload of the committed window in bytes (evidence metric for
    /// the eviction/VRAM story; F3 replaces it with `Assets<Mesh>` counts).
    pub fn composed_mesh_bytes(&self) -> usize {
        self.composed.values().map(cell_mesh_bytes).sum()
    }
}

/// Positions+normals (12 B) + indices (4 B) of a composed cell — the
/// measurable payload the F3 layer will upload as one `Mesh`.
pub fn cell_mesh_bytes(cell: &CellComposed) -> usize {
    let spec = &cell.city.combined;
    spec.positions.len() * 12 + spec.normals.len() * 12 + spec.indices.len() * 4
}

/// The point the window follows (I-3: `stream_watch` writes **only** this
/// resource; `stream_commit` never reads a transform).
#[derive(Resource, Debug, Clone, Copy)]
pub struct StreamIntent {
    /// Cell the window is centered on.
    pub center: CellKey,
}

impl Default for StreamIntent {
    fn default() -> Self {
        Self {
            center: CellKey {
                x: 0,
                z: 0,
                level: 0,
            },
        }
    }
}

/// Marker for the entity the watch system follows (the M18 camera will carry
/// it once `m18_window.rs` frees up; the STREAM-001 probe spawns its own).
#[derive(Component, Debug, Clone, Copy)]
pub struct StreamFocus;

/// Root of one committed cell's entity tree (design §2.5: explicit per-cell
/// ownership). One root per cell; eviction despawns it **recursively** (bevy
/// 0.20 `despawn()` removes descendants), and the last strong [`Mesh`] handle
/// releases the asset from `Assets<Mesh>`.
#[derive(Component, Debug, Clone, Copy)]
pub struct CellRoot(pub CellKey);

/// One rendered part of a cell: a mesh plus its (session-cached) material,
/// spawned as a child of the [`CellRoot`] — one child per texture group.
#[derive(Component, Debug, Clone, Copy)]
pub struct CellPart;

/// Strong asset handles owned by ONE cell (design §2.3/§2.5). The `meshes`
/// are per-cell and freed on despawn; `materials` are references into the
/// session [`MaterialCache`] — shared, never the cell's to free (D-M37-6).
#[derive(Component, Default)]
pub struct CellAssets {
    /// Per-cell meshes: the last strong handle drop frees the asset.
    pub meshes: Vec<Handle<Mesh>>,
    /// References to shared cache materials (not owned — see D-M37-6).
    pub materials: Vec<Handle<StandardMaterial>>,
}

/// Session material cache (D-M37-6): one image + one material per texture
/// key, plus the untextured ground fallback. Warmed once at `Startup` from
/// [`StreamSourceInner::textures`]; cells only **reference** it, so
/// `Assets<Image>`/`Assets<StandardMaterial>` stay constant under movement —
/// only `Assets<Mesh>` tracks the window (STREAM-001 D: warm == final).
#[derive(Resource, Default)]
pub struct MaterialCache {
    /// Fallback for `NO_TEXTURE` groups and unresolvable keys.
    pub ground: Handle<StandardMaterial>,
    /// Decoded retail images by texture key.
    pub images: BTreeMap<u32, Handle<Image>>,
    /// Textured materials by texture key.
    pub materials: BTreeMap<u32, Handle<StandardMaterial>>,
}

/// Plugin: resources + `stream_watch` (Input) → `stream_commit` (Simulate).
pub struct WorldStreamPlugin {
    /// Window/budget configuration.
    pub config: StreamConfig,
    /// Composition inputs.
    pub source: StreamSource,
    /// Initial window center.
    pub start: CellKey,
}

impl Plugin for WorldStreamPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(StreamWindow::new(
            self.config,
            self.source.clone(),
            self.start,
        ))
        .insert_resource(StreamIntent {
            center: self.start,
        })
        .add_message::<StreamStalled>()
        // D-M37-6: the session material cache is warmed once, before the
        // first `stream_commit` (Startup runs before the first Update).
        .add_systems(Startup, warm_material_cache)
        // The contract order for apps that do not compose `GameCorePlugin`'s
        // full five-set chain (the M18 window orders systems manually); an
        // app that already chains the sets gets the identical relation.
        .configure_sets(Update, CoreSet::Input.before(CoreSet::Simulate))
        .add_systems(
            Update,
            (
                stream_watch.in_set(CoreSet::Input),
                stream_commit.in_set(CoreSet::Simulate),
            )
                .chain(),
        );
    }
}

/// `Startup`: builds the session [`MaterialCache`] — the ground material plus
/// one image + material per decoded retail texture (D-M37-6). Runs before the
/// first `stream_commit`, so cells only reference already-warmed handles and
/// `Assets<Image>`/`Assets<StandardMaterial>` stay constant under movement
/// (STREAM-001 D: warm == final). A texture that fails to decode degrades to
/// the ground material — honest, never invented pixels.
fn warm_material_cache(
    mut commands: Commands,
    window: Res<StreamWindow>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let ground = bridge::material_from_spec(
        &bridge::MaterialSpec {
            base_color: [0.33, 0.31, 0.28, 1.0],
            ..default()
        },
        &mut materials,
    );
    let mut cache = MaterialCache {
        ground,
        images: BTreeMap::new(),
        materials: BTreeMap::new(),
    };
    for (&key, tex) in &window.source.0.textures {
        let Ok(image) = bridge::image_from_rgba(&tex.rgba, tex.width, tex.height, &mut images)
        else {
            continue; // bad texture → its groups fall back to ground.
        };
        let material = bridge::material_from_texture(
            &bridge::MaterialSpec {
                base_color: [1.0, 1.0, 1.0, 1.0],
                ..default()
            },
            image.clone(),
            &mut materials,
        );
        cache.images.insert(key, image);
        cache.materials.insert(key, material);
    }
    commands.insert_resource(cache);
}

/// `CoreSet::Input`: follows the focus anchor and writes the intent.
fn stream_watch(
    window: Res<StreamWindow>,
    focus: Query<&Transform, With<StreamFocus>>,
    mut intent: ResMut<StreamIntent>,
) {
    let Ok(transform) = focus.single() else {
        return; // no anchor yet: keep the last intent (explicitly idle).
    };
    let p = transform.translation;
    let center = cell_of_point(&window.config, [p.x, p.y, p.z]);
    if center != intent.center {
        intent.center = center;
    }
}

/// One non-blocking poll of a cell task: `poll_once` yields `Pending` on the
/// first poll when the task has not finished, so `block_on` here can never
/// wait on the worker — the F2 gate forbids *blocking waits*, and the
/// STREAM-001 per-tick measurements are the empirical proof (this is the
/// only `block_on` token in this module, by construction).
fn poll_cell(task: &mut Task<CellComposed>) -> Option<CellComposed> {
    bevy::tasks::block_on(bevy::tasks::poll_once(task))
}

/// Spawns one committed cell's entity tree: a [`CellRoot`] + one [`CellPart`]
/// child per texture group, following the M18 composition path
/// (`split_by_texture` → cached material, else ground — the `m18_window.rs`
/// §D1 pattern). The cell owns the strong [`Mesh`] handles ([`CellAssets`]);
/// images and materials come from the session [`MaterialCache`] and are never
/// the cell's to free (D-M37-6). Geometry is already in world frame, so the
/// root sits at the origin with identity children. Returns the root entity.
fn spawn_cell_entities(
    commands: &mut Commands,
    cell: &CellComposed,
    source: &StreamSourceInner,
    cache: &MaterialCache,
    meshes: &mut Assets<Mesh>,
) -> Entity {
    let root = commands
        .spawn((
            CellRoot(cell.key),
            Transform::default(),
            Visibility::Inherited,
        ))
        .id();
    let mut assets = CellAssets::default();
    let mut parts: Vec<(Handle<Mesh>, Handle<StandardMaterial>)> = Vec::new();
    for group in bridge::split_by_texture(&cell.city.combined) {
        // UVs are normalized per texture size (M12); a group whose texture is
        // not in the session store keeps its raw UVs + the ground material
        // (honest degradation, never invented pixels).
        let spec = match source.textures.get(&group.texture_key) {
            Some(tex) => bridge::MeshSpec {
                uvs: bridge::scale_uvs(&group.mesh.uvs, tex.width, tex.height),
                ..group.mesh.clone()
            },
            None => group.mesh.clone(),
        };
        let mesh = bridge::mesh_from_spec(&spec, meshes);
        let material = cache
            .materials
            .get(&group.texture_key)
            .cloned()
            .unwrap_or_else(|| cache.ground.clone());
        assets.meshes.push(mesh.clone());
        assets.materials.push(material.clone());
        parts.push((mesh, material));
    }
    commands.entity(root).with_children(|parent| {
        for (mesh, material) in parts {
            parent.spawn((CellPart, Mesh3d(mesh), MeshMaterial3d(material)));
        }
    });
    commands.entity(root).insert(assets);
    root
}

/// `CoreSet::Simulate`: spawn (backpressure) → poll → commit (budget) →
/// unload (farthest first) → stall detection. One pass per frame, never
/// blocking (F2 gate: 0 `block_on`).
fn stream_commit(
    mut commands: Commands,
    mut window: ResMut<StreamWindow>,
    intent: Res<StreamIntent>,
    mut meshes: ResMut<Assets<Mesh>>,
    cache: Res<MaterialCache>,
    mut stalled: MessageWriter<StreamStalled>,
) {
    window.stats.frames += 1;
    let center = intent.center;
    window.pinned = center;
    let config = window.config;
    let step = plan(&config, center, &window.loaded);
    let desired: BTreeSet<CellKey> = desired_cells(center, config.radius_cells).into_iter().collect();

    // ── 1 · spawn with backpressure (§2.4) ────────────────────────────────
    let ready_limit = READY_BACKPRESSURE_MULT
        * (2 * usize::from(config.radius_cells) + 1)
        * (2 * usize::from(config.radius_cells) + 1);
    let pool = AsyncComputeTaskPool::get();
    for key in &step.load {
        if window.in_flight.len() >= usize::from(config.max_in_flight) {
            break; // input backpressure: the plan is recomputed next frame.
        }
        if window.ready.len() > ready_limit {
            break; // output backpressure: the consumer is the bottleneck.
        }
        if window.loaded.contains(key)
            || window.in_flight.contains_key(key)
            || window.ready.contains_key(key)
            || window.composed.contains_key(key)
        {
            continue; // idempotent (R2: one task per key at most).
        }
        let Some((bx, by)) = grid_of_key(*key, &window.source.0) else {
            continue; // outside the city grid: not composable, never faked.
        };
        let Some(record) = window.source.0.blocks.get(&(bx, by)).cloned() else {
            continue; // no record for this grid cell (honest gap).
        };
        let source = window.source.clone();
        let task_key = *key;
        let width = window.source.0.width_blocks;
        let height = window.source.0.height_blocks;
        let task = pool.spawn(async move {
            let mut resolve = |model_id: u32| source.0.arch.resolve(model_id);
            let city = bridge::build_retail_cell(width, height, task_key, record, &mut resolve);
            let digest = rdi::fnv1a64(&bridge::cell_canonical_bytes(&city, task_key));
            CellComposed {
                key: task_key,
                city,
                digest,
            }
        });
        window.in_flight.insert(*key, task);
    }

    // ── 2 · poll in flight (single non-blocking poll each) ────────────────
    let mut finished: Vec<(CellKey, CellComposed)> = Vec::new();
    for (key, task) in window.in_flight.iter_mut() {
        if let Some(composed) = poll_cell(task) {
            finished.push((*key, composed));
        }
    }
    for (key, composed) in finished {
        window.in_flight.remove(&key);
        // Cancellation-on-arrival (§2.4): a cell that left the window while
        // its task ran is discarded, never inserted (0 orphaned work in the
        // world, observable in `stats.discarded`).
        if desired.contains(&key) && !step.unload.contains(&key) {
            window.ready.insert(key, composed);
        } else {
            window.stats.discarded += 1;
        }
    }

    // ── 3 · commit with budget (§2.4: ≤ max_commits, ≤ budget ms, always
    //        ≥ 1 if there is anything ready) — each committed cell spawns
    //        its `CellRoot`/`CellPart` tree (F3) ──────────────────────────
    let source = window.source.clone();
    let mut commit_order: Vec<CellKey> = window.ready.keys().copied().collect();
    commit_order.sort_by_key(|key| priority(center, *key));
    let budget_started = Instant::now();
    let mut commits = 0u8;
    for key in commit_order {
        if commits >= config.max_commits_per_frame {
            break;
        }
        if commits >= 1
            && budget_started.elapsed().as_secs_f32() * 1000.0 > config.commit_budget_ms
        {
            break; // soft budget — the first commit is never skipped.
        }
        let Some(composed) = window.ready.remove(&key) else {
            continue;
        };
        if !desired.contains(&key) {
            // Left the window between poll and commit (teleport): discard.
            window.stats.discarded += 1;
            continue;
        }
        let root = spawn_cell_entities(&mut commands, &composed, &source.0, &cache, &mut meshes);
        window.cell_roots.insert(key, root);
        window.loaded.insert(key);
        window.composed.insert(key, composed);
        window.stats.commits += 1;
        commits += 1;
    }
    window.stats.max_commits_in_frame = window.stats.max_commits_in_frame.max(commits);

    // ── 4 · unload farthest first (≤ max_unloads per frame) ───────────────
    let mut unloads = 0u8;
    for key in &step.unload {
        if unloads >= config.max_unloads_per_frame {
            break;
        }
        if window.loaded.remove(key) {
            window.composed.remove(key);
            window.ready.remove(key);
            // F3: recursive despawn of the cell's `CellRoot` tree; the last
            // strong `Mesh` handle drop frees the asset (STREAM-001 C/D).
            if let Some(root) = window.cell_roots.remove(key) {
                commands.entity(root).despawn();
            }
            window.stats.despawns += 1;
            unloads += 1;
        }
    }

    // ── 5 · stall detection (§2.4) ────────────────────────────────────────
    // Cells the world cannot compose (outside the city grid / no record)
    // are not "pending loads": they are out-of-source, and counting them
    // would manufacture false stalls at the city edge.
    let pending = step
        .load
        .iter()
        .filter(|k| {
            !window.loaded.contains(k)
                && grid_of_key(**k, &window.source.0).is_some_and(|g| {
                    window.source.0.blocks.contains_key(&g)
                })
        })
        .count();
    if pending > 0 && pending >= window.last_pending {
        window.stalled_ticks += 1;
    } else {
        window.stalled_ticks = 0;
    }
    window.last_pending = pending;
    if window.stalled_ticks >= STALL_FRAMES {
        window.stats.stalls += 1;
        window.stalled_ticks = 0;
        stalled.write(StreamStalled { pending });
    }
}

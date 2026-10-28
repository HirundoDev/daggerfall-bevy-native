//! `read_models` — PD4 T7 session read models (M7).
//!
//! **T7 "Read Models → Projections"** (REQ-L99-038/039/040, Addendum T0):
//! the three session projections of the required anchor —
//!
//! | Read model | Projection | Organ | REQ |
//! |---|---|---|---|
//! | [`HudReadModel`] (health/stamina/level) | from `RpgStats` (via [`HudSource`]) | skills→perception | REQ-L99-038 |
//! | [`AutomapReadModel`] (loaded chunks) | from `Chunks` (via [`AutomapSource`]) | memory→perception | REQ-L99-039 |
//! | [`SessionPanelReadModel`] (inventory + active quests) | from `AssetSet`/`Quest` (via [`PanelSource`]) | inventory/behavior→perception | REQ-L99-040 |
//!
//! **Composition contract (bus-only, bevy-bridge-matrix):** organs do not
//! call each other — `app` composes (M5 precedent: "`app` composes: DFU
//! formula + session"). That is why each projection receives a flat
//! **source DTO** (`*Source`) that the composer (ECS in `app`) fills from
//! its organs' types; this crate keeps **0 organ dependencies** (and
//! 0 Bevy). **Pure** projections: `project(source)` is a total function of
//! the session state — re-executable, deterministic, with no hidden state
//! (Anti-Screen: the only way to change a read model is to change its ECS
//! source).
//!
//! Rules of the 👁️ Perception organ: the Observer is **not** the owner of
//! domain decisions (guide §2/§6.2 FDC) — here it only projects what the
//! domain organs already decided.

use std::collections::BTreeSet;

// ─────────────────────────────────────────────────────────────────────────
// HUD (REQ-L99-038) — skills→perception
// ─────────────────────────────────────────────────────────────────────────

/// HUD source: the anchor triple (health/stamina/level) + maxima, copied by
/// the composer from `daggerfall_skills::stats::RpgStats`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HudSource {
    /// Current health.
    pub health: i32,
    /// Maximum health.
    pub max_health: i32,
    /// Current stamina.
    pub stamina: i32,
    /// Maximum stamina.
    pub max_stamina: i32,
    /// Level.
    pub level: i32,
}

/// HUD read model: health/stamina/level **projected** (values + integer
/// percentage bands + combat state).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HudReadModel {
    /// Projected current health.
    pub health: i32,
    /// Projected maximum health.
    pub max_health: i32,
    /// Projected current stamina.
    pub stamina: i32,
    /// Projected maximum stamina.
    pub max_stamina: i32,
    /// Projected level.
    pub level: i32,
    /// Health in integer % (0..=100, clamped — NFR NUMERIC-never-FLOAT).
    pub health_pct: i32,
    /// Stamina in integer % (0..=100, clamped).
    pub stamina_pct: i32,
    /// `true` while health is > 0 (0 = downed).
    pub conscious: bool,
}

fn pct(value: i32, max: i32) -> i32 {
    if max <= 0 {
        return 0;
    }
    ((value.max(0) * 100) / max).min(100)
}

impl HudReadModel {
    /// Pure HUD projection from its ECS source.
    pub fn project(source: &HudSource) -> Self {
        Self {
            health: source.health,
            max_health: source.max_health,
            stamina: source.stamina,
            max_stamina: source.max_stamina,
            level: source.level,
            health_pct: pct(source.health, source.max_health),
            stamina_pct: pct(source.stamina, source.max_stamina),
            conscious: source.health > 0,
        }
    }

    /// HUD summary line (title bar / probe output).
    pub fn summary(&self) -> String {
        format!(
            "HP {}/{} ({}%) · STA {}/{} ({}%) · Lv {}{}",
            self.health,
            self.max_health,
            self.health_pct,
            self.stamina,
            self.max_stamina,
            self.stamina_pct,
            self.level,
            if self.conscious { "" } else { " · DOWNED" },
        )
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Automap (REQ-L99-039) — memory→perception
// ─────────────────────────────────────────────────────────────────────────

/// Automap cell (chunk X/Z coordinate — copied by the composer from
/// `daggerfall_memory::chunks::ChunkCoord`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MapCell {
    /// Chunk column X.
    pub x: i32,
    /// Chunk row Z.
    pub z: i32,
}

/// Automap source: chunks loaded in the session.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AutomapSource {
    /// Loaded chunks (the composer copies them from the `ChunkMap`).
    pub cells: Vec<MapCell>,
}

/// Automap bounds (bbox of the loaded chunks).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapBounds {
    /// Minimum column.
    pub min_x: i32,
    /// Minimum row.
    pub min_z: i32,
    /// Maximum column.
    pub max_x: i32,
    /// Maximum row.
    pub max_z: i32,
}

impl MapBounds {
    /// Width in columns (inclusive).
    pub fn width(&self) -> i32 {
        self.max_x - self.min_x + 1
    }

    /// Height in rows (inclusive).
    pub fn height(&self) -> i32 {
        self.max_z - self.min_z + 1
    }
}

/// Maximum size renderable in ASCII (beyond it only the legend is emitted
/// — a giant grid is not legible evidence). GV-16.
pub const ASCII_MAX_SIDE: i32 = 24;

// GV-16 is a legibility budget, not a rendering preference: a map larger than
// this cannot be read in a terminal column, so it degrades to its legend.
const _: () = assert!(ASCII_MAX_SIDE == 24, "GV-16: ASCII_MAX_SIDE must stay 24");

/// Automap read model: loaded chunks **projected** (ordered cells
/// without duplicates + bbox).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AutomapReadModel {
    /// Loaded cells, deterministic order (x, then z), 0 duplicates.
    pub cells: Vec<MapCell>,
    /// Bounds (bbox) of the loaded cells (`None` = 0 chunks).
    pub bounds: Option<MapBounds>,
}

impl AutomapReadModel {
    /// Pure automap projection from its ECS source: sorts and dedups the
    /// cells (the map is reproducible byte for byte).
    pub fn project(source: &AutomapSource) -> Self {
        let unique: BTreeSet<MapCell> = source.cells.iter().copied().collect();
        let cells: Vec<MapCell> = unique.iter().copied().collect();
        let bounds = unique.iter().fold(None, |acc: Option<MapBounds>, cell| {
            Some(match acc {
                None => MapBounds {
                    min_x: cell.x,
                    min_z: cell.z,
                    max_x: cell.x,
                    max_z: cell.z,
                },
                Some(mut bounds) => {
                    bounds.min_x = bounds.min_x.min(cell.x);
                    bounds.min_z = bounds.min_z.min(cell.z);
                    bounds.max_x = bounds.max_x.max(cell.x);
                    bounds.max_z = bounds.max_z.max(cell.z);
                    bounds
                }
            })
        });
        Self { cells, bounds }
    }

    /// Is chunk `(x, z)` loaded in the projection?
    pub fn contains(&self, x: i32, z: i32) -> bool {
        self.cells.contains(&MapCell { x, z })
    }

    /// Number of projected chunks.
    pub fn chunk_count(&self) -> usize {
        self.cells.len()
    }

    /// ASCII automap grid (`#` = loaded chunk, `·` = empty; first row = highest
    /// Z, first column = lowest X). Above a bbox larger than
    /// `ASCII_MAX_SIDE` it returns only the legend (legibility).
    pub fn render_ascii(&self) -> String {
        let Some(bounds) = self.bounds else {
            return "(no chunks loaded)".to_string();
        };
        if bounds.width() > ASCII_MAX_SIDE || bounds.height() > ASCII_MAX_SIDE {
            return format!(
                "(bbox {}x{} cells — outside the ASCII grid, {} chunks loaded)",
                bounds.width(),
                bounds.height(),
                self.cells.len()
            );
        }
        let mut grid = vec![vec!['·'; bounds.width() as usize]; bounds.height() as usize];
        for cell in &self.cells {
            let row = (bounds.max_z - cell.z) as usize;
            let col = (cell.x - bounds.min_x) as usize;
            grid[row][col] = '#';
        }
        let mut out = String::new();
        for row in grid {
            out.extend(row);
            out.push('\n');
        }
        out.trim_end_matches('\n').to_string()
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Dungeon and city automap (M34/W34-5) — memory→perception
// ─────────────────────────────────────────────────────────────────────────

/// Placement depth of the exterior automap nameplates
/// (`DFU-UP:Assets/Scripts/Game/UserInterface/ExteriorAutomap.cs:59-76`,
/// `nameplatesPlacementDepth = 4.0`).
pub const NAMEPLATES_PLACEMENT_DEPTH: f32 = 4.0;

// Comparing floats in a const context needs the bit pattern (`PartialEq` is
// not `const` for `f32`), but the assertion is the same one the probe makes.
const _: () = assert!(
    NAMEPLATES_PLACEMENT_DEPTH.to_bits() == 4.0f32.to_bits(),
    "ExteriorAutomap: nameplatesPlacementDepth must stay 4.0"
);

/// Glyph rasterised in place of a hidden nameplate
/// (`ExteriorAutomap.cs:1365`, `nameplateReplaced`).
pub const HIDDEN_NAMEPLATE_GLYPH: char = '*';

/// State of one automap block (`DFU-UP:Assets/Scripts/Game/Automap.cs:66-78`):
/// `discovered` = seen at least once, `visited_in_this_run` = visited since the
/// dungeon was entered (that is what separates colour from grayscale).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BlockState {
    /// Grid column (block X).
    pub x: i32,
    /// Grid row (block Z).
    pub z: i32,
    /// `discovered`: the block was seen at least once.
    pub discovered: bool,
    /// `visitedInThisRun`: the block was visited in the current run.
    pub visited_in_this_run: bool,
}

/// Material the automap model injects into a block
/// (`DFU-UP:Assets/Scripts/Game/UserInterface/AutomapModel.cs:22,46-100`).
///
/// The model does **not** invent a third rendering state: a block that was
/// never discovered is `enabled = false` (`:71-72`) and a discovered block that
/// was not visited in this run renders with `RENDER_IN_GRAYSCALE` (`:97-100`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockMaterial {
    /// `enabled` — `false` = never discovered: the block is not drawn.
    pub enabled: bool,
    /// `RENDER_IN_GRAYSCALE` — discovered but not visited in this run.
    pub grayscale: bool,
}

impl BlockMaterial {
    /// Material of one projected block state.
    pub const fn of(state: &BlockState) -> Self {
        Self {
            enabled: state.discovered,
            grayscale: state.discovered && !state.visited_in_this_run,
        }
    }
}

/// Source of one block: grid position + the persisted `discovered` flag
/// (the composer fills it from `dungeon_grid::DungeonGrid::blocks` and the
/// saved automap record; perception never re-parses a `.RDB`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockSource {
    /// Grid column (block X).
    pub x: i32,
    /// Grid row (block Z).
    pub z: i32,
    /// `discovered` — seen at least once (persisted).
    pub discovered: bool,
}

/// User note marker of the per-dungeon automap
/// (`Automap.cs:86-95`, `SortedList<int, NoteMarker>`: keyed, ordered).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteMarker {
    /// Marker index (the `SortedList` key).
    pub index: i32,
    /// Marker text (user authored).
    pub text: String,
}

/// Teleporter connection of the per-dungeon automap
/// (`Automap.cs:86-95`, `dictTeleporterConnections`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TeleporterConnection {
    /// Origin block index.
    pub from: i32,
    /// Destination block index.
    pub to: i32,
}

/// Dungeon automap source: the real block grid of one location + the session
/// state the composer copies from the memory organ.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DungeonAutomapSource {
    /// Location index of the dungeon (key of the saved record).
    pub location: i64,
    /// `locationName`.
    pub location_name: String,
    /// `timeInSecondsLastVisited`.
    pub time_in_seconds_last_visited: i64,
    /// `entranceDiscovered`.
    pub entrance_discovered: bool,
    /// Blocks DFU places for this dungeon (`DungeonGrid::blocks`), with the
    /// persisted discovery flag.
    pub blocks: Vec<BlockSource>,
    /// Blocks visited in this run (`(x, z)`; a set — duplicates ignored).
    pub visited_in_this_run: Vec<(i32, i32)>,
    /// `listUserNoteMarkers` (any order; the projection sorts by index).
    pub note_markers: Vec<NoteMarker>,
    /// `dictTeleporterConnections` (any order; the projection sorts by origin).
    pub teleporter_connections: Vec<TeleporterConnection>,
}

/// Per-dungeon automap read model (`Automap.cs:86-95`): blocks with their two
/// state flags, the note markers in `SortedList` order and the teleporter
/// connections in key order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DungeonAutomapReadModel {
    /// Location index of the dungeon.
    pub location: i64,
    /// `locationName`.
    pub location_name: String,
    /// `timeInSecondsLastVisited`.
    pub time_in_seconds_last_visited: i64,
    /// `entranceDiscovered`.
    pub entrance_discovered: bool,
    /// Blocks, deterministic order (x, then z), 0 duplicates.
    pub blocks: Vec<BlockState>,
    /// Note markers, ascending by index, one entry per index.
    pub note_markers: Vec<NoteMarker>,
    /// Teleporter connections, ascending by origin block.
    pub teleporter_connections: Vec<TeleporterConnection>,
}

impl DungeonAutomapReadModel {
    /// Pure projection from the ECS source (re-executable: the only way to
    /// change the map is to change its source).
    pub fn project(source: &DungeonAutomapSource) -> Self {
        let visited: BTreeSet<(i32, i32)> = source.visited_in_this_run.iter().copied().collect();
        let unique: BTreeSet<BlockState> = source
            .blocks
            .iter()
            .map(|block| BlockState {
                x: block.x,
                z: block.z,
                discovered: block.discovered,
                visited_in_this_run: visited.contains(&(block.x, block.z)),
            })
            .collect();
        let notes: std::collections::BTreeMap<i32, String> = source
            .note_markers
            .iter()
            .map(|marker| (marker.index, marker.text.clone()))
            .collect();
        let teleports: std::collections::BTreeMap<i32, i32> = source
            .teleporter_connections
            .iter()
            .map(|link| (link.from, link.to))
            .collect();
        Self {
            location: source.location,
            location_name: source.location_name.clone(),
            time_in_seconds_last_visited: source.time_in_seconds_last_visited,
            entrance_discovered: source.entrance_discovered,
            blocks: unique.into_iter().collect(),
            note_markers: notes
                .into_iter()
                .map(|(index, text)| NoteMarker { index, text })
                .collect(),
            teleporter_connections: teleports
                .into_iter()
                .map(|(from, to)| TeleporterConnection { from, to })
                .collect(),
        }
    }

    /// Blocks placed by the dungeon grid (discovered or not).
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// `blocks_descubiertos`: blocks seen at least once.
    pub fn blocks_discovered(&self) -> usize {
        self.blocks.iter().filter(|block| block.discovered).count()
    }

    /// Blocks visited in this run (the colour ones).
    pub fn blocks_visited(&self) -> usize {
        self.blocks
            .iter()
            .filter(|block| block.visited_in_this_run)
            .count()
    }

    /// Blocks that render in grayscale (discovered, not visited this run).
    pub fn blocks_grayscale(&self) -> usize {
        self.blocks
            .iter()
            .filter(|block| BlockMaterial::of(block).grayscale)
            .count()
    }

    /// Bounds (bbox) of the placed blocks (`None` = 0 blocks).
    pub fn bounds(&self) -> Option<MapBounds> {
        fold_bounds(self.blocks.iter().map(|block| (block.x, block.z)))
    }

    /// Material of block `(x, z)`; `None` = the grid does not place it.
    pub fn material_of(&self, x: i32, z: i32) -> Option<BlockMaterial> {
        self.blocks
            .iter()
            .find(|block| block.x == x && block.z == z)
            .map(BlockMaterial::of)
    }

    /// ASCII dungeon grid (`#` = discovered block, `·` = not discovered; first
    /// row = highest Z, first column = lowest X). Above a bbox larger than
    /// [`ASCII_MAX_SIDE`] it returns only the legend (legibility).
    pub fn render_ascii(&self) -> String {
        let Some(bounds) = self.bounds() else {
            return "(no blocks placed)".to_string();
        };
        if bounds.width() > ASCII_MAX_SIDE || bounds.height() > ASCII_MAX_SIDE {
            return format!(
                "(bbox {}x{} cells — outside the ASCII grid, {}/{} blocks discovered)",
                bounds.width(),
                bounds.height(),
                self.blocks_discovered(),
                self.blocks.len()
            );
        }
        let mut grid = vec![vec!['·'; bounds.width() as usize]; bounds.height() as usize];
        for block in &self.blocks {
            let row = (bounds.max_z - block.z) as usize;
            let col = (block.x - bounds.min_x) as usize;
            grid[row][col] = if block.discovered { '#' } else { '·' };
        }
        let mut out = String::new();
        for row in grid {
            out.extend(row);
            out.push('\n');
        }
        out.trim_end_matches('\n').to_string()
    }
}

/// bbox fold shared by the two automaps (inclusive bounds).
fn fold_bounds(cells: impl Iterator<Item = (i32, i32)>) -> Option<MapBounds> {
    cells.fold(None, |acc: Option<MapBounds>, (x, z)| {
        Some(match acc {
            None => MapBounds {
                min_x: x,
                min_z: z,
                max_x: x,
                max_z: z,
            },
            Some(mut bounds) => {
                bounds.min_x = bounds.min_x.min(x);
                bounds.min_z = bounds.min_z.min(z);
                bounds.max_x = bounds.max_x.max(x);
                bounds.max_z = bounds.max_z.max(z);
                bounds
            }
        })
    })
}

/// Source of one exterior nameplate: the location it labels and its name
/// (the composer copies them from `MAPNAMES`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameplateSource {
    /// Location index inside the region (draw order key).
    pub location_index: usize,
    /// Location name (`MAPNAMES`).
    pub name: String,
}

/// Exterior automap source: the nameplates of one map.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExteriorAutomapSource {
    /// Nameplates in `MAPNAMES` order.
    pub nameplates: Vec<NameplateSource>,
}

/// One projected exterior nameplate.
#[derive(Debug, Clone, PartialEq)]
pub struct Nameplate {
    /// Location index inside the region.
    pub location_index: usize,
    /// Rasterised text: the name, or [`HIDDEN_NAMEPLATE_GLYPH`] when hidden.
    pub text: String,
    /// Placement depth (`nameplatesPlacementDepth`).
    pub depth: f32,
    /// `nameplateReplaced` — the label was hidden by a collision.
    pub hidden: bool,
}

/// Exterior automap read model: the nameplates of one map, placed at
/// [`NAMEPLATES_PLACEMENT_DEPTH`] and with collisions replaced by
/// [`HIDDEN_NAMEPLATE_GLYPH`] (`ExteriorAutomap.cs:59-76,116,1365`).
///
/// **Declared delta:** DFU decides `nameplateReplaced` from the screen-space
/// overlap of the label rectangles (`:1365`). This engine has no label layout
/// yet, so a collision is **two nameplates of the same map with the same
/// rendered name** (identical labels cannot both be read). The rule is
/// order-independent and it is reported as a delta, never as DFU equality.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExteriorAutomapReadModel {
    /// Nameplates, ascending by location index.
    pub nameplates: Vec<Nameplate>,
}

impl ExteriorAutomapReadModel {
    /// Pure projection from its source.
    pub fn project(source: &ExteriorAutomapSource) -> Self {
        let mut counts: std::collections::BTreeMap<&str, usize> =
            std::collections::BTreeMap::new();
        for plate in &source.nameplates {
            *counts.entry(plate.name.as_str()).or_insert(0) += 1;
        }
        let mut nameplates: Vec<Nameplate> = source
            .nameplates
            .iter()
            .map(|plate| {
                let hidden = counts.get(plate.name.as_str()).copied().unwrap_or(0) > 1;
                Nameplate {
                    location_index: plate.location_index,
                    text: nameplate_text(&plate.name, hidden),
                    depth: NAMEPLATES_PLACEMENT_DEPTH,
                    hidden,
                }
            })
            .collect();
        nameplates.sort_by_key(|plate| plate.location_index);
        Self { nameplates }
    }

    /// How many names were replaced by the hidden glyph.
    pub fn hidden_count(&self) -> usize {
        self.nameplates.iter().filter(|plate| plate.hidden).count()
    }

    /// Placement depth of every nameplate (must be the model's depth).
    pub fn depth(&self) -> f32 {
        NAMEPLATES_PLACEMENT_DEPTH
    }
}

/// Rasterised text of a nameplate: the name, or the hidden glyph
/// (`nameplateReplaced`).
pub fn nameplate_text(name: &str, hidden: bool) -> String {
    if hidden {
        HIDDEN_NAMEPLATE_GLYPH.to_string()
    } else {
        name.to_string()
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Panel inventario/quests (REQ-L99-040) — inventory/behavior→perception
// ─────────────────────────────────────────────────────────────────────────

/// Panel source: gold + goods (from `InventoryLite` = AssetSet) + active
/// quests (from `QuestLog`), copied by the composer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PanelSource {
    /// Gold (integer — NFR NUMERIC-never-FLOAT).
    pub gold: i32,
    /// Goods `(id, quantity)` in registration order.
    pub items: Vec<(String, i32)>,
    /// Titles of the active quests, in acceptance order.
    pub quests: Vec<String>,
}

/// Session panel read model: inventory + active quests
/// **projected** (+ derived totals).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionPanelReadModel {
    /// Projected gold.
    pub gold: i32,
    /// Projected goods `(id, quantity)`.
    pub items: Vec<(String, i32)>,
    /// Projected active quests (titles, in acceptance order).
    pub quests: Vec<String>,
}

impl SessionPanelReadModel {
    /// Pure panel projection from its ECS source.
    pub fn project(source: &PanelSource) -> Self {
        Self {
            gold: source.gold,
            items: source.items.clone(),
            quests: source.quests.clone(),
        }
    }

    /// Number of distinct goods kinds.
    pub fn item_kinds(&self) -> usize {
        self.items.len()
    }

    /// Total goods units.
    pub fn total_units(&self) -> i32 {
        self.items.iter().map(|(_, qty)| *qty).sum()
    }

    /// Number of active quests.
    pub fn quest_count(&self) -> usize {
        self.quests.len()
    }

    /// Panel summary line (title bar / probe output).
    pub fn summary(&self) -> String {
        let items = if self.items.is_empty() {
            "no goods".to_string()
        } else {
            self.items
                .iter()
                .map(|(id, qty)| format!("{id}×{qty}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let quests = if self.quests.is_empty() {
            "no quests".to_string()
        } else {
            self.quests.join(" | ")
        };
        format!(
            "gold={} · goods [{items}] ({} kinds, {} units) · active quests [{quests}] ({})",
            self.gold,
            self.item_kinds(),
            self.total_units(),
            self.quest_count(),
        )
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Complete session snapshot
// ─────────────────────────────────────────────────────────────────────────

/// The three PD4 T7 session read models, projected together (the vault
/// consumed by the M7 window title bar and the PROBE-READMODEL-001 probe).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionReadModels {
    /// HUD (REQ-L99-038).
    pub hud: HudReadModel,
    /// Automap (REQ-L99-039).
    pub automap: AutomapReadModel,
    /// Inventory/quests panel (REQ-L99-040).
    pub panel: SessionPanelReadModel,
    /// Character creator (M38/UI-2, D-1) — declared no-session state by
    /// default.
    pub createchar: CreateCharModels,
    /// Typed configuration (M38/UI-2, D-1) — declared no-session state by
    /// default.
    pub config: ConfigModels,
    /// The 96 M39/UI-3 routes (A21 C14 D16 E12 F10 G11 H12) — declared
    /// no-session state by default.
    pub m39: M39Models,
}

impl SessionReadModels {
    /// Joint projection of the three read models from their ECS sources.
    ///
    /// The `createchar.*` and `config.*` families start in their declared
    /// no-session state; a session with a live creator sets them through
    /// [`CreateCharModels::project`] / [`ConfigModels::project`].
    pub fn project(hud: &HudSource, automap: &AutomapSource, panel: &PanelSource) -> Self {
        Self {
            hud: HudReadModel::project(hud),
            automap: AutomapReadModel::project(automap),
            panel: SessionPanelReadModel::project(panel),
            createchar: CreateCharModels::default(),
            config: ConfigModels::default(),
            m39: M39Models::default(),
        }
    }

    /// Resolves one declared read-model path (`None` = undeclared path).
    ///
    /// Single source of truth: `hud.*`/`panel.*` are computed from their
    /// fields (the M37 semantics, byte for byte) and `createchar.*`/`config.*`
    /// delegate to their own families, so a read model can never be stale with
    /// respect to the fields the probe or the ECS mutated.
    pub fn resolve(&self, path: &str) -> Option<String> {
        Some(match path {
            "hud.health" => self.hud.health.to_string(),
            "hud.max_health" => self.hud.max_health.to_string(),
            "hud.stamina" => self.hud.stamina.to_string(),
            "hud.max_stamina" => self.hud.max_stamina.to_string(),
            "hud.level" => self.hud.level.to_string(),
            "hud.health_pct" => self.hud.health_pct.to_string(),
            "hud.stamina_pct" => self.hud.stamina_pct.to_string(),
            "panel.gold" => self.panel.gold.to_string(),
            "panel.item_count" => self.panel.item_kinds().to_string(),
            "panel.quest_count" => self.panel.quest_count().to_string(),
            path => {
                return self
                    .createchar
                    .resolve(path)
                    .or_else(|| self.config.resolve(path))
                    .or_else(|| self.m39.resolve(path))
            }
        })
    }

    /// M7 title line: the three read models on a single line (M4/M5
    /// title bar pattern).
    pub fn title_line(&self) -> String {
        format!(
            "{} | automap: {} chunks | panel: gold={} goods={} quests={}",
            self.hud.summary(),
            self.automap.chunk_count(),
            self.panel.gold,
            self.panel.item_kinds(),
            self.panel.quest_count(),
        )
    }
}

// ─────────────────────────────────────────────────────────────────────────
// M38/UI-2 — createchar.* (36) and config.* (11) read models (D-1)
// ─────────────────────────────────────────────────────────────────────────

/// Maximum of one projected attribute (`stat.strength`, `stat.agility`); the
/// eight attributes of a character therefore cap `stat_sum` at 800.
pub const ATTRIBUTE_MAX: u16 = 100;
/// Attributes of a character — mirror of
/// `daggerfall_skills::character::ATTRIBUTE_COUNT` (this crate keeps 0 organ
/// dependencies; `m38_surfaces` asserts the mirror against the real constant).
pub const CHARACTER_ATTRIBUTE_COUNT: usize = 8;
/// Skills of a character — mirror of
/// `daggerfall_skills::character::SKILL_COUNT`.
pub const CHARACTER_SKILL_COUNT: usize = 35;
/// Standard classes — mirror of `daggerfall_skills::career::CLASS_COUNT`.
pub const CHARACTER_CLASS_COUNT: usize = 18;
/// Characters accepted by the name field (declared capacity of the UI field).
pub const NAME_MAX_LEN: usize = 32;
/// Highest character level the creator projects.
pub const LEVEL_MAX: u8 = 100;
/// Upper bound of the per-mille amounts (`bloom`, `motionblur`, `vignette`,
/// `dof.distance`): integer per-mille, never FLOAT.
pub const AMOUNT_MAX: u16 = 1000;
/// Highest number of declared effects (typed capacity of the effects list).
pub const EFFECTS_MAX: u16 = 64;
/// Advantage/disadvantage slots of the special-advantages window.
pub const SPECIAL_SLOTS: u16 = 8;
/// Skills of a custom class (primary 3 + major 3).
pub const MAX_CUSTOM_SKILLS: u8 = 6;
/// Absolute bound of `reputation.value` (`-100..=100`).
pub const REPUTATION_MAX: i16 = 100;
/// Gender captions of the creator (the two people it projects).
pub const GENDER_NAMES: [&str; 2] = ["Male", "Female"];
/// Race captions: the three people banks of the M15 census
/// (`daggerfall_parser_bevy_bridge::npc_spec::Race`), never a string a caller
/// invents.
pub const RACE_NAMES: [&str; 3] = ["Breton", "Redguard", "Nord"];
/// Label of a read model with **no session** behind it (the `Default` state:
/// declared, printed, never an empty run).
pub const NO_SESSION: &str = "NO SESSION";
/// Reputation bands, low to high (`reputation.band`).
pub const REPUTATION_BANDS: [&str; 5] = ["hostile", "cold", "neutral", "warm", "friendly"];

/// Why a `createchar.*` / `config.*` read model could not be constructed.
///
/// The two variants are the whole falsifier surface of D-1: a value outside
/// its declared range is **rejected**, never clamped, and an empty caption is
/// rejected too (an empty run paints nothing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadModelError {
    /// A numeric value outside `min..=max` (the range is the contract).
    OutOfRange {
        /// Read-model field that was rejected.
        field: &'static str,
        /// Value handed in.
        value: i64,
        /// Highest admissible value.
        max: i64,
    },
    /// A caption that resolves to nothing.
    Empty {
        /// Read-model field that was rejected.
        field: &'static str,
    },
}

impl core::fmt::Display for ReadModelError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::OutOfRange { field, value, max } => {
                write!(formatter, "{field} = {value} outside 0..={max}")
            }
            Self::Empty { field } => write!(formatter, "{field} is empty"),
        }
    }
}

impl std::error::Error for ReadModelError {}

fn non_empty(field: &'static str, text: &str) -> Result<(), ReadModelError> {
    if text.trim().is_empty() {
        Err(ReadModelError::Empty { field })
    } else {
        Ok(())
    }
}

fn within(field: &'static str, value: i64, max: i64) -> Result<(), ReadModelError> {
    if value < 0 || value > max {
        Err(ReadModelError::OutOfRange { field, value, max })
    } else {
        Ok(())
    }
}

fn between(field: &'static str, value: i64, min: i64, max: i64) -> Result<(), ReadModelError> {
    if value < min || value > max {
        Err(ReadModelError::OutOfRange {
            field,
            value,
            max: if value < min { min } else { max },
        })
    } else {
        Ok(())
    }
}

/// Reputation band of a value (`reputation.band`), declared projection.
pub fn reputation_band(value: i16) -> &'static str {
    match value {
        value if value <= -50 => REPUTATION_BANDS[0],
        value if value < 0 => REPUTATION_BANDS[1],
        0 => REPUTATION_BANDS[2],
        value if value < 50 => REPUTATION_BANDS[3],
        _ => REPUTATION_BANDS[4],
    }
}

/// How the class of the new character was chosen (`classgen.method`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClassGenMethod {
    /// The questionnaire of `BIOGnnT0.TXT`.
    #[default]
    Questionnaire,
    /// A class built by the player.
    Custom,
    /// One of the standard `CLASSnn.CFG` careers.
    Pregenerated,
}

impl ClassGenMethod {
    /// The three routes, in declaration order.
    pub const ALL: [ClassGenMethod; 3] = [
        ClassGenMethod::Questionnaire,
        ClassGenMethod::Custom,
        ClassGenMethod::Pregenerated,
    ];

    /// Caption of the route.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Questionnaire => "questionnaire",
            Self::Custom => "custom",
            Self::Pregenerated => "pregenerated",
        }
    }
}

/// Source of the `createchar.*` family: the composer copies it from the
/// creator session (BIO.DAT, `BIOGnnT0.TXT`, `CLASSnn.CFG`, `FACES.CIF`).
///
/// Typed, but **not yet validated**: [`CreateCharModels::project`] is the
/// gate. Indices are rejected when out of range, never clamped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateCharSource {
    /// Typed name of the character.
    pub name: String,
    /// Capacity of the name field (1..=`NAME_MAX_LEN`).
    pub name_max_len: usize,
    /// Gender index (`GENDER_NAMES`).
    pub gender: u8,
    /// Race index (`RACE_NAMES`).
    pub race: u8,
    /// Class caption (`CLASSnn.CFG` bytes 28..44).
    pub class_name: String,
    /// Standard class index (`0..CHARACTER_CLASS_COUNT`).
    pub class_index: u8,
    /// Selected face (`FACES.CIF` record).
    pub face_index: u16,
    /// Faces measured in `FACES.CIF`.
    pub face_count: u16,
    /// Character level.
    pub level: u8,
    /// Skill bonus points still to spend.
    pub bonus_remaining: u16,
    /// Attribute points still to spend.
    pub stats_remaining: u16,
    /// The eight attributes of the class (`CLASSnn.CFG`).
    pub attributes: [u16; CHARACTER_ATTRIBUTE_COUNT],
    /// Primary skill slots (indices into the 35 skills).
    pub skills: [u8; 3],
    /// Biography body (`BIO.DAT`).
    pub biography: String,
    /// Non-empty `BIO.DAT` lines the body carries.
    pub biography_lines: usize,
    /// `BIOGnnT0.TXT` files present (the retail count).
    pub biography_count: u16,
    /// Title of the selected biography.
    pub biography_title: String,
    /// Text of the selected question.
    pub question_text: String,
    /// First answer of the selected question.
    pub answer1: String,
    /// Second answer of the selected question.
    pub answer2: String,
    /// Index of the selected question.
    pub question_index: u16,
    /// Questions declared by the selected `BIOGnnT0.TXT`.
    pub question_count: u16,
    /// Route that chose the class.
    pub classgen: ClassGenMethod,
    /// Caption of the custom class.
    pub customclass_name: String,
    /// Skills of the custom class (`1..=MAX_CUSTOM_SKILLS`).
    pub customclass_skills: u8,
    /// Attribute sum of the custom class (`<= 800`).
    pub customclass_stat_sum: u16,
    /// Reflex points still to spend.
    pub reflex_remaining: u16,
    /// Index of the attribute the reflexes favour.
    pub reflex_stat_top: u8,
    /// Reputation of the character (`-100..=100`).
    pub reputation: i16,
    /// Special advantages taken.
    pub advantage_count: u16,
    /// Special disadvantages taken.
    pub disadvantage_count: u16,
}

/// Projected `createchar.*` read model (36 paths).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateCharModels {
    /// `createchar.name.value`.
    pub name: String,
    /// `createchar.name.max_len`.
    pub name_max_len: usize,
    /// `createchar.gender.name`.
    pub gender: &'static str,
    /// `createchar.race.name`.
    pub race: &'static str,
    /// `createchar.class.name`.
    pub class_name: String,
    /// `createchar.class.count`.
    pub class_count: usize,
    /// `createchar.face.index`.
    pub face_index: u16,
    /// `createchar.face.count`.
    pub face_count: u16,
    /// `createchar.level`.
    pub level: u8,
    /// `createchar.bonus_remaining`.
    pub bonus_remaining: u16,
    /// `createchar.stats_remaining`.
    pub stats_remaining: u16,
    /// `createchar.stat.strength` (attribute 0).
    pub strength: u16,
    /// `createchar.stat.agility` (attribute 3).
    pub agility: u16,
    /// `createchar.stat_sum` (the eight attributes).
    pub stat_sum: u16,
    /// `createchar.skill_slot1..3`.
    pub skills: [u8; 3],
    /// `createchar.biography.body`.
    pub biography: String,
    /// `createchar.biography.line_count`.
    pub biography_lines: usize,
    /// `createchar.biography_count`.
    pub biography_count: u16,
    /// `createchar.biography.selected_title`.
    pub biography_title: String,
    /// `createchar.question.text`.
    pub question_text: String,
    /// `createchar.question.answer1`.
    pub answer1: String,
    /// `createchar.question.answer2`.
    pub answer2: String,
    /// `createchar.question.index`.
    pub question_index: u16,
    /// `createchar.question.index` bound.
    pub question_count: u16,
    /// `createchar.classgen.method`.
    pub classgen: ClassGenMethod,
    /// `createchar.customclass.name`.
    pub customclass_name: String,
    /// `createchar.customclass.skill_count`.
    pub customclass_skills: u8,
    /// `createchar.customclass.stat_sum`.
    pub customclass_stat_sum: u16,
    /// `createchar.reflex.remaining`.
    pub reflex_remaining: u16,
    /// `createchar.reflex.stat_top`.
    pub reflex_stat_top: u8,
    /// `createchar.reputation.value`.
    pub reputation: i16,
    /// `createchar.advantage.count`.
    pub advantage_count: u16,
    /// `createchar.disadvantage.count`.
    pub disadvantage_count: u16,
}

/// The declared **no-session** state: every path resolves to a caption and
/// every index is legal, so a probe/test can read a `createchar.*` path before
/// the creator exists (never an empty run, never an out-of-range index).
impl Default for CreateCharModels {
    fn default() -> Self {
        Self {
            name: NO_SESSION.to_string(),
            name_max_len: NAME_MAX_LEN,
            gender: GENDER_NAMES[0],
            race: RACE_NAMES[0],
            class_name: NO_SESSION.to_string(),
            class_count: 0,
            face_index: 0,
            face_count: 0,
            level: 1,
            bonus_remaining: 0,
            stats_remaining: 0,
            strength: 0,
            agility: 0,
            stat_sum: 0,
            skills: [0; 3],
            biography: NO_SESSION.to_string(),
            biography_lines: 0,
            biography_count: 0,
            biography_title: NO_SESSION.to_string(),
            question_text: NO_SESSION.to_string(),
            answer1: NO_SESSION.to_string(),
            answer2: NO_SESSION.to_string(),
            question_index: 0,
            question_count: 0,
            classgen: ClassGenMethod::Questionnaire,
            customclass_name: NO_SESSION.to_string(),
            customclass_skills: 0,
            customclass_stat_sum: 0,
            reflex_remaining: 0,
            reflex_stat_top: 0,
            reputation: 0,
            advantage_count: 0,
            disadvantage_count: 0,
        }
    }
}

impl CreateCharModels {
    /// Validating projection of the `createchar.*` family.
    ///
    /// Every rejection is a [`ReadModelError`]: an out-of-range index or an
    /// empty caption stops the projection instead of degrading into `0` or `""`.
    pub fn project(source: &CreateCharSource) -> Result<Self, ReadModelError> {
        non_empty("createchar.name.value", &source.name)?;
        between(
            "createchar.name.max_len",
            source.name_max_len as i64,
            1,
            NAME_MAX_LEN as i64,
        )?;
        if source.name.chars().count() > source.name_max_len {
            return Err(ReadModelError::OutOfRange {
                field: "createchar.name.value",
                value: source.name.chars().count() as i64,
                max: source.name_max_len as i64,
            });
        }
        within(
            "createchar.gender.name",
            source.gender as i64,
            GENDER_NAMES.len() as i64 - 1,
        )?;
        within(
            "createchar.race.name",
            source.race as i64,
            RACE_NAMES.len() as i64 - 1,
        )?;
        non_empty("createchar.class.name", &source.class_name)?;
        within(
            "createchar.class.count",
            source.class_index as i64,
            CHARACTER_CLASS_COUNT as i64 - 1,
        )?;
        if source.face_count == 0 {
            return Err(ReadModelError::Empty {
                field: "createchar.face.count",
            });
        }
        within(
            "createchar.face.index",
            source.face_index as i64,
            source.face_count as i64 - 1,
        )?;
        between("createchar.level", source.level as i64, 1, LEVEL_MAX as i64)?;
        within(
            "createchar.bonus_remaining",
            source.bonus_remaining as i64,
            CHARACTER_SKILL_COUNT as i64,
        )?;
        within(
            "createchar.stats_remaining",
            source.stats_remaining as i64,
            (CHARACTER_ATTRIBUTE_COUNT * ATTRIBUTE_MAX as usize) as i64,
        )?;
        let mut stat_sum = 0u16;
        for attribute in source.attributes {
            within(
                "createchar.stat",
                attribute as i64,
                ATTRIBUTE_MAX as i64,
            )?;
            stat_sum += attribute;
        }
        for skill in source.skills {
            within(
                "createchar.skill_slot",
                skill as i64,
                CHARACTER_SKILL_COUNT as i64 - 1,
            )?;
        }
        non_empty("createchar.biography.body", &source.biography)?;
        between(
            "createchar.biography.line_count",
            source.biography_lines as i64,
            1,
            isize::MAX as i64 / 4,
        )?;
        if source.biography_count == 0 {
            return Err(ReadModelError::Empty {
                field: "createchar.biography_count",
            });
        }
        non_empty(
            "createchar.biography.selected_title",
            &source.biography_title,
        )?;
        non_empty("createchar.question.text", &source.question_text)?;
        non_empty("createchar.question.answer1", &source.answer1)?;
        non_empty("createchar.question.answer2", &source.answer2)?;
        if source.question_count == 0 {
            return Err(ReadModelError::Empty {
                field: "createchar.question.index",
            });
        }
        within(
            "createchar.question.index",
            source.question_index as i64,
            source.question_count as i64 - 1,
        )?;
        non_empty("createchar.customclass.name", &source.customclass_name)?;
        between(
            "createchar.customclass.skill_count",
            source.customclass_skills as i64,
            1,
            MAX_CUSTOM_SKILLS as i64,
        )?;
        within(
            "createchar.customclass.stat_sum",
            source.customclass_stat_sum as i64,
            (CHARACTER_ATTRIBUTE_COUNT * ATTRIBUTE_MAX as usize) as i64,
        )?;
        within(
            "createchar.reflex.remaining",
            source.reflex_remaining as i64,
            CHARACTER_ATTRIBUTE_COUNT as i64,
        )?;
        within(
            "createchar.reflex.stat_top",
            source.reflex_stat_top as i64,
            CHARACTER_ATTRIBUTE_COUNT as i64 - 1,
        )?;
        between(
            "createchar.reputation.value",
            source.reputation as i64,
            -(REPUTATION_MAX as i64),
            REPUTATION_MAX as i64,
        )?;
        within(
            "createchar.advantage.count",
            source.advantage_count as i64,
            SPECIAL_SLOTS as i64,
        )?;
        within(
            "createchar.disadvantage.count",
            source.disadvantage_count as i64,
            SPECIAL_SLOTS as i64,
        )?;
        Ok(Self {
            name: source.name.clone(),
            name_max_len: source.name_max_len,
            gender: GENDER_NAMES[source.gender as usize],
            race: RACE_NAMES[source.race as usize],
            class_name: source.class_name.clone(),
            class_count: CHARACTER_CLASS_COUNT,
            face_index: source.face_index,
            face_count: source.face_count,
            level: source.level,
            bonus_remaining: source.bonus_remaining,
            stats_remaining: source.stats_remaining,
            strength: source.attributes[0],
            agility: source.attributes[3],
            stat_sum,
            skills: source.skills,
            biography: source.biography.clone(),
            biography_lines: source.biography_lines,
            biography_count: source.biography_count,
            biography_title: source.biography_title.clone(),
            question_text: source.question_text.clone(),
            answer1: source.answer1.clone(),
            answer2: source.answer2.clone(),
            question_index: source.question_index,
            question_count: source.question_count,
            classgen: source.classgen,
            customclass_name: source.customclass_name.clone(),
            customclass_skills: source.customclass_skills,
            customclass_stat_sum: source.customclass_stat_sum,
            reflex_remaining: source.reflex_remaining,
            reflex_stat_top: source.reflex_stat_top,
            reputation: source.reputation,
            advantage_count: source.advantage_count,
            disadvantage_count: source.disadvantage_count,
        })
    }

    /// The 36 `createchar.*` paths of this family.
    pub const PATHS: [&'static str; 36] = [
        "createchar.name.value",
        "createchar.name.max_len",
        "createchar.gender.name",
        "createchar.race.name",
        "createchar.race.count",
        "createchar.class.name",
        "createchar.class.count",
        "createchar.face.index",
        "createchar.face.count",
        "createchar.level",
        "createchar.bonus_remaining",
        "createchar.stats_remaining",
        "createchar.stat.strength",
        "createchar.stat.agility",
        "createchar.skill_slot1",
        "createchar.skill_slot2",
        "createchar.skill_slot3",
        "createchar.biography.body",
        "createchar.biography.line_count",
        "createchar.biography_count",
        "createchar.biography.selected_title",
        "createchar.question.text",
        "createchar.question.answer1",
        "createchar.question.answer2",
        "createchar.question.index",
        "createchar.classgen.method",
        "createchar.customclass.name",
        "createchar.customclass.skill_count",
        "createchar.customclass.stat_sum",
        "createchar.reflex.remaining",
        "createchar.reflex.stat_top",
        "createchar.reputation.value",
        "createchar.reputation.band",
        "createchar.advantage.count",
        "createchar.disadvantage.count",
        "createchar.stat_sum",
    ];

    /// Resolves one declared path (`None` = not a path of this family).
    pub fn resolve(&self, path: &str) -> Option<String> {
        Some(match path {
            "createchar.name.value" => self.name.clone(),
            "createchar.name.max_len" => self.name_max_len.to_string(),
            "createchar.gender.name" => self.gender.to_string(),
            "createchar.race.name" => self.race.to_string(),
            "createchar.race.count" => RACE_NAMES.len().to_string(),
            "createchar.class.name" => self.class_name.clone(),
            "createchar.class.count" => self.class_count.to_string(),
            "createchar.face.index" => self.face_index.to_string(),
            "createchar.face.count" => self.face_count.to_string(),
            "createchar.level" => self.level.to_string(),
            "createchar.bonus_remaining" => self.bonus_remaining.to_string(),
            "createchar.stats_remaining" => self.stats_remaining.to_string(),
            "createchar.stat.strength" => self.strength.to_string(),
            "createchar.stat.agility" => self.agility.to_string(),
            "createchar.skill_slot1" => self.skills[0].to_string(),
            "createchar.skill_slot2" => self.skills[1].to_string(),
            "createchar.skill_slot3" => self.skills[2].to_string(),
            "createchar.biography.body" => self.biography.clone(),
            "createchar.biography.line_count" => self.biography_lines.to_string(),
            "createchar.biography_count" => self.biography_count.to_string(),
            "createchar.biography.selected_title" => self.biography_title.clone(),
            "createchar.question.text" => self.question_text.clone(),
            "createchar.question.answer1" => self.answer1.clone(),
            "createchar.question.answer2" => self.answer2.clone(),
            "createchar.question.index" => self.question_index.to_string(),
            "createchar.classgen.method" => self.classgen.as_str().to_string(),
            "createchar.customclass.name" => self.customclass_name.clone(),
            "createchar.customclass.skill_count" => self.customclass_skills.to_string(),
            "createchar.customclass.stat_sum" => self.customclass_stat_sum.to_string(),
            "createchar.reflex.remaining" => self.reflex_remaining.to_string(),
            "createchar.reflex.stat_top" => self.reflex_stat_top.to_string(),
            "createchar.reputation.value" => self.reputation.to_string(),
            "createchar.reputation.band" => reputation_band(self.reputation).to_string(),
            "createchar.advantage.count" => self.advantage_count.to_string(),
            "createchar.disadvantage.count" => self.disadvantage_count.to_string(),
            "createchar.stat_sum" => self.stat_sum.to_string(),
            _ => return None,
        })
    }
}

/// Ambient occlusion mode (`config.ao.mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AoMode {
    /// Off.
    #[default]
    Off,
    /// Screen-space occlusion.
    Ssao,
    /// Horizon-based occlusion.
    Hbao,
}

impl AoMode {
    /// Caption of the mode.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Ssao => "ssao",
            Self::Hbao => "hbao",
        }
    }
}

/// Antialiasing mode (`config.antialiasing.mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AntialiasingMode {
    /// Off.
    #[default]
    None,
    /// Fast approximate AA.
    Fxaa,
    /// Subpixel morphological AA.
    Smaa,
    /// Temporal AA.
    Taa,
}

impl AntialiasingMode {
    /// Caption of the mode.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Fxaa => "fxaa",
            Self::Smaa => "smaa",
            Self::Taa => "taa",
        }
    }
}

/// Dither mode (`config.dither.mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DitherMode {
    /// Off.
    #[default]
    Off,
    /// Ordered (Bayer) dither.
    Ordered,
    /// Error-diffusion dither.
    Diffused,
}

impl DitherMode {
    /// Caption of the mode.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Ordered => "ordered",
            Self::Diffused => "diffused",
        }
    }
}

/// Retro mode (`config.retromode.mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RetroMode {
    /// Off.
    #[default]
    Off,
    /// On.
    On,
}

impl RetroMode {
    /// Caption of the mode.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::On => "on",
        }
    }
}

/// Source of the `config.*` family: the typed settings of the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ConfigSource {
    /// `config.ao.mode`.
    pub ao: AoMode,
    /// `config.antialiasing.mode`.
    pub antialiasing: AntialiasingMode,
    /// `config.bloom.amount` (per-mille).
    pub bloom: u16,
    /// `config.colorboost.saturation` (`-100..=100`).
    pub colorboost: i16,
    /// `config.dof.distance` (per-mille, never 0).
    pub dof_distance: u16,
    /// `config.dither.mode`.
    pub dither: DitherMode,
    /// `config.effects.count`.
    pub effects: u16,
    /// `config.effects_applied.count`.
    pub effects_applied: u16,
    /// `config.motionblur.amount` (per-mille).
    pub motionblur: u16,
    /// `config.retromode.mode`.
    pub retromode: RetroMode,
    /// `config.vignette.amount` (per-mille).
    pub vignette: u16,
}

/// Projected `config.*` read model (11 paths).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ConfigModels {
    /// `config.ao.mode`.
    pub ao: AoMode,
    /// `config.antialiasing.mode`.
    pub antialiasing: AntialiasingMode,
    /// `config.bloom.amount`.
    pub bloom: u16,
    /// `config.colorboost.saturation`.
    pub colorboost: i16,
    /// `config.dof.distance`.
    pub dof_distance: u16,
    /// `config.dither.mode`.
    pub dither: DitherMode,
    /// `config.effects.count`.
    pub effects: u16,
    /// `config.effects_applied.count`.
    pub effects_applied: u16,
    /// `config.motionblur.amount`.
    pub motionblur: u16,
    /// `config.retromode.mode`.
    pub retromode: RetroMode,
    /// `config.vignette.amount`.
    pub vignette: u16,
}

impl ConfigModels {
    /// The eleven `config.*` paths of this family.
    pub const PATHS: [&'static str; 11] = [
        "config.ao.mode",
        "config.antialiasing.mode",
        "config.bloom.amount",
        "config.colorboost.saturation",
        "config.dof.distance",
        "config.dither.mode",
        "config.effects.count",
        "config.effects_applied.count",
        "config.motionblur.amount",
        "config.retromode.mode",
        "config.vignette.amount",
    ];

    /// Validating projection of the `config.*` family.
    pub fn project(source: &ConfigSource) -> Result<Self, ReadModelError> {
        within(
            "config.bloom.amount",
            source.bloom as i64,
            AMOUNT_MAX as i64,
        )?;
        between(
            "config.colorboost.saturation",
            source.colorboost as i64,
            -100,
            100,
        )?;
        between(
            "config.dof.distance",
            source.dof_distance as i64,
            1,
            AMOUNT_MAX as i64,
        )?;
        within(
            "config.effects.count",
            source.effects as i64,
            EFFECTS_MAX as i64,
        )?;
        within(
            "config.effects_applied.count",
            source.effects_applied as i64,
            source.effects as i64,
        )?;
        within(
            "config.motionblur.amount",
            source.motionblur as i64,
            AMOUNT_MAX as i64,
        )?;
        within(
            "config.vignette.amount",
            source.vignette as i64,
            AMOUNT_MAX as i64,
        )?;
        Ok(Self {
            ao: source.ao,
            antialiasing: source.antialiasing,
            bloom: source.bloom,
            colorboost: source.colorboost,
            dof_distance: source.dof_distance,
            dither: source.dither,
            effects: source.effects,
            effects_applied: source.effects_applied,
            motionblur: source.motionblur,
            retromode: source.retromode,
            vignette: source.vignette,
        })
    }

    /// Resolves one declared path (`None` = not a path of this family).
    pub fn resolve(&self, path: &str) -> Option<String> {
        Some(match path {
            "config.ao.mode" => self.ao.as_str().to_string(),
            "config.antialiasing.mode" => self.antialiasing.as_str().to_string(),
            "config.bloom.amount" => self.bloom.to_string(),
            "config.colorboost.saturation" => self.colorboost.to_string(),
            "config.dof.distance" => self.dof_distance.to_string(),
            "config.dither.mode" => self.dither.as_str().to_string(),
            "config.effects.count" => self.effects.to_string(),
            "config.effects_applied.count" => self.effects_applied.to_string(),
            "config.motionblur.amount" => self.motionblur.to_string(),
            "config.retromode.mode" => self.retromode.as_str().to_string(),
            "config.vignette.amount" => self.vignette.to_string(),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod m38_read_model_tests {
    use super::*;

    fn createchar_source() -> CreateCharSource {
        CreateCharSource {
            name: "Agronak".to_string(),
            name_max_len: NAME_MAX_LEN,
            gender: 0,
            race: 0,
            class_name: "Mage".to_string(),
            class_index: 0,
            face_index: 4,
            face_count: 61,
            level: 1,
            bonus_remaining: 3,
            stats_remaining: 4,
            attributes: [55, 60, 50, 45, 40, 35, 30, 25],
            skills: [0, 1, 2],
            biography: "A BIOGRAPHY".to_string(),
            biography_lines: 30,
            biography_count: 18,
            biography_title: "BIOG00T0".to_string(),
            question_text: "A QUESTION".to_string(),
            answer1: "A".to_string(),
            answer2: "B".to_string(),
            question_index: 0,
            question_count: 8,
            classgen: ClassGenMethod::Questionnaire,
            customclass_name: "CUSTOM".to_string(),
            customclass_skills: 6,
            customclass_stat_sum: 800,
            reflex_remaining: 2,
            reflex_stat_top: 0,
            reputation: -60,
            advantage_count: 1,
            disadvantage_count: 1,
        }
    }

    /// D-1 falsifier 1 — a path outside the two families is not resolvable
    /// (never echoed back, never a default).
    #[test]
    fn unknown_path_is_not_resolved() {
        let models = SessionReadModels::default();
        assert!(models.resolve("createchar.secret").is_none());
        assert!(models.resolve("config.secret").is_none());
        assert!(models.resolve("hud.secret").is_none());
    }

    /// D-1 falsifier 2 — a value outside its range is rejected at construction
    /// (no clamp: 101 attributes do not become 100).
    #[test]
    fn out_of_range_value_is_rejected() {
        let mut source = createchar_source();
        source.face_index = source.face_count;
        assert_eq!(
            CreateCharModels::project(&source),
            Err(ReadModelError::OutOfRange {
                field: "createchar.face.index",
                value: 61,
                max: 60,
            })
        );
        let mut source = createchar_source();
        source.attributes[0] = ATTRIBUTE_MAX + 1;
        assert!(matches!(
            CreateCharModels::project(&source),
            Err(ReadModelError::OutOfRange {
                field: "createchar.stat",
                ..
            })
        ));
        let mut source = createchar_source();
        source.level = 0;
        assert!(CreateCharModels::project(&source).is_err());
        let mut source = createchar_source();
        source.reputation = REPUTATION_MAX + 1;
        assert!(CreateCharModels::project(&source).is_err());
        let mut config = ConfigSource {
            dof_distance: 1,
            effects: 4,
            effects_applied: 4,
            ..ConfigSource::default()
        };
        config.effects_applied = 5;
        assert_eq!(
            ConfigModels::project(&config),
            Err(ReadModelError::OutOfRange {
                field: "config.effects_applied.count",
                value: 5,
                max: 4,
            })
        );
        config.effects_applied = 4;
        config.dof_distance = 0;
        assert!(ConfigModels::project(&config).is_err());
    }

    /// D-1 falsifier 3 — an empty caption is rejected (an empty run paints
    /// nothing, so it never reaches the screen).
    #[test]
    fn empty_caption_is_rejected() {
        let mut source = createchar_source();
        source.name = "   ".to_string();
        assert_eq!(
            CreateCharModels::project(&source),
            Err(ReadModelError::Empty {
                field: "createchar.name.value"
            })
        );
        let mut source = createchar_source();
        source.biography = String::new();
        assert!(matches!(
            CreateCharModels::project(&source),
            Err(ReadModelError::Empty {
                field: "createchar.biography.body"
            })
        ));
        let mut source = createchar_source();
        source.question_count = 0;
        assert!(CreateCharModels::project(&source).is_err());
    }

    /// Every declared path of both families resolves, and the categories are
    /// disjoint (36 + 11 = 47).
    #[test]
    fn every_declared_path_resolves() {
        let models = SessionReadModels::default();
        assert_eq!(CreateCharModels::PATHS.len(), 36);
        assert_eq!(ConfigModels::PATHS.len(), 11);
        for path in CreateCharModels::PATHS {
            let text = models
                .resolve(path)
                .unwrap_or_else(|| panic!("{path} must resolve"));
            assert!(!text.trim().is_empty(), "{path} resolved empty");
        }
        for path in ConfigModels::PATHS {
            let text = models
                .resolve(path)
                .unwrap_or_else(|| panic!("{path} must resolve"));
            assert!(!text.trim().is_empty(), "{path} resolved empty");
        }
        // The no-session state keeps the invariants of the projection.
        let default = CreateCharModels::default();
        assert!(GENDER_NAMES.contains(&default.gender));
        assert!(RACE_NAMES.contains(&default.race));
        assert!((1..=LEVEL_MAX).contains(&default.level));
        assert!(default.stat_sum <= ATTRIBUTE_MAX * CHARACTER_ATTRIBUTE_COUNT as u16);
    }

    /// The projection is pure: identical sources give identical models, and
    /// the derived fields (`stat_sum`, band, captions) follow the source.
    #[test]
    fn projection_is_pure_and_derived_fields_follow_the_source() {
        let source = createchar_source();
        let first = CreateCharModels::project(&source).expect("createchar");
        let second = CreateCharModels::project(&source).expect("createchar");
        assert_eq!(first, second);
        assert_eq!(first.stat_sum, 340);
        assert_eq!(first.strength, 55);
        assert_eq!(first.agility, 45);
        assert_eq!(first.class_count, CHARACTER_CLASS_COUNT);
        assert_eq!(reputation_band(-60), "hostile");
        assert_eq!(reputation_band(0), "neutral");
        assert_eq!(reputation_band(60), "friendly");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hud_source() -> HudSource {
        HudSource {
            health: 100,
            max_health: 100,
            stamina: 80,
            max_stamina: 80,
            level: 1,
        }
    }

    #[test]
    fn hud_projects_bands_and_change_follows_source() {
        let mut source = hud_source();
        let before = HudReadModel::project(&source);
        assert_eq!(
            (
                before.health_pct,
                before.stamina_pct,
                before.conscious,
                before.level,
            ),
            (100, 100, true, 1)
        );
        // Change in the source ⇒ change in the read model (re-executable).
        source.health = 30;
        source.level = 2;
        let after = HudReadModel::project(&source);
        assert_eq!((after.health, after.health_pct, after.level), (30, 30, 2));
        assert_ne!(before, after);
    }

    #[test]
    fn hud_clamps_bands_and_flags_ko() {
        let source = HudSource {
            health: 0,
            max_health: 0,
            stamina: 250,
            max_stamina: 100,
            level: 3,
        };
        let model = HudReadModel::project(&source);
        assert_eq!((model.health_pct, model.stamina_pct), (0, 100));
        assert!(!model.conscious);
        assert!(model.summary().contains("DOWNED"));
    }

    #[test]
    fn automap_sorts_dedups_and_bounds() {
        let source = AutomapSource {
            cells: vec![
                MapCell { x: 1, z: 0 },
                MapCell { x: 0, z: 1 },
                MapCell { x: 1, z: 0 },
                MapCell { x: 0, z: 0 },
            ],
        };
        let model = AutomapReadModel::project(&source);
        assert_eq!(model.chunk_count(), 3);
        assert_eq!(
            model.cells,
            vec![
                MapCell { x: 0, z: 0 },
                MapCell { x: 0, z: 1 },
                MapCell { x: 1, z: 0 },
            ]
        );
        let bounds = model.bounds.expect("bbox present");
        assert_eq!(
            (bounds.min_x, bounds.min_z, bounds.max_x, bounds.max_z),
            (0, 0, 1, 1)
        );
        assert!(model.contains(1, 0));
        assert!(!model.contains(9, 9));
        // ASCII grid: 1st row = highest Z (z=1 → cell x=0), 2nd row z=0.
        assert_eq!(model.render_ascii(), "#·\n##");
    }

    #[test]
    fn automap_empty_and_huge_bbox_have_legible_output() {
        let empty = AutomapReadModel::project(&AutomapSource::default());
        assert_eq!(empty.render_ascii(), "(no chunks loaded)");
        let huge = AutomapReadModel::project(&AutomapSource {
            cells: vec![MapCell { x: 0, z: 0 }, MapCell { x: 100, z: 100 }],
        });
        assert!(huge.render_ascii().starts_with("(bbox 101x101"));
    }

    #[test]
    fn panel_projects_totals_and_follows_source() {
        let source = PanelSource {
            gold: 100,
            items: vec![("hides".to_string(), 1)],
            quests: vec!["q-intro".to_string()],
        };
        let before = SessionPanelReadModel::project(&source);
        assert_eq!((before.gold, before.item_kinds(), before.total_units()), (100, 1, 1));
        assert_eq!(before.quest_count(), 1);
        let changed = PanelSource {
            gold: 55,
            items: vec![("hides".to_string(), 1), ("potion".to_string(), 1)],
            quests: vec!["q-intro".to_string(), "q-cavern".to_string()],
        };
        let after = SessionPanelReadModel::project(&changed);
        assert_eq!((after.gold, after.item_kinds(), after.quest_count()), (55, 2, 2));
        assert_ne!(before, after);
    }

    #[test]
    fn session_snapshot_projects_all_three_and_title_line() {
        let automap = AutomapSource {
            cells: vec![MapCell { x: 0, z: 0 }, MapCell { x: 1, z: 0 }],
        };
        let panel = PanelSource {
            gold: 100,
            items: vec![("hides".to_string(), 1)],
            quests: vec!["q-intro".to_string()],
        };
        let models = SessionReadModels::project(&hud_source(), &automap, &panel);
        assert_eq!(models.hud.health, 100);
        assert_eq!(models.automap.chunk_count(), 2);
        assert_eq!(models.panel.gold, 100);
        let title = models.title_line();
        assert!(title.contains("HP 100/100"));
        assert!(title.contains("automap: 2 chunks"));
        assert!(title.contains("quests=1"));
        // Identical re-projection (pure, 0 hidden state).
        let again = SessionReadModels::project(&hud_source(), &automap, &panel);
        assert_eq!(models, again);
    }

// ── M34/W34-5: dungeon automap (state per block + ASCII) ───────────────

    fn dungeon_source() -> DungeonAutomapSource {
        DungeonAutomapSource {
            location: 19,
            location_name: "Villagemoor".to_string(),
            time_in_seconds_last_visited: 3600,
            entrance_discovered: true,
            blocks: vec![
                BlockSource { x: 0, z: 0, discovered: true },
                BlockSource { x: 1, z: 0, discovered: true },
                BlockSource { x: 0, z: 1, discovered: false },
                BlockSource { x: 1, z: 1, discovered: true },
            ],
            visited_in_this_run: vec![(0, 0)],
            note_markers: vec![
                NoteMarker { index: 7, text: "door".to_string() },
                NoteMarker { index: 2, text: "stairs".to_string() },
            ],
            teleporter_connections: vec![
                TeleporterConnection { from: 4, to: 9 },
                TeleporterConnection { from: 1, to: 2 },
            ],
        }
    }

    #[test]
    fn ascii_max_side_is_the_gv16_budget() {
        assert_eq!(ASCII_MAX_SIDE, 24);
        // Exactly 24 sides still renders; 25 degrades to the legend.
        let fits = DungeonAutomapSource {
            blocks: (0..24).map(|x| BlockSource { x, z: 0, discovered: true }).collect(),
            ..DungeonAutomapSource::default()
        };
        assert_eq!(DungeonAutomapReadModel::project(&fits).render_ascii().lines().count(), 1);
        let wide = DungeonAutomapSource {
            blocks: (0..25).map(|x| BlockSource { x, z: 0, discovered: true }).collect(),
            ..DungeonAutomapSource::default()
        };
        let rendered = DungeonAutomapReadModel::project(&wide).render_ascii();
        assert!(rendered.starts_with("(bbox 25x1 cells"), "{rendered}");
        assert!(!rendered.contains('#'), "a 25-side map must not render cells");
    }

    #[test]
    fn dungeon_automap_projects_discovery_and_run_state() {
        let model = DungeonAutomapReadModel::project(&dungeon_source());
        assert_eq!(model.location, 19);
        assert_eq!(model.location_name, "Villagemoor");
        assert_eq!(model.time_in_seconds_last_visited, 3600);
        assert!(model.entrance_discovered);
        assert_eq!(model.block_count(), 4);
        assert_eq!(model.blocks_discovered(), 3);
        assert_eq!(model.blocks_visited(), 1);
        assert_eq!(model.blocks_grayscale(), 2, "discovered, not visited this run");
        // Deterministic order (x, then z) with no duplicates.
        let order: Vec<(i32, i32)> = model.blocks.iter().map(|b| (b.x, b.z)).collect();
        assert_eq!(order, vec![(0, 0), (0, 1), (1, 0), (1, 1)]);
        // Re-projection is pure (the source is the only input).
        assert_eq!(model, DungeonAutomapReadModel::project(&dungeon_source()));
    }

    #[test]
    fn block_material_mirrors_the_model_states() {
        let model = DungeonAutomapReadModel::project(&dungeon_source());
        let undiscovered = model.material_of(0, 1).expect("block placed");
        assert_eq!(undiscovered, BlockMaterial { enabled: false, grayscale: false });
        let discovered_only = model.material_of(1, 0).expect("block placed");
        assert_eq!(discovered_only, BlockMaterial { enabled: true, grayscale: true });
        let visited = model.material_of(0, 0).expect("block placed");
        assert_eq!(visited, BlockMaterial { enabled: true, grayscale: false });
        assert!(model.material_of(9, 9).is_none(), "not placed by the grid");
    }

    #[test]
    fn dungeon_ascii_renders_hashes_and_dots_with_highest_z_first() {
        let model = DungeonAutomapReadModel::project(&dungeon_source());
        // Row 0 is z = 1 (highest Z): (0,1) undiscovered, (1,1) discovered.
        assert_eq!(model.render_ascii(), "·#\n##");
    }

    #[test]
    fn dungeon_notes_and_teleporters_are_keyed_and_ordered() {
        let mut source = dungeon_source();
        // A duplicate key rewrites (SortedList is keyed) — last writer wins.
        source.note_markers.push(NoteMarker { index: 2, text: "stairs (new)".to_string() });
        let model = DungeonAutomapReadModel::project(&source);
        let notes: Vec<(i32, &str)> = model
            .note_markers
            .iter()
            .map(|marker| (marker.index, marker.text.as_str()))
            .collect();
        assert_eq!(notes, vec![(2, "stairs (new)"), (7, "door")]);
        let links: Vec<(i32, i32)> = model
            .teleporter_connections
            .iter()
            .map(|link| (link.from, link.to))
            .collect();
        assert_eq!(links, vec![(1, 2), (4, 9)]);
    }

    #[test]
    fn exterior_nameplates_sit_at_depth_four() {
        let source = ExteriorAutomapSource {
            nameplates: vec![
                NameplateSource { location_index: 0, name: "Daggerfall".to_string() },
                NameplateSource { location_index: 1, name: "Gothway Garden".to_string() },
            ],
        };
        let model = ExteriorAutomapReadModel::project(&source);
        assert_eq!(model.nameplates.len(), 2);
        assert!(model.nameplates.iter().all(|plate| plate.depth == 4.0));
        assert_eq!(model.depth(), 4.0);
        assert_eq!(model.hidden_count(), 0);
        assert_eq!(model.nameplates[0].text, "Daggerfall");
    }

    #[test]
    fn colliding_names_are_rasterised_as_the_hidden_glyph() {
        let source = ExteriorAutomapSource {
            nameplates: vec![
                NameplateSource { location_index: 0, name: "Temple".to_string() },
                NameplateSource { location_index: 1, name: "Temple".to_string() },
                NameplateSource { location_index: 2, name: "Bank".to_string() },
            ],
        };
        let model = ExteriorAutomapReadModel::project(&source);
        assert_eq!(model.hidden_count(), 2);
        assert_eq!(model.nameplates[0].text, "*");
        assert_eq!(model.nameplates[1].text, "*");
        assert_eq!(model.nameplates[2].text, "Bank", "unique names stay readable");
        assert_eq!(nameplate_text("Temple", true), "*");
        assert_eq!(nameplate_text("Temple", false), "Temple");
    }
}

// ─────────────────────────────────────────────────────────────────────────
// M39/UI-3 — `m39.*` read models (96): A21 C14 D16 E12 F10 G11 H12
// ─────────────────────────────────────────────────────────────────────────
//
// One typed family per UI block of the M39 census partition. Every route is
// an `Option<T>`: `None` means *absent in this session* (no live datum of that
// kind, e.g. no save on disk), which the family reports as [`M39_ABSENT`];
// when the session itself does not exist, [`M39Models::resolve`] reports
// [`M39_NO_SESSION`] for every declared route. Neither marker is numeric and
// neither is empty, so a consumer can never mistake a missing datum for a
// measured zero.

/// Marker for "the route is declared but there is no session to read it from".
pub const M39_NO_SESSION: &str = "NO SESSION";
/// Marker for "the route is declared, the session exists, and this datum is
/// not present in it" — never `0` and never an empty string.
pub const M39_ABSENT: &str = "ABSENT";

/// Typed admission of one setter value into an M39 route's own type.
///
/// `from_*` returns `None` when the route's type does not match the setter (a
/// text route refuses an integer, a `u16` route refuses `70_000`) and when the
/// value cannot be represented **without clamping or truncation** — the setter
/// contract of M39/UI-3 is "no silent clamp" (plan §3.6), so an out-of-range
/// value is a refusal, never a saturating write. `String` refuses the empty
/// text for the same reason [`M39_ABSENT`] is not empty: an empty read model
/// must not be indistinguishable from a datum that was never written.
pub trait M39Value: Sized {
    /// Admits an integer, `None` when it does not fit the route's own type.
    fn from_i64(value: i64) -> Option<Self>;
    /// Admits a text, `None` for every non-text route and for `""`.
    fn from_text(value: &str) -> Option<Self>;
    /// Admits a flag, `None` for every non-flag route.
    fn from_bool(value: bool) -> Option<Self>;
}

impl M39Value for u8 {
    fn from_i64(value: i64) -> Option<Self> {
        u8::try_from(value).ok()
    }

    fn from_text(_value: &str) -> Option<Self> {
        None
    }

    fn from_bool(_value: bool) -> Option<Self> {
        None
    }
}

impl M39Value for u16 {
    fn from_i64(value: i64) -> Option<Self> {
        u16::try_from(value).ok()
    }

    fn from_text(_value: &str) -> Option<Self> {
        None
    }

    fn from_bool(_value: bool) -> Option<Self> {
        None
    }
}

impl M39Value for i64 {
    fn from_i64(value: i64) -> Option<Self> {
        Some(value)
    }

    fn from_text(_value: &str) -> Option<Self> {
        None
    }

    fn from_bool(_value: bool) -> Option<Self> {
        None
    }
}

impl M39Value for bool {
    fn from_i64(_value: i64) -> Option<Self> {
        None
    }

    fn from_text(_value: &str) -> Option<Self> {
        None
    }

    fn from_bool(value: bool) -> Option<Self> {
        Some(value)
    }
}

impl M39Value for String {
    fn from_i64(_value: i64) -> Option<Self> {
        None
    }

    fn from_text(value: &str) -> Option<Self> {
        (!value.is_empty()).then(|| value.to_string())
    }

    fn from_bool(_value: bool) -> Option<Self> {
        None
    }
}

/// Declares one M39 family: `Option<T>` per route, its `PATHS` and `resolve`.
macro_rules! m39_family {
    ($(#[$meta:meta])* $name:ident, [$(($field:ident, $path:literal, $ty:ty)),* $(,)?]) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Default, PartialEq, Eq)]
        pub struct $name {
            $(
                #[doc = concat!("`", $path, "` — `None` = absent in this session.")]
                pub $field: Option<$ty>,
            )*
        }

        impl $name {
            /// Every route of the family, in declaration order.
            pub const PATHS: &'static [&'static str] = &[$($path),*];

            /// Resolves one declared route.
            ///
            /// `None` = the route does not belong to this family;
            /// `Some(None)` = declared but absent; `Some(Some(text))` = value.
            pub fn resolve(&self, path: &str) -> Option<Option<String>> {
                Some(match path {
                    $($path => self.$field.as_ref().map(|value| value.to_string()),)*
                    _ => return None,
                })
            }

            /// Writes one integer-shaped route of the family.
            ///
            /// `false` = the route is not in this family **or** its type is
            /// not integer-shaped **or** `value` does not fit it. Never
            /// clamps ([`M39Value`]).
            pub fn set_i64(&mut self, path: &str, value: i64) -> bool {
                match path {
                    $(
                        $path => match <$ty as $crate::read_models::M39Value>::from_i64(value) {
                            Some(value) => {
                                self.$field = Some(value);
                                true
                            }
                            None => false,
                        },
                    )*
                    _ => false,
                }
            }

            /// Writes one text-shaped route of the family (`false` = not in
            /// this family, not text-shaped, or `""`).
            pub fn set_text(&mut self, path: &str, value: &str) -> bool {
                match path {
                    $(
                        $path => match <$ty as $crate::read_models::M39Value>::from_text(value) {
                            Some(value) => {
                                self.$field = Some(value);
                                true
                            }
                            None => false,
                        },
                    )*
                    _ => false,
                }
            }

            /// Writes one flag-shaped route of the family (`false` = not in
            /// this family or not flag-shaped).
            pub fn set_bool(&mut self, path: &str, value: bool) -> bool {
                match path {
                    $(
                        $path => match <$ty as $crate::read_models::M39Value>::from_bool(value) {
                            Some(value) => {
                                self.$field = Some(value);
                                true
                            }
                            None => false,
                        },
                    )*
                    _ => false,
                }
            }
        }
    };
}

m39_family!(
    /// Block A — menú, opciones, guardar/cargar, mensajes, entrada, controles,
    /// vídeo (21 routes).
    M39MenuModels,
    [
        (menu_items, "m39.menu.items", u16),
        (menu_selected, "m39.menu.selected", u16),
        (menu_selected_name, "m39.menu.selected.name", String),
        (menu_depth, "m39.menu.depth", u8),
        (settings_scale, "m39.settings.scale", u16),
        (settings_language, "m39.settings.language", String),
        (settings_applied, "m39.settings.applied", bool),
        (saves_count, "m39.saves.count", u16),
        (saves_selected_slot, "m39.saves.selected.slot", u16),
        (saves_selected_name, "m39.saves.selected.name", String),
        (saves_loaded_name, "m39.saves.loaded.name", String),
        (messages_count, "m39.messages.count", u16),
        (messages_selected_prefix, "m39.messages.selected.prefix", String),
        (messages_selected_text, "m39.messages.selected.text", String),
        (messages_decision, "m39.messages.decision", String),
        (input_prompt, "m39.input.prompt", String),
        (input_value, "m39.input.value", String),
        (input_max_len, "m39.input.max_len", u16),
        (controls_binding, "m39.controls.binding", String),
        (controls_conflicts, "m39.controls.conflicts", u16),
        (video_frames, "m39.video.frames", u16),
    ]
);

m39_family!(
    /// Block C — inventario, hoja de personaje (14 routes).
    M39InventoryModels,
    [
        (inv_slots, "m39.inv.slots", u16),
        (inv_used_slots, "m39.inv.used_slots", u16),
        (inv_weight_g, "m39.inv.weight_g", i64),
        (inv_max_weight_g, "m39.inv.max_weight_g", i64),
        (inv_equipped_weapon, "m39.inv.equipped.weapon", String),
        (inv_equipped_armor, "m39.inv.equipped.armor", String),
        (inv_equipped_offhand, "m39.inv.equipped.offhand", String),
        (inv_selected_name, "m39.inv.selected.name", String),
        (inv_action, "m39.inv.action", String),
        (inv_reject_reason, "m39.inv.reject_reason", String),
        (inv_gold, "m39.inv.gold", i64),
        (sheet_slots_filled, "m39.sheet.slots_filled", u16),
        (sheet_level, "m39.sheet.level", u16),
        (sheet_encumbrance_band, "m39.sheet.encumbrance_band", String),
    ]
);

m39_family!(
    /// Block D — comercio, banca, taberna, descanso, reparación (16 routes).
    M39CommerceModels,
    [
        (trade_display_price, "m39.trade.display_price", i64),
        (trade_charged_price, "m39.trade.charged_price", i64),
        (trade_total_display, "m39.trade.total_display", i64),
        (trade_total_charged, "m39.trade.total_charged", i64),
        (trade_rows, "m39.trade.rows", u16),
        (bank_balance, "m39.bank.balance", i64),
        (bank_loan_owed, "m39.bank.loan_owed", i64),
        (bank_purchase_price, "m39.bank.purchase_price", i64),
        (bank_action, "m39.bank.action", String),
        (inn_room_price, "m39.inn.room_price", i64),
        (rest_hours, "m39.rest.hours", u16),
        (rest_cost, "m39.rest.cost", i64),
        (repair_condition, "m39.repair.condition", u16),
        (repair_cost, "m39.repair.cost", i64),
        (merchant_service, "m39.merchant.service", String),
        (merchant_charged, "m39.merchant.charged", i64),
    ]
);

m39_family!(
    /// Block E — libros de magia, creadores, objetos mágicos (12 routes).
    M39MagicModels,
    [
        (spellbook_count, "m39.spellbook.count", u16),
        (spellbook_selected_name, "m39.spellbook.selected.name", String),
        (spellbook_selected_cost, "m39.spellbook.selected.cost", u16),
        (spellmaker_elements, "m39.spellmaker.elements", u16),
        (spellmaker_composed_cost, "m39.spellmaker.composed_cost", u16),
        (spellmaker_persisted, "m39.spellmaker.persisted", bool),
        (itemmaker_cost, "m39.itemmaker.cost", i64),
        (itemmaker_persisted, "m39.itemmaker.persisted", bool),
        (potionmaker_cost, "m39.potionmaker.cost", i64),
        (potionmaker_persisted, "m39.potionmaker.persisted", bool),
        (magicitem_charges, "m39.magicitem.charges", u16),
        (spellicon_selected, "m39.spellicon.selected", u16),
    ]
);

m39_family!(
    /// Block F — diario, notas, libro, historial (10 routes).
    M39JournalModels,
    [
        (journal_entries, "m39.journal.entries", u16),
        (journal_selected_text, "m39.journal.selected.text", String),
        (journal_line_chars, "m39.journal.line_chars", u16),
        (journal_line_limit, "m39.journal.line_limit", u16),
        (journal_prefixes, "m39.journal.prefixes", u16),
        (notes_count, "m39.notes.count", u16),
        (notes_persisted, "m39.notes.persisted", bool),
        (book_pages, "m39.book.pages", u16),
        (book_page_index, "m39.book.page_index", u16),
        (history_entries, "m39.history.entries", u16),
    ]
);

m39_family!(
    /// Block G — automap, viaje, transporte, teletransporte, tiempo
    /// (11 routes).
    M39TravelModels,
    [
        (automap_chunks, "m39.automap.chunks", u16),
        (automap_markers, "m39.automap.markers", u16),
        (automap_dungeon_blocks, "m39.automap.dungeon_blocks", u16),
        (travel_regions, "m39.travel.regions", u16),
        (travel_selected_region, "m39.travel.selected_region", String),
        (travel_cost_days, "m39.travel.cost_days", u16),
        (travel_cost_gold, "m39.travel.cost_gold", i64),
        (transport_fare, "m39.transport.fare", i64),
        (transport_time_hours, "m39.transport.time_hours", u16),
        (teleport_destination, "m39.teleport.destination", String),
        (time_day, "m39.time.day", u16),
    ]
);

m39_family!(
    /// Block H — conversación, servicios, encargos, corte, Daedra, coven
    /// (12 routes).
    M39SocialModels,
    [
        (talk_topics, "m39.talk.topics", u16),
        (talk_selected_topic, "m39.talk.selected.topic", String),
        (talk_answer, "m39.talk.answer", String),
        (talk_faction, "m39.talk.faction", String),
        (service_kind, "m39.service.kind", String),
        (service_charged, "m39.service.charged", i64),
        (service_executed, "m39.service.executed", bool),
        (quest_offers, "m39.quest.offers", u16),
        (quest_decision, "m39.quest.decision", String),
        (court_ruling, "m39.court.ruling", String),
        (daedra_summoned, "m39.daedra.summoned", String),
        (coven_service, "m39.coven.service", String),
    ]
);

/// The seven M39 route families plus the session flag.
///
/// Order of the route space is A, C, D, E, F, G, H — the same order as
/// [`crate::read_models`]'s `READ_MODEL_PATHS` suffix in
/// `parser-bevy-bridge::ui_compat::text`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct M39Models {
    /// Whether a live UI session exists. `false` (the default) makes every
    /// declared route resolve to [`M39_NO_SESSION`].
    pub session: bool,
    /// A — menú/opciones/guardar-cargar/mensajes/entrada/controles/vídeo.
    pub a: M39MenuModels,
    /// C — inventario y hoja.
    pub c: M39InventoryModels,
    /// D — comercio/banca/taberna/descanso/reparación.
    pub d: M39CommerceModels,
    /// E — magia.
    pub e: M39MagicModels,
    /// F — diario/notas/libro/historial.
    pub f: M39JournalModels,
    /// G — automap/viaje/transporte/tiempo.
    pub g: M39TravelModels,
    /// H — conversación/servicios/encargos/corte/cultos.
    pub h: M39SocialModels,
}

impl M39Models {
    /// Every declared M39 route in block order (A, C, D, E, F, G, H) — 96.
    pub fn paths() -> Vec<&'static str> {
        let mut paths = Vec::new();
        for family in [
            M39MenuModels::PATHS,
            M39InventoryModels::PATHS,
            M39CommerceModels::PATHS,
            M39MagicModels::PATHS,
            M39JournalModels::PATHS,
            M39TravelModels::PATHS,
            M39SocialModels::PATHS,
        ] {
            paths.extend_from_slice(family);
        }
        paths
    }

    /// Typed lookup: `None` = not a declared M39 route, `Some(None)` =
    /// declared but absent, `Some(Some(text))` = value.
    pub fn value(&self, path: &str) -> Option<Option<String>> {
        self.a
            .resolve(path)
            .or_else(|| self.c.resolve(path))
            .or_else(|| self.d.resolve(path))
            .or_else(|| self.e.resolve(path))
            .or_else(|| self.f.resolve(path))
            .or_else(|| self.g.resolve(path))
            .or_else(|| self.h.resolve(path))
    }

    /// Resolves one declared route to its text, applying the two markers.
    ///
    /// `None` is reserved for a route this family does not declare, so a
    /// declared-but-missing datum can never be confused with an unknown path.
    pub fn resolve(&self, path: &str) -> Option<String> {
        let value = self.value(path)?;
        if !self.session {
            return Some(M39_NO_SESSION.to_string());
        }
        Some(value.unwrap_or_else(|| M39_ABSENT.to_string()))
    }

    /// Writes one integer-shaped route of any family.
    ///
    /// `false` = undeclared route, non-integer route, or a value that does not
    /// fit the route's own type. Nothing is clamped (plan §3.6): the caller
    /// must reject, never truncate.
    pub fn set_i64(&mut self, path: &str, value: i64) -> bool {
        self.a.set_i64(path, value)
            || self.c.set_i64(path, value)
            || self.d.set_i64(path, value)
            || self.e.set_i64(path, value)
            || self.f.set_i64(path, value)
            || self.g.set_i64(path, value)
            || self.h.set_i64(path, value)
    }

    /// Writes one text-shaped route of any family (`false` = undeclared route,
    /// non-text route, or empty text).
    pub fn set_text(&mut self, path: &str, value: &str) -> bool {
        self.a.set_text(path, value)
            || self.c.set_text(path, value)
            || self.d.set_text(path, value)
            || self.e.set_text(path, value)
            || self.f.set_text(path, value)
            || self.g.set_text(path, value)
            || self.h.set_text(path, value)
    }

    /// Writes one flag-shaped route of any family (`false` = undeclared route
    /// or non-flag route).
    pub fn set_bool(&mut self, path: &str, value: bool) -> bool {
        self.a.set_bool(path, value)
            || self.c.set_bool(path, value)
            || self.d.set_bool(path, value)
            || self.e.set_bool(path, value)
            || self.f.set_bool(path, value)
            || self.g.set_bool(path, value)
            || self.h.set_bool(path, value)
    }
}

#[cfg(test)]
mod m39_tests {
    use super::*;

    #[test]
    fn m39_declares_96_routes_in_block_order() {
        let paths = M39Models::paths();
        assert_eq!(paths.len(), 96);
        assert_eq!(M39MenuModels::PATHS.len(), 21);
        assert_eq!(M39InventoryModels::PATHS.len(), 14);
        assert_eq!(M39CommerceModels::PATHS.len(), 16);
        assert_eq!(M39MagicModels::PATHS.len(), 12);
        assert_eq!(M39JournalModels::PATHS.len(), 10);
        assert_eq!(M39TravelModels::PATHS.len(), 11);
        assert_eq!(M39SocialModels::PATHS.len(), 12);
        // Block order: every route's prefix belongs to a UI block and the
        // blocks appear in A, C, D, E, F, G, H order; every route is unique.
        let mut seen = std::collections::BTreeSet::new();
        let mut last = 0usize;
        for path in &paths {
            assert!(seen.insert(*path), "duplicate route {path}");
            let mut segments = path.split('.');
            assert_eq!(segments.next(), Some("m39"), "{path} is not an m39 route");
            let prefix = segments.next().expect("m39.<prefix>");
            let block = block_of(prefix)
                .unwrap_or_else(|| panic!("{path}: prefix {prefix} belongs to no block"));
            let index = M39_BLOCK_ORDER
                .iter()
                .position(|candidate| *candidate == block)
                .expect("declared block");
            assert!(index >= last, "{path} breaks the A,C,D,E,F,G,H order");
            last = index;
        }
    }

    /// The seven route families in [`M39Models::paths`] order.
    const M39_BLOCK_ORDER: [char; 7] = ['A', 'C', 'D', 'E', 'F', 'G', 'H'];

    /// UI block of one `m39.<prefix>` route (plan §2 partition / §5 routes).
    fn block_of(prefix: &str) -> Option<char> {
        Some(match prefix {
            "menu" | "settings" | "saves" | "messages" | "input" | "controls" | "video" => 'A',
            "inv" | "sheet" => 'C',
            "trade" | "bank" | "inn" | "rest" | "repair" | "merchant" => 'D',
            "spellbook" | "spellmaker" | "itemmaker" | "potionmaker" | "magicitem"
            | "spellicon" => 'E',
            "journal" | "notes" | "book" | "history" => 'F',
            "automap" | "travel" | "transport" | "teleport" | "time" => 'G',
            "talk" | "service" | "quest" | "court" | "daedra" | "coven" => 'H',
            _ => return None,
        })
    }

    #[test]
    fn m39_markers_are_not_numeric_and_resolve_never_empty() {
        // No session: every declared route says so.
        let no_session = M39Models::default();
        for path in M39Models::paths() {
            assert_eq!(no_session.resolve(path).as_deref(), Some(M39_NO_SESSION));
        }
        // Session without data: every route is ABSENT, never 0 or "".
        let empty_session = M39Models {
            session: true,
            ..M39Models::default()
        };
        for path in M39Models::paths() {
            let text = empty_session.resolve(path).expect("declared");
            assert_eq!(text, M39_ABSENT);
            assert!(!text.trim().is_empty());
            assert!(text.parse::<i64>().is_err(), "{path} resolved numeric");
        }
        // An undeclared route stays undeclared in both states.
        assert_eq!(no_session.resolve("m39.nope"), None);
        assert_eq!(empty_session.value("m39.nope"), None);
    }

    #[test]
    fn m39_values_follow_the_fields() {
        let models = M39Models {
            session: true,
            a: M39MenuModels {
                menu_items: Some(5),
                menu_selected: Some(2),
                menu_selected_name: Some("DaggerfallPauseOptionsWindow".to_string()),
                settings_applied: Some(false),
                ..M39MenuModels::default()
            },
            c: M39InventoryModels {
                inv_weight_g: Some(12_500),
                ..M39InventoryModels::default()
            },
            ..M39Models::default()
        };
        assert_eq!(models.resolve("m39.menu.items").as_deref(), Some("5"));
        assert_eq!(models.resolve("m39.menu.selected").as_deref(), Some("2"));
        assert_eq!(
            models.resolve("m39.menu.selected.name").as_deref(),
            Some("DaggerfallPauseOptionsWindow")
        );
        assert_eq!(models.resolve("m39.settings.applied").as_deref(), Some("false"));
        assert_eq!(models.resolve("m39.inv.weight_g").as_deref(), Some("12500"));
        assert_eq!(models.resolve("m39.saves.count").as_deref(), Some(M39_ABSENT));
    }

    #[test]
    fn m39_setters_round_trip_every_declared_route() {
        // Every one of the 96 routes accepts exactly one setter shape and
        // `resolve` hands the written value back verbatim.
        for path in M39Models::paths() {
            let mut models = M39Models {
                session: true,
                ..M39Models::default()
            };
            let text = models.set_text(path, "probe");
            let flag = models.set_bool(path, true);
            let number = models.set_i64(path, 7);
            let accepted =
                usize::from(text) + usize::from(flag) + usize::from(number);
            assert_eq!(accepted, 1, "{path} accepts {accepted} setter shapes");
            let expected = if text {
                "probe"
            } else if flag {
                "true"
            } else {
                "7"
            };
            assert_eq!(models.resolve(path).as_deref(), Some(expected), "{path}");
        }
    }

    #[test]
    fn m39_setters_refuse_out_of_range_types_and_empty_text() {
        let mut models = M39Models {
            session: true,
            ..M39Models::default()
        };
        // `u8` route: 256 does not fit and must not wrap to 0 (no clamp).
        assert!(!models.set_i64("m39.menu.depth", 256));
        assert!(!models.set_i64("m39.menu.depth", -1));
        assert_eq!(models.resolve("m39.menu.depth").as_deref(), Some(M39_ABSENT));
        assert!(models.set_i64("m39.menu.depth", 255));
        assert_eq!(models.resolve("m39.menu.depth").as_deref(), Some("255"));
        // `u16` route: 65_536 does not fit, an `i64` route takes any `i64`.
        assert!(!models.set_i64("m39.menu.items", 65_536));
        assert!(models.set_i64("m39.inv.weight_g", i64::MAX));
        assert_eq!(
            models.resolve("m39.inv.weight_g").as_deref(),
            Some(i64::MAX.to_string().as_str())
        );
        // Shape mismatches.
        assert!(!models.set_text("m39.menu.items", "5"));
        assert!(!models.set_bool("m39.menu.items", true));
        assert!(!models.set_i64("m39.menu.selected.name", 5));
        // `String` refuses the empty text (never a datum that looks absent).
        assert!(!models.set_text("m39.menu.selected.name", ""));
        assert!(models.set_text("m39.menu.selected.name", "DaggerfallStartWindow"));
        assert_eq!(
            models.resolve("m39.menu.selected.name").as_deref(),
            Some("DaggerfallStartWindow")
        );
        // Flags take only flags.
        assert!(models.set_bool("m39.settings.applied", true));
        assert_eq!(models.resolve("m39.settings.applied").as_deref(), Some("true"));
        assert!(!models.set_i64("m39.settings.applied", 1));
        // Undeclared route: every setter refuses.
        assert!(!models.set_i64("m39.nope", 1));
        assert!(!models.set_text("m39.nope", "x"));
        assert!(!models.set_bool("m39.nope", true));
    }
}

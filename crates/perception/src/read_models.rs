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
/// — a giant grid is not legible evidence).
const ASCII_MAX_SIDE: i32 = 24;

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
    /// [`ASCII_MAX_SIDE`] it returns only the legend (legibility).
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
}

impl SessionReadModels {
    /// Joint projection of the three read models from their ECS sources.
    pub fn project(hud: &HudSource, automap: &AutomapSource, panel: &PanelSource) -> Self {
        Self {
            hud: HudReadModel::project(hud),
            automap: AutomapReadModel::project(automap),
            panel: SessionPanelReadModel::project(panel),
        }
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
}

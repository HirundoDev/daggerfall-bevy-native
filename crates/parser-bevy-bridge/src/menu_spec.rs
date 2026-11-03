//! **Modern menu as a component model** — `MENU-MODERN-DESIGN.md` §5 / §9.2.
//!
//! One 320×180 canvas, four zone primitives (`Nav`, `Steps`, `Choices`, `Detail`)
//! and one plate grammar. A screen is **data**: [`MenuScreen`] carries its chrome,
//! its plates and its zones, and [`MenuScreen::compose`] paints them through the
//! single compositor of [`crate::ui_spec`] (`Fill` · `Sprite` · `GlyphRun`). A
//! second screen is a second [`MenuScreen`] value — no renderer change:
//! [`options_stub`] is exactly that (`docs/sintergic/design/MENU-MODERN-DESIGN.md`
//! §9.2, "añadir un menú = añadir datos").
//!
//! # Content rect is **derived**, never declared
//!
//! A plate declares its chrome (outer border). Its content rect is
//! `chrome + stroke + PAD` = inset **9** px, and every node painted by a zone is
//! validated against that derived rect while it is emitted: a node whose margin
//! to the plate chrome is `< 9` px aborts with [`MenuError::OutOfContent`]
//! carrying the measured margin. The assertion compares the **drawing** against
//! the **real border**, not the rect the node declared itself — the tautological
//! assert that let the 2026-10-31 overflow through.
//!
//! # Pixel contract
//!
//! The reference is `docs/sintergic/design/mockups/main-menu-320x180.png`
//! (regenerated from `_gen/render.py`; `sha256`
//! `c50e77b10c3b8ee12cef9c3a1b3e37594966be6748a103414c5d3b2eec60853f`). The paint
//! order here is the one the generator uses — field → plate chrome → lintel →
//! zones → footer — so `present_fullscreen(fb, 4)` reproduces the 1280×720 mockup
//! pixel for pixel.

use daggerfall_parser_rs::image::PaletteFamily;

use crate::ui_spec::{
    fill_node, glyph_node_tracked, sprite_node_at, UiAssets, UiNode, UiPanel, UiPanelId, UiRect,
    UiSpecError,
};

/// Canvas width of the modern menu (design px; presented ×4 = 1280).
pub const MENU_WIDTH: u16 = 320;
/// Canvas height of the modern menu (design px; presented ×4 = 720).
pub const MENU_HEIGHT: u16 = 180;
/// Outer border of a plate.
pub const STROKE: u16 = 1;
/// Padding between the border and the content of a plate.
pub const PAD: u16 = 8;
/// `STROKE + PAD` — the inset every painted node of a plate must respect.
pub const INSET: u16 = STROKE + PAD;
/// Minimum margin, in px, between a painted node and its plate chrome.
pub const MIN_MARGIN: i16 = 9;
/// Symmetric air of the text block inside a row.
pub const ROW_PAD_X: i16 = 14;
/// Display font (`14×11`) — the lintel title.
pub const FONT_DISPLAY: &str = "FONT0000.FNT";
/// Micro font (`5×7`) — every other run.
pub const FONT_MICRO: &str = "FONT0003.FNT";
/// Portrait asset of the detail plate (61 × 64×64 RCI, no header).
pub const PORTRAIT_ASSET: &str = "FACES.CIF";
/// The portrait frame the design declares for the main menu detail plate.
pub const PORTRAIT_FRAME: usize = 3;
/// Panel name used in compositor errors.
pub const MENU_PANEL: &str = "menu";
/// Footer version string (the mockup's `v0.1.0`).
pub const MENU_VERSION: &str = concat!("v", env!("CARGO_PKG_VERSION"));

/// Palette indices of the design tokens (`PAL.PAL`, measured — DESIGN.md §3).
pub mod ink {
    /// Stone field — the canvas background.
    pub const FIELD: u8 = 221;
    /// Plate paper.
    pub const PAPER: u8 = 112;
    /// Inner rule / secondary ink on paper.
    pub const PAPER_SHADE: u8 = 81;
    /// Top/bottom cap of the retail plate.
    pub const OLIVE: u8 = 78;
    /// Side edge of the retail plate.
    pub const LAVENDER: u8 = 82;
    /// Ink on paper and focus bar.
    pub const INK: u8 = 111;
    /// Secondary ink on paper.
    pub const INK_MUTED: u8 = 90;
    /// Editorial ink on paper (disabled rows).
    pub const SLATE: u8 = 107;
    /// Interface gold on stone and on the bar.
    pub const GOLD: u8 = 227;
    /// Gold on paper (5.98:1).
    pub const GOLD_DEEP: u8 = 156;
    /// Semantic brick — marks only.
    pub const BRICK: u8 = 179;
}

/// Rows of the main menu, in order (the ids the shell routes).
pub const MENU_ROW_IDS: [&str; 5] = ["new-game", "continue", "load", "options", "quit"];
/// Labels of the main menu rows, in order.
pub const MENU_ROW_LABELS: [&str; 5] = ["NEW GAME", "CONTINUE", "LOAD", "OPTIONS", "QUIT"];
/// Row of `CONTINUE` — the only one with a state driven by the save store.
pub const CONTINUE_ROW: usize = 1;
/// Value a disabled `CONTINUE` shows.
pub const NO_SAVES: &str = "NO SAVES";
/// Default active row of the shell.
pub const DEFAULT_ACTIVE_ROW: usize = 0;
/// Screen id of the main menu.
pub const MAIN_MENU_ID: &str = "main-plates";
/// Screen id of the options stub.
pub const OPTIONS_ID: &str = "options";

const NAV_PLATE: &str = "nav_plate";
const DETAIL_PLATE: &str = "detail_plate";
const NAV_RECT: (i16, i16, u16, u16) = (25, 41, 178, 100);
const DETAIL_RECT: (i16, i16, u16, u16) = (229, 41, 66, 100);
const ROW_STRIDE: i16 = 17;
const ROW_HEIGHT: u16 = 14;

/// State of a navigation row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowState {
    /// Active row: focus bar, gold rail and chevron.
    Focus,
    /// Selectable, not active.
    Idle,
    /// Not selectable: dash marker, parenthesised value, no highlight.
    Disabled,
}

/// State of a step row (`Steps` zone).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepState {
    /// Walked step: bullet + deep gold.
    Done,
    /// Current step: focus bar + gold rail.
    Current,
    /// Not reached yet.
    Pending,
}

/// One row of a `Nav` or `Choices` zone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// Stable id (`new-game`) — also the node-id stem.
    pub id: String,
    /// Visible label.
    pub label: String,
    /// Value at the right edge (`NO SAVES`); empty = no value.
    pub value: String,
    /// Row state.
    pub state: RowState,
}

/// One row of a `Steps` zone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepRow {
    /// Stable id.
    pub id: String,
    /// Visible label.
    pub label: String,
    /// Step state.
    pub state: StepState,
}

/// Plate chrome: outer rect; content is derived ([`Plate::content`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plate {
    /// Plate id (`nav_plate`) — node-id stem and zone reference.
    pub id: &'static str,
    /// Outer rect (the chrome).
    pub rect: UiRect,
}

impl Plate {
    /// Content rect **derived** from the chrome: inset by [`INSET`].
    pub fn content(&self) -> UiRect {
        UiRect::new(
            self.rect.x + INSET as i16,
            self.rect.y + INSET as i16,
            self.rect.width - 2 * INSET,
            self.rect.height - 2 * INSET,
        )
    }
}

/// Zone 1 — selectable rows (main menu, options, any list).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavZone {
    /// Zone id (node-id stem).
    pub id: &'static str,
    /// Plate this zone lives in.
    pub plate: &'static str,
    /// Content rect inside the plate (validated against [`Plate::content`]).
    pub rect: UiRect,
    /// Eyebrow with its own rule, or `None` for a bare list.
    pub eyebrow: Option<String>,
    /// Vertical distance between rows.
    pub stride: i16,
    /// Row height.
    pub height: u16,
    /// Rows, top to bottom.
    pub rows: Vec<Row>,
}

impl NavZone {
    /// Rect of row `index`.
    pub fn row_rect(&self, index: usize) -> UiRect {
        let top = self.rect.y + if self.eyebrow.is_some() { 14 } else { 0 };
        UiRect::new(
            self.rect.x,
            top + index as i16 * self.stride,
            self.rect.width,
            self.height,
        )
    }
}

/// Zone 2 — ordered steps with a progress reading (`character` screen).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepsZone {
    /// Zone id.
    pub id: &'static str,
    /// Plate this zone lives in.
    pub plate: &'static str,
    /// Content rect.
    pub rect: UiRect,
    /// Eyebrow with its own rule.
    pub eyebrow: String,
    /// Vertical distance between rows.
    pub stride: i16,
    /// Row height.
    pub height: u16,
    /// Steps, in order (numbering starts at `01`).
    pub rows: Vec<StepRow>,
}

impl StepsZone {
    /// Rect of step `index` (0-based).
    pub fn row_rect(&self, index: usize) -> UiRect {
        UiRect::new(
            self.rect.x,
            self.rect.y + 12 + index as i16 * self.stride,
            self.rect.width,
            self.height,
        )
    }
}

/// Zone 3 — mutually exclusive choices with an eyebrow (`character` screen).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoicesZone {
    /// Zone id.
    pub id: &'static str,
    /// Plate this zone lives in.
    pub plate: &'static str,
    /// Content rect.
    pub rect: UiRect,
    /// Eyebrow with its own rule.
    pub eyebrow: String,
    /// Vertical distance between rows.
    pub stride: i16,
    /// Row height.
    pub height: u16,
    /// Rows, top to bottom.
    pub rows: Vec<Row>,
}

impl ChoicesZone {
    /// Rect of row `index`.
    pub fn row_rect(&self, index: usize) -> UiRect {
        UiRect::new(
            self.rect.x,
            self.rect.y + 16 + index as i16 * self.stride,
            self.rect.width,
            self.height,
        )
    }
}

/// Zone 4 — content panel: header + rule + optional portrait + key/value rows
/// (+ optional progress bar). The **only** panel component of the system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetailZone {
    /// Zone id.
    pub id: &'static str,
    /// Plate this zone lives in.
    pub plate: &'static str,
    /// Content rect.
    pub rect: UiRect,
    /// Header run (tracking +1).
    pub header: String,
    /// `FACES.CIF` frame shown as a 64×64 portrait, or `None`.
    pub portrait: Option<usize>,
    /// Key/value rows.
    pub rows: Vec<(String, String)>,
    /// Optional progress: `(label, filled percent 0..=100)`.
    pub progress: Option<(String, u8)>,
}

/// The four zone primitives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Zone {
    /// Selectable rows.
    Nav(NavZone),
    /// Ordered steps.
    Steps(StepsZone),
    /// Mutually exclusive choices.
    Choices(ChoicesZone),
    /// Content panel.
    Detail(DetailZone),
}

impl Zone {
    /// Zone id.
    pub fn id(&self) -> &'static str {
        match self {
            Self::Nav(zone) => zone.id,
            Self::Steps(zone) => zone.id,
            Self::Choices(zone) => zone.id,
            Self::Detail(zone) => zone.id,
        }
    }

    /// Plate the zone lives in.
    pub fn plate(&self) -> &'static str {
        match self {
            Self::Nav(zone) => zone.plate,
            Self::Steps(zone) => zone.plate,
            Self::Choices(zone) => zone.plate,
            Self::Detail(zone) => zone.plate,
        }
    }

    /// Content rect the zone declares for itself.
    pub fn rect(&self) -> UiRect {
        match self {
            Self::Nav(zone) => zone.rect,
            Self::Steps(zone) => zone.rect,
            Self::Choices(zone) => zone.rect,
            Self::Detail(zone) => zone.rect,
        }
    }
}

/// Screen chrome: lintel (title + right run) and footer (hints + version).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuChrome {
    /// Lintel title (`DAGGERFALL`, display font).
    pub title: String,
    /// Lintel right run (`CHAPTER II`, micro font, no tracking).
    ///
    /// Tracking is `0`, not `+1`: `text_width_tracked` adds `tracking` **per
    /// glyph** to the advance, so `+1` moved the run 10 canvas px left of the
    /// sealed mockup (`render.py::lintel` → `glyphs_right(..., 304, ...)` with
    /// its default tracking). The mockup is the reference, so the composition
    /// matches it: right edge at canvas 304, first ink column at 262.
    pub right: String,
    /// Footer hints, `(key, action)`.
    pub hints: Vec<(String, String)>,
    /// Footer right run (version).
    pub version: String,
}

/// One screen of the modern menu — data, not code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuScreen {
    /// Screen id (`main-plates`).
    pub id: &'static str,
    /// Lintel + footer data.
    pub chrome: MenuChrome,
    /// Plates in painting order.
    pub plates: Vec<Plate>,
    /// Zones in painting order.
    pub zones: Vec<Zone>,
    /// Panel id this screen composes to.
    pub panel: UiPanelId,
}

/// Why a screen could not be composed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuError {
    /// The compositor rejected a node.
    Ui(UiSpecError),
    /// A node or plate falls outside the 320×180 canvas.
    OutOfCanvas {
        /// Screen id.
        screen: &'static str,
        /// Node id.
        node: String,
        /// Offending rect.
        rect: UiRect,
    },
    /// A node's margin to its plate chrome is below [`MIN_MARGIN`].
    OutOfContent {
        /// Screen id.
        screen: &'static str,
        /// Node id.
        node: String,
        /// Node rect.
        rect: UiRect,
        /// Derived content rect of the plate.
        limit: UiRect,
        /// Measured margin in px (negative = the node leaves the rect).
        margin: i16,
    },
    /// A zone names a plate the screen does not declare.
    UnknownPlate {
        /// Screen id.
        screen: &'static str,
        /// Plate id.
        plate: &'static str,
    },
    /// A zone's own rect does not fit its plate content.
    ZoneOutOfContent {
        /// Screen id.
        screen: &'static str,
        /// Zone id.
        zone: &'static str,
        /// Zone rect.
        rect: UiRect,
        /// Derived content rect of the plate.
        limit: UiRect,
    },
}

impl std::fmt::Display for MenuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ui(err) => write!(f, "compositor: {err}"),
            Self::OutOfCanvas { screen, node, rect } => write!(
                f,
                "{screen}/{node}: {:?} outside the {MENU_WIDTH}×{MENU_HEIGHT} canvas",
                rect
            ),
            Self::OutOfContent {
                screen,
                node,
                rect,
                limit,
                margin,
            } => write!(
                f,
                "{screen}/{node}: {:?} with margin {margin} < {MIN_MARGIN} to its plate content {limit:?}",
                rect
            ),
            Self::UnknownPlate { screen, plate } => {
                write!(f, "{screen}: zone names plate `{plate}`, not declared")
            }
            Self::ZoneOutOfContent {
                screen,
                zone,
                rect,
                limit,
            } => write!(
                f,
                "{screen}/{zone}: zone {:?} does not fit its plate content {limit:?}",
                rect
            ),
        }
    }
}

impl std::error::Error for MenuError {}

impl From<UiSpecError> for MenuError {
    fn from(err: UiSpecError) -> Self {
        Self::Ui(err)
    }
}

/// Margin, in px, between `inner` and `outer` (negative when it leaves).
fn margin(outer: &UiRect, inner: &UiRect) -> i16 {
    let left = inner.x - outer.x;
    let top = inner.y - outer.y;
    let right = outer.right() - inner.right();
    let bottom = outer.bottom() - inner.bottom();
    left.min(top).min(right).min(bottom)
}

/// `inner` fully inside `outer`.
fn contains(outer: &UiRect, inner: &UiRect) -> bool {
    margin(outer, inner) >= 0
}

/// The main menu of the modern design (`main-plates`).
///
/// `active_row` is the row the shell has selected (`0..5`); `continue_enabled`
/// is whether a save exists — the only state the plate reads from the outside.
pub fn main_menu(active_row: usize, continue_enabled: bool) -> MenuScreen {
    let active = active_row.min(MENU_ROW_LABELS.len() - 1);
    let rows = MENU_ROW_IDS
        .iter()
        .zip(MENU_ROW_LABELS)
        .enumerate()
        .map(|(index, (id, label))| {
            let disabled = index == CONTINUE_ROW && !continue_enabled;
            Row {
                id: (*id).to_string(),
                label: label.to_string(),
                value: if index == CONTINUE_ROW && !continue_enabled {
                    NO_SAVES.to_string()
                } else {
                    String::new()
                },
                state: if index == active {
                    RowState::Focus
                } else if disabled {
                    RowState::Disabled
                } else {
                    RowState::Idle
                },
            }
        })
        .collect();
    MenuScreen {
        id: MAIN_MENU_ID,
        chrome: MenuChrome {
            title: "DAGGERFALL".to_string(),
            right: "CHAPTER II".to_string(),
            hints: vec![
                ("UP/DOWN".to_string(), "MOVE".to_string()),
                ("ENTER".to_string(), "SELECT".to_string()),
            ],
            version: MENU_VERSION.to_string(),
        },
        plates: vec![nav_plate(), detail_plate()],
        zones: vec![
            Zone::Nav(NavZone {
                id: "nav",
                plate: NAV_PLATE,
                rect: UiRect::new(NAV_RECT.0, NAV_RECT.1, NAV_RECT.2, NAV_RECT.3),
                eyebrow: Some("SELECT DESTINATION".to_string()),
                stride: ROW_STRIDE,
                height: ROW_HEIGHT,
                rows,
            }),
            Zone::Detail(DetailZone {
                id: "detail",
                plate: DETAIL_PLATE,
                rect: UiRect::new(DETAIL_RECT.0, DETAIL_RECT.1, DETAIL_RECT.2, DETAIL_RECT.3),
                header: MENU_ROW_LABELS[active].to_string(),
                portrait: Some(PORTRAIT_FRAME),
                rows: vec![
                    ("PORTRAITS".to_string(), "16".to_string()),
                    ("STEPS".to_string(), "10".to_string()),
                ],
                progress: None,
            }),
        ],
        panel: UiPanelId::MainMenu,
    }
}

/// Second screen created **only with data** — the component model proof.
///
/// Same [`compose`](MenuScreen::compose), same plate grammar, same zone
/// primitives: two [`Row`]s in a [`NavZone`] and nothing else. The renderer is
/// untouched by this screen's existence.
pub fn options_stub() -> MenuScreen {
    MenuScreen {
        id: OPTIONS_ID,
        chrome: MenuChrome {
            title: "DAGGERFALL".to_string(),
            right: "CHAPTER II".to_string(),
            hints: vec![
                ("UP/DOWN".to_string(), "MOVE".to_string()),
                ("ESC".to_string(), "BACK".to_string()),
            ],
            version: MENU_VERSION.to_string(),
        },
        plates: vec![nav_plate()],
        zones: vec![Zone::Nav(NavZone {
            id: "nav",
            plate: NAV_PLATE,
            rect: nav_plate().content(),
            eyebrow: Some("OPTIONS".to_string()),
            stride: ROW_STRIDE,
            height: ROW_HEIGHT,
            rows: vec![
                Row {
                    id: "music".to_string(),
                    label: "MUSIC".to_string(),
                    value: "ON".to_string(),
                    state: RowState::Focus,
                },
                Row {
                    id: "sound".to_string(),
                    label: "SOUND".to_string(),
                    value: "OFF".to_string(),
                    state: RowState::Idle,
                },
            ],
        })],
        panel: UiPanelId::Options,
    }
}

fn nav_plate() -> Plate {
    Plate {
        id: NAV_PLATE,
        rect: UiRect::new(16, 32, 196, 118),
    }
}

fn detail_plate() -> Plate {
    Plate {
        id: DETAIL_PLATE,
        rect: UiRect::new(220, 32, 84, 118),
    }
}

impl MenuScreen {
    /// Paints the screen and returns its panel.
    ///
    /// # Errors
    ///
    /// [`MenuError`] when a node leaves its plate content (margin `< 9`), leaves
    /// the canvas, names an undeclared plate, or the compositor rejects it.
    pub fn compose(&self, assets: &UiAssets) -> Result<UiPanel, MenuError> {
        let mut painter = Painter {
            screen: self,
            assets,
            nodes: Vec::new(),
            limits: Vec::new(),
        };
        painter.field()?;
        for plate in &self.plates {
            painter.plate(plate)?;
        }
        for zone in &self.zones {
            let limit = painter.limit(zone.plate())?;
            if !contains(&limit, &zone.rect()) {
                return Err(MenuError::ZoneOutOfContent {
                    screen: self.id,
                    zone: zone.id(),
                    rect: zone.rect(),
                    limit,
                });
            }
        }
        painter.lintel()?;
        for zone in &self.zones {
            painter.zone(zone)?;
        }
        painter.footer()?;
        Ok(UiPanel {
            id: self.panel,
            nodes: painter.nodes,
        })
    }

    /// Content rect of the plate `id`, as the assertion derives it.
    pub fn content_of(&self, id: &str) -> Option<UiRect> {
        self.plates
            .iter()
            .find(|plate| plate.id == id)
            .map(Plate::content)
    }

    /// `(row id, row rect)` of the `Nav` zone `id`, in order — what a probe
    /// measures the focus bar and the margins on.
    pub fn nav_rows(&self, id: &str) -> Vec<(String, UiRect)> {
        self.zones
            .iter()
            .find_map(|zone| match zone {
                Zone::Nav(nav) if nav.id == id => Some(
                    nav.rows
                        .iter()
                        .enumerate()
                        .map(|(index, row)| (row.id.clone(), nav.row_rect(index)))
                        .collect(),
                ),
                _ => None,
            })
            .unwrap_or_default()
    }
}

struct Painter<'a> {
    screen: &'a MenuScreen,
    assets: &'a UiAssets,
    nodes: Vec<UiNode>,
    limits: Vec<(&'static str, UiRect)>,
}

impl Painter<'_> {
    fn limit(&self, plate: &'static str) -> Result<UiRect, MenuError> {
        self.limits
            .iter()
            .find(|(id, _)| *id == plate)
            .map(|(_, rect)| *rect)
            .ok_or(MenuError::UnknownPlate {
                screen: self.screen.id,
                plate,
            })
    }

    /// Validates a rect against the canvas and — when the node belongs to a
    /// plate — against that plate's **derived** content rect.
    fn guard(&self, id: &str, rect: UiRect, plate: Option<&'static str>) -> Result<(), MenuError> {
        if rect.x < 0
            || rect.y < 0
            || rect.right() > MENU_WIDTH as i16
            || rect.bottom() > MENU_HEIGHT as i16
            || rect.width == 0
            || rect.height == 0
        {
            return Err(MenuError::OutOfCanvas {
                screen: self.screen.id,
                node: id.to_string(),
                rect,
            });
        }
        if let Some(plate) = plate {
            let limit = self.limit(plate)?;
            if !contains(&limit, &rect) {
                return Err(MenuError::OutOfContent {
                    screen: self.screen.id,
                    node: id.to_string(),
                    rect,
                    limit,
                    margin: margin(&limit, &rect),
                });
            }
        }
        Ok(())
    }

    fn push(&mut self, node: UiNode, plate: Option<&'static str>) -> Result<(), MenuError> {
        self.guard(&node.id, node.rect, plate)?;
        self.nodes.push(node);
        Ok(())
    }

    fn fill(
        &mut self,
        id: &str,
        plate: Option<&'static str>,
        rect: UiRect,
        ink: u8,
    ) -> Result<(), MenuError> {
        let node = fill_node(id, self.assets, rect, PaletteFamily::Hud, ink)?;
        self.push(node, plate)
    }

    fn glyph(
        &mut self,
        id: &str,
        plate: Option<&'static str>,
        font: &str,
        text: &str,
        x: i16,
        y: i16,
        ink: u8,
        tracking: u8,
    ) -> Result<(), MenuError> {
        if text.is_empty() {
            return Ok(());
        }
        let node = glyph_node_tracked(MENU_PANEL, id, self.assets, font, text, x, y, ink, tracking)?;
        self.push(node, plate)
    }

    fn advance(&self, font: &str, text: &str, tracking: u8) -> Result<i16, MenuError> {
        let font = self
            .assets
            .font(font)
            .ok_or_else(|| UiSpecError::MissingAsset {
                panel: MENU_PANEL,
                asset: font.to_string(),
            })?;
        Ok(font.text_width_tracked(text, u16::from(tracking)) as i16)
    }

    fn glyph_right(
        &mut self,
        id: &str,
        plate: Option<&'static str>,
        font: &str,
        text: &str,
        right: i16,
        y: i16,
        ink: u8,
        tracking: u8,
    ) -> Result<(), MenuError> {
        if text.is_empty() {
            return Ok(());
        }
        let advance = self.advance(font, text, tracking)?;
        self.glyph(id, plate, font, text, right - advance, y, ink, tracking)
    }

    fn field(&mut self) -> Result<(), MenuError> {
        self.fill(
            "menu-field",
            None,
            UiRect::new(0, 0, MENU_WIDTH, MENU_HEIGHT),
            ink::FIELD,
        )
    }

    /// Plate grammar: olive caps, lavender sides, paper interior. The content
    /// rect is **derived** and registered as the limit of the plate's zones.
    fn plate(&mut self, plate: &Plate) -> Result<(), MenuError> {
        let rect = plate.rect;
        self.fill(
            &format!("{}-olive-top", plate.id),
            None,
            UiRect::new(rect.x, rect.y, rect.width, STROKE),
            ink::OLIVE,
        )?;
        self.fill(
            &format!("{}-olive-bottom", plate.id),
            None,
            UiRect::new(rect.x, rect.bottom() - STROKE as i16, rect.width, STROKE),
            ink::OLIVE,
        )?;
        let side = UiRect::new(rect.x, rect.y + STROKE as i16, STROKE, rect.height - 2 * STROKE);
        self.fill(&format!("{}-lavender-left", plate.id), None, side, ink::LAVENDER)?;
        self.fill(
            &format!("{}-lavender-right", plate.id),
            None,
            UiRect::new(
                rect.right() - STROKE as i16,
                rect.y + STROKE as i16,
                STROKE,
                rect.height - 2 * STROKE,
            ),
            ink::LAVENDER,
        )?;
        let content = plate.content();
        self.fill(
            &format!("{}-paper", plate.id),
            None,
            UiRect::new(
                content.x - 1,
                content.y - 1,
                content.width + 2,
                content.height + 2,
            ),
            ink::PAPER,
        )?;
        self.limits.push((plate.id, content));
        Ok(())
    }

    fn lintel(&mut self) -> Result<(), MenuError> {
        let chrome = &self.screen.chrome;
        self.fill(
            "lintel",
            None,
            UiRect::new(0, 0, MENU_WIDTH, 25),
            ink::FIELD,
        )?;
        self.glyph(
            "lintel-title",
            None,
            FONT_DISPLAY,
            &chrome.title,
            16,
            7,
            ink::GOLD,
            0,
        )?;
        self.glyph_right(
            "lintel-right",
            None,
            FONT_MICRO,
            &chrome.right,
            304,
            9,
            ink::PAPER_SHADE,
            0,
        )?;
        self.fill(
            "lintel-rule",
            None,
            UiRect::new(0, 24, MENU_WIDTH, 1),
            ink::GOLD,
        )
    }

    fn footer(&mut self) -> Result<(), MenuError> {
        let chrome = &self.screen.chrome;
        self.fill(
            "footer",
            None,
            UiRect::new(0, 151, MENU_WIDTH, 29),
            ink::FIELD,
        )?;
        // The rule goes **after** the field: painted before it, the fill erases
        // it (the generator defect fixed on 2026-10-31, CR-178183d7).
        self.fill(
            "footer-rule",
            None,
            UiRect::new(0, 151, MENU_WIDTH, 1),
            ink::GOLD_DEEP,
        )?;
        let mut cursor = 16i16;
        for (index, (key, action)) in chrome.hints.iter().enumerate() {
            if index > 0 {
                cursor += 12;
            }
            self.glyph(
                &format!("footer-hint{index}-key"),
                None,
                FONT_MICRO,
                key,
                cursor,
                158,
                ink::GOLD,
                0,
            )?;
            cursor += self.advance(FONT_MICRO, key, 0)? + 4;
            self.glyph(
                &format!("footer-hint{index}-action"),
                None,
                FONT_MICRO,
                action,
                cursor,
                158,
                ink::PAPER_SHADE,
                0,
            )?;
            cursor += self.advance(FONT_MICRO, action, 0)?;
        }
        self.glyph_right(
            "footer-version",
            None,
            FONT_MICRO,
            &chrome.version,
            304,
            158,
            ink::PAPER_SHADE,
            0,
        )
    }

    fn zone(&mut self, zone: &Zone) -> Result<(), MenuError> {
        match zone {
            Zone::Nav(nav) => self.nav(nav),
            Zone::Steps(steps) => self.steps(steps),
            Zone::Choices(choices) => self.choices(choices),
            Zone::Detail(detail) => self.detail(detail),
        }
    }

    fn eyebrow(&mut self, zone: &str, plate: &'static str, rect: UiRect, text: &str, ink: u8) -> Result<(), MenuError> {
        self.glyph(
            &format!("{zone}-eyebrow"),
            Some(plate),
            FONT_MICRO,
            text,
            rect.x,
            rect.y,
            ink,
            1,
        )?;
        self.fill(
            &format!("{zone}-rule"),
            Some(plate),
            UiRect::new(rect.x, rect.y + 11, rect.width, 1),
            ink::LAVENDER,
        )
    }

    fn nav(&mut self, nav: &NavZone) -> Result<(), MenuError> {
        if let Some(eyebrow) = &nav.eyebrow {
            self.eyebrow(nav.id, nav.plate, nav.rect, eyebrow, ink::INK_MUTED)?;
        }
        for (index, row) in nav.rows.iter().enumerate() {
            self.nav_row(nav, row, nav.row_rect(index))?;
        }
        Ok(())
    }

    fn nav_row(&mut self, nav: &NavZone, row: &Row, rect: UiRect) -> Result<(), MenuError> {
        let plate = Some(nav.plate);
        let stem = format!("{}-{}", nav.id, row.id);
        let label_y = rect.y + (rect.height as i16 - 7) / 2;
        let (label_ink, value_ink) = match row.state {
            RowState::Focus => {
                self.fill(&format!("{stem}-bar"), plate, rect, ink::INK)?;
                self.fill(
                    &format!("{stem}-rail"),
                    plate,
                    UiRect::new(rect.x, rect.y, 2, rect.height),
                    ink::GOLD,
                )?;
                self.glyph(
                    &format!("{stem}-marker"),
                    plate,
                    FONT_MICRO,
                    ">",
                    rect.x + 6,
                    label_y,
                    ink::GOLD,
                    0,
                )?;
                (ink::PAPER, ink::PAPER_SHADE)
            }
            RowState::Disabled => {
                self.glyph(
                    &format!("{stem}-marker"),
                    plate,
                    FONT_MICRO,
                    "-",
                    rect.x + 6,
                    label_y,
                    ink::INK_MUTED,
                    0,
                )?;
                (ink::SLATE, ink::SLATE)
            }
            RowState::Idle => (ink::INK, ink::INK_MUTED),
        };
        self.glyph(
            &format!("{stem}-label"),
            plate,
            FONT_MICRO,
            &row.label,
            rect.x + ROW_PAD_X,
            label_y,
            label_ink,
            0,
        )?;
        let value = if row.state == RowState::Disabled && !row.value.is_empty() {
            format!("({})", row.value)
        } else {
            row.value.clone()
        };
        self.glyph_right(
            &format!("{stem}-value"),
            plate,
            FONT_MICRO,
            &value,
            rect.right() - ROW_PAD_X,
            label_y,
            value_ink,
            0,
        )
    }

    fn steps(&mut self, steps: &StepsZone) -> Result<(), MenuError> {
        self.eyebrow(steps.id, steps.plate, steps.rect, &steps.eyebrow, ink::INK)?;
        for (index, row) in steps.rows.iter().enumerate() {
            let rect = steps.row_rect(index);
            let plate = Some(steps.plate);
            let stem = format!("{}-{}", steps.id, row.id);
            let text_y = rect.y + (rect.height as i16 - 7) / 2;
            let number = format!("{:02}", index + 1);
            match row.state {
                StepState::Current => {
                    self.fill(&format!("{stem}-bar"), plate, rect, ink::INK)?;
                    self.fill(
                        &format!("{stem}-rail"),
                        plate,
                        UiRect::new(rect.x, rect.y, 2, rect.height),
                        ink::GOLD,
                    )?;
                    self.glyph(&format!("{stem}-number"), plate, FONT_MICRO, &number, rect.x + 8, text_y, ink::PAPER_SHADE, 0)?;
                    self.glyph(&format!("{stem}-label"), plate, FONT_MICRO, &row.label, rect.x + 22, text_y, ink::PAPER, 0)?;
                }
                StepState::Done => {
                    self.fill(
                        &format!("{stem}-bullet"),
                        plate,
                        UiRect::new(rect.x, text_y + 2, 3, 3),
                        ink::GOLD_DEEP,
                    )?;
                    self.glyph(&format!("{stem}-number"), plate, FONT_MICRO, &number, rect.x + 8, text_y, ink::GOLD_DEEP, 0)?;
                    self.glyph(&format!("{stem}-label"), plate, FONT_MICRO, &row.label, rect.x + 22, text_y, ink::GOLD_DEEP, 0)?;
                }
                StepState::Pending => {
                    self.glyph(&format!("{stem}-number"), plate, FONT_MICRO, &number, rect.x + 8, text_y, ink::INK_MUTED, 0)?;
                    self.glyph(&format!("{stem}-label"), plate, FONT_MICRO, &row.label, rect.x + 22, text_y, ink::INK_MUTED, 0)?;
                }
            }
        }
        Ok(())
    }

    fn choices(&mut self, choices: &ChoicesZone) -> Result<(), MenuError> {
        self.eyebrow(choices.id, choices.plate, choices.rect, &choices.eyebrow, ink::INK)?;
        for (index, row) in choices.rows.iter().enumerate() {
            let rect = choices.row_rect(index);
            let plate = Some(choices.plate);
            let stem = format!("{}-{}", choices.id, row.id);
            let text_y = rect.y + (rect.height as i16 - 7) / 2;
            let (ink_index, indent) = if row.state == RowState::Focus {
                self.fill(&format!("{stem}-bar"), plate, rect, ink::INK)?;
                self.fill(
                    &format!("{stem}-rail"),
                    plate,
                    UiRect::new(rect.x, rect.y, 2, rect.height),
                    ink::GOLD,
                )?;
                (ink::PAPER, 6)
            } else {
                (ink::INK, 4)
            };
            self.glyph(
                &format!("{stem}-label"),
                plate,
                FONT_MICRO,
                &row.label,
                rect.x + indent,
                text_y,
                ink_index,
                0,
            )?;
        }
        Ok(())
    }

    fn detail(&mut self, detail: &DetailZone) -> Result<(), MenuError> {
        let plate = Some(detail.plate);
        let rect = detail.rect;
        self.glyph(
            &format!("{}-header", detail.id),
            plate,
            FONT_MICRO,
            &detail.header,
            rect.x,
            rect.y,
            ink::INK,
            1,
        )?;
        self.fill(
            &format!("{}-rule", detail.id),
            plate,
            UiRect::new(rect.x, rect.y + 11, rect.width, 1),
            ink::LAVENDER,
        )?;
        let mut cursor = rect.y + if detail.portrait.is_some() { 14 } else { 16 };
        if detail.portrait.is_some() {
            let side = 66u16;
            self.fill(
                &format!("{}-portrait-frame", detail.id),
                plate,
                UiRect::new(rect.x, cursor - 1, side, side),
                ink::INK,
            )?;
            let node = sprite_node_at(
                MENU_PANEL,
                &format!("{}-portrait", detail.id),
                self.assets,
                PORTRAIT_ASSET,
                UiRect::new(rect.x + 1, cursor, 64, 64),
                Some(UiRect::new(0, 0, 64, 64)),
            )?;
            self.push(node, plate)?;
            cursor += 64 + 3;
        }
        for (key, value) in &detail.rows {
            self.glyph(
                &format!("{}-row-{key}-key", detail.id),
                plate,
                FONT_MICRO,
                key,
                rect.x,
                cursor,
                ink::INK_MUTED,
                0,
            )?;
            self.glyph_right(
                &format!("{}-row-{key}-value", detail.id),
                plate,
                FONT_MICRO,
                value,
                rect.right(),
                cursor,
                ink::INK,
                0,
            )?;
            cursor += 10;
        }
        if let Some((label, filled)) = &detail.progress {
            self.glyph(
                &format!("{}-progress", detail.id),
                plate,
                FONT_MICRO,
                label,
                rect.x,
                rect.y + rect.height as i16 - 18,
                ink::INK_MUTED,
                0,
            )?;
            self.fill(
                &format!("{}-progress-track", detail.id),
                plate,
                UiRect::new(rect.x, rect.y + rect.height as i16 - 8, rect.width, 3),
                ink::PAPER_SHADE,
            )?;
            let width = (u16::from(*filled) * rect.width).div_ceil(100).max(2);
            self.fill(
                &format!("{}-progress-fill", detail.id),
                plate,
                UiRect::new(rect.x, rect.y + rect.height as i16 - 8, width.min(rect.width), 3),
                ink::GOLD_DEEP,
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plate_content_is_derived_from_the_chrome() {
        assert_eq!(
            nav_plate().content(),
            UiRect::new(25, 41, 178, 100),
            "content = chrome + stroke(1) + pad(8)"
        );
        assert_eq!(detail_plate().content(), UiRect::new(229, 41, 66, 100));
    }

    #[test]
    fn margins_are_measured_against_the_chrome() {
        let plate = nav_plate();
        let nav = main_menu(0, false)
            .zones
            .iter()
            .find_map(|zone| match zone {
                Zone::Nav(nav) => Some(nav.clone()),
                _ => None,
            })
            .expect("nav zone");
        // Row 0 is the focus bar: it spans the whole content width, so its
        // margin is exactly the inset on the left and on the right.
        assert_eq!(margin(&plate.rect, &nav.row_rect(0)), MIN_MARGIN);
        // One pixel left of the content edge is below the floor.
        let mut shifted = nav.row_rect(0);
        shifted.x -= 1;
        assert!(margin(&plate.rect, &shifted) < MIN_MARGIN);
    }

    #[test]
    fn main_menu_rows_are_the_shell_items() {
        let screen = main_menu(0, false);
        let rows = screen.nav_rows("nav");
        assert_eq!(rows.len(), MENU_ROW_IDS.len());
        for (index, (id, _)) in rows.iter().enumerate() {
            assert_eq!(id, MENU_ROW_IDS[index]);
        }
        assert_eq!(rows[0].1, UiRect::new(25, 55, 178, 14), "y = 55 + 17i");
        assert_eq!(rows[4].1, UiRect::new(25, 123, 178, 14));
    }

    #[test]
    fn second_screen_is_data_only() {
        let screen = options_stub();
        assert_eq!(screen.id, OPTIONS_ID);
        assert_eq!(screen.panel, UiPanelId::Options);
        let rows = screen.nav_rows("nav");
        assert_eq!(rows.len(), 2, "options stub declares two rows");
        assert_eq!(rows[0].0, "music");
        // Its geometry comes from the same plate grammar, not from the menu.
        assert_eq!(screen.content_of(NAV_PLATE), Some(nav_plate().content()));
    }
}

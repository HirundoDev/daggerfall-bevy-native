//! 1996 UI spec: 4 panels composed from pure data + a 320×200 framebuffer
//! compositor (M17, DoD §h / WI-9 · REQ-L99-050).
//!
//! **Boundary:** this module is **data + math** — 0 Bevy, 0 input,
//! 0 persistence (bridge rules PRE-2 §8). The Bevy window
//! (`crates/app/src/m17_window.rs`) consumes the spec and only creates
//! `Sprite`/`Image`; the headless probe (`m17_probe.rs`) consumes the spec and
//! the compositor to verify C1/C2/C7 without a GPU.
//!
//! **Layout rule (spec §6):** each node declares `{panel → asset → rect}`.
//! Assets with an `ImgFile` header contribute `x/y/w/h` from that header
//! ([`UiAnchor::Header`]); RCI/CIF assets without a header use a **declared
//! and justified composition constant** ([`UiAnchor::Declared`]) — never an
//! ad hoc layout.
//!
//! **Palette rule (C2):** the index map is **per family**
//! (`PaletteFamily`): HUD/inventory 0..=254 with `PAL.PAL`, maps 63..=254
//! with `FMAP_PAL.COL`, masks 144..=158, bodies 0..=127, `TITL00I0` with its
//! embedded palette. The compositor rejects any pixel whose index falls
//! outside its asset range.
//!
//! **Presentation rule (C1/C1b):** exact 320×200 internal framebuffer;
//! present with integer ×2 nearest scaling (0 interpolation) and letterbox
//! 40 + 40 over 640×480 (`320×2 = 640 × 400`, 80 px of letterbox).

use std::collections::BTreeMap;

use daggerfall_parser_rs::image::{
    asset_index_range_resolved, family_index_range, DecodedImage, FntFile, PaletteFamily,
};
use daggerfall_parser_rs::texture::Palette;

/// Width of the original UI internal canvas (320 px).
pub const CANVAS_WIDTH: u16 = 320;

/// Height of the original UI internal canvas (200 px).
pub const CANVAS_HEIGHT: u16 = 200;

/// Integer presentation scale (`320×2 = 640`).
pub const PRESENT_SCALE: u8 = 2;

/// Width of the presentation window.
pub const PRESENT_WIDTH: u16 = CANVAS_WIDTH * PRESENT_SCALE as u16;

/// Height of the 640×480 output (C1b).
pub const PRESENT_HEIGHT: u16 = 480;

/// Top/bottom letterbox: `(480 − 200×2) / 2 = 40` px.
pub const LETTERBOX: u16 = (PRESENT_HEIGHT - CANVAS_HEIGHT * PRESENT_SCALE as u16) / 2;

/// Panel tag reported by [`Framebuffer::paint_nodes`] in typed errors.
///
/// A bare node slice carries no [`UiPanelId`], so the compositor names its own
/// entry point instead of inventing a panel; [`compose`] therefore reports
/// `"nodes"` (not the panel id) in `MissingAsset`/`MissingFont`.
pub const NODES_PANEL: &str = "nodes";

/// Runtime `.FNT` fonts — the complete registry is exactly this
/// set (C7b: 0 system fonts).
pub const FONT_FILES: [&str; 5] = [
    "FONT0000.FNT",
    "FONT0001.FNT",
    "FONT0002.FNT",
    "FONT0003.FNT",
    "FONT0004.FNT",
];

/// Default font of the panels (14×11 cell).
pub const DEFAULT_FONT: &str = "FONT0000.FNT";

/// Default text ink (index 127 = `(44,44,44)` in `PAL.PAL`).
pub const DEFAULT_INK: u8 = 127;

/// **The main menu carries no dialog widgets.** `BUTN00I0.IMG` is the
/// confirmation-button set of the dialogs (`ACCEPT`, `REJECT`, `CANCEL`,
/// `YES`, `NO`, `OK`, `MALE`, `FEMALE`): next to `NEW GAME` its baked word
/// contradicts the item (`ACCEPT` beside `NEW GAME`), so it is not a menu
/// widget regardless of how well it fits geometrically.
///
/// The selection indicator is the `>` glyph of the panel font itself
/// (`FONT0003`, measured width 3, ink rows 1..5 of its 7-row cell), painted
/// immediately left of the active row: the retail font is already loaded, the
/// chevron points at the item it selects and no pixel is invented.
///
/// Corrections still in force (measured on the retail art): `MENU_Y 17→35`
/// · `MENU_CLIP_HEIGHT 183→125` · **no `MENU.IMG` fragment**: its plates carry
/// the animation-mode list and its left 24 columns are frame plus speckled
/// background, so the band is the title backdrop alone (see [`MENU_GUTTER`])
/// · labels `244, 44+25k` in `FONT0003`.
pub const MENU_SELECTION_TEXT: &str = ">>";

/// Ink of the selection marker (chevron **and** active label): `PAL.PAL[227]`
/// = `(254,225,156)`, the gold of the retail interface, luminance 225 (indices
/// 1..19 are the magenta transparency key, never an ink). Measured on the
/// 1280×720 capture: the label column sits on a dark title area (median
/// luminance 12), so the old `DEFAULT_INK` (luminance 44) rendered the menu
/// nearly invisible — 44 against 12.
pub const MENU_SELECTION_INK: u8 = 227;

/// Ink of the non-active labels: `PAL.PAL[83]` = `(145,145,159)`, luminance
/// 146 — dim enough for the gold row to stand out (measured on the capture:
/// 146 against 225 is an unambiguous step) and still 14× the background.
pub const MENU_LABEL_INK: u8 = 83;

/// Row of the active item (chevron + white label). Mirrors
/// `shell::MENU_NEW_GAME`; `PROBE-MENU-LAYOUT-001` asserts both agree, so the
/// indicator cannot drift away from the declared initial selection.
///
/// This is the **initial** state only: [`build_main_menu_panel`] bakes it into
/// the picture, so a window that shows the selection moving composes
/// [`build_main_menu_panel_with_selection`] per row instead of mutating it
/// (CR-4a9c1e77 — the constant is the shell binding, not a runtime cursor).
pub const MENU_ACTIVE_ROW: usize = 0;

/// Height of the menu band: `MENU.IMG` shows 125 rows (35..160), stopping 3 px
/// above the parchment (measured at y=163).
pub const MENU_CLIP_HEIGHT: u16 = 125;

/// Strip of the panel **left of the menu column** (`x 212..240`, `y 35..160`):
/// the declared `TITL00I0` backdrop is the only node allowed to paint here.
///
/// `MENU.IMG` used to fill it from its source `x 0..24` as a "word-free
/// margin", but that crop is not a margin (measured on the retail bytes over
/// the 125 rows): columns 0..5 are the panel's lavender frame and columns
/// 6..24 are the **speckled background field** of the animation-mode panel,
/// palette indices 40..236. Blitted 1:1 over the dark title stone it read as
/// 24×125 px of television static beside `NEW GAME` — the last vestige of the
/// "everything overlaid" panel. `PROBE-MENU-LAYOUT-001` measures this strip
/// against the backdrop alone, so no foreign fragment can return unnoticed.
pub const MENU_GUTTER: UiRect = UiRect {
    x: MENU_X,
    y: MENU_Y,
    width: (MENU_SELECTION_X - MENU_X) as u16,
    height: MENU_CLIP_HEIGHT,
};

/// Retail labels of the main menu, in plate order (`shell::MENU_ITEMS`).
pub const MENU_ITEM_LABELS: [&str; 5] = ["NEW GAME", "CONTINUE", "LOAD", "OPTIONS", "QUIT"];

/// Painted states of the main menu: one per row
/// (`== MENU_ITEM_LABELS.len()`, asserted in the module tests). A consumer that
/// wants the selection to move composes [`MENU_ROWS`] panels — one per active
/// row — instead of re-using the single frozen picture (CR-4a9c1e77).
pub const MENU_ROWS: usize = 5;

/// Font of the menu labels: `FONT0003` (5×7 cell). `FONT0000` needs 94 px for
/// `NEW GAME` and cannot fit the 39-px label column.
pub const MENU_FONT: &str = "FONT0003.FNT";

const HUD_HP_X: i16 = 4;
const HUD_HP_Y: i16 = 157;
const HUD_STA_X: i16 = 150;
const HUD_LVL_X: i16 = 266;
const COMPASS_Y: i16 = 186;
const COMPASS_HEIGHT: u16 = 14;
const INV_GOLD_X: i16 = 8;
const INV_GOLD_Y: i16 = 8;
const INV_ITEMS_Y: i16 = 24;
const INV_ITEM01_X: i16 = 250;
const INV_ITEM01_Y: i16 = 110;
const QUEST_TEXT_X: i16 = 8;
const QUEST_TEXT_Y: i16 = 8;
const QUEST_TITLE_Y: i16 = 26;
const QUEST_TITLE_MAX_WIDTH: u16 = 180;
const MENU_X: i16 = (CANVAS_WIDTH - 108) as i16;
const MENU_Y: i16 = 35;
const MENU_LABEL_X: i16 = 248;
const MENU_LABEL_Y: i16 = 44;
const MENU_LABEL_MAX_WIDTH: u16 = 39;
/// Vertical advance between menu rows: the row's baseline is
/// `MENU_LABEL_Y + row × this`, so the chevron of row `r` sits `r × this` px
/// below row 0 — the number `PROBE-MENU-LAYOUT-001 --probe selection` measures.
pub const MENU_LABEL_STRIDE: i16 = 25;
/// X of the selection chevron: against the stone margin (`MENU.IMG` ends at
/// 236) and 2 px left of the labels (244), so the `>>` occupies 236..242 and
/// never touches the words.
const MENU_SELECTION_X: i16 = 240;

/// Rectangle in 320×200 canvas coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiRect {
    /// X of the left edge.
    pub x: i16,
    /// Y of the top edge.
    pub y: i16,
    /// Width.
    pub width: u16,
    /// Height.
    pub height: u16,
}

impl UiRect {
    /// Builds a rectangle.
    pub fn new(x: i16, y: i16, width: u16, height: u16) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Right edge (exclusive).
    pub fn right(&self) -> i16 {
        self.x + self.width as i16
    }

    /// Bottom edge (exclusive).
    pub fn bottom(&self) -> i16 {
        self.y + self.height as i16
    }

    /// `true` if the rectangle fits entirely in the 320×200 canvas.
    pub fn fits_canvas(&self) -> bool {
        self.x >= 0
            && self.y >= 0
            && self.right() <= CANVAS_WIDTH as i16
            && self.bottom() <= CANVAS_HEIGHT as i16
    }

    /// Full-canvas rectangle.
    pub fn canvas() -> Self {
        Self::new(0, 0, CANVAS_WIDTH, CANVAS_HEIGHT)
    }
}

/// One window of the M34 census, as the composition contract declares it
/// (spec M34 §4(i)-(iv)).
///
/// W34-1 shipped the **contract**; W34-2a..2f composed its rows for the **51
/// playable windows** in [`crate::ui_playable_g1`] (17),
/// [`crate::ui_playable_g2`] (11) and [`crate::ui_playable_g3`] (23) —
/// 17 + 11 + 23 = 51, the `g3` column of [`window_group_counts`]. Spec M34
/// §4(i)
/// (`docs/sintergic/game/programming/m34-e2e-integration-presentation-spec-CR-5243af63.md:219`)
/// binds this struct to those 51 only: [`crate::ui_windows_g1`] (16
/// `CreateChar*`) and [`crate::ui_windows_g2`] (11 config + 7 infra, all
/// `N/A-engine`) are census rows counted by the shell registry, so
/// [`crate::ui_windows_g1::specs`] and [`crate::ui_windows_g2::specs`] return
/// `&[]` **by design**, not as pending composition — `PROBE-UI-FULL-001`
/// prints `grupos=[g1=0 g2=0 g3=51]` with `EXIT=0` on that basis.
/// [`crate::ui_windows_g3`] is retained for the census contract but is no
/// longer a composition slot (see [`window_group_counts`]).
///
/// - `id` is the DFU class name (census `m34…:36-60`, GV-01/GV-02/GV-19), so
///   `PROBE-UI-FULL-001` can bind every spec to its census row;
/// - `asset_retail` is the primary `ARENA2` asset the window decodes
///   (`IMG`/`CIF`/`FNT`/`RCI`); `None` is reserved for the `N/A-engine`
///   pages, which carry no spec (§4) and therefore no row here;
/// - `rect` is the declared placement inside the 320×200 canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowSpec {
    /// Census name (DFU class name, no `.cs`).
    pub id: &'static str,
    /// Primary retail asset, `None` for engine pages.
    pub asset_retail: Option<&'static str>,
    /// Declared placement in the 320×200 canvas.
    pub rect: UiRect,
}

impl WindowSpec {
    /// A concrete 1996 window: retail asset present and rectangle inside the
    /// canvas (§4(ii) + §4(iv)).
    pub fn is_concrete(&self) -> bool {
        match self.asset_retail {
            Some(asset) => !asset_is_generic(asset) && self.rect.fits_canvas(),
            None => false,
        }
    }
}

/// Is `asset` an empty/placeholder name instead of a retail asset? (§4(iv):
/// no node may paint a `default`/generic/plain-colour widget).
pub fn asset_is_generic(asset: &str) -> bool {
    let trimmed = asset.trim();
    trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("default")
        || trimmed.eq_ignore_ascii_case("generic")
        || trimmed.eq_ignore_ascii_case("color")
        || trimmed.eq_ignore_ascii_case("none")
}

/// **Single** concatenation point of the three census window groups
/// (G1 createchar → G2 config+infra → G3 playable).
///
/// Callers never chain the groups themselves: one place to read the whole set
/// is what lets `PROBE-UI-FULL-001` compare "specs declared" against the
/// census without a second, drifting list.
pub fn window_specs() -> impl Iterator<Item = &'static WindowSpec> {
    crate::ui_windows_g1::specs()
        .iter()
        .chain(crate::ui_windows_g2::specs())
        .chain(crate::ui_playable_g1::specs())
        .chain(crate::ui_playable_g2::specs())
        .chain(crate::ui_playable_g3::specs())
}

/// Rows declared per group: `[g1 createchar, g2 config+infra, g3 playable]`.
///
/// `g3` is the sum of the three wave-3 composition slots (`ui_playable_g1..g3`,
/// W34-2a..2f); `ui_windows_g3` is retained for the census contract but is no
/// longer part of the concatenation.
pub fn window_group_counts() -> [usize; 3] {
    [
        crate::ui_windows_g1::specs().len(),
        crate::ui_windows_g2::specs().len(),
        crate::ui_playable_g1::specs().len()
            + crate::ui_playable_g2::specs().len()
            + crate::ui_playable_g3::specs().len(),
    ]
}

/// Where a node's rectangle comes from (auditable, 0 ad hoc).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiAnchor {
    /// `x/y/w/h` read from the asset `ImgFile` header.
    Header,
    /// Declared composition constant (RCI/CIF assets without header).
    Declared(&'static str),
}

/// Node primitive kind — the closed inventory of the 1996 UI.
///
/// **There is no** widget variant (text/bevy_ui/native button): the
/// [`assert_no_generic_widgets`] assert fails if a node declares anything else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiNodeKind {
    /// Sprite from a `.IMG/.CIF` asset (palette indices).
    Sprite,
    /// Glyph run from a `.FNT` (1 bpp mask).
    GlyphRun,
    /// Rectangular fill of one palette index ([`fill_node`]).
    ///
    /// The primitive exists because a 1×1 sprite **cannot** fill a rect:
    /// [`Framebuffer::paint_nodes`] blits a sprite 1:1 from its asset
    /// (`asset.image.pixel(sx, sy)`), so a 1×1 source covers exactly one
    /// pixel and `continue`s over the rest of the destination — and there is
    /// no `Repeat`. Without `Fill` there is no plate, no focus bar and no
    /// rule line (`docs/sintergic/design/MENU-MODERN-DESIGN.md` §9.2).
    Fill,
}

impl UiNodeKind {
    /// Stable name (for the inventory and the asserts).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Sprite => "sprite",
            Self::GlyphRun => "glyph-run",
            Self::Fill => "fill",
        }
    }
}

/// UI node: primitive + rect + source asset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiNode {
    /// Stable identifier within the panel.
    pub id: String,
    /// Primitive.
    pub kind: UiNodeKind,
    /// Destination rectangle on the canvas.
    pub rect: UiRect,
    /// Declared origin of the rectangle.
    pub anchor: UiAnchor,
    /// Asset it paints (`IMG`/`CIF`/`FNT`).
    pub asset: String,
    /// Palette family of the asset.
    pub family: PaletteFamily,
    /// FNV-1a64 of the resolved palette.
    pub palette_hash: u64,
    /// Source cell within the asset (`None` = whole asset).
    pub source: Option<UiRect>,
    /// Text (only `GlyphRun`).
    pub text: Option<String>,
    /// Ink index (only `GlyphRun`).
    pub ink: u8,
    /// Extra pixels after every advance (only `GlyphRun`; `0` = retail metric).
    pub tracking: u8,
}

/// Panel identifier of the 1996 UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum UiPanelId {
    /// Bottom HUD bar.
    Hud,
    /// Inventory / paper-doll.
    Inventory,
    /// Journal / quests.
    Quests,
    /// Main menu.
    MainMenu,
    /// Options screen (component-model stub; never in the production path).
    Options,
    /// Local map screen (`MAP100I0.IMG`, 320×200) — **B3-c**.
    MapLocal,
    /// World / travel map screen (`TRAV0I00.IMG`, 320×200) — **B3-c**.
    MapWorld,
}

impl UiPanelId {
    /// Stable name.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Hud => "hud",
            Self::Inventory => "inventory",
            Self::Quests => "quests",
            Self::MainMenu => "main-menu",
            Self::Options => "options",
            Self::MapLocal => "map-local",
            Self::MapWorld => "map-world",
        }
    }
}

/// Composed panel: ordered list of nodes (order = painting order).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiPanel {
    /// Identifier.
    pub id: UiPanelId,
    /// Nodes in painting order.
    pub nodes: Vec<UiNode>,
}

impl UiPanel {
    /// Palettes (asset → hash) used by the panel, unique and sorted.
    pub fn palette_hashes(&self) -> Vec<(String, u64)> {
        let mut map: BTreeMap<String, u64> = BTreeMap::new();
        for node in &self.nodes {
            map.insert(node.asset.clone(), node.palette_hash);
        }
        map.into_iter().collect()
    }

    /// FNV-1a64 panel fingerprint: combines the sorted (asset, hash) pairs.
    /// Distinguishes panels that share a palette family but not assets.
    pub fn palette_fingerprint(&self) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for (asset, palette) in self.palette_hashes() {
            for byte in asset.bytes() {
                hash ^= byte as u64;
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
            for byte in palette.to_le_bytes() {
                hash ^= byte as u64;
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        hash
    }

    /// Node kind inventory (`sprite`, `glyph-run`, `fill`).
    pub fn node_inventory(&self) -> BTreeMap<&'static str, usize> {
        let mut map: BTreeMap<&'static str, usize> = BTreeMap::new();
        for node in &self.nodes {
            *map.entry(node.kind.as_str()).or_insert(0) += 1;
        }
        map
    }
}

/// HUD data projected from `HudReadModel` (M7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HudPanelData {
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

/// Inventory data projected from `SessionPanelReadModel` (M7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryPanelData {
    /// Gold.
    pub gold: i32,
    /// Item kinds.
    pub item_kinds: usize,
    /// Total units.
    pub total_units: i32,
}

/// Journal data projected from `SessionPanelReadModel` (M7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestPanelData {
    /// Titles of active quests in acceptance order.
    pub quests: Vec<String>,
}

/// Decoded asset + palette resolved from its family.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiAsset {
    /// Retail name (`MAIN00I0.IMG`).
    pub name: String,
    /// Decoded image (palette indices).
    pub image: DecodedImage,
    /// Palette family.
    pub family: PaletteFamily,
    /// Palette resolved for this family/asset.
    pub palette: Palette,
    /// FNV-1a64 of the resolved palette.
    pub palette_hash: u64,
}

/// Asset + font bank consumed by the spec.
#[derive(Debug, Clone)]
pub struct UiAssets {
    /// Decoded image assets.
    pub images: Vec<UiAsset>,
    /// `.FNT` fonts (complete registry, C7b).
    pub fonts: Vec<(String, FntFile)>,
    /// HUD palette (`PAL.PAL`).
    pub hud_palette: Palette,
    /// Map palette (`FMAP_PAL.COL`).
    pub map_palette: Palette,
    /// 3D world palette (`ART_PAL.COL`).
    pub art_palette: Palette,
}

impl UiAssets {
    /// Empty bank with the three family palettes.
    pub fn new(hud: Palette, map: Palette, art: Palette) -> Self {
        Self {
            images: Vec::new(),
            fonts: Vec::new(),
            hud_palette: hud,
            map_palette: map,
            art_palette: art,
        }
    }

    /// Adds a decoded asset. The palette is resolved per family; for
    /// `Embedded` the asset's own embedded `PalFile` is used (mandatory).
    pub fn with_image(
        mut self,
        name: &str,
        image: DecodedImage,
    ) -> Result<Self, UiSpecError> {
        let family = daggerfall_parser_rs::image::family_for_asset(name);
        let palette = match family {
            PaletteFamily::Hud => self.hud_palette,
            PaletteFamily::Map => self.map_palette,
            PaletteFamily::Art => self.art_palette,
            PaletteFamily::Embedded => image.embedded_palette.ok_or(UiSpecError::MissingAsset {
                panel: "palette",
                asset: name.to_string(),
            })?,
        };
        let palette_hash = palette_hash(&palette);
        self.images.push(UiAsset {
            name: name.to_string(),
            image,
            family,
            palette,
            palette_hash,
        });
        Ok(self)
    }

    /// Adds a font to the registry.
    pub fn with_font(mut self, name: &str, font: FntFile) -> Self {
        self.fonts.push((name.to_string(), font));
        self
    }

    /// Asset by name.
    pub fn image(&self, name: &str) -> Option<&UiAsset> {
        self.images.iter().find(|asset| asset.name == name)
    }

    /// Font by name.
    pub fn font(&self, name: &str) -> Option<&FntFile> {
        self.fonts
            .iter()
            .find(|(font_name, _)| font_name == name)
            .map(|(_, font)| font)
    }

    /// Resolves a palette per family (for text nodes).
    pub fn palette_for(&self, family: PaletteFamily) -> &Palette {
        match family {
            PaletteFamily::Map => &self.map_palette,
            PaletteFamily::Art | PaletteFamily::Embedded => &self.art_palette,
            PaletteFamily::Hud => &self.hud_palette,
        }
    }
}

fn require_image<'a>(
    assets: &'a UiAssets,
    panel: &'static str,
    name: &str,
) -> Result<&'a UiAsset, UiSpecError> {
    assets.image(name).ok_or_else(|| UiSpecError::MissingAsset {
        panel,
        asset: name.to_string(),
    })
}

fn require_font<'a>(
    assets: &'a UiAssets,
    panel: &'static str,
    name: &str,
) -> Result<&'a FntFile, UiSpecError> {
    assets.font(name).ok_or_else(|| UiSpecError::MissingFont {
        panel,
        font: name.to_string(),
    })
}

/// FNV-1a 64 of the resolved palette (256 × RGB).
pub fn palette_hash(palette: &Palette) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for color in &palette.colors {
        for channel in color {
            hash ^= *channel as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

/// UI spec construction/composition error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiSpecError {
    /// Asset required by a panel missing from the bank.
    MissingAsset {
        /// Panel that requires it.
        panel: &'static str,
        /// Asset name.
        asset: String,
    },
    /// Required font missing.
    MissingFont {
        /// Panel that requires it.
        panel: &'static str,
        /// `.FNT` name.
        font: String,
    },
    /// Rectangle outside the 320×200 canvas.
    OutOfCanvas {
        /// Panel.
        panel: &'static str,
        /// Node.
        node: String,
        /// Offending rectangle.
        rect: UiRect,
    },
    /// Source cell outside the image.
    SourceOutOfBounds {
        /// Asset.
        asset: String,
        /// Requested cell.
        source: UiRect,
        /// Actual asset size.
        image: (u16, u16),
    },
    /// Node with a disallowed primitive (generic widget).
    GenericWidget {
        /// Panel.
        panel: &'static str,
        /// Node.
        node: String,
        /// What violated the rule.
        what: String,
    },
    /// Composed pixel with an index outside its asset range.
    PaletteViolation {
        /// Asset.
        asset: String,
        /// Observed index.
        index: u8,
        /// Allowed range (`start`, `end`).
        range: (u8, u8),
    },
    /// Active row requested outside the panel's rows.
    ///
    /// The menu's chevron is placed from the row
    /// ([`build_main_menu_panel_with_selection`]): an index past the last label
    /// would paint it over nothing and read as a legitimate selection.
    ActiveRowOutOfRange {
        /// Panel.
        panel: &'static str,
        /// Requested row.
        row: usize,
        /// Declared rows.
        rows: usize,
    },
}

impl std::fmt::Display for UiSpecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingAsset { panel, asset } => {
                write!(f, "panel {panel}: missing asset {asset}")
            }
            Self::MissingFont { panel, font } => write!(f, "panel {panel}: missing font {font}"),
            Self::OutOfCanvas { panel, node, rect } => write!(
                f,
                "panel {panel}: node {node} outside the 320×200 canvas ({rect:?})"
            ),
            Self::SourceOutOfBounds {
                asset,
                source,
                image,
            } => write!(
                f,
                "asset {asset}: cell {source:?} outside the image {image:?}"
            ),
            Self::GenericWidget { panel, node, what } => {
                write!(f, "panel {panel}: node {node} violates C7a ({what})")
            }
            Self::PaletteViolation {
                asset,
                index,
                range,
            } => write!(
                f,
                "asset {asset}: index {index} outside its palette {:?} (C2)",
                range.0..=range.1
            ),
            Self::ActiveRowOutOfRange { panel, row, rows } => {
                write!(f, "panel {panel}: active row {row} outside 0..{rows}")
            }
        }
    }
}

impl std::error::Error for UiSpecError {}

fn sprite_node(
    panel: &'static str,
    id: &str,
    asset: &UiAsset,
    rect: UiRect,
    anchor: UiAnchor,
    source: Option<UiRect>,
) -> Result<UiNode, UiSpecError> {
    if !rect.fits_canvas() {
        return Err(UiSpecError::OutOfCanvas {
            panel,
            node: id.to_string(),
            rect,
        });
    }
    if let Some(source) = source {
        let image = (asset.image.width, asset.image.height);
        if source.x < 0
            || source.y < 0
            || source.right() > image.0 as i16
            || source.bottom() > image.1 as i16
            || source.width == 0
            || source.height == 0
        {
            return Err(UiSpecError::SourceOutOfBounds {
                asset: asset.name.clone(),
                source,
                image,
            });
        }
    }
    Ok(UiNode {
        id: id.to_string(),
        kind: UiNodeKind::Sprite,
        rect,
        anchor,
        asset: asset.name.clone(),
        family: asset.family,
        palette_hash: asset.palette_hash,
        source,
        text: None,
        ink: DEFAULT_INK,
        tracking: 0,
    })
}

/// Public sprite node addressed by asset name (callers that are not the 1996
/// panel builders, e.g. [`crate::menu_spec`], get the same validation).
///
/// # Errors
///
/// [`UiSpecError::MissingAsset`] when `name` is not registered;
/// [`UiSpecError::OutOfCanvas`] / [`UiSpecError::SourceOutOfBounds`] for a rect
/// or `source` the asset does not carry.
pub fn sprite_node_at(
    panel: &'static str,
    id: &str,
    assets: &UiAssets,
    name: &str,
    rect: UiRect,
    source: Option<UiRect>,
) -> Result<UiNode, UiSpecError> {
    let asset = require_image(assets, panel, name)?;
    sprite_node(panel, id, asset, rect, UiAnchor::Declared("sprite: rect declared by the screen"), source)
}

/// Sprite node whose rect comes from the asset `ImgFile` header.
fn header_sprite(
    panel: &'static str,
    id: &str,
    assets: &UiAssets,
    name: &str,
) -> Result<UiNode, UiSpecError> {
    let asset = require_image(assets, panel, name)?;
    let rect = UiRect::new(
        asset.image.x_offset,
        asset.image.y_offset,
        asset.image.width,
        asset.image.height,
    );
    sprite_node(panel, id, asset, rect, UiAnchor::Header, None)
}

/// Sprite node with a declared rect (RCI/CIF without header) and optional cell.
fn declared_sprite(
    panel: &'static str,
    id: &str,
    assets: &UiAssets,
    name: &str,
    rect: UiRect,
    reason: &'static str,
    source: Option<UiRect>,
) -> Result<UiNode, UiSpecError> {
    let asset = require_image(assets, panel, name)?;
    sprite_node(panel, id, asset, rect, UiAnchor::Declared(reason), source)
}

/// Truncates `text` so that it does not exceed `max_width` with `font`.
pub fn fit_text(font: &FntFile, text: &str, max_width: u16) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        out.push(ch);
        if font.text_width(&out) > max_width {
            out.pop();
            break;
        }
    }
    out
}

/// Palette source of a [`UiNodeKind::Fill`] node: a fill reads **no** asset,
/// its palette is the one of its family. The label is what `palette_hashes`,
/// the palette slots and the probes print in the `asset` column, so it says
/// where the colours came from instead of inventing a file name.
pub const FILL_PALETTE_SOURCE: &str = "family-palette";

/// Ink range a [`UiNodeKind::Fill`] may paint: its family range **minus index
/// 0**.
///
/// Index 0 is transparency for the compositor, so an "ink 0" fill writes
/// nothing and the plate it was meant to be reads as a hole in the surface.
/// Excluding 0 from the *range* — instead of adding a second rule — is what
/// makes construction and paint report the same typed error
/// ([`UiSpecError::PaletteViolation`], the C2 rule `Sprite`/`GlyphRun` already
/// report) carrying the range it enforces.
fn fill_ink_range(family: PaletteFamily) -> std::ops::RangeInclusive<u8> {
    let family = family_index_range(family);
    (*family.start()).max(1)..=*family.end()
}

/// Text node served by a `.FNT` (0 Bevy `Text`/`Text2d`).
#[allow(clippy::too_many_arguments)] // private painter: explicit parameters = readable call sites
fn glyph_node(
    panel: &'static str,
    id: &str,
    assets: &UiAssets,
    font_name: &str,
    text: &str,
    x: i16,
    y: i16,
    max_width: u16,
    ink: u8,
) -> Result<UiNode, UiSpecError> {
    let font = require_font(assets, panel, font_name)?;
    let text = fit_text(font, text, max_width);
    glyph_node_tracked(panel, id, assets, font_name, &text, x, y, ink, 0)
}

/// Text node with `tracking` extra pixels after every advance.
///
/// The rect width is the **tracked** advance (`Σadv + t·n − 1`), so the box the
/// layout assertion measures is the box the mask occupies.
#[allow(clippy::too_many_arguments)] // explicit parameters = readable call sites
pub fn glyph_node_tracked(
    panel: &'static str,
    id: &str,
    assets: &UiAssets,
    font_name: &str,
    text: &str,
    x: i16,
    y: i16,
    ink: u8,
    tracking: u8,
) -> Result<UiNode, UiSpecError> {
    let font = require_font(assets, panel, font_name)?;
    let rect = UiRect::new(
        x,
        y,
        font.text_width_tracked(text, u16::from(tracking)),
        font.fixed_height,
    );
    if !rect.fits_canvas() {
        return Err(UiSpecError::OutOfCanvas {
            panel,
            node: id.to_string(),
            rect,
        });
    }
    let palette = assets.palette_for(PaletteFamily::Hud);
    Ok(UiNode {
        id: id.to_string(),
        kind: UiNodeKind::GlyphRun,
        rect,
        anchor: UiAnchor::Declared("text: position declared in the panel"),
        asset: font_name.to_string(),
        family: PaletteFamily::Hud,
        palette_hash: palette_hash(palette),
        source: None,
        text: Some(text.to_string()),
        ink,
        tracking,
    })
}

/// Rectangular fill of one palette index — [`UiNodeKind::Fill`].
///
/// **Why it exists.** A 1×1 sprite does **not** fill a rect:
/// [`Framebuffer::paint_nodes`] blits a sprite 1:1
/// (`asset.image.pixel(sx, sy)`), so beyond the first pixel the source lookup
/// returns `None` and the `continue` leaves the destination untouched — holes
/// — and the primitive set has no `Repeat`. `Fill` writes `node.ink` on every
/// pixel of the clipped rect, reads no asset and allocates nothing.
///
/// Contract (`docs/sintergic/design/MENU-MODERN-DESIGN.md` §9.2):
///
/// | Property | Rule |
/// |:---|:---|
/// | fields | `rect` + `ink` ([`FILL_PALETTE_SOURCE`] is the palette label) |
/// | clipping | node rect ∩ framebuffer; never writes outside |
/// | degenerate | `w == 0 \|\| h == 0` → silent no-op (never an error) |
/// | index 0 | forbidden as `ink` (a fill of transparency is a hole) |
/// | family | `ink` outside [`fill_ink_range`] → [`UiSpecError::PaletteViolation`] |
/// | cost | `w·h` `u8` writes, no asset read, no allocation |
///
/// The rect is **not** required to fit the canvas: clipping is the
/// compositor's job, so a plate may bleed off an edge by declaration
/// (`x = -5` paints from column 0).
pub fn fill_node(
    id: &str,
    assets: &UiAssets,
    rect: UiRect,
    family: PaletteFamily,
    ink: u8,
) -> Result<UiNode, UiSpecError> {
    let range = fill_ink_range(family);
    if !range.contains(&ink) {
        return Err(UiSpecError::PaletteViolation {
            asset: FILL_PALETTE_SOURCE.to_string(),
            index: ink,
            range: (*range.start(), *range.end()),
        });
    }
    Ok(UiNode {
        id: id.to_string(),
        kind: UiNodeKind::Fill,
        rect,
        anchor: UiAnchor::Declared("fill: rect declared in the panel"),
        asset: FILL_PALETTE_SOURCE.to_string(),
        family,
        palette_hash: palette_hash(assets.palette_for(family)),
        source: None,
        text: None,
        ink,
        tracking: 0,
    })
}

/// HUD panel: `MAIN00` + `MAIN02` + `MAIN03` + `COMPASS` + status text.
pub fn build_hud_panel(data: &HudPanelData, assets: &UiAssets) -> Result<UiPanel, UiSpecError> {
    let compass = require_image(assets, "hud", "COMPASS.IMG")?;
    let compass_source = UiRect::new(0, 0, CANVAS_WIDTH, compass.image.height.min(COMPASS_HEIGHT));
    let nodes = vec![
        header_sprite("hud", "main00", assets, "MAIN00I0.IMG")?,
        header_sprite("hud", "main02", assets, "MAIN02I0.IMG")?,
        header_sprite("hud", "main03", assets, "MAIN03I0.IMG")?,
        declared_sprite(
            "hud",
            "compass",
            assets,
            "COMPASS.IMG",
            UiRect::new(0, COMPASS_Y, CANVAS_WIDTH, COMPASS_HEIGHT),
            "COMPASS is RCI without header: compass bar at the bottom (322 px → 320)",
            Some(compass_source),
        )?,
        glyph_node(
            "hud",
            "hp",
            assets,
            DEFAULT_FONT,
            &format!("{}/{}", data.health, data.max_health),
            HUD_HP_X,
            HUD_HP_Y,
            140,
            DEFAULT_INK,
        )?,
        glyph_node(
            "hud",
            "stamina",
            assets,
            DEFAULT_FONT,
            &format!("{}/{}", data.stamina, data.max_stamina),
            HUD_STA_X,
            HUD_HP_Y,
            110,
            DEFAULT_INK,
        )?,
        glyph_node(
            "hud",
            "level",
            assets,
            DEFAULT_FONT,
            &format!("LVL {}", data.level),
            HUD_LVL_X,
            HUD_HP_Y,
            54,
            DEFAULT_INK,
        )?,
    ];
    Ok(UiPanel {
        id: UiPanelId::Hud,
        nodes,
    })
}

/// Inventory panel: `ITEM00`/`INVE00` + slots `INVE03/06..14` + `ITEM01`.
pub fn build_inventory_panel(
    data: &InventoryPanelData,
    assets: &UiAssets,
) -> Result<UiPanel, UiSpecError> {
    let mut nodes = vec![declared_sprite(
        "inventory",
        "item00",
        assets,
        "ITEM00I0.IMG",
        UiRect::canvas(),
        "ITEM00 is RCI 320×200 without header: art layer under the frame",
        None,
    )?];
    nodes.push(declared_sprite(
        "inventory",
        "inve00",
        assets,
        "INVE00I0.IMG",
        UiRect::canvas(),
        "INVE00 is RCI 320×200 without header: inventory frame",
        None,
    )?);
    nodes.push(header_sprite("inventory", "slot03", assets, "INVE03I0.IMG")?);
    nodes.push(header_sprite("inventory", "slot06", assets, "INVE06I0.IMG")?);
    nodes.push(header_sprite("inventory", "slot07", assets, "INVE07I0.IMG")?);
    for index in 8..=14 {
        nodes.push(header_sprite(
            "inventory",
            &format!("slot{index:02}"),
            assets,
            &format!("INVE{index:02}I0.IMG"),
        )?);
    }
    nodes.push(declared_sprite(
        "inventory",
        "item01",
        assets,
        "ITEM01I0.IMG",
        UiRect::new(INV_ITEM01_X, INV_ITEM01_Y, 36, 81),
        "ITEM01 is RCI 36×81 without header: declared paper-doll position",
        None,
    )?);
    nodes.push(glyph_node(
        "inventory",
        "gold",
        assets,
        DEFAULT_FONT,
        &format!("GOLD {}", data.gold),
        INV_GOLD_X,
        INV_GOLD_Y,
        180,
        DEFAULT_INK,
    )?);
    nodes.push(glyph_node(
        "inventory",
        "items",
        assets,
        DEFAULT_FONT,
        &format!("ITEMS {} x{}", data.item_kinds, data.total_units),
        INV_GOLD_X,
        INV_ITEMS_Y,
        180,
        DEFAULT_INK,
    )?);
    Ok(UiPanel {
        id: UiPanelId::Inventory,
        nodes,
    })
}

/// Journal/quests panel: `BOOK00` + pages `SCBG00..08` + `SCRL04..14`.
pub fn build_quests_panel(data: &QuestPanelData, assets: &UiAssets) -> Result<UiPanel, UiSpecError> {
    let mut nodes = vec![declared_sprite(
        "quests",
        "book00",
        assets,
        "BOOK00I0.IMG",
        UiRect::canvas(),
        "BOOK00 is RCI 320×200 without header: journal page",
        None,
    )?];
    for index in 0..=8 {
        nodes.push(header_sprite(
            "quests",
            &format!("scbg{index}"),
            assets,
            &format!("SCBG{index:02}I0.IMG"),
        )?);
    }
    for index in [4usize, 5, 7, 8, 9, 10, 11, 12, 13, 14] {
        let asset = require_image(assets, "quests", &format!("SCRL{index:02}I0.IMG"))?;
        let page = UiRect::new(0, 1, 192, 198.min(asset.image.height));
        nodes.push(declared_sprite(
            "quests",
            &format!("scrl{index:02}"),
            assets,
            &format!("SCRL{index:02}I0.IMG"),
            page,
            "SCRL is RCI 320×200 without header: left half page of the journal",
            Some(UiRect::new(0, 0, page.width, page.height)),
        )?);
    }
    nodes.push(glyph_node(
        "quests",
        "quest-count",
        assets,
        DEFAULT_FONT,
        &format!("QUESTS {}", data.quests.len()),
        QUEST_TEXT_X,
        QUEST_TEXT_Y,
        QUEST_TITLE_MAX_WIDTH,
        DEFAULT_INK,
    )?);
    if let Some(title) = data.quests.first() {
        nodes.push(glyph_node(
            "quests",
            "quest-title",
            assets,
            DEFAULT_FONT,
            title,
            QUEST_TEXT_X,
            QUEST_TITLE_Y,
            QUEST_TITLE_MAX_WIDTH,
            DEFAULT_INK,
        )?);
    }
    Ok(UiPanel {
        id: UiPanelId::Quests,
        nodes,
    })
}

/// Main menu panel: `TITL00I0` backdrop + `MENU.IMG` stone margin +
/// `FONT0003` labels + the selection chevron on the active row.
///
/// Layout (320×200 canvas, measured against the retail art):
///
/// | node | asset | source | dest |
/// |---|---|---|---|
/// | title | `TITL00I0` | whole | `0,0,320,200` |
/// | label0..4 | `FONT0003` | glyphs | `248, 44+25k, w, 7` |
/// | selection | `FONT0003` | `>>` glyphs | `240, 44, 7, 7` |
///
/// Two disjoint columns by construction: gutter `212..240` (backdrop only,
/// [`MENU_GUTTER`]), indicator `240..247`, labels `248..287`. No `MENU.IMG`
/// node: its plates are rotuled Normal/Moving/Solid/Ramp/Alternate/Slide/
/// Cycle/Blink — the animation-mode list, never main-menu items — and its left
/// 24 columns are that panel's frame plus its speckled background field, which
/// over the title stone read as a 24×125 band of static (CR-3f6a1d84).
///
/// Menu items are text rows, not buttons: `BUTN00I0` is the confirmation set
/// of the dialogs and is **not** painted here (see [`MENU_SELECTION_TEXT`]).
pub fn build_main_menu_panel(assets: &UiAssets) -> Result<UiPanel, UiSpecError> {
    build_main_menu_panel_with_selection(assets, MENU_ACTIVE_ROW)
}

/// The same composition with an **explicit** active row — the paint a consumer
/// needs when the selection moves.
///
/// [`build_main_menu_panel`] bakes [`MENU_ACTIVE_ROW`] into the panel, so its
/// result is a *picture of one state*: the chevron sits at
/// `MENU_LABEL_Y + row × MENU_LABEL_STRIDE` and only that label carries
/// [`MENU_SELECTION_INK`]. A window that must show a moving selection asks for
/// the row instead of mutating the constant (CR-4a9c1e77).
///
/// `active_row >= MENU_ROWS` is [`UiSpecError::ActiveRowOutOfRange`]: a row
/// that does not exist would otherwise paint the chevron in the void, on
/// nothing, and look like a legitimate selection.
pub fn build_main_menu_panel_with_selection(
    assets: &UiAssets,
    active_row: usize,
) -> Result<UiPanel, UiSpecError> {
    if active_row >= MENU_ROWS {
        return Err(UiSpecError::ActiveRowOutOfRange {
            panel: "main-menu",
            row: active_row,
            rows: MENU_ROWS,
        });
    }
    let mut nodes = vec![declared_sprite(
        "main-menu",
        "title",
        assets,
        "TITL00I0.IMG",
        UiRect::canvas(),
        "TITL00I0 is RCI 320×200 palettized without header: title background",
        None,
    )?];
    for (row, label) in MENU_ITEM_LABELS.iter().enumerate() {
        let ink = if row == active_row {
            MENU_SELECTION_INK
        } else {
            MENU_LABEL_INK
        };
        nodes.push(glyph_node(
            "main-menu",
            &format!("label{row}"),
            assets,
            MENU_FONT,
            label,
            MENU_LABEL_X,
            MENU_LABEL_Y + row as i16 * MENU_LABEL_STRIDE,
            MENU_LABEL_MAX_WIDTH,
            ink,
        )?);
    }
    nodes.push(glyph_node(
        "main-menu",
        "selection",
        assets,
        MENU_FONT,
        MENU_SELECTION_TEXT,
        MENU_SELECTION_X,
        MENU_LABEL_Y + active_row as i16 * MENU_LABEL_STRIDE,
        font_cell_width(assets, MENU_FONT, MENU_SELECTION_TEXT)?,
        MENU_SELECTION_INK,
    )?);
    Ok(UiPanel {
        id: UiPanelId::MainMenu,
        nodes,
    })
}

/// Advance width of `text` in `font_name` — the box the glyphs need, with the
/// per-glyph 1-px advance already included.
fn font_cell_width(
    assets: &UiAssets,
    font_name: &str,
    text: &str,
) -> Result<u16, UiSpecError> {
    let font = require_font(assets, "main-menu", font_name)?;
    Ok(font.text_width(text))
}

/// Builds the three **world** panels of the 1996 UI (`hud`, `inventory`,
/// `quests`) — the production surface of the demo.
///
/// The retail main-menu panel is **not** here: `MENU-MODERN-DESIGN.md` §9.4
/// retires it from production and replaces it with
/// [`crate::menu_spec::MainMenuScreen`] on the 320×180 canvas. It survives only
/// as the `UI-001` retro fixture behind [`build_all_panels`].
pub fn build_world_panels(
    hud: &HudPanelData,
    inventory: &InventoryPanelData,
    quests: &QuestPanelData,
    assets: &UiAssets,
) -> Result<Vec<UiPanel>, UiSpecError> {
    Ok(vec![
        build_hud_panel(hud, assets)?,
        build_inventory_panel(inventory, assets)?,
        build_quests_panel(quests, assets)?,
    ])
}

/// Builds the 4 M17 panels — **retro fixture of `UI-001`, not production**.
///
/// Kept because `PROBE-UI-001` measures the 1996 composition (asset census,
/// palette families, the retail menu plate). The demo composes
/// [`build_world_panels`] + the 320×180 menu screen instead.
pub fn build_all_panels(
    hud: &HudPanelData,
    inventory: &InventoryPanelData,
    quests: &QuestPanelData,
    assets: &UiAssets,
) -> Result<Vec<UiPanel>, UiSpecError> {
    let mut panels = build_world_panels(hud, inventory, quests, assets)?;
    panels.push(build_main_menu_panel(assets)?);
    Ok(panels)
}

/// Full-screen asset of the local map (`320×200`, raw `RCI` — **B3-c**).
pub const MAP_LOCAL_ASSET: &str = "MAP100I0.IMG";
/// Full-screen asset of the world/travel map (`320×200`, raw `RCI` — **B3-c**).
pub const MAP_WORLD_ASSET: &str = "TRAV0I00.IMG";

/// **B3-c (O7)** — the two map panels, in painting order (`map-local`,
/// `map-world`).
///
/// Each panel is the retail full-screen screen asset placed at its own
/// declaration (`0,0`) with no invented layout, no invented text and no
/// widget: 1 sprite node per panel, exactly like the shipped screen.
///
/// **Index-range rule:** both assets are raw `RCI` 320×200 files whose measured
/// non-zero span falls below the `Map` family floor of `63`
/// (`MAP100I0`: `16..=254` · `TRAV0I00`: `33..=254`). The compositor resolves
/// their range **per asset** from `image::ASSET_RANGE_DERIVATIONS` — the
/// measurement, not a global widening (M18 B3-c, `image.rs`).
pub fn build_map_panels(assets: &UiAssets) -> Result<[UiPanel; 2], UiSpecError> {
    let local = UiPanel {
        id: UiPanelId::MapLocal,
        nodes: vec![declared_sprite(
            "map-local",
            "map100",
            assets,
            MAP_LOCAL_ASSET,
            UiRect::canvas(),
            "MAP100I0 is a raw 320×200 RCI (no ImgFile header): the local-map screen fills the canvas",
            None,
        )?],
    };
    let world = UiPanel {
        id: UiPanelId::MapWorld,
        nodes: vec![declared_sprite(
            "map-world",
            "trav00",
            assets,
            MAP_WORLD_ASSET,
            UiRect::canvas(),
            "TRAV0I00 is a raw 320×200 RCI (no ImgFile header): the world/travel map fills the canvas",
            None,
        )?],
    };
    Ok([local, world])
}

/// C7a assert: no node may be a generic widget nor use fonts
/// outside the `.FNT` registry. Returns the violation list (empty = PASS).
pub fn assert_no_generic_widgets(panels: &[UiPanel]) -> Vec<UiSpecError> {
    let mut violations = Vec::new();
    for panel in panels {
        for node in &panel.nodes {
            if !matches!(
                node.kind,
                UiNodeKind::Sprite | UiNodeKind::GlyphRun | UiNodeKind::Fill
            ) {
                violations.push(UiSpecError::GenericWidget {
                    panel: panel.id.as_str(),
                    node: node.id.clone(),
                    what: format!("primitive {:?}", node.kind),
                });
            }
            if node.kind == UiNodeKind::GlyphRun {
                if !FONT_FILES.contains(&node.asset.as_str()) {
                    violations.push(UiSpecError::GenericWidget {
                        panel: panel.id.as_str(),
                        node: node.id.clone(),
                        what: format!("font {} outside the .FNT registry", node.asset),
                    });
                }
                if node.text.is_none() {
                    violations.push(UiSpecError::GenericWidget {
                        panel: panel.id.as_str(),
                        node: node.id.clone(),
                        what: "glyph-run without text".to_string(),
                    });
                }
            } else if node.text.is_some() {
                violations.push(UiSpecError::GenericWidget {
                    panel: panel.id.as_str(),
                    node: node.id.clone(),
                    what: format!("{} with text", node.kind.as_str()),
                });
            }
            // A `Fill`'s contract *is* clipping, so a plate may bleed off an
            // edge by declaration; the canvas rule bounds source-blitted rects.
            if node.kind != UiNodeKind::Fill && !node.rect.fits_canvas() {
                violations.push(UiSpecError::OutOfCanvas {
                    panel: panel.id.as_str(),
                    node: node.id.clone(),
                    rect: node.rect,
                });
            }
        }
    }
    violations
}

/// Resolved palette of a node (with its hash) for the compositor.
fn node_palette(node: &UiNode, assets: &UiAssets) -> (Palette, u64) {
    // `GlyphRun` (a `.FNT` mask) and `Fill` (a rect of one index) carry no
    // image: their colours come from the family palette, never from an asset
    // their `asset` label happens to name.
    if matches!(node.kind, UiNodeKind::GlyphRun | UiNodeKind::Fill) {
        let palette = assets.palette_for(node.family);
        (*palette, palette_hash(palette))
    } else if let Some(asset) = assets.image(&node.asset) {
        (asset.palette, asset.palette_hash)
    } else {
        let palette = assets.palette_for(node.family);
        (*palette, palette_hash(palette))
    }
}

/// Framebuffer palette entry (asset + family + colors).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FramebufferPalette {
    /// Asset (or font) that provides the palette.
    pub asset: String,
    /// Family.
    pub family: PaletteFamily,
    /// FNV-1a64 of the palette.
    pub hash: u64,
    /// Resolved colors.
    pub colors: [[u8; 3]; 256],
}

/// Internal 320×200 framebuffer in palette indices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Framebuffer {
    /// Width (`CANVAS_WIDTH`).
    pub width: u16,
    /// Height (`CANVAS_HEIGHT`).
    pub height: u16,
    /// Row-major palette indices (0 = transparent).
    pub pixels: Vec<u8>,
    /// Palette slot per pixel.
    pub slots: Vec<u8>,
    /// Palettes used.
    pub palettes: Vec<FramebufferPalette>,
}

impl Framebuffer {
    /// Empty framebuffer of the retail canvas (320×200, fully transparent).
    pub fn new() -> Self {
        Self::sized(CANVAS_WIDTH, CANVAS_HEIGHT)
    }

    /// Empty framebuffer of an explicit canvas size, fully transparent.
    ///
    /// The 320×180 menu screen shares this compositor with the 320×200 retail
    /// panels: the size is a parameter, never a second implementation.
    pub fn sized(width: u16, height: u16) -> Self {
        let len = width as usize * height as usize;
        Self {
            width,
            height,
            pixels: vec![0; len],
            slots: vec![0; len],
            palettes: Vec::new(),
        }
    }

    /// Palette index at `(x, y)`.
    pub fn pixel(&self, x: u16, y: u16) -> u8 {
        self.pixels[y as usize * self.width as usize + x as usize]
    }

    /// Palette of the pixel.
    pub fn palette_at(&self, x: u16, y: u16) -> Option<&FramebufferPalette> {
        let slot = self.slots[y as usize * self.width as usize + x as usize];
        self.palettes.get(slot as usize)
    }

    /// Number of opaque pixels (index ≠ 0).
    pub fn opaque_pixels(&self) -> usize {
        self.pixels.iter().filter(|index| **index != 0).count()
    }

    fn slot_for(&mut self, node: &UiNode, assets: &UiAssets) -> u8 {
        let (palette, hash) = node_palette(node, assets);
        if let Some(index) = self
            .palettes
            .iter()
            .position(|entry| entry.asset == node.asset && entry.hash == hash)
        {
            return index as u8;
        }
        self.palettes.push(FramebufferPalette {
            asset: node.asset.clone(),
            family: node.family,
            hash,
            colors: palette.colors,
        });
        (self.palettes.len() - 1) as u8
    }

    /// Paints `nodes` in declaration order onto this framebuffer.
    ///
    /// This is the single compositor of the crate: [`compose`] is a panel
    /// loop over this function, and any later layer (the §3.8 window
    /// composition of `ui_compat`) reuses it instead of blitting again.
    ///
    /// - sprites: 1:1 blit of palette indices from `node.source`
    ///   (`None` = whole asset), index 0 = transparent;
    /// - glyph-runs: 1 bpp `.FNT` mask painted with `node.ink`;
    /// - fills: `node.ink` written on every pixel of the node rect clipped to
    ///   the framebuffer (negative origin paints from 0; zero extent is a
    ///   no-op; no asset is read and nothing is allocated);
    /// - every written pixel is validated against its asset index range
    ///   (C2); the first violation aborts with
    ///   [`UiSpecError::PaletteViolation`].
    ///
    /// A node slice carries no panel identity, so the `panel` field of
    /// [`UiSpecError::MissingAsset`] / [`UiSpecError::MissingFont`] reports
    /// [`NODES_PANEL`].
    pub fn paint_nodes(&mut self, nodes: &[UiNode], assets: &UiAssets) -> Result<(), UiSpecError> {
        for node in nodes {
            let slot = self.slot_for(node, assets);
            match node.kind {
                UiNodeKind::Sprite => {
                    let asset = require_image(assets, NODES_PANEL, &node.asset)?;
                    // **Per-asset range (M18 B3-c):** the declared family/asset
                    // range, widened only for the assets whose own bytes were
                    // measured to exceed it (`image::ASSET_RANGE_DERIVATIONS`).
                    let range = asset_index_range_resolved(&node.asset);
                    let source = node
                        .source
                        .unwrap_or_else(|| UiRect::new(0, 0, asset.image.width, asset.image.height));
                    for dy in 0..node.rect.height {
                        for dx in 0..node.rect.width {
                            let sx = source.x as u16 + dx;
                            let sy = source.y as u16 + dy;
                            let Some(index) = asset.image.pixel(sx, sy) else {
                                continue;
                            };
                            if index == 0 {
                                continue;
                            }
                            if !range.contains(&index) {
                                return Err(UiSpecError::PaletteViolation {
                                    asset: node.asset.clone(),
                                    index,
                                    range: (*range.start(), *range.end()),
                                });
                            }
                            let x = node.rect.x as u16 + dx;
                            let y = node.rect.y as u16 + dy;
                            let at = y as usize * self.width as usize + x as usize;
                            self.pixels[at] = index;
                            self.slots[at] = slot;
                        }
                    }
                }
                UiNodeKind::GlyphRun => {
                    let font = require_font(assets, NODES_PANEL, &node.asset)?;
                    let text = node.text.as_deref().unwrap_or_default();
                    let mask = font.render_text_tracked(text, u16::from(node.tracking));
                    if !family_index_range(node.family).contains(&node.ink) {
                        return Err(UiSpecError::PaletteViolation {
                            asset: node.asset.clone(),
                            index: node.ink,
                            range: (0, 254),
                        });
                    }
                    for dy in 0..mask.height {
                        for dx in 0..mask.width {
                            if !mask.pixel(dx, dy) {
                                continue;
                            }
                            let x = node.rect.x as u16 + dx;
                            let y = node.rect.y as u16 + dy;
                            if x >= self.width || y >= self.height {
                                continue;
                            }
                            let at = y as usize * self.width as usize + x as usize;
                            self.pixels[at] = node.ink;
                            self.slots[at] = slot;
                        }
                    }
                }
                UiNodeKind::Fill => {
                    // **Clip to the node rect ∩ framebuffer.** The maths runs in
                    // `i32` so a negative origin (`x = -5`) paints from column 0
                    // and a rect wider than the canvas cannot wrap a `u16`.
                    let range = fill_ink_range(node.family);
                    if !range.contains(&node.ink) {
                        return Err(UiSpecError::PaletteViolation {
                            asset: FILL_PALETTE_SOURCE.to_string(),
                            index: node.ink,
                            range: (*range.start(), *range.end()),
                        });
                    }
                    let left = i32::from(node.rect.x).max(0);
                    let top = i32::from(node.rect.y).max(0);
                    let right = (i32::from(node.rect.x) + i32::from(node.rect.width))
                        .min(i32::from(self.width));
                    let bottom = (i32::from(node.rect.y) + i32::from(node.rect.height))
                        .min(i32::from(self.height));
                    // Empty range (`w == 0`, `h == 0`, or fully outside) = no-op.
                    for y in top..bottom {
                        let row = y as usize * self.width as usize;
                        for x in left..right {
                            let at = row + x as usize;
                            self.pixels[at] = node.ink;
                            self.slots[at] = slot;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

impl Default for Framebuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// Compositor: paints the panels in order onto a 320×200 framebuffer.
///
/// - sprites: blit of palette indices, index 0 = transparent;
/// - glyph-runs: 1 bpp `.FNT` mask painted with `node.ink`;
/// - fills: a rect of one palette index, clipped to the node rect and to the
///   framebuffer (the modern menu's plates, focus bars and rules);
/// - every written pixel is validated against its asset index range
///   (C2). The first violation aborts with [`UiSpecError::PaletteViolation`].
pub fn compose(panels: &[UiPanel], assets: &UiAssets) -> Result<Framebuffer, UiSpecError> {
    compose_sized(panels, assets, CANVAS_WIDTH, CANVAS_HEIGHT)
}

/// Composes `panels` on a canvas of explicit size.
///
/// [`compose`] is this function with the retail `320×200`; the 320×180 menu
/// screen passes its own canvas and still paints through
/// [`Framebuffer::paint_nodes`] — one compositor, two canvases.
pub fn compose_sized(
    panels: &[UiPanel],
    assets: &UiAssets,
    width: u16,
    height: u16,
) -> Result<Framebuffer, UiSpecError> {
    let mut fb = Framebuffer::sized(width, height);
    for panel in panels {
        fb.paint_nodes(&panel.nodes, assets)?;
    }
    Ok(fb)
}

/// Canvas RGBA (320×200), with index 0 and background composited over black.
pub fn canvas_rgba(fb: &Framebuffer) -> Vec<u8> {
    let mut rgba = vec![0u8; fb.width as usize * fb.height as usize * 4];
    for y in 0..fb.height as usize {
        for x in 0..fb.width as usize {
            let at = y * fb.width as usize + x;
            let index = fb.pixels[at];
            let colors = fb
                .palettes
                .get(fb.slots[at] as usize)
                .map(|entry| entry.colors)
                .unwrap_or([[0u8; 3]; 256]);
            let [r, g, b] = if index == 0 {
                [0u8, 0, 0]
            } else {
                colors[index as usize]
            };
            let out = at * 4;
            rgba[out] = r;
            rgba[out + 1] = g;
            rgba[out + 2] = b;
            rgba[out + 3] = 255;
        }
    }
    rgba
}

/// 640×480 presentation mode (C1b, decided in M17).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentMode {
    /// `320×200 ×2 = 640×400` centered + letterbox 40 + 40.
    Letterbox,
    /// Canvas × integer scale, **no bands**: output is exactly `w·s × h·s`.
    Fullscreen,
}

/// Frame presented at 640×480 (RGBA, integer nearest scaling).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentFrame {
    /// Output width.
    pub width: u16,
    /// Output height.
    pub height: u16,
    /// Integer scale applied.
    pub scale: u8,
    /// Top letterbox.
    pub letterbox_top: u16,
    /// Bottom letterbox.
    pub letterbox_bottom: u16,
    /// Chosen mode.
    pub mode: PresentMode,
    /// Output RGBA.
    pub rgba: Vec<u8>,
}

/// Presents the canvas with integer nearest scaling + letterbox (C1/C1b).
pub fn present_nearest(fb: &Framebuffer) -> PresentFrame {
    let canvas = canvas_rgba(fb);
    let mut rgba = vec![0u8; PRESENT_WIDTH as usize * PRESENT_HEIGHT as usize * 4];
    let scaled = fb.height * u16::from(PRESENT_SCALE);
    let letterbox = PRESENT_HEIGHT.saturating_sub(scaled) / 2;
    for y in 0..fb.height as usize {
        for x in 0..fb.width as usize {
            let at = (y * fb.width as usize + x) * 4;
            let [r, g, b, a] = [
                canvas[at],
                canvas[at + 1],
                canvas[at + 2],
                canvas[at + 3],
            ];
            for sy in 0..PRESENT_SCALE as usize {
                for sx in 0..PRESENT_SCALE as usize {
                    let px = x * PRESENT_SCALE as usize + sx;
                    let py = letterbox as usize + y * PRESENT_SCALE as usize + sy;
                    if px >= PRESENT_WIDTH as usize || py >= PRESENT_HEIGHT as usize {
                        continue;
                    }
                    let out = (py * PRESENT_WIDTH as usize + px) * 4;
                    rgba[out] = r;
                    rgba[out + 1] = g;
                    rgba[out + 2] = b;
                    rgba[out + 3] = a;
                }
            }
        }
    }
    PresentFrame {
        width: PRESENT_WIDTH,
        height: PRESENT_HEIGHT,
        scale: PRESENT_SCALE,
        letterbox_top: letterbox,
        letterbox_bottom: letterbox,
        mode: PresentMode::Letterbox,
        rgba,
    }
}

/// Presents the canvas at an integer scale with **no bands** (menu surface).
///
/// `present_nearest` composes the 640×480 letterboxed window of the retail
/// panels; the main menu covers the whole window instead, so this mode emits
/// exactly `fb.width·scale × fb.height·scale` — at `320×180` and `scale = 4`
/// that is the 1280×720 canvas with `letterbox_top = letterbox_bottom = 0`.
pub fn present_fullscreen(fb: &Framebuffer, scale: u8) -> PresentFrame {
    let canvas = canvas_rgba(fb);
    let width = fb.width.saturating_mul(u16::from(scale));
    let height = fb.height.saturating_mul(u16::from(scale));
    let mut rgba = vec![0u8; width as usize * height as usize * 4];
    for y in 0..fb.height as usize {
        for x in 0..fb.width as usize {
            let at = (y * fb.width as usize + x) * 4;
            let [r, g, b, a] = [
                canvas[at],
                canvas[at + 1],
                canvas[at + 2],
                canvas[at + 3],
            ];
            for sy in 0..scale as usize {
                for sx in 0..scale as usize {
                    let px = x * scale as usize + sx;
                    let py = y * scale as usize + sy;
                    let out = (py * width as usize + px) * 4;
                    rgba[out] = r;
                    rgba[out + 1] = g;
                    rgba[out + 2] = b;
                    rgba[out + 3] = a;
                }
            }
        }
    }
    PresentFrame {
        width,
        height,
        scale,
        letterbox_top: 0,
        letterbox_bottom: 0,
        mode: PresentMode::Fullscreen,
        rgba,
    }
}

/// Rebuilds the canvas from the present with nearest downscale and counts the
/// pixels that differ (C1: Δ = 0 expected).
pub fn present_delta(fb: &Framebuffer, frame: &PresentFrame) -> usize {
    let canvas = canvas_rgba(fb);
    let mut delta = 0usize;
    for y in 0..CANVAS_HEIGHT as usize {
        for x in 0..CANVAS_WIDTH as usize {
            let at = (y * CANVAS_WIDTH as usize + x) * 4;
            let px = x * PRESENT_SCALE as usize;
            let py = LETTERBOX as usize + y * PRESENT_SCALE as usize;
            let out = (py * frame.width as usize + px) * 4;
            if frame.rgba[out..out + 4] != canvas[at..at + 4] {
                delta += 1;
            }
        }
    }
    delta
}

/// Checks that every 2×2 block of the present is uniform (0 interpolation).
pub fn present_interpolated_pixels(frame: &PresentFrame) -> usize {
    let mut interpolated = 0usize;
    for y in 0..CANVAS_HEIGHT as usize {
        for x in 0..CANVAS_WIDTH as usize {
            let px = x * PRESENT_SCALE as usize;
            let py = LETTERBOX as usize + y * PRESENT_SCALE as usize;
            let first = ((py * frame.width as usize) + px) * 4;
            let reference = &frame.rgba[first..first + 4];
            for sy in 0..PRESENT_SCALE as usize {
                for sx in 0..PRESENT_SCALE as usize {
                    let at = ((py + sy) * frame.width as usize + (px + sx)) * 4;
                    if &frame.rgba[at..at + 4] != reference {
                        interpolated += 1;
                    }
                }
            }
        }
    }
    interpolated
}

/// Framebuffer pixels whose index does not belong to their slot palette
/// (independent re-check of the compositor, for the probe).
pub fn palette_compliance(fb: &Framebuffer) -> Vec<(usize, u8, (u8, u8))> {
    let mut violations = Vec::new();
    for (at, index) in fb.pixels.iter().enumerate() {
        if *index == 0 {
            continue;
        }
        let Some(entry) = fb.palettes.get(fb.slots[at] as usize) else {
            violations.push((at, *index, (0, 0)));
            continue;
        };
        let range = asset_index_range_resolved(&entry.asset);
        if !range.contains(index) {
            violations.push((at, *index, (*range.start(), *range.end())));
        }
    }
    violations
}

/// Histogram of the palette families present in the framebuffer.
pub fn family_histogram(fb: &Framebuffer) -> BTreeMap<&'static str, usize> {
    let mut map: BTreeMap<&'static str, usize> = BTreeMap::new();
    for (at, index) in fb.pixels.iter().enumerate() {
        if *index == 0 {
            continue;
        }
        let Some(entry) = fb.palettes.get(fb.slots[at] as usize) else {
            continue;
        };
        let name = match entry.family {
            PaletteFamily::Hud => "hud",
            PaletteFamily::Map => "map",
            PaletteFamily::Art => "art",
            PaletteFamily::Embedded => "embedded",
        };
        *map.entry(name).or_insert(0) += 1;
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use daggerfall_parser_rs::image::{DecodedImage, FntFile, ImageKind};

    fn palette(seed: u8) -> Palette {
        let mut colors = [[0u8; 3]; 256];
        for (index, color) in colors.iter_mut().enumerate() {
            *color = [seed, index as u8, 0];
        }
        Palette { colors }
    }

    fn image(width: u16, height: u16, x: i16, y: i16, fill: u8) -> DecodedImage {
        DecodedImage {
            x_offset: x,
            y_offset: y,
            width,
            height,
            compression: 0,
            pixels: vec![fill; width as usize * height as usize],
            embedded_palette: None,
            kind: ImageKind::ImgHeader,
        }
    }

    fn synthetic_fnt() -> FntFile {
        let mut bytes = vec![0u8; daggerfall_parser_rs::image::FNT_FILE_LEN];
        bytes[0..2].copy_from_slice(&14u16.to_le_bytes());
        bytes[2..4].copy_from_slice(&11u16.to_le_bytes());
        for index in 0..240usize {
            let offset = (4 + 240 * 4 + index * 32) as u16;
            let entry = 4 + index * 4;
            bytes[entry..entry + 2].copy_from_slice(&offset.to_le_bytes());
            bytes[entry + 2..entry + 4].copy_from_slice(&5u16.to_le_bytes());
        }
        // Non-empty glyphs for the composition test: set bit 15 of row 0
        // (`bytes[start + 1]`), i.e. pixel x=0, the only one a 5-px glyph
        // reaches. Bit 7 (`bytes[start]`) would decode to x=8 and paint
        // nothing (CR-4a9c1e77: the previous fixture masked no ink at all).
        for index in 0..240usize {
            let base = 964 + index * 32;
            bytes[base + 1] = 0x80;
        }
        FntFile::parse(&bytes).expect("synthetic fnt")
    }

    /// `FONT0003`-shaped font: 5×7 cell, 3-px glyphs → `NEW GAME` = 32 px,
    /// which fits the 39-px label column of the main menu.
    fn synthetic_menu_fnt() -> FntFile {
        let mut bytes = vec![0u8; daggerfall_parser_rs::image::FNT_FILE_LEN];
        bytes[0..2].copy_from_slice(&5u16.to_le_bytes());
        bytes[2..4].copy_from_slice(&7u16.to_le_bytes());
        for index in 0..240usize {
            let offset = (4 + 240 * 4 + index * 32) as u16;
            let entry = 4 + index * 4;
            bytes[entry..entry + 2].copy_from_slice(&offset.to_le_bytes());
            bytes[entry + 2..entry + 4].copy_from_slice(&3u16.to_le_bytes());
        }
        // Real ink: bit 15 of row 0 = pixel x=0 (see `synthetic_fnt`).
        for index in 0..240usize {
            let base = 964 + index * 32;
            bytes[base + 1] = 0x80;
        }
        FntFile::parse(&bytes).expect("synthetic menu fnt")
    }

    fn assets() -> UiAssets {
        let hud = palette(10);
        let mut assets = UiAssets::new(hud, palette(20), palette(30));
        for name in [
            "MAIN00I0.IMG",
            "MAIN02I0.IMG",
            "MAIN03I0.IMG",
            "COMPASS.IMG",
            "INVE00I0.IMG",
            "INVE03I0.IMG",
            "ITEM00I0.IMG",
            "ITEM01I0.IMG",
            "BOOK00I0.IMG",
            "MENU.IMG",
            "BUTN00I0.IMG",
            "BUTN03I0.IMG",
        ] {
            let (width, height, x, y) = match name {
                "MAIN00I0.IMG" => (320, 46, 0, 154),
                "MAIN02I0.IMG" => (190, 46, 83, 154),
                "MAIN03I0.IMG" => (4, 32, 49, 161),
                "COMPASS.IMG" => (322, 14, 0, 0),
                "INVE03I0.IMG" => (39, 66, 222, 10),
                "MENU.IMG" => (108, 258, 416, 17),
                "BUTN00I0.IMG" => (32, 128, 0, 0),
                "BUTN03I0.IMG" => (22, 27, 0, 0),
                _ => (320, 200, 0, 0),
            };
            assets = assets
                .with_image(name, image(width, height, x, y, 1))
                .expect("synthetic asset");
        }
        for index in 6..=14 {
            let name = format!("INVE{index:02}I0.IMG");
            assets = assets
                .with_image(&name, image(39, 190, 222, 10, 1))
                .expect("synthetic slot");
        }
        for index in 0..=8 {
            let name = format!("SCBG{index:02}I0.IMG");
            assets = assets
                .with_image(&name, image(125, 198, 192, 1, 1))
                .expect("synthetic scbg");
        }
        for index in [4usize, 5, 7, 8, 9, 10, 11, 12, 13, 14] {
            let name = format!("SCRL{index:02}I0.IMG");
            assets = assets
                .with_image(&name, image(320, 200, 0, 0, 1))
                .expect("synthetic scrl");
        }
        // TITL00I0 is the only `Embedded` family: its PalFile travels in the asset.
        let mut title = image(320, 200, 0, 0, 1);
        title.embedded_palette = Some(palette(40));
        assets = assets
            .with_image("TITL00I0.IMG", title)
            .expect("synthetic titl");
        assets = assets.with_font(DEFAULT_FONT, synthetic_fnt());
        assets.with_font(MENU_FONT, synthetic_menu_fnt())
    }

    fn hud_data() -> HudPanelData {
        HudPanelData {
            health: 100,
            max_health: 100,
            stamina: 80,
            max_stamina: 100,
            level: 3,
        }
    }

    #[test]
    fn four_panels_build_with_header_and_declared_anchors() {
        let assets = assets();
        let panels = build_all_panels(
            &hud_data(),
            &InventoryPanelData {
                gold: 120,
                item_kinds: 2,
                total_units: 5,
            },
            &QuestPanelData {
                quests: vec!["A quest".to_string()],
            },
            &assets,
        )
        .expect("4 panels");
        assert_eq!(panels.len(), 4);
        let hud = &panels[0];
        let main00 = hud.nodes.iter().find(|node| node.id == "main00").expect("main00");
        assert_eq!(main00.rect, UiRect::new(0, 154, 320, 46));
        assert_eq!(main00.anchor, UiAnchor::Header);
        let compass = hud
            .nodes
            .iter()
            .find(|node| node.id == "compass")
            .expect("compass");
        assert!(matches!(compass.anchor, UiAnchor::Declared(_)));
        assert!(assert_no_generic_widgets(&panels).is_empty());
        for panel in &panels {
            let inventory = panel.node_inventory();
            assert!(inventory.keys().all(|kind| ["sprite", "glyph-run"].contains(kind)));
            assert_eq!(inventory.values().sum::<usize>(), panel.nodes.len());
        }
    }

    #[test]
    fn menu_layout_has_disjoint_columns() {
        let assets = assets();
        let panel = build_main_menu_panel(&assets).expect("menu");
        // The strip left of the menu column carries the declared backdrop and
        // nothing else: `MENU.IMG`'s left 24 columns are that panel's frame
        // plus its speckled background, never a menu margin (CR-3f6a1d84).
        assert_eq!(MENU_GUTTER, UiRect::new(212, 35, 28, MENU_CLIP_HEIGHT));
        assert_eq!(MENU_GUTTER.right(), MENU_SELECTION_X);
        for node in panel.nodes.iter().filter(|node| node.id != "title") {
            assert!(
                node.rect.x >= MENU_GUTTER.right(),
                "{} paints in the menu gutter {:?}",
                node.id,
                MENU_GUTTER
            );
        }
        assert!(
            !panel.nodes.iter().any(|node| node.id == "menu-panel"),
            "the MENU.IMG band is not part of the main menu"
        );
        for (row, label) in MENU_ITEM_LABELS.iter().enumerate() {
            let node = panel
                .nodes
                .iter()
                .find(|node| node.id == format!("label{row}"))
                .expect("label");
            assert_eq!(node.kind, UiNodeKind::GlyphRun);
            assert_eq!(node.asset, MENU_FONT);
            assert_eq!(node.rect.y, MENU_LABEL_Y + row as i16 * MENU_LABEL_STRIDE);
            assert_eq!(node.rect.height, 7);
            assert_eq!(node.text.as_deref(), Some(*label));
            assert!(node.rect.x >= MENU_GUTTER.right(), "label {row} starts left of the band");
            let ink = if row == MENU_ACTIVE_ROW {
                MENU_SELECTION_INK
            } else {
                MENU_LABEL_INK
            };
            assert_eq!(node.ink, ink, "label {row} ink");
        }
        // The menu is five labels + one chevron: no dialog plate may appear
        // (they are `ACCEPT/REJECT/CANCEL/YES…`, not main-menu items).
        for node in &panel.nodes {
            assert!(
                !node.asset.starts_with("BUTN00"),
                "{} paints a dialog button in the main menu",
                node.id
            );
        }
        let labels: Vec<&str> = panel
            .nodes
            .iter()
            .filter(|node| node.id.starts_with("label"))
            .map(|node| node.text.as_deref().unwrap_or(""))
            .collect();
        assert_eq!(labels, MENU_ITEM_LABELS.to_vec(), "menu labels out of order");
        let selection: Vec<&UiNode> = panel
            .nodes
            .iter()
            .filter(|node| node.id == "selection")
            .collect();
        assert_eq!(selection.len(), 1, "exactly one selection indicator");
        let selection = selection[0];
        assert_eq!(selection.kind, UiNodeKind::GlyphRun);
        assert_eq!(selection.asset, MENU_FONT);
        assert_eq!(selection.text.as_deref(), Some(MENU_SELECTION_TEXT));
        assert_eq!(selection.ink, MENU_SELECTION_INK);
        assert_eq!(selection.rect.y, MENU_LABEL_Y, "chevron on the active row");
        assert!(
            selection.rect.right() <= MENU_LABEL_X,
            "chevron ends before the words"
        );
        assert_eq!(selection.rect.height, 7);
        // No rect pair overlaps with area > 0 (the `title` backdrop covers the
        // whole canvas by design, so it is excluded), and every node stays inside.
        let foreground: Vec<&UiNode> =
            panel.nodes.iter().filter(|node| node.id != "title").collect();
        for (index, left) in foreground.iter().enumerate() {
            assert!(left.rect.fits_canvas(), "{} outside canvas", left.id);
            for right in foreground.iter().skip(index + 1) {
                let ix = left.rect.right().min(right.rect.right()) - left.rect.x.max(right.rect.x);
                let iy = left.rect.bottom().min(right.rect.bottom()) - left.rect.y.max(right.rect.y);
                assert!(
                    ix <= 0 || iy <= 0,
                    "{} overlaps {} ({}×{})",
                    left.id,
                    right.id,
                    ix,
                    iy
                );
            }
        }
    }

    #[test]
    fn menu_selection_paints_the_active_row() {
        let assets = assets();
        assert_eq!(MENU_ROWS, MENU_ITEM_LABELS.len(), "one painted state per row");
        // The default composition IS the `MENU_ACTIVE_ROW` picture: every
        // consumer that reads `build_main_menu_panel` (probe legs, `MENU-001`)
        // keeps measuring the same panel it always measured.
        assert_eq!(
            build_main_menu_panel(&assets).expect("menu"),
            build_main_menu_panel_with_selection(&assets, MENU_ACTIVE_ROW).expect("active row")
        );
        let mut pictures = Vec::with_capacity(MENU_ROWS);
        for row in 0..MENU_ROWS {
            let panel = build_main_menu_panel_with_selection(&assets, row).expect("row");
            let gold: Vec<String> = panel
                .nodes
                .iter()
                .filter(|node| node.id.starts_with("label") && node.ink == MENU_SELECTION_INK)
                .map(|node| node.id.clone())
                .collect();
            assert_eq!(gold, vec![format!("label{row}")], "row {row} gold labels");
            let selection = panel
                .nodes
                .iter()
                .find(|node| node.id == "selection")
                .expect("chevron");
            assert_eq!(
                selection.rect.y,
                MENU_LABEL_Y + row as i16 * MENU_LABEL_STRIDE,
                "chevron on row {row}"
            );
            pictures.push(compose(std::slice::from_ref(&panel), &assets).expect("composed"));
        }
        // The chevron and the gold ink move together: every row is a DIFFERENT
        // picture, counted in pixels (a chevron that stays put would leave two
        // identical framebuffers here).
        for left in 0..MENU_ROWS {
            for right in (left + 1)..MENU_ROWS {
                let a = &pictures[left];
                let b = &pictures[right];
                let differing = (0..CANVAS_HEIGHT)
                    .flat_map(|y| (0..CANVAS_WIDTH).map(move |x| (x, y)))
                    .filter(|(x, y)| a.pixel(*x, *y) != b.pixel(*x, *y))
                    .count();
                assert!(
                    differing > 0,
                    "rows {left} and {right} compose the same picture"
                );
            }
        }
        // A row that does not exist is an error, never a chevron over nothing.
        match build_main_menu_panel_with_selection(&assets, MENU_ROWS) {
            Err(UiSpecError::ActiveRowOutOfRange { row, rows, .. }) => {
                assert_eq!((row, rows), (MENU_ROWS, MENU_ROWS));
            }
            other => panic!("row {MENU_ROWS} must be rejected, got {other:?}"),
        }
    }

    fn fnv1a64(bytes: &[u8]) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }

    /// Digest of a composed picture: canvas size, palette indices and the
    /// palette slot of every pixel (the provenance, not only the colour).
    fn picture_digest(fb: &Framebuffer) -> u64 {
        let mut bytes = Vec::with_capacity(2 + 2 + fb.pixels.len() + fb.slots.len());
        bytes.extend_from_slice(&fb.width.to_le_bytes());
        bytes.extend_from_slice(&fb.height.to_le_bytes());
        bytes.extend_from_slice(&fb.pixels);
        bytes.extend_from_slice(&fb.slots);
        for palette in &fb.palettes {
            bytes.extend_from_slice(palette.asset.as_bytes());
            bytes.extend_from_slice(&palette.hash.to_le_bytes());
        }
        fnv1a64(&bytes)
    }

    /// Digest of the declared composition (no paint involved): every field of
    /// every node in declaration order.
    fn declaration_digest(panel: &UiPanel) -> u64 {
        fnv1a64(format!("{panel:?}").as_bytes())
    }

    #[test]
    fn menu_composition_is_byte_identical() {
        let assets = assets();
        let panel = build_main_menu_panel(&assets).expect("menu");
        let composed = compose(std::slice::from_ref(&panel), &assets).expect("composed");
        assert_eq!(
            declaration_digest(&panel),
            3_895_403_110_225_419_952,
            "declaration digest"
        );
        assert_eq!(
            picture_digest(&composed),
            17_916_637_375_780_639_113,
            "picture digest"
        );
    }

    #[test]
    fn fixture_selection_digests_are_unchanged() {
        let assets = assets();
        let mut rows = Vec::new();
        // (row, FNV-1a over the composed indices, chevron y) — the same pair of
        // rows and the same formula as `PROBE-MENU-LAYOUT-001`, but composed
        // over the synthetic `assets()` fixture of this module: the probe loads
        // retail `arena2`, so its numbers (`selection_diff_px = 174`) measure
        // other glyph masks and are deliberately NOT comparable. Measured at
        // HEAD `2ba74c6` BEFORE `Fill` existed.
        for (row, sha, chevron_y) in [
            (MENU_ACTIVE_ROW, 0x88f4_6531_b59e_7535u64, 44i16),
            (3usize, 0x2eec_b7c0_14bf_1315u64, 119),
        ] {
            let panel = build_main_menu_panel_with_selection(&assets, row).expect("row");
            let chevron = panel
                .nodes
                .iter()
                .find(|node| node.id == "selection")
                .expect("chevron node");
            assert_eq!(chevron.rect.y, chevron_y, "row {row} chevron y");
            let framebuffer = compose(std::slice::from_ref(&panel), &assets).expect("row picture");
            assert_eq!(
                fnv1a64(&framebuffer.pixels),
                sha,
                "selection_sha_row{row}_fixture"
            );
            rows.push(framebuffer);
        }
        let differing = rows[0]
            .pixels
            .iter()
            .zip(&rows[1].pixels)
            .filter(|(left, right)| left != right)
            .count();
        assert_eq!(differing, 18, "selection_diff_px_fixture");
    }

    #[test]
    fn compose_is_deterministic_and_palette_clean() {
        let assets = assets();
        let panels = build_all_panels(
            &hud_data(),
            &InventoryPanelData {
                gold: 7,
                item_kinds: 1,
                total_units: 1,
            },
            &QuestPanelData { quests: Vec::new() },
            &assets,
        )
        .expect("panels");
        let first = compose(&panels, &assets).expect("composition");
        let second = compose(&panels, &assets).expect("composition");
        assert_eq!(first, second);
        assert!(palette_compliance(&first).is_empty());
        assert!(first.opaque_pixels() > 0);
        assert!(!first.palettes.is_empty());
    }

    #[test]
    fn present_is_letterboxed_nearest_and_lossless() {
        let assets = assets();
        let panels = build_all_panels(
            &hud_data(),
            &InventoryPanelData {
                gold: 1,
                item_kinds: 1,
                total_units: 1,
            },
            &QuestPanelData { quests: Vec::new() },
            &assets,
        )
        .expect("panels");
        let fb = compose(&panels, &assets).expect("composition");
        let frame = present_nearest(&fb);
        assert_eq!((frame.width, frame.height), (640, 480));
        assert_eq!((frame.letterbox_top, frame.letterbox_bottom), (40, 40));
        assert_eq!(frame.scale, 2);
        assert_eq!(frame.mode, PresentMode::Letterbox);
        assert_eq!(present_delta(&fb, &frame), 0);
        assert_eq!(present_interpolated_pixels(&frame), 0);
    }

    #[test]
    fn palette_violation_aborts_composition() {
        let mut assets = assets();
        for asset in assets.images.iter_mut() {
            if asset.name == "MAIN00I0.IMG" {
                asset.image.pixels[0] = 255;
            }
        }
        let result = build_hud_panel(&hud_data(), &assets);
        assert!(result.is_ok());
        let panels = vec![result.expect("panel")];
        assert!(matches!(
            compose(&panels, &assets),
            Err(UiSpecError::PaletteViolation { .. })
        ));
    }

    #[test]
    fn missing_asset_is_typed_error() {
        let assets = UiAssets::new(palette(1), palette(2), palette(3));
        assert!(matches!(
            build_hud_panel(&hud_data(), &assets),
            Err(UiSpecError::MissingAsset { .. })
        ));
    }

    #[test]
    fn fit_text_truncates_to_width() {
        let font = synthetic_fnt();
        let fitted = fit_text(&font, "AAAAAAAAAA", 20);
        assert!(font.text_width(&fitted) <= 20);
        assert!(fitted.len() < 10);
    }
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;
    // ── `Fill`: the primitive a 1×1 sprite cannot express ────────────────────

    /// `Fill` node built by hand, bypassing [`fill_node`]'s validation: every
    /// `UiNode` field is public, so the compositor must refuse a bad one too.
    fn raw_fill(x: i16, y: i16, w: u16, h: u16, family: PaletteFamily, ink: u8) -> UiNode {
        UiNode {
            id: "raw-fill".to_string(),
            kind: UiNodeKind::Fill,
            rect: UiRect::new(x, y, w, h),
            anchor: UiAnchor::Declared("test"),
            asset: FILL_PALETTE_SOURCE.to_string(),
            family,
            palette_hash: 0,
            source: None,
            text: None,
            tracking: 0,
            ink,
        }
    }

    fn paint_one(fb: &mut Framebuffer, node: &UiNode, assets: &UiAssets) {
        fb.paint_nodes(std::slice::from_ref(node), assets)
            .expect("paint");
    }

    /// Every canvas pixel is `ink` inside `x0..x1 × y0..y1` and untouched (0)
    /// outside it — read pixel by pixel, no counting shortcuts.
    fn assert_only_this_rect(fb: &Framebuffer, x0: i32, y0: i32, x1: i32, y1: i32, ink: u8) {
        for y in 0..CANVAS_HEIGHT {
            for x in 0..CANVAS_WIDTH {
                let inside = i32::from(x) >= x0
                    && i32::from(x) < x1
                    && i32::from(y) >= y0
                    && i32::from(y) < y1;
                assert_eq!(
                    fb.pixel(x, y),
                    if inside { ink } else { 0 },
                    "pixel ({x},{y}) of a {x0}..{x1} × {y0}..{y1} fill"
                );
            }
        }
    }

    #[test]
    fn fill_paints_exactly_the_requested_rect() {
        let assets = assets();
        let node = fill_node(
            "plate",
            &assets,
            UiRect::new(10, 20, 5, 4),
            PaletteFamily::Hud,
            127,
        )
        .expect("fill");
        assert_eq!(node.kind, UiNodeKind::Fill);
        assert_eq!(node.kind.as_str(), "fill");
        assert_eq!(node.asset, FILL_PALETTE_SOURCE);
        assert_eq!(node.ink, 127);

        let mut fb = Framebuffer::new();
        paint_one(&mut fb, &node, &assets);
        assert_only_this_rect(&fb, 10, 20, 15, 24, 127);
        assert_eq!(fb.opaque_pixels(), 5 * 4);
        // Provenance: a fill paints with the FAMILY palette, never with an
        // asset's slot.
        let palette = fb.palette_at(10, 20).expect("palette slot");
        assert_eq!(palette.family, PaletteFamily::Hud);
        assert_eq!(
            palette.hash,
            palette_hash(assets.palette_for(PaletteFamily::Hud))
        );
    }

    #[test]
    fn fill_clips_on_each_of_the_four_edges() {
        let assets = assets();
        // (declared rect, surviving columns, surviving rows, ink)
        let cases = [
            (UiRect::new(-5, 10, 8, 3), (0, 3), (10, 13), 100u8),
            (UiRect::new(318, 10, 8, 3), (318, 320), (10, 13), 101),
            (UiRect::new(10, -3, 4, 8), (10, 14), (0, 5), 102),
            (UiRect::new(10, 197, 4, 8), (10, 14), (197, 200), 103),
        ];
        for (rect, (x0, x1), (y0, y1), ink) in cases {
            let node = fill_node("clip", &assets, rect, PaletteFamily::Hud, ink).expect("fill");
            let mut fb = Framebuffer::new();
            paint_one(&mut fb, &node, &assets);
            assert_only_this_rect(&fb, x0, y0, x1, y1, ink);
            let expected = ((x1 - x0) * (y1 - y0)) as usize;
            assert_eq!(
                fb.opaque_pixels(),
                expected,
                "{rect:?} writes {expected} px"
            );
        }
    }

    #[test]
    fn fill_clips_at_the_corner_and_never_writes_outside() {
        let assets = assets();
        // Corner: half of the declared columns/rows on each axis survive.
        let corner = fill_node(
            "corner",
            &assets,
            UiRect::new(-5, -5, 10, 10),
            PaletteFamily::Hud,
            90,
        )
        .expect("fill");
        let mut fb = Framebuffer::new();
        paint_one(&mut fb, &corner, &assets);
        assert_only_this_rect(&fb, 0, 0, 5, 5, 90);
        assert_eq!(fb.opaque_pixels(), 25);

        // A node entirely outside one edge: no write, no panic.
        for rect in [
            UiRect::new(-20, 0, 10, 10),
            UiRect::new(0, -20, 10, 10),
            UiRect::new(400, 0, 10, 10),
            UiRect::new(0, 400, 10, 10),
            UiRect::new(-20, -20, 10, 10),
        ] {
            let node = fill_node("off", &assets, rect, PaletteFamily::Hud, 91).expect("fill");
            let mut fb = Framebuffer::new();
            paint_one(&mut fb, &node, &assets);
            assert_eq!(
                fb.opaque_pixels(),
                0,
                "{rect:?} wrote outside the framebuffer"
            );
        }

        // Wider than the canvas in `i32` terms: `x + w` must not wrap in
        // `i16`, and the writes stop at the last column.
        let wide = raw_fill(300, 4, 60_000, 2, PaletteFamily::Hud, 92);
        let mut fb = Framebuffer::new();
        paint_one(&mut fb, &wide, &assets);
        assert_only_this_rect(&fb, 300, 4, 320, 6, 92);
    }

    #[test]
    fn fill_with_zero_extent_is_a_silent_noop() {
        let assets = assets();
        let mut fb = Framebuffer::new();
        let plate = fill_node(
            "plate",
            &assets,
            UiRect::new(4, 4, 6, 6),
            PaletteFamily::Hud,
            100,
        )
        .expect("fill");
        paint_one(&mut fb, &plate, &assets);
        assert_eq!(fb.opaque_pixels(), 36);

        for rect in [
            UiRect::new(4, 4, 0, 6),
            UiRect::new(4, 4, 6, 0),
            UiRect::new(4, 4, 0, 0),
        ] {
            let empty = fill_node("empty", &assets, rect, PaletteFamily::Hud, 200).expect("fill");
            assert_eq!(
                fb.paint_nodes(std::slice::from_ref(&empty), &assets),
                Ok(()),
                "{rect:?} must be a silent no-op, not an error"
            );
            // Not one pixel of the plate under it moved to the new ink.
            assert_only_this_rect(&fb, 4, 4, 10, 10, 100);
        }
    }

    #[test]
    fn fill_ink_zero_is_rejected() {
        let assets = assets();
        // One range rule: a fill may paint `family \ {0}` (0 is transparency),
        // so `ink = 0` is the same typed C2 violation an out-of-range sprite
        // index reports — the error names the range it enforced.
        assert_eq!(fill_ink_range(PaletteFamily::Hud), 1..=254);
        assert_eq!(fill_ink_range(PaletteFamily::Art), 1..=255);
        assert_eq!(fill_ink_range(PaletteFamily::Map), 63..=254);
        match fill_node(
            "hole",
            &assets,
            UiRect::new(0, 0, 4, 4),
            PaletteFamily::Hud,
            0,
        ) {
            Err(UiSpecError::PaletteViolation {
                asset,
                index,
                range,
            }) => {
                assert_eq!(asset, FILL_PALETTE_SOURCE);
                assert_eq!((index, range), (0, (1, 254)));
            }
            other => panic!("ink 0 must be rejected at construction, got {other:?}"),
        }
        // … and at paint, for a node whose fields were written by hand.
        let hole = raw_fill(0, 0, 4, 4, PaletteFamily::Hud, 0);
        let mut fb = Framebuffer::new();
        assert!(matches!(
            fb.paint_nodes(std::slice::from_ref(&hole), &assets),
            Err(UiSpecError::PaletteViolation { index: 0, .. })
        ));
        assert_eq!(fb.opaque_pixels(), 0, "a rejected fill painted nothing");
    }

    #[test]
    fn fill_ink_outside_its_family_is_a_palette_violation() {
        let assets = assets();
        // `Map` is 63..=254: index 10 is outside the family.
        match fill_node(
            "bad",
            &assets,
            UiRect::new(0, 0, 4, 4),
            PaletteFamily::Map,
            10,
        ) {
            Err(UiSpecError::PaletteViolation {
                asset,
                index,
                range,
            }) => {
                assert_eq!(asset, FILL_PALETTE_SOURCE);
                assert_eq!((index, range), (10, (63, 254)));
            }
            other => panic!("map ink 10 must be rejected at construction, got {other:?}"),
        }
        // The compositor refuses the same node, whichever way it was built.
        let bad = raw_fill(0, 0, 4, 4, PaletteFamily::Map, 10);
        let mut fb = Framebuffer::new();
        assert!(matches!(
            fb.paint_nodes(std::slice::from_ref(&bad), &assets),
            Err(UiSpecError::PaletteViolation { index: 10, .. })
        ));
        assert_eq!(fb.opaque_pixels(), 0);
    }

    // Gated by a thread-local flag: only the measuring thread counts, so the
    // suite keeps running in parallel and each test measures its own work.
    thread_local! {
        static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
        static COUNTING: Cell<bool> = const { Cell::new(false) };
    }

    struct CountingAllocator;

    // SAFETY: forwards every call to `System` unchanged; the counters are
    // thread-local `Cell`s and never allocate.
    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            if COUNTING.with(|flag| flag.get()) {
                ALLOCATIONS.with(|count| count.set(count.get() + 1));
            }
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) }
        }
    }

    #[global_allocator]
    static COUNTING_ALLOCATOR: CountingAllocator = CountingAllocator;

    #[test]
    fn fill_allocates_nothing_per_frame() {
        let assets = assets();
        let node = fill_node(
            "plate",
            &assets,
            UiRect::new(8, 8, 64, 32),
            PaletteFamily::Hud,
            120,
        )
        .expect("fill");
        let mut fb = Framebuffer::new();
        // First paint resolves the palette slot (one `push`, per composition,
        // not per frame).
        paint_one(&mut fb, &node, &assets);
        let before = (fb.pixels.clone(), fb.palettes.len());

        ALLOCATIONS.with(|count| count.set(0));
        COUNTING.with(|flag| flag.set(true));
        let painted = fb.paint_nodes(std::slice::from_ref(&node), &assets);
        COUNTING.with(|flag| flag.set(false));
        let allocations = ALLOCATIONS.with(|count| count.get());

        assert_eq!(painted, Ok(()));
        assert_eq!(
            allocations, 0,
            "repainting the same 64×32 fill allocated {allocations} times"
        );
        // Repainted over itself in place: same indices, same palette table.
        assert_eq!((fb.pixels.clone(), fb.palettes.len()), before);
    }

    #[test]
    fn fill_matches_the_sprite_path_and_differs_from_a_one_pixel_source() {
        let mut assets = assets();
        // A flat source: the existing path CAN cover this rect from it.
        assets = assets
            .with_image("FLAT00I0.IMG", image(320, 200, 0, 0, 7))
            .expect("flat source");
        // A single-pixel source: the naive "fill" that leaves holes.
        assets = assets
            .with_image("DOT00I0.IMG", image(1, 1, 0, 0, 7))
            .expect("dot");

        let rect = UiRect::new(20, 30, 40, 12);
        let by_sprite = {
            let node = declared_sprite(
                "test",
                "flat",
                &assets,
                "FLAT00I0.IMG",
                rect,
                "flat test source",
                Some(UiRect::new(0, 0, rect.width, rect.height)),
            )
            .expect("sprite");
            let mut fb = Framebuffer::new();
            paint_one(&mut fb, &node, &assets);
            fb
        };
        let by_fill = {
            let node = fill_node("fill", &assets, rect, PaletteFamily::Hud, 7).expect("fill");
            let mut fb = Framebuffer::new();
            paint_one(&mut fb, &node, &assets);
            fb
        };
        let by_dot = {
            let node = declared_sprite(
                "test",
                "dot",
                &assets,
                "DOT00I0.IMG",
                rect,
                "1×1 test dot",
                None,
            )
            .expect("dot");
            let mut fb = Framebuffer::new();
            paint_one(&mut fb, &node, &assets);
            fb
        };

        // Control: `Fill` and the sprite path paint the SAME pixels.
        assert_eq!(by_fill.pixels, by_sprite.pixels, "fill and sprite disagree");
        assert_eq!(by_fill.opaque_pixels(), 40 * 12);
        // The 1×1 source: one pixel of the 480 and 479 holes (no `Repeat`).
        assert_eq!(by_dot.opaque_pixels(), 1, "a 1×1 sprite left holes");
        let differing = by_fill
            .pixels
            .iter()
            .zip(&by_dot.pixels)
            .filter(|(fill, dot)| fill != dot)
            .count();
        assert_eq!(
            differing,
            40 * 12 - 1,
            "fill must differ from the 1×1 sprite exactly where it filled"
        );
    }

    #[test]
    fn fill_is_a_primitive_not_a_generic_widget() {
        let assets = assets();
        // A plate that bleeds off the left edge is legal for `Fill`: clipping
        // is its contract, so the canvas-fit rule (which bounds the window of
        // a source-blitted rect) does not apply to it.
        let bleed = fill_node(
            "bleed",
            &assets,
            UiRect::new(-5, 0, 40, 4),
            PaletteFamily::Hud,
            221,
        )
        .expect("fill");
        let panel = UiPanel {
            id: UiPanelId::MainMenu,
            nodes: vec![bleed],
        };
        let violations = assert_no_generic_widgets(std::slice::from_ref(&panel));
        assert!(
            violations.is_empty(),
            "fill flagged as a widget: {violations:?}"
        );

        // Text on a fill is the C7a violation (a text node is a `GlyphRun`).
        let mut with_text = fill_node(
            "labelled",
            &assets,
            UiRect::new(4, 4, 8, 4),
            PaletteFamily::Hud,
            221,
        )
        .expect("fill");
        with_text.text = Some("x".to_string());
        let panel = UiPanel {
            id: UiPanelId::MainMenu,
            nodes: vec![with_text],
        };
        let violations = assert_no_generic_widgets(std::slice::from_ref(&panel));
        assert!(matches!(
            violations.first(),
            Some(UiSpecError::GenericWidget { what, .. }) if what == "fill with text"
        ));
    }
}

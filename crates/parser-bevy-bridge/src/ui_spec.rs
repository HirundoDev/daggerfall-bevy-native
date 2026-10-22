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
    asset_index_range, family_index_range, DecodedImage, FntFile, PaletteFamily,
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

/// Button cell of `BUTN00I0.IMG` (32×128 → 4 cells of 32×32, [INFERENCE]).
pub const BUTTON_CELL: u16 = 32;

/// Vertical clip of `MENU.IMG` (258 high from y=17 → 183 visible).
pub const MENU_CLIP_HEIGHT: u16 = CANVAS_HEIGHT - 17;

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
const MENU_Y: i16 = 17;
const MENU_BUTTON_X: i16 = 270;
const MENU_BUTTON_Y: i16 = 40;
const MENU_BUTTON_STRIDE: i16 = 34;
const MENU_EXTRA_BUTTON_X: i16 = 280;
const MENU_EXTRA_BUTTON_Y: i16 = 160;

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
}

impl UiNodeKind {
    /// Stable name (for the inventory and the asserts).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Sprite => "sprite",
            Self::GlyphRun => "glyph-run",
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
}

impl UiPanelId {
    /// Stable name.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Hud => "hud",
            Self::Inventory => "inventory",
            Self::Quests => "quests",
            Self::MainMenu => "main-menu",
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

    /// Node kind inventory (`sprite`, `glyph-run`).
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
    })
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
    let rect = UiRect::new(x, y, font.text_width(&text), font.fixed_height);
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
        text: Some(text),
        ink,
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

/// Main menu panel: `TITL00I0` + `MENU.IMG` (transformed) + buttons.
pub fn build_main_menu_panel(assets: &UiAssets) -> Result<UiPanel, UiSpecError> {
    let mut nodes = vec![declared_sprite(
        "main-menu",
        "title",
        assets,
        "TITL00I0.IMG",
        UiRect::canvas(),
        "TITL00I0 is RCI 320×200 palettized without header: title background",
        None,
    )?];
    let menu = require_image(assets, "main-menu", "MENU.IMG")?;
    nodes.push(declared_sprite(
        "main-menu",
        "menu-panel",
        assets,
        "MENU.IMG",
        UiRect::new(MENU_X, MENU_Y, 108, MENU_CLIP_HEIGHT),
        "MENU.IMG declares (416,17,108,258) OUTSIDE the 320×200 canvas: declared transform x=320−108, y=17, height clipped to 183",
        Some(UiRect::new(0, 0, menu.image.width, menu.image.height.min(MENU_CLIP_HEIGHT))),
    )?);
    for cell in 0..4u16 {
        nodes.push(declared_sprite(
            "main-menu",
            &format!("button{cell}"),
            assets,
            "BUTN00I0.IMG",
            UiRect::new(
                MENU_BUTTON_X,
                MENU_BUTTON_Y + cell as i16 * MENU_BUTTON_STRIDE,
                BUTTON_CELL,
                BUTTON_CELL,
            ),
            "BUTN00 is a 32×128 strip without declared cells: 4 cells of 32×32 [INFERENCE]",
            Some(UiRect::new(0, cell as i16 * BUTTON_CELL as i16, BUTTON_CELL, BUTTON_CELL)),
        )?);
    }
    nodes.push(declared_sprite(
        "main-menu",
        "button-extra",
        assets,
        "BUTN03I0.IMG",
        UiRect::new(MENU_EXTRA_BUTTON_X, MENU_EXTRA_BUTTON_Y, 22, 27),
        "BUTN03 header (0,0) is the canvas corner: declared position of the extra button",
        None,
    )?);
    Ok(UiPanel {
        id: UiPanelId::MainMenu,
        nodes,
    })
}

/// Builds the 4 M17 panels.
pub fn build_all_panels(
    hud: &HudPanelData,
    inventory: &InventoryPanelData,
    quests: &QuestPanelData,
    assets: &UiAssets,
) -> Result<Vec<UiPanel>, UiSpecError> {
    Ok(vec![
        build_hud_panel(hud, assets)?,
        build_inventory_panel(inventory, assets)?,
        build_quests_panel(quests, assets)?,
        build_main_menu_panel(assets)?,
    ])
}

/// C7a assert: no node may be a generic widget nor use fonts
/// outside the `.FNT` registry. Returns the violation list (empty = PASS).
pub fn assert_no_generic_widgets(panels: &[UiPanel]) -> Vec<UiSpecError> {
    let mut violations = Vec::new();
    for panel in panels {
        for node in &panel.nodes {
            if !matches!(node.kind, UiNodeKind::Sprite | UiNodeKind::GlyphRun) {
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
                    what: "sprite with text".to_string(),
                });
            }
            if !node.rect.fits_canvas() {
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
    if node.kind == UiNodeKind::GlyphRun {
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
    /// Empty framebuffer (fully transparent).
    pub fn new() -> Self {
        let len = CANVAS_WIDTH as usize * CANVAS_HEIGHT as usize;
        Self {
            width: CANVAS_WIDTH,
            height: CANVAS_HEIGHT,
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
/// - every written pixel is validated against its asset index range
///   (C2). The first violation aborts with [`UiSpecError::PaletteViolation`].
pub fn compose(panels: &[UiPanel], assets: &UiAssets) -> Result<Framebuffer, UiSpecError> {
    let mut fb = Framebuffer::new();
    for panel in panels {
        for node in &panel.nodes {
            let slot = fb.slot_for(node, assets);
            match node.kind {
                UiNodeKind::Sprite => {
                    let asset = require_image(assets, panel.id.as_str(), &node.asset)?;
                    let range = asset_index_range(&node.asset);
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
                            let at = y as usize * fb.width as usize + x as usize;
                            fb.pixels[at] = index;
                            fb.slots[at] = slot;
                        }
                    }
                }
                UiNodeKind::GlyphRun => {
                    let font = require_font(assets, panel.id.as_str(), &node.asset)?;
                    let text = node.text.as_deref().unwrap_or_default();
                    let mask = font.render_text(text);
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
                            if x >= fb.width || y >= fb.height {
                                continue;
                            }
                            let at = y as usize * fb.width as usize + x as usize;
                            fb.pixels[at] = node.ink;
                            fb.slots[at] = slot;
                        }
                    }
                }
            }
        }
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
    for y in 0..CANVAS_HEIGHT as usize {
        for x in 0..CANVAS_WIDTH as usize {
            let at = (y * CANVAS_WIDTH as usize + x) * 4;
            let [r, g, b, a] = [
                canvas[at],
                canvas[at + 1],
                canvas[at + 2],
                canvas[at + 3],
            ];
            for sy in 0..PRESENT_SCALE as usize {
                for sx in 0..PRESENT_SCALE as usize {
                    let px = x * PRESENT_SCALE as usize + sx;
                    let py = LETTERBOX as usize + y * PRESENT_SCALE as usize + sy;
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
        letterbox_top: LETTERBOX,
        letterbox_bottom: LETTERBOX,
        mode: PresentMode::Letterbox,
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
        let range = asset_index_range(&entry.asset);
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
        // Non-empty glyphs (one ink row) for the composition test.
        for index in 0..240usize {
            let base = 964 + index * 32;
            bytes[base + 1] = 0x80;
        }
        FntFile::parse(&bytes).expect("synthetic fnt")
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
        assets.with_font(DEFAULT_FONT, synthetic_fnt())
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
    fn menu_asset_outside_canvas_uses_declared_transform() {
        let assets = assets();
        let panel = build_main_menu_panel(&assets).expect("menu");
        let menu = panel.nodes.iter().find(|node| node.id == "menu-panel").expect("menu");
        assert_eq!(menu.rect, UiRect::new(212, 17, 108, 183));
        assert!(matches!(menu.anchor, UiAnchor::Declared(_)));
        assert_eq!(menu.source, Some(UiRect::new(0, 0, 108, 183)));
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
}

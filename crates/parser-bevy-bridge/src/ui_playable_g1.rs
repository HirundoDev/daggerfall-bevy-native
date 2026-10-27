//! Wave 3 composition slot — **G1 (núcleo) + G2 (comercio)**, 17 windows.
//!
//! Derived from `docs/sintergic/l5-mission/m34-window-assets-CR-2d5c8f41.tsv`
//! (rows whose `group` is `G1` or `G2`): **10 + 7 = 17** rows, the exact
//! subset of `m34-playable-groups-CR-2d5c8f41.txt` this slot owns.
//!
//! ## Provenance (input already measured, not re-derived here)
//!
//! - TSV: 51 rows × 13 columns, sha256 `09c85d8e…aae246`, `fnv1a64
//!   7bb4a17485157a42` ×2; **48 `yes` / 0 `no` / 3 `na`** — the 17 rows of
//!   this slot are all `decodes=yes`.
//! - Census names are DFU class names without `.cs` (GV-19,
//!   `c85e0bb5ae867373`); the 17-id subset digests to `c5fcb0de795aa998`
//!   (450 B, ascending by byte, `\n`, no trailing newline — same rule that
//!   reproduces GV-02 `1da6a516914e33db`).
//! - `asset_retail` is the `ARENA2` record each window decodes; `rect` is the
//!   TSV row's rectangle, always inside 320×200.
//!
//! ## Two traps the TSV author left written
//!
//! 1. `generic_widget = 0` is audited against **`asset_retail`**, never against
//!    `asset_kind`: a row can be `asset_kind = RCI` and still be a concrete
//!    retail asset (`INVE00I0.IMG`, `SPOP.RCI`).
//! 2. `SPOP.RCI` (2 rows here: `DaggerfallMessageBox`,
//!    `DaggerfallInputMessageBox`) does **not** decode as a whole file
//!    (`UnrecognizedImg { len: 4356 }`): it is 9 records of 22×22 and needs
//!    `decode_rci(record, 22, 22)` per record (`CifRciFile.ReadRci`, cited by
//!    the TSV at `DFU:…/DaggerfallMessageBox.cs:231`). The whole-file failure
//!    is a container artefact, not a table error, so the declared rect of both
//!    windows is the 22×22 **border record**, never the container's extent.
//!
//! ## Node composition rule (0 invented geometry)
//!
//! [`nodes`] declares each window's panel in painting order. The **only**
//! rectangles used are the TSV row's own rect (surface) and that same rect
//! again as the text band (a run cannot paint outside the panel it belongs
//! to); the `.FNT` binding is the TSV `font` column. Elements that `note`
//! mentions but the TSV never places are **not** placed here: the satellite
//! buttons of the inventory/trade surfaces (`INVE06..INVE14I0.IMG`,
//! `…InventoryWindow.cs:141` + trade `:79-83`) and `HUDLarge`
//! (`MAIN00I0.IMG` 320×46, `HUDCompass.cs:100`) have no origin in the input, so
//! inventing one would be the gravest failure of this lane. They are reported
//! as a deviation in the lane's HH-1 instead.
//!
//! `UiNode::palette_hash` is [`PALETTE_UNRESOLVED`] at declaration time: the
//! palette is resolved per node from `UiAssets` when composing
//! (`ui_spec::node_palette` never reads this field), and no palette can be
//! resolved without `ARENA2`.
//!
//! ## Honest intermediate state
//!
//! `PROBE-UI-FULL-001` prints `playable: 17/51` while the sibling slots stay
//! empty, so the probe still exits `EXIT=1`; `EXIT=0` with `51/51` is the
//! orchestrator's end-of-wave validation, not this lane's claim.

use daggerfall_parser_rs::image::{family_for_asset, PaletteFamily};

use crate::ui_spec::{
    UiAnchor, UiNode, UiNodeKind, UiPanel, UiPanelId, UiRect, WindowSpec, DEFAULT_INK,
};

/// `UiNode::palette_hash` at declaration time: unresolved (`0`).
///
/// The compositor resolves each node's palette from `UiAssets`
/// (`ui_spec::node_palette`) and never reads this field, so a declared `0`
/// claims nothing about the retail palette.
pub const PALETTE_UNRESOLVED: u64 = 0;

/// TSV `font` column of the 17 rows (all of them): body font.
const DECLARED_BODY_FONT: &str = "FONT0003.FNT";

/// Large font of the journal title (`DaggerfallQuestJournalWindow.cs:163`,
/// cited by the TSV `note`), not the body font of the window.
const DECLARED_TITLE_FONT: &str = "FONT0000.FNT";

/// `Declared` reason — RCI/IMG 320×200 surface without `ImgFile` header.
const DECLARED_CANVAS_320X200: &str = "320x200 RCI/IMG surface without ImgFile header: TSV rect is the declared full-canvas placement";

/// `Declared` reason — HUD compass bar (`COMPASS_Y = 186`, `COMPASS_H = 14`,
/// same convention as `ui_spec`; the 322 px bar is clipped to 320).
const DECLARED_COMPASS_BAR: &str = "COMPASS.IMG is 322x14 without header: declared bar at y=186 h=14, clipped to 320 (ui_spec COMPASS_Y/COMPASS_HEIGHT)";

/// `Declared` reason — 22×22 border record of the multi-record `SPOP.RCI`
/// 9-slice parchment.
const DECLARED_SPOP_BORDER: &str = "SPOP.RCI is a 9-record 22x22 CifRciFile: declared rect is the border record, decoded per record";

/// `Declared` reason — text band of the window (font from the TSV column).
const DECLARED_TEXT_BAND: &str = "text band bounded by the window rect; .FNT comes from the TSV font column, content from the read model";

/// Windows whose TSV `anchor` column says `Header` (rect read from the asset's
/// own `ImgFile` header; the TSV row reproduces that header).
const HEADER_WINDOWS: [&str; 8] = [
    "DaggerfallPauseOptionsWindow",
    "DaggerfallListPickerWindow",
    "DaggerfallBankingWindow",
    "DaggerfallBankPurchasePopUp",
    "DaggerfallTavernWindow",
    "DaggerfallRestWindow",
    "DaggerfallMerchantRepairPopupWindow",
    "DaggerfallMerchantServicePopupWindow",
];

/// The 17 `WindowSpec` of G1 (núcleo) + G2 (comercio), one per TSV row.
///
/// Every row carries its `source` (`DFU:<path>:<line>`) and its TSV `anchor`
/// (`Header` / `Declared`) as the justification of the declared placement.
static SPECS: [WindowSpec; 17] = [
    // ── G1 (W34-2a) núcleo — 10 ──────────────────────────────────────────
    // TSV anchor=Declared · DFU:…/HUDCompass.cs:100 — compass bar at the
    // bottom (rect 320x14 clips the 322 px bar; ShowCompass=true by default,
    // DaggerfallHUD.cs:150).
    WindowSpec {
        id: "DaggerfallHUD",
        asset_retail: Some("COMPASS.IMG"),
        rect: UiRect { x: 0, y: 186, width: 320, height: 14 },
    },
    // TSV anchor=Declared · DFU:…/DaggerfallInventoryWindow.cs:141 — RCI
    // 320x200 without header, the panel's baseTexture.
    WindowSpec {
        id: "DaggerfallInventoryWindow",
        asset_retail: Some("INVE00I0.IMG"),
        rect: UiRect { x: 0, y: 0, width: 320, height: 200 },
    },
    // TSV anchor=Declared · DFU:…/DaggerfallCharacterSheetWindow.cs:30 —
    // RCI 320x200 parchment; levels and skills in DefaultFont.
    WindowSpec {
        id: "DaggerfallCharacterSheetWindow",
        asset_retail: Some("INFO00I0.IMG"),
        rect: UiRect { x: 0, y: 0, width: 320, height: 200 },
    },
    // TSV anchor=Declared · DFU:…/DaggerfallQuestJournalWindow.cs:28 — RCI
    // 320x200 book; the title uses LargeFont FONT0000 (line 163).
    WindowSpec {
        id: "DaggerfallQuestJournalWindow",
        asset_retail: Some("LGBK00I0.IMG"),
        rect: UiRect { x: 0, y: 0, width: 320, height: 200 },
    },
    // TSV anchor=Declared · DFU:…/DaggerfallMessageBox.cs:231 — SPOP.RCI
    // 9x22x22 multi-record parchment; rect = border record (trap 2).
    WindowSpec {
        id: "DaggerfallMessageBox",
        asset_retail: Some("SPOP.RCI"),
        rect: UiRect { x: 0, y: 0, width: 22, height: 22 },
    },
    // TSV anchor=Header · DFU:…/DaggerfallPauseOptionsWindow.cs:26 — IMG
    // with its own header (12612 B / 12600 px).
    WindowSpec {
        id: "DaggerfallPauseOptionsWindow",
        asset_retail: Some("OPTN00I0.IMG"),
        rect: UiRect { x: 85, y: 40, width: 150, height: 84 },
    },
    // TSV anchor=Declared · DFU:…/DaggerfallBookReaderWindow.cs:30 — RCI
    // 320x200; the font can be switched by book token (line 236).
    WindowSpec {
        id: "DaggerfallBookReaderWindow",
        asset_retail: Some("BOOK00I0.IMG"),
        rect: UiRect { x: 0, y: 0, width: 320, height: 200 },
    },
    // TSV anchor=Declared · DFU:…/DaggerfallPlayerHistoryWindow.cs:25 — RCI
    // 320x200; ChangeFont to FONT0003 at line 55.
    WindowSpec {
        id: "DaggerfallPlayerHistoryWindow",
        asset_retail: Some("LGBK00I0.IMG"),
        rect: UiRect { x: 0, y: 0, width: 320, height: 200 },
    },
    // TSV anchor=Declared · DFU:…/DaggerfallInputMessageBox.cs:146 —
    // parchment via SetDaggerfallPopupStyle(Parchment, …); same 9x22x22
    // multi-record warning as DaggerfallMessageBox (trap 2).
    WindowSpec {
        id: "DaggerfallInputMessageBox",
        asset_retail: Some("SPOP.RCI"),
        rect: UiRect { x: 0, y: 0, width: 22, height: 22 },
    },
    // TSV anchor=Header · DFU:…/DaggerfallListPickerWindow.cs:31 — IMG with
    // its own header (x/y/w/h read from the file).
    WindowSpec {
        id: "DaggerfallListPickerWindow",
        asset_retail: Some("PICK00I0.IMG"),
        rect: UiRect { x: 60, y: 36, width: 200, height: 128 },
    },
    // ── G2 (W34-2b) comercio — 7 ─────────────────────────────────────────
    // TSV anchor=Declared · DFU:…/DaggerfallInventoryWindow.cs:141 — inherits
    // DaggerfallInventoryWindow: the surface is the base's INVE00I0.IMG; its
    // own .cs only adds buttons INVE08/10/11/12/14I0.IMG (lines 79-83). The
    // indirection is real: this row's source cites the *base* class.
    WindowSpec {
        id: "DaggerfallTradeWindow",
        asset_retail: Some("INVE00I0.IMG"),
        rect: UiRect { x: 0, y: 0, width: 320, height: 200 },
    },
    // TSV anchor=Header · DFU:…/DaggerfallBankingWindow.cs:26 — IMG with its
    // own header.
    WindowSpec {
        id: "DaggerfallBankingWindow",
        asset_retail: Some("BANK00I0.IMG"),
        rect: UiRect { x: 48, y: 5, width: 225, height: 181 },
    },
    // TSV anchor=Header · DFU:…/DaggerfallBankPurchasePopUp.cs:49 — IMG with
    // its own header.
    WindowSpec {
        id: "DaggerfallBankPurchasePopUp",
        asset_retail: Some("BANK01I0.IMG"),
        rect: UiRect { x: 48, y: 15, width: 225, height: 129 },
    },
    // TSV anchor=Header · DFU:…/DaggerfallTavernWindow.cs:48 — IMG with its
    // own header.
    WindowSpec {
        id: "DaggerfallTavernWindow",
        asset_retail: Some("TVRN00I0.IMG"),
        rect: UiRect { x: 95, y: 47, width: 130, height: 44 },
    },
    // TSV anchor=Header · DFU:…/DaggerfallRestWindow.cs:62 — IMG with its own
    // header.
    WindowSpec {
        id: "DaggerfallRestWindow",
        asset_retail: Some("REST00I0.IMG"),
        rect: UiRect { x: 83, y: 50, width: 154, height: 41 },
    },
    // TSV anchor=Header · DFU:…/DaggerfallMerchantRepairPopupWindow.cs:43 —
    // IMG with its own header.
    WindowSpec {
        id: "DaggerfallMerchantRepairPopupWindow",
        asset_retail: Some("REPR01I0.IMG"),
        rect: UiRect { x: 95, y: 47, width: 130, height: 51 },
    },
    // TSV anchor=Header · DFU:…/DaggerfallMerchantServicePopupWindow.cs:40 —
    // IMG with its own header.
    WindowSpec {
        id: "DaggerfallMerchantServicePopupWindow",
        asset_retail: Some("GNRC01I0.IMG"),
        rect: UiRect { x: 95, y: 56, width: 130, height: 42 },
    },
];

/// G1 + G2 window specs (17 rows of the M34 asset table).
pub fn specs() -> &'static [WindowSpec] {
    &SPECS
}

/// Panel of one window as declared: surface first, then its text runs.
///
/// Returns `None` for a window outside this slot (0 specs invented, 0 ids
/// guessed). Painting order: the `surface` sprite (the TSV `asset_retail` at
/// the TSV rect, anchored `Header`/`Declared` per the TSV `anchor` column),
/// then the `text` glyph run bound to the TSV `font` column and — only for the
/// journal — the `title` run bound to LargeFont `FONT0000`
/// (`DaggerfallQuestJournalWindow.cs:163`, cited by the TSV `note`).
pub fn nodes(window: &str) -> Option<Vec<UiNode>> {
    let spec = SPECS.iter().find(|spec| spec.id == window)?;
    let asset = spec.asset_retail?;
    let mut nodes = Vec::with_capacity(3);
    nodes.push(UiNode {
        id: "surface".to_string(),
        kind: UiNodeKind::Sprite,
        rect: spec.rect,
        anchor: surface_anchor(window),
        asset: asset.to_string(),
        family: family_for_asset(asset),
        palette_hash: PALETTE_UNRESOLVED,
        source: surface_source(window),
        text: None,
        ink: DEFAULT_INK,
    });
    nodes.push(text_node(spec.rect, DECLARED_BODY_FONT, "text"));
    if window == "DaggerfallQuestJournalWindow" {
        nodes.push(text_node(spec.rect, DECLARED_TITLE_FONT, "title"));
    }
    Some(nodes)
}

/// The 17 declared panels, in [`specs`] order.
///
/// `UiPanel` is the crate's node-audit container and [`UiPanelId`] only names
/// the six M17 screens, so the id here is an **audit label** (error-message
/// name), never a claim that a M34 window *is* that M17 panel: HUD → the
/// bottom bar, inventory + its trade subclass → the paper-doll surface, the
/// book/journal/history screens → the journal book, every other window (the
/// menu-style dialogs/popups) → the menu overlay.
pub fn panels() -> Vec<UiPanel> {
    SPECS
        .iter()
        .filter_map(|spec| {
            nodes(spec.id).map(|nodes| UiPanel {
                id: audit_panel_id(spec.id),
                nodes,
            })
        })
        .collect()
}

/// Anchor of the surface node, straight from the TSV `anchor` column.
fn surface_anchor(window: &str) -> UiAnchor {
    if HEADER_WINDOWS.contains(&window) {
        UiAnchor::Header
    } else {
        UiAnchor::Declared(declared_reason(window))
    }
}

/// `Declared` constant of a window without an `ImgFile` header.
fn declared_reason(window: &str) -> &'static str {
    match window {
        "DaggerfallHUD" => DECLARED_COMPASS_BAR,
        "DaggerfallMessageBox" | "DaggerfallInputMessageBox" => DECLARED_SPOP_BORDER,
        _ => DECLARED_CANVAS_320X200,
    }
}

/// Source cell inside the surface, when the TSV `note` states the crop.
///
/// Only the HUD does: `COMPASS.IMG` is 322×14 and the declared 320×14 bar
/// clips it (`HUDCompass.cs:100`). The `SPOP.RCI` windows declare no source
/// cell: which of the 9 records is "the border" is not in the input, so it is
/// left for the per-record decoder instead of being guessed here.
fn surface_source(window: &str) -> Option<UiRect> {
    if window == "DaggerfallHUD" {
        return Some(UiRect::new(0, 0, 320, 14));
    }
    None
}

/// Glyph-run declaration: `.FNT` binding inside the window's text band.
///
/// The run is declared **empty** on purpose: the content is data (HUD read
/// model / panel read model) and any literal here would be invented copy. What
/// the declaration carries is what the TSV states — the font — so the spec can
/// be audited asset-only (C7b: 0 system fonts).
fn text_node(rect: UiRect, font: &'static str, id: &str) -> UiNode {
    UiNode {
        id: id.to_string(),
        kind: UiNodeKind::GlyphRun,
        rect,
        anchor: UiAnchor::Declared(DECLARED_TEXT_BAND),
        asset: font.to_string(),
        family: PaletteFamily::Hud,
        palette_hash: PALETTE_UNRESOLVED,
        source: None,
        text: Some(String::new()),
        ink: DEFAULT_INK,
    }
}

/// Audit label of a window's panel (see [`panels`]).
fn audit_panel_id(window: &str) -> UiPanelId {
    match window {
        "DaggerfallHUD" => UiPanelId::Hud,
        "DaggerfallInventoryWindow" | "DaggerfallTradeWindow" => UiPanelId::Inventory,
        "DaggerfallQuestJournalWindow"
        | "DaggerfallBookReaderWindow"
        | "DaggerfallPlayerHistoryWindow" => UiPanelId::Quests,
        _ => UiPanelId::MainMenu,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_spec::{
        assert_no_generic_widgets, asset_is_generic, CANVAS_HEIGHT, CANVAS_WIDTH, FONT_FILES,
    };

    /// `m34-playable-groups-CR-2d5c8f41.txt` §G1 (W34-2a) — 10 núcleo windows,
    /// in the partition's order.
    const G1: [&str; 10] = [
        "DaggerfallHUD",
        "DaggerfallInventoryWindow",
        "DaggerfallCharacterSheetWindow",
        "DaggerfallQuestJournalWindow",
        "DaggerfallMessageBox",
        "DaggerfallPauseOptionsWindow",
        "DaggerfallBookReaderWindow",
        "DaggerfallPlayerHistoryWindow",
        "DaggerfallInputMessageBox",
        "DaggerfallListPickerWindow",
    ];

    /// `m34-playable-groups-CR-2d5c8f41.txt` §G2 (W34-2b) — 7 comercio windows.
    const G2: [&str; 7] = [
        "DaggerfallTradeWindow",
        "DaggerfallBankingWindow",
        "DaggerfallBankPurchasePopUp",
        "DaggerfallTavernWindow",
        "DaggerfallRestWindow",
        "DaggerfallMerchantRepairPopupWindow",
        "DaggerfallMerchantServicePopupWindow",
    ];

    /// GV-19 subset digest of the 17 ids (ascending by byte, `\n`, no trailing
    /// newline, 450 B) — same rule that reproduces GV-02 `1da6a516914e33db`.
    const SUBSET_DIGEST: u64 = 0xc5fc_b0de_795a_a998;

    fn fnv1a64(bytes: &[u8]) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }

    #[test]
    fn g1_is_the_ten_nucleo_windows() {
        let ids: Vec<&str> = specs()[..G1.len()].iter().map(|spec| spec.id).collect();
        assert_eq!(ids, G1, "G1 rows must be the partition's 10 núcleo windows");
    }

    #[test]
    fn g2_is_the_seven_comercio_windows() {
        let ids: Vec<&str> = specs()[G1.len()..].iter().map(|spec| spec.id).collect();
        assert_eq!(ids, G2, "G2 rows must be the partition's 7 comercio windows");
    }

    #[test]
    fn seventeen_concrete_specs_inside_the_canvas() {
        assert_eq!(specs().len(), 17, "G1(10) + G2(7)");
        for spec in specs() {
            assert!(spec.is_concrete(), "{} is not concrete", spec.id);
            assert!(
                !asset_is_generic(spec.asset_retail.unwrap_or_default()),
                "{} has a generic asset_retail",
                spec.id
            );
            let rect = spec.rect;
            assert!(rect.x >= 0 && rect.y >= 0, "{} has a negative origin", spec.id);
            assert!(
                rect.x + rect.width as i16 <= CANVAS_WIDTH as i16,
                "{} right edge {} > {CANVAS_WIDTH}",
                spec.id,
                rect.x + rect.width as i16
            );
            assert!(
                rect.y + rect.height as i16 <= CANVAS_HEIGHT as i16,
                "{} bottom edge {} > {CANVAS_HEIGHT}",
                spec.id,
                rect.y + rect.height as i16
            );
        }
    }

    #[test]
    fn spec_table_reproduces_the_tsv_rows() {
        // (id, asset_retail, x, y, w, h) copied from the TSV, column by column.
        let tsv: [(&str, &str, i16, i16, u16, u16); 17] = [
            ("DaggerfallHUD", "COMPASS.IMG", 0, 186, 320, 14),
            ("DaggerfallInventoryWindow", "INVE00I0.IMG", 0, 0, 320, 200),
            ("DaggerfallCharacterSheetWindow", "INFO00I0.IMG", 0, 0, 320, 200),
            ("DaggerfallQuestJournalWindow", "LGBK00I0.IMG", 0, 0, 320, 200),
            ("DaggerfallMessageBox", "SPOP.RCI", 0, 0, 22, 22),
            ("DaggerfallPauseOptionsWindow", "OPTN00I0.IMG", 85, 40, 150, 84),
            ("DaggerfallBookReaderWindow", "BOOK00I0.IMG", 0, 0, 320, 200),
            ("DaggerfallPlayerHistoryWindow", "LGBK00I0.IMG", 0, 0, 320, 200),
            ("DaggerfallInputMessageBox", "SPOP.RCI", 0, 0, 22, 22),
            ("DaggerfallListPickerWindow", "PICK00I0.IMG", 60, 36, 200, 128),
            ("DaggerfallTradeWindow", "INVE00I0.IMG", 0, 0, 320, 200),
            ("DaggerfallBankingWindow", "BANK00I0.IMG", 48, 5, 225, 181),
            ("DaggerfallBankPurchasePopUp", "BANK01I0.IMG", 48, 15, 225, 129),
            ("DaggerfallTavernWindow", "TVRN00I0.IMG", 95, 47, 130, 44),
            ("DaggerfallRestWindow", "REST00I0.IMG", 83, 50, 154, 41),
            ("DaggerfallMerchantRepairPopupWindow", "REPR01I0.IMG", 95, 47, 130, 51),
            ("DaggerfallMerchantServicePopupWindow", "GNRC01I0.IMG", 95, 56, 130, 42),
        ];
        for (index, (id, asset, x, y, width, height)) in tsv.iter().enumerate() {
            let spec = specs()[index];
            assert_eq!(spec.id, *id, "row {index} id");
            assert_eq!(spec.asset_retail, Some(*asset), "{} asset_retail", spec.id);
            assert_eq!(
                spec.rect,
                UiRect::new(*x, *y, *width, *height),
                "{} rect",
                spec.id
            );
        }
    }

    #[test]
    fn ids_digest_is_the_g1_g2_subset_of_gv19() {
        let mut ids: Vec<&str> = specs().iter().map(|spec| spec.id).collect();
        ids.sort_unstable_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
        ids.dedup();
        assert_eq!(ids.len(), 17, "0 duplicated ids");
        let text = ids.join("\n");
        assert_eq!(text.len(), 450, "subset byte length");
        assert_eq!(fnv1a64(text.as_bytes()), SUBSET_DIGEST);
    }

    #[test]
    fn nodes_are_not_generic_widgets() {
        let panels = panels();
        assert_eq!(panels.len(), 17);
        let violations = assert_no_generic_widgets(&panels);
        assert!(violations.is_empty(), "untagged widgets: {violations:?}");
    }

    #[test]
    fn surface_node_reuses_the_tsv_rect_and_anchor() {
        for spec in specs() {
            let declared = nodes(spec.id).expect("every spec declares its panel");
            let surface = &declared[0];
            assert_eq!(surface.id, "surface");
            assert_eq!(surface.kind, UiNodeKind::Sprite);
            assert_eq!(surface.asset, spec.asset_retail.unwrap());
            assert_eq!(surface.rect, spec.rect, "{} surface rect", spec.id);
            assert_eq!(surface.text, None, "{} sprite with text", spec.id);
            let expected = if HEADER_WINDOWS.contains(&spec.id) {
                UiAnchor::Header
            } else {
                UiAnchor::Declared(declared_reason(spec.id))
            };
            assert_eq!(surface.anchor, expected, "{} anchor", spec.id);
        }
        assert_eq!(nodes("DaggerfallSpellBookWindow"), None, "foreign window");
    }

    #[test]
    fn text_runs_bind_the_tsv_font() {
        for spec in specs() {
            let declared = nodes(spec.id).unwrap();
            let text = &declared[1];
            assert_eq!(text.kind, UiNodeKind::GlyphRun, "{}", spec.id);
            assert_eq!(text.asset, DECLARED_BODY_FONT, "{} body font", spec.id);
            assert!(FONT_FILES.contains(&text.asset.as_str()), "{}", spec.id);
            assert_eq!(text.rect, spec.rect, "{} text band", spec.id);
            assert_eq!(text.palette_hash, PALETTE_UNRESOLVED);
        }
    }

    #[test]
    fn journal_title_uses_large_font_on_the_documented_line() {
        // TSV note: "el titulo usa LargeFont FONT0000 (linea 163)".
        let journal = nodes("DaggerfallQuestJournalWindow").unwrap();
        assert_eq!(journal.len(), 3);
        assert_eq!(journal[2].id, "title");
        assert_eq!(journal[2].kind, UiNodeKind::GlyphRun);
        assert_eq!(journal[2].asset, "FONT0000.FNT");
        assert!(FONT_FILES.contains(&journal[2].asset.as_str()));
    }

    #[test]
    fn trade_window_inherits_the_inventory_surface() {
        // TSV note (indirection): DaggerfallTradeWindow inherits
        // DaggerfallInventoryWindow — same surface, its own .cs adds buttons.
        let inventory = specs()
            .iter()
            .find(|spec| spec.id == "DaggerfallInventoryWindow")
            .unwrap();
        let trade = specs()
            .iter()
            .find(|spec| spec.id == "DaggerfallTradeWindow")
            .unwrap();
        assert_eq!(trade.asset_retail, inventory.asset_retail);
        assert_eq!(trade.rect, inventory.rect);
        // Same declared surface, distinct census rows / distinct panels.
        assert_eq!(panels().len(), 17);
    }

    #[test]
    fn spop_windows_declare_the_22x22_border_record() {
        // Trap 2: the container fails as a whole (UnrecognizedImg 4356), the
        // rect is one 22×22 record — never the container or the canvas.
        for window in ["DaggerfallMessageBox", "DaggerfallInputMessageBox"] {
            let spec = specs().iter().find(|spec| spec.id == window).unwrap();
            assert_eq!(spec.asset_retail, Some("SPOP.RCI"));
            assert_eq!(spec.rect, UiRect::new(0, 0, 22, 22));
            assert_ne!(spec.rect, UiRect::canvas(), "{window} declared the canvas");
            let surface = &nodes(window).unwrap()[0];
            assert_eq!(
                surface.anchor,
                UiAnchor::Declared(DECLARED_SPOP_BORDER),
                "{window} anchor"
            );
            assert_eq!(surface.source, None, "border record not guessed");
        }
    }

    #[test]
    fn header_windows_are_the_eight_tsv_header_rows() {
        let header: Vec<&str> = specs()
            .iter()
            .filter(|spec| matches!(surface_anchor(spec.id), UiAnchor::Header))
            .map(|spec| spec.id)
            .collect();
        assert_eq!(header, HEADER_WINDOWS);
        assert_eq!(header.len(), 8);
        assert_eq!(specs().len() - header.len(), 9, "9 Declared rows");
    }
}

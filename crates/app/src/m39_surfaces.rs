//! M39/UI-3 — the **51 `Playable` surfaces** (plan §1/§2/§3).
//!
//! One row per `WindowCategory::Playable` census entry, in plan §1 order
//! (ascending census ordinal), each composed as a
//! [`daggerfall_app::m37_pilots::PilotWindow`]: the retail panel of the TSV (or an empty
//! composition for the nine rows without raster) plus one read-model glyph
//! line from its **own** block (plan §3.5).
//!
//! ## Provenance (plan §3.1: zero invention)
//!
//! * `id` — census ordinal (full 85-entry census index + 1; `SEE
//!   daggerfall_perception::ui_census`), the same ordinal the probe prints.
//! * `name` — census class name; the typed handle is resolved **by name**
//!   through [`daggerfall_perception::ui_shell::canonical_id_for`] (plan §3.3:
//!   an unknown name is an error, never a default, never a numeric
//!   `WindowId`).
//! * `asset` / `rect` — the `asset_retail` and `rect_x/rect_y/rect_w/rect_h`
//!   columns of `docs/sintergic/l5-mission/m34-window-assets-CR-2d5c8f41.tsv`,
//!   i.e. the values plan §1 measured (they are the same values the wave-3
//!   composition slot `ui_playable_g1/g2/g3::specs()` declares; test (c)
//!   falsifies any drift).
//! * `text` — one declared route of the row's block family
//!   ([`daggerfall_perception::read_models::M39Models::paths`]); `038`
//!   (`DaggerfallHUD`, block B) has no `m39.*` family, so it reads the
//!   **pre-existing** HUD route `hud.health` (`HudReadModel`, plan §3.5).
//! * `font` — the TSV `font` column (`FONT0003.FNT` → [`RetailFont::Default`],
//!   `FONT0000.FNT` → [`RetailFont::Large`]); the TSV `-` rows take the plan
//!   §1 default `FONT0003.FNT`.
//!
//! ## Composition (plan §1 notes)
//!
//! * The six `asset_kind=FNT` rows (022 042 058 067 068 069) declare
//!   `WindowComposition::new(window, vec![])` and paint their glyph line, so
//!   they are **text-only but interactive** (plan §7.6).
//! * The three `asset_kind=none` rows (031 071 074) have no raster in the tree
//!   either; they are the only **read-only** surfaces (plan §3.4), with the
//!   TSV note as the citable reason.
//! * A row whose TSV note says it inherits (or builds on) another window's
//!   panel declares that panel as `SurfaceSpec::nested` with the owning window
//!   resolved by name; the citation is on the row itself:
//!   - 063 `DaggerfallTradeWindow` — "hereda DaggerfallInventoryWindow: la
//!     superficie es INVE00I0.IMG de la base" (TSV row note, citing
//!     `DaggerfallTradeWindow.cs:79-83` for its own satellite buttons);
//!   - 070 `DaggerfallUseMagicItemWindow` — "extiende
//!     DaggerfallListPickerWindow y no define superficie propia"
//!     (`DaggerfallUseMagicItemWindow.cs:20` in the TSV note);
//!   - 034 `DaggerfallGuildServiceCureDisease` — "hereda DaggerfallMessageBox:
//!     su superficie es el pergamino SPOP.RCI de la base";
//!   - 035 `DaggerfallGuildServiceDonation` — "hereda
//!     DaggerfallInputMessageBox (pergamino de la base)";
//!   - 037 `DaggerfallGuildServiceTraining` — "construye DaggerfallMessageBox,
//!     cuya superficie es el pergamino SPOP.RCI";
//!   - 053 `DaggerfallQuestOfferWindow` — "superficie = pergamino del
//!     DaggerfallMessageBox que crea `QuestMachine.CreateMessagePrompt`
//!     (linea 79)".
//!
//!   Every other raster row owns its panel (`SurfaceSpec::owned`). The text run
//!   starts at the panel's own origin: no new geometry is introduced.
//!
//! ## `SPOP.RCI` rows (C-4, measured — 034 035 037 039 047 053)
//!
//! `SPOP.RCI` = 3×3 celdas de 22×22 (medido); el destino mide una celda ⇒ se
//! pinta la celda centro (record 4), no se rebanan 9. The container decodes to
//! 66×66 (`nine_slice_side` ⇒ side 22), so `SurfaceSource::Whole` on a 22×22
//! target is rejected by rule 3 (a nine-slice needs ≥ 2×side per axis) — the
//! declaration was wrong, not the geometry. Rect and ownership are unchanged;
//! the centre cell is the parchment interior (the corners are the finials).
//!
//! ## HUD bar (038, measured)
//!
//! `COMPASS.IMG` is 322×14 while the declared bar is 320×14 (`ui_spec`
//! `COMPASS_Y`/`COMPASS_HEIGHT`), so the surface declares the source clip
//! `(0, 0, 320, 14)` — the same clip the HUD panel already declares ("COMPASS
//! is RCI without header: compass bar at the bottom (322 px → 320)"), never a
//! stretched whole-source placement (rule 2). Found by the retail-resolution
//! test, not by the probe.
//!
//! **Deviation (reported):** the three `asset_retail=-` rows keep the TSV value
//! (`-`) here, while `ui_playable_g3::specs()` names one representative retail
//! container of the same family (`HIRCINE.FLC`, `ANIM0001.VID`, `AZURA.FLC`)
//! because a `.FLC`/`.VID` is outside the `IMG`/`CIF`/`RCI`/`FNT` enum the spec
//! models. Test (c) pins that deviation instead of hiding it.

use std::sync::LazyLock;

use daggerfall_nervous::events::WindowId;
use daggerfall_parser_bevy_bridge::ui_compat::{
    RetailFont, SurfaceSource, SurfaceSpec, TextSource, WindowComposition,
};
use daggerfall_parser_bevy_bridge::ui_spec::UiRect;
use daggerfall_perception::ui_shell::canonical_id_for;

use daggerfall_app::m37_pilots::{PilotWindow, TextDecl};

/// Coded ink of the body text (A3: never 0, inside the HUD ink window).
const INK_BODY: u8 = 127;

/// The eight blocks of plan §2, in partition order.
pub const BLOCKS: [char; 8] = ['A', 'B', 'C', 'D', 'E', 'F', 'G', 'H'];

/// `asset_retail` of the rows the TSV declares without raster (031, 071, 074).
const NO_RASTER: &str = "-";

/// TSV `font` of the default face (plan §1: also the default of `-` rows).
const FONT_DEFAULT: &str = "FONT0003.FNT";

/// Rule string: a census name the shell registry does not know (plan §3.3).
const RULE_WINDOW: &str = "m39: census window name not registered";

/// Citable reason of 031 (`asset_kind=none`; TSV row note).
const READ_ONLY_031: &str = "m34-window-assets TSV, DaggerfallDaedraSummonedWindow.cs:77: superficie propia = FLC (playerPanel.Load), fuera del enum IMG/CIF/RCI/FNT — sin raster propio";

/// Citable reason of 071 (`asset_kind=none`; TSV row note).
const READ_ONLY_071: &str = "m34-window-assets TSV, DaggerfallVidPlayerWindow.cs:101: superficie = DaggerfallVideo .VID, fuera del enum IMG/CIF/RCI/FNT y la ventana no tiene texto propio";

/// Citable reason of 074 (`asset_kind=none`; TSV row note).
const READ_ONLY_074: &str = "m34-window-assets TSV, DemoFLCWindow.cs:37: FLCPlayer de 320x200 sobre un .FLC arbitrario, fuera del enum IMG/CIF/RCI/FNT";

/// One row of the measured surface table (plan §1).
///
/// Plain data: the paint-ready [`M39Surface`] is built from it once, by
/// [`surfaces`], because a composition owns `Vec`s.
#[derive(Debug, Clone, Copy)]
struct SurfaceRow {
    /// Census ordinal (`022`…`082`), the probe's `id`.
    id: u16,
    /// Block letter of plan §2.
    block: char,
    /// Census class name.
    name: &'static str,
    /// TSV `asset_retail` (`-` for rows without raster).
    asset: &'static str,
    /// TSV `rect_x, rect_y, rect_w, rect_h`.
    rect: (i16, i16, u16, u16),
    /// TSV `asset_kind` is `IMG` or `RCI`: the row declares a retail panel.
    panel: bool,
    /// Declared read-model route of the row's own block (plan §3.5).
    text: &'static str,
    /// TSV `font` column, mapped to its retail face.
    font: RetailFont,
    /// RCI record painted 1:1 (`Some` for the `SPOP.RCI` rows), `None` = the
    /// whole asset.
    record: Option<u16>,
    /// Source clip when the asset is larger than the destination (`Some` for
    /// the HUD bar: 322×14 → 320×14), `None` = no clip.
    clip: Option<(i16, i16, u16, u16)>,
    /// Citable read-only reason (plan §3.4), `None` = interactive.
    read_only: Option<&'static str>,
    /// Window whose panel this surface reuses (TSV `hereda` note), `None` =
    /// owned.
    inherited_from: Option<&'static str>,
}

/// A row with a retail panel (`asset_kind ∈ {IMG, RCI}`), owned by its window.
const fn panel(
    id: u16,
    block: char,
    name: &'static str,
    asset: &'static str,
    rect: (i16, i16, u16, u16),
    text: &'static str,
) -> SurfaceRow {
    SurfaceRow {
        id,
        block,
        name,
        asset,
        rect,
        panel: true,
        text,
        font: RetailFont::Default,
        record: None,
        clip: None,
        read_only: None,
        inherited_from: None,
    }
}

/// A row without a retail panel (`asset_kind ∈ {FNT, none}`): empty
/// composition (plan §1 notes).
const fn no_panel(
    id: u16,
    block: char,
    name: &'static str,
    asset: &'static str,
    rect: (i16, i16, u16, u16),
    text: &'static str,
) -> SurfaceRow {
    SurfaceRow {
        id,
        block,
        name,
        asset,
        rect,
        panel: false,
        text,
        font: RetailFont::Default,
        record: None,
        clip: None,
        read_only: None,
        inherited_from: None,
    }
}

/// The row's text face, when the TSV column is not `FONT0003.FNT`.
const fn with_font(mut row: SurfaceRow, font: RetailFont) -> SurfaceRow {
    row.font = font;
    row
}

/// Declares the row read-only with its citable reason.
const fn read_only(mut row: SurfaceRow, reason: &'static str) -> SurfaceRow {
    row.read_only = Some(reason);
    row
}

/// Declares the row's panel as nested in `owner`'s frame.
const fn inherited(mut row: SurfaceRow, owner: &'static str) -> SurfaceRow {
    row.inherited_from = Some(owner);
    row
}

/// Paints `record` of an RCI container 1:1 instead of the whole asset (C-4:
/// `SPOP.RCI` is a 3×3 container of 22×22 cells, so a 22×22 destination is one
/// cell, not a nine-slice).
const fn rci_cell(mut row: SurfaceRow, record: u16) -> SurfaceRow {
    row.record = Some(record);
    row
}

/// Paints `rect` of the asset 1:1 when the asset is larger than the declared
/// destination (the HUD bar clips 322×14 → 320×14, exactly as the HUD panel of
/// `ui_spec` declares it).
const fn clip(mut row: SurfaceRow, rect: (i16, i16, u16, u16)) -> SurfaceRow {
    row.clip = Some(rect);
    row
}

/// The 51 rows of plan §1, ascending by census ordinal (never reordered).
static ROWS: [SurfaceRow; 51] = [
    // Order and values are plan §1 verbatim (ascending census ordinal); the
    // block letter of each row is the plan §2 partition.
    no_panel(22, 'A', "DaggerfallAdvancedSettingsWindow", FONT_DEFAULT, (0, 0, 5, 7), "m39.settings.scale"),
    panel(23, 'G', "DaggerfallAutomapWindow", "AMAP00I0.IMG", (0, 0, 320, 200), "m39.automap.markers"),
    panel(24, 'D', "DaggerfallBankPurchasePopUp", "BANK01I0.IMG", (48, 15, 225, 129), "m39.bank.purchase_price"),
    panel(25, 'D', "DaggerfallBankingWindow", "BANK00I0.IMG", (48, 5, 225, 181), "m39.bank.balance"),
    panel(27, 'F', "DaggerfallBookReaderWindow", "BOOK00I0.IMG", (0, 0, 320, 200), "m39.book.pages"),
    panel(28, 'C', "DaggerfallCharacterSheetWindow", "INFO00I0.IMG", (0, 0, 320, 200), "m39.sheet.level"),
    panel(29, 'A', "DaggerfallControlsWindow", "CNFG00I0.IMG", (0, 0, 320, 200), "m39.controls.binding"),
    panel(30, 'H', "DaggerfallCourtWindow", "CORT01I0.IMG", (0, 0, 320, 200), "m39.court.ruling"),
    read_only(
        no_panel(31, 'H', "DaggerfallDaedraSummonedWindow", NO_RASTER, (0, 0, 0, 0), "m39.daedra.summoned"),
        READ_ONLY_031,
    ),
    panel(32, 'E', "DaggerfallEffectSettingsEditorWindow", "MASK05I0.IMG", (0, 0, 320, 200), "m39.spellmaker.elements"),
    panel(33, 'G', "DaggerfallExteriorAutomapWindow", "AMAP00I0.IMG", (0, 0, 320, 200), "m39.automap.chunks"),
    // "inherits DaggerfallMessageBox: its surface is the SPOP.RCI parchment of
    // the base".
    inherited(
        rci_cell(panel(34, 'H', "DaggerfallGuildServiceCureDisease", "SPOP.RCI", (0, 0, 22, 22), "m39.service.kind"), 4),
        "DaggerfallMessageBox",
    ),
    // "inherits DaggerfallInputMessageBox (parchment of the base); it also
    // builds a DaggerfallMessageBox at DaggerfallGuildServiceDonation.cs:72".
    inherited(
        rci_cell(panel(35, 'H', "DaggerfallGuildServiceDonation", "SPOP.RCI", (0, 0, 22, 22), "m39.service.charged"), 4),
        "DaggerfallInputMessageBox",
    ),
    panel(36, 'H', "DaggerfallGuildServicePopupWindow", "GILD00I0.IMG", (95, 47, 130, 51), "m39.service.executed"),
    // "construye DaggerfallMessageBox, cuya superficie es el pergamino SPOP.RCI".
    inherited(
        rci_cell(panel(37, 'H', "DaggerfallGuildServiceTraining", "SPOP.RCI", (0, 0, 22, 22), "m39.service.kind"), 4),
        "DaggerfallMessageBox",
    ),
    // Block B has no `m39.*` family: the HUD reads the pre-existing projection
    // route (`HudReadModel`, `crate::m39_state::M39State::hud`).
    clip(
        panel(38, 'B', "DaggerfallHUD", "COMPASS.IMG", (0, 186, 320, 14), "hud.health"),
        (0, 0, 320, 14),
    ),
    rci_cell(panel(39, 'A', "DaggerfallInputMessageBox", "SPOP.RCI", (0, 0, 22, 22), "m39.input.prompt"), 4),
    panel(40, 'C', "DaggerfallInventoryWindow", "INVE00I0.IMG", (0, 0, 320, 200), "m39.inv.slots"),
    panel(41, 'E', "DaggerfallItemMakerWindow", "ITEM00I0.IMG", (0, 0, 320, 200), "m39.itemmaker.cost"),
    no_panel(42, 'A', "DaggerfallJoystickControlsWindow", FONT_DEFAULT, (0, 0, 5, 7), "m39.controls.conflicts"),
    panel(43, 'A', "DaggerfallListPickerWindow", "PICK00I0.IMG", (60, 36, 200, 128), "m39.menu.selected.name"),
    panel(44, 'A', "DaggerfallLoadClassicGameWindow", "LOAD00I0.IMG", (0, 0, 320, 200), "m39.saves.count"),
    panel(45, 'D', "DaggerfallMerchantRepairPopupWindow", "REPR01I0.IMG", (95, 47, 130, 51), "m39.repair.cost"),
    panel(46, 'D', "DaggerfallMerchantServicePopupWindow", "GNRC01I0.IMG", (95, 56, 130, 42), "m39.merchant.service"),
    rci_cell(panel(47, 'A', "DaggerfallMessageBox", "SPOP.RCI", (0, 0, 22, 22), "m39.messages.selected.text"), 4),
    panel(48, 'A', "DaggerfallPauseOptionsWindow", "OPTN00I0.IMG", (85, 40, 150, 84), "m39.menu.items"),
    panel(49, 'F', "DaggerfallPlayerHistoryWindow", "LGBK00I0.IMG", (0, 0, 320, 200), "m39.history.entries"),
    panel(51, 'E', "DaggerfallPotionMakerWindow", "MASK00I0.IMG", (0, 0, 320, 200), "m39.potionmaker.cost"),
    panel(52, 'F', "DaggerfallQuestJournalWindow", "LGBK00I0.IMG", (0, 0, 320, 200), "m39.journal.entries"),
    // "surface = parchment of the DaggerfallMessageBox that
    // QuestMachine.CreateMessagePrompt creates (line 79)".
    inherited(
        rci_cell(panel(53, 'H', "DaggerfallQuestOfferWindow", "SPOP.RCI", (0, 0, 22, 22), "m39.quest.offers"), 4),
        "DaggerfallMessageBox",
    ),
    panel(55, 'D', "DaggerfallRestWindow", "REST00I0.IMG", (83, 50, 154, 41), "m39.rest.hours"),
    panel(56, 'E', "DaggerfallSpellBookWindow", "SPBK00I0.IMG", (23, 18, 259, 164), "m39.spellbook.count"),
    panel(57, 'E', "DaggerfallSpellMakerWindow", "INFO01I0.IMG", (0, 0, 320, 200), "m39.spellmaker.composed_cost"),
    no_panel(58, 'A', "DaggerfallStartNewGameWizard", FONT_DEFAULT, (0, 0, 5, 7), "m39.menu.selected"),
    panel(59, 'A', "DaggerfallStartWindow", "PICK03I0.IMG", (0, 0, 320, 200), "m39.menu.depth"),
    panel(60, 'H', "DaggerfallTalkWindow", "TALK01I0.IMG", (0, 0, 320, 200), "m39.talk.topics"),
    panel(61, 'D', "DaggerfallTavernWindow", "TVRN00I0.IMG", (95, 47, 130, 44), "m39.inn.room_price"),
    panel(62, 'G', "DaggerfallTeleportPopUp", "TELE00I0.IMG", (75, 58, 171, 57), "m39.teleport.destination"),
    // "inherits DaggerfallInventoryWindow: the surface is INVE00I0.IMG of the
    // base; its own .cs only adds INVE08/10/11/12/14I0.IMG buttons".
    inherited(
        panel(63, 'D', "DaggerfallTradeWindow", "INVE00I0.IMG", (0, 0, 320, 200), "m39.trade.charged_price"),
        "DaggerfallInventoryWindow",
    ),
    panel(64, 'G', "DaggerfallTransportWindow", "MOVE00I0.IMG", (95, 47, 130, 60), "m39.transport.fare"),
    panel(65, 'G', "DaggerfallTravelMapWindow", "TRAV0I00.IMG", (0, 0, 320, 200), "m39.travel.regions"),
    panel(66, 'G', "DaggerfallTravelPopUp", "TRAV0I04.IMG", (49, 29, 223, 96), "m39.travel.cost_gold"),
    no_panel(67, 'A', "DaggerfallUnityMouseControlsWindow", FONT_DEFAULT, (0, 0, 5, 7), "m39.controls.binding"),
    no_panel(68, 'A', "DaggerfallUnitySaveGameWindow", FONT_DEFAULT, (0, 0, 5, 7), "m39.saves.selected.name"),
    no_panel(69, 'A', "DaggerfallUnitySetupGameWizard", FONT_DEFAULT, (0, 0, 5, 7), "m39.settings.language"),
    // "heredado: la clase (DaggerfallUseMagicItemWindow.cs:20) extiende
    // DaggerfallListPickerWindow y no define superficie propia".
    inherited(
        panel(70, 'E', "DaggerfallUseMagicItemWindow", "PICK00I0.IMG", (60, 36, 200, 128), "m39.magicitem.charges"),
        "DaggerfallListPickerWindow",
    ),
    read_only(
        no_panel(71, 'A', "DaggerfallVidPlayerWindow", NO_RASTER, (0, 0, 0, 0), "m39.video.frames"),
        READ_ONLY_071,
    ),
    panel(72, 'H', "DaggerfallWitchesCovenPopupWindow", "DAED00I0.IMG", (95, 47, 130, 51), "m39.coven.service"),
    panel(73, 'A', "DemoClassQuestionsWindow", "CHGN00I0.IMG", (0, 0, 320, 200), "m39.messages.decision"),
    read_only(
        no_panel(74, 'A', "DemoFLCWindow", NO_RASTER, (0, 0, 0, 0), "m39.video.frames"),
        READ_ONLY_074,
    ),
    with_font(
        panel(82, 'E', "SpellIconPickerWindow", "ICON00I0.IMG", (0, 0, 320, 64), "m39.spellicon.selected"),
        RetailFont::Large,
    ),
];

/// One interactive surface of the 51 `Playable` windows (plan §1/§2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct M39Surface {
    /// Census ordinal (`022`…`082`).
    pub id: u16,
    /// Block letter of plan §2.
    pub block: char,
    /// Citable read-only reason (plan §3.4); `None` = interactive.
    pub read_only: Option<&'static str>,
    /// Paint-ready composition: retail panel, nested/owned, plus its glyph
    /// line. `key` is the surface key (the census name, stable and unique).
    pub pilot: PilotWindow,
}

/// The 51 surfaces, in plan §1 order, built once.
static SURFACES: LazyLock<Vec<M39Surface>> = LazyLock::new(|| ROWS.iter().map(build).collect());

/// The 51 surfaces, in plan §1 order.
pub fn surfaces() -> &'static [M39Surface] {
    SURFACES.as_slice()
}

/// One surface by census ordinal.
pub fn decl(id: u16) -> Option<&'static M39Surface> {
    surfaces().iter().find(|surface| surface.id == id)
}

/// Block letter of one census ordinal.
pub fn block_of(id: u16) -> Option<char> {
    decl(id).map(|surface| surface.block)
}

/// Number of surfaces per block, in [`BLOCKS`] order (plan §2 partition:
/// `A 16 + B 1 + C 2 + D 7 + E 7 + F 3 + G 6 + H 9 = 51`).
///
/// Eight entries, not seven: plan §2 — and the acceptance of the lane — needs
/// the count of **every** block, block B (HUD, one window) included.
pub fn count_by_block() -> [(char, usize); 8] {
    let mut counts = BLOCKS.map(|block| (block, 0usize));
    for surface in surfaces() {
        if let Some(entry) = counts
            .iter_mut()
            .find(|(block, _)| *block == surface.block)
        {
            entry.1 += 1;
        }
    }
    counts
}

/// Resolves a census name to its typed handle (plan §3.3: error, never a
/// default).
fn window(name: &'static str) -> WindowId {
    canonical_id_for(name).unwrap_or_else(|| panic!("{RULE_WINDOW}: {name}"))
}

/// Builds the paint-ready surface from its measured row.
fn build(row: &SurfaceRow) -> M39Surface {
    let window_id = window(row.name);
    let rect = UiRect::new(row.rect.0, row.rect.1, row.rect.2, row.rect.3);
    let source = match (row.record, row.clip) {
        (Some(record), _) => SurfaceSource::RciRecord { record },
        (None, Some((x, y, width, height))) => {
            SurfaceSource::Rect(UiRect::new(x, y, width, height))
        }
        (None, None) => SurfaceSource::Whole,
    };
    let surfaces = if row.panel {
        vec![match row.inherited_from {
            Some(owner) => SurfaceSpec::nested(row.asset, rect, source, window(owner)),
            None => SurfaceSpec::owned(row.asset, rect, source),
        }]
    } else {
        Vec::new()
    };
    // The run starts at the panel's own origin (no new geometry is declared).
    let at = (rect.x, rect.y);
    let texts = vec![TextDecl::line(
        TextSource::ReadModel(row.text),
        row.font,
        INK_BODY,
        at,
    )];
    M39Surface {
        id: row.id,
        block: row.block,
        read_only: row.read_only,
        pilot: PilotWindow {
            key: row.name,
            name: row.name,
            asset: row.asset,
            window: window_id,
            composition: WindowComposition::new(window_id, surfaces),
            texts,
            // Measured, never declared: the probe derives one row per delivered
            // outcome of this surface's block (no invented control inventory);
            // the read-only rows (031/071/074, plan §3.4) declare none.
            focus: Vec::new(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    use daggerfall_parser_bevy_bridge::ui_playable_g1;
    use daggerfall_parser_bevy_bridge::ui_playable_g2;
    use daggerfall_parser_bevy_bridge::ui_playable_g3;
    use daggerfall_parser_bevy_bridge::ui_spec::{WindowSpec, CANVAS_HEIGHT, CANVAS_WIDTH};
    use daggerfall_perception::read_models::{
        M39CommerceModels, M39InventoryModels, M39JournalModels, M39MagicModels, M39MenuModels,
        M39Models, M39SocialModels, M39TravelModels, SessionReadModels,
    };
    use daggerfall_perception::ui_census::{census, WindowCategory};

    /// The only **read-only** surfaces (plan §3.4 / §7.6).
    const READ_ONLY_IDS: [u16; 3] = [31, 71, 74];

    /// The six `asset_kind=FNT` rows: text-only, still interactive (§7.6).
    const FNT_IDS: [u16; 6] = [22, 42, 58, 67, 68, 69];

    /// The six rows that reuse their base window's panel (TSV `hereda` note).
    const INHERITED_IDS: [u16; 6] = [34, 35, 37, 53, 63, 70];

    /// Deviation, measured: the TSV carries `asset_retail=-` for the three
    /// `asset_kind=none` rows, while `ui_playable_g3::specs()` names one
    /// representative retail container of the same family. The table keeps the
    /// TSV value; this pin makes the mismatch explicit and catches drift.
    const NO_RASTER_SPEC_ASSETS: [(u16, &str); 3] = [
        (31, "HIRCINE.FLC"),
        (71, "ANIM0001.VID"),
        (74, "AZURA.FLC"),
    ];

    /// Every block but B maps to one declared route family (plan §5).
    fn block_routes(block: char) -> Option<&'static [&'static str]> {
        match block {
            'A' => Some(M39MenuModels::PATHS),
            'C' => Some(M39InventoryModels::PATHS),
            'D' => Some(M39CommerceModels::PATHS),
            'E' => Some(M39MagicModels::PATHS),
            'F' => Some(M39JournalModels::PATHS),
            'G' => Some(M39TravelModels::PATHS),
            'H' => Some(M39SocialModels::PATHS),
            _ => None,
        }
    }

    /// The 51 `WindowSpec` of the wave-3 composition slot, in slot order.
    fn composed_specs() -> Vec<WindowSpec> {
        ui_playable_g1::specs()
            .iter()
            .chain(ui_playable_g2::specs())
            .chain(ui_playable_g3::specs())
            .copied()
            .collect()
    }

    /// **(a)** 51 rows, 51 unique ascending ids, equal to the `Playable` census
    /// set by **name and ordinal**.
    #[test]
    fn fifty_one_rows_are_the_playable_census() {
        let surfaces = surfaces();
        assert_eq!(surfaces.len(), 51, "plan §1: 51 rows");
        assert!(
            surfaces.windows(2).all(|pair| pair[0].id < pair[1].id),
            "plan §1 order: ascending census ordinal, no duplicates"
        );

        let expected: Vec<(u16, &'static str)> = census()
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.category == WindowCategory::Playable)
            .map(|(index, entry)| ((index + 1) as u16, entry.name))
            .collect();
        assert_eq!(expected.len(), 51, "census: 51 Playable windows");

        for (ordinal, name) in expected {
            let surface = decl(ordinal)
                .unwrap_or_else(|| panic!("missing surface for ordinal {ordinal} ({name})"));
            assert_eq!(surface.pilot.name, name, "ordinal {ordinal} name");
            assert_eq!(
                surface.pilot.window,
                canonical_id_for(name).expect("census name resolves"),
                "ordinal {ordinal} typed handle"
            );
            assert_eq!(block_of(ordinal), Some(surface.block));
        }
    }

    /// **(b)** Partition `A16 B1 C2 D7 E7 F3 G6 H9` (plan §2).
    #[test]
    fn block_partition_is_16_1_2_7_7_3_6_9() {
        assert_eq!(
            count_by_block(),
            [
                ('A', 16),
                ('B', 1),
                ('C', 2),
                ('D', 7),
                ('E', 7),
                ('F', 3),
                ('G', 6),
                ('H', 9),
            ]
        );
        assert_eq!(
            count_by_block().iter().map(|(_, n)| *n).sum::<usize>(),
            surfaces().len()
        );
    }

    /// **(c)** TSV `asset_retail` == `ui_playable_g1/g2/g3::specs()`; the three
    /// `asset_retail=-` rows are the declared deviation.
    #[test]
    fn tsv_asset_matches_the_composed_window_specs() {
        let specs = composed_specs();
        assert_eq!(specs.len(), 51, "wave 3 slot: 51 specs");
        for surface in surfaces() {
            let spec = specs
                .iter()
                .find(|spec| spec.id == surface.pilot.name)
                .unwrap_or_else(|| panic!("no composed spec for {}", surface.pilot.name));
            let asset = spec
                .asset_retail
                .unwrap_or_else(|| panic!("spec {} has no retail asset", spec.id));
            if surface.pilot.asset == NO_RASTER {
                let expected = NO_RASTER_SPEC_ASSETS
                    .iter()
                    .find(|(id, _)| *id == surface.id)
                    .map(|(_, asset)| *asset)
                    .unwrap_or_else(|| panic!("{} has no declared deviation", surface.id));
                assert_eq!(
                    asset, expected,
                    "declared deviation of ordinal {} drifted",
                    surface.id
                );
            } else {
                assert_eq!(
                    asset, surface.pilot.asset,
                    "ordinal {}: TSV asset != composed spec",
                    surface.id
                );
            }
        }
    }

    /// **(d)** Read-only == `{031, 071, 074}`, each with a citable reason and
    /// no raster panel.
    #[test]
    fn read_only_is_exactly_031_071_074() {
        let read_only: Vec<u16> = surfaces()
            .iter()
            .filter(|surface| surface.read_only.is_some())
            .map(|surface| surface.id)
            .collect();
        assert_eq!(read_only, READ_ONLY_IDS);
        for surface in surfaces()
            .iter()
            .filter(|surface| surface.read_only.is_some())
        {
            let reason = surface.read_only.unwrap_or_default();
            assert!(!reason.is_empty(), "{}: read-only needs a reason", surface.id);
            assert_eq!(
                surface.pilot.asset, NO_RASTER,
                "{}: read-only is the `assets=-` set",
                surface.id
            );
            assert!(
                surface.pilot.composition.surfaces.is_empty(),
                "{}: read-only paints no panel",
                surface.id
            );
        }
    }

    /// **(e)** Text-only == the six `FNT` windows (empty composition, not
    /// read-only), each painting its glyph line.
    #[test]
    fn text_only_surfaces_are_the_six_fnt_windows() {
        let text_only: Vec<u16> = surfaces()
            .iter()
            .filter(|surface| surface.pilot.composition.surfaces.is_empty())
            .filter(|surface| surface.read_only.is_none())
            .map(|surface| surface.id)
            .collect();
        assert_eq!(text_only, FNT_IDS);
        for surface in surfaces().iter().filter(|s| FNT_IDS.contains(&s.id)) {
            assert_eq!(surface.pilot.asset, FONT_DEFAULT);
            assert!(!surface.pilot.texts.is_empty(), "{}: no glyph line", surface.id);
        }
    }

    /// **(f)** Every route referenced by `texts` is declared: `m39.*` in
    /// `M39Models::paths()` **and in the surface's own block family**; block B
    /// (no `m39.*` family) in the pre-existing session projection.
    #[test]
    fn every_text_route_is_declared_and_belongs_to_its_block() {
        let declared = M39Models::paths();
        let session = SessionReadModels::default();
        for surface in surfaces() {
            assert!(!surface.pilot.texts.is_empty(), "{}: no text", surface.id);
            let mut read_models = 0;
            for decl in &surface.pilot.texts {
                let TextSource::ReadModel(path) = decl.source else {
                    continue;
                };
                read_models += 1;
                assert!(
                    declared.contains(&path) || path.starts_with("hud."),
                    "{}: undeclared route {path}",
                    surface.id
                );
                match block_routes(surface.block) {
                    Some(routes) => assert!(
                        routes.contains(&path),
                        "{} (block {}): route {path} is not of its own block",
                        surface.id,
                        surface.block
                    ),
                    None => {
                        assert_eq!(surface.block, 'B');
                        assert!(
                            path.starts_with("hud."),
                            "{}: block B reads the HUD projection, not {path}",
                            surface.id
                        );
                        assert!(session.resolve(path).is_some(), "{}: {path}", surface.id);
                    }
                }
            }
            assert!(read_models >= 1, "{}: no ReadModel text", surface.id);
        }
    }

    /// **(g)** Every panel composition has ≥1 placed surface and every
    /// `canvas_rect` fits the 320×200 canvas; the no-raster rows compose
    /// nothing.
    #[test]
    fn compositions_are_placed_inside_the_canvas() {
        for surface in surfaces() {
            let placed = &surface.pilot.composition.surfaces;
            if placed.is_empty() {
                assert!(
                    surface.pilot.asset == NO_RASTER || surface.pilot.asset == FONT_DEFAULT,
                    "{}: only the no-raster and FNT rows lack a panel",
                    surface.id
                );
                continue;
            }
            for spec in placed {
                assert_eq!(
                    spec.asset, surface.pilot.asset,
                    "{}: panel asset != TSV asset",
                    surface.id
                );
                assert!(
                    spec.canvas_rect.fits_canvas(),
                    "{}: rect {:?} leaves 320x200",
                    surface.id,
                    spec.canvas_rect
                );
                assert!(
                    spec.canvas_rect.width > 0
                        && spec.canvas_rect.height > 0
                        && spec.canvas_rect.right() <= CANVAS_WIDTH as i16
                        && spec.canvas_rect.bottom() <= CANVAS_HEIGHT as i16,
                    "{}: degenerate or out-of-canvas rect {:?}",
                    surface.id,
                    spec.canvas_rect
                );
                let nested = matches!(
                    spec.ownership,
                    daggerfall_parser_bevy_bridge::ui_compat::SurfaceOwnership::Nested(_)
                );
                assert_eq!(
                    nested,
                    INHERITED_IDS.contains(&surface.id),
                    "{}: nested ownership != TSV `hereda` note",
                    surface.id
                );
            }
        }
    }

    /// The six `SPOP.RCI` rows: one 22×22 **centre cell**, not a nine-slice.
    const SPOP_IDS: [u16; 6] = [34, 35, 37, 39, 47, 53];

    /// ARENA2 of the probe: `DAGGERFALL_ARENA2`, then the shared resolution.
    fn retail_dir() -> Option<PathBuf> {
        match std::env::var("DAGGERFALL_ARENA2") {
            Ok(value) if !value.trim().is_empty() => Some(PathBuf::from(value)),
            _ => crate::m37_ui1_probe::arena2_dir(),
        }
    }

    /// Every retail asset named by a composition (the probe's own extra list).
    fn composition_image_assets() -> Vec<&'static str> {
        let mut assets: Vec<&'static str> = Vec::new();
        for surface in surfaces() {
            for spec in &surface.pilot.composition.surfaces {
                let name = spec.asset.as_str();
                if name.ends_with(".IMG") && !assets.contains(&name) {
                    assets.push(name);
                }
            }
        }
        assets
    }

    /// **(h)** All 51 compositions resolve against the **retail** bank:
    /// `WindowComposition::nodes(&assets)` fails on none, so rule 3 cannot
    /// break the probe again without this test going red. The six `SPOP.RCI`
    /// rows declare the centre cell (record 4) of the measured 3×3 container
    /// (C-4).
    ///
    /// The retail bytes are never packaged (INV-L99-6): without ARENA2 the
    /// test states the skip instead of substituting a synthetic bank, exactly
    /// like the probe (`VERDICT: SKIP - no ARENA2`).
    #[test]
    fn all_51_compositions_resolve_against_the_retail_bank() {
        for id in SPOP_IDS {
            let surface = decl(id).unwrap_or_else(|| panic!("missing surface {id}"));
            let spec = surface
                .pilot
                .composition
                .surfaces
                .first()
                .unwrap_or_else(|| panic!("{id}: no panel"));
            assert_eq!(spec.asset, "SPOP.RCI");
            assert_eq!(
                spec.source,
                SurfaceSource::RciRecord { record: 4 },
                "{id}: a 22x22 target is one centre cell, not a nine-slice"
            );
            assert_eq!(
                (spec.canvas_rect.width, spec.canvas_rect.height),
                (22, 22),
                "{id}: the destination stays the measured cell"
            );
        }
        let Some(dir) = retail_dir() else {
            eprintln!("skip all_51_compositions_resolve_against_the_retail_bank: no ARENA2");
            return;
        };
        let bank = crate::m37_ui1_probe::load_bank_with(&dir, &composition_image_assets())
            .unwrap_or_else(|err| panic!("retail bank: {err}"));
        let mut failures: Vec<String> = Vec::new();
        for surface in surfaces() {
            if let Err(err) = surface.pilot.nodes(&bank.assets) {
                failures.push(format!("{:03}: {err}", surface.id));
            }
        }
        assert!(
            failures.is_empty(),
            "{} of 51 compositions do not resolve:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}

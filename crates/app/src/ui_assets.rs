//! Shared loader for the 1996 UI retail assets (M17/M18).
//!
//! A single read of `PAL.PAL` + `FMAP_PAL.COL` + `ART_PAL.COL` +
//! [`PANEL_ASSETS`] + [`FONT_FILES`] from ARENA2 Mode A, consumed by the
//! `UI-001` probe, the M17 window and the integrated M18 demo (0 duplicated
//! loading logic; INV-L99-6 custody: the bytes are read at runtime, nothing is
//! packaged).
//!
//! The session panels ([`session_panels`]) are built from the M7 read models
//! with the same canonical data on all three surfaces.

use std::path::Path;

use daggerfall_inventory::data_root::DataRoot;
use daggerfall_parser_bevy_bridge::menu_spec::{
    self, MenuScreen, DEFAULT_ACTIVE_ROW, PORTRAIT_ASSET, PORTRAIT_FRAME,
};
use daggerfall_parser_bevy_bridge::ui_spec::{
    build_map_panels, build_world_panels, HudPanelData, InventoryPanelData, QuestPanelData,
    UiAssets, UiPanel, FONT_FILES,
};
use daggerfall_parser_rs::image::{decode_cif_frame, decode_img_file, FntFile};
use daggerfall_parser_rs::texture::Palette;
use daggerfall_perception::read_models::{
    HudReadModel, HudSource, PanelSource, SessionPanelReadModel,
};

use crate::m17_probe::PANEL_ASSETS;

/// Loads the UI retail assets from the ARENA2 directory.
///
/// `dir` is the ARENA2 directory already resolved by the caller (Mode A). The
/// error is a `String` with the literal reason (never a synthetic replacement
/// asset, C7).
pub(crate) fn load_ui_assets(dir: &Path) -> Result<UiAssets, String> {
    // **§4.6** — the `DataRoot` owns the case-insensitive lookup and the
    // provenance; the caller only names the data directory it came from.
    let root = DataRoot::from_arena2(dir).map_err(|err| err.message())?;
    let read = |name: &str| -> Result<Vec<u8>, String> {
        root.open(&root.rel(name))
            .map(|(bytes, _provenance)| bytes)
            .map_err(|err| err.message())
    };
    let mut assets = UiAssets::new(
        Palette::parse(&read("PAL.PAL")?).map_err(|err| err.to_string())?,
        Palette::parse(&read("FMAP_PAL.COL")?).map_err(|err| err.to_string())?,
        Palette::parse(&read("ART_PAL.COL")?).map_err(|err| err.to_string())?,
    );
    for name in PANEL_ASSETS {
        let bytes = read(name)?;
        let image = decode_img_file(&bytes).map_err(|err| format!("{name}: {err}"))?;
        assets = assets
            .with_image(name, image)
            .map_err(|err| err.to_string())?;
    }
    // The modern menu's detail plate portrait: `FACES.CIF` frame 3 (plain RCI,
    // 61 × 64×64 raw). The container has no header and no palette; the index is
    // addressed as data, never guessed.
    let faces = decode_cif_frame(&read(PORTRAIT_ASSET)?, PORTRAIT_FRAME)
        .map_err(|err| format!("{PORTRAIT_ASSET}[{PORTRAIT_FRAME}]: {err}"))?;
    assets = assets
        .with_image(PORTRAIT_ASSET, faces)
        .map_err(|err| err.to_string())?;
    for name in FONT_FILES {
        let font = FntFile::parse(&read(name)?).map_err(|err| format!("{name}: {err}"))?;
        assets = assets.with_font(name, font);
    }
    Ok(assets)
}

/// The session panels: the three **world** panels of the 1996 UI (HUD ·
/// inventory · quests), the **modern menu** on its own 320×180 canvas, and the
/// two **B3-c** map panels (`map-local` · `map-world`).
///
/// Census: **6** panels; `main-menu` is index **3** on both surfaces because the
/// menu occupies the same slot the retail menu plate used to (`m18_phase`).
/// The retail menu panel is out of the production path
/// (`MENU-MODERN-DESIGN.md` §9.4) and survives only as the `UI-001` fixture.
/// The map panels are the retail full-screen screens (`MAP100I0.IMG` /
/// `TRAV0I00.IMG`); their index range is resolved **per asset** by the
/// compositor (see `build_map_panels`).
pub(crate) fn session_panels(assets: &UiAssets) -> Result<Vec<UiPanel>, String> {
    session_panels_with_quests(
        assets,
        &[
            "The Scholar's Request".to_string(),
            "A Rat in the Cellar".to_string(),
        ],
    )
}

/// The menu screen of the session: shell default row + the save store.
pub(crate) fn menu_screen(active_row: usize, continue_enabled: bool) -> MenuScreen {
    menu_spec::main_menu(active_row, continue_enabled)
}

/// Whether `CONTINUE` is selectable — the save store the shell routes with
/// (`shell_slots::scan`, the only consumer of `DF_SAVE_DIR`).
pub(crate) fn continue_enabled() -> bool {
    crate::shell_slots::scan().count() > 0
}

/// The session panels with an explicit quest list (the demo passes the retail
/// journal titles; `PROBE-UI-001` keeps its own fixture).
pub(crate) fn session_panels_with_quests(
    assets: &UiAssets,
    quests: &[String],
) -> Result<Vec<UiPanel>, String> {
    let hud = HudReadModel::project(&HudSource {
        health: 100,
        max_health: 100,
        stamina: 80,
        max_stamina: 100,
        level: 3,
    });
    let session = SessionPanelReadModel::project(&PanelSource {
        gold: 120,
        items: vec![
            ("saber".to_string(), 1),
            ("potion".to_string(), 3),
            ("scroll".to_string(), 2),
        ],
        quests: quests.to_vec(),
    });
    let mut panels = build_world_panels(
        &HudPanelData {
            health: hud.health,
            max_health: hud.max_health,
            stamina: hud.stamina,
            max_stamina: hud.max_stamina,
            level: hud.level,
        },
        &InventoryPanelData {
            gold: session.gold,
            item_kinds: session.item_kinds(),
            total_units: session.total_units(),
        },
        &QuestPanelData {
            quests: session.quests.clone(),
        },
        assets,
    )
    .map_err(|err| err.to_string())?;
    panels.push(
        MenuScreen::compose(&menu_screen(DEFAULT_ACTIVE_ROW, continue_enabled()), assets)
            .map_err(|err| err.to_string())?,
    );
    let [local, world] = build_map_panels(assets).map_err(|err| err.to_string())?;
    panels.push(local);
    panels.push(world);
    Ok(panels)
}

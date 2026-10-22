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

use daggerfall_parser_bevy_bridge::ui_spec::{
    build_all_panels, HudPanelData, InventoryPanelData, QuestPanelData, UiAssets, UiPanel,
    FONT_FILES,
};
use daggerfall_parser_rs::image::{decode_img_file, FntFile};
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
    let read = |name: &str| -> Result<Vec<u8>, String> {
        std::fs::read(dir.join(name)).map_err(|err| format!("{name}: {err}"))
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
    for name in FONT_FILES {
        let font = FntFile::parse(&read(name)?).map_err(|err| format!("{name}: {err}"))?;
        assets = assets.with_font(name, font);
    }
    Ok(assets)
}

/// The 4 session panels (HUD · inventory · quests · menu) from the M7 read
/// models with the canonical contract M17 data.
pub(crate) fn session_panels(assets: &UiAssets) -> Result<Vec<UiPanel>, String> {
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
        quests: vec![
            "The Scholar's Request".to_string(),
            "A Rat in the Cellar".to_string(),
        ],
    });
    build_all_panels(
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
    .map_err(|err| err.to_string())
}

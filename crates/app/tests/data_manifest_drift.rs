//! Antibody for the single source of truth of the retail file inventory.
//!
//! The canonical manifest lives in `daggerfall_inventory::retail_manifest`
//! (data contract §2). The Bevy bridge carries its own copy of the font list
//! (`ui_spec::FONT_FILES`), which cannot import the inventory crate, so the
//! two can drift silently: a font added to the manifest would be validated by
//! the data gate and then never loaded, or the reverse. This test pins them
//! together so the drift fails here instead of at runtime under a window.
//!
//! Likewise `PANEL_ASSETS` must stay the manifest's list (the probes re-export
//! it; a local re-definition would reopen the four copies this lane killed).

use daggerfall_inventory::retail_manifest;
use daggerfall_parser_bevy_bridge::ui_spec;

/// Every font the data gate accepts is every font the UI can load.
#[test]
fn bridge_fonts_match_the_canonical_manifest() {
    assert_eq!(
        ui_spec::FONT_FILES.to_vec(),
        retail_manifest::FONT_FILES.to_vec(),
        "ui_spec::FONT_FILES drifted from retail_manifest::FONT_FILES"
    );
}

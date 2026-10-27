//! Wave 3 composition slot — **G3 (magia, 6) + G4 (mapas/viaje, 5) = 11 windows**.
//!
//! Rows of `docs/sintergic/l5-mission/m34-window-assets-CR-2d5c8f41.tsv` whose
//! `group` is `G3` or `G4`; the partition is
//! `docs/sintergic/l5-mission/m34-playable-groups-CR-2d5c8f41.txt` (G3 = 6,
//! G4 = 5, no holes, no duplicates). `id` is the census name (GV-19,
//! `c85e0bb5ae867373` over the 51); every `asset_retail` and `rect` is copied
//! from that TSV, never re-derived here.
//!
//! **Anchors.** [`WindowSpec`] has no anchor field (`ui_spec.rs` is frozen for
//! this lane), so the TSV's `anchor` column lives beside the specs in
//! [`WINDOW_ROWS`]: [`UiAnchor::Header`] for the four `ImgFile` records
//! (`x/y/w/h` from the header) and [`UiAnchor::Declared`] for the seven
//! raw-`RCI` frames, which use one of the two named composition constants
//! `DECLARED_CANVAS` / `DECLARED_ICON_ATLAS`.
//!
//! **`RCI` trap (TSV row notes).** `TRAV0I00.IMG` (travel map) and
//! `AMAP00I0.IMG` ×2 (the two automaps) — like `INFO01I0`/`ITEM00I0`/
//! `MASK00I0`/`ICON00I0` — are raw `RCI` frames behind an `.IMG` name:
//! `decode_img_file` classifies them (`classify_img` → `ImgClass::Rci`) and
//! returns the frame, so their declared rect is the frame's own `W×H`.
//! `SPOP.RCI` is the *other* case — multi-record, needs
//! `decode_rci(record, 22, 22)` per record — and **no G3/G4 row touches it**
//! (only G1/G5/G6 do). This slot decodes nothing: it declares data.
//!
//! [`WindowSpec`]: crate::ui_spec::WindowSpec

use crate::ui_spec::{UiAnchor, UiRect, WindowSpec, CANVAS_HEIGHT, CANVAS_WIDTH};

/// Declared rect of a full-canvas raw-`RCI` frame (`anchor=Declared` in the
/// TSV): the composition constant **is** the canvas, and the asset's own
/// `W×H` is `320×200`.
const DECLARED_CANVAS: UiRect = UiRect {
    x: 0,
    y: 0,
    width: CANVAS_WIDTH,
    height: CANVAS_HEIGHT,
};

/// Declared rect of the `ICON00I0.IMG` atlas (`320×64` raw `RCI`, 69 icons —
/// `SpellIconCollection.cs:27`): `SpellIconPickerWindow` paints an atlas row,
/// not the canvas. The panel itself carries no texture of its own.
const DECLARED_ICON_ATLAS: UiRect = UiRect {
    x: 0,
    y: 0,
    width: 320,
    height: 64,
};

/// Group of a window in the W34-2 partition (`m34-playable-groups`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowGroup {
    /// `G3` (W34-2c) — magic: spell book, spell/item/potion makers, magic
    /// item picker, spell icon picker.
    G3Magic,
    /// `G4` (W34-2d) — maps and travel: travel map, travel popup, the two
    /// automaps, transport.
    G4Maps,
}

/// One TSV row of this lane that [`WindowSpec`] cannot carry: which group the
/// window belongs to and where its rect comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowRow {
    /// Census name — must equal the matching [`WindowSpec::id`].
    pub id: &'static str,
    /// W34-2 partition group.
    pub group: WindowGroup,
    /// Origin of the rect (TSV `anchor` column).
    pub anchor: UiAnchor,
}

/// The 6 `G3` windows, in TSV order.
pub const G3_LEN: usize = 6;

/// The 5 `G4` windows, in TSV order (`SPECS[G3_LEN..]`).
pub const G4_LEN: usize = 5;

/// Anchor/group of the 11 specs, **in [`specs`] order** (enforced by
/// `window_rows_follow_specs_order`).
pub const WINDOW_ROWS: [WindowRow; 11] = [
    WindowRow {
        id: "DaggerfallSpellBookWindow",
        group: WindowGroup::G3Magic,
        anchor: UiAnchor::Header,
    },
    WindowRow {
        id: "DaggerfallSpellMakerWindow",
        group: WindowGroup::G3Magic,
        anchor: UiAnchor::Declared("DECLARED_CANVAS_320X200"),
    },
    WindowRow {
        id: "DaggerfallItemMakerWindow",
        group: WindowGroup::G3Magic,
        anchor: UiAnchor::Declared("DECLARED_CANVAS_320X200"),
    },
    WindowRow {
        id: "DaggerfallPotionMakerWindow",
        group: WindowGroup::G3Magic,
        anchor: UiAnchor::Declared("DECLARED_CANVAS_320X200"),
    },
    WindowRow {
        id: "DaggerfallUseMagicItemWindow",
        group: WindowGroup::G3Magic,
        anchor: UiAnchor::Header,
    },
    WindowRow {
        id: "SpellIconPickerWindow",
        group: WindowGroup::G3Magic,
        anchor: UiAnchor::Declared("DECLARED_ICON_ATLAS_320X64"),
    },
    WindowRow {
        id: "DaggerfallTravelMapWindow",
        group: WindowGroup::G4Maps,
        anchor: UiAnchor::Declared("DECLARED_CANVAS_320X200"),
    },
    WindowRow {
        id: "DaggerfallTravelPopUp",
        group: WindowGroup::G4Maps,
        anchor: UiAnchor::Header,
    },
    WindowRow {
        id: "DaggerfallAutomapWindow",
        group: WindowGroup::G4Maps,
        anchor: UiAnchor::Declared("DECLARED_CANVAS_320X200"),
    },
    WindowRow {
        id: "DaggerfallExteriorAutomapWindow",
        group: WindowGroup::G4Maps,
        anchor: UiAnchor::Declared("DECLARED_CANVAS_320X200"),
    },
    WindowRow {
        id: "DaggerfallTransportWindow",
        group: WindowGroup::G4Maps,
        anchor: UiAnchor::Header,
    },
];

/// The 11 composed windows: `G3` (`[..G3_LEN]`) then `G4` (`[G3_LEN..]`).
static SPECS: [WindowSpec; G3_LEN + G4_LEN] = [
    // ── G3 (W34-2c) magia ─────────────────────────────────────────────────
    // DFU:Assets/Scripts/Game/UserInterfaceWindows/DaggerfallSpellBookWindow.cs:96
    // Header IMG (pergamino grande): 12 B header + 259×164 ⇒ 42488 B.
    WindowSpec {
        id: "DaggerfallSpellBookWindow",
        asset_retail: Some("SPBK00I0.IMG"),
        rect: UiRect {
            x: 23,
            y: 18,
            width: 259,
            height: 164,
        },
    },
    // DFU:Assets/Scripts/Game/UserInterfaceWindows/DaggerfallSpellMakerWindow.cs:126
    // Raw RCI 320×200 behind `.IMG` (64000 B); effect names in LargeFont.
    WindowSpec {
        id: "DaggerfallSpellMakerWindow",
        asset_retail: Some("INFO01I0.IMG"),
        rect: DECLARED_CANVAS,
    },
    // DFU:Assets/Scripts/Game/UserInterfaceWindows/DaggerfallItemMakerWindow.cs:111
    // Raw RCI 320×200; material selectors in SmallFont (`FONT0002`).
    WindowSpec {
        id: "DaggerfallItemMakerWindow",
        asset_retail: Some("ITEM00I0.IMG"),
        rect: DECLARED_CANVAS,
    },
    // DFU:Assets/Scripts/Game/UserInterfaceWindows/DaggerfallPotionMakerWindow.cs:74
    // Raw RCI 320×200; ingredient labels in SmallFont (`FONT0002`).
    WindowSpec {
        id: "DaggerfallPotionMakerWindow",
        asset_retail: Some("MASK00I0.IMG"),
        rect: DECLARED_CANVAS,
    },
    // DFU:Assets/Scripts/Game/UserInterfaceWindows/DaggerfallListPickerWindow.cs:31
    // Inherited surface: the class extends the list picker
    // (DaggerfallUseMagicItemWindow.cs:20) and declares no surface of its own,
    // so the rect is the `PICK00I0.IMG` header the base loads.
    WindowSpec {
        id: "DaggerfallUseMagicItemWindow",
        asset_retail: Some("PICK00I0.IMG"),
        rect: UiRect {
            x: 60,
            y: 36,
            width: 200,
            height: 128,
        },
    },
    // DFU:Assets/Scripts/Game/UserInterface/SpellIconCollection.cs:34
    // The panel has no texture of its own; content is the 320×64 raw-RCI atlas
    // of 69 icons (SpellIconCollection.cs:27); header in LargeFont.
    WindowSpec {
        id: "SpellIconPickerWindow",
        asset_retail: Some("ICON00I0.IMG"),
        rect: DECLARED_ICON_ATLAS,
    },
    // ── G4 (W34-2d) mapas/viaje ───────────────────────────────────────────
    // DFU:Assets/Scripts/Game/UserInterfaceWindows/DaggerfallTravelMapWindow.cs:40
    // Raw RCI 320×200 frame; the regional map is runtime world tiles and the
    // side buttons are the `TRAV0I0xI0.IMG` satellites.
    WindowSpec {
        id: "DaggerfallTravelMapWindow",
        asset_retail: Some("TRAV0I00.IMG"),
        rect: DECLARED_CANVAS,
    },
    // DFU:Assets/Scripts/Game/UserInterfaceWindows/DaggerfallTravelPopUp.cs:29
    // Header IMG: 12 B header + 223×96 ⇒ 21420 B.
    WindowSpec {
        id: "DaggerfallTravelPopUp",
        asset_retail: Some("TRAV0I04.IMG"),
        rect: UiRect {
            x: 49,
            y: 29,
            width: 223,
            height: 96,
        },
    },
    // DFU:Assets/Scripts/Game/UserInterfaceWindows/DaggerfallAutomapWindow.cs:112
    // Raw RCI 320×200 frame; the dungeon map is drawn at runtime from the RDB
    // side (W34-D, `RDBSide` 51.2 m) — the frame is the only retail surface.
    WindowSpec {
        id: "DaggerfallAutomapWindow",
        asset_retail: Some("AMAP00I0.IMG"),
        rect: DECLARED_CANVAS,
    },
    // DFU:Assets/Scripts/Game/UserInterfaceWindows/DaggerfallExteriorAutomapWindow.cs:88
    // Raw RCI 320×200 frame; the exterior map is drawn at runtime.
    WindowSpec {
        id: "DaggerfallExteriorAutomapWindow",
        asset_retail: Some("AMAP00I0.IMG"),
        rect: DECLARED_CANVAS,
    },
    // DFU:Assets/Scripts/Game/UserInterfaceWindows/DaggerfallTransportWindow.cs:53
    // Header IMG: 12 B header + 130×60 ⇒ 7812 B. No text of its own (labels are
    // baked in the IMG); buttons use TextureReplacement.
    WindowSpec {
        id: "DaggerfallTransportWindow",
        asset_retail: Some("MOVE00I0.IMG"),
        rect: UiRect {
            x: 95,
            y: 47,
            width: 130,
            height: 60,
        },
    },
];

/// The 11 `G3` + `G4` window specs, reachable through the single
/// concatenation point [`crate::ui_spec::window_specs`].
pub fn specs() -> &'static [WindowSpec] {
    &SPECS
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use daggerfall_parser_rs::image::decode_img_file;

    use super::{
        specs, WindowGroup, G3_LEN, G4_LEN, WINDOW_ROWS, DECLARED_CANVAS, DECLARED_ICON_ATLAS,
    };
    use crate::ui_spec::{asset_is_generic, window_specs, UiAnchor, UiRect};

    #[test]
    fn declares_eleven_windows() {
        assert_eq!(specs().len(), 11, "G3 6 + G4 5 from the TSV");
        assert_eq!(G3_LEN + G4_LEN, 11);
    }

    #[test]
    fn groups_split_six_magic_and_five_maps() {
        let g3 = WINDOW_ROWS
            .iter()
            .filter(|row| row.group == WindowGroup::G3Magic)
            .count();
        let g4 = WINDOW_ROWS
            .iter()
            .filter(|row| row.group == WindowGroup::G4Maps)
            .count();
        assert_eq!((g3, g4), (G3_LEN, G4_LEN), "partition 6 + 5, no holes");
        // Block order: G3 first, then G4 (`SPECS[..G3_LEN]` / `SPECS[G3_LEN..]`).
        for (index, row) in WINDOW_ROWS.iter().enumerate() {
            let expected = if index < G3_LEN {
                WindowGroup::G3Magic
            } else {
                WindowGroup::G4Maps
            };
            assert_eq!(row.group, expected, "row {index} ({}) out of block", row.id);
        }
    }

    #[test]
    fn window_rows_follow_specs_order() {
        assert_eq!(WINDOW_ROWS.len(), specs().len(), "anchor table ↔ specs");
        for (spec, row) in specs().iter().zip(WINDOW_ROWS.iter()) {
            assert_eq!(spec.id, row.id, "WINDOW_ROWS must follow specs() order");
        }
    }

    #[test]
    fn every_window_is_concrete() {
        for spec in specs() {
            assert!(spec.is_concrete(), "{} is not concrete", spec.id);
        }
    }

    #[test]
    fn rects_stay_inside_the_canvas() {
        for spec in specs() {
            assert!(spec.rect.width > 0 && spec.rect.height > 0, "{}", spec.id);
            assert!(spec.rect.fits_canvas(), "{} rect outside 320×200", spec.id);
        }
    }

    /// `generic_widget=0` is audited against `asset_retail`, **never** against
    /// the TSV's `asset_kind` column.
    #[test]
    fn no_generic_widget_against_asset_retail() {
        for spec in specs() {
            let asset = spec.asset_retail.expect("concrete window");
            assert!(!asset_is_generic(asset), "{}: asset_retail={asset}", spec.id);
        }
    }

    #[test]
    fn assets_are_img_records() {
        for spec in specs() {
            let asset = spec.asset_retail.expect("concrete window");
            assert!(
                asset.ends_with(".IMG"),
                "{}: TSV asset_kind=IMG, got {asset}",
                spec.id
            );
        }
    }

    #[test]
    fn declared_rows_use_a_named_constant() {
        let mut header = 0;
        let mut declared = 0;
        for (spec, row) in specs().iter().zip(WINDOW_ROWS.iter()) {
            match row.anchor {
                UiAnchor::Header => {
                    header += 1;
                    assert_ne!(
                        (spec.rect.x, spec.rect.y),
                        (0, 0),
                        "{}: header rect must carry the header offsets",
                        spec.id
                    );
                }
                UiAnchor::Declared(name) => {
                    declared += 1;
                    let named = (spec.rect == DECLARED_CANVAS
                        && name == "DECLARED_CANVAS_320X200")
                        || (spec.rect == DECLARED_ICON_ATLAS
                            && name == "DECLARED_ICON_ATLAS_320X64");
                    assert!(
                        named,
                        "{}: Declared({name}) does not name its rect {:?}",
                        spec.id, spec.rect
                    );
                }
            }
        }
        assert_eq!((header, declared), (4, 7), "TSV anchor column: 4 Header + 7 Declared");
    }

    /// The four `ImgFile` rects, golden from the TSV (header `w/h` + offsets).
    #[test]
    fn header_rows_match_the_tsv_rects() {
        let golden: [(&str, UiRect); 4] = [
            ("DaggerfallSpellBookWindow", UiRect::new(23, 18, 259, 164)),
            ("DaggerfallUseMagicItemWindow", UiRect::new(60, 36, 200, 128)),
            ("DaggerfallTravelPopUp", UiRect::new(49, 29, 223, 96)),
            ("DaggerfallTransportWindow", UiRect::new(95, 47, 130, 60)),
        ];
        for (id, rect) in golden {
            let spec = specs()
                .iter()
                .find(|spec| spec.id == id)
                .unwrap_or_else(|| panic!("{id} missing"));
            assert_eq!(spec.rect, rect, "{id}");
        }
    }

    /// Trap honoured, not ignored: `SPOP.RCI` is multi-record and needs
    /// `decode_rci(record, 22, 22)` per record — this lane declares **zero**
    /// such rows, so no whole-file decode of it can ever happen here.
    #[test]
    fn lane_declares_no_spop_rci() {
        for spec in specs() {
            let asset = spec.asset_retail.expect("concrete window");
            assert_ne!(asset, "SPOP.RCI", "{}: SPOP.RCI needs decode_rci per record", spec.id);
            assert!(
                !asset.ends_with(".RCI"),
                "{}: {asset} is a multi-record container; the raw frames of this lane sit \
                 behind .IMG names and decode through decode_img_file",
                spec.id
            );
        }
    }

    /// The single concatenation point must expose each of these 11 ids exactly
    /// once (two slots claiming the same window would break the 51 census).
    #[test]
    fn each_window_reaches_the_concatenation_point_once() {
        let all: Vec<&str> = window_specs().map(|spec| spec.id).collect();
        for spec in specs() {
            let hits = all.iter().filter(|id| **id == spec.id).count();
            assert_eq!(hits, 1, "{} appears {hits}× through window_specs()", spec.id);
        }
    }

    /// Read-only runtime check over the user's ARENA2 (repo policy: skip with
    /// an explicit message when the runtime is absent; 0 bytes in git).
    ///
    /// Every declared rect must equal the record's own `W×H` — that is the
    /// probe's rule for `RCI` (`UiAnchor::Declared`) and the header's rule for
    /// `ImgFile`, so it also proves the `Header`/`Declared` split of the TSV.
    #[test]
    fn every_declared_asset_decodes_from_arena2() {
        let Some(dir) = arena2_dir() else {
            eprintln!("SKIP every_declared_asset_decodes_from_arena2: ARENA2 not found");
            return;
        };
        for spec in specs() {
            let asset = spec.asset_retail.expect("concrete window");
            let path = dir.join(asset);
            let Ok(bytes) = std::fs::read(&path) else {
                eprintln!("SKIP {asset}: not readable at {path:?}");
                continue;
            };
            let image = decode_img_file(&bytes)
                .unwrap_or_else(|err| panic!("{}: {asset} does not decode: {err}", spec.id));
            assert_eq!(
                (image.width, image.height),
                (spec.rect.width, spec.rect.height),
                "{}: {asset} decodes to {}×{} but the TSV rect is {}×{}",
                spec.id,
                image.width,
                image.height,
                spec.rect.width,
                spec.rect.height
            );
        }
    }

    /// `$DF_ARENA2`, `$DF_APP_ROOT/ARENA2` or `$HOME/Games/Daggerfall/app/ARENA2`.
    fn arena2_dir() -> Option<PathBuf> {
        if let Ok(dir) = std::env::var("DF_ARENA2") {
            return Some(PathBuf::from(dir));
        }
        if let Ok(root) = std::env::var("DF_APP_ROOT") {
            let dir = PathBuf::from(root).join("ARENA2");
            if dir.is_dir() {
                return Some(dir);
            }
        }
        let dir = std::env::var("HOME")
            .ok()
            .map(|home| PathBuf::from(home).join("Games/Daggerfall/app/ARENA2"))?;
        dir.is_dir().then_some(dir)
    }
}

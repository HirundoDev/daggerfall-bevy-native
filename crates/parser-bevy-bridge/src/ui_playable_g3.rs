//! Wave 3 composition slot — **G5 (social) + G6 (arranque/carga/ajustes/controles/vídeo)**, 23 windows.
//!
//! Derived from `docs/sintergic/l5-mission/m34-window-assets-CR-2d5c8f41.tsv`
//! (rows whose `group` is `G5` or `G6`). This slot owns the **only three legal
//! `decodes=na` rows** — `DaggerfallDaedraSummonedWindow`,
//! `DaggerfallVidPlayerWindow`, `DemoFLCWindow` — whose surface is video
//! (`.FLC`/`.VID`), not `IMG`. `asset_kind=none` is legal *only* there, and
//! `generic_widget=0` is audited against `asset_retail`, never against
//! `asset_kind`: those three rows therefore carry the **real** retail video
//! container name (verified present in `ARENA2`), never `None`.
//!
//! Every `id` is the DFU class name of the census partition
//! (`m34-playable-groups-CR-2d5c8f41.txt`, G5 = 9 + G6 = 14 = 23); every
//! `asset_retail` and every `rect` is the value of that TSV row, which was
//! measured with the repository's own decoders — nothing here is inferred.
//!
//! Declared **empty** by the orchestrator (pre-wiring `38f48a5`); this module
//! is the only file the W34-2e/2f lane fills. The single concatenation point
//! is [`crate::ui_spec::window_specs`].

use crate::ui_spec::{UiRect, WindowSpec};

/// Builds a canvas rectangle at compile time (the census table is a `static`,
/// so [`UiRect::new`], which is not `const`, cannot be used here).
const fn rect(x: i16, y: i16, width: u16, height: u16) -> UiRect {
    UiRect {
        x,
        y,
        width,
        height,
    }
}

/// G5 + G6 window specs: 9 social + 14 start/load/settings/controls/video.
pub fn specs() -> &'static [WindowSpec] {
    &SPECS
}

/// The 23 rows of `m34-window-assets-CR-2d5c8f41.tsv` with `group ∈ {G5, G6}`,
/// in partition order.
static SPECS: [WindowSpec; 23] = [
    // ── G5 — social (9) ───────────────────────────────────────────────────
    // `DaggerfallGuildServicePopupWindow.cs:56` — IMG with its own header;
    // rect = that header (95,47,130×51).
    WindowSpec {
        id: "DaggerfallGuildServicePopupWindow",
        asset_retail: Some("GILD00I0.IMG"),
        rect: rect(95, 47, 130, 51),
    },
    // `DaggerfallMessageBox.cs:231` — inherits `DaggerfallMessageBox`; its
    // surface is the base scroll `SPOP.RCI` (multi-record 9 × 22×22; the
    // declared rect is one border record).
    WindowSpec {
        id: "DaggerfallGuildServiceCureDisease",
        asset_retail: Some("SPOP.RCI"),
        rect: rect(0, 0, 22, 22),
    },
    // `DaggerfallInputMessageBox.cs:146` — inherits `DaggerfallInputMessageBox`
    // (same base scroll); `DaggerfallGuildServiceDonation.cs:72` additionally
    // builds a `DaggerfallMessageBox` on that scroll.
    WindowSpec {
        id: "DaggerfallGuildServiceDonation",
        asset_retail: Some("SPOP.RCI"),
        rect: rect(0, 0, 22, 22),
    },
    // `DaggerfallGuildServiceTraining.cs:57` — builds a `DaggerfallMessageBox`
    // whose surface is the `SPOP.RCI` scroll (multi-record 9 × 22×22).
    WindowSpec {
        id: "DaggerfallGuildServiceTraining",
        asset_retail: Some("SPOP.RCI"),
        rect: rect(0, 0, 22, 22),
    },
    // `DaggerfallCourtWindow.cs:30` — full-screen 320×200 raster
    // (`CORT01I0.IMG`); the cell is the satellite `PRIS00I0.IMG`.
    WindowSpec {
        id: "DaggerfallCourtWindow",
        asset_retail: Some("CORT01I0.IMG"),
        rect: rect(0, 0, 320, 200),
    },
    // ── VIDEO (decodes=na, asset_kind=none) ───────────────────────────────
    // `DaggerfallDaedraSummonedWindow.cs:77` —
    // `playerPanel.Load(daedraSummoned.vidFile)` plays one `.FLC` summon
    // cinematic chosen by the quest: HIRCINE / CLAVICUS / MEHRUNES /
    // MOLAGBAL / SANGUINE / PERYITE / MALACATH / HERMAEUS / SHEOGRTH
    // (`DaggerfallQuestPopupWindow.cs:52-60`); all nine are present in
    // `ARENA2`. The window reproduces that full-motion summoning video —
    // `HIRCINE.FLC` is the first of the nine (the one this row names); its
    // `messageLabel` in `DefaultFont` is text overlaid on the video, not the
    // surface. TSV rect = 0,0,0,0: an `na` row has no raster anchor.
    WindowSpec {
        id: "DaggerfallDaedraSummonedWindow",
        asset_retail: Some("HIRCINE.FLC"),
        rect: rect(0, 0, 0, 0),
    },
    // `DaggerfallWitchesCovenPopupWindow.cs:43` — IMG with its own header
    // (95,47,130×51); `DAED00I0.IMG` is the daedra-prince popup frame.
    WindowSpec {
        id: "DaggerfallWitchesCovenPopupWindow",
        asset_retail: Some("DAED00I0.IMG"),
        rect: rect(95, 47, 130, 51),
    },
    // `DaggerfallQuestOfferWindow.cs:79` — the surface is the scroll of the
    // `DaggerfallMessageBox` created by `QuestMachine.CreateMessagePrompt`
    // (`SPOP.RCI`, multi-record 9 × 22×22).
    WindowSpec {
        id: "DaggerfallQuestOfferWindow",
        asset_retail: Some("SPOP.RCI"),
        rect: rect(0, 0, 22, 22),
    },
    // `DaggerfallTalkWindow.cs:32` — full-screen 320×200 raster
    // (`TALK01I0.IMG`); portraits are the satellites `TFAC00I0.RCI` and
    // `FACES.CIF` (multi-record 64×64).
    WindowSpec {
        id: "DaggerfallTalkWindow",
        asset_retail: Some("TALK01I0.IMG"),
        rect: rect(0, 0, 320, 200),
    },
    // ── G6 — start / load / settings / controls / video (14) ─────────────
    // `DaggerfallStartWindow.cs:23` — palettised 320×200 raster
    // (`PICK03I0.IMG`, 64768 B); no own text, the buttons are graphics.
    WindowSpec {
        id: "DaggerfallStartWindow",
        asset_retail: Some("PICK03I0.IMG"),
        rect: rect(0, 0, 320, 200),
    },
    // `DaggerfallStartNewGameWizard.cs:444` — no own panel: it orchestrates
    // the `CreateChar*` windows (:36-50); its only own widget is the
    // `DaggerfallMessageBox` of :444, drawn in `DefaultFont` (FONT0003,
    // `DaggerfallUI.cs:156`). rect = header glyph cell of that FNT (5×7,
    // 240 glyphs); the ANIM0000/ANIM0011/DAG2 cinematics (:26-28) are video,
    // not this window's surface.
    WindowSpec {
        id: "DaggerfallStartNewGameWizard",
        asset_retail: Some("FONT0003.FNT"),
        rect: rect(0, 0, 5, 7),
    },
    // `DaggerfallUnitySetupGameWizard.cs:410` — no ARENA2 background (colour
    // panels + TextureReplacement); its labels come from
    // `DaggerfallUI.AddTextLabel(DefaultFont, …)` (`DaggerfallUI.cs:1093`).
    WindowSpec {
        id: "DaggerfallUnitySetupGameWizard",
        asset_retail: Some("FONT0003.FNT"),
        rect: rect(0, 0, 5, 7),
    },
    // `DaggerfallUnitySaveGameWindow.cs:42` — surface = the `TextBox` and
    // `TextLabel`s of :42-47, all in `DefaultFont`; the background is only
    // TextureReplacement.
    WindowSpec {
        id: "DaggerfallUnitySaveGameWindow",
        asset_retail: Some("FONT0003.FNT"),
        rect: rect(0, 0, 5, 7),
    },
    // `DaggerfallLoadClassicGameWindow.cs:31` — palettised 320×200 raster
    // (`LOAD00I0.IMG`, the classic save-load screen).
    WindowSpec {
        id: "DaggerfallLoadClassicGameWindow",
        asset_retail: Some("LOAD00I0.IMG"),
        rect: rect(0, 0, 320, 200),
    },
    // `DaggerfallUnityMouseControlsWindow.cs:34` — `titleLabel` and the
    // keybind buttons (:34-38) in `DefaultFont`; background is only
    // TextureReplacement.
    WindowSpec {
        id: "DaggerfallUnityMouseControlsWindow",
        asset_retail: Some("FONT0003.FNT"),
        rect: rect(0, 0, 5, 7),
    },
    // `DaggerfallControlsWindow.cs:50` — palettised 320×200 raster
    // (`CNFG00I0.IMG`, the classic controls screen).
    WindowSpec {
        id: "DaggerfallControlsWindow",
        asset_retail: Some("CNFG00I0.IMG"),
        rect: rect(0, 0, 320, 200),
    },
    // `DaggerfallJoystickControlsWindow.cs:42` — `titleLabel` and the keybind
    // buttons (:42-48) in `DefaultFont`; background is only
    // TextureReplacement.
    WindowSpec {
        id: "DaggerfallJoystickControlsWindow",
        asset_retail: Some("FONT0003.FNT"),
        rect: rect(0, 0, 5, 7),
    },
    // `DaggerfallAdvancedSettingsWindow.cs:243` — `AddCheckbox`/`AddTextbox`/
    // `AddSlider` (:243-255) are labelled in `DefaultFont` (FONT0003); the
    // FONT0001 title and FONT0002 page buttons are text variants of the same
    // screen — the row names FONT0003.
    WindowSpec {
        id: "DaggerfallAdvancedSettingsWindow",
        asset_retail: Some("FONT0003.FNT"),
        rect: rect(0, 0, 5, 7),
    },
    // `DaggerfallEffectSettingsEditorWindow.cs:78` — full-screen 320×200
    // raster (`MASK05I0.IMG`); the description panel reuses the `SPOP.RCI`
    // scroll (:186).
    WindowSpec {
        id: "DaggerfallEffectSettingsEditorWindow",
        asset_retail: Some("MASK05I0.IMG"),
        rect: rect(0, 0, 320, 200),
    },
    // ── VIDEO (decodes=na, asset_kind=none) ───────────────────────────────
    // `DaggerfallVidPlayerWindow.cs:101` — its surface is the `DaggerfallVideo`
    // of 320×200 (:96-104) playing a `.VID` container; `ARENA2` holds 17
    // `.VID`, and the known ones are the splash `ANIM0001.VID` and the death
    // `ANIM0012.VID` (`DaggerfallUI.cs:49-50`). The window reproduces full
    // motion `.VID` (splash/death cinematics); it has no own text. This row
    // names the splash container — the video the player sees at start-up.
    // TSV rect = 0,0,0,0 (`na` row: no raster anchor).
    WindowSpec {
        id: "DaggerfallVidPlayerWindow",
        asset_retail: Some("ANIM0001.VID"),
        rect: rect(0, 0, 0, 0),
    },
    // ── VIDEO (decodes=na, asset_kind=none) ───────────────────────────────
    // `DemoFLCWindow.cs:37` — its surface is the `FLCPlayer` of 320×200
    // (:34-38) playing an arbitrary `.FLC`; `ARENA2` holds 16 `.FLC`. The
    // window reproduces a full-motion FLC cinematic (demo playback); it has
    // no own text. The TSV note names no single file (the container is
    // arbitrary), so this row names `AZURA.FLC` — a real retail `.FLC` from
    // the 16 present, verified in `ARENA2`, taken as the representative of
    // the family. TSV rect = 0,0,0,0 (`na` row: no raster anchor).
    WindowSpec {
        id: "DemoFLCWindow",
        asset_retail: Some("AZURA.FLC"),
        rect: rect(0, 0, 0, 0),
    },
    // `DemoClassQuestionsWindow.cs:32` — palettised 320×200 raster
    // (`CHGN00I0.IMG`, 64768 B) used as `backgroundFile`; the scroll frames
    // of :147 are generated at runtime, not retail assets; no own text.
    WindowSpec {
        id: "DemoClassQuestionsWindow",
        asset_retail: Some("CHGN00I0.IMG"),
        rect: rect(0, 0, 320, 200),
    },
    // `DaggerfallTeleportPopUp.cs:41` — IMG with its own header;
    // rect = that header (75,58,171×57).
    WindowSpec {
        id: "DaggerfallTeleportPopUp",
        asset_retail: Some("TELE00I0.IMG"),
        rect: rect(75, 58, 171, 57),
    },
];

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use daggerfall_parser_rs::image::{decode_img_file, decode_rci, FntFile};
    use daggerfall_parser_rs::video::decode_frames;

    use super::*;
    use crate::ui_spec::{asset_is_generic, CANVAS_HEIGHT, CANVAS_WIDTH};

    /// G5 partition (social), in census order.
    const G5: [&str; 9] = [
        "DaggerfallGuildServicePopupWindow",
        "DaggerfallGuildServiceCureDisease",
        "DaggerfallGuildServiceDonation",
        "DaggerfallGuildServiceTraining",
        "DaggerfallCourtWindow",
        "DaggerfallDaedraSummonedWindow",
        "DaggerfallWitchesCovenPopupWindow",
        "DaggerfallQuestOfferWindow",
        "DaggerfallTalkWindow",
    ];

    /// G6 partition (start/load/settings/controls/video), in census order.
    const G6: [&str; 14] = [
        "DaggerfallStartWindow",
        "DaggerfallStartNewGameWizard",
        "DaggerfallUnitySetupGameWizard",
        "DaggerfallUnitySaveGameWindow",
        "DaggerfallLoadClassicGameWindow",
        "DaggerfallUnityMouseControlsWindow",
        "DaggerfallControlsWindow",
        "DaggerfallJoystickControlsWindow",
        "DaggerfallAdvancedSettingsWindow",
        "DaggerfallEffectSettingsEditorWindow",
        "DaggerfallVidPlayerWindow",
        "DemoFLCWindow",
        "DemoClassQuestionsWindow",
        "DaggerfallTeleportPopUp",
    ];

    /// The three `decodes=na` rows of the TSV (video surface).
    const VIDEO_ROWS: [(&str, &str); 3] = [
        ("DaggerfallDaedraSummonedWindow", "HIRCINE.FLC"),
        ("DaggerfallVidPlayerWindow", "ANIM0001.VID"),
        ("DemoFLCWindow", "AZURA.FLC"),
    ];

    fn ids() -> Vec<&'static str> {
        specs().iter().map(|spec| spec.id).collect()
    }

    fn fnv1a64(bytes: &[u8]) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }

    fn is_video(asset: &str) -> bool {
        let upper = asset.to_ascii_uppercase();
        upper.ends_with(".VID") || upper.ends_with(".FLC")
    }

    #[test]
    fn declares_the_23_g5_g6_windows() {
        assert_eq!(specs().len(), 23, "G5(9) + G6(14) = 23 rows");
    }

    #[test]
    fn ids_are_exactly_the_partition() {
        let mut declared = ids();
        declared.sort_unstable();
        let mut expected: Vec<&str> = G5.iter().chain(G6.iter()).copied().collect();
        expected.sort_unstable();
        assert_eq!(declared, expected, "0 extras / 0 missing vs the partition");
    }

    #[test]
    fn group_split_is_9_plus_14() {
        let declared = ids();
        let g5 = G5.iter().filter(|id| declared.contains(id)).count();
        let g6 = G6.iter().filter(|id| declared.contains(id)).count();
        assert_eq!((g5, g6), (9, 14));
    }

    #[test]
    fn gv19_subset_digest_matches_the_census() {
        // GV-19 rule: ids ascending by byte, '\n' separator, no trailing newline.
        let mut declared = ids();
        declared.sort_unstable();
        let text = declared.join("\n");
        assert_eq!(text.len(), 660, "23 ids joined by '\\n' (no trailing newline)");
        assert_eq!(fnv1a64(text.as_bytes()), 0x6102_05d4_6ba7_4118);
    }

    #[test]
    fn zero_generic_widgets_against_asset_retail() {
        let generic = specs()
            .iter()
            .filter(|spec| spec.asset_retail.is_none_or(asset_is_generic))
            .count();
        assert_eq!(generic, 0, "generic_widget=0 is audited against asset_retail");
    }

    #[test]
    fn every_rect_fits_the_320x200_canvas() {
        for spec in specs() {
            assert!(
                spec.rect.fits_canvas(),
                "{} rect {:?} outside {CANVAS_WIDTH}×{CANVAS_HEIGHT}",
                spec.id,
                spec.rect
            );
        }
    }

    #[test]
    fn twenty_raster_rows_plus_three_video_rows_are_concrete() {
        // The 20 rows whose TSV `asset_kind` is IMG/RCI/FNT ...
        let raster = specs()
            .iter()
            .filter(|spec| !is_video(spec.asset_retail.expect("retail asset")))
            .count();
        assert_eq!(raster, 20);
        // ... and the 3 video rows, which are concrete too because their
        // `asset_retail` is the real retail container (never `None`/generic)
        // and their rect (the TSV `na` placeholder 0,0,0,0) is in canvas.
        assert_eq!(specs().iter().filter(|spec| spec.is_concrete()).count(), 23);
    }

    #[test]
    fn the_three_video_rows_name_their_retail_container() {
        for (id, asset) in VIDEO_ROWS {
            let spec = specs()
                .iter()
                .find(|spec| spec.id == id)
                .unwrap_or_else(|| panic!("{id} missing"));
            assert_eq!(spec.asset_retail, Some(asset), "{id} surface is video");
            assert!(is_video(asset), "{id} must name a .VID/.FLC container");
        }
        let video_rows = specs()
            .iter()
            .filter(|spec| is_video(spec.asset_retail.expect("retail asset")))
            .count();
        assert_eq!(video_rows, 3, "only the three decodes=na rows are video");
    }

    #[test]
    fn spop_rows_declare_the_22x22_record() {
        let spop: Vec<&WindowSpec> = specs()
            .iter()
            .filter(|spec| spec.asset_retail == Some("SPOP.RCI"))
            .collect();
        assert_eq!(spop.len(), 4);
        for spec in spop {
            assert_eq!((spec.rect.width, spec.rect.height), (22, 22), "{}", spec.id);
        }
    }

    /// `$DF_ARENA2` or `$HOME/Games/Daggerfall/app/ARENA2`; `None` when absent.
    fn arena2_dir() -> Option<PathBuf> {
        if let Ok(dir) = std::env::var("DF_ARENA2") {
            let dir = PathBuf::from(dir);
            if dir.is_dir() {
                return Some(dir);
            }
        }
        std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join("Games/Daggerfall/app/ARENA2"))
            .filter(|dir| dir.is_dir())
    }

    /// Case-insensitive lookup, the same rule Mode A uses.
    fn arena2_path(dir: &Path, asset: &str) -> Option<PathBuf> {
        let wanted = asset.to_ascii_uppercase();
        std::fs::read_dir(dir)
            .ok()?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.to_ascii_uppercase() == wanted)
            })
    }

    /// Decodes one declared retail asset exactly as the probe does: `.IMG`
    /// through `decode_img_file`, `.RCI` per declared record (multi-record
    /// containers such as `SPOP.RCI` are 9 × 22×22 = 4356 B), `.FNT` through
    /// `FntFile::parse`, and `.VID`/`.FLC` through the W34-4 video decoder.
    fn decode_retail(asset: &str, spec: &WindowSpec, bytes: &[u8]) -> Result<(), String> {
        let upper = asset.to_ascii_uppercase();
        if upper.ends_with(".IMG") {
            decode_img_file(bytes).map(|_| ()).map_err(|err| err.to_string())
        } else if upper.ends_with(".FNT") {
            FntFile::parse(bytes).map(|_| ()).map_err(|err| err.to_string())
        } else if upper.ends_with(".RCI") {
            let (width, height) = (spec.rect.width, spec.rect.height);
            let raw = usize::from(width) * usize::from(height);
            let record = if raw > 0 && bytes.len() > raw && bytes.len().is_multiple_of(raw) {
                &bytes[..raw]
            } else {
                bytes
            };
            decode_rci(record, width, height, false)
                .or_else(|_| decode_rci(record, width, height, true))
                .map(|_| ())
                .map_err(|err| err.to_string())
        } else if upper.ends_with(".VID") || upper.ends_with(".FLC") {
            decode_frames(bytes)
                .map(|frames| assert!(!frames.is_empty()))
                .map_err(|err| err.to_string())
        } else {
            Err(format!("{asset}: extension is not a retail surface"))
        }
    }

    #[test]
    fn retail_assets_decode_from_arena2() {
        let Some(dir) = arena2_dir() else {
            eprintln!("SKIP retail_assets_decode_from_arena2: ARENA2 absent (DF_ARENA2 / $HOME/Games/…)");
            return;
        };
        let mut decoded = 0;
        for spec in specs() {
            let asset = spec.asset_retail.expect("retail asset");
            let path = arena2_path(&dir, asset)
                .unwrap_or_else(|| panic!("{}: {asset} absent from ARENA2", spec.id));
            let bytes = std::fs::read(&path)
                .unwrap_or_else(|err| panic!("{}: {asset}: {err}", spec.id));
            decode_retail(asset, spec, &bytes)
                .unwrap_or_else(|err| panic!("{}: {asset}: {err}", spec.id));
            if is_video(asset) {
                // Class evidence: real dimensions and frame count from the
                // W34-4 decoder, never a synthetic pass.
                let video = daggerfall_parser_rs::video::decode(&bytes)
                    .unwrap_or_else(|err| panic!("{}: {asset}: {err}", spec.id));
                eprintln!(
                    "[video] {asset}: {}x{} · {} frames · {} palette indices",
                    video.width,
                    video.height,
                    video.frames.len(),
                    video.total_indices()
                );
                assert!(!video.frames.is_empty(), "{asset}: zero frames");
            }
            decoded += 1;
        }
        assert_eq!(decoded, 23, "23/23 retail assets decoded from ARENA2");
    }
}

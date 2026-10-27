//! `PROBE-UI-FULL-001` — 85-window census + shell navigation + HUD census
//! (spec M34 §4, GV-01/GV-02/GV-03/GV-19; W34-1, `CR-2d5c8f41`).
//!
//! ## Block A — census + shell + HUD (W34-1)
//!
//! - prints the **85 census rows** one per line — name, category, and the
//!   explicit `N/A-engine` tag on the 11 config + 7 infra rows (§9.5: they
//!   must appear; omitting them would be a census FAIL, not an exclusion);
//! - builds the canonical [`UiShell`] over [`WindowRegistry::canonical`] (85
//!   entries) and opens/closes **each** window, asserting the stack returns to
//!   depth 0 (criterion (v));
//! - measures the HUD composition against GV-03 (15 instances, 14 registered,
//!   11 flags, 14 types; digest of the 15 field names);
//! - recomputes the three digests **from data** and compares them with the §8
//!   literals. The whole measurement runs **twice** over independent
//!   registries; the two combined digests must agree.
//!
//! GV-02's literal `1da6a516914e33db` **is** reproduced by §3.1's rule
//! (ascending byte order, `\n`, no trailing newline: 2126 B) and GV-19's
//! `c85e0bb5ae867373` too, so the probe asserts the literals. If a future
//! change breaks them, the probe prints the **measured** value and a `[WARN]`
//! naming the deviation — it never rewrites the literal to fit.
//!
//! ## Block B — the 51 playable windows (W34-2a..2f, composed)
//!
//! `playable` reads [`window_specs`] — the 51 `WindowSpec` rows composed in the
//! `ui_playable_g*` slots (17 + 11 + 23, `window_group_counts()[2]`) — binds
//! every `id` to its census row, and decodes each declared retail asset from
//! `ARENA2`. It reports **two honest numbers**: the image class `{decoded}/51`
//! (`IMG`/`CIF`/`FNT`/`RCI`, §4(ii) literal) and the video class
//! `{video}/{video}` (`VID`/`.FLC`, decoded with the W34-4 `decode_frames`).
//! Measured state: **`48/51` image + `3/3` video = `51/51`**, so the block
//! passes. The three `decodes=na` rows are video surfaces: they are decoded and
//! counted as video, so `48` is never inflated to `51` and nothing is silently
//! excluded.
//!
//! `ui_windows_g1` (16 `CreateChar*`) and `ui_windows_g2` (11 config + 7 infra,
//! `N/A-engine`) contribute no specs **by design**: §4(i) binds `WindowSpec` to
//! the 51 playables only, so the census group counts print
//! `grupos=[g1=0 g2=0 g3=51]` — the spec boundary, not pending composition.
//!
//! ## Exit semantics (§4, literal)
//!
//! - `EXIT=0` ⟺ census exactly 85 = 16+11+7+51 ∧ 85/85 navigable ∧ HUD GV-03
//!   ∧ `playable 51/51` (= 48 image-class + 3 video) ∧ three digests equal
//!   their literals ∧ both passes identical.
//! - `EXIT=1` on any failed assertion.
//! - `EXIT=2` when `ARENA2` is not accessible — the probe measures nothing it
//!   cannot read.

use std::path::Path;

use daggerfall_parser_bevy_bridge::ui_spec::{
    asset_is_generic, window_group_counts, window_specs, WindowSpec,
};
use daggerfall_parser_rs::image::{decode_cif, decode_img_file, decode_rci, FntFile};
use daggerfall_parser_rs::video::decode_frames;
use daggerfall_perception::ui_census::{
    self, WindowCategory, CENSUS_BYTES, CENSUS_DIGEST, CENSUS_SIZE, HUD_DIGEST, HUD_FLAGS,
    HUD_INSTANCES, HUD_REGISTERED, HUD_TYPES, PLAYABLE_BYTES, PLAYABLE_DIGEST,
};
use daggerfall_perception::ui_shell::{canonical_id_for, UiShell};

/// One independent measurement pass (`a` and `b` must agree).
#[derive(Debug, Clone, PartialEq, Eq)]
struct CensusRun {
    /// Windows the canonical registry holds.
    registered: usize,
    /// Windows opened and closed with the stack back at depth 0.
    navigable: usize,
    /// Open windows left behind (must be 0).
    leaked: usize,
    /// Playable specs that decoded their retail image asset (IMG/CIF/FNT/RCI).
    decoded: usize,
    /// Playable specs of the **video class** (`.VID`/`.FLC`) that decoded.
    video_decoded: usize,
    /// Playable specs that declare a video surface (`.VID`/`.FLC`).
    video_expected: usize,
    /// Playable specs that failed to decode (asset missing/undecodable).
    missing_assets: usize,
    /// Specs whose `asset_retail` is empty/placeholder (§4(iv)).
    generic_widgets: usize,
    /// Specs whose `rect` falls outside 320×200 (§4(ii)).
    out_of_canvas: usize,
    /// Spec ids that are not a **playable** census row.
    foreign_ids: usize,
    /// Census text digest (GV-02).
    windows_digest: u64,
    /// Playable text digest (GV-19).
    playable_digest: u64,
    /// HUD field-name digest (GV-03).
    hud_digest: u64,
}

impl CensusRun {
    /// Digest of the three digests, in fixed order (the probe's own pair).
    fn combined(&self) -> u64 {
        let text = format!(
            "{:016x}\n{:016x}\n{:016x}",
            self.windows_digest, self.playable_digest, self.hud_digest
        );
        ui_census::fnv1a64(text.as_bytes())
    }
}

/// Runs [`PROBE-UI-FULL-001`].
///
/// Returns the process exit code (0 pass · 1 assertion failed · 2 no `ARENA2`).
pub fn run_ui_full_001() -> i32 {
    println!("PROBE-UI-FULL-001 - 85 ventanas / 55 widgets (REQ-L99-067, M34)");

    let dir = match crate::mode_a_arena2_dir() {
        Ok(dir) => dir,
        Err(reason) => {
            println!("ARENA2 not accessible: {reason}");
            println!("reason=missing_arena2");
            println!("EXIT=2");
            println!("VEREDICTO: PROBE-UI-FULL-001 NO DATA - ARENA2 no accesible");
            return 2;
        }
    };
    println!("[info] arena2={}", dir.display());

    // ── census rows: 85 names, one per line, `N/A-engine` explicit ─────────
    for (index, row) in ui_census::census().iter().enumerate() {
        let tag = if row.category.is_na_engine() {
            "N/A-engine"
        } else {
            "1996-ui"
        };
        println!(
            "[{:03}] {:<40} {:<10} {}",
            index + 1,
            row.name,
            row.category.as_str(),
            tag
        );
    }

    let counts = [
        ui_census::count(WindowCategory::CreateChar),
        ui_census::count(WindowCategory::Config),
        ui_census::count(WindowCategory::Infra),
        ui_census::count(WindowCategory::Playable),
    ];
    let census_ok = ui_census::census().len() == CENSUS_SIZE
        && counts == ui_census::CENSUS_COUNTS
        && ui_census::is_ascending();
    if census_ok {
        println!(
            "[OK] census: {} = {} createchar + {} config + {} infra + {} jugables",
            ui_census::census().len(),
            counts[0],
            counts[1],
            counts[2],
            counts[3]
        );
    } else {
        println!(
            "[FAIL] census: medida {} = {} createchar + {} config + {} infra + {} jugables (GV-01 espera {} = 16+11+7+51 · ascendente={})",
            ui_census::census().len(),
            counts[0],
            counts[1],
            counts[2],
            counts[3],
            CENSUS_SIZE,
            ui_census::is_ascending()
        );
    }

    // ── two independent passes over the same frozen data ──────────────────
    let first = measure(&dir);
    let second = measure(&dir);
    let pair_identical = first == second;

    let shell_ok = first.registered == CENSUS_SIZE && first.navigable == CENSUS_SIZE && first.leaked == 0;
    if shell_ok {
        println!(
            "[OK] shell: {}/{} registradas · {}/{} abiertas y cerradas · depth={}",
            first.registered, CENSUS_SIZE, first.navigable, CENSUS_SIZE, first.leaked
        );
    } else {
        println!(
            "[FAIL] shell: {}/{} registradas · {}/{} abiertas y cerradas · depth={}",
            first.registered, CENSUS_SIZE, first.navigable, CENSUS_SIZE, first.leaked
        );
    }

    // ── block B: the 51 playable windows (composed in ui_playable_g*) ──────
    let playable_total = ui_census::count(WindowCategory::Playable);
    // Two honest numbers, never one inflated total: the image class
    // (IMG/CIF/FNT/RCI, §4(ii) literal) and the video class (`.VID`/`.FLC`,
    // decoded by `decode_frames`, W34-4).
    let playable_ok = first.decoded + first.video_decoded == playable_total
        && first.video_expected == first.video_decoded
        && first.missing_assets == 0
        && first.generic_widgets == 0
        && first.out_of_canvas == 0
        && first.foreign_ids == 0;
    println!(
        "[{}] playable: {}/{} con asset retail IMG/CIF/FNT/RCI decodificado · video: {}/{} decodificado (decode_frames) · generic_widgets={} · assets_missing={} · fuera_de_lienzo={} · ids_fuera_del_censo={} · grupos=[g1={} g2={} g3={}]",
        if playable_ok { "OK" } else { "FAIL" },
        first.decoded,
        playable_total,
        first.video_decoded,
        first.video_expected,
        first.generic_widgets,
        first.missing_assets,
        first.out_of_canvas,
        first.foreign_ids,
        window_group_counts()[0],
        window_group_counts()[1],
        window_group_counts()[2]
    );

    // ── HUD census (GV-03) ────────────────────────────────────────────────
    let hud_ok = ui_census::HUD_FIELDS.len() == HUD_INSTANCES
        && ui_census::hud_registered() == HUD_REGISTERED
        && ui_census::HUD_VISIBILITY_FLAGS.len() == HUD_FLAGS
        && ui_census::hud_types() == HUD_TYPES
        && first.hud_digest == HUD_DIGEST;
    println!(
        "[{}] hud: {} instancias / {} registradas / {} flags · types={}",
        if hud_ok { "OK" } else { "FAIL" },
        ui_census::HUD_FIELDS.len(),
        ui_census::hud_registered(),
        ui_census::HUD_VISIBILITY_FLAGS.len(),
        ui_census::hud_types()
    );

    // ── digests against the §8 literals (measured first, verdict after) ────
    let digests_ok = first.windows_digest == CENSUS_DIGEST
        && first.playable_digest == PLAYABLE_DIGEST
        && first.hud_digest == HUD_DIGEST;
    println!(
        "[{}] windows_digest={:016x} playable_digest={:016x} hud_digest={:016x}",
        if digests_ok { "OK" } else { "FAIL" },
        first.windows_digest,
        first.playable_digest,
        first.hud_digest
    );
    if first.windows_digest == CENSUS_DIGEST {
        println!(
            "[OK] GV-02 literal {:016x} reproducido ({} B, regla §3.1: ascendente por byte, \\n, sin salto final)",
            CENSUS_DIGEST, CENSUS_BYTES
        );
    } else {
        println!(
            "[WARN] GV-02 literal {:016x} no reproducible; medido {:016x} ({} B de {} esperados)",
            CENSUS_DIGEST,
            first.windows_digest,
            ui_census::census_text().len(),
            CENSUS_BYTES
        );
    }
    if first.playable_digest == PLAYABLE_DIGEST {
        println!(
            "[OK] GV-19 literal {:016x} reproducido ({} B)",
            PLAYABLE_DIGEST, PLAYABLE_BYTES
        );
    } else {
        println!(
            "[WARN] GV-19 literal {:016x} no reproducible; medido {:016x} ({} B de {} esperados)",
            PLAYABLE_DIGEST,
            first.playable_digest,
            ui_census::playable_text().len(),
            PLAYABLE_BYTES
        );
    }

    println!(
        "[{}] pasada 2 independiente: windows={:016x} playable={:016x} hud={:016x} · idéntica={}",
        if pair_identical { "OK" } else { "FAIL" },
        second.windows_digest,
        second.playable_digest,
        second.hud_digest,
        pair_identical
    );

    let pass = census_ok && shell_ok && playable_ok && hud_ok && digests_ok && pair_identical;
    let mut digest = first.combined();
    if !pair_identical {
        // Never print "×2 idéntico" over two different values: fold both.
        digest = ui_census::fnv1a64(
            format!("{:016x}{:016x}", first.combined(), second.combined()).as_bytes(),
        );
    }
    println!(
        "VEREDICTO: PROBE-UI-FULL-001 {} · digest={digest:016x} ×2{}",
        if pass { "PASS" } else { "FAIL" },
        if pair_identical { " idéntico" } else { " NO idéntico" }
    );
    let code = i32::from(!pass);
    println!("EXIT={code}");
    code
}

/// One full pass: census digests, the canonical shell navigation and the
/// playable block, all recomputed from data.
fn measure(dir: &Path) -> CensusRun {
    let windows_digest = ui_census::census_digest();
    let playable_digest = ui_census::playable_digest();
    let hud_digest = ui_census::hud_digest();

    let (registered, navigable, leaked) = navigate_all();

    let mut decoded = 0_usize;
    let mut video_decoded = 0_usize;
    let mut video_expected = 0_usize;
    let mut missing_assets = 0_usize;
    let mut generic_widgets = 0_usize;
    let mut out_of_canvas = 0_usize;
    let mut foreign_ids = 0_usize;
    for spec in window_specs() {
        let Some(row) = ui_census::census().iter().find(|row| row.name == spec.id) else {
            foreign_ids += 1;
            continue;
        };
        if row.category != WindowCategory::Playable {
            foreign_ids += 1;
            continue;
        }
        if spec.asset_retail.is_none_or(asset_is_generic) {
            generic_widgets += 1;
        }
        if !spec.rect.fits_canvas() {
            out_of_canvas += 1;
        }
        match spec.asset_retail {
            Some(asset) if is_video_asset(asset) => {
                video_expected += 1;
                match decode_video_asset(dir, asset) {
                    Ok(()) => video_decoded += 1,
                    Err(_) => missing_assets += 1,
                }
            }
            Some(asset) => match decode_retail_asset(dir, asset, spec) {
                Ok(()) => decoded += 1,
                Err(_) => missing_assets += 1,
            },
            None => missing_assets += 1,
        }
    }

    CensusRun {
        registered,
        navigable,
        leaked,
        decoded,
        video_decoded,
        video_expected,
        missing_assets,
        generic_widgets,
        out_of_canvas,
        foreign_ids,
        windows_digest,
        playable_digest,
        hud_digest,
    }
}

/// Builds the canonical registry and opens/closes every census window.
///
/// Returns `(registered, navigable, leaked)`; `leaked` counts windows still
/// open at the end (criterion (v): the shell returns to 0 open windows).
fn navigate_all() -> (usize, usize, usize) {
    let mut shell = match UiShell::canonical() {
        Ok(shell) => shell,
        Err(_) => return (0, 0, 0),
    };
    let registered = shell.registry().len();
    let mut navigable = 0;
    for row in ui_census::census() {
        let Some(id) = canonical_id_for(row.name) else {
            continue;
        };
        if shell.open(id).is_ok() {
            shell.close(id);
            if !shell.is_open(id) {
                navigable += 1;
            }
        }
    }
    (registered, navigable, shell.depth())
}

/// Decodes the spec's primary retail image asset from `ARENA2` (§4(ii)).
///
/// The path is resolved with the Mode A case-insensitive open
/// (`daggerfall_app::mode_a::arena2_path`), the same root the probe gated on.
/// `IMG`/`CIF`/`FNT` are self-describing; raw `RCI` is not, so its record is
/// the one the window **declares** through `spec.rect` (see
/// [`rci_declared_record`]). Video surfaces (`.VID`/`.FLC`) are a different
/// class handled by [`decode_video_asset`].
fn decode_retail_asset(dir: &Path, asset: &str, spec: &WindowSpec) -> Result<(), String> {
    let path = daggerfall_app::mode_a::arena2_path(asset)?;
    let bytes = std::fs::read(&path).map_err(|err| format!("{asset}: {err}"))?;
    decode_retail_bytes(dir, asset, &bytes, spec)
}

/// Decodes already-read retail bytes for `asset` (§4(ii)).
///
/// Split from [`decode_retail_asset`] so the declared-record rule and its
/// spec-hole rejection are testable without a retail install: an `Err` here is
/// exactly what [`measure`] counts as `assets_missing`.
fn decode_retail_bytes(dir: &Path, asset: &str, bytes: &[u8], spec: &WindowSpec) -> Result<(), String> {
    let upper = asset.to_ascii_uppercase();
    let decoded = if upper.ends_with(".IMG") {
        decode_img_file(bytes).map(|_| ()).map_err(|err| err.to_string())
    } else if upper.ends_with(".CIF") {
        decode_cif(bytes).map(|_| ()).map_err(|err| err.to_string())
    } else if upper.ends_with(".FNT") {
        FntFile::parse(bytes).map(|_| ()).map_err(|err| err.to_string())
    } else if upper.ends_with(".RCI") {
        // Raw image: the declared rect is the record size (§4(ii)
        // `UiAnchor::Declared`); a multi-record container (`SPOP.RCI` = 9 ×
        // 22×22 = 4356 B) is sliced at that declared cell, never at an
        // inferred one.
        let (surface, records) = rci_declared_record(bytes, spec.rect.width, spec.rect.height)
            .map_err(|err| format!("{asset}: {err}"))?;
        decode_rci(surface, spec.rect.width, spec.rect.height, false)
            .or_else(|_| decode_rci(surface, spec.rect.width, spec.rect.height, true))
            .map(|_| ())
            .map_err(|err| {
                format!(
                    "rect {}×{} · {} record(s): {err}",
                    spec.rect.width, spec.rect.height, records
                )
            })
    } else {
        return Err(format!(
            "{asset}: extensión no retailable (IMG/CIF/FNT/RCI) — dir={}",
            dir.display()
        ));
    };
    decoded.map_err(|err| format!("{asset}: {err}"))
}

/// Declared record of a multi-record raw `RCI` container: the border record,
/// first in file order (`Assets/Scripts/API/CifRciFile.cs` `ReadRci`, ~355-370
/// — records are read sequentially, each `width*height` palette indices long;
/// the container carries no per-record size field). The *cell* is declared by
/// the window's `spec.rect`; this constant only names the record inside it.
const RCI_DECLARED_RECORD_INDEX: usize = 0;

/// Extracts the record of `bytes` whose size equals the **declared** rect.
///
/// Raw `RCI` is not self-describing, so the declared cell is the only valid
/// record size: a buffer longer than one cell is sliced at `cell` bytes and the
/// declared record is taken. A buffer whose length is not an exact multiple of
/// the declared cell is a **spec hole** — the asset fails (counted as
/// `assets_missing`) and is never decoded at an inferred size, which would turn
/// a wrong art cell into a false PASS. A palettised multi-record container has
/// no declared per-record palette layout here and is reported as a hole too.
fn rci_declared_record(bytes: &[u8], width: u16, height: u16) -> Result<(&[u8], usize), String> {
    let cell = usize::from(width) * usize::from(height);
    if cell == 0 {
        return Err("declared rect is 0×0: no RCI record size".to_owned());
    }
    if bytes.len() < cell {
        return Err(format!(
            "{} B < declared cell {width}×{height} = {cell} B: spec hole",
            bytes.len()
        ));
    }
    if bytes.len() == cell || bytes.len() == cell + 768 {
        return Ok((bytes, 1));
    }
    if bytes.len().is_multiple_of(cell) {
        let records = bytes.len() / cell;
        let start = RCI_DECLARED_RECORD_INDEX * cell;
        return Ok((&bytes[start..start + cell], records));
    }
    Err(format!(
        "declared rect {width}×{height} = {cell} B does not divide {} B ({} record(s) at best): spec hole",
        bytes.len(),
        bytes.len().div_ceil(cell)
    ))
}

/// `.VID`/`.FLC` are a **video class** — §4(ii) names only `IMG`/`CIF`/`FNT`/
/// `RCI` — decoded through the W34-4 `decode_frames`. They are reported as
/// their own number by [`measure`], never folded into the image count.
fn decode_video_asset(dir: &Path, asset: &str) -> Result<(), String> {
    let path = daggerfall_app::mode_a::arena2_path(asset)?;
    let bytes = std::fs::read(&path).map_err(|err| format!("{asset}: {err}"))?;
    let frames = decode_frames(&bytes).map_err(|err| format!("{asset}: {err}"))?;
    if frames.is_empty() {
        return Err(format!(
            "{asset}: video sin frames (dir={})",
            dir.display()
        ));
    }
    Ok(())
}

/// A declared surface that only a video container can satisfy.
fn is_video_asset(asset: &str) -> bool {
    let upper = asset.to_ascii_uppercase();
    upper.ends_with(".VID") || upper.ends_with(".FLC")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A declared 22×22 `SPOP.RCI` window (the only multi-record asset class).
    fn spop_spec() -> &'static WindowSpec {
        window_specs()
            .find(|spec| spec.asset_retail == Some("SPOP.RCI"))
            .expect("SPOP.RCI is declared by a playable spec")
    }

    /// `$DF_ARENA2` or `$HOME/Games/Daggerfall/app/ARENA2`; `None` when absent.
    fn arena2_dir() -> Option<PathBuf> {
        if let Ok(dir) = std::env::var("DF_ARENA2") {
            return Some(PathBuf::from(dir));
        }
        std::env::var("HOME")
            .ok()
            .map(|home| PathBuf::from(home).join("Games/Daggerfall/app/ARENA2"))
            .filter(|dir| dir.is_dir())
    }

    #[test]
    fn rci_declared_record_slices_the_declared_cell() {
        let bytes = vec![0_u8; 484 * 9];
        let (surface, records) = rci_declared_record(&bytes, 22, 22).expect("9 exact records");
        assert_eq!(records, 9);
        assert_eq!(surface.len(), 484);
        assert_eq!(surface.as_ptr(), bytes.as_ptr(), "record 0 starts at offset 0");
    }

    #[test]
    fn rci_declared_record_rejects_a_spec_hole() {
        // 500 B is not a multiple of the declared 22×22 = 484 B cell.
        let err = rci_declared_record(&vec![0_u8; 500], 22, 22).unwrap_err();
        assert!(err.contains("spec hole"), "{err}");
        assert!(err.contains("484"), "{err}");
    }

    #[test]
    fn rci_declared_record_rejects_an_undersized_or_zero_cell() {
        assert!(rci_declared_record(&[], 22, 22).is_err());
        assert!(rci_declared_record(&vec![0_u8; 400], 22, 22).is_err());
        assert!(rci_declared_record(&vec![0_u8; 484], 0, 4).is_err());
    }

    #[test]
    fn a_hole_makes_the_asset_missing_not_absorbed() {
        // The decode layer returns `Err`, which is exactly what `measure`
        // counts as `assets_missing`: a wrong declared rect can never PASS.
        let hole = vec![0_u8; 500];
        assert!(decode_retail_bytes(Path::new("."), "SPOP.RCI", &hole, spop_spec()).is_err());
    }

    #[test]
    fn the_whole_container_is_never_a_single_record() {
        let Some(dir) = arena2_dir() else {
            eprintln!("SKIP the_whole_container_is_never_a_single_record: ARENA2 ausente");
            return;
        };
        let bytes = std::fs::read(dir.join("SPOP.RCI")).expect("SPOP.RCI in ARENA2");
        // The pre-fix pathology: a 4356 B container is not one 22×22 record,
        // palettised or not.
        assert!(decode_rci(&bytes, 22, 22, false).is_err());
        assert!(decode_rci(&bytes, 22, 22, true).is_err());
        let (surface, records) = rci_declared_record(&bytes, 22, 22).expect("declared cell");
        assert_eq!(records, 9);
        let image = decode_rci(surface, 22, 22, false).expect("declared record decodes");
        assert_eq!((image.width, image.height), (22, 22));
    }

    #[test]
    fn video_is_a_separate_class() {
        assert!(is_video_asset("ANIM0001.VID"));
        assert!(is_video_asset("AZURA.FLC"));
        assert!(!is_video_asset("SPOP.RCI"));
        assert!(!is_video_asset("TALK01I0.IMG"));
    }
}

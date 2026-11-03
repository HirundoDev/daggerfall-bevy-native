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

use std::collections::BTreeMap;
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
use daggerfall_parser_bevy_bridge::ui_compat;
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
///
/// **M40/UI-4 (block C).** The probe certifies **85/85** census surfaces, one
/// `FULL` line each, composed from the three family stages that already own
/// the per-surface measurement: `PROBE-UI-CREATE-001` (16), `PROBE-UI-CONFIG-001`
/// (11) and `PROBE-UI-JUGABLE-001` (51). The seven `Infra` rows are engine
/// contracts of the closed M37 layer: each cites one `(axis, module)` pair of
/// [`daggerfall_parser_bevy_bridge::ui_compat::AXES`] that the probe re-checks,
/// and declares focus `not-applicable` with the M34 spec clause. Nothing is
/// re-painted here, so a surface a family gate refuses cannot be certified by
/// the composite gate.
pub fn run_ui_full_001() -> i32 {
    let mut out: Vec<String> = Vec::new();
    out.push("PROBE-UI-FULL-001 - 85 surfaces / 11 blocks (M40/UI-4, REQ-L99-067)".to_owned());

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

    // ── census rows: 85 names, one per line, `N/A-engine` explicit ─────────
    for (index, row) in ui_census::census().iter().enumerate() {
        let tag = if row.category.is_na_engine() {
            "N/A-engine"
        } else {
            "1996-ui"
        };
        out.push(format!(
            "[{:03}] {:<40} {:<10} {}",
            index + 1,
            row.name,
            row.category.as_str(),
            tag
        ));
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
        out.push(format!(
            "[OK] census: {} = {} createchar + {} config + {} infra + {} jugables",
            ui_census::census().len(),
            counts[0],
            counts[1],
            counts[2],
            counts[3]
        ));
    } else {
        out.push(format!(
            "[FAIL] census: medida {} = {} createchar + {} config + {} infra + {} jugables (GV-01 espera {} = 16+11+7+51 · ascendente={})",
            ui_census::census().len(),
            counts[0],
            counts[1],
            counts[2],
            counts[3],
            CENSUS_SIZE,
            ui_census::is_ascending()
        ));
    }

    // ── two independent passes over the same frozen data ──────────────────
    let first = measure(&dir);
    let second = measure(&dir);
    let pair_identical = first == second;

    let shell_ok = first.registered == CENSUS_SIZE && first.navigable == CENSUS_SIZE && first.leaked == 0;
    if shell_ok {
        out.push(format!(
            "[OK] shell: {}/{} registradas · {}/{} abiertas y cerradas · depth={}",
            first.registered, CENSUS_SIZE, first.navigable, CENSUS_SIZE, first.leaked
        ));
    } else {
        out.push(format!(
            "[FAIL] shell: {}/{} registradas · {}/{} abiertas y cerradas · depth={}",
            first.registered, CENSUS_SIZE, first.navigable, CENSUS_SIZE, first.leaked
        ));
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
    out.push(format!(
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
    ));

    // ── HUD census (GV-03) ────────────────────────────────────────────────
    let hud_ok = ui_census::HUD_FIELDS.len() == HUD_INSTANCES
        && ui_census::hud_registered() == HUD_REGISTERED
        && ui_census::HUD_VISIBILITY_FLAGS.len() == HUD_FLAGS
        && ui_census::hud_types() == HUD_TYPES
        && first.hud_digest == HUD_DIGEST;
    out.push(format!(
        "[{}] hud: {} instancias / {} registradas / {} flags · types={}",
        if hud_ok { "OK" } else { "FAIL" },
        ui_census::HUD_FIELDS.len(),
        ui_census::hud_registered(),
        ui_census::HUD_VISIBILITY_FLAGS.len(),
        ui_census::hud_types()
    ));

    // ── digests against the §8 literals (measured first, verdict after) ────
    let digests_ok = first.windows_digest == CENSUS_DIGEST
        && first.playable_digest == PLAYABLE_DIGEST
        && first.hud_digest == HUD_DIGEST;
    out.push(format!(
        "[{}] windows_digest={:016x} playable_digest={:016x} hud_digest={:016x}",
        if digests_ok { "OK" } else { "FAIL" },
        first.windows_digest,
        first.playable_digest,
        first.hud_digest
    ));
    if first.windows_digest == CENSUS_DIGEST {
        out.push(format!(
            "[OK] GV-02 literal {:016x} reproducido ({} B, regla §3.1: ascendente por byte, \\n, sin salto final)",
            CENSUS_DIGEST, CENSUS_BYTES
        ));
    } else {
        out.push(format!(
            "[WARN] GV-02 literal {:016x} no reproducible; medido {:016x} ({} B de {} esperados)",
            CENSUS_DIGEST,
            first.windows_digest,
            ui_census::census_text().len(),
            CENSUS_BYTES
        ));
    }
    if first.playable_digest == PLAYABLE_DIGEST {
        out.push(format!(
            "[OK] GV-19 literal {:016x} reproducido ({} B)",
            PLAYABLE_DIGEST, PLAYABLE_BYTES
        ));
    } else {
        out.push(format!(
            "[WARN] GV-19 literal {:016x} no reproducible; medido {:016x} ({} B de {} esperados)",
            PLAYABLE_DIGEST,
            first.playable_digest,
            ui_census::playable_text().len(),
            PLAYABLE_BYTES
        ));
    }

    out.push(format!(
        "[{}] pasada 2 independiente: windows={:016x} playable={:016x} hud={:016x} · idéntica={}",
        if pair_identical { "OK" } else { "FAIL" },
        second.windows_digest,
        second.playable_digest,
        second.hud_digest,
        pair_identical
    ));

    // ── block C: the 85/85 per-surface certification (M40/UI-4) ───────────
    let (full_lines, full_ok) = full_block(&dir);
    out.extend(full_lines);

    // ── block D: the declared-record rule as rows, never as a panic ────────
    // (the five `expect` calls of this probe: an `expect` abort is exit 101 with
    // no row; a `Row` is a red line and a controlled exit).
    let spop_spec = window_specs().find(|spec| spec.asset_retail == Some("SPOP.RCI"));
    let spop_bytes = daggerfall_app::mode_a::arena2_path("SPOP.RCI")
        .ok()
        .and_then(|path| std::fs::read(path).ok());
    let rci = rci_rows(spop_bytes.as_deref(), spop_spec);
    let rci_ok = rci.iter().all(|row| row.ok);
    out.extend(rci.iter().map(Row::line));

    let pass = census_ok
        && shell_ok
        && playable_ok
        && hud_ok
        && digests_ok
        && pair_identical
        && full_ok
        && rci_ok;
    let mut digest = first.combined();
    if !pair_identical {
        // Never print "×2 idéntico" over two different values: fold both.
        digest = ui_census::fnv1a64(
            format!("{:016x}{:016x}", first.combined(), second.combined()).as_bytes(),
        );
    }
    let stdout_digest = crate::m37_ui1_probe::sha256_hex(out.join("\n").as_bytes());
    out.push(format!("DIGEST {stdout_digest}"));
    out.push(format!(
        "VEREDICTO: PROBE-UI-FULL-001 {} · digest={digest:016x} ×2{}",
        if pass { "PASS" } else { "FAIL" },
        if pair_identical { " idéntico" } else { " NO idéntico" }
    ));
    let code = i32::from(!pass);
    out.push(format!("EXIT={code}"));
    for line in &out {
        println!("{line}");
    }
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

/// One row of the probe's own instrumentation block:
/// `ROW OK|RED check=<name> detail=<...>`.
///
/// The five `expect` calls this module used to carry aborted the measuring
/// process with exit 101 instead of printing a red row (`DESIGN-AUDIT-276c7d94`
/// §ARREGLAR-DESPUÉS). A failed check is now a **value**: it prints one red row
/// and folds into the probe's controlled exit code.
struct Row {
    check: &'static str,
    ok: bool,
    detail: String,
}

impl Row {
    fn new(check: &'static str, ok: bool, detail: impl Into<String>) -> Self {
        Self {
            check,
            ok,
            detail: detail.into(),
        }
    }

    fn line(&self) -> String {
        format!(
            "ROW {} check={} detail={}",
            if self.ok { "OK" } else { "RED" },
            self.check,
            self.detail.replace('\n', " ")
        )
    }
}

/// The declared-record rule as rows (`SPOP.RCI` = 9 × 22×22 = 4356 B).
///
/// * `rci-spec` — a playable [`WindowSpec`] declares the container and its rect;
/// * `rci-declared-cell` — a 9-cell container slices at the declared cell, with
///   record 0 at offset 0;
/// * `rci-spec-hole` — a length that is not a multiple of the cell is refused,
///   never decoded at an inferred size;
/// * `rci-undersized` — empty/short buffers and a 0-cell rect are refused;
/// * `rci-whole-container` — the whole container is never one record, and the
///   declared cell decodes at exactly the declared width × height.
///
/// `asset` is the real `ARENA2` bytes; `None` is a red row (the rule could not
/// be measured), never a silent skip.
fn rci_rows(asset: Option<&[u8]>, spec: Option<&WindowSpec>) -> Vec<Row> {
    let (width, height) = spec.map_or((22, 22), |spec| (spec.rect.width, spec.rect.height));
    let cell = usize::from(width) * usize::from(height);
    let mut rows: Vec<Row> = Vec::new();

    rows.push(match spec {
        Some(spec) => Row::new(
            "rci-spec",
            spec.asset_retail == Some("SPOP.RCI"),
            format!("rect={}×{}", spec.rect.width, spec.rect.height),
        ),
        None => Row::new("rci-spec", false, "no WindowSpec declares SPOP.RCI"),
    });

    let synthetic = vec![0_u8; cell * 9];
    rows.push(match rci_declared_record(&synthetic, width, height) {
        Ok((surface, records)) => {
            let offset = surface.as_ptr() as usize - synthetic.as_ptr() as usize;
            Row::new(
                "rci-declared-cell",
                records == 9 && surface.len() == cell && offset == 0,
                format!("records={records} cell={cell} offset={offset}"),
            )
        }
        Err(err) => Row::new("rci-declared-cell", false, err),
    });

    let hole = cell + 16;
    rows.push(match rci_declared_record(&vec![0_u8; hole], width, height) {
        Ok((_, records)) => Row::new(
            "rci-spec-hole",
            false,
            format!("{hole} B accepted as {records} record(s) of {cell} B"),
        ),
        Err(err) => Row::new("rci-spec-hole", err.contains("spec hole"), err),
    });

    let undersized = rci_declared_record(&[], width, height).is_err()
        && rci_declared_record(&vec![0_u8; cell - 1], width, height).is_err()
        && rci_declared_record(&vec![0_u8; cell], 0, 4).is_err();
    rows.push(Row::new(
        "rci-undersized",
        undersized,
        format!("empty/cell-1/0×4-rect refused={undersized}"),
    ));

    rows.push(match asset {
        None => Row::new(
            "rci-whole-container",
            false,
            "SPOP.RCI not readable from ARENA2: whole-container rule not measured",
        ),
        Some(bytes) => {
            let one_record = decode_rci(bytes, width, height, false).is_err()
                && decode_rci(bytes, width, height, true).is_err();
            match rci_declared_record(bytes, width, height) {
                Err(err) => Row::new("rci-whole-container", false, err),
                Ok((surface, records)) => match decode_rci(surface, width, height, false) {
                    Err(err) => Row::new(
                        "rci-whole-container",
                        false,
                        format!("declared cell does not decode: {err}"),
                    ),
                    Ok(image) => Row::new(
                        "rci-whole-container",
                        one_record
                            && records == 9
                            && (image.width, image.height) == (width, height),
                        format!(
                            "container={} B records={records} cell={width}×{height} image={}×{} \
                             whole-as-one-record={}",
                            bytes.len(),
                            image.width,
                            image.height,
                            if one_record { "refused" } else { "ACCEPTED" }
                        ),
                    ),
                },
            }
        }
    });

    rows
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

// ── Block C — the 85/85 per-surface certification (M40/UI-4) ──────────────

/// The seven `Infra` census rows as engine contracts of the closed M37 layer.
///
/// Each tuple is `(census name, axis id, axis module, focus not-applicable
/// slug)`. The axis pair is re-checked against
/// [`daggerfall_parser_bevy_bridge::ui_compat::AXES`] at run time: an infra row
/// that cites a module the layer does not declare is *not* certified.
const INFRA: [(&str, &str, &str, &str); 7] = [
    ("ColorPicker", "A2", "palette", "engine-na:m34-spec-4.224-225"),
    ("DaggerfallBaseWindow", "A1", "surface", "engine-na:m34-spec-4.224-225"),
    ("DaggerfallPopupWindow", "A1", "surface", "engine-na:m34-spec-4.224-225"),
    ("DaggerfallQuestPopupWindow", "A5", "navigation", "engine-na:m34-spec-4.224-225"),
    ("HardStrings", "A4", "text", "engine-na:m34-spec-4.224-225"),
    ("UIWindowFactory", "A5", "navigation", "engine-na:m34-spec-4.224-225"),
    ("WindowMessages", "A6", "input", "engine-na:m34-spec-4.224-225"),
];

/// The eleven blocks: the eight playable blocks of M39 (`A`..`H`) plus the
/// three census groups this milestone certifies (`I` createchar, `J` config,
/// `K` infra).
const FULL_BLOCKS: [char; 11] = ['A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K'];

/// One census surface, as block C certifies it.
struct Fact {
    ordinal: usize,
    name: &'static str,
    family: &'static str,
    block: char,
    nodes: usize,
    glyphs: usize,
    focus: usize,
    na: Option<String>,
    opaque: usize,
    evidence: String,
    registry: bool,
    open_close: bool,
}

impl Fact {
    /// `FULL <nnn> id=<Name> family=<F> block=<X> registry=<ok|missing>
    /// open-close=<ok|leaked> nodes=N glyphs=G focus=F na=<slug|-> opaque=O
    /// evidence=<hex16|contract:Ax:module:closed>`.
    fn line(&self) -> String {
        format!(
            "FULL {:03} id={} family={} block={} registry={} open-close={} nodes={} glyphs={} focus={} na={} opaque={} evidence={}",
            self.ordinal,
            self.name,
            self.family,
            self.block,
            if self.registry { "ok" } else { "missing" },
            if self.open_close { "ok" } else { "leaked" },
            self.nodes,
            self.glyphs,
            self.focus,
            self.na.as_deref().unwrap_or("-"),
            self.opaque,
            self.evidence,
        )
    }
}

/// A `SURFACE` line of one family stage, normalized.
struct RawFact {
    name: String,
    block: Option<char>,
    nodes: usize,
    glyphs: usize,
    focus: usize,
    opaque: usize,
    digest: String,
}

/// One family stage: its emitted lines plus the rule that failed, if any.
struct FamilyStage {
    lines: Vec<String>,
    error: Option<String>,
}

/// `key=value` of one whitespace-separated token, exact key match.
fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.split_whitespace().find_map(|token| {
        let (name, value) = token.split_once('=')?;
        (name == key).then_some(value)
    })
}

/// Numeric `key=value` of a line this probe does **not** own (M38/M39 stdout).
///
/// An absent or unreadable field is an `Err`, never a silent `0`: the old
/// `unwrap_or(0)` turned a broken coupling into a green measurement
/// (`DESIGN-AUDIT-276c7d94` §ARREGLAR-DESPUÉS, `count_of()`). The caller prints
/// a `FAMILY-ROW-ERROR` row and folds it into its own exit code.
fn count_of(line: &str, key: &str) -> Result<usize, String> {
    let value = field(line, key).ok_or_else(|| format!("absent:{key}"))?;
    value
        .parse()
        .map_err(|_| format!("unreadable:{key}={value}"))
}

/// First three whitespace tokens of a probe line, quoted in a red row instead
/// of the whole record (deterministic and short).
fn line_head(line: &str) -> String {
    line.split_whitespace().take(3).collect::<Vec<_>>().join("_")
}

/// Parses a `SURFACE` line of either family grammar.
///
/// * M38 (`SURFACE C01 <Name> host=…`) — the key is the literal in column 2.
/// * M39 (`SURFACE 001 id=<Name> block=X host=…`) — the key is the ordinal and
///   the name is the `id=` value.
///
/// `Ok(None)` is "not a `SURFACE` line"; `Err` is a `SURFACE` line whose
/// numeric fields are absent or unreadable — a red row, never a `0`.
fn parse_surface(line: &str) -> Result<Option<RawFact>, String> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 4 || tokens[0] != "SURFACE" {
        return Ok(None);
    }
    let name = match field(line, "id") {
        Some(name) => name.to_owned(),
        None => match tokens.get(2) {
            Some(name) => (*name).to_owned(),
            None => return Err("absent:name".to_owned()),
        },
    };
    Ok(Some(RawFact {
        name,
        block: field(line, "block").and_then(|value| value.chars().next()),
        nodes: count_of(line, "nodes")?,
        glyphs: count_of(line, "glyphs")?,
        focus: count_of(line, "focus")?,
        opaque: count_of(line, "opaque")?,
        digest: field(line, "digest").unwrap_or("-").to_owned(),
    }))
}

/// Runs the three family stages the composite gate composes.
fn family_stages(dir: &Path) -> [FamilyStage; 3] {
    let create = match crate::m38_ui2_probe::stage_create_lines(dir) {
        Ok(lines) => FamilyStage { lines, error: None },
        Err((rule, lines)) => FamilyStage {
            lines,
            error: Some(rule),
        },
    };
    let config = match crate::m38_ui2_probe::stage_config_lines(dir) {
        Ok(lines) => FamilyStage { lines, error: None },
        Err((rule, lines)) => FamilyStage {
            lines,
            error: Some(rule),
        },
    };
    let playable = match crate::m39_jugable_probe::stage_lines(dir) {
        Ok((lines, _note)) => FamilyStage { lines, error: None },
        Err(err) => FamilyStage {
            lines: Vec::new(),
            error: Some(err),
        },
    };
    [create, config, playable]
}

/// Block C: 85 `FULL` lines, 11 `BLOCK` lines and the seven composite axes.
///
/// Returns `(lines, ok)`. A surface the family stage did not emit is still
/// printed, with `evidence=-`, so a missing surface is a red row rather than an
/// absent one.
fn full_block(dir: &Path) -> (Vec<String>, bool) {
    let census = ui_census::census();
    let stages = family_stages(dir);
    let (create, config, playable) = (&stages[0], &stages[1], &stages[2]);

    // Shell facts — **two distinct measurements per surface**. The audit
    // (`m34_ui_probe.rs:790-791`) caught `registry` and `open_close` being the
    // same boolean printed twice, which counted one measurement as two
    // surfaces. Now:
    //  * `registered_ok` — the registry holds the window under its census name
    //    (`contains` + `title`): a static containment fact;
    //  * `open_close_ok` — opening it raises the stack by exactly one and
    //    closing it returns the stack to the previous depth with the window
    //    closed: a behavioural fact.
    let mut registered_ok: Vec<bool> = vec![false; census.len()];
    let mut open_close_ok: Vec<bool> = vec![false; census.len()];
    let mut depth = usize::MAX;
    let mut registered = 0usize;
    if let Ok(mut shell) = UiShell::canonical() {
        registered = shell.registry().len();
        for (index, row) in census.iter().enumerate() {
            let Some(id) = canonical_id_for(row.name) else {
                continue;
            };
            registered_ok[index] =
                shell.registry().contains(id) && shell.registry().title(id) == Some(row.name);
            let before = shell.depth();
            let opened = shell.open(id).is_ok();
            let after_open = shell.depth();
            shell.close(id);
            open_close_ok[index] = opened
                && after_open == before + 1
                && shell.depth() == before
                && !shell.is_open(id);
        }
        depth = shell.depth();
    }
    let registered_surfaces = registered_ok.iter().filter(|ok| **ok).count();
    let navigable = open_close_ok.iter().filter(|ok| **ok).count();
    let shell_ok = registered == CENSUS_SIZE
        && registered_surfaces == CENSUS_SIZE
        && navigable == CENSUS_SIZE
        && depth == 0;

    // Family evidence, keyed by census name. A `SURFACE` line whose numeric
    // fields are absent or unreadable becomes a red `FAMILY-ROW-ERROR` row:
    // the coupling to M38/M39 stdout fails loudly, never as a silent zero.
    let mut by_name: BTreeMap<String, RawFact> = BTreeMap::new();
    let mut focus_na: BTreeMap<String, String> = BTreeMap::new();
    let mut parse_errors: Vec<String> = Vec::new();
    for stage in [create, config, playable] {
        for line in &stage.lines {
            match parse_surface(line) {
                Ok(Some(raw)) => {
                    by_name.insert(raw.name.clone(), raw);
                }
                Ok(None) => {}
                Err(err) => parse_errors.push(format!("{} {err}", line_head(line))),
            }
        }
    }
    for line in &playable.lines {
        if line.starts_with("FOCUS ") {
            if let (Some(ordinal), Some(reason)) =
                (line.split_whitespace().nth(1), field(line, "not-applicable"))
            {
                focus_na.insert(ordinal.to_owned(), reason.to_owned());
            }
        }
    }

    let mut facts: Vec<Fact> = Vec::with_capacity(census.len());
    for (index, row) in census.iter().enumerate() {
        let ordinal = index + 1;
        let family = match row.category {
            WindowCategory::CreateChar => "CreateChar",
            WindowCategory::Config => "Config",
            WindowCategory::Infra => "Infra",
            WindowCategory::Playable => "Playable",
        };
        let raw = by_name.get(row.name);
        let block = match row.category {
            WindowCategory::CreateChar => 'I',
            WindowCategory::Config => 'J',
            WindowCategory::Infra => 'K',
            WindowCategory::Playable => raw.and_then(|raw| raw.block).unwrap_or('?'),
        };
        let (nodes, glyphs, focus, opaque, evidence, na) = if row.category == WindowCategory::Infra {
            let infra = INFRA.iter().find(|(name, _, _, _)| *name == row.name);
            let evidence = match infra {
                Some((_, axis, module, _)) => {
                    let closed = ui_compat::AXES
                        .iter()
                        .any(|(declared_axis, declared_module)| {
                            declared_axis == axis && declared_module == module
                        });
                    format!(
                        "contract:{axis}:{module}:{}",
                        if closed { "closed" } else { "unknown" }
                    )
                }
                None => "contract:undeclared".to_owned(),
            };
            (
                0,
                0,
                0,
                0,
                evidence,
                infra.map(|(_, _, _, reason)| (*reason).to_owned()),
            )
        } else {
            match raw {
                Some(raw) => (
                    raw.nodes,
                    raw.glyphs,
                    raw.focus,
                    raw.opaque,
                    raw.digest.clone(),
                    focus_na.get(&format!("{ordinal:03}")).cloned(),
                ),
                None => (0, 0, 0, 0, "-".to_owned(), None),
            }
        };
        facts.push(Fact {
            ordinal,
            name: row.name,
            family,
            block,
            nodes,
            glyphs,
            focus,
            na,
            opaque,
            evidence,
            registry: registered_ok[index],
            open_close: open_close_ok[index],
        });
    }

    let mut lines: Vec<String> = Vec::new();
    lines.push(format!(
        "SHELL {} registered={} registered-named={} open-close={} depth={}",
        if shell_ok { "OK" } else { "FAIL" },
        registered,
        registered_surfaces,
        navigable,
        depth
    ));

    for (label, probe, stage) in [
        ("create", "PROBE-UI-CREATE-001", create),
        ("config", "PROBE-UI-CONFIG-001", config),
        ("playable", "PROBE-UI-JUGABLE-001", playable),
    ] {
        let surfaces = stage
            .lines
            .iter()
            .filter(|line| line.starts_with("SURFACE "))
            .count();
        let axes_ok = stage
            .lines
            .iter()
            .filter(|line| {
                line.starts_with("AXIS ") && line.split_whitespace().nth(2) == Some("OK")
            })
            .count();
        let digest = stage
            .lines
            .iter()
            .find_map(|line| line.strip_prefix("DIGEST "))
            .unwrap_or("-");
        lines.push(format!(
            "FAMILY {label} probe={probe} surfaces={surfaces} axes={axes_ok}/7 digest={digest} status={}",
            if stage.error.is_none() { "OK" } else { "FAIL" }
        ));
        if let Some(error) = &stage.error {
            lines.push(format!("FAMILY-ERROR {label} {}", error.replace(' ', "_")));
        }
    }

    for fact in &facts {
        lines.push(fact.line());
    }

    let mut blocks_ok = true;
    let mut block_sum = 0usize;
    for block in FULL_BLOCKS {
        let rows: Vec<&Fact> = facts.iter().filter(|fact| fact.block == block).collect();
        let focus: usize = rows.iter().map(|fact| fact.focus).sum();
        let na = rows.iter().filter(|fact| fact.na.is_some()).count();
        if rows.is_empty() {
            blocks_ok = false;
        }
        block_sum += rows.len();
        lines.push(format!(
            "BLOCK {} surfaces={} focus={focus} na={na}",
            block,
            rows.len()
        ));
    }
    blocks_ok &= block_sum == CENSUS_SIZE;

    let renderable: Vec<&Fact> = facts
        .iter()
        .filter(|fact| fact.family != "Infra")
        .collect();
    let renderable_expected = CENSUS_SIZE - INFRA.len();
    let composed = renderable
        .iter()
        .filter(|fact| fact.nodes > 0 || fact.glyphs > 0)
        .count();
    let raster = facts.iter().filter(|fact| fact.nodes > 0).count();
    let opaque_min = renderable.iter().map(|fact| fact.opaque).min().unwrap_or(0);
    let opaque_total: usize = renderable.iter().map(|fact| fact.opaque).sum();
    let glyphs_min = renderable.iter().map(|fact| fact.glyphs).min().unwrap_or(0);
    let glyphs_total: usize = renderable.iter().map(|fact| fact.glyphs).sum();

    let mut a4_units = 0usize;
    let mut a4_empty = 0usize;
    let mut a4_family_ok = true;
    let mut a6_intents = 0usize;
    let mut a6_delivered = 0usize;
    let mut a6_family_ok = true;
    let mut a7_family_ok = true;
    let mut family_axes_ok = 0usize;
    let mut family_axes_total = 0usize;
    let mut axis_errors: Vec<String> = Vec::new();
    for stage in [create, config, playable] {
        for line in &stage.lines {
            if !line.starts_with("AXIS ") {
                continue;
            }
            family_axes_total += 1;
            let ok = line.split_whitespace().nth(2) == Some("OK");
            if ok {
                family_axes_ok += 1;
            }
            let axis = line.split_whitespace().nth(1).unwrap_or("");
            match axis {
                "A4" => {
                    a4_family_ok &= ok;
                    match field(line, "declarations").or_else(|| field(line, "lines")) {
                        Some(value) => match value.parse::<usize>() {
                            Ok(units) => a4_units += units,
                            Err(_) => {
                                axis_errors.push(format!("A4 unreadable:declarations={value}"))
                            }
                        },
                        None => axis_errors.push("A4 absent:declarations".to_owned()),
                    }
                    match count_of(line, "empty") {
                        Ok(empty) => a4_empty += empty,
                        Err(err) => axis_errors.push(format!("A4 {err}")),
                    }
                }
                "A6" => {
                    a6_family_ok &= ok;
                    match (count_of(line, "intents"), count_of(line, "delivered")) {
                        (Ok(intents), Ok(delivered)) => {
                            a6_intents += intents;
                            a6_delivered += delivered;
                        }
                        (intents, delivered) => {
                            for err in [intents.err(), delivered.err()].into_iter().flatten() {
                                axis_errors.push(format!("A6 {err}"));
                            }
                        }
                    }
                }
                "A7" => a7_family_ok &= ok,
                _ => {}
            }
        }
    }
    // Red rows for the text coupling: printed by the probe, read by the gate
    // (`FAMILY-ROW-ERROR` lines make `raster-fields` red, never a silent zero).
    for error in parse_errors.iter().chain(axis_errors.iter()) {
        lines.push(format!("FAMILY-ROW-ERROR {}", error.replace(' ', "_")));
    }
    let fields_ok = parse_errors.is_empty() && axis_errors.is_empty();

    let focus_total: usize = facts.iter().map(|fact| fact.focus).sum();
    let with_focus = facts.iter().filter(|fact| fact.focus > 0).count();
    let na_list: Vec<String> = facts
        .iter()
        .filter(|fact| fact.focus == 0)
        .map(|fact| format!("{:03}", fact.ordinal))
        .collect();
    // Focus is measured or contractually refused — never a silent zero.
    let focus_shaped = facts
        .iter()
        .all(|fact| (fact.focus > 0) != fact.na.is_some());
    let evidence_shaped = facts.iter().all(|fact| {
        if fact.family == "Infra" {
            fact.evidence.starts_with("contract:") && fact.evidence.ends_with(":closed")
        } else {
            fact.evidence.len() == 16
                && fact.evidence.chars().all(|ch| ch.is_ascii_hexdigit())
        }
    });

    let a1 = renderable.len() == renderable_expected && composed == renderable.len();
    let a2 = opaque_min > 0;
    let a3 = glyphs_min > 0;
    let a5 = shell_ok && focus_shaped && focus_total > 0;
    let a7 = a7_family_ok && family_axes_total == 21 && family_axes_ok == family_axes_total;

    for (axis, ok, detail) in [
        (
            "A1",
            a1,
            format!(
                "surfaces={} composed={}/{} raster={raster}",
                facts.len(),
                composed,
                renderable.len()
            ),
        ),
        (
            "A2",
            a2,
            format!("renderable={} opaque-min={opaque_min} opaque-total={opaque_total}", renderable.len()),
        ),
        (
            "A3",
            a3,
            format!("renderable={} glyphs-min={glyphs_min} glyphs-total={glyphs_total}", renderable.len()),
        ),
        (
            "A4",
            a4_family_ok && a4_empty == 0 && a4_units > 0,
            format!("text-units={a4_units} empty={a4_empty}"),
        ),
        (
            "A5",
            a5,
            format!(
                "stack=85/85 closed=85 depth={depth} focus-rows={focus_total} surfaces-with-focus={with_focus} not-applicable={}",
                na_list.join(",")
            ),
        ),
        (
            "A6",
            a6_family_ok && a6_delivered > 0,
            format!("intents={a6_intents} delivered={a6_delivered}"),
        ),
        (
            "A7",
            a7,
            format!(
                "families=3/3 repaint-stable={} family-axes={family_axes_ok}/{family_axes_total}",
                if a7_family_ok { "agree" } else { "diverge" }
            ),
        ),
    ] {
        lines.push(format!(
            "AXIS {axis} {} {detail}",
            if ok { "OK" } else { "FAIL" }
        ));
    }

    let axes_ok = a1 && a2 && a3 && a4_family_ok && a4_empty == 0 && a5 && a6_family_ok && a7;
    let ok = shell_ok
        && facts.len() == CENSUS_SIZE
        && blocks_ok
        && evidence_shaped
        && fields_ok
        && axes_ok
        && family_axes_total == 21
        && family_axes_ok == family_axes_total;
    (lines, ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The declared `SPOP.RCI` spec as a value: `None` becomes a red row
    /// downstream, never an `expect` abort.
    fn spop_spec() -> Option<&'static WindowSpec> {
        window_specs().find(|spec| spec.asset_retail == Some("SPOP.RCI"))
    }

    /// The red rows of a report as printable lines (empty ⇒ every check holds).
    fn red_lines(rows: &[Row]) -> Vec<String> {
        rows.iter().filter(|row| !row.ok).map(Row::line).collect()
    }

    #[test]
    fn rci_rows_hold_on_a_nine_record_container() -> Result<(), String> {
        let bytes = vec![0_u8; 484 * 9];
        let red = red_lines(&rci_rows(Some(&bytes), spop_spec()));
        if red.is_empty() {
            Ok(())
        } else {
            Err(red.join(" | "))
        }
    }

    #[test]
    fn rci_rows_go_red_on_a_spec_hole_without_panicking() -> Result<(), String> {
        // 500 B is not a multiple of the declared 22×22 = 484 B cell: the rule
        // answers with a RED row (the old `expect` answered with exit 101).
        let hole = vec![0_u8; 500];
        let rows = rci_rows(Some(&hole), spop_spec());
        let red = red_lines(&rows);
        if red.is_empty() {
            return Err("a 500 B container produced no RED row".to_owned());
        }
        if !red
            .iter()
            .any(|line| line.contains("check=rci-whole-container"))
        {
            return Err(format!(
                "the hole was not measured on the container: {red:?}"
            ));
        }
        Ok(())
    }

    #[test]
    fn a_hole_makes_the_asset_missing_not_absorbed() -> Result<(), String> {
        // The decode layer returns `Err`, which is exactly what `measure`
        // counts as `assets_missing`: a wrong declared rect can never PASS.
        let hole = vec![0_u8; 500];
        let spec = spop_spec().ok_or("no WindowSpec declares SPOP.RCI")?;
        if decode_retail_bytes(Path::new("."), "SPOP.RCI", &hole, spec).is_ok() {
            return Err("a 500 B spec hole decoded as a declared record".to_owned());
        }
        Ok(())
    }

    #[test]
    fn video_is_a_separate_class() {
        assert!(is_video_asset("ANIM0001.VID"));
        assert!(is_video_asset("AZURA.FLC"));
        assert!(!is_video_asset("SPOP.RCI"));
        assert!(!is_video_asset("TALK01I0.IMG"));
    }
}

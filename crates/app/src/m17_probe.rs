//! M17 — anti-screen probe for the original 1996 UI aesthetics
//! (`DoD §h` / WI-9 · REQ-L99-050).
//!
//! **PROBE-UI-001** (`--probe UI-001`) decodes at runtime the retail assets
//! of the user's ARENA2 (Mode A read-only) and verifies:
//!
//! 1. **inventory**: 263 `.IMG` · 70 `.CIF` · 5 `.FNT` · 6 `.RCI` · 2 `.GFX`;
//! 2. **decoders**: 188 headers + 3 empty + 66 RCI + 6 palettized RCI;
//!    50 frame CIF + 18 animation CIF + 1 RCI + 1 declared unresolved;
//! 3. **font**: 5 `.FNT` with 14/11 header, 240 glyphs and 1 bpp bitmap;
//! 4. **palettes**: `PAL.PAL` 776 B (`ColHeader` 0xB123), `MAP.PAL` 768 B,
//!    `FMAP_PAL.COL`/`ART_PAL.COL` 776 B, `SKYPAL.DAT` 1,536 B;
//! 5. **golden vectors GV-01..GV-18** (spec §5) with FNV-1a64 digests;
//! 6. **malformed**: truncated/unreadable buffers → typed error, 0 panics;
//! 7. **composition**: 4 panels (HUD/inventory/quests/menu) over the M7 read
//!    models, 0 generic widgets, 320×200 framebuffer, 0 pixels outside the
//!    palette (C2), 640×480 ×2 nearest present with 40+40 letterbox (C1/C1b).
//!
//! **Custody (INV-L99-6):** Mode A only; retail bytes are read from ARENA2
//! at runtime and never enter the repository — only dimension constants and
//! `u64` digests live here. Without an accessible ARENA2 the probe exits with
//! **exit 2** and a literal reason, never with a false PASS.
//!
//! **0 GPU**: the whole path runs headless (pure decoders + compositor).

use std::collections::BTreeMap;
use std::path::Path;

use daggerfall_parser_rs::image::{
    classify_img, decode_cif, decode_img, decode_img_file, project_packed, CifContent, FntFile,
    ImageError, PaintFile,
};
use daggerfall_parser_rs::texture::Palette;
use daggerfall_parser_bevy_bridge::ui_spec::{
    assert_no_generic_widgets, build_all_panels, compose, family_histogram, palette_compliance,
    present_delta, present_interpolated_pixels, present_nearest, HudPanelData, InventoryPanelData,
    QuestPanelData, UiAssets, FONT_FILES,
};
use daggerfall_perception::read_models::{
    HudReadModel, HudSource, PanelSource, SessionPanelReadModel,
};

/// Assets consumed by the 4 panels (headerless `RCI`/`CIF` included).
pub const PANEL_ASSETS: [&str; 43] = [
    "MAIN00I0.IMG",
    "MAIN02I0.IMG",
    "MAIN03I0.IMG",
    "COMPASS.IMG",
    "ITEM00I0.IMG",
    "INVE00I0.IMG",
    "INVE03I0.IMG",
    "INVE06I0.IMG",
    "INVE07I0.IMG",
    "INVE08I0.IMG",
    "INVE09I0.IMG",
    "INVE10I0.IMG",
    "INVE11I0.IMG",
    "INVE12I0.IMG",
    "INVE13I0.IMG",
    "INVE14I0.IMG",
    "ITEM01I0.IMG",
    "BOOK00I0.IMG",
    "SCBG00I0.IMG",
    "SCBG01I0.IMG",
    "SCBG02I0.IMG",
    "SCBG03I0.IMG",
    "SCBG04I0.IMG",
    "SCBG05I0.IMG",
    "SCBG06I0.IMG",
    "SCBG07I0.IMG",
    "SCBG08I0.IMG",
    "SCRL04I0.IMG",
    "SCRL05I0.IMG",
    "SCRL07I0.IMG",
    "SCRL08I0.IMG",
    "SCRL09I0.IMG",
    "SCRL10I0.IMG",
    "SCRL11I0.IMG",
    "SCRL12I0.IMG",
    "SCRL13I0.IMG",
    "SCRL14I0.IMG",
    "TITL00I0.IMG",
    "MENU.IMG",
    "BUTN00I0.IMG",
    "BUTN03I0.IMG",
    // **B3-c** — the two map screens (raw 320×200 RCI). Their palette index
    // range is resolved per asset (`image::ASSET_RANGE_DERIVATIONS`), not by
    // widening the `Map` family floor.
    "MAP100I0.IMG",
    "TRAV0I00.IMG",
];

/// Aggregated result of the probe checks.
struct Checks {
    passed: usize,
    failed: Vec<(&'static str, String)>,
}

impl Checks {
    fn new() -> Self {
        Self {
            passed: 0,
            failed: Vec::new(),
        }
    }

    fn check(&mut self, id: &'static str, ok: bool, detail: String) {
        if ok {
            self.passed += 1;
            println!("  [PASS] {id} — {detail}");
        } else {
            self.failed.push((id, detail.clone()));
            println!("  [FAIL] {id} — {detail}");
        }
    }

    fn fail(&mut self, id: &'static str, detail: String) {
        self.check(id, false, detail);
    }

    fn total(&self) -> usize {
        self.passed + self.failed.len()
    }
}

/// FNV-1a 64 (deterministic, 0 dependencies) over palette-index pixels.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn load(dir: &Path, name: &str) -> Result<Vec<u8>, String> {
    let path = dir.join(name);
    std::fs::read(&path).map_err(|err| format!("reading {path:?}: {err}"))
}

fn count_ext(dir: &Path, ext: &str) -> Result<usize, String> {
    let entries = std::fs::read_dir(dir).map_err(|err| format!("read_dir {dir:?}: {err}"))?;
    let mut count = 0usize;
    for entry in entries {
        let entry = entry.map_err(|err| format!("entry of {dir:?}: {err}"))?;
        let path = entry.path();
        let is_file = entry.file_type().map(|kind| kind.is_file()).unwrap_or(false);
        let matches = path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case(ext))
            .unwrap_or(false);
        if is_file && matches {
            count += 1;
        }
    }
    Ok(count)
}

/// Stage (a): exact UI asset inventory.
fn stage_inventory(dir: &Path, checks: &mut Checks) {
    println!("\n-- (a) ARENA2 inventory --");
    let mut counts = [("IMG", 263usize), ("CIF", 70), ("FNT", 5), ("RCI", 6), ("GFX", 2)];
    for (ext, expected) in counts.iter_mut() {
        match count_ext(dir, ext) {
            Ok(actual) => checks.check(
                "A1",
                actual == *expected,
                format!("{ext}: {actual} (expected {expected})"),
            ),
            Err(reason) => checks.fail("A1", format!("{ext}: {reason}")),
        }
    }
}

/// Stage (b): `.IMG`/`.CIF` decoders.
fn stage_decoders(dir: &Path, checks: &mut Checks) {
    println!("\n-- (b) IMG/CIF decoders --");
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) => {
            checks.fail("B1", format!("read_dir: {err}"));
            return;
        }
    };
    let mut img_names: Vec<String> = Vec::new();
    let mut cif_names: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        let upper = name.to_ascii_uppercase();
        if upper.ends_with(".IMG") {
            img_names.push(name.to_string());
        } else if upper.ends_with(".CIF") {
            cif_names.push(name.to_string());
        }
    }
    img_names.sort();
    cif_names.sort();

    let (mut headers, mut empty, mut rci_raw, mut rci_pal, mut img_errors) = (0, 0, 0, 0, 0);
    for name in &img_names {
        let bytes = match load(dir, name) {
            Ok(bytes) => bytes,
            Err(reason) => {
                img_errors += 1;
                eprintln!("  unreadable IMG {name}: {reason}");
                continue;
            }
        };
        let class = match classify_img(&bytes) {
            Ok(class) => class,
            Err(err) => {
                img_errors += 1;
                eprintln!("  unclassifiable IMG {name}: {err}");
                continue;
            }
        };
        if let Err(err) = decode_img_file(&bytes) {
            img_errors += 1;
            eprintln!("  undecodable IMG {name}: {err}");
            continue;
        }
        match class {
            daggerfall_parser_rs::image::ImgClass::Header => headers += 1,
            daggerfall_parser_rs::image::ImgClass::Empty => empty += 1,
            daggerfall_parser_rs::image::ImgClass::Rci { palettized, .. } => {
                if palettized {
                    rci_pal += 1;
                } else {
                    rci_raw += 1;
                }
            }
        }
    }
    checks.check(
        "B1",
        img_names.len() == 263
            && headers == 188
            && empty == 3
            && rci_raw == 66
            && rci_pal == 6
            && img_errors == 0,
        format!(
            "IMG total={} header={headers} empty={empty} rci_raw={rci_raw} rci_palettized={rci_pal} errors={img_errors}",
            img_names.len()
        ),
    );

    let (mut frames, mut anims, mut rci, mut unresolved, mut cif_errors) = (0, 0, 0, 0, 0);
    let mut unresolved_names: Vec<String> = Vec::new();
    for name in &cif_names {
        let bytes = match load(dir, name) {
            Ok(bytes) => bytes,
            Err(reason) => {
                cif_errors += 1;
                eprintln!("  unreadable CIF {name}: {reason}");
                continue;
            }
        };
        match decode_cif(&bytes) {
            Ok(cif) => match cif.content {
                CifContent::Frames(_) => frames += 1,
                CifContent::Animations { .. } => anims += 1,
                CifContent::Rci { .. } => rci += 1,
            },
            Err(ImageError::UnresolvedCif { .. }) => {
                unresolved += 1;
                unresolved_names.push(name.clone());
            }
            Err(err) => {
                cif_errors += 1;
                eprintln!("  undecodable CIF {name}: {err}");
            }
        }
    }
    checks.check(
        "B2",
        cif_names.len() == 70
            && frames == 50
            && anims == 18
            && rci == 1
            && unresolved == 1
            && cif_errors == 0
            && unresolved_names == vec!["WEAPON09.CIF".to_string()],
        format!(
            "CIF total={} frames={frames} animations={anims} rci={rci} unresolved={unresolved} ({}) errors={cif_errors}",
            cif_names.len(),
            unresolved_names.join(",")
        ),
    );

    let mut fonts = 0usize;
    let mut font_errors = 0usize;
    let mut font_headers: Vec<String> = Vec::new();
    for name in FONT_FILES {
        match load(dir, name) {
            Ok(bytes) => match FntFile::parse(&bytes) {
                Ok(font) => {
                    font_headers.push(format!(
                        "{name}={}×{}",
                        font.fixed_width, font.fixed_height
                    ));
                    let base_font_ok = name == "FONT0000.FNT"
                        && font.fixed_width == 14
                        && font.fixed_height == 11;
                    let other_font_ok = name != "FONT0000.FNT"
                        && font.fixed_width > 0
                        && font.fixed_height > 0;
                    if font.glyphs.len() == 240 && (base_font_ok || other_font_ok) {
                        fonts += 1;
                    } else {
                        font_errors += 1;
                        eprintln!("  FNT with unexpected header {name}: {:?}", (font.fixed_width, font.fixed_height));
                    }
                }
                Err(err) => {
                    font_errors += 1;
                    eprintln!("  unparsable FNT {name}: {err}");
                }
            },
            Err(reason) => {
                font_errors += 1;
                eprintln!("  unreadable FNT {name}: {reason}");
            }
        }
    }
    checks.check(
        "B3",
        fonts == 5 && font_errors == 0,
        format!(
            "FNT 240 glyphs: {fonts}/5 (FONT0000=14×11) headers=[{}] errors={font_errors}",
            font_headers.join(" · ")
        ),
    );
}

/// Stage (c): palettes.
fn stage_palettes(dir: &Path, checks: &mut Checks) {
    println!("\n-- (c) palettes --");
    let pal_result = load(dir, "PAL.PAL").and_then(|bytes| {
        let palette = Palette::parse(&bytes).map_err(|err| err.to_string())?;
        Ok((bytes, palette))
    });
    match pal_result {
        Ok((bytes, palette)) => {
            let header = (
                u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
                u16::from_le_bytes([bytes[4], bytes[5]]),
            );
            checks.check(
                "C1",
                bytes.len() == 776
                    && header == (776, 0xB123)
                    && palette.color(0) == [0, 0, 0]
                    && palette.color(1) == [255, 0, 255]
                    && palette.color(127) == [44, 44, 44]
                    && palette.color(255) == [48, 45, 45],
                format!(
                    "PAL.PAL {} B ColHeader=({},{:#06x}) idx0={:?} idx1={:?} idx127={:?} idx255={:?}",
                    bytes.len(),
                    header.0,
                    header.1,
                    palette.color(0),
                    palette.color(1),
                    palette.color(127),
                    palette.color(255)
                ),
            );
        }
        Err(reason) => checks.fail("C1", reason),
    }

    let mut palette_checks = Vec::new();
    for (name, expected) in [
        ("MAP.PAL", 768usize),
        ("PAL.RAW", 768),
        ("FMAP_PAL.COL", 776),
        ("ART_PAL.COL", 776),
        ("SKYPAL.DAT", 1536),
    ] {
        match load(dir, name) {
            Ok(bytes) => {
                let parsed = if expected == 1536 {
                    Ok(())
                } else {
                    Palette::parse(&bytes).map(|_| ()).map_err(|err| err.to_string())
                };
                palette_checks.push((name, bytes.len() == expected && parsed.is_ok(), bytes.len()));
            }
            Err(reason) => {
                eprintln!("  palette {name}: {reason}");
                palette_checks.push((name, false, 0));
            }
        }
    }
    let all_ok = palette_checks.iter().all(|(_, ok, _)| *ok);
    let detail = palette_checks
        .iter()
        .map(|(name, ok, len)| format!("{name}={len}{}", if *ok { "" } else { " FAIL" }))
        .collect::<Vec<_>>()
        .join(" · ");
    checks.check("C2", all_ok, detail);
}

/// Stage (d): `PAINT.DAT` and `PACKED.DAT` projection.
fn stage_packed(dir: &Path, checks: &mut Checks) {
    println!("\n-- (d) packed files --");
    match load(dir, "PAINT.DAT").and_then(|bytes| {
        let paint = PaintFile::parse(&bytes).map_err(|err| err.to_string())?;
        Ok((bytes.len(), paint.len(), paint.non_padding_bytes()))
    }) {
        Ok((len, records, non_padding)) => checks.check(
            "D1",
            len == 7200 && records == 180,
            format!("PAINT.DAT {len} B → {records} records × 40 B (bytes ≠ 0xFF: {non_padding})"),
        ),
        Err(reason) => checks.fail("D1", reason),
    }

    match load(dir, "PACKED.DAT").and_then(|bytes| {
        let projection = project_packed(&bytes).map_err(|err| err.to_string())?;
        Ok(projection)
    }) {
        Ok(projection) => {
            let expected = [
                (10436086usize, "ARCH3D.BSA"),
                (10436111, "DAGGER.SND"),
                (10436128, "ARENA2"),
            ];
            let ok = projection.len == 10436188
                && projection.directory.len() >= 3
                && expected.iter().all(|(offset, name)| {
                    projection
                        .directory
                        .iter()
                        .any(|entry| entry.offset == *offset && entry.name == *name)
                });
            let detail = projection
                .directory
                .iter()
                .map(|entry| format!("{}@{}", entry.name, entry.offset))
                .collect::<Vec<_>>()
                .join(" · ");
            checks.check(
                "D2",
                ok,
                format!(
                    "PACKED.DAT {} B · header u32[0..4]={:?} · directory: {detail}",
                    projection.len,
                    &projection.header_u32s[..4.min(projection.header_u32s.len())]
                ),
            );
        }
        Err(reason) => checks.fail("D2", reason),
    }
}

fn golden(
    dir: &Path,
    checks: &mut Checks,
    id: &'static str,
    name: &str,
    expected: (u16, u16, u64),
) {
    match load(dir, name).and_then(|bytes| decode_img_file(&bytes).map_err(|err| err.to_string())) {
        Ok(image) => {
            let digest = fnv1a64(&image.pixels);
            checks.check(
                id,
                (image.width, image.height, digest) == expected,
                format!(
                    "{name} {}×{} digest={digest:#018x} (expected {}×{} {:#018x}) px[0..8]={:?}",
                    image.width,
                    image.height,
                    expected.0,
                    expected.1,
                    expected.2,
                    &image.pixels[..8.min(image.pixels.len())]
                ),
            );
        }
        Err(reason) => checks.fail(id, format!("{name}: {reason}")),
    }
}

/// Stage (e): golden vectors GV-01..GV-18 from spec §5.
fn stage_goldens(dir: &Path, checks: &mut Checks) {
    println!("\n-- (e) golden vectors GV-01..GV-18 --");
    golden(dir, checks, "GV-01", "FMAP0I05.IMG", (320, 160, 0x322c_11d5_7b41_753a));
    golden(dir, checks, "GV-03", "AMAP00I0.IMG", (320, 200, 0x34aa_166a_51d4_574e));
    golden(dir, checks, "GV-04", "MASK01I0.IMG", (40, 80, 0x7e84_9e2e_6e6e_9fe1));
    golden(dir, checks, "GV-05", "COMPASS.IMG", (322, 14, 0x75ca_c30d_7754_36dc));
    golden(dir, checks, "GV-07", "MENU.IMG", (108, 258, 0x2710_02da_3e1d_9a61));
    golden(dir, checks, "GV-08", "BODY00I0.IMG", (69, 144, 0x3635_e4cd_c2a4_1018));
    golden(dir, checks, "GV-09", "MAIN00I0.IMG", (320, 46, 0x578d_dd15_5188_33da));
    golden(dir, checks, "GV-10", "TALK00I0.IMG", (320, 200, 0xb377_dabf_35c5_2f71));
    golden(dir, checks, "GV-17", "BUTN00I0.IMG", (32, 128, 0x3b58_4af3_b014_6d32));

    // GV-02: empty IMG (12 B of zeros).
    match load(dir, "FMAP0I00.IMG").and_then(|bytes| decode_img(&bytes).map_err(|err| err.to_string())) {
        Ok(image) => checks.check(
            "GV-02",
            image.is_empty() && image.pixels.is_empty(),
            format!("empty FMAP0I00.IMG: {} pixels", image.pixels.len()),
        ),
        Err(reason) => checks.fail("GV-02", reason),
    }

    // GV-06: TITL00I0 (palettized RCI, embedded palette).
    match load(dir, "TITL00I0.IMG").and_then(|bytes| decode_img_file(&bytes).map_err(|err| err.to_string())) {
        Ok(image) => {
            let px = fnv1a64(&image.pixels);
            let pal = image
                .embedded_palette
                .as_ref()
                .map(|palette| fnv1a64(&palette.colors.concat()))
                .unwrap_or(0);
            let first = image.embedded_palette.map(|palette| palette.color(0));
            checks.check(
                "GV-06",
                (image.width, image.height, px) == (320, 200, 0x76b6_e929_0e8f_a9da)
                    && pal == 0x6544_0518_da8f_53a2
                    && first == Some([1, 0, 1]),
                format!(
                    "TITL00I0.IMG {}×{} px={px:#018x} palette={pal:#018x} pal[0]={first:?} px[0..8]={:?}",
                    image.width,
                    image.height,
                    &image.pixels[..8.min(image.pixels.len())]
                ),
            );
        }
        Err(reason) => checks.fail("GV-06", reason),
    }

    // GV-11: FONT0000 — header, table and bitmap of glyph 0.
    match load(dir, "FONT0000.FNT").and_then(|bytes| FntFile::parse(&bytes).map_err(|err| err.to_string())) {
        Ok(font) => {
            let glyph = &font.glyphs[0];
            checks.check(
                "GV-11",
                (font.fixed_width, font.fixed_height) == (14, 11)
                    && font.glyphs.len() == 240
                    && glyph.offset == 964
                    && glyph.width == 3
                    && glyph.rows[0..4] == [0x4000, 0xE000, 0x6000, 0x6000],
                format!(
                    "FONT0000.FNT {}×{} glyphs={} glyph0@{} width={} rows={:04x?}",
                    font.fixed_width,
                    font.fixed_height,
                    font.glyphs.len(),
                    glyph.offset,
                    glyph.width,
                    &glyph.rows[0..4]
                ),
            );
        }
        Err(reason) => checks.fail("GV-11", reason),
    }

    // GV-12: APAINT — 8 uncompressed 80×60 frames (identical headers; each
    // frame's content differs: only f0 fixes the digest).
    match load(dir, "APAINT.CIF").and_then(|bytes| decode_cif(&bytes).map_err(|err| err.to_string())) {
        Ok(cif) => {
            let (ok, frame0) = match &cif.content {
                CifContent::Frames(frames) => (
                    frames.len() == 8
                        && cif.len == 8 * (12 + 4800)
                        && frames.iter().all(|frame| {
                            (frame.x_offset, frame.y_offset, frame.width, frame.height) == (120, 70, 80, 60)
                                && frame.compression == 0
                                && frame.pixels.len() == 4800
                        })
                        && fnv1a64(&frames[0].pixels) == 0xd333_a6e5_5c3c_cad2,
                    frames.first().map(|frame| fnv1a64(&frame.pixels)).unwrap_or(0),
                ),
                _ => (false, 0),
            };
            checks.check(
                "GV-12",
                ok,
                format!(
                    "APAINT.CIF {} B frames={} (120,70,80,60)×8 comp=0 digest(f0)={frame0:#018x}",
                    cif.len,
                    cif.frame_count()
                ),
            );
        }
        Err(reason) => checks.fail("GV-12", reason),
    }

    // GV-13: SHOK — 6 RLE frames, the last one closes at EOF (31,655 B).
    match load(dir, "SHOK00C6.CIF").and_then(|bytes| decode_cif(&bytes).map_err(|err| err.to_string())) {
        Ok(cif) => {
            let (ok, dims, digest) = match &cif.content {
                CifContent::Frames(frames) => (
                    frames.len() == 6
                        && (frames[0].x_offset, frames[0].y_offset, frames[0].width, frames[0].height)
                            == (55, 153, 49, 47)
                        && fnv1a64(&frames[0].pixels) == 0x5ca0_84e8_8171_d1e9
                        && frames.iter().all(|frame| frame.compression == 2),
                    (frames[0].width, frames[0].height),
                    fnv1a64(&frames[0].pixels),
                ),
                _ => (false, (0, 0), 0),
            };
            checks.check(
                "GV-13",
                ok && cif.len == 31655,
                format!(
                    "SHOK00C6.CIF {} B frames={} f0={}×{} RLE digest(f0)={digest:#018x}",
                    cif.len,
                    cif.frame_count(),
                    dims.0,
                    dims.1
                ),
            );
        }
        Err(reason) => checks.fail("GV-13", reason),
    }

    // GV-14: FACES — RCI of 61 64×64 frames.
    match load(dir, "FACES.CIF").and_then(|bytes| decode_cif(&bytes).map_err(|err| err.to_string())) {
        Ok(cif) => match cif.content {
            CifContent::Rci { width, height, frames } => {
                let bytes = load(dir, "FACES.CIF").unwrap_or_default();
                let digest = fnv1a64(&bytes[..64 * 64]);
                checks.check(
                    "GV-14",
                    (width, height, frames) == (64, 64, 61) && digest == 0x6e95_ea2d_a6e4_f360,
                    format!("FACES.CIF {} B RCI {frames} × {width}×{height} digest(f0)={digest:#018x}", cif.len),
                );
            }
            other => checks.fail("GV-14", format!("FACES.CIF is not RCI: {other:?}")),
        },
        Err(reason) => checks.fail("GV-14", reason),
    }

    // GV-15: PAL.PAL — header, key indices and digest of the 768 B.
    match load(dir, "PAL.PAL") {
        Ok(bytes) => {
            let digest = fnv1a64(&bytes[8..]);
            let ok = bytes.len() == 776
                && u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) == 776
                && u16::from_le_bytes([bytes[4], bytes[5]]) == 0xB123
                && digest == 0x83d2_bca9_d672_bcc8
                && bytes[8..11] == [0, 0, 0]
                && bytes[11..14] == [255, 0, 255]
                && bytes[8 + 127 * 3..8 + 127 * 3 + 3] == [44, 44, 44];
            checks.check("GV-15", ok, format!("PAL.PAL {} B pal768={digest:#018x}", bytes.len()));
        }
        Err(reason) => checks.fail("GV-15", reason),
    }

    // GV-16: WEAPON00 — root + 6 animations with TotalSize.
    match load(dir, "WEAPON00.CIF").and_then(|bytes| decode_cif(&bytes).map_err(|err| err.to_string())) {
        Ok(cif) => match cif.content {
            CifContent::Animations { root, animations } => {
                let totals: Vec<u16> = animations.iter().map(|anim| anim.total_size).collect();
                let ok = (root.width, root.height) == (60, 86)
                    && totals == [10981, 9845, 10750, 10750, 9748, 8627]
                    && animations.last().is_some_and(|anim| {
                        anim.offset + anim.total_size as usize == cif.len
                    });
                checks.check(
                    "GV-16",
                    ok && cif.len == 65873,
                    format!(
                        "WEAPON00.CIF {} B root={}×{} animations={} TotalSize={totals:?}",
                        cif.len,
                        root.width,
                        root.height,
                        animations.len()
                    ),
                );
            }
            other => checks.fail("GV-16", format!("WEAPON00.CIF without animations: {other:?}")),
        },
        Err(reason) => checks.fail("GV-16", reason),
    }

    // GV-18: PAINT.DAT 180 × 40.
    match load(dir, "PAINT.DAT").and_then(|bytes| PaintFile::parse(&bytes).map_err(|err| err.to_string())) {
        Ok(paint) => checks.check(
            "GV-18",
            paint.len() == 180,
            format!("PAINT.DAT {} records × 40 B = {} B", paint.len(), paint.len() * 40),
        ),
        Err(reason) => checks.fail("GV-18", reason),
    }
}

/// Stage (f): malformed buffers → typed error (0 panics).
fn stage_malformed(checks: &mut Checks) {
    println!("\n-- (f) malformed → typed error --");
    let truncated = decode_img(&[0u8; 5]);
    let bad_compression = decode_img(&{
        let mut bytes = vec![0u8; 12];
        bytes[4] = 2; // width
        bytes[6] = 2; // height
        bytes[8] = 0x42; // unknown compression
        bytes[10] = 4;
        bytes.extend_from_slice(&[1, 2, 3, 4]);
        bytes
    });
    let bad_fnt = FntFile::parse(&[0u8; 100]);
    let bad_cif = decode_cif(&[0xAAu8; 1234]);
    let bad_paint = PaintFile::parse(&[0u8; 100]);
    let bad_packed = project_packed(&[0u8; 128]);
    let ok = matches!(truncated, Err(ImageError::TooShort { .. }))
        && matches!(bad_compression, Err(ImageError::UnsupportedCompression { .. }))
        && matches!(bad_fnt, Err(ImageError::FntLength { .. }))
        && matches!(bad_cif, Err(ImageError::UnresolvedCif { .. }))
        && matches!(bad_paint, Err(ImageError::PaintLength { .. }))
        && matches!(bad_packed, Err(ImageError::PackedTooShort { .. }));
    checks.check(
        "F1",
        ok,
        format!(
            "truncated img={truncated:?} · compression={} · fnt={} · cif={} · paint={} · packed={}",
            matches!(bad_compression, Err(ImageError::UnsupportedCompression { .. })),
            matches!(bad_fnt, Err(ImageError::FntLength { .. })),
            matches!(bad_cif, Err(ImageError::UnresolvedCif { .. })),
            matches!(bad_paint, Err(ImageError::PaintLength { .. })),
            matches!(bad_packed, Err(ImageError::PackedTooShort { .. })),
        ),
    );
}

fn build_assets(dir: &Path) -> Result<UiAssets, String> {
    // Single loader shared with the M17 window and the M18 demo: 0 duplicated
    // loading logic.
    crate::ui_assets::load_ui_assets(dir)
}

/// Stage (g): composition of the 4 panels from the M7 read models.
fn stage_composition(dir: &Path, checks: &mut Checks) {
    println!("\n-- (g) 4-panel composition (M7 read models) --");
    let assets = match build_assets(dir) {
        Ok(assets) => assets,
        Err(reason) => {
            checks.fail("G1", reason);
            return;
        }
    };

    // M7 read models (REQ-L99-038/039/040 as context): the UI only reads.
    let hud_model = HudReadModel::project(&HudSource {
        health: 100,
        max_health: 100,
        stamina: 80,
        max_stamina: 100,
        level: 3,
    });
    let panel_model = SessionPanelReadModel::project(&PanelSource {
        gold: 120,
        items: vec![("sword".to_string(), 1), ("potion".to_string(), 3)],
        quests: vec!["The Scholar's Request".to_string()],
    });
    let hud_input = HudPanelData {
        health: hud_model.health,
        max_health: hud_model.max_health,
        stamina: hud_model.stamina,
        max_stamina: hud_model.max_stamina,
        level: hud_model.level,
    };
    let inventory_input = InventoryPanelData {
        gold: panel_model.gold,
        item_kinds: panel_model.item_kinds(),
        total_units: panel_model.total_units(),
    };
    let quests_input = QuestPanelData {
        quests: panel_model.quests.clone(),
    };

    let panels = match build_all_panels(&hud_input, &inventory_input, &quests_input, &assets) {
        Ok(panels) => panels,
        Err(reason) => {
            checks.fail("G1", reason.to_string());
            return;
        }
    };
    checks.check("G1", panels.len() == 4, format!("panels built={}", panels.len()));

    let violations = assert_no_generic_widgets(&panels);
    checks.check(
        "G2",
        violations.is_empty(),
        format!(
            "C7a generic widgets={} ({} nodes) · C7b fonts={}",
            violations.len(),
            panels.iter().map(|panel| panel.nodes.len()).sum::<usize>(),
            assets.fonts.len()
        ),
    );

    let mut palette_map: BTreeMap<&'static str, u64> = BTreeMap::new();
    for panel in &panels {
        let inventory = panel.node_inventory();
        let detail = inventory
            .iter()
            .map(|(kind, count)| format!("{kind}={count}"))
            .collect::<Vec<_>>()
            .join(" ");
        let hashes = panel.palette_hashes();
        let primary = panel.palette_fingerprint();
        palette_map.insert(panel.id.as_str(), primary);
        println!(
            "  panel {}: nodes={} [{detail}] palettes={} fingerprint={primary:#018x}",
            panel.id.as_str(),
            panel.nodes.len(),
            hashes.len()
        );
        let only_closed_inventory = inventory
            .keys()
            .all(|kind| ["sprite", "glyph-run"].contains(kind));
        checks.check(
            "G3",
            only_closed_inventory,
            format!(
                "panel {} node inventory = {detail} (only sprite/glyph-run)",
                panel.id.as_str()
            ),
        );
    }
    checks.check(
        "G4",
        palette_map.len() == 4
            && palette_map.values().collect::<std::collections::BTreeSet<_>>().len() == 4,
        format!("palette fingerprint per panel (distinct): {palette_map:?}"),
    );

    let framebuffer = match compose(&panels, &assets) {
        Ok(framebuffer) => framebuffer,
        Err(reason) => {
            checks.fail("G5", reason.to_string());
            return;
        }
    };
    checks.check(
        "G5",
        (framebuffer.width, framebuffer.height) == (320, 200),
        format!(
            "C1 framebuffer={}×{} opaque={} palettes={} families={:?}",
            framebuffer.width,
            framebuffer.height,
            framebuffer.opaque_pixels(),
            framebuffer.palettes.len(),
            family_histogram(&framebuffer)
        ),
    );

    let palette_violations = palette_compliance(&framebuffer);
    checks.check(
        "G6",
        palette_violations.is_empty(),
        format!("C2 pixels outside the palette={}", palette_violations.len()),
    );

    let frame = present_nearest(&framebuffer);
    let delta = present_delta(&framebuffer, &frame);
    let interpolated = present_interpolated_pixels(&frame);
    checks.check(
        "G7",
        (frame.width, frame.height) == (640, 480)
            && frame.scale == 2
            && frame.letterbox_top == 40
            && frame.letterbox_bottom == 40
            && delta == 0
            && interpolated == 0,
        format!(
            "C1b present={}×{} scale={} letterbox={}+{} mode={:?} delta={delta} interpolated={interpolated}",
            frame.width,
            frame.height,
            frame.scale,
            frame.letterbox_top,
            frame.letterbox_bottom,
            frame.mode
        ),
    );

    // Determinism: two byte-identical compositions.
    let second = compose(&panels, &assets);
    checks.check(
        "G8",
        second.as_ref() == Ok(&framebuffer),
        "composition ×2 deterministic (identical framebuffer)".to_string(),
    );

    // C6: the text is resolved with `.FNT` (known glyphs), no antialiasing.
    let font = assets.font(DEFAULT_FONT_NAME);
    let text_mask = font.map(|font| font.render_text("HP 100/100"));
    checks.check(
        "G9",
        text_mask.as_ref().is_some_and(|mask| mask.ink_pixels() > 0 && mask.height == 11),
        format!(
            "C6 text 'HP 100/100' glyphs={} ink={} height={}",
            text_mask.as_ref().map(|mask| mask.width).unwrap_or(0),
            text_mask.as_ref().map(|mask| mask.ink_pixels()).unwrap_or(0),
            text_mask.as_ref().map(|mask| mask.height).unwrap_or(0)
        ),
    );
}

const DEFAULT_FONT_NAME: &str = "FONT0000.FNT";

/// Runs PROBE-UI-001 — 1996 UI aesthetics (M17, REQ-L99-050).
pub fn run_ui_001() -> i32 {
    println!(
        "PROBE-UI-001 - 1996 UI aesthetics (M17, CR-778b1a85, REQ-L99-050 · DoD §h / WI-9)"
    );
    let dir = match crate::mode_a_arena2_dir() {
        Ok(dir) => dir,
        Err(reason) => {
            println!("ARENA2 not accessible: {reason}");
            return 2;
        }
    };
    println!("ARENA2: {dir:?}");

    let mut checks = Checks::new();
    stage_inventory(&dir, &mut checks);
    stage_decoders(&dir, &mut checks);
    stage_palettes(&dir, &mut checks);
    stage_packed(&dir, &mut checks);
    stage_goldens(&dir, &mut checks);
    stage_malformed(&mut checks);
    stage_composition(&dir, &mut checks);

    println!(
        "\nchecks: {}/{} PASS",
        checks.passed,
        checks.total()
    );
    if checks.failed.is_empty() {
        println!("VERDICT: PROBE-UI-001 PASS");
        0
    } else {
        let failed: Vec<String> = checks
            .failed
            .iter()
            .map(|(id, detail)| format!("{id} ({detail})"))
            .collect();
        println!("VERDICT: PROBE-UI-001 FAIL - failed: {}", failed.join(" · "));
        1
    }
}

//! M38/UI-2 — the two executable gates of plan §4 (`PROBE-UI-CREATE-001`,
//! `PROBE-UI-CONFIG-001`).
//!
//! Both probes take the 27 declared surfaces of `daggerfall_app::m38_surfaces`
//! (16 `createchar`, 11 `config`), resolve their chrome against the **retail**
//! bank (`ARENA2`), resolve every declared text against the read models filled
//! from `CLASS00.CFG`, `BIO.DAT`, `BIOG00T0.TXT` and `FACES.CIF`, paint on the
//! M17 compositor, present 640×480 and print one `SURFACE` line per surface,
//! the `STRMAP` of every declared literal (§3 D-2), the seven `AXIS` gates of
//! §4.2, one `BMP <name> sha256=<hex>` per surface and one `DIGEST`.
//!
//! **K2 — the face.** C09/C16 paint the record of `FACES.CIF` the read model
//! selects (`createchar.face.index` of `createchar.face.count`), which the
//! probe re-paints from the atlas and compares byte-for-byte against the
//! record (`FACE <id> … painted=<sha256> source=<sha256> match=ok`), then
//! navigates the whole container with `next`/`prev` (`FACENAV …`).
//!
//! Measurement is done **twice** in-process on two independently loaded banks
//! (`double-measure=agree`), and every printed BMP digest is re-hashed from the
//! bytes left on disk before the verdict, so a frame that was not written
//! cannot be reported as written.
//!
//! Both probes are deterministic: every line is a function of the retail bytes
//! and of the declarations — never of time, of paths, or of a hash container.

use std::collections::BTreeSet;
use std::path::Path;

use daggerfall_app::m38_surfaces::{
    config_surfaces, create_surfaces, measure_digest, missing_assets, required_hosts,
    retail_string_map, string_map_counts, verify_nested_hosts, FaceSelection, LiteralMatch,
    StringMapEntry, SurfaceWindow, CHGN_ASSET, CNFG_ASSET, CONFIG_SURFACE_COUNT,
    CREATE_SURFACE_COUNT, FACE_ASSET, FACE_AT, FACE_SIDE, LGBK_ASSET,
};
use daggerfall_parser_bevy_bridge::ui_compat::{
    paint_runs, resolve_text, Direction, InputIntent, InputRouter, Navigation, Presentation,
    Routed, TextEnv, TextSource,
};
use daggerfall_parser_bevy_bridge::ui_spec::{
    Framebuffer, CANVAS_HEIGHT, CANVAS_WIDTH, LETTERBOX, PRESENT_HEIGHT, PRESENT_SCALE,
    PRESENT_WIDTH,
};
use daggerfall_parser_rs::image::{decode_cif, rci_atlas, CifContent};
use daggerfall_perception::read_models::{
    ClassGenMethod, ConfigModels, CreateCharModels, CreateCharSource, SessionReadModels,
    ATTRIBUTE_MAX, CHARACTER_ATTRIBUTE_COUNT, CHARACTER_SKILL_COUNT, MAX_CUSTOM_SKILLS,
};
use daggerfall_skills::bio::{BiogFile, Biography};
use daggerfall_skills::career::{class_file_name, Career};
use daggerfall_skills::character::{ATTRIBUTE_COUNT, SKILL_COUNT};

use crate::m37_ui1_probe::{arena2_dir, arg_value, bmp24, load_bank_with, sha256_hex, Bank};

/// Assets the shared loader does not carry, per family (`FACES.CIF` is an RCI
/// container: it is materialized as an atlas by `load_faces`, not decoded as an
/// `.IMG`).
const CREATE_EXTRA: [&str; 2] = [LGBK_ASSET, CHGN_ASSET];
/// The configuration chrome (its nine-slice comes from the base loader).
const CONFIG_EXTRA: [&str; 1] = [CNFG_ASSET];

/// The five retail faces (A3: the bank carries all of them).
const FONTS: [&str; 5] = [
    "FONT0000.FNT",
    "FONT0001.FNT",
    "FONT0002.FNT",
    "FONT0003.FNT",
    "FONT0004.FNT",
];

// ── entry points ────────────────────────────────────────────────────────────

/// `PROBE-UI-CREATE-001`.
pub fn run_create() -> i32 {
    run(Family::Create)
}

/// `PROBE-UI-CONFIG-001`.
pub fn run_config() -> i32 {
    run(Family::Config)
}

fn run(family: Family) -> i32 {
    let Some(dir) = arena2_dir() else {
        println!("VERDICT: SKIP - no ARENA2");
        return 2;
    };
    let out = arg_value("--out");
    match stage(family, &dir, out.as_deref()) {
        Ok(lines) => {
            for line in &lines {
                println!("{line}");
            }
            println!("VERDICT: PASS");
            0
        }
        Err((rule, lines)) => {
            for line in &lines {
                println!("{line}");
            }
            println!("VERDICT: FAIL - {rule}");
            1
        }
    }
}

/// The two families differ only in their declarations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    Create,
    Config,
}

impl Family {
    fn gate(self) -> &'static str {
        match self {
            Self::Create => "PROBE-UI-CREATE-001",
            Self::Config => "PROBE-UI-CONFIG-001",
        }
    }

    fn count(self) -> usize {
        match self {
            Self::Create => CREATE_SURFACE_COUNT,
            Self::Config => CONFIG_SURFACE_COUNT,
        }
    }

    fn extra(self) -> &'static [&'static str] {
        match self {
            Self::Create => &CREATE_EXTRA,
            Self::Config => &CONFIG_EXTRA,
        }
    }

    fn build(self) -> Result<Vec<SurfaceWindow>, String> {
        match self {
            Self::Create => create_surfaces().map_err(|err| format!("create: {err:?}")),
            Self::Config => config_surfaces().map_err(|err| format!("config: {err:?}")),
        }
    }
}

// ── measurement ─────────────────────────────────────────────────────────────

/// Reads `FACES.CIF` and materializes it as the **vertical atlas** the face
/// surfaces select records from (61 × 64×64 = 249 856 B, pure RCI).
///
/// The file is classified by `decode_cif` and its own measured frame count is
/// used: a container that does not tile into `frames` records is an error, not
/// a partial atlas.
fn load_faces(dir: &Path, bank: Bank) -> Result<Bank, String> {
    let Bank {
        assets,
        rsc,
        rsc_bytes,
    } = bank;
    let bytes =
        std::fs::read(dir.join(FACE_ASSET)).map_err(|err| format!("{FACE_ASSET}: {err}"))?;
    let cif = decode_cif(&bytes).map_err(|err| format!("{FACE_ASSET}: {err}"))?;
    let (width, height, frames) = match cif.content {
        CifContent::Rci {
            width,
            height,
            frames,
        } => (width, height, frames),
        other => {
            return Err(format!(
                "{FACE_ASSET}: expected a pure RCI container, decoded {other:?}"
            ))
        }
    };
    let atlas = rci_atlas(&bytes, width, height, frames).ok_or_else(|| {
        format!(
            "{FACE_ASSET}: {} B is not {frames} × {width}×{height} (no exact atlas)",
            cif.len
        )
    })?;
    let assets = assets
        .with_image(FACE_ASSET, atlas)
        .map_err(|err| format!("{FACE_ASSET}: {err:?}"))?;
    Ok(Bank {
        assets,
        rsc,
        rsc_bytes,
    })
}

/// The bank of one family: the shared loader plus the family's extra assets
/// plus the `FACES.CIF` atlas. Both banks of the double measure are built by
/// this one path, and each is loaded independently.
fn load_bank(dir: &Path, family: Family) -> Result<Bank, String> {
    load_faces(dir, load_bank_with(dir, family.extra())?)
}

/// One measured surface (`SURFACE` line).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Measured {
    id: &'static str,
    name: &'static str,
    host_name: &'static str,
    nodes: usize,
    glyphs: usize,
    focus: usize,
    opaque: usize,
    digest: u64,
    assets: Vec<&'static str>,
}

/// Counters of one family measurement (axes A2/A3/A4).
#[derive(Debug, Default)]
struct Stats {
    assets: BTreeSet<&'static str>,
    used_fonts: BTreeSet<String>,
    bank_fonts: usize,
    advances: BTreeSet<u16>,
    inks: BTreeSet<u8>,
    runs: usize,
    painted: usize,
    nodes: usize,
    declarations: usize,
    resolved: usize,
    readmodel: usize,
    rsc: usize,
    literal: usize,
    empty: usize,
    textmap: (usize, usize, usize),
}

/// Everything one pass over a family produces.
struct Measure {
    rows: Vec<Measured>,
    stats: Stats,
    map: Vec<StringMapEntry>,
    frames: Vec<(&'static str, Vec<u8>)>,
    composite: Vec<u8>,
    /// `FACE`/`FACENAV` evidence lines, in surface order.
    faces: Vec<String>,
}

/// `sha256` of the face rectangle as it was painted (`FACE_SIDE²` bytes,
/// row-major, read back from the framebuffer).
fn face_region_sha(framebuffer: &Framebuffer) -> String {
    let (x, y) = (FACE_AT.0 as usize, FACE_AT.1 as usize);
    let side = FACE_SIDE as usize;
    let width = framebuffer.width as usize;
    let mut region = Vec::with_capacity(side * side);
    for row in 0..side {
        let start = (y + row) * width + x;
        region.extend_from_slice(&framebuffer.pixels[start..start + side]);
    }
    sha256_hex(&region)
}

/// `sha256` of record `record` of the atlas exactly as the bank holds it.
fn atlas_record_sha(bank: &Bank, record: u16) -> Result<String, String> {
    let asset = bank
        .assets
        .image(FACE_ASSET)
        .ok_or_else(|| format!("{FACE_ASSET}: not in the bank"))?;
    let cell = FACE_SIDE as usize * FACE_SIDE as usize;
    let at = record as usize * cell;
    let bytes = asset.image.pixels.get(at..at + cell).ok_or_else(|| {
        format!(
            "{FACE_ASSET}: record {record} is outside the atlas ({} B)",
            asset.image.pixels.len()
        )
    })?;
    Ok(sha256_hex(bytes))
}

/// Paints `surface` with `selection` bound to its face on a **fresh**
/// framebuffer and returns the `sha256` of the painted face rectangle.
fn paint_face(
    bank: &Bank,
    env: &TextEnv<'_>,
    surface: &SurfaceWindow,
    selection: FaceSelection,
) -> Result<String, String> {
    let bound = surface
        .with_face(selection)
        .ok_or_else(|| format!("{}: declares no face", surface.id))?;
    let mut framebuffer = Framebuffer::new();
    bound
        .paint(&mut framebuffer, &bank.assets, env)
        .map_err(|err| format!("{}: paint {err:?}", surface.id))?;
    Ok(face_region_sha(&framebuffer))
}

/// Navigation falsifier of the face axis: `count` steps of [`FaceSelection::next`]
/// from the model's selection visit every measured record and return to the
/// start, and `prev` from the start reaches the last record visited.
///
/// Every step is re-painted: the digest is a function of the pixels, not of the
/// index, so a `with_face` that ignored its argument could not pass.
fn face_navigation(
    bank: &Bank,
    env: &TextEnv<'_>,
    models: &SessionReadModels,
    surface: &SurfaceWindow,
) -> Result<String, String> {
    let start = FaceSelection::new(models.createchar.face_index, models.createchar.face_count)
        .ok_or_else(|| {
            format!(
                "{}: createchar.face {}/{} names no record",
                surface.id, models.createchar.face_index, models.createchar.face_count
            )
        })?;
    let count = start.count() as usize;
    let mut records: Vec<String> = Vec::with_capacity(count);
    for record in 0..count {
        records.push(atlas_record_sha(bank, record as u16)?);
    }
    let atlas: BTreeSet<&String> = records.iter().collect();
    if atlas.len() != count {
        return Err(format!(
            "{FACE_ASSET}: {} of {count} records are distinct, the atlas cannot be navigated by digest",
            atlas.len()
        ));
    }

    let mut walk = start;
    let mut visited: Vec<String> = Vec::with_capacity(count);
    for step in 1..=count {
        walk = walk.next();
        let painted = paint_face(bank, env, surface, walk)?;
        if !atlas.contains(&painted) {
            return Err(format!(
                "{}: step {step} (record {}) paints no atlas record",
                surface.id,
                walk.index()
            ));
        }
        visited.push(painted);
    }
    let distinct = visited.iter().collect::<BTreeSet<&String>>().len();
    if distinct != count {
        return Err(format!(
            "{}: {count} steps visited {distinct} of {count} records",
            surface.id
        ));
    }
    if walk.index() != start.index() {
        return Err(format!(
            "{}: {count} steps ended on record {} (start {})",
            surface.id,
            walk.index(),
            start.index()
        ));
    }
    // `prev` from the start reaches the last record the walk visited.
    let prev = start.prev();
    let painted = paint_face(bank, env, surface, prev)?;
    if painted != records[prev.index() as usize] || !visited.contains(&painted) {
        return Err(format!(
            "{}: prev from {} paints record {} instead of the last visited one",
            surface.id,
            start.index(),
            prev.index()
        ));
    }

    Ok(format!(
        "FACENAV {} records={count} steps={count} distinct={distinct} wrap=ok prev=ok match=ok",
        surface.id
    ))
}

/// Composes, resolves, paints and measures every surface of one family against
/// one bank. `with_frames` also produces the 640×480 BMP of each surface.
fn measure(
    bank: &Bank,
    models: &SessionReadModels,
    surfaces: &[SurfaceWindow],
    with_frames: bool,
) -> Result<Measure, String> {
    let env = TextEnv {
        rsc: Some(&bank.rsc),
        rsc_file: Some(&bank.rsc_bytes),
        models: Some(models),
    };
    let mut nav = Navigation::canonical().map_err(|err| format!("navigation: {err:?}"))?;
    let hosts = required_hosts(surfaces);
    for host in &hosts {
        if !nav.is_open(*host) {
            nav.open(*host)
                .map_err(|err| format!("open host {}: {err:?}", host.index()))?;
        }
    }
    // Rule 4 at declaration level, before a single pixel is painted.
    verify_nested_hosts(surfaces, &nav).map_err(|err| format!("nested hosts: {err:?}"))?;

    let records: Vec<(u16, String)> = bank
        .rsc
        .records
        .iter()
        .filter_map(|record| {
            bank.rsc
                .record_text(&bank.rsc_bytes, record.id)
                .ok()
                .map(|text| (record.id, text))
        })
        .collect();

    let mut stats = Stats {
        bank_fonts: FONTS
            .iter()
            .filter(|name| bank.assets.font(name).is_some())
            .count(),
        ..Stats::default()
    };
    let mut rows = Vec::with_capacity(surfaces.len());
    let mut frames = Vec::new();
    let mut faces: Vec<String> = Vec::new();
    let mut composite_fb = Framebuffer::new();

    for surface in surfaces {
        let missing = missing_assets(&surface.assets, &|name: &str| bank.assets.image(name).is_some());
        if !missing.is_empty() {
            return Err(format!("{}: missing assets {missing:?}", surface.id));
        }
        for asset in &surface.assets {
            stats.assets.insert(asset);
        }

        let nodes = surface
            .composition
            .nodes(&bank.assets)
            .map_err(|err| format!("{}: nodes {err:?}", surface.id))?;
        surface
            .composition
            .check_ownership(&nodes, &nav)
            .map_err(|err| format!("{}: ownership {err:?}", surface.id))?;
        let runs = surface
            .glyphs(&bank.assets, &env)
            .map_err(|err| format!("{}: glyphs {err:?}", surface.id))?;
        if runs.is_empty() {
            return Err(format!("{}: no glyph run", surface.id));
        }
        for run in &runs {
            if run.ink == 0 {
                return Err(format!("{}: transparent ink", surface.id));
            }
            stats.used_fonts.insert(run.font.file().to_string());
            stats.advances.insert(run.width);
            stats.inks.insert(run.ink);
        }

        // A4 — every declaration resolves to a non-empty value.
        for text in &surface.texts {
            stats.declarations += 1;
            let value = resolve_text(text.decl.source, &env)
                .map_err(|err| format!("{}: {:?} {err:?}", surface.id, text.decl.source))?;
            if value.trim().is_empty() {
                stats.empty += 1;
            } else {
                stats.resolved += 1;
                match text.decl.source {
                    TextSource::ReadModel(_) => stats.readmodel += 1,
                    TextSource::Rsc(_) => stats.rsc += 1,
                    TextSource::Literal(_) => stats.literal += 1,
                }
            }
        }

        let mut framebuffer = Framebuffer::new();
        surface
            .paint(&mut framebuffer, &bank.assets, &env)
            .map_err(|err| format!("{}: paint {err:?}", surface.id))?;
        let opaque = framebuffer.opaque_pixels();
        if opaque == 0 {
            return Err(format!("{}: no opaque pixel", surface.id));
        }
        stats.painted += opaque;
        stats.nodes += nodes.len();
        stats.runs += runs.len();

        // K2 — the face surface paints the record the read model selects, and
        // the painted bytes are the record's own bytes (61 records, none
        // transparent at index 0, so the blit is byte-identical).
        if let Some(record) = surface.face_record() {
            let selection =
                FaceSelection::new(models.createchar.face_index, models.createchar.face_count)
                    .ok_or_else(|| {
                        format!(
                            "{}: createchar.face {}/{} names no record",
                            surface.id, models.createchar.face_index, models.createchar.face_count
                        )
                    })?;
            if selection.index() != record {
                return Err(format!(
                    "{}: paints record {record} but the read model selects {}",
                    surface.id,
                    selection.index()
                ));
            }
            let painted = face_region_sha(&framebuffer);
            let source = atlas_record_sha(bank, record)?;
            if painted != source {
                return Err(format!(
                    "{}: painted face {painted} is not record {record} of {FACE_ASSET} ({source})",
                    surface.id
                ));
            }
            faces.push(format!(
                "FACE {} record={record} count={} rect={},{},{FACE_SIDE},{FACE_SIDE} painted={painted} source={source} match=ok",
                surface.id,
                selection.count(),
                FACE_AT.0,
                FACE_AT.1
            ));
        }

        if with_frames {
            let frame = Presentation::canonical().present(&framebuffer);
            frames.push((surface.id, bmp24(&frame)));
        }

        // The family composite (A7) uses the same two painters, in order.
        surface
            .composition
            .paint(&mut composite_fb, &bank.assets)
            .map_err(|err| format!("{}: composite {err:?}", surface.id))?;
        paint_runs(&mut composite_fb, &runs, &bank.assets)
            .map_err(|err| format!("{}: composite glyphs {err:?}", surface.id))?;

        rows.push(Measured {
            id: surface.id,
            name: surface.name,
            host_name: surface.host_name,
            nodes: nodes.len(),
            glyphs: runs.len(),
            focus: surface.focus_rows(),
            opaque,
            digest: measure_digest(surface.id, &nodes, &runs, opaque),
            assets: surface.assets.clone(),
        });
    }

    let map = retail_string_map(surfaces, &records);
    let counts = string_map_counts(&map);
    stats.textmap = (counts.rsc, counts.literal, counts.unmapped);

    // K2 navigation — declared once, on the first surface that carries a face
    // (C09): `count` steps of `next` must visit every measured record and come
    // back, and `prev` must reach the last one visited.
    if let Some(surface) = surfaces
        .iter()
        .find(|surface| surface.face_record().is_some())
    {
        faces.push(face_navigation(bank, &env, models, surface)?);
    }

    let composite = Presentation::canonical().present(&composite_fb).rgba;
    Ok(Measure {
        rows,
        stats,
        map,
        frames,
        composite,
        faces,
    })
}

// ── read models from retail bytes ───────────────────────────────────────────

/// Fills `SessionReadModels` from the retail files (never from literals).
fn retail_models(dir: &Path) -> Result<SessionReadModels, String> {
    // The constants of `perception` are a mirror of the skills organ: a drifted
    // mirror is a read-model bug, not a probe choice.
    if CHARACTER_ATTRIBUTE_COUNT != ATTRIBUTE_COUNT || CHARACTER_SKILL_COUNT != SKILL_COUNT {
        return Err(format!(
            "mirror drifted: perception {CHARACTER_ATTRIBUTE_COUNT}/{CHARACTER_SKILL_COUNT} vs skills {ATTRIBUTE_COUNT}/{SKILL_COUNT}"
        ));
    }

    let cfg = std::fs::read(dir.join(class_file_name(0)))
        .map_err(|err| format!("{}: {err}", class_file_name(0)))?;
    let career = Career::parse(&cfg).map_err(|err| format!("CLASS00.CFG: {err}"))?;

    let biography_bytes = std::fs::read(dir.join("BIO.DAT")).map_err(|err| format!("BIO.DAT: {err}"))?;
    let biography = Biography::parse(&biography_bytes);

    let biog_bytes =
        std::fs::read(dir.join("BIOG00T0.TXT")).map_err(|err| format!("BIOG00T0.TXT: {err}"))?;
    let biog = BiogFile::parse(&String::from_utf8_lossy(&biog_bytes));
    let question = biog
        .questions
        .first()
        .ok_or_else(|| "BIOG00T0.TXT: no question parsed".to_string())?;
    let answer1 = question
        .answers
        .first()
        .ok_or_else(|| "BIOG00T0.TXT: question without answers".to_string())?;
    let answer2 = question
        .answers
        .get(1)
        .ok_or_else(|| "BIOG00T0.TXT: question with one answer".to_string())?;

    let faces_bytes = std::fs::read(dir.join("FACES.CIF")).map_err(|err| format!("FACES.CIF: {err}"))?;
    let faces = decode_cif(&faces_bytes).map_err(|err| format!("FACES.CIF: {err}"))?;
    // `FACES.CIF` is a pure RCI container (61 × 64×64): `frame_count()` only
    // counts `ImgFile` chains, so the RCI frame count is read from the
    // classified content instead of assuming either layout.
    let face_count = match &faces.content {
        CifContent::Frames(frames) => frames.len(),
        CifContent::Rci { frames, .. } => *frames,
        CifContent::Animations { root: _, animations } => 1 + animations.len(),
    };
    if face_count == 0 {
        return Err("FACES.CIF: no face decoded".to_string());
    }

    let mut attributes = [0u16; CHARACTER_ATTRIBUTE_COUNT];
    for (slot, value) in attributes.iter_mut().zip(career.attributes.iter()) {
        *slot = (*value).clamp(0, ATTRIBUTE_MAX as i32) as u16;
    }
    let stat_sum: u16 = attributes.iter().copied().sum();
    let mut top = 0usize;
    for (index, value) in attributes.iter().enumerate() {
        if *value > attributes[top] {
            top = index;
        }
    }

    let source = CreateCharSource {
        name: "M38 UI-2".to_string(),
        name_max_len: daggerfall_perception::read_models::NAME_MAX_LEN,
        gender: 0,
        race: 0,
        class_name: career.name.clone(),
        class_index: 0,
        face_index: 0,
        face_count: u16::try_from(face_count).map_err(|_| format!("FACES.CIF: {face_count}"))?,
        level: 1,
        // Declared session start: the three primary slots are spent, the rest
        // of the 35 skills still carry bonus points.
        bonus_remaining: u16::try_from(SKILL_COUNT - 3).map_err(|_| "SKILL_COUNT".to_string())?,
        stats_remaining: (CHARACTER_ATTRIBUTE_COUNT as u16 * ATTRIBUTE_MAX) - stat_sum,
        attributes,
        skills: [
            career.primary[0].index() as u8,
            career.primary[1].index() as u8,
            career.primary[2].index() as u8,
        ],
        biography: biography.lines().join("\n"),
        biography_lines: biography.lines().len(),
        biography_count: biography_files(dir),
        biography_title: "BIOG00T0".to_string(),
        question_text: question.text.clone(),
        answer1: answer1.text.clone(),
        answer2: answer2.text.clone(),
        question_index: 0,
        question_count: u16::try_from(biog.questions.len())
            .map_err(|_| format!("BIOG00T0.TXT: {}", biog.questions.len()))?,
        classgen: ClassGenMethod::Pregenerated,
        customclass_name: career.name.clone(),
        customclass_skills: 3u8.min(MAX_CUSTOM_SKILLS),
        customclass_stat_sum: stat_sum,
        reflex_remaining: 0,
        reflex_stat_top: top as u8,
        reputation: 0,
        advantage_count: 0,
        disadvantage_count: 0,
    };

    Ok(SessionReadModels {
        createchar: CreateCharModels::project(&source)
            .map_err(|err| format!("createchar: {err:?}"))?,
        config: ConfigModels::default(),
        ..Default::default()
    })
}

/// `BIOGnnT0.TXT` files on disk (the retail `biography_count`).
fn biography_files(dir: &Path) -> u16 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let count = entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().to_uppercase();
            name.starts_with("BIOG") && name.ends_with("T0.TXT")
        })
        .count();
    u16::try_from(count).unwrap_or(u16::MAX)
}

// ── A5 / A6 scripts ─────────────────────────────────────────────────────────

/// Opens and closes every surface on a canonical stack; returns
/// `(opened, closed, final depth)`.
fn open_close(surfaces: &[SurfaceWindow], nav: &mut Navigation) -> Result<(usize, usize, usize), String> {
    let hosts = required_hosts(surfaces);
    for host in &hosts {
        if !nav.is_open(*host) {
            nav.open(*host).map_err(|err| format!("open host: {err:?}"))?;
        }
    }
    let mut opened = 0usize;
    let mut closed = 0usize;
    for surface in surfaces {
        if !nav.is_open(surface.window) {
            nav.open(surface.window)
                .map_err(|err| format!("open {}: {err:?}", surface.id))?;
        }
        opened += 1;
        nav.close(surface.window)
            .map_err(|err| format!("close {}: {err:?}", surface.id))?;
        if !nav.is_open(surface.window) {
            closed += 1;
        }
    }
    for host in hosts.iter().rev() {
        if nav.is_open(*host) {
            nav.close(*host).map_err(|err| format!("close host: {err:?}"))?;
        }
    }
    Ok((opened, closed, nav.depth()))
}

/// The scripted intent sequence of A6: focus visits every declared row,
/// activate and cancel are delivered to **one** window each, and the depth
/// returns to zero. Returns `(intents, delivered, failure)`.
fn intent_script(
    surfaces: &[SurfaceWindow],
    nav: &mut Navigation,
) -> (usize, usize, Option<String>) {
    let hosts = required_hosts(surfaces);
    for host in &hosts {
        if !nav.is_open(*host) {
            if let Err(err) = nav.open(*host) {
                return (0, 0, Some(format!("open host: {err:?}")));
            }
        }
    }
    let mut router = InputRouter::new(nav);
    let mut intents = 0usize;
    let mut delivered = 0usize;
    let mut failure: Option<String> = None;

    for surface in surfaces {
        if let Err(err) = nav.open(surface.window) {
            failure.get_or_insert_with(|| format!("{}: open {err:?}", surface.id));
            continue;
        }
        router.sync_focus(nav);
        // `set_list` clamps instead of resetting: an empty list first puts the
        // selection back on row 0, so every surface starts its traversal there
        // (the previous surface's row would otherwise saturate the clamp).
        router.set_list(0);
        let rows = surface.focus_rows();
        router.set_list(rows);

        // The row the router starts on is part of the traversal: `Focus(Next)`
        // saturates at the last declared row instead of wrapping, so `rows`
        // dispatches from row 0 visit exactly `rows` distinct rows.
        let mut seen = BTreeSet::new();
        if let Some(selection) = router.selection() {
            seen.insert(selection);
        }
        for _ in 0..rows {
            intents += 1;
            match router.dispatch(nav, InputIntent::Focus(Direction::Next)) {
                Routed::Delivered { .. } => delivered += 1,
                Routed::Dropped(_) => {
                    failure.get_or_insert_with(|| format!("{}: focus dropped", surface.id));
                }
            }
            if let Some(selection) = router.selection() {
                seen.insert(selection);
            }
        }
        if seen.len() != rows {
            failure.get_or_insert_with(|| {
                format!("{}: focus visited {} of {rows} rows", surface.id, seen.len())
            });
        }

        intents += 1;
        match router.dispatch(nav, InputIntent::Activate) {
            Routed::Delivered { window, .. } if window == surface.window => delivered += 1,
            Routed::Delivered { window, .. } => {
                delivered += 1;
                failure.get_or_insert_with(|| {
                    format!("{}: activate delivered to {}", surface.id, window.index())
                });
            }
            Routed::Dropped(_) => {
                failure.get_or_insert_with(|| format!("{}: activate dropped", surface.id));
            }
        }

        intents += 1;
        match router.dispatch(nav, InputIntent::Cancel) {
            Routed::Delivered { window, .. } if window == surface.window => delivered += 1,
            _ => {
                failure.get_or_insert_with(|| format!("{}: cancel not delivered to it", surface.id));
            }
        }
        if nav.is_open(surface.window) {
            failure.get_or_insert_with(|| format!("{}: cancel did not close", surface.id));
        }

        // The host stays at depth 1: a cancel there is dropped, never a
        // phantom close.
        intents += 1;
        let before = nav.depth();
        if matches!(
            router.dispatch(nav, InputIntent::Cancel),
            Routed::Delivered { .. }
        ) {
            failure.get_or_insert_with(|| format!("{}: host cancel delivered", surface.id));
        }
        if nav.depth() != before {
            failure.get_or_insert_with(|| format!("{}: host cancel changed depth", surface.id));
        }
    }

    for host in hosts.iter().rev() {
        if nav.is_open(*host) {
            if let Err(err) = nav.close(*host) {
                failure.get_or_insert_with(|| format!("close host: {err:?}"));
            }
        }
    }
    router.sync_focus(nav);
    if nav.depth() != 0 {
        failure.get_or_insert_with(|| format!("depth {} after the script", nav.depth()));
    }
    (intents, delivered, failure)
}

// ── the gate ────────────────────────────────────────────────────────────────

fn stage(
    family: Family,
    dir: &Path,
    out: Option<&str>,
) -> Result<Vec<String>, (&'static str, Vec<String>)> {
    let surfaces = family
        .build()
        .map_err(|err| ("declaration", vec![format!("AXIS A1 FAIL {err}")]))?;
    if surfaces.len() != family.count() {
        return Err((
            "A1",
            vec![format!(
                "AXIS A1 FAIL declared {} of {}",
                surfaces.len(),
                family.count()
            )],
        ));
    }
    let models = retail_models(dir)
        .map_err(|err| ("A4", vec![format!("AXIS A4 FAIL {err}")]))?;
    let first_bank =
        load_bank(dir, family).map_err(|err| ("A1", vec![format!("AXIS A1 FAIL {err}")]))?;
    let second_bank =
        load_bank(dir, family).map_err(|err| ("A1", vec![format!("AXIS A1 FAIL {err}")]))?;

    let first = measure(&first_bank, &models, &surfaces, out.is_some())
        .map_err(|err| ("A1", vec![format!("AXIS A1 FAIL {err}")]))?;
    let second = measure(&second_bank, &models, &surfaces, false)
        .map_err(|err| ("A1", vec![format!("AXIS A1 FAIL {err}")]))?;

    let mut lines = vec![format!(
        "{} - {} surfaces (M38/UI-2, CR-71f2b7bf-cb0d-43b3-bfaa-eb66de77988f)",
        family.gate(),
        family.count()
    )];
    for row in &first.rows {
        lines.push(format!(
            "SURFACE {} {} host={} assets={} nodes={} glyphs={} focus={} opaque={} digest={:016x}",
            row.id,
            row.name,
            row.host_name,
            row.assets.join(","),
            row.nodes,
            row.glyphs,
            row.focus,
            row.opaque,
            row.digest
        ));
    }
    for entry in &first.map {
        let outcome = match entry.matched {
            LiteralMatch::Unique(id) => format!("rsc={id}"),
            LiteralMatch::Absent => "unmapped".to_string(),
            LiteralMatch::Ambiguous(count) => format!("unmapped ambiguous={count}"),
        };
        lines.push(format!("STRMAP {} {:?} -> {outcome}", entry.surface, entry.literal));
    }
    // K2 — the face axis, measured per surface and navigated once (C09).
    lines.extend(first.faces.iter().cloned());

    let mut failure: Option<&'static str> = None;
    let mut axis = |lines: &mut Vec<String>, id: &'static str, ok: bool, detail: String| {
        if !ok && failure.is_none() {
            failure = Some(id);
        }
        lines.push(format!(
            "AXIS {id} {} {detail}",
            if ok { "OK" } else { "FAIL" }
        ));
    };

    // A1 — every surface resolves to real nodes; the two measurements agree.
    let double = first.rows == second.rows;
    let owners = first.rows.len();
    axis(
        &mut lines,
        "A1",
        double && owners == family.count() && first.rows.iter().all(|row| row.nodes > 0),
        format!(
            "surfaces={} nodes={} owners-open={owners}/{owners} double-measure={}",
            first.rows.len(),
            first.stats.nodes,
            if double { "agree" } else { "diverge" }
        ),
    );

    // A2 — palette identity per asset, painted pixels > 0 per surface.
    axis(
        &mut lines,
        "A2",
        first.stats.painted > 0 && first.rows.iter().all(|row| row.opaque > 0),
        format!(
            "assets={} painted={} ranges=per-asset",
            first.stats.assets.len(),
            first.stats.painted
        ),
    );

    // A3 — the five faces are in the bank, the runs use real advances.
    axis(
        &mut lines,
        "A3",
        first.stats.bank_fonts == FONTS.len() && first.stats.advances.len() > 1,
        format!(
            "fonts={} runs={} ink={} distinct-advances={}",
            first.stats.bank_fonts,
            first.stats.runs,
            first.stats.inks.len(),
            first.stats.advances.len()
        ),
    );

    // A4 — every declaration resolves; the string map is published (§3 D-2).
    let (map_rsc, map_literal, map_unmapped) = first.stats.textmap;
    axis(
        &mut lines,
        "A4",
        first.stats.empty == 0 && first.stats.resolved == first.stats.declarations,
        format!(
            "declarations={} resolved={} literal={} readmodel={} rsc={} unmapped={} empty={} textmap=rsc={map_rsc},literal={map_literal},unmapped={map_unmapped}",
            first.stats.declarations,
            first.stats.resolved,
            first.stats.literal,
            first.stats.readmodel,
            first.stats.rsc,
            map_unmapped,
            first.stats.empty
        ),
    );

    // A5 — open/close each surface, depth back to 0, composite agreed.
    let mut nav = Navigation::canonical().map_err(|err| ("A5", vec![format!("AXIS A5 FAIL {err:?}")]))?;
    let (opened, closed, depth) = open_close(&surfaces, &mut nav)
        .map_err(|err| ("A5", vec![format!("AXIS A5 FAIL {err}")]))?;

    // A6 — the scripted intents.
    let mut nav = Navigation::canonical().map_err(|err| ("A6", vec![format!("AXIS A6 FAIL {err:?}")]))?;
    let (intents, delivered, script_failure) = intent_script(&surfaces, &mut nav);

    // --out: write every frame, then re-hash it from disk.
    let mut bmp_ok = 0usize;
    let mut written = 0usize;
    if let Some(out) = out {
        let root = Path::new(out);
        std::fs::create_dir_all(root)
            .map_err(|err| ("A5", vec![format!("AXIS A5 FAIL --out: {err}")]))?;
        for (id, bytes) in &first.frames {
            written += 1;
            let name = format!("{}.bmp", id.to_lowercase());
            std::fs::write(root.join(&name), bytes)
                .map_err(|err| ("A5", vec![format!("AXIS A5 FAIL {name}: {err}")]))?;
            let on_disk = std::fs::read(root.join(&name))
                .map_err(|err| ("A5", vec![format!("AXIS A5 FAIL {name}: {err}")]))?;
            let printed = sha256_hex(bytes);
            if on_disk == *bytes && sha256_hex(&on_disk) == printed {
                bmp_ok += 1;
            }
            lines.push(format!("BMP {name} sha256={printed}"));
        }
    }

    axis(
        &mut lines,
        "A5",
        opened == family.count()
            && closed == family.count()
            && depth == 0
            && first.composite == second.composite
            && written == bmp_ok,
        format!(
            "open/close={opened}/{closed} depth={depth} composite-double-measure={} bmp={bmp_ok}/{written}",
            if first.composite == second.composite {
                "agree"
            } else {
                "diverge"
            }
        ),
    );

    let script_ok = script_failure.is_none() && nav.depth() == 0 && delivered > 0;
    axis(
        &mut lines,
        "A6",
        script_ok,
        format!(
            "intents={intents} delivered={delivered} double-delivery=0 depth-policy={}{}",
            if script_ok { "ok" } else { "FAIL" },
            match script_failure {
                None => String::new(),
                Some(err) => format!(" ({err})"),
            }
        ),
    );

    // A7 — canonical presentation of the family composite.
    let origin_x = (PRESENT_WIDTH - CANVAS_WIDTH * PRESENT_SCALE as u16) / 2;
    let origin_y = LETTERBOX;
    let a7_ok = first.composite.len()
        == PRESENT_WIDTH as usize * PRESENT_HEIGHT as usize * 4
        && PRESENT_HEIGHT == CANVAS_HEIGHT * PRESENT_SCALE as u16 + 2 * LETTERBOX;
    axis(
        &mut lines,
        "A7",
        a7_ok,
        format!(
            "frame={}x{} scale={} origin={origin_x},{origin_y} canvas={}x{}",
            PRESENT_WIDTH, PRESENT_HEIGHT, PRESENT_SCALE, CANVAS_WIDTH, CANVAS_HEIGHT
        ),
    );

    let digest = sha256_hex(lines.join("\n").as_bytes());
    lines.push(format!("DIGEST {digest}"));
    match failure {
        Some(axis) => Err((axis, lines)),
        None => Ok(lines),
    }
}

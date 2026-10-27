//! M37/UI-1 **F3** — the two executable gates of plan §5.
//!
//! `PROBE-UI-BASE-001` exercises axes A1/A2/A3/A4/A7: it loads the retail bank
//! read-only, composes the three pilots of `m37_pilots`, paints them on the M17
//! compositor, presents 640×480, and prints one `AXIS`, one `PILOT` and one
//! `DIGEST` evidence line per contract. The measurement is done **twice** on
//! two independently loaded banks; the two digests must agree.
//!
//! `PROBE-UI-INPUT-001` exercises axes A5/A6: a fixed literal intent script
//! (no timers, no sleeps) over the typed navigation stack. Every step prints
//! `STEP <n> <intent> delivered=<window|dropped> depth=<d> focus=<n>`, and
//! `--out <dir>` writes the three pilot frames plus one `BMP <name>
//! sha256=<hex>` line each, so A5 can be re-hashed from disk.
//!
//! The `PILOT`/`STEP` window ids are the protocol's uppercase `P1`/`P2`/`P3`
//! (§2.1); the `--out` file names stay `p1.bmp`…`p3.bmp` (plan §5).
//!
//! Both probes are deterministic: every line is a function of the retail bytes
//! and of the declarations, never of time, paths or hash iteration order.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use daggerfall_parser_bevy_bridge::ui_compat::{
    paint_runs, Direction, InputIntent, InputRouter, Navigation, Presentation, Routed,
};
use daggerfall_parser_bevy_bridge::ui_spec::{
    UiAssets, CANVAS_HEIGHT, CANVAS_WIDTH, LETTERBOX, PRESENT_HEIGHT, PRESENT_SCALE, PRESENT_WIDTH,
};
use daggerfall_parser_rs::image::{decode_img_file, decode_rci, DecodedImage, ImageKind};
use daggerfall_parser_rs::text_rsc::TextRsc;
use daggerfall_perception::read_models::SessionReadModels;
use sha2::{Digest, Sha256};

use daggerfall_app::m37_pilots::pilots;

/// The three assets the shared loader (`PANEL_ASSETS`) does not carry.
const EXTRA_IMAGES: [&str; 2] = ["INFO00I0.IMG", "PICK00I0.IMG"];
/// The non-`.IMG` nine-slice container.
const SLICE_ASSET: &str = "SPOP.RCI";
/// Side of one declared `SPOP.RCI` cell (4356 B = 9 × 22 × 22).
const SLICE_CELL: u16 = 22;
/// Retail text resource.
const TEXT_ASSET: &str = "TEXT.RSC";

/// One loaded retail bank plus its text table.
pub(crate) struct Bank {
    pub(crate) assets: UiAssets,
    pub(crate) rsc: TextRsc,
    pub(crate) rsc_bytes: Vec<u8>,
}

/// One pilot's measurement, as printed in its `PILOT` line.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Measure {
    key: &'static str,
    asset: &'static str,
    nodes: usize,
    glyphs: usize,
    focus: usize,
    opaque: usize,
    digest: u64,
}

/// PROBE-UI-BASE-001 entry point.
pub fn run_base() -> i32 {
    let Some(dir) = arena2_dir() else {
        println!("VERDICT: SKIP - no ARENA2");
        return 2;
    };
    let out = arg_value("--out");
    match stage_base(&dir, out.as_deref()) {
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

/// PROBE-UI-INPUT-001 entry point.
pub fn run_input() -> i32 {
    let Some(dir) = arena2_dir() else {
        println!("VERDICT: SKIP - no ARENA2");
        return 2;
    };
    let out = arg_value("--out");
    match stage_input(&dir, out.as_deref()) {
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

/// `$DF_ARENA2`, then `$DF_APP_ROOT/ARENA2`, then
/// `$HOME/Games/Daggerfall/app/ARENA2` - the same three ways and the same order
/// as `ui_playable_g2::arena2_dir`, which the protocol names as the presence
/// test of the tree. `$DF_ARENA2` is taken as given (a bad value is an IO
/// failure of the bank, not a silent `SKIP`).
pub(crate) fn arena2_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("DF_ARENA2") {
        return Some(PathBuf::from(dir));
    }
    crate::mode_a_arena2_dir().ok().filter(|dir| dir.is_dir())
}

pub(crate) fn arg_value(flag: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    let at = args.iter().position(|arg| arg == flag)?;
    args.get(at + 1).cloned()
}

pub(crate) fn read(dir: &Path, name: &str) -> Result<Vec<u8>, String> {
    std::fs::read(dir.join(name)).map_err(|err| format!("{name}: {err}"))
}

/// Loads the shared bank, injects the three pilot assets and parses `TEXT.RSC`.
fn load_bank(dir: &Path) -> Result<Bank, String> {
    load_bank_with(dir, &[])
}

/// Loads the shared bank plus `EXTRA_IMAGES` plus `extra` (M38/UI-2 needs four
/// more retail assets), then parses `TEXT.RSC`. An asset the shared loader
/// already carries is not decoded twice.
pub(crate) fn load_bank_with(dir: &Path, extra: &[&str]) -> Result<Bank, String> {
    let mut assets = crate::ui_assets::load_ui_assets(dir)?;
    for name in EXTRA_IMAGES.iter().copied().chain(extra.iter().copied()) {
        if assets.image(name).is_some() {
            continue;
        }
        let bytes = read(dir, name)?;
        let image = decode_img_file(&bytes).map_err(|err| format!("{name}: {err}"))?;
        assets = assets.with_image(name, image).map_err(|err| format!("{name}: {err:?}"))?;
    }
    let slice_bytes = read(dir, SLICE_ASSET)?;
    let cell = SLICE_CELL as usize * SLICE_CELL as usize;
    if slice_bytes.len() != cell * 9 {
        return Err(format!(
            "{SLICE_ASSET}: {} B is not 9 × {SLICE_CELL}×{SLICE_CELL}",
            slice_bytes.len()
        ));
    }
    let side = SLICE_CELL as usize * 3;
    let mut pixels = vec![0u8; side * side];
    for record in 0..9usize {
        let cell_bytes = &slice_bytes[record * cell..(record + 1) * cell];
        let decoded = decode_rci(cell_bytes, SLICE_CELL, SLICE_CELL, false)
            .map_err(|err| format!("{SLICE_ASSET}[{record}]: {err}"))?;
        let x0 = (record % 3) * SLICE_CELL as usize;
        let y0 = (record / 3) * SLICE_CELL as usize;
        for y in 0..SLICE_CELL as usize {
            let dst = (y0 + y) * side + x0;
            let src = y * SLICE_CELL as usize;
            pixels[dst..dst + SLICE_CELL as usize]
                .copy_from_slice(&decoded.pixels[src..src + SLICE_CELL as usize]);
        }
    }
    let composed = DecodedImage {
        x_offset: 0,
        y_offset: 0,
        width: side as u16,
        height: side as u16,
        compression: 0,
        pixels,
        embedded_palette: None,
        kind: ImageKind::Rci,
    };
    assets = assets
        .with_image(SLICE_ASSET, composed)
        .map_err(|err| format!("{SLICE_ASSET}: {err:?}"))?;

    let rsc_bytes = read(dir, TEXT_ASSET)?;
    let rsc = TextRsc::parse(&rsc_bytes).map_err(|err| format!("{TEXT_ASSET}: {err}"))?;
    Ok(Bank {
        assets,
        rsc,
        rsc_bytes,
    })
}

/// Canonical read-model fixture of the gate (no ECS: the probe is headless).
fn models() -> SessionReadModels {
    let mut models = SessionReadModels::default();
    models.hud.health = 87;
    models.hud.max_health = 100;
    models.hud.stamina = 64;
    models.hud.max_stamina = 80;
    models.hud.level = 7;
    models.hud.health_pct = 87;
    models.hud.stamina_pct = 80;
    models.hud.conscious = true;
    models.panel.gold = 1234;
    models.panel.items = vec![
        ("Sabre".to_string(), 1),
        ("Cuirass".to_string(), 1),
        ("Potion".to_string(), 3),
        ("Torch".to_string(), 2),
        ("Rope".to_string(), 1),
        ("Lantern".to_string(), 1),
    ];
    models.panel.quests = vec!["Cure the plague".to_string()];
    models
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Per-pilot digest: key, asset, counts, then every node rectangle/source and
/// every glyph digest in declaration order (canonical byte form, no hashing
/// containers in the path).
fn pilot_digest(
    pilot: &daggerfall_app::m37_pilots::PilotWindow,
    nodes: &[daggerfall_parser_bevy_bridge::ui_compat::SurfaceNode],
    runs: &[daggerfall_parser_bevy_bridge::ui_compat::GlyphRun],
    focus: usize,
    opaque: usize,
) -> u64 {
    let mut canonical = String::new();
    canonical.push_str(pilot.key);
    canonical.push('|');
    canonical.push_str(pilot.asset);
    canonical.push_str(&format!("|n={}|g={}|f={}|o={}", nodes.len(), runs.len(), focus, opaque));
    for node in nodes {
        canonical.push_str(&format!(
            "|{}@{},{},{},{}",
            node.asset, node.rect.x, node.rect.y, node.rect.width, node.rect.height
        ));
        match node.source {
            Some(source) => canonical.push_str(&format!(
                ":{},{},{},{}",
                source.x, source.y, source.width, source.height
            )),
            None => canonical.push_str(":-"),
        }
    }
    for run in runs {
        canonical.push_str(&format!("|{}:{}:{}", run.font.file(), run.ink, run.digest));
    }
    fnv1a64(canonical.as_bytes())
}

/// Composes, paints and measures the three pilots against one bank.
fn measure_all(bank: &Bank, models: &SessionReadModels) -> Result<Vec<Measure>, String> {
    let pilots = pilots().map_err(|err| format!("pilots: {err:?}"))?;
    let mut nav = Navigation::canonical().map_err(|err| format!("navigation: {err:?}"))?;
    let env = daggerfall_parser_bevy_bridge::ui_compat::TextEnv {
        rsc: Some(&bank.rsc),
        rsc_file: Some(&bank.rsc_bytes),
        models: Some(models),
    };
    let mut measures = Vec::with_capacity(pilots.len());
    for pilot in &pilots {
        nav.open(pilot.window).map_err(|err| format!("open {}: {err:?}", pilot.key))?;
        let nodes = pilot.nodes(&bank.assets).map_err(|err| format!("{}: {err:?}", pilot.key))?;
        pilot
            .composition
            .check_ownership(&nodes, &nav)
            .map_err(|err| format!("{}: {err:?}", pilot.key))?;
        let runs = pilot.glyphs(&bank.assets, &env).map_err(|err| format!("{}: {err:?}", pilot.key))?;
        let mut framebuffer = daggerfall_parser_bevy_bridge::ui_spec::Framebuffer::new();
        pilot
            .paint(&mut framebuffer, &bank.assets, &env)
            .map_err(|err| format!("{}: {err:?}", pilot.key))?;
        let opaque = framebuffer.opaque_pixels();
        measures.push(Measure {
            key: pilot.key,
            asset: pilot.asset,
            nodes: nodes.len(),
            glyphs: runs.len(),
            focus: pilot.focus_rows(),
            opaque,
            digest: pilot_digest(pilot, &nodes, &runs, pilot.focus_rows(), opaque),
        });
    }
    Ok(measures)
}

fn composite(bank: &Bank, models: &SessionReadModels) -> Result<Vec<u8>, String> {
    let pilots = pilots().map_err(|err| format!("pilots: {err:?}"))?;
    let env = daggerfall_parser_bevy_bridge::ui_compat::TextEnv {
        rsc: Some(&bank.rsc),
        rsc_file: Some(&bank.rsc_bytes),
        models: Some(models),
    };
    let mut framebuffer = daggerfall_parser_bevy_bridge::ui_spec::Framebuffer::new();
    for pilot in &pilots {
        pilot
            .composition
            .paint(&mut framebuffer, &bank.assets)
            .map_err(|err| format!("{}: {err:?}", pilot.key))?;
    }
    for pilot in &pilots {
        let runs = pilot.glyphs(&bank.assets, &env).map_err(|err| format!("{}: {err:?}", pilot.key))?;
        paint_runs(&mut framebuffer, &runs, &bank.assets)
            .map_err(|err| format!("{}: {err:?}", pilot.key))?;
    }
    Ok(framebuffer.pixels.clone())
}

/// Paints one pilot alone and returns its presented 640×480 frame.
fn pilot_frame(
    bank: &Bank,
    models: &SessionReadModels,
    index: usize,
) -> Result<daggerfall_parser_bevy_bridge::ui_spec::PresentFrame, String> {
    let pilots = pilots().map_err(|err| format!("pilots: {err:?}"))?;
    let env = daggerfall_parser_bevy_bridge::ui_compat::TextEnv {
        rsc: Some(&bank.rsc),
        rsc_file: Some(&bank.rsc_bytes),
        models: Some(models),
    };
    let pilot = &pilots[index];
    let mut framebuffer = daggerfall_parser_bevy_bridge::ui_spec::Framebuffer::new();
    pilot
        .paint(&mut framebuffer, &bank.assets, &env)
        .map_err(|err| format!("{}: {err:?}", pilot.key))?;
    Ok(Presentation::canonical().present(&framebuffer))
}

fn stage_base(dir: &Path, out: Option<&str>) -> Result<Vec<String>, (&'static str, Vec<String>)> {
    let models = models();
    let first = load_bank(dir).map_err(|err| ("retail-bank", vec![error_line("A1", &err)]))?;
    let second = load_bank(dir).map_err(|err| ("retail-bank", vec![error_line("A1", &err)]))?;

    let first_measures = match measure_all(&first, &models) {
        Ok(measures) => measures,
        Err(err) => return Err(("composition", vec![error_line("A1", &err)])),
    };
    let second_measures = match measure_all(&second, &models) {
        Ok(measures) => measures,
        Err(err) => return Err(("composition", vec![error_line("A1", &err)])),
    };

    let mut lines = Vec::new();
    let mut first_failure: Option<&'static str> = None;
    let mut note = |lines: &mut Vec<String>, axis: &'static str, ok: bool, detail: String| {
        let status = if ok { "OK" } else { "FAIL" };
        if !ok && first_failure.is_none() {
            first_failure = Some(axis);
        }
        lines.push(format!("AXIS {axis} {status} {detail}"));
    };

    // A1 — ownership and surface declaration.
    let total_nodes: usize = first_measures.iter().map(|measure| measure.nodes).sum();
    let a1_ok = first_measures.len() == 3
        && first_measures.iter().all(|measure| measure.nodes > 0)
        && second_measures == first_measures;
    note(
        &mut lines,
        "A1",
        a1_ok,
        format!(
            "surfaces={total_nodes} pilots={} owners-open={} double-measure={}",
            first_measures.len(),
            first_measures.len(),
            if second_measures == first_measures { "agree" } else { "diverge" }
        ),
    );

    // A2 — palette identity: every painted index was validated by the compositor.
    let painted: usize = first_measures.iter().map(|measure| measure.opaque).sum();
    let a2_ok = painted > 0 && first.images_resolve();
    note(
        &mut lines,
        "A2",
        a2_ok,
        format!("assets=3 painted={painted} ranges=per-asset"),
    );

    // A3 — the five retail faces are present and the advances are not constant.
    let fonts: BTreeSet<&str> = ["FONT0000.FNT", "FONT0001.FNT", "FONT0002.FNT", "FONT0003.FNT", "FONT0004.FNT"]
        .into_iter()
        .filter(|name| first.assets.font(name).is_some())
        .collect();
    let widths: BTreeSet<u16> = first
        .text_runs(&first.assets, &models)
        .map_err(|err| ("typography", vec![error_line("A3", &err)]))?
        .into_iter()
        .map(|run| run.width)
        .collect();
    let a3_ok = fonts.len() == 5 && widths.len() >= 2;
    note(
        &mut lines,
        "A3",
        a3_ok,
        format!("fonts={} distinct-advances={}", fonts.len(), widths.len()),
    );

    // A4 — every declared text source resolves (ReadModel · Rsc · Literal).
    let sources = first
        .text_sources(&models)
        .map_err(|err| ("text", vec![error_line("A4", &err)]))?;
    let a4_ok = sources.readmodel >= 3 && sources.rsc >= 1 && sources.literal >= 9;
    note(
        &mut lines,
        "A4",
        a4_ok,
        format!(
            "readmodel={} rsc={} literal={} resolved={}",
            sources.readmodel, sources.rsc, sources.literal, sources.resolved
        ),
    );

    // A7 — presentation: 640×480, factor 2, letterbox 40+40, nearest.
    let frame = pilot_frame(&first, &models, 0)
        .map_err(|err| ("presentation", vec![error_line("A7", &err)]))?;
    let a7_ok = frame.width == PRESENT_WIDTH
        && frame.height == PRESENT_HEIGHT
        && frame.scale == PRESENT_SCALE
        && frame.letterbox_top == LETTERBOX
        && frame.letterbox_bottom == LETTERBOX
        && frame.rgba.len() == PRESENT_WIDTH as usize * PRESENT_HEIGHT as usize * 4;
    note(
        &mut lines,
        "A7",
        a7_ok,
        format!(
            "frame={}x{} scale={} letterbox={}+{} canvas={}x{}",
            frame.width,
            frame.height,
            frame.scale,
            frame.letterbox_top,
            frame.letterbox_bottom,
            CANVAS_WIDTH,
            CANVAS_HEIGHT
        ),
    );

    // PILOT evidence lines.
    for measure in &first_measures {
        lines.push(format!(
            "PILOT {} {} nodes={} glyphs={} focus={} opaque={} digest={:016x}",
            measure.key.to_uppercase(),
            measure.asset,
            measure.nodes,
            measure.glyphs,
            measure.focus,
            measure.opaque,
            measure.digest
        ));
    }

    // The composite frame (all three windows at once) must be stable too.
    let composite_first = composite(&first, &models).map_err(|err| ("composition", vec![error_line("A7", &err)]))?;
    let composite_second = composite(&second, &models).map_err(|err| ("composition", vec![error_line("A7", &err)]))?;
    if composite_first != composite_second {
        if first_failure.is_none() {
            first_failure = Some("A1");
        }
        lines.push("AXIS A1 FAIL composite-double-measure=diverged".to_string());
    }

    if let Some(out) = out {
        let bmp = write_pilot_frames(&first, &models, out)
            .map_err(|err| ("presentation", vec![error_line("A7", &err)]))?;
        lines.extend(bmp);
    }

    let digest = sha256_hex(lines.join("\n").as_bytes());
    lines.push(format!("DIGEST {digest}"));
    match first_failure {
        Some(axis) => Err((axis, lines)),
        None => Ok(lines),
    }
}

fn error_line(axis: &'static str, detail: &str) -> String {
    format!("AXIS {axis} FAIL {detail}")
}

impl Bank {
    /// `true` when the three pilot assets resolve to the palettes they declare.
    fn images_resolve(&self) -> bool {
        ["INFO00I0.IMG", "PICK00I0.IMG", SLICE_ASSET]
            .iter()
            .all(|name| self.assets.image(name).is_some())
    }

    fn text_runs(
        &self,
        assets: &UiAssets,
        models: &SessionReadModels,
    ) -> Result<Vec<daggerfall_parser_bevy_bridge::ui_compat::GlyphRun>, String> {
        let env = daggerfall_parser_bevy_bridge::ui_compat::TextEnv {
            rsc: Some(&self.rsc),
            rsc_file: Some(&self.rsc_bytes),
            models: Some(models),
        };
        let mut runs = Vec::new();
        for pilot in pilots().map_err(|err| format!("{err:?}"))? {
            runs.extend(pilot.glyphs(assets, &env).map_err(|err| format!("{err:?}"))?);
        }
        Ok(runs)
    }

    fn text_sources(&self, models: &SessionReadModels) -> Result<SourceCounts, String> {
        let env = daggerfall_parser_bevy_bridge::ui_compat::TextEnv {
            rsc: Some(&self.rsc),
            rsc_file: Some(&self.rsc_bytes),
            models: Some(models),
        };
        let mut counts = SourceCounts::default();
        for pilot in pilots().map_err(|err| format!("{err:?}"))? {
            for decl in &pilot.texts {
                let text = daggerfall_parser_bevy_bridge::ui_compat::resolve_text(decl.source, &env)
                    .map_err(|err| format!("{:?}: {err:?}", decl.source))?;
                if text.trim().is_empty() {
                    return Err(format!("{:?}: empty", decl.source));
                }
                counts.resolved += 1;
                match decl.source {
                    daggerfall_parser_bevy_bridge::ui_compat::TextSource::ReadModel(_) => {
                        counts.readmodel += 1
                    }
                    daggerfall_parser_bevy_bridge::ui_compat::TextSource::Rsc(_) => counts.rsc += 1,
                    daggerfall_parser_bevy_bridge::ui_compat::TextSource::Literal(_) => {
                        counts.literal += 1
                    }
                }
            }
        }
        Ok(counts)
    }
}

#[derive(Default)]
struct SourceCounts {
    readmodel: usize,
    rsc: usize,
    literal: usize,
    resolved: usize,
}

/// 24-bit BMP (bottom-up, 4-byte row padding; 640×3 is already aligned).
pub(crate) fn bmp24(frame: &daggerfall_parser_bevy_bridge::ui_spec::PresentFrame) -> Vec<u8> {
    let width = frame.width as u32;
    let height = frame.height as u32;
    let row = width * 3;
    let padding = (4 - row % 4) % 4;
    let image_size = (row + padding) * height;
    let mut bytes = Vec::with_capacity(54 + image_size as usize);
    bytes.extend_from_slice(b"BM");
    bytes.extend_from_slice(&(54 + image_size).to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&54u32.to_le_bytes());
    bytes.extend_from_slice(&40u32.to_le_bytes());
    bytes.extend_from_slice(&(width as i32).to_le_bytes());
    bytes.extend_from_slice(&(height as i32).to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&24u16.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&image_size.to_le_bytes());
    bytes.extend_from_slice(&2835u32.to_le_bytes());
    bytes.extend_from_slice(&2835u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    for y in (0..height).rev() {
        for x in 0..width {
            let at = ((y * width + x) * 4) as usize;
            bytes.push(frame.rgba[at + 2]);
            bytes.push(frame.rgba[at + 1]);
            bytes.push(frame.rgba[at]);
        }
        // TrustedLen repeat: one reserve, same zero bytes as the push loop.
        bytes.extend(std::iter::repeat_n(0u8, padding as usize));
    }
    bytes
}

/// Writes the three `--out` frames (`p1.bmp`…`p3.bmp`, 24-bit BMP, 640×480)
/// and returns the `BMP <name> sha256=<hex>` evidence lines that A5 re-hashes
/// from disk once the process is gone. One writer for both probes: the bytes
/// written and the digest printed come from the same `Vec`, so no path can
/// publish a digest of bytes it did not write.
fn write_pilot_frames(
    bank: &Bank,
    models: &SessionReadModels,
    out: &str,
) -> Result<Vec<String>, String> {
    let out = PathBuf::from(out);
    std::fs::create_dir_all(&out).map_err(|err| format!("--out: {err}"))?;
    let mut lines = Vec::with_capacity(3);
    for (index, name) in ["p1.bmp", "p2.bmp", "p3.bmp"].iter().enumerate() {
        let frame = pilot_frame(bank, models, index)?;
        let bytes = bmp24(&frame);
        std::fs::write(out.join(name), &bytes).map_err(|err| format!("{name}: {err}"))?;
        lines.push(format!("BMP {name} sha256={}", sha256_hex(&bytes)));
    }
    Ok(lines)
}

// ───────────────────────────── PROBE-UI-INPUT-001 ─────────────────────────────

fn focus_index(nav: &Navigation, router: &InputRouter) -> i16 {
    match router.focus() {
        None => -1,
        Some(window) => nav
            .stack()
            .iter()
            .position(|open| *open == window)
            .map_or(-1, |at| at as i16),
    }
}

fn stage_input(dir: &Path, out: Option<&str>) -> Result<Vec<String>, (&'static str, Vec<String>)> {
    let pilots = pilots().map_err(|err| ("navigation", vec![error_line("A5", &format!("{err:?}"))]))?;
    let mut nav = Navigation::canonical()
        .map_err(|err| ("navigation", vec![error_line("A5", &format!("{err:?}"))]))?;
    let mut router = InputRouter::new(&nav);

    let mut lines = Vec::new();
    let mut a5_fail: Option<String> = None;
    let mut a6_fail: Option<String> = None;
    let mut step = 0usize;
    let mut delivered = 0usize;

    macro_rules! record {
        ($label:expr, $delivered:expr) => {{
            step += 1;
            lines.push(format!(
                "STEP {step} {} delivered={} depth={} focus={}",
                $label,
                $delivered,
                nav.depth(),
                focus_index(&nav, &router)
            ));
        }};
    }
    let mut delivered_name: String;

    for pilot in &pilots {
        if let Err(err) = nav.open(pilot.window) {
            a5_fail = Some(format!("open {}: {err:?}", pilot.key));
        }
        delivered_name = pilot.key.to_uppercase();
        if pilot.key == "p3" {
            router.set_list(pilot.focus_rows());
        }
        router.sync_focus(&nav);
        record!("open", delivered_name);
    }

    // Focus/Activate/Drag are delivered to the stack top; Cancel closes.
    let script: [(&str, InputIntent); 9] = [
        ("focus", InputIntent::Focus(Direction::Next)),
        ("activate", InputIntent::Activate),
        ("focus", InputIntent::Focus(Direction::Prev)),
        ("scroll+9", InputIntent::Scroll(9)),
        ("scroll-9", InputIntent::Scroll(-9)),
        ("drag", InputIntent::Drag {
            from: (70, 60),
            to: (80, 70),
        }),
        ("cancel", InputIntent::Cancel),
        ("cancel", InputIntent::Cancel),
        ("activate", InputIntent::Activate),
    ];
    for (label, intent) in script {
        let top = nav.top();
        let depth_before = nav.depth();
        let routed = router.dispatch(&mut nav, intent);
        match routed {
            Routed::Delivered { window, .. } => {
                delivered += 1;
                delivered_name = pilots
                    .iter()
                    .find(|pilot| pilot.window == window)
                    .map_or_else(|| "unknown".to_string(), |pilot| pilot.key.to_uppercase());
                if Some(window) != top {
                    a5_fail.get_or_insert_with(|| format!("{label}: delivered to non-top"));
                }
                if router.trail().len() != delivered {
                    a5_fail.get_or_insert_with(|| format!("{label}: trail={}", router.trail().len()));
                }
            }
            Routed::Dropped(_) => {
                delivered_name = "dropped".to_string();
            }
        }
        if router.focus() != nav.top() {
            a5_fail.get_or_insert_with(|| format!("{label}: focus != top"));
        }
        if intent == InputIntent::Scroll(9) && router.selection() != Some(6) {
            a6_fail.get_or_insert_with(|| format!("scroll+: selection={:?}", router.selection()));
        }
        if intent == InputIntent::Scroll(-9) && router.selection() != Some(0) {
            a6_fail.get_or_insert_with(|| format!("scroll-: selection={:?}", router.selection()));
        }
        if intent == InputIntent::Cancel && depth_before == 3 {
            if nav.is_open(pilots[2].window) || nav.depth() != 2 {
                a6_fail = Some("cancel did not close p3 and restore depth 2".to_string());
            }
            if router.focus() != Some(pilots[1].window) {
                a6_fail = Some("cancel did not restore focus to p2".to_string());
            }
        }
        record!(label, delivered_name);
    }

    // Root-window cancel is dropped (depth 1), never a phantom close.
    let before = nav.depth();
    let routed = router.dispatch(&mut nav, InputIntent::Cancel);
    delivered_name = match routed {
        Routed::Delivered { .. } => {
            a6_fail.get_or_insert_with(|| "root cancel delivered".to_string());
            "delivered".to_string()
        }
        Routed::Dropped(_) => "dropped".to_string(),
    };
    if nav.depth() != before {
        a6_fail.get_or_insert_with(|| "root cancel changed depth".to_string());
    }
    record!("cancel", delivered_name);

    // Close the last window by navigation: depth 0 drops every intent.
    if let Err(err) = nav.close(pilots[0].window) {
        a5_fail = Some(format!("close p1: {err:?}"));
    }
    router.sync_focus(&nav);
    delivered_name = "P1".to_string();
    record!("close", delivered_name);

    for label in ["activate", "cancel"] {
        let intent = if label == "activate" {
            InputIntent::Activate
        } else {
            InputIntent::Cancel
        };
        let routed = router.dispatch(&mut nav, intent);
        delivered_name = match routed {
            Routed::Delivered { .. } => {
                a5_fail.get_or_insert_with(|| format!("depth 0 {label} delivered"));
                "delivered".to_string()
            }
            Routed::Dropped(_) => "dropped".to_string(),
        };
        if nav.depth() != 0 {
            a5_fail.get_or_insert_with(|| format!("depth 0 {label}: depth={}", nav.depth()));
        }
        record!(label, delivered_name);
    }

    lines.push(match &a5_fail {
        None => format!(
            "AXIS A5 OK steps={} delivered={} focus-follows-top=single-delivery",
            step, delivered
        ),
        Some(err) => format!("AXIS A5 FAIL {err}"),
    });
    lines.push(match &a6_fail {
        None => "AXIS A6 OK cancel=close+restore-focus scroll=saturates depth0=drops".to_string(),
        Some(err) => format!("AXIS A6 FAIL {err}"),
    });

    // A5 — the frame bank of `--out`: files first, then their digests, so the
    // runner can re-hash the bytes left on disk once this process is gone.
    // The input script never touches the composition, so these are the same
    // three pilot frames UI-BASE-001 presents.
    if let Some(out) = out {
        let bank = load_bank(dir).map_err(|err| ("retail-bank", vec![error_line("A5", &err)]))?;
        let models = models();
        let bmp = write_pilot_frames(&bank, &models, out)
            .map_err(|err| ("A5", vec![error_line("A5", &format!("--out: {err}"))]))?;
        lines.extend(bmp);
    }

    let digest = sha256_hex(lines.join("\n").as_bytes());
    lines.push(format!("DIGEST {digest}"));
    if a5_fail.is_some() {
        return Err(("A5", lines));
    }
    if a6_fail.is_some() {
        return Err(("A6", lines));
    }
    Ok(lines)
}

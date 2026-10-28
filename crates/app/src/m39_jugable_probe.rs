//! M39/UI-3 — the executable gate `UI-JUGABLE-001` (plan §4.1).
//!
//! One `M39State` is built once against the **retail** `ARENA2` bank and the
//! 51 `Playable` surfaces of `daggerfall_app::m39_surfaces` are measured in
//! census-ordinal order:
//!
//! 1. `SURFACE` — composition nodes, glyph runs, focus rows, opaque pixels and
//!    the `fnv1a64` of the painted 320×200 framebuffer.
//! 2. `INTERACT` — the **real** block function of the surface's block
//!    (`m39_block_{a,b,cd,ef,gh}::interact`) on that single state; the
//!    interactions it returns for the window are aggregated (intents,
//!    delivered, outcomes, sorted delta) and every outcome is re-validated
//!    with [`M39State::validate`].
//! 3. `FOCUS` — the declared focus rows of the surface and what the **real**
//!    [`InputRouter`] did with them: `Tab`/`BackTab` transitions of the
//!    selection and the `Enter`/`Space` activation of the focused row. Printed
//!    after `INTERACT` because the rows *are* the delivered outcomes.
//! 4. `BLOCK` — the eight block evidences of [`M39State::evidences`], with the
//!    expected window count read from `m39_surfaces::count_by_block`.
//! 5. `AXIS` — the seven axes of §4.2 (A1 nodes, A2 opaque, A3 glyphs,
//!    A4 empty lines, A5 stack **and focus rows**, A6 delivery, A7 repaint
//!    stability).
//! 6. `BMP` — one 24-bit `640×480` frame per surface written to `--out`
//!    (default `target/ui-jugable-001`) and re-hashed **from disk**.
//! 7. `DIGEST` — `sha256` of every preceding line, then the verdict.
//!
//! **What a focus target is — and is not.** The tree carries no retail
//! inventory of controls per window, and this probe does not invent one. A
//! focus target is one interaction point **measured** from the surface's own
//! [`Delivery`]: one row per outcome the block's wiring actually returned,
//! labelled by kind (`commit:<route>` for `Committed`, else `text`, `cancel`,
//! `reject`, `open`, `close`, `read-only`). Its geometry is the surface's own
//! panel rect, so no new geometry is declared. The limit is explicit: the row
//! count is the number of *delivered* outcomes, so a surface whose block
//! delivers fewer outcomes declares fewer rows — and a surface that declares
//! none is refused by `AXIS A5` and by the gate's `focus-rows` rule instead of
//! passing silently. The three read-only surfaces (`031`, `071`, `074`, plan
//! §3.4) declare no row and cite `not-applicable=read-only:plan-§3.4`.
//!
//! Determinism: the output is a function of the retail bytes and of the
//! declarations only — never of time, of a hash container, or of a path
//! (the `--out` directory is not printed). Two processes (`PROBE_RUN=1`,
//! `PROBE_RUN=2`) print byte-identical stdout.
//!
//! **Partial emission:** a failing axis never suppresses the lines that *can*
//! be measured. [`measure_surfaces`] drops only the surface that failed (the
//! first error, verbatim, is what the verdict prints), and every later stage
//! (interactions, block evidences, axes, frames) still runs with the rows it
//! got, degrading to its own declared failure when a prerequisite is missing
//! (`AXIS A1` then reports `surfaces=50/51`, `A6` the deliveries it received).
//! `DIGEST` is always the `sha256` of the lines actually emitted, so a failing
//! run can never report the digest of the empty string.
//!
//! `EXIT=2` without ARENA2 (`DAGGERFALL_ARENA2`, then `DF_ARENA2` and the
//! canonical autodetection of the other probes).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use daggerfall_nervous::events::WindowId;
use daggerfall_parser_bevy_bridge::ui_compat::{
    map_key, InputIntent, InputRouter, Navigation, Presentation, RetailKey, Routed,
    SurfaceOwnership, TextEnv,
};
use daggerfall_parser_bevy_bridge::ui_spec::{Framebuffer, UiAssets};
use daggerfall_perception::read_models::SessionReadModels;
use daggerfall_perception::ui_census::census;
use daggerfall_perception::ui_shell::canonical_id_for;

use crate::m37_ui1_probe::{
    arg_value, arena2_dir, bmp24, load_bank_with, sha256_hex, Bank,
};
use crate::m39_state::{fnv1a64_hex, M39State, ModalOutcome};
use crate::m39_surfaces::{count_by_block, surfaces, M39Surface};

/// What one surface measured before any interaction.
struct SurfaceMeasure {
    id: u16,
    name: &'static str,
    block: char,
    host: String,
    assets: String,
    nodes: usize,
    glyphs: usize,
    focus: usize,
    opaque: usize,
    digest: u64,
    raster: bool,
}

impl SurfaceMeasure {
    /// `SURFACE <id> id=<Name> block=<L> host=<self|Name> assets=<a[,b]|->
    /// nodes=N glyphs=G focus=F opaque=O digest=<16hex>` (plan §4.1).
    fn line(&self) -> String {
        format!(
            "SURFACE {} id={} block={} host={} assets={} nodes={} glyphs={} focus={} opaque={} digest={:016x}",
            id3(self.id),
            self.name,
            self.block,
            self.host,
            self.assets,
            self.nodes,
            self.glyphs,
            self.focus,
            self.opaque,
            self.digest,
        )
    }
}

/// The aggregated interactions one surface delivered.
struct Delivery {
    id: u16,
    name: &'static str,
    block: char,
    intents: usize,
    delivered: usize,
    outcomes: usize,
    /// One label per outcome, in delivery order (the `targets=` of `FOCUS`).
    targets: Vec<String>,
    delta: BTreeMap<&'static str, i64>,
    read_only: Option<&'static str>,
    digest: String,
}

impl Delivery {
    /// `INTERACT <id> id=<Name> block=<L> intents=N delivered=D outcomes=K
    /// delta=<path:+N[,..]|-> read-only=<reason|-> digest=<16hex>`.
    fn line(&self) -> String {
        let delta = if self.delta.is_empty() {
            "-".to_string()
        } else {
            self.delta
                .iter()
                .map(|(path, value)| format!("{path}:{value:+}"))
                .collect::<Vec<_>>()
                .join(",")
        };
        format!(
            "INTERACT {} id={} block={} intents={} delivered={} outcomes={} delta={} read-only={} digest={}",
            id3(self.id),
            self.name,
            self.block,
            self.intents,
            self.delivered,
            self.outcomes,
            delta,
            self.read_only.unwrap_or("-"),
            self.digest,
        )
    }
}

/// Citable `not-applicable` reason of the three read-only surfaces
/// (`031`, `071`, `074` — plan §3.4): they declare no focus row at all.
const NOT_APPLICABLE_READ_ONLY: &str = "read-only:plan-§3.4";

/// Label of one interaction point (an entry of the `targets=` list).
///
/// The label identifies **the control** behind the outcome, never a new
/// geometry: `commit:<route>` names the route a `Committed` outcome changed and
/// every other variant names its own kind.
fn focus_label(outcome: &ModalOutcome) -> String {
    match outcome {
        ModalOutcome::Committed { read_model, .. } => format!("commit:{read_model}"),
        ModalOutcome::ReadOnly { .. } => "read-only".to_string(),
        ModalOutcome::Cancelled => "cancel".to_string(),
        ModalOutcome::Rejected { .. } => "reject".to_string(),
        ModalOutcome::Text { .. } => "text".to_string(),
        ModalOutcome::Opened { .. } => "open".to_string(),
        ModalOutcome::Closed => "close".to_string(),
    }
}

/// One label per outcome, in delivery order.
fn focus_targets(outcomes: &[ModalOutcome]) -> Vec<String> {
    outcomes.iter().map(focus_label).collect()
}

/// The focus rows one surface declares and what the real router did with them.
struct FocusMeasure {
    id: u16,
    name: &'static str,
    block: char,
    /// `true` for the three read-only surfaces of plan §3.4.
    read_only: bool,
    /// Declared rows (`0` when `not_applicable`).
    rows: usize,
    /// Real `Focus(Next)` transitions of the selection (`Tab`).
    next: usize,
    /// Real `Focus(Prev)` transitions of the selection (`BackTab`).
    prev: usize,
    /// Selection after the backward walk (`None` when there is no list).
    sel: Option<usize>,
    /// `Enter` delivered an `Activate` to the surface's own window.
    enter: Option<bool>,
    /// `Space` delivered an `Activate` to the surface's own window.
    space: Option<bool>,
    /// One label per row, in delivery order.
    targets: Vec<String>,
    /// Citable reason when the surface declares no row.
    not_applicable: Option<String>,
}

impl FocusMeasure {
    /// `FOCUS <id> id=<Name> block=<L> rows=N next=A prev=B sel=<k|-> enter=<ok|no|->
    /// space=<ok|no|-> targets=<label,label|->` plus `not-applicable=<reason>`
    /// on the surfaces that declare no row.
    fn line(&self) -> String {
        let targets = if self.targets.is_empty() {
            "-".to_string()
        } else {
            self.targets.join(",")
        };
        let sel = self
            .sel
            .map_or_else(|| "-".to_string(), |row| row.to_string());
        let mut line = format!(
            "FOCUS {} id={} block={} rows={} next={} prev={} sel={sel} enter={} space={} targets={targets}",
            id3(self.id),
            self.name,
            self.block,
            self.rows,
            self.next,
            self.prev,
            flag(self.enter),
            flag(self.space),
        );
        if let Some(reason) = &self.not_applicable {
            line.push_str(" not-applicable=");
            line.push_str(reason);
        }
        line
    }
}

/// `ok` / `no` / `-` (`-` only on the surfaces that declare no row).
fn flag(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "ok",
        Some(false) => "no",
        None => "-",
    }
}

/// Focus rows per surface id, as measured (`0` when the surface cites `-`).
fn focus_rows_of(focus: &[FocusMeasure]) -> BTreeMap<u16, usize> {
    focus.iter().map(|row| (row.id, row.rows)).collect()
}

/// Every surface's focus rows, in census order, walked by the real router.
fn focus_measures(deliveries: &[Delivery]) -> (Vec<FocusMeasure>, Option<String>) {
    let mut rows = Vec::with_capacity(surfaces().len());
    let mut failure: Option<String> = None;
    for surface in surfaces() {
        match focus_one(surface, deliveries) {
            Ok(row) => rows.push(row),
            Err(err) => failure = failure.or(Some(err)),
        }
    }
    (rows, failure)
}

/// One surface's focus rows: declared from its delivery, then walked.
fn focus_one(surface: &M39Surface, deliveries: &[Delivery]) -> Result<FocusMeasure, String> {
    let targets = deliveries
        .iter()
        .find(|row| row.id == surface.id)
        .map(|row| row.targets.clone())
        .unwrap_or_default();
    let mut measure = focus_measure(surface, targets);
    if measure.not_applicable.is_some() {
        return Ok(measure);
    }
    let walk = walk_focus(surface, measure.rows)?;
    measure.next = walk.next;
    measure.prev = walk.prev;
    measure.sel = walk.sel;
    measure.enter = walk.enter;
    measure.space = walk.space;
    Ok(measure)
}

/// The declared rows of one surface, before the router walks them.
///
/// A read-only surface declares none and cites plan §3.4; an interactive
/// surface whose block delivered no outcome declares none **with** a citable
/// `no-outcome:` reason — the shape `AXIS A5` and the gate's `focus-rows` rule
/// refuse instead of letting a silent zero through.
fn focus_measure(surface: &M39Surface, targets: Vec<String>) -> FocusMeasure {
    let not_applicable = if surface.read_only.is_some() {
        Some(NOT_APPLICABLE_READ_ONLY.to_string())
    } else if targets.is_empty() {
        Some(format!("no-outcome:{}", id3(surface.id)))
    } else {
        None
    };
    let rows = if not_applicable.is_some() {
        0
    } else {
        targets.len()
    };
    FocusMeasure {
        id: surface.id,
        name: surface.pilot.name,
        block: surface.block,
        read_only: surface.read_only.is_some(),
        rows,
        next: 0,
        prev: 0,
        sel: None,
        enter: None,
        space: None,
        targets: if rows == 0 { Vec::new() } else { targets },
        not_applicable,
    }
}

/// What the real router did with one surface's declared rows.
struct FocusWalk {
    /// `Focus(Next)` transitions of the selection (`Tab`).
    next: usize,
    /// `Focus(Prev)` transitions of the selection (`BackTab`).
    prev: usize,
    /// Selection after the backward walk (`None` when there is no list).
    sel: Option<usize>,
    /// `Enter` delivered an `Activate` to the surface's own window.
    enter: Option<bool>,
    /// `Space` delivered an `Activate` to the surface's own window.
    space: Option<bool>,
}

/// Walks the real [`InputRouter`] over one surface's rows and reports what the
/// keyboard actually did.
///
/// The router is independent of [`M39State`]: the surface window is opened on a
/// fresh canonical [`Navigation`], the list is declared with
/// [`InputRouter::set_list`] and the keys go through the **real** total map
/// ([`map_key`]) — `Tab` → `Focus(Next)`, `BackTab` → `Focus(Prev)`,
/// `Enter`/`Space` → `Activate`. `next`/`prev` count *real* transitions of the
/// selection (the clamp at an end makes it `rows - 1`); the selection must end
/// at `0`, so a wrap-around or a lost key is visible in the line.
fn walk_focus(surface: &M39Surface, rows: usize) -> Result<FocusWalk, String> {
    let at = id3(surface.id);
    let window = surface.pilot.window;
    let mut nav = Navigation::canonical().map_err(|err| format!("focus {at}: {err}"))?;
    nav.open(window)
        .map_err(|err| format!("focus {at}: open {err}"))?;
    let mut router = InputRouter::new(&nav);
    router.set_list(rows);
    router.sync_focus(&nav);

    let next_key = map_key(RetailKey::Tab).ok_or_else(|| format!("focus {at}: Tab is inert"))?;
    let prev_key =
        map_key(RetailKey::BackTab).ok_or_else(|| format!("focus {at}: BackTab is inert"))?;

    let mut next = 0;
    let mut last = router.selection();
    for _ in 0..rows {
        let _ = router.dispatch(&mut nav, next_key);
        let now = router.selection();
        if now != last {
            next += 1;
        }
        last = now;
    }

    let mut prev = 0;
    let mut last = router.selection();
    for _ in 0..rows {
        let _ = router.dispatch(&mut nav, prev_key);
        let now = router.selection();
        if now != last {
            prev += 1;
        }
        last = now;
    }

    let sel = router.selection();
    let enter = activate(&mut router, &mut nav, window, rows, RetailKey::Enter);
    let space = activate(&mut router, &mut nav, window, rows, RetailKey::Space);

    nav.close(window)
        .map_err(|err| format!("focus {at}: close {err}"))?;
    Ok(FocusWalk {
        next,
        prev,
        sel,
        enter,
        space,
    })
}

/// One `Activate` from one key: `ok` only when the selection sits inside the
/// declared rows and the delivery reaches **this** surface's window.
fn activate(
    router: &mut InputRouter,
    nav: &mut Navigation,
    window: WindowId,
    rows: usize,
    key: RetailKey,
) -> Option<bool> {
    let intent = map_key(key)?;
    if !matches!(intent, InputIntent::Activate) {
        return Some(false);
    }
    let focused = router.focus() == Some(window);
    let in_row = matches!(router.selection(), Some(row) if row < rows);
    Some(match router.dispatch(nav, intent) {
        Routed::Delivered {
            window: delivered, ..
        } => focused && in_row && delivered == window,
        Routed::Dropped(_) => false,
    })
}

/// Runs `PROBE-UI-JUGABLE-001` and returns the process exit code.
pub fn run() -> i32 {
    let Some(dir) = arena2() else {
        println!("VERDICT: SKIP - no ARENA2");
        return 2;
    };
    match stage(&dir) {
        Ok((lines, failure)) => {
            for line in &lines {
                println!("{line}");
            }
            match failure {
                None => {
                    println!("VERDICT: PASS");
                    0
                }
                Some(rule) => {
                    println!("VERDICT: FAIL - {rule}");
                    1
                }
            }
        }
        Err(rule) => {
            println!("VERDICT: FAIL - {rule}");
            1
        }
    }
}

/// ARENA2 of this probe: the M39 override first, then the shared resolution.
fn arena2() -> Option<PathBuf> {
    match std::env::var("DAGGERFALL_ARENA2") {
        Ok(value) if !value.trim().is_empty() => Some(PathBuf::from(value)),
        _ => arena2_dir(),
    }
}

/// Measures, interacts, gates, writes and digests; returns the emitted lines
/// (including the closing `DIGEST`) and the first failed rule, if any.
///
/// Every stage runs even when an earlier one failed: the lines that *can* be
/// measured are emitted, and each stage degrades to its own declared failure
/// when it lacks a prerequisite (`AXIS A1` sees `surfaces=n/51`, `A6` sees the
/// deliveries it actually got). Only the bank/state construction is fatal,
/// because without them there is nothing to measure at all.
fn stage(dir: &Path) -> Result<(Vec<String>, Option<String>), String> {
    let extra = composition_assets();
    let bank = load_bank_with(dir, &extra).map_err(|err| format!("bank {err}"))?;
    let mut state = M39State::new(&bank.assets, dir).map_err(|err| format!("state {err}"))?;

    // SURFACE is measured against the declared no-session start (the state the
    // route census declares before any interaction); the same snapshot is what
    // the A7 repaint has to reproduce.
    let models = models_of(&state);
    let env = TextEnv::with_models(&models);

    let out = PathBuf::from(arg_value("--out").unwrap_or_else(|| "target/ui-jugable-001".to_string()));

    let mut lines: Vec<String> = Vec::new();
    let mut failure: Option<String> = None;

    // The deliveries are measured **before** the surfaces (their outcomes are
    // the focus rows `SURFACE ... focus=` reports) but printed after them, so
    // the grammar order of §4.1 is unchanged. The read-model snapshot above is
    // taken before any interaction and `measure_surfaces` paints from
    // `assets` + `env` only — never from `state` — so no paint digest moves.
    let (deliveries, interact_failure) = interact_all(&mut state);
    let (focus, focus_failure) = focus_measures(&deliveries);
    let focus_rows = focus_rows_of(&focus);

    let (measured, measure_failure) = measure_surfaces(&bank.assets, &env, &focus_rows);
    lines.extend(measured.iter().map(SurfaceMeasure::line));
    failure = failure.or(measure_failure);

    lines.extend(deliveries.iter().map(Delivery::line));
    failure = failure.or(interact_failure);

    lines.extend(focus.iter().map(FocusMeasure::line));
    failure = failure.or(focus_failure);

    let (block_lines, block_failure) = block_evidence(&state);
    lines.extend(block_lines);
    failure = failure.or(block_failure);

    let (axis_lines, axis_failure) = axes(&measured, &deliveries, &focus, &lines, &bank, &env);
    lines.extend(axis_lines);
    failure = failure.or(axis_failure);

    let (bmp_lines, bmp_failure) = write_frames(&bank, &env, &out);
    lines.extend(bmp_lines);
    failure = failure.or(bmp_failure);

    let digest = state.digest_of(&lines);
    lines.push(format!("DIGEST {digest}"));
    Ok((lines, failure))
}

/// Builds the read-model snapshot the declared text resolves against.
fn models_of(state: &M39State) -> SessionReadModels {
    let mut models = SessionReadModels::default();
    models.hud = state.hud.clone();
    models.m39 = state.models.clone();
    models
}

/// Every retail asset declared by a composition, in census order, no repeats.
///
/// The shared bank already loads the fonts and the panel set; the extra names
/// here are the M39 surface assets it does not carry (`BANK00I0`, `AMAP00I0`,
/// the `SPOP.RCI` nine-slice container, …).
fn composition_assets() -> Vec<&'static str> {
    let mut assets: Vec<&'static str> = Vec::new();
    for surface in surfaces() {
        for spec in &surface.pilot.composition.surfaces {
            let name = spec.asset.as_str();
            // `.RCI` containers (the `SPOP.RCI` nine-slice) and the fonts are
            // composed by the shared bank; only plain `.IMG` panels are added.
            if name.ends_with(".IMG") && !assets.contains(&name) {
                assets.push(name);
            }
        }
    }
    assets
}

/// Measures every surface, in ordinal order.
///
/// A surface that fails to measure does **not** abort the sweep: its row is
/// dropped, the first error is kept verbatim (`surface <id3>: <err>` — the text
/// the verdict prints) and the remaining surfaces are still measured, so every
/// line that *can* be measured is emitted.
fn measure_surfaces(
    assets: &UiAssets,
    env: &TextEnv<'_>,
    focus: &BTreeMap<u16, usize>,
) -> (Vec<SurfaceMeasure>, Option<String>) {
    let mut rows = Vec::with_capacity(surfaces().len());
    let mut failure: Option<String> = None;
    for surface in surfaces() {
        let rows_declared = focus.get(&surface.id).copied().unwrap_or(0);
        match measure_surface(surface, assets, env, rows_declared) {
            Ok(row) => rows.push(row),
            Err(err) => failure = failure.or(Some(err)),
        }
    }
    (rows, failure)
}

/// One surface: nodes, glyph runs, focus rows, opaque pixels and paint digest.
fn measure_surface(
    surface: &M39Surface,
    assets: &UiAssets,
    env: &TextEnv<'_>,
    focus_rows: usize,
) -> Result<SurfaceMeasure, String> {
    let at = id3(surface.id);
    let pilot = &surface.pilot;
    let nodes = pilot
        .nodes(assets)
        .map_err(|err| format!("surface {at}: {err}"))?
        .len();
    let glyphs = pilot
        .glyphs(assets, env)
        .map_err(|err| format!("surface {at}: {err}"))?
        .len();
    let mut framebuffer = Framebuffer::new();
    pilot
        .paint(&mut framebuffer, assets, env)
        .map_err(|err| format!("surface {at}: {err}"))?;
    Ok(SurfaceMeasure {
        id: surface.id,
        name: pilot.name,
        block: surface.block,
        host: host_of(surface)?,
        assets: assets_of(surface),
        nodes,
        glyphs,
        // Measured, never declared: the rows are the interaction points this
        // surface's block actually delivered (see [`FocusMeasure`]).
        focus: focus_rows,
        opaque: framebuffer.opaque_pixels(),
        digest: fnv1a64(&framebuffer.pixels),
        raster: !pilot.composition.surfaces.is_empty(),
    })
}

/// `self` for an owned composition, the census name of the owner otherwise.
fn host_of(surface: &M39Surface) -> Result<String, String> {
    for spec in &surface.pilot.composition.surfaces {
        if let SurfaceOwnership::Nested(owner) = spec.ownership {
            return owner_name(owner).map(str::to_string);
        }
    }
    Ok("self".to_string())
}

/// Inverse of `canonical_id_for` over the census (`WindowId` is opaque).
fn owner_name(owner: WindowId) -> Result<&'static str, String> {
    census()
        .iter()
        .find(|row| canonical_id_for(row.name) == Some(owner))
        .map(|row| row.name)
        .ok_or_else(|| format!("nested owner {} is not a census window", owner.index()))
}

/// The assets of the composition (`-` when the surface declares none).
fn assets_of(surface: &M39Surface) -> String {
    let names: Vec<&str> = surface
        .pilot
        .composition
        .surfaces
        .iter()
        .map(|spec| spec.asset.as_str())
        .collect();
    if names.is_empty() {
        "-".to_string()
    } else {
        names.join(",")
    }
}

/// Dispatches every surface to the real function of its block on one state.
///
/// Like [`measure_surfaces`], a surface that cannot deliver does not abort the
/// sweep: its row is dropped and the first error is kept (`interact <id3>: …`),
/// so the deliveries that *were* measured still reach the `INTERACT` lines and
/// `AXIS A6` reports the count it actually got.
fn interact_all(state: &mut M39State) -> (Vec<Delivery>, Option<String>) {
    let mut rows = Vec::with_capacity(surfaces().len());
    let mut failure: Option<String> = None;
    for surface in surfaces() {
        match interact_one(state, surface) {
            Ok(row) => rows.push(row),
            Err(err) => failure = failure.or(Some(err)),
        }
    }
    (rows, failure)
}

/// One surface's aggregated delivery (the block's real interaction function).
fn interact_one(state: &mut M39State, surface: &M39Surface) -> Result<Delivery, String> {
    let at = id3(surface.id);
    let name = surface.pilot.name;
    let returned = match surface.block {
        'A' => crate::m39_block_a::interact(state, name),
        'B' => crate::m39_block_b::interact(state, name),
        'C' | 'D' => crate::m39_block_cd::interact(state, name),
        'E' | 'F' => crate::m39_block_ef::interact(state, name),
        'G' | 'H' => crate::m39_block_gh::interact(state, name),
        other => return Err(format!("interact {at}: unknown block {other}")),
    }
    .map_err(|err| format!("interact {at}: {err}"))?;

    // Block A returns every delivery it has recorded so far; the other
    // blocks return the delivery of this window. Selecting by identity
    // keeps the line about *this* surface under either convention.
    let mine: Vec<_> = returned
        .into_iter()
        .filter(|row| row.id == surface.id && row.name == name)
        .collect();
    if mine.is_empty() {
        return Err(format!(
            "interact {at}: the block delivered no interaction for {name}"
        ));
    }

    for row in &mine {
        for outcome in &row.outcomes {
            M39State::validate(outcome).map_err(|err| format!("interact {at}: {err}"))?;
        }
    }

    let intents: usize = mine.iter().map(|row| row.intents).sum();
    let delivered: usize = mine.iter().map(|row| row.delivered).sum();
    let targets: Vec<String> = mine
        .iter()
        .flat_map(|row| focus_targets(&row.outcomes))
        .collect();
    let outcomes = targets.len();
    let mut delta: BTreeMap<&'static str, i64> = BTreeMap::new();
    for row in &mine {
        for (path, value) in &row.delta {
            *delta.entry(path).or_insert(0) += value;
        }
    }
    let read_only = mine.iter().find_map(|row| row.read_only);
    let digest = fnv1a64_hex(
        &mine
            .iter()
            .map(|row| row.signature())
            .collect::<Vec<_>>()
            .join(";"),
    );
    Ok(Delivery {
        id: surface.id,
        name,
        block: surface.block,
        intents,
        delivered,
        outcomes,
        targets,
        delta,
        read_only,
        digest,
    })
}

/// The eight `BLOCK` lines; `expected` is measured from the registry.
fn block_evidence(state: &M39State) -> (Vec<String>, Option<String>) {
    let expected = count_by_block();
    let mut lines = Vec::with_capacity(expected.len());
    let mut failure = None;

    // The declared partition (block C/D/E/F lists) must equal the registry it
    // is measured from; a drift here is a broken `windows=n/expected`.
    for (block, declared) in [
        ('C', crate::m39_block_cd::BLOCK_C_NAMES.len()),
        ('D', crate::m39_block_cd::BLOCK_D_NAMES.len()),
        ('E', crate::m39_block_ef::BLOCK_E_IDS.len()),
        ('F', crate::m39_block_ef::BLOCK_F_IDS.len()),
    ] {
        let measured = expected
            .iter()
            .find(|(letter, _)| *letter == block)
            .map(|(_, count)| *count)
            .unwrap_or(0);
        if declared != measured && failure.is_none() {
            failure = Some(format!("partition {block}"));
        }
    }

    for evidence in state.evidences() {
        let want = expected
            .iter()
            .find(|(block, _)| *block == evidence.block)
            .map(|(_, count)| *count)
            .unwrap_or(0);
        if evidence.ok {
            lines.push(format!(
                "BLOCK {} OK windows={}/{} flows={} intents={} delivered={} state_delta={} read-only={}",
                evidence.block,
                evidence.windows,
                want,
                evidence.flows,
                evidence.intents,
                evidence.delivered,
                evidence.state_delta,
                evidence.read_only,
            ));
        } else {
            let reason = evidence.reason.unwrap_or("no reason");
            lines.push(format!("BLOCK {} FAIL reason={reason}", evidence.block));
            if failure.is_none() {
                failure = Some(format!("block {} {reason}", evidence.block));
            }
        }
    }
    (lines, failure)
}

/// The seven `AXIS` lines of §4.2, in order, and the first failed rule.
fn axes(
    measured: &[SurfaceMeasure],
    deliveries: &[Delivery],
    focus: &[FocusMeasure],
    lines: &[String],
    bank: &Bank,
    env: &TextEnv<'_>,
) -> (Vec<String>, Option<String>) {
    let mut out = Vec::with_capacity(7);
    let mut failure: Option<String> = None;
    let mut push = |id: &'static str, ok: bool, detail: String| {
        if !ok && failure.is_none() {
            failure = Some(id.to_string());
        }
        out.push(format!("AXIS {id} {} {detail}", if ok { "OK" } else { "FAIL" }));
    };

    let count = surfaces().len();
    let raster: Vec<&SurfaceMeasure> = measured.iter().filter(|row| row.raster).collect();

    // A1 — the composition resolved real nodes (a raster surface with zero
    // nodes is a declaration that did not reach the compositor).
    let nodes: usize = measured.iter().map(|row| row.nodes).sum();
    let a1 = measured.len() == count && !raster.is_empty() && raster.iter().all(|row| row.nodes > 0);
    push(
        "A1",
        a1,
        format!(
            "surfaces={} raster={} nodes={nodes}",
            measured.len(),
            raster.len()
        ),
    );

    // A2 — every raster surface left opaque pixels on the framebuffer.
    let opaque_min = raster.iter().map(|row| row.opaque).min().unwrap_or(0);
    let opaque_total: usize = raster.iter().map(|row| row.opaque).sum();
    let a2 = !raster.is_empty() && opaque_min > 0;
    push(
        "A2",
        a2,
        format!(
            "raster={} opaque-min={opaque_min} opaque-total={opaque_total}",
            raster.len()
        ),
    );

    // A3 — every surface measured glyph runs; no blank run reaches the screen.
    let glyph_total: usize = measured.iter().map(|row| row.glyphs).sum();
    let glyph_min = measured.iter().map(|row| row.glyphs).min().unwrap_or(0);
    let a3 = measured.len() == count && !measured.is_empty() && glyph_min > 0;
    push(
        "A3",
        a3,
        format!(
            "surfaces={} glyphs={glyph_total} min={glyph_min}",
            measured.len()
        ),
    );

    // A4 — no emitted record is blank.
    let empty = lines.iter().filter(|line| line.trim().is_empty()).count();
    let a4 = !lines.is_empty() && empty == 0;
    push(
        "A4",
        a4,
        format!("lines={} empty={empty}", lines.len()),
    );

    // A5 — the typed stack opens every surface (nested owners first) and
    // closes back to zero, **and** every surface declares focus rows that the
    // real router walked: `Tab`/`BackTab` move the selection `rows-1` times
    // (clamped at the ends), the selection ends at `0`, and `Enter`/`Space`
    // deliver `Activate` to the surface's own window. The read-only surfaces of
    // plan §3.4 declare none and cite why.
    let focus_total: usize = focus.iter().map(|row| row.rows).sum();
    let with_focus = focus
        .iter()
        .filter(|row| !row.read_only && row.rows > 0)
        .count();
    let na_ids: Vec<String> = focus
        .iter()
        .filter(|row| row.read_only)
        .map(|row| id3(row.id))
        .collect();
    let na_count = na_ids.len();
    let na_list = if na_ids.is_empty() {
        "-".to_string()
    } else {
        na_ids.join(",")
    };
    let zero_rows = focus.iter().any(|row| !row.read_only && row.rows == 0);
    let nav_ok = focus.iter().filter(|row| !row.read_only).all(|row| {
        row.rows > 0
            && row.next == row.rows - 1
            && row.prev == row.rows - 1
            && row.sel == Some(0)
            && row.targets.len() == row.rows
            && row.not_applicable.is_none()
    });
    let activate_ok = focus
        .iter()
        .filter(|row| !row.read_only)
        .all(|row| row.enter == Some(true) && row.space == Some(true));
    let na_ok = focus.iter().filter(|row| row.read_only).all(|row| {
        row.rows == 0
            && row.sel.is_none()
            && row.enter.is_none()
            && row.space.is_none()
            && matches!(row.not_applicable.as_deref(), Some(reason) if !reason.trim().is_empty())
    });
    let focus_reason = if focus.len() != count {
        Some("focus-missing")
    } else if focus_total == 0 || zero_rows {
        Some("focus-zero")
    } else if !nav_ok {
        Some("focus-nav")
    } else if !activate_ok {
        Some("focus-activate")
    } else if with_focus + na_count != count || !na_ok {
        Some("focus-na")
    } else {
        None
    };
    match stack_check() {
        Ok((open, closed, depth)) => {
            let a5 = open == count && closed == count && depth == 0 && focus_reason.is_none();
            let mut detail = format!(
                "stack={open}/{count} closed={closed} depth={depth} \
                 focus-rows={focus_total} surfaces-with-focus={with_focus} \
                 not-applicable={na_list}"
            );
            if let Some(reason) = focus_reason {
                detail.push_str(" reason=");
                detail.push_str(reason);
            }
            push("A5", a5, detail);
        }
        Err(err) => push("A5", false, format!("stack error: {err}")),
    }

    // A6 — every intent of every surface was delivered with an outcome.
    let intents: usize = deliveries.iter().map(|row| row.intents).sum();
    let delivered: usize = deliveries.iter().map(|row| row.delivered).sum();
    let a6 = deliveries.len() == count
        && !deliveries.is_empty()
        && deliveries
            .iter()
            .all(|row| row.intents > 0 && row.delivered == row.intents)
        && delivered > 0;
    push(
        "A6",
        a6,
        format!(
            "surfaces={} intents={intents} delivered={delivered}",
            deliveries.len()
        ),
    );

    // A7 — repainting the same declaration reproduces the printed digest.
    let mut agree = 0;
    let mut a7_err = None;
    for (row, surface) in measured.iter().zip(surfaces()) {
        if row.id != surface.id {
            a7_err = Some(format!("surface order broke at {}", id3(surface.id)));
            break;
        }
        let mut framebuffer = Framebuffer::new();
        match surface.pilot.paint(&mut framebuffer, &bank.assets, env) {
            Ok(()) => {
                if fnv1a64(&framebuffer.pixels) == row.digest {
                    agree += 1;
                }
            }
            Err(err) => {
                a7_err = Some(format!("surface {}: {err}", id3(surface.id)));
                break;
            }
        }
    }
    let a7 = a7_err.is_none() && agree == measured.len() && measured.len() == count;
    match a7_err {
        Some(err) => push("A7", false, format!("repaint={agree}/{} {err}", measured.len())),
        None => push(
            "A7",
            a7,
            format!("repaint={agree}/{} stable=agree", measured.len()),
        ),
    }

    (out, failure)
}

/// Opens all 51 windows (nested owners first) and closes them back to zero.
fn stack_check() -> Result<(usize, usize, usize), String> {
    let mut navigation = Navigation::canonical().map_err(|err| format!("{err}"))?;
    for surface in surfaces() {
        for spec in &surface.pilot.composition.surfaces {
            if let SurfaceOwnership::Nested(owner) = spec.ownership {
                if !navigation.is_open(owner) {
                    navigation
                        .open(owner)
                        .map_err(|err| format!("open {}: {err}", owner.index()))?;
                }
            }
        }
        if !navigation.is_open(surface.pilot.window) {
            navigation
                .open(surface.pilot.window)
                .map_err(|err| format!("open {}: {err}", id3(surface.id)))?;
        }
    }
    let open = surfaces()
        .iter()
        .filter(|surface| navigation.is_open(surface.pilot.window))
        .count();
    let mut closed = 0;
    for surface in surfaces().iter().rev() {
        if navigation.is_open(surface.pilot.window) {
            navigation
                .close(surface.pilot.window)
                .map_err(|err| format!("close {}: {err}", id3(surface.id)))?;
            closed += 1;
        }
    }
    Ok((open, closed, navigation.depth()))
}

/// Paints, presents, writes and **re-reads** one BMP per surface.
///
/// A surface that cannot be painted drops only its own frame and records the
/// first error (`bmp <id>.bmp: …`); the frames that *were* written still reach
/// the `BMP` lines. The output directory itself is a fatal prerequisite: with
/// no directory nothing can be written, so that error ends the stage.
fn write_frames(
    bank: &Bank,
    env: &TextEnv<'_>,
    out: &Path,
) -> (Vec<String>, Option<String>) {
    if let Err(err) = std::fs::create_dir_all(out) {
        return (Vec::new(), Some(format!("out dir: {err}")));
    }
    let presentation = Presentation::canonical();
    let mut lines = Vec::with_capacity(surfaces().len());
    let mut failure: Option<String> = None;
    for surface in surfaces() {
        match write_frame(bank, env, &presentation, out, surface) {
            Ok(line) => lines.push(line),
            Err(err) => failure = failure.or(Some(err)),
        }
    }
    (lines, failure)
}

/// One `640×480` frame: painted, presented, written and re-hashed from disk.
fn write_frame(
    bank: &Bank,
    env: &TextEnv<'_>,
    presentation: &Presentation,
    out: &Path,
    surface: &M39Surface,
) -> Result<String, String> {
    let name = format!("{}.bmp", id3(surface.id));
    let mut framebuffer = Framebuffer::new();
    surface
        .pilot
        .paint(&mut framebuffer, &bank.assets, env)
        .map_err(|err| format!("bmp {name}: {err}"))?;
    let bytes = bmp24(&presentation.present(&framebuffer));
    let path = out.join(&name);
    std::fs::write(&path, &bytes).map_err(|err| format!("bmp {name}: {err}"))?;
    let on_disk = std::fs::read(&path).map_err(|err| format!("bmp {name}: {err}"))?;
    if on_disk != bytes {
        return Err(format!("bmp {name}: written bytes differ from disk"));
    }
    Ok(format!("BMP {name} sha256={}", sha256_hex(&on_disk)))
}

/// Three-digit census ordinal (`022`…`082`).
fn id3(id: u16) -> String {
    format!("{id:03}")
}

/// FNV-1a 64 of the painted framebuffer bytes.
///
/// `m39_state::fnv1a64_hex` hashes the text signatures of an interaction; the
/// surface digest is over pixels, so the same function is applied to bytes.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `Committed` outcome on `read_model` (`before != after`, plan §7.3).
    fn committed(read_model: &'static str) -> ModalOutcome {
        ModalOutcome::Committed {
            read_model,
            before: "0".to_string(),
            after: "1".to_string(),
        }
    }

    /// Every `ModalOutcome` variant has its own stable label, and the target
    /// list is exactly one label per outcome — never an invented control and
    /// never a lost one.
    #[test]
    fn focus_targets_are_one_label_per_outcome() {
        let outcomes = vec![
            committed("m39.sheet.level"),
            ModalOutcome::ReadOnly {
                reason: "plan §3.4",
            },
            ModalOutcome::Cancelled,
            ModalOutcome::Rejected { reason: "no gold" },
            ModalOutcome::Text {
                value: "hello".to_string(),
            },
            ModalOutcome::Opened {
                window: WindowId::HUD,
            },
            ModalOutcome::Closed,
        ];
        let labels = focus_targets(&outcomes);
        assert_eq!(labels.len(), outcomes.len());
        assert_eq!(
            labels,
            vec![
                "commit:m39.sheet.level".to_string(),
                "read-only".to_string(),
                "cancel".to_string(),
                "reject".to_string(),
                "text".to_string(),
                "open".to_string(),
                "close".to_string(),
            ]
        );
    }

    /// The `FOCUS` line is a stable grammar in both forms, and an interactive
    /// surface that delivered no outcome declares `rows=0` **with** a citable
    /// `not-applicable=` — the shape the gate's `focus-rows` rule must refuse.
    #[test]
    fn focus_line_format_and_zero_outcome_case() {
        let interactive = FocusMeasure {
            id: 22,
            name: "DaggerfallAdvancedSettingsWindow",
            block: 'A',
            read_only: false,
            rows: 2,
            next: 1,
            prev: 1,
            sel: Some(0),
            enter: Some(true),
            space: Some(true),
            targets: vec![
                "commit:m39.settings.scale".to_string(),
                "close".to_string(),
            ],
            not_applicable: None,
        };
        assert_eq!(
            interactive.line(),
            "FOCUS 022 id=DaggerfallAdvancedSettingsWindow block=A rows=2 next=1 prev=1 sel=0 \
             enter=ok space=ok targets=commit:m39.settings.scale,close"
        );

        let absent = FocusMeasure {
            id: 31,
            name: "DaggerfallDaedraSummonedWindow",
            block: 'H',
            read_only: true,
            rows: 0,
            next: 0,
            prev: 0,
            sel: None,
            enter: None,
            space: None,
            targets: Vec::new(),
            not_applicable: Some(NOT_APPLICABLE_READ_ONLY.to_string()),
        };
        assert_eq!(
            absent.line(),
            "FOCUS 031 id=DaggerfallDaedraSummonedWindow block=H rows=0 next=0 prev=0 sel=- \
             enter=- space=- targets=- not-applicable=read-only:plan-§3.4"
        );

        // The case the gate exists for: an interactive surface with no declared
        // row must never print a bare `rows=0`.
        let zero = focus_measure(&surfaces()[0], Vec::new());
        assert!(!zero.read_only, "022 is an interactive surface");
        assert_eq!(zero.rows, 0);
        assert!(zero.line().contains("rows=0"));
        assert!(
            matches!(zero.not_applicable.as_deref(), Some(reason) if !reason.trim().is_empty()),
            "the zero row count must carry a citable reason: {:?}",
            zero.not_applicable
        );
    }
}

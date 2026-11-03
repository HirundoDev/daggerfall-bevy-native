//! **PROBE-MENU-LAYOUT-001** — the main menu **that ships** measured as a
//! screen: `menu_spec::main_menu(row, true)` composed at `320×180` and
//! presented **full-bleed** at `×4` (`1280×720`, 0 bands).
//!
//! The previous probe measured `ui_spec::build_main_menu_panel` — the retail
//! `320×200` plate that is no longer on the production route (`MENU-MODERN-DESIGN.md`
//! §9.4: it survives only as a `UI-001` fixture). Its metrics died with the old
//! screen: the `212,35,108,125` band and the `MENU_GUTTER` strip do not
//! exist in the new design, and a full-bleed menu makes "outside the panel"
//! menu as well. What is measured now is what the player sees:
//!
//! - **content** — a plate's content rect is **derived**
//!   (`chrome + INSET`, `INSET = STROKE + PAD = 9`), never the rect the node
//!   declares. Every node of a zone is measured against `plate.rect + INSET` and its margin
//!   to the plate chrome must be `>= MIN_MARGIN`. `out_of_content` counts the ones
//!   that do not comply; the probe prints both rects so the measurement is
//!   reproducible by hand.
//! - **rows** — the five rows of `MENU_ROW_IDS` at `y = 55 + 17i`, height `14`,
//!   `x 25..203` (at `×4`: `y = 220 + 68i`, height `56`, bar `x 100..812`), and the
//!   painted nodes (focus bar, label, marker) on their row.
//! - **present** — `present_fullscreen(fb, 4)`: `1280×720`, `letterbox 0/0`,
//!   `Fullscreen`, exact nearest upscale (`mismatch_px = 0`) and **zero** black
//!   pixels on the edges (a black band betrays that the screen is not full-bleed).
//! - **labels** — the painted labels are `MENU_ROW_LABELS`, in order.
//! - **selection** — the active row **is painted**: two different rows give two
//!   different buffers and the chevron moves `17` px per row.
//! - **clips** — clipped content (a `Sprite` whose `src` exceeds its window,
//!   a `GlyphRun` that does not fit its box or leaves the canvas).
//! - **outside** — nodes outside the `320×180` canvas.
//! - **dialogs** — dialogue plates (`BUTN00...`) in the main menu: zero.
//! - **row_overlap** — two `Nav` rows cannot overlap.
//!
//! `EXIT=0` ⟺ everything at 0; `EXIT=1` on FAIL; `EXIT=2` if the retail assets are not
//! readable.
//!
//! Falsifiers (one per real defect):
//! `--force-margin` injects a node with **margin 8** to the `nav_plate` chrome
//! (1 px outside the derived content) — the same defect the tautological assert
//! let through on 2026-10-31; `--force-frozen-row` composes the **same** row twice
//! (`eb2aece`'s painting) and `diff_px` must collapse to 0.
//!
//! The `--force-bad`, `--force-crop`, `--force-dialog` and `--force-gutter` legs
//! are withdrawn: they injected `BUTN00`/`MENU.IMG` plates **over the retail plate**.
//! The plate that made them possible is no longer composed, so the leg could not
//! fail for the defect it claimed to measure (dead falsifier = lie). The
//! semantic guard that remains is `dialogs == 0`: the modern menu carries
//! no dialogue widget.

use daggerfall_parser_bevy_bridge::menu_spec::{
    ink, main_menu, MenuScreen, DEFAULT_ACTIVE_ROW, INSET, MENU_HEIGHT, MENU_ROW_IDS,
    MENU_ROW_LABELS, MENU_WIDTH, MIN_MARGIN, ROW_PAD_X,
};
use daggerfall_parser_bevy_bridge::ui_spec::{
    canvas_rgba, compose_sized, fill_node, present_fullscreen, Framebuffer, PresentFrame,
    PresentMode, UiAssets, UiNode, UiNodeKind, UiPanel, UiRect,
};
use daggerfall_parser_rs::image::PaletteFamily;

/// Presentation scale of the menu screen (`320×180 ×4 = 1280×720`).
const PANEL_SCALE: u8 = 4;
/// Demo window (`WindowPlugin` of `m18_window::build_app`).
const WINDOW_WIDTH: u16 = 1280;
/// Ventana del demo.
const WINDOW_HEIGHT: u16 = 720;

/// First `Nav` row on the canvas: `content.y (41) + eyebrow (14)`.
const ROW_FIRST_Y: i16 = 55;
/// Paso vertical entre filas (`y = 55 + 17i`).
const ROW_STRIDE: i16 = 17;
/// Row height on the canvas (`×4` = 56).
const ROW_HEIGHT: u16 = 14;
/// `x` of the `nav_plate` content (`×4` = 100).
const ROW_X: i16 = 25;
/// Width of the `nav_plate` content (`25..203`, `×4` = `100..812`).
const ROW_WIDTH: u16 = 178;

/// Asset family of the dialogue buttons: never a menu widget.
const DIALOG_BUTTON_PREFIX: &str = "BUTN00";

/// Margin, in px, between `inner` and `outer` (negative when it overflows).
fn margin(outer: &UiRect, inner: &UiRect) -> i16 {
    let left = inner.x - outer.x;
    let top = inner.y - outer.y;
    let right = outer.right() - inner.right();
    let bottom = outer.bottom() - inner.bottom();
    left.min(top).min(right).min(bottom)
}

/// `inner` completamente dentro de `outer`.
fn contains(outer: &UiRect, inner: &UiRect) -> bool {
    margin(outer, inner) >= 0
}

/// Intersection area of two rects (`0` if they only touch).
fn overlap_area(left: &UiRect, right: &UiRect) -> u32 {
    let width = left.right().min(right.right()) - left.x.max(right.x);
    let height = left.bottom().min(right.bottom()) - left.y.max(right.y);
    if width <= 0 || height <= 0 {
        0
    } else {
        u32::try_from(width).unwrap_or(0) * u32::try_from(height).unwrap_or(0)
    }
}

/// FNV-1a 64 over the framebuffer indices: one number per painting.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Pixels of the `frame` that are **not** the `×scale` nearest upscale of the canvas.
///
/// `ui_spec::present_delta`/`present_interpolated_pixels` are wired to the
/// retail canvas (`320×200`, `×2`); the menu is `320×180` at `×4`, so the
/// check is done here over the real frame, without touching the compositor.
fn present_mismatch(framebuffer: &Framebuffer, frame: &PresentFrame) -> usize {
    let canvas = canvas_rgba(framebuffer);
    let scale = usize::from(frame.scale.max(1));
    let mut mismatch = 0usize;
    for y in 0..usize::from(framebuffer.height) {
        for x in 0..usize::from(framebuffer.width) {
            let at = (y * usize::from(framebuffer.width) + x) * 4;
            let expected = &canvas[at..at + 4];
            for sy in 0..scale {
                for sx in 0..scale {
                    let px = x * scale + sx;
                    let py = y * scale + sy;
                    let out = (py * usize::from(frame.width) + px) * 4;
                    if frame.rgba.get(out..out + 4) != Some(expected) {
                        mismatch += 1;
                    }
                }
            }
        }
    }
    mismatch
}

/// Black pixels in the first and last row of the frame: a black band
/// means the screen is **not** full-bleed.
fn edge_black(frame: &PresentFrame) -> usize {
    let width = usize::from(frame.width);
    let mut black = 0usize;
    for row in [0usize, usize::from(frame.height) - 1] {
        for x in 0..width {
            let at = (row * width + x) * 4;
            if frame.rgba.get(at..at + 3) == Some(&[0u8, 0, 0][..]) {
                black += 1;
            }
        }
    }
    black
}

fn print_node(node: &UiNode) {
    println!(
        "  {:<26} {:<14} x={:<4} y={:<4} w={:<4} h={:<4} src={} text={}",
        node.id,
        node.asset,
        node.rect.x,
        node.rect.y,
        node.rect.width,
        node.rect.height,
        node.source.map_or_else(
            || "-".to_string(),
            |source| format!("{},{},{},{}", source.x, source.y, source.width, source.height)
        ),
        node.text.as_deref().unwrap_or("-"),
    );
}

/// `--force-margin`: a content node with **margin 8** to the
/// `nav_plate` chrome — 1 px inside the chrome, 1 px outside the derived content.
///
/// It is the defect of 2026-10-31 (`MENU-MODERN-DESIGN.md` §9.3): the assert
/// compared the node against the rect the node itself declared and the overflow
/// passed. Here the limit is `plate.rect + INSET`, so 8 must fail.
fn insert_margin_violation(panel: &mut UiPanel, screen: &MenuScreen, assets: &UiAssets) {
    let Some(plate) = screen.plates.first() else {
        return;
    };
    let chrome = plate.rect;
    let at = UiRect::new(
        chrome.x + i16::try_from(INSET).unwrap_or(9) - 1,
        chrome.y + 40,
        2,
        2,
    );
    match fill_node("nav-force-margin", assets, at, PaletteFamily::Hud, ink::PAPER) {
        Ok(node) => panel.nodes.push(node),
        Err(err) => println!("[FAIL] force-margin - no se pudo inyectar el nodo: {err}"),
    }
}

/// `--probe MENU-LAYOUT-001 [--force-margin|--force-frozen-row]` — exit code.
pub fn run_menu_layout_001(force_margin: bool, force_frozen_row: bool) -> i32 {
    let leg = if force_margin {
        " · --force-margin (nodo a margen 8 del cromo)"
    } else if force_frozen_row {
        " · --force-frozen-row (fila activa congelada: la pintura de eb2aece)"
    } else {
        ""
    };
    println!("PROBE-MENU-LAYOUT-001 - menú moderno 320×180 ×4 = 1280×720 medido{leg}");
    let dir = match crate::mode_a_arena2_dir() {
        Ok(dir) => dir,
        Err(reason) => {
            println!("ARENA2 not accessible: {reason}");
            println!("reason=missing_arena2");
            println!("EXIT=2");
            println!("VERDICT: PROBE-MENU-LAYOUT-001 NO DATA - ARENA2 not accessible");
            return 2;
        }
    };
    let assets = match crate::ui_assets::load_ui_assets(&dir) {
        Ok(assets) => assets,
        Err(reason) => {
            println!("ARENA2 assets not loadable: {reason}");
            println!("reason=missing_ui_assets");
            println!("EXIT=2");
            println!("VERDICT: PROBE-MENU-LAYOUT-001 NO DATA - UI assets not loadable");
            return 2;
        }
    };

    // ── the screen that ships ─────────────────────────────────────────────
    let screen = main_menu(DEFAULT_ACTIVE_ROW, true);
    let mut panel = match screen.compose(&assets) {
        Ok(panel) => panel,
        Err(err) => {
            println!("[FAIL] composición - main_menu.compose: {err}");
            println!("EXIT=1");
            println!("VERDICT: PROBE-MENU-LAYOUT-001 FAIL");
            return 1;
        }
    };
    println!(
        "screen={} plates={} zones={} nodes={}",
        screen.id,
        screen.plates.len(),
        screen.zones.len(),
        panel.nodes.len()
    );
    if force_margin {
        insert_margin_violation(&mut panel, &screen, &assets);
    }
    for node in &panel.nodes {
        print_node(node);
    }

    // ── content · the limit is `plate.rect + INSET`, not the node's rect ───
    let mut out_of_content = 0usize;
    let mut attributable = 0usize;
    let mut unattributed = 0usize;
    let mut min_margin = i16::MAX;
    for zone in &screen.zones {
        let Some(plate) = screen.plates.iter().find(|plate| plate.id == zone.plate()) else {
            continue;
        };
        let chrome = plate.rect;
        let limit = plate.content();
        let mut nodes = 0usize;
        let mut zone_min = i16::MAX;
        for node in &panel.nodes {
            if !node.id.starts_with(&format!("{}-", zone.id())) {
                continue;
            }
            nodes += 1;
            attributable += 1;
            let to_chrome = margin(&chrome, &node.rect);
            let to_content = margin(&limit, &node.rect);
            zone_min = zone_min.min(to_chrome);
            min_margin = min_margin.min(to_chrome);
            if to_content < 0 {
                out_of_content += 1;
                println!(
                    "[FAIL] margin {} chrome={:?} content={:?} margen_cromo={to_chrome} \
                     margen_contenido={to_content} (< {MIN_MARGIN})",
                    node.id, chrome, limit
                );
            }
        }
        let zone_fits = contains(&limit, &zone.rect());
        if !zone_fits {
            out_of_content += 1;
            println!(
                "[FAIL] zone {} rect={:?} no cabe en content={:?}",
                zone.id(),
                zone.rect(),
                limit
            );
        }
        println!(
            "[{}] content {} - chrome={},{},{},{} content={},{},{},{} (chrome+INSET={INSET}) \
             nodos={nodes} margen_min={} zona_cabe={zone_fits}",
            if zone_fits && zone_min >= MIN_MARGIN { "OK" } else { "FAIL" },
            plate.id,
            chrome.x,
            chrome.y,
            chrome.width,
            chrome.height,
            limit.x,
            limit.y,
            limit.width,
            limit.height,
            if zone_min == i16::MAX { 0 } else { zone_min },
        );
    }
    // Every node is declared chrome or a zone's content: otherwise the probe's
    // mental model and the screen no longer agree.
    let chrome_prefixes = [
        "menu-field",
        "lintel",
        "footer",
    ];
    for node in &panel.nodes {
        let in_zone = screen
            .zones
            .iter()
            .any(|zone| node.id.starts_with(&format!("{}-", zone.id())));
        let in_plate = screen
            .plates
            .iter()
            .any(|plate| node.id.starts_with(&format!("{}-", plate.id)));
        if !in_zone
            && !in_plate
            && !chrome_prefixes.iter().any(|prefix| node.id.starts_with(prefix))
        {
            unattributed += 1;
            println!("[FAIL] unattributed {} - no es cromo ni contenido de una zona", node.id);
        }
    }
    println!(
        "[{}] content - {attributable} nodo(s) de contenido · margen mínimo {} · \
         fuera_de_contenido={out_of_content} · sin_atribuir={unattributed}",
        if out_of_content == 0 && unattributed == 0 { "OK" } else { "FAIL" },
        if min_margin == i16::MAX { 0 } else { min_margin }
    );

    // ── rows · `y = 55 + 17i`, alto 14 ─────────────────────────────────────
    let rows = screen.nav_rows("nav");
    let mut rows_ok = rows.len() == MENU_ROW_IDS.len();
    if !rows_ok {
        println!(
            "[FAIL] rows - {} fila(s), se esperan {}",
            rows.len(),
            MENU_ROW_IDS.len()
        );
    }
    for (index, (id, rect)) in rows.iter().enumerate() {
        let wanted_y = ROW_FIRST_Y + index as i16 * ROW_STRIDE;
        if rect.y != wanted_y || rect.height != ROW_HEIGHT || rect.x != ROW_X || rect.width != ROW_WIDTH
        {
            rows_ok = false;
            println!(
                "[FAIL] rows[{index}] {id} {:?} != {ROW_X},{wanted_y},{ROW_WIDTH},{ROW_HEIGHT}",
                rect
            );
        }
        // The painted row: focus bar (only the active one), label and marker.
        let label_id = format!("nav-{id}-label");
        match panel.nodes.iter().find(|node| node.id == label_id) {
            Some(label) => {
                let wanted = (rect.x + ROW_PAD_X, rect.y + 3);
                if (label.rect.x, label.rect.y) != wanted {
                    rows_ok = false;
                    println!(
                        "[FAIL] rows[{index}] {label_id} en {:?}, se espera x={} y={}",
                        label.rect, wanted.0, wanted.1
                    );
                }
            }
            None => {
                rows_ok = false;
                println!("[FAIL] rows[{index}] {label_id} no pintado");
            }
        }
        let bar_id = format!("nav-{id}-bar");
        if let Some(bar) = panel.nodes.iter().find(|node| node.id == bar_id) {
            if bar.rect != *rect {
                rows_ok = false;
                println!("[FAIL] rows[{index}] {bar_id} {:?} != fila {:?}", bar.rect, rect);
            }
        } else if index == DEFAULT_ACTIVE_ROW {
            rows_ok = false;
            println!("[FAIL] rows[{index}] {bar_id} (barra de foco) no pintado");
        }
    }
    if rows_ok {
        println!(
            "rows={} y={ROW_FIRST_Y}+{ROW_STRIDE}i alto={ROW_HEIGHT} x={ROW_X}..{} \
             ×4: y={}+{}i alto={} barra x {}..{}",
            rows.len(),
            ROW_X + ROW_WIDTH as i16,
            ROW_FIRST_Y * i16::try_from(PANEL_SCALE).unwrap_or(4),
            ROW_STRIDE * i16::try_from(PANEL_SCALE).unwrap_or(4),
            ROW_HEIGHT * u16::from(PANEL_SCALE),
            ROW_X * i16::try_from(PANEL_SCALE).unwrap_or(4),
            (ROW_X + ROW_WIDTH as i16) * i16::try_from(PANEL_SCALE).unwrap_or(4),
        );
    }
    println!(
        "[{}] rows - {} fila(s) en su sitio",
        if rows_ok { "OK" } else { "FAIL" },
        rows.len()
    );

    // ── row_overlap · dos filas del Nav no se pisan ────────────────────────
    let mut row_overlap = 0usize;
    for (index, (left_id, left)) in rows.iter().enumerate() {
        for (right_id, right) in rows.iter().skip(index + 1) {
            let area = overlap_area(left, right);
            if area > 0 {
                row_overlap += 1;
                println!("[FAIL] row_overlap {left_id} × {right_id} = {area}px²");
            }
        }
    }
    println!(
        "[{}] row_overlap - {row_overlap} par(es) solapado(s)",
        if row_overlap == 0 { "OK" } else { "FAIL" }
    );

    // ── present · 320×180 ×4 = 1280×720, 0 bandas, nearest exacto ──────────
    let framebuffer = match compose_sized(&[panel.clone()], &assets, MENU_WIDTH, MENU_HEIGHT) {
        Ok(fb) => fb,
        Err(err) => {
            println!("[FAIL] present - compose_sized: {err}");
            println!("EXIT=1");
            println!("VERDICT: PROBE-MENU-LAYOUT-001 FAIL");
            return 1;
        }
    };
    let frame = present_fullscreen(&framebuffer, PANEL_SCALE);
    let mismatch_px = present_mismatch(&framebuffer, &frame);
    let edge_black_px = edge_black(&frame);
    let opaque = framebuffer.opaque_pixels();
    let canvas_px = usize::from(MENU_WIDTH) * usize::from(MENU_HEIGHT);
    let mut present_ok = true;
    if frame.width != WINDOW_WIDTH || frame.height != WINDOW_HEIGHT {
        present_ok = false;
        println!(
            "[FAIL] present - {}×{} != {WINDOW_WIDTH}×{WINDOW_HEIGHT}",
            frame.width, frame.height
        );
    }
    if frame.letterbox_top != 0 || frame.letterbox_bottom != 0 || frame.mode != PresentMode::Fullscreen
    {
        present_ok = false;
        println!(
            "[FAIL] present - bandas={}/{} modo={:?} (se espera 0/0 Fullscreen)",
            frame.letterbox_top, frame.letterbox_bottom, frame.mode
        );
    }
    if mismatch_px != 0 {
        present_ok = false;
        println!("[FAIL] present - {mismatch_px} px no son el upscale nearest ×{PANEL_SCALE}");
    }
    if edge_black_px != 0 {
        present_ok = false;
        println!("[FAIL] present - {edge_black_px} px negro(s) en los bordes: hay banda");
    }
    if opaque != canvas_px {
        present_ok = false;
        println!("[FAIL] present - opaque={opaque}/{canvas_px}: la pantalla no cubre el canvas");
    }
    println!(
        "[{}] present - {MENU_WIDTH}×{MENU_HEIGHT} ×{PANEL_SCALE} = {}×{} bandas={}/{} \
         modo={:?} mismatch_px={mismatch_px} edge_black={edge_black_px} opaque={opaque}/{canvas_px}",
        if present_ok { "OK" } else { "FAIL" },
        frame.width,
        frame.height,
        frame.letterbox_top,
        frame.letterbox_bottom,
        frame.mode,
    );

    // ── labels · the screen says exactly what the component declares ─
    let actual: Vec<String> = panel
        .nodes
        .iter()
        .filter(|node| node.id.starts_with("nav-") && node.id.ends_with("-label"))
        .map(|node| node.text.clone().unwrap_or_default())
        .collect();
    let expected: Vec<String> = MENU_ROW_LABELS.iter().map(|label| (*label).to_string()).collect();
    println!("expected_labels={expected:?}");
    println!("actual_labels={actual:?}");
    let mut labels = 0usize;
    for (index, wanted) in expected.iter().enumerate() {
        match actual.get(index) {
            Some(got) if got == wanted => {}
            Some(got) => {
                labels += 1;
                println!("[FAIL] label[{index}] {got:?} != {wanted:?}");
            }
            None => {
                labels += 1;
                println!("[FAIL] label[{index}] ausente (esperado {wanted:?})");
            }
        }
    }
    if actual.len() > expected.len() {
        labels += actual.len() - expected.len();
        println!(
            "[FAIL] label extra: {} rótulo(s) fuera de MENU_ROW_LABELS",
            actual.len() - expected.len()
        );
    }
    println!(
        "[{}] labels - {}/{} rótulo(s) en orden",
        if labels == 0 { "OK" } else { "FAIL" },
        expected.len() - labels.min(expected.len()),
        expected.len()
    );

    // ── dialogs · the main menu carries no dialogue widgets ────────────
    let dialogs = panel
        .nodes
        .iter()
        .filter(|node| node.asset.starts_with(DIALOG_BUTTON_PREFIX))
        .count();
    if dialogs > 0 {
        println!("[FAIL] dialogs={dialogs} - platos de diálogo ({DIALOG_BUTTON_PREFIX}) en el menú");
    }
    println!(
        "[{}] dialogs - {dialogs} plat(s) de diálogo",
        if dialogs == 0 { "OK" } else { "FAIL" }
    );

    // ── clips · contenido recortado ────────────────────────────────────────
    let mut clips = 0usize;
    let mut cropped_px = 0u32;
    for node in &panel.nodes {
        match node.kind {
            UiNodeKind::Sprite => {
                let source = node.source.unwrap_or_else(|| {
                    assets.image(&node.asset).map_or(node.rect, |asset| {
                        UiRect::new(0, 0, asset.image.width, asset.image.height)
                    })
                });
                let cols = i32::from(source.width) - i32::from(node.rect.width);
                let rows = i32::from(source.height) - i32::from(node.rect.height);
                if cols > 0 || rows > 0 {
                    let lost_cols = u32::try_from(cols.max(0)).unwrap_or(0);
                    let lost_rows = u32::try_from(rows.max(0)).unwrap_or(0);
                    let px = lost_cols * u32::from(source.height)
                        + lost_rows * u32::from(node.rect.width.min(source.width));
                    clips += 1;
                    cropped_px = cropped_px.saturating_add(px);
                    println!(
                        "[FAIL] clip {} src={}x{} ventana={}x{} - {lost_cols}x{lost_rows} de la fuente nunca se pintan",
                        node.id, source.width, source.height, node.rect.width, node.rect.height
                    );
                }
            }
            UiNodeKind::GlyphRun => {
                let Some(font) = assets.font(&node.asset) else {
                    continue;
                };
                let text = node.text.as_deref().unwrap_or_default();
                let advance = i32::from(font.text_width(text));
                let over_w = advance - i32::from(node.rect.width);
                let over_h = i32::from(font.fixed_height) - i32::from(node.rect.height);
                let canvas = UiRect::new(0, 0, MENU_WIDTH, MENU_HEIGHT);
                let outside_canvas = !contains(&canvas, &node.rect);
                if over_w > 0 || over_h > 0 || outside_canvas {
                    clips += 1;
                    cropped_px = cropped_px.saturating_add(u32::try_from(over_w.max(0)).unwrap_or(0));
                    println!(
                        "[FAIL] clip {} texto={text:?} avance={advance} caja={}x{} dentro_canvas={}",
                        node.id,
                        node.rect.width,
                        node.rect.height,
                        !outside_canvas
                    );
                }
            }
            // A `Fill` writes its own rect: there is no source to clip.
            UiNodeKind::Fill => {}
        }
    }
    println!(
        "[{}] clippings - {clips} nodo(s) recortado(s) · {cropped_px} px de fuente sin pintar",
        if clips == 0 { "OK" } else { "FAIL" }
    );

    // ── outside · everything inside the 320×180 canvas ───────────────────
    let canvas = UiRect::new(0, 0, MENU_WIDTH, MENU_HEIGHT);
    let mut outside = 0usize;
    for node in &panel.nodes {
        if !contains(&canvas, &node.rect) {
            outside += 1;
            println!("[FAIL] outside {} {:?} fuera del canvas", node.id, node.rect);
        }
    }
    println!(
        "[{}] outside - {outside} nodo(s) fuera del canvas {MENU_WIDTH}×{MENU_HEIGHT}",
        if outside == 0 { "OK" } else { "FAIL" }
    );

    // ── selection · the active row IS PAINTED ────────────────────────────────
    // Two different rows ⇒ two different buffers and the chevron `17` px further
    // down per row; `--force-frozen-row` composes the same row twice (the
    // baked painting of `eb2aece`) and the falsifier must collapse to 0.
    const SELECTION_ROW_A: usize = DEFAULT_ACTIVE_ROW;
    const SELECTION_ROW_B: usize = 3;
    let mut selection_pictures: Vec<(u64, i16, Framebuffer)> = Vec::new();
    for row in [SELECTION_ROW_A, SELECTION_ROW_B] {
        let row = if force_frozen_row { SELECTION_ROW_A } else { row };
        let screen = main_menu(row, true);
        let composed: Result<(i16, UiPanel), String> = match screen.compose(&assets) {
            Ok(panel) => {
                let chevron_y = panel
                    .nodes
                    .iter()
                    .find(|node| node.id == format!("nav-{}-bar", MENU_ROW_IDS[row]))
                    .map(|node| node.rect.y);
                match chevron_y {
                    Some(chevron_y) => Ok((chevron_y, panel)),
                    None => Err(format!("fila {row}: sin barra de foco")),
                }
            }
            Err(err) => Err(err.to_string()),
        };
        match composed {
            Ok((bar_y, panel)) => {
                match compose_sized(&[panel], &assets, MENU_WIDTH, MENU_HEIGHT) {
                    Ok(framebuffer) => {
                        selection_pictures.push((fnv1a(&framebuffer.pixels), bar_y, framebuffer))
                    }
                    Err(err) => println!("[FAIL] selection - fila {row}: compose: {err}"),
                }
            }
            Err(err) => println!("[FAIL] selection - {err}"),
        }
    }
    let mut selection_sha_a = 0u64;
    let mut selection_sha_b = 0u64;
    let mut selection_diff_px = 0usize;
    let mut selection_chevron_dy = 0i16;
    let mut selection_measured = false;
    if let [row_a, row_b] = selection_pictures.as_slice() {
        selection_measured = true;
        selection_sha_a = row_a.0;
        selection_sha_b = row_b.0;
        selection_diff_px = row_a
            .2
            .pixels
            .iter()
            .zip(row_b.2.pixels.iter())
            .filter(|(left, right)| left != right)
            .count();
        selection_chevron_dy = row_b.1 - row_a.1;
        let expected_chevron_dy = (SELECTION_ROW_B - SELECTION_ROW_A) as i16 * ROW_STRIDE;
        if selection_diff_px == 0 {
            println!(
                "[FAIL] selection - filas {SELECTION_ROW_A} y {SELECTION_ROW_B} pintan el MISMO buffer \
                 (diff_px=0, sha={selection_sha_a:016x}): la fila activa no se pinta"
            );
        }
        if selection_chevron_dy != expected_chevron_dy {
            println!(
                "[FAIL] selection - la barra se movió {selection_chevron_dy} px, esperado \
                 {expected_chevron_dy} ({SELECTION_ROW_B}−{SELECTION_ROW_A} filas × {ROW_STRIDE})"
            );
        }
    } else {
        println!("[FAIL] selection - no se pintaron las dos filas");
    }
    let expected_chevron_dy = (SELECTION_ROW_B - SELECTION_ROW_A) as i16 * ROW_STRIDE;
    let selection_ok = selection_measured
        && selection_diff_px > 0
        && selection_chevron_dy == expected_chevron_dy;
    println!(
        "[{}] selection - fila {SELECTION_ROW_A} sha={selection_sha_a:016x} · fila \
         {SELECTION_ROW_B} sha={selection_sha_b:016x} · diff_px={selection_diff_px} · \
         chevron_dy={selection_chevron_dy}",
        if selection_ok { "OK" } else { "FAIL" }
    );

    println!(
        "labels={labels} out_of_content={out_of_content} unattributed={unattributed} \
         rows_ok={rows_ok} row_overlap={row_overlap} present_ok={present_ok} \
         present_mismatch_px={mismatch_px} edge_black={edge_black_px} \
         selection_diff_px={selection_diff_px} selection_chevron_dy={selection_chevron_dy} \
         selection_sha_row{SELECTION_ROW_A}={selection_sha_a:016x} \
         selection_sha_row{SELECTION_ROW_B}={selection_sha_b:016x} \
         clips={clips} cropped_px={cropped_px} outside={outside} dialogs={dialogs}"
    );
    let pass = labels == 0
        && out_of_content == 0
        && unattributed == 0
        && rows_ok
        && row_overlap == 0
        && present_ok
        && selection_ok
        && clips == 0
        && outside == 0
        && dialogs == 0;
    println!("EXIT={}", if pass { 0 } else { 1 });
    println!(
        "VERDICT: PROBE-MENU-LAYOUT-001 {}",
        if pass { "PASS" } else { "FAIL" }
    );
    if pass {
        0
    } else {
        1
    }
}

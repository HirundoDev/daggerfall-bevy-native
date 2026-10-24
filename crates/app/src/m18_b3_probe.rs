//! M18 **B3-d (O3, CS-12)** probe — `PROBE-WEATHER-VISUAL-001`.
//!
//! The visual layer of the demo (rain/snow streaks) is a **consumer** of the
//! M13 climate index (`CLIMATE.PAK`, read-only) and of the rows the binding M31
//! spec publishes (§2.1 `WeatherTable`). This probe certifies the consumer
//! before any pixel is drawn:
//!
//! - **A** — spec vectors W1…W11 through [`ui_weather::draw`]
//!   (`Woodlands/Summer`, `Mountains/Winter`, the normalised
//!   `Woodlands/Winter` row).
//! - **B** — 10 000 seeded draws on the humid row `Jungle/Spring`
//!   (`Rainforest`): observed frequency vs the published percentages,
//!   `max_abs_delta < 0.02`, FNV-1a 64 digest identical ×2.
//! - **C** — **two-sided control**: every roll `0.0 … 100.0` on the dry row
//!   `Desert/Summer` ⇒ **0** precipitation and **0 drops**; the same roll
//!   sweep on `Jungle/Spring` ⇒ ≥1 rain with **drops > 0**.
//! - **D** — retail `CLIMATE.PAK` (M13): distinct values of the grid and the
//!   precipitation each climate family resolves (row · state · drops at
//!   roll 50), with the 10 `Climates` values covered.
//!
//! `EXIT=0` ⟺ A∧B∧C∧D; `EXIT=2` when `CLIMATE.PAK` is not readable (D).
//! `EXIT=1` on FAIL. **0 panic**, 0 retail bytes in the repository.

use daggerfall_parser_rs::climate::{ClimateMap, WorldClimate};
use daggerfall_parser_rs::dfrandom::DfRandom;

use crate::ui_weather::{
    draw, drops, Precipitation, Season, WeatherKind, WeatherRow, PUBLISHED_ROWS,
};

/// Seed of block B (same policy as the M31 spec §5: fixed seeds for the
/// statistical blocks, none for the vector blocks).
const WEATHER_SEED: u32 = 0x0038_A1E6;
/// Draws of block B.
const DRAWS: usize = 10_000;
/// Tolerance per cell of block B (spec §5 `PROBE-WEATHER-001` B).
const MAX_ABS_DELTA: f32 = 0.02;

/// FNV-1a 64 over the frequency bytes (determinism evidence).
fn digest_of(counts: &[usize]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for count in counts {
        for byte in (*count as u64).to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

/// `[0,100)` from the seeded stream (the source uses `Random.Range(0f, 100f)`).
fn roll(rng: &mut DfRandom) -> f32 {
    // `DFRandom.rand()` is `(next >> 16) & 0x7FFF` ⇒ `0…32767`: map it onto
    // the source's `Random.Range(0f, 100f)` half-open range.
    f32::from(u16::try_from(rng.rand() % 32_768).unwrap_or(0)) / 32_768.0 * 100.0
}

/// `PROBE-WEATHER-VISUAL-001` — exit code of the probe.
pub fn run_weather_visual_001() -> i32 {
    println!(
        "PROBE-WEATHER-VISUAL-001 - visual precipitation fed by the M13 climate index (M18 B3-d, CS-12)"
    );
    let mut ok = true;

    // ── A · spec vectors W1…W11 ──────────────────────────────────────────
    let w = |row: &WeatherRow, rand: f32| draw(row, rand);
    let woodlands_summer = &PUBLISHED_ROWS[8];
    let mountains_winter = &PUBLISHED_ROWS[2];
    let woodlands_winter = &PUBLISHED_ROWS[7];
    let vectors: [(&str, &WeatherRow, f32, WeatherKind); 11] = [
        ("W1", woodlands_summer, 0.0, WeatherKind::Sunny),
        ("W2", woodlands_summer, 60.1, WeatherKind::Cloudy),
        ("W3", woodlands_summer, 80.1, WeatherKind::Overcast),
        ("W4", woodlands_summer, 85.1, WeatherKind::Rain),
        ("W5", woodlands_summer, 95.1, WeatherKind::Thunder),
        ("W6", mountains_winter, 18.0, WeatherKind::Sunny),
        ("W7", mountains_winter, 18.01, WeatherKind::Cloudy),
        ("W8", mountains_winter, 65.0, WeatherKind::Fog),
        ("W9", mountains_winter, 65.01, WeatherKind::Snow),
        ("W10", woodlands_winter, 27.0, WeatherKind::Sunny),
        ("W11", woodlands_winter, 27.9, WeatherKind::Cloudy),
    ];
    let mut passed = 0usize;
    for (id, row, rand, want) in vectors {
        let got = w(row, rand);
        if got == want {
            passed += 1;
        } else {
            println!(
                "[FAIL] A {id} - {} roll={rand} got={} want={}",
                row.label(),
                got.as_str(),
                want.as_str()
            );
        }
    }
    let a_ok = passed == 11;
    ok &= a_ok;
    println!(
        "[{}] A spec_vectors - {passed}/11 (W1-W11 · CompileOdds order, Woodlands/Winter normalised ×100/90)",
        if a_ok { "OK" } else { "FAIL" }
    );

    // ── B · 10 000 seeded draws on the humid row ─────────────────────────
    let humid = &PUBLISHED_ROWS[4]; // Jungle/Spring (Rainforest)
    let mut counts_a = vec![0usize; WeatherKind::ORDER.len()];
    let mut counts_b = vec![0usize; WeatherKind::ORDER.len()];
    for pass in 0..2 {
        let mut rng = DfRandom::new();
        rng.srand(WEATHER_SEED);
        let target = if pass == 0 { &mut counts_a } else { &mut counts_b };
        for _ in 0..DRAWS {
            let kind = draw(humid, roll(&mut rng));
            let index = WeatherKind::ORDER
                .iter()
                .position(|candidate| *candidate == kind)
                .unwrap_or(0);
            target[index] += 1;
        }
    }
    let digest_a = digest_of(&counts_a);
    let digest_b = digest_of(&counts_b);
    let mut max_delta = 0.0f32;
    let mut delta_rows = Vec::new();
    for (index, kind) in WeatherKind::ORDER.iter().enumerate() {
        let observed = counts_a[index] as f32 / DRAWS as f32;
        let expected = humid.percent(*kind) / 100.0;
        let delta = (observed - expected).abs();
        max_delta = max_delta.max(delta);
        delta_rows.push(format!(
            "{} {:.4}/{:.4} Δ{:.4}",
            kind.as_str(),
            observed,
            expected,
            delta
        ));
    }
    let b_ok = digest_a == digest_b && max_delta < MAX_ABS_DELTA;
    ok &= b_ok;
    println!(
        "[{}] B seeded_draws - {} draws · {} · max_abs_delta {:.4} < {} · digest 0x{:016x} ×2 {}",
        if b_ok { "OK" } else { "FAIL" },
        DRAWS,
        humid.label(),
        max_delta,
        MAX_ABS_DELTA,
        digest_a,
        if digest_a == digest_b { "identical" } else { "DIFFER" }
    );
    println!("      {}", delta_rows.join(" · "));

    // ── C · two-sided control over the whole roll domain ─────────────────
    let dry = &PUBLISHED_ROWS[1]; // Desert/Summer
    let mut dry_wet_rolls = 0usize;
    let mut dry_drops_max = 0usize;
    let mut wet_rain_rolls = 0usize;
    let mut wet_drops_max = 0usize;
    let mut steps = 0usize;
    let mut roll_value = 0.0f32;
    while roll_value <= 100.0 {
        let dry_kind = draw(dry, roll_value);
        let dry_drops = drops(dry, dry_kind);
        if dry_kind.is_precipitation() || dry_drops != 0 {
            dry_wet_rolls += 1;
        }
        dry_drops_max = dry_drops_max.max(dry_drops);
        let wet_kind = draw(humid, roll_value);
        if wet_kind == WeatherKind::Rain {
            wet_rain_rolls += 1;
        }
        wet_drops_max = wet_drops_max.max(drops(humid, wet_kind));
        steps += 1;
        roll_value = (steps as f32) / 10.0;
    }
    let c_ok = dry_wet_rolls == 0 && dry_drops_max == 0 && wet_rain_rolls > 0 && wet_drops_max > 0;
    ok &= c_ok;
    println!(
        "[{}] C dry_negative - {} rolls 0.0..100.0: {} rain/snow/thunder rolls, max drops {} · wet control {}: {} rain rolls, max drops {}",
        if c_ok { "OK" } else { "FAIL" },
        steps,
        dry_wet_rolls,
        dry_drops_max,
        humid.label(),
        wet_rain_rolls,
        wet_drops_max
    );
    println!("      {}", Precipitation { climate: 224, row: dry, roll: 50.0, kind: draw(dry, 50.0), drops: drops(dry, draw(dry, 50.0)) }.evidence());
    println!("      {}", Precipitation { climate: 227, row: humid, roll: 50.0, kind: draw(humid, 50.0), drops: drops(humid, draw(humid, 50.0)) }.evidence());

    // ── D · retail CLIMATE.PAK (M13, read-only) ──────────────────────────
    let climate_path = match crate::mode_a_arena2_path("CLIMATE.PAK") {
        Ok(path) => path,
        Err(reason) => {
            println!("CLIMATE.PAK not accessible: {reason}");
            return 2;
        }
    };
    let bytes = match std::fs::read(&climate_path) {
        Ok(bytes) => bytes,
        Err(err) => {
            println!("CLIMATE.PAK not readable: {climate_path:?} ({err})");
            return 2;
        }
    };
    let map = match ClimateMap::parse(&bytes) {
        Ok(map) => map,
        Err(err) => {
            println!("[FAIL] D climate_parse - CLIMATE.PAK: {err:?}");
            return 1;
        }
    };
    let distinct = map.distinct_climates();
    let mut families = Vec::new();
    let mut d_ok = distinct.len() == 10;
    for value in &distinct {
        let Ok(climate) = WorldClimate::from_index(*value) else {
            d_ok = false;
            println!("[FAIL] D climate_range - value {value} outside Climates (223..=232)");
            continue;
        };
        let precipitation = match Precipitation::resolve(*value, 50.0) {
            Ok(precipitation) => precipitation,
            Err(err) => {
                d_ok = false;
                println!("[FAIL] D climate_resolve - {value}: {err:?}");
                continue;
            }
        };
        families.push(format!(
            "{}→{} {} drops={}",
            climate.name(),
            precipitation.row.label(),
            precipitation.kind.as_str(),
            precipitation.drops
        ));
    }
    let woodlands_dry = Precipitation::resolve(224, 50.0).map(|p| p.drops) == Ok(0);
    let rainforest_wet = Precipitation::resolve(227, 50.0).is_ok_and(|p| p.drops > 0);
    d_ok &= woodlands_dry && rainforest_wet;
    ok &= d_ok;
    println!(
        "[{}] D climate_index - CLIMATE.PAK {} B · {} distinct values · Desert(224) roll 50 ⇒ {} drops · Rainforest(227) roll 50 ⇒ {} drops",
        if d_ok { "OK" } else { "FAIL" },
        bytes.len(),
        distinct.len(),
        if woodlands_dry { 0 } else { 1 },
        if rainforest_wet { ">0" } else { "0" }
    );
    println!("      {}", families.join(" · "));
    let season_ok = Season::of_month(5) == Season::Summer;
    if !season_ok {
        println!("[FAIL] D season_table - month 5 must be Summer (spec §3.2)");
        ok = false;
    }

    if ok {
        println!(
            "VERDICT: PROBE-WEATHER-VISUAL-001 PASS - 11/11 vectors · {} draws max Δ{:.4} · dry 0 drops / wet {} drops · 10 climates resolved from CLIMATE.PAK",
            DRAWS, max_delta, wet_drops_max
        );
        0
    } else {
        println!("VERDICT: PROBE-WEATHER-VISUAL-001 FAIL");
        1
    }
}

// ── B3-c · map panels (`PROBE-PANELS-001`) ─────────────────────────────────

/// `PROBE-PANELS-001` — **B3-c (O7)** census and composition of the 6 retail
/// panels (`hud`, `inventory`, `quests`, `main-menu`, `map-local`, `map-world`).
///
/// - **A** — census: `ui_assets::session_panels` returns **6** panels, the 6
///   ids are the expected ones and `assert_no_generic_widgets` reports **0**
///   violations (C7a: only `sprite`/`glyph-run`, `.FNT` registry untouched).
/// - **B** — the three assets the map/menu screens are made of compose:
///   `MENU.IMG` (main menu), `MAP100I0.IMG` (`map-local`), `TRAV0I00.IMG`
///   (`map-world`) — each panel composes with `opaque > 0` pixels of its own
///   asset and `palette_compliance` empty.
/// - **C** — the per-asset range rule is **not** a global widening: for each
///   recorded derivation the measured span of the retail bytes is printed and
///   must be contained in the resolved range; a `Map` asset **without** a
///   derivation that paints index 5 is still a `PaletteViolation` (synthetic
///   fixture, 0 retail bytes).
///
/// `EXIT=0` ⟺ A∧B∧C; `EXIT=2` when the ARENA2 assets are not readable;
/// `EXIT=1` on FAIL.
pub fn run_panels_001() -> i32 {
    use std::collections::BTreeMap;

    use daggerfall_parser_bevy_bridge::ui_spec::{
        assert_no_generic_widgets, build_map_panels, compose, palette_compliance, palette_hash,
        UiAnchor, UiAssets, UiNode, UiNodeKind, UiPanel, UiSpecError,
        CANVAS_HEIGHT, CANVAS_WIDTH,
    };
    use daggerfall_parser_rs::image::{
        observed_index_range, DecodedImage, ImageKind, ASSET_RANGE_DERIVATIONS,
    };
    use daggerfall_parser_rs::texture::Palette;

    const EXPECTED: [&str; 6] = [
        "hud",
        "inventory",
        "quests",
        "main-menu",
        "map-local",
        "map-world",
    ];

    println!("PROBE-PANELS-001 - B3-c map panels: census of 6 + per-asset palette range (M18)");
    let dir = match crate::mode_a_arena2_dir() {
        Ok(dir) => dir,
        Err(reason) => {
            println!("ARENA2 not accessible: {reason}");
            println!("reason=missing_arena2");
            println!("EXIT=2");
            println!("VERDICT: PROBE-PANELS-001 NO DATA - ARENA2 not accessible");
            return 2;
        }
    };
    let assets = match crate::ui_assets::load_ui_assets(&dir) {
        Ok(assets) => assets,
        Err(reason) => {
            println!("ARENA2 assets not loadable: {reason}");
            println!("reason=missing_ui_assets");
            println!("EXIT=2");
            println!("VERDICT: PROBE-PANELS-001 NO DATA - UI assets not loadable");
            return 2;
        }
    };
    let panels = match crate::ui_assets::session_panels(&assets) {
        Ok(panels) => panels,
        Err(reason) => {
            println!("[FAIL] A census - session_panels: {reason}");
            println!("EXIT=1");
            println!("VERDICT: PROBE-PANELS-001 FAIL");
            return 1;
        }
    };
    let mut ok = true;

    // ── A · census ───────────────────────────────────────────────────────
    let ids: Vec<&str> = panels.iter().map(|panel| panel.id.as_str()).collect();
    let violations = assert_no_generic_widgets(&panels);
    let a_ok = panels.len() == EXPECTED.len() && ids == EXPECTED && violations.is_empty();
    ok &= a_ok;
    println!(
        "[{}] A census - panels={} ids=[{}] generic_widgets={} nodes={}",
        if a_ok { "OK" } else { "FAIL" },
        panels.len(),
        ids.join(" "),
        violations.len(),
        panels.iter().map(|panel| panel.nodes.len()).sum::<usize>()
    );
    if panels.len() != EXPECTED.len() {
        println!("      [FAIL] expected {EXPECTED:?}");
    }
    for violation in &violations {
        println!("      [FAIL] {violation}");
    }

    // ── B · the three screen assets compose ──────────────────────────────
    let mut composed: BTreeMap<String, usize> = BTreeMap::new();
    let mut b_ok = true;
    for panel in &panels {
        let framebuffer = match compose(std::slice::from_ref(panel), &assets) {
            Ok(framebuffer) => framebuffer,
            Err(reason) => {
                b_ok = false;
                println!("      [FAIL] B {} - {reason}", panel.id.as_str());
                continue;
            }
        };
        let compliance = palette_compliance(&framebuffer);
        let mut per_asset: BTreeMap<String, usize> = BTreeMap::new();
        for (at, index) in framebuffer.pixels.iter().enumerate() {
            if *index == 0 {
                continue;
            }
            let Some(entry) = framebuffer.palettes.get(framebuffer.slots[at] as usize) else {
                continue;
            };
            *per_asset.entry(entry.asset.clone()).or_insert(0) += 1;
        }
        for (name, count) in per_asset {
            *composed.entry(name).or_insert(0) += count;
        }
        let opaque = framebuffer.opaque_pixels();
        let panel_ok = opaque > 0 && compliance.is_empty();
        b_ok &= panel_ok;
        println!(
            "      panel {}: opaque={} palette_violations={}",
            panel.id.as_str(),
            opaque,
            compliance.len()
        );
    }
    let screens = [
        ("MENU.IMG", "main-menu"),
        ("MAP100I0.IMG", "map-local"),
        ("TRAV0I00.IMG", "map-world"),
    ];
    for (asset, panel) in screens {
        let pixels = composed.get(asset).copied().unwrap_or(0);
        let panel_ok = pixels > 0;
        b_ok &= panel_ok;
        println!(
            "      {} composes panel {panel}: {pixels} painted px {}",
            asset,
            if panel_ok { "OK" } else { "FAIL" }
        );
    }
    ok &= b_ok;
    println!(
        "[{}] B screen_assets - MENU/MAP100I0/TRAV0I00 compose with palette compliance",
        if b_ok { "OK" } else { "FAIL" }
    );

    // ── C · per-asset range derives from the asset (never a global widening)
    let mut c_ok = true;
    for (asset, low, high, reason) in ASSET_RANGE_DERIVATIONS {
        let name = format!("{asset}.IMG");
        let Some(decoded) = assets.image(&name) else {
            c_ok = false;
            println!("      [FAIL] C {name} - not in the asset bank");
            continue;
        };
        let observed = observed_index_range(&decoded.image);
        let resolved = daggerfall_parser_rs::image::asset_index_range_resolved(&name);
        let span_ok = match &observed {
            Some(span) => resolved.contains(span.start()) && resolved.contains(span.end()),
            None => false,
        };
        let canvas_ok = decoded_ok(&decoded.image);
        c_ok &= span_ok && canvas_ok;
        println!(
            "      {name}: {} B · observed {:?} · resolved {}..={} · map floor {} · derivation {low}..={high} \"{reason}\"",
            decoded.image.pixels.len(),
            observed,
            resolved.start(),
            resolved.end(),
            daggerfall_parser_rs::image::family_index_range(
                daggerfall_parser_rs::image::PaletteFamily::Map
            )
            .start()
        );
    }
    // Negative control: a `Map` asset with no derivation still violates.
    let mut colors = [[0u8; 3]; 256];
    for (index, color) in colors.iter_mut().enumerate() {
        *color = [index as u8, 0, 0];
    }
    let map_palette = Palette { colors };
    let mut pixels = vec![0u8; usize::from(CANVAS_WIDTH) * usize::from(CANVAS_HEIGHT)];
    pixels[0] = 5;
    let synthetic = UiAssets::new(map_palette, map_palette, map_palette).with_image(
        "FMAP9I99.IMG",
        DecodedImage {
            x_offset: 0,
            y_offset: 0,
            width: CANVAS_WIDTH,
            height: CANVAS_HEIGHT,
            compression: 0,
            pixels,
            embedded_palette: None,
            kind: ImageKind::Rci,
        },
    );
    let negative_ok = match synthetic {
        Ok(synthetic) => {
            let panel = UiPanel {
                id: daggerfall_parser_bevy_bridge::ui_spec::UiPanelId::MapLocal,
                nodes: vec![UiNode {
                    id: "synthetic-low-index".to_string(),
                    kind: UiNodeKind::Sprite,
                    rect: daggerfall_parser_bevy_bridge::ui_spec::UiRect::canvas(),
                    anchor: UiAnchor::Declared("synthetic control"),
                    asset: "FMAP9I99.IMG".to_string(),
                    family: daggerfall_parser_rs::image::PaletteFamily::Map,
                    palette_hash: palette_hash(&map_palette),
                    source: None,
                    text: None,
                    ink: 0,
                }],
            };
            matches!(
                compose(std::slice::from_ref(&panel), &synthetic),
                Err(UiSpecError::PaletteViolation { index: 5, .. })
            )
        }
        Err(reason) => {
            println!("      [FAIL] C negative_control - fixture rejected: {reason}");
            false
        }
    };
    c_ok &= negative_ok;
    ok &= c_ok;
    println!(
        "[{}] C per_asset_range - {} derivations · Map asset without derivation + index 5 ⇒ PaletteViolation ({negative_ok})",
        if c_ok { "OK" } else { "FAIL" },
        ASSET_RANGE_DERIVATIONS.len()
    );

    // ── D · the map panels are the retail screens (ids + node inventory) ──
    let maps = match build_map_panels(&assets) {
        Ok(maps) => maps,
        Err(reason) => {
            println!("[FAIL] D map_panels - {reason}");
            println!("EXIT=1");
            println!("VERDICT: PROBE-PANELS-001 FAIL");
            return 1;
        }
    };
    let mut d_ok = true;
    for panel in &maps {
        let inventory = panel.node_inventory();
        let detail = inventory
            .iter()
            .map(|(kind, count)| format!("{kind}={count}"))
            .collect::<Vec<_>>()
            .join(" ");
        let node_ok = panel.nodes.len() == 1
            && inventory.get("sprite").copied() == Some(1)
            && panel.nodes.iter().all(|node| {
                node.asset == "MAP100I0.IMG" || node.asset == "TRAV0I00.IMG"
            });
        d_ok &= node_ok;
        println!(
            "      panel {}: nodes={} [{detail}] asset={} fingerprint={:#018x}",
            panel.id.as_str(),
            panel.nodes.len(),
            panel.nodes.first().map(|node| node.asset.as_str()).unwrap_or("-"),
            panel.palette_fingerprint()
        );
    }
    ok &= d_ok;
    println!(
        "[{}] D map_panels - map-local=MAP100I0 map-world=TRAV0I00 · 1 sprite node each · distinct fingerprints {}",
        if d_ok { "OK" } else { "FAIL" },
        maps[0].palette_fingerprint() != maps[1].palette_fingerprint()
    );


    if ok {
        println!(
            "VERDICT: PROBE-PANELS-001 PASS - census=6 (hud inventory quests main-menu map-local map-world) · generic_widgets=0 · MENU/MAP100I0/TRAV0I00 compose · per-asset range from the asset bytes"
        );
        0
    } else {
        println!("VERDICT: PROBE-PANELS-001 FAIL");
        1
    }
}

/// The asset decodes as a raw 320×200 palette-index canvas (no invented
/// geometry): width/height match the canvas and it carries pixels.
fn decoded_ok(image: &daggerfall_parser_rs::image::DecodedImage) -> bool {
    image.width == 320 && image.height == 200 && !image.pixels.is_empty()
}

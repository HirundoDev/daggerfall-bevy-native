//! `PROBE-LIGHT-001` — M34 §1.6 «iluminación» (S47): the five light sources of
//! W34-6 measured against the closed rule table (spec
//! `m34-e2e-integration-presentation-spec-CR-5243af63.md:109-123`, GV-10/11/12/14).
//!
//! ## What is measured
//!
//! Every row of the §1.6 table is recomputed from
//! [`daggerfall_perception::lighting`] and compared against the **literal of
//! the spec**, derived in-code from the cited `DFU@0038a1e6` line — never
//! copied from the module's own constants (a constant compared to itself is
//! not a measurement):
//!
//! | Block | Asserted | Source |
//! |---|---|---|
//! | Torch | base `1.0`, item `1.25`, ratio **exactly** `1.25`; `intensityMod ∈ {1, 1.25, 0.85 + cos(x)·0.2}` (band checked over 33 sampled phases); guttering gate at `currentCondition < 3`; lit outside with `IsCityLightsOn` **or** inside a dungeon | `EnablePlayerTorch.cs:31-32,64,93-94,98,105,108` |
//! | Ambient | the six values `0.9 · 0.25 · 0.18 · (0.20,0.18,0.20) · 0.12 · 0.58` | `PlayerAmbientLight.cs:25-35` |
//! | Fade | `increment == 0.1/3`, `poll == 1/3`, 30 steps land **exactly** on the target | `PlayerAmbientLight.cs:34-35,58,101-107` |
//! | GV-11 | `round(x·255)` ⇒ `0.9→230 · 0.18→46 · 0.12→31 · 0.58→148` | `PlayerAmbientLight.cs:25-32` → byte conversion |
//! | Sun | `dawn = 6·60 = 360`, `dayRange = (18−6)·60 = 720`, `time = (m−dawn)/dayRange`, `xrot = 180·time`; the two ends `xrot 0` (dawn) and `179.75` (dusk−1 min); out-of-range `MinuteOfDay` keeps `xrot_raw` outside and reports the declared clamp `[0,180]` | `SunlightManager.cs:110-115`, `DaggerfallDateTime.cs:48-49` |
//! | Underwater fog | `in = 0.025·(−waterLevel) + 0.38` (GV-13 level `−128`), `out = in − 0.02`, `fogT` at both extremes `0`/`1`, density inside `0…0.25` | `UnderwaterFog.cs:30,45-51,70` |
//! | Dungeon light | `2060 cu · 0.025 = 51.5 m` (exact in `f32`), refresh `0.4 s` | `DungeonLightHandler.cs:25-26,60` |
//!
//! A parity check closes the day/night duplication: this lane's
//! `DAWN_HOUR`/`DUSK_HOUR` must equal `daggerfall_heart::clock`'s (the
//! time-of-day predicate the ambient selector consumes).
//!
//! ## Digest transcript (so an independent run can be compared)
//!
//! Each measurement appends one canonical line `label=value|value_bits` to a
//! transcript; the digest is FNV-1a 64 over that transcript joined with `\n`
//! (decimal `{:.9}` **and** raw `f32` bits, so a digest match is bit-identity,
//! not a printing coincidence). The transcript is built twice inside this
//! process and asserted equal; the whole binary is run twice in two separate
//! processes, whose printed digests must also match. Pure arithmetic, no
//! retail data, no clock: `EXIT=2` does not exist here — `EXIT=0` PASS ·
//! `EXIT=1` any assertion FAIL.
//!
//! No numeric deviation is silent: every asserted literal is derived from the
//! cited line; a measured value that disagreed would be printed and reported
//! as `[DESVÍO]` (as W34-4 did with the GV-08 unit), never rounded into a
//! pass.

use daggerfall_perception::lighting as light;

/// FNV-1a 64 offset basis.
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a 64 prime.
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a 64 continued from `state`.
fn fnv1a64(bytes: &[u8], state: u64) -> u64 {
    let mut hash = state;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Canonical measurement transcript: `label=value|bits` lines.
#[derive(Default)]
struct Transcript {
    lines: Vec<String>,
}

impl Transcript {
    /// Append one `f32` measurement (decimal + raw bits).
    fn push(&mut self, label: &str, value: f32) {
        self.lines
            .push(format!("{label}={value:.9}|{:#010x}", value.to_bits()));
    }

    /// Append one integer measurement.
    fn push_i(&mut self, label: &str, value: i64) {
        self.lines.push(format!("{label}={value}"));
    }

    /// FNV-1a 64 over `\n`-joined lines.
    fn digest(&self) -> u64 {
        fnv1a64(self.lines.join("\n").as_bytes(), FNV_OFFSET)
    }
}

/// `check` with a printable detail.
fn check(passed: &mut bool, condition: bool, label: &str, detail: String) {
    if !condition {
        *passed = false;
    }
    println!(
        "[{}] {label} - {detail}",
        if condition { "OK" } else { "FAIL" }
    );
}

/// One full measurement pass: asserts everything and fills the transcript.
///
/// Split from [`run_light_001`] so the digest can be recomputed a second time
/// in the same process without repeating the (deterministic) printing.
fn measure(passed: &mut bool, verbose: bool) -> Transcript {
    let mut t = Transcript::default();

    // ── Torch — EnablePlayerTorch.cs:31-32,64,93-94,98,105,108 ───────────
    let base = light::TorchState::new(false).intensity(0.0);
    let with_item = light::TorchState::new(true).intensity(0.0);
    let ratio = with_item / base;
    t.push("torch.base", base);
    t.push("torch.item", with_item);
    t.push("torch.ratio", ratio);
    check(
        passed,
        base == 1.0 && with_item == 1.25,
        "torch literals 1.0 / 1.25 (EnablePlayerTorch.cs:31-32,64,98)",
        format!("base={base} item={with_item}"),
    );
    check(
        passed,
        ratio == 1.25,
        "torch ratio exactly 1.25",
        format!("{with_item}/{base} = {ratio} ({:#010x})", ratio.to_bits()),
    );

    // intensityMod set: {1.0, 1.25, 0.85 + cos(x)·0.2} (:93-94).
    let base_mod = light::TorchModifier::Base.factor();
    let item_mod = light::TorchModifier::Item.factor();
    t.push("torch.mod.base", base_mod);
    t.push("torch.mod.item", item_mod);
    let mut band_min = f32::MAX;
    let mut band_max = f32::MIN;
    let mut phases = 0;
    for step in 0..=32_u32 {
        let phase = step as f32 * std::f32::consts::TAU / 32.0;
        let factor = light::TorchModifier::Guttering {
            phase_radians: phase,
        }
        .factor();
        band_min = band_min.min(factor);
        band_max = band_max.max(factor);
        phases += 1;
    }
    t.push("torch.gutter.band_min", band_min);
    t.push("torch.gutter.band_max", band_max);
    t.push_i("torch.gutter.phases", i64::from(phases));
    check(
        passed,
        base_mod == 1.0 && item_mod == 1.25,
        "intensityMod base/item (EnablePlayerTorch.cs:93-94,108)",
        format!("{base_mod} / {item_mod}"),
    );
    // 0.85 ± 0.2 over the sampled circle, plus the endpoints at phase 0 / π.
    check(
        passed,
        (band_min - 0.65).abs() < 1e-5 && (band_max - 1.05).abs() < 1e-5,
        "guttering band 0.85 ± 0.2",
        format!("[{band_min}, {band_max}] over {phases} phases"),
    );
    let mut gate_ok = true;
    for condition in 0..=6_i32 {
        let modifier = light::torch_modifier_for_condition(condition, 0.5);
        let guttering = matches!(modifier, light::TorchModifier::Guttering { .. });
        if guttering != (condition < 3) {
            gate_ok = false;
            if verbose {
                println!("  [FAIL] condition {condition} -> {modifier:?}");
            }
        }
    }
    check(
        passed,
        gate_ok,
        "guttering gate currentCondition < 3 (EnablePlayerTorch.cs:93-94)",
        "conditions 0..=6: guttering iff < 3".to_string(),
    );
    // Final product = torchIntensity · PlayerTorchLightScale · intensityMod
    // (:31-32,64,98,108), checked against an independent recomputation.
    let phase = 1.234_f32;
    let mut state = light::TorchState::new(true);
    state.player_torch_light_scale = 0.75;
    state.current_condition = 2;
    let measured = state.intensity(phase);
    let recomputed = 1.25_f32 * 0.75_f32 * (0.85_f32 + phase.cos() * 0.2_f32);
    t.push("torch.final", measured);
    t.push("torch.final.recomputed", recomputed);
    check(
        passed,
        (measured - recomputed).abs() < 1e-6,
        "final = torchIntensity · PlayerTorchLightScale · intensityMod",
        format!("measured={measured:.9} recomputed={recomputed:.9}"),
    );
    // Torch lit: exterior with city lights ON, or inside a dungeon (:105).
    let mut activation_ok = true;
    for (exterior, lights, dungeon) in [
        (true, true, false),
        (true, false, false),
        (false, true, false),
        (false, false, true),
        (false, false, false),
        (true, true, true),
    ] {
        let lit = light::torch_active(exterior, lights, dungeon);
        let expected = dungeon || (exterior && lights);
        if lit != expected {
            activation_ok = false;
        }
    }
    check(
        passed,
        activation_ok,
        "torch lit: exterior+IsCityLightsOn or dungeon (EnablePlayerTorch.cs:105)",
        "6-combination truth table".to_string(),
    );

    // ── Ambient — PlayerAmbientLight.cs:25-35 ────────────────────────────
    // Literals derived from the spec row, not read from the module constants.
    let expected_ambient: [(light::AmbientContext, [f32; 3]); 6] = [
        (light::AmbientContext::ExteriorDay, [0.9, 0.9, 0.9]),
        (light::AmbientContext::ExteriorNight, [0.25, 0.25, 0.25]),
        (light::AmbientContext::Interior, [0.18, 0.18, 0.18]),
        (light::AmbientContext::InteriorNight, [0.20, 0.18, 0.20]),
        (light::AmbientContext::Dungeon, [0.12, 0.12, 0.12]),
        (light::AmbientContext::Castle, [0.58, 0.58, 0.58]),
    ];
    let mut ambient_ok = true;
    for (context, expected) in expected_ambient {
        let measured = context.light();
        let exact =
            measured.r == expected[0] && measured.g == expected[1] && measured.b == expected[2];
        if !exact {
            ambient_ok = false;
        }
        t.push(&format!("ambient.{}.r", context.as_str()), measured.r);
        t.push(&format!("ambient.{}.g", context.as_str()), measured.g);
        t.push(&format!("ambient.{}.b", context.as_str()), measured.b);
        if verbose {
            println!(
                "  {} = {} (expected {},{},{}) u8 {:?}",
                context.as_str(),
                measured.label(),
                expected[0],
                expected[1],
                expected[2],
                measured.to_u8()
            );
        }
    }
    check(
        passed,
        ambient_ok,
        "six ambient values (PlayerAmbientLight.cs:25-35, GV-10)",
        "0.9 · 0.25 · 0.18 · (0.20,0.18,0.20) · 0.12 · 0.58".to_string(),
    );
    check(
        passed,
        light::AmbientContext::ALL.len() == 6,
        "ambient context count",
        format!("{} contexts", light::AmbientContext::ALL.len()),
    );

    // ── Fade — PlayerAmbientLight.cs:34-35,58,101-107 ────────────────────
    let increment = light::FADE_INCREMENT;
    let poll = light::FADE_POLL_S;
    let derived = light::FADE_STEP_S / light::FADE_DURATION_S;
    t.push("fade.increment", increment);
    t.push("fade.increment.derived", derived);
    t.push("fade.poll", poll);
    t.push_i("fade.steps", i64::from(light::FADE_STEPS));
    check(
        passed,
        increment == derived && (increment - 0.1_f32 / 3.0).abs() <= f32::EPSILON,
        "fade increment == 0.1/3 (PlayerAmbientLight.cs:34-35,101-107)",
        format!("{increment:.9} vs 0.1/3 = {:.9}", 0.1_f32 / 3.0),
    );
    check(
        passed,
        (poll - 1.0_f32 / 3.0).abs() <= f32::EPSILON,
        "fade poll == 1/3 s (PlayerAmbientLight.cs:58)",
        format!("{poll:.9}"),
    );
    let night = light::AmbientContext::ExteriorNight.light();
    let day = light::AmbientContext::ExteriorDay.light();
    let end = light::fade_colour(night, day, light::FADE_STEPS);
    t.push("fade.end.r", end.r);
    t.push(
        "fade.half.r",
        light::fade_colour(night, day, light::FADE_STEPS / 2).r,
    );
    check(
        passed,
        end == day && light::FADE_STEPS == 30,
        "30 steps (3 s / 0.1 s) land exactly on the target",
        format!(
            "{} -> {}, mid {:.6}",
            night.label(),
            end.label(),
            light::fade_colour(night, day, light::FADE_STEPS / 2).r
        ),
    );
    // Day scaled: Lerp(night, midday, DaylightScale·ScaleFactor) (:114-118).
    let scaled = light::day_scaled(night, day, 0.5, 1.0);
    let scaled_expected = 0.25_f32 + (0.9_f32 - 0.25_f32) * 0.5_f32;
    t.push("day_scaled.half.r", scaled.r);
    t.push("day_scaled.half.expected_r", scaled_expected);
    check(
        passed,
        (scaled.r - scaled_expected).abs() < 1e-6,
        "day scaled = Lerp(night, midday, DaylightScale·ScaleFactor)",
        format!("{:.9} vs {:.9}", scaled.r, scaled_expected),
    );

    // ── GV-11 — round(x·255) ─────────────────────────────────────────────
    let gv11: [([u8; 3], [u8; 3], &str); 4] = [
        (
            light::AmbientContext::ExteriorDay.light().to_u8(),
            [230; 3],
            "0.9",
        ),
        (
            light::AmbientContext::Interior.light().to_u8(),
            [46; 3],
            "0.18",
        ),
        (
            light::AmbientContext::Dungeon.light().to_u8(),
            [31; 3],
            "0.12",
        ),
        (
            light::AmbientContext::Castle.light().to_u8(),
            [148; 3],
            "0.58",
        ),
    ];
    let mut gv11_ok = true;
    for (measured, expected, literal) in gv11 {
        if measured != expected {
            gv11_ok = false;
        }
        t.push_i(&format!("gv11.{literal}"), i64::from(measured[0]));
        if verbose {
            println!(
                "  GV-11 round({literal}·255) = {:?} (expected {:?})",
                measured, expected
            );
        }
    }
    check(
        passed,
        gv11_ok,
        "GV-11 round(x·255): 230 / 46 / 31 / 148",
        "u8 per channel".to_string(),
    );

    // ── Sun — SunlightManager.cs:110-115, DaggerfallDateTime.cs:48-49 ────
    let dawn_minute = light::DAWN_HOUR * light::MINUTES_PER_HOUR;
    let day_range = (light::DUSK_HOUR - light::DAWN_HOUR) * light::MINUTES_PER_HOUR;
    t.push_i("sun.dawn_minute", i64::from(dawn_minute));
    t.push_i("sun.day_range", i64::from(day_range));
    check(
        passed,
        light::DAWN_HOUR == 6 && light::DUSK_HOUR == 18 && dawn_minute == 360 && day_range == 720,
        "dawn 6 h / dusk 18 h ⇒ dawn 360 min, dayRange 720 min",
        format!("dawn={dawn_minute} dayRange={day_range}"),
    );
    // Parity with the clock that decides day/night for the ambient selector.
    check(
        passed,
        light::DAWN_HOUR == daggerfall_heart::clock::DAWN_HOUR
            && light::DUSK_HOUR == daggerfall_heart::clock::DUSK_HOUR,
        "dawn/dusk parity with heart::clock",
        format!(
            "perception {}/{} vs heart {}/{}",
            light::DAWN_HOUR,
            light::DUSK_HOUR,
            daggerfall_heart::clock::DAWN_HOUR,
            daggerfall_heart::clock::DUSK_HOUR
        ),
    );
    let mut sun_ok = true;
    for minute in [6 * 60, 12 * 60, 18 * 60, 6 * 60 - 1, 18 * 60 - 1] {
        let sun = light::sun_orientation(minute);
        let expected_time = (minute as f32 - 360.0) / 720.0;
        let expected_xrot = 180.0 * expected_time;
        let formula = (sun.time_raw - expected_time).abs() < 1e-6
            && (sun.xrot_raw_degrees - expected_xrot).abs() < 1e-5;
        if !formula {
            sun_ok = false;
        }
        t.push(&format!("sun.{minute}.time_raw"), sun.time_raw);
        t.push(&format!("sun.{minute}.xrot"), sun.xrot_raw_degrees);
        if verbose {
            println!(
                "  MinuteOfDay {minute:>4} -> time {:.6} xrot {:.4}° (clamped {:.4}°{})",
                sun.time_raw,
                sun.xrot_raw_degrees,
                sun.xrot_degrees,
                if sun.clamped { " [CLAMP]" } else { "" }
            );
        }
    }
    check(
        passed,
        sun_ok,
        "xrot = 180·(MinuteOfDay−360)/720 for 06:00·12:00·18:00·ends",
        "5 sample minutes".to_string(),
    );
    let dawn = light::sun_orientation(6 * 60);
    let dusk_edge = light::sun_orientation(18 * 60 - 1);
    check(
        passed,
        dawn.xrot_degrees == 0.0 && (dusk_edge.xrot_degrees - 179.75).abs() < 1e-3,
        "sun ends: dawn 0°, dusk−1 min 179.75°",
        format!("{:.6}° / {:.6}°", dawn.xrot_degrees, dusk_edge.xrot_degrees),
    );
    // Out of range: raw leaves [0,180]; the clamp is declared and reported.
    let midnight = light::sun_orientation(0);
    let late = light::sun_orientation(23 * 60 + 59);
    t.push("sun.midnight.time_raw", midnight.time_raw);
    t.push("sun.midnight.xrot_raw", midnight.xrot_raw_degrees);
    t.push("sun.midnight.xrot_clamped", midnight.xrot_degrees);
    t.push("sun.late.time_raw", late.time_raw);
    t.push("sun.late.xrot_raw", late.xrot_raw_degrees);
    t.push("sun.late.xrot_clamped", late.xrot_degrees);
    check(
        passed,
        midnight.time_raw < 0.0
            && midnight.clamped
            && midnight.xrot_raw_degrees == -90.0
            && midnight.xrot_degrees == 0.0
            && late.time_raw > 1.0
            && late.clamped
            && late.xrot_degrees == 180.0,
        "out-of-range MinuteOfDay: declared clamp xrot ∈ [0, 180]",
        format!(
            "00:00 raw {:.3}° → clamped {:.1}° · 23:59 raw {:.3}° → clamped {:.1}° [{DESVIO_NONE}]",
            midnight.xrot_raw_degrees,
            midnight.xrot_degrees,
            late.xrot_raw_degrees,
            late.xrot_degrees
        ),
    );

    // ── Underwater fog — UnderwaterFog.cs:30,45-51,70 ────────────────────
    // GV-13 synthetic level of W34-3: blockWaterLevel = −128.
    let water_level = -128.0_f32;
    let fog = light::UnderwaterFog::from_water_level(water_level);
    let expected_entry = 0.025_f32 * (-water_level) + 0.38_f32;
    t.push("fog.entry", fog.entry_y);
    t.push("fog.entry.expected", expected_entry);
    t.push("fog.exit", fog.exit_y);
    t.push("fog.exit.expected", expected_entry - 0.02);
    let t_entry = fog.fog_t(fog.entry_y);
    let t_exit = fog.fog_t(fog.exit_y);
    t.push("fog.t.entry", t_entry);
    t.push("fog.t.exit", t_exit);
    t.push("fog.density.min", fog.density(fog.entry_y));
    t.push("fog.density.max", fog.density(fog.exit_y));
    check(
        passed,
        fog.entry_y == expected_entry && fog.exit_y == expected_entry - 0.02,
        "fog entry = 0.025·(−waterLevel)+0.38, exit = entry−0.02 (UnderwaterFog.cs:30,45-51)",
        format!(
            "in={:.9} out={:.9} (level {water_level})",
            fog.entry_y, fog.exit_y
        ),
    );
    check(
        passed,
        t_entry == 0.0 && t_exit == 1.0,
        "fogT extremes: 0 at entry plane, 1 at exit plane",
        format!("fogT(entry)={t_entry} fogT(exit)={t_exit}"),
    );
    check(
        passed,
        fog.density(fog.entry_y) == 0.0
            && fog.density(fog.exit_y) == light::UNDERWATER_FOG_DENSITY_MAX
            && light::UNDERWATER_FOG_DENSITY_MAX == 0.25,
        "fog density band 0…0.25 (UnderwaterFog.cs:70)",
        format!(
            "[{}, {}]",
            fog.density(fog.entry_y),
            fog.density(fog.exit_y)
        ),
    );

    // ── Dungeon light — DungeonLightHandler.cs:25-26,60 ──────────────────
    let range_m = light::DUNGEON_LIGHT_RANGE_METERS;
    let expected_range = 2060.0_f32 * 0.025_f32;
    t.push("dungeon.range_m", range_m);
    t.push("dungeon.range_m.expected", expected_range);
    t.push("dungeon.refresh_s", light::DUNGEON_LIGHT_REFRESH_S);
    check(
        passed,
        range_m == 51.5 && range_m == expected_range,
        "dungeon light range 2060 cu · 0.025 = 51.5 m (DungeonLightHandler.cs:25-26)",
        format!("{range_m:.9} m (= {expected_range:.9})"),
    );
    check(
        passed,
        light::DUNGEON_LIGHT_REFRESH_S == 0.4,
        "dungeon light refresh 0.4 s (DungeonLightHandler.cs:60)",
        format!("{:.9} s", light::DUNGEON_LIGHT_REFRESH_S),
    );
    check(
        passed,
        light::dungeon_light_reaches(range_m)
            && light::dungeon_light_reaches(-range_m)
            && !light::dungeon_light_reaches(range_m + 0.01),
        "dungeon light reach closed at ±51.5 m",
        format!("±{range_m} m reached, +0.01 m not"),
    );

    t
}

/// `PROBE-LIGHT-001` entry point: prints, returns the process exit code.
pub fn run_light_001() -> i32 {
    println!("== PROBE-LIGHT-001 — M34 §1.6 lighting, S47 (W34-6) ==");
    println!(
        "rules: spec m34-e2e-integration-presentation-spec-CR-5243af63.md:109-123 (GV-10/11/12/14)"
    );
    println!();

    let mut passed = true;
    let first = measure(&mut passed, true);
    let second = measure(&mut passed, false);
    let digest_a = first.digest();
    let digest_b = second.digest();
    println!();
    println!("transcript: {} measurements", first.lines.len());
    println!("digest FNV-1a 64 (in-process pass 1): {digest_a:#018x}");
    println!("digest FNV-1a 64 (in-process pass 2): {digest_b:#018x}");
    check(
        &mut passed,
        digest_a == digest_b && first.lines.len() == second.lines.len(),
        "digest reproducible in-process",
        format!("{digest_a:#018x} == {digest_b:#018x}"),
    );
    println!();
    println!("digest FNV-1a 64: {digest_a:#018x}");

    if passed {
        println!("VEREDICTO: PASS");
        println!("EXIT=0");
        0
    } else {
        println!("VEREDICTO: FAIL");
        println!("EXIT=1");
        1
    }
}

/// Marker used in the clamp detail line: no numeric deviation was observed.
/// (A real deviation would be printed here as `[DESVÍO] <detail>`.)
const DESVIO_NONE: &str = "clamp declarado, 0 desvío";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a64_matches_the_reference_vectors() {
        assert_eq!(fnv1a64(b"", FNV_OFFSET), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a", FNV_OFFSET), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(b"foobar", FNV_OFFSET), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn transcript_is_deterministic_across_passes() {
        let mut passed = true;
        let first = measure(&mut passed, false);
        let second = measure(&mut passed, false);
        assert!(passed, "every measurement assertion must hold");
        assert_eq!(first.lines, second.lines);
        assert_eq!(first.digest(), second.digest());
    }
}

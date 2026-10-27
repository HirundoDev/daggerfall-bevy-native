//! M31 probes — travel, weather and calendar (SPEC-M31-TRAVEL-WEATHER-CALENDAR,
//! REQ-L99-064, CR-f716455c).
//!
//! - **`PROBE-TRAVEL-001`**
//!   - **A** T1-T22 + C1-C5 + A1-A7 golden vectors (`vectors=34 ok=34`),
//!     transcribed from SPEC-M31 §4.1-§4.3 and §4.6.
//!   - **B** synthetic square spiral (2 200 pixels) over the real
//!     `CLIMATE.PAK` (1001×500): `pixels`, `ocean_pixels`, `minutes`,
//!     `pieces`, `days` + FNV-1a 64 digest computed twice in-process; the
//!     whole stdout is byte-identical across two processes (0 clock, 0
//!     unseeded RNG: the trip is deterministic).
//!   - **C** retail census `CLIMATE.PAK` (14 771 B) / `MAPSAVE.SAV`
//!     (16 406 B when present) / `WOODS.WLD` (26 001 168 B), reading at most
//!     64 bytes per file.
//!   - **D** demo over the real capital and its nearest settlement
//!     (`MAPTABLE.017`): clock before/after, gold before/after, arrival hour
//!     and the weather of the destination climate after the trip.
//! - **`PROBE-WEATHER-001`**: A W1-W13 (`vectors=13 ok=13`); B 10 000 draws
//!   per (climate, season) cell with `WeatherRng` seeded `0x0038A1E6`,
//!   `max_abs_delta < 0.02` against the normalized table, digest ×2.
//! - **`PROBE-CALENDAR-001`**: A L1-L12 + season + birth sign
//!   (`vectors=14 ok=14`); B 360 consecutive days from 405-05-00: per-moon
//!   phase counts, `sum=360` per moon, Masser Full/New in 11..=12, digest ×2.
//!
//! Exit: `0` ⟺ the blocks pass; `2` when ARENA2 is inaccessible (TRAVEL
//! blocks B/C/D only); `1` on any FAIL. Mode A custody: the retail files are
//! read at runtime from the user's ARENA2 and never packaged (0 retail bytes).

use daggerfall_heart::calendar::{self, LUNAR_PHASES, LunarPhase, Season};
use daggerfall_heart::clock::GameClock;
use daggerfall_heart::travel::{self, ArrivalFlags, Transport, TravelOptions, UniformTerrain};
use daggerfall_heart::weather::{
    self, CLIMATES, Climate, WEATHER_SEED, WEATHER_TYPES, WeatherClimateSeason, WeatherRng,
    WeatherState, WeatherType,
};
use daggerfall_parser_rs::climate::ClimateMap;

/// Retail census: `CLIMATE.PAK` exact size (SPEC-M31 §5-C).
const CLIMATE_PAK_BYTES: u64 = 14_771;
/// Retail census: `MAPSAVE.SAV` exact size when the user has a save.
const MAPSAVE_BYTES: u64 = 16_406;
/// Retail census: `WOODS.WLD` exact size (SPEC-M31 §5-C).
const WOODS_WLD_BYTES: u64 = 26_001_168;
/// Bytes read per file by the census block (SPEC-M31 §5-C).
const CENSUS_READ_CAP: usize = 64;
/// Pixels of the synthetic spiral (SPEC-M31 §5-B).
const SPIRAL_STEPS: usize = 2_200;

// ── evidence rows (M19/M25/M26/M27/M30 template) ───────────────────────────

/// One printed vector: `id|expected|actual`.
struct Vector {
    id: String,
    expected: String,
    actual: String,
}

impl Vector {
    fn ok(&self) -> bool {
        self.expected == self.actual
    }
}

fn vector(id: impl Into<String>, expected: impl Into<String>, actual: impl Into<String>) -> Vector {
    Vector {
        id: id.into(),
        expected: expected.into(),
        actual: actual.into(),
    }
}

/// Prints one line and appends it to the evidence buffer (digest input).
fn say(out: &mut String, line: impl AsRef<str>) {
    let line = line.as_ref();
    println!("{line}");
    out.push_str(line);
    out.push('\n');
}

/// FNV-1a 64 (offset basis `0xcbf2_9ce4_8422_2325`, prime `0x100000001b3`).
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// First `cap` bytes of a file (census: never loads the whole file).
fn read_head(path: &std::path::Path, cap: usize) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|e| format!("open {path:?}: {e}"))?;
    let mut buffer = vec![0u8; cap];
    let mut filled = 0;
    while filled < cap {
        let read = file
            .read(&mut buffer[filled..])
            .map_err(|e| format!("read {path:?}: {e}"))?;
        if read == 0 {
            break;
        }
        filled += read;
    }
    buffer.truncate(filled);
    Ok(buffer)
}

/// Prints a vector block and returns `(ok, fail)`.
fn print_vectors(out: &mut String, title: &str, vectors: &[Vector]) -> (usize, usize) {
    say(out, format!("\n-- {title} (id|expected|actual) --"));
    let mut ok = 0usize;
    let mut fail = 0usize;
    for vector in vectors {
        say(
            out,
            format!("{}|{}|{}", vector.id, vector.expected, vector.actual),
        );
        if vector.ok() {
            ok += 1;
        } else {
            fail += 1;
            say(
                out,
                format!(
                    "FAIL {} expected={} actual={}",
                    vector.id, vector.expected, vector.actual
                ),
            );
        }
    }
    (ok, fail)
}

/// `PASS`/`FAIL` word for the summary lines.
fn verdict(ok: bool) -> &'static str {
    if ok {
        "PASS"
    } else {
        "FAIL"
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-TRAVEL-001
// ═══════════════════════════════════════════════════════════════════════════

/// SPEC-M31 §4.1-§4.3, §4.6: 34 golden vectors (T1-T22, C1-C5, A1-A7).
fn travel_vectors() -> Vec<Vector> {
    let mut vectors = Vec::with_capacity(34);

    // T1-T10: minutes per pixel (SPEC §4.1).
    let per_pixel: [(u32, Transport, u32); 10] = [
        (240, Transport::Foot, 108),
        (220, Transport::Foot, 116),
        (200, Transport::Foot, 124),
        (230, Transport::Foot, 112),
        (250, Transport::Foot, 104),
        (200, Transport::Horse, 62),
        (240, Transport::Horse, 54),
        (250, Transport::Horse, 52),
        (200, Transport::Cart, 92),
        (240, Transport::Cart, 80),
    ];
    for (index, (modifier, transport, expected)) in per_pixel.iter().enumerate() {
        vectors.push(vector(
            format!("T{}", index + 1),
            expected.to_string(),
            travel::minutes_per_pixel(*modifier, *transport).to_string(),
        ));
    }
    // T11-T12: ocean minutes per pixel.
    vectors.push(vector(
        "T11",
        "255",
        travel::ocean_minutes(false).to_string(),
    ));
    vectors.push(vector(
        "T12",
        "51",
        travel::ocean_minutes(true).to_string(),
    ));

    // T13-T22: complete trips (SPEC §4.2). Raw climate values:
    // 224 Desert (mod 240), 227 Rainforest (200), 231 Woodlands (250),
    // 223 Ocean.
    let cautious_inn = TravelOptions::default();
    let reckless_inn = TravelOptions {
        cautious: false,
        ..TravelOptions::default()
    };
    let no_inn = TravelOptions {
        sleep_inn: false,
        ..TravelOptions::default()
    };
    let no_ship = TravelOptions {
        travel_ship: false,
        ..TravelOptions::default()
    };
    let reckless_no_ship_no_inn = TravelOptions {
        cautious: false,
        travel_ship: false,
        sleep_inn: false,
    };
    let trip = |raw: u8, pixels: i32, transport: Transport, options: TravelOptions| {
        travel::calculate_travel_time(&UniformTerrain(raw), (0, 0), (pixels, 0), transport, &options)
            .expect("uniform terrain always resolves")
    };
    let t13 = trip(227, 10, Transport::Foot, cautious_inn);
    vectors.push(vector("T13", "1240", t13.minutes.to_string()));
    let t14 = trip(227, 10, Transport::Foot, reckless_inn);
    vectors.push(vector("T14", "620", t14.minutes.to_string()));
    let t15 = trip(227, 10, Transport::Horse, cautious_inn);
    vectors.push(vector("T15", "620", t15.minutes.to_string()));
    let t16 = trip(227, 10, Transport::Cart, cautious_inn);
    vectors.push(vector("T16", "920", t16.minutes.to_string()));
    let t17 = trip(223, 3, Transport::Foot, cautious_inn);
    vectors.push(vector("T17", "153", t17.minutes.to_string()));
    let t18 = trip(223, 3, Transport::Foot, no_ship);
    vectors.push(vector("T18", "765", t18.minutes.to_string()));
    let t19 = trip(224, 5, Transport::Foot, no_inn);
    vectors.push(vector("T19", "630", t19.minutes.to_string()));
    let t20 = trip(223, 1, Transport::Foot, reckless_no_ship_no_inn);
    vectors.push(vector("T20", "149", t20.minutes.to_string()));
    let t21 = trip(231, 100, Transport::Horse, cautious_inn);
    vectors.push(vector(
        "T21",
        "5200/4d",
        format!("{}/{}d", t21.minutes, t21.days()),
    ));
    let t22 = trip(227, 100, Transport::Foot, cautious_inn);
    vectors.push(vector(
        "T22",
        "12400/9d",
        format!("{}/{}d", t22.minutes, t22.days()),
    ));

    // C1-C5: trip cost (SPEC §4.3); the vector states the total.
    let cost = |minutes: u64,
                ocean: u32,
                sleep_inn: bool,
                free_tavern: bool,
                has_ship: bool,
                travel_ship: bool| {
        travel::calculate_trip_cost(minutes, ocean, sleep_inn, free_tavern, has_ship, travel_ship)
    };
    let c1 = cost(5200, 0, true, false, false, false);
    vectors.push(vector("C1", "20", c1.total.to_string()));
    let c2 = cost(153, 30, true, false, false, true);
    vectors.push(vector("C2", "55", c2.total.to_string()));
    let c3 = cost(153, 30, false, false, false, true);
    vectors.push(vector("C3", "50", c3.total.to_string()));
    let c4 = cost(153, 3, false, false, true, true);
    vectors.push(vector("C4", "0", c4.total.to_string()));
    let c5 = cost(1440, 0, true, false, false, true);
    vectors.push(vector("C5", "10", c5.total.to_string()));

    // A1-A7: arrival adjustments (SPEC §4.6).
    let arrival = |hour: u32,
                   minute: u32,
                   flags: ArrivalFlags|
     -> (u32, String, u32, u32, u32) {
        let mut clock = GameClock::classic_default();
        clock.hour = hour;
        clock.minute = minute;
        travel::apply_arrival_rules(&mut clock, flags);
        (
            clock.day,
            clock.time_string(),
            clock.hour,
            clock.minute,
            clock.second,
        )
    };
    let vampire = ArrivalFlags {
        vampire_or_sunlight: true,
        cautious: true,
    };
    let cautious = ArrivalFlags {
        vampire_or_sunlight: false,
        cautious: true,
    };
    let reckless = ArrivalFlags::default();
    let a1 = arrival(10, 0, vampire);
    vectors.push(vector("A1", "18:00:00 d0", format!("{} d{}", a1.1, a1.0)));
    let a2 = arrival(19, 0, vampire);
    vectors.push(vector("A2", "19:00:00 d0", format!("{} d{}", a2.1, a2.0)));
    let a3 = arrival(5, 0, cautious);
    vectors.push(vector("A3", "07:10:00 d0", format!("{} d{}", a3.1, a3.0)));
    let a4 = arrival(7, 5, cautious);
    vectors.push(vector("A4", "07:10:00 d0", format!("{} d{}", a4.1, a4.0)));
    let a5 = arrival(17, 5, cautious);
    vectors.push(vector("A5", "17:05:00 d0", format!("{} d{}", a5.1, a5.0)));
    let a6 = arrival(18, 30, cautious);
    vectors.push(vector("A6", "07:10:00 d1", format!("{} d{}", a6.1, a6.0)));
    let a7 = arrival(3, 0, reckless);
    vectors.push(vector("A7", "03:00:00 d0", format!("{} d{}", a7.1, a7.0)));

    vectors
}

/// First coastal pixel in raster order (land with an orthogonal ocean
/// neighbour) at least 36 px from the border: a deterministic origin whose
/// spiral covers both land and ocean (the map centre is open ocean, so it
/// would only exercise the 51 min/px branch).
fn spiral_origin(climate: &ClimateMap) -> Option<(i32, i32)> {
    const MARGIN: i32 = 36;
    let is_ocean = |x: i32, y: i32| climate.climate_index(x, y) == Some(travel::OCEAN_CLIMATE);
    for y in MARGIN..(500 - MARGIN) {
        for x in MARGIN..(1000 - MARGIN) {
            if is_ocean(x, y) {
                continue;
            }
            let coastal = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .any(|(dx, dy)| is_ocean(x + dx, y + dy));
            if coastal {
                return Some((x, y));
            }
        }
    }
    None
}

/// Square spiral: unit steps with run lengths 1,1,2,2,3,3,… from `origin`;
/// 2 200 steps stay inside `CLIMATE.PAK` (1001×500) with the 36 px margin.
fn spiral_points(origin: (i32, i32), steps: usize) -> Vec<(i32, i32)> {
    const DIRECTIONS: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];
    let mut points = Vec::with_capacity(steps + 1);
    points.push(origin);
    let (mut x, mut y) = origin;
    let mut direction = 0usize;
    let mut run_length = 1usize;
    let mut in_run = 0usize;
    while points.len() <= steps {
        let (dx, dy) = DIRECTIONS[direction % 4];
        x += dx;
        y += dy;
        points.push((x, y));
        in_run += 1;
        if in_run == run_length {
            in_run = 0;
            direction += 1;
            if direction.is_multiple_of(2) {
                run_length += 1;
            }
        }
    }
    points
}

/// Loads the retail `CLIMATE.PAK` (14 771 B) from ARENA2.
fn load_climate_map() -> Result<ClimateMap, String> {
    let path = crate::mode_a_arena2_path("CLIMATE.PAK")?;
    let bytes = std::fs::read(&path).map_err(|e| format!("read {path:?}: {e}"))?;
    ClimateMap::parse(&bytes).map_err(|e| format!("parse {path:?}: {e}"))
}

/// Block B — spiral route over the real climate grid, computed twice.
fn run_travel_spiral(out: &mut String, climate: &ClimateMap) -> bool {
    let Some(origin) = spiral_origin(climate) else {
        say(out, "[FAIL] no coastal pixel to anchor the spiral");
        return false;
    };
    let points = spiral_points(origin, SPIRAL_STEPS);
    let in_bounds = points
        .iter()
        .all(|(x, y)| (0..1000).contains(x) && (0..500).contains(y));
    let lookup = |x: i32, y: i32| climate.climate_index(x, y);
    let options = TravelOptions::default();
    let first = travel::calculate_travel_route(&lookup, &points, Transport::Horse, &options);
    let second = travel::calculate_travel_route(&lookup, &points, Transport::Horse, &options);
    let (Ok(first), Ok(second)) = (first, second) else {
        say(out, "[FAIL] spiral route did not resolve the climate grid");
        return false;
    };
    let cost = travel::calculate_trip_cost(
        first.minutes,
        first.ocean_pixels,
        options.sleep_inn,
        false,
        false,
        options.travel_ship,
    );
    let line = format!(
        "pixels={} ocean_pixels={} minutes={} pieces={} days={}",
        first.pixels,
        first.ocean_pixels,
        first.minutes,
        cost.total,
        first.days()
    );
    say(out, "\n-- B · spiral route over CLIMATE.PAK (1001x500) --");
    say(
        out,
        format!("origin=({},{}) steps={SPIRAL_STEPS}", origin.0, origin.1),
    );
    say(out, line.clone());
    let canonical = |report: &travel::TravelReport| {
        format!(
            "pixels={} ocean_pixels={} minutes={} days={}",
            report.pixels,
            report.ocean_pixels,
            report.minutes,
            report.days()
        )
    };
    let first_digest = fnv1a64(canonical(&first).as_bytes());
    let second_digest = fnv1a64(canonical(&second).as_bytes());
    say(
        out,
        format!(
            "route_digest={first_digest:016x} run2={second_digest:016x} identical={} in_bounds={in_bounds}",
            first_digest == second_digest
        ),
    );
    let mixed = first.ocean_pixels > 0 && first.ocean_pixels < first.pixels;
    in_bounds
        && first.pixels == SPIRAL_STEPS as u32
        && first_digest == second_digest
        && first.minutes > 0
        && mixed
}

/// Block C — retail census (`bytes_leidos <= 64` per file).
fn run_travel_census(out: &mut String) -> Result<bool, String> {
    say(out, "\n-- C · retail census (64 B max per file) --");
    let mut ok = true;
    for (name, expected, required) in [
        ("CLIMATE.PAK", CLIMATE_PAK_BYTES, true),
        ("MAPSAVE.SAV", MAPSAVE_BYTES, false),
        ("WOODS.WLD", WOODS_WLD_BYTES, true),
    ] {
        let path = match crate::mode_a_arena2_path(name) {
            Ok(path) => path,
            Err(reason) => {
                if required {
                    return Err(format!("ARENA2 without {name}: {reason}"));
                }
                say(out, format!("mapsave=absent ({name})"));
                continue;
            }
        };
        let head = read_head(&path, CENSUS_READ_CAP)?;
        let size = std::fs::metadata(&path).map_err(|e| format!("stat {path:?}: {e}"))?.len();
        let label = match name {
            "CLIMATE.PAK" => "climate_pak",
            "MAPSAVE.SAV" => "mapsave",
            _ => "woods_wld",
        };
        let size_ok = size == expected;
        say(
            out,
            format!(
                "{label}=present size={size} expected={expected} bytes_read={} ok={size_ok}",
                head.len()
            ),
        );
        if !size_ok {
            ok = false;
        }
    }
    Ok(ok)
}

/// Demo target: a real settlement of the Daggerfall region with its map
/// pixel, plus the capital's pixel as the trip origin.
struct NeighbourCity {
    /// Retail settlement name (`MAPNAMES.017`).
    name: String,
    /// Capital map pixel (trip origin).
    from: (i32, i32),
    /// Neighbour settlement map pixel (trip destination).
    to: (i32, i32),
}

/// Nearest real settlement of the Daggerfall region (`MAPTABLE.017` /
/// `MAPPITEM.017`), with its map pixel. Prefers a neighbour at least 20 px
/// away (a meaningful trip); falls back to the nearest one.
fn locate_neighbour_city() -> Result<NeighbourCity, String> {
    use daggerfall_parser_bevy_bridge as bridge;
    use daggerfall_parser_rs::maps;

    const REGION: &str = "017";
    const CAPITAL: &str = "Daggerfall";

    let maps_path = crate::mode_a_arena2_path("MAPS.BSA")?;
    let data = std::fs::read(&maps_path).map_err(|e| format!("read {maps_path:?}: {e}"))?;
    let archive = bridge::parse_bsa(&data).map_err(|e| format!("parse MAPS.BSA: {e}"))?;
    let names = maps::parse_map_names(crate::m10_probe::named_entry(
        &archive,
        &data,
        &format!("MAPNAMES.{REGION}"),
    )?)
    .map_err(|e| format!("MAPNAMES: {e}"))?;
    let table = maps::parse_map_table(
        crate::m10_probe::named_entry(&archive, &data, &format!("MAPTABLE.{REGION}"))?,
        names.len(),
    )
    .map_err(|e| format!("MAPTABLE: {e}"))?;
    let locations = maps::parse_pitem(
        crate::m10_probe::named_entry(&archive, &data, &format!("MAPPITEM.{REGION}"))?,
        names.len(),
    )
    .map_err(|e| format!("MAPPITEM: {e}"))?;
    let capital = names
        .iter()
        .position(|name| name == CAPITAL)
        .ok_or_else(|| format!("capital {CAPITAL} absent from MAPNAMES.{REGION}"))?;
    let capital_pixel = crate::m18_probe::map_pixel(&table[capital]);

    let mut best: Option<(i64, &str, (i32, i32))> = None;
    let mut fallback: Option<(i64, &str, (i32, i32))> = None;
    for (index, name) in names.iter().enumerate() {
        if index == capital {
            continue;
        }
        // Same settlement filter M10 uses for city selection (0..=2).
        if !matches!(locations[index].exterior.location_type, 0..=2) {
            continue;
        }
        let pixel = crate::m18_probe::map_pixel(&table[index]);
        if !(0..1000).contains(&pixel.0) || !(0..500).contains(&pixel.1) {
            continue;
        }
        let dx = i64::from(pixel.0 - capital_pixel.0);
        let dy = i64::from(pixel.1 - capital_pixel.1);
        let distance = dx * dx + dy * dy;
        if fallback.is_none_or(|(current, ..)| distance < current) {
            fallback = Some((distance, name, pixel));
        }
        if distance >= 20 * 20 && best.is_none_or(|(current, ..)| distance < current) {
            best = Some((distance, name, pixel));
        }
    }
    let chosen = best.or(fallback).ok_or("no settlement with a map pixel")?;
    Ok(NeighbourCity {
        name: chosen.1.to_string(),
        from: capital_pixel,
        to: chosen.2,
    })
}

/// Block D — demo: real capital → nearest real settlement.
fn run_travel_demo(out: &mut String, climate: &ClimateMap, neighbour: &NeighbourCity) -> bool {
    say(out, "\n-- D · demo (retail capital → neighbour settlement) --");
    let NeighbourCity { name, from, to } = neighbour;
    let lookup = |x: i32, y: i32| climate.climate_index(x, y);
    let options = TravelOptions::default();
    let report = match travel::calculate_travel_time(
        &lookup,
        *from,
        *to,
        Transport::Horse,
        &options,
    ) {
        Ok(report) => report,
        Err(error) => {
            say(out, format!("[FAIL] demo route: {error}"));
            return false;
        }
    };
    let cost = travel::calculate_trip_cost(
        report.minutes,
        report.ocean_pixels,
        options.sleep_inn,
        false,
        false,
        options.travel_ship,
    );
    const GOLD_BEFORE: i64 = 1_000;
    if GOLD_BEFORE < cost.total {
        say(out, "[FAIL] demo purse below the trip cost");
        return false;
    }
    let gold_after = GOLD_BEFORE - cost.total;

    let mut clock = GameClock::classic_default();
    let minutes_before = clock.total_minutes();
    let clock_before = clock.time_string();
    clock.raise_seconds(report.minutes * 60);
    let arrival_before_rules = clock.time_string();
    travel::apply_arrival_rules(
        &mut clock,
        ArrivalFlags {
            vampire_or_sunlight: false,
            cautious: options.cautious,
        },
    );
    let minutes_after = clock.total_minutes();
    let arrival_hour = clock.time_string();
    let delta = minutes_after - minutes_before;
    let delta_ok = delta == report.minutes
        || (options.cautious && delta > report.minutes && delta - report.minutes <= 24 * 60);

    let destination_climate = climate.climate_index(to.0, to.1).and_then(Climate::from_climate_value);
    let mut rng = WeatherRng::new(WEATHER_SEED);
    let weather_after = match destination_climate {
        Some(climate_row) => rng.draw(climate_row, calendar::season(&clock)),
        None => WeatherType::Sunny,
    };
    let weather_ok = destination_climate.is_none_or(|climate_row| {
        WeatherClimateSeason::new(climate_row, calendar::season(&clock)).odds()
            [weather_after.index()]
            > 0.0
    });
    let arrival_ok = (7..=17).contains(&clock.hour);

    say(
        out,
        format!(
            "destination={name} from=({},{}) to=({},{}) pixels={} ocean_pixels={} minutes={} days={}",
            from.0, from.1, to.0, to.1, report.pixels, report.ocean_pixels, report.minutes, report.days()
        ),
    );
    say(
        out,
        format!(
            "minutes_before={minutes_before} minutes_after={minutes_after} clock_before={clock_before} arrival_raw={arrival_before_rules} arrival_hour={arrival_hour} clock_after={}",
            clock.time_string()
        ),
    );
    say(
        out,
        format!(
            "gold_before={GOLD_BEFORE} gold_after={gold_after} pieces={} total={cost_total}",
            cost.pieces,
            cost_total = cost.total
        ),
    );
    say(
        out,
        format!(
            "weather_after={} climate_value={} season={}",
            weather_after.name(),
            climate.climate_index(to.0, to.1).map_or("none".to_string(), |v| v.to_string()),
            calendar::season(&clock).name()
        ),
    );
    let ok = delta_ok && arrival_ok && cost.total >= 0 && weather_ok;
    say(
        out,
        format!(
            "{} - D1 clock, D2 gold, D3 arrival, D4 weather",
            verdict(ok)
        ),
    );
    ok
}

/// `PROBE-TRAVEL-001` — blocks A-D.
pub fn run_travel_001() -> i32 {
    let mut out = String::new();
    say(
        &mut out,
        "PROBE-TRAVEL-001 - DFU travel: time per pixel, trip cost, arrival (M31, CR-f716455c, REQ-L99-064 · spec §1 §4.1-§4.3 §4.6)",
    );

    let vectors = travel_vectors();
    let (ok_count, fail_count) = print_vectors(&mut out, "A · golden vectors", &vectors);
    say(
        &mut out,
        format!(
            "vectors={} ok={ok_count} fail={fail_count}",
            vectors.len()
        ),
    );
    let mut ok = fail_count == 0;

    // ── B/C/D need ARENA2: inaccessible ⇒ EXIT=2 (never a false PASS) ──────
    let climate = match load_climate_map() {
        Ok(climate) => climate,
        Err(reason) => {
            say(
                &mut out,
                format!("ARENA2 not accessible: {reason} (EXIT=2 by SPEC-M31 §5)"),
            );
            return 2;
        }
    };
    ok &= run_travel_spiral(&mut out, &climate);
    match run_travel_census(&mut out) {
        Ok(census_ok) => ok &= census_ok,
        Err(reason) => {
            say(&mut out, format!("ARENA2 not accessible: {reason} (EXIT=2)"));
            return 2;
        }
    }
    match locate_neighbour_city() {
        Ok(neighbour) => ok &= run_travel_demo(&mut out, &climate, &neighbour),
        Err(reason) => {
            say(&mut out, format!("ARENA2 not accessible: {reason} (EXIT=2)"));
            return 2;
        }
    }

    let digest = fnv1a64(out.as_bytes());
    say(&mut out, format!("digest={digest:016x}"));
    println!(
        "TRAVEL-001 {} - vectors={} ok={ok_count} fail={fail_count}",
        verdict(ok),
        vectors.len()
    );
    if ok { 0 } else { 1 }
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-WEATHER-001
// ═══════════════════════════════════════════════════════════════════════════

/// SPEC-M31 §4.5 W1-W13.
fn weather_vectors() -> Vec<Vector> {
    let wood_summer = WeatherClimateSeason::new(Climate::Woodlands, Season::Summer);
    let mountain_winter = WeatherClimateSeason::new(Climate::Mountains, Season::Winter);
    let wood_winter = WeatherClimateSeason::new(Climate::Woodlands, Season::Winter);
    let mut vectors = Vec::with_capacity(13);
    let cases: [(&WeatherClimateSeason, f64, WeatherType); 11] = [
        (&wood_summer, 0.0, WeatherType::Sunny),
        (&wood_summer, 60.1, WeatherType::Cloudy),
        (&wood_summer, 80.1, WeatherType::Overcast),
        (&wood_summer, 85.1, WeatherType::Rain),
        (&wood_summer, 95.1, WeatherType::Thunder),
        (&mountain_winter, 18.0, WeatherType::Sunny),
        (&mountain_winter, 18.01, WeatherType::Cloudy),
        (&mountain_winter, 65.0, WeatherType::Fog),
        (&mountain_winter, 65.01, WeatherType::Snow),
        (&wood_winter, 27.0, WeatherType::Sunny),
        (&wood_winter, 27.9, WeatherType::Cloudy),
    ];
    for (index, (row, rand, expected)) in cases.iter().enumerate() {
        vectors.push(vector(
            format!("W{}", index + 1),
            expected.name(),
            row.get_weather(*rand).name(),
        ));
    }
    let rain = weather::attenuated_shadow(WeatherType::Rain, false);
    let snow = weather::attenuated_shadow(WeatherType::Snow, false);
    vectors.push(vector("W12", "0.3443", format!("{rain:.4}")));
    vectors.push(vector("W13", "0.1947", format!("{snow:.4}")));
    vectors
}

/// `PROBE-WEATHER-001` — blocks A-B.
pub fn run_weather_001() -> i32 {
    let mut out = String::new();
    say(
        &mut out,
        "PROBE-WEATHER-001 - DFU weather table + GetWeather + fog attenuation (M31, CR-f716455c, REQ-L99-064 · spec §2 §4.5)",
    );
    let vectors = weather_vectors();
    let (ok_count, fail_count) = print_vectors(&mut out, "A · golden vectors", &vectors);
    say(
        &mut out,
        format!(
            "vectors={} ok={ok_count} fail={fail_count}",
            vectors.len()
        ),
    );

    // ── B · 10 000 draws per (climate, season) with the seeded stream ─────
    say(&mut out, "\n-- B · 10 000 draws per (climate, season) --");
    let mut rng = WeatherRng::new(WEATHER_SEED);
    let mut worst: f64 = 0.0;
    let mut worst_cell = String::new();
    let mut cells = 0usize;
    for climate in CLIMATES {
        for season in [Season::Fall, Season::Spring, Season::Summer, Season::Winter] {
            let row = WeatherClimateSeason::new(climate, season);
            let mut counts = [0u32; 7];
            for _ in 0..10_000 {
                let drawn = row.get_weather(rng.next_percent());
                counts[drawn.index()] += 1;
            }
            cells += 1;
            for weather in WEATHER_TYPES {
                let observed = f64::from(counts[weather.index()]) / 10_000.0;
                let expected = row.odds()[weather.index()] / 100.0;
                let delta = (observed - expected).abs();
                if delta > worst {
                    worst = delta;
                    worst_cell = format!("{}/{} {}", climate.name(), season.name(), weather.name());
                }
            }
        }
    }
    let draws_ok = worst < 0.02;
    say(
        &mut out,
        format!(
            "draws=10000 cells={cells} seed=0x{WEATHER_SEED:08X} max_abs_delta={worst:.4} threshold=0.02 cell=\"{worst_cell}\"",
        ),
    );
    say(
        &mut out,
        format!(
            "{} - B1 frequency within 2 % of the declared distribution",
            verdict(draws_ok)
        ),
    );

    // State machine smoke (SPEC §2.3): Thunder raises raining+storming, Fog
    // implies overcast, Sunny clears every flag.
    let mut state = WeatherState::default();
    state.apply(WeatherType::Thunder);
    let thunder_ok = state.is_raining && state.is_storming && state.is_overcast && !state.is_snowing;
    state.apply(WeatherType::Fog);
    let fog_ok = !state.is_raining && !state.is_storming && state.is_overcast;
    state.apply(WeatherType::Sunny);
    let sunny_ok = !state.is_overcast && state.fog_density() == 0.0;
    say(
        &mut out,
        format!(
            "state_thunder={thunder_ok} state_fog={fog_ok} state_sunny={sunny_ok}"
        ),
    );

    let digest = fnv1a64(out.as_bytes());
    say(&mut out, format!("digest={digest:016x}"));
    let ok = fail_count == 0 && draws_ok && thunder_ok && fog_ok && sunny_ok;
    println!(
        "WEATHER-001 {} - vectors={} ok={ok_count} fail={fail_count} max_abs_delta={worst:.4}",
        verdict(ok),
        vectors.len()
    );
    if ok { 0 } else { 1 }
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-CALENDAR-001
// ═══════════════════════════════════════════════════════════════════════════

/// SPEC-M31 §4.4 L1-L12 + season + birth sign (14 vectors).
fn calendar_vectors() -> Vec<Vector> {
    const LUNAR: [(u32, &str, &str); 12] = [
        (0, "ThreeWane", "ThreeWax"),
        (1, "ThreeWane", "ThreeWax"),
        (2, "ThreeWane", "Full"),
        (5, "HalfWane", "ThreeWane"),
        (10, "OneWane", "HalfWane"),
        (14, "New", "OneWane"),
        (16, "OneWax", "OneWane"),
        (20, "OneWax", "OneWax"),
        (23, "HalfWax", "OneWax"),
        (29, "ThreeWax", "HalfWax"),
        (30, "Full", "HalfWax"),
        (31, "ThreeWane", "ThreeWax"),
    ];
    let mut vectors = Vec::with_capacity(14);
    for (index, (day, masser, secunda)) in LUNAR.iter().enumerate() {
        let mut clock = GameClock::classic_default();
        clock.day = *day;
        let (masser_phase, secunda_phase) = calendar::lunar_phases(&clock);
        vectors.push(vector(
            format!("L{}", index + 1),
            format!("{masser}/{secunda}"),
            format!("{}/{}", masser_phase.name(), secunda_phase.name()),
        ));
    }
    let clock = GameClock::classic_default();
    vectors.push(vector(
        "SEASON",
        "Summer",
        calendar::season(&clock).name(),
    ));
    vectors.push(vector("SIGN", "TheSteed", calendar::birth_sign(&clock)));
    vectors
}

/// Phase index in [`LUNAR_PHASES`] (probe-local census order).
fn phase_index(phase: LunarPhase) -> usize {
    LUNAR_PHASES
        .iter()
        .position(|candidate| *candidate == phase)
        .expect("LUNAR_PHASES covers every phase")
}

/// `PROBE-CALENDAR-001` — blocks A-B.
pub fn run_calendar_001() -> i32 {
    let mut out = String::new();
    say(
        &mut out,
        "PROBE-CALENDAR-001 - DFU calendar: derived dates, seasons, signs, lunar phases (M31, CR-f716455c, REQ-L99-064 · spec §3 §4.4)",
    );
    let vectors = calendar_vectors();
    let (ok_count, fail_count) = print_vectors(&mut out, "A · golden vectors", &vectors);
    say(
        &mut out,
        format!(
            "vectors={} ok={ok_count} fail={fail_count}",
            vectors.len()
        ),
    );

    // ── B · 360 consecutive days from 405-05-00 (noon) ────────────────────
    say(&mut out, "\n-- B · 360 days phase census --");
    let mut clock = GameClock::classic_default();
    let mut masser_counts = [0u32; 8];
    let mut secunda_counts = [0u32; 8];
    for _ in 0..360 {
        let (masser, secunda) = calendar::lunar_phases(&clock);
        masser_counts[phase_index(masser)] += 1;
        secunda_counts[phase_index(secunda)] += 1;
        clock.raise_seconds(calendar::SECONDS_PER_DAY);
    }
    let masser_sum: u32 = masser_counts.iter().sum();
    let secunda_sum: u32 = secunda_counts.iter().sum();
    for (label, counts, sum) in [
        ("masser", &masser_counts, masser_sum),
        ("secunda", &secunda_counts, secunda_sum),
    ] {
        let phases: Vec<String> = LUNAR_PHASES
            .iter()
            .map(|phase| format!("{}={}", phase.name(), counts[phase_index(*phase)]))
            .collect();
        say(
            &mut out,
            format!("{label}: {} sum={sum}", phases.join(" ")),
        );
    }
    let full = phase_index(LunarPhase::Full);
    let new = phase_index(LunarPhase::New);
    let full_new_ok = (11..=12).contains(&masser_counts[full]) && (11..=12).contains(&masser_counts[new]);
    let counts_ok = masser_sum == 360 && secunda_sum == 360 && full_new_ok;
    say(
        &mut out,
        format!(
            "masser_full={} masser_new={} full_new_range=11..12 {} - B1 360 days, sum=360 per moon",
            masser_counts[full],
            masser_counts[new],
            verdict(counts_ok)
        ),
    );

    // Two independent climbs must agree (digest ×2 in-process).
    let digest_first = fnv1a64(out.as_bytes());
    let mut replay = GameClock::classic_default();
    let mut replay_counts = [0u32; 8];
    for _ in 0..360 {
        replay_counts[phase_index(calendar::lunar_phase(&replay, true))] += 1;
        replay.raise_seconds(calendar::SECONDS_PER_DAY);
    }
    let digest_second = fnv1a64(format!("{:?}", replay_counts).as_bytes());
    let digest_first_second = fnv1a64(format!("{:?}", masser_counts).as_bytes());
    say(
        &mut out,
        format!(
            "digest={digest_first:016x} count_digest={digest_first_second:016x} replay={digest_second:016x} identical={}",
            digest_first_second == digest_second
        ),
    );

    let ok = fail_count == 0 && counts_ok && digest_first_second == digest_second;
    println!(
        "CALENDAR-001 {} - vectors={} ok={ok_count} fail={fail_count} masser_sum={masser_sum} secunda_sum={secunda_sum}",
        verdict(ok),
        vectors.len()
    );
    if ok { 0 } else { 1 }
}

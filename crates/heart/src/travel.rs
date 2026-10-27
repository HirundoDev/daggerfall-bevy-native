//! `travel` — map-pixel travel time, trip cost and arrival adjustments
//! (M31, SPEC-M31 §1 · REQ-L99-064).
//!
//! **Source custody (clean-room, MIT):** transcribed from
//! `reference/dfu-spec/Assets/Scripts/Game/Utility/TravelTimeCalculator.cs`
//! (`:30-174`) and `DaggerfallTravelPopUp.cs:270-360` (DFU v1.1.1, upstream
//! `0038a1e6…`). Constants, the Bresenham walk, the per-pixel formula, the
//! trip cost and the arrival rules are the source's; the only substitution is
//! the RNG (the source uses `UnityEngine.Random` only for weather, not for
//! travel: the trip is fully deterministic).
//!
//! **Terrain is injected, never re-read:** §7.1 assigns `GetClimateIndex` to
//! M18, so this module consumes a [`TerrainLookup`] (a closure in the probe
//! over the real `CLIMATE.PAK`). `heart` keeps 0 parser edges.
//!
//! **Weather does not change travel time** (SPEC §1.2, `NEGATIVE`: 0 matches
//! of `weather|rain|snow` in `TravelTimeCalculator.cs`).

use crate::calendar::is_day;
use crate::clock::{GameClock, DUSK_HOUR, MINUTES_PER_DAY};

/// Raw `CLIMATE.PAK` value of the ocean (`MapsFile.Climates.Ocean`).
pub const OCEAN_CLIMATE: u8 = 223;
/// `climateIndices` (`TravelTimeCalculator.cs:30`): raw climate value minus
/// [`OCEAN_CLIMATE`] indexes this table (terrain-movement group 0..5). The
/// weather array route reuses the same table
/// (`WeatherManager.cs:434-446`), so it is single-sourced here.
pub const CLIMATE_INDICES: [u8; 10] = [0, 0, 0, 1, 2, 3, 4, 5, 5, 5];
/// `terrainMovementModifiers` (`:33`), indexed by [`CLIMATE_INDICES`].
pub const TERRAIN_MOVEMENT_MODIFIERS: [u32; 6] = [240, 220, 200, 200, 230, 250];
/// Transport modifier on foot (`:71-77`).
pub const FOOT_MODIFIER: u32 = 256;
/// Transport modifier with a horse (`:71-77`).
pub const HORSE_MODIFIER: u32 = 128;
/// Transport modifier with a cart (`:71-77`).
pub const CART_MODIFIER: u32 = 192;
/// Minutes per ocean pixel on foot (`:136-138`).
pub const OCEAN_MINUTES_FOOT: u32 = 255;
/// Minutes per ocean pixel by ship (`:136-138`).
pub const OCEAN_MINUTES_SHIP: u32 = 51;
/// Default seed of the travel stream (SPEC-M31 §5 seed policy). The trip is
/// deterministic; the constant exists so probes declare the seed they use.
pub const TRAVEL_SEED: u64 = 0x00C0_FFEE;

/// Transport mode of the trip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    /// On foot (`transportModifier = 256`).
    Foot,
    /// With a horse (`128`).
    Horse,
    /// With a cart (`192`); a horse takes precedence in the source.
    Cart,
}

impl Transport {
    /// `transportModifier` (`:71-77`).
    pub const fn modifier(self) -> u32 {
        match self {
            Self::Foot => FOOT_MODIFIER,
            Self::Horse => HORSE_MODIFIER,
            Self::Cart => CART_MODIFIER,
        }
    }

    /// Stable name for probes.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Foot => "foot",
            Self::Horse => "horse",
            Self::Cart => "cart",
        }
    }
}

/// Terrain provider: the raw `CLIMATE.PAK` value at a map pixel, or `None`
/// outside the grid. Implemented for any `Fn(i32, i32) -> Option<u8>`.
pub trait TerrainLookup {
    /// Raw climate value at `(x, y)` (223 = ocean).
    fn climate_at(&self, x: i32, y: i32) -> Option<u8>;
}

impl<F> TerrainLookup for F
where
    F: Fn(i32, i32) -> Option<u8>,
{
    fn climate_at(&self, x: i32, y: i32) -> Option<u8> {
        self(x, y)
    }
}

/// Uniform terrain for golden vectors (every pixel the same raw value).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UniformTerrain(pub u8);

impl TerrainLookup for UniformTerrain {
    fn climate_at(&self, _x: i32, _y: i32) -> Option<u8> {
        Some(self.0)
    }
}

/// Typed travel error: never a silent default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TravelError {
    /// The terrain provider has no value at that pixel (outside the grid).
    MissingTerrain {
        /// Map pixel X.
        x: i32,
        /// Map pixel Y.
        y: i32,
    },
    /// A raw climate value outside `223..=232` has no movement group.
    UnknownClimate {
        /// Offending raw value.
        value: u8,
    },
}

impl std::fmt::Display for TravelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingTerrain { x, y } => write!(f, "no terrain at ({x}, {y})"),
            Self::UnknownClimate { value } => {
                write!(f, "climate {value} outside 223..=232")
            }
        }
    }
}

impl std::error::Error for TravelError {}

/// Trip options (`CalculateTravelTime` defaults are `false` in the source;
/// the popup calls it with all three enabled, `:85-87`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TravelOptions {
    /// `speedCautious`: `false` halves the total (`:154`).
    pub cautious: bool,
    /// `sleepModeInn`: `false` applies `(300*x)>>8` per pixel (`:147-148`).
    pub sleep_inn: bool,
    /// `travelShip`: ocean pixels cost 51 instead of 255 (`:136-138`).
    pub travel_ship: bool,
}

impl Default for TravelOptions {
    /// Popup defaults: cautious, sleeping at inns, ship available (`:85-87`).
    fn default() -> Self {
        Self {
            cautious: true,
            sleep_inn: true,
            travel_ship: true,
        }
    }
}

/// Result of one trip: minutes plus the pixel census M25 consumes (§7.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TravelReport {
    /// Total minutes (`minutesTakenTotal`).
    pub minutes: u64,
    /// Map pixels walked (`numberOfMovements`).
    pub pixels: u32,
    /// Pixels whose terrain was ocean (`pixelsTraveledOnOcean`).
    pub ocean_pixels: u32,
}

impl TravelReport {
    /// Days shown by the popup: `mins/1440`, plus 1 when there is a remainder
    /// (`DaggerfallTravelPopUp.cs:286-290`).
    pub fn days(&self) -> u64 {
        self.minutes / MINUTES_PER_DAY + u64::from(!self.minutes.is_multiple_of(MINUTES_PER_DAY))
    }
}

/// `minutesTakenThisMove = (((102 * transportModifier) >> 8) *
/// (512 - terrainMovementModifier)) >> 8` (`:142-144`). The `>> 8` happens
/// **before** the second multiplication (SPEC §8.2).
pub fn minutes_per_pixel(terrain_modifier: u32, transport: Transport) -> u32 {
    (((102 * transport.modifier()) >> 8) * (512 - terrain_modifier)) >> 8
}

/// Ocean minutes per pixel (`:136-138`).
pub const fn ocean_minutes(travel_ship: bool) -> u32 {
    if travel_ship {
        OCEAN_MINUTES_SHIP
    } else {
        OCEAN_MINUTES_FOOT
    }
}

/// `climateIndices[raw - Ocean]` → `terrainMovementModifiers` (`:142-144`).
pub fn terrain_modifier(climate_value: u8) -> Result<u32, TravelError> {
    let offset = usize::from(
        climate_value
            .checked_sub(OCEAN_CLIMATE)
            .ok_or(TravelError::UnknownClimate {
                value: climate_value,
            })?,
    );
    let index = CLIMATE_INDICES
        .get(offset)
        .copied()
        .ok_or(TravelError::UnknownClimate {
            value: climate_value,
        })?;
    Ok(TERRAIN_MOVEMENT_MODIFIERS[usize::from(index)])
}

/// One Bresenham segment: accumulates minutes and the ocean census.
fn walk_segment<T: TerrainLookup>(
    terrain: &T,
    start: (i32, i32),
    end: (i32, i32),
    transport: Transport,
    options: &TravelOptions,
) -> Result<(u64, u32, u32), TravelError> {
    let (mut x, mut y) = start;
    let distance_x = end.0 - start.0;
    let distance_y = end.1 - start.1;
    let abs_x = distance_x.abs();
    let abs_y = distance_y.abs();
    let furthest = abs_x.max(abs_y);
    let step_x = if distance_x >= 0 { 1 } else { -1 };
    let step_y = if distance_y >= 0 { 1 } else { -1 };

    let mut increments: i64 = 0;
    let mut minutes_total: u64 = 0;
    let mut pixels: u32 = 0;
    let mut ocean_pixels: u32 = 0;

    for _ in 0..furthest {
        if furthest == abs_x {
            x += step_x;
            increments += i64::from(abs_y);
            // `>` (not `>=`): the accumulator decides the minor axis (SPEC §8.1).
            if increments > i64::from(abs_x) {
                increments -= i64::from(abs_x);
                y += step_y;
            }
        } else {
            y += step_y;
            increments += i64::from(abs_x);
            if increments > i64::from(abs_y) {
                increments -= i64::from(abs_y);
                x += step_x;
            }
        }

        let climate = terrain
            .climate_at(x, y)
            .ok_or(TravelError::MissingTerrain { x, y })?;
        let mut minutes = if climate == OCEAN_CLIMATE {
            ocean_pixels += 1;
            ocean_minutes(options.travel_ship)
        } else {
            minutes_per_pixel(terrain_modifier(climate)?, transport)
        };
        if !options.sleep_inn {
            minutes = (300 * minutes) >> 8;
        }
        minutes_total += u64::from(minutes);
        pixels += 1;
    }

    Ok((minutes_total, pixels, ocean_pixels))
}

/// `CalculateTravelTime(endPos, …)` (`:64-157`) over one Bresenham segment.
pub fn calculate_travel_time<T: TerrainLookup>(
    terrain: &T,
    start: (i32, i32),
    end: (i32, i32),
    transport: Transport,
    options: &TravelOptions,
) -> Result<TravelReport, TravelError> {
    calculate_travel_route(terrain, &[start, end], transport, options)
}

/// The same walk over a polyline (the probe's synthetic spiral): every
/// consecutive pair is one Bresenham segment, and the cautious `>> 1` is
/// applied **once** at the end (`:154`), like a single source call.
pub fn calculate_travel_route<T: TerrainLookup>(
    terrain: &T,
    points: &[(i32, i32)],
    transport: Transport,
    options: &TravelOptions,
) -> Result<TravelReport, TravelError> {
    let mut minutes_total: u64 = 0;
    let mut pixels: u32 = 0;
    let mut ocean_pixels: u32 = 0;
    for pair in points.windows(2) {
        let (minutes, walked, ocean) = walk_segment(terrain, pair[0], pair[1], transport, options)?;
        minutes_total += minutes;
        pixels += walked;
        ocean_pixels += ocean;
    }
    if !options.cautious {
        minutes_total >>= 1;
    }
    Ok(TravelReport {
        minutes: minutes_total,
        pixels,
        ocean_pixels,
    })
}

/// Trip cost (`CalculateTripCost`, `:159-174`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TripCost {
    /// `PiecesCost`: inn nights paid in gold pieces.
    pub pieces: i64,
    /// `TotalCost`: pieces plus the ship fare.
    pub total: i64,
}

/// `CalculateTripCost(travelTimeInMinutes, sleepModeInn, hasShip,
/// travelShip)` (`:159-174`). `free_tavern_rooms` is
/// `KnightlyOrder.FreeTavernRooms()` (the inn line is skipped for knights).
///
/// The signed subtraction is load-bearing: `(hours - ocean)/24` is negative in
/// C2 and C# truncates toward zero, which Rust `i64 / i64` reproduces
/// (SPEC §8.3).
pub fn calculate_trip_cost(
    minutes: u64,
    ocean_pixels: u32,
    sleep_inn: bool,
    free_tavern_rooms: bool,
    has_ship: bool,
    travel_ship: bool,
) -> TripCost {
    let hours = minutes.div_ceil(60);
    let mut pieces: i64 = 0;
    if sleep_inn && !free_tavern_rooms {
        pieces = 5 * ((hours as i64 - i64::from(ocean_pixels)) / 24);
        // Absent from classic; without it C2 becomes negative (`:165-167`).
        if pieces < 0 {
            pieces = 0;
        }
        pieces += 5;
    }
    let mut total = pieces;
    if ocean_pixels > 0 && !has_ship && travel_ship {
        total += 25 * (i64::from(ocean_pixels) / 24 + 1);
    }
    TripCost { pieces, total }
}

/// Arrival adjustment flags (`DaggerfallTravelPopUp.cs:336-360`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ArrivalFlags {
    /// Vampirism **or** `Career.DamageFromSunlight`: never arrive by day.
    pub vampire_or_sunlight: bool,
    /// `speedCautious`: night arrivals move to 07:10.
    pub cautious: bool,
}

/// Applies the arrival rules to `clock` in place (`:336-360`):
///
/// - vampire/sunlight and `IsDay` → raise to `DuskHour` (same day);
/// - else cautious and time before 07:10 → raise to 07:10;
/// - else cautious and `Hour > 17` → raise to 07:10 of the next day.
///
/// Cautious travel also refills health/fatigue/magicka in the source
/// (`:338-343`); that state belongs to M20/M32, so this lane only moves the
/// clock (§7.6 frontier).
pub fn apply_arrival_rules(clock: &mut GameClock, flags: ArrivalFlags) {
    if flags.vampire_or_sunlight {
        if is_day(clock) {
            clock.raise_seconds(u64::from(DUSK_HOUR - clock.hour) * 3600);
        }
        return;
    }
    if !flags.cautious {
        return;
    }
    // Signed arithmetic: the source computes `(10 - Minute) * 60` as a
    // negative contribution for minutes > 10 (A6: 18:30 -> next day 07:10).
    if clock.hour < 7 || (clock.hour == 7 && clock.minute < 10) {
        let seconds = i64::from(7 - clock.hour) * 3600
            + (10i64 - i64::from(clock.minute)) * 60
            - i64::from(clock.second);
        clock.raise_seconds(seconds as u64);
    } else if clock.hour > 17 {
        let seconds = i64::from(31 - clock.hour) * 3600
            + (10i64 - i64::from(clock.minute)) * 60
            - i64::from(clock.second);
        clock.raise_seconds(seconds as u64);
    }
}

/// `GuildManager.FastTravel` with the Akatosh blessing
/// (`Guilds/Temple.cs:430-437`): `((95 - rank) / 100) * duration`, truncated.
/// Rank is the guild rank in `0..=95`; another guild's override is the
/// identity (`Guild.cs:236-239`).
pub fn akatosh_fast_travel(minutes: u64, rank: u32) -> u64 {
    let factor = f64::from(95u32.saturating_sub(rank)) / 100.0;
    (factor * minutes as f64) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn travel(raw: u8, pixels: i32, transport: Transport, options: TravelOptions) -> TravelReport {
        calculate_travel_time(&UniformTerrain(raw), (0, 0), (pixels, 0), transport, &options)
            .expect("uniform terrain")
    }

    /// SPEC-M31 §4.1 T1-T12: minutes per pixel.
    #[test]
    fn per_pixel_golden_vectors_t1_t12() {
        let cases = [
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
        for (index, (modifier, transport, expected)) in cases.iter().enumerate() {
            assert_eq!(
                minutes_per_pixel(*modifier, *transport),
                *expected,
                "T{}",
                index + 1
            );
        }
        assert_eq!(ocean_minutes(false), 255);
        assert_eq!(ocean_minutes(true), 51);
    }

    /// SPEC-M31 §4.2 T13-T22.
    #[test]
    fn trip_golden_vectors_t13_t22() {
        let cautious_inn = TravelOptions::default();
        let reckless_inn = TravelOptions {
            cautious: false,
            ..TravelOptions::default()
        };
        let cautious_no_inn = TravelOptions {
            sleep_inn: false,
            ..TravelOptions::default()
        };
        let reckless_no_inn = TravelOptions {
            cautious: false,
            sleep_inn: false,
            travel_ship: false,
        };

        // raw 227 Rainforest -> movement modifier 200; 224 Desert -> 240;
        // 231 Woodlands -> 250; 223 Ocean.
        assert_eq!(
            travel(227, 10, Transport::Foot, cautious_inn).minutes,
            1240 // T13
        );
        assert_eq!(
            travel(227, 10, Transport::Foot, reckless_inn).minutes,
            620 // T14
        );
        assert_eq!(
            travel(227, 10, Transport::Horse, cautious_inn).minutes,
            620 // T15
        );
        assert_eq!(
            travel(227, 10, Transport::Cart, cautious_inn).minutes,
            920 // T16
        );
        let ship = TravelOptions {
            travel_ship: true,
            ..cautious_inn
        };
        assert_eq!(
            travel(223, 3, Transport::Foot, ship).minutes,
            153 // T17
        );
        let no_ship = TravelOptions {
            travel_ship: false,
            ..cautious_inn
        };
        assert_eq!(
            travel(223, 3, Transport::Foot, no_ship).minutes,
            765 // T18
        );
        assert_eq!(
            travel(224, 5, Transport::Foot, cautious_no_inn).minutes,
            630 // T19
        );
        assert_eq!(
            travel(223, 1, Transport::Foot, reckless_no_inn).minutes,
            149 // T20
        );
        let t21 = travel(231, 100, Transport::Horse, cautious_inn);
        assert_eq!(t21.minutes, 5200);
        assert_eq!(t21.days(), 4);
        let t22 = travel(227, 100, Transport::Foot, cautious_inn);
        assert_eq!(t22.minutes, 12400);
        assert_eq!(t22.days(), 9);
    }

    /// SPEC-M31 §4.3 C1-C5.
    #[test]
    fn cost_golden_vectors_c1_c5() {
        assert_eq!(
            calculate_trip_cost(5200, 0, true, false, false, false),
            TripCost {
                pieces: 20,
                total: 20
            }
        );
        assert_eq!(
            calculate_trip_cost(153, 30, true, false, false, true),
            TripCost {
                pieces: 5,
                total: 55
            }
        );
        assert_eq!(
            calculate_trip_cost(153, 30, false, false, false, true),
            TripCost {
                pieces: 0,
                total: 50
            }
        );
        assert_eq!(
            calculate_trip_cost(153, 3, false, false, true, true),
            TripCost {
                pieces: 0,
                total: 0
            }
        );
        assert_eq!(
            calculate_trip_cost(1440, 0, true, false, false, true),
            TripCost {
                pieces: 10,
                total: 10
            }
        );
    }

    /// SPEC-M31 §4.6 A1-A7.
    #[test]
    fn arrival_golden_vectors_a1_a7() {
        let at = |hour: u32, minute: u32| {
            let mut clock = GameClock::classic_default();
            clock.hour = hour;
            clock.minute = minute;
            clock.second = 0;
            clock
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

        let mut a1 = at(10, 0);
        apply_arrival_rules(&mut a1, vampire);
        assert_eq!((a1.hour, a1.minute), (18, 0)); // A1

        let mut a2 = at(19, 0);
        apply_arrival_rules(&mut a2, vampire);
        assert_eq!((a2.hour, a2.minute), (19, 0)); // A2

        let mut a3 = at(5, 0);
        apply_arrival_rules(&mut a3, cautious);
        assert_eq!((a3.hour, a3.minute), (7, 10)); // A3

        let mut a4 = at(7, 5);
        apply_arrival_rules(&mut a4, cautious);
        assert_eq!((a4.hour, a4.minute), (7, 10)); // A4

        let mut a5 = at(17, 5);
        apply_arrival_rules(&mut a5, cautious);
        assert_eq!((a5.hour, a5.minute), (17, 5)); // A5

        let mut a6 = at(18, 30);
        apply_arrival_rules(&mut a6, cautious);
        assert_eq!((a6.day, a6.hour, a6.minute), (1, 7, 10)); // A6

        let mut a7 = at(3, 0);
        apply_arrival_rules(&mut a7, reckless);
        assert_eq!((a7.hour, a7.minute), (3, 0)); // A7
    }

    #[test]
    fn bresenham_counts_each_pixel_once() {
        // Diagonal: max(|dx|,|dy|) movements, the minor axis advances when
        // the accumulator exceeds the major axis (SPEC §8.1).
        let report = calculate_travel_time(
            &UniformTerrain(227),
            (10, 10),
            (13, 11),
            Transport::Foot,
            &TravelOptions::default(),
        )
        .expect("uniform terrain");
        assert_eq!(report.pixels, 3);
        assert_eq!(report.ocean_pixels, 0);

        let report = calculate_travel_route(
            &UniformTerrain(227),
            &[(0, 0), (1, 0), (2, 0), (3, 0)],
            Transport::Foot,
            &TravelOptions::default(),
        )
        .expect("uniform terrain");
        assert_eq!(report.pixels, 3);
        assert_eq!(report.minutes, 3 * 124);
    }

    #[test]
    fn missing_terrain_and_unknown_climate_are_typed_errors() {
        let lookup = |x: i32, _y: i32| if x < 0 { None } else { Some(231u8) };
        let error = calculate_travel_time(
            &lookup,
            (0, 0),
            (-3, 0),
            Transport::Foot,
            &TravelOptions::default(),
        )
        .expect_err("negative side has no terrain");
        assert!(matches!(error, TravelError::MissingTerrain { .. }));

        assert_eq!(
            terrain_modifier(222),
            Err(TravelError::UnknownClimate { value: 222 })
        );
        assert_eq!(terrain_modifier(226), Ok(220));
        assert_eq!(terrain_modifier(232), Ok(250));
        assert!(terrain_modifier(233).is_err());
    }

    #[test]
    fn cost_guard_keeps_c2_positive() {
        // Without the guard: (3 - 30)/24 = -1 => pieces = -5 + 5 = 0.
        let guarded = calculate_trip_cost(153, 30, true, false, false, true);
        assert!(guarded.pieces >= 0);
        // Free tavern rooms (KnightlyOrder) skip the inn charge.
        let free = calculate_trip_cost(5200, 0, true, true, false, false);
        assert_eq!(free.pieces, 0);
    }

    #[test]
    fn akatosh_blessing_truncates_like_the_source() {
        assert_eq!(akatosh_fast_travel(5200, 0), 4940);
        assert_eq!(akatosh_fast_travel(5200, 95), 0);
    }
}

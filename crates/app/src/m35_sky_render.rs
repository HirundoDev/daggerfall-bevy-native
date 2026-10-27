//! M35 V7 — the L0–L4 sky of the annex, drawn inside the M18 window.
//!
//! The existing window only cleared the framebuffer with
//! `ClearColor(0.40, 0.62, 0.94)` (the flat blue CS-11 rejects). This module
//! replaces that constant with the five documented layers of
//! [`daggerfall_heart::sky::compose`], consumed verbatim — M35 never restates
//! an hour band, a palette or a moon phase:
//!
//! | Layer | Model input | Render |
//! |---|---|---|
//! | L0 | `SkyPalette{top, horizon}` | RGBA8 256² gradient, unlit quad |
//! | L1 | `star_field(seed)` + `moons(&clock)` | RGBA8 512² baked, `AlphaMode::Blend` |
//! | L2 | `CloudCover{quads, density, drift}` | ≤ `CLOUD_MAX_QUADS` drifting quads |
//! | L3 | `PrecipitationSpread{particles, intensity}` | intensity of the M18 sheet (no second system) |
//! | L4 | `FogVolume{density}` | [`DistanceFog`] on the `Camera3d` |
//!
//! Every quad is a child of the `Camera3d` entity, so the sky is glued to the
//! viewport at any pitch/yaw with no per-frame parenting work; the size is
//! recomputed from the camera's **real** [`Projection`] (fov, aspect, far),
//! never from a hardcoded 45°/16:9/1000.
//!
//! Measured API of this build (`bevy 0.20.0`):
//! `Projection::Perspective(PerspectiveProjection { fov: FRAC_PI_4, aspect_ratio: 1.0,
//! near: 0.1, far: 1000.0 })` is the `Camera3d` default and `camera_system`
//! rewrites `aspect_ratio` from the window every resize, so the quad size is
//! resynced whenever `(fov, aspect, far)` changes. `DistanceFog` is
//! `{ color: Color, directional_light_color: Color, directional_light_exponent: f32,
//! falloff: FogFalloff }` and `FogFalloff::Exponential { density: f32 }` is the
//! variant used here (`bevy_pbr-0.20.0/src/fog.rs:60-120`).

use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;

use daggerfall_heart::clock::{GameClock, MINUTES_PER_DAY, MINUTES_PER_HOUR};
use daggerfall_heart::sky::{
    compose, hour_band, star_field, CloudCover, HourBand, MoonDisc, SkyLayers, SkyPalette,
    CLOUD_MAX_QUADS, MOON_NAMES, PRECIPITATION_MAX_PARTICLES,
};
use daggerfall_heart::weather::WeatherType;
use daggerfall_parser_rs::climate::{WorldClimate, WorldSeason};

use crate::m18_demo::NPC_SUBCLIMATE;
use daggerfall_parser_bevy_bridge as bridge;

/// `true` while the M35 sky owns the framebuffer. Flipping it to `false`
/// restores the pure M18 path: no sky quads are spawned, `ClearColor` keeps the
/// literal of the M18 window and no `DistanceFog` is inserted.
pub const M35_ENABLED: bool = true;

/// `true` while the M35 sky also owns `ClearColor` (set to the horizon colour
/// of the band). With `false` the window keeps its own constant.
pub const M35_OWNS_CLEAR_COLOR: bool = true;

/// Overscan of the L0/L1 quads: 1.0 covers the frustum exactly, and a
/// hairline of clear colour can still show at the border after the float
/// rounding of `tan(fov/2)`. 2 % removes it in every aspect.
pub const M35_SKY_OVERSCAN: f32 = 1.02;

/// Depth of L0 as a fraction of the camera `far` plane (spec: `far × 0.9`).
pub const M35_L0_DEPTH: f32 = 0.90;
/// Depth of L1 (stars + moons): 2 % in front of L0 to win the depth test.
pub const M35_L1_DEPTH: f32 = 0.88;
/// Depth of L2 (cloud planes): inside the terrain's far range so mountains
/// still occlude a cloud that is behind them.
pub const M35_L2_DEPTH: f32 = 0.60;

/// Side of the L0 gradient texture (px).
pub const M35_GRADIENT_SIZE: u32 = 256;
/// Side of the baked L1 texture (px).
pub const M35_STARS_SIZE: u32 = 512;
/// Side of the L2 cloud texture (px, tiled by the `Repeat` sampler).
pub const M35_CLOUD_SIZE: u32 = 256;

/// Ground tone of the lower half of L0, as a fraction of `horizon`
/// (`[DECLARED]`: the model pins `top`/`horizon` only — `sky.rs`
/// `SkyPalette` — so the below-horizon falloff is this declared darkening).
pub const M35_GROUND_DARKEN: f32 = 0.35;

/// Radius of the Masser disc on the L1 texture (px of 512).
pub const M35_MASSER_RADIUS: u32 = 44;
/// Radius of the Secunda disc on the L1 texture (px of 512).
pub const M35_SECUNDA_RADIUS: u32 = 24;

/// Cloud drift speed in viewport widths per second at `density == 1`.
pub const M35_CLOUD_DRIFT: f32 = 0.012;

/// Seed of the L1/L2 layers (deterministic capture).
pub const M35_SEED: u64 = 20_261_026;

/// Fallback `far` when the projection does not carry a finite one.
pub const M35_DEFAULT_FAR: f32 = 1000.0;

/// Weather cycle of the `K` key (annex: Clear → Rain → Snow → Fog).
pub const M35_WEATHER_CYCLE: [WeatherType; 4] = [
    WeatherType::Sunny,
    WeatherType::Rain,
    WeatherType::Snow,
    WeatherType::Fog,
];

/// The four seasons the `N` key cycles (M35 V7: the demo has no calendar).
pub const M35_SEASON_CYCLE: [WorldSeason; 4] = [
    WorldSeason::Spring,
    WorldSeason::Summer,
    WorldSeason::Autumn,
    WorldSeason::Winter,
];

// ── Model state ────────────────────────────────────────────────────────────

/// M35 state of the window: the two inputs the model does not take from the
/// clock (`season`, `weather`) plus the resolved `sky_base` of the demo capital.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct M35SkyState {
    /// Season driving `nature_archive` (V6) and the winter ground.
    pub season: WorldSeason,
    /// Weather state driving L2/L3/L4.
    pub weather: WeatherType,
    /// Seed of L1/L2.
    pub seed: u64,
    /// `ClimateSettings::sky_base` of the demo capital (`0/8/16/24`).
    pub sky_base: u32,
    /// `false` = the model returned `None` (outside `SKY_BASES`); the sky is
    /// then left to the M18 path instead of inventing a palette.
    pub active: bool,
}

impl Default for M35SkyState {
    fn default() -> Self {
        Self {
            season: WorldSeason::Summer,
            weather: WeatherType::Sunny,
            seed: M35_SEED,
            sky_base: 16,
            active: true,
        }
    }
}

/// Climate of the demo capital: the same `CLIMATE.PAK` subclimate the M18
/// scene declares for the capital (`NPC_SUBCLIMATE = 231`, Woodlands).
pub fn demo_climate() -> WorldClimate {
    WorldClimate::from_index(NPC_SUBCLIMATE).unwrap_or(WorldClimate::Woodlands)
}

// ── Pure geometry (asserted headless) ──────────────────────────────────────
/// Size of a camera-space quad at `distance` that covers the whole frustum.
///
/// A quad centred on the camera axis at `z = -distance` spans
/// `2·d·tan(fov/2)` vertically and `2·d·tan(fov/2)·aspect` horizontally; the
/// result is multiplied by [`M35_SKY_OVERSCAN`] so float rounding cannot leave
/// a border of clear colour. Returns `[width, height]` in meters.
pub fn quad_extent(fov_y: f32, aspect: f32, distance: f32) -> [f32; 2] {
    let height = 2.0 * distance * (fov_y * 0.5).tan() * M35_SKY_OVERSCAN;
    let width = height * aspect.max(f32::EPSILON);
    [width, height]
}

/// Fraction of the viewport the quad of [`quad_extent`] covers on each axis
/// (`1.0` = exactly the frustum, `> 1.0` = overscan). The headless test asserts
/// `>= 1.0` on both axes for four aspects, which is the "the sky never leaves a
/// border" property.
#[cfg(test)]
pub fn coverage(fov_y: f32, aspect: f32, distance: f32, extent: [f32; 2]) -> [f32; 2] {
    let height = 2.0 * distance * (fov_y * 0.5).tan();
    let width = height * aspect.max(f32::EPSILON);
    [extent[0] / width, extent[1] / height]
}

/// Distances of the three sky layers for one camera `far`.
pub fn layer_depths(far: f32) -> [f32; 3] {
    let far = if far.is_finite() && far > 0.0 {
        far
    } else {
        M35_DEFAULT_FAR
    };
    [far * M35_L0_DEPTH, far * M35_L1_DEPTH, far * M35_L2_DEPTH]
}

/// Ground tone of the lower half of L0: `horizon` darkened by
/// [`M35_GROUND_DARKEN`].
pub fn ground_color(horizon: [u8; 3]) -> [u8; 3] {
    let scale = |channel: u8| (f32::from(channel) * (1.0 - M35_GROUND_DARKEN)).round() as u8;
    [scale(horizon[0]), scale(horizon[1]), scale(horizon[2])]
}

/// Interpolates two sRGB triples (`t = 0` ⇒ `from`, `t = 1` ⇒ `to`).
fn lerp_rgb(from: [u8; 3], to: [u8; 3], t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
    [mix(from[0], to[0]), mix(from[1], to[1]), mix(from[2], to[2])]
}

/// RGBA8 L0 texture: `top` at row 0 → `horizon` at the vertical middle →
/// [`ground_color`] at the last row.
///
/// The model pins only two colours, so the third stop is the declared
/// darkening: the spec asks for "top→horizon in the upper half, horizon→ground
/// in the lower half", and a horizon that is the *same* colour at the middle
/// row makes both halves continuous by construction.
pub fn gradient_rgba(palette: SkyPalette, size: u32) -> Vec<u8> {
    let size = size.max(2);
    let rows = size as usize;
    let ground = ground_color(palette.horizon);
    let mut bytes = Vec::with_capacity(rows * rows * 4);
    for y in 0..rows {
        let v = y as f32 / (rows - 1) as f32;
        let rgb = if v <= 0.5 {
            lerp_rgb(palette.top, palette.horizon, v * 2.0)
        } else {
            lerp_rgb(palette.horizon, ground, (v - 0.5) * 2.0)
        };
        for _ in 0..rows {
            bytes.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
    }
    bytes
}

/// Alpha of one star: proportional to its magnitude (`magnitude ∈ 0..=1`).
pub fn star_alpha(magnitude: f32) -> u8 {
    (magnitude.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Fraction of the unit disc that lies right of the terminator `x = t`.
///
/// `A(t) = ½ − (asin(t) + t·√(1−t²)) / π`, monotone decreasing on `[-1, 1]`
/// with `A(-1) = 1` (full disc) and `A(1) = 0` (new moon).
pub fn lit_area(t: f32) -> f32 {
    let t = t.clamp(-1.0, 1.0);
    let root = (1.0 - t * t).max(0.0).sqrt();
    0.5 - (t.asin() + t * root) / std::f32::consts::PI
}

/// Terminator position of an illuminated fraction: the `t` whose lit area is
/// exactly `illuminated` (bisection on the monotone [`lit_area`], 40 steps).
///
/// A straight terminator would make the lit *width* equal to the fraction but
/// not the lit *mass*; the bisection keeps `lit_area(terminator) == fraction`,
/// which is what `MoonDisc::illuminated_fraction` documents and what the unit
/// test asserts for the four phases.
pub fn terminator_x(illuminated: f32) -> f32 {
    let want = illuminated.clamp(0.0, 1.0);
    if want <= 0.0 {
        return 1.0;
    }
    if want >= 1.0 {
        return -1.0;
    }
    let (mut lo, mut hi) = (-1.0_f32, 1.0_f32);
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        if lit_area(mid) > want {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// `(lit, total)` pixels of a disc of `radius` at an illuminated fraction —
/// the headless measurement of the phase mask.
#[cfg(test)]
pub fn lit_mass(illuminated: f32, radius: u32) -> (u32, u32) {
    let terminator = terminator_x(illuminated);
    let radius_i = radius as i32;
    let mut lit = 0_u32;
    let mut total = 0_u32;
    for y in -radius_i..=radius_i {
        for x in -radius_i..=radius_i {
            if x * x + y * y > radius_i * radius_i {
                continue;
            }
            total += 1;
            let nx = x as f32 / radius_i as f32;
            if nx >= terminator {
                lit += 1;
            }
        }
    }
    (lit, total)
}

/// Radius of a moon on the L1 texture: Masser (the larger) vs Secunda.
pub fn moon_radius(name: &str) -> u32 {
    if name == MOON_NAMES[0] {
        M35_MASSER_RADIUS
    } else {
        M35_SECUNDA_RADIUS
    }
}

/// Centre of a moon on the L1 texture, in pixels (declared placement: the two
/// discs never overlap and both stay inside the quad).
pub fn moon_center(name: &str, size: u32) -> [u32; 2] {
    let side = size as f32;
    if name == MOON_NAMES[0] {
        [(side * 0.70) as u32, (side * 0.30) as u32]
    } else {
        [(side * 0.28) as u32, (side * 0.18) as u32]
    }
}

/// RGBA8 L1 texture: the [`star_field`] of `seed` plus the two [`MoonDisc`]s.
///
/// A star is one pixel with `alpha = star_alpha(magnitude)`; a moon is a disc
/// of its own radius whose lit mass follows `illuminated_fraction` (the shadow
/// is `alpha = 0`, so a new moon is invisible instead of a black hole).
pub fn bake_discs_rgba(seed: u64, moons: &[MoonDisc; 2], size: u32) -> Vec<u8> {
    let size = size.max(8);
    let side = size as usize;
    let mut bytes = vec![0_u8; side * side * 4];
    for star in star_field(seed) {
        let x = ((star.x.clamp(0.0, 0.999_999) * side as f32) as usize).min(side - 1);
        let y = ((star.y.clamp(0.0, 0.999_999) * side as f32) as usize).min(side - 1);
        let at = (y * side + x) * 4;
        bytes[at..at + 4].copy_from_slice(&[255, 255, 240, star_alpha(star.magnitude)]);
    }
    for moon in moons {
        let radius = moon_radius(moon.name).min(size / 2) as i32;
        let center = moon_center(moon.name, size);
        let terminator = terminator_x(moon.illuminated_fraction);
        let (cx, cy) = (center[0] as i32, center[1] as i32);
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy > radius * radius {
                    continue;
                }
                let px = cx + dx;
                let py = cy + dy;
                if px < 0 || py < 0 || px >= side as i32 || py >= side as i32 {
                    continue;
                }
                let nx = dx as f32 / radius as f32;
                if nx < terminator {
                    continue;
                }
                // Limb darkening of the lit mass (declared: the model pins no
                // colour, so the disc is a flat grey with a soft edge).
                let edge = 1.0 - (dx * dx + dy * dy) as f32 / (radius * radius) as f32;
                let shade = (200.0 + 55.0 * edge.sqrt()).min(255.0) as u8;
                let at = ((py as usize) * side + px as usize) * 4;
                bytes[at..at + 4].copy_from_slice(&[shade, shade, shade.min(250), 255]);
            }
        }
    }
    bytes
}

/// Quads the L2 layer draws: `min(CLOUD_MAX_QUADS, quads)`, and **0** when the
/// density is 0 (a clear sky draws no cloud at all).
pub fn cloud_quad_count(cover: CloudCover) -> usize {
    // `!(density > 0.0)` also catches NaN, so the clear-sky branch is explicit.
    if cover.density <= 0.0 || cover.density.is_nan() {
        return 0;
    }
    usize::from(cover.quads.min(CLOUD_MAX_QUADS))
}

/// Deterministic value hash of one lattice point (`fbm` input).
fn lattice(x: i32, y: i32, seed: u64) -> f32 {
    let mut hash = seed ^ (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    hash ^= (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    hash ^= hash >> 29;
    hash = hash.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    hash ^= hash >> 32;
    (hash & 0xFFFF) as f32 / 65_535.0
}

/// Value noise with a smoothstep interpolant (tileable on `period` cells).
fn value_noise(x: f32, y: f32, period: i32, seed: u64) -> f32 {
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let smooth = |t: f32| t * t * (3.0 - 2.0 * t);
    let wrap = |v: i32| v.rem_euclid(period);
    let (sx, sy) = (smooth(fx), smooth(fy));
    let a = lattice(wrap(x0), wrap(y0), seed);
    let b = lattice(wrap(x0 + 1), wrap(y0), seed);
    let c = lattice(wrap(x0), wrap(y0 + 1), seed);
    let d = lattice(wrap(x0 + 1), wrap(y0 + 1), seed);
    let top = a + (b - a) * sx;
    let bottom = c + (d - c) * sx;
    top + (bottom - top) * sy
}

/// RGBA8 L2 cloud texture: 4-octave value noise, `Repeat`-tileable, with the
/// alpha being the smoothstepped density (`0` below 0.45, `255` above 0.78).
pub fn cloud_rgba(seed: u64, size: u32) -> Vec<u8> {
    let size = size.max(8);
    let side = size as usize;
    let mut bytes = Vec::with_capacity(side * side * 4);
    for y in 0..side {
        for x in 0..side {
            let u = x as f32 / side as f32;
            let v = y as f32 / side as f32;
            let mut amplitude = 0.5_f32;
            let mut frequency = 4_i32;
            let mut sum = 0.0_f32;
            let mut norm = 0.0_f32;
            for _ in 0..4 {
                sum += amplitude * value_noise(u * frequency as f32, v * frequency as f32, frequency, seed);
                norm += amplitude;
                amplitude *= 0.5;
                frequency *= 2;
            }
            let noise = if norm > 0.0 { sum / norm } else { 0.0 };
            let alpha = ((noise - 0.45) / 0.33).clamp(0.0, 1.0);
            bytes.extend_from_slice(&[242, 244, 248, (alpha * 255.0).round() as u8]);
        }
    }
    bytes
}

/// Minutes to advance so the clock lands on the next band boundary
/// (`hour_band` windows: `[0,360) night`, `[360,480) dawn`, `[480,1020) day`
/// `[1020,1080) dusk`, `[1080,1440) night`; the boundaries are the clock's own
/// `DAWN_HOUR`/`LIGHTS_OFF_HOUR`/`LIGHTS_ON_HOUR`/`DUSK_HOUR`, never literals).
pub fn minutes_to_next_band(minute_of_day: u64) -> u64 {
    let now = minute_of_day % MINUTES_PER_DAY;
    let hours = MINUTES_PER_DAY / MINUTES_PER_HOUR;
    let mut next = u64::MAX;
    // The boundaries are derived from `hour_band` itself, so this can never
    // drift from the band the palette actually uses: a candidate counts only
    // when the band changes across it.
    for hour in 1..=hours {
        let boundary = hour * MINUTES_PER_HOUR;
        if hour_band(boundary - 1) == hour_band(boundary % MINUTES_PER_DAY) {
            continue;
        }
        // `18:00` is itself a boundary: a minute sitting on one must advance
        // to the *following* boundary, so the comparison is strictly greater.
        let candidate = if boundary > now {
            boundary
        } else {
            boundary + MINUTES_PER_DAY
        };
        next = next.min(candidate);
    }
    debug_assert!(next != u64::MAX, "hour_band has no transition in a day");
    next - now
}

/// One `Season` cycle step (`Spring → Summer → Autumn → Winter → Spring`).
pub fn next_season(season: WorldSeason) -> WorldSeason {
    let at = M35_SEASON_CYCLE
        .iter()
        .position(|candidate| *candidate == season)
        .unwrap_or(1);
    M35_SEASON_CYCLE[(at + 1) % M35_SEASON_CYCLE.len()]
}

/// One weather cycle step (`Clear → Rain → Snow → Fog → Clear`).
pub fn next_weather(weather: WeatherType) -> WeatherType {
    let at = M35_WEATHER_CYCLE
        .iter()
        .position(|candidate| *candidate == weather)
        .unwrap_or(0);
    M35_WEATHER_CYCLE[(at + 1) % M35_WEATHER_CYCLE.len()]
}

/// `DistanceFog` of the L4 layer: horizon colour + the model's fog density as
/// an exponential falloff (`0` density ⇒ no fog, because
/// `1 − exp(0) = 0`).
pub fn distance_fog(layers: &SkyLayers) -> DistanceFog {
    DistanceFog {
        color: Color::srgb_u8(
            layers.l0.horizon[0],
            layers.l0.horizon[1],
            layers.l0.horizon[2],
        ),
        falloff: FogFalloff::Exponential {
            density: layers.l4.density.max(0.0) as f32,
        },
        ..default()
    }
}

/// `ClearColor` of the band: the horizon colour (the L0 quad hides it anyway;
/// it is the safety net under the overscan).
pub fn clear_color(layers: &SkyLayers) -> Color {
    Color::srgb_u8(
        layers.l0.horizon[0],
        layers.l0.horizon[1],
        layers.l0.horizon[2],
    )
}

/// L3 streaks of the existing M18 sheet for one intensity
/// (`intensity × PRECIPITATION_MAX_PARTICLES` of the model, clamped to the
/// sheet's own cap).
pub fn streak_count(layers: &SkyLayers, sheet_cap: usize) -> usize {
    let particles = f32::from(layers.l3.particles.min(PRECIPITATION_MAX_PARTICLES));
    let by_sheet = (particles / f32::from(PRECIPITATION_MAX_PARTICLES) * sheet_cap as f32)
        .round() as usize;
    by_sheet.min(sheet_cap)
}

// ── Render rig ─────────────────────────────────────────────────────────────

/// Handles and child entities of the sky of one camera.
#[derive(Resource, Debug)]
struct M35SkyRig {
    camera: Entity,
    l0: Entity,
    l1: Entity,
    clouds: Vec<Entity>,
    // The two images are rewritten in place by `m35_recompose_sky`; the
    // materials and the cloud image are only reachable through the entities'
    // `MeshMaterial3d`, so they are deliberately not kept here.
    l0_image: Handle<Image>,
    l1_image: Handle<Image>,
}

/// Last projection seen by [`m35_sync_sky_geometry`] (`(fov, aspect, far)`):
/// the quads are only resized when the camera changes, never per frame.
#[derive(Default)]
struct M35ProjectionCache(Option<(f32, f32, f32)>);

/// Last composition key seen by [`m35_recompose_sky`]: the layers are only
/// rebuilt when the band, the weather, the season, the seed or `sky_base`
/// changed — never per frame.
#[derive(Default)]
struct M35CompositionKey(Option<(HourBand, WeatherType, WorldSeason, u64, u32)>);

/// Centred unit quad (`[-0.5, 0.5]²`) facing `+Z`, UV `v = 0` at the top row.
fn sky_quad_spec() -> bridge::MeshSpec {
    bridge::MeshSpec {
        positions: vec![
            [-0.5, -0.5, 0.0],
            [0.5, -0.5, 0.0],
            [0.5, 0.5, 0.0],
            [-0.5, 0.5, 0.0],
        ],
        normals: vec![[0.0, 0.0, 1.0]; 4],
        indices: vec![0, 1, 2, 0, 2, 3],
        uvs: vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
        face_textures: Vec::new(),
    }
}

/// Unlit, two-sided, alpha-blended material over one sky texture. `fog_enabled`
/// is `false`: the L4 fog must not flatten the sky it is derived from.
fn sky_material(texture: Handle<Image>, blend: bool) -> StandardMaterial {
    StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(texture),
        unlit: true,
        cull_mode: None,
        fog_enabled: false,
        alpha_mode: if blend {
            AlphaMode::Blend
        } else {
            AlphaMode::Opaque
        },
        ..default()
    }
}

/// Composition of the clock + state; `None` when the model rejects `sky_base`
/// (outside `SKY_BASES = {0, 8, 16, 24}`) or carries no palette.
fn current_layers(clock: &GameClock, state: &M35SkyState) -> Option<SkyLayers> {
    compose(clock, state.sky_base, state.weather, state.seed)
}

/// `Projection` → `(vertical fov, aspect, far)` with the documented defaults
/// when the camera carries a custom/orthographic projection.
fn projection_params(projection: &Projection) -> (f32, f32, f32) {
    match projection {
        Projection::Perspective(perspective) => {
            (perspective.fov, perspective.aspect_ratio, perspective.far)
        }
        // `Orthographic` and `Custom` carry no vertical fov / perspective far:
        // the documented default frustum of the demo is used instead.
        Projection::Orthographic(_) | Projection::Custom(_) => (
            std::f32::consts::FRAC_PI_4,
            16.0 / 9.0,
            M35_DEFAULT_FAR,
        ),
    }
}

/// Startup: builds the three textures, the two materials and the child quads of
/// the `Camera3d` (L0, L1 and up to `CLOUD_MAX_QUADS` clouds).
#[allow(clippy::too_many_arguments)]
fn m35_spawn_sky(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut clear: ResMut<ClearColor>,
    mut state: ResMut<M35SkyState>,
    world: Option<Res<super::DemoWorld>>,
    clock: Option<Res<super::ProgressionClock>>,
    camera: Query<(Entity, &Projection), With<Camera3d>>,
) {
    if !M35_ENABLED {
        info!("M35 VE: sky disabled by M35_ENABLED — M18 clear colour path");
        return;
    }
    let Ok((camera_entity, projection)) = camera.single() else {
        info!("M35 VE: no Camera3d yet — sky not spawned");
        return;
    };
    let _ = world; // the scene is read by the vegetation layer (V6).

    let climate = demo_climate();
    state.sky_base = climate.settings().sky_base;
    let game_clock = clock
        .as_ref()
        .map(|progression| progression.clock)
        .unwrap_or_else(GameClock::classic_default);
    // The M35 clock is read from the window's own `ProgressionClock`; it is
    // moved to the first band after the spawn so a session that starts in the
    // menu still shows a composed sky.
    let Some(layers) = current_layers(&game_clock, &state) else {
        state.active = false;
        info!(
            "M35 VE: sky_base {} outside SKY_BASES — M18 clear colour kept",
            state.sky_base
        );
        return;
    };
    state.active = true;

    let (fov, aspect, far) = projection_params(projection);
    let depths = layer_depths(far);
    let l0_extent = quad_extent(fov, aspect, depths[0]);
    let l1_extent = quad_extent(fov, aspect, depths[1]);
    let l2_extent = quad_extent(fov, aspect, depths[2]);

    let l0_rgba = gradient_rgba(layers.l0, M35_GRADIENT_SIZE);
    let l1_rgba = bake_discs_rgba(state.seed, &layers.l1, M35_STARS_SIZE);
    let l2_rgba = cloud_rgba(state.seed, M35_CLOUD_SIZE);

    let mut image = |rgba: &[u8], side: u32| {
        bridge::image_from_rgba(rgba, side, side, &mut images)
            .map_err(|error| error.to_string())
    };
    let (Ok(l0_image), Ok(l1_image), Ok(cloud_image)) = (
        image(&l0_rgba, M35_GRADIENT_SIZE),
        image(&l1_rgba, M35_STARS_SIZE),
        image(&l2_rgba, M35_CLOUD_SIZE),
    ) else {
        state.active = false;
        info!("M35 VE: sky texture upload failed — M18 clear colour kept");
        return;
    };

    let l0_material = materials.add(sky_material(l0_image.clone(), false));
    let l1_material = materials.add(sky_material(l1_image.clone(), true));
    let cloud_material = materials.add(sky_material(cloud_image.clone(), true));
    let l0_mesh = bridge::mesh_from_spec(&sky_quad_spec(), &mut meshes);
    let cloud_mesh = bridge::mesh_from_spec(&sky_quad_spec(), &mut meshes);

    let l0 = commands
        .spawn((
            Mesh3d(l0_mesh.clone()),
            MeshMaterial3d(l0_material.clone()),
            Transform::from_xyz(0.0, 0.0, -depths[0])
                .with_scale(Vec3::new(l0_extent[0], l0_extent[1], 1.0)),
            ChildOf(camera_entity),
        ))
        .id();
    let l1 = commands
        .spawn((
            Mesh3d(l0_mesh.clone()),
            MeshMaterial3d(l1_material.clone()),
            Transform::from_xyz(0.0, 0.0, -depths[1])
                .with_scale(Vec3::new(l1_extent[0], l1_extent[1], 1.0)),
            ChildOf(camera_entity),
        ))
        .id();

    // L2: one quad per model quad, spread vertically and wider than the
    // viewport so the horizontal drift never uncovers a border.
    let quads = cloud_quad_count(layers.l2);
    let mut clouds = Vec::with_capacity(quads);
    for index in 0..quads {
        let slot = index as f32;
        let spread = if quads > 1 {
            slot / (quads - 1) as f32
        } else {
            0.5
        };
        let y = (spread - 0.5) * l2_extent[1] * 0.55;
        let entity = commands
            .spawn((
                Mesh3d(cloud_mesh.clone()),
                MeshMaterial3d(cloud_material.clone()),
                Transform::from_xyz(0.0, y, -depths[2]).with_scale(Vec3::new(
                    l2_extent[0] * 2.0,
                    l2_extent[1] * 0.35,
                    1.0,
                )),
                ChildOf(camera_entity),
            ))
            .id();
        clouds.push(entity);
    }

    if M35_OWNS_CLEAR_COLOR {
        *clear = ClearColor(clear_color(&layers));
    }
    info!(
        "M35 VE: sky spawned camera={camera_entity:?} far={far:.1} fov={fov:.4} aspect={aspect:.4} \
extent_l0=({:.1},{:.1}) extent_l1=({:.1},{:.1}) clouds={quads} band={} weather={:?} sky_base={}",
        l0_extent[0],
        l0_extent[1],
        l1_extent[0],
        l1_extent[1],
        hour_band(game_clock.minute_of_day()).name(),
        state.weather,
        state.sky_base
    );

    commands.insert_resource(M35SkyRig {
        camera: camera_entity,
        l0,
        l1,
        clouds,
        l0_image,
        l1_image,
    });
}

/// Update: keeps the quads glued to the camera frustum. Only writes transforms
/// when `(fov, aspect, far)` changed (`camera_system` rewrites `aspect_ratio`
/// on every resize), so the steady state is a single `Projection` read.
fn m35_sync_sky_geometry(
    rig: Option<Res<M35SkyRig>>,
    camera: Query<&Projection, With<Camera3d>>,
    mut transforms: Query<&mut Transform>,
    mut cache: Local<M35ProjectionCache>,
) {
    let Some(rig) = rig else { return };
    let Ok(projection) = camera.get(rig.camera) else {
        return;
    };
    let (fov, aspect, far) = projection_params(projection);
    let key = (fov, aspect, far);
    if cache.0 == Some(key) {
        return;
    }
    cache.0 = Some(key);
    let depths = layer_depths(far);
    let l0_extent = quad_extent(fov, aspect, depths[0]);
    let l1_extent = quad_extent(fov, aspect, depths[1]);
    let l2_extent = quad_extent(fov, aspect, depths[2]);
    let mut place = |entity: Entity, depth: f32, extent: [f32; 2]| {
        if let Ok(mut transform) = transforms.get_mut(entity) {
            transform.translation = Vec3::new(0.0, 0.0, -depth);
            transform.scale = Vec3::new(extent[0], extent[1], 1.0);
        }
    };
    place(rig.l0, depths[0], l0_extent);
    place(rig.l1, depths[1], l1_extent);
    for (index, entity) in rig.clouds.iter().enumerate() {
        let spread = if rig.clouds.len() > 1 {
            index as f32 / (rig.clouds.len() - 1) as f32
        } else {
            0.5
        };
        if let Ok(mut transform) = transforms.get_mut(*entity) {
            transform.translation.y = (spread - 0.5) * l2_extent[1] * 0.55;
            transform.scale = Vec3::new(l2_extent[0] * 2.0, l2_extent[1] * 0.35, 1.0);
        }
    }
}

/// Update: recomposes the five layers when the clock band, the weather, the
/// season, the seed or `sky_base` changed — not per frame.
#[allow(clippy::too_many_arguments)]
fn m35_recompose_sky(
    mut commands: Commands,
    rig: Option<Res<M35SkyRig>>,
    state: Res<M35SkyState>,
    clock: Option<Res<super::ProgressionClock>>,
    mut images: ResMut<Assets<Image>>,
    mut clear: ResMut<ClearColor>,
    mut cache: Local<M35CompositionKey>,
) {
    let Some(rig) = rig else { return };
    if !M35_ENABLED {
        return;
    }
    let game_clock = clock
        .as_ref()
        .map(|progression| progression.clock)
        .unwrap_or_else(GameClock::classic_default);
    let band = hour_band(game_clock.minute_of_day());
    let key = (band, state.weather, state.season, state.seed, state.sky_base);
    if cache.0 == Some(key) {
        return;
    }
    let Some(layers) = current_layers(&game_clock, &state) else {
        info!("M35 VE: sky_base {} rejected by the model", state.sky_base);
        return;
    };
    cache.0 = Some(key);

    let mut write = |handle: &Handle<Image>, bytes: Vec<u8>| {
        if let Some(mut image) = images.get_mut(handle) {
            image.data = Some(bytes);
        }
    };
    write(&rig.l0_image, gradient_rgba(layers.l0, M35_GRADIENT_SIZE));
    write(
        &rig.l1_image,
        bake_discs_rgba(state.seed, &layers.l1, M35_STARS_SIZE),
    );
    if M35_OWNS_CLEAR_COLOR {
        *clear = ClearColor(clear_color(&layers));
    }
    commands
        .entity(rig.camera)
        .insert(distance_fog(&layers));
    info!(
        "M35 VE: band={} season={:?} weather={:?} sky_base={} top={:?} horizon={:?} \
moon_masser={:.2} moon_secunda={:.2} l2_quads={} l2_density={:.2} l3_particles={} l4_fog={:.3}",
        band.name(),
        state.season,
        state.weather,
        state.sky_base,
        layers.l0.top,
        layers.l0.horizon,
        layers.l1[0].illuminated_fraction,
        layers.l1[1].illuminated_fraction,
        layers.l2.quads,
        layers.l2.density,
        layers.l3.particles,
        layers.l4.density
    );
}

/// Update: horizontal drift of the cloud planes, scaled by the density and
/// bounced at ±half the viewport (a ping-pong instead of a wrap, so no seam is
/// ever visible).
fn m35_drift_clouds(
    rig: Option<Res<M35SkyRig>>,
    state: Res<M35SkyState>,
    clock: Option<Res<super::ProgressionClock>>,
    time: Res<Time>,
    camera: Query<&Projection, With<Camera3d>>,
    mut transforms: Query<&mut Transform>,
) {
    let Some(rig) = rig else { return };
    let Ok(projection) = camera.get(rig.camera) else {
        return;
    };
    let game_clock = clock
        .as_ref()
        .map(|progression| progression.clock)
        .unwrap_or_else(GameClock::classic_default);
    let Some(layers) = current_layers(&game_clock, &state) else {
        return;
    };
    let cover = layers.l2;
    if cloud_quad_count(cover) == 0 {
        return;
    }
    let (fov, aspect, far) = projection_params(projection);
    let depths = layer_depths(far);
    let extent = quad_extent(fov, aspect, depths[2]);
    let bound = extent[0] * 0.5;
    let step = M35_CLOUD_DRIFT * cover.density * extent[0] * time.delta_secs();
    for entity in &rig.clouds {
        let Ok(mut transform) = transforms.get_mut(*entity) else {
            continue;
        };
        transform.translation.x += step;
        if transform.translation.x > bound {
            transform.translation.x = bound - (transform.translation.x - bound);
        } else if transform.translation.x < -bound {
            transform.translation.x = -bound + (-bound - transform.translation.x);
        }
    }
}

/// Update: L3 wiring — the M35 intensity scales the streaks of the **existing**
/// M18 precipitation sheet (no second particle system). A dry state leaves the
/// `O` path untouched.
fn m35_weather_bridge(
    state: Res<M35SkyState>,
    clock: Option<Res<super::ProgressionClock>>,
    weather: Option<ResMut<super::DemoWeather>>,
    mut images: ResMut<Assets<Image>>,
    mut last: Local<Option<WeatherType>>,
) {
    let Some(mut weather) = weather else { return };
    if *last == Some(state.weather) {
        return;
    }
    *last = Some(state.weather);
    // Read the sheet geometry before borrowing `precip` mutably.
    let size = weather.size;
    let game_clock = clock
        .as_ref()
        .map(|progression| progression.clock)
        .unwrap_or_else(GameClock::classic_default);
    let Some(layers) = current_layers(&game_clock, &state) else {
        return;
    };
    if layers.l3.intensity <= 0.0 {
        return;
    }
    let Some(precip) = weather.precip.as_mut() else {
        info!(
            "M35 VE: L3 intensity {:.2} without a resolved Precipitation — M18 sheet has no substrate",
            layers.l3.intensity
        );
        return;
    };
    let drops = streak_count(&layers, super::PRECIP_MAX_STREAKS);
    if drops == 0 {
        return;
    }
    precip.kind = if state.weather.is_snowing() {
        super::WeatherKind::Snow
    } else {
        super::WeatherKind::Rain
    };
    precip.drops = drops;
    let bytes = super::precipitation_sheet(precip, size);
    let Some(handle) = weather.handle.clone() else {
        return;
    };
    if let Some(mut image) = images.get_mut(&handle) {
        image.data = Some(bytes);
    }
    info!(
        "M35 VE: L3 intensity={:.2} particles={} streaks={}",
        layers.l3.intensity, layers.l3.particles, drops
    );
}

/// Update: `N` (season), `K` (weather) and `M` (next hour band).
///
/// The three keys are the only free letters of the M18 window (`Enter`,
/// `,`, `.`, `1..6`, `T`, `R`, `L`, `J`, `I`, `O` are taken).
fn m35_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<M35SkyState>,
    clock: Option<ResMut<super::ProgressionClock>>,
) {
    if keys.just_pressed(KeyCode::KeyN) {
        state.season = next_season(state.season);
        info!("M35 VE: season={:?}", state.season);
    }
    if keys.just_pressed(KeyCode::KeyK) {
        state.weather = next_weather(state.weather);
        info!("M35 VE: weather={:?}", state.weather);
    }
    if keys.just_pressed(KeyCode::KeyM) {
        let Some(mut progression) = clock else {
            info!("M35 VE: no ProgressionClock — M is a no-op");
            return;
        };
        let minutes = minutes_to_next_band(progression.clock.minute_of_day());
        progression.clock.raise_seconds(minutes * 60 + 1);
        progression.last_minute = progression.clock.total_minutes() as i64;
        info!(
            "M35 VE: clock advanced {minutes} min to band={} minute_of_day={}",
            hour_band(progression.clock.minute_of_day()).name(),
            progression.clock.minute_of_day()
        );
    }
}

/// M35 sky plugin: one registration in the window's `App`.
pub struct M35SkyPlugin;

impl Plugin for M35SkyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<M35SkyState>()
            .add_systems(Startup, m35_spawn_sky.after(super::m18_spawn_world))
            .add_systems(
                Update,
                (
                    m35_sync_sky_geometry,
                    m35_recompose_sky,
                    m35_drift_clouds,
                    m35_weather_bridge,
                    m35_keys,
                ),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use daggerfall_heart::calendar::LunarPhase;
    use daggerfall_heart::sky::illuminated_fraction;

    const DAY: SkyPalette = SkyPalette {
        top: [58, 118, 190],
        horizon: [170, 205, 232],
    };

    #[test]
    fn sky_quad_covers_the_viewport_in_four_aspects() {
        // Real defaults of this build: fov π/4, far 1000 → L0 at 900 m.
        let fov = std::f32::consts::FRAC_PI_4;
        let far = M35_DEFAULT_FAR;
        let distance = layer_depths(far)[0];
        for aspect in [1280.0 / 720.0, 1920.0 / 1080.0, 4.0 / 3.0, 21.0 / 9.0] {
            let extent = quad_extent(fov, aspect, distance);
            let covered = coverage(fov, aspect, distance, extent);
            assert!(
                covered[0] >= 1.0 && covered[1] >= 1.0,
                "aspect {aspect}: coverage {covered:?} leaves a border"
            );
            // The unfiltered frustum is fully inside the quad.
            let raw = [extent[0] / M35_SKY_OVERSCAN, extent[1] / M35_SKY_OVERSCAN];
            let raw_covered = coverage(fov, aspect, distance, raw);
            assert!((raw_covered[0] - 1.0).abs() < 1e-3);
            assert!((raw_covered[1] - 1.0).abs() < 1e-3);
        }
    }

    #[test]
    fn the_layer_depths_use_the_real_far() {
        let depths = layer_depths(M35_DEFAULT_FAR);
        assert!((depths[0] - 900.0).abs() < 1e-3);
        assert!(depths[1] < depths[0] && depths[2] < depths[1]);
        // Non-finite far falls back to the documented default.
        let fallback = layer_depths(f32::INFINITY);
        assert!((fallback[0] - M35_DEFAULT_FAR * M35_L0_DEPTH).abs() < 1e-3);
    }

    #[test]
    fn gradient_hits_top_horizon_and_ground() {
        let size = M35_GRADIENT_SIZE;
        let bytes = gradient_rgba(DAY, size);
        assert_eq!(bytes.len(), (size * size * 4) as usize);
        let pixel = |row: u32, column: u32| {
            let at = ((row * size + column) * 4) as usize;
            [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]
        };
        assert_eq!(pixel(0, 0), [58, 118, 190, 255], "top row must be `top`");
        let middle = pixel(size / 2, 0);
        for channel in 0..3 {
            let want = i32::from(DAY.horizon[channel]);
            let got = i32::from(middle[channel]);
            assert!(
                (want - got).abs() <= 2,
                "middle row {middle:?} != horizon {:?}",
                DAY.horizon
            );
        }
        let ground = ground_color(DAY.horizon);
        assert_eq!(pixel(size - 1, 0), [ground[0], ground[1], ground[2], 255]);
    }

    #[test]
    fn star_alpha_is_proportional_to_magnitude() {
        assert_eq!(star_alpha(0.0), 0);
        assert_eq!(star_alpha(1.0), 255);
        assert_eq!(star_alpha(0.5), 128);
        assert!(star_alpha(0.9) > star_alpha(0.2));
        // Out of range inputs saturate instead of wrapping.
        assert_eq!(star_alpha(-1.0), 0);
        assert_eq!(star_alpha(4.0), 255);
    }

    #[test]
    fn moon_phase_mass_matches_the_four_phases() {
        let radius = M35_MASSER_RADIUS;
        // The four canonical phases of `illuminated_fraction`: new, one-quarter,
        // half, full (the eighths grid of `daggerfall_heart::sky`).
        for (phase, want) in [
            (LunarPhase::New, 0.0_f32),
            (LunarPhase::OneWax, 0.25),
            (LunarPhase::HalfWax, 0.5),
            (LunarPhase::Full, 1.0),
        ] {
            let fraction = illuminated_fraction(phase);
            let (lit, total) = lit_mass(fraction, radius.min(16));
            let got = lit as f32 / total as f32;
            assert!(
                (got - want).abs() < 0.05,
                "{phase:?}: lit mass {got:.3} != {want:.3}"
            );
        }
        // Waxing is mirrored: the shadow grows back on the other side.
        let (lit_wax, _) = lit_mass(0.25, 16);
        let (lit_wane, _) = lit_mass(0.75, 16);
        assert!(lit_wax < lit_wane);
    }

    #[test]
    fn the_disc_texture_keeps_the_moons_of_the_model() {
        let moons = [
            MoonDisc {
                name: MOON_NAMES[0],
                phase: LunarPhase::Full,
                illuminated_fraction: illuminated_fraction(LunarPhase::Full),
            },
            MoonDisc {
                name: MOON_NAMES[1],
                phase: LunarPhase::New,
                illuminated_fraction: illuminated_fraction(LunarPhase::New),
            },
        ];
        let size = M35_STARS_SIZE;
        let bytes = bake_discs_rgba(M35_SEED, &moons, size);
        assert_eq!(bytes.len(), (size * size * 4) as usize);
        // Masser (full) drew its whole disc; Secunda (new) drew nothing.
        let lit_pixels = |center: [u32; 2], radius: u32| {
            let mut count = 0;
            for y in center[1] - radius..=center[1] + radius {
                for x in center[0] - radius..=center[0] + radius {
                    let at = ((y * size + x) * 4) as usize;
                    if bytes[at + 3] == 255 && bytes[at] > 200 {
                        count += 1;
                    }
                }
            }
            count
        };
        let masser = lit_pixels(moon_center(MOON_NAMES[0], size), M35_MASSER_RADIUS);
        let secunda = lit_pixels(moon_center(MOON_NAMES[1], size), M35_SECUNDA_RADIUS);
        assert!(masser > 5_000, "Masser full disc drew only {masser} px");
        assert_eq!(secunda, 0, "a new moon must not draw a shadow disc");
        // Radii differ by name.
        assert!(moon_radius(MOON_NAMES[0]) > moon_radius(MOON_NAMES[1]));
    }

    #[test]
    fn cloud_quads_follow_the_cover_and_vanish_at_zero_density() {
        let cover = |quads, density| CloudCover {
            quads,
            density,
            drift: 0.5,
        };
        assert_eq!(cloud_quad_count(cover(8, 0.0)), 0, "density 0 ⇒ 0 quads");
        assert_eq!(cloud_quad_count(cover(3, 0.4)), 3);
        assert_eq!(
            cloud_quad_count(cover(9, 1.0)),
            usize::from(CLOUD_MAX_QUADS),
            "the model cap is never exceeded"
        );
        assert_eq!(cloud_quad_count(cover(0, 1.0)), 0);
    }

    #[test]
    fn cloud_texture_is_deterministic_and_tileable() {
        let a = cloud_rgba(M35_SEED, 32);
        let b = cloud_rgba(M35_SEED, 32);
        assert_eq!(a, b, "the same seed must bake the same texture");
        assert_ne!(a, cloud_rgba(M35_SEED + 1, 32));
        assert!(a.chunks(4).any(|pixel| pixel[3] > 0), "no opaque cloud at all");
        assert!(a.chunks(4).any(|pixel| pixel[3] == 0), "the sky is fully overcast");
    }

    #[test]
    fn next_band_lands_on_a_different_band() {
        for minute in 0..MINUTES_PER_DAY {
            let advance = minutes_to_next_band(minute);
            assert!(advance > 0, "minute {minute}: no advance");
            let target = (minute + advance) % MINUTES_PER_DAY;
            assert_ne!(
                hour_band(minute),
                hour_band(target),
                "minute {minute}: landed inside the same band ({target})"
            );
        }
        assert_eq!(minutes_to_next_band(0), 360);
        assert_eq!(minutes_to_next_band(359), 1);
        // 18:01 is Night; the next band change is Dawn at 06:00 the next day,
        // not the midnight wrap (midnight is still Night).
        assert_eq!(minutes_to_next_band(1081), 719);
        // Sitting exactly on the 18:00 boundary advances the whole night.
        assert_eq!(minutes_to_next_band(1080), 720);
    }

    #[test]
    fn seasons_and_weathers_cycle_without_repetition() {
        let mut season = WorldSeason::Spring;
        for _ in 0..4 {
            season = next_season(season);
        }
        assert_eq!(season, WorldSeason::Spring);
        assert_eq!(next_season(WorldSeason::Winter), WorldSeason::Spring);
        let mut weather = WeatherType::Sunny;
        let mut seen = Vec::new();
        for _ in 0..M35_WEATHER_CYCLE.len() {
            weather = next_weather(weather);
            seen.push(weather);
        }
        assert_eq!(weather, WeatherType::Sunny);
        assert_eq!(seen.len(), 4);
    }

    #[test]
    fn streaks_are_scaled_from_the_model_particles() {
        let layers = |particles: u16, intensity: f32| SkyLayers {
            l0: DAY,
            l1: [
                MoonDisc {
                    name: MOON_NAMES[0],
                    phase: LunarPhase::Full,
                    illuminated_fraction: 1.0,
                },
                MoonDisc {
                    name: MOON_NAMES[1],
                    phase: LunarPhase::Full,
                    illuminated_fraction: 1.0,
                },
            ],
            l2: CloudCover {
                quads: 0,
                density: 0.0,
                drift: 0.0,
            },
            l3: daggerfall_heart::sky::PrecipitationSpread {
                particles,
                intensity,
            },
            l4: daggerfall_heart::sky::FogVolume {
                volumes: 1,
                density: 0.0,
            },
        };
        assert_eq!(streak_count(&layers(0, 0.0), 1_400), 0);
        assert_eq!(streak_count(&layers(4_096, 1.0), 1_400), 1_400);
        let half = streak_count(&layers(2_048, 0.5), 1_400);
        assert!((600..=800).contains(&half), "half intensity → {half} streaks");
    }

    #[test]
    fn distance_fog_uses_the_horizon_and_the_model_density() {
        let layers = SkyLayers {
            l0: DAY,
            l1: [
                MoonDisc {
                    name: MOON_NAMES[0],
                    phase: LunarPhase::Full,
                    illuminated_fraction: 1.0,
                },
                MoonDisc {
                    name: MOON_NAMES[1],
                    phase: LunarPhase::Full,
                    illuminated_fraction: 1.0,
                },
            ],
            l2: CloudCover {
                quads: 0,
                density: 0.0,
                drift: 0.0,
            },
            l3: daggerfall_heart::sky::PrecipitationSpread {
                particles: 0,
                intensity: 0.0,
            },
            l4: daggerfall_heart::sky::FogVolume {
                volumes: 1,
                density: 0.25,
            },
        };
        let fog = distance_fog(&layers);
        assert_eq!(fog.color, Color::srgb_u8(170, 205, 232));
        match fog.falloff {
            FogFalloff::Exponential { density } => assert!((density - 0.25).abs() < 1e-6),
            other => panic!("unexpected falloff {other:?}"),
        }
    }
}

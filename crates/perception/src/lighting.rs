//! `lighting` — M34 §1.6 «lighting» (S47): torch, spell/item light,
//! underwater fog, sun and dungeon light (W34-6, `CR-2d5c8f41`).
//!
//! The rules are **closed** by the spec
//! (`m34-e2e-integration-presentation-spec-CR-5243af63.md:109-123`,
//! GV-10/11/12/14) and by the DFU sources each row cites. This module
//! **consumes** them; it defines no formula of its own. Every constant below
//! carries its `DFU@0038a1e6:file:line`, and every derived value is a total,
//! pure function so the probe can recompute it in a second process and
//! compare digests (Anti-Screen: nothing is remembered, everything is
//! recomputed from the cited rule).
//!
//! | Block | Rule (spec §1.6) | Source |
//! |---|---|---|
//! | Torch | base `1.0`, with item `1.25`; final `torchIntensity · PlayerTorchLightScale · intensityMod`, `intensityMod ∈ {1, 1.25, 0.85 + cos(x)·0.2}` while `currentCondition < 3`; active outside with `IsCityLightsOn` or inside a dungeon | `EnablePlayerTorch.cs:31-32,64,93-94,98,105,108` |
//! | Ambient | exterior day `0.9` · exterior night `0.25` · interior `0.18` · interior night `0.20,0.18,0.20` · dungeon `0.12` · castle `0.58` | `PlayerAmbientLight.cs:25-35,58,101-118` |
//! | Fade | `FadeDuration = 3 s`, `FadeStep = 0.1 s` ⇒ increment `0.1/3` per step; poll `1/3 s` | `PlayerAmbientLight.cs:34-35,58,101-107` |
//! | Day scaled | `Lerp(night, midday, DaylightScale · ScaleFactor)` | `PlayerAmbientLight.cs:114-118` |
//! | Sun | `dawn = DawnHour·60`, `dayRange = (DuskHour − DawnHour)·60`, `time = (MinuteOfDay − dawn)/dayRange`, `xrot = 180·time`; `DawnHour = 6`, `DuskHour = 18` | `SunlightManager.cs:110-115`, `DaggerfallDateTime.cs:48-49` |
//! | Underwater fog | entry `= 0.025·(−waterLevel) + 0.38`, exit `= entry − 0.02`, `fogT = 1 − (y − exit)/(entry − exit)`, density `0…0.25` | `UnderwaterFog.cs:30,45-51,70` |
//! | Dungeon light | range `2060 cu · 0.025 = 51.5 m`, refresh `0.4 s` | `DungeonLightHandler.cs:25-26,60` |
//!
//! ## Declared clamps (never silent)
//!
//! Three rules in the table are unbounded in the source and bounded in the
//! consumer. Each clamp is explicit, named, and returned alongside the raw
//! value so a caller (and the probe) can see both:
//!
//! - [`AmbientLight::lerp`] clamps `t` to `[0, 1]` (Unity `Color.Lerp`
//!   semantics — `DaylightScale · ScaleFactor` can exceed 1 at midday).
//! - [`SunOrientation`] keeps `time_raw`/`xrot_raw_degrees` **unclamped** and
//!   exposes the renderer's `time`/`xrot_degrees` clamped to `0..1` / `0..180`
//!   (`MinuteOfDay` outside `[dawn, dusk)` would otherwise rotate the light
//!   below the horizon or past sunset).
//! - [`UnderwaterFog::fog_t_raw`] is the literal formula;
//!   [`UnderwaterFog::fog_t`] clamps it to `[0, 1]` because the declared
//!   density range is `0…0.25` (a `fogDensity` cannot be negative nor exceed
//!   its maximum).
//!
//! ## Unit note (`GLOBAL_SCALE`)
//!
//! `0.025 m/cu` is [`daggerfall_nervous::audio::GLOBAL_SCALE`] — the one
//! declaration of `MeshReader.GlobalScale` in the workspace, consumed here
//! instead of being re-typed (same rod as `RDBSide = 2048·0.025` in W34-D).

use daggerfall_nervous::audio::GLOBAL_SCALE;

// ─────────────────────────────────────────────────────────────────────────
// Torch — EnablePlayerTorch.cs:31-32,64,93-94,98,105,108
// ─────────────────────────────────────────────────────────────────────────

/// `torchIntensity` without a torch in the inventory
/// (`EnablePlayerTorch.cs:31-32,98`; spec §1.6 «base `1.0`»).
pub const TORCH_INTENSITY: f32 = 1.0;

/// `torchIntensity` with a torch in the inventory
/// (`EnablePlayerTorch.cs:64,98`; spec §1.6 «con ítem `1.25`»).
pub const TORCH_ITEM_INTENSITY: f32 = 1.25;

/// Ratio between the two intensities — **exactly** `1.25` (spec §1.6 asserts
/// the ratio, so it is derived here, never typed twice).
pub const TORCH_ITEM_RATIO: f32 = TORCH_ITEM_INTENSITY / TORCH_INTENSITY;

/// Floor of the guttering modifier: `0.85`
/// (`EnablePlayerTorch.cs:93-94`).
pub const TORCH_GUTTER_FLOOR: f32 = 0.85;

/// Amplitude of the guttering modifier: `0.2` (`EnablePlayerTorch.cs:93-94`).
pub const TORCH_GUTTER_AMPLITUDE: f32 = 0.2;

/// Guttering applies while `currentCondition < 3`
/// (`EnablePlayerTorch.cs:93-94`; a torch below condition 3 sputters).
pub const TORCH_GUTTER_CONDITION_MAX: i32 = 3;

/// Identity value of the DFU setting `PlayerTorchLightScale` — the middle
/// factor of the final product. It is a user setting, not a spec literal, so
/// it is a parameter of [`torch_intensity`]; this constant is only the
/// multiplicative identity used by the calls that do not override it.
pub const PLAYER_TORCH_LIGHT_SCALE_IDENTITY: f32 = 1.0;

/// The three `intensityMod` values of the spec (§1.6): `{1.0, 1.25,
/// 0.85 + cos(x)·0.2}`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TorchModifier {
    /// No modifier: `1.0`.
    Base,
    /// Torch item modifier: `1.25` (`TORCH_ITEM_INTENSITY / TORCH_INTENSITY`).
    Item,
    /// Guttering torch: `0.85 + cos(phase)·0.2`, range `[0.65, 1.05]`
    /// (`EnablePlayerTorch.cs:93-94`).
    Guttering {
        /// Guttering phase in radians (the source feeds `Time.time`; the
        /// caller owns the clock, we own the arithmetic).
        phase_radians: f32,
    },
}

impl TorchModifier {
    /// The modifier's factor (`intensityMod` of the spec product).
    pub fn factor(self) -> f32 {
        match self {
            Self::Base => 1.0,
            Self::Item => TORCH_ITEM_RATIO,
            Self::Guttering { phase_radians } => guttering_factor(phase_radians),
        }
    }
}

/// `0.85 + cos(phase)·0.2` (`EnablePlayerTorch.cs:93-94`).
pub fn guttering_factor(phase_radians: f32) -> f32 {
    TORCH_GUTTER_FLOOR + phase_radians.cos() * TORCH_GUTTER_AMPLITUDE
}

/// Which modifier a torch at `current_condition` gets: `Guttering` while
/// `current_condition < 3`, `Base` otherwise (`EnablePlayerTorch.cs:93-94`).
/// The `Item` modifier is a different axis (inventory), see [`TorchState`].
pub fn torch_modifier_for_condition(current_condition: i32, phase_radians: f32) -> TorchModifier {
    if current_condition < TORCH_GUTTER_CONDITION_MAX {
        TorchModifier::Guttering { phase_radians }
    } else {
        TorchModifier::Base
    }
}

/// Final torch intensity: `torchIntensity · PlayerTorchLightScale ·
/// intensityMod` (`EnablePlayerTorch.cs:31-32,64,98,108`).
pub fn torch_intensity(
    torch_intensity: f32,
    player_torch_light_scale: f32,
    modifier: TorchModifier,
) -> f32 {
    torch_intensity * player_torch_light_scale * modifier.factor()
}

/// Live torch state: the two intensity literals plus the condition gate.
///
/// `torchIntensity` and the modifier fold the same axis twice in the source
/// (`:64` writes `1.25` when a torch is held, `:93-94` writes the guttering
/// value while `currentCondition < 3`); both are kept, and the *product* is
/// what the light receives — a guttering torch-item therefore reads
/// `1.25 · 0.85 + cos(x)·0.2` exactly as the two assignments compose.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TorchState {
    /// `torchIntensity`: [`TORCH_INTENSITY`] or [`TORCH_ITEM_INTENSITY`].
    pub torch_intensity: f32,
    /// `PlayerTorchLightScale` (DFU setting; identity `1.0`).
    pub player_torch_light_scale: f32,
    /// `EntityEffect.currentCondition` (gutter while `< 3`).
    pub current_condition: i32,
}

impl TorchState {
    /// State for a player holding (or not) a torch, with the identity light
    /// scale and a healthy condition (no guttering).
    pub fn new(has_torch_item: bool) -> Self {
        Self {
            torch_intensity: if has_torch_item {
                TORCH_ITEM_INTENSITY
            } else {
                TORCH_INTENSITY
            },
            player_torch_light_scale: PLAYER_TORCH_LIGHT_SCALE_IDENTITY,
            current_condition: TORCH_GUTTER_CONDITION_MAX,
        }
    }

    /// The modifier this state produces at `phase_radians`.
    pub fn modifier(&self, phase_radians: f32) -> TorchModifier {
        torch_modifier_for_condition(self.current_condition, phase_radians)
    }

    /// Final intensity of this state at `phase_radians`.
    pub fn intensity(&self, phase_radians: f32) -> f32 {
        torch_intensity(
            self.torch_intensity,
            self.player_torch_light_scale,
            self.modifier(phase_radians),
        )
    }
}

/// Is the player torch lit? Exterior with the city lights on **or** inside a
/// dungeon (`EnablePlayerTorch.cs:105`; spec §1.6 «cuándo»).
pub fn torch_active(is_exterior: bool, city_lights_on: bool, in_dungeon: bool) -> bool {
    in_dungeon || (is_exterior && city_lights_on)
}

// ─────────────────────────────────────────────────────────────────────────
// Ambient light — PlayerAmbientLight.cs:25-35,58,101-118
// ─────────────────────────────────────────────────────────────────────────

/// An ambient colour in linear `[0, 1]` channels (Unity `Color`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AmbientLight {
    /// Red channel.
    pub r: f32,
    /// Green channel.
    pub g: f32,
    /// Blue channel.
    pub b: f32,
}

impl AmbientLight {
    /// Const constructor.
    pub const fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    /// Grey (all three channels equal).
    pub const fn grey(value: f32) -> Self {
        Self::new(value, value, value)
    }

    /// `Color.Lerp(a, b, t)` (`PlayerAmbientLight.cs:114-118`): `t` clamped to
    /// `[0, 1]`, and the endpoints returned **exactly** at `t = 0`/`t = 1`.
    pub fn lerp(self, other: Self, t: f32) -> Self {
        if t <= 0.0 {
            return self;
        }
        if t >= 1.0 {
            return other;
        }
        Self::new(
            self.r + (other.r - self.r) * t,
            self.g + (other.g - self.g) * t,
            self.b + (other.b - self.b) * t,
        )
    }

    /// GV-11 conversion to bytes: `round(x · 255)` per channel.
    pub fn to_u8(self) -> [u8; 3] {
        [to_u8(self.r), to_u8(self.g), to_u8(self.b)]
    }

    /// `r,g,b` with 6 decimals (evidence formatting, no rounding to bytes).
    pub fn label(self) -> String {
        format!("{:.6},{:.6},{:.6}", self.r, self.g, self.b)
    }
}

/// Exterior, day: `0.9` (`PlayerAmbientLight.cs:25-35`; GV-10).
pub const AMBIENT_EXTERIOR_DAY: AmbientLight = AmbientLight::grey(0.9);

/// Exterior, night: `0.25` (`PlayerAmbientLight.cs:25-35`; GV-10).
pub const AMBIENT_EXTERIOR_NIGHT: AmbientLight = AmbientLight::grey(0.25);

/// Interior, day: `0.18` (`PlayerAmbientLight.cs:25-35`; GV-10).
pub const AMBIENT_INTERIOR: AmbientLight = AmbientLight::grey(0.18);

/// Interior, night: `0.20, 0.18, 0.20` (`PlayerAmbientLight.cs:25-35`; GV-10).
pub const AMBIENT_INTERIOR_NIGHT: AmbientLight = AmbientLight::new(0.20, 0.18, 0.20);

/// Dungeon: `0.12` (`PlayerAmbientLight.cs:25-35`; GV-10).
pub const AMBIENT_DUNGEON: AmbientLight = AmbientLight::grey(0.12);

/// Castle: `0.58` (`PlayerAmbientLight.cs:25-35`; GV-10).
pub const AMBIENT_CASTLE: AmbientLight = AmbientLight::grey(0.58);

/// Where the player is, as far as ambient light is concerned (spec §1.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmbientContext {
    /// Exterior, day.
    ExteriorDay,
    /// Exterior, night.
    ExteriorNight,
    /// Interior (buildings), day.
    Interior,
    /// Interior (buildings), night — the only non-grey entry.
    InteriorNight,
    /// Dungeon.
    Dungeon,
    /// Castle interior.
    Castle,
}

impl AmbientContext {
    /// Every context, in the spec table order.
    pub const ALL: [Self; 6] = [
        Self::ExteriorDay,
        Self::ExteriorNight,
        Self::Interior,
        Self::InteriorNight,
        Self::Dungeon,
        Self::Castle,
    ];

    /// Stable lower-case name (probe/doc literal).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExteriorDay => "exterior_day",
            Self::ExteriorNight => "exterior_night",
            Self::Interior => "interior",
            Self::InteriorNight => "interior_night",
            Self::Dungeon => "dungeon",
            Self::Castle => "castle",
        }
    }

    /// The ambient colour of this context (spec §1.6, GV-10).
    pub fn light(self) -> AmbientLight {
        match self {
            Self::ExteriorDay => AMBIENT_EXTERIOR_DAY,
            Self::ExteriorNight => AMBIENT_EXTERIOR_NIGHT,
            Self::Interior => AMBIENT_INTERIOR,
            Self::InteriorNight => AMBIENT_INTERIOR_NIGHT,
            Self::Dungeon => AMBIENT_DUNGEON,
            Self::Castle => AMBIENT_CASTLE,
        }
    }
}

/// Coarse spatial classification used to pick an [`AmbientContext`]
/// (`PlayerAmbientLight.cs:58,101-118`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpatialContext {
    /// Open air (city, wilderness).
    Exterior,
    /// A building interior.
    Interior,
    /// A dungeon.
    Dungeon,
    /// A castle interior.
    Castle,
}

/// Pick the ambient context: exterior/interior depend on day/night; dungeon
/// and castle are fixed (`PlayerAmbientLight.cs:58,101-118`). Day/night
/// itself is the `heart::clock` `IsDay` predicate (`DawnHour ≤ hour <
/// DuskHour`) — this module only maps it.
pub fn ambient_context(spatial: SpatialContext, is_day: bool) -> AmbientContext {
    match (spatial, is_day) {
        (SpatialContext::Exterior, true) => AmbientContext::ExteriorDay,
        (SpatialContext::Exterior, false) => AmbientContext::ExteriorNight,
        (SpatialContext::Interior, true) => AmbientContext::Interior,
        (SpatialContext::Interior, false) => AmbientContext::InteriorNight,
        (SpatialContext::Dungeon, _) => AmbientContext::Dungeon,
        (SpatialContext::Castle, _) => AmbientContext::Castle,
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Fade — PlayerAmbientLight.cs:34-35,58,101-107
// ─────────────────────────────────────────────────────────────────────────

/// `FadeDuration = 3 s` (`PlayerAmbientLight.cs:34`; GV-12).
pub const FADE_DURATION_S: f32 = 3.0;

/// `FadeStep = 0.1 s` (`PlayerAmbientLight.cs:35`; GV-12).
pub const FADE_STEP_S: f32 = 0.1;

/// Per-step advance: `0.1 / 3` (`PlayerAmbientLight.cs:101-107`; GV-12).
pub const FADE_INCREMENT: f32 = FADE_STEP_S / FADE_DURATION_S;

/// Change-detection poll: `1/3 s` (`PlayerAmbientLight.cs:58`; GV-12).
pub const FADE_POLL_S: f32 = 1.0 / 3.0;

/// Steps needed to cross the whole fade, derived (`3 / 0.1 = 30`) — never a
/// second literal.
pub const FADE_STEPS: u32 = 30;

/// Fraction crossed after `step` steps, clamped to `1.0` so the last step
/// lands exactly on the target instead of `31·increment > 1`.
pub fn fade_fraction(step: u32) -> f32 {
    (step as f32 * FADE_INCREMENT).min(1.0)
}

/// Colour after `step` steps of the fade `from → to`
/// (`PlayerAmbientLight.cs:101-118`).
pub fn fade_colour(from: AmbientLight, to: AmbientLight, step: u32) -> AmbientLight {
    from.lerp(to, fade_fraction(step))
}

/// `Color.Lerp(night, midday, DaylightScale · ScaleFactor)`
/// (`PlayerAmbientLight.cs:114-118`; GV-12 «día escalado»). The product is
/// clamped by [`AmbientLight::lerp`].
pub fn day_scaled(
    night: AmbientLight,
    midday: AmbientLight,
    daylight_scale: f32,
    scale_factor: f32,
) -> AmbientLight {
    night.lerp(midday, daylight_scale * scale_factor)
}

// ─────────────────────────────────────────────────────────────────────────
// GV-11 — u8 conversion
// ─────────────────────────────────────────────────────────────────────────

/// GV-11 factor (`round(x · 255)`).
pub const U8_SCALE: f32 = 255.0;

/// GV-11: `round(x · 255)` clamped to the byte range. Computed in `f32` (the
/// renderer's channel type) so the four spec literals are reproduced rather
/// than approximated in another precision.
pub fn to_u8(channel: f32) -> u8 {
    (channel * U8_SCALE).round().clamp(0.0, U8_SCALE) as u8
}

// ─────────────────────────────────────────────────────────────────────────
// Sun — SunlightManager.cs:110-115, DaggerfallDateTime.cs:48-49
// ─────────────────────────────────────────────────────────────────────────

/// `DaggerfallDateTime.DawnHour` (`DaggerfallDateTime.cs:48-49`).
pub const DAWN_HOUR: u32 = 6;

/// `DaggerfallDateTime.DuskHour` (`DaggerfallDateTime.cs:48-49`).
pub const DUSK_HOUR: u32 = 18;

/// Minutes per hour (`MinuteOfDay = Hour·60 + Minute`).
pub const MINUTES_PER_HOUR: u32 = 60;

/// Sun `xrot` span: `180·time` (`SunlightManager.cs:110-115`).
pub const SUN_XROT_SPAN_DEGREES: f32 = 180.0;

/// Sun orientation for one `MinuteOfDay`, with the raw and the clamped
/// values side by side (the declared clamp is part of the contract, see the
/// module docs).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SunOrientation {
    /// `dawn = DawnHour · 60` (minutes).
    pub dawn_minute: f32,
    /// `dayRange = (DuskHour − DawnHour) · 60` (minutes).
    pub day_range_minutes: f32,
    /// Literal `time = (MinuteOfDay − dawn) / dayRange` — **not** clamped.
    pub time_raw: f32,
    /// `time_raw` clamped to `[0, 1]` (what the renderer consumes).
    pub time: f32,
    /// Literal `xrot = 180 · time_raw` (degrees).
    pub xrot_raw_degrees: f32,
    /// `180 · time` (degrees), the clamped rotation.
    pub xrot_degrees: f32,
    /// `true` when `MinuteOfDay` lies outside `[dawn, dusk)`.
    pub clamped: bool,
}

/// `SunlightManager.cs:110-115`:
/// `dawn = DawnHour·60`, `dayRange = (DuskHour − DawnHour)·60`,
/// `time = (MinuteOfDay − dawn)/dayRange`, `xrot = 180·time`.
pub fn sun_orientation(minute_of_day: u32) -> SunOrientation {
    let dawn_minute = (DAWN_HOUR * MINUTES_PER_HOUR) as f32;
    let dusk_minute = (DUSK_HOUR * MINUTES_PER_HOUR) as f32;
    let day_range_minutes = dusk_minute - dawn_minute;
    let time_raw = (minute_of_day as f32 - dawn_minute) / day_range_minutes;
    let time = time_raw.clamp(0.0, 1.0);
    SunOrientation {
        dawn_minute,
        day_range_minutes,
        time_raw,
        time,
        xrot_raw_degrees: SUN_XROT_SPAN_DEGREES * time_raw,
        xrot_degrees: SUN_XROT_SPAN_DEGREES * time,
        clamped: !(0.0..=1.0).contains(&time_raw),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Underwater fog — UnderwaterFog.cs:30,45-51,70
// ─────────────────────────────────────────────────────────────────────────

/// Entry offset of the underwater fog: `0.38 m` (`UnderwaterFog.cs:30`).
pub const UNDERWATER_FOG_ENTRY_OFFSET: f32 = 0.38;

/// Exit is `entry − 0.02` (`UnderwaterFog.cs:45-51`).
pub const UNDERWATER_FOG_THICKNESS: f32 = 0.02;

/// Declared density ceiling `0.25` (`UnderwaterFog.cs:70`; spec §1.6
/// «densidad `0…0.25`»).
pub const UNDERWATER_FOG_DENSITY_MAX: f32 = 0.25;

/// The two water planes, both derived from the block water level in metres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnderwaterFog {
    /// `entry = 0.025·(−waterLevel) + 0.38` (metres, `UnderwaterFog.cs:30`).
    pub entry_y: f32,
    /// `exit = entry − 0.02` (metres, `UnderwaterFog.cs:45-51`).
    pub exit_y: f32,
}

impl UnderwaterFog {
    /// From the DFU `blockWaterLevel` in **world units** (negative below the
    /// ground): `entry = GLOBAL_SCALE · (−waterLevel) + 0.38`.
    pub fn from_water_level(water_level_cu: f32) -> Self {
        let entry_y = GLOBAL_SCALE * (-water_level_cu) + UNDERWATER_FOG_ENTRY_OFFSET;
        Self {
            entry_y,
            exit_y: entry_y - UNDERWATER_FOG_THICKNESS,
        }
    }

    /// Literal `fogT = 1 − (y − exit)/(entry − exit)` (`UnderwaterFog.cs:45-51`),
    /// **unclamped**.
    pub fn fog_t_raw(&self, y: f32) -> f32 {
        1.0 - (y - self.exit_y) / (self.entry_y - self.exit_y)
    }

    /// `fog_t_raw` clamped to `[0, 1]` (declared; the density range is
    /// `0…0.25`).
    pub fn fog_t(&self, y: f32) -> f32 {
        self.fog_t_raw(y).clamp(0.0, 1.0)
    }

    /// Fog density at height `y`: the clamped `fogT` mapped linearly onto the
    /// declared `0…0.25` band (`UnderwaterFog.cs:70`). Linear over the
    /// declared endpoints is the minimal reading of the range — the source
    /// gives the range, not a transfer function.
    pub fn density(&self, y: f32) -> f32 {
        self.fog_t(y) * UNDERWATER_FOG_DENSITY_MAX
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Dungeon light — DungeonLightHandler.cs:25-26,60
// ─────────────────────────────────────────────────────────────────────────

/// `DungeonLightHandler` light range in world units: `2060`
/// (`DungeonLightHandler.cs:25-26`).
pub const DUNGEON_LIGHT_RANGE_CU: f32 = 2060.0;

/// The same range in metres: `2060 · 0.025` (`DungeonLightHandler.cs:25-26`;
/// spec §1.6 «`2060 cu · 0.025 = 51.5 m`»). Derived, never typed twice.
pub const DUNGEON_LIGHT_RANGE_METERS: f32 = DUNGEON_LIGHT_RANGE_CU * GLOBAL_SCALE;

/// The handler's refresh interval: `0.4 s` (`DungeonLightHandler.cs:60`).
pub const DUNGEON_LIGHT_REFRESH_S: f32 = 0.4;

/// Does a dungeon light at distance `distance_m` (metres, absolute) still
/// reach? Range is the closed interval `[−51.5, 51.5]` m.
pub fn dungeon_light_reaches(distance_m: f32) -> bool {
    distance_m.abs() <= DUNGEON_LIGHT_RANGE_METERS
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rule 1 (spec §1.6, first two rows): base `1.0`, item `1.25`, ratio
    /// exactly `1.25` (`EnablePlayerTorch.cs:31-32,64,98`).
    #[test]
    fn torch_base_and_item_ratio_is_exactly_1_25() {
        assert_eq!(TORCH_INTENSITY, 1.0);
        assert_eq!(TORCH_ITEM_INTENSITY, 1.25);
        assert_eq!(TORCH_ITEM_RATIO, 1.25);
        assert_eq!(TORCH_ITEM_INTENSITY / TORCH_INTENSITY, 1.25);
        // Through the state helper, with the identity light scale.
        assert_eq!(TorchState::new(false).intensity(0.0), 1.0);
        assert_eq!(TorchState::new(true).intensity(0.0), 1.25);
        assert_eq!(
            TorchState::new(true).intensity(0.0) / TorchState::new(false).intensity(0.0),
            1.25
        );
    }

    /// Rule 2: `intensityMod ∈ {1.0, 1.25, 0.85 + cos(x)·0.2}` and the
    /// guttering case applies while `currentCondition < 3`
    /// (`EnablePlayerTorch.cs:93-94`).
    #[test]
    fn torch_modifier_set_and_condition_gate() {
        assert_eq!(TorchModifier::Base.factor(), 1.0);
        assert_eq!(TorchModifier::Item.factor(), 1.25);
        // 0.85 + cos(0)·0.2 = 1.05 and 0.85 + cos(π)·0.2 = 0.65 (the band).
        assert!((guttering_factor(0.0) - 1.05).abs() < 1e-6);
        assert!((guttering_factor(std::f32::consts::PI) - 0.65).abs() < 1e-7);
        let low = torch_modifier_for_condition(2, 0.0);
        assert!(matches!(low, TorchModifier::Guttering { .. }));
        let gate = torch_modifier_for_condition(3, 0.0);
        assert!(matches!(gate, TorchModifier::Base));
        let healthy = torch_modifier_for_condition(100, 0.0);
        assert!(matches!(healthy, TorchModifier::Base));
    }

    /// Rule 3: final `= torchIntensity · PlayerTorchLightScale ·
    /// intensityMod` (`EnablePlayerTorch.cs:31-32,64,98,108`).
    #[test]
    fn torch_final_intensity_is_the_three_factor_product() {
        let phase = 1.234_f32;
        let expected = 1.25 * 0.5 * (0.85 + phase.cos() * 0.2);
        let mut state = TorchState::new(true);
        state.player_torch_light_scale = 0.5;
        state.current_condition = 0;
        assert!((state.intensity(phase) - expected).abs() < 1e-6);
        // Guttering scales the item intensity, both axes composed (the
        // half light scale included).
        assert!(state.intensity(phase) < 1.25 * 0.5);
        assert!(state.intensity(phase) > 1.25 * 0.5 * 0.65);
        // Identity scale + healthy condition + no item = the bare literal.
        let plain = TorchState {
            torch_intensity: TORCH_INTENSITY,
            player_torch_light_scale: PLAYER_TORCH_LIGHT_SCALE_IDENTITY,
            current_condition: TORCH_GUTTER_CONDITION_MAX,
        };
        assert_eq!(plain.intensity(3.0), 1.0);
    }

    /// Rule 4: torch is lit outside with `IsCityLightsOn` **or** inside a
    /// dungeon (`EnablePlayerTorch.cs:105`).
    #[test]
    fn torch_activation_rule() {
        assert!(torch_active(true, true, false), "exterior with city lights");
        assert!(torch_active(false, false, true), "inside a dungeon");
        assert!(torch_active(true, true, true), "both");
        assert!(!torch_active(true, false, false), "exterior, lights off");
        assert!(!torch_active(false, false, false), "interior, no dungeon");
        // City lights are irrelevant inside a dungeon.
        assert!(torch_active(false, true, true));
    }

    /// Rule 5: the six ambient values, exactly as the spec table
    /// (`PlayerAmbientLight.cs:25-35`; GV-10).
    #[test]
    fn ambient_six_contexts() {
        assert_eq!(AmbientContext::ALL.len(), 6);
        assert_eq!(AMBIENT_EXTERIOR_DAY, AmbientLight::grey(0.9));
        assert_eq!(AMBIENT_EXTERIOR_NIGHT, AmbientLight::grey(0.25));
        assert_eq!(AMBIENT_INTERIOR, AmbientLight::grey(0.18));
        assert_eq!(AMBIENT_INTERIOR_NIGHT, AmbientLight::new(0.20, 0.18, 0.20));
        assert_eq!(AMBIENT_DUNGEON, AmbientLight::grey(0.12));
        assert_eq!(AMBIENT_CASTLE, AmbientLight::grey(0.58));
        let table = [
            (AmbientContext::ExteriorDay, AMBIENT_EXTERIOR_DAY),
            (AmbientContext::ExteriorNight, AMBIENT_EXTERIOR_NIGHT),
            (AmbientContext::Interior, AMBIENT_INTERIOR),
            (AmbientContext::InteriorNight, AMBIENT_INTERIOR_NIGHT),
            (AmbientContext::Dungeon, AMBIENT_DUNGEON),
            (AmbientContext::Castle, AMBIENT_CASTLE),
        ];
        assert_eq!(AmbientContext::ALL.len(), table.len());
        for (context, expected) in table {
            assert_eq!(context.light(), expected, "context {}", context.as_str());
        }
        // The selector: day/night only moves exterior and interior.
        assert_eq!(
            ambient_context(SpatialContext::Exterior, true),
            AmbientContext::ExteriorDay
        );
        assert_eq!(
            ambient_context(SpatialContext::Exterior, false),
            AmbientContext::ExteriorNight
        );
        assert_eq!(
            ambient_context(SpatialContext::Interior, true),
            AmbientContext::Interior
        );
        assert_eq!(
            ambient_context(SpatialContext::Interior, false),
            AmbientContext::InteriorNight
        );
        assert_eq!(
            ambient_context(SpatialContext::Dungeon, true),
            ambient_context(SpatialContext::Dungeon, false)
        );
        assert_eq!(
            ambient_context(SpatialContext::Castle, true),
            ambient_context(SpatialContext::Castle, false)
        );
    }

    /// Rule 6: `increment = 0.1/3`, 30 steps to cross, poll `1/3 s`
    /// (`PlayerAmbientLight.cs:34-35,58,101-107`; GV-12).
    #[test]
    fn fade_increment_and_poll() {
        assert_eq!(FADE_DURATION_S, 3.0);
        assert_eq!(FADE_STEP_S, 0.1);
        assert_eq!(FADE_INCREMENT, FADE_STEP_S / FADE_DURATION_S);
        assert!((FADE_INCREMENT - 0.1 / 3.0).abs() <= f32::EPSILON);
        assert_eq!(FADE_STEPS, 30);
        assert!((FADE_POLL_S - 1.0 / 3.0).abs() <= f32::EPSILON);
        // 30 steps cross the whole span; step 0 is the origin.
        assert_eq!(fade_fraction(0), 0.0);
        assert_eq!(fade_fraction(FADE_STEPS), 1.0);
        assert!((FADE_STEPS as f32 * FADE_INCREMENT - 1.0).abs() < 0.01);
        // A real fade lands on the target exactly (clamp declared).
        let from = AMBIENT_EXTERIOR_NIGHT;
        let to = AMBIENT_EXTERIOR_DAY;
        assert_eq!(fade_colour(from, to, 0), from);
        assert_eq!(fade_colour(from, to, FADE_STEPS), to);
        let half = fade_colour(from, to, FADE_STEPS / 2);
        assert!(half.r > from.r && half.r < to.r);
    }

    /// Rule 7 (GV-11): `round(x·255)` ⇒ `0.9→230`, `0.18→46`, `0.12→31`,
    /// `0.58→148`.
    #[test]
    fn gv11_u8_conversion_four_literals() {
        assert_eq!(to_u8(0.9), 230);
        assert_eq!(to_u8(0.18), 46);
        assert_eq!(to_u8(0.12), 31);
        assert_eq!(to_u8(0.58), 148);
        // Through the ambient table (the consumer path).
        assert_eq!(AMBIENT_EXTERIOR_DAY.to_u8(), [230, 230, 230]);
        assert_eq!(AMBIENT_INTERIOR.to_u8(), [46, 46, 46]);
        assert_eq!(AMBIENT_DUNGEON.to_u8(), [31, 31, 31]);
        assert_eq!(AMBIENT_CASTLE.to_u8(), [148, 148, 148]);
        // The non-grey entry keeps its asymmetry after conversion.
        assert_eq!(AMBIENT_INTERIOR_NIGHT.to_u8(), [51, 46, 51]);
        // Clamp declared at both ends.
        assert_eq!(to_u8(2.0), 255);
        assert_eq!(to_u8(-1.0), 0);
    }

    /// Rule 8: `Lerp(noche, mediodía, DaylightScale·ScaleFactor)`
    /// (`PlayerAmbientLight.cs:114-118`).
    #[test]
    fn day_scaled_lerp() {
        let night = AMBIENT_EXTERIOR_NIGHT;
        let midday = AMBIENT_EXTERIOR_DAY;
        assert_eq!(day_scaled(night, midday, 0.0, 1.0), night);
        assert_eq!(day_scaled(night, midday, 1.0, 1.0), midday);
        let half = day_scaled(night, midday, 0.5, 1.0);
        assert!((half.r - (0.25 + 0.65 * 0.5)).abs() < 1e-6);
        // ScaleFactor multiplies: 0.5·0.5 = 0.25 of the span.
        let quarter = day_scaled(night, midday, 0.5, 0.5);
        assert!((quarter.r - (0.25 + 0.65 * 0.25)).abs() < 1e-6);
        // Product above 1 is clamped to the midday end (declared).
        assert_eq!(day_scaled(night, midday, 2.0, 2.0), midday);
    }

    /// Rule 9: `dawn = DawnHour·60`, `dayRange = (DuskHour−DawnHour)·60`,
    /// `time = (MinuteOfDay−dawn)/dayRange`, `xrot = 180·time`
    /// (`SunlightManager.cs:110-115`, `DaggerfallDateTime.cs:48-49`).
    #[test]
    fn sun_orientation_dawn_dusk_and_clamp() {
        assert_eq!((DAWN_HOUR, DUSK_HOUR), (6, 18));
        let dawn = sun_orientation(6 * 60);
        assert_eq!(dawn.dawn_minute, 360.0);
        assert_eq!(dawn.day_range_minutes, 720.0);
        assert_eq!(dawn.time_raw, 0.0);
        assert_eq!(dawn.xrot_degrees, 0.0);
        assert!(!dawn.clamped);

        let noon = sun_orientation(12 * 60);
        assert_eq!(noon.time_raw, 0.5);
        assert_eq!(noon.xrot_degrees, 90.0);

        let last = sun_orientation(18 * 60 - 1);
        assert!(last.time_raw < 1.0);
        assert!((last.xrot_degrees - 179.75).abs() < 1e-4);

        // Out of range: raw goes below 0 / above 180, clamped stays inside.
        let midnight = sun_orientation(0);
        assert_eq!(midnight.time_raw, -0.5);
        assert_eq!(midnight.xrot_raw_degrees, -90.0);
        assert_eq!(midnight.xrot_degrees, 0.0);
        assert!(midnight.clamped);

        let late = sun_orientation(23 * 60 + 59);
        assert!(late.time_raw > 1.0);
        assert_eq!(late.xrot_degrees, 180.0);
        assert!(late.clamped);
        assert!((late.xrot_raw_degrees - 269.749_98).abs() < 1e-2);
    }

    /// Rule 10: underwater fog entry/exit/`fogT` extremes and the declared
    /// density band (`UnderwaterFog.cs:30,45-51,70`).
    #[test]
    fn underwater_fog_entry_exit_and_t() {
        // GV-13 synthetic level of W34-3: blockWaterLevel = -128.
        let fog = UnderwaterFog::from_water_level(-128.0);
        let expected_entry = GLOBAL_SCALE * 128.0 + 0.38;
        assert_eq!(fog.entry_y, expected_entry);
        assert_eq!(fog.exit_y, expected_entry - 0.02);
        assert!((fog.entry_y - 3.58).abs() < 1e-5);
        // Extremes: at the entry plane fogT = 0, at the exit plane fogT = 1.
        assert_eq!(fog.fog_t(fog.entry_y), 0.0);
        assert_eq!(fog.fog_t(fog.exit_y), 1.0);
        // Outside the slab the raw value leaves [0,1] and the clamp holds.
        assert!(fog.fog_t_raw(fog.entry_y + 1.0) < 0.0);
        assert_eq!(fog.fog_t(fog.entry_y + 1.0), 0.0);
        assert!(fog.fog_t_raw(fog.exit_y - 1.0) > 1.0);
        assert_eq!(fog.fog_t(fog.exit_y - 1.0), 1.0);
        // Density band 0…0.25.
        assert_eq!(fog.density(fog.entry_y), 0.0);
        assert_eq!(fog.density(fog.exit_y), UNDERWATER_FOG_DENSITY_MAX);
        // A water level of 0 puts the entry plane at 0.38 m.
        assert_eq!(UnderwaterFog::from_water_level(0.0).entry_y, 0.38);
    }

    /// Rule 11: dungeon light range `2060 cu · 0.025 = 51.5 m`, refresh
    /// `0.4 s` (`DungeonLightHandler.cs:25-26,60`).
    #[test]
    fn dungeon_light_range_and_refresh() {
        assert_eq!(DUNGEON_LIGHT_RANGE_CU, 2060.0);
        assert_eq!(DUNGEON_LIGHT_RANGE_METERS, 51.5);
        assert_eq!(DUNGEON_LIGHT_RANGE_CU * GLOBAL_SCALE, 51.5);
        assert_eq!(DUNGEON_LIGHT_REFRESH_S, 0.4);
        assert!(dungeon_light_reaches(0.0));
        assert!(dungeon_light_reaches(DUNGEON_LIGHT_RANGE_METERS));
        assert!(dungeon_light_reaches(-DUNGEON_LIGHT_RANGE_METERS));
        assert!(!dungeon_light_reaches(DUNGEON_LIGHT_RANGE_METERS + 0.01));
    }
}

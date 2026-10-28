//! M19 — locomotion and player body (REQ-L99-052).
//!
//! **Organ:** `behavior` — row REQ-L99-052 of the 00c assigns locomotion to
//! *BC Behavior (locomotion)*. This module is **pure** (0 Bevy, 0 I/O, 0 clock):
//! it receives `dt` and a [`LocomotionWorld`] (ground height + collision + wall +
//! water) that the caller implements. The demo and the probe use **the same** engine.
//!
//! **Measured spec:** `docs/sintergic/game/programming/m19-locomotion-spec.md`,
//! with `file:line` from `daggerfall-unity@0038a1e61` (MIT). Every constant below
//! carries its citation; none was invented.
//!
//! **Declared boundaries (0 stubs):** M20 supplies live stats/skills
//! ([`PlayerBody`] is a read-only view); fall damage and drowning are
//! emitted as **events** ([`Frame`]) — subtracting HP is M32; levitation
//! duration is M23; world water is M31/M34 (the demo adapter does not have it
//! and declares so).

/// Classic units → m/s (`DFU-UP:Game/Player/PlayerSpeedChanger.cs:30`).
pub const CLASSIC_TO_UNITY_SPEED_UNIT_RATIO: f32 = 39.5;
/// Walking base in classic units (`PlayerSpeedChanger.cs:31`).
pub const DF_WALK_BASE: f32 = 150.0;
/// Crouch base in classic units (`PlayerSpeedChanger.cs:32`).
pub const DF_CROUCH_BASE: f32 = 50.0;

/// Base jump speed, m/s (`AcrobatMotor.cs:10`).
pub const DEFAULT_JUMP_SPEED: f32 = 4.5;
/// Gravity, m/s² (`AcrobatMotor.cs:11`).
pub const DEFAULT_GRAVITY: f32 = 20.0;
/// Jump skill multiplier per point (`AcrobatMotor.cs:13`).
pub const JUMP_SKILL_MULTIPLIER: f32 = 0.5;
/// Athleticism (`AcrobatMotor.cs:14`).
pub const ATHLETICISM_MULTIPLIER: f32 = 0.1;
/// Improved athleticism (`AcrobatMotor.cs:15`).
pub const IMPROVED_ATHLETICISM_MULTIPLIER: f32 = 0.1;
/// Jump spell (`AcrobatMotor.cs:16`).
pub const JUMP_SPELL_MULTIPLIER: f32 = 0.6;
/// Crouching jump (`AcrobatMotor.cs:19`).
pub const CROUCHING_JUMP_DELTA: f32 = 0.8;
/// Horizontal impulse when jumping while moving: `jumpSpeed · 0.05`
/// (`AcrobatMotor.cs:111-115`), in metres.
pub const JUMP_FORWARD_IMPULSE_M: f32 = DEFAULT_JUMP_SPEED * 0.05;
/// Minimum grounded delay to jump, s (`AcrobatMotor.cs:76`).
pub const GROUNDED_JUMP_DELAY_S: f32 = 0.1;
/// Slow fall (`AcrobatMotor.cs:9`).
pub const SLOW_FALL_SPEED: f32 = 105.0;
/// Fall damage threshold, m (`AcrobatMotor.cs:20`, `PlayerHealth.cs:51`).
pub const FALLING_DAMAGE_THRESHOLD_M: f32 = 5.0;
/// HP per metre above the threshold (`PlayerHealth.cs:52`).
pub const FALL_DAMAGE_HP_PER_METRE: f32 = 5.0;
/// Anti-bump against steps (`AcrobatMotor.cs:191`). Documented; not applied
/// because the target world is heightfield + horizontal grid (delta §6).
pub const ANTI_BUMP_FACTOR: f32 = 20.75;

/// Maximum height drop the grounded step absorbs without arming a fall, m.
///
/// `[INFERENCE]` — heightfield analogue of the Unity
/// `CharacterController.stepOffset` (engine default 0.3 m; the DFU prefab does
/// not override it in source), and the accident that moves the terrain out from
/// under the feet. DFU never reads a step offset: it reads
/// `grounded = (collisionFlags & CollisionFlags.Below) != 0`
/// (`PlayerMotor.cs:278`) and, when that flag is gone, arms `CheckInitFall`
/// (`PlayerMotor.cs:350` → `AcrobatMotor.cs:167-177`) instead of snapping. A
/// drop above this limit is therefore a fall, and `CheckFallingDamage`
/// (`AcrobatMotor.cs:207-225`) bills it on landing.
pub const STEP_DOWN_LIMIT_M: f32 = 0.3;

/// Levitation speed, m/s (`LevitateMotor.cs:25`).
pub const LEVITATE_MOVE_SPEED: f32 = 4.0;
/// Diagonal input factor (`PlayerMotor.cs:30`, `FrictionMotor.cs:93`).
/// The literal `0.7071` is the one from the DFU source (4 decimals), **not**
/// an approximation of `1/√2`: it is kept as is.
#[allow(clippy::approx_constant)]
pub const DIAGONAL_FACTOR: f32 = 0.7071;

/// Classic update interval, s (`GameManager.cs:42`).
pub const CLASSIC_UPDATE_INTERVAL_S: f32 = 0.0625;
/// Tally gate of the breath countdown (`PlayerEntity.cs:329`):
/// `if (breathUpdateTally > 18) { --currentBreath; breathUpdateTally = 0; }
/// else ++breathUpdateTally;`. The tally therefore holds 19 without draining
/// and the **20th** classic update decrements — 19 increments plus the
/// decrementing update itself.
pub const BREATH_TALLY_LIMIT: u32 = 18;
/// Classic overencumbrance weight (`LevitateMotor.cs:83`).
pub const OVERENCUMBERED_CLASSIC_LIMIT: f32 = 250.0;

/// System timer divisor (`PlayerMotor.cs:32`).
pub const SYSTEM_TIMER_UPDATES_DIVISOR: f32 = 0.054_925_4;
/// Checks to start climbing (`ClimbingMotor.cs:497-529`).
pub const START_CLIMB_SKILL_CHECK_FREQUENCY: f32 = 14.0;
/// Checks to keep climbing (`ClimbingMotor.cs:72`).
pub const CONTINUE_CLIMBING_SKILL_CHECK_FREQUENCY: f32 = 15.0;
/// Checks to regain hold while slipping (`ClimbingMotor.cs:74`).
pub const REGAIN_HOLD_SKILL_CHECK_FREQUENCY: f32 = 5.0;
/// Minimum % to start (`ClimbingMotor.cs:78`).
pub const START_CLIMB_MIN_CHANCE: i32 = 70;
/// Minimum % to continue (`ClimbingMotor.cs:80`).
pub const CONTINUE_CLIMB_MIN_CHANCE: i32 = 50;
/// Minimum % to regain hold (`ClimbingMotor.cs:76`).
pub const REGAIN_HOLD_MIN_CHANCE: i32 = 20;
/// Minimum % to grasp the wall while falling (`ClimbingMotor.cs:82`).
pub const GRASP_WALL_MIN_CHANCE: i32 = 40;

/// Classic scale → metres (`MeshReader.cs:58`).
pub const GLOBAL_SCALE: f32 = 0.025;

/// Read-only view of the live stats/skills the engine consumes (M20).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerBody {
    pub live_speed: i32,
    pub live_endurance: i32,
    pub live_luck: i32,
    pub live_strength: i32,
    pub running_skill: i32,
    pub swimming_skill: i32,
    pub climbing_skill: i32,
    pub jumping_skill: i32,
    pub stealth_skill: i32,
    pub pickpocket_skill: i32,
    pub lockpicking_skill: i32,
    pub khajiit: bool,
    pub argonian: bool,
    pub athleticism: bool,
    pub improved_athleticism: bool,
    pub enhanced_jumping: bool,
    pub enhanced_climbing: bool,
    pub slow_falling: bool,
    pub water_breathing: bool,
    /// `PlayerEntity.IsWaterWalking` (`WaterWalking.cs:98`): swimming keeps the
    /// normal walking speed and does not sink under encumbrance
    /// (`LevitateMotor.cs:83-85,116-121`, `PlayerMotor.cs:385-388`).
    pub water_walking: bool,
    /// Carried weight (kg) — `PlayerEntity.CarriedWeight`
    /// (`DFU-UP:Game/Entities/PlayerEntity.cs:184`).
    pub carried_weight_kg: f32,
    /// Player level (lockpicking/pickpocketing) — M20.
    pub level: i32,
}

impl Default for PlayerBody {
    /// Target's "base human" body: LiveSpeed 50, Endurance 50, Luck 50,
    /// Strength 50, skills 0, no effects. Same defaults the target's earlier
    /// probes used for the player.
    fn default() -> Self {
        Self {
            live_speed: 50,
            live_endurance: 50,
            live_luck: 50,
            live_strength: 50,
            running_skill: 0,
            swimming_skill: 0,
            climbing_skill: 0,
            jumping_skill: 0,
            stealth_skill: 0,
            pickpocket_skill: 0,
            lockpicking_skill: 0,
            khajiit: false,
            argonian: false,
            athleticism: false,
            improved_athleticism: false,
            enhanced_jumping: false,
            enhanced_climbing: false,
            slow_falling: false,
            water_breathing: false,
            water_walking: false,
            carried_weight_kg: 0.0,
            level: 1,
        }
    }
}

// ── §1 speeds ───────────────────────────────────────────────────────────

/// Walking (`PlayerSpeedChanger.cs:389-393`).
pub fn walk_speed(live_speed: i32) -> f32 {
    let speed = live_speed as f32;
    let drag = 0.5 * (100.0 - speed.max(30.0));
    (speed + DF_WALK_BASE - drag) / CLASSIC_TO_UNITY_SPEED_UNIT_RATIO
}

/// Crouching (`PlayerSpeedChanger.cs:154-155`).
pub fn crouch_speed(live_speed: i32) -> f32 {
    (live_speed as f32 + DF_CROUCH_BASE) / CLASSIC_TO_UNITY_SPEED_UNIT_RATIO
}

/// Running (`PlayerSpeedChanger.cs:400-405`).
pub fn run_speed(live_speed: i32, running_skill: i32) -> f32 {
    let base = (live_speed as f32 + DF_WALK_BASE) / CLASSIC_TO_UNITY_SPEED_UNIT_RATIO;
    base * (1.35 + running_skill as f32 / 200.0)
}

/// Sneak: half minus one classic unit (`PlayerSpeedChanger.cs:127-132`).
pub fn sneak_speed(speed: f32) -> f32 {
    speed / 2.0 - 1.0 / CLASSIC_TO_UNITY_SPEED_UNIT_RATIO
}

/// Swimming (`PlayerSpeedChanger.cs:412-416`).
pub fn swim_speed(base: f32, swimming_skill: i32) -> f32 {
    base * (swimming_skill as f32 / 200.0) + base / 4.0
}

/// Climbing (`PlayerSpeedChanger.cs:418-423`).
pub fn climb_speed(base: f32, enhanced_climbing: bool) -> f32 {
    let boost = if enhanced_climbing { 2.0 } else { 1.0 };
    base / 3.0 * boost
}

/// Levitation (`LevitateMotor.cs:25`).
pub fn levitate_speed() -> f32 {
    LEVITATE_MOVE_SPEED
}

/// Resolved frame speed, in source order:
/// base (crouch/walk) → running replaces → sneaking divides → swimming transforms
/// (`PlayerSpeedChanger.cs:104-166`, `PlayerMotor.cs:383-389`).
pub fn resolve_speed(
    body: &PlayerBody,
    crouching: bool,
    running: bool,
    sneaking: bool,
    swimming: bool,
) -> f32 {
    let base = if crouching && !swimming {
        crouch_speed(body.live_speed)
    } else {
        walk_speed(body.live_speed)
    };
    let mut speed = if running {
        run_speed(body.live_speed, body.running_skill)
    } else {
        base
    };
    if !running && sneaking {
        speed = sneak_speed(speed);
    }
    if swimming {
        speed = swim_speed(speed, body.swimming_skill);
    }
    speed
}

// ── §2 jump / fall ──────────────────────────────────────────────────────

/// Jump strength multiplier (`AcrobatMotor.cs:81-118`).
pub fn jump_multiplier(body: &PlayerBody) -> f32 {
    let mut multiplier = 1.0 + body.jumping_skill as f32 * JUMP_SKILL_MULTIPLIER / 100.0;
    if body.athleticism {
        multiplier += ATHLETICISM_MULTIPLIER;
        if body.improved_athleticism {
            multiplier += IMPROVED_ATHLETICISM_MULTIPLIER;
        }
    }
    if body.enhanced_jumping {
        multiplier += JUMP_SPELL_MULTIPLIER;
    }
    multiplier
}

/// Initial vertical jump speed, m/s.
pub fn jump_velocity(body: &PlayerBody, crouching: bool) -> f32 {
    let mut velocity = DEFAULT_JUMP_SPEED * jump_multiplier(body);
    if crouching {
        velocity *= CROUCHING_JUMP_DELTA;
    }
    velocity
}

/// Theoretical jump apex (m) — `v²/2g`.
pub fn jump_apex(velocity: f32) -> f32 {
    velocity * velocity / (2.0 * DEFAULT_GRAVITY)
}

/// Outcome of a fall (`AcrobatMotor.cs:208-231`, `PlayerHealth.cs:49-59`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallOutcome {
    /// No consequence.
    None,
    /// Fell more than 2.5 m but less than 5 m: alert, 0 damage.
    HardFall,
    /// Integer damage by `(int)(5·(d−5))` — C# truncates toward zero.
    Damage(i32),
}

/// Classifies a fall of `fall_distance_m` metres.
pub fn fall_outcome(fall_distance_m: f32) -> FallOutcome {
    if fall_distance_m > FALLING_DAMAGE_THRESHOLD_M {
        FallOutcome::Damage(
            (FALL_DAMAGE_HP_PER_METRE * (fall_distance_m - FALLING_DAMAGE_THRESHOLD_M)) as i32,
        )
    } else if fall_distance_m > FALLING_DAMAGE_THRESHOLD_M / 2.0 {
        FallOutcome::HardFall
    } else {
        FallOutcome::None
    }
}

impl FallOutcome {
    /// Damage of the outcome (0 if no damage).
    pub fn as_damage(self) -> i32 {
        match self {
            Self::Damage(damage) => damage,
            _ => 0,
        }
    }
}

// ── §3 climbing ─────────────────────────────────────────────────────────

/// Check whose frequency is queried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClimbCheckKind {
    Start,
    Continue,
    Slipping,
}

/// Interval between checks, s (`ClimbingMotor.cs:72-74`, `PlayerMotor.cs:32`).
pub fn climb_check_interval_s(kind: ClimbCheckKind) -> f32 {
    let frequency = match kind {
        ClimbCheckKind::Start => START_CLIMB_SKILL_CHECK_FREQUENCY,
        ClimbCheckKind::Continue => CONTINUE_CLIMBING_SKILL_CHECK_FREQUENCY,
        ClimbCheckKind::Slipping => REGAIN_HOLD_SKILL_CHECK_FREQUENCY,
    };
    frequency * SYSTEM_TIMER_UPDATES_DIVISOR
}

fn clamp_percent(value: i32) -> i32 {
    value.clamp(5, 95)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

/// Climb % (`FormulaHelper.cs:293-316`). Does not cap at 100 — neither does the
/// source (with effective skill 95 and luck 100 it returns 103).
pub fn climb_chance(body: &PlayerBody, base_percent_success: i32) -> i32 {
    let mut skill = body.climbing_skill;
    if body.khajiit {
        skill += 30;
    }
    if body.enhanced_climbing {
        skill *= 2;
    }
    let skill = clamp_percent(skill);
    let luck_factor = lerp(0.0, 10.0, body.live_luck as f32 * 0.01);
    (lerp(base_percent_success as f32, 100.0, skill as f32 * 0.01) + luck_factor) as i32
}

// ── §4 breath / weight ──────────────────────────────────────────────────

/// Maximum breath (`DaggerfallEntity.cs:267`).
pub fn max_breath(live_endurance: i32) -> i32 {
    (live_endurance / 2).max(0)
}

/// Breath drain interval (`PlayerEntity.cs:329-337`, `GameManager.cs:42`):
/// 19 tally increments **plus** the update that resets the tally and decrements
/// the breath = 20 classic updates = `20 · 0.0625 = 1.25 s`.
pub fn breath_drain_interval_s() -> f32 {
    (BREATH_TALLY_LIMIT + 2) as f32 * CLASSIC_UPDATE_INTERVAL_S
}

/// Maximum carry weight (`FormulaHelper.cs:75-83`).
pub fn max_encumbrance(live_strength: i32) -> i32 {
    (live_strength as f32 * 1.5).floor() as i32
}

/// Classic weight from kg (`LevitateMotor.cs:83`).
pub fn classic_weight(kg: f32) -> f32 {
    kg * 4.0
}

/// Overencumbrance that prevents floating while swimming (`LevitateMotor.cs:83`).
pub fn is_overencumbered(carried_weight_kg: f32) -> bool {
    classic_weight(carried_weight_kg) > OVERENCUMBERED_CLASSIC_LIMIT
}

/// Does the load fit? — literal comparison from the source
/// (`DaggerfallTradeWindow.cs:1039`: kg against `MaxEncumbrance`).
pub fn can_carry(body: &PlayerBody, added_kg: f32) -> bool {
    body.carried_weight_kg + added_kg <= max_encumbrance(body.live_strength) as f32
}

// ── §5 stealth / thievery ───────────────────────────────────────────────

/// Stealth (`FormulaHelper.cs:282-290`).
pub fn stealth_chance(distance_m: f32, target_stealth_skill: i32) -> i32 {
    let classic_distance = (distance_m / GLOBAL_SCALE) as i32;
    2 * ((classic_distance * target_stealth_skill) >> 10)
}

/// Interior lockpicking (`FormulaHelper.cs:232-241`).
pub fn interior_lockpicking_chance(level: i32, lock_value: i32, skill: i32) -> i32 {
    clamp_percent(5 * (level - lock_value) + skill)
}

/// Exterior lockpicking (`FormulaHelper.cs:243-251`).
pub fn exterior_lockpicking_chance(lock_value: i32, skill: i32) -> i32 {
    clamp_percent(skill - 5 * lock_value)
}

/// Pickpocketing (`FormulaHelper.cs:254-267`).
pub fn pickpocketing_chance(skill: i32, player_level: i32, target_level: i32) -> i32 {
    clamp_percent(skill + 5 * (player_level - target_level))
}

/// Shoplifting (`FormulaHelper.cs:270-279`).
pub fn shoplifting_chance(skill: i32, shop_quality: i32, weight_and_num_items: i32) -> i32 {
    clamp_percent(100 - skill + shop_quality + weight_and_num_items)
}

// ── §6 deterministic die ────────────────────────────────────────────────

/// Deterministic `Dice100` with the SAME inequality as the source
/// (`Dice100.cs:14-24`): `success(c) ⇔ roll ∈ [0,99] < c`.
#[derive(Debug, Clone)]
pub struct Dice100 {
    state: u64,
}

impl Dice100 {
    /// Explicit seed. `0` is replaced by 1 (SplitMix64 needs state).
    pub fn new(seed: u64) -> Self {
        Self {
            state: seed.max(1),
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// `Random.Range(0, 100)`.
    pub fn roll(&mut self) -> i32 {
        (self.next_u64() % 100) as i32
    }

    /// `Dice100.SuccessRoll`.
    pub fn success(&mut self, chance: i32) -> bool {
        self.roll() < chance
    }
}

// ── engine ──────────────────────────────────────────────────────────────

/// Engine state (`PlayerMotor` + `LevitateMotor` + `ClimbingMotor`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MotionMode {
    #[default]
    Grounded,
    Airborne,
    Climbing,
    Swimming,
    Levitating,
}

impl MotionMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Grounded => "grounded",
            Self::Airborne => "airborne",
            Self::Climbing => "climbing",
            Self::Swimming => "swimming",
            Self::Levitating => "levitating",
        }
    }
}

/// Outcome of a horizontal step against the world (implemented by the grid).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StepOutcome {
    pub pos: [f32; 3],
    pub moved: bool,
    pub blocked: bool,
}

/// World the engine consumes. The demo implements it over the `.RMB` grid and the
/// M13 sampler; the probe, over deterministic synthetic plane/water.
pub trait LocomotionWorld {
    /// Ground height (m) at `(x, z)`.
    fn ground_height(&self, x: f32, z: f32) -> f32;
    /// Horizontal step with collision (authority INV-L99-5).
    fn step(&self, from: [f32; 3], delta: [f32; 3], radius: f32) -> StepOutcome;
    /// Is there a wall ahead of the eye within `reach` m?
    fn wall_ahead(&self, eye: [f32; 3], dir: [f32; 3], reach: f32) -> bool;
    /// Is the eye underwater? (by default: no water — demo M19).
    fn submerged(&self, _eye: [f32; 3]) -> bool {
        false
    }
}

/// Frame input (the `app` action layer fills it; 0 winit here).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MoveInput {
    /// Forward (+) / back (−), −1..1.
    pub forward: f32,
    /// Right (+) / left (−), −1..1.
    pub strafe: f32,
    /// Absolute yaw (rad). Target convention: yaw 0 → −Z.
    pub yaw: f32,
    /// Jump (or float up while swimming/levitating).
    pub jump: bool,
    /// Run.
    pub run: bool,
    /// Crouch (or descend while swimming/levitating).
    pub crouch: bool,
    /// Sneak (toggle resolved by the caller).
    pub sneak: bool,
    /// Active levitation effect (applied by M23).
    pub levitating: bool,
}

/// Events and measurements of a step. The events are consumed by M32 (health) and the HUD.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Frame {
    pub mode: MotionMode,
    /// Resolved horizontal frame speed (m/s).
    pub speed: f32,
    pub moved: bool,
    pub blocked: bool,
    /// Frame fall damage (0 = none) — event for M32.
    pub fall_damage: i32,
    /// Hard fall without damage (`AcrobatMotor.cs:221-223`).
    pub hard_fall: bool,
    /// Breath exhausted — event for M32 (the source sets health to 0).
    pub drowned: bool,
    /// The engine is gripping a wall.
    pub climbing: bool,
    /// A climbing die was consumed in this step — the flag is raised once per
    /// executed skill check, failed checks included
    /// (`ClimbingMotor.ClimbingSkillCheck`, `ClimbingMotor.cs:823`). Consumed
    /// by the M20 progression driver to tally the Climbing skill.
    pub climb_check: bool,
    /// A **jump started** on this step: the frame the jump input is consumed
    /// and `AcrobatMotor.jumping` is set (`AcrobatMotor.cs:109`), exactly once
    /// per started jump. Falls, ledge drops, levitation exits and climbing
    /// aborts never raise it — the grounded-time guard returns **before** the
    /// input is read (`AcrobatMotor.cs:76-77`), so `PlayerEntity`'s
    /// `!CheckedCurrentJump && IsJumping` edge (`PlayerEntity.cs:425-438`)
    /// never fires for them.
    pub jumped: bool,
    /// Resolved `PlayerSpeedChanger.isRunning` (`PlayerSpeedChanger.cs:106-118`):
    /// the run mode latched **while grounded** and kept untouched in the air
    /// ("you can't switch running on/off while in mid air"). It carries no
    /// movement test (`CanRun()` is `CanRunUnlessRiding()`, `:137-140`), so
    /// holding Run standing still still reports `true`.
    pub running: bool,
}

/// Complete locomotion + body state.
#[derive(Debug, Clone)]
pub struct Locomotion {
    /// **Eye** position (`[x, y, z]`, m).
    pub position: [f32; 3],
    /// Eye height above the ground (m).
    pub eye_height: f32,
    /// Collision radius (m).
    pub radius: f32,
    pub mode: MotionMode,
    pub vertical_velocity: f32,
    /// Horizontal momentum (m/s) — persists in the air (`PlayerMotor.cs:44`).
    pub horizontal_velocity: [f32; 2],
    pub grounded_time: f32,
    pub falling: bool,
    pub fall_start_y: f32,
    pub slipping: bool,
    /// Timer of the ongoing climb check (start/continue/regain).
    pub climb_timer: f32,
    pub breath: i32,
    pub breath_tally: f32,
    /// DFU air control (`AcrobatMotor.cs:21`, `false` by default).
    pub air_control: bool,
    /// Latched run mode — the resolved `PlayerSpeedChanger.isRunning`
    /// (`PlayerSpeedChanger.cs:106-118`): `runningMode && CanRun()` is sampled
    /// **only while grounded** and the previous value is kept in the air, so
    /// releasing or pressing Run mid-air only takes effect on the ground — and
    /// from the **next** step, because the resolution happens at step entry
    /// with the mode the body ended the previous step in.
    ///
    /// `CanRun()` in the pinned source is `CanRunUnlessRiding()`
    /// (`:137-140`): the `!IsRiding` conjunct is vacuous here because the demo
    /// has no mounts, so the latch is `input.run`. No movement and no crouch
    /// test, by source order.
    pub running_mode: bool,
}

impl Locomotion {
    pub fn new(position: [f32; 3], eye_height: f32, radius: f32) -> Self {
        Self {
            position,
            eye_height,
            radius,
            mode: MotionMode::Grounded,
            vertical_velocity: 0.0,
            horizontal_velocity: [0.0, 0.0],
            grounded_time: 0.0,
            falling: false,
            fall_start_y: position[1],
            slipping: false,
            climb_timer: 0.0,
            breath: 0,
            breath_tally: 0.0,
            air_control: false,
            running_mode: false,
        }
    }

    /// Horizontal input direction in m/s.
    fn input_velocity(&self, input: &MoveInput, speed: f32) -> [f32; 2] {
        let (mut forward, mut strafe) = (
            input.forward.clamp(-1.0, 1.0),
            input.strafe.clamp(-1.0, 1.0),
        );
        if forward != 0.0 && strafe != 0.0 {
            forward *= DIAGONAL_FACTOR;
            strafe *= DIAGONAL_FACTOR;
        }
        let (sin, cos) = input.yaw.sin_cos();
        let forward_dir = [-sin, -cos];
        let right_dir = [cos, -sin];
        [
            (forward_dir[0] * forward + right_dir[0] * strafe) * speed,
            (forward_dir[1] * forward + right_dir[1] * strafe) * speed,
        ]
    }

    fn forward_dir(&self, yaw: f32) -> [f32; 3] {
        let (sin, cos) = yaw.sin_cos();
        [-sin, 0.0, -cos]
    }

    /// One engine step. `dt` in seconds.
    pub fn step(
        &mut self,
        input: &MoveInput,
        body: &PlayerBody,
        world: &dyn LocomotionWorld,
        dice: &mut Dice100,
        dt: f32,
    ) -> Frame {
        let mut frame = Frame {
            mode: self.mode,
            ..Default::default()
        };
        if dt <= 0.0 {
            return frame;
        }
        let submerged = world.submerged(self.position);
        let water_mode = input.levitating || (submerged && self.mode != MotionMode::Climbing);

        // `PlayerEntity.cs:323-343`: the **single** copy of the breath block.
        // Its only gate is `IsPlayerSubmerged && !IsWaterBreathing` — the motor
        // is not part of the condition, so a submerged body drains (and drowns)
        // while swimming, levitating, climbing or falling alike. It runs before
        // the water engine takes over and before any early return.
        if submerged && !body.water_breathing {
            // `:325-328`: an exhausted breath refills **before** this update's
            // decrement. `DeepBreath` only folds guild memberships over the
            // value (`GuildManager.cs:385-391`), so without memberships it is
            // the identity and the refill is `MaxBreath`
            // (`DaggerfallEntity.cs:267`).
            if self.breath == 0 {
                self.breath = max_breath(body.live_endurance);
            }
            self.breath_tally += dt;
            let interval = breath_drain_interval_s();
            while self.breath_tally >= interval {
                self.breath_tally -= interval;
                // `:330-331`: Argonian 50 % chance of not consuming.
                let skip = body.argonian && dice.roll() % 2 == 1;
                if !skip {
                    self.breath -= 1;
                }
            }
            // `:339-340`: `SetHealth(0)` only when the breath is still ≤ 0
            // after the refill — always the case while `MaxBreath` is 0, never
            // a second time with a positive maximum. There is no clamp: the
            // source does not write the field back.
            if self.breath <= 0 {
                frame.drowned = true;
            }
        } else {
            // `:342-343`: `else currentBreath = 0;` — covers the dry body, the
            // water-breathing body and the moment the immersion ends.
            self.breath = 0;
            self.breath_tally = 0.0;
        }

        // `PlayerSpeedChanger.ApplyInputSpeedAdjustment` (`PlayerSpeedChanger.cs:106-118`):
        // the run mode is resolved **only while grounded** (`isRunning =
        // CanRun() && runningMode`); anywhere else the previous resolution is
        // kept — "you can't switch running on/off while in mid air". The state
        // sampled is the mode the body ended the previous step in (DFU reads
        // `playerMotor.IsGrounded`, i.e. the motor's last grounded check).
        if self.mode == MotionMode::Grounded && !water_mode {
            self.running_mode = input.run;
        }
        frame.running = self.running_mode;

        // DFU PlayerMotor.cs:323 — levitating/swimming take over the engine and
        // cancel accumulated velocity.
        if water_mode {
            self.falling = false;
            self.slipping = false;
            self.vertical_velocity = 0.0;
            self.horizontal_velocity = [0.0, 0.0];
            self.climb_timer = 0.0;
            self.mode = if input.levitating {
                MotionMode::Levitating
            } else {
                MotionMode::Swimming
            };
            self.step_water(input, body, world, dt, &mut frame);
            return frame;
        }

        // Leaving the water or stopping levitation → falls (`LevitateMotor.cs:168-187`).
        if matches!(self.mode, MotionMode::Swimming | MotionMode::Levitating) {
            self.mode = MotionMode::Airborne;
            self.falling = true;
            self.fall_start_y = self.position[1];
            self.vertical_velocity = 0.0;
        }

        if self.mode == MotionMode::Climbing {
            self.step_climbing(input, body, world, dice, dt, &mut frame);
            frame.mode = self.mode;
            return frame;
        }

        // Climb attempt (`ClimbingMotor.cs:307-462`).
        let wall = world.wall_ahead(
            self.position,
            self.forward_dir(input.yaw),
            self.radius + 0.1,
        );
        if input.forward > 0.0 && wall && !input.crouch && !input.jump {
            self.climb_timer += dt;
            if self.climb_timer >= climb_check_interval_s(ClimbCheckKind::Start) {
                self.climb_timer = 0.0;
                // The die is consumed ⇒ the check ran (`ClimbingMotor.cs:823`).
                frame.climb_check = true;
                let base_chance = if self.mode == MotionMode::Airborne {
                    GRASP_WALL_MIN_CHANCE
                } else {
                    START_CLIMB_MIN_CHANCE
                };
                if dice.success(climb_chance(body, base_chance)) {
                    self.mode = MotionMode::Climbing;
                    self.slipping = false;
                    self.falling = false;
                    self.vertical_velocity = 0.0;
                    self.horizontal_velocity = [0.0, 0.0];
                    frame.climbing = true;
                    frame.mode = self.mode;
                    self.step_climbing(input, body, world, dice, dt, &mut frame);
                    frame.mode = self.mode;
                    return frame;
                }
            }
        } else {
            self.climb_timer = 0.0;
        }

        match self.mode {
            MotionMode::Grounded => self.step_grounded(input, body, world, dt, &mut frame),
            MotionMode::Airborne => self.step_airborne(body, world, dt, &mut frame),
            _ => {}
        }
        frame.mode = self.mode;
        frame.climbing = self.mode == MotionMode::Climbing;
        frame
    }

    fn step_grounded(
        &mut self,
        input: &MoveInput,
        body: &PlayerBody,
        world: &dyn LocomotionWorld,
        dt: f32,
        frame: &mut Frame,
    ) {
        self.grounded_time += dt;
        // The speed path consumes the **resolved** run mode, not the raw key
        // (`PlayerSpeedChanger.cs:120-123`: `if (isRunning) RefreshRunSpeed()`).
        let speed = resolve_speed(body, input.crouch, self.running_mode, input.sneak, false);
        frame.speed = speed;
        let velocity = self.input_velocity(input, speed);
        self.horizontal_velocity = velocity;
        let outcome = world.step(
            self.position,
            [velocity[0] * dt, 0.0, velocity[1] * dt],
            self.radius,
        );
        self.position[0] = outcome.pos[0];
        self.position[2] = outcome.pos[2];
        frame.moved = outcome.moved;
        frame.blocked = outcome.blocked;

        let ground_eye = world.ground_height(self.position[0], self.position[2]) + self.eye_height;
        if input.jump && self.grounded_time >= GROUNDED_JUMP_DELAY_S {
            // Only here is `AcrobatMotor.jumping` raised (`AcrobatMotor.cs:109`):
            // the guard above is the source's `GroundedTime < 0.1f` early
            // return (`:77-78`), so this edge is one started jump, never a fall.
            frame.jumped = true;
            self.vertical_velocity = jump_velocity(body, input.crouch);
            self.falling = true;
            self.fall_start_y = self.position[1];
            self.mode = MotionMode::Airborne;
            self.grounded_time = 0.0;
            if velocity != [0.0, 0.0] {
                // `AcrobatMotor.cs:111-115`: one-shot horizontal impulse.
                let forward = self.forward_dir(input.yaw);
                let boosted = world.step(
                    self.position,
                    [
                        forward[0] * JUMP_FORWARD_IMPULSE_M,
                        0.0,
                        forward[2] * JUMP_FORWARD_IMPULSE_M,
                    ],
                    self.radius,
                );
                self.position[0] = boosted.pos[0];
                self.position[2] = boosted.pos[2];
            }
        } else {
            let drop = self.position[1] - ground_eye;
            if drop > STEP_DOWN_LIMIT_M {
                // The ground is gone from under the feet: `CollisionFlags.Below`
                // stops being set, so `PlayerMotor.cs:350` calls
                // `acrobatMotor.CheckInitFall` (`AcrobatMotor.cs:167-177`), which
                // arms `falling` with the level of this moment. The landing bills
                // `fallStartLevel - y` (`:215-223`).
                self.mode = MotionMode::Airborne;
                self.falling = true;
                self.fall_start_y = self.position[1];
                self.vertical_velocity = 0.0;
                self.grounded_time = 0.0;
            } else {
                // Heightfield: the eye follows the ground (step-up/down without anti-bump).
                self.position[1] = ground_eye;
                self.vertical_velocity = 0.0;
            }
        }
    }

    fn step_airborne(
        &mut self,
        body: &PlayerBody,
        world: &dyn LocomotionWorld,
        dt: f32,
        frame: &mut Frame,
    ) {
        let velocity = self.horizontal_velocity;
        let horizontal = world.step(
            self.position,
            [velocity[0] * dt, 0.0, velocity[1] * dt],
            self.radius,
        );
        self.position[0] = horizontal.pos[0];
        self.position[2] = horizontal.pos[2];
        frame.moved = horizontal.moved;
        frame.blocked = horizontal.blocked;
        frame.speed = (velocity[0] * velocity[0] + velocity[1] * velocity[1]).sqrt();

        // `AcrobatMotor.cs:183-190`: slow fall assigns the frame speed
        // (the source multiplies by deltaTime twice).
        if self.falling && body.slow_falling {
            self.vertical_velocity = -SLOW_FALL_SPEED * dt;
        } else {
            self.vertical_velocity -= DEFAULT_GRAVITY * dt;
        }
        self.position[1] += self.vertical_velocity * dt;

        // `AcrobatMotor.cs:184-187`: while slow-falling `fallStartLevel` follows
        // the player every frame, so only the current descent is accumulated
        // and a slow fall never reaches the damage threshold.
        if self.falling && body.slow_falling {
            self.fall_start_y = self.position[1];
        }

        let ground_eye = world.ground_height(self.position[0], self.position[2]) + self.eye_height;
        if self.position[1] <= ground_eye {
            // A landed fall is billed unless the body is a non-slipping climb
            // (`AcrobatMotor.cs:217`); airborne never is.
            self.land_on_ground(ground_eye, true, frame);
            self.horizontal_velocity = [0.0, 0.0];
        }
    }

    /// Landing hand-off shared by [`Self::step_airborne`] and
    /// [`Self::step_climbing`] — the single copy of `PlayerMotor.cs:337-341`
    /// (`if (grounded) { Jumping = false; acrobatMotor.CheckFallingDamage(); … }`).
    ///
    /// `CheckFallingDamage` (`AcrobatMotor.cs:207-225`) clears `falling` and
    /// bills `fallStartLevel - position.y` only through the gate
    /// `!climbingMotor.IsClimbing || climbingMotor.IsSlipping` (`:217`) — a
    /// climb that reaches the ground while holding on takes nothing, a slip
    /// does. Callers pass that gate as `bill_fall`; `slipping` is cleared by
    /// the climb caller, not here.
    fn land_on_ground(&mut self, ground_eye: f32, bill_fall: bool, frame: &mut Frame) {
        if self.falling && bill_fall {
            match fall_outcome(self.fall_start_y - ground_eye) {
                FallOutcome::Damage(damage) => frame.fall_damage = damage,
                FallOutcome::HardFall => frame.hard_fall = true,
                FallOutcome::None => {}
            }
        }
        self.position[1] = ground_eye;
        self.vertical_velocity = 0.0;
        self.falling = false;
        self.mode = MotionMode::Grounded;
        self.grounded_time = 0.0;
    }

    fn step_water(
        &mut self,
        input: &MoveInput,
        body: &PlayerBody,
        world: &dyn LocomotionWorld,
        dt: f32,
        frame: &mut Frame,
    ) {
        let levitating = self.mode == MotionMode::Levitating;
        let speed = if levitating {
            levitate_speed()
        } else if body.water_walking {
            // `LevitateMotor.cs:116-121`: swimming with `IsWaterWalking` moves at
            // `PlayerMotor.Speed`, i.e. the **resolved normal** speed;
            // `PlayerMotor.cs:385-388` only applies `GetSwimSpeed` when the flag
            // is off. So the swim transform is skipped, run/sneak/crouch stay.
            resolve_speed(body, input.crouch, self.running_mode, input.sneak, false)
        } else {
            swim_speed(walk_speed(body.live_speed), body.swimming_skill)
        };
        frame.speed = speed;
        let velocity = self.input_velocity(input, speed);
        // DFU LevitateMotor.cs:113-114: swimming without levitating has no vertical
        // from horizontal movement (it is already 0); the step uses the same grid.
        let horizontal = world.step(
            self.position,
            [velocity[0] * dt, 0.0, velocity[1] * dt],
            self.radius,
        );
        frame.moved = horizontal.moved;
        frame.blocked = horizontal.blocked;
        self.position[0] = horizontal.pos[0];
        self.position[2] = horizontal.pos[2];

        let vertical_speed = if levitating {
            levitate_speed()
        } else {
            speed
        };
        let overencumbered = is_overencumbered(body.carried_weight_kg);
        let dy = if levitating {
            if input.jump {
                vertical_speed
            } else if input.crouch {
                -vertical_speed
            } else {
                0.0
            }
        } else if overencumbered && !body.water_walking {
            // `LevitateMotor.cs:83-85`: overencumbered it sinks and cannot rise —
            // the sink is skipped with `IsWaterWalking` (`&& !...IsWaterWalking`).
            -vertical_speed
        } else if input.jump {
            vertical_speed
        } else if input.crouch {
            -vertical_speed
        } else {
            0.0
        };
        let mut new_y = self.position[1] + dy * dt;
        if !levitating && dy > 0.0 {
            // `LevitateMotor.cs:125-129`: you cannot leave the water by swimming.
            let candidate = [self.position[0], new_y, self.position[2]];
            if !world.submerged(candidate) {
                new_y = self.position[1];
            }
        }
        self.position[1] = new_y;

        // The breath block is **not** here: `PlayerEntity.cs:323-343` gates it on
        // the immersion alone, not on the motor, so it lives once in
        // [`Self::step`] before the water engine takes over.
    }

    fn step_climbing(
        &mut self,
        input: &MoveInput,
        body: &PlayerBody,
        world: &dyn LocomotionWorld,
        dice: &mut Dice100,
        dt: f32,
        frame: &mut Frame,
    ) {
        let wall = world.wall_ahead(
            self.position,
            self.forward_dir(input.yaw),
            self.radius + 0.1,
        );
        let abort = input.crouch || input.jump;
        if abort || !wall {
            // `ClimbingMotor.cs:324-330,399-420`: jumping/crouching or losing the
            // wall ends the climb; the fall starts here. Same `CheckInitFall`
            // rule as the slip (`AcrobatMotor.cs:167-177`): the start level is
            // armed only when the body is not already falling, so an abort in
            // the middle of a slip keeps the slip's start level.
            self.mode = MotionMode::Airborne;
            if !self.falling {
                self.falling = true;
                self.fall_start_y = self.position[1];
            }
            self.vertical_velocity = 0.0;
            self.slipping = false;
            self.climb_timer = 0.0;
            frame.climbing = false;
            return;
        }

        let base = resolve_speed(body, false, false, false, false);
        let climb = climb_speed(base, body.enhanced_climbing);
        frame.speed = if self.slipping { 0.0 } else { climb };
        frame.climbing = true;

        // `ClimbingMotor.cs:449-452`: without movement input there is no slip.
        if input.forward == 0.0 && input.strafe == 0.0 {
            self.slipping = false;
        }

        if self.slipping {
            self.vertical_velocity -= DEFAULT_GRAVITY * dt;
            self.position[1] += self.vertical_velocity * dt;
        } else {
            // `ClimbingMotor.cs:684-691`: forward goes up, back goes down.
            let dy = if input.forward < 0.0 { -climb } else { climb };
            self.position[1] += dy * dt;
            self.vertical_velocity = 0.0;
        }

        self.climb_timer += dt;
        let kind = if self.slipping {
            ClimbCheckKind::Slipping
        } else {
            ClimbCheckKind::Continue
        };
        if self.climb_timer >= climb_check_interval_s(kind) {
            self.climb_timer = 0.0;
            // Continue/regain check ran (`ClimbingMotor.cs:823`).
            frame.climb_check = true;
            if self.slipping {
                if dice.success(climb_chance(body, REGAIN_HOLD_MIN_CHANCE)) {
                    self.slipping = false;
                    self.vertical_velocity = 0.0;
                }
            } else if !dice.success(climb_chance(body, CONTINUE_CLIMB_MIN_CHANCE)) {
                self.slipping = true;
                self.vertical_velocity = 0.0;
                // `ClimbingMotor.cs:760-764`: the slipping branch calls
                // `acrobatMotor.CheckInitFall` + `ApplyGravity`, so a failed
                // continue check starts a real fall. `CheckInitFall`
                // (`AcrobatMotor.cs:167-177`) arms it only when the body is not
                // already falling and takes `fallStartLevel` from the position
                // of that moment; `CheckFallingDamage` later bills exactly that
                // descent (`AcrobatMotor.cs:215-223`).
                if !self.falling {
                    self.falling = true;
                    self.fall_start_y = self.position[1];
                }
            }
        }

        let ground_eye = world.ground_height(self.position[0], self.position[2]) + self.eye_height;
        if self.position[1] <= ground_eye {
            // `AcrobatMotor.cs:217`: reaching the ground while **climbing**
            // takes no damage — only `IsSlipping` does, and that is the fall
            // armed above.
            let slipping = self.slipping;
            self.land_on_ground(ground_eye, slipping, frame);
            self.slipping = false;
            frame.climbing = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic synthetic world: flat ground, optional wall and optional water.
    struct FlatWorld {
        ground: f32,
        /// Heightfield ledge: `ledge_height` on the `x < 0` half.
        ledge_height: Option<f32>,
        wall_x: Option<f32>,
        water_y: Option<f32>,
        solids: Vec<([f32; 3], f32)>,
    }

    impl FlatWorld {
        fn flat() -> Self {
            Self {
                ground: 0.0,
                ledge_height: None,
                wall_x: None,
                water_y: None,
                solids: Vec::new(),
            }
        }
    }

    impl LocomotionWorld for FlatWorld {
        fn ground_height(&self, x: f32, _z: f32) -> f32 {
            match self.ledge_height {
                Some(height) if x < 0.0 => height,
                _ => self.ground,
            }
        }
        fn step(&self, from: [f32; 3], delta: [f32; 3], radius: f32) -> StepOutcome {
            let target = [from[0] + delta[0], from[1] + delta[1], from[2] + delta[2]];
            if let Some(wall) = self.wall_x {
                if target[0] + radius > wall {
                    return StepOutcome {
                        pos: [wall - radius - 1e-4, target[1], target[2]],
                        moved: false,
                        blocked: true,
                    };
                }
            }
            for (center, solid_radius) in &self.solids {
                let dx = target[0] - center[0];
                let dz = target[2] - center[2];
                if dx * dx + dz * dz < (solid_radius + radius).powi(2) {
                    return StepOutcome {
                        pos: from,
                        moved: false,
                        blocked: true,
                    };
                }
            }
            StepOutcome {
                pos: target,
                moved: target != from,
                blocked: false,
            }
        }
        fn wall_ahead(&self, eye: [f32; 3], dir: [f32; 3], reach: f32) -> bool {
            match self.wall_x {
                Some(wall) => dir[0] > 0.5 && eye[0] + reach >= wall,
                None => false,
            }
        }
        fn submerged(&self, eye: [f32; 3]) -> bool {
            self.water_y.is_some_and(|level| eye[1] < level)
        }
    }

    fn approx(actual: f32, expected: f32, tolerance: f32) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "actual {actual} vs expected {expected} (tol {tolerance})"
        );
    }

    #[test]
    fn golden_speeds_match_dfu() {
        approx(walk_speed(50), 4.430_38, 1e-4);
        approx(walk_speed(10), 3.164_557, 1e-4);
        approx(walk_speed(100), 6.329_114, 1e-4);
        approx(crouch_speed(50), 2.531_645_7, 1e-4);
        approx(run_speed(50, 0), 6.835_443, 1e-4);
        approx(run_speed(50, 100), 9.367_088, 1e-4);
        approx(sneak_speed(walk_speed(50)), 2.189_873_4, 1e-4);
        approx(swim_speed(walk_speed(50), 0), 1.107_594_9, 1e-4);
        approx(swim_speed(walk_speed(50), 100), 3.322_785, 1e-4);
        approx(climb_speed(walk_speed(50), false), 1.476_793_2, 1e-4);
        approx(climb_speed(walk_speed(50), true), 2.953_586_5, 1e-4);
        approx(levitate_speed(), 4.0, 1e-6);
    }

    #[test]
    fn run_faster_than_walk_and_sneak_slower() {
        // Anti-false-positive of the constants: the order of the modes.
        assert!(run_speed(50, 0) > walk_speed(50));
        assert!(walk_speed(50) > crouch_speed(50));
        assert!(sneak_speed(walk_speed(50)) < walk_speed(50));
        assert!(swim_speed(walk_speed(50), 100) > swim_speed(walk_speed(50), 0));
        assert!(walk_speed(100) > walk_speed(50));
        assert!(walk_speed(30) > walk_speed(10), "drag floor at 30");
    }

    #[test]
    fn golden_jump_and_fall() {
        approx(jump_velocity(&PlayerBody::default(), false), 4.5, 1e-5);
        approx(jump_apex(jump_velocity(&PlayerBody::default(), false)), 0.506_25, 1e-5);
        let mut body = PlayerBody {
            jumping_skill: 50,
            ..PlayerBody::default()
        };
        approx(jump_velocity(&body, false), 5.625, 1e-5);
        body.jumping_skill = 100;
        approx(jump_velocity(&body, false), 6.75, 1e-5);
        approx(jump_apex(6.75), 1.139_062_5, 1e-5);
        body.jumping_skill = 0;
        approx(jump_velocity(&body, true), 3.6, 1e-5);
        body.athleticism = true;
        approx(jump_velocity(&body, false), 4.95, 1e-5);
        body.improved_athleticism = true;
        approx(jump_velocity(&body, false), 5.4, 1e-5);
        body.athleticism = false;
        body.improved_athleticism = false;
        body.enhanced_jumping = true;
        approx(jump_velocity(&body, false), 7.2, 1e-5);

        assert_eq!(fall_outcome(2.5), FallOutcome::None);
        assert_eq!(fall_outcome(5.0), FallOutcome::HardFall);
        assert_eq!(fall_outcome(2.6), FallOutcome::HardFall);
        assert_eq!(fall_outcome(6.0), FallOutcome::Damage(5));
        assert_eq!(fall_outcome(10.5), FallOutcome::Damage(27));
    }

    #[test]
    fn golden_climb_chances_and_timers() {
        let mut body = PlayerBody {
            climbing_skill: 0,
            ..PlayerBody::default()
        };
        assert_eq!(climb_chance(&body, START_CLIMB_MIN_CHANCE), 76);
        body.climbing_skill = 50;
        assert_eq!(climb_chance(&body, START_CLIMB_MIN_CHANCE), 90);
        assert_eq!(climb_chance(&body, CONTINUE_CLIMB_MIN_CHANCE), 80);
        assert_eq!(climb_chance(&body, GRASP_WALL_MIN_CHANCE), 75);
        assert_eq!(climb_chance(&body, REGAIN_HOLD_MIN_CHANCE), 65);
        body.live_luck = 100;
        assert_eq!(climb_chance(&body, START_CLIMB_MIN_CHANCE), 95);
        body.live_luck = 50;
        body.khajiit = true;
        assert_eq!(climb_chance(&body, START_CLIMB_MIN_CHANCE), 99);
        body.khajiit = false;
        body.enhanced_climbing = true;
        assert_eq!(climb_chance(&body, START_CLIMB_MIN_CHANCE), 103);
        body.enhanced_climbing = false;
        body.climbing_skill = 95;
        assert_eq!(climb_chance(&body, START_CLIMB_MIN_CHANCE), 103);

        approx(climb_check_interval_s(ClimbCheckKind::Start), 0.768_955_6, 1e-6);
        approx(climb_check_interval_s(ClimbCheckKind::Continue), 0.823_881, 1e-6);
        approx(climb_check_interval_s(ClimbCheckKind::Slipping), 0.274_627, 1e-6);
    }

    #[test]
    fn golden_stealth_and_weight() {
        assert_eq!(stealth_chance(10.0, 50), 38);
        assert_eq!(stealth_chance(1.0, 50), 2);
        assert_eq!(stealth_chance(10.0, 100), 78);
        assert_eq!(stealth_chance(0.5, 10), 0);
        assert_eq!(interior_lockpicking_chance(5, 10, 40), 15);
        assert_eq!(interior_lockpicking_chance(1, 20, 10), 5);
        assert_eq!(exterior_lockpicking_chance(10, 40), 5);
        assert_eq!(exterior_lockpicking_chance(5, 100), 75);
        assert_eq!(pickpocketing_chance(40, 1, 10), 5);
        assert_eq!(pickpocketing_chance(60, 10, 5), 85);
        assert_eq!(shoplifting_chance(40, 5, 10), 75);

        assert_eq!(max_encumbrance(50), 75);
        assert_eq!(max_encumbrance(100), 150);
        assert_eq!(max_encumbrance(33), 49);
        assert!(!is_overencumbered(62.5));
        assert!(is_overencumbered(62.51));
        let body = PlayerBody {
            carried_weight_kg: 70.0,
            ..PlayerBody::default()
        };
        assert!(can_carry(&body, 5.0));
        assert!(!can_carry(&body, 6.0));
    }

    #[test]
    fn golden_breath() {
        assert_eq!(max_breath(50), 25);
        // `PlayerEntity.cs:329-337`: 19 tally increments + the decrementing
        // update = 20 classic updates = 1.25 s.
        approx(breath_drain_interval_s(), 1.25, 1e-6);
        approx(25.0 * breath_drain_interval_s(), 31.25, 1e-5);
    }

    #[test]
    fn walking_measures_the_spec_speed() {
        let world = FlatWorld::flat();
        let body = PlayerBody::default();
        let mut dice = Dice100::new(20261020);
        let mut locomotion = Locomotion::new([0.0, 1.6, 0.0], 1.6, 0.3);
        let input = MoveInput {
            forward: 1.0,
            yaw: 0.0,
            ..Default::default()
        };
        let mut travelled = 0.0;
        for _ in 0..120 {
            let before = locomotion.position;
            let frame = locomotion.step(&input, &body, &world, &mut dice, 1.0 / 60.0);
            approx(frame.speed, walk_speed(50), 1e-5);
            travelled += (locomotion.position[0] - before[0])
                .hypot(locomotion.position[2] - before[2]);
        }
        approx(travelled, walk_speed(50) * 2.0, 1e-2);
        approx(locomotion.position[1], 1.6, 1e-6);
    }

    #[test]
    fn diagonal_input_uses_dfu_factor() {
        let world = FlatWorld::flat();
        let body = PlayerBody::default();
        let mut dice = Dice100::new(3);
        let mut locomotion = Locomotion::new([0.0, 1.6, 0.0], 1.6, 0.3);
        let input = MoveInput {
            forward: 1.0,
            strafe: 1.0,
            yaw: 0.0,
            ..Default::default()
        };
        locomotion.step(&input, &body, &world, &mut dice, 1.0 / 60.0);
        let speed = (locomotion.position[0].powi(2) + locomotion.position[2].powi(2)).sqrt() * 60.0;
        approx(speed, walk_speed(50), 1e-2);
    }

    #[test]
    fn jump_reaches_apex_and_lands() {
        let world = FlatWorld::flat();
        let body = PlayerBody::default();
        let mut dice = Dice100::new(11);
        let mut locomotion = Locomotion::new([0.0, 1.6, 0.0], 1.6, 0.3);
        let input = MoveInput {
            jump: true,
            ..Default::default()
        };
        let mut apex = 0.0f32;
        let mut landed_after = None;
        for frame in 0..240 {
            let outcome = locomotion.step(&input, &body, &world, &mut dice, 1.0 / 60.0);
            apex = apex.max(locomotion.position[1] - 1.6);
            if frame > 5 && outcome.mode == MotionMode::Grounded && landed_after.is_none() {
                landed_after = Some(frame);
            }
        }
        // Semi-implicit Euler: discrete peak = v²/2g − v·dt/2 = 0.46875.
        approx(apex, jump_apex(4.5) - 4.5 * (1.0 / 60.0) / 2.0, 2e-3);
        assert!(
            landed_after.is_some_and(|frame| (20..=40).contains(&frame)),
            "must land ~0.45 s after the jump: {landed_after:?}"
        );
    }

    #[test]
    fn falling_ten_metres_deals_spec_damage() {
        let world = FlatWorld::flat();
        let body = PlayerBody::default();
        let mut dice = Dice100::new(13);
        let mut locomotion = Locomotion::new([0.0, 12.0, 0.0], 1.6, 0.3);
        locomotion.mode = MotionMode::Airborne;
        locomotion.falling = true;
        locomotion.fall_start_y = 12.0;
        let input = MoveInput::default();
        let mut damage = 0;
        for _ in 0..240 {
            let outcome = locomotion.step(&input, &body, &world, &mut dice, 1.0 / 60.0);
            damage += outcome.fall_damage;
            if outcome.mode == MotionMode::Grounded {
                break;
            }
        }
        // Eye fall = 12.0 − 1.6 = 10.4 m (f32: 10.3999996) →
        // (int)(5·5.3999996) = 26. Same f32 arithmetic as the source.
        let measured = 12.0f32 - 1.6f32;
        assert_eq!(damage, 26, "damage over the real eye fall ({measured})");
        assert_eq!(damage, fall_outcome(measured).as_damage());
        approx(locomotion.position[1], 1.6, 1e-6);
    }

    #[test]
    fn slow_fall_cancels_the_accumulated_fall_distance() {
        // Same 10.4 m eye fall as `falling_ten_metres_deals_spec_damage`, but
        // with `Entity.IsSlowFalling` set (`AcrobatMotor.cs:184-187`).
        let world = FlatWorld::flat();
        let body = PlayerBody {
            slow_falling: true,
            ..PlayerBody::default()
        };
        let mut dice = Dice100::new(13);
        let mut locomotion = Locomotion::new([0.0, 12.0, 0.0], 1.6, 0.3);
        locomotion.mode = MotionMode::Airborne;
        locomotion.falling = true;
        locomotion.fall_start_y = 12.0;
        let input = MoveInput::default();
        let mut damage = 0;
        let mut hard_fall = false;
        for _ in 0..600 {
            let outcome = locomotion.step(&input, &body, &world, &mut dice, 1.0 / 60.0);
            damage += outcome.fall_damage;
            hard_fall |= outcome.hard_fall;
            if outcome.mode == MotionMode::Grounded {
                break;
            }
        }
        assert_eq!(damage, 0, "slow fall never accumulates damage distance");
        assert!(!hard_fall, "slow fall never triggers the hard-fall alert");
        approx(locomotion.position[1], 1.6, 1e-6);
    }

    #[test]
    fn walking_into_a_wall_never_penetrates() {
        let world = FlatWorld {
            wall_x: Some(2.0),
            ..FlatWorld::flat()
        };
        let body = PlayerBody::default();
        let mut dice = Dice100::new(17);
        let mut locomotion = Locomotion::new([0.0, 1.6, 0.0], 1.6, 0.3);
        let input = MoveInput {
            forward: 1.0,
            yaw: -std::f32::consts::FRAC_PI_2, // looks toward +X
            ..Default::default()
        };
        for _ in 0..240 {
            locomotion.step(&input, &body, &world, &mut dice, 1.0 / 60.0);
        }
        assert!(
            locomotion.position[0] + 0.3 <= 2.0 + 1e-3,
            "the eye cannot pass through the wall: x={}",
            locomotion.position[0]
        );
    }

    #[test]
    fn climbing_rises_after_the_start_check() {
        let world = FlatWorld {
            wall_x: Some(1.0),
            ..FlatWorld::flat()
        };
        let body = PlayerBody {
            climbing_skill: 95,
            live_luck: 100, // chance 103 → never fails
            ..PlayerBody::default()
        };
        let mut dice = Dice100::new(19);
        let mut locomotion = Locomotion::new([0.6, 1.6, 0.0], 1.6, 0.3);
        let input = MoveInput {
            forward: 1.0,
            yaw: -std::f32::consts::FRAC_PI_2,
            ..Default::default()
        };
        // First check after 14·divisor ≈ 0.769 s.
        for _ in 0..47 {
            locomotion.step(&input, &body, &world, &mut dice, 1.0 / 60.0);
        }
        assert_eq!(locomotion.mode, MotionMode::Climbing, "must have grabbed the wall");
        let start_y = locomotion.position[1];
        for _ in 0..60 {
            locomotion.step(&input, &body, &world, &mut dice, 1.0 / 60.0);
        }
        approx(
            locomotion.position[1] - start_y,
            climb_speed(walk_speed(50), false),
            2e-2,
        );
    }

    #[test]
    fn climb_check_flags_every_consumed_die() {
        // Intervals at 1/60 s frames: start `0.768956 s` ⇒ 47 frames, so the
        // start die is consumed at index 46 (that step also advances the climb
        // timer once, inside `step_climbing`); the continue interval
        // `0.823881 s` therefore needs 49 further frames ⇒ index 95. Without
        // the flag the M20 driver cannot tally Climbing
        // (`ClimbingMotor.cs:823`).
        let world = FlatWorld {
            wall_x: Some(1.0),
            ..FlatWorld::flat()
        };
        let body = PlayerBody {
            climbing_skill: 95,
            live_luck: 100, // chance 103 → the start check never fails
            ..PlayerBody::default()
        };
        let mut dice = Dice100::new(19);
        let mut locomotion = Locomotion::new([0.6, 1.6, 0.0], 1.6, 0.3);
        let input = MoveInput {
            forward: 1.0,
            yaw: -std::f32::consts::FRAC_PI_2,
            ..Default::default()
        };
        let mut flagged = Vec::new();
        for index in 0..97 {
            let frame = locomotion.step(&input, &body, &world, &mut dice, 1.0 / 60.0);
            if frame.climb_check {
                flagged.push(index);
            }
        }
        assert_eq!(flagged, vec![46, 95], "start check + one continue check");
        assert_eq!(locomotion.mode, MotionMode::Climbing);

        // No wall ⇒ no climbing die ⇒ no flag.
        let open = FlatWorld::flat();
        let mut walker = Locomotion::new([0.0, 1.6, 0.0], 1.6, 0.3);
        let mut walk_dice = Dice100::new(19);
        for _ in 0..97 {
            assert!(!walker.step(&input, &body, &open, &mut walk_dice, 1.0 / 60.0).climb_check);
        }
    }

    /// Climbs the flat wall from `start_y` holding on, until the continue check
    /// fails and the body slips to the ground. Returns the fall start level the
    /// slip armed, the damage billed on grounding and the hard-fall flag.
    ///
    /// Seed 3: the first continue check fails and every regain check fails until
    /// the ground, so the descent stays a single slip.
    fn climb_and_slip(start_y: f32) -> (f32, i32, bool) {
        let world = FlatWorld {
            wall_x: Some(1.0),
            ..FlatWorld::flat()
        };
        // Luck 0: continue 50 %, regain 20 % (`FormulaHelper.cs:293-316`).
        let body = PlayerBody {
            live_luck: 0,
            ..PlayerBody::default()
        };
        let mut dice = Dice100::new(3);
        let mut locomotion = Locomotion::new([0.6, start_y, 0.0], 1.6, 0.3);
        // Already holding the wall: this measures the continue check, the slip
        // and the landing (the grab is `climbing_rises_after_the_start_check`).
        locomotion.mode = MotionMode::Climbing;
        let input = MoveInput {
            forward: 1.0,
            yaw: -std::f32::consts::FRAC_PI_2,
            ..Default::default()
        };
        let mut slip_start = None;
        let mut damage = 0;
        let mut hard = false;
        for _ in 0..600 {
            let frame = locomotion.step(&input, &body, &world, &mut dice, 1.0 / 60.0);
            if slip_start.is_none() && locomotion.slipping {
                // `ClimbingMotor.cs:760-764` → `CheckInitFall`
                // (`AcrobatMotor.cs:167-177`): the slip arms the fall with the
                // level of that moment.
                assert!(locomotion.falling, "the slip arms CheckInitFall");
                slip_start = Some(locomotion.fall_start_y);
            }
            damage += frame.fall_damage;
            hard |= frame.hard_fall;
            if frame.mode == MotionMode::Grounded {
                break;
            }
        }
        assert_eq!(locomotion.mode, MotionMode::Grounded, "the slip must reach the ground");
        assert!(!locomotion.falling, "CheckFallingDamage clears the fall");
        (slip_start.expect("the continue check must fail"), damage, hard)
    }

    #[test]
    fn slipping_bills_the_fall_on_grounding() {
        // `ClimbingMotor.cs:760-764`: the slipping branch calls
        // `acrobatMotor.CheckInitFall` + `ApplyGravity`, so a failed continue
        // check is a real fall; `PlayerMotor.cs:337-341` calls
        // `CheckFallingDamage` on grounding and `AcrobatMotor.cs:215-223` bills
        // `fallStartLevel - y` through the gate
        // `!climbingMotor.IsClimbing || climbingMotor.IsSlipping` (`:217`).
        let ground_eye = 1.6;

        // Drop above the 5 m threshold → damage, from the armed level.
        let (start_high, damage_high, hard_high) = climb_and_slip(9.0);
        let drop_high = start_high - ground_eye;
        assert!(
            drop_high > FALLING_DAMAGE_THRESHOLD_M,
            "the slip must start above the threshold: {drop_high} m"
        );
        assert!(damage_high > 0, "a slip over the threshold bills: {damage_high}");
        assert_eq!(damage_high, fall_outcome(drop_high).as_damage());
        assert!(!hard_high, "damage replaces the alert (`AcrobatMotor.cs:219-223`)");

        // Drop between threshold/2 and the threshold → hard fall, 0 damage.
        let (start_low, damage_low, hard_low) = climb_and_slip(3.5);
        let drop_low = start_low - ground_eye;
        assert!(
            drop_low > FALLING_DAMAGE_THRESHOLD_M / 2.0
                && drop_low <= FALLING_DAMAGE_THRESHOLD_M,
            "the slip must stay under the threshold: {drop_low} m"
        );
        assert_eq!(damage_low, 0, "under the threshold nothing is billed");
        assert!(hard_low, "the billing ran and classified a hard fall");

        // A **normal** climb reaching the ground takes nothing: `IsClimbing`
        // without `IsSlipping` fails the gate (`AcrobatMotor.cs:217`).
        let world = FlatWorld {
            wall_x: Some(1.0),
            ..FlatWorld::flat()
        };
        let body = PlayerBody {
            climbing_skill: 95,
            live_luck: 100, // chance 103 → continue and regain never fail
            ..PlayerBody::default()
        };
        let mut dice = Dice100::new(19);
        let mut locomotion = Locomotion::new([0.6, 3.5, 0.0], 1.6, 0.3);
        locomotion.mode = MotionMode::Climbing;
        let down = MoveInput {
            forward: -1.0,
            yaw: -std::f32::consts::FRAC_PI_2,
            ..Default::default()
        };
        let mut damage = 0;
        let mut hard = false;
        for _ in 0..600 {
            let frame = locomotion.step(&down, &body, &world, &mut dice, 1.0 / 60.0);
            damage += frame.fall_damage;
            hard |= frame.hard_fall;
            if frame.mode == MotionMode::Grounded {
                break;
            }
        }
        assert_eq!(locomotion.mode, MotionMode::Grounded, "the descent reaches the ground");
        assert!(!locomotion.slipping, "a held climb never slips");
        assert_eq!(damage, 0, "climbing down is not a fall");
        assert!(!hard, "and it is not a hard fall either");
    }

    /// Walks from `x = -1` to `x ≥ 0` across a ledge of `height` metres and
    /// returns (fall armed at, damage billed, hard fall, final mode, final eye y).
    fn walk_off_ledge(height: f32) -> (Option<f32>, i32, bool, MotionMode, f32) {
        let world = FlatWorld {
            ledge_height: Some(height),
            ..FlatWorld::flat()
        };
        let body = PlayerBody::default();
        let mut dice = Dice100::new(11);
        let mut locomotion = Locomotion::new([-1.0, height + 1.6, 0.0], 1.6, 0.3);
        let input = MoveInput {
            forward: 1.0,
            yaw: -std::f32::consts::FRAC_PI_2,
            ..Default::default()
        };
        let mut armed = None;
        let mut damage = 0;
        let mut hard = false;
        for _ in 0..600 {
            let frame = locomotion.step(&input, &body, &world, &mut dice, 1.0 / 60.0);
            if armed.is_none() && frame.mode == MotionMode::Airborne {
                armed = Some(locomotion.fall_start_y);
            }
            damage += frame.fall_damage;
            hard |= frame.hard_fall;
            if armed.is_some() && frame.mode == MotionMode::Grounded {
                break;
            }
        }
        (armed, damage, hard, locomotion.mode, locomotion.position[1])
    }

    #[test]
    fn ledge_step_down_arms_the_fall_and_bills_the_descent() {
        // `PlayerMotor.cs:337-353`: when the ground disappears from under the
        // feet (`:278`, `CollisionFlags.Below` false) DFU arms
        // `CheckInitFall` (`AcrobatMotor.cs:167-177`) instead of snapping to the
        // new floor; `CheckFallingDamage` (`:207-225`) then bills
        // `fallStartLevel - y` = (int)(5·(d−5)) for d = 10 m ⇒ 25.
        let (armed, damage, hard, mode, eye_y) = walk_off_ledge(10.0);
        let start = armed.expect("losing the ledge arms CheckInitFall");
        let drop = start - 1.6;
        approx(drop, 10.0, 1e-3);
        assert_eq!(mode, MotionMode::Grounded, "the fall reaches the ground");
        approx(eye_y, 1.6, 1e-4);
        assert_eq!(damage, 25, "d = 10 m ⇒ (int)(5·(10−5)) = 25");
        assert_eq!(damage, fall_outcome(drop).as_damage());
        assert!(!hard, "damage replaces the hard-fall alert (`:219-223`)");
    }

    #[test]
    fn small_step_down_never_arms_a_fall() {
        // Under the step-down limit the heightfield snaps as before: no fall,
        // no damage, no hard-fall alert, still grounded.
        let (armed, damage, hard, mode, eye_y) = walk_off_ledge(0.2);
        assert!(armed.is_none(), "a 0.2 m step is not a fall");
        assert_eq!(mode, MotionMode::Grounded, "the body never leaves the ground");
        approx(eye_y, 1.6, 1e-4);
        assert_eq!(damage, 0);
        assert!(!hard);
    }

    #[test]
    fn submerged_levitation_drowns_like_swimming() {
        // `PlayerEntity.cs:323`: the breath block's only gate is
        // `IsPlayerSubmerged && !IsWaterBreathing` — the motor is irrelevant, so
        // a levitating submerged body drains and drowns. `MaxBreath` 0 drowns on
        // the first submerged tick (`:339-340`).
        let world = FlatWorld {
            water_y: Some(20.0),
            ..FlatWorld::flat()
        };
        let fragile = PlayerBody {
            live_endurance: 0,
            ..PlayerBody::default()
        };
        let mut locomotion = Locomotion::new([0.0, 0.0, 0.0], 1.6, 0.3);
        let mut dice = Dice100::new(41);
        let input = MoveInput {
            levitating: true,
            ..Default::default()
        };
        let frame = locomotion.step(&input, &fragile, &world, &mut dice, 1.0 / 60.0);
        assert_eq!(locomotion.mode, MotionMode::Levitating);
        assert!(frame.drowned, "levitating does not exempt the breath block");

        // The water-breathing body never drains and the field is written to 0
        // (`:342-343`), which is the DFU value, not `MaxBreath`.
        let breather = PlayerBody {
            water_breathing: true,
            ..PlayerBody::default()
        };
        let mut locomotion = Locomotion::new([0.0, 0.0, 0.0], 1.6, 0.3);
        let mut dice = Dice100::new(43);
        let mut drowned = false;
        for _ in 0..180 {
            drowned |= locomotion.step(&input, &breather, &world, &mut dice, 1.0 / 60.0).drowned;
        }
        assert_eq!(locomotion.breath, 0, "`:342-343` writes 0 with the effect on");
        assert!(!drowned);
    }

    #[test]
    fn swimming_advances_breath_and_sinks_when_overencumbered() {
        let world = FlatWorld {
            water_y: Some(20.0),
            ..FlatWorld::flat()
        };
        let body = PlayerBody::default();
        let mut dice = Dice100::new(23);
        let mut locomotion = Locomotion::new([0.0, 0.0, 0.0], 1.6, 0.3);
        // The eye is below the water level: the engine enters swimming.
        let input = MoveInput {
            forward: 1.0,
            ..Default::default()
        };
        locomotion.step(&input, &body, &world, &mut dice, 1.0 / 60.0);
        assert_eq!(locomotion.mode, MotionMode::Swimming);
        assert_eq!(locomotion.breath, max_breath(50));
        for _ in 0..(3.0 * 60.0) as usize {
            locomotion.step(&input, &body, &world, &mut dice, 1.0 / 60.0);
        }
        assert_eq!(locomotion.breath, 23, "3 s → 2 breath points");

        let loaded = PlayerBody {
            carried_weight_kg: 70.0,
            ..PlayerBody::default()
        };
        let mut diver = Locomotion::new([0.0, 0.0, 0.0], 1.6, 0.3);
        let sink = MoveInput {
            jump: true,
            ..Default::default()
        };
        let mut dice = Dice100::new(29);
        let start = diver.position[1];
        for _ in 0..60 {
            diver.step(&sink, &loaded, &world, &mut dice, 1.0 / 60.0);
        }
        assert!(diver.position[1] < start, "overencumbered does not float: it sinks");
        approx(
            start - diver.position[1],
            swim_speed(walk_speed(50), 0),
            2e-2,
        );
    }

    #[test]
    fn exhausted_breath_refills_instead_of_drowning_forever() {
        // `PlayerEntity.cs:325-328`: the submerged block opens with
        // `if (currentBreath == 0) currentBreath = DeepBreath(MaxBreath)` —
        // **before** the decrement of the same update. `DeepBreath` only folds
        // guild memberships over the value (`GuildManager.cs:385-391`), so with
        // no memberships it is the identity and the refill is `MaxBreath`
        // (`DaggerfallEntity.cs:267`) = 25 for the default body. `:339-340` sets
        // health to 0 only when the breath is still ≤ 0 after the refill.
        let world = FlatWorld {
            water_y: Some(20.0),
            ..FlatWorld::flat()
        };
        let body = PlayerBody::default();
        let input = MoveInput {
            forward: 1.0,
            ..Default::default()
        };
        // `dt` == the drain interval (exactly representable: 20·0.0625 = 1.25),
        // so every step consumes exactly one breath point.
        let interval = breath_drain_interval_s();
        let mut dice = Dice100::new(73);
        let mut locomotion = Locomotion::new([0.0, 0.0, 0.0], 1.6, 0.3);
        let mut drowned_on = Vec::new();
        for frame in 0..max_breath(50) as usize {
            let outcome = locomotion.step(&input, &body, &world, &mut dice, interval);
            if outcome.drowned {
                drowned_on.push(frame);
            }
        }
        assert_eq!(
            drowned_on,
            vec![24],
            "one full drain 25→0: only the tick that reaches 0 drowns"
        );
        assert_eq!(locomotion.breath, 0, "the source does not clamp the field");

        // The next tick enters with 0 ⇒ refill to MaxBreath **before** draining,
        // and it is not a drowning tick. Before this fix the breath stuck at 0
        // and every submerged tick drowned.
        let refilled = locomotion.step(&input, &body, &world, &mut dice, interval / 2.0);
        assert!(!refilled.drowned, "the refill runs before the death check");
        assert_eq!(locomotion.breath, max_breath(50), "refilled to DeepBreath(MaxBreath)");
        let drained = locomotion.step(&input, &body, &world, &mut dice, interval / 2.0);
        assert!(!drained.drowned);
        assert_eq!(locomotion.breath, max_breath(50) - 1, "and drains normally again");

        // `MaxBreath` 0 (`LiveEndurance` 0): the refill writes 0, so every
        // draining frame still drowns (`PlayerEntity.cs:339-340`).
        let fragile = PlayerBody {
            live_endurance: 0,
            ..PlayerBody::default()
        };
        let mut zero = Locomotion::new([0.0, 0.0, 0.0], 1.6, 0.3);
        let mut dice = Dice100::new(79);
        let mut drowns = 0;
        for _ in 0..3 {
            if zero.step(&input, &fragile, &world, &mut dice, interval).drowned {
                drowns += 1;
            }
        }
        assert_eq!(drowns, 3, "MaxBreath 0 drowns on every draining frame");
    }

    #[test]
    fn levitation_rises_at_four_metres_per_second() {
        let world = FlatWorld::flat();
        let body = PlayerBody::default();
        let mut dice = Dice100::new(31);
        let mut locomotion = Locomotion::new([0.0, 3.0, 0.0], 1.6, 0.3);
        let input = MoveInput {
            levitating: true,
            jump: true,
            ..Default::default()
        };
        let start = locomotion.position[1];
        for _ in 0..60 {
            locomotion.step(&input, &body, &world, &mut dice, 1.0 / 60.0);
        }
        approx(locomotion.position[1] - start, 4.0, 1e-2);
        assert_eq!(locomotion.mode, MotionMode::Levitating);
    }

    #[test]
    fn dice_matches_dfu_inequality() {
        // `success(c) ⇔ roll < c`; with c=0 never and with c=100 always.
        let mut dice = Dice100::new(41);
        for _ in 0..50 {
            assert!(!dice.success(0));
            assert!(dice.success(100));
        }
    }

    #[test]
    fn jump_edge_is_raised_once_per_started_jump() {
        // `AcrobatMotor.cs:109`: `jumping = true` only in the frame the input
        // is consumed. Holding jump must not re-raise it while airborne, and
        // the landing must not either.
        let world = FlatWorld::flat();
        let body = PlayerBody::default();
        let mut dice = Dice100::new(43);
        let mut locomotion = Locomotion::new([0.0, 1.6, 0.0], 1.6, 0.3);
        let input = MoveInput {
            jump: true,
            ..Default::default()
        };
        // `GroundedTime` needs 0.1 s first (`AcrobatMotor.cs:76-77`).
        let mut raised = Vec::new();
        let mut takeoffs = Vec::new();
        let mut airborne = 0;
        let mut was_grounded = locomotion.mode == MotionMode::Grounded;
        for frame in 0..140 {
            let outcome = locomotion.step(&input, &body, &world, &mut dice, 1.0 / 60.0);
            if outcome.jumped {
                raised.push(frame);
            }
            if was_grounded && outcome.mode == MotionMode::Airborne {
                takeoffs.push(frame);
            }
            was_grounded = outcome.mode == MotionMode::Grounded;
            if outcome.mode == MotionMode::Airborne {
                airborne += 1;
            }
        }
        assert!(raised.len() >= 2, "several full jumps over 140 frames: {raised:?}");
        assert_eq!(raised, takeoffs, "one edge per takeoff, never re-raised in the air");
        assert!(
            airborne > raised.len(),
            "each jump spans several airborne steps: {airborne} airborne vs {} edges",
            raised.len()
        );
    }

    #[test]
    fn falls_and_ledge_drops_are_not_jumps() {
        // Falling from 12 m without ever pressing jump: `AcrobatMotor.jumping`
        // is never set, so the M20 driver must not tally Jumping.
        let world = FlatWorld::flat();
        let body = PlayerBody::default();
        let mut dice = Dice100::new(47);
        let mut locomotion = Locomotion::new([0.0, 12.0, 0.0], 1.6, 0.3);
        locomotion.mode = MotionMode::Airborne;
        locomotion.falling = true;
        locomotion.fall_start_y = 12.0;
        let mut steps = 0;
        for _ in 0..240 {
            let outcome = locomotion.step(&MoveInput::default(), &body, &world, &mut dice, 1.0 / 60.0);
            assert!(!outcome.jumped, "a ledge drop is not a jump");
            steps += 1;
            if outcome.mode == MotionMode::Grounded {
                break;
            }
        }
        assert!(steps > 1, "the drop spanned {steps} steps");
        assert_eq!(locomotion.mode, MotionMode::Grounded);

        // Leaving levitation also turns `Airborne` without a jump.
        let mut floating = Locomotion::new([0.0, 3.0, 0.0], 1.6, 0.3);
        let mut dice = Dice100::new(53);
        let up = MoveInput {
            levitating: true,
            jump: true,
            ..Default::default()
        };
        for _ in 0..10 {
            let outcome = floating.step(&up, &body, &world, &mut dice, 1.0 / 60.0);
            assert!(!outcome.jumped, "levitating up is not a jump");
        }
        assert_eq!(floating.mode, MotionMode::Levitating);
        let outcome = floating.step(&MoveInput::default(), &body, &world, &mut dice, 1.0 / 60.0);
        assert_eq!(outcome.mode, MotionMode::Airborne);
        assert!(!outcome.jumped, "the fall after levitation is not a jump");
    }

    #[test]
    fn jumped_jump_is_refused_by_the_grounded_delay() {
        // `GroundedTime < 0.1f` returns before the input is read
        // (`AcrobatMotor.cs:76-77`): no jump, no edge.
        let world = FlatWorld::flat();
        let body = PlayerBody::default();
        let mut dice = Dice100::new(59);
        let mut locomotion = Locomotion::new([0.0, 1.6, 0.0], 1.6, 0.3);
        let input = MoveInput {
            jump: true,
            ..Default::default()
        };
        let refused = locomotion.step(&input, &body, &world, &mut dice, 0.05);
        assert!(!refused.jumped, "0.05 s grounded < 0.1 s");
        assert_eq!(refused.mode, MotionMode::Grounded);
        let accepted = locomotion.step(&input, &body, &world, &mut dice, 0.06);
        assert!(accepted.jumped, "0.11 s grounded ≥ 0.1 s");
        assert_eq!(accepted.mode, MotionMode::Airborne);
    }

    #[test]
    fn running_mode_is_latched_while_standing_still() {
        // `isRunning = CanRun() && runningMode` (`PlayerSpeedChanger.cs:110`)
        // carries **no** movement test: standing still with Run held is running.
        let world = FlatWorld::flat();
        let body = PlayerBody::default();
        let mut dice = Dice100::new(61);
        let mut locomotion = Locomotion::new([0.0, 1.6, 0.0], 1.6, 0.3);
        let input = MoveInput {
            run: true,
            ..Default::default()
        };
        for _ in 0..60 {
            let outcome = locomotion.step(&input, &body, &world, &mut dice, 1.0 / 60.0);
            assert!(outcome.running, "Run held standing still");
            assert!(!outcome.moved, "no movement input");
        }
        assert!(locomotion.running_mode);
    }

    #[test]
    fn running_mode_survives_the_airborne_steps() {
        // "you can't switch running on/off while in mid air"
        // (`PlayerSpeedChanger.cs:113-117`): the resolution only happens on the
        // ground, so the key released mid-air takes effect after landing.
        let world = FlatWorld::flat();
        let body = PlayerBody::default();
        let mut dice = Dice100::new(67);
        let mut locomotion = Locomotion::new([0.0, 1.6, 0.0], 1.6, 0.3);
        let run = MoveInput {
            run: true,
            ..Default::default()
        };
        for _ in 0..7 {
            assert!(locomotion.step(&run, &body, &world, &mut dice, 1.0 / 60.0).running);
        }
        let jump = MoveInput {
            run: true,
            jump: true,
            ..Default::default()
        };
        assert!(locomotion.step(&jump, &body, &world, &mut dice, 1.0 / 60.0).jumped);
        assert_eq!(locomotion.mode, MotionMode::Airborne);

        // Key released in the air: the latch must not follow.
        let mut airborne_steps = 0;
        for _ in 0..240 {
            let outcome = locomotion.step(&MoveInput::default(), &body, &world, &mut dice, 1.0 / 60.0);
            assert!(outcome.running, "running is kept in the air");
            if outcome.mode == MotionMode::Airborne {
                airborne_steps += 1;
            } else {
                break;
            }
        }
        assert!(airborne_steps > 0, "the jump must have airborne steps");
        assert_eq!(locomotion.mode, MotionMode::Grounded);

        // First grounded step after landing: the released key takes effect.
        let landed = locomotion.step(&MoveInput::default(), &body, &world, &mut dice, 1.0 / 60.0);
        assert!(!landed.running, "the key is released and the body is grounded");
        assert!(!locomotion.running_mode);
        // And pressing it again on the ground raises it on the next step.
        let pressed = locomotion.step(&run, &body, &world, &mut dice, 1.0 / 60.0);
        assert!(pressed.running);
    }

    #[test]
    fn running_mode_stays_off_when_the_key_is_never_pressed() {
        let world = FlatWorld::flat();
        let body = PlayerBody::default();
        let mut dice = Dice100::new(71);
        let mut locomotion = Locomotion::new([0.0, 1.6, 0.0], 1.6, 0.3);
        let jump = MoveInput {
            jump: true,
            ..Default::default()
        };
        let mut jumped = false;
        for _ in 0..240 {
            let outcome = locomotion.step(&jump, &body, &world, &mut dice, 1.0 / 60.0);
            jumped |= outcome.jumped;
            assert!(!outcome.running, "Run was never pressed");
        }
        assert!(jumped, "the jump must have happened");
    }
}

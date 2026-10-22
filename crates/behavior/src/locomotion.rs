//! M19 — locomotion and player body (REQ-L99-052).
//!
//! **Organ:** `behavior` — row REQ-L99-052 of the 00c assigns locomotion to
//! *BC Behavior (locomotion)*. This module is **pure** (0 Bevy, 0 I/O, 0 clock):
//! it receives `dt` and a [`LocomotionWorld`] (ground height + collision + wall +
//! water) that the caller implements. The demo and the probe use **the same** engine.
//!
//! **Measured spec:** `docs/sintergic/game/programming/m19-locomotion-spec-CR-8313e821.md`,
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

/// Levitation speed, m/s (`LevitateMotor.cs:25`).
pub const LEVITATE_MOVE_SPEED: f32 = 4.0;
/// Diagonal input factor (`PlayerMotor.cs:30`, `FrictionMotor.cs:93`).
/// The literal `0.7071` is the one from the DFU source (4 decimals), **not**
/// an approximation of `1/√2`: it is kept as is.
#[allow(clippy::approx_constant)]
pub const DIAGONAL_FACTOR: f32 = 0.7071;

/// Classic update interval, s (`GameManager.cs:42`).
pub const CLASSIC_UPDATE_INTERVAL_S: f32 = 0.0625;
/// Classic updates per breath point minus 1 (`PlayerEntity.cs:329`).
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

/// Breath drain interval (`PlayerEntity.cs:329`, `GameManager.cs:42`).
pub fn breath_drain_interval_s() -> f32 {
    (BREATH_TALLY_LIMIT + 1) as f32 * CLASSIC_UPDATE_INTERVAL_S
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

        // DFU PlayerMotor.cs:323 — levitating/swimming take over the engine and
        // cancel accumulated velocity.
        if water_mode {
            let was_swimming = self.mode == MotionMode::Swimming;
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
            if !was_swimming && self.mode == MotionMode::Swimming {
                // `PlayerEntity.cs:327`: breath refills when the immersion starts.
                self.breath = max_breath(body.live_endurance);
                self.breath_tally = 0.0;
            }
            self.step_water(input, body, world, dice, dt, &mut frame);
            return frame;
        }

        // Leaving the water or stopping levitation → falls (`LevitateMotor.cs:168-187`).
        if matches!(self.mode, MotionMode::Swimming | MotionMode::Levitating) {
            self.mode = MotionMode::Airborne;
            self.falling = true;
            self.fall_start_y = self.position[1];
            self.vertical_velocity = 0.0;
            self.breath = 0;
            self.breath_tally = 0.0;
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
        let speed = resolve_speed(body, input.crouch, input.run, input.sneak, false);
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
            // Heightfield: the eye follows the ground (step-up/down without anti-bump).
            self.position[1] = ground_eye;
            self.vertical_velocity = 0.0;
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

        let ground_eye = world.ground_height(self.position[0], self.position[2]) + self.eye_height;
        if self.position[1] <= ground_eye {
            let fall_distance = self.fall_start_y - ground_eye;
            if self.falling {
                match fall_outcome(fall_distance) {
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
            self.horizontal_velocity = [0.0, 0.0];
        }
    }

    fn step_water(
        &mut self,
        input: &MoveInput,
        body: &PlayerBody,
        world: &dyn LocomotionWorld,
        dice: &mut Dice100,
        dt: f32,
        frame: &mut Frame,
    ) {
        let levitating = self.mode == MotionMode::Levitating;
        let speed = if levitating {
            levitate_speed()
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
        } else if overencumbered {
            // `LevitateMotor.cs:83-85`: overencumbered it sinks and cannot rise.
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

        if self.mode == MotionMode::Swimming && !body.water_breathing {
            self.breath_tally += dt;
            let interval = breath_drain_interval_s();
            while self.breath_tally >= interval {
                self.breath_tally -= interval;
                // `PlayerEntity.cs:330-331`: Argonian 50 % chance of not consuming.
                let skip = body.argonian && dice.roll() % 2 == 1;
                if !skip {
                    self.breath -= 1;
                }
            }
            if self.breath <= 0 {
                self.breath = 0;
                frame.drowned = true;
            }
        }
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
            // wall ends the climb; the fall starts here.
            self.mode = MotionMode::Airborne;
            self.falling = true;
            self.fall_start_y = self.position[1];
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
            if self.slipping {
                if dice.success(climb_chance(body, REGAIN_HOLD_MIN_CHANCE)) {
                    self.slipping = false;
                    self.vertical_velocity = 0.0;
                }
            } else if !dice.success(climb_chance(body, CONTINUE_CLIMB_MIN_CHANCE)) {
                self.slipping = true;
                self.vertical_velocity = 0.0;
            }
        }

        let ground_eye = world.ground_height(self.position[0], self.position[2]) + self.eye_height;
        if self.position[1] <= ground_eye {
            self.position[1] = ground_eye;
            self.mode = MotionMode::Grounded;
            self.grounded_time = 0.0;
            self.falling = false;
            self.slipping = false;
            self.vertical_velocity = 0.0;
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
        wall_x: Option<f32>,
        water_y: Option<f32>,
        solids: Vec<([f32; 3], f32)>,
    }

    impl FlatWorld {
        fn flat() -> Self {
            Self {
                ground: 0.0,
                wall_x: None,
                water_y: None,
                solids: Vec::new(),
            }
        }
    }

    impl LocomotionWorld for FlatWorld {
        fn ground_height(&self, _x: f32, _z: f32) -> f32 {
            self.ground
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
        approx(breath_drain_interval_s(), 1.1875, 1e-6);
        approx(25.0 * breath_drain_interval_s(), 29.6875, 1e-5);
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
}

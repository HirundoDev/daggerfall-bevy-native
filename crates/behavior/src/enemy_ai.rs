//! Observable enemy AI (spec M25 §3-§4 and §6, REQ-L99-058).
//!
//! **R-G5:** there is no `MonoBehaviour` here — this implements the
//! **observable behavior** of spec §4.3 (states, thresholds in meters and
//! times in seconds). Timers advance per 0.0625 s `ClassicUpdate`
//! (`GameManager.cs:42`), never per video frames.
//!
//! | What | Source |
//! |---|---|
//! | Speed `(LiveSpeed + 150) · 0.025` m/s | `EnemyMotor.cs:433` · `PlayerSpeedChanger.cs:31` · `MeshReader.cs:58` |
//! | `stopDistance` 2.25 m (or 1.5 m classic AI) | `EnemyMotor.cs:446-448` · `EnemyAttack.cs:30-31` |
//! | Pursuit `stopDistance · 2.75` = 6.1875 m · retreat `· 0.8` = 1.8 m | `EnemyMotor.cs:487` · `:506-507` |
//! | Senses 4096 u ⇒ 102.4 m, hearing 25 m, FOV 180° | `EnemySenses.cs` |
//! | `GiveUpTimer` 200 ticks ⇒ 12.5 s | `EnemyMotor.cs:94`, `:201`, `:420`, `:423-424` |
//! | Door at ≤ 2 m | `EnemyMotor.cs:34` |
//! | Attack gate `(speed >> 3) + 6`, floor `speedFloor = 8` | `EnemyAttack.cs:81`, `:53`, `:70-73` |
//! | Cadence `max(0, r − 50·(level−10) + 450·(reflexes−2)) / 980` | `EnemyAttack.cs:114-131` · `FormulaHelper.cs:55` |
//! | Class HP `10 + L·[1, hppl]` | `FormulaHelper.cs:1729-1743` |
//! | Pacification by language (domain `roll < chance`, 200) | `FormulaHelper.cs:357-391` |
//!
//! Pure: 0 Bevy, 0 I/O, 0 global RNG — rolls and perception come in as
//! parameters (the composer decides the seed; `PROBE-ENEMY-AI-001` fixes it).

/// One DFU `ClassicUpdate` in seconds (`GameManager.cs:42`).
pub const CLASSIC_TICK: f32 = 0.0625;

/// `MeshReader.GlobalScale` — classic units → meters.
pub const GLOBAL_SCALE: f32 = 0.025;
/// `PlayerSpeedChanger.dfWalkBase` added to `LiveSpeed` (`EnemyMotor.cs:433`).
pub const DF_WALK_BASE: f32 = 150.0;
/// `EnemySenses.SightRadius` in classic units.
pub const SIGHT_RADIUS_UNITS: f32 = 4096.0;
/// Sight radius in meters (`4096 · 0.025`, D9).
pub const SIGHT_RADIUS_M: f32 = SIGHT_RADIUS_UNITS * GLOBAL_SCALE;
/// Hearing radius in meters (`EnemySenses.HearingRadius` = 25).
pub const HEARING_RADIUS_M: f32 = 25.0;
/// Field of view in degrees (`EnemySenses.FieldOfView` = 180 ⇒ ±90°).
pub const FIELD_OF_VIEW_DEG: f32 = 180.0;
/// `EnemySenses.predictionInterval` (not the perception cadence).
pub const PREDICTION_INTERVAL: f32 = 0.0625;
/// `EnemyMotor.GiveUpTimer` in ticks (`:94`).
pub const GIVE_UP_TICKS: u32 = 200;
/// Give-up time in seconds (D8: `200 · 0.0625`).
pub const GIVE_UP_SECONDS: f32 = GIVE_UP_TICKS as f32 * CLASSIC_TICK;
/// `attack.MeleeDistance` — stop distance in meters (spec §4.2).
pub const MELEE_DISTANCE: f32 = 2.25;
/// `ClassicMeleeDistanceVsAI` — classic AI variant (spec §4.2).
pub const CLASSIC_MELEE_DISTANCE: f32 = 1.5;
/// Pursuit threshold factor (`EnemyMotor.cs:487`).
pub const PURSUE_FACTOR: f32 = 2.75;
/// Retreat threshold factor (`EnemyMotor.cs:506-507`).
pub const RETREAT_FACTOR: f32 = 0.8;
/// `EnemyMotor.OpenDoorDistance` in meters (`:34`).
pub const OPEN_DOOR_DISTANCE: f32 = 2.0;
/// `EnemyMotor.doorCrouchingHeight` in meters (`:37`) — consumed by the
/// door geometry (M34); it does not decide state on its own.
pub const DOOR_CROUCHING_HEIGHT: f32 = 1.65;
/// `avoidObstaclesTimer` in seconds (`EnemyMotor.cs:1136`).
pub const AVOID_OBSTACLES_TIMER: f32 = 0.75;
/// `attackSpeedDivisor` (`EnemyMotor.cs:35`): pursuit moves at half
/// speed (`moveSpeed /= attackSpeedDivisor`, `:440`).
pub const ATTACK_SPEED_DIVISOR: f32 = 2.0;
/// `LiveSpeed` floor at the attack gate (`EnemyAttack.cs:53`).
pub const SPEED_FLOOR: i32 = 8;
/// `classicFrameUpdate` cadence divisor (`FormulaHelper.cs:55`).
pub const CLASSIC_FRAME_UPDATE: f32 = 980.0;
/// Class HP base (`FormulaHelper.cs:1735`).
pub const CLASS_HEALTH_BASE: i32 = 10;
/// `moveInForAttackTimer` — `Random.Range(1, 3)`, `int` overload ⇒ `{1,2}` s.
pub const MOVE_IN_FOR_ATTACK_MIN: f32 = 1.0;
/// Exclusive upper bound of `moveInForAttackTimer` (`:1276`).
pub const MOVE_IN_FOR_ATTACK_MAX_EXCLUSIVE: i32 = 3;
/// `changeStateTimer` — `Random.Range(0.2f, .8f)` (`:1322-1323`).
pub const CHANGE_STATE_TIMER: (f32, f32) = (0.2, 0.8);
/// `strafeTimer` — `Random.Range(1f, 2f)` (`:642`).
pub const STRAFE_TIMER: (f32, f32) = (1.0, 2.0);
/// Domain of the pacification roll (`Random.Range(0, 200)`, §6).
pub const PACIFICATION_ROLL_DOMAIN: i32 = 200;
/// Pacification adjustment with the weapon sheathed (`:375`).
pub const PACIFICATION_SHEATHED_BONUS: i32 = 10;
/// Pacification adjustment with the weapon drawn (`:375`).
pub const PACIFICATION_DRAWN_PENALTY: i32 = -25;

/// Movement speed in m/s (D1): `(LiveSpeed + 150) · 0.025`.
pub fn move_speed(live_speed: i32) -> f32 {
    (live_speed as f32 + DF_WALK_BASE) * GLOBAL_SCALE
}

/// Pursuit speed in m/s (D2): `moveSpeed / attackSpeedDivisor`.
pub fn attack_move_speed(live_speed: i32) -> f32 {
    move_speed(live_speed) / ATTACK_SPEED_DIVISOR
}

/// `LiveSpeed` with the `speedFloor` floor from `EnemyAttack` (D5).
pub fn speed_floor(live_speed: i32) -> i32 {
    live_speed.max(SPEED_FLOOR)
}

/// Attack gate (D3/D5): `(speed >> 3) + 6` over the floor.
///
/// It fires if `rand() % speed >= gate` (`EnemyAttack.cs:81`), so that
/// `P(no attack) = gate / speed` (D4: 11/40; D5: 7/8).
pub fn attack_gate(live_speed: i32) -> i32 {
    (speed_floor(live_speed) >> 3) + 6
}

/// Attack cadence in seconds (D6/D7/D12/D13).
///
/// Actual order of `EnemyAttack.ResetMeleeTimer` (`:116-131`): **add** the
/// roll, **subtract** `50·(level − 10)`, **add** `450·(reflexes − 2)`,
/// **clamp to 0** and only then **divide by 980**.
///
/// `reflexes ∈ {VeryHigh 0 … VeryLow 4}` (`EntityEnums.cs:171-178`): high
/// values in the config **add** time.
pub fn melee_timer_seconds(roll: i32, level: i32, reflexes: i32) -> f32 {
    let raw = roll as f32 - 50.0 * (level as f32 - 10.0) + 450.0 * (reflexes as f32 - 2.0);
    raw.max(0.0) / CLASSIC_FRAME_UPDATE
}

/// Pursuit threshold in meters: `stopDistance · 2.75` (6.1875 by default).
pub fn pursue_threshold(stop_distance: f32) -> f32 {
    stop_distance * PURSUE_FACTOR
}

/// Retreat threshold in meters: `stopDistance · 0.8` (1.8 by default).
pub fn retreat_threshold(stop_distance: f32) -> f32 {
    stop_distance * RETREAT_FACTOR
}

/// Class HP interval for a level (spec §3.1, C1-C6).
///
/// `10 + L·[1, max(hppl, 1)]`: `Range(1, hppl + 1)` with the `int` overload
/// and `min == max` returns `min`, so `hppl = 0` degenerates to 1 per level
/// (C6 = `[15, 15]` at level 5), never to "no die".
pub fn class_max_health_bounds(level: i32, hppl: i32) -> (i32, i32) {
    let level = level.max(0);
    let per_level = hppl.max(1);
    (
        CLASS_HEALTH_BASE + level,
        CLASS_HEALTH_BASE + level * per_level,
    )
}

/// Rolls the maximum class HP within the interval of
/// [`class_max_health_bounds`] (`RollEnemyClassMaxHealth`,
/// `FormulaHelper.cs:1729-1743`).
pub fn roll_class_max_health(
    level: i32,
    hppl: i32,
    mut range: impl FnMut(i32, i32) -> i32,
) -> i32 {
    let per_level = hppl.max(1);
    let mut health = CLASS_HEALTH_BASE;
    for _ in 0..level.max(0) {
        // `Random.Range(1, hppl + 1)` is max-**exclusive**; the closure takes
        // an inclusive interval ⇒ `[1, hppl]` (same as §3.1/`bounds`).
        health += range(1, per_level);
    }
    health
}

/// Raw pacification `chance` (spec §6, `FormulaHelper.cs:357-391`).
///
/// `skill/10 + Personality/5` if the skill is Etiquette or Streetwise; else
/// `skill + Personality/10`; `+10` sheathed, `−25` drawn; `+` the
/// `ChanceValue()` of the *Comprehend Languages* effect if it is active.
pub fn pacification_chance(
    skill: i32,
    social_skill: bool,
    personality: i32,
    sheathed: bool,
    comprehend_languages: i32,
) -> i32 {
    let base = if social_skill {
        skill / 10 + personality / 5
    } else {
        skill + personality / 10
    };
    let weapon = if sheathed {
        PACIFICATION_SHEATHED_BONUS
    } else {
        PACIFICATION_DRAWN_PENALTY
    };
    base + weapon + comprehend_languages
}

/// Normalized probability of pacifying (F1-F5): `clamp(chance, 0, 200)/200`.
///
/// The roll is `Random.Range(0, 200)` with success `roll < chance`, so the
/// domain is literally `0..199` (§6: the normalization is **not**
/// `[INFERENCE]`).
pub fn pacification_probability(chance: i32) -> f64 {
    f64::from(chance.clamp(0, PACIFICATION_ROLL_DOMAIN)) / f64::from(PACIFICATION_ROLL_DOMAIN)
}

/// Does it pacify the target with the given roll? (`roll < chance`, §6).
pub fn pacifies(chance: i32, roll: i32) -> bool {
    roll < chance.clamp(0, PACIFICATION_ROLL_DOMAIN)
}

/// Observable states of spec §4.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiState {
    /// No target.
    Idle,
    /// Target visible/audible: closes in at half speed.
    Pursue,
    /// At stop distance (`≤ 2.25 m`).
    Attack,
    /// Too close (`< 1.8 m`): backs off.
    Retreat,
    /// Closed door on the route at `≤ 2 m`.
    OpenDoor,
    /// Low HP or quest flight — triggered by M22/M29, not by this machine.
    Flee,
    /// Levitate effect active (M23) — body state, not an AI decision.
    Levitating,
}

impl AiState {
    /// Stable name for raw evidence (`Idle`, `Pursue`, …).
    pub const fn label(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Pursue => "Pursue",
            Self::Attack => "Attack",
            Self::Retreat => "Retreat",
            Self::OpenDoor => "OpenDoor",
            Self::Flee => "Flee",
            Self::Levitating => "Levitating",
        }
    }
}

/// Transition or measurable fact emitted by one AI tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AiEvent {
    /// State change with its absolute instant.
    Transition {
        /// Instant in seconds since the motor was created.
        at: f32,
        /// Origin state.
        from: AiState,
        /// Destination state.
        to: AiState,
    },
    /// First contact: stop distance reached.
    Contact {
        /// Instant in seconds.
        at: f32,
        /// Observed distance (meters).
        distance: f32,
    },
    /// Door opened on the route.
    DoorOpened {
        /// Instant in seconds.
        at: f32,
        /// Distance to the door (meters), `≤ 2.0`.
        distance: f32,
    },
}

/// Perception of one `ClassicUpdate` (supplied by the world motor / the probe).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Perception {
    /// Distance to the player in meters.
    pub distance: f32,
    /// Angle relative to the front (degrees, `0` = head-on).
    pub angle_deg: f32,
    /// Is there an obstacle cutting the line of sight?
    pub blocked: bool,
}

impl Perception {
    /// Head-on perception with no obstacle at the given distance.
    pub fn visible(distance: f32) -> Self {
        Self {
            distance,
            angle_deg: 0.0,
            blocked: false,
        }
    }

    /// Out-of-range perception (the world delivers no target).
    pub fn lost() -> Self {
        Self {
            distance: f32::INFINITY,
            angle_deg: 0.0,
            blocked: true,
        }
    }
}

/// Door seen by the AI (the world opens it; the AI only decides to open it).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Door {
    /// Distance to the door in meters.
    pub distance: f32,
    /// Is it already open?
    pub open: bool,
}

/// Enemy senses (spec §4.1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnemySenses {
    /// Sight radius in meters (102.4).
    pub sight_radius: f32,
    /// Hearing radius in meters (25).
    pub hearing_radius: f32,
    /// Total field of view in degrees (180 ⇒ ±90°).
    pub fov_deg: f32,
}

impl Default for EnemySenses {
    fn default() -> Self {
        Self {
            sight_radius: SIGHT_RADIUS_M,
            hearing_radius: HEARING_RADIUS_M,
            fov_deg: FIELD_OF_VIEW_DEG,
        }
    }
}

impl EnemySenses {
    /// Does it detect the target in this perception?
    ///
    /// Sight: `d ≤ SightRadius ∧ |angle| ≤ FOV/2 ∧ no obstacle`;
    /// hearing: `d ≤ HearingRadius` (no cone) when there is an obstacle or
    /// the target is outside the cone.
    pub fn detects(&self, perception: &Perception) -> bool {
        let in_cone = perception.angle_deg.abs() <= self.fov_deg / 2.0;
        if !perception.blocked && perception.distance <= self.sight_radius && in_cone {
            return true;
        }
        perception.distance <= self.hearing_radius
    }
}

/// Enemy AI state machine (§4.3) driven by `ClassicUpdate`.
#[derive(Debug, Clone)]
pub struct EnemyAi {
    /// Current state.
    pub state: AiState,
    /// Senses used per tick.
    pub senses: EnemySenses,
    /// Stop distance (2.25 m; 1.5 m in classic AI).
    pub stop_distance: f32,
    /// Pursuit speed (m/s) = `move_speed · 0.5`.
    pub attack_speed: f32,
    /// Retreat speed (m/s) = full `move_speed`.
    pub retreat_speed: f32,
    /// Pursuit threshold (`stop · 2.75`).
    pub pursue_distance: f32,
    /// Retreat threshold (`stop · 0.8`).
    pub retreat_distance: f32,
    /// Remaining `GiveUpTimer` ticks.
    pub give_up_ticks: u32,
    /// `GiveUpTimer` reset ticks (200).
    pub give_up_ticks_max: u32,
    /// Last known distance to the target (meters).
    pub distance: f32,
    /// Simulation clock in seconds.
    pub elapsed: f32,
    /// Perceptions evaluated (one per `ClassicUpdate`).
    pub perceptions: u64,
    /// Doors opened by this AI.
    pub doors_opened: u32,
    /// Accumulated measurable facts (transitions, contacts, doors).
    pub events: Vec<AiEvent>,
}

impl EnemyAi {
    /// AI for a given `LiveSpeed` with the melee `stopDistance` (2.25 m).
    pub fn new(live_speed: i32) -> Self {
        Self::with_stop_distance(live_speed, MELEE_DISTANCE)
    }

    /// AI with an explicit stop distance (1.5 m = classic variant).
    pub fn with_stop_distance(live_speed: i32, stop_distance: f32) -> Self {
        Self {
            state: AiState::Idle,
            senses: EnemySenses::default(),
            stop_distance,
            attack_speed: attack_move_speed(live_speed),
            retreat_speed: move_speed(live_speed),
            pursue_distance: pursue_threshold(stop_distance),
            retreat_distance: retreat_threshold(stop_distance),
            give_up_ticks: GIVE_UP_TICKS,
            give_up_ticks_max: GIVE_UP_TICKS,
            distance: f32::INFINITY,
            elapsed: 0.0,
            perceptions: 0,
            doors_opened: 0,
            events: Vec::new(),
        }
    }

    fn transition(&mut self, to: AiState, events: &mut Vec<AiEvent>) {
        if self.state == to {
            return;
        }
        events.push(AiEvent::Transition {
            at: self.elapsed,
            from: self.state,
            to,
        });
        self.state = to;
    }

    /// Advances one `ClassicUpdate` (0.0625 s) with the tick's perception.
    ///
    /// The machine **decides**: the distance is the one the world measures in
    /// this tick, and the driver integrates physical advance with
    /// [`Self::close_distance`] / [`Self::open_distance`] (0 duplication between
    /// probe, demo and window).
    ///
    /// Returns the facts of **this** tick and accumulates them in [`Self::events`].
    pub fn classic_update(
        &mut self,
        perception: Perception,
        door: Option<Door>,
    ) -> Vec<AiEvent> {
        self.elapsed += CLASSIC_TICK;
        self.perceptions += 1;
        let detected = self.senses.detects(&perception);
        let mut events = Vec::new();

        if detected {
            self.distance = perception.distance;
            self.give_up_ticks = self.give_up_ticks_max;
        }

        // 1 · entry from Idle: at stop distance it already attacks; otherwise
        // it pursues (§4.3: pursuit fires on seeing the player).
        if self.state == AiState::Idle && detected {
            if self.distance <= self.stop_distance {
                events.push(AiEvent::Contact {
                    at: self.elapsed,
                    distance: self.distance,
                });
                self.transition(AiState::Attack, &mut events);
            } else {
                self.transition(AiState::Pursue, &mut events);
            }
        }

        // 2 · OpenDoor returns to pursuit when the world opens the door.
        if self.state == AiState::OpenDoor && door.map(|d| d.open).unwrap_or(true) {
            self.transition(AiState::Pursue, &mut events);
        }

        // 3 · closed door on the route (only while pursuing and with a target).
        if self.state == AiState::Pursue && detected {
            if let Some(door) = door.filter(|d| !d.open && d.distance <= OPEN_DOOR_DISTANCE) {
                self.doors_opened += 1;
                events.push(AiEvent::DoorOpened {
                    at: self.elapsed,
                    distance: door.distance,
                });
                self.transition(AiState::OpenDoor, &mut events);
            }
        }

        // 4 · motor thresholds (`:446-507`) over the measured distance.
        if detected {
            match self.state {
                AiState::Pursue => {
                    if self.distance <= self.retreat_distance {
                        self.transition(AiState::Retreat, &mut events);
                    } else if self.distance <= self.stop_distance {
                        events.push(AiEvent::Contact {
                            at: self.elapsed,
                            distance: self.distance,
                        });
                        self.transition(AiState::Attack, &mut events);
                    }
                }
                AiState::Attack => {
                    if self.distance > self.pursue_distance {
                        self.transition(AiState::Pursue, &mut events);
                    } else if self.distance < self.retreat_distance {
                        self.transition(AiState::Retreat, &mut events);
                    }
                }
                AiState::Retreat if self.distance >= self.pursue_distance => {
                    self.transition(AiState::Pursue, &mut events)
                }
                _ => {}
            }
        }

        // 5 · `GiveUpTimer`: 1 tick per `ClassicUpdate` without a target.
        if !detected
            && matches!(
                self.state,
                AiState::Pursue | AiState::Attack | AiState::Retreat | AiState::OpenDoor
            )
            && self.give_up_ticks > 0
        {
            self.give_up_ticks -= 1;
            if self.give_up_ticks == 0 {
                self.transition(AiState::Idle, &mut events);
            }
        }

        self.events.extend(events.iter().copied());
        events
    }

    /// Pursuit motor: brings the distance toward `stopDistance` at the
    /// attack speed (`moveSpeed / 2`) and **clamps** at the stop
    /// distance (`EnemyMotor.cs:446-448`).
    pub fn close_distance(&self, distance: f32, dt: f32) -> f32 {
        (distance - self.attack_speed * dt).max(self.stop_distance)
    }

    /// Retreat motor: increases the distance at full speed up to the
    /// pursuit threshold (`stopDistance · 2.75`).
    pub fn open_distance(&self, distance: f32, dt: f32) -> f32 {
        (distance + self.retreat_speed * dt).min(self.pursue_distance)
    }

    /// Desired speed (m/s) for the current state: positive closes in, negative
    /// backs off, `0` stands still. Consumed by the driver (ECS / probe / demo).
    pub fn desired_speed(&self) -> f32 {
        match self.state {
            AiState::Pursue => self.attack_speed,
            AiState::Retreat => -self.retreat_speed,
            AiState::Idle | AiState::Attack | AiState::OpenDoor => 0.0,
            AiState::Flee => self.retreat_speed,
            AiState::Levitating => 0.0,
        }
    }

    /// Instant of the first `from → to` transition (if it happened).
    pub fn first_transition(&self, from: AiState, to: AiState) -> Option<f32> {
        self.events.iter().find_map(|event| match event {
            AiEvent::Transition { at, from: f, to: t } if *f == from && *t == to => Some(*at),
            _ => None,
        })
    }

    /// Distance of the first recorded contact (if any).
    pub fn first_contact(&self) -> Option<(f32, f32)> {
        self.events.iter().find_map(|event| match event {
            AiEvent::Contact { at, distance } => Some((*at, *distance)),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-6;

    #[test]
    fn speeds_match_the_spec_goldens() {
        assert!((move_speed(40) - 4.75).abs() < EPS);
        assert!((move_speed(100) - 6.25).abs() < EPS);
        assert!((move_speed(0) - 3.75).abs() < EPS);
        assert!((attack_move_speed(40) - 2.375).abs() < EPS);
        assert!((SIGHT_RADIUS_M - 102.4).abs() < EPS);
        assert!((GIVE_UP_SECONDS - 12.5).abs() < EPS);
        assert!((pursue_threshold(MELEE_DISTANCE) - 6.1875).abs() < EPS);
        assert!((retreat_threshold(MELEE_DISTANCE) - 1.8).abs() < EPS);
    }

    #[test]
    fn attack_gate_and_floor_match_the_spec_goldens() {
        assert_eq!(attack_gate(40), 11);
        assert_eq!(attack_gate(0), 7);
        assert_eq!(speed_floor(0), SPEED_FLOOR);
        // P(no attack) = gate/speed ⇒ 11/40 and 7/8.
        assert!((11.0f32 / 40.0 - 0.275).abs() < EPS);
        assert!((7.0f32 / 8.0 - 0.875).abs() < EPS);
    }

    #[test]
    fn melee_timer_matches_the_spec_goldens() {
        assert!((melee_timer_seconds(2400, 10, 2) - 2.448_979_6).abs() < 1e-4);
        assert!((melee_timer_seconds(2400, 20, 2) - 1.938_775_5).abs() < 1e-4);
        assert!((melee_timer_seconds(1500, 0, 0) - 1.122_449).abs() < 1e-4);
        assert!((melee_timer_seconds(3000, 20, 4) - 3.469_387_8).abs() < 1e-4);
        // The clamp to 0 happens before dividing.
        assert_eq!(melee_timer_seconds(1500, 90, 0), 0.0);
    }

    #[test]
    fn class_health_bounds_cover_the_goldens() {
        assert_eq!(class_max_health_bounds(1, 1), (11, 11));
        assert_eq!(class_max_health_bounds(10, 1), (20, 20));
        assert_eq!(class_max_health_bounds(1, 17), (11, 27));
        assert_eq!(class_max_health_bounds(5, 17), (15, 95));
        assert_eq!(class_max_health_bounds(10, 17), (20, 180));
        assert_eq!(class_max_health_bounds(5, 0), (15, 15));
        // Degenerate roll: hppl 1 adds 1 per level.
        let mut calls = 0;
        let total = roll_class_max_health(10, 1, |low, high| {
            calls += 1;
            assert_eq!((low, high), (1, 1));
            low
        });
        assert_eq!(total, 20);
        assert_eq!(calls, 10);
        // The roll's ceiling never exceeds the one from `class_max_health_bounds`.
        let (_, upper) = class_max_health_bounds(7, 2);
        let ceiling = roll_class_max_health(7, 2, |_, high| high);
        assert_eq!(ceiling, upper);
        let floor = roll_class_max_health(7, 2, |low, _| low);
        assert_eq!(floor, class_max_health_bounds(7, 2).0);
    }

    #[test]
    fn pacification_normalizes_over_200() {
        assert!((pacification_probability(26) - 0.13).abs() < 1e-12);
        assert!((pacification_probability(100) - 0.5).abs() < 1e-12);
        assert_eq!(pacification_probability(200), 1.0);
        assert_eq!(pacification_probability(250), 1.0);
        assert_eq!(pacification_probability(-9), 0.0);
        // F6: sheathed > drawn except chance = 225 (both saturate).
        for chance in [0, 26, 100, 200, 250] {
            let sheathed = chance + PACIFICATION_SHEATHED_BONUS;
            let drawn = chance + PACIFICATION_DRAWN_PENALTY;
            assert!(pacification_probability(sheathed) >= pacification_probability(drawn));
        }
        assert_eq!(
            pacification_probability(225 + PACIFICATION_SHEATHED_BONUS),
            pacification_probability(225 + PACIFICATION_DRAWN_PENALTY)
        );
        // F7: monotonic in the skill (same personality).
        assert!(
            pacification_probability(pacification_chance(80, false, 50, true, 0))
                >= pacification_probability(pacification_chance(20, false, 50, true, 0))
        );
        // Source coefficients: social divides, non-social does not.
        assert_eq!(pacification_chance(50, true, 50, true, 0), 5 + 10 + 10);
        assert_eq!(pacification_chance(50, false, 50, false, 0), 50 + 5 - 25);
        assert!(pacifies(26, 25) && !pacifies(26, 26));
    }

    /// Player 20 m head-on: `Idle→Pursue` in ≤ 0.0625 s and first
    /// contact exactly at 2.25 m (the driver integrates the advance).
    #[test]
    fn acquire_and_contact_the_spec_scenario() {
        let mut ai = EnemyAi::new(40);
        let mut distance = 20.0f32;
        let mut events = ai.classic_update(Perception::visible(distance), None);
        assert!(matches!(
            events.first(),
            Some(AiEvent::Transition {
                at,
                from: AiState::Idle,
                to: AiState::Pursue
            }) if (*at - CLASSIC_TICK).abs() < EPS
        ));
        assert!((ai.elapsed - CLASSIC_TICK).abs() < EPS);
        let mut ticks = 1;
        while ai.state == AiState::Pursue && ticks < 1000 {
            distance = ai.close_distance(distance, CLASSIC_TICK);
            events.extend(ai.classic_update(Perception::visible(distance), None));
            ticks += 1;
        }
        let (at, contact) = ai.first_contact().expect("contact");
        assert!((contact - MELEE_DISTANCE).abs() < EPS);
        assert_eq!(ai.state, AiState::Attack);
        assert_eq!(ai.desired_speed(), 0.0);
        println!("contact at {at:.4} s after {ticks} ticks · d = {contact}");
    }

    /// Target lost: `Pursue→Idle` exactly 200 perceptions after the
    /// last detection (12.5 s).
    #[test]
    fn give_up_is_exactly_12_5_seconds() {
        let mut ai = EnemyAi::new(40);
        let mut distance = 20.0f32;
        ai.classic_update(Perception::visible(distance), None);
        // Detection until tick 10; from there on, lost.
        for _ in 0..9 {
            distance = ai.close_distance(distance, CLASSIC_TICK);
            ai.classic_update(Perception::visible(distance), None);
        }
        assert_eq!(ai.state, AiState::Pursue);
        let last_detection = ai.elapsed;
        let mark = ai.perceptions;
        while ai.state != AiState::Idle {
            ai.classic_update(Perception::lost(), None);
        }
        let at = ai
            .first_transition(AiState::Pursue, AiState::Idle)
            .expect("Pursue→Idle");
        assert!((at - (last_detection + GIVE_UP_SECONDS)).abs() < EPS);
        assert!((at - last_detection - 12.5).abs() < EPS);
        assert_eq!(ai.perceptions - mark, 200);
        println!("Pursue→Idle at {at:.4} s (last detection {last_detection:.4} s) · perceptions=200");
    }

    /// Closed door at ≤ 2 m on the route: `OpenDoor` + `doors_opened = 1`;
    /// pursuit resumes once it opens.
    #[test]
    fn closed_door_triggers_open_door_within_two_meters() {
        let mut ai = EnemyAi::new(40);
        let mut distance = 20.0f32;
        ai.classic_update(Perception::visible(distance), None);
        let mut opened_at = None;
        let mut door_distance = 10.0f32;
        for _ in 0..400 {
            distance = ai.close_distance(distance, CLASSIC_TICK);
            door_distance = (door_distance - ai.attack_speed * CLASSIC_TICK).max(0.0);
            let events = ai.classic_update(
                Perception::visible(distance),
                Some(Door {
                    distance: door_distance,
                    open: false,
                }),
            );
            if let Some(AiEvent::DoorOpened { at, distance }) = events
                .iter()
                .find(|event| matches!(event, AiEvent::DoorOpened { .. }))
            {
                opened_at = Some((*at, *distance));
                break;
            }
        }
        let (at, door_distance) = opened_at.expect("DoorOpened");
        assert!(door_distance <= OPEN_DOOR_DISTANCE);
        assert_eq!(ai.doors_opened, 1);
        assert_eq!(ai.state, AiState::OpenDoor);
        println!("door opened at {at:.4} s · d = {door_distance:.4}");
        ai.classic_update(
            Perception::visible(distance),
            Some(Door {
                distance: door_distance,
                open: true,
            }),
        );
        assert_eq!(ai.state, AiState::Pursue);
    }

    #[test]
    fn senses_cone_and_hearing() {
        let senses = EnemySenses::default();
        assert!(senses.detects(&Perception::visible(20.0)));
        assert!(!senses.detects(&Perception::visible(200.0)));
        // Outside the 180° cone and beyond hearing (25 m).
        assert!(!senses.detects(&Perception {
            distance: 30.0,
            angle_deg: 120.0,
            blocked: false,
        }));
        // Hearing: no sight but at ≤ 25 m (radius, no cone).
        assert!(senses.detects(&Perception {
            distance: 24.0,
            angle_deg: 179.0,
            blocked: true,
        }));
        assert!(!senses.detects(&Perception {
            distance: 25.1,
            angle_deg: 0.0,
            blocked: true,
        }));
    }

    #[test]
    fn retreat_triggers_below_the_retreat_threshold() {
        let mut ai = EnemyAi::new(40);
        ai.classic_update(Perception::visible(7.0), None);
        assert_eq!(ai.state, AiState::Pursue);
        let events = ai.classic_update(Perception::visible(1.7), None);
        assert!(events.iter().any(|event| matches!(
            event,
            AiEvent::Transition {
                to: AiState::Retreat,
                ..
            }
        )));
    }
}

//! `death` — the player death state machine (M32, REQ-L99-065 · spec §6).
//!
//! **Source (Mode A, clean-room transcription):**
//! `PlayerEntity.cs:1200-1214` (HP ≤ 0 → guild `AvoidDeath`, else the death
//! event), `Temple.cs:450-460` (Stendarr), `PlayerDeath.cs:29-33,119-162`
//! (fall/fade/reset).
//!
//! **Declared NEGATIVE:** the source has no revival path — death ends the
//! session and the classic recovery is loading a save. This module therefore
//! exposes **no** state transition from [`DeathState`] back to a live entity;
//! the probe measures that absence over `crates/` (spec §6, §8).
//!
//! 0 Bevy, 0 persistence, 0 retail bytes.

/// Camera fall speed while dying (`PlayerDeath.cs:29`).
pub const DEATH_FALL_SPEED: f64 = 2.5;
/// Fade-to-black duration (`:32`).
pub const DEATH_FADE_SECONDS: f64 = 2.0;
/// Seconds before the game returns to the title menu (`:33`).
pub const DEATH_RESET_SECONDS: f64 = 3.0;
/// Health left by the guild `AvoidDeath` benefit (`PlayerEntity.cs:1209`):
/// `(int)(MaxHealth * 0.1f)`.
pub const AVOID_DEATH_HEALTH_PERCENT: f64 = 0.1;
/// `UnityEngine.Random.Range(0, 50)` upper bound (exclusive) of the Stendarr
/// roll (`Temple.cs:456`).
pub const STENDARR_ROLL_BOUND: i32 = 50;

/// Outcome of applying lethal damage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifeOutcome {
    /// Still alive with the given health.
    Alive(i32),
    /// Would have died, but the guild benefit left `MaxHealth / 10`.
    Avoided(i32),
    /// The death event fired (`RaiseOnDeathEvent`).
    Dead,
}

impl LifeOutcome {
    /// Health after the hit, when there is one.
    pub fn health(self) -> Option<i32> {
        match self {
            LifeOutcome::Alive(health) | LifeOutcome::Avoided(health) => Some(health),
            LifeOutcome::Dead => None,
        }
    }

    /// `true` when the death event fired.
    pub fn is_dead(self) -> bool {
        self == LifeOutcome::Dead
    }
}

/// `DaggerfallEntity.SetHealth` (`PlayerEntity.cs:1195-1214`) for the player.
///
/// The new health is clamped to `0..=max_health`; when it reaches zero the
/// guild benefit is consulted **before** the death event.
pub fn apply_damage(health: i32, max_health: i32, damage: i32, avoid_death: bool) -> LifeOutcome {
    let max_health = max_health.max(1);
    let current = (health - damage.max(0)).clamp(0, max_health);
    if current > 0 {
        return LifeOutcome::Alive(current);
    }
    if avoid_death {
        return LifeOutcome::Avoided(avoid_death_health(max_health));
    }
    LifeOutcome::Dead
}

/// `(int)(MaxHealth * 0.1f)` — integer truncation, never below 1.
pub fn avoid_death_health(max_health: i32) -> i32 {
    ((max_health.max(1) as f64) * AVOID_DEATH_HEALTH_PERCENT) as i32
}

/// `Temple.AvoidDeath` (`Temple.cs:450-460`): only Stendarr, only while not
/// submerged, and only when `Range(0, 50) < rank`.
pub fn stendarr_avoids_death(rank: i32, roll: i32, submerged: bool) -> bool {
    !submerged && (0..STENDARR_ROLL_BOUND).contains(&roll) && roll < rank
}

/// Runtime death animation state (`PlayerDeath`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeathState {
    /// `deathInProgress`.
    pub in_progress: bool,
    /// `timeOfDeath` (fixed time in seconds).
    pub time_of_death: f64,
    /// Camera start height (for the fall animation).
    pub start_camera_height: f64,
}

impl DeathState {
    /// Not dead.
    pub const fn alive() -> Self {
        Self {
            in_progress: false,
            time_of_death: 0.0,
            start_camera_height: 0.0,
        }
    }

    /// `PlayerEntity_OnDeath` (`:142-162`): latches, pauses input and stores
    /// the camera height.
    pub fn begin(&mut self, now_seconds: f64, camera_height: f64) {
        self.in_progress = true;
        self.time_of_death = now_seconds;
        self.start_camera_height = camera_height;
    }

    /// `:92-100`: after `TimeBeforeReset` the game returns to the title menu.
    pub fn reset_due(&self, now_seconds: f64) -> bool {
        self.in_progress && now_seconds - self.time_of_death > DEATH_RESET_SECONDS
    }

    /// `ClearDeathAnimation` (`:119-128`): clears the latch after the reset.
    pub fn clear(&mut self) {
        self.in_progress = false;
        self.start_camera_height = 0.0;
    }

    /// `true` when input is paused by the death animation (`:156`).
    pub fn input_paused(&self) -> bool {
        self.in_progress
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lethal_damage_death_event_without_guild_benefit() {
        // X1
        assert_eq!(apply_damage(10, 100, 10, false), LifeOutcome::Dead);
        assert_eq!(apply_damage(10, 100, 50, false), LifeOutcome::Dead);
        assert_eq!(apply_damage(10, 100, 9, false), LifeOutcome::Alive(1));
    }

    #[test]
    fn guild_benefit_leaves_ten_percent() {
        // X2
        assert_eq!(apply_damage(10, 100, 10, true), LifeOutcome::Avoided(10));
        assert_eq!(avoid_death_health(1), 0);
        assert_eq!(apply_damage(0, 1, 1, true), LifeOutcome::Avoided(0));
    }

    #[test]
    fn stendarr_roll_is_exclusive_and_ranked() {
        // V8
        assert!(stendarr_avoids_death(10, 9, false));
        assert!(!stendarr_avoids_death(10, 10, false));
        assert!(!stendarr_avoids_death(10, 9, true));
        assert!(!stendarr_avoids_death(10, 50, false));
        assert!(!stendarr_avoids_death(0, 0, false));
    }

    #[test]
    fn death_state_latches_and_resets_after_three_seconds() {
        let mut death = DeathState::alive();
        assert!(!death.input_paused());
        death.begin(100.0, 1.75);
        assert!(death.input_paused());
        assert!(!death.reset_due(102.9));
        assert!(death.reset_due(103.1));
        death.clear();
        assert!(!death.in_progress);
        assert!(!death.reset_due(200.0));
    }
}

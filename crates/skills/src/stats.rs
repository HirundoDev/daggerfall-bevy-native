//! `stats` — player RPG statistics (M7).
//!
//! **Declared minimal contract** (M7 C3 pack — "create it additively with a
//! declared minimal contract"): the source of the PD4 T7 HUD read model
//! ("HUD (health/stamina/level) → projection from **RPGStats**", anchor
//! REQ-L99-038). `RpgStats` IS that RPGStats with the code's snake_case
//! name; the fields are the anchor's required triple (health, stamina,
//! level) + maxima and experience.
//!
//! Rules of the 🎮 Skills organ (bevy-bridge-matrix): deterministic gameplay
//! · **0 Bevy** · **0 persistence** (save/load is ❤️/🧠, M8) · 0 chance
//! (INV-L99-3: `outcome = rules(stats)`). Full per-use progression
//! (`PROBE-PROGRESSION-001`, RPG loop) is later scope; here only what M7
//! projects and mutates: `damage`/`rest` (clamped) and `gain_experience`
//! with a documented integer threshold.

/// Player RPG statistics in session (source of the HUD read model).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpgStats {
    /// Character level (≥ 1).
    pub level: i32,
    /// Experience accumulated within the current level (≥ 0).
    pub experience: i32,
    /// Current health (0..=max_health — 0 = unconscious/downed).
    pub health: i32,
    /// Maximum health (≥ 1).
    pub max_health: i32,
    /// Current stamina (0..=max_stamina).
    pub stamina: i32,
    /// Maximum stamina (≥ 1).
    pub max_stamina: i32,
}

/// DFU level formula offset (`FormulaHelper.cs:330-337`).
pub const LEVEL_OFFSET: i32 = 28;
/// DFU level formula divisor (`FormulaHelper.cs:330-337`).
pub const LEVEL_DIVISOR: i32 = 15;

impl RpgStats {
    /// Starting statistics with the M7 triple and 0 experience.
    pub fn new(level: i32, health: i32, stamina: i32) -> Self {
        Self {
            level: level.max(1),
            experience: 0,
            health: health.max(0),
            max_health: health.max(1),
            stamina: stamina.max(0),
            max_stamina: stamina.max(1),
        }
    }

    /// Applies clamped damage: health never drops below 0 (integers — NFR
    /// NUMERIC-never-FLOAT). `amount <= 0` is a no-op.
    pub fn damage(&mut self, amount: i32) {
        if amount > 0 {
            self.health = (self.health - amount).max(0);
        }
    }

    /// Rests: stamina restored and clamped to the maximum (`amount <= 0` =
    /// no-op).
    pub fn rest_stamina(&mut self, amount: i32) {
        if amount > 0 {
            self.stamina = (self.stamina + amount).min(self.max_stamina);
        }
    }

    /// Restores health clamped to the maximum (`amount <= 0` = no-op).
    pub fn heal(&mut self, amount: i32) {
        if amount > 0 {
            self.health = (self.health + amount).min(self.max_health);
        }
    }

    /// Records skill progress and recomputes the level with the DFU rule
    /// `floor((progress + 28) / 15)` (`CalculatePlayerLevel`,
    /// `FormulaHelper.cs:330-337`). The level never decreases.
    ///
    /// M20: `experience` is the **accumulated progress of the skill sum**
    /// (`currentLevelUpSkillSum − startingLevelUpSkillSum`), not synthetic
    /// XP. Progression authority lives in [`crate::character::Character`];
    /// this view only mirrors the HUD/session.
    pub fn gain_level_progress(&mut self, amount: i32) {
        if amount <= 0 {
            return;
        }
        self.experience += amount;
        let computed = (self.experience + LEVEL_OFFSET) / LEVEL_DIVISOR;
        if computed > self.level {
            self.level = computed;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_new_clamps_and_defaults() {
        let stats = RpgStats::new(1, 100, 80);
        assert_eq!(stats.level, 1);
        assert_eq!((stats.health, stats.max_health), (100, 100));
        assert_eq!((stats.stamina, stats.max_stamina), (80, 80));
        assert_eq!(stats.experience, 0);
    }

    #[test]
    fn damage_clamps_at_zero_and_rest_at_max() {
        let mut stats = RpgStats::new(2, 100, 60);
        stats.damage(30);
        assert_eq!(stats.health, 70);
        stats.damage(1_000);
        assert_eq!(stats.health, 0);
        stats.damage(-5);
        assert_eq!(stats.health, 0);
        stats.heal(1_000);
        assert_eq!(stats.health, 100);
        stats.stamina = 0;
        stats.rest_stamina(20);
        assert_eq!(stats.stamina, 20);
        stats.rest_stamina(1_000);
        assert_eq!(stats.stamina, stats.max_stamina);
    }

    #[test]
    fn level_progress_uses_dfu_formula() {
        let mut stats = RpgStats::new(1, 100, 100);
        // (0 + 28) / 15 = 1 → still level 1.
        stats.gain_level_progress(0);
        assert_eq!((stats.level, stats.experience), (1, 0));
        stats.gain_level_progress(2);
        // (2 + 28) / 15 = 2.
        assert_eq!((stats.level, stats.experience), (2, 2));
        stats.gain_level_progress(13);
        // (15 + 28) / 15 = 2 → does not level up yet.
        assert_eq!((stats.level, stats.experience), (2, 15));
        stats.gain_level_progress(2);
        // (17 + 28) / 15 = 3.
        assert_eq!((stats.level, stats.experience), (3, 17));
        stats.gain_level_progress(100);
        // (117 + 28) / 15 = 9.
        assert_eq!((stats.level, stats.experience), (9, 117));
        stats.gain_level_progress(-10);
        assert_eq!((stats.level, stats.experience), (9, 117));
        // The level never goes backwards even with negative progress.
        let mut low = RpgStats::new(5, 100, 100);
        low.experience = -100;
        low.gain_level_progress(1);
        assert_eq!(low.level, 5);
    }
}

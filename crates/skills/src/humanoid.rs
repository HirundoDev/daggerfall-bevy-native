//! `humanoid` — career/level data of a hostile humanoid (M20 → M25).
//!
//! Closes residual R1 #4 of audit: M25 cannot build HP and
//! attributes of a humanoid without the career contract, which DFU builds in
//! `EnemyEntity.SetEnemyCareer`, branch `EntityTypes.EnemyClass`
//! (`EnemyEntity.cs:290-330`):
//!
//! ```csharp
//! careerIndex = mobileEnemy.ID - 128;                     // :292
//! career = GetClassCareerTemplate((ClassCareers)careerIndex);
//! stats.SetPermanentFromCareer(career);                   // :294
//! level = GameManager.Instance.PlayerEntity.Level;        // :299
//! if (careerIndex == (int)MobileTypes.Knight_CityWatch - 128)
//!     level += UnityEngine.Random.Range(3, 7);            // :300-301
//! maxHealth = FormulaHelper.RollEnemyClassMaxHealth(level, career.HitPointsPerLevel); // :303
//! short skillsLevel = (short)((level * 5) + 30);
//! if (skillsLevel > 100) skillsLevel = 100;               // :318-326
//! ```
//!
//! The HP formula is **not** duplicated here: it comes from
//! `daggerfall_behavior::enemy_ai` (`class_max_health_bounds`,
//! `roll_class_max_health`), the implementation M25 already uses.
//!
//! **Boundary:** `MobileEnemy.Level` — `[INFERENCE]` from
//! `DaggerfallUnityEnums.cs:206` (the mob table field) — is the level of
//! `EnemyMonster` only; the `EnemyClass` branch above **ignores** it and uses
//! the player level (+3..6 for the city watch). Monster levels are M25's
//! `level_range`/`roll_spawn`; this module exposes the class path.
//!
//! **Boundary:** DFU writes `i <= DaggerfallSkills.Count` (36 slots, the last
//! one ignoring `SetPermanentSkillValue`'s range check); the engine keeps the
//! 35 skills of `DFCareer.Skills`, so the loop fills `SKILL_COUNT` slots.

use daggerfall_behavior::enemy_ai::{class_max_health_bounds, roll_class_max_health};

use crate::career::Career;
use crate::character::{ATTRIBUTE_COUNT, SKILL_COUNT};
use crate::rng::Rng64;

/// Standard humanoid classes (`ClassCareers` 0..17, `EntityEnums.cs:51-72`).
pub const HUMAN_CLASS_COUNT: u8 = 18;

/// `Knight_CityWatch − 128`: `MobileTypes.Knight_CityWatch = 146`
/// (`DaggerfallUnityEnums.cs:206-208`), class index 18.
pub const CITY_WATCH_CLASS_ID: u8 = 18;

/// Skill value of an enemy for a level: `min(5·Level + 30, 100)`
/// (`EnemyEntity.cs:318-326`), applied to all 35 skills.
///
/// No lower clamp: the source only clamps the top, and `level` is already
/// `>= 1` on the path that builds a [`HumanoidProfile`].
pub fn enemy_skill_level(level: i32) -> i16 {
    (level * 5 + 30).min(100) as i16
}

/// Is this class index the city watch? (`EnemyEntity.cs:300`).
pub fn is_city_watch(class_id: u8) -> bool {
    class_id == CITY_WATCH_CLASS_ID
}

/// Level of a city guard: player level + `Random.Range(3, 7)` (`int` overload,
/// max exclusive ⇒ `+3..=6`, `EnemyEntity.cs:300-301`).
pub fn city_watch_level(player_level: i32, rng: &mut Rng64) -> i32 {
    player_level + rng.range_inclusive(3, 6)
}

/// Rejection of a class index outside the standard careers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HumanoidProfileError {
    /// `class_id` is not a standard humanoid class (`0..=18`).
    ///
    /// Not clamped on purpose: a silent clamp would build a Knight for an
    /// unknown index.
    ClassId(u8),
}

impl std::fmt::Display for HumanoidProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HumanoidProfileError::ClassId(class_id) => write!(
                f,
                "class id {class_id} outside 0..={CITY_WATCH_CLASS_ID} (standard careers)"
            ),
        }
    }
}

impl std::error::Error for HumanoidProfileError {}

/// Career/level data of a hostile humanoid (M20 → M25).
#[derive(Debug, Clone, PartialEq)]
pub struct HumanoidProfile {
    /// Class index = mobile id − 128 (`EnemyEntity.cs:292`).
    pub class_id: u8,
    /// Career name (`career.Name`, `EnemyEntity.cs:314`).
    pub name: String,
    /// Effective level (player level, +3..6 for the city watch).
    pub level: i32,
    /// Permanent attributes from the career (`SetPermanentFromCareer`).
    pub attributes: [i32; ATTRIBUTE_COUNT],
    /// 35 skills at [`enemy_skill_level`].
    pub skills: [i16; SKILL_COUNT],
    /// Maximum health (`RollEnemyClassMaxHealth`).
    pub max_health: i32,
    /// Career `HitPointsPerLevel` (input of the HP interval).
    pub hit_points_per_level: i32,
}

impl HumanoidProfile {
    /// Builds the profile of an `EnemyClass` from its career
    /// (`EnemyEntity.cs:290-330`), **without** the city-watch bonus: the
    /// caller adds it with [`city_watch_level`] before calling this function.
    ///
    /// `level` is clamped to `>= 1` (a level 0 entity is not in the source;
    /// the clamp keeps the HP interval sane). `class_id` outside
    /// `0..=18` is rejected: the caller must not get a silent Knight.
    pub fn from_career(
        class_id: u8,
        career: &Career,
        level: i32,
        rng: &mut Rng64,
    ) -> Result<Self, HumanoidProfileError> {
        Self::from_parts(
            class_id,
            career.name.clone(),
            career.attributes,
            career.hit_points_per_level,
            level,
            rng,
        )
    }

    /// Same profile from the raw `CLASSnn.CFG` fields — the path the M25
    /// enemy spawns use (ORDER M25-CLOSURE criterion 6).
    ///
    /// `from_career` delegates here so player and enemy draws share the one
    /// HP/level routine. The enemy path feeds the fields through
    /// [`crate::career::parse_enemy_class_template`] because the `EnemyClass`
    /// branch consumes only the name, the attributes and
    /// `career.HitPointsPerLevel` (`EnemyEntity.cs:294-303`), while the
    /// player-facing skill-group validation of [`Career::parse`] rejects the
    /// retail `CLASS18.CFG` (City Watch, `major3 = 35` — see the template's
    /// doc).
    pub fn from_parts(
        class_id: u8,
        name: String,
        attributes: [i32; ATTRIBUTE_COUNT],
        hit_points_per_level: i32,
        level: i32,
        rng: &mut Rng64,
    ) -> Result<Self, HumanoidProfileError> {
        if class_id > CITY_WATCH_CLASS_ID {
            return Err(HumanoidProfileError::ClassId(class_id));
        }
        let level = level.max(1);
        let max_health = roll_class_max_health(level, hit_points_per_level, |low, high| {
            rng.range_inclusive(low, high)
        });
        Ok(Self {
            class_id,
            name,
            level,
            attributes,
            skills: [enemy_skill_level(level); SKILL_COUNT],
            max_health,
            hit_points_per_level,
        })
    }

    /// DFU HP interval of the class at this level
    /// ([`class_max_health_bounds`], `FormulaHelper.cs:1729-1743`).
    pub fn health_bounds(&self) -> (i32, i32) {
        class_max_health_bounds(self.level, self.hit_points_per_level)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::career::Career;

    /// 74-byte synthetic career: attributes `60/50/55/50/50/40/45/50`, HPL 12,
    /// advancement ×1.00.
    fn test_career() -> Career {
        let mut bytes = vec![0u8; crate::career::CFG_BYTES];
        bytes[4..6].copy_from_slice(&0x0C00u16.to_le_bytes()); // SP× 1.5
        bytes[16..19].copy_from_slice(&[31, 29, 32]);
        bytes[19..22].copy_from_slice(&[22, 24, 25]);
        bytes[22..28].copy_from_slice(&[23, 26, 27, 28, 30, 33]);
        bytes[28..38].copy_from_slice(b"Spellsword");
        bytes[52..54].copy_from_slice(&12u16.to_le_bytes());
        bytes[54..58].copy_from_slice(&0x0001_0000u32.to_le_bytes());
        let attributes: [u16; 8] = [60, 50, 55, 50, 50, 40, 45, 50];
        for (index, value) in attributes.iter().enumerate() {
            bytes[58 + index * 2..60 + index * 2].copy_from_slice(&value.to_le_bytes());
        }
        Career::parse(&bytes).expect("synthetic Spellsword career")
    }

    #[test]
    fn enemy_skill_level_matches_the_source_goldens() {
        assert_eq!(enemy_skill_level(1), 35);
        assert_eq!(enemy_skill_level(10), 80);
        assert_eq!(enemy_skill_level(14), 100);
        assert_eq!(enemy_skill_level(20), 100);
        assert_eq!(enemy_skill_level(0), 30);
    }

    #[test]
    fn city_watch_is_class_eighteen() {
        assert_eq!(CITY_WATCH_CLASS_ID, 18);
        assert!(is_city_watch(18));
        assert!(!is_city_watch(17));
        assert!(!is_city_watch(0));
    }

    #[test]
    fn city_watch_level_stays_three_to_six_above_the_player() {
        let mut rng = Rng64::new(20_261_020);
        let mut seen = [false; 4];
        for _ in 0..256 {
            let level = city_watch_level(7, &mut rng);
            assert!((10..=13).contains(&level), "level {level}");
            seen[(level - 10) as usize] = true;
        }
        assert_eq!(seen, [true; 4], "Range(3, 7) covers 3..=6");
        // Same seed ⇒ same guard level; the level is the only player-dependent
        // input (offset, not scale).
        let mut first = Rng64::new(5);
        let mut second = Rng64::new(5);
        let offset = city_watch_level(0, &mut first);
        assert_eq!(city_watch_level(30, &mut second), 30 + offset);
    }

    #[test]
    fn profile_copies_career_attributes_and_fills_all_skills() {
        let career = test_career();
        let mut rng = Rng64::new(20_261_020);
        // Literal `min(5·Level + 30, 100)` (`EnemyEntity.cs:318-326`), written
        // out so the assertion cannot restate the function under test.
        for (level, expected) in [(1, 35), (10, 80), (14, 100), (20, 100)] {
            let profile =
                HumanoidProfile::from_career(1, &career, level, &mut rng).expect("valid class");
            assert_eq!(profile.class_id, 1);
            assert_eq!(profile.name, "Spellsword");
            assert_eq!(profile.level, level);
            assert_eq!(profile.attributes, [60, 50, 55, 50, 50, 40, 45, 50]);
            assert_eq!(
                profile.skills,
                [expected; SKILL_COUNT],
                "level {level} ⇒ {expected}"
            );
            assert_eq!(profile.hit_points_per_level, 12);
            let (low, high) = profile.health_bounds();
            assert!(
                (low..=high).contains(&profile.max_health),
                "level {level}: {} not in {low}..={high}",
                profile.max_health
            );
        }
    }

    #[test]
    fn health_bounds_follow_the_level_and_hpl() {
        let career = test_career();
        let mut rng = Rng64::new(1);
        let profile = HumanoidProfile::from_career(2, &career, 5, &mut rng).expect("valid");
        // `10 + L·[1, HPL]` ⇒ level 5, HPL 12 ⇒ [15, 70].
        assert_eq!(profile.health_bounds(), (15, 70));
        assert_eq!(profile.attributes, career.attributes);
    }

    #[test]
    fn level_zero_is_clamped_and_class_nineteen_is_rejected() {
        let career = test_career();
        let mut rng = Rng64::new(1);
        let profile = HumanoidProfile::from_career(0, &career, 0, &mut rng).expect("valid");
        assert_eq!(profile.level, 1);
        assert_eq!(profile.skills, [35; SKILL_COUNT]);
        assert_eq!(
            HumanoidProfile::from_career(19, &career, 1, &mut rng),
            Err(HumanoidProfileError::ClassId(19))
        );
        assert_eq!(
            HumanoidProfile::from_career(255, &career, 1, &mut rng),
            Err(HumanoidProfileError::ClassId(255))
        );
    }

    #[test]
    fn profile_is_deterministic_for_the_same_seed() {
        let career = test_career();
        let mut a = Rng64::new(99);
        let mut b = Rng64::new(99);
        let left = HumanoidProfile::from_career(18, &career, 9, &mut a).expect("valid");
        let right = HumanoidProfile::from_career(18, &career, 9, &mut b).expect("valid");
        assert_eq!(left, right);
        assert_eq!(a, b);
    }
}

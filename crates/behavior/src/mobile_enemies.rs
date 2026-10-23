//! `mobile_enemies` — the `EnemyBasics.Enemies` table (ORDER M25-CLOSURE
//! criterion 2, spec M25 §3, REQ-L99-058).
//!
//! **Verified source (honesty note).** `MONSTER.BSA` supplies the 43
//! `ENEMY###.CFG` career records (74 B each) and the animation records
//! (`MonsterFile.cs:46-47`); the fields transcribed here — `Level`,
//! `MinHealth`/`MaxHealth`, `ArmorValue`, `MinMetalToHit`, `Team`,
//! `LootTableKey`, `MapChance` and the three damage bands — are **not** in
//! the BSA. They live in the DFU C# table
//! `EnemyBasics.Enemies` (`EnemyBasics.cs:234-2236` @ `0038a1e6`), whose
//! struct layout is `DaggerfallUnityStructs.cs:174-238` (`MobileEnemy`).
//! That file is the "fuente verificada" the ORDER allows: every row below
//! was transcribed mechanically from it, and the module tests pin three
//! goldens (Rat id 0, Lamia id 42, Horse id 39 defaults).
//!
//! **Census:** 62 entries — monsters `0..=42` (43) and humanoids
//! `128..=146` (19), the two `MobileTypes` bands
//! (`DaggerfallUnityEnums.cs:142-212`). 17 distinct loot keys (`A..U`).
//!
//! **Fields not consumed yet (declared, not invented):**
//! `MobileEnemy.CastsMagic`, `SeesThroughInvisibility`, `SoulPts`, the
//! animation frames and sounds are M25-AI/render work; `ArmorValue` is the
//! base of `ArmorValues[i] = ArmorValue * 5` (`EnemyEntity.cs:286-288`) and
//! its equipment clamp is WU-2 (`SetEnemyEquipment`,
//! `EnemyEntity.cs:403-456`).
//!
//! **Humanoid rows** carry the struct defaults for `Level`, `Min/MaxHealth`,
//! `ArmorValue` and `MinMetalToHit` (all `0` / `WeaponMetal::None`): the
//! `EnemyClass` branch ignores them and levels to the player with
//! `RollEnemyClassMaxHealth` (`EnemyEntity.cs:294-303`). The **Horse** entry
//! (`ID = 39`) is a bare `new MobileEnemy() { ID = 39 }`
//! (`EnemyBasics.cs:1561-1564`) — all its fields are the C# struct defaults,
//! transcribed here as such (level 0, HP 0, team `PlayerEnemy`).
//!
//! Pure: 0 RNG, 0 I/O, `static` data only — rolls come in as parameters at
//! the spawn site.

/// Number of transcribed entries (43 monsters + 19 humanoids).
pub const MOBILE_ENEMY_COUNT: usize = 62;

/// `MobileTeams` (`DaggerfallUnityEnums.cs:262-285`), in enum order.
///
/// **Frontier:** in the tree this engine ships today there is **no**
/// in-fighting consumer; DFU reads the team in `EnemyMotor.cs:209-212` to
/// reset a former ally's team when the player is attacked. The field is
/// exposed here and on the spawned combatants; no behaviour is derived from
/// it yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MobileTeams {
    /// `PlayerEnemy` (0) — also the C# default of unset rows.
    PlayerEnemy,
    /// `PlayerAlly` (1).
    PlayerAlly,
    /// `Vermin` (2).
    Vermin,
    /// `Spriggans` (3).
    Spriggans,
    /// `Bears` (4).
    Bears,
    /// `Tigers` (5).
    Tigers,
    /// `Spiders` (6).
    Spiders,
    /// `Orcs` (7).
    Orcs,
    /// `Centaurs` (8).
    Centaurs,
    /// `Werecreatures` (9).
    Werecreatures,
    /// `Nymphs` (10).
    Nymphs,
    /// `Aquatic` (11).
    Aquatic,
    /// `Harpies` (12).
    Harpies,
    /// `Undead` (13).
    Undead,
    /// `Giants` (14).
    Giants,
    /// `Scorpions` (15).
    Scorpions,
    /// `Magic` (16).
    Magic,
    /// `Daedra` (17).
    Daedra,
    /// `Dragonlings` (18).
    Dragonlings,
    /// `KnightsAndMages` (19).
    KnightsAndMages,
    /// `Criminals` (20).
    Criminals,
    /// `CityWatch` (21).
    CityWatch,
}

/// `WeaponMaterialTypes` (`ItemEnums.cs:64-77`): `None = -1`,
/// `Iron = 0 … Daedric = 9`.
///
/// [`WeaponMetal::raw`] is the numeric scale the material gate compares
/// (`FormulaHelper.cs:576-583`: `target.MinMetalToHit > NativeMaterialValue`
/// ⇒ 0 damage, no roll, no wear).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeaponMetal {
    /// `None = -1` — no metal required (also the C# default of unset rows).
    None,
    /// `Iron = 0x0000`.
    Iron,
    /// `Steel = 0x0001`.
    Steel,
    /// `Silver = 0x0002`.
    Silver,
    /// `Elven = 0x0003`.
    Elven,
    /// `Dwarven = 0x0004`.
    Dwarven,
    /// `Mithril = 0x0005`.
    Mithril,
    /// `Adamantium = 0x0006`.
    Adamantium,
    /// `Ebony = 0x0007`.
    Ebony,
    /// `Orcish = 0x0008`.
    Orcish,
    /// `Daedric = 0x0009`.
    Daedric,
}

impl WeaponMetal {
    /// Numeric value on the `WeaponMaterialTypes` scale.
    pub const fn raw(self) -> i32 {
        match self {
            Self::None => -1,
            Self::Iron => 0,
            Self::Steel => 1,
            Self::Silver => 2,
            Self::Elven => 3,
            Self::Dwarven => 4,
            Self::Mithril => 5,
            Self::Adamantium => 6,
            Self::Ebony => 7,
            Self::Orcish => 8,
            Self::Daedric => 9,
        }
    }
}

/// One `EnemyBasics.Enemies` row (`EnemyBasics.cs:234-2236` @ `0038a1e6`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MobileEnemyEntry {
    /// `ID` — the `MobileTypes` id of the row.
    pub id: u16,
    /// `// <name>` comment above the row in the source.
    pub name: &'static str,
    /// `Level` — the level of an `EnemyMonster` spawn
    /// (`EnemyEntity.cs:284`); **ignored** for `EnemyClass`
    /// (`:299` levels to the player).
    pub level: i32,
    /// `MinHealth` — inclusive floor of `Random.Range(MinHealth,
    /// MaxHealth + 1)` (`EnemyEntity.cs:285`).
    pub min_health: i32,
    /// `MaxHealth` — inclusive ceiling of the same roll.
    pub max_health: i32,
    /// `ArmorValue` — base of `ArmorValues[i] = ArmorValue * 5`
    /// (`EnemyEntity.cs:286-288`); the equipment clamp is WU-2.
    pub armor_value: i32,
    /// `MinMetalToHit` (`EnemyEntity.cs:315` → `Combatant.min_metal_to_hit`).
    pub min_metal_to_hit: WeaponMetal,
    /// `Team` (`EnemyEntity.cs:316`) — exposed, no in-fighting consumer yet.
    pub team: MobileTeams,
    /// `LootTableKey` (`EnemyEntity.cs:330` → `daggerfall_inventory::loot`).
    pub loot_table_key: Option<char>,
    /// `MapChance` (`EnemyEntity.cs:389` → `RandomlyAddMap`).
    pub map_chance: i32,
    /// `(MinDamage, MaxDamage)`, `(MinDamage2, MaxDamage2)`,
    /// `(MinDamage3, MaxDamage3)` — the three `Fighter::monster_bands`
    /// ranges; `(0, 0)` where the source declares no extra hit.
    pub damage_bands: [(i32, i32); 3],
}

/// Row constructor of the retail field order (a macro, not a function, so
/// the 62 table rows stay const data without a 13-argument call).
macro_rules! entry {
    ($id:expr, $name:expr, $level:expr, $min_health:expr, $max_health:expr,
     $armor_value:expr, $min_metal_to_hit:expr, $team:expr,
     $loot_table_key:expr, $map_chance:expr,
     $band1:expr, $band2:expr, $band3:expr) => {
        MobileEnemyEntry {
            id: $id,
            name: $name,
            level: $level,
            min_health: $min_health,
            max_health: $max_health,
            armor_value: $armor_value,
            min_metal_to_hit: $min_metal_to_hit,
            team: $team,
            loot_table_key: $loot_table_key,
            map_chance: $map_chance,
            damage_bands: [$band1, $band2, $band3],
        }
    };
}

/// Monster rows, in `ID` order (`EnemyBasics.cs:234-1600`, ids `0..=42`).
///
/// Row 39 (`Horse`) is the bare `new MobileEnemy() { ID = 39 }` of
/// `EnemyBasics.cs:1561-1564`: every other field is the C# struct default.
const MONSTER_ENEMIES: [MobileEnemyEntry; 43] = [
    entry!(0, "Rat", 1, 9, 16, 6, WeaponMetal::None, MobileTeams::Vermin, None, 0, (1, 4), (0, 0), (0, 0)),
    entry!(1, "Imp", 2, 11, 18, 3, WeaponMetal::Steel, MobileTeams::Magic, Some('D'), 1, (2, 15), (0, 0), (0, 0)),
    entry!(2, "Spriggan", 3, 12, 26, -4, WeaponMetal::None, MobileTeams::Spriggans, Some('B'), 0, (1, 8), (1, 8), (1, 10)),
    entry!(3, "Giant Bat", 3, 12, 26, 6, WeaponMetal::None, MobileTeams::Vermin, None, 0, (2, 12), (0, 0), (0, 0)),
    entry!(4, "Grizzly Bear", 4, 13, 34, 6, WeaponMetal::None, MobileTeams::Bears, None, 0, (1, 8), (1, 8), (1, 10)),
    entry!(5, "Sabertooth Tiger", 4, 13, 34, 6, WeaponMetal::None, MobileTeams::Tigers, None, 0, (1, 10), (1, 10), (3, 15)),
    entry!(6, "Spider", 4, 13, 34, 5, WeaponMetal::None, MobileTeams::Spiders, None, 0, (5, 15), (0, 0), (0, 0)),
    entry!(7, "Orc", 5, 13, 34, 7, WeaponMetal::None, MobileTeams::Orcs, Some('A'), 0, (1, 6), (0, 0), (0, 0)),
    entry!(8, "Centaur", 5, 14, 46, 6, WeaponMetal::None, MobileTeams::Centaurs, Some('C'), 1, (5, 15), (0, 0), (0, 0)),
    entry!(9, "Werewolf", 6, 17, 66, 5, WeaponMetal::Silver, MobileTeams::Werecreatures, None, 0, (1, 10), (1, 10), (2, 12)),
    entry!(10, "Nymph", 6, 15, 50, 0, WeaponMetal::Silver, MobileTeams::Nymphs, Some('C'), 1, (1, 5), (0, 0), (0, 0)),
    entry!(11, "Slaughterfish", 7, 15, 50, 6, WeaponMetal::None, MobileTeams::Aquatic, None, 0, (2, 12), (0, 0), (0, 0)),
    entry!(12, "Orc Sergeant", 7, 15, 50, 5, WeaponMetal::None, MobileTeams::Orcs, Some('A'), 1, (5, 15), (0, 0), (0, 0)),
    entry!(13, "Harpy", 8, 16, 85, 2, WeaponMetal::Dwarven, MobileTeams::Harpies, Some('D'), 0, (5, 15), (0, 0), (0, 0)),
    entry!(14, "Wereboar", 8, 17, 66, 3, WeaponMetal::Silver, MobileTeams::Werecreatures, None, 0, (2, 12), (2, 12), (5, 15)),
    entry!(15, "Skeletal Warrior", 9, 17, 66, 2, WeaponMetal::None, MobileTeams::Undead, Some('H'), 1, (5, 15), (0, 0), (0, 0)),
    entry!(16, "Giant", 10, 18, 74, 3, WeaponMetal::None, MobileTeams::Giants, Some('F'), 1, (10, 30), (0, 0), (0, 0)),
    entry!(17, "Zombie", 10, 52, 66, 0, WeaponMetal::None, MobileTeams::Undead, Some('G'), 1, (15, 50), (0, 0), (0, 0)),
    entry!(18, "Ghost", 11, 17, 66, 0, WeaponMetal::Silver, MobileTeams::Undead, Some('I'), 1, (10, 35), (0, 0), (0, 0)),
    entry!(19, "Mummy", 11, 17, 66, 2, WeaponMetal::Silver, MobileTeams::Undead, Some('E'), 1, (5, 15), (0, 0), (0, 0)),
    entry!(20, "Giant Scorpion", 12, 18, 74, 0, WeaponMetal::None, MobileTeams::Scorpions, None, 0, (15, 25), (0, 0), (0, 0)),
    entry!(21, "Orc Shaman", 13, 18, 74, 7, WeaponMetal::None, MobileTeams::Orcs, Some('U'), 3, (2, 20), (0, 0), (0, 0)),
    entry!(22, "Gargoyle", 14, 19, 82, 0, WeaponMetal::Mithril, MobileTeams::Magic, None, 0, (10, 15), (0, 0), (0, 0)),
    entry!(23, "Wraith", 15, 30, 90, 0, WeaponMetal::Silver, MobileTeams::Undead, Some('I'), 1, (20, 45), (0, 0), (0, 0)),
    entry!(24, "Orc Warlord", 16, 20, 90, 0, WeaponMetal::None, MobileTeams::Orcs, Some('T'), 2, (5, 50), (0, 0), (0, 0)),
    entry!(25, "Frost Daedra", 17, 25, 130, -5, WeaponMetal::Mithril, MobileTeams::Daedra, Some('J'), 0, (50, 100), (0, 0), (0, 0)),
    entry!(26, "Fire Daedra", 17, 26, 138, 1, WeaponMetal::Mithril, MobileTeams::Daedra, Some('J'), 0, (15, 50), (0, 0), (0, 0)),
    entry!(27, "Daedroth", 18, 27, 146, 1, WeaponMetal::Mithril, MobileTeams::Daedra, Some('E'), 0, (15, 50), (0, 0), (0, 0)),
    entry!(28, "Vampire", 19, 28, 154, -2, WeaponMetal::Silver, MobileTeams::Undead, Some('Q'), 3, (20, 50), (0, 0), (0, 0)),
    entry!(29, "Daedra Seducer", 19, 27, 146, 1, WeaponMetal::Mithril, MobileTeams::Daedra, Some('Q'), 1, (15, 50), (0, 0), (0, 0)),
    entry!(30, "Vampire Ancient", 20, 30, 170, -5, WeaponMetal::Mithril, MobileTeams::Undead, Some('Q'), 3, (20, 60), (0, 0), (0, 0)),
    entry!(31, "Daedra Lord", 20, 35, 210, -10, WeaponMetal::Mithril, MobileTeams::Daedra, Some('S'), 0, (15, 50), (0, 0), (0, 0)),
    entry!(32, "Lich", 20, 30, 170, -10, WeaponMetal::Mithril, MobileTeams::Undead, Some('S'), 4, (70, 100), (0, 0), (0, 0)),
    entry!(33, "Ancient Lich", 21, 30, 170, -12, WeaponMetal::Mithril, MobileTeams::Undead, Some('S'), 4, (70, 100), (0, 0), (0, 0)),
    entry!(34, "Dragonling", 16, 14, 42, 6, WeaponMetal::None, MobileTeams::Dragonlings, None, 0, (5, 15), (0, 0), (0, 0)),
    entry!(35, "Fire Atronach", 16, 25, 130, 6, WeaponMetal::None, MobileTeams::Magic, None, 0, (5, 15), (0, 0), (0, 0)),
    entry!(36, "Iron Atronach", 16, 25, 130, 6, WeaponMetal::None, MobileTeams::Magic, None, 0, (5, 15), (0, 0), (0, 0)),
    entry!(37, "Flesh Atronach", 16, 25, 130, 6, WeaponMetal::None, MobileTeams::Magic, None, 0, (5, 15), (0, 0), (0, 0)),
    entry!(38, "Ice Atronach", 16, 25, 130, 6, WeaponMetal::None, MobileTeams::Magic, None, 0, (5, 15), (0, 0), (0, 0)),
    entry!(39, "Horse", 0, 0, 0, 0, WeaponMetal::None, MobileTeams::PlayerEnemy, None, 0, (0, 0), (0, 0), (0, 0)),
    entry!(40, "Dragonling", 16, 14, 42, 6, WeaponMetal::None, MobileTeams::Dragonlings, None, 0, (5, 15), (0, 0), (0, 0)),
    entry!(41, "Dreugh", 16, 13, 34, 6, WeaponMetal::None, MobileTeams::Aquatic, Some('R'), 0, (5, 15), (0, 0), (0, 0)),
    entry!(42, "Lamia", 16, 16, 58, 6, WeaponMetal::None, MobileTeams::Aquatic, Some('R'), 0, (5, 15), (0, 0), (0, 0)),
];

/// Humanoid rows, in `ID` order (`EnemyBasics.cs:1605-2236`, ids
/// `128..=146`). Level/health/armor/metal are the C# struct defaults — the
/// `EnemyClass` branch ignores them (`EnemyEntity.cs:294-303`) — while
/// `Team`, `LootTableKey` and `MapChance` are set per row and consumed.
const HUMANOID_ENEMIES: [MobileEnemyEntry; 19] = [
    entry!(128, "Mage", 0, 0, 0, 0, WeaponMetal::None, MobileTeams::KnightsAndMages, Some('U'), 3, (0, 0), (0, 0), (0, 0)),
    entry!(129, "Spellsword", 0, 0, 0, 0, WeaponMetal::None, MobileTeams::KnightsAndMages, Some('P'), 1, (0, 0), (0, 0), (0, 0)),
    entry!(130, "Battlemage", 0, 0, 0, 0, WeaponMetal::None, MobileTeams::KnightsAndMages, Some('U'), 2, (0, 0), (0, 0), (0, 0)),
    entry!(131, "Sorcerer", 0, 0, 0, 0, WeaponMetal::None, MobileTeams::KnightsAndMages, Some('U'), 3, (0, 0), (0, 0), (0, 0)),
    entry!(132, "Healer", 0, 0, 0, 0, WeaponMetal::None, MobileTeams::KnightsAndMages, Some('U'), 1, (0, 0), (0, 0), (0, 0)),
    entry!(133, "Nightblade", 0, 0, 0, 0, WeaponMetal::None, MobileTeams::Criminals, Some('U'), 1, (0, 0), (0, 0), (0, 0)),
    entry!(134, "Bard", 0, 0, 0, 0, WeaponMetal::None, MobileTeams::KnightsAndMages, Some('O'), 2, (0, 0), (0, 0), (0, 0)),
    entry!(135, "Burglar", 0, 0, 0, 0, WeaponMetal::None, MobileTeams::Criminals, Some('O'), 1, (0, 0), (0, 0), (0, 0)),
    entry!(136, "Rogue", 0, 0, 0, 0, WeaponMetal::None, MobileTeams::Criminals, Some('O'), 1, (0, 0), (0, 0), (0, 0)),
    entry!(137, "Acrobat", 0, 0, 0, 0, WeaponMetal::None, MobileTeams::KnightsAndMages, Some('O'), 0, (0, 0), (0, 0), (0, 0)),
    entry!(138, "Thief", 0, 0, 0, 0, WeaponMetal::None, MobileTeams::Criminals, Some('O'), 2, (0, 0), (0, 0), (0, 0)),
    entry!(139, "Assassin", 0, 0, 0, 0, WeaponMetal::None, MobileTeams::Criminals, Some('O'), 0, (0, 0), (0, 0), (0, 0)),
    entry!(140, "Monk", 0, 0, 0, 0, WeaponMetal::None, MobileTeams::KnightsAndMages, Some('T'), 1, (0, 0), (0, 0), (0, 0)),
    entry!(141, "Archer", 0, 0, 0, 0, WeaponMetal::None, MobileTeams::KnightsAndMages, Some('C'), 0, (0, 0), (0, 0), (0, 0)),
    entry!(142, "Ranger", 0, 0, 0, 0, WeaponMetal::None, MobileTeams::KnightsAndMages, Some('C'), 1, (0, 0), (0, 0), (0, 0)),
    entry!(143, "Barbarian", 0, 0, 0, 0, WeaponMetal::None, MobileTeams::Criminals, Some('T'), 0, (0, 0), (0, 0), (0, 0)),
    entry!(144, "Warrior", 0, 0, 0, 0, WeaponMetal::None, MobileTeams::KnightsAndMages, Some('T'), 0, (0, 0), (0, 0), (0, 0)),
    entry!(145, "Knight", 0, 0, 0, 0, WeaponMetal::None, MobileTeams::KnightsAndMages, Some('T'), 1, (0, 0), (0, 0), (0, 0)),
    entry!(146, "City Watch", 0, 0, 0, 0, WeaponMetal::None, MobileTeams::CityWatch, None, 0, (0, 0), (0, 0), (0, 0)),
];

/// A `MobileTypes` id outside the two bands of `EnemyBasics.Enemies`
/// (monsters `0..=42`, humanoids `128..=146`).
///
/// Never clamped and never defaulted: an unknown id is a typed error at
/// every call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MobileEnemyError {
    /// `id` is not a transcribed `MobileTypes` entry.
    OutOfBand {
        /// The rejected id.
        id: u16,
    },
}

impl std::fmt::Display for MobileEnemyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutOfBand { id } => write!(
                f,
                "mobile {id} outside EnemyBasics.Enemies (monsters 0..=42, humanoids 128..=146)"
            ),
        }
    }
}

impl std::error::Error for MobileEnemyError {}

/// Typed lookup of one `EnemyBasics.Enemies` row.
///
/// Accepts exactly the two bands the source table declares; anything else
/// is [`MobileEnemyError::OutOfBand`] — the value is never clamped into a
/// neighbouring band and never defaulted to row 0.
pub fn mobile_enemy(id: u16) -> Result<&'static MobileEnemyEntry, MobileEnemyError> {
    match id {
        0..=42 => Ok(&MONSTER_ENEMIES[usize::from(id)]),
        128..=146 => Ok(&HUMANOID_ENEMIES[usize::from(id - 128)]),
        _ => Err(MobileEnemyError::OutOfBand { id }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn census_is_62_over_two_bands() {
        assert_eq!(MONSTER_ENEMIES.len(), 43);
        assert_eq!(HUMANOID_ENEMIES.len(), 19);
        let resolved = (0..=42u16)
            .chain(128..=146u16)
            .filter(|id| mobile_enemy(*id).is_ok())
            .count();
        assert_eq!(resolved, MOBILE_ENEMY_COUNT);
    }

    #[test]
    fn every_row_id_matches_its_band_position() {
        for (index, row) in MONSTER_ENEMIES.iter().enumerate() {
            assert_eq!(usize::from(row.id), index);
        }
        for (index, row) in HUMANOID_ENEMIES.iter().enumerate() {
            assert_eq!(usize::from(row.id), index + 128);
        }
    }

    /// Goldens measured from `EnemyBasics.cs@0038a1e6`: Rat (`:239-267`),
    /// Lamia and the Horse struct defaults (`:1561-1564`), Mage (`:1605`).
    #[test]
    fn goldens_rat_lamia_horse_mage() {
        let rat = mobile_enemy(0).expect("Rat");
        assert_eq!(
            *rat,
            entry!(0, "Rat", 1, 9, 16, 6, WeaponMetal::None, MobileTeams::Vermin, None, 0, (1, 4), (0, 0), (0, 0))
        );
        let lamia = mobile_enemy(42).expect("Lamia");
        assert_eq!((lamia.level, lamia.min_health, lamia.max_health), (16, 16, 58));
        assert_eq!(lamia.armor_value, 6);
        assert_eq!(lamia.min_metal_to_hit, WeaponMetal::None);
        assert_eq!(lamia.team, MobileTeams::Aquatic);
        assert_eq!(lamia.loot_table_key, Some('R'));
        let horse = mobile_enemy(39).expect("Horse");
        assert_eq!((horse.level, horse.min_health, horse.max_health), (0, 0, 0));
        assert_eq!(horse.team, MobileTeams::PlayerEnemy);
        assert_eq!(horse.loot_table_key, None);
        let mage = mobile_enemy(128).expect("Mage");
        assert_eq!(mage.team, MobileTeams::KnightsAndMages);
        assert_eq!(mage.loot_table_key, Some('U'));
        assert_eq!(mage.map_chance, 3);
        // The humanoid rows carry the struct defaults the EnemyClass branch
        // ignores (`EnemyEntity.cs:294-303`).
        assert_eq!((mage.level, mage.min_health, mage.max_health), (0, 0, 0));
        assert_eq!(mage.min_metal_to_hit, WeaponMetal::None);
    }

    /// Negative control: `43..=127` and `>= 147` are typed errors.
    #[test]
    fn out_of_band_ids_are_errors() {
        for id in 43u16..=127 {
            assert_eq!(mobile_enemy(id), Err(MobileEnemyError::OutOfBand { id }), "id {id}");
        }
        for id in [147u16, 148, 200, 255, 383, 384, 1000, u16::MAX] {
            assert_eq!(mobile_enemy(id), Err(MobileEnemyError::OutOfBand { id }), "id {id}");
        }
    }

    /// The 17 distinct loot keys of the table (A..U subset) — the set the
    /// M21 matrices must resolve.
    #[test]
    fn loot_keys_are_seventeen_distinct() {
        let mut keys: Vec<char> = MONSTER_ENEMIES
            .iter()
            .chain(HUMANOID_ENEMIES.iter())
            .filter_map(|row| row.loot_table_key)
            .collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), 17);
    }

    #[test]
    fn weapon_metal_scale_matches_item_enums() {
        assert_eq!(WeaponMetal::None.raw(), -1);
        assert_eq!(WeaponMetal::Iron.raw(), 0);
        assert_eq!(WeaponMetal::Steel.raw(), 1);
        assert_eq!(WeaponMetal::Silver.raw(), 2);
        assert_eq!(WeaponMetal::Mithril.raw(), 5);
        assert_eq!(WeaponMetal::Daedric.raw(), 9);
    }
}

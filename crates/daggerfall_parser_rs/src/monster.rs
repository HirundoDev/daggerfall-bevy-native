//! `MONSTER.BSA` — container census and `ENEMY###.CFG` (spec M25 §1-§2,
//! REQ-L99-058).
//!
//! DFU's `MonsterFile` **delegates** everything to `BsaFile` (`MonsterFile.cs:30`,
//! `:78-81`, `:124-137`): here the container is resolved by [`crate::bsa`] and this
//! module only classifies the census records and parses the 74 B of each
//! `ENEMY###.CFG` (identical to `CLASS.CFG`, `MonsterFile.cs:46-47`).
//!
//! Measured census (spec §1.2, `DISK:MONSTER.BSA` 27136 B):
//! `records=103` · `ASCR###.ANC`=60 (335..458 B) · `ENEMY###.CFG`=43 (exactly
//! 74 B) · directory at the end at `27136 − 18·103 = 25282` · sum of
//! sizes `25278 = 25282 − 4`.
//!
//! **N-1 declared (spec §7):** the `ASCR###.ANC` format is unknown in
//! the source (`MonsterANC` is an empty struct, `MonsterFile.cs:36-44`); this
//! module **preserves the blob** ([`MonsterArchive::ascr_bytes`]) without
//! interpreting it, for M34/assets.
//!
//! **N-6 declared:** `Unknown1` (@8) and `Unknown2` (@44..51) are preserved
//! raw, without semantics.

use crate::bsa::{self, BsaArchive, BsaError};

/// Retail container name (spec §1).
pub const MONSTER_FILENAME: &str = "MONSTER.BSA";

/// Size of the `ENEMY###.CFG` / `CLASS.CFG` record (spec §2.1: 58 + 8·2).
pub const CFG_BYTES: usize = 74;

/// Animation census prefix (`ASCR0000.ANC`…`ASCR0059.ANC`).
pub const ASCR_PREFIX: &str = "ASCR";
/// Class census prefix (`ENEMY000.CFG`…`ENEMY042.CFG`).
pub const ENEMY_PREFIX: &str = "ENEMY";
/// Suffix of the animation records.
pub const ASCR_SUFFIX: &str = ".ANC";
/// Suffix of the class records.
pub const ENEMY_SUFFIX: &str = ".CFG";

/// Number of `MobileTypes` monsters (ids 0..42, `DaggerfallUnityEnums.cs:144`).
pub const MOBILE_TYPE_COUNT: usize = 43;
/// First humanoid `MobileTypes` id (128 = `Mage`).
pub const MOBILE_TYPE_HUMANOID_START: u16 = 128;
/// Last humanoid `MobileTypes` id (146 = `Knight_CityWatch`).
pub const MOBILE_TYPE_HUMANOID_END: u16 = 146;

/// `MobileTypes` in id order 0..42 (`DaggerfallUnityEnums.cs:144-189`).
pub const MOBILE_TYPE_NAMES: [&str; MOBILE_TYPE_COUNT] = [
    "Rat",
    "Imp",
    "Spriggan",
    "GiantBat",
    "GrizzlyBear",
    "SabertoothTiger",
    "Spider",
    "Orc",
    "Centaur",
    "Werewolf",
    "Nymph",
    "Slaughterfish",
    "OrcSergeant",
    "Harpy",
    "Wereboar",
    "SkeletalWarrior",
    "Giant",
    "Zombie",
    "Ghost",
    "Mummy",
    "GiantScorpion",
    "OrcShaman",
    "Gargoyle",
    "Wraith",
    "OrcWarlord",
    "FrostDaedra",
    "FireDaedra",
    "Daedroth",
    "Vampire",
    "DaedraSeducer",
    "VampireAncient",
    "DaedraLord",
    "Lich",
    "AncientLich",
    "Dragonling",
    "FireAtronach",
    "IronAtronach",
    "FleshAtronach",
    "IceAtronach",
    "Horse_Invalid",
    "Dragonling_Alternate",
    "Dreugh",
    "Lamia",
];

/// `MobileTypes` id → **number** of the `ENEMY###.CFG` record that carries it.
///
/// `MonsterFile.LoadMonster` builds the name `ENEMY{mobile:000}.CFG` and
/// resolves it **by name** in the `BsaFile` (`MonsterFile.cs:159-183`): the
/// record number **is** the `MobileTypes` id. Measured over `ARENA2`
/// (2026-10-21): the 43 records match field by field in that order —
/// `ENEMY000`=Rat, `ENEMY001`=Imp, …, `ENEMY034`=Dragonling, `ENEMY035`=Fire
/// Atronach, …, `ENEMY042`=Lamia ⇒ identity map (verified by
/// `PROBE-MONSTER-001` block B).
///
/// The **directory order is not** that of `MobileTypes`: the `ENEMY###`
/// are interleaved with the `ASCR###` (erratum 26 of spec §1.2), so a
/// positional index in the census would filter the wrong record.
///
/// Three measured aliases (declared, not silently "corrected"):
/// 1. `GiantScorpion` (20) → `ENEMY020.CFG` whose retail `Name` is
///    **`Giant Scorpian`** (source erratum, not reader).
/// 2. `Horse_Invalid` (39) → `ENEMY039.CFG` whose `Name` is **`horse`**.
/// 3. `Dragonling_Alternate` (40) → the **second** record whose `Name` is
///    `dragonling` (`ENEMY040.CFG`); the first (`ENEMY034.CFG`) is
///    `Dragonling` (34).
pub const MOBILE_TO_RECORD: [u8; MOBILE_TYPE_COUNT] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42,
];

/// Container or `ENEMY###.CFG` read error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MonsterError {
    /// The BSA container does not parse.
    Bsa(BsaError),
    /// A class record does not measure [`CFG_BYTES`].
    ClassLength {
        /// Bytes observed.
        len: usize,
    },
    /// `Name` (@28) empty after cutting at the first NUL.
    EmptyName,
    /// Index outside the corresponding census.
    RecordOutOfBounds {
        /// Requested index.
        index: usize,
        /// Census size.
        len: usize,
    },
    /// `MobileTypes` id outside `0..=42` and `128..=146`.
    UnknownMobileType {
        /// Requested id.
        mobile: u16,
    },
}

impl std::fmt::Display for MonsterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bsa(err) => write!(f, "{err}"),
            Self::ClassLength { len } => {
                write!(f, "CFG record of {len} B (expected {CFG_BYTES})")
            }
            Self::EmptyName => write!(f, "empty record name"),
            Self::RecordOutOfBounds { index, len } => {
                write!(f, "index {index} outside 0..{len}")
            }
            Self::UnknownMobileType { mobile } => {
                write!(f, "MobileTypes {mobile} outside 0..=42 / 128..=146")
            }
        }
    }
}

impl std::error::Error for MonsterError {}

impl From<BsaError> for MonsterError {
    fn from(err: BsaError) -> Self {
        Self::Bsa(err)
    }
}

/// `ASCR###.ANC` record — preserved animation blob (N-1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimRecord {
    /// DOS name of the record (`ASCR0000.ANC`…).
    pub name: String,
    /// Directory `RecordSize`.
    pub size: u32,
    /// Absolute index in the BSA directory (for
    /// [`MonsterArchive::ascr_bytes`]).
    pub entry: usize,
}

/// `ENEMY###.CFG` record — 74 B monster class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnemyRecord {
    /// DOS name of the record (`ENEMY000.CFG`…).
    pub name: String,
    /// Number from the DOS name (`ENEMY{number:03}.CFG`) = `MobileTypes` id.
    /// It is the read key: directory order does not respect it.
    pub number: usize,
    /// Directory `RecordSize`.
    pub size: u32,
    /// Absolute index in the BSA directory.
    pub entry: usize,
}

/// Classified container: BSA metadata + the two censuses of spec §1.2.
#[derive(Debug, Clone)]
pub struct MonsterArchive {
    /// Generic BSA container (directory at the end, contiguous data from 4).
    pub archive: BsaArchive,
    /// The 60 `ASCR###.ANC` records in directory order.
    pub anim: Vec<AnimRecord>,
    /// The 43 `ENEMY###.CFG` records in directory order.
    pub enemy: Vec<EnemyRecord>,
    /// `enemy_by_number[n]` = position in [`Self::enemy`] of the
    /// `ENEMY{n:03}.CFG` record (or `None` if the container does not carry it).
    pub enemy_by_number: Vec<Option<usize>>,
}

/// Container census as printed by `PROBE-MONSTER-001` block A.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonsterCensus {
    /// `DirectoryCount` (103 in retail).
    pub records: usize,
    /// `ASCR*` records (60).
    pub ascr: usize,
    /// `ENEMY*` records (43).
    pub enemy: usize,
    /// Directory offset = `len − 18·records` (25282).
    pub dir_offset: u64,
    /// `Σ RecordSize` = `dir_offset − 4` (25278).
    pub record_sum: u64,
    /// Unique size of the `ENEMY*` if all match (74); `None` otherwise.
    pub enemy_stride: Option<u32>,
    /// Minimum `RecordSize` of the `ASCR*` (335).
    pub ascr_min: u32,
    /// Maximum `RecordSize` of the `ASCR*` (458).
    pub ascr_max: u32,
}

impl MonsterArchive {
    /// Classifies a complete `MONSTER.BSA` from its buffer (0 copies).
    pub fn parse(data: &[u8]) -> Result<Self, MonsterError> {
        let archive = bsa::parse(data)?;
        let mut anim = Vec::new();
        let mut enemy = Vec::new();
        for (index, entry) in archive.entries.iter().enumerate() {
            let name = match entry {
                bsa::Entry::Named { name, .. } => name,
                // MONSTER.BSA is 0x0100 (named); a numeric directory does not
                // carry the census names (§1.2) and is not classified.
                bsa::Entry::Numbered { .. } => continue,
            };
            let size = entry.data_size();
            if name.starts_with(ASCR_PREFIX) && name.ends_with(ASCR_SUFFIX) {
                anim.push(AnimRecord {
                    name: name.clone(),
                    size,
                    entry: index,
                });
            } else if let Some(number) = enemy_number(name) {
                enemy.push(EnemyRecord {
                    name: name.clone(),
                    number,
                    size,
                    entry: index,
                });
            }
        }
        let numbers = enemy.iter().map(|rec| rec.number).max().map_or(0, |max| max + 1);
        let mut enemy_by_number = vec![None; numbers];
        for (position, rec) in enemy.iter().enumerate() {
            enemy_by_number[rec.number] = Some(position);
        }
        Ok(Self {
            archive,
            anim,
            enemy,
            enemy_by_number,
        })
    }

    /// Container census (§1.2 / goldens A1-A8).
    pub fn census(&self) -> MonsterCensus {
        let record_sum = self.archive.directory_offset.saturating_sub(self.archive.data_start);
        let stride = self.enemy.first().map(|first| first.size);
        let uniform = stride.filter(|size| self.enemy.iter().all(|rec| rec.size == *size));
        MonsterCensus {
            records: self.archive.entries.len(),
            ascr: self.anim.len(),
            enemy: self.enemy.len(),
            dir_offset: self.archive.directory_offset,
            record_sum,
            enemy_stride: uniform,
            ascr_min: self.anim.iter().map(|rec| rec.size).min().unwrap_or(0),
            ascr_max: self.anim.iter().map(|rec| rec.size).max().unwrap_or(0),
        }
    }

    /// Raw bytes of an `ASCR###.ANC` (preserved blob, N-1).
    pub fn ascr_bytes<'a>(&self, data: &'a [u8], index: usize) -> Result<&'a [u8], MonsterError> {
        let rec = self
            .anim
            .get(index)
            .ok_or(MonsterError::RecordOutOfBounds {
                index,
                len: self.anim.len(),
            })?;
        bsa::entry_data(&self.archive, data, rec.entry).map_err(MonsterError::Bsa)
    }

    /// Raw bytes of `ENEMY{index:03}.CFG` (74 B) — unparsed.
    ///
    /// `index` is the **name number** (= `MobileTypes` id), not the
    /// census position: the `MONSTER.BSA` directory interleaves `ASCR`
    /// and `ENEMY` (erratum 26 of spec §1.2).
    pub fn enemy_bytes<'a>(&self, data: &'a [u8], index: usize) -> Result<&'a [u8], MonsterError> {
        let position = self
            .enemy_by_number
            .get(index)
            .copied()
            .flatten()
            .ok_or(MonsterError::RecordOutOfBounds {
                index,
                len: self.enemy.len(),
            })?;
        bsa::entry_data(&self.archive, data, self.enemy[position].entry).map_err(MonsterError::Bsa)
    }

    /// Parses `ENEMY{index:03}.CFG` directly from the buffer (`index` =
    /// `MobileTypes` id, not census position — see [`Self::enemy_bytes`]).
    pub fn monster_class(&self, data: &[u8], index: usize) -> Result<MonsterClass, MonsterError> {
        MonsterClass::parse(self.enemy_bytes(data, index)?)
    }

    /// Index of the `ENEMY###.CFG` record carrying the given `MobileTypes`.
    ///
    /// Only covers monsters (`0..=42`): humanoids `128..=146` do not live in
    /// `MONSTER.BSA` (they are resolved with `CLASS.CFG`, M20 boundary).
    pub fn record_for_mobile(&self, mobile: u16) -> Result<usize, MonsterError> {
        record_index_of_mobile(mobile)
    }
}

/// `ENEMY###.CFG` record number for a monster `MobileTypes` id.
///
/// It is the DOS name number (`ENEMY{mobile:000}.CFG`), the same key
/// used by `MonsterFile.LoadMonster`. Humanoids `128..=146` exist in
/// `MobileTypes` but not in `MONSTER.BSA` (they are resolved with `CLASS.CFG`,
/// M20 boundary) ⇒ [`MonsterError::UnknownMobileType`].
pub fn record_index_of_mobile(mobile: u16) -> Result<usize, MonsterError> {
    let index = usize::from(mobile);
    if index < MOBILE_TYPE_COUNT {
        Ok(usize::from(MOBILE_TO_RECORD[index]))
    } else {
        Err(MonsterError::UnknownMobileType { mobile })
    }
}

/// `ENEMY{number:03}.CFG` ⇒ `Some(number)`; any other name ⇒ `None`.
fn enemy_number(name: &str) -> Option<usize> {
    name.strip_prefix(ENEMY_PREFIX)?
        .strip_suffix(ENEMY_SUFFIX)?
        .parse()
        .ok()
}

impl AnimRecord {
    /// Is it an `ASCR###.ANC` from the census? (name, not position).
    pub fn is_anim(&self) -> bool {
        self.name.starts_with(ASCR_PREFIX)
    }
}

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// The 74 B of an `ENEMY###.CFG` in `ClassFile.ReadFile` read
/// order (spec §2.1, `ClassFile.cs:110-189`).
///
/// Skill indexes are preserved **raw** (`u8`): retail contains at
/// least one value outside `0..=34` (`ENEMY005.CFG` byte 27 = 40, measured) and
/// `daggerfall_skills::career::Career` would reject them by design — here the
/// monster class is a data record, not a playable career.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonsterClass {
    /// Resistance flags per element (@0).
    pub resistance_flags: u8,
    /// Immunity flags (@1).
    pub immunity_flags: u8,
    /// Low tolerance flags (@2).
    pub low_tolerance_flags: u8,
    /// Critical weakness flags (@3).
    pub critical_weakness_flags: u8,
    /// `AbilityFlagsAndSpellPointsBitfield` (@4).
    pub ability_flags: u16,
    /// `RapidHealing` (@6).
    pub rapid_healing: u8,
    /// `Regeneration` (@7).
    pub regeneration: u8,
    /// `Unknown1` (@8) — **N-6**: preserved, no semantics.
    pub unknown1: u8,
    /// `SpellAbsorptionFlags` (@9).
    pub spell_absorption_flags: u8,
    /// `AttackModifierFlags` (@10).
    pub attack_modifier_flags: u8,
    /// `ForbiddenMaterialsFlags` (@11, LE).
    pub forbidden_materials_flags: u16,
    /// High byte of the weapon/armor/shield bitfield (@13).
    pub weapon_armor_high: u8,
    /// Low byte of the bitfield (@14).
    pub weapon_armor_low: u8,
    /// Middle byte of the bitfield (@15) — the order is **not** natural (R-B).
    pub weapon_armor_mid: u8,
    /// `PrimarySkill1..3` (@16..18), raw skill index.
    pub primary: [u8; 3],
    /// `MajorSkill1..3` (@19..21), raw index.
    pub major: [u8; 3],
    /// `MinorSkill1..6` (@22..27), raw index.
    pub minor: [u8; 6],
    /// `Name` (@28..43, cut at the first NUL).
    pub name: String,
    /// `Unknown2` (@44..51) — **N-6**: preserved.
    pub unknown2: [u8; 8],
    /// `HitPointsPerLevel` (@52, LE).
    pub hit_points_per_level: u16,
    /// `AdvancementMultiplier` **raw** (@54, LE `u32`) — the spec §2.2
    /// value (`Rat` = 75008) is the integer, not the `DFCareer` fixed-point.
    pub advancement_multiplier_raw: u32,
    /// `Attributes[8]` (@58, LE) — `DFCareer.Stats` (STR, INT, WIL, AGI, END,
    /// PER, SPD, LCK).
    pub attributes: [u16; 8],
}

impl MonsterClass {
    /// Parses the 74 B of an `ENEMY###.CFG` / `CLASS.CFG` (spec §2.1).
    pub fn parse(bytes: &[u8]) -> Result<Self, MonsterError> {
        if bytes.len() != CFG_BYTES {
            return Err(MonsterError::ClassLength { len: bytes.len() });
        }
        let name_bytes = &bytes[28..44];
        let name_len = name_bytes
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(name_bytes.len());
        let name: String = name_bytes[..name_len].iter().map(|&b| b as char).collect();
        if name.is_empty() {
            return Err(MonsterError::EmptyName);
        }
        let mut attributes = [0u16; 8];
        for (index, attribute) in attributes.iter_mut().enumerate() {
            *attribute = u16_at(bytes, 58 + index * 2);
        }
        let mut unknown2 = [0u8; 8];
        unknown2.copy_from_slice(&bytes[44..52]);
        Ok(Self {
            resistance_flags: bytes[0],
            immunity_flags: bytes[1],
            low_tolerance_flags: bytes[2],
            critical_weakness_flags: bytes[3],
            ability_flags: u16_at(bytes, 4),
            rapid_healing: bytes[6],
            regeneration: bytes[7],
            unknown1: bytes[8],
            spell_absorption_flags: bytes[9],
            attack_modifier_flags: bytes[10],
            forbidden_materials_flags: u16_at(bytes, 11),
            weapon_armor_high: bytes[13],
            weapon_armor_low: bytes[14],
            weapon_armor_mid: bytes[15],
            primary: [bytes[16], bytes[17], bytes[18]],
            major: [bytes[19], bytes[20], bytes[21]],
            minor: [bytes[22], bytes[23], bytes[24], bytes[25], bytes[26], bytes[27]],
            name,
            unknown2,
            hit_points_per_level: u16_at(bytes, 52),
            advancement_multiplier_raw: u32_at(bytes, 54),
            attributes,
        })
    }

    /// `WeaponArmorShieldsBitfield = (a << 16) | (c << 8) | b` (spec §2.1:
    /// the bytes are 13/14/15, not in natural order — R-B).
    pub fn weapon_armor_shields(&self) -> u32 {
        (u32::from(self.weapon_armor_high) << 16)
            | (u32::from(self.weapon_armor_mid) << 8)
            | u32::from(self.weapon_armor_low)
    }

    /// `Attributes[4]` (Endurance) — golden B2.
    pub fn endurance(&self) -> u16 {
        self.attributes[4]
    }

    /// `HitPointsPerLevel` as `i32` (consumed by the §3.1 formula).
    pub fn hppl(&self) -> i32 {
        i32::from(self.hit_points_per_level)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic `MONSTER.BSA` from raw payloads (name → bytes).
    fn synthetic_raw(records: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(records.len() as u16).to_le_bytes());
        out.extend_from_slice(&0x0100u16.to_le_bytes());
        for (_, payload) in records {
            out.extend_from_slice(payload);
        }
        for (name, payload) in records {
            let mut rec = [0u8; 18];
            let bytes = name.as_bytes();
            rec[..bytes.len()].copy_from_slice(bytes);
            rec[14..18].copy_from_slice(&(payload.len() as u32).to_le_bytes());
            out.extend_from_slice(&rec);
        }
        out
    }

    /// Builds a synthetic 0x0100 `MONSTER.BSA` with both censuses.
    fn synthetic(ascr_sizes: &[u32], enemy_records: &[(&str, u16)]) -> Vec<u8> {
        let mut names: Vec<(String, Vec<u8>)> = Vec::new();
        for (index, size) in ascr_sizes.iter().enumerate() {
            names.push((format!("ASCR{index:04}.ANC"), vec![0xAB; *size as usize]));
        }
        for (index, (monster_name, hppl)) in enemy_records.iter().enumerate() {
            let mut cfg = vec![0u8; CFG_BYTES];
            let bytes = monster_name.as_bytes();
            cfg[28..28 + bytes.len()].copy_from_slice(bytes);
            cfg[52..54].copy_from_slice(&hppl.to_le_bytes());
            names.push((format!("ENEMY{index:03}.CFG"), cfg));
        }
        let records: Vec<(&str, Vec<u8>)> = names
            .iter()
            .map(|(name, payload)| (name.as_str(), payload.clone()))
            .collect();
        synthetic_raw(&records)
    }

    #[test]
    fn census_classifies_both_families() {
        let data = synthetic(&[335, 458], &[("Rat", 1), ("Lamia", 17)]);
        let archive = MonsterArchive::parse(&data).expect("parse");
        let census = archive.census();
        assert_eq!(census.records, 4);
        assert_eq!(census.ascr, 2);
        assert_eq!(census.enemy, 2);
        assert_eq!(census.dir_offset, (data.len() - 18 * 4) as u64);
        assert_eq!(census.record_sum, census.dir_offset - 4);
        assert_eq!(census.enemy_stride, Some(74));
        assert_eq!((census.ascr_min, census.ascr_max), (335, 458));
        assert_eq!(archive.anim[0].name, "ASCR0000.ANC");
        assert_eq!(archive.enemy[0].name, "ENEMY000.CFG");
    }

    #[test]
    fn non_uniform_enemy_stride_is_reported_as_none() {
        let mut rat = vec![0u8; CFG_BYTES];
        rat[28..31].copy_from_slice(b"Rat");
        let mut short = vec![0u8; CFG_BYTES - 1];
        short[28..31].copy_from_slice(b"Imp");
        let data = synthetic_raw(&[("ENEMY000.CFG", rat), ("ENEMY001.CFG", short)]);
        let archive = MonsterArchive::parse(&data).expect("parse");
        assert_eq!(archive.census().enemy, 2);
        assert_eq!(archive.census().enemy_stride, None);
        assert!(matches!(
            archive.monster_class(&data, 1),
            Err(MonsterError::ClassLength { len: 73 })
        ));
    }

    #[test]
    fn monster_class_reads_the_measured_layout() {
        let mut cfg = vec![0u8; CFG_BYTES];
        cfg[0] = 68; // resistance
        cfg[4..6].copy_from_slice(&36u16.to_le_bytes());
        cfg[13] = 0x01;
        cfg[14] = 0x02;
        cfg[15] = 0x03;
        cfg[16..19].copy_from_slice(&[19, 32, 18]);
        cfg[28..33].copy_from_slice(b"Lamia");
        cfg[52..54].copy_from_slice(&17u16.to_le_bytes());
        cfg[54..58].copy_from_slice(&52992u32.to_le_bytes());
        for (index, value) in [75u16, 84, 80, 85, 95, 62, 65, 70].iter().enumerate() {
            cfg[58 + index * 2..60 + index * 2].copy_from_slice(&value.to_le_bytes());
        }
        let class = MonsterClass::parse(&cfg).expect("parse");
        assert_eq!(class.name, "Lamia");
        assert_eq!(class.resistance_flags, 68);
        assert_eq!(class.ability_flags, 36);
        assert_eq!(class.hit_points_per_level, 17);
        assert_eq!(class.hppl(), 17);
        assert_eq!(class.advancement_multiplier_raw, 52992);
        assert_eq!(class.endurance(), 95);
        assert_eq!(class.primary, [19, 32, 18]);
        // R-B: the bitfield composes a<<16 | c<<8 | b with a/b/c = 13/14/15.
        assert_eq!(class.weapon_armor_shields(), 0x0001_0302);
        assert_eq!(class.attributes[0], 75);
    }

    #[test]
    fn monster_class_rejects_wrong_length_and_empty_name() {
        assert!(matches!(
            MonsterClass::parse(&[0u8; 73]),
            Err(MonsterError::ClassLength { len: 73 })
        ));
        assert!(matches!(
            MonsterClass::parse(&[0u8; CFG_BYTES]),
            Err(MonsterError::EmptyName)
        ));
    }

    #[test]
    fn mobile_to_record_is_a_bijection_over_the_census() {
        let mut seen = [false; MOBILE_TYPE_COUNT];
        for record in MOBILE_TO_RECORD {
            let index = usize::from(record);
            assert!(index < MOBILE_TYPE_COUNT);
            assert!(!seen[index], "record {record} repeated");
            seen[index] = true;
        }
        assert_eq!(seen.iter().filter(|v| **v).count(), MOBILE_TYPE_COUNT);
        // `MonsterFile.LoadMonster` builds `ENEMY{mobile:000}.CFG`: identity.
        for mobile in 0..MOBILE_TYPE_COUNT as u16 {
            assert_eq!(record_index_of_mobile(mobile), Ok(usize::from(mobile)));
        }
        assert_eq!(record_index_of_mobile(0), Ok(0)); // Rat
        assert_eq!(record_index_of_mobile(42), Ok(42)); // Lamia
        assert_eq!(record_index_of_mobile(1), Ok(1)); // Imp
        assert!(record_index_of_mobile(128).is_err()); // humanoid: not here
        assert!(record_index_of_mobile(43).is_err());
    }

    /// Erratum 26 antibody: the retail directory interleaves `ASCR` and
    /// `ENEMY`, so reading must go by name number, not by
    /// census position.
    #[test]
    fn enemy_records_resolve_by_name_number_not_directory_position() {
        let mut rat = vec![0u8; CFG_BYTES];
        rat[28..31].copy_from_slice(b"Rat");
        rat[52..54].copy_from_slice(&1u16.to_le_bytes());
        let mut lamia = vec![0u8; CFG_BYTES];
        lamia[28..33].copy_from_slice(b"Lamia");
        lamia[52..54].copy_from_slice(&17u16.to_le_bytes());
        let data = synthetic_raw(&[
            ("ASCR0000.ANC", vec![0xAB; 335]),
            ("ENEMY042.CFG", lamia),
            ("ENEMY000.CFG", rat),
        ]);
        let archive = MonsterArchive::parse(&data).expect("parse");
        assert_eq!(archive.enemy[0].number, 42);
        assert_eq!(archive.enemy[0].name, "ENEMY042.CFG");
        assert_eq!(archive.monster_class(&data, 0).expect("rat").name, "Rat");
        assert_eq!(
            archive.monster_class(&data, 42).expect("lamia").hppl(),
            17
        );
        assert!(matches!(
            archive.monster_class(&data, 1),
            Err(MonsterError::RecordOutOfBounds { index: 1, .. })
        ));
    }

    #[test]
    fn ascr_blob_is_preserved_not_interpreted() {
        let data = synthetic(&[335, 458], &[("Rat", 1)]);
        let archive = MonsterArchive::parse(&data).expect("parse");
        let blob = archive.ascr_bytes(&data, 1).expect("blob");
        assert_eq!(blob.len(), 458);
        assert!(blob.iter().all(|byte| *byte == 0xAB));
        assert!(matches!(
            archive.ascr_bytes(&data, 2),
            Err(MonsterError::RecordOutOfBounds { index: 2, len: 2 })
        ));
    }
}

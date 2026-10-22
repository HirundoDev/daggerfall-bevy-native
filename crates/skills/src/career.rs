//! `career` — `CLASSnn.CFG` career record (M20).
//!
//! **74-byte** format measured in `DFU:API/ClassFile.cs:111-186` and
//! `DFU:API/DFCareer.cs:134-227` (offset comments of the `CFGData` struct).
//! Pure: it takes bytes, opens no files (Mode A reading belongs to the
//! probe/app).
//!
//! Conversions faithful to the source:
//! - name = 16 B CString (`ClassFile.cs:172`), Latin-1;
//! - `AdvancementMultiplier` = 16.16 fixed point rounded to 2 decimals
//!   (`DFCareer.cs:585-593`);
//! - spell multiplier from bits `0x1C00 >> 8`
//!   (`DFCareer.cs:778-815`);
//! - tolerances with precedence `Resistant > Immune > LowTolerance >
//!   CriticalWeakness > Normal` (`DFCareer.cs:746-762`).

use crate::character::{Skill, ATTRIBUTE_COUNT, SKILL_COUNT};

/// Size of the `CLASSnn.CFG` record (`ClassFile.cs:97`).
pub const CFG_BYTES: usize = 74;
/// Standard classes (`ClassCareers` 0..17, `EntityEnums.cs:51-72`).
pub const CLASS_COUNT: usize = 18;
/// File name of a standard class.
pub fn class_file_name(index: usize) -> String {
    format!("CLASS{index:02}.CFG")
}

/// Tolerance of a career to one element (`DFCareer.cs:229-236`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tolerance {
    /// No modifier.
    Normal,
    /// Immune.
    Immune,
    /// Resistant.
    Resistant,
    /// Low tolerance.
    LowTolerance,
    /// Critical weakness.
    CriticalWeakness,
}

/// Elements with tolerance, in `DFCareer` order (masks
/// `1,2,4,8,16,32,64`, `DFCareer.cs:375-393`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Element {
    /// Paralysis (mask 1).
    Paralysis,
    /// Magic (2).
    Magic,
    /// Poison (4).
    Poison,
    /// Fire (8).
    Fire,
    /// Frost (16).
    Frost,
    /// Shock (32).
    Shock,
    /// Disease (64).
    Disease,
}

impl Element {
    /// The seven elements in source order.
    pub const ALL: [Element; 7] = [
        Element::Paralysis,
        Element::Magic,
        Element::Poison,
        Element::Fire,
        Element::Frost,
        Element::Shock,
        Element::Disease,
    ];

    /// Element mask in the flag bytes.
    pub fn mask(self) -> u8 {
        match self {
            Element::Paralysis => 1,
            Element::Magic => 2,
            Element::Poison => 4,
            Element::Fire => 8,
            Element::Frost => 16,
            Element::Shock => 32,
            Element::Disease => 64,
        }
    }

    /// Index 0..6 (source order).
    pub fn index(self) -> usize {
        match self {
            Element::Paralysis => 0,
            Element::Magic => 1,
            Element::Poison => 2,
            Element::Fire => 3,
            Element::Frost => 4,
            Element::Shock => 5,
            Element::Disease => 6,
        }
    }
}

/// Structured career/class (`DFCareer`).
#[derive(Debug, Clone, PartialEq)]
pub struct Career {
    /// Class name (`CFG` bytes 28-43).
    pub name: String,
    /// Base attributes STR/INT/WIL/AGI/END/PER/SPD/LCK (`DFCareer.cs:561-568`).
    pub attributes: [i32; ATTRIBUTE_COUNT],
    /// Primary skills (3).
    pub primary: [Skill; 3],
    /// Major skills (3).
    pub major: [Skill; 3],
    /// Minor skills (6).
    pub minor: [Skill; 6],
    /// Maximum HP gained per level (`DFCareer.cs:35,583`).
    pub hit_points_per_level: i32,
    /// Career advancement multiplier (`DFCareer.cs:31,585-593`).
    pub advancement_multiplier: f32,
    /// Spell point multiplier (`DFCareer.cs:787-815`).
    pub spell_point_multiplier: f32,
    /// `NoRegenSpellPoints` ability bit (`DFCareer.cs:624`).
    pub no_regen_spell_points: bool,
    /// `Athleticism` ability bit (`DFCareer.cs:627`).
    pub athleticism: bool,
    /// `AcuteHearing` ability bit (`DFCareer.cs:626`).
    pub acute_hearing: bool,
    /// `AdrenalineRush` ability bit (`DFCareer.cs:628`).
    pub adrenaline_rush: bool,
    /// Rapid healing flags (`DFCareer.cs:386-392,631`).
    pub rapid_healing: u8,
    /// Regeneration flags (`DFCareer.cs:373-380,630`).
    pub regeneration: u8,
    /// Tolerances per element ([`Element::ALL`] order).
    pub tolerances: [Tolerance; 7],
    /// Raw resistance/immunity/tolerance/weakness flags.
    pub resistance_flags: u8,
    /// Raw immunity flags.
    pub immunity_flags: u8,
    /// Raw low tolerance flags.
    pub low_tolerance_flags: u8,
    /// Raw critical weakness flags.
    pub critical_weakness_flags: u8,
}

/// Parse error for a `CLASSnn.CFG`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CareerError {
    /// The record is not 74 bytes long.
    Length(usize),
    /// A skill-group index falls outside `0..=34`.
    SkillIndex {
        /// `CFG` field (`primary3`, `major3`, `minor6`…).
        field: &'static str,
        /// Value read.
        value: u8,
    },
    /// Empty name.
    EmptyName,
}

impl std::fmt::Display for CareerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CareerError::Length(len) => write!(f, "CFG record of {len} B (expected {CFG_BYTES})"),
            CareerError::SkillIndex { field, value } => {
                write!(f, "{field} = {value} outside 0..={}", SKILL_COUNT - 1)
            }
            CareerError::EmptyName => write!(f, "empty class name"),
        }
    }
}

impl std::error::Error for CareerError {}

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

fn skill_field(bytes: &[u8], at: usize, field: &'static str) -> Result<Skill, CareerError> {
    let value = bytes[at];
    Skill::from_index(i32::from(value))
        .ok_or(CareerError::SkillIndex { field, value })
}

impl Career {
    /// Parses the 74 bytes of a `CLASSnn.CFG` (offset table spec M20 §1).
    ///
    /// Rejects out-of-range skill indices: `CLASS18.CFG` (custom class
    /// template with `PrimarySkill3 = 35`) is not a standard career
    /// (spec §2, declared boundary).
    pub fn parse(bytes: &[u8]) -> Result<Self, CareerError> {
        if bytes.len() != CFG_BYTES {
            return Err(CareerError::Length(bytes.len()));
        }
        let name_bytes = &bytes[28..44];
        let name_len = name_bytes
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(name_bytes.len());
        let name: String = name_bytes[..name_len].iter().map(|&b| b as char).collect();
        if name.is_empty() {
            return Err(CareerError::EmptyName);
        }

        let bits = u16_at(bytes, 4);
        let spell_point_multiplier = match (bits & 0x1C00) >> 8 {
            0 => 3.0,
            4 => 2.0,
            8 => 1.75,
            12 => 1.5,
            16 => 1.0,
            20 => 0.5,
            _ => 3.0,
        };

        let advancement_raw = u32_at(bytes, 54);
        let advancement_value =
            f64::from(advancement_raw >> 16) + f64::from(advancement_raw & 0xffff) / 65536.0;
        // DFU rounds to 2 decimals via `string.Format("{0:0.00}")`.
        let advancement_multiplier = ((advancement_value * 100.0).round() / 100.0) as f32;

        let mut attributes = [0i32; ATTRIBUTE_COUNT];
        for (index, attribute) in attributes.iter_mut().enumerate() {
            *attribute = i32::from(u16_at(bytes, 58 + index * 2));
        }

        let flags = |byte: u8, mask: u8| (byte & mask) == mask;
        let tolerance = |flag_byte: u8, element: Element| flags(flag_byte, element.mask());

        let mut tolerances = [Tolerance::Normal; 7];
        for element in Element::ALL {
            let result = if tolerance(bytes[0], element) {
                Tolerance::Resistant
            } else if tolerance(bytes[1], element) {
                Tolerance::Immune
            } else if tolerance(bytes[2], element) {
                Tolerance::LowTolerance
            } else if tolerance(bytes[3], element) {
                Tolerance::CriticalWeakness
            } else {
                Tolerance::Normal
            };
            tolerances[element.index()] = result;
        }

        Ok(Self {
            name,
            attributes,
            primary: [
                skill_field(bytes, 16, "primary1")?,
                skill_field(bytes, 17, "primary2")?,
                skill_field(bytes, 18, "primary3")?,
            ],
            major: [
                skill_field(bytes, 19, "major1")?,
                skill_field(bytes, 20, "major2")?,
                skill_field(bytes, 21, "major3")?,
            ],
            minor: [
                skill_field(bytes, 22, "minor1")?,
                skill_field(bytes, 23, "minor2")?,
                skill_field(bytes, 24, "minor3")?,
                skill_field(bytes, 25, "minor4")?,
                skill_field(bytes, 26, "minor5")?,
                skill_field(bytes, 27, "minor6")?,
            ],
            hit_points_per_level: i32::from(u16_at(bytes, 52)),
            advancement_multiplier,
            spell_point_multiplier,
            no_regen_spell_points: flags(bytes[4], 8),
            acute_hearing: flags(bytes[4], 1),
            athleticism: flags(bytes[4], 2),
            adrenaline_rush: flags(bytes[4], 4),
            rapid_healing: bytes[6],
            regeneration: bytes[7],
            tolerances,
            resistance_flags: bytes[0],
            immunity_flags: bytes[1],
            low_tolerance_flags: bytes[2],
            critical_weakness_flags: bytes[3],
        })
    }

    /// Tolerance to one element.
    pub fn tolerance(&self, element: Element) -> Tolerance {
        self.tolerances[element.index()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic 74-B CFG with the measured shape of the format (0 retail bytes).
    fn synthetic_cfg() -> Vec<u8> {
        let mut bytes = vec![0u8; CFG_BYTES];
        bytes[4..6].copy_from_slice(&0x0C00u16.to_le_bytes()); // SP× 1.5
        bytes[6] = 4; // RapidHealing.Always
        bytes[16..19].copy_from_slice(&[31, 29, 32]);
        bytes[19..22].copy_from_slice(&[22, 24, 25]);
        bytes[22..28].copy_from_slice(&[23, 26, 27, 28, 30, 33]);
        bytes[28..32].copy_from_slice(b"Test");
        bytes[52..54].copy_from_slice(&12u16.to_le_bytes());
        // 0.91 in 16.16 = 0x0000_E8F6 (59638 / 65536 = 0.91000…)
        bytes[54..58].copy_from_slice(&0x0000_E8F6u32.to_le_bytes());
        let attributes: [u16; 8] = [60, 50, 55, 50, 50, 40, 45, 50];
        for (index, value) in attributes.iter().enumerate() {
            bytes[58 + index * 2..60 + index * 2].copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn parses_measured_layout() {
        let career = Career::parse(&synthetic_cfg()).expect("valid CFG");
        assert_eq!(career.name, "Test");
        assert_eq!(career.hit_points_per_level, 12);
        assert_eq!(career.advancement_multiplier, 0.91);
        assert_eq!(career.spell_point_multiplier, 1.5);
        assert_eq!(career.attributes, [60, 50, 55, 50, 50, 40, 45, 50]);
        assert_eq!(career.primary, [Skill::Axe, Skill::LongBlade, Skill::BluntWeapon]);
        assert_eq!(career.major, [Skill::Destruction, Skill::Illusion, Skill::Alteration]);
        assert_eq!(career.minor[0], Skill::Restoration);
        assert!(!career.no_regen_spell_points);
        assert_eq!(career.rapid_healing, 4);
        assert_eq!(career.tolerance(Element::Magic), Tolerance::Normal);
    }

    #[test]
    fn rejects_wrong_length_and_out_of_range_skill() {
        assert_eq!(Career::parse(&[0u8; 73]), Err(CareerError::Length(73)));
        let mut bytes = synthetic_cfg();
        bytes[18] = 35;
        assert_eq!(
            Career::parse(&bytes),
            Err(CareerError::SkillIndex {
                field: "primary3",
                value: 35
            })
        );
    }

    #[test]
    fn tolerance_precedence_matches_source() {
        let mut bytes = synthetic_cfg();
        bytes[0] = 2; // resistance to magic
        bytes[1] = 2; // also immune to magic → Resistant wins
        bytes[2] = 8; // low tolerance to fire
        bytes[3] = 8; // critical weakness to fire → LowTolerance wins
        let career = Career::parse(&bytes).expect("valid CFG");
        assert_eq!(career.tolerance(Element::Magic), Tolerance::Resistant);
        assert_eq!(career.tolerance(Element::Fire), Tolerance::LowTolerance);
    }
}

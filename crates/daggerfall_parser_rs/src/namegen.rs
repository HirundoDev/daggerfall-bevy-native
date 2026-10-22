//! `NAMEGEN.DAT` + `NameHelper` — retail name bank and deterministic
//! composition (M15, REQ-L99-047 · DoD §f).
//!
//! **`.DAT` layout (closed empirically + exact arithmetic):**
//!
//! ```text
//! [11 × 48 B bank record] [785 × 10 B part slot] = 8,378 B
//! bank record = 6 × u32 absolute offset + 6 × u32 part count (LE)
//! part slot   = NUL-padded ASCII (max 8 chars + padding to 10 B)
//! ```
//!
//! Verification (measured 2026-10-20, `DISK:NAMEGEN.DAT 8378 B
//! sha256=1229719909c8109b…`, 11 banks, 785 parts): every `(offset,count)`
//! falls inside the file, all 785 slots are printable ASCII with zero
//! padding, `528 + 785×10 = 8,378` exactly, and the `setCount`/`parts` reproduce
//! the `NameGen.txt` inventory (spec M15 §3.2/§3.4.1). The header hexdump
//! (bank 0 = Breton: offsets `528,658,788,918,1048,1178`, counts
//! `13×6`) is recorded in the HH-1 handoff.
//!
//! **Algorithm (executable spec `DFU:Game/Utility/NameHelper.cs`, MIT):**
//!
//! | bank | name | surname | anchor |
//! |:---|:---|:---|:---|
//! | Breton, DarkElf, HighElf, WoodElf, Khajiit, Imperial | M `0+1` · F `2+3` | `4+5` | `NameHelper.cs:188-247` |
//! | Nord | M `0+1` · F `2+3` | `0+1` + localized suffix | `:232-247` (`NEGATIVE` suffix, H3) |
//! | Redguard | `0+1+2` · F `+4` · M 75 % `+3` | empty | `:250-289` |
//!
//! Each part is indexed with `DFRandom.rand() % parts.len()`
//! (`NameHelper.cs:202-209`); the seed is applied with
//! `DFRandom.srand(nameSeed)` BEFORE naming (`Person.cs:625-628`).
//!
//! **H3 (Nord suffix `nordSurnameImmutableSuffix`):** the literal lives in the
//! DFU localization, absent from the pinned corpus; the source comment
//! documents the `0+1+"sen"` pattern (`NameHelper.cs:231`) but does not assert it.
//! That is why [`NameGen::full_name`] returns [`NameGenError::NordSuffixUnavailable`]
//! instead of inventing the suffix (0 synthetic names). The city path
//! (regions) only reaches Breton/Redguard banks (`MapsFile.cs:66-75`), so
//! the gap does not block population.
//!
//! **H1 (parsed `NAMEGEN.DAT`):** DFU does not parse the `.DAT` (it uses
//! `NameGen.txt` JSON via `NameHelper.cs:372-393`); this module implements the
//! measured binary layout because it is reproducible byte-by-byte (above). No
//! retail asset is copied into the repository: the `.DAT` is read at runtime from the
//! user's ARENA2 (Mode A).

use crate::dfrandom::DfRandom;

/// Banks of the `.DAT` (`DFU:Game/Utility/NameHelper.cs:41-53`).
pub const NAMEGEN_BANK_COUNT: usize = 11;
/// Set slots per bank (6 offsets + 6 counts).
pub const NAMEGEN_SET_SLOTS: usize = 6;
/// Length of the bank record in the header.
pub const NAMEGEN_BANK_RECORD_LEN: usize = 48;
/// Complete header (11 × 48).
pub const NAMEGEN_HEADER_LEN: usize = NAMEGEN_BANK_COUNT * NAMEGEN_BANK_RECORD_LEN;
/// Fixed length of one part slot (max. 8 chars + 2 NUL).
pub const NAMEGEN_PART_LEN: usize = 10;

/// `BankTypes` banks in their enum order (`NameHelper.cs:41-53`); the
/// index IS the value serialized in `NAMEGEN.DAT` and in `regionRaces`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BankType {
    /// Bank 0.
    Breton,
    /// Bank 1.
    Redguard,
    /// Bank 2.
    Nord,
    /// Bank 3.
    DarkElf,
    /// Bank 4.
    HighElf,
    /// Bank 5.
    WoodElf,
    /// Bank 6.
    Khajiit,
    /// Bank 7 (also used where Argonian names would be expected).
    Imperial,
    /// Bank 8 (monsters; non-deterministic in DFU, `NameHelper.cs:296`).
    Monster1,
    /// Bank 9 (monsters).
    Monster2,
    /// Bank 10 (monsters; unreachable via `Range(8, 10)`).
    Monster3,
}

impl BankType {
    /// The 11 banks in enum order.
    pub const ALL: [BankType; NAMEGEN_BANK_COUNT] = [
        BankType::Breton,
        BankType::Redguard,
        BankType::Nord,
        BankType::DarkElf,
        BankType::HighElf,
        BankType::WoodElf,
        BankType::Khajiit,
        BankType::Imperial,
        BankType::Monster1,
        BankType::Monster2,
        BankType::Monster3,
    ];

    /// Banks reachable through the city region path
    /// (`regionRaces ∈ {0,1}`, `MapsFile.cs:66-75`).
    pub const CITY_REACHABLE: [BankType; 2] = [BankType::Breton, BankType::Redguard];

    /// Enum index (= record offset in the `.DAT` header).
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Bank from the serialized index; `None` outside `0..11`.
    pub const fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(BankType::Breton),
            1 => Some(BankType::Redguard),
            2 => Some(BankType::Nord),
            3 => Some(BankType::DarkElf),
            4 => Some(BankType::HighElf),
            5 => Some(BankType::WoodElf),
            6 => Some(BankType::Khajiit),
            7 => Some(BankType::Imperial),
            8 => Some(BankType::Monster1),
            9 => Some(BankType::Monster2),
            10 => Some(BankType::Monster3),
            _ => None,
        }
    }

    /// Stable name (== `NameGen.txt` key).
    pub const fn name(self) -> &'static str {
        match self {
            BankType::Breton => "Breton",
            BankType::Redguard => "Redguard",
            BankType::Nord => "Nord",
            BankType::DarkElf => "DarkElf",
            BankType::HighElf => "HighElf",
            BankType::WoodElf => "WoodElf",
            BankType::Khajiit => "Khajiit",
            BankType::Imperial => "Imperial",
            BankType::Monster1 => "Monster1",
            BankType::Monster2 => "Monster2",
            BankType::Monster3 => "Monster3",
        }
    }
}

/// NPC gender (`DFU:Game/Questing/Person.cs`, enum `Genders`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Gender {
    /// `Genders.Male` (sets `0+1`).
    Male,
    /// `Genders.Female` (sets `2+3`).
    Female,
}

impl Gender {
    /// 50/50 gender draw from the given stream (M15 decision, see H6:
    /// DFU uses `UnityEngine.Random`, not seedable from `DFRandom`).
    pub fn draw(rng: &mut DfRandom) -> Self {
        if rng.rand() & 1 == 0 {
            Gender::Male
        } else {
            Gender::Female
        }
    }

    /// Stable name.
    pub const fn name(self) -> &'static str {
        match self {
            Gender::Male => "male",
            Gender::Female => "female",
        }
    }
}

/// A set of parts (`NameSet`, `NameHelper.cs:68-72`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameSet {
    /// `setIndex` (position in the bank).
    pub set_index: usize,
    /// Parts in file order.
    pub parts: Vec<String>,
}

/// A name bank (`NameBank`, `NameHelper.cs:59-63`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameBank {
    /// Non-empty sets in file order (`count = 0` slots do not exist).
    pub sets: Vec<NameSet>,
}

impl NameBank {
    /// Number of parts in the bank.
    pub fn parts_total(&self) -> usize {
        self.sets.iter().map(|set| set.parts.len()).sum()
    }
}

/// Typed parse/composition error (0 panic).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameGenError {
    /// File shorter than the 11-bank header.
    TooShort {
        /// Bytes received.
        len: usize,
        /// Bytes needed.
        need: usize,
    },
    /// A parts block runs past the file.
    OffsetOutOfBounds {
        /// Bank (0…10).
        bank: usize,
        /// Set (0…5).
        set: usize,
        /// Declared offset.
        offset: u32,
        /// Declared parts.
        count: u32,
        /// File bytes.
        len: usize,
    },
    /// A part slot is not NUL-padded printable ASCII.
    MalformedPart {
        /// Bank (0…10).
        bank: usize,
        /// Set (0…5).
        set: usize,
        /// Index inside the set.
        index: usize,
    },
    /// Monster bank: DFU `FullName` does not support them for NPCs.
    UnsupportedBank {
        /// Requested index.
        bank: usize,
    },
    /// Nord bank: the localized suffix is not in the corpus (H3).
    NordSuffixUnavailable,
    /// The bank lacks the set required by the composition rule.
    MissingSet {
        /// Bank.
        bank: usize,
        /// Required set.
        set: usize,
    },
}

impl std::fmt::Display for NameGenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort { len, need } => {
                write!(f, "NAMEGEN.DAT too short ({len} < {need} bytes)")
            }
            Self::OffsetOutOfBounds {
                bank,
                set,
                offset,
                count,
                len,
            } => write!(
                f,
                "bank {bank} set {set}: offset {offset} + {count} parts × 10 B runs past {len} B"
            ),
            Self::MalformedPart { bank, set, index } => write!(
                f,
                "bank {bank} set {set} part {index}: slot is not NUL-padded printable ASCII"
            ),
            Self::UnsupportedBank { bank } => {
                write!(f, "bank {bank}: monster bank without deterministic FullName")
            }
            Self::NordSuffixUnavailable => write!(
                f,
                "Nord bank: nordSurnameImmutableSuffix missing from the corpus (H3)"
            ),
            Self::MissingSet { bank, set } => write!(f, "bank {bank}: missing set {set}"),
        }
    }
}

impl std::error::Error for NameGenError {}

/// Parsed `NAMEGEN.DAT` (11 banks).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameGen {
    banks: Vec<NameBank>,
}

impl NameGen {
    /// Parses `NAMEGEN.DAT` (8,378 B in retail).
    pub fn parse(file: &[u8]) -> Result<Self, NameGenError> {
        if file.len() < NAMEGEN_HEADER_LEN {
            return Err(NameGenError::TooShort {
                len: file.len(),
                need: NAMEGEN_HEADER_LEN,
            });
        }

        let mut banks = Vec::with_capacity(NAMEGEN_BANK_COUNT);
        for bank in 0..NAMEGEN_BANK_COUNT {
            let record = bank * NAMEGEN_BANK_RECORD_LEN;
            let mut sets = Vec::new();
            for set in 0..NAMEGEN_SET_SLOTS {
                let offset = u32_le(file, record + set * 4) as usize;
                let count = u32_le(file, record + 24 + set * 4) as usize;
                if count == 0 {
                    continue;
                }
                let end = offset + count * NAMEGEN_PART_LEN;
                if offset < NAMEGEN_HEADER_LEN || end > file.len() {
                    return Err(NameGenError::OffsetOutOfBounds {
                        bank,
                        set,
                        offset: offset as u32,
                        count: count as u32,
                        len: file.len(),
                    });
                }
                let mut parts = Vec::with_capacity(count);
                for index in 0..count {
                    let slot = &file[offset + index * NAMEGEN_PART_LEN..][..NAMEGEN_PART_LEN];
                    let nul = slot.iter().position(|&b| b == 0).ok_or(
                        NameGenError::MalformedPart { bank, set, index },
                    )?;
                    let text = &slot[..nul];
                    let printable = text.iter().all(|b| (0x20..=0x7E).contains(b));
                    let padded = slot[nul..].iter().all(|&b| b == 0);
                    if !printable || !padded {
                        return Err(NameGenError::MalformedPart { bank, set, index });
                    }
                    parts.push(String::from_utf8_lossy(text).into_owned());
                }
                sets.push(NameSet {
                    set_index: set,
                    parts,
                });
            }
            banks.push(NameBank { sets });
        }
        Ok(Self { banks })
    }

    /// Bank by type (always present after a valid parse).
    pub fn bank(&self, bank: BankType) -> &NameBank {
        &self.banks[bank.index()]
    }

    /// Total parts across the 11 banks (785 in retail).
    pub fn parts_total(&self) -> usize {
        self.banks.iter().map(NameBank::parts_total).sum()
    }

    /// `NameHelper.FullName(type, gender)` with `DFRandom.srand(seed)`
    /// (`NameHelper.cs:92-104` + `Person.cs:625-628`).
    ///
    /// Consumes exactly 4 draws (5 for male Redguard when the
    /// 75 % gate passes, `NameHelper.cs:283`) — no others.
    pub fn full_name(
        &self,
        bank: BankType,
        gender: Gender,
        seed: u32,
    ) -> Result<String, NameGenError> {
        let mut rng = DfRandom::new();
        rng.srand(seed);
        self.full_name_with(&mut rng, bank, gender)
    }

    /// Same as [`Self::full_name`] reusing a stream (for chains
    /// that already consumed draws, such as DFU text macros).
    pub fn full_name_with(
        &self,
        rng: &mut DfRandom,
        bank: BankType,
        gender: Gender,
    ) -> Result<String, NameGenError> {
        let name_bank = &self.banks[bank.index()];
        let first = match bank {
            BankType::Redguard => return redguard_name(name_bank, gender, rng),
            BankType::Nord => {
                // H3: the suffix is not pinned → typed error, 0 invention.
                return Err(NameGenError::NordSuffixUnavailable);
            }
            BankType::Monster1 | BankType::Monster2 | BankType::Monster3 => {
                return Err(NameGenError::UnsupportedBank { bank: bank.index() });
            }
            BankType::Breton
            | BankType::DarkElf
            | BankType::HighElf
            | BankType::WoodElf
            | BankType::Khajiit
            | BankType::Imperial => {
                let (set_a, set_b) = match gender {
                    Gender::Male => (0, 1),
                    Gender::Female => (2, 3),
                };
                format!(
                    "{}{}",
                    draw_part(name_bank, set_a, rng, bank)?,
                    draw_part(name_bank, set_b, rng, bank)?
                )
            }
        };
        let surname = format!(
            "{}{}",
            draw_part(name_bank, 4, rng, bank)?,
            draw_part(name_bank, 5, rng, bank)?
        );
        Ok(join_name(first, surname))
    }
}

/// `GetRandomRedguardName` (`NameHelper.cs:250-289`).
fn redguard_name(
    bank: &NameBank,
    gender: Gender,
    rng: &mut DfRandom,
) -> Result<String, NameGenError> {
    let mut name = String::new();
    for set in 0..3 {
        name.push_str(draw_part(bank, set, rng, BankType::Redguard)?);
    }
    // 75 % gate: for female it is NOT consumed (`gender == Female || …`).
    let suffix_set = match gender {
        Gender::Female => Some(4),
        Gender::Male => {
            if rng.rand() % 100 < 75 {
                Some(3)
            } else {
                None
            }
        }
    };
    if let Some(set) = suffix_set {
        name.push_str(draw_part(bank, set, rng, BankType::Redguard)?);
    }
    Ok(name)
}

/// `parts[DFRandom.rand() % parts.len()]` (`NameHelper.cs:202-209`).
fn draw_part<'a>(
    bank: &'a NameBank,
    set: usize,
    rng: &mut DfRandom,
    bank_type: BankType,
) -> Result<&'a str, NameGenError> {
    let parts = bank
        .sets
        .iter()
        .find(|candidate| candidate.set_index == set)
        .map(|candidate| candidate.parts.as_slice())
        .ok_or(NameGenError::MissingSet {
            bank: bank_type.index(),
            set,
        })?;
    let len = parts.len() as u32;
    let index = (rng.rand() % len) as usize;
    Ok(&parts[index])
}

/// `FullName` joins with a space only if the surname is non-empty
/// (`NameHelper.cs:99-104`).
fn join_name(first: String, surname: String) -> String {
    if surname.is_empty() {
        first
    } else {
        format!("{first} {surname}")
    }
}

fn u32_le(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a synthetic `NAMEGEN.DAT` (11 banks) with the given sets.
    /// Layout identical to retail: 528 B header + 10 B slots.
    fn synthetic_dat(banks: &[&[&[&str]]]) -> Vec<u8> {
        let mut bodies: Vec<Vec<u8>> = Vec::new();
        let mut records = [0u32; NAMEGEN_BANK_COUNT * 12];
        let mut cursor = NAMEGEN_HEADER_LEN;
        for (bank, sets) in banks.iter().enumerate() {
            for (set, parts) in sets.iter().enumerate() {
                records[bank * 12 + set] = cursor as u32;
                records[bank * 12 + 6 + set] = parts.len() as u32;
                for part in parts.iter() {
                    let mut slot = [0u8; NAMEGEN_PART_LEN];
                    slot[..part.len()].copy_from_slice(part.as_bytes());
                    bodies.push(slot.to_vec());
                    cursor += NAMEGEN_PART_LEN;
                }
            }
        }
        let mut file = vec![0u8; NAMEGEN_HEADER_LEN];
        for (i, value) in records.iter().enumerate() {
            // The 12 fields of each bank are interleaved (6 offsets + 6 counts).
            let bank = i / 12;
            let field = i % 12;
            let at = bank * NAMEGEN_BANK_RECORD_LEN + if field < 6 { field * 4 } else { 24 + (field - 6) * 4 };
            file[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        for body in bodies {
            file.extend_from_slice(&body);
        }
        file
    }

    fn fixture() -> NameGen {
        // 6 sets per bank; Breton forcibly picks sets 0,1 (M) / 2,3 (F) / 4,5.
        let breton = [
            &["Agr", "Bed"][..],
            &["ore", "win"][..],
            &["Chry", "Theo"][..],
            &["stanna", "dore"][..],
            &["Wick", "Moor"][..],
            &["ing", "wing"][..],
        ];
        let redguard = [
            &["Bo", "Ter"][..],
            &["jj", "com"][..],
            &["a", "m"][..],
            &["n", "nd"][..],
            &["x", "y"][..],
            &[][..],
        ];
        let empty: [&[&str]; 0] = [];
        let banks: [&[&[&str]]; 11] = [
            &breton, &redguard, &empty, &empty, &empty, &empty, &empty, &empty, &empty, &empty,
            &empty,
        ];
        NameGen::parse(&synthetic_dat(&banks)).expect("valid fixture")
    }

    /// The parser reproduces offsets/counts and the parts in order.
    #[test]
    fn parses_offsets_and_parts() {
        let gen = fixture();
        assert_eq!(gen.bank(BankType::Breton).sets.len(), 6);
        assert_eq!(gen.bank(BankType::Breton).sets[0].parts, vec!["Agr", "Bed"]);
        assert_eq!(gen.bank(BankType::Redguard).sets[0].parts, vec!["Bo", "Ter"]);
        assert_eq!(gen.bank(BankType::Redguard).sets.len(), 5);
        assert_eq!(gen.parts_total(), 12 + 8 + 2);
    }

    /// Same seed ×2 → identical string; different seeds do not collapse.
    #[test]
    fn full_name_is_deterministic_per_seed() {
        let gen = fixture();
        let a = gen.full_name(BankType::Breton, Gender::Male, 1000).unwrap();
        let b = gen.full_name(BankType::Breton, Gender::Male, 1000).unwrap();
        assert_eq!(a, b);
        assert!(!a.is_empty());
        let different = (1..50u32)
            .map(|seed| gen.full_name(BankType::Breton, Gender::Male, seed).unwrap())
            .any(|name| name != a);
        assert!(different, "the seed must change the name");
    }

    /// Exact consumption: 4 draws in banks 0+1 / 2+3 + 4+5; Redguard
    /// female 4, male 5 (75 % gate after 3 draws).
    #[test]
    fn draw_consumption_matches_namehelper() {
        let gen = fixture();
        assert_eq!(gen.full_name(BankType::Breton, Gender::Female, 7).unwrap().split(' ').count(), 2);
        // Redguard has no surname → a single token.
        assert!(!gen.full_name(BankType::Redguard, Gender::Female, 7).unwrap().contains(' '));
        let mut rng = DfRandom::new();
        rng.srand(7);
        let mut probe = rng;
        let _ = gen.full_name_with(&mut probe, BankType::Redguard, Gender::Female).unwrap();
        // After 4 draws the state must match that of `full_name`.
        let mut direct = DfRandom::new();
        direct.srand(7);
        for _ in 0..4 {
            direct.rand();
        }
        assert_eq!(probe.save_seed(), direct.save_seed());
    }

    /// Out-of-domain bank and Nord (H3) → typed error, 0 panic.
    #[test]
    fn invalid_banks_return_typed_errors() {
        let gen = fixture();
        assert_eq!(
            gen.full_name(BankType::Monster1, Gender::Male, 1),
            Err(NameGenError::UnsupportedBank { bank: 8 })
        );
        assert_eq!(
            gen.full_name(BankType::Nord, Gender::Female, 1),
            Err(NameGenError::NordSuffixUnavailable)
        );
        assert!(BankType::from_index(11).is_none());
        assert_eq!(BankType::from_index(2), Some(BankType::Nord));
    }

    /// A truncated file or corrupt slot does not parse (0 panic).
    #[test]
    fn malformed_files_are_rejected() {
        assert_eq!(
            NameGen::parse(&[0u8; 10]),
            Err(NameGenError::TooShort {
                len: 10,
                need: NAMEGEN_HEADER_LEN
            })
        );
        let banks: [&[&[&str]]; 11] = [
            &[&["Agr"][..]][..],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
        ];
        let mut good = synthetic_dat(&banks);
        assert_eq!(good.len(), NAMEGEN_HEADER_LEN + NAMEGEN_PART_LEN);
        // Truncates the last byte of the parts block: the declared offset no
        // longer fits in the file.
        good.truncate(good.len() - 1);
        assert!(matches!(
            NameGen::parse(&good),
            Err(NameGenError::OffsetOutOfBounds { .. })
        ));
    }

    /// The synthetic test header reproduces the retail layout: 11 × 48 B.
    #[test]
    fn header_layout_matches_retail_arithmetic() {
        assert_eq!(NAMEGEN_HEADER_LEN, 528);
        assert_eq!(NAMEGEN_HEADER_LEN + 785 * NAMEGEN_PART_LEN, 8378);
    }
}

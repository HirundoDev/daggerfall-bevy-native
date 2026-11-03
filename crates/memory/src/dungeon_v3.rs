//! Save v3 dungeon families (M44 block E, spec §2).
//!
//! The v3 document adds **four** families to the twelve v2 sections, in this
//! fixed order: `dungeon_state`, `dungeon_elig`, `dungeon_loot`,
//! `dungeon_instance`. Each one is opened by a `<family>_count=<n>` line
//! followed by `n` `<family>=<csv>` lines; the CSV arity is closed and every
//! field has a declared kind (unsigned range, enum spelling or 16 lowercase
//! hex digits).
//!
//! ## One table, one parser
//!
//! The grammar is data ([`Family::spec`]) and the parse loop exists once
//! ([`DungeonSection::parse_lines`]): the four families differ only in their
//! table, never in their code. Emission is four tiny writers, and
//! [`DungeonSection::validate`] re-emits and re-parses the section, so the
//! writer and the reader cannot drift apart.
//!
//! ## Closed rejection tokens (§2.5)
//!
//! A rejected document is described by a [`GrammarToken`], never by prose:
//! the caller decides how to name it. [`DocumentError`] carries the token of
//! the **first** defect in the declared precedence order
//! (`unknown_key` → `missing_family` → `count_mismatch` → `duplicate_key` →
//! `order` → field token), and it carries the v2 body's own error untouched
//! when the defect is in the twelve inherited sections (never re-labelled).
//!
//! ## Declared divergence
//!
//! Inside one entry the **first offending field** wins (the reference parser
//! of the block document does the same); the §2.5 token order is applied
//! across the document, not across the fields of a single row. A defect in
//! the inherited v2 body (an unknown key between two fixed sections) is the
//! v2 grammar's own [`SaveError`], not `unknown_key`: the token names the v3
//! tail, and the v2 body keeps its historical diagnosis.

use std::fmt;

use crate::session_store::SaveError;

// ═══════════════════════════════════════════════════════════════════════════
// Closed rejection tokens (§2.5)
// ═══════════════════════════════════════════════════════════════════════════

/// The closed set of tokens a rejected document is described with (§2.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrammarToken {
    /// The first line is not `DFL-SESSION <version>`.
    BadMagic,
    /// The header version is not a canonical unsigned integer of 2 or 3.
    BadVersion,
    /// The document does not end with a `checksum=` line.
    NoChecksum,
    /// The checksum line is not 16 lowercase hex digits, or does not match.
    BadChecksum,
    /// A v2 document carries a dungeon family: the families require v3.
    FamilyRequiresV3,
    /// A key outside the closed v2 vocabulary and the four families.
    UnknownKey,
    /// A v3 document does not declare one of the four families.
    MissingFamily,
    /// A `<family>_count` disagrees with the entries that follow it.
    CountMismatch,
    /// Two rows of one family share their order key (§2.3).
    DuplicateKey,
    /// A family is not strictly ascending by its order key (§2.3).
    Order,
    /// An entry does not carry exactly the declared number of fields.
    BadFieldCount,
    /// An unsigned field is not a canonical unsigned integer.
    BadInt,
    /// An unsigned field is outside its declared range.
    OutOfRange,
    /// An enum field is not one of the declared spellings.
    BadEnum,
    /// A hex field is not exactly 16 lowercase hex digits.
    BadHex,
    /// `looted` and `looted_minutes` contradict each other (DELTA-D1).
    BadLootMinutes,
}

impl GrammarToken {
    /// Exact spelling of the token in the block document.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BadMagic => "bad_magic",
            Self::BadVersion => "bad_version",
            Self::NoChecksum => "no_checksum",
            Self::BadChecksum => "bad_checksum",
            Self::FamilyRequiresV3 => "family_requires_v3",
            Self::UnknownKey => "unknown_key",
            Self::MissingFamily => "missing_family",
            Self::CountMismatch => "count_mismatch",
            Self::DuplicateKey => "duplicate_key",
            Self::Order => "order",
            Self::BadFieldCount => "bad_field_count",
            Self::BadInt => "bad_int",
            Self::OutOfRange => "out_of_range",
            Self::BadEnum => "bad_enum",
            Self::BadHex => "bad_hex",
            Self::BadLootMinutes => "bad_loot_minutes",
        }
    }
}

impl fmt::Display for GrammarToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Why a document was refused: the §2.5 token of its first defect, the
/// version it declared, or the inherited v2 body's own error.
#[derive(Debug)]
pub struct DocumentError {
    token: Option<GrammarToken>,
    version: Option<i64>,
    detail: String,
    body: Option<SaveError>,
}

impl DocumentError {
    /// A defect of the v3 grammar, named by its token.
    pub(crate) fn reject(token: GrammarToken, detail: impl Into<String>) -> Self {
        Self {
            token: Some(token),
            version: None,
            detail: detail.into(),
            body: None,
        }
    }

    /// A header version this build does not know.
    pub(crate) fn bad_version(version: i64, detail: impl Into<String>) -> Self {
        Self {
            token: None,
            version: Some(version),
            detail: detail.into(),
            body: None,
        }
    }

    /// A defect of the inherited v2 body, kept exactly as the v2 parser said.
    pub(crate) fn body(error: SaveError) -> Self {
        Self {
            token: None,
            version: None,
            detail: error.to_string(),
            body: Some(error),
        }
    }

    /// An internal invariant broke while reading validated data (never a
    /// document defect: the table and the builder disagree).
    pub(crate) fn corrupt(kind: &str, detail: impl Into<String>) -> Self {
        Self {
            token: None,
            version: None,
            detail: format!("{kind}: {}", detail.into()),
            body: None,
        }
    }

    /// The §2.5 token of the first defect (`None` for a body error or a
    /// version this build does not know).
    pub fn token(&self) -> Option<GrammarToken> {
        self.token
    }

    /// The declared version, when the header itself was the defect.
    pub fn version(&self) -> Option<i64> {
        self.version
    }

    /// Human-readable detail of the first defect.
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for DocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.token, self.version) {
            (Some(token), _) => write!(formatter, "{token}: {}", self.detail),
            (None, Some(version)) => {
                write!(formatter, "bad_version: {version} ({})", self.detail)
            }
            (None, None) => formatter.write_str(&self.detail),
        }
    }
}

/// Maps a document refusal into the error the callers already handle.
///
/// `bad_magic` and `bad_version` keep their own variants (the v2 reader's
/// historical diagnosis); every other token is a [`SaveError::Corrupt`] whose
/// message starts with the token, so a caller can still tell them apart.
impl From<DocumentError> for SaveError {
    fn from(error: DocumentError) -> Self {
        if let Some(body) = error.body {
            return body;
        }
        match (error.token, error.version) {
            (Some(GrammarToken::BadMagic), _) => Self::BadMagic,
            (_, Some(version)) => Self::BadVersion(version),
            (Some(token), _) => Self::Corrupt(format!("{token}: {}", error.detail)),
            (None, None) => Self::Corrupt(error.detail),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// The four families, as data
// ═══════════════════════════════════════════════════════════════════════════

/// Spellings of `dungeon_state.machine`.
pub const MACHINE_STATES: &[&str] = &["Dormant", "Open", "Engaged", "Cleared", "Cooldown", "Closed"];
/// Spellings of `dungeon_loot.source`.
pub const LOOT_SOURCES: &[&str] = &["Chest", "Drop", "Boss"];

/// Kind of one dungeon field (§2.2).
#[derive(Debug, Clone, Copy)]
pub enum FieldKind {
    /// Unsigned integer with its inclusive range.
    Uint {
        /// Lowest accepted value.
        min: u64,
        /// Highest accepted value.
        max: u64,
    },
    /// One of the declared spellings, case sensitive.
    Enum(&'static [&'static str]),
    /// Exactly 16 lowercase hexadecimal digits.
    Hex16,
}

/// One field of one family.
#[derive(Debug, Clone, Copy)]
pub struct Field {
    /// Column name in the block document.
    pub name: &'static str,
    /// Declared kind of the column.
    pub kind: FieldKind,
}

/// One dungeon family: its key, its columns and its order key (§2.3).
#[derive(Debug, Clone, Copy)]
pub struct FamilySpec {
    /// Line key of an entry (`dungeon_state`, ...).
    pub key: &'static str,
    /// Line key of the count (`dungeon_state_count`, ...).
    pub count_key: &'static str,
    /// Columns, in CSV order.
    pub fields: &'static [Field],
    /// Field indices of the order key, in comparison order.
    pub order: &'static [usize],
}

/// The four dungeon families, in canonical order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// `dungeon_state` — open/cleared doors and machines.
    State,
    /// `dungeon_elig` — containers the player may still loot.
    Elig,
    /// `dungeon_loot` — looted containers and their deadlines.
    Loot,
    /// `dungeon_instance` — placed dungeon instances.
    Instance,
}

const STATE_FIELDS: &[Field] = &[
    Field { name: "location_id", kind: FieldKind::Uint { min: 0, max: 65535 } },
    Field { name: "block_index", kind: FieldKind::Uint { min: 0, max: 255 } },
    Field { name: "door_id", kind: FieldKind::Uint { min: 0, max: 4_294_967_295 } },
    Field { name: "machine", kind: FieldKind::Enum(MACHINE_STATES) },
    Field { name: "since_minutes", kind: FieldKind::Uint { min: 0, max: u64::MAX } },
    Field { name: "epoch", kind: FieldKind::Uint { min: 1, max: 4_294_967_295 } },
];
const ELIG_FIELDS: &[Field] = &[
    Field { name: "token", kind: FieldKind::Hex16 },
    Field { name: "location_id", kind: FieldKind::Uint { min: 0, max: 65535 } },
    Field { name: "container_id", kind: FieldKind::Uint { min: 0, max: 4_294_967_295 } },
    Field { name: "player_id", kind: FieldKind::Uint { min: 0, max: 255 } },
    Field { name: "epoch", kind: FieldKind::Uint { min: 1, max: 4_294_967_295 } },
    Field { name: "seen", kind: FieldKind::Uint { min: 0, max: 1 } },
];
const LOOT_FIELDS: &[Field] = &[
    Field { name: "location_id", kind: FieldKind::Uint { min: 0, max: 65535 } },
    Field { name: "container_id", kind: FieldKind::Uint { min: 0, max: 4_294_967_295 } },
    Field { name: "source", kind: FieldKind::Enum(LOOT_SOURCES) },
    Field { name: "spawn_minutes", kind: FieldKind::Uint { min: 0, max: u64::MAX } },
    Field { name: "looted_minutes", kind: FieldKind::Uint { min: 0, max: u64::MAX } },
    Field { name: "looted", kind: FieldKind::Uint { min: 0, max: 1 } },
    Field { name: "epoch", kind: FieldKind::Uint { min: 1, max: 4_294_967_295 } },
];
const INSTANCE_FIELDS: &[Field] = &[
    Field { name: "instance_id", kind: FieldKind::Hex16 },
    Field { name: "location_id", kind: FieldKind::Uint { min: 0, max: 65535 } },
    Field { name: "epoch", kind: FieldKind::Uint { min: 1, max: 4_294_967_295 } },
    Field { name: "player_id", kind: FieldKind::Uint { min: 0, max: 255 } },
];

const STATE_SPEC: FamilySpec = FamilySpec {
    key: "dungeon_state",
    count_key: "dungeon_state_count",
    fields: STATE_FIELDS,
    order: &[0, 1, 2],
};
const ELIG_SPEC: FamilySpec = FamilySpec {
    key: "dungeon_elig",
    count_key: "dungeon_elig_count",
    fields: ELIG_FIELDS,
    order: &[1, 2, 3, 4],
};
const LOOT_SPEC: FamilySpec = FamilySpec {
    key: "dungeon_loot",
    count_key: "dungeon_loot_count",
    fields: LOOT_FIELDS,
    order: &[0, 1, 6],
};
const INSTANCE_SPEC: FamilySpec = FamilySpec {
    key: "dungeon_instance",
    count_key: "dungeon_instance_count",
    fields: INSTANCE_FIELDS,
    order: &[1, 2, 0],
};

impl Family {
    /// The four families in canonical order.
    pub const ALL: [Self; 4] = [Self::State, Self::Elig, Self::Loot, Self::Instance];

    /// The table of this family.
    pub const fn spec(self) -> &'static FamilySpec {
        match self {
            Self::State => &STATE_SPEC,
            Self::Elig => &ELIG_SPEC,
            Self::Loot => &LOOT_SPEC,
            Self::Instance => &INSTANCE_SPEC,
        }
    }

    /// The family whose entry key is `key`, if any.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|family| family.spec().key == key)
    }

    /// The family whose count key is `key`, if any.
    pub fn from_count_key(key: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|family| family.spec().count_key == key)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// The typed rows
// ═══════════════════════════════════════════════════════════════════════════

/// `dungeon_state` — a door or machine of one dungeon block (§2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DungeonStateRow {
    /// Dungeon location index inside the region.
    pub location_id: u16,
    /// Block index inside the dungeon.
    pub block_index: u8,
    /// Door/machine id inside the block.
    pub door_id: u32,
    /// State of the machine.
    pub machine: MachineState,
    /// Game minutes since the state was entered.
    pub since_minutes: u64,
    /// Instance epoch the row belongs to.
    pub epoch: u32,
}

/// `dungeon_state.machine` (§2.2, closed set).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachineState {
    /// Never opened.
    Dormant,
    /// Opened and still open.
    Open,
    /// Being used.
    Engaged,
    /// Cleared for this epoch.
    Cleared,
    /// Recharging.
    Cooldown,
    /// Closed again.
    Closed,
}

impl MachineState {
    /// The declared spelling of this state.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dormant => "Dormant",
            Self::Open => "Open",
            Self::Engaged => "Engaged",
            Self::Cleared => "Cleared",
            Self::Cooldown => "Cooldown",
            Self::Closed => "Closed",
        }
    }

    /// The state of a declared spelling.
    pub fn parse(spelling: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|state| state.as_str() == spelling)
    }

    const ALL: [Self; 6] = [
        Self::Dormant,
        Self::Open,
        Self::Engaged,
        Self::Cleared,
        Self::Cooldown,
        Self::Closed,
    ];
}

/// `dungeon_elig` — a container the player may still loot (§2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DungeonEligRow {
    /// Opaque 64-bit token (16 lowercase hex digits).
    pub token: [u8; 8],
    /// Dungeon location index inside the region.
    pub location_id: u16,
    /// Container id inside the block.
    pub container_id: u32,
    /// Player the eligibility belongs to.
    pub player_id: u8,
    /// Instance epoch the row belongs to.
    pub epoch: u32,
    /// Whether the container was already seen.
    pub seen: bool,
}

/// `dungeon_loot` — a looted container and its respawn deadline (§2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DungeonLootRow {
    /// Dungeon location index inside the region.
    pub location_id: u16,
    /// Container id inside the block.
    pub container_id: u32,
    /// Where the loot came from.
    pub source: LootSource,
    /// Game minute the container spawned.
    pub spawn_minutes: u64,
    /// Game minute it was looted (`0` = never, DELTA-D1).
    pub looted_minutes: u64,
    /// Whether it was looted.
    pub looted: bool,
    /// Instance epoch the row belongs to.
    pub epoch: u32,
}

/// `dungeon_loot.source` (§2.2, closed set).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LootSource {
    /// A chest.
    Chest,
    /// A drop.
    Drop,
    /// A boss reward.
    Boss,
}

impl LootSource {
    /// The declared spelling of this source.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Chest => "Chest",
            Self::Drop => "Drop",
            Self::Boss => "Boss",
        }
    }

    /// The source of a declared spelling.
    pub fn parse(spelling: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|source| source.as_str() == spelling)
    }

    const ALL: [Self; 3] = [Self::Chest, Self::Drop, Self::Boss];
}

/// `dungeon_instance` — a placed dungeon instance (§2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DungeonInstanceRow {
    /// Opaque 64-bit instance id (16 lowercase hex digits).
    pub instance_id: [u8; 8],
    /// Dungeon location index inside the region.
    pub location_id: u16,
    /// Instance epoch.
    pub epoch: u32,
    /// Player the instance belongs to.
    pub player_id: u8,
}

/// The four v3 dungeon families, empty by default (§6).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DungeonSection {
    /// `dungeon_state` rows, ascending by `(location_id, block_index, door_id)`.
    pub states: Vec<DungeonStateRow>,
    /// `dungeon_elig` rows, ascending by `(location_id, container_id, player_id, epoch)`.
    pub eligible: Vec<DungeonEligRow>,
    /// `dungeon_loot` rows, ascending by `(location_id, container_id, epoch)`.
    pub loot: Vec<DungeonLootRow>,
    /// `dungeon_instance` rows, ascending by `(location_id, epoch, instance_id)`.
    pub instances: Vec<DungeonInstanceRow>,
}

impl DungeonSection {
    /// Entries across the four families.
    pub fn entry_count(&self) -> usize {
        self.states.len() + self.eligible.len() + self.loot.len() + self.instances.len()
    }

    /// Lines this section writes: four counts plus every entry.
    pub fn line_count(&self) -> usize {
        Family::ALL.len() + self.entry_count()
    }

    /// Whether the section carries no row at all.
    pub fn is_empty(&self) -> bool {
        self.entry_count() == 0
    }

    /// Appends the canonical lines of the section (`<family>_count=` then the
    /// entries, family by family in the declared order).
    pub fn write_lines(&self, out: &mut String) {
        // One family at a time: its count line, then its entries. The tail is
        // closed and ordered (§2.3), so the four counts never travel together.
        for family in Family::ALL {
            out.push_str(family.spec().count_key);
            out.push('=');
            out.push_str(&self.rows(family).to_string());
            out.push('\n');
            match family {
                Family::State => self.states.iter().for_each(|row| push_state(out, row)),
                Family::Elig => self.eligible.iter().for_each(|row| push_elig(out, row)),
                Family::Loot => self.loot.iter().for_each(|row| push_loot(out, row)),
                Family::Instance => self.instances.iter().for_each(|row| push_instance(out, row)),
            }
        }
    }

    /// Number of rows of `family` (the value its count line declares).
    pub fn rows(&self, family: Family) -> usize {
        match family {
            Family::State => self.states.len(),
            Family::Elig => self.eligible.len(),
            Family::Loot => self.loot.len(),
            Family::Instance => self.instances.len(),
        }
    }

    /// Parses the four families from `key=value` lines, in canonical order.
    ///
    /// The slice must hold **exactly** the dungeon lines: a key outside the
    /// four families is `unknown_key`, an absent family is `missing_family`, a
    /// count that disagrees with its entries is `count_mismatch`, and a
    /// repeated or unordered order key is `duplicate_key` / `order` (§2.5).
    pub fn parse_lines(lines: &[(&str, &str)]) -> Result<Self, DocumentError> {
        for (key, _) in lines {
            if Family::from_key(key).is_none() && Family::from_count_key(key).is_none() {
                return Err(DocumentError::reject(
                    GrammarToken::UnknownKey,
                    format!("{key:?} is not a dungeon family key"),
                ));
            }
        }

        let mut section = Self::default();
        let mut at = 0usize;
        for family in Family::ALL {
            let spec = family.spec();
            let Some((key, raw_count)) = lines.get(at).copied() else {
                return Err(DocumentError::reject(
                    GrammarToken::MissingFamily,
                    format!("{} is absent", spec.count_key),
                ));
            };
            if key != spec.count_key {
                let later = lines[at..]
                    .iter()
                    .any(|(candidate, _)| *candidate == spec.count_key);
                let token = if later {
                    GrammarToken::Order
                } else {
                    GrammarToken::MissingFamily
                };
                return Err(DocumentError::reject(
                    token,
                    format!("expected {:?}, found {key:?}", spec.count_key),
                ));
            }
            let count = canonical_uint(raw_count).map_err(|_| {
                DocumentError::reject(
                    GrammarToken::CountMismatch,
                    format!("{}={raw_count:?} is not a canonical count", spec.count_key),
                )
            })?;
            let count = usize::try_from(count).map_err(|_| {
                DocumentError::reject(
                    GrammarToken::CountMismatch,
                    format!("{}={count} does not fit this platform", spec.count_key),
                )
            })?;
            at += 1;

            let mut raws: Vec<&str> = Vec::new();
            while let Some((key, value)) = lines.get(at).copied() {
                if key != spec.key {
                    break;
                }
                raws.push(value);
                at += 1;
            }
            if raws.len() != count {
                return Err(DocumentError::reject(
                    GrammarToken::CountMismatch,
                    format!(
                        "{} declares {count}, the document carries {}",
                        spec.key,
                        raws.len()
                    ),
                ));
            }

            let mut keys: Vec<Vec<u64>> = Vec::with_capacity(raws.len());
            for raw in &raws {
                let values = parse_entry(family, raw)?;
                keys.push(sort_key(spec, &values));
                push_row(&mut section, family, &values)?;
            }
            for index in 0..keys.len() {
                if keys[..index].contains(&keys[index]) {
                    return Err(DocumentError::reject(
                        GrammarToken::DuplicateKey,
                        format!("{}: two rows share the key {:?}", spec.key, keys[index]),
                    ));
                }
            }
            for pair in keys.windows(2) {
                if pair[1] < pair[0] {
                    return Err(DocumentError::reject(
                        GrammarToken::Order,
                        format!(
                            "{}: {:?} must come after {:?}",
                            spec.key, pair[0], pair[1]
                        ),
                    ));
                }
            }
        }

        if at != lines.len() {
            let key = lines[at].0;
            let token = if Family::from_key(key).is_some() {
                GrammarToken::Order
            } else {
                GrammarToken::UnknownKey
            };
            return Err(DocumentError::reject(
                token,
                format!("{key:?} is out of place in the dungeon tail"),
            ));
        }
        Ok(section)
    }

    /// Checks the section's invariants by re-emitting and re-parsing it: the
    /// writer and the reader cannot drift apart, and a hand-built section with
    /// an unordered family, a contradictory loot row or a lossy row is refused.
    pub fn validate(&self) -> Result<(), DocumentError> {
        let mut text = String::new();
        self.write_lines(&mut text);
        let lines: Vec<(&str, &str)> = text
            .lines()
            .filter_map(|line| line.split_once('='))
            .collect();
        let reparsed = Self::parse_lines(&lines)?;
        if reparsed != *self {
            return Err(DocumentError::corrupt(
                "dungeon_section",
                "the section is not the canonical form of its own rows",
            ));
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Field parsing
// ═══════════════════════════════════════════════════════════════════════════

/// One validated field value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldValue {
    /// A canonical unsigned integer inside its declared range.
    Uint(u64),
    /// One of the declared spellings.
    Enum(&'static str),
    /// Exactly 16 lowercase hex digits, big endian.
    Hex([u8; 8]),
}

impl FieldValue {
    fn uint(self) -> Result<u64, DocumentError> {
        match self {
            Self::Uint(number) => Ok(number),
            other => Err(DocumentError::corrupt(
                "field_kind",
                format!("expected an unsigned field, found {other:?}"),
            )),
        }
    }

    fn enum_key(self) -> Result<&'static str, DocumentError> {
        match self {
            Self::Enum(spelling) => Ok(spelling),
            other => Err(DocumentError::corrupt(
                "field_kind",
                format!("expected an enum field, found {other:?}"),
            )),
        }
    }

    fn hex(self) -> Result<[u8; 8], DocumentError> {
        match self {
            Self::Hex(bytes) => Ok(bytes),
            other => Err(DocumentError::corrupt(
                "field_kind",
                format!("expected a hex field, found {other:?}"),
            )),
        }
    }
}

/// A canonical unsigned integer: ASCII digits only, no sign and no leading
/// zero unless the value is `0` (Rust's `parse` accepts `+1` and `001`).
fn canonical_uint(text: &str) -> Result<u64, GrammarToken> {
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return Err(GrammarToken::BadInt);
    }
    if bytes.len() > 1 && bytes[0] == b'0' {
        return Err(GrammarToken::BadInt);
    }
    let mut value: u64 = 0;
    for byte in bytes {
        if !byte.is_ascii_digit() {
            return Err(GrammarToken::BadInt);
        }
        let digit = u64::from(byte - b'0');
        value = match value.checked_mul(10).and_then(|v| v.checked_add(digit)) {
            Some(value) => value,
            // Too large for the declared type: the range check names it.
            None => return Err(GrammarToken::OutOfRange),
        };
    }
    Ok(value)
}

/// Exactly 16 lowercase hex digits, big endian.
fn hex16(text: &str) -> Option<[u8; 8]> {
    let bytes = text.as_bytes();
    if bytes.len() != 16 {
        return None;
    }
    let mut out = [0u8; 8];
    for (index, pair) in bytes.chunks(2).enumerate() {
        out[index] = (nibble(pair[0])? << 4) | nibble(pair[1])?;
    }
    Some(out)
}

fn nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

/// Parses one CSV entry of `family`, in the declared field order.
fn parse_entry(family: Family, raw: &str) -> Result<Vec<FieldValue>, DocumentError> {
    let spec = family.spec();
    let parts: Vec<&str> = raw.split(',').collect();
    if parts.len() != spec.fields.len() {
        return Err(DocumentError::reject(
            GrammarToken::BadFieldCount,
            format!(
                "{}: {} fields, want {}",
                spec.key,
                parts.len(),
                spec.fields.len()
            ),
        ));
    }
    let mut values = Vec::with_capacity(parts.len());
    for (index, field) in spec.fields.iter().enumerate() {
        let part = parts[index];
        let value = match field.kind {
            FieldKind::Uint { min, max } => {
                let number = canonical_uint(part).map_err(|token| {
                    DocumentError::reject(token, format!("{}.{}={part:?}", spec.key, field.name))
                })?;
                if number < min || number > max {
                    return Err(DocumentError::reject(
                        GrammarToken::OutOfRange,
                        format!(
                            "{}.{}={number} is outside {min}..={max}",
                            spec.key, field.name
                        ),
                    ));
                }
                FieldValue::Uint(number)
            }
            FieldKind::Enum(allowed) => {
                let spelling = allowed.iter().find(|spelling| **spelling == part);
                match spelling {
                    Some(spelling) => FieldValue::Enum(spelling),
                    None => {
                        return Err(DocumentError::reject(
                            GrammarToken::BadEnum,
                            format!(
                                "{}.{}={part:?} is not one of {allowed:?}",
                                spec.key, field.name
                            ),
                        ))
                    }
                }
            }
            FieldKind::Hex16 => match hex16(part) {
                Some(bytes) => FieldValue::Hex(bytes),
                None => {
                    return Err(DocumentError::reject(
                        GrammarToken::BadHex,
                        format!(
                            "{}.{}={part:?} is not 16 lowercase hex digits",
                            spec.key, field.name
                        ),
                    ))
                }
            },
        };
        values.push(value);
    }
    Ok(values)
}

/// The order key of one row (§2.3), as comparable numbers. No order key
/// contains an enum field, so the enum arm is unreachable by table
/// construction and sorts as `0`.
fn sort_key(spec: &FamilySpec, values: &[FieldValue]) -> Vec<u64> {
    spec.order
        .iter()
        .map(|index| match values[*index] {
            FieldValue::Uint(number) => number,
            FieldValue::Hex(bytes) => u64::from_be_bytes(bytes),
            FieldValue::Enum(_) => 0,
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════════
// Typed rows from validated fields
// ═══════════════════════════════════════════════════════════════════════════

fn push_row(
    section: &mut DungeonSection,
    family: Family,
    values: &[FieldValue],
) -> Result<(), DocumentError> {
    match family {
        Family::State => section.states.push(build_state(values)?),
        Family::Elig => section.eligible.push(build_elig(values)?),
        Family::Loot => section.loot.push(build_loot(values)?),
        Family::Instance => section.instances.push(build_instance(values)?),
    }
    Ok(())
}

fn field(values: &[FieldValue], index: usize) -> Result<FieldValue, DocumentError> {
    values.get(index).copied().ok_or_else(|| {
        DocumentError::corrupt("field_index", format!("column {index} is absent"))
    })
}

fn build_state(values: &[FieldValue]) -> Result<DungeonStateRow, DocumentError> {
    let location_id = u16::try_from(field(values, 0)?.uint()?).map_err(|_| narrow("location_id"))?;
    let block_index = u8::try_from(field(values, 1)?.uint()?).map_err(|_| narrow("block_index"))?;
    let door_id = u32::try_from(field(values, 2)?.uint()?).map_err(|_| narrow("door_id"))?;
    let machine = MachineState::parse(field(values, 3)?.enum_key()?)
        .ok_or_else(|| DocumentError::corrupt("machine", "the validated spelling is unknown"))?;
    let since_minutes = field(values, 4)?.uint()?;
    let epoch = u32::try_from(field(values, 5)?.uint()?).map_err(|_| narrow("epoch"))?;
    Ok(DungeonStateRow {
        location_id,
        block_index,
        door_id,
        machine,
        since_minutes,
        epoch,
    })
}

fn build_elig(values: &[FieldValue]) -> Result<DungeonEligRow, DocumentError> {
    let token = field(values, 0)?.hex()?;
    let location_id = u16::try_from(field(values, 1)?.uint()?).map_err(|_| narrow("location_id"))?;
    let container_id =
        u32::try_from(field(values, 2)?.uint()?).map_err(|_| narrow("container_id"))?;
    let player_id = u8::try_from(field(values, 3)?.uint()?).map_err(|_| narrow("player_id"))?;
    let epoch = u32::try_from(field(values, 4)?.uint()?).map_err(|_| narrow("epoch"))?;
    let seen = field(values, 5)?.uint()? == 1;
    Ok(DungeonEligRow {
        token,
        location_id,
        container_id,
        player_id,
        epoch,
        seen,
    })
}

fn build_loot(values: &[FieldValue]) -> Result<DungeonLootRow, DocumentError> {
    let location_id = u16::try_from(field(values, 0)?.uint()?).map_err(|_| narrow("location_id"))?;
    let container_id =
        u32::try_from(field(values, 1)?.uint()?).map_err(|_| narrow("container_id"))?;
    let source = LootSource::parse(field(values, 2)?.enum_key()?)
        .ok_or_else(|| DocumentError::corrupt("source", "the validated spelling is unknown"))?;
    let spawn_minutes = field(values, 3)?.uint()?;
    let looted_minutes = field(values, 4)?.uint()?;
    let looted = field(values, 5)?.uint()? == 1;
    let epoch = u32::try_from(field(values, 6)?.uint()?).map_err(|_| narrow("epoch"))?;
    // DELTA-D1 (§2.2): minute 0 is the sentinel "never looted", so a looted
    // container can never carry minute 0 and a never-looted one can never
    // carry a minute. The grammar refuses the contradiction instead of handing
    // the consumer a fake deadline.
    if !looted && looted_minutes != 0 {
        return Err(DocumentError::reject(
            GrammarToken::BadLootMinutes,
            format!("dungeon_loot: looted=0 with looted_minutes={looted_minutes}"),
        ));
    }
    if looted && looted_minutes == 0 {
        return Err(DocumentError::reject(
            GrammarToken::BadLootMinutes,
            "dungeon_loot: looted=1 with looted_minutes=0".to_string(),
        ));
    }
    Ok(DungeonLootRow {
        location_id,
        container_id,
        source,
        spawn_minutes,
        looted_minutes,
        looted,
        epoch,
    })
}

fn build_instance(values: &[FieldValue]) -> Result<DungeonInstanceRow, DocumentError> {
    let instance_id = field(values, 0)?.hex()?;
    let location_id = u16::try_from(field(values, 1)?.uint()?).map_err(|_| narrow("location_id"))?;
    let epoch = u32::try_from(field(values, 2)?.uint()?).map_err(|_| narrow("epoch"))?;
    let player_id = u8::try_from(field(values, 3)?.uint()?).map_err(|_| narrow("player_id"))?;
    Ok(DungeonInstanceRow {
        instance_id,
        location_id,
        epoch,
        player_id,
    })
}

fn narrow(column: &str) -> DocumentError {
    DocumentError::corrupt("field_width", format!("{column} does not fit its type"))
}

// ═══════════════════════════════════════════════════════════════════════════
// Canonical writers
// ═══════════════════════════════════════════════════════════════════════════

/// 16 lowercase hex digits, big endian.
fn hex(value: [u8; 8]) -> String {
    format!("{:016x}", u64::from_be_bytes(value))
}

fn push_state(out: &mut String, row: &DungeonStateRow) {
    out.push_str("dungeon_state=");
    out.push_str(&row.location_id.to_string());
    out.push(',');
    out.push_str(&row.block_index.to_string());
    out.push(',');
    out.push_str(&row.door_id.to_string());
    out.push(',');
    out.push_str(row.machine.as_str());
    out.push(',');
    out.push_str(&row.since_minutes.to_string());
    out.push(',');
    out.push_str(&row.epoch.to_string());
    out.push('\n');
}

fn push_elig(out: &mut String, row: &DungeonEligRow) {
    out.push_str("dungeon_elig=");
    out.push_str(&hex(row.token));
    out.push(',');
    out.push_str(&row.location_id.to_string());
    out.push(',');
    out.push_str(&row.container_id.to_string());
    out.push(',');
    out.push_str(&row.player_id.to_string());
    out.push(',');
    out.push_str(&row.epoch.to_string());
    out.push(',');
    out.push_str(if row.seen { "1" } else { "0" });
    out.push('\n');
}

fn push_loot(out: &mut String, row: &DungeonLootRow) {
    out.push_str("dungeon_loot=");
    out.push_str(&row.location_id.to_string());
    out.push(',');
    out.push_str(&row.container_id.to_string());
    out.push(',');
    out.push_str(row.source.as_str());
    out.push(',');
    out.push_str(&row.spawn_minutes.to_string());
    out.push(',');
    out.push_str(&row.looted_minutes.to_string());
    out.push(',');
    out.push_str(if row.looted { "1" } else { "0" });
    out.push(',');
    out.push_str(&row.epoch.to_string());
    out.push('\n');
}

fn push_instance(out: &mut String, row: &DungeonInstanceRow) {
    out.push_str("dungeon_instance=");
    out.push_str(&hex(row.instance_id));
    out.push(',');
    out.push_str(&row.location_id.to_string());
    out.push(',');
    out.push_str(&row.epoch.to_string());
    out.push(',');
    out.push_str(&row.player_id.to_string());
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_one(family: Family, raw: &str) -> Result<Vec<FieldValue>, DocumentError> {
        let values = parse_entry(family, raw)?;
        // The cross-field invariants live in the row builder, so an entry is
        // only accepted once it became a row (exactly as the document parser
        // does it).
        let mut scratch = DungeonSection::default();
        push_row(&mut scratch, family, &values)?;
        Ok(values)
    }

    fn token_of(result: Result<Vec<FieldValue>, DocumentError>) -> &'static str {
        match result {
            Ok(_) => "accept",
            Err(error) => match error.token() {
                Some(token) => token.as_str(),
                None => "body",
            },
        }
    }

    #[test]
    fn golden_vectors_of_the_block_document_are_the_parser() {
        // GV-E-01..08 — dungeon_state.
        let state = [
            ("26381,0,26,Dormant,142561,1", "accept"),
            ("0,0,0,Open,0,1", "accept"),
            ("65535,255,4294967295,Closed,18446744073709551615,4294967295", "accept"),
            ("26381,0,26,sleeping,142561,1", "bad_enum"),
            ("26381,256,26,Dormant,142561,1", "out_of_range"),
            ("26381,0,26,Dormant,142561,0", "out_of_range"),
            ("26381,0,26,Dormant,142561", "bad_field_count"),
            ("026381,0,26,Dormant,142561,1", "bad_int"),
        ];
        for (raw, want) in state {
            assert_eq!(token_of(parse_one(Family::State, raw)), want, "state {raw}");
        }
        // GV-E-09..16 — dungeon_loot (DELTA-D1 included).
        let loot = [
            ("26381,7,Chest,142561,0,0,1", "accept"),
            ("0,0,Drop,0,1,1,1", "accept"),
            ("65535,4294967295,Boss,18446744073709551615,18446744073709551615,1,4294967295", "accept"),
            ("26381,7,chest,142561,0,0,1", "bad_enum"),
            ("26381,7,Chest,142561,5,0,1", "bad_loot_minutes"),
            ("26381,7,Chest,142561,0,1,1", "bad_loot_minutes"),
            ("26381,7,Chest,142561,0,0", "bad_field_count"),
            ("26381,7,Chest,142561,0,2,1", "out_of_range"),
        ];
        for (raw, want) in loot {
            assert_eq!(token_of(parse_one(Family::Loot, raw)), want, "loot {raw}");
        }
        // GV-E-17..23 — dungeon_elig.
        let elig = [
            ("0123456789abcdef,26381,7,0,1,0", "accept"),
            ("ffffffffffffffff,65535,4294967295,255,4294967295,1", "accept"),
            ("0123456789ABCDEF,26381,7,0,1,0", "bad_hex"),
            ("0123456789abcde,26381,7,0,1,0", "bad_hex"),
            ("0123456789abcdef,26381,7,256,1,0", "out_of_range"),
            ("0123456789abcdef,26381,7,0,0,0", "out_of_range"),
            ("0123456789abcdef,26381,7,0,1", "bad_field_count"),
        ];
        for (raw, want) in elig {
            assert_eq!(token_of(parse_one(Family::Elig, raw)), want, "elig {raw}");
        }
        // GV-E-24..30 — dungeon_instance.
        let instance = [
            ("0123456789abcdef,26381,1,0", "accept"),
            ("0000000000000000,0,1,255", "accept"),
            ("0123456789abcdef,26381,1,0,9", "bad_field_count"),
            ("0123456789abcdeg,26381,1,0", "bad_hex"),
            ("0123456789abcdef,65536,1,0", "out_of_range"),
            ("0123456789abcdef,26381,1,256", "out_of_range"),
            ("0123456789abcdef,26381,4294967296,0", "out_of_range"),
        ];
        for (raw, want) in instance {
            assert_eq!(
                token_of(parse_one(Family::Instance, raw)),
                want,
                "instance {raw}"
            );
        }
    }

    #[test]
    fn canonical_uint_refuses_signs_and_leading_zeros() {
        assert_eq!(canonical_uint("0"), Ok(0));
        assert_eq!(canonical_uint("10"), Ok(10));
        assert_eq!(canonical_uint("01"), Err(GrammarToken::BadInt));
        assert_eq!(canonical_uint("+1"), Err(GrammarToken::BadInt));
        assert_eq!(canonical_uint("-1"), Err(GrammarToken::BadInt));
        assert_eq!(canonical_uint(""), Err(GrammarToken::BadInt));
        assert_eq!(
            canonical_uint("18446744073709551616"),
            Err(GrammarToken::OutOfRange)
        );
    }

    #[test]
    fn empty_section_is_four_counts_and_nothing_else() {
        let section = DungeonSection::default();
        let mut text = String::new();
        section.write_lines(&mut text);
        assert_eq!(
            text,
            "dungeon_state_count=0\ndungeon_elig_count=0\ndungeon_loot_count=0\n\
             dungeon_instance_count=0\n"
        );
        assert_eq!(section.line_count(), 4);
        assert!(section.is_empty());
        section.validate().expect("the empty section is valid");
    }

    #[test]
    fn one_entry_per_family_roundtrips_and_is_valid() {
        let section = DungeonSection {
            states: vec![DungeonStateRow {
                location_id: 26381,
                block_index: 0,
                door_id: 26,
                machine: MachineState::Dormant,
                since_minutes: 142561,
                epoch: 1,
            }],
            eligible: vec![DungeonEligRow {
                token: [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef],
                location_id: 26381,
                container_id: 7,
                player_id: 0,
                epoch: 1,
                seen: false,
            }],
            loot: vec![DungeonLootRow {
                location_id: 26381,
                container_id: 7,
                source: LootSource::Chest,
                spawn_minutes: 142561,
                looted_minutes: 0,
                looted: false,
                epoch: 1,
            }],
            instances: vec![DungeonInstanceRow {
                instance_id: [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef],
                location_id: 26381,
                epoch: 1,
                player_id: 0,
            }],
        };
        let mut text = String::new();
        section.write_lines(&mut text);
        assert_eq!(section.line_count(), 8);
        let lines: Vec<(&str, &str)> = text
            .lines()
            .filter_map(|line| line.split_once('='))
            .collect();
        assert_eq!(DungeonSection::parse_lines(&lines).expect("parses"), section);
        section.validate().expect("valid");
        assert!(text.contains("dungeon_elig=0123456789abcdef,26381,7,0,1,0\n"));
    }

    #[test]
    fn tail_tokens_are_named_in_the_declared_precedence() {
        let count = |family: Family| family.spec().count_key;
        let entry = |family: Family| family.spec().key;
        let unknown = DungeonSection::parse_lines(&[("dungeon_extra", "0")]);
        assert_eq!(unknown.expect_err("unknown").token(), Some(GrammarToken::UnknownKey));

        let missing = DungeonSection::parse_lines(&[
            (count(Family::State), "0"),
            (count(Family::Elig), "0"),
        ]);
        assert_eq!(missing.expect_err("missing").token(), Some(GrammarToken::MissingFamily));

        let mismatch = DungeonSection::parse_lines(&[
            (count(Family::State), "2"),
            (entry(Family::State), "0,0,0,Open,0,1"),
            (count(Family::Elig), "0"),
            (count(Family::Loot), "0"),
            (count(Family::Instance), "0"),
        ]);
        assert_eq!(
            mismatch.expect_err("count").token(),
            Some(GrammarToken::CountMismatch)
        );

        let duplicate = DungeonSection::parse_lines(&[
            (count(Family::State), "2"),
            (entry(Family::State), "0,0,0,Open,0,1"),
            (entry(Family::State), "0,0,0,Open,0,1"),
            (count(Family::Elig), "0"),
            (count(Family::Loot), "0"),
            (count(Family::Instance), "0"),
        ]);
        assert_eq!(
            duplicate.expect_err("duplicate").token(),
            Some(GrammarToken::DuplicateKey)
        );

        let unordered = DungeonSection::parse_lines(&[
            (count(Family::State), "2"),
            (entry(Family::State), "0,0,1,Open,0,1"),
            (entry(Family::State), "0,0,0,Open,0,1"),
            (count(Family::Elig), "0"),
            (count(Family::Loot), "0"),
            (count(Family::Instance), "0"),
        ]);
        assert_eq!(unordered.expect_err("order").token(), Some(GrammarToken::Order));
    }

    #[test]
    fn a_hand_built_section_with_an_unordered_family_is_refused() {
        let section = DungeonSection {
            states: vec![
                DungeonStateRow {
                    location_id: 2,
                    block_index: 0,
                    door_id: 0,
                    machine: MachineState::Open,
                    since_minutes: 0,
                    epoch: 1,
                },
                DungeonStateRow {
                    location_id: 1,
                    block_index: 0,
                    door_id: 0,
                    machine: MachineState::Open,
                    since_minutes: 0,
                    epoch: 1,
                },
            ],
            ..DungeonSection::default()
        };
        assert_eq!(
            section.validate().expect_err("unordered").token(),
            Some(GrammarToken::Order)
        );
    }

    #[test]
    fn a_contradictory_loot_row_is_refused_by_validate() {
        let section = DungeonSection {
            loot: vec![DungeonLootRow {
                location_id: 1,
                container_id: 1,
                source: LootSource::Drop,
                spawn_minutes: 0,
                looted_minutes: 5,
                looted: false,
                epoch: 1,
            }],
            ..DungeonSection::default()
        };
        assert_eq!(
            section.validate().expect_err("contradiction").token(),
            Some(GrammarToken::BadLootMinutes)
        );
    }
}

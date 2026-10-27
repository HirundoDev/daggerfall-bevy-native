//! `save_v2` — the versioned save **envelope** (spine contract
//! `docs/sintergic/game/programming/spine-contracts-CR-f716455c.md` §3).
//!
//! This module is the envelope and the [`SectionId`] registry — nothing else.
//! It does **not** implement M33's 14 list families: the payload of every
//! section belongs to the module that owns it (`m33…:163`), and `memory` only
//! composes the sections into [`SaveStateV2`] and frames them with
//! [`SAVE_MAGIC`] + [`SAVE_VERSION`].
//!
//! ## Owner rule (contract §3)
//!
//! > every module registers its `SectionId` and supplies `encode/decode`;
//! > `memory` only composes the envelope. A module can **not** write another
//! > module's section.
//!
//! - A module **declares** the section it owns with [`SectionRegistry::declare`].
//! - It **attaches** its codec with [`SectionRegistry::attach_codec`]
//!   ([`SectionCodec`]): `memory` sees opaque bytes, never the module's types.
//! - All writes go through [`SaveWriter`], which rejects an undeclared section
//!   ([`SaveError::NotDeclared`]), a write by a non-owner
//!   ([`SaveError::NotOwner`]) and a second write of the same section
//!   ([`SaveError::DuplicateSection`]).
//!
//! ## Wire format (deterministic, `sections` in canonical id order)
//!
//! ```text
//! "DFSV" | version u16 LE | section_count u16 LE
//!        | ( key_len u16 LE | key ASCII | payload_len u32 LE | payload ) * count
//! ```
//!
//! `encode` is a pure function of the state, so `load → save` is
//! byte-identical; `decode` is closed-key (a section the registry does not
//! know is [`SaveError::UnknownSection`], never accepted and never dropped) and
//! order-strict (a non-canonical envelope is [`SaveError::OutOfOrder`]).

use std::fmt;

use crate::session_store::fnv1a64;

/// Envelope magic (contract §3).
pub const SAVE_MAGIC: [u8; 4] = *b"DFSV";
/// Envelope version this build writes and accepts (contract §3).
pub const SAVE_VERSION: u16 = 2;
/// Longest section key the envelope accepts (bounded, closed keys).
pub const MAX_KEY_LEN: usize = 32;

/// The 12 canonical sections of M33 §3.3 as `(key, owner)` — the **public
/// spec table**, not a module internal.
///
/// `memory` needs it to answer one question without knowing any module's
/// types: *is the whole save available?* ([`SectionRegistry::coverage`]).
pub const M33_SECTIONS: [(&str, &str); 12] = [
    ("world", "M19/M20/M18"),
    ("clock", "heart"),
    ("character", "M20"),
    ("inventory", "M21/M30"),
    ("effects", "M23/M24/M32"),
    ("guilds", "M27"),
    ("crime", "M28"),
    ("quests", "M29"),
    ("economy", "M30"),
    ("calendar", "M31/M18"),
    ("map", "M18/M34"),
    ("rng", "all"),
];

/// Identifier of a save section (contract §3).
///
/// Canonically ordered by key: [`SaveStateV2::sections`] is kept in `SectionId`
/// order, which is what makes two encodings of the same state byte-identical.
/// The key is `&'static str` because sections are a **closed** set: every id is
/// declared by the module that owns it ([`SectionRegistry`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SectionId {
    key: &'static str,
}

impl SectionId {
    /// Declares an id from its stable key. Ownership and the closed-key rule
    /// are enforced by [`SectionRegistry`], not by the key itself.
    pub const fn new(key: &'static str) -> Self {
        Self { key }
    }

    /// Stable key written in the envelope.
    pub const fn key(self) -> &'static str {
        self.key
    }
}

impl fmt::Display for SectionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key)
    }
}

/// One section of the envelope: an opaque payload owned by one module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveSection {
    /// Which section this is.
    pub id: SectionId,
    /// The bytes the owning module produced (opaque to `memory`).
    pub bytes: Vec<u8>,
}

/// The versioned save envelope (contract §3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveStateV2 {
    /// Envelope version (always [`SAVE_VERSION`] for a composed state).
    pub version: u16,
    /// Sections in canonical [`SectionId`] order.
    pub sections: Vec<SaveSection>,
}

impl Default for SaveStateV2 {
    fn default() -> Self {
        Self::new()
    }
}

impl SaveStateV2 {
    /// Empty envelope at the current version.
    pub fn new() -> Self {
        Self {
            version: SAVE_VERSION,
            sections: Vec::new(),
        }
    }

    /// The section `id`, if the envelope carries it.
    pub fn section(&self, id: SectionId) -> Option<&SaveSection> {
        self.sections
            .binary_search_by(|section| section.id.cmp(&id))
            .ok()
            .map(|index| &self.sections[index])
    }

    /// Does the envelope carry `id`?
    pub fn contains(&self, id: SectionId) -> bool {
        self.sections
            .binary_search_by(|section| section.id.cmp(&id))
            .is_ok()
    }

    /// Inserts `bytes` for `id`, keeping canonical order.
    ///
    /// A second write of the same id is refused: one section, one writer.
    pub fn push(&mut self, id: SectionId, bytes: Vec<u8>) -> Result<(), SaveError> {
        match self.sections.binary_search_by(|section| section.id.cmp(&id)) {
            Ok(_) => Err(SaveError::DuplicateSection { key: id.key() }),
            Err(index) => {
                self.sections.insert(index, SaveSection { id, bytes });
                Ok(())
            }
        }
    }

    /// Canonical bytes of the envelope (magic + version + sections in id order).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(
            SAVE_MAGIC.len()
                + 4
                + self
                    .sections
                    .iter()
                    .map(|s| 6 + s.id.key().len() + s.bytes.len())
                    .sum::<usize>(),
        );
        out.extend_from_slice(&SAVE_MAGIC);
        out.extend_from_slice(&self.version.to_le_bytes());
        out.extend_from_slice(&(self.sections.len() as u16).to_le_bytes());
        for section in &self.sections {
            let key = section.id.key().as_bytes();
            out.extend_from_slice(&(key.len() as u16).to_le_bytes());
            out.extend_from_slice(key);
            out.extend_from_slice(&(section.bytes.len() as u32).to_le_bytes());
            out.extend_from_slice(&section.bytes);
        }
        out
    }

    /// Parses an envelope. `registry` is the closed set of known sections: an
    /// unknown key is an error (never silently ignored).
    pub fn decode(bytes: &[u8], registry: &SectionRegistry) -> Result<Self, SaveError> {
        let mut cursor = Cursor::new(bytes);
        let magic = cursor.take_array::<4>()?;
        if magic != SAVE_MAGIC {
            return Err(SaveError::BadMagic { found: magic });
        }
        let version = cursor.take_u16()?;
        if version != SAVE_VERSION {
            return Err(SaveError::UnsupportedVersion { version });
        }
        let count = usize::from(cursor.take_u16()?);
        let mut state = Self {
            version,
            sections: Vec::with_capacity(count),
        };
        let mut previous: Option<SectionId> = None;
        for _ in 0..count {
            let key_len = usize::from(cursor.take_u16()?);
            if key_len == 0 || key_len > MAX_KEY_LEN {
                return Err(SaveError::Malformed {
                    why: "section key length out of bounds",
                });
            }
            let key_bytes = cursor.take(key_len)?;
            let key = std::str::from_utf8(key_bytes).map_err(|_| SaveError::Malformed {
                why: "section key is not UTF-8",
            })?;
            let id = registry
                .resolve(key)
                .ok_or_else(|| SaveError::UnknownSection {
                    key: key.to_string(),
                })?;
            if let Some(previous) = previous {
                if id == previous {
                    return Err(SaveError::DuplicateSection { key: id.key() });
                }
                if id < previous {
                    return Err(SaveError::OutOfOrder {
                        key: id.key(),
                        after: previous.key(),
                    });
                }
            }
            previous = Some(id);
            let payload_len = cursor.take_u32()? as usize;
            let payload = cursor.take(payload_len)?.to_vec();
            state.sections.push(SaveSection {
                id,
                bytes: payload,
            });
        }
        let extra = cursor.remaining();
        if extra != 0 {
            return Err(SaveError::TrailingBytes { extra });
        }
        Ok(state)
    }

    /// FNV-1a 64 of [`Self::encode`] — the byte-identity evidence of a
    /// `save → load → save` roundtrip.
    pub fn digest(&self) -> u64 {
        fnv1a64(&self.encode())
    }

    /// Total payload bytes carried (header excluded). An envelope whose
    /// sections are all empty carries no state and must not satisfy a
    /// "section present" check (`§7` anti-false-positive rule).
    pub fn payload_bytes(&self) -> usize {
        self.sections.iter().map(|s| s.bytes.len()).sum()
    }
}

/// Little-endian reader with bounds checks (0 panics on hostile input).
struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], SaveError> {
        let end = self.pos.saturating_add(len);
        if end > self.bytes.len() {
            return Err(SaveError::Truncated {
                need: len,
                have: self.bytes.len() - self.pos,
            });
        }
        let slice = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn take_array<const N: usize>(&mut self) -> Result<[u8; N], SaveError> {
        let slice = self.take(N)?;
        let mut out = [0u8; N];
        out.copy_from_slice(slice);
        Ok(out)
    }

    fn take_u16(&mut self) -> Result<u16, SaveError> {
        Ok(u16::from_le_bytes(self.take_array::<2>()?))
    }

    fn take_u32(&mut self) -> Result<u32, SaveError> {
        Ok(u32::from_le_bytes(self.take_array::<4>()?))
    }

    fn remaining(&self) -> usize {
        self.bytes.len() - self.pos
    }
}

/// A module's `encode/decode` for the section it owns (contract §3).
///
/// `memory` drives it and never inspects the bytes' meaning: the type behind
/// the trait stays private to the module (0 knowledge of module internals in
/// the persistence organ).
pub trait SectionCodec {
    /// Section this codec produces.
    fn id(&self) -> SectionId;
    /// Canonical bytes of the module's current state.
    fn encode(&self) -> Vec<u8>;
    /// Applies decoded bytes to the module's state.
    fn decode(&mut self, bytes: &[u8]) -> Result<(), SectionError>;
}

/// Why a codec refused to decode (module-owned reason, opaque to `memory`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectionError {
    /// Module-provided explanation.
    pub reason: &'static str,
}

impl SectionError {
    /// Wraps a module reason.
    pub const fn new(reason: &'static str) -> Self {
        Self { reason }
    }
}

impl fmt::Display for SectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.reason)
    }
}

impl std::error::Error for SectionError {}

/// Which module owns which section, and which of them ship a codec.
///
/// The registry is the only thing that knows section **names**; it knows no
/// payload types. Keyed by [`SectionId`] in canonical order.
#[derive(Default)]
pub struct SectionRegistry {
    entries: Vec<RegistryEntry>,
}

struct RegistryEntry {
    id: SectionId,
    owner: &'static str,
    codec: Option<Box<dyn SectionCodec>>,
}

impl SectionRegistry {
    /// Empty registry.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// A module declares the section it owns. Declaring the same key twice is
    /// refused (two owners for one section is exactly the state §9 falsifies).
    pub fn declare(&mut self, id: SectionId, owner: &'static str) -> Result<(), RegistryError> {
        if id.key().is_empty() || id.key().len() > MAX_KEY_LEN {
            return Err(RegistryError::InvalidKey { key: id.key() });
        }
        match self
            .entries
            .binary_search_by(|entry| entry.id.cmp(&id))
        {
            Ok(_) => Err(RegistryError::AlreadyDeclared { key: id.key() }),
            Err(index) => {
                self.entries.insert(
                    index,
                    RegistryEntry {
                        id,
                        owner,
                        codec: None,
                    },
                );
                Ok(())
            }
        }
    }

    /// Declares every `(key, owner)` pair of a table (canonical order is the
    /// id's, not the table's).
    pub fn declare_all(
        &mut self,
        table: &[(&'static str, &'static str)],
    ) -> Result<(), RegistryError> {
        for (key, owner) in table.iter().copied() {
            self.declare(SectionId::new(key), owner)?;
        }
        Ok(())
    }

    fn entry(&self, id: SectionId) -> Option<&RegistryEntry> {
        self.entries
            .binary_search_by(|entry| entry.id.cmp(&id))
            .ok()
            .map(|index| &self.entries[index])
    }

    /// Owner declared for `id`.
    pub fn owner(&self, id: SectionId) -> Option<&'static str> {
        self.entry(id).map(|entry| entry.owner)
    }

    /// Interning lookup used by [`SaveStateV2::decode`]: maps a key read from
    /// the envelope to the declared id (`None` ⇒ unknown section).
    pub fn resolve(&self, key: &str) -> Option<SectionId> {
        self.entries
            .iter()
            .find(|entry| entry.id.key() == key)
            .map(|entry| entry.id)
    }

    /// The owning module attached its codec (encode/decode).
    pub fn attach_codec(
        &mut self,
        owner: &'static str,
        codec: Box<dyn SectionCodec>,
    ) -> Result<(), RegistryError> {
        let id = codec.id();
        let index = self
            .entries
            .binary_search_by(|entry| entry.id.cmp(&id))
            .map_err(|_| RegistryError::NotDeclared { key: id.key() })?;
        let entry = &mut self.entries[index];
        if entry.owner != owner {
            return Err(RegistryError::NotOwner {
                key: id.key(),
                declared: entry.owner,
                given: owner,
            });
        }
        if entry.codec.is_some() {
            return Err(RegistryError::CodecAlreadyAttached { key: id.key() });
        }
        entry.codec = Some(codec);
        Ok(())
    }

    /// Does `id` have a real codec (a module that can encode **and** decode it)?
    pub fn has_codec(&self, id: SectionId) -> bool {
        self.entry(id).map(|e| e.codec.is_some()).unwrap_or(false)
    }

    /// Declared sections in canonical order.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// No section declared.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `(id, owner, has_codec)` in canonical order (evidence lines).
    pub fn iter(&self) -> impl Iterator<Item = (SectionId, &'static str, bool)> + '_ {
        self.entries
            .iter()
            .map(|entry| (entry.id, entry.owner, entry.codec.is_some()))
    }

    /// Composes the envelope from **every attached codec** — the "`memory` sólo
    /// compone" path of contract §3. A state with no attached codec is empty
    /// (and therefore cannot satisfy any "section present" check).
    pub fn compose(&self) -> SaveStateV2 {
        let mut state = SaveStateV2::new();
        for entry in &self.entries {
            if let Some(codec) = &entry.codec {
                state.sections.push(SaveSection {
                    id: entry.id,
                    bytes: codec.encode(),
                });
            }
        }
        state
    }

    /// Canonical coverage over `table`: which sections have a real codec.
    ///
    /// This is the measurement that decides `EXIT=2` in `SPINE-001` §7: the
    /// spine cannot claim a byte-identical save of the whole game state while
    /// any canonical section has no module codec.
    pub fn coverage(&self, table: &[(&'static str, &'static str)]) -> Coverage {
        let mut coverage = Coverage::default();
        for (key, _owner) in table.iter().copied() {
            match self.resolve(key) {
                Some(id) if self.has_codec(id) => coverage.present.push(key),
                _ => coverage.missing.push(key),
            }
        }
        coverage
    }
}

impl fmt::Debug for SectionRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SectionRegistry")
            .field("sections", &self.len())
            .field(
                "with_codec",
                &self.iter().filter(|(_, _, has)| *has).count(),
            )
            .finish()
    }
}

/// Which sections of a table ship a codec and which do not (table order).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Coverage {
    /// Keys with a declared section **and** an attached codec.
    pub present: Vec<&'static str>,
    /// Keys without one.
    pub missing: Vec<&'static str>,
}

impl Coverage {
    /// Every key of the table is available.
    pub fn complete(&self) -> bool {
        self.missing.is_empty()
    }

    /// How many are available.
    pub fn count(&self) -> usize {
        self.present.len()
    }

    /// Size of the table.
    pub fn total(&self) -> usize {
        self.present.len() + self.missing.len()
    }

    /// `count/total` for evidence lines.
    pub fn ratio(&self) -> String {
        format!("{}/{}", self.count(), self.total())
    }

    /// Comma-separated missing keys (empty ⇒ `-`).
    pub fn missing_list(&self) -> String {
        if self.missing.is_empty() {
            "-".to_string()
        } else {
            self.missing.join(", ")
        }
    }
}

/// Writer that enforces the owner rule while composing an envelope.
///
/// This is the path a module uses to contribute its section; `SaveStateV2`
/// fields stay public because contract §3 fixes them, so the rule is enforced
/// here and demonstrated by `SPINE-001`'s falsifier battery.
pub struct SaveWriter<'a> {
    registry: &'a SectionRegistry,
    state: SaveStateV2,
}

impl<'a> SaveWriter<'a> {
    /// Writer bound to the registry that knows who owns what.
    pub fn new(registry: &'a SectionRegistry) -> Self {
        Self {
            registry,
            state: SaveStateV2::new(),
        }
    }

    /// Writes `bytes` into `id` as `owner`.
    pub fn write(
        &mut self,
        id: SectionId,
        owner: &'static str,
        bytes: Vec<u8>,
    ) -> Result<(), SaveError> {
        let declared = self
            .registry
            .owner(id)
            .ok_or(SaveError::NotDeclared { key: id.key() })?;
        if declared != owner {
            return Err(SaveError::NotOwner {
                key: id.key(),
                declared,
                given: owner,
            });
        }
        self.state.push(id, bytes)
    }

    /// Encodes `codec` (the owner's own `encode`) into its section.
    pub fn write_codec(
        &mut self,
        owner: &'static str,
        codec: &dyn SectionCodec,
    ) -> Result<(), SaveError> {
        self.write(codec.id(), owner, codec.encode())
    }

    /// The composed envelope.
    pub fn finish(self) -> SaveStateV2 {
        self.state
    }
}

/// Why a section could not be written or decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveError {
    /// The envelope does not start with [`SAVE_MAGIC`].
    BadMagic {
        /// Bytes found instead.
        found: [u8; 4],
    },
    /// The envelope is not version [`SAVE_VERSION`].
    UnsupportedVersion {
        /// Version found.
        version: u16,
    },
    /// The input ended before the envelope did.
    Truncated {
        /// Bytes the envelope still needed.
        need: usize,
        /// Bytes available.
        have: usize,
    },
    /// Bytes after the last section (a corrupt or concatenated save).
    TrailingBytes {
        /// Extra bytes found.
        extra: usize,
    },
    /// Structurally impossible header/section.
    Malformed {
        /// What is wrong.
        why: &'static str,
    },
    /// A section key the registry does not know (closed keys, M33 §3.1(e)).
    UnknownSection {
        /// Key found in the envelope.
        key: String,
    },
    /// The same section twice.
    DuplicateSection {
        /// Section key.
        key: &'static str,
    },
    /// Sections out of canonical order.
    OutOfOrder {
        /// Section that came too late.
        key: &'static str,
        /// Section already written before it.
        after: &'static str,
    },
    /// The section was never declared by its owner.
    NotDeclared {
        /// Section key.
        key: &'static str,
    },
    /// A module tried to write a section another module owns.
    NotOwner {
        /// Section key.
        key: &'static str,
        /// Owner the registry knows.
        declared: &'static str,
        /// Owner that tried to write.
        given: &'static str,
    },
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic { found } => write!(f, "bad magic {found:?} (want {SAVE_MAGIC:?})"),
            Self::UnsupportedVersion { version } => {
                write!(f, "unsupported save version {version} (want {SAVE_VERSION})")
            }
            Self::Truncated { need, have } => {
                write!(f, "truncated save: need {need} bytes, have {have}")
            }
            Self::TrailingBytes { extra } => write!(f, "{extra} trailing bytes after the envelope"),
            Self::Malformed { why } => write!(f, "malformed save: {why}"),
            Self::UnknownSection { key } => write!(f, "unknown section {key:?}"),
            Self::DuplicateSection { key } => write!(f, "section {key:?} written twice"),
            Self::OutOfOrder { key, after } => {
                write!(f, "section {key:?} out of canonical order (after {after:?})")
            }
            Self::NotDeclared { key } => write!(f, "section {key:?} was never declared"),
            Self::NotOwner {
                key,
                declared,
                given,
            } => write!(
                f,
                "section {key:?} is owned by {declared:?}, not by {given:?}"
            ),
        }
    }
}

impl std::error::Error for SaveError {}

/// Why a registry operation was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    /// The key is empty or longer than [`MAX_KEY_LEN`].
    InvalidKey {
        /// Offending key.
        key: &'static str,
    },
    /// Two owners for one section.
    AlreadyDeclared {
        /// Section key.
        key: &'static str,
    },
    /// The section was not declared before its codec was attached.
    NotDeclared {
        /// Section key.
        key: &'static str,
    },
    /// The codec was attached under a foreign owner.
    NotOwner {
        /// Section key.
        key: &'static str,
        /// Owner the registry knows.
        declared: &'static str,
        /// Owner that tried to attach.
        given: &'static str,
    },
    /// The section already has a codec.
    CodecAlreadyAttached {
        /// Section key.
        key: &'static str,
    },
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidKey { key } => write!(f, "invalid section key {key:?}"),
            Self::AlreadyDeclared { key } => write!(f, "section {key:?} already declared"),
            Self::NotDeclared { key } => write!(f, "section {key:?} was never declared"),
            Self::NotOwner {
                key,
                declared,
                given,
            } => write!(
                f,
                "section {key:?} is owned by {declared:?}, not by {given:?}"
            ),
            Self::CodecAlreadyAttached { key } => {
                write!(f, "section {key:?} already has a codec")
            }
        }
    }
}

impl std::error::Error for RegistryError {}

#[cfg(test)]
mod tests {
    use super::*;

    const SESSION: SectionId = SectionId::new("session");
    const ECONOMY: SectionId = SectionId::new("economy");

    fn registry() -> SectionRegistry {
        let mut registry = SectionRegistry::new();
        registry.declare(SESSION, "heart").expect("declare session");
        registry.declare(ECONOMY, "M30").expect("declare economy");
        registry
    }

    fn envelope() -> SaveStateV2 {
        let mut state = SaveStateV2::new();
        state.push(ECONOMY, vec![1, 2, 3, 4]).expect("economy");
        state.push(SESSION, vec![9, 9]).expect("session");
        state
    }

    #[test]
    fn sections_are_kept_in_canonical_order() {
        let state = envelope();
        let keys: Vec<&'static str> = state.sections.iter().map(|s| s.id.key()).collect();
        assert_eq!(keys, vec!["economy", "session"], "id order, not write order");
    }

    #[test]
    fn encode_decode_roundtrip_is_byte_identical() {
        let registry = registry();
        let state = envelope();
        let bytes = state.encode();
        let loaded = SaveStateV2::decode(&bytes, &registry).expect("decode");
        assert_eq!(loaded, state);
        assert_eq!(loaded.encode(), bytes);
        assert_eq!(loaded.digest(), state.digest());
    }

    #[test]
    fn magic_and_version_are_the_contract_values() {
        assert_eq!(&SAVE_MAGIC, b"DFSV");
        assert_eq!(SAVE_VERSION, 2);
        let bytes = SaveStateV2::new().encode();
        assert_eq!(&bytes[..4], b"DFSV");
        assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), 2);
    }

    #[test]
    fn bad_magic_and_version_are_rejected() {
        let registry = registry();
        let mut bytes = envelope().encode();
        bytes[0] = b'X';
        assert!(matches!(
            SaveStateV2::decode(&bytes, &registry),
            Err(SaveError::BadMagic { .. })
        ));

        let mut bytes = envelope().encode();
        bytes[4] = 1;
        assert_eq!(
            SaveStateV2::decode(&bytes, &registry),
            Err(SaveError::UnsupportedVersion { version: 1 })
        );
    }

    #[test]
    fn unknown_section_is_not_ignored() {
        let registry = registry();
        let mut state = SaveStateV2::new();
        state.push(SectionId::new("ghost"), vec![7]).expect("push");
        let bytes = state.encode();
        assert_eq!(
            SaveStateV2::decode(&bytes, &registry),
            Err(SaveError::UnknownSection {
                key: "ghost".to_string()
            })
        );
    }

    #[test]
    fn out_of_order_envelope_is_rejected() {
        let registry = registry();
        let mut state = SaveStateV2::new();
        state.push(SESSION, vec![1]).expect("session");
        state.push(ECONOMY, vec![2]).expect("economy");
        // Reorder by hand: a non-canonical envelope is corrupt, not "valid".
        state.sections.swap(0, 1);
        assert_eq!(
            SaveStateV2::decode(&state.encode(), &registry),
            Err(SaveError::OutOfOrder {
                key: "economy",
                after: "session"
            })
        );
    }

    #[test]
    fn writer_enforces_the_owner_rule() {
        let registry = registry();
        let mut writer = SaveWriter::new(&registry);
        writer
            .write(ECONOMY, "M30", vec![1])
            .expect("owner writes");
        assert_eq!(
            writer.write(ECONOMY, "M30", vec![2]),
            Err(SaveError::DuplicateSection { key: "economy" })
        );

        let mut writer = SaveWriter::new(&registry);
        assert_eq!(
            writer.write(ECONOMY, "M29", vec![2]),
            Err(SaveError::NotOwner {
                key: "economy",
                declared: "M30",
                given: "M29"
            })
        );
        assert_eq!(
            writer.write(SectionId::new("ghost"), "M30", vec![2]),
            Err(SaveError::NotDeclared { key: "ghost" })
        );
        assert_eq!(writer.finish().sections.len(), 0, "0 refused writes land");
    }

    #[test]
    fn two_owners_cannot_declare_one_section() {
        let mut registry = registry();
        assert_eq!(
            registry.declare(ECONOMY, "M29"),
            Err(RegistryError::AlreadyDeclared { key: "economy" })
        );
    }

    #[test]
    fn coverage_counts_only_attached_codecs() {
        let registry = registry();
        let coverage = registry.coverage(&M33_SECTIONS);
        assert_eq!(coverage.total(), 12);
        assert_eq!(coverage.count(), 0);
        assert!(!coverage.complete());
        assert_eq!(coverage.ratio(), "0/12");
    }

    #[test]
    fn empty_sections_carry_no_payload() {
        let mut state = SaveStateV2::new();
        state.push(ECONOMY, Vec::new()).expect("push");
        assert_eq!(state.payload_bytes(), 0);
    }

    #[test]
    fn truncated_envelope_is_an_error_not_a_panic() {
        let registry = registry();
        let bytes = envelope().encode();
        for cut in 0..bytes.len() {
            assert!(SaveStateV2::decode(&bytes[..cut], &registry).is_err());
        }
    }
}

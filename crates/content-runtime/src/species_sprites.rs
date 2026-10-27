//! `species_sprites` — the M35 species sprite catalogue (`PROBE-SPRITE-001`,
//! spec §6.2, GAP-TAX-08).
//!
//! **Normative source:** `docs/sintergic/game/programming/m35-vegetation-night-sky-spec-CR-f716455c.md`
//! §6.2 (blocks A/B/C), §1 row 6, §9 R4 and §10 row `REQ-L99-069 (6)` (probe
//! `SPRITE A-C`). **Custody:** 0 retail bytes, 0 file I/O, 0 parser edge — this
//! module owns the *budget and disposition*, never the bytes. The archives
//! `TEXTURE.255..=297` are read by the runtime probe, which measures the disk
//! and the corpus and builds the [`NegativeEvidence`] from outside.
//!
//! ## Disposition A — the declared table is not available (gap H4)
//!
//! §6.2 block A accepts a species → archive table **or** a `NEGATIVE` with an
//! absence census. The pinned corpus cannot supply the table: `EnemyBasics.cs`
//! is absent from `reference/dfu-spec/` (a 120-file `.cs` census), so **no**
//! pinned line states which of `TEXTURE.255..=297` holds which mobile type.
//! [`declared_table_available`] therefore returns `false` and the disposition
//! is [`SpeciesCatalogDisposition::Negative`], signed rather than converted
//! into a silent `PASS` (§6.2 last paragraph).
//!
//! Two facts keep the module honest instead of empty:
//!
//! - the **names** of the 43 mobile types do exist — M25's
//!   `daggerfall_parser_rs::monster::MOBILE_TYPE_NAMES`
//!   (`crates/daggerfall_parser_rs/src/monster.rs:40`, `MOBILE_TYPE_COUNT`
//!   = 43) — and they are **not** duplicated here: a row only ever carries a
//!   `mobile_type` index plus the archive the probe measured;
//! - the archive **range** is pinned: `TEXTURE.255..=297` = 43 files
//!   ([`SPECIES_ARCHIVE_RANGE`], [`SPECIES_ARCHIVE_COUNT`]).
//!
//! ## Disposition B — the budget is measurable today
//!
//! Block B is independent of block A: ≤ [`PER_SPECIES_BUDGET_BYTES`] per
//! species, an LRU of [`LRU_CAPACITY`] species ⇒ ≤ [`TOTAL_BUDGET_BYTES`]
//! total, and 0 loads of the 43 at once. [`SpeciesSpriteCache`] enforces and
//! reports exactly that, so the lane can sign 3/3 even without the table.

use std::ops::RangeInclusive;

/// Archive range of the species sprites, `TEXTURE.255..=297` (spec §6.2
/// block A; 43 files, [`SPECIES_ARCHIVE_COUNT`]).
pub const SPECIES_ARCHIVE_RANGE: RangeInclusive<u32> = 255..=297;

/// Number of archives in [`SPECIES_ARCHIVE_RANGE`] (`297 − 255 + 1`), the
/// count §6.2 pairs with the 43 mobile types.
pub const SPECIES_ARCHIVE_COUNT: usize = 43;

/// Per-species budget in bytes (`1.5 MiB`, `1.5 × 1 048 576`, spec §6.2
/// block B).
pub const PER_SPECIES_BUDGET_BYTES: u64 = 1_572_864;

/// LRU capacity in species: four resident archives at most (spec §6.2 block
/// B, "0 loads of the 43 at once").
pub const LRU_CAPACITY: usize = 4;

/// Total budget in bytes (`4 × 1.5 MiB = 6 MiB`, `6 × 1 048 576`, spec §6.2
/// block B).
pub const TOTAL_BUDGET_BYTES: u64 = 6_291_456;

/// Is `archive` inside [`SPECIES_ARCHIVE_RANGE`]?
pub fn is_species_archive(archive: u32) -> bool {
    SPECIES_ARCHIVE_RANGE.contains(&archive)
}

/// One row of a declared species → archive table (spec §6.2 block A).
///
/// No such row can be built from the pinned corpus today (gap H4); the type
/// exists so the disposition is expressed by the data model, not by prose, and
/// so a future pinned source only has to fill it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpeciesSpriteRow {
    /// Mobile type index (M25 `MOBILE_TYPE_NAMES` position).
    pub mobile_type: u16,
    /// Name of the mobile type, borrowed from the M25 table (never recopied).
    pub name: &'static str,
    /// Archive holding the sprite, inside [`SPECIES_ARCHIVE_RANGE`].
    pub archive: u32,
}

/// Evidence that the declared table has no pinned source (GAP-TAX-08).
///
/// The probe fills it after measuring: `corpus_census` is the count of `.cs`
/// files scanned, `archives_present`/`archives_absent` the on-disk census of
/// [`SPECIES_ARCHIVE_RANGE`]. Nothing here is guessed by this module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NegativeEvidence {
    /// The file whose absence removes the source, e.g.
    /// `Assets/Scripts/Game/EnemyBasics.cs`.
    pub missing_source: &'static str,
    /// Number of `.cs` files in the pinned corpus census.
    pub corpus_census: usize,
    /// Archives of [`SPECIES_ARCHIVE_RANGE`] present on disk.
    pub archives_present: usize,
    /// Archives of [`SPECIES_ARCHIVE_RANGE`] absent on disk.
    pub archives_absent: Vec<u32>,
}

/// Block A disposition of the catalogue: a declared table or a signed
/// `NEGATIVE` (spec §6.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpeciesCatalogDisposition {
    /// Species → archive rows with a pinned source.
    Declared(Vec<SpeciesSpriteRow>),
    /// `NEGATIVE` with an absence census (the live case, gap H4).
    Negative(NegativeEvidence),
}

impl SpeciesCatalogDisposition {
    /// `true` for [`Self::Negative`] — what the probe signs.
    pub fn is_negative(&self) -> bool {
        matches!(self, Self::Negative(_))
    }

    /// The declared rows, or `None` for a `NEGATIVE`.
    pub fn rows(&self) -> Option<&[SpeciesSpriteRow]> {
        match self {
            Self::Declared(rows) => Some(rows),
            Self::Negative(_) => None,
        }
    }
}

/// LRU of at most [`LRU_CAPACITY`] resident species with byte accounting
/// (spec §6.2 block B).
///
/// The cache never owns pixels: `insert` records the byte size the loader
/// measured, and the cache keeps the window, the byte totals and the eviction
/// count. Entries are ordered most-recently-used first, so re-inserting a
/// resident species refreshes it instead of duplicating it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpeciesSpriteCache {
    /// Resident `(mobile_type, bytes)`, most recently used first.
    entries: Vec<(u16, u64)>,
    /// Bytes of the resident entries.
    resident_bytes: u64,
    /// High-water mark of [`Self::resident_bytes`].
    peak_bytes: u64,
    /// Cumulative `insert` calls.
    total_inserts: u64,
    /// Cumulative evictions (an insert beyond capacity, or a replacement).
    evictions: u64,
}

impl SpeciesSpriteCache {
    /// Empty cache with the fixed capacity [`LRU_CAPACITY`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Capacity in species ([`LRU_CAPACITY`]).
    pub fn capacity(&self) -> usize {
        LRU_CAPACITY
    }

    /// Records the load of `mobile_type` with `bytes` pixels, evicting the
    /// least recently used species while over capacity.
    pub fn insert(&mut self, mobile_type: u16, bytes: u64) {
        self.total_inserts += 1;
        if let Some(position) = self
            .entries
            .iter()
            .position(|(resident, _)| *resident == mobile_type)
        {
            let (_, previous) = self.entries.remove(position);
            self.resident_bytes = self.resident_bytes.saturating_sub(previous);
            self.evictions += 1;
        }
        self.entries.insert(0, (mobile_type, bytes));
        self.resident_bytes = self.resident_bytes.saturating_add(bytes);
        while self.entries.len() > LRU_CAPACITY {
            if let Some((_, evicted)) = self.entries.pop() {
                self.resident_bytes = self.resident_bytes.saturating_sub(evicted);
                self.evictions += 1;
            }
        }
        self.peak_bytes = self.peak_bytes.max(self.resident_bytes);
    }

    /// Bytes resident right now.
    pub fn resident_bytes(&self) -> u64 {
        self.resident_bytes
    }

    /// High-water mark of [`Self::resident_bytes`].
    pub fn peak_bytes(&self) -> u64 {
        self.peak_bytes
    }

    /// Species resident right now — the `loads` of §6.2 block B, capped at
    /// [`LRU_CAPACITY`] no matter how many `insert` calls happened.
    pub fn loads(&self) -> usize {
        self.entries.len()
    }

    /// Cumulative `insert` calls (diagnostic: `loads + evictions` is not the
    /// invariant, replacements evict too).
    pub fn total_inserts(&self) -> u64 {
        self.total_inserts
    }

    /// Cumulative evictions.
    pub fn evictions(&self) -> u64 {
        self.evictions
    }

    /// `true` when every block-B check holds: at most [`LRU_CAPACITY`]
    /// resident, no species above [`PER_SPECIES_BUDGET_BYTES`] and the window
    /// under [`TOTAL_BUDGET_BYTES`].
    pub fn budget_ok(&self) -> bool {
        self.entries.len() <= LRU_CAPACITY
            && self.resident_bytes <= TOTAL_BUDGET_BYTES
            && self
                .entries
                .iter()
                .all(|(_, bytes)| *bytes <= PER_SPECIES_BUDGET_BYTES)
    }
}

/// `false` — the species → archive column has no pinned source (gap H4).
///
/// Census behind the `false` (spec §6.2 block A, `instance-taxonomy-biomes-CR-fbd6c53e.md:253`
/// GAP-TAX-08):
///
/// - `reference/dfu-spec/` (120 `.cs` files) has **no** `EnemyBasics.cs`, so
///   no pinned line maps a mobile type to `TEXTURE.255..=297`;
/// - the archives themselves are mostly present on disk, which is precisely
///   why a guessed mapping would look plausible and must not be invented;
/// - the names table `daggerfall_parser_rs::monster::MOBILE_TYPE_NAMES` (43)
///   *is* pinned and is **not** duplicated here.
///
/// The probe measures the corpus and disk censuses; they travel in
/// [`NegativeEvidence`], never in this constant.
pub fn declared_table_available() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    const KIB: u64 = 1024;

    #[test]
    fn six_insertions_keep_four_resident_and_stay_in_budget() {
        let mut cache = SpeciesSpriteCache::new();
        for species in 0..6_u16 {
            cache.insert(species, 100 * KIB);
        }
        assert_eq!(LRU_CAPACITY, 4);
        assert_eq!(cache.loads(), 4);
        assert_eq!(cache.evictions(), 2);
        assert_eq!(cache.total_inserts(), 6);
        assert_eq!(cache.resident_bytes(), 400 * KIB);
        assert_eq!(cache.peak_bytes(), 400 * KIB);
        assert!(cache.resident_bytes() <= TOTAL_BUDGET_BYTES);
        assert!(cache.budget_ok());
    }

    #[test]
    fn a_two_mib_species_breaks_the_budget() {
        let mut cache = SpeciesSpriteCache::new();
        assert!(cache.budget_ok());
        cache.insert(3, 2 * 1024 * 1024);
        assert!(!cache.budget_ok());
        assert!(cache.resident_bytes() < TOTAL_BUDGET_BYTES);
        assert_eq!(PER_SPECIES_BUDGET_BYTES, 1_572_864);
    }

    #[test]
    fn reloading_a_resident_species_does_not_double_count() {
        let mut cache = SpeciesSpriteCache::new();
        cache.insert(1, 512 * KIB);
        cache.insert(2, 512 * KIB);
        cache.insert(1, 512 * KIB);
        assert_eq!(cache.loads(), 2);
        assert_eq!(cache.resident_bytes(), 1024 * KIB);
        assert!(cache.budget_ok());
    }

    #[test]
    fn the_declared_table_is_not_available_and_budgets_are_exact() {
        assert!(!declared_table_available());
        assert_eq!(SPECIES_ARCHIVE_RANGE.start(), &255);
        assert_eq!(SPECIES_ARCHIVE_RANGE.end(), &297);
        assert_eq!(SPECIES_ARCHIVE_COUNT, 43);
        assert_eq!(SPECIES_ARCHIVE_RANGE.count(), SPECIES_ARCHIVE_COUNT);
        assert_eq!(TOTAL_BUDGET_BYTES, LRU_CAPACITY as u64 * PER_SPECIES_BUDGET_BYTES);
        assert_eq!(TOTAL_BUDGET_BYTES, 6 * 1_048_576);
        assert_eq!(PER_SPECIES_BUDGET_BYTES * 2 / 3, 1_048_576);
        assert!(is_species_archive(255));
        assert!(is_species_archive(297));
        assert!(!is_species_archive(254));
        assert!(!is_species_archive(298));
    }

    #[test]
    fn the_disposition_carries_the_negative_evidence() {
        let negative = SpeciesCatalogDisposition::Negative(NegativeEvidence {
            missing_source: "Assets/Scripts/Game/EnemyBasics.cs",
            corpus_census: 120,
            archives_present: 42,
            archives_absent: vec![294],
        });
        assert!(negative.is_negative());
        assert_eq!(negative.rows(), None);
        let rows = SpeciesCatalogDisposition::Declared(vec![SpeciesSpriteRow {
            mobile_type: 0,
            name: "Rat",
            archive: 255,
        }]);
        assert!(!rows.is_negative());
        assert_eq!(rows.rows().map(|declared| declared.len()), Some(1));
    }
}

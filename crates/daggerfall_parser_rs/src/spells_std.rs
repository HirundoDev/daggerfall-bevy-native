//! `SPELLS.STD` — classic spell record parser (M23, REQ-L99-056).
//!
//! Spec: `docs/sintergic/game/programming/m23-magic-spec-CR-5243af63.md` §1
//! (measured layout + census), reference reader `DFU-UP:Assets/Scripts/
//! Utility/DaggerfallSpellReader.cs:27-204@0038a1e6` (MIT, executable spec —
//! the pinned DFU corpus does not include the file in the harness sparse
//! checkout; it is read via `git show`, same regime as `.RDB` in M11).
//!
//! Layout (fixed stride `SPELLRECORDSIZE = 0x59` = **89 B**, `:27`; read
//! condition `position + 89 <= length`, `:61`):
//! ```text
//!  0–5   3 × (type i8, subType i8)            SetSpellTypes      :164-187
//!  6     element u8                            ReadSpellData      :136
//!  7     rangeType u8                          ReadSpellData      :137
//!  8–9   cost u16 LE                           ReadSpellData      :138
//! 10–13  gap (seek 4, not read)                ReadSpellData      :139
//! 14–22  3 × (durationBase, Mod, PerLevel)     SetSpellDurations  :207-224
//! 23–31  3 × (chanceBase, Mod, PerLevel)       SetSpellChances    :226-243
//! 32–46  3 × (magBaseLow, BaseHigh, LevelBase, LevelHigh, PerLevel)
//!                                               SetSpellMagnitudes :245-264
//! 47–71  spellName (C-string, cut at NUL)    ReadCStringSkip 25 :148
//! 72     icon u8                               ReadSpellData      :150
//! 73     index u8                              ReadSpellData      :151
//! 74–88  gap (seek 15, not read)               ReadSpellData      :152
//! ```
//!
//! **Stride — behavior, not intent (spec §1.2):** the C# check
//! `if (effects[i].type == 0xFF) continue;` (`:180-181`) **never fires**
//! (`ReadSByte()` returns `sbyte` and `-1 == 0xFF (255)` is `false` in C#).
//! `subType` is always consumed and the 6 B of types are constant even when
//! the slot is empty (`0xFF 0xFF`). The parser replicates that behavior:
//! 6+1+1+2+4+9+9+15+25+1+1+15 = **89** (golden A8).
//!
//! `SetTextIndices` (`:190-204`) sets `descriptionTextIndex`/
//! `spellMakerTextIndex` to 0 and is **never called** from `ReadSpellData`
//! (spec §1.2, NEGATIVE N-3): the parser does NOT derive +1200/+1500 indices.
//!
//! Endianness: `BinaryReader` = little-endian; `cost` u16 LE (spec §0).
//! 0 retail bytes in the repository: the file is read at runtime from the
//! user's ARENA2 (Mode A, INV-L99-6); the test fixtures are synthetic.

/// Fixed size of one record (`DaggerfallSpellReader.cs:27`).
pub const SPELL_RECORD_SIZE: usize = 89;
/// Effect slots per record (3, `SpellRecord.cs:69-72`).
pub const EFFECT_SLOT_COUNT: usize = 3;
/// Bytes of the `spellName` field (25 B C-string, `:148`).
pub const SPELL_NAME_BYTES: usize = 25;
/// Default file name (`:30`).
pub const DEFAULT_FILENAME: &str = "SPELLS.STD";

/// Field offsets inside the record (spec §1.2).
pub const EFFECT_TYPES_OFFSET: usize = 0;
/// `element` (spec §1.2, `:136`).
pub const ELEMENT_OFFSET: usize = 6;
/// `rangeType` (spec §1.2, `:137`).
pub const RANGE_TYPE_OFFSET: usize = 7;
/// `cost` u16 LE (spec §1.2, `:138`).
pub const COST_OFFSET: usize = 8;
/// Start of the durations block (spec §1.2, `:207`).
pub const DURATION_OFFSET: usize = 14;
/// Start of the chances block (spec §1.2, `:226`).
pub const CHANCE_OFFSET: usize = 23;
/// Start of the magnitudes block (spec §1.2, `:245`).
pub const MAGNITUDE_OFFSET: usize = 32;
/// Absolute offset of `spellName` inside the record (spec §1.2, `:148`).
pub const SPELL_NAME_OFFSET: usize = 47;
/// Offset of `icon` (spec §1.2, `:150`).
pub const ICON_OFFSET: usize = 72;
/// Offset of `index` (classic spell ID, spec §1.2, `:151`).
pub const SPELL_INDEX_OFFSET: usize = 73;

/// Raw effect slot (3 slots × 2 B of type + 3+3+5 B of settings).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EffectSlot {
    /// Classic type (`-1` = empty slot; the 0xFF byte is read as `sbyte`).
    pub effect_type: i8,
    /// Classic subtype (`-1` = no subtype).
    pub sub_type: i8,
    /// `(durationBase, durationMod, durationPerLevel)`.
    pub duration: [u8; 3],
    /// `(chanceBase, chanceMod, chancePerLevel)`.
    pub chance: [u8; 3],
    /// `(magnitudeBaseLow, BaseHigh, LevelBase, LevelHigh, PerLevel)`.
    pub magnitude: [u8; 5],
}

impl EffectSlot {
    /// Empty slot (`0xFF 0xFF` on disk ⇒ `-1/-1`, spec §1.2).
    pub const fn unused() -> Self {
        Self {
            effect_type: -1,
            sub_type: -1,
            duration: [1, 1, 1],
            chance: [1, 1, 1],
            magnitude: [1, 1, 1, 1, 1],
        }
    }

    /// The slot is in use (`type != -1`; DFU C# semantics, e.g.
    /// `CalculateCastingCost` `FormulaHelper.cs:2540` and
    /// `ClassicSpellRecordDataToEffectBundleSettings` `EntityEffectBroker.cs:900`).
    pub fn is_used(&self) -> bool {
        self.effect_type != -1
    }
}

/// Parsed spell record (89 B, spec §1.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpellRecord {
    /// 3 effect slots (always consumed, see stride above).
    pub effects: [EffectSlot; EFFECT_SLOT_COUNT],
    /// Classic element 0..4 (`EntityEffectBroker.cs:1010-1027`).
    pub element: u8,
    /// Classic target type 0..4 (`EntityEffectBroker.cs:986-999`).
    pub range_type: u8,
    /// Cost written only once in the file (`DaggerfallSpellReader.cs:138`);
    /// no reader at cast time (spec §6 N-4, bounded `[INFERENCE]`).
    pub cost: u16,
    /// ASCII name (C-string; cuts at the first NUL).
    pub spell_name: String,
    /// Spell icon.
    pub icon: u8,
    /// Classic spell ID (may repeat across records).
    pub index: u8,
    /// Absolute offset of the record inside the file (for A6: `last_start`).
    pub offset: usize,
}

impl SpellRecord {
    /// Internal name (starts with `!`, not for sale — `DaggerfallSpellBookWindow.cs:299-307`).
    pub fn is_internal(&self) -> bool {
        self.spell_name.starts_with('!')
    }

    /// Absolute offset of `spellName` inside the file (golden A6).
    pub fn name_offset(&self) -> usize {
        self.offset + SPELL_NAME_OFFSET
    }

    /// Effect slots in use (1..3).
    pub fn used_slots(&self) -> usize {
        self.effects.iter().filter(|slot| slot.is_used()).count()
    }
}

/// Parses a complete record at `bytes[offset..offset + 89]`.
///
/// `None` if the chunk does not fit (`ReadSpellData` returns `false`,
/// `DaggerfallSpellReader.cs:114-116`).
fn parse_record(bytes: &[u8], offset: usize) -> Option<SpellRecord> {
    let end = offset.checked_add(SPELL_RECORD_SIZE)?;
    if bytes.len() < end {
        return None;
    }
    let chunk = &bytes[offset..end];

    let mut effects = [EffectSlot::unused(); EFFECT_SLOT_COUNT];
    for (slot, effect) in effects.iter_mut().enumerate() {
        let base = EFFECT_TYPES_OFFSET + 2 * slot;
        effect.effect_type = chunk[base] as i8;
        effect.sub_type = chunk[base + 1] as i8;

        let base = DURATION_OFFSET + 3 * slot;
        effect.duration = [chunk[base], chunk[base + 1], chunk[base + 2]];

        let base = CHANCE_OFFSET + 3 * slot;
        effect.chance = [chunk[base], chunk[base + 1], chunk[base + 2]];

        let base = MAGNITUDE_OFFSET + 5 * slot;
        effect.magnitude = [
            chunk[base],
            chunk[base + 1],
            chunk[base + 2],
            chunk[base + 3],
            chunk[base + 4],
        ];
    }

    let name_bytes = &chunk[SPELL_NAME_OFFSET..SPELL_NAME_OFFSET + SPELL_NAME_BYTES];
    let name_end = name_bytes.iter().position(|&b| b == 0).unwrap_or(SPELL_NAME_BYTES);
    let spell_name = String::from_utf8_lossy(&name_bytes[..name_end]).into_owned();

    Some(SpellRecord {
        effects,
        element: chunk[ELEMENT_OFFSET],
        range_type: chunk[RANGE_TYPE_OFFSET],
        cost: u16::from_le_bytes([chunk[COST_OFFSET], chunk[COST_OFFSET + 1]]),
        spell_name,
        icon: chunk[ICON_OFFSET],
        index: chunk[SPELL_INDEX_OFFSET],
        offset,
    })
}

/// Parses all of `SPELLS.STD` with the reader's loop control condition:
/// `position + 89 <= length` (`DaggerfallSpellReader.cs:61`). Leftover bytes
/// (< 89) are ignored, exactly as in C#.
pub fn parse_spells_std(bytes: &[u8]) -> Vec<SpellRecord> {
    let mut records = Vec::with_capacity(bytes.len() / SPELL_RECORD_SIZE);
    let mut offset = 0;
    while offset + SPELL_RECORD_SIZE <= bytes.len() {
        // The loop condition already guaranteed the size: `expect` is unreachable.
        records.push(parse_record(bytes, offset).expect("stride already verified"));
        offset += SPELL_RECORD_SIZE;
    }
    records
}

/// `index` duplicate found (spec §1.3: `index 58`, cost 550 in both).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateIndex {
    /// Duplicated classic ID.
    pub index: u8,
    /// Cost of the first record with that index.
    pub first_cost: u16,
    /// Cost of the later record dropped by the broker
    /// (`EntityEffectBroker.cs:747-752`: it keeps the first one).
    pub second_cost: u16,
}

/// Measured census of a parsed `SPELLS.STD` (spec §1.3, block A of the probe).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SpellsCensus {
    /// Parsed records.
    pub records: usize,
    /// Records with 1 / 2 / 3 effect slots in use.
    pub used_one: usize,
    /// (see [`Self::used_one`]).
    pub used_two: usize,
    /// (see [`Self::used_one`]).
    pub used_three: usize,
    /// Count per classic element 0..4 (Fire, Cold, Poison, Shock, Magic).
    pub element_counts: [usize; 5],
    /// Elements outside 0..4 (anti-false-positive (iii): a mishandled
    /// `type == 0xFF` shifts the stride and yields `element > 4`).
    pub element_out_of_range: usize,
    /// Count per classic target type 0..4.
    pub range_counts: [usize; 5],
    /// Target types outside 0..4.
    pub range_out_of_range: usize,
    /// Names with the `!` prefix (internal, over ALL records).
    pub internal: usize,
    /// Offerable spells: dedupe by `index` keeping the first
    /// (broker `:747-752`) and without the `!` prefix (`DaggerfallSpellBookWindow.cs:299-307`).
    pub offered: usize,
    /// Records with `cost == 0`.
    pub zero_cost: usize,
    /// Maximum cost in the file.
    pub max_cost: u16,
    /// Distinct classic indexes (no dedupe: 89 records ⇒ 88 if there is 1 duplicate).
    pub unique_indexes: usize,
    /// Duplicates in order of appearance.
    pub duplicates: Vec<DuplicateIndex>,
    /// Start offset of the last record (golden A6: `7832`).
    pub last_start: usize,
    /// Absolute offset of the last record's name (golden A6: `7879`).
    pub last_name_offset: usize,
}

/// Aggregates the §1.3 census over already parsed records.
pub fn census(records: &[SpellRecord]) -> SpellsCensus {
    let mut result = SpellsCensus {
        records: records.len(),
        ..SpellsCensus::default()
    };
    let mut seen_indexes = std::collections::HashSet::new();
    let mut deduped: Vec<&SpellRecord> = Vec::with_capacity(records.len());

    for record in records {
        match record.used_slots() {
            1 => result.used_one += 1,
            2 => result.used_two += 1,
            3 => result.used_three += 1,
            _ => {}
        }
        if let Some(slot) = record.element_counts_index() {
            result.element_counts[slot] += 1;
        } else {
            result.element_out_of_range += 1;
        }
        if (record.range_type as usize) < result.range_counts.len() {
            result.range_counts[record.range_type as usize] += 1;
        } else {
            result.range_out_of_range += 1;
        }
        if record.is_internal() {
            result.internal += 1;
        }
        if record.cost == 0 {
            result.zero_cost += 1;
        }
        result.max_cost = result.max_cost.max(record.cost);
        result.last_start = record.offset;
        result.last_name_offset = record.name_offset();

        if seen_indexes.insert(record.index) {
            deduped.push(record);
        } else {
            let first_cost = records
                .iter()
                .find(|other| other.index == record.index)
                .map(|other| other.cost)
                .unwrap_or(record.cost);
            result.duplicates.push(DuplicateIndex {
                index: record.index,
                first_cost,
                second_cost: record.cost,
            });
        }
    }

    result.unique_indexes = seen_indexes.len();
    result.offered = deduped.iter().filter(|record| !record.is_internal()).count();
    result
}

impl SpellRecord {
    /// 0..4 index of the element, or `None` if out of range.
    fn element_counts_index(&self) -> Option<usize> {
        if (self.element as usize) < 5 {
            Some(self.element as usize)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a synthetic 89 B record with the given values
    /// (0 retail bytes: the fixtures live in memory, INV-L99-6).
    #[allow(clippy::too_many_arguments)]
    fn record_bytes(
        types: [u8; 6],
        element: u8,
        range_type: u8,
        cost: u16,
        durations: [u8; 9],
        chances: [u8; 9],
        magnitudes: [u8; 15],
        name: &str,
        icon: u8,
        index: u8,
    ) -> [u8; SPELL_RECORD_SIZE] {
        let mut bytes = [0u8; SPELL_RECORD_SIZE];
        bytes[0..6].copy_from_slice(&types);
        bytes[ELEMENT_OFFSET] = element;
        bytes[RANGE_TYPE_OFFSET] = range_type;
        bytes[COST_OFFSET..COST_OFFSET + 2].copy_from_slice(&cost.to_le_bytes());
        // gap 10..14 stays 0 (not read)
        bytes[DURATION_OFFSET..DURATION_OFFSET + 9].copy_from_slice(&durations);
        bytes[CHANCE_OFFSET..CHANCE_OFFSET + 9].copy_from_slice(&chances);
        bytes[MAGNITUDE_OFFSET..MAGNITUDE_OFFSET + 15].copy_from_slice(&magnitudes);
        let name_bytes = name.as_bytes();
        assert!(name_bytes.len() < SPELL_NAME_BYTES, "fits in 25 B");
        bytes[SPELL_NAME_OFFSET..SPELL_NAME_OFFSET + name_bytes.len()].copy_from_slice(name_bytes);
        bytes[ICON_OFFSET] = icon;
        bytes[SPELL_INDEX_OFFSET] = index;
        bytes
    }

    /// Retail-analogous record 0: `Fenrik's Door Jam` (goldens A2–A5).
    fn record_zero() -> [u8; SPELL_RECORD_SIZE] {
        record_bytes(
            [16, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
            4,
            0,
            0,
            [1, 1, 25, 1, 1, 1, 1, 1, 1],
            [1, 1, 1, 1, 1, 1, 1, 1, 1],
            [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            "Fenrik's Door Jam",
            1,
            1,
        )
    }

    #[test]
    fn stride_sums_to_89() {
        // Golden A8: 6+1+1+2+4+9+9+15+25+1+1+15 = 89 (spec §1.2).
        assert_eq!(SPELL_RECORD_SIZE, 89);
        assert_eq!(
            EFFECT_TYPES_OFFSET
                + EFFECT_SLOT_COUNT * 2
                + (ELEMENT_OFFSET - (EFFECT_TYPES_OFFSET + EFFECT_SLOT_COUNT * 2))
                + 1
                + 1
                + 2
                + (DURATION_OFFSET - (COST_OFFSET + 2))
                + 9
                + 9
                + 15
                + SPELL_NAME_BYTES
                + 1
                + 1
                + (SPELL_RECORD_SIZE - (SPELL_INDEX_OFFSET + 1)),
            SPELL_RECORD_SIZE
        );
    }

    #[test]
    fn parses_record_zero_like_goldens_a2_a5() {
        let bytes = record_zero();
        let records = parse_spells_std(&bytes);
        assert_eq!(records.len(), 1);
        let record = &records[0];
        // A2: bytes 0–5 = 10 ff ff ff ff ff ⇒ (16, −1) and two slots (−1, −1).
        assert_eq!(record.effects[0].effect_type, 16);
        assert_eq!(record.effects[0].sub_type, -1);
        assert_eq!(record.effects[1].effect_type, -1);
        assert_eq!(record.effects[1].sub_type, -1);
        assert_eq!(record.effects[2].effect_type, -1);
        assert_eq!(record.effects[2].sub_type, -1);
        // A3: element@6 = 4, range@7 = 0, cost@8-9 = 0 (LE).
        assert_eq!(record.element, 4);
        assert_eq!(record.range_type, 0);
        assert_eq!(record.cost, 0);
        // A4: durations slots 0..2 = (1,1,25) (1,1,1) (1,1,1).
        assert_eq!(record.effects[0].duration, [1, 1, 25]);
        assert_eq!(record.effects[1].duration, [1, 1, 1]);
        assert_eq!(record.effects[2].duration, [1, 1, 1]);
        // A5: name@47, icon@72, index@73.
        assert_eq!(record.spell_name, "Fenrik's Door Jam");
        assert_eq!(record.icon, 1);
        assert_eq!(record.index, 1);
        assert_eq!(record.used_slots(), 1, "a single slot with type != -1");
    }

    #[test]
    fn cost_is_little_endian_and_name_cuts_at_nul() {
        // A7-analogous: `04 01 86 0b` ⇒ element 4, range 1, cost 0x0B86 = 2950.
        let bytes = record_bytes(
            [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
            4,
            1,
            2950,
            [1, 1, 1, 1, 1, 1, 1, 1, 1],
            [1, 1, 1, 1, 1, 1, 1, 1, 1],
            [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            "Tame",
            1,
            99,
        );
        assert_eq!(&bytes[COST_OFFSET..COST_OFFSET + 2], &[0x86, 0x0B]);
        let records = parse_spells_std(&bytes);
        assert_eq!(records[0].cost, 2950);
        assert_eq!(records[0].spell_name, "Tame");
        assert_eq!(records[0].index, 99);
        assert_eq!(records[0].range_type, 1);
        // Name with an embedded NUL: cuts at the first one (ReadCStringSkip).
        let mut with_nul = record_zero();
        with_nul[SPELL_NAME_OFFSET..SPELL_NAME_OFFSET + 5].copy_from_slice(b"Ab\0CD");
        let records = parse_spells_std(&with_nul);
        assert_eq!(records[0].spell_name, "Ab");
    }

    #[test]
    fn control_loop_consumes_full_records_and_ignores_tail() {
        // Condition `position + 89 <= length` (DaggerfallSpellReader.cs:61):
        // 2 records + 88 trailing bytes ⇒ only 2 records.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&record_zero());
        let second = record_bytes(
            [4, 0, 0xFF, 0xFF, 0xFF, 0xFF],
            0,
            2,
            100,
            [2, 3, 1, 1, 1, 1, 1, 1, 1],
            [5, 2, 1, 1, 1, 1, 1, 1, 1],
            [3, 6, 1, 4, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            "Fireball",
            2,
            33,
        );
        bytes.extend_from_slice(&second);
        bytes.extend_from_slice(&[0u8; SPELL_RECORD_SIZE - 1]);
        let records = parse_spells_std(&bytes);
        assert_eq!(records.len(), 2);
        assert_eq!(records[1].offset, SPELL_RECORD_SIZE, "absolute offsets");
        assert_eq!(records[1].name_offset(), SPELL_RECORD_SIZE + SPELL_NAME_OFFSET);
        assert_eq!(records[1].effects[0].chance, [5, 2, 1]);
        assert_eq!(records[1].effects[0].magnitude, [3, 6, 1, 4, 1]);
    }

    #[test]
    fn census_aggregates_like_golden_family() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&record_zero()); // not internal: name without '!'
        let mut dup = record_zero();
        dup[SPELL_INDEX_OFFSET] = 58;
        dup[COST_OFFSET..COST_OFFSET + 2].copy_from_slice(&550u16.to_le_bytes());
        dup[SPELL_NAME_OFFSET] = b'!';
        bytes.extend_from_slice(&dup); // index 58, cost 550, internal
        let mut dup2 = dup;
        dup2[SPELL_NAME_OFFSET + 1] = b'x';
        bytes.extend_from_slice(&dup2); // duplicate of index 58 (internal too)
        bytes.extend_from_slice(&record_zero()); // index 1 again ⇒ also duplicated

        let records = parse_spells_std(&bytes);
        let census = census(&records);
        assert_eq!(census.records, 4);
        assert_eq!(census.used_one, 4);
        assert_eq!(census.element_counts[4], 4, "element 4 = Magic");
        assert_eq!(census.range_counts[0], 4);
        assert_eq!(census.internal, 2, "the two '!x…' records");
        // Dedupe by index keeping the first: index 1 and 58 remain.
        assert_eq!(census.unique_indexes, 2);
        assert_eq!(census.offered, 1, "index 1 (not internal) is offerable; 58 is internal");
        assert_eq!(census.zero_cost, 2, "only the cost-0 records (the duplicates cost 550)");
        assert_eq!(census.duplicates.len(), 2);
        assert_eq!(census.duplicates[0].index, 58);
        assert_eq!(census.duplicates[0].first_cost, 550);
        assert_eq!(census.duplicates[0].second_cost, 550);
        assert_eq!(census.last_start, 3 * SPELL_RECORD_SIZE);
        assert_eq!(census.last_name_offset, 3 * SPELL_RECORD_SIZE + SPELL_NAME_OFFSET);
    }

    #[test]
    fn census_flags_out_of_range_element_and_range() {
        let bytes = record_bytes(
            [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
            9,
            7,
            1,
            [1, 1, 1, 1, 1, 1, 1, 1, 1],
            [1, 1, 1, 1, 1, 1, 1, 1, 1],
            [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            "Broken",
            0,
            200,
        );
        let census = census(&parse_spells_std(&bytes));
        assert_eq!(census.element_out_of_range, 1);
        assert_eq!(census.range_out_of_range, 1);
        assert_eq!(census.used_one, 0, "all slots empty ⇒ 0 used");
    }

    #[test]
    fn internal_and_max_cost_follow_spec() {
        let mut bytes = record_zero();
        bytes[SPELL_NAME_OFFSET] = b'!';
        let mut costly = record_zero();
        costly[COST_OFFSET..COST_OFFSET + 2].copy_from_slice(&2950u16.to_le_bytes());
        let mut file = Vec::new();
        file.extend_from_slice(&bytes);
        file.extend_from_slice(&costly);
        let census = census(&parse_spells_std(&file));
        assert_eq!(census.internal, 1);
        assert_eq!(census.max_cost, 2950);
    }
}

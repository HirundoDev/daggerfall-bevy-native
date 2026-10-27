//! M26: `rumor` — rumor mill over `RUMOR.DAT`
//! (SPEC-M26-NPC-AI §4, REQ-L99-059).
//!
//! **R-A (spec §7): do not duplicate the parser.** The file layout lives in
//! [`crate::quest::RumorFile`] (`quest.rs:636-712`); this module only adds the
//! mill **rules**:
//!
//! | Rule | Source |
//! |:---|:---|
//! | loads from disk when a game is loaded; discards quests (`Flags & 4`), signs (`Flags & 1`) and NPC-bound rumors (`NPCID != 0`) | `SaveGames.cs:251-260`; `TalkManager.cs:2643-2671` |
//! | `type ∈ {4,7,10,18,26,27,28}` ⇒ flags **1** (sign); rest ⇒ **8** (spoken) | `TalkManager.cs:2695-2711` |
//! | region filter `regionID != −1 && regionID != current` ⇒ discarded | `TalkManager.cs:1464-1465` |
//! | channel filter: talking discards `flags & 1`; sign requires `flags & 1` | `TalkManager.cs:1468-1475` |
//! | faction with the `flags & 1` bit ⇒ 75 % drop | `TalkManager.cs:1483-1489` |
//! | weighted reservoir: weight 50 if `questID != 0`, otherwise 1 | `TalkManager.cs:1428-1444` |
//! | expiry: `timeLimit < now` | `TalkManager.cs:2713-2721` |
//! | `rumor mill <id>` adds without setting `timeLimit` | `Actions/RumorMill.cs:24-55` |
//! | `AddNonQuestRumor` sets `timeLimit = now + 43140` | `TalkManager.cs:2657-2685` |
//!
//! **Header equivalence (spec §4.1).** DFU reads `NPCID` as u32 @22, which
//! overlaps bytes 22..26 of `QuestName` (10..26); measured empty in 31/31, so
//! `NPCID = 0` across all retail. [`crate::quest::Rumor::npc_id`] performs that read.
//!
//! The module is pure: 0 I/O, 0 Bevy; the file arrives already parsed.

use crate::dfrandom::DfRandom;
use crate::quest::RumorFile;

/// Default weight of a quest rumor (`QuestRumorWeight`, range 1..100).
pub const QUEST_RUMOR_WEIGHT: u32 = 50;
/// Lifetime of a code-added rumor (`AddNonQuestRumor`, in classic
/// minutes, `TalkManager.cs:2683`).
pub const NON_QUEST_RUMOR_TIME_LIMIT: u32 = 43_140;
/// `allowedBulletinTextIds` (`TalkManager.cs:121`): valid sign texts.
pub const ALLOWED_BULLETIN_TEXT_IDS: [u16; 7] = [1475, 1476, 1477, 1478, 1479, 1482, 1483];
/// Probability (%) of dropping a flagged-faction rumor
/// (`Dice100.SuccessRoll(75)`, `TalkManager.cs:1487`).
pub const FACTION_RUMOR_DROP_PERCENT: u32 = 75;
/// Types that produce a sign (`GetFlagsForNewRumor`).
pub const SIGN_RUMOR_TYPES: [u8; 7] = [4, 7, 10, 18, 26, 27, 28];
/// `flags` of a sign.
pub const FLAG_SIGN: u8 = 1;
/// `flags` of a spoken rumor.
pub const FLAG_SPOKEN: u8 = 8;
/// `flags & 4` = quest rumor (not imported from the file).
pub const FLAG_QUEST: u8 = 4;

/// `GetFlagsForNewRumor(type)` (`TalkManager.cs:2695-2711`).
pub const fn flags_for_new_rumor(kind: u8) -> u8 {
    let mut index = 0;
    while index < SIGN_RUMOR_TYPES.len() {
        if SIGN_RUMOR_TYPES[index] == kind {
            return FLAG_SIGN;
        }
        index += 1;
    }
    FLAG_SPOKEN
}

/// A mill rumor (`RumorMillEntry`, `TalkManager.cs:341-364`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RumorMillEntry {
    /// `CommonRumor` (from file or `AddNonQuestRumor`) or
    /// `QuestRumorMill` (`questID != 0`).
    pub quest_id: u64,
    /// `timeLimit` in classic minutes (0 = no limit until refreshed).
    pub time_limit: u32,
    /// `Faction1`.
    pub faction1: u16,
    /// `Faction2`.
    pub faction2: u16,
    /// `RegionID` (file `u8`; −1 is represented as `None`).
    pub region: Option<i16>,
    /// `Flags` (1 sign / 8 spoken).
    pub flags: u8,
    /// File `Type`.
    pub kind: u8,
    /// `textID` (0 = text from the file itself, not `TEXT.RSC`).
    pub text_id: u16,
    /// Record text (`RUMOR.DAT`), already resolved; `None` for quest
    /// rumors (their variants are supplied by M29).
    pub text: Option<String>,
}

impl RumorMillEntry {
    /// Is this a quest rumor (weight [`QUEST_RUMOR_WEIGHT`])?
    pub const fn is_quest(&self) -> bool {
        self.quest_id != 0
    }

    /// Is this a sign?
    pub const fn is_sign(&self) -> bool {
        self.flags & FLAG_SIGN == FLAG_SIGN
    }

    /// Imports a `RUMOR.DAT` record (`ImportClassicRumor`): discards
    /// quests (`Flags & 4`), signs (`Flags & 1`) and rumors bound to an NPC
    /// (`NPCID != 0`).
    pub fn from_record(
        rumor: &crate::quest::Rumor,
        text: Option<String>,
    ) -> Option<RumorMillEntry> {
        if rumor.flags & FLAG_QUEST != 0 || rumor.flags & FLAG_SIGN != 0 || rumor.npc_id() != 0 {
            return None;
        }
        Some(RumorMillEntry {
            quest_id: 0,
            time_limit: rumor.time_limit,
            faction1: rumor.faction1,
            faction2: rumor.faction2,
            region: Some(i16::from(rumor.region)),
            flags: rumor.flags,
            kind: rumor.kind,
            text_id: 0,
            text,
        })
    }
}

/// Rumor mill (`listRumorMill`, `TalkManager.cs:341-364`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RumorMill {
    entries: Vec<RumorMillEntry>,
}

impl RumorMill {
    /// Empty mill (0 disk reads: `SetupRumorMill` only initializes).
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads from an already parsed `RUMOR.DAT` (`ImportClassicRumor` rule).
    pub fn from_file(file: &RumorFile) -> Self {
        let entries = file
            .rumors
            .iter()
            .enumerate()
            .filter_map(|(index, rumor)| RumorMillEntry::from_record(rumor, file.text(index)))
            .collect();
        Self { entries }
    }

    /// Mill entries.
    pub fn entries(&self) -> &[RumorMillEntry] {
        &self.entries
    }

    /// Adds an already built entry (caller-side selective import).
    pub fn add_entry(&mut self, entry: RumorMillEntry) {
        self.entries.push(entry);
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Is the mill empty?
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `AddQuestRumorToRumorMill` (`Actions/RumorMill.cs:24-55`): **without**
    /// setting `timeLimit`.
    pub fn add_quest_rumor(&mut self, quest_id: u64, text: Option<String>) -> &RumorMillEntry {
        self.entries.push(RumorMillEntry {
            quest_id,
            time_limit: 0,
            faction1: 0,
            faction2: 0,
            region: None,
            flags: FLAG_SPOKEN,
            kind: 0,
            text_id: 0,
            text,
        });
        self.entries.last().expect("just added")
    }

    /// `AddNonQuestRumor` (`TalkManager.cs:2657-2685`): flags derived from the
    /// type and `timeLimit = now + 43140`.
    pub fn add_non_quest_rumor(
        &mut self,
        faction1: u16,
        faction2: u16,
        region: i16,
        kind: u8,
        text_id: u16,
        now: u32,
    ) -> &RumorMillEntry {
        self.entries.push(RumorMillEntry {
            quest_id: 0,
            time_limit: now.saturating_add(NON_QUEST_RUMOR_TIME_LIMIT),
            faction1,
            faction2,
            region: Some(region),
            flags: flags_for_new_rumor(kind),
            kind,
            text_id,
            text: None,
        });
        self.entries.last().expect("just added")
    }

    /// `RefreshRumorMill` (`TalkManager.cs:2713-2721`): removes
    /// `timeLimit < now` (condition is **strict**).
    pub fn refresh(&mut self, now: u32) {
        self.entries.retain(|entry| entry.time_limit >= now);
    }

    /// `GetValidRumors` (`TalkManager.cs:1446-1497`): region, channel, signs
    /// and the 75 % flagged-faction drop (with the roll already seeded).
    pub fn valid<'a>(
        &'a self,
        current_region: i16,
        reading_sign: bool,
        faction_flagged: impl Fn(u16) -> bool,
        rng: &mut DfRandom,
    ) -> Vec<&'a RumorMillEntry> {
        let mut valid = Vec::new();
        for entry in &self.entries {
            if reading_sign && entry.text_id != 0 && !ALLOWED_BULLETIN_TEXT_IDS.contains(&entry.text_id)
            {
                continue;
            }
            if entry.quest_id == 0 {
                if let Some(region) = entry.region {
                    if region != -1 && region != current_region {
                        continue;
                    }
                }
                if !reading_sign && entry.is_sign() {
                    continue;
                }
                if reading_sign && !entry.is_sign() {
                    continue;
                }
                if entry.faction1 != 0 || entry.faction2 != 0 || entry.kind != 100 {
                    let flagged = (entry.faction1 != 0 && faction_flagged(entry.faction1))
                        || (entry.faction2 != 0 && faction_flagged(entry.faction2));
                    if flagged && rng.rand() % 100 < FACTION_RUMOR_DROP_PERCENT {
                        continue;
                    }
                }
            }
            valid.push(entry);
        }
        valid
    }

    /// Weighted reservoir (`WeightedRandomRumor`, `TalkManager.cs:1428-1444`):
    /// weight `quest_weight` if `questID != 0`, otherwise 1.
    pub fn select_weighted<'a>(
        valid: &[&'a RumorMillEntry],
        quest_weight: u32,
        rng: &mut DfRandom,
    ) -> Option<&'a RumorMillEntry> {
        let mut selected: Option<&RumorMillEntry> = None;
        let mut total = 0u32;
        for entry in valid {
            let weight = if entry.is_quest() { quest_weight } else { 1 };
            let roll = rng.rand() % (total + weight);
            if roll >= total {
                selected = Some(entry);
            }
            total += weight;
        }
        selected
    }

    /// `GetValidRumors` + `WeightedRandomRumor` in one call.
    pub fn pick<'a>(
        &'a self,
        current_region: i16,
        reading_sign: bool,
        faction_flagged: impl Fn(u16) -> bool,
        quest_weight: u32,
        rng: &mut DfRandom,
    ) -> Option<&'a RumorMillEntry> {
        let valid = self.valid(current_region, reading_sign, faction_flagged, rng);
        Self::select_weighted(&valid, quest_weight, rng)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quest::Rumor;

    fn synthetic(region: u8, flags: u8, kind: u8, time_limit: u32) -> Rumor {
        Rumor {
            faction1: 0,
            faction2: 0,
            kind,
            reserved: [0; 3],
            region,
            flags,
            quest_name: [0; 16],
            text_length: 2,
            unknown: 0,
            time_limit,
        }
    }

    #[test]
    fn flags_by_type_match_the_source_table() {
        for kind in [4u8, 7, 10, 18, 26, 27, 28] {
            assert_eq!(flags_for_new_rumor(kind), FLAG_SIGN);
        }
        assert_eq!(flags_for_new_rumor(12), FLAG_SPOKEN);
    }

    #[test]
    fn imports_only_spoken_unbound_rumors() {
        let spoken = synthetic(5, FLAG_SPOKEN, 12, 566_670);
        assert!(RumorMillEntry::from_record(&spoken, None).is_some());
        let sign = synthetic(5, FLAG_SIGN, 26, 566_670);
        assert!(RumorMillEntry::from_record(&sign, None).is_none());
        let quest = synthetic(5, FLAG_QUEST, 12, 566_670);
        assert!(RumorMillEntry::from_record(&quest, None).is_none());
        let mut bound = synthetic(5, FLAG_SPOKEN, 12, 566_670);
        bound.quest_name[12] = 1;
        assert!(RumorMillEntry::from_record(&bound, None).is_none());
    }

    #[test]
    fn expiry_is_strictly_less_than_now() {
        let mut mill = RumorMill::new();
        mill.add_non_quest_rumor(0, 0, 0, 12, 0, 100);
        assert_eq!(mill.entries()[0].time_limit, 100 + NON_QUEST_RUMOR_TIME_LIMIT);
        let limit = mill.entries()[0].time_limit;
        mill.refresh(limit);
        assert_eq!(mill.len(), 1);
        mill.refresh(limit + 1);
        assert!(mill.is_empty());
    }

    #[test]
    fn region_and_channel_filters() {
        let mut mill = RumorMill::new();
        mill.entries.push(RumorMillEntry {
            quest_id: 0,
            time_limit: 0,
            faction1: 0,
            faction2: 0,
            region: Some(60),
            flags: FLAG_SPOKEN,
            kind: 12,
            text_id: 0,
            text: None,
        });
        mill.entries.push(RumorMillEntry {
            quest_id: 0,
            time_limit: 0,
            faction1: 0,
            faction2: 0,
            region: Some(-1),
            flags: FLAG_SIGN,
            kind: 26,
            text_id: 0,
            text: None,
        });
        let mut rng = DfRandom::new();
        rng.srand(1);
        let talking = mill.valid(0, false, |_| false, &mut rng);
        assert_eq!(talking.len(), 0);
        let sign = mill.valid(0, true, |_| false, &mut rng);
        assert_eq!(sign.len(), 1);
        assert!(sign[0].is_sign());
        let mut rng2 = DfRandom::new();
        rng2.srand(1);
        let unlimited = mill.valid(0, true, |_| false, &mut rng2);
        assert_eq!(unlimited.len(), 1, "region −1 is kept");
    }

    #[test]
    fn weighted_reservoir_gives_quest_rumors_weight_50() {
        let mut mill = RumorMill::new();
        mill.add_quest_rumor(1, None);
        mill.add_quest_rumor(2, None);
        mill.add_non_quest_rumor(0, 0, 0, 12, 0, 0);
        let mut rng = DfRandom::new();
        rng.srand(20_261_021);
        let mut quest = 0usize;
        let mut ambient = 0usize;
        for _ in 0..1_000 {
            let valid: Vec<&RumorMillEntry> = mill.entries().iter().collect();
            let Some(picked) = RumorMill::select_weighted(&valid, QUEST_RUMOR_WEIGHT, &mut rng) else {
                panic!("empty selection")
            };
            if picked.is_quest() {
                quest += 1;
            } else {
                ambient += 1;
            }
        }
        assert!((950..=1_010).contains(&quest), "P(quest) out of range: {quest}");
        assert!(ambient > 0);
    }

    #[test]
    fn flagged_faction_rumors_are_dropped_three_times_in_four() {
        let mut mill = RumorMill::new();
        mill.entries.push(RumorMillEntry {
            quest_id: 0,
            time_limit: 0,
            faction1: 42,
            faction2: 0,
            region: None,
            flags: FLAG_SPOKEN,
            kind: 100,
            text_id: 0,
            text: None,
        });
        let mut rng = DfRandom::new();
        rng.srand(1);
        const DRAWS: usize = 1_000;
        let kept = (0..DRAWS)
            .filter(|_| !mill.valid(0, false, |id| id == 42, &mut rng).is_empty())
            .count();
        assert!(
            (200..=300).contains(&kept),
            "flagged kept {kept}/{DRAWS}, expected ≈250 (75 % dropped)"
        );
        let mut unmarked = DfRandom::new();
        unmarked.srand(1);
        assert_eq!(
            mill.valid(0, false, |_| false, &mut unmarked).len(),
            1,
            "unflagged is always kept"
        );
    }
}

//! M26: [`dialogue`] — NPC dialogue: greeting, topics by
//! faction/locality, knowledge and answer (SPEC-M26-NPC-AI §3,
//! REQ-L99-059).
//!
//! **Text rule (N-1 / §6.5).** This module **does not** contain prose: it
//! emits `TEXT.RSC` **IDs** (M16 contract, REQ-L99-048) and topic keys. The
//! probe requires every emitted ID to be resolvable in the user's file;
//! an invented answer does not pass.
//!
//! **Source** (`0038a1e6`, MIT, executable spec): `TalkManager.cs:91-93,107-119,
//! 566-605,887-975,1970-2040,3064-3478` · `DaggerfallTalkWindow.cs:86-105`.
//! The ID tables (`greetings`, `answersToDirections`,
//! `answersToNonDirections`, `knowledgeModifiers`, `questionTypeReactionMods`,
//! `etiquetteReactionMods`, `streetwiseReactionMods`) are transcribed **as
//! data** from the source; 0 retail text.
//!
//! **Deterministic knowledge (N-3, closed).** The roll is normalized:
//! `rand(1..=20) ≤ knowledgeModifiers[q·5 + group] + 10` seeded with
//! `hash(NPC) + hash(topic key)` truncated to `uint` and fed into DFU's LCG
//! (`DFRandom.Seed = npcHash; DFRandom.Seed += keyHash`,
//! `TalkManager.cs:585-604`). The same pair (NPC, topic) answers the same
//! within the session and across runs.
//!
//! **Topics by faction/locality.** DFU assembles the lists from the active
//! quest resources, the locality's buildings and a fixed table of 34
//! factions (`infoFactionIDs`, `TalkManager.cs:378,3166-3174`). This module
//! receives those three sources as **input** ([`TalkContext`]) and invents
//! none: the informative faction list is the caller's (the fixed
//! table, plus the NPC's faction if the target decides to add it).

use crate::determinism::{fnv1a64, DfRandom};

/// Greeting `≥ 30` (`minVeryLikeReaction`, `TalkManager.cs:93,960-971`).
pub const GREETING_VERY_LIKE: u16 = 7209;
/// Greeting `≥ 10`.
pub const GREETING_LIKE: u16 = 7208;
/// Greeting `≥ 0` (`minNeutralReaction`).
pub const GREETING_NEUTRAL: u16 = 7207;
/// Final greeting (`≥ −20`).
pub const GREETING_DISLIKE: u16 = 7206;
/// `minVeryLikeReaction`.
pub const MIN_VERY_LIKE_REACTION: i16 = 30;
/// `minLikeReaction`.
pub const MIN_LIKE_REACTION: i16 = 10;
/// `minNeutralReaction`.
pub const MIN_NEUTRAL_REACTION: i16 = 0;
/// Below this reaction there is no answer (`TalkManager.cs:2608-2612`).
pub const NO_ANSWER_BELOW: i16 = -20;
/// `maxNumAnswersNpcGivesTellMeAboutOrRumors` (`TalkManager.cs:125`).
pub const MAX_ANSWERS_TELL_ME_ABOUT: u8 = 1;
/// `answersToDirections` (`TalkManager.cs:107-108`).
pub const ANSWERS_TO_DIRECTIONS: [u16; 30] = [
    7251, 7266, 7281, 7250, 7265, 7280, 7252, 7267, 7282, 7253, 7268, 7283, 7304, 7269, 7284, 7256,
    7271, 7286, 7255, 7270, 7285, 7257, 7272, 7287, 7258, 7273, 7288, 7259, 7274, 7289,
];
/// `answersToNonDirections` (`TalkManager.cs:109-110`).
pub const ANSWERS_TO_NON_DIRECTIONS: [u16; 30] = [
    7251, 7266, 7281, 7250, 7265, 7280, 7252, 7267, 7282, 7253, 7268, 7283, 7304, 7269, 7284, 7261,
    7276, 7291, 7260, 7275, 7290, 7262, 7277, 7292, 7263, 7278, 7293, 7264, 7279, 7294,
];
/// `greetings` (`TalkManager.cs:117-119`), 9 indices × 3 answers.
pub const GREETINGS: [u16; 27] = [
    8550, 8551, 8552, 8553, 8554, 8555, 8556, 8557, 8558, 8559, 8560, 8561, 8562, 8562, 8563, 8564,
    8564, 8565, 8566, 8566, 8567, 8568, 8568, 8569, 8570, 8570, 8571,
];
/// `knowledgeModifiers` (`TalkManager.cs:112-115`): 8 types × 5 groups.
pub const KNOWLEDGE_MODIFIERS: [i8; 40] = [
    5, 7, 0, 0, 4, 1, 2, -2, 3, 7, -3, 0, 7, 2, 4, 4, 3, -2, -4, -3, 0, 3, 5, 2, 0, -4, -3, -6, -3, 4,
    -7, -5, 7, 0, 1, -1, 1, 6, 4, 2,
];
/// `questionTypeReactionMods` (`TalkManager.cs:97`).
pub const QUESTION_TYPE_REACTION_MODS: [i8; 8] = [5, 0, 0, 0, 5, 0, 0, 0];
/// `etiquetteReactionMods` (`TalkManager.cs:100`).
pub const ETIQUETTE_REACTION_MODS: [i8; 5] = [-10, 5, 10, 15, -15];
/// `streetwiseReactionMods` (`TalkManager.cs:101`).
pub const STREETWISE_REACTION_MODS: [i8; 5] = [10, 5, -10, -15, 15];

/// Conversation tone (`TalkTone`, `DaggerfallTalkWindow.cs:86-105`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TalkTone {
    /// Etiquette (index 0).
    Polite,
    /// Normal (index 1).
    Normal,
    /// Street (index 2).
    Blunt,
}

impl TalkTone {
    /// Source index.
    pub const fn index(self) -> usize {
        match self {
            Self::Polite => 0,
            Self::Normal => 1,
            Self::Blunt => 2,
        }
    }
}

/// Bounded social group (mobiles ⇒ `Commoners`; statics ⇒ `sgroup < 5 ?
/// sgroup : Merchants`, `TalkManager.cs:802,836`). Indices 0..4 index
/// the answer and modifier tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocialGroup {
    /// 0.
    Commoners,
    /// 1.
    Merchants,
    /// 2.
    Scholars,
    /// 3.
    Nobility,
    /// 4.
    Underworld,
}

impl SocialGroup {
    /// Normalizes a raw `sgroup`: `>= 5` ⇒ `Merchants`; negative ⇒
    /// `Commoners`.
    pub fn from_raw(raw: i16) -> Self {
        match raw {
            0 => Self::Commoners,
            1 => Self::Merchants,
            2 => Self::Scholars,
            3 => Self::Nobility,
            4 => Self::Underworld,
            r if r >= 5 => Self::Merchants,
            _ => Self::Commoners,
        }
    }

    /// Index 0..4.
    pub const fn index(self) -> usize {
        match self {
            Self::Commoners => 0,
            Self::Merchants => 1,
            Self::Scholars => 2,
            Self::Nobility => 3,
            Self::Underworld => 4,
        }
    }

    /// Index used by the reaction/knowledge tables: `sgroup >= 5`
    /// collapses to `Merchants` (`TalkManager.cs:614-616`).
    pub const fn table_index(self) -> usize {
        self.index()
    }
}

/// `QuestionType` (`TalkManager.cs:135-149`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestionType {
    /// "any news?".
    News,
    /// "where am I?".
    WhereAmI,
    /// "tell me about" → organizations.
    OrganizationInfo,
    /// "is there any work?".
    Work,
    /// "where is" → local building.
    LocalBuilding,
    /// "where is" → regional building.
    Regional,
    /// "where is" → person.
    Person,
    /// "where is" → thing (unused in classic).
    Thing,
    /// Quest resource: location.
    QuestLocation,
    /// Quest resource: person.
    QuestPerson,
    /// Quest resource: item.
    QuestItem,
}

impl QuestionType {
    /// Classic index 0..7 (`GetClassicQuestionIndex`,
    /// `TalkManager.cs:669-699`).
    pub const fn classic_index(self) -> usize {
        match self {
            Self::LocalBuilding | Self::Regional => 0,
            Self::Person => 1,
            Self::Thing => 2,
            Self::Work => 3,
            Self::QuestLocation | Self::OrganizationInfo => 4,
            Self::QuestPerson => 5,
            Self::QuestItem => 6,
            Self::News | Self::WhereAmI => 7,
        }
    }

    /// Does the answer come from `answersToDirections`?
    pub const fn is_direction(self) -> bool {
        matches!(self, Self::LocalBuilding | Self::Regional | Self::Person)
    }
}

/// Result of the greeting route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Greeting {
    /// `TEXT.RSC` record.
    Record(u16),
    /// No answer (`reactionToPlayer < −20` or a previous rejection).
    NoAnswer,
}

/// **Simple** greeting route (`TalkManager.cs:955-971`): valid only with a
/// parent faction of `Province`, no quest greeting, `reaction ≥ −20` and no
/// previous rejection. Returns 7209/7208/7207/7206.
pub fn simple_greeting(reaction_to_player: i16, already_rejected: bool) -> Greeting {
    if already_rejected || reaction_to_player < NO_ANSWER_BELOW {
        return Greeting::NoAnswer;
    }
    let record = if reaction_to_player >= MIN_VERY_LIKE_REACTION {
        GREETING_VERY_LIKE
    } else if reaction_to_player >= MIN_LIKE_REACTION {
        GREETING_LIKE
    } else if reaction_to_player >= MIN_NEUTRAL_REACTION {
        GREETING_NEUTRAL
    } else {
        GREETING_DISLIKE
    };
    Greeting::Record(record)
}

/// Guild-indexed route (`TalkManager.cs:931-957`): `greetings[3·idx]`
/// (very good), `[1+3·idx]` (normal) and `[2+3·idx]` (rejection). Returns
/// `None` when the source falls back to the simple route (index 8 with
/// reputation between 6 and 29).
pub fn indexed_greeting(
    index: usize,
    faction_reputation: i16,
    reaction: i16,
    already_rejected: bool,
) -> Option<Greeting> {
    if already_rejected {
        return Some(Greeting::NoAnswer);
    }
    let index = index.min(GREETINGS.len() / 3 - 1);
    if faction_reputation >= reaction {
        if faction_reputation >= MIN_VERY_LIKE_REACTION && index != 8 {
            return Some(Greeting::Record(GREETINGS[3 * index]));
        }
        if faction_reputation <= 5 || index != 8 {
            return Some(Greeting::Record(GREETINGS[1 + 3 * index]));
        }
        return None;
    }
    Some(Greeting::Record(GREETINGS[2 + 3 * index]))
}

/// Reaction band `0..2` (`GetReactionToPlayer_0_1_2`,
/// `TalkManager.cs:639-666`): `0` if `reaction < roll`, `1` if
/// `reaction < roll + 20`, `2` otherwise.
pub fn reaction_band(reaction: i32, roll_to_beat: i32) -> u8 {
    if reaction < roll_to_beat {
        0
    } else if reaction < roll_to_beat + 20 {
        1
    } else {
        2
    }
}

/// Reaction score to a question (`TalkManager.cs:639-646`), before
/// the band: `personality/5 + mod[type] + tone + mod[social]`.
pub fn reaction_score(
    personality: i16,
    question: QuestionType,
    tone: TalkTone,
    social_group: SocialGroup,
    equipped_skill_check_passed: bool,
) -> i32 {
    let group = social_group.table_index();
    let mut score = i32::from(personality) / 5 + i32::from(QUESTION_TYPE_REACTION_MODS[question.classic_index()]);
    match tone {
        TalkTone::Polite => {
            score += i32::from(ETIQUETTE_REACTION_MODS[group]);
            score += if equipped_skill_check_passed { 5 } else { -10 };
        }
        TalkTone::Blunt => {
            score += i32::from(STREETWISE_REACTION_MODS[group]);
            score += if equipped_skill_check_passed { 5 } else { -10 };
        }
        TalkTone::Normal => {}
    }
    score
}

/// Knowledge seed `hash(NPC) + hash(key)` (`TalkManager.cs:585-595`:
/// `DFRandom.Seed = npcHash; DFRandom.Seed += keyHash`, `uint` seed).
pub fn knowledge_hash(npc_id: u64, topic_key: &str) -> u32 {
    fnv1a64(&npc_id.to_le_bytes()).wrapping_add(fnv1a64(topic_key.as_bytes())) as u32
}

/// Does the NPC know the topic? `rand(1..=20) ≤ knowledgeModifiers[q·5 + group] +
/// 10` (`TalkManager.cs:597-604`), with DFU's LCG `random_range_inclusive`.
pub fn knows_topic(seed: u32, question: QuestionType, social_group: SocialGroup) -> bool {
    let index = question.classic_index() * 5 + social_group.table_index();
    let roll_to_beat = i32::from(KNOWLEDGE_MODIFIERS[index]) + 10;
    let mut rng = DfRandom::srand(seed);
    rng.random_range_inclusive(1, 20) <= roll_to_beat
}

/// Answer to a direction question (`answersToDirections`,
/// `TalkManager.cs:1825,1843`).
pub fn direction_answer(social_group: SocialGroup, reaction: u8) -> u16 {
    ANSWERS_TO_DIRECTIONS[3 * social_group.table_index() + usize::from(reaction.min(2))]
}

/// "Tell me about" answer (`answersToNonDirections`,
/// `TalkManager.cs:2028-2039`): negative branch in `0..15`, positive in
/// `15..30`.
pub fn non_direction_answer(knows: bool, social_group: SocialGroup, reaction: u8) -> u16 {
    let group = social_group.table_index();
    let reaction = usize::from(reaction.min(2));
    let index = if knows {
        15 + 3 * group + reaction
    } else {
        3 * group + reaction
    };
    ANSWERS_TO_NON_DIRECTIONS[index]
}

/// Length of the fixed `infoFactionIDs` table (`TalkManager.cs:378`): only its
/// positions (`0..34`) have `OrganizationInfo` text in `TEXT.RSC`
/// (860..894). A faction added by the caller beyond the table has no
/// fixed record: `organization_answer(34) = 895` **does not exist** in
/// retail (1408 records measured).
pub const INFO_FACTION_TABLE_LEN: usize = 34;

/// `OrganizationInfo` answer: `860 + (i > 7 ? i + 1 : i)`
/// (`GetOrganizationInfo`, `TalkManager.cs:1513-1517`). Valid as a `TEXT.RSC`
/// record only for `index < INFO_FACTION_TABLE_LEN`.
pub fn organization_answer(index: usize) -> u16 {
    860 + (if index > 7 { index + 1 } else { index }) as u16
}

/// Topic type (the four lists of SPEC §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopicKind {
    /// "any news".
    AnyNews,
    /// "where am I".
    WhereAmI,
    /// Organization (faction from the fixed table).
    Organization,
    /// Quest resource: location.
    QuestLocation,
    /// Quest resource: person.
    QuestPerson,
    /// Quest resource: item.
    QuestItem,
    /// Locality building.
    LocalBuilding,
}

impl TopicKind {
    /// Stable code for digests.
    pub const fn code(self) -> u8 {
        match self {
            Self::AnyNews => 0,
            Self::WhereAmI => 1,
            Self::Organization => 2,
            Self::QuestLocation => 3,
            Self::QuestPerson => 4,
            Self::QuestItem => 5,
            Self::LocalBuilding => 6,
        }
    }
}

/// Topic in a list: stable key, type and (when applicable) `TEXT.RSC` ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Topic {
    /// Stable key (quest symbol, `building:<key>`, `org:<id>`…).
    pub key: String,
    /// Topic type.
    pub kind: TopicKind,
    /// Associated faction (only `Organization`).
    pub faction_id: Option<u32>,
    /// `TEXT.RSC` ID when the answer is a fixed record.
    pub text_id: Option<u16>,
}

/// Locality building (entry of the *Location* list).
///
/// **Identity.** The key is the triple `(block_x, block_y, record_index)`,
/// equivalent to DFU's `MakeBuildingKey` and unique within a location.
/// The `.RMB` `sector` and `location_id` do **not** identify a building (measured
/// 2026-10-21: `sector` comes from the grid in 97.5 % of cases and
/// `location_id` repeats across 7,533 locations), so they are not used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingTopic {
    /// Block X cell in the location grid.
    pub block_x: usize,
    /// Block Y cell in the grid.
    pub block_y: usize,
    /// Sub-record index within the block.
    pub record_index: usize,
    /// Retail building name.
    pub name: String,
    /// Building type (`DFLocation.BuildingTypes`).
    pub building_type: u8,
}

impl BuildingTopic {
    /// Stable key `building:<block_x>:<block_y>:<record_index>`.
    pub fn key(&self) -> String {
        format!(
            "building:{}:{}:{}",
            self.block_x, self.block_y, self.record_index
        )
    }
}

/// Quest resource visible to dialogue (M29 publishes them; here they are input).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestTopic {
    /// Resource symbol.
    pub key: String,
    /// Type.
    pub kind: TopicKind,
    /// `availableForDialog`.
    pub available_for_dialog: bool,
    /// `hasEntryInTellMeAbout`.
    pub has_entry_in_tell_me_about: bool,
    /// `hasEntryInWhereIs`.
    pub has_entry_in_where_is: bool,
    /// The resource is in the player's locality.
    pub same_location: bool,
    /// The resource **is** the NPC being talked to.
    pub is_dialog_partner: bool,
}

/// Dialogue data context (all borrowed, 0 invention).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TalkContext<'a> {
    /// Buildings of the current locality (*Location* list).
    pub buildings: &'a [BuildingTopic],
    /// Active quest resources (*Tell me about* and *Person* lists).
    pub quest_topics: &'a [QuestTopic],
    /// Informative factions in fixed-table order (34 entries in
    /// DFU, `TalkManager.cs:378`); the index fixes the answer ID.
    pub info_factions: &'a [u32],
}

/// The four topic lists (`TalkManager.cs:3064-3478`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TopicLists {
    /// "Tell me about": news, where am I, quest resources and organizations.
    pub tell_me_about: Vec<Topic>,
    /// "Where is": locality buildings and quest locations.
    pub location: Vec<Topic>,
    /// "People": locality quest persons.
    pub person: Vec<Topic>,
    /// "Things": empty by design (`TalkManager.cs:3474-3478`).
    pub thing: Vec<Topic>,
}

impl TopicLists {
    /// FNV-1a 64 digest of the keys and types (same context ⇒ same integer).
    pub fn digest(&self) -> u64 {
        let mut bytes = Vec::new();
        for list in [&self.tell_me_about, &self.location, &self.person, &self.thing] {
            bytes.extend_from_slice(&(list.len() as u32).to_le_bytes());
            for topic in list {
                bytes.push(topic.kind.code());
                bytes.extend_from_slice(&(topic.key.len() as u32).to_le_bytes());
                bytes.extend_from_slice(topic.key.as_bytes());
                if let Some(faction) = topic.faction_id {
                    bytes.extend_from_slice(&faction.to_le_bytes());
                }
                if let Some(text_id) = topic.text_id {
                    bytes.extend_from_slice(&text_id.to_le_bytes());
                }
            }
        }
        fnv1a64(&bytes)
    }

    /// Total number of topics.
    pub fn len(&self) -> usize {
        self.tell_me_about.len() + self.location.len() + self.person.len() + self.thing.len()
    }

    /// No topic in any list?
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Are all visible lists empty? (anti-false-positive (i)).
    pub fn visible_lists_empty(&self) -> bool {
        self.tell_me_about.is_empty() && self.location.is_empty()
    }
}

/// Assembles the topic lists (`AssembleTopicLists`,
/// `TalkManager.cs:3064-3478`): fixed ones (news, where am I), visible
/// quest resources, organizations from the fixed table, locality buildings
/// and quest persons. `Thing` stays empty by design.
pub fn assemble_topics(context: &TalkContext<'_>) -> TopicLists {
    let mut lists = TopicLists::default();

    lists.tell_me_about.push(Topic {
        key: "news".to_string(),
        kind: TopicKind::AnyNews,
        faction_id: None,
        text_id: None,
    });
    lists.tell_me_about.push(Topic {
        key: "where_am_i".to_string(),
        kind: TopicKind::WhereAmI,
        faction_id: None,
        text_id: None,
    });

    for quest in context.quest_topics {
        if !quest.available_for_dialog || quest.is_dialog_partner {
            continue;
        }
        if quest.has_entry_in_tell_me_about {
            lists.tell_me_about.push(Topic {
                key: quest.key.clone(),
                kind: quest.kind,
                faction_id: None,
                text_id: None,
            });
        }
        if quest.same_location && quest.has_entry_in_where_is {
            match quest.kind {
                TopicKind::QuestPerson => lists.person.push(Topic {
                    key: quest.key.clone(),
                    kind: quest.kind,
                    faction_id: None,
                    text_id: None,
                }),
                TopicKind::QuestLocation => lists.location.push(Topic {
                    key: quest.key.clone(),
                    kind: quest.kind,
                    faction_id: None,
                    text_id: None,
                }),
                _ => {}
            }
        }
    }

    for (index, faction) in context.info_factions.iter().enumerate() {
        let key = format!("org:{faction}");
        if lists
            .tell_me_about
            .iter()
            .any(|topic| topic.kind == TopicKind::Organization && topic.key == key)
        {
            continue;
        }
        lists.tell_me_about.push(Topic {
            key,
            kind: TopicKind::Organization,
            faction_id: Some(*faction),
            // Factions outside the fixed table (the NPC's own) have no
            // measured `OrganizationInfo` record in `TEXT.RSC`.
            text_id: (index < INFO_FACTION_TABLE_LEN).then(|| organization_answer(index)),
        });
    }

    for building in context.buildings {
        lists.location.push(Topic {
            key: building.key(),
            kind: TopicKind::LocalBuilding,
            faction_id: None,
            text_id: None,
        });
    }

    lists
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_greeting_family_d() {
        assert_eq!(simple_greeting(30, false), Greeting::Record(7209));
        assert_eq!(simple_greeting(29, false), Greeting::Record(7208));
        assert_eq!(simple_greeting(10, false), Greeting::Record(7208));
        assert_eq!(simple_greeting(0, false), Greeting::Record(7207));
        assert_eq!(simple_greeting(-1, false), Greeting::Record(7206));
        assert_eq!(simple_greeting(100, false), Greeting::Record(7209));
        assert_eq!(simple_greeting(-21, false), Greeting::NoAnswer);
        assert_eq!(simple_greeting(40, true), Greeting::NoAnswer);
    }

    #[test]
    fn indexed_greeting_uses_three_columns() {
        assert_eq!(
            indexed_greeting(1, 40, 0, false),
            Some(Greeting::Record(GREETINGS[3]))
        );
        assert_eq!(
            indexed_greeting(1, 6, 0, false),
            Some(Greeting::Record(GREETINGS[4]))
        );
        assert_eq!(
            indexed_greeting(2, -5, 0, false),
            Some(Greeting::Record(GREETINGS[8]))
        );
        // Index 8 with reputation 6..29 falls back to the simple route.
        assert_eq!(indexed_greeting(8, 6, 0, false), None);
    }

    #[test]
    fn knowledge_seed_and_roll_match_the_dfu_lcg() {
        // `srand(1).rand() = 16838` (next = 1·1103515245 + 12345; (next>>16)&0x7FFF).
        let mut rng = DfRandom::srand(1);
        assert_eq!(rng.rand(), 16838);
        let mut rng = DfRandom::srand(1);
        assert_eq!(rng.random_range_inclusive(1, 20), 19);
        // sum (not XOR) of both hashes, truncated to DFU's `uint` seed
        assert_eq!(
            knowledge_hash(7, "org:42"),
            fnv1a64(&7u64.to_le_bytes()).wrapping_add(fnv1a64(b"org:42")) as u32
        );
    }

    #[test]
    fn knowledge_is_stable_per_npc_and_topic() {
        let hash = knowledge_hash(7, "org:42");
        let first = knows_topic(hash, QuestionType::OrganizationInfo, SocialGroup::Commoners);
        let second = knows_topic(hash, QuestionType::OrganizationInfo, SocialGroup::Commoners);
        assert_eq!(first, second);
    }

    #[test]
    fn answer_tables_have_the_expected_bounds() {
        // Negative (0..15) and positive (15..30) branches never overlap.
        for group in [
            SocialGroup::Commoners,
            SocialGroup::Merchants,
            SocialGroup::Scholars,
            SocialGroup::Nobility,
            SocialGroup::Underworld,
        ] {
            for reaction in 0..=2u8 {
                let negative = non_direction_answer(false, group, reaction);
                let positive = non_direction_answer(true, group, reaction);
                assert!(ANSWERS_TO_NON_DIRECTIONS.contains(&negative));
                assert!(ANSWERS_TO_NON_DIRECTIONS.contains(&positive));
                assert_ne!(negative, positive);
                assert!(ANSWERS_TO_DIRECTIONS.contains(&direction_answer(group, reaction)));
            }
        }
        assert_eq!(organization_answer(0), 860);
        assert_eq!(organization_answer(7), 867);
        assert_eq!(organization_answer(8), 869);
    }

    #[test]
    fn reaction_band_thresholds() {
        assert_eq!(reaction_band(5, 6), 0);
        assert_eq!(reaction_band(6, 6), 1);
        assert_eq!(reaction_band(25, 6), 1);
        assert_eq!(reaction_band(26, 6), 2);
    }

    #[test]
    fn topic_lists_follow_quest_and_faction_inputs() {
        let buildings = [BuildingTopic {
            block_x: 0,
            block_y: 3,
            record_index: 2,
            name: "The Rusty Ogre".to_string(),
            building_type: 3,
        }];
        let quests = [QuestTopic {
            key: "Q0person".to_string(),
            kind: TopicKind::QuestPerson,
            available_for_dialog: true,
            has_entry_in_tell_me_about: true,
            has_entry_in_where_is: true,
            same_location: true,
            is_dialog_partner: false,
        }];
        let factions_a = [42u32, 40];
        let factions_b = [42u32, 41];
        let a = assemble_topics(&TalkContext {
            buildings: &buildings,
            quest_topics: &quests,
            info_factions: &factions_a,
        });
        let b = assemble_topics(&TalkContext {
            buildings: &buildings,
            quest_topics: &quests,
            info_factions: &factions_b,
        });
        assert!(!a.visible_lists_empty());
        assert_eq!(a.tell_me_about.len(), 5); // news + where + quest person + 2 orgs
        assert_eq!(a.person.len(), 1);
        assert_eq!(a.location.len(), 1);
        assert!(a.thing.is_empty());
        assert_ne!(a.digest(), b.digest());
        assert_eq!(a.digest(), assemble_topics(&TalkContext {
            buildings: &buildings,
            quest_topics: &quests,
            info_factions: &factions_a,
        }).digest());
    }
}

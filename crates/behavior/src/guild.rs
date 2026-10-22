//! M27 · Guilds — membership, rank, services, training, cure and donation
//! (SPEC-M27-FACTIONS §2 and §3; REQ-L99-060).
//!
//! Port of `Game/Guilds/Guild.cs` (rank tables and formulas `:37-39`,
//! `CalculateNewRank` `:95-111`, `UpdateRank` `:69-93`, eligibility
//! `:314-321`, `Join` `:304-308`, `CanAccessService` base `:260-283`,
//! training `:289-298`), `MagesGuild.cs:108-157`, `Temple.cs:132-141`,
//! `:413-424`, `DaggerfallGuildServiceTraining.cs:52-128`,
//! `DaggerfallGuildServiceCureDisease.cs:56-92` and
//! `DaggerfallGuildServiceDonation.cs:60-70` over `0038a1e6`.
//!
//! Declared boundaries (spec §6): skill values are set by M20
//! (`SkillSet::permanents()`); the regional price and haggling are M30
//! (`calculate_cost` stops before `ApplyRegionalPriceAdjustment`); the
//! effective cure of the disease is M32; persistence is M33 (here only the
//! `rank + lastRankChange` shape is declared).
//!
//! Pure: 0 Bevy, 0 I/O, 0 global RNG (rolls come in as a parameter or from a
//! seeded PRNG).

use std::collections::BTreeMap;

use crate::determinism::SplitMix64;

/// The ten ranks of the static table (`Guild.cs:37-39`).
pub const RANK_COUNT: usize = 10;
/// `rankReqReputation` (`Guild.cs:37`).
pub const RANK_REQ_REPUTATION: [i32; RANK_COUNT] = [0, 10, 20, 30, 40, 50, 60, 70, 80, 90];
/// `rankReqSkillHigh` (`Guild.cs:38`).
pub const RANK_REQ_SKILL_HIGH: [i32; RANK_COUNT] = [22, 23, 31, 39, 47, 55, 63, 71, 79, 87];
/// `rankReqSkillLow` (`Guild.cs:39`).
pub const RANK_REQ_SKILL_LOW: [i32; RANK_COUNT] = [4, 5, 9, 13, 17, 21, 25, 29, 33, 37];

/// Minimum cadence between rank changes (`Guild.cs:74`).
pub const PROMOTION_DAYS: i64 = 28;
/// Demotion message (`Guild.cs:30`).
pub const DEMOTION_MESSAGE_ID: i32 = 667;
/// Expulsion message (`Guild.cs:31`).
pub const EXPULSION_MESSAGE_ID: i32 = 668;

/// Default trainable skill cap (`Guild.cs:26`).
pub const DEFAULT_TRAINING_MAX: i16 = 50;
/// Cost per level as a member (`Guild.cs:27`).
pub const MEMBER_TRAINING_COST: i32 = 100;
/// Cost per level as a non-member (`Guild.cs:28`).
pub const NON_MEMBER_TRAINING_COST: i32 = 400;
/// Training window in classic minutes (`DaggerfallGuildServiceTraining.cs:54`).
pub const TRAINING_COOLDOWN_MINUTES: i64 = 720;
/// Hours consumed by training (`DaggerfallGuildServiceTraining.cs:123`).
pub const TRAINING_HOURS: i64 = 3;
/// Fatigue multiplier spent when training (`:124`).
pub const TRAINING_FATIGUE_MULTIPLIER: i32 = 180;
/// Lower component of the skill advance (`:126`): `Random.Range(10, 20 + 1)`.
pub const TRAINING_ADVANCE_MIN: i32 = 10;
/// Inclusive upper component of the skill advance (`:126`).
pub const TRAINING_ADVANCE_MAX: i32 = 21;

/// Base cost per disease (`DaggerfallGuildServiceCureDisease.cs:79`).
pub const CURE_BASE_COST: i32 = 250;
/// Blessing points applied by the temple (`FormulaHelper.cs:394-397`).
pub const TEMPLE_BLESSING_POINTS: i32 = 1;

/// `DaysPerYear` of `DaggerfallDateTime`.
pub const DAYS_PER_YEAR: i64 = 365;

/// Indices of `DFCareer.Skills` (`DFCareer.cs:448-487`), shared with the
/// `skills` organ (`Skill::index()`).
pub mod skill {
    /// Medical.
    pub const MEDICAL: usize = 0;
    /// Etiquette.
    pub const ETIQUETTE: usize = 1;
    /// Streetwise.
    pub const STREETWISE: usize = 2;
    /// Jumping.
    pub const JUMPING: usize = 3;
    /// Orcish.
    pub const ORCISH: usize = 4;
    /// Harpy.
    pub const HARPY: usize = 5;
    /// Giantish.
    pub const GIANTISH: usize = 6;
    /// Dragonish.
    pub const DRAGONISH: usize = 7;
    /// Nymph.
    pub const NYMPH: usize = 8;
    /// Daedric.
    pub const DAEDRIC: usize = 9;
    /// Spriggan.
    pub const SPRIGGAN: usize = 10;
    /// Centaurian.
    pub const CENTAURIAN: usize = 11;
    /// Impish.
    pub const IMPISH: usize = 12;
    /// Lockpicking.
    pub const LOCKPICKING: usize = 13;
    /// Mercantile.
    pub const MERCANTILE: usize = 14;
    /// Pickpocket.
    pub const PICKPOCKET: usize = 15;
    /// Stealth.
    pub const STEALTH: usize = 16;
    /// Swimming.
    pub const SWIMMING: usize = 17;
    /// Climbing.
    pub const CLIMBING: usize = 18;
    /// Backstabbing.
    pub const BACKSTABBING: usize = 19;
    /// Dodging.
    pub const DODGING: usize = 20;
    /// Running.
    pub const RUNNING: usize = 21;
    /// Destruction.
    pub const DESTRUCTION: usize = 22;
    /// Restoration.
    pub const RESTORATION: usize = 23;
    /// Illusion.
    pub const ILLUSION: usize = 24;
    /// Alteration.
    pub const ALTERATION: usize = 25;
    /// Thaumaturgy.
    pub const THAUMATURGY: usize = 26;
    /// Mysticism.
    pub const MYSTICISM: usize = 27;
    /// ShortBlade.
    pub const SHORT_BLADE: usize = 28;
    /// LongBlade.
    pub const LONG_BLADE: usize = 29;
    /// HandToHand.
    pub const HAND_TO_HAND: usize = 30;
    /// Axe.
    pub const AXE: usize = 31;
    /// BluntWeapon.
    pub const BLUNT_WEAPON: usize = 32;
    /// Archery.
    pub const ARCHERY: usize = 33;
    /// CriticalStrike.
    pub const CRITICAL_STRIKE: usize = 34;
}

/// Complete `FactionFile.GuildGroups` (`FactionFile.cs:568-596`); R-27-6
/// requires the values 22/23/24 that retail uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GuildGroup {
    /// `-1`.
    None,
    /// `0`.
    GGroup0,
    /// `1`.
    GGroup1,
    /// `2`.
    Oblivion,
    /// `3` — Dark Brotherhood.
    DarkBrotherHood,
    /// `4` — thieves guild with socialGroup Underworld.
    GeneralPopulace,
    /// `5`.
    Bards,
    /// `6`.
    TheFey,
    /// `7`.
    Prostitutes,
    /// `8`.
    GGroup8,
    /// `9` — knightly order.
    KnightlyOrder,
    /// `10` — mages guild.
    MagesGuild,
    /// `11` — fighters guild.
    FightersGuild,
    /// `12`.
    GGroup12,
    /// `13`.
    GGroup13,
    /// `14`.
    Necromancers,
    /// `15`.
    Region,
    /// `16`.
    GGroup16,
    /// `17` — holy order (temples).
    HolyOrder,
    /// `18`.
    GGroup18,
    /// `19`.
    GGroup19,
    /// `20`.
    GGroup20,
    /// `21`.
    GGroup21,
    /// `22`.
    Witches,
    /// `23`.
    Vampires,
    /// `24` — Orsinium.
    Orsinium,
}

const GUILD_GROUPS: [(GuildGroup, i32); 26] = [
    (GuildGroup::None, -1),
    (GuildGroup::GGroup0, 0),
    (GuildGroup::GGroup1, 1),
    (GuildGroup::Oblivion, 2),
    (GuildGroup::DarkBrotherHood, 3),
    (GuildGroup::GeneralPopulace, 4),
    (GuildGroup::Bards, 5),
    (GuildGroup::TheFey, 6),
    (GuildGroup::Prostitutes, 7),
    (GuildGroup::GGroup8, 8),
    (GuildGroup::KnightlyOrder, 9),
    (GuildGroup::MagesGuild, 10),
    (GuildGroup::FightersGuild, 11),
    (GuildGroup::GGroup12, 12),
    (GuildGroup::GGroup13, 13),
    (GuildGroup::Necromancers, 14),
    (GuildGroup::Region, 15),
    (GuildGroup::GGroup16, 16),
    (GuildGroup::HolyOrder, 17),
    (GuildGroup::GGroup18, 18),
    (GuildGroup::GGroup19, 19),
    (GuildGroup::GGroup20, 20),
    (GuildGroup::GGroup21, 21),
    (GuildGroup::Witches, 22),
    (GuildGroup::Vampires, 23),
    (GuildGroup::Orsinium, 24),
];

impl GuildGroup {
    /// Complete enum from the file integer.
    pub fn from_id(id: i32) -> Option<Self> {
        GUILD_GROUPS
            .iter()
            .find(|(_, value)| *value == id)
            .map(|(group, _)| *group)
    }

    /// File integer.
    pub fn id(self) -> i32 {
        GUILD_GROUPS
            .iter()
            .find(|(group, _)| *group == self)
            .map(|(_, value)| *value)
            .expect("GUILD_GROUPS covers every GuildGroup")
    }
}

/// Deities of `Temple.Divines` (value = factionId, `Temple.cs:61-84`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Deity {
    /// Akatosh (26).
    Akatosh,
    /// Arkay (21).
    Arkay,
    /// Dibella (29).
    Dibella,
    /// Julianos (27).
    Julianos,
    /// Kynareth (35).
    Kynareth,
    /// Mara (24).
    Mara,
    /// Stendarr (33).
    Stendarr,
    /// Zenithar (22).
    Zenithar,
}

impl Deity {
    /// The eight deities in the order of `Temple.cs:134-141`.
    pub const ALL: [Deity; 8] = [
        Deity::Akatosh,
        Deity::Arkay,
        Deity::Dibella,
        Deity::Julianos,
        Deity::Kynareth,
        Deity::Mara,
        Deity::Stendarr,
        Deity::Zenithar,
    ];

    /// `factionId` of the deity.
    pub fn faction_id(self) -> i32 {
        match self {
            Deity::Akatosh => 26,
            Deity::Arkay => 21,
            Deity::Dibella => 29,
            Deity::Julianos => 27,
            Deity::Kynareth => 35,
            Deity::Mara => 24,
            Deity::Stendarr => 33,
            Deity::Zenithar => 22,
        }
    }

    /// Index in [`Deity::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }

    /// `Temple.GetDivine(factionId)`.
    pub fn from_faction_id(id: i32) -> Option<Self> {
        Deity::ALL
            .into_iter()
            .find(|deity| deity.faction_id() == id)
    }

    /// Canonical name of the deity.
    pub fn name(self) -> &'static str {
        match self {
            Deity::Akatosh => "Akatosh",
            Deity::Arkay => "Arkay",
            Deity::Dibella => "Dibella",
            Deity::Julianos => "Julianos",
            Deity::Kynareth => "Kynareth",
            Deity::Mara => "Mara",
            Deity::Stendarr => "Stendarr",
            Deity::Zenithar => "Zenithar",
        }
    }
}

/// Row of `Temple.templeData` (`Temple.cs:132-141`): rank threshold per
/// service; `-1` = service disabled in that temple.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TempleTier {
    /// Library.
    pub library: i32,
    /// Cure (`FreeHealing`).
    pub healing: i32,
    /// Buy potions.
    pub buy_potions: i32,
    /// Make potions.
    pub make_potions: i32,
    /// Buy magic items.
    pub buy_magic: i32,
    /// Make magic items.
    pub make_magic: i32,
    /// Buy spells.
    pub buy_spells: i32,
    /// Make spells.
    pub make_spells: i32,
    /// Buy soul gems.
    pub soul_gems: i32,
    /// Daedra summoning.
    pub summoning: i32,
}

impl TempleTier {
    /// Threshold per service (`None` if the service uses no temple threshold).
    pub fn threshold(self, service: GuildService) -> Option<i32> {
        Some(match service {
            GuildService::BuyPotions => self.buy_potions,
            GuildService::MakePotions => self.make_potions,
            GuildService::BuyMagicItems => self.buy_magic,
            GuildService::MakeMagicItems => self.make_magic,
            GuildService::BuySpells => self.buy_spells,
            GuildService::MakeSpells => self.make_spells,
            GuildService::BuySoulgems => self.soul_gems,
            GuildService::DaedraSummoning => self.summoning,
            _ => return None,
        })
    }
}

/// `templeData` in [`Deity::ALL`] order (`Temple.cs:134-141`; Zenithar
/// corrected by the citation audit of 2026-10-21).
pub const TEMPLE_TIERS: [TempleTier; 8] = [
    TempleTier {
        library: 2,
        healing: 1,
        buy_potions: 4,
        make_potions: 5,
        buy_magic: -1,
        make_magic: -1,
        buy_spells: -1,
        make_spells: -1,
        soul_gems: -1,
        summoning: 7,
    },
    TempleTier {
        library: 3,
        healing: 0,
        buy_potions: 1,
        make_potions: 4,
        buy_magic: -1,
        make_magic: -1,
        buy_spells: -1,
        make_spells: -1,
        soul_gems: 4,
        summoning: 7,
    },
    TempleTier {
        library: 4,
        healing: 2,
        buy_potions: 1,
        make_potions: 5,
        buy_magic: -1,
        make_magic: -1,
        buy_spells: -1,
        make_spells: -1,
        soul_gems: -1,
        summoning: 7,
    },
    TempleTier {
        library: 0,
        healing: 2,
        buy_potions: -1,
        make_potions: -1,
        buy_magic: 3,
        make_magic: 5,
        buy_spells: -1,
        make_spells: -1,
        soul_gems: -1,
        summoning: 6,
    },
    TempleTier {
        library: 4,
        healing: 1,
        buy_potions: -1,
        make_potions: -1,
        buy_magic: -1,
        make_magic: -1,
        buy_spells: 3,
        make_spells: 6,
        soul_gems: -1,
        summoning: 7,
    },
    TempleTier {
        library: 4,
        healing: 1,
        buy_potions: 2,
        make_potions: 5,
        buy_magic: -1,
        make_magic: -1,
        buy_spells: -1,
        make_spells: -1,
        soul_gems: -1,
        summoning: 7,
    },
    TempleTier {
        library: 4,
        healing: 0,
        buy_potions: 2,
        make_potions: 5,
        buy_magic: -1,
        make_magic: -1,
        buy_spells: -1,
        make_spells: -1,
        soul_gems: -1,
        summoning: 7,
    },
    TempleTier {
        library: 4,
        healing: 1,
        buy_potions: 1,
        make_potions: 6,
        buy_magic: -1,
        make_magic: -1,
        buy_spells: -1,
        make_spells: -1,
        soul_gems: -1,
        summoning: 8,
    },
];

/// Temple row of the deity.
pub fn temple_tier(deity: Deity) -> TempleTier {
    TEMPLE_TIERS[deity.index()]
}

/// `GuildServices` in the order of `Services.cs:19-49`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GuildService {
    /// Training.
    Training,
    /// Quests.
    Quests,
    /// Repair.
    Repair,
    /// Identification.
    Identify,
    /// Donation.
    Donate,
    /// Cure disease.
    CureDisease,
    /// Buy potions.
    BuyPotions,
    /// Make potions.
    MakePotions,
    /// Buy spells.
    BuySpells,
    /// Buy spells (mages).
    BuySpellsMages,
    /// Make spells.
    MakeSpells,
    /// Buy magic items.
    BuyMagicItems,
    /// Make magic items.
    MakeMagicItems,
    /// Sell magic items.
    SellMagicItems,
    /// Teleport.
    Teleport,
    /// Daedra summoning.
    DaedraSummoning,
    /// Spymaster.
    Spymaster,
    /// Buy soul gems.
    BuySoulgems,
    /// Receive armor.
    ReceiveArmor,
    /// Receive house.
    ReceiveHouse,
}

/// The 20 services in source order.
pub const ALL_SERVICES: [GuildService; 20] = [
    GuildService::Training,
    GuildService::Quests,
    GuildService::Repair,
    GuildService::Identify,
    GuildService::Donate,
    GuildService::CureDisease,
    GuildService::BuyPotions,
    GuildService::MakePotions,
    GuildService::BuySpells,
    GuildService::BuySpellsMages,
    GuildService::MakeSpells,
    GuildService::BuyMagicItems,
    GuildService::MakeMagicItems,
    GuildService::SellMagicItems,
    GuildService::Teleport,
    GuildService::DaedraSummoning,
    GuildService::Spymaster,
    GuildService::BuySoulgems,
    GuildService::ReceiveArmor,
    GuildService::ReceiveHouse,
];

impl GuildService {
    /// Stable name for probe evidence.
    pub fn name(self) -> &'static str {
        match self {
            GuildService::Training => "Training",
            GuildService::Quests => "Quests",
            GuildService::Repair => "Repair",
            GuildService::Identify => "Identify",
            GuildService::Donate => "Donate",
            GuildService::CureDisease => "CureDisease",
            GuildService::BuyPotions => "BuyPotions",
            GuildService::MakePotions => "MakePotions",
            GuildService::BuySpells => "BuySpells",
            GuildService::BuySpellsMages => "BuySpellsMages",
            GuildService::MakeSpells => "MakeSpells",
            GuildService::BuyMagicItems => "BuyMagicItems",
            GuildService::MakeMagicItems => "MakeMagicItems",
            GuildService::SellMagicItems => "SellMagicItems",
            GuildService::Teleport => "Teleport",
            GuildService::DaedraSummoning => "DaedraSummoning",
            GuildService::Spymaster => "Spymaster",
            GuildService::BuySoulgems => "BuySoulgems",
            GuildService::ReceiveArmor => "ReceiveArmor",
            GuildService::ReceiveHouse => "ReceiveHouse",
        }
    }
}

/// Guild type (one membership per `GuildGroup`, `GuildManager.cs:105-112`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuildKind {
    /// Fighters Guild (faction 41).
    Fighters,
    /// Mages Guild (faction 40).
    Mages,
    /// Thieves Guild (faction 42).
    Thieves,
    /// Dark Brotherhood (faction 108).
    DarkBrotherhood,
    /// Temple of a deity.
    Temple(Deity),
    /// Generic knightly order (faction 844): **does not train**
    /// (`KnightlyOrder.cs:79-81`).
    KnightlyOrder,
}

/// Static definition of a guild.
#[derive(Debug, Clone, Copy)]
pub struct GuildDef {
    /// Kind.
    pub kind: GuildKind,
    /// Root faction of the guild.
    pub faction_id: i32,
    /// Guild group (membership key).
    pub group: GuildGroup,
    /// Skills that count for rank (`GuildSkills`).
    pub guild_skills: &'static [usize],
    /// Trainable skills (`TrainingSkills`); empty = does not train.
    pub training_skills: &'static [usize],
}

const FIGHTERS_SKILLS: [usize; 7] = [
    skill::ARCHERY,
    skill::AXE,
    skill::BLUNT_WEAPON,
    skill::GIANTISH,
    skill::LONG_BLADE,
    skill::ORCISH,
    skill::SHORT_BLADE,
];

const FIGHTERS_TRAINING: [usize; 11] = [
    skill::ARCHERY,
    skill::AXE,
    skill::BLUNT_WEAPON,
    skill::CRITICAL_STRIKE,
    skill::GIANTISH,
    skill::JUMPING,
    skill::LONG_BLADE,
    skill::ORCISH,
    skill::RUNNING,
    skill::SHORT_BLADE,
    skill::SWIMMING,
];

const MAGES_SKILLS: [usize; 6] = [
    skill::ALTERATION,
    skill::DESTRUCTION,
    skill::ILLUSION,
    skill::MYSTICISM,
    skill::RESTORATION,
    skill::THAUMATURGY,
];

const MAGES_TRAINING: [usize; 12] = [
    skill::ALTERATION,
    skill::DAEDRIC,
    skill::DESTRUCTION,
    skill::DRAGONISH,
    skill::HARPY,
    skill::ILLUSION,
    skill::IMPISH,
    skill::MYSTICISM,
    skill::ORCISH,
    skill::RESTORATION,
    skill::SPRIGGAN,
    skill::THAUMATURGY,
];

const THIEVES_SKILLS: [usize; 7] = [
    skill::BACKSTABBING,
    skill::CLIMBING,
    skill::LOCKPICKING,
    skill::PICKPOCKET,
    skill::SHORT_BLADE,
    skill::STEALTH,
    skill::STREETWISE,
];

const THIEVES_TRAINING: [usize; 11] = [
    skill::BACKSTABBING,
    skill::BLUNT_WEAPON,
    skill::CLIMBING,
    skill::DODGING,
    skill::JUMPING,
    skill::LOCKPICKING,
    skill::PICKPOCKET,
    skill::SHORT_BLADE,
    skill::STEALTH,
    skill::STREETWISE,
    skill::SWIMMING,
];

const DARK_BROTHERHOOD_SKILLS: [usize; 9] = [
    skill::ARCHERY,
    skill::BACKSTABBING,
    skill::CLIMBING,
    skill::CRITICAL_STRIKE,
    skill::DAEDRIC,
    skill::DESTRUCTION,
    skill::SHORT_BLADE,
    skill::STEALTH,
    skill::STREETWISE,
];

const DARK_BROTHERHOOD_TRAINING: [usize; 12] = [
    skill::ARCHERY,
    skill::BACKSTABBING,
    skill::CLIMBING,
    skill::CRITICAL_STRIKE,
    skill::DAEDRIC,
    skill::DESTRUCTION,
    skill::DODGING,
    skill::RUNNING,
    skill::SHORT_BLADE,
    skill::STEALTH,
    skill::STREETWISE,
    skill::SWIMMING,
];

const KNIGHTLY_ORDER_SKILLS: [usize; 7] = [
    skill::ARCHERY,
    skill::CRITICAL_STRIKE,
    skill::DRAGONISH,
    skill::ETIQUETTE,
    skill::GIANTISH,
    skill::LONG_BLADE,
    skill::MEDICAL,
];

const AKATOSH_SKILLS: [usize; 7] = [
    skill::ALTERATION,
    skill::DAEDRIC,
    skill::DESTRUCTION,
    skill::DRAGONISH,
    skill::LONG_BLADE,
    skill::RUNNING,
    skill::STEALTH,
];

const AKATOSH_TRAINING: [usize; 9] = [
    skill::ALTERATION,
    skill::ARCHERY,
    skill::DAEDRIC,
    skill::DESTRUCTION,
    skill::DRAGONISH,
    skill::LONG_BLADE,
    skill::RUNNING,
    skill::STEALTH,
    skill::SWIMMING,
];

const ARKAY_SKILLS: [usize; 7] = [
    skill::AXE,
    skill::BACKSTABBING,
    skill::DAEDRIC,
    skill::DESTRUCTION,
    skill::MEDICAL,
    skill::RESTORATION,
    skill::SHORT_BLADE,
];

const ARKAY_TRAINING: [usize; 9] = [
    skill::AXE,
    skill::BACKSTABBING,
    skill::CLIMBING,
    skill::CRITICAL_STRIKE,
    skill::DAEDRIC,
    skill::DESTRUCTION,
    skill::MEDICAL,
    skill::RESTORATION,
    skill::SHORT_BLADE,
];

const DIBELLA_SKILLS: [usize; 8] = [
    skill::DAEDRIC,
    skill::ETIQUETTE,
    skill::ILLUSION,
    skill::LOCKPICKING,
    skill::LONG_BLADE,
    skill::NYMPH,
    skill::ORCISH,
    skill::RESTORATION,
];

const DIBELLA_TRAINING: [usize; 10] = [
    skill::DAEDRIC,
    skill::ETIQUETTE,
    skill::HARPY,
    skill::ILLUSION,
    skill::LOCKPICKING,
    skill::LONG_BLADE,
    skill::NYMPH,
    skill::ORCISH,
    skill::RESTORATION,
    skill::STREETWISE,
];

const JULIANOS_SKILLS: [usize; 7] = [
    skill::ALTERATION,
    skill::DAEDRIC,
    skill::IMPISH,
    skill::LOCKPICKING,
    skill::MYSTICISM,
    skill::SHORT_BLADE,
    skill::THAUMATURGY,
];

const JULIANOS_TRAINING: [usize; 9] = [
    skill::ALTERATION,
    skill::CRITICAL_STRIKE,
    skill::DAEDRIC,
    skill::IMPISH,
    skill::LOCKPICKING,
    skill::MERCANTILE,
    skill::MYSTICISM,
    skill::SHORT_BLADE,
    skill::THAUMATURGY,
];

const KYNARETH_SKILLS: [usize; 11] = [
    skill::ARCHERY,
    skill::CLIMBING,
    skill::DAEDRIC,
    skill::DESTRUCTION,
    skill::DODGING,
    skill::DRAGONISH,
    skill::HARPY,
    skill::ILLUSION,
    skill::JUMPING,
    skill::RUNNING,
    skill::STEALTH,
];

const KYNARETH_TRAINING: [usize; 11] = KYNARETH_SKILLS;

const MARA_SKILLS: [usize; 10] = [
    skill::ARCHERY,
    skill::CRITICAL_STRIKE,
    skill::DAEDRIC,
    skill::ETIQUETTE,
    skill::HARPY,
    skill::ILLUSION,
    skill::MEDICAL,
    skill::NYMPH,
    skill::RESTORATION,
    skill::STREETWISE,
];

const MARA_TRAINING: [usize; 10] = MARA_SKILLS;

const STENDARR_SKILLS: [usize; 7] = [
    skill::AXE,
    skill::BLUNT_WEAPON,
    skill::CRITICAL_STRIKE,
    skill::DAEDRIC,
    skill::DODGING,
    skill::MEDICAL,
    skill::RESTORATION,
];

const STENDARR_TRAINING: [usize; 9] = [
    skill::AXE,
    skill::BLUNT_WEAPON,
    skill::CRITICAL_STRIKE,
    skill::DAEDRIC,
    skill::DODGING,
    skill::MEDICAL,
    skill::ORCISH,
    skill::RESTORATION,
    skill::SPRIGGAN,
];

const ZENITHAR_SKILLS: [usize; 12] = [
    skill::BLUNT_WEAPON,
    skill::CENTAURIAN,
    skill::DAEDRIC,
    skill::ETIQUETTE,
    skill::GIANTISH,
    skill::HARPY,
    skill::MERCANTILE,
    skill::ORCISH,
    skill::PICKPOCKET,
    skill::SPRIGGAN,
    skill::STREETWISE,
    skill::THAUMATURGY,
];

const ZENITHAR_TRAINING: [usize; 12] = ZENITHAR_SKILLS;

/// Skill rows per deity in [`Deity::ALL`] order.
const TEMPLE_SKILLS: [&[usize]; 8] = [
    &AKATOSH_SKILLS,
    &ARKAY_SKILLS,
    &DIBELLA_SKILLS,
    &JULIANOS_SKILLS,
    &KYNARETH_SKILLS,
    &MARA_SKILLS,
    &STENDARR_SKILLS,
    &ZENITHAR_SKILLS,
];

/// Training rows per deity in [`Deity::ALL`] order.
const TEMPLE_TRAINING: [&[usize]; 8] = [
    &AKATOSH_TRAINING,
    &ARKAY_TRAINING,
    &DIBELLA_TRAINING,
    &JULIANOS_TRAINING,
    &KYNARETH_TRAINING,
    &MARA_TRAINING,
    &STENDARR_TRAINING,
    &ZENITHAR_TRAINING,
];

/// Static definition of the guild/temple (factionId + `GuildGroup` + lists).
pub fn guild(kind: GuildKind) -> GuildDef {
    match kind {
        GuildKind::Fighters => GuildDef {
            kind,
            faction_id: 41,
            group: GuildGroup::FightersGuild,
            guild_skills: &FIGHTERS_SKILLS,
            training_skills: &FIGHTERS_TRAINING,
        },
        GuildKind::Mages => GuildDef {
            kind,
            faction_id: 40,
            group: GuildGroup::MagesGuild,
            guild_skills: &MAGES_SKILLS,
            training_skills: &MAGES_TRAINING,
        },
        GuildKind::Thieves => GuildDef {
            kind,
            faction_id: 42,
            group: GuildGroup::GeneralPopulace,
            guild_skills: &THIEVES_SKILLS,
            training_skills: &THIEVES_TRAINING,
        },
        GuildKind::DarkBrotherhood => GuildDef {
            kind,
            faction_id: 108,
            group: GuildGroup::DarkBrotherHood,
            guild_skills: &DARK_BROTHERHOOD_SKILLS,
            training_skills: &DARK_BROTHERHOOD_TRAINING,
        },
        GuildKind::Temple(deity) => GuildDef {
            kind,
            faction_id: deity.faction_id(),
            group: GuildGroup::HolyOrder,
            guild_skills: TEMPLE_SKILLS[deity.index()],
            training_skills: TEMPLE_TRAINING[deity.index()],
        },
        GuildKind::KnightlyOrder => GuildDef {
            kind,
            faction_id: 844,
            group: GuildGroup::KnightlyOrder,
            guild_skills: &KNIGHTLY_ORDER_SKILLS,
            training_skills: &[],
        },
    }
}

// ── Rank ────────────────────────────────────────────────────────────────────

/// `CalculateDaySinceZero` (`Guild.cs:128`): absolute calendar day.
pub fn day_since_zero(year: i64, day_of_year: i64) -> i64 {
    year * DAYS_PER_YEAR + day_of_year
}

/// `CalculateNumHighLowSkills` (`Guild.cs:112-126`) over the **permanent**
/// skill values (M20): `high` counts ≥ `rankReqSkillHigh[r]`, `low` counts
/// ≥ `rankReqSkillLow[r]` (disjoint branches: `else if`).
pub fn num_high_low(skills: &[i16], guild_skills: &[usize], rank: usize) -> (i32, i32) {
    let high_req = RANK_REQ_SKILL_HIGH[rank];
    let low_req = RANK_REQ_SKILL_LOW[rank];
    let mut high = 0;
    let mut low = 0;
    for &index in guild_skills {
        let value = i32::from(skills.get(index).copied().unwrap_or(0));
        if value >= high_req {
            high += 1;
        } else if value >= low_req {
            low += 1;
        }
    }
    (high, low)
}

/// `AllowGuildExpulsion` (`DarkBrotherhood.cs:129-133`): the dark guild
/// never expels.
fn allow_expulsion(def: &GuildDef, rank: i32) -> i32 {
    if def.kind == GuildKind::DarkBrotherhood && rank < 0 {
        0
    } else {
        rank
    }
}

/// `CalculateNewRank` (`Guild.cs:97-110`): walks the table and returns the
/// last satisfied rank; `-1` means expulsion.
pub fn calculate_new_rank(reputation: i32, skills: &[i16], def: &GuildDef) -> i32 {
    if reputation < 0 {
        return allow_expulsion(def, -1);
    }
    let mut rank = 0;
    while rank < RANK_COUNT {
        let (high, low) = num_high_low(skills, def.guild_skills, rank);
        if reputation < RANK_REQ_REPUTATION[rank] || high < 1 || low + high < 2 {
            break;
        }
        rank += 1;
    }
    allow_expulsion(def, rank as i32 - 1)
}

/// `IsEligibleToJoin` (`Guild.cs:314-321`): reputation ≥ 0, ≥1 high skill and
/// ≥2 skills between high+low for rank 0.
pub fn is_eligible(reputation: i32, skills: &[i16], def: &GuildDef) -> bool {
    let (high, low) = num_high_low(skills, def.guild_skills, 0);
    reputation >= RANK_REQ_REPUTATION[0] && high > 0 && low + high > 1
}

/// Rank change emitted by [`update_rank`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RankEvent {
    /// Promotion to the given rank.
    Promotion(i32),
    /// Demotion to the given rank.
    Demotion(i32),
    /// Expulsion (the caller removes the membership).
    Expulsion,
}

impl RankEvent {
    /// Associated retail message (667/668) or `None` on promotion.
    pub fn message_id(self) -> Option<i32> {
        match self {
            RankEvent::Promotion(_) => None,
            RankEvent::Demotion(_) => Some(DEMOTION_MESSAGE_ID),
            RankEvent::Expulsion => Some(EXPULSION_MESSAGE_ID),
        }
    }
}

/// `UpdateRank` (`Guild.cs:69-93`): only evaluates past 28 days and only
/// touches `last_rank_change` when the rank changes.
pub fn update_rank(
    day_now: i64,
    last_rank_change: i64,
    current_rank: i32,
    new_rank: i32,
) -> (i32, i64, Option<RankEvent>) {
    if day_now < last_rank_change + PROMOTION_DAYS || new_rank == current_rank {
        return (current_rank, last_rank_change, None);
    }
    let event = if new_rank > current_rank {
        RankEvent::Promotion(new_rank)
    } else if new_rank < 0 {
        RankEvent::Expulsion
    } else {
        RankEvent::Demotion(new_rank)
    };
    (new_rank, day_now, Some(event))
}

/// Player membership in a guild group (`GuildManager.cs:105-112`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuildMembership {
    /// Guild group (unique key).
    pub group: GuildGroup,
    /// Root faction.
    pub faction_id: i32,
    /// Rank; `0..=9` is a member, negative expelled (the membership is removed).
    pub rank: i32,
    /// `lastRankChange` in days since zero.
    pub last_rank_change: i64,
}

impl GuildMembership {
    /// `IsMember()` = `rank >= 0` (`Guild.cs:236`).
    pub fn is_member(&self) -> bool {
        self.rank >= 0
    }
}

/// Set of memberships (one per `GuildGroup`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GuildDirectory {
    memberships: BTreeMap<GuildGroup, GuildMembership>,
}

impl GuildDirectory {
    /// Empty directory.
    pub fn new() -> Self {
        Self::default()
    }

    /// `Join` (`Guild.cs:304-308`): rank 0 and day stamp.
    pub fn join(&mut self, def: &GuildDef, day: i64) -> GuildMembership {
        let membership = GuildMembership {
            group: def.group,
            faction_id: def.faction_id,
            rank: 0,
            last_rank_change: day,
        };
        self.memberships.insert(def.group, membership);
        membership
    }

    /// Membership of the group, if any.
    pub fn get(&self, group: GuildGroup) -> Option<&GuildMembership> {
        self.memberships.get(&group)
    }

    /// Rank in the group, if there is a membership.
    pub fn rank(&self, group: GuildGroup) -> Option<i32> {
        self.memberships.get(&group).map(|entry| entry.rank)
    }

    /// `RemoveMembership` (expulsion/leaving).
    pub fn remove(&mut self, group: GuildGroup) -> Option<GuildMembership> {
        self.memberships.remove(&group)
    }

    /// Number of memberships.
    pub fn len(&self) -> usize {
        self.memberships.len()
    }

    /// Whether the directory has no memberships.
    pub fn is_empty(&self) -> bool {
        self.memberships.is_empty()
    }

    /// Groups with membership, in stable order.
    pub fn groups(&self) -> impl Iterator<Item = GuildGroup> + '_ {
        self.memberships.keys().copied()
    }

    /// `Guild.UpdateRank`: computes the rank with the given reputation and
    /// skills, applies the 28-day cadence and removes the membership on
    /// expulsion.
    pub fn update(
        &mut self,
        def: &GuildDef,
        reputation: i32,
        skills: &[i16],
        day_now: i64,
    ) -> Option<RankEvent> {
        let membership = *self.memberships.get(&def.group)?;
        let new_rank = calculate_new_rank(reputation, skills, def);
        let (rank, last, event) = update_rank(
            day_now,
            membership.last_rank_change,
            membership.rank,
            new_rank,
        );
        match event {
            Some(RankEvent::Expulsion) => {
                self.memberships.remove(&def.group);
            }
            Some(_) => {
                self.memberships.insert(
                    def.group,
                    GuildMembership {
                        rank,
                        last_rank_change: last,
                        ..membership
                    },
                );
            }
            None => {}
        }
        event
    }
}

// ── Service access ──────────────────────────────────────────────────────────

/// Base `Guild.CanAccessService` (`Guild.cs:260-283`), used by Fighters.
fn base_access(rank: i32, service: GuildService) -> bool {
    match service {
        GuildService::Training | GuildService::Repair => rank >= 0,
        GuildService::Quests
        | GuildService::Identify
        | GuildService::BuySpellsMages
        | GuildService::Donate
        | GuildService::CureDisease => true,
        _ => false,
    }
}

/// `CanAccessService` per guild (overrides cited in the header).
pub fn can_access_service(def: &GuildDef, rank: i32, service: GuildService) -> bool {
    match def.kind {
        GuildKind::Fighters => base_access(rank, service),
        GuildKind::Mages => match service {
            GuildService::Training => rank >= 0,
            GuildService::Quests | GuildService::Identify | GuildService::BuySpellsMages => true,
            GuildService::MakeSpells => rank >= 0,
            GuildService::BuyMagicItems => rank >= 3,
            GuildService::MakeMagicItems => rank >= 5,
            GuildService::Teleport => rank >= 8,
            GuildService::DaedraSummoning => rank >= 6,
            GuildService::BuySoulgems => rank >= 4,
            _ => false,
        },
        GuildKind::Thieves => match service {
            GuildService::Training => rank >= 0,
            GuildService::Quests => true,
            GuildService::SellMagicItems => rank >= 2,
            GuildService::Spymaster => rank >= 4,
            _ => false,
        },
        GuildKind::DarkBrotherhood => match service {
            GuildService::Training => rank >= 0,
            GuildService::Quests => true,
            GuildService::BuyPotions => rank >= 1,
            GuildService::MakePotions => rank >= 3,
            GuildService::BuySoulgems => rank >= 5,
            GuildService::Spymaster => rank >= 7,
            _ => false,
        },
        GuildKind::Temple(deity) => {
            let tier = temple_tier(deity);
            match service {
                GuildService::Training
                | GuildService::Quests
                | GuildService::Donate
                | GuildService::CureDisease => true,
                GuildService::BuyPotions => tier.buy_potions <= rank,
                GuildService::MakePotions => tier.make_potions <= rank,
                GuildService::BuySpells => tier.buy_spells <= rank,
                GuildService::MakeSpells => tier.make_spells <= rank,
                GuildService::BuyMagicItems => tier.buy_magic <= rank,
                GuildService::MakeMagicItems => tier.make_magic <= rank,
                GuildService::DaedraSummoning => tier.summoning <= rank,
                GuildService::BuySoulgems => tier.soul_gems <= rank,
                _ => false,
            }
        }
        GuildKind::KnightlyOrder => match service {
            GuildService::Quests | GuildService::ReceiveHouse => true,
            GuildService::ReceiveArmor => rank >= 0,
            _ => false,
        },
    }
}

/// Services visible for the rank (those the menu offers).
pub fn services_visible(def: &GuildDef, rank: i32) -> Vec<GuildService> {
    ALL_SERVICES
        .into_iter()
        .filter(|service| can_access_service(def, rank, *service))
        .collect()
}

/// Library: only Mages (rank ≥ 2) and temples (`library <= rank`).
pub fn can_access_library(def: &GuildDef, rank: i32) -> bool {
    match def.kind {
        GuildKind::Mages => rank >= 2,
        GuildKind::Temple(deity) => temple_tier(deity).library <= rank,
        _ => false,
    }
}

/// `HallAccessAnytime`: Fighters/Mages rank ≥ 6; Thieves/DB any
/// member; temples and orders never (`GuildManager` opens by schedule).
pub fn hall_access_anytime(def: &GuildDef, rank: i32) -> bool {
    match def.kind {
        GuildKind::Fighters | GuildKind::Mages => rank >= 6,
        GuildKind::Thieves | GuildKind::DarkBrotherhood => rank >= 0,
        _ => false,
    }
}

/// `CanRest`: only the Fighter's Guild lets a member sleep in the hall.
pub fn can_rest(def: &GuildDef, rank: i32) -> bool {
    def.kind == GuildKind::Fighters && rank >= 0
}

/// `FreeHealing`: temple with the cure threshold reached (`Temple.cs:415-418`).
pub fn free_healing(def: &GuildDef, rank: i32) -> bool {
    match def.kind {
        GuildKind::Temple(deity) => temple_tier(deity).healing <= rank,
        _ => false,
    }
}

/// `ReducedCureCost` (`Temple.cs:420-426`): only Arkay discounts by rank.
pub fn reduced_cure_cost(def: &GuildDef, rank: i32, price: i32) -> i32 {
    match def.kind {
        GuildKind::Temple(Deity::Arkay) => (((10 - rank) << 8) / 10 * price) >> 8,
        _ => price,
    }
}

/// `FreeMagickaRecharge` (`MagesGuild.cs:116-122`): member with a career
/// without magicka regeneration.
pub fn free_magicka_recharge(def: &GuildDef, rank: i32, no_regen_spell_points: bool) -> bool {
    def.kind == GuildKind::Mages && rank >= 0 && no_regen_spell_points
}

/// Akatosh's `FastTravel` (`Temple.cs:430-438`): returns the remaining
/// hours after the rank discount.
pub fn fast_travel_hours(def: &GuildDef, rank: i32, duration: i32) -> i32 {
    match def.kind {
        GuildKind::Temple(Deity::Akatosh) => ((95 - rank) * duration) / 100,
        _ => duration,
    }
}

/// Kynareth's `DeepBreath` (`Temple.cs:440-449`): multiplies the duration.
pub fn deep_breath_seconds(def: &GuildDef, rank: i32, duration: i32) -> i32 {
    match def.kind {
        GuildKind::Temple(Deity::Kynareth) => ((10 + rank) * duration) / 10,
        _ => duration,
    }
}

/// Stendarr's `AvoidDeath` (`Temple.cs:452-462`): `roll` in `[0,50)`.
pub fn avoid_death(def: &GuildDef, rank: i32, roll: u32, submerged: bool) -> bool {
    matches!(def.kind, GuildKind::Temple(Deity::Stendarr)) && !submerged && roll < rank as u32
}

/// `AlterReward` of the Fighter's Guild (`FightersGuild.cs:93-96`).
pub fn alter_reward(rank: i32, reward: i32) -> i32 {
    ((((10 + rank) << 8) / 10).wrapping_mul(reward)) >> 8
}

/// `ReducedRepairCost` of the Fighter's Guild (`FightersGuild.cs:98-101`).
pub fn reduced_repair_cost(rank: i32, price: i32) -> i32 {
    ((((10 - rank) << 8) / 10).wrapping_mul(price)) >> 8
}

/// `FreeTavernRooms` of the order (`KnightlyOrder.cs:149-160`).
pub fn free_tavern_rooms(def: &GuildDef, rank: i32, same_region: bool) -> bool {
    def.kind == GuildKind::KnightlyOrder && (rank >= 4 || same_region)
}

/// `FreeShipTravel` of the order (`KnightlyOrder.cs:162-165`).
pub fn free_ship_travel(def: &GuildDef, rank: i32) -> bool {
    def.kind == GuildKind::KnightlyOrder && rank >= 6
}

/// Promotion message per rank: only DB/Thieves/Temple have a table
/// (`DarkBrotherhood.cs:104-127`, `MagesGuild.cs:84-101`).
pub fn promotion_message_id(def: &GuildDef, new_rank: i32) -> Option<i32> {
    match def.kind {
        GuildKind::DarkBrotherhood => Some(match new_rank {
            1 => 6611,
            3 => 6612,
            5 => 6613,
            7 => 6614,
            _ => 666,
        }),
        GuildKind::Mages => Some(match new_rank {
            2 => 5230,
            3 => 5231,
            6 => 5233,
            8 => 5234,
            _ => 5235,
        }),
        _ => None,
    }
}

// ── Service: training ───────────────────────────────────────────────────────

/// Quote of a training session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrainingQuote {
    /// Price to pay (level × cost per level).
    pub price: i32,
    /// Hours consumed (`RaiseTime(3 h)`).
    pub hours: i64,
    /// Fatigue spent (`DefaultFatigueLoss × 180`).
    pub fatigue: i32,
}

/// Training rejection reason (order of `TrainingSkill_OnItemPicked`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrainingReject {
    /// Fewer than 720 classic minutes have passed since the last session.
    TooSoon {
        /// Minutes elapsed since the last session.
        minutes_since: i64,
    },
    /// The skill is not in the guild list.
    NotTrainable {
        /// `DFCareer` index queried.
        skill: usize,
    },
    /// The skill exceeds the trainable cap.
    TooSkilled {
        /// Current permanent value.
        value: i16,
    },
}

/// `GetTrainingPrice` (`Guild.cs:288-292`): cost per level × hero level.
pub fn training_price(level: i32, is_member: bool) -> i32 {
    let per_level = if is_member {
        MEMBER_TRAINING_COST
    } else {
        NON_MEMBER_TRAINING_COST
    };
    per_level * level
}

/// `GetTrainingMax` (`Guild.cs:289-292`): default cap.
pub fn training_max() -> i16 {
    DEFAULT_TRAINING_MAX
}

/// Does the guild train that skill? (`TrainingSkills`; KO does not train).
pub fn trains_skill(def: &GuildDef, skill_index: usize) -> bool {
    def.training_skills.contains(&skill_index)
}

/// Checks of a training session, in source order:
/// 720 minutes → skill in the list → skill cap.
pub fn training_window(
    def: &GuildDef,
    rank: i32,
    skill_index: usize,
    skill_value: i16,
    level: i32,
    now_minutes: i64,
    last_minutes: i64,
) -> Result<TrainingQuote, TrainingReject> {
    let elapsed = now_minutes - last_minutes;
    if elapsed < TRAINING_COOLDOWN_MINUTES {
        return Err(TrainingReject::TooSoon {
            minutes_since: elapsed,
        });
    }
    if !trains_skill(def, skill_index) {
        return Err(TrainingReject::NotTrainable { skill: skill_index });
    }
    if skill_value > training_max() {
        return Err(TrainingReject::TooSkilled { value: skill_value });
    }
    Ok(TrainingQuote {
        price: training_price(level, rank >= 0),
        hours: TRAINING_HOURS,
        fatigue: TRAINING_FATIGUE_MULTIPLIER,
    })
}

/// Skill advance: `Random.Range(10, 21) × multiplier`; `draw` in `[0,11)`.
pub fn training_advance(multiplier: i16, draw: u32) -> i16 {
    let roll = TRAINING_ADVANCE_MIN + (i32::try_from(draw.min(10)).unwrap_or(0));
    (roll * i32::from(multiplier)) as i16
}

/// Same as [`training_advance`] taking the roll from a seeded PRNG.
pub fn training_advance_seeded(multiplier: i16, seed: u64) -> i16 {
    let mut rng = SplitMix64::new(seed);
    training_advance(multiplier, u32::try_from(rng.below(11)).unwrap_or(0))
}

// ── Service: cure disease ───────────────────────────────────────────────────

/// Holidays with an effect on the cure (`DLOCATION.Holidays`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Holiday {
    /// Normal day.
    None,
    /// `South_Winds_Prayer`: free cure.
    SouthWindsPrayer,
    /// `First_Harvest`: free cure.
    FirstHarvest,
    /// `Second_Harvest`: free cure.
    SecondHarvest,
    /// `North_Winds_Festival`: half price.
    NorthWindsFestival,
}

impl Holiday {
    /// Is the cure free that day?
    pub fn free_cure(self) -> bool {
        matches!(
            self,
            Holiday::SouthWindsPrayer | Holiday::FirstHarvest | Holiday::SecondHarvest
        )
    }

    /// Is the price halved?
    pub fn halves_cure(self) -> bool {
        self == Holiday::NorthWindsFestival
    }
}

/// `DiseaseCount` + the pending vampirism/lycanthropy
/// (`DaggerfallGuildServiceCureDisease.cs:61-64`).
pub fn disease_burden(diseases: i32, pending_vampire_or_werebeast: bool) -> i32 {
    diseases + i32::from(pending_vampire_or_werebeast)
}

/// `CalculateCost` (`FormulaHelper.cs:1868-1884`) without the override and
/// without the regional adjustment (M30): `cost = 2·(cost·(quality−10)/100 + cost)`.
pub fn calculate_cost(base_value: i32, shop_quality: i32) -> i32 {
    let mut cost = base_value;
    if cost < 1 {
        cost = 1;
    }
    let adjusted = cost * (shop_quality - 10) / 100 + cost;
    2 * adjusted
}

/// Full cure price (`CureDisease.cs:71-93`): base 250 × diseases,
/// Arkay discount, `CalculateCost`, half on North Winds; free on the
/// three holidays. The later haggling is M30.
pub fn cure_price(
    burden: i32,
    def: &GuildDef,
    rank: i32,
    shop_quality: i32,
    holiday: Holiday,
) -> i32 {
    if burden <= 0 || holiday.free_cure() {
        return 0;
    }
    let base = CURE_BASE_COST * burden;
    let base = reduced_cure_cost(def, rank, base);
    let mut cost = calculate_cost(base, shop_quality);
    if holiday.halves_cure() {
        cost /= 2;
    }
    cost
}

// ── Service: donation ───────────────────────────────────────────────────────

/// Result of a donation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DonationOutcome {
    /// Resulting reputation (without propagating).
    pub reputation: i32,
    /// Did the roll reach the blessing?
    pub applied: bool,
}

/// Blessing probability (`Donation.cs:66`):
/// `2·amount / max(|rep|,1) + 1` over `Dice100` (`roll` in `[0,100)`).
pub fn donation_chance(amount: i32, reputation: i32) -> i32 {
    2 * amount / reputation.abs().max(1) + 1
}

/// Donation: reputation rises by 1 if the roll succeeds and **does not
/// propagate** (`ChangeReputation(factionId, 1)` with `propagate` defaulting
/// to `false`).
pub fn donate(reputation: i32, amount: i32, roll: u32) -> DonationOutcome {
    let chance = donation_chance(amount, reputation);
    let applied = i32::try_from(roll.min(99)).unwrap_or(0) < chance;
    DonationOutcome {
        reputation: reputation + i32::from(applied),
        applied,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skills_all(value: i16) -> [i16; 35] {
        [value; 35]
    }

    fn fighters() -> GuildDef {
        guild(GuildKind::Fighters)
    }

    fn mages() -> GuildDef {
        guild(GuildKind::Mages)
    }

    fn arkay() -> GuildDef {
        guild(GuildKind::Temple(Deity::Arkay))
    }

    fn knights() -> GuildDef {
        guild(GuildKind::KnightlyOrder)
    }

    #[test]
    fn guild_groups_roundtrip_every_id() {
        for id in -1..=24 {
            let group = GuildGroup::from_id(id).expect("id 0..24 declared");
            assert_eq!(group.id(), id);
        }
        assert_eq!(GuildGroup::from_id(25), None);
        assert_eq!(GuildGroup::from_id(9), Some(GuildGroup::KnightlyOrder));
        assert_eq!(GuildGroup::from_id(22), Some(GuildGroup::Witches));
    }

    #[test]
    fn guild_definitions_match_retail_factions() {
        assert_eq!((fighters().faction_id, fighters().group), (41, GuildGroup::FightersGuild));
        assert_eq!((mages().faction_id, mages().group), (40, GuildGroup::MagesGuild));
        assert_eq!(guild(GuildKind::Thieves).group, GuildGroup::GeneralPopulace);
        assert_eq!(guild(GuildKind::DarkBrotherhood).faction_id, 108);
        let zenithar = guild(GuildKind::Temple(Deity::Zenithar));
        assert_eq!((zenithar.faction_id, zenithar.group), (22, GuildGroup::HolyOrder));
        assert_eq!(knights().faction_id, 844);
        assert_eq!(fighters().guild_skills.len(), 7);
        assert_eq!(fighters().training_skills.len(), 11);
        assert_eq!(mages().training_skills.len(), 12);
        assert_eq!(knights().training_skills.len(), 0);
    }

    #[test]
    fn num_high_low_is_disjoint_else_if() {
        // 40 exceeds low (4) and also high (22) at rank 0 ⇒ all high.
        assert_eq!(num_high_low(&skills_all(40), fighters().guild_skills, 0), (7, 0));
        // 10 only reaches low[0]=4 ⇒ 0 high, 7 low.
        assert_eq!(num_high_low(&skills_all(10), fighters().guild_skills, 0), (0, 7));
        // Rank 9: high 87 / low 37.
        assert_eq!(num_high_low(&skills_all(50), fighters().guild_skills, 9), (0, 7));
        assert_eq!(num_high_low(&skills_all(90), fighters().guild_skills, 9), (7, 0));
    }

    #[test]
    fn new_rank_walks_the_table() {
        // Reputation to spare and skills 50 ⇒ rank 4 (rank 5 requires 55).
        assert_eq!(calculate_new_rank(100, &skills_all(50), &fighters()), 4);
        // Skills 90 ⇒ maximum rank.
        assert_eq!(calculate_new_rank(100, &skills_all(90), &fighters()), 9);
        // Reputation 25 falls short at rank 3 (requires 30) ⇒ stays at 2.
        assert_eq!(calculate_new_rank(25, &skills_all(90), &fighters()), 2);
        // Negative reputation ⇒ expulsion.
        assert_eq!(calculate_new_rank(-1, &skills_all(90), &fighters()), -1);
        // No high skills and not two low ones ⇒ -1 from rank 0.
        assert_eq!(calculate_new_rank(50, &skills_all(0), &fighters()), -1);
    }

    #[test]
    fn dark_brotherhood_never_expels() {
        let db = guild(GuildKind::DarkBrotherhood);
        assert_eq!(calculate_new_rank(-50, &skills_all(0), &db), 0);
        assert_eq!(calculate_new_rank(50, &skills_all(0), &db), 0);
    }

    #[test]
    fn eligibility_needs_high_and_two_low() {
        let def = fighters();
        assert!(is_eligible(0, &skills_all(22), &def));
        // Without high skills it does not enter even with many low ones.
        assert!(!is_eligible(0, &skills_all(10), &def));
        assert!(!is_eligible(-1, &skills_all(90), &def));
    }

    #[test]
    fn update_rank_respects_28_days_and_keeps_stamp() {
        // 28 days have not passed yet ⇒ no change and no new stamp.
        assert_eq!(update_rank(127, 100, 0, 3), (0, 100, None));
        // Exactly 28 days ⇒ promotion and stamp to the current day.
        assert_eq!(
            update_rank(128, 100, 0, 3),
            (3, 128, Some(RankEvent::Promotion(3)))
        );
        // Same rank ⇒ nothing.
        assert_eq!(update_rank(200, 100, 4, 4), (4, 100, None));
        // Demotion and expulsion.
        assert_eq!(
            update_rank(200, 100, 4, 2),
            (2, 200, Some(RankEvent::Demotion(2)))
        );
        assert_eq!(
            update_rank(200, 100, 4, -1),
            (-1, 200, Some(RankEvent::Expulsion))
        );
        assert_eq!(RankEvent::Expulsion.message_id(), Some(EXPULSION_MESSAGE_ID));
        assert_eq!(RankEvent::Demotion(2).message_id(), Some(DEMOTION_MESSAGE_ID));
    }

    #[test]
    fn directory_join_and_expulsion() {
        let def = fighters();
        let mut directory = GuildDirectory::new();
        let joined = directory.join(&def, 10);
        assert_eq!((joined.rank, joined.last_rank_change), (0, 10));
        assert_eq!(directory.rank(GuildGroup::FightersGuild), Some(0));
        assert!(!directory.is_empty());
        // Expulsion: the membership disappears.
        let event = directory.update(&def, -1, &skills_all(90), 100);
        assert_eq!(event, Some(RankEvent::Expulsion));
        assert!(directory.is_empty());
        assert_eq!(directory.get(GuildGroup::FightersGuild), None);
        // Promotion after 28 days.
        directory.join(&def, 200);
        let event = directory.update(&def, 100, &skills_all(50), 228);
        assert_eq!(event, Some(RankEvent::Promotion(4)));
        assert_eq!(directory.rank(GuildGroup::FightersGuild), Some(4));
        assert_eq!(directory.groups().collect::<Vec<_>>(), vec![GuildGroup::FightersGuild]);
        assert_eq!(directory.len(), 1);
        assert!(directory.remove(GuildGroup::FightersGuild).is_some());
    }

    #[test]
    fn service_access_goldens_per_guild() {
        let fighters_def = fighters();
        assert!(can_access_service(&fighters_def, 0, GuildService::Training));
        assert!(can_access_service(&fighters_def, 0, GuildService::Repair));
        assert!(!can_access_service(&fighters_def, 0, GuildService::BuySpells));
        assert!(!can_access_service(&fighters_def, -1, GuildService::Training));

        let mages_def = mages();
        assert!(!can_access_service(&mages_def, 2, GuildService::BuyMagicItems));
        assert!(can_access_service(&mages_def, 3, GuildService::BuyMagicItems));
        assert!(!can_access_service(&mages_def, 7, GuildService::Teleport));
        assert!(can_access_service(&mages_def, 8, GuildService::Teleport));
        assert!(can_access_service(&mages_def, 6, GuildService::DaedraSummoning));
        assert!(can_access_service(&mages_def, 4, GuildService::BuySoulgems));
        assert!(can_access_service(&mages_def, 0, GuildService::BuySpellsMages));

        let thieves = guild(GuildKind::Thieves);
        assert!(!can_access_service(&thieves, 1, GuildService::SellMagicItems));
        assert!(can_access_service(&thieves, 2, GuildService::SellMagicItems));
        assert!(can_access_service(&thieves, 4, GuildService::Spymaster));

        let db = guild(GuildKind::DarkBrotherhood);
        assert!(can_access_service(&db, 1, GuildService::BuyPotions));
        assert!(can_access_service(&db, 3, GuildService::MakePotions));
        assert!(can_access_service(&db, 5, GuildService::BuySoulgems));
        assert!(can_access_service(&db, 7, GuildService::Spymaster));

        let arkay_def = arkay();
        assert!(can_access_service(&arkay_def, 0, GuildService::CureDisease));
        assert!(!can_access_service(&arkay_def, 0, GuildService::BuyPotions));
        assert!(can_access_service(&arkay_def, 1, GuildService::BuyPotions));
        assert!(can_access_service(&arkay_def, 4, GuildService::BuySoulgems));
        assert!(!can_access_service(&arkay_def, 6, GuildService::DaedraSummoning));
        assert!(can_access_service(&arkay_def, 7, GuildService::DaedraSummoning));

        assert!(can_access_service(&knights(), 0, GuildService::ReceiveArmor));
        assert!(can_access_service(&knights(), 0, GuildService::ReceiveHouse));
        assert!(!can_access_service(&knights(), 9, GuildService::Training));
    }

    #[test]
    fn visible_services_follow_access() {
        let visible = services_visible(&fighters(), 0);
        assert_eq!(
            visible,
            vec![
                GuildService::Training,
                GuildService::Quests,
                GuildService::Repair,
                GuildService::Identify,
                GuildService::Donate,
                GuildService::CureDisease,
                GuildService::BuySpellsMages,
            ]
        );
        assert!(services_visible(&knights(), 0).contains(&GuildService::ReceiveHouse));
    }

    #[test]
    fn library_and_hall_benefits() {
        assert!(!can_access_library(&mages(), 1));
        assert!(can_access_library(&mages(), 2));
        assert!(!can_access_library(&arkay(), 2));
        assert!(can_access_library(&arkay(), 3));
        assert!(hall_access_anytime(&mages(), 6));
        assert!(!hall_access_anytime(&mages(), 5));
        assert!(hall_access_anytime(&guild(GuildKind::Thieves), 0));
        assert!(!hall_access_anytime(&arkay(), 9));
        assert!(can_rest(&fighters(), 0));
        assert!(!can_rest(&mages(), 9));
    }

    #[test]
    fn temple_benefit_formulas() {
        assert!(free_healing(&arkay(), 0));
        let akatosh = guild(GuildKind::Temple(Deity::Akatosh));
        assert!(!free_healing(&akatosh, 0));
        assert!(free_healing(&akatosh, 1));
        assert!(!free_healing(&fighters(), 9));

        // Arkay rank 5 over 1000 ⇒ 500.
        assert_eq!(reduced_cure_cost(&arkay(), 5, 1000), 500);
        assert_eq!(reduced_cure_cost(&fighters(), 5, 1000), 1000);

        // Akatosh rank 5 over 100 h ⇒ 90 h.
        assert_eq!(fast_travel_hours(&akatosh, 5, 100), 90);
        assert_eq!(fast_travel_hours(&arkay(), 5, 100), 100);

        let kynareth = guild(GuildKind::Temple(Deity::Kynareth));
        assert_eq!(deep_breath_seconds(&kynareth, 6, 100), 160);
        assert_eq!(deep_breath_seconds(&arkay(), 6, 100), 100);

        let stendarr = guild(GuildKind::Temple(Deity::Stendarr));
        assert!(avoid_death(&stendarr, 5, 4, false));
        assert!(!avoid_death(&stendarr, 5, 5, false));
        assert!(!avoid_death(&stendarr, 5, 0, true));
        assert!(!avoid_death(&arkay(), 5, 0, false));

        assert!(free_magicka_recharge(&mages(), 0, true));
        assert!(!free_magicka_recharge(&mages(), 0, false));
        assert!(!free_magicka_recharge(&fighters(), 0, true));

        assert!(free_tavern_rooms(&knights(), 4, false));
        assert!(free_tavern_rooms(&knights(), 0, true));
        assert!(!free_tavern_rooms(&knights(), 0, false));
        assert!(free_ship_travel(&knights(), 6));
        assert!(!free_ship_travel(&knights(), 5));
    }

    #[test]
    fn fighter_reward_and_repair_scaling() {
        assert_eq!(alter_reward(5, 1000), 1500);
        assert_eq!(alter_reward(0, 1000), 1000);
        assert_eq!(reduced_repair_cost(5, 1000), 500);
        assert_eq!(reduced_repair_cost(0, 1000), 1000);
    }

    #[test]
    fn training_price_and_checks_in_source_order() {
        assert_eq!(training_price(5, true), 500);
        assert_eq!(training_price(5, false), 2000);
        assert_eq!(training_max(), 50);
        assert!(trains_skill(&fighters(), skill::LONG_BLADE));
        assert!(!trains_skill(&knights(), skill::LONG_BLADE));

        // 719 minutes ⇒ too soon (before any other check).
        assert_eq!(
            training_window(&fighters(), 0, skill::LONG_BLADE, 10, 1, 719, 0),
            Err(TrainingReject::TooSoon { minutes_since: 719 })
        );
        // Skill outside the guild list.
        assert_eq!(
            training_window(&fighters(), 0, skill::ILLUSION, 10, 1, 900, 0),
            Err(TrainingReject::NotTrainable {
                skill: skill::ILLUSION
            })
        );
        // Cap exceeded.
        assert_eq!(
            training_window(&fighters(), 0, skill::LONG_BLADE, 51, 1, 900, 0),
            Err(TrainingReject::TooSkilled { value: 51 })
        );
        // Valid session: member price, 3 h, fatigue 180.
        assert_eq!(
            training_window(&fighters(), 0, skill::LONG_BLADE, 50, 4, 720, 0),
            Ok(TrainingQuote {
                price: 400,
                hours: 3,
                fatigue: 180,
            })
        );
        // Without membership the price is four times as much.
        assert_eq!(
            training_window(&arkay(), -1, skill::RESTORATION, 10, 4, 720, 0)
                .map(|quote| quote.price),
            Ok(1600)
        );
    }

    #[test]
    fn training_advance_is_bounded_and_deterministic() {
        assert_eq!(training_advance(3, 0), 30);
        assert_eq!(training_advance(3, 10), 60);
        // Out of range saturates at the upper edge (20 inclusive).
        assert_eq!(training_advance(1, 99), 20);
        let a = training_advance_seeded(2, 20_262_121);
        let b = training_advance_seeded(2, 20_262_121);
        assert_eq!(a, b);
        assert!((20..=40).contains(&a));
    }

    #[test]
    fn cost_formula_matches_source_goldens() {
        // 250 with quality 20 ⇒ 2·(250·10/100 + 250) = 550.
        assert_eq!(calculate_cost(250, 20), 550);
        // Base 0 is raised to 1 before the adjustment.
        assert_eq!(calculate_cost(0, 20), 2);
        // Quality 10 leaves the cost at double the base.
        assert_eq!(calculate_cost(100, 10), 200);
        // Quality below 10 discounts (truncated integer division).
        assert_eq!(calculate_cost(100, 0), 180);
    }

    #[test]
    fn cure_price_applies_holiday_rank_and_quality() {
        assert_eq!(disease_burden(1, false), 1);
        assert_eq!(disease_burden(1, true), 2);
        let def = fighters();
        assert_eq!(cure_price(0, &def, 0, 20, Holiday::None), 0);
        assert_eq!(cure_price(1, &def, 0, 20, Holiday::None), 550);
        assert_eq!(cure_price(1, &def, 0, 20, Holiday::NorthWindsFestival), 275);
        for holiday in [
            Holiday::SouthWindsPrayer,
            Holiday::FirstHarvest,
            Holiday::SecondHarvest,
        ] {
            assert!(holiday.free_cure());
            assert_eq!(cure_price(2, &def, 0, 20, holiday), 0);
        }
        // Arkay rank 5: 250 ⇒ 125 ⇒ 274.
        assert_eq!(cure_price(1, &arkay(), 5, 20, Holiday::None), 274);
    }

    #[test]
    fn donation_chance_and_blessing() {
        // 2·1000/max(0,1)+1 = 2001 ⇒ always succeeds.
        assert_eq!(donation_chance(1000, 0), 2001);
        assert_eq!(donate(0, 1000, 99).reputation, 1);
        // 2·1/100+1 = 1 ⇒ only succeeds on roll 0.
        assert_eq!(donation_chance(1, 100), 1);
        assert_eq!(
            donate(100, 1, 0),
            DonationOutcome {
                reputation: 101,
                applied: true
            }
        );
        assert_eq!(
            donate(100, 1, 1),
            DonationOutcome {
                reputation: 100,
                applied: false
            }
        );
        // Negative reputation: the absolute value is used.
        assert_eq!(donation_chance(1, -100), 1);
        assert_eq!(donate(-100, 1, 0).reputation, -99);
    }

    #[test]
    fn day_since_zero_uses_365() {
        assert_eq!(day_since_zero(0, 1), 1);
        assert_eq!(day_since_zero(3, 0), 3 * DAYS_PER_YEAR);
        assert_eq!(day_since_zero(405, 1), 405 * 365 + 1);
    }

    #[test]
    fn promotion_messages_follow_retail_tables() {
        let db = guild(GuildKind::DarkBrotherhood);
        assert_eq!(promotion_message_id(&db, 1), Some(6611));
        assert_eq!(promotion_message_id(&db, 7), Some(6614));
        assert_eq!(promotion_message_id(&db, 9), Some(666));
        assert_eq!(promotion_message_id(&mages(), 2), Some(5230));
        assert_eq!(promotion_message_id(&mages(), 8), Some(5234));
        assert_eq!(promotion_message_id(&mages(), 0), Some(5235));
        assert_eq!(promotion_message_id(&fighters(), 3), None);
    }

    #[test]
    fn temple_tiers_match_source_rows() {
        assert_eq!(temple_tier(Deity::Akatosh).library, 2);
        assert_eq!(temple_tier(Deity::Arkay).healing, 0);
        assert_eq!(temple_tier(Deity::Julianos).buy_magic, 3);
        assert_eq!(temple_tier(Deity::Kynareth).make_spells, 6);
        assert_eq!(temple_tier(Deity::Zenithar).summoning, 8);
        assert_eq!(temple_tier(Deity::Zenithar).make_potions, 6);
        assert_eq!(temple_tier(Deity::Mara).soul_gems, -1);
        assert_eq!(temple_tier(Deity::Arkay).threshold(GuildService::BuySoulgems), Some(4));
        assert_eq!(temple_tier(Deity::Arkay).threshold(GuildService::Training), None);
    }
}

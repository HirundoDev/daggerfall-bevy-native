//! M28 · Crime, law and courts — the 15 crimes, the legal reputation of a
//! region, the court table and the prison clock (SPEC-M28-CRIME-LAW;
//! REQ-L99-061).
//!
//! Port of `Game/Entities/PlayerEntity.cs` over `0038a1e6` — `Crimes`
//! `:2263-2281`, `reputationLossPerCrime` `:2284`, `LowerRepForCrime`
//! `:2291-2299`, `RaiseReputationForDoingSentence` `:2301-2311`,
//! `SurrenderToCityGuards` `:2313-2332`, `CourtWindow` `:2334-2344`,
//! `SetCrimeCommitted` `:2346-2355`, `SpawnCityGuards` `:621-745`,
//! `ClampLegalReputations` `:2247-2256` — and of
//! `Game/UserInterfaceWindows/DaggerfallCourtWindow.cs` — tables `:57-60`,
//! sentence `:115-173`, guild rescue `:176-218`, states `:231-353`,
//! debate/lie `:369-427`, `OnPop` `:431-449`, prison `:464-479`, release
//! `:481-490` — plus `Game/LoanChecker.cs:53-70` and
//! `Game/Questing/Actions/SetPlayerCrime.cs:28,46-60`.
//!
//! Declared boundaries (spec §4): guard entities and their AI are M26 — this
//! module only decides the [`GuardSpawnRequest`] and consumes the guard hit;
//! reputation propagation is M27 ([`FactionGraph::change_reputation`]); loans
//! and banking are M30 — [`Crimes::LoanDefault`] arrives here as a crime;
//! disease during prison is M32; the `setplayercrime` verb itself is M29 — here
//! live the crime state and its enum validation; persistence is M33; court
//! text is M16 and the 1996 window is M17/M34.
//!
//! Pure: 0 Bevy, 0 I/O, 0 global RNG. Rolls enter as a seeded [`DfRandom`]
//! (the engine LCG, `DFU:API/DFRandom.cs:52-56`); the source mixes Unity
//! `Random` in `Dice100` and `Random.Range`, which the spec (R-28-3) replaces
//! by the engine PRNG so that the sentence is reproducible.

pub use crate::determinism::DfRandom;

use crate::determinism::fnv1a64;
use crate::factions::FactionGraph;

// ── crimes ─────────────────────────────────────────────────────────────────

/// Number of `Crimes` values, `None` included (`PlayerEntity.cs:2263-2281`).
pub const CRIME_KINDS: usize = 16;
/// Rows of the court table: one per crime that can reach a court
/// (`crimeType = (int)crime - 1`, `DaggerfallCourtWindow.cs:109-113`).
pub const CRIME_TABLE_LEN: usize = 15;

/// `Crimes` (`PlayerEntity.cs:2263-2281`), `u8` 0..=15.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Crimes {
    /// `None = 0` — no crime; the court closes before reading the table.
    None = 0,
    /// `Attempted_Breaking_And_Entering = 1`.
    AttemptedBreakingAndEntering = 1,
    /// `Trespassing = 2`.
    Trespassing = 2,
    /// `Breaking_And_Entering = 3`.
    BreakingAndEntering = 3,
    /// `Assault = 4`.
    Assault = 4,
    /// `Murder = 5`.
    Murder = 5,
    /// `Tax_Evasion = 6`.
    TaxEvasion = 6,
    /// `Criminal_Conspiracy = 7`.
    CriminalConspiracy = 7,
    /// `Vagrancy = 8`.
    Vagrancy = 8,
    /// `Smuggling = 9`.
    Smuggling = 9,
    /// `Piracy = 10`.
    Piracy = 10,
    /// `High_Treason = 11`.
    HighTreason = 11,
    /// `Pickpocketing = 12`.
    Pickpocketing = 12,
    /// `Theft = 13`.
    Theft = 13,
    /// `Treason = 14` (classic has no value of its own: half of High Treason).
    Treason = 14,
    /// `LoanDefault = 15` (column added by DFU, `:2281`).
    LoanDefault = 15,
}

impl Crimes {
    /// Every crime in enum order (`0..=15`).
    pub const ALL: [Self; CRIME_KINDS] = [
        Self::None,
        Self::AttemptedBreakingAndEntering,
        Self::Trespassing,
        Self::BreakingAndEntering,
        Self::Assault,
        Self::Murder,
        Self::TaxEvasion,
        Self::CriminalConspiracy,
        Self::Vagrancy,
        Self::Smuggling,
        Self::Piracy,
        Self::HighTreason,
        Self::Pickpocketing,
        Self::Theft,
        Self::Treason,
        Self::LoanDefault,
    ];

    /// Numeric value (`(int)Crimes`).
    pub const fn value(self) -> u8 {
        self as u8
    }

    /// Rebuilds a crime from its numeric value; `None` outside `0..=15`.
    pub const fn from_value(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::AttemptedBreakingAndEntering),
            2 => Some(Self::Trespassing),
            3 => Some(Self::BreakingAndEntering),
            4 => Some(Self::Assault),
            5 => Some(Self::Murder),
            6 => Some(Self::TaxEvasion),
            7 => Some(Self::CriminalConspiracy),
            8 => Some(Self::Vagrancy),
            9 => Some(Self::Smuggling),
            10 => Some(Self::Piracy),
            11 => Some(Self::HighTreason),
            12 => Some(Self::Pickpocketing),
            13 => Some(Self::Theft),
            14 => Some(Self::Treason),
            15 => Some(Self::LoanDefault),
            _ => None,
        }
    }

    /// Rebuilds a crime from its enum name, case-sensitive, as the quest verb
    /// `setplayercrime <crime>` does (`SetPlayerCrime.cs:28,46-51`); unknown
    /// names return `None` so the caller raises the source exception.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|crime| crime.name() == name)
    }

    /// Enum member name in the source.
    pub const fn name(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::AttemptedBreakingAndEntering => "Attempted_Breaking_And_Entering",
            Self::Trespassing => "Trespassing",
            Self::BreakingAndEntering => "Breaking_And_Entering",
            Self::Assault => "Assault",
            Self::Murder => "Murder",
            Self::TaxEvasion => "Tax_Evasion",
            Self::CriminalConspiracy => "Criminal_Conspiracy",
            Self::Vagrancy => "Vagrancy",
            Self::Smuggling => "Smuggling",
            Self::Piracy => "Piracy",
            Self::HighTreason => "High_Treason",
            Self::Pickpocketing => "Pickpocketing",
            Self::Theft => "Theft",
            Self::Treason => "Treason",
            Self::LoanDefault => "LoanDefault",
        }
    }

    /// `reputationLossPerCrime[(int)crime]` (`PlayerEntity.cs:2284`).
    pub const fn reputation_loss(self) -> i16 {
        REPUTATION_LOSS_PER_CRIME[self.value() as usize]
    }

    /// Court table index (`(int)crime - 1`); `None` for [`Crimes::None`],
    /// which closes the court before reading the table (`:109-113`).
    pub const fn court_index(self) -> Option<usize> {
        match self {
            Self::None => None,
            other => Some(other.value() as usize - 1),
        }
    }
}

/// `reputationLossPerCrime` (`PlayerEntity.cs:2284`), hex `0x00,0x0A,0x05,
/// 0x0A,0x08,0x14,0x0A,0x02,0x01,0x02,0x02,0x4B,0x02,0x08,0x24,0x0A`.
pub const REPUTATION_LOSS_PER_CRIME: [i16; CRIME_KINDS] =
    [0, 10, 5, 10, 8, 20, 10, 2, 1, 2, 2, 75, 2, 8, 36, 10];

// ── court tables ───────────────────────────────────────────────────────────

/// `PenaltyPerLegalRepPoint` (`DaggerfallCourtWindow.cs:59`), FALL.EXE offset
/// `0x1B34E0`.
pub const PENALTY_PER_LEGAL_REP: [i32; CRIME_TABLE_LEN] =
    [5, 5, 6, 6, 10, 5, 5, 3, 8, 8, 9, 6, 0, 8, 0];
/// `BasePenaltyAmounts` (`:60`).
pub const BASE_PENALTY_AMOUNTS: [i32; CRIME_TABLE_LEN] =
    [300, 200, 600, 1000, 10000, 200, 500, 100, 500, 500, 1200, 200, 200, 1000, 100];
/// `MinimumPenaltyAmounts` (`:61`). With the retail table the lower clamp is
/// unreachable (see [`penalty_amount`]): it is kept because the source keeps it.
pub const MINIMUM_PENALTY_AMOUNTS: [i32; CRIME_TABLE_LEN] =
    [50, 10, 80, 10, 9000, 10, 10, 2, 10, 10, 160, 5, 5, 10, 4];
/// `MaximumPenaltyAmounts` (`:62`).
pub const MAXIMUM_PENALTY_AMOUNTS: [i32; CRIME_TABLE_LEN] =
    [1000, 800, 1200, 1500, 12000, 12000, 1500, 700, 1500, 1500, 2000, 1000, 1000, 1500, 700];

/// Regional legal reputation bounds (`ClampLegalReputations`,
/// `PlayerEntity.cs:2247-2256`); the faction reputation has its own clamp in
/// M27 (`factions::MIN_REPUTATION/MAX_REPUTATION`) and is not unified (R-28-4).
pub const MIN_LEGAL_REP: i16 = -100;
/// Upper bound of the regional legal reputation (`:2249-2253`).
pub const MAX_LEGAL_REP: i16 = 100;
/// `FindFactionByTypeAndRegion(15, …)` — the People faction of a region
/// (`:2295-2296`).
pub const PEOPLE_FACTION_TYPE: i32 = 15;

/// `NotFoundException` default of `punishmentType` is `0`: banishment or
/// execution (`DaggerfallCourtWindow.cs:122-138`).
pub const PUNISHMENT_SEVERE: i32 = 0;
/// Execution; the source never assigns it (state 5 is unreachable, `:278`).
pub const PUNISHMENT_EXECUTION: i32 = 1;
/// Fine and/or prison (`:136-138`).
pub const PUNISHMENT_FINE_OR_PRISON: i32 = 2;
/// Cap of both severity thresholds (`:124-134`).
pub const PUNISHMENT_THRESHOLD_CAP: i32 = 75;
/// `SeverePunishmentFlags |= 1` — banished from the region (`:262-277`).
pub const SEVERE_PUNISHMENT_BANISHED: u8 = 1;
/// `SeverePunishmentFlags |= 2` — executed (`:279-290`).
pub const SEVERE_PUNISHMENT_EXECUTED: u8 = 2;
/// Gold added to the fine per split iteration (`:158`).
pub const FINE_STEP: i32 = 40;
/// Prison days added per split iteration (`:160`).
pub const PRISON_DAYS_STEP: i32 = 3;
/// `daysInPrison * 1440 * 60` (`:474`).
pub const SECONDS_PER_DAY: i64 = 1440 * 60;
/// `RaiseTime(240 * 60)` on release (`:484`).
pub const RELEASE_SECONDS: i64 = 240 * 60;
/// `ReleaseFromPrison` requests `GameManager.ClearEnemies()` (`:488`); the
/// enemy registry is M26/engine, so M28 only declares the request.
pub const RELEASE_CLEARS_ENEMIES: bool = true;
/// `prisonUpdateInterval` measured from a video (`:54`).
pub const PRISON_UI_INTERVAL: f32 = 0.3;
/// Accelerated countdown (`:300-301`).
pub const PRISON_UI_FAST_INTERVAL: f32 = 0.001;
/// Lower clamp of `chanceToGoFree` (`:392-395`).
pub const MIN_CHANCE_TO_GO_FREE: i32 = 5;
/// Upper clamp of `chanceToGoFree` (`:390-391`).
pub const MAX_CHANCE_TO_GO_FREE: i32 = 95;
/// `roll < 25 ⇒ fine *= 2` (`:403-405`).
pub const PLEA_ROLL_DOUBLE_BELOW: i32 = 25;
/// `roll > 75 ⇒ fine >>= 1` (`:406-407`).
pub const PLEA_ROLL_HALVE_ABOVE: i32 = 75;

// ── guard response (M28 asks, M26 owns the guards) ─────────────────────────

/// `maxActiveGuardSpawns = 5` (`PlayerEntity.cs:623`); the spawn only happens
/// with **at most** five active city guards (`:626`).
pub const MAX_ACTIVE_GUARD_SPAWNS: usize = 5;
/// `UnityEngine.Random.Range(2, 6)` indoors (`:635`): guards in `2..=5`.
pub const INTERIOR_GUARD_MIN: usize = 2;
/// Inclusive upper bound of [`INTERIOR_GUARD_MIN`].
pub const INTERIOR_GUARD_MAX: usize = 5;
/// `radius + 0.1f` from the lowest outer door (`:633-634`).
pub const INTERIOR_DOOR_OFFSET_METERS: f32 = 0.1;
/// Immediate radius around the player (`:665`).
pub const GUARD_ATTENTION_RADIUS_METERS: f32 = 77.5;
/// `Vector3.Angle(...) >= 105.469` for replacing a non-guard NPC (`:674`).
pub const GUARD_REPLACE_MIN_ANGLE_DEGREES: f32 = 105.469;
/// `UnityEngine.Random.Range(0, 4) == 0` — one in four (`:675`).
pub const GUARD_REPLACE_ONE_IN: u32 = 4;
/// `CreateFoeSpawner(..., Random.Range(2, 5 + 1), 12.8f, 51.2f)` fallback
/// (`:687`): guards in `2..=5`.
pub const FOE_SPAWNER_GUARD_MIN: usize = 2;
/// Inclusive upper bound of [`FOE_SPAWNER_GUARD_MIN`].
pub const FOE_SPAWNER_GUARD_MAX: usize = 5;
/// Fallback spawner inner radius (`:687`).
pub const FOE_SPAWNER_MIN_RADIUS_METERS: f32 = 12.8;
/// Fallback spawner outer radius (`:687`).
pub const FOE_SPAWNER_MAX_RADIUS_METERS: f32 = 51.2;
/// Non-immediate vision cone (`:700`).
pub const GUARD_VISION_MAX_ANGLE_DEGREES: f32 = 95.0;
/// `Random.Range(5, 10 + 1)` seconds (`:739`): countdown in `5..=10`.
pub const GUARD_COUNTDOWN_MIN_SECONDS: u32 = 5;
/// Inclusive upper bound of [`GUARD_COUNTDOWN_MIN_SECONDS`].
pub const GUARD_COUNTDOWN_MAX_SECONDS: u32 = 10;
/// `enemyMotor.GiveUpTimer *= 3` for a spawned guard (`:760`).
pub const GUARD_GIVE_UP_MULTIPLIER: i32 = 3;
/// The countdown is bound to the current location (`:740-741`).
pub const GUARD_COUNTDOWN_BOUND_TO_LOCATION: bool = true;

// ── hourly crime and loan ──────────────────────────────────────────────────

/// `LegalRep < -10` raises `Criminal_Conspiracy` (`PlayerEntity.cs:498-504`).
pub const CRIMINAL_CONSPIRACY_LEGAL_REP_BELOW: i16 = -10;
/// Hourly `Dice100.SuccessRoll(5)` for conspiracy by legal reputation (`:502`).
pub const CRIMINAL_CONSPIRACY_CHANCE: i32 = 5;
/// Hourly `Dice100.SuccessRoll(10)` in a region with `SeverePunishmentFlags & 1`
/// (`:506-511`).
pub const BANISHED_CONSPIRACY_CHANCE: i32 = 10;
/// Loan default never spawns guards (`LoanChecker.cs:53-70` only lowers rep).
pub const LOAN_DEFAULT_SPAWNS_GUARDS: bool = false;

// ── dice helpers (engine PRNG, R-28-3) ─────────────────────────────────────

/// `Dice100.Roll()` — `Random.Range(1, 101)` (`Dice100.cs:15-18`) as the
/// engine PRNG: `rand() % 100 + 1`.
fn roll_1_100(rng: &mut DfRandom) -> i32 {
    (rng.rand() % 100) as i32 + 1
}

/// `Dice100.SuccessRoll(c)` — `Range(0, 100) < c` (`Dice100.cs:20-23`).
fn success_roll(chance: i32, rng: &mut DfRandom) -> bool {
    ((rng.rand() % 100) as i32) < chance
}

/// `Dice100.FailedRoll(c)` — `Range(0, 100) >= c` (`Dice100.cs:25-28`).
fn failed_roll(chance: i32, rng: &mut DfRandom) -> bool {
    ((rng.rand() % 100) as i32) >= chance
}

/// Uniform integer in `min..=max` over the engine LCG.
fn draw_inclusive(rng: &mut DfRandom, min: i32, max: i32) -> i32 {
    let span = (max - min + 1).max(1) as u32;
    min + (rng.rand() % span) as i32
}

// ── regional legal state ───────────────────────────────────────────────────

/// One region's legal record: `RegionData[].LegalRep` and
/// `RegionData[].SeverePunishmentFlags` (`PlayerEntity.cs:1580`, `:262-290`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RegionLaw {
    /// `short LegalRep` (`:1580`), kept inside [`MIN_LEGAL_REP`]..=[`MAX_LEGAL_REP`].
    pub legal_rep: i16,
    /// `SeverePunishmentFlags`: bit 1 banished, bit 2 executed (`:263`, `:280`).
    pub severe_punishment_flags: u8,
}

impl RegionLaw {
    /// A region with neutral reputation and no punishment flags.
    pub const fn new() -> Self {
        Self {
            legal_rep: 0,
            severe_punishment_flags: 0,
        }
    }

    /// `ClampLegalReputations` (`:2247-2256`).
    pub fn set_legal_rep(&mut self, value: i16) -> i16 {
        self.legal_rep = value.clamp(MIN_LEGAL_REP, MAX_LEGAL_REP);
        self.legal_rep
    }

    /// Applies a delta and returns the clamped result.
    pub fn apply_legal_rep(&mut self, delta: i16) -> i16 {
        self.set_legal_rep(self.legal_rep.saturating_add(delta))
    }

    /// `SeverePunishmentFlags & 1` (`:263`).
    pub const fn banished(&self) -> bool {
        self.severe_punishment_flags & SEVERE_PUNISHMENT_BANISHED != 0
    }

    /// `SeverePunishmentFlags & 2` (`:280`).
    pub const fn executed(&self) -> bool {
        self.severe_punishment_flags & SEVERE_PUNISHMENT_EXECUTED != 0
    }
}

/// Result of a reputation change: the regional value after clamping and the
/// M27 faction that received the propagated change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReputationDelta {
    /// `RegionData[region].LegalRep` after the change.
    pub legal_rep_after: i16,
    /// People faction of the region, if the graph declares one.
    pub people_faction: Option<i32>,
    /// Amount passed to `FactionGraph::change_reputation` (propagated).
    pub people_delta: i32,
}

/// Player-level law state: the crime and the per-region legal records
/// (`PlayerEntity.cs:101,1580`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerLaw {
    regions: Vec<RegionLaw>,
    /// Field `crimeCommitted` (`:101`), property `CrimeCommitted` (`:189`).
    pub crime: Crimes,
    /// `Arrested` (`:2338`).
    pub arrested: bool,
    /// `InPrison` (`:253-261`).
    pub in_prison: bool,
    /// `halfOfLegalRepPlayerLostFromCrime` set by `CourtWindow` (`:2342`).
    pub half_of_legal_rep_lost_from_crime: i16,
}

impl PlayerLaw {
    /// A player with `regions` neutral legal records and no crime.
    pub fn new(regions: usize) -> Self {
        Self {
            regions: vec![RegionLaw::new(); regions],
            crime: Crimes::None,
            arrested: false,
            in_prison: false,
            half_of_legal_rep_lost_from_crime: 0,
        }
    }

    /// All regional records (62 in the retail game).
    pub fn regions(&self) -> &[RegionLaw] {
        &self.regions
    }

    /// One regional record.
    pub fn region(&self, index: usize) -> Option<&RegionLaw> {
        self.regions.get(index)
    }

    /// Mutable regional record.
    pub fn region_mut(&mut self, index: usize) -> Option<&mut RegionLaw> {
        self.regions.get_mut(index)
    }

    /// `SetCrimeCommitted` (`:2346-2355`): a racial override (e.g. a
    /// transformed lycanthrope) suppresses the crime by storing
    /// [`Crimes::None`]. Returns the stored crime, i.e. the value the
    /// `OnCrimeUpdate` event carries (`:2357-2369`).
    pub fn set_crime(&mut self, crime: Crimes, suppressed_by_racial_override: bool) -> Crimes {
        self.crime = if suppressed_by_racial_override {
            Crimes::None
        } else {
            crime
        };
        self.crime
    }

    /// `LowerRepForCrime` (`:2291-2299`): subtracts the loss from the region
    /// and propagates **half** of it to the People faction of that region.
    pub fn lower_rep_for_crime(
        &mut self,
        region: usize,
        crime: Crimes,
        graph: &mut FactionGraph,
    ) -> Option<ReputationDelta> {
        let loss = crime.reputation_loss();
        let legal_rep_after = self.regions.get_mut(region)?.apply_legal_rep(-loss);
        let people_faction = graph.find_faction_by_type_and_region(PEOPLE_FACTION_TYPE, region as i32);
        let people_delta = -i32::from(loss / 2);
        if let Some(faction) = people_faction {
            graph.change_reputation(faction, people_delta, true);
        }
        Some(ReputationDelta {
            legal_rep_after,
            people_faction,
            people_delta,
        })
    }

    /// `RaiseReputationForDoingSentence` (`:2301-2311`): adds
    /// `halfOfLegalRepPlayerLostFromCrime - 1` to the region and
    /// `(half - 1) / 2` to the People faction, propagated.
    pub fn raise_rep_for_sentence(
        &mut self,
        region: usize,
        graph: &mut FactionGraph,
    ) -> Option<ReputationDelta> {
        let gain = self.half_of_legal_rep_lost_from_crime - 1;
        let legal_rep_after = self.regions.get_mut(region)?.apply_legal_rep(gain);
        let people_faction = graph.find_faction_by_type_and_region(PEOPLE_FACTION_TYPE, region as i32);
        let people_delta = i32::from(gain / 2);
        if let Some(faction) = people_faction {
            graph.change_reputation(faction, people_delta, true);
        }
        Some(ReputationDelta {
            legal_rep_after,
            people_faction,
            people_delta,
        })
    }

    /// State 3 (`:253-261`): the player enters prison and the reputation for
    /// doing the sentence is raised. Returns the reputation delta.
    pub fn serve_sentence(
        &mut self,
        session: &CourtSession,
        graph: &mut FactionGraph,
    ) -> Option<ReputationDelta> {
        if session.state != CourtState::ServingSentence {
            return None;
        }
        self.in_prison = true;
        self.raise_rep_for_sentence(session.region, graph)
    }

    /// `CourtWindow` (`:2334-2344`): marks the player arrested and fixes
    /// `halfOfLegalRepPlayerLostFromCrime` from the current crime before the
    /// session opens.
    pub fn open_court(
        &mut self,
        region: usize,
        gold: i32,
        rescue: GuildRescue,
        rng: &mut DfRandom,
    ) -> Option<CourtSession> {
        let legal_rep = self.regions.get(region)?.legal_rep;
        self.arrested = true;
        self.half_of_legal_rep_lost_from_crime = self.crime.reputation_loss() / 2;
        Some(CourtSession::open(
            self.crime, region, legal_rep, gold, rescue, rng,
        ))
    }

    /// `OnPop` (`:431-449`): resets `Arrested` and `InPrison`, not the
    /// reputation (R-28-7).
    pub fn on_pop(&mut self) {
        self.arrested = false;
        self.in_prison = false;
    }
}

// ── court: punishment and sentence ─────────────────────────────────────────

/// Outcome of the court sentence before any halving: `punishmentType`, the
/// split between `fine` and `daysInPrison`, and the number of split
/// iterations (`penaltyAmount /= 40`, `:148-165`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sentence {
    /// `punishmentType` — 0 severe, 1 execution (unreachable), 2 fine/prison.
    pub punishment_type: i32,
    /// Fine after the gold cap (`:167-173`).
    pub fine: i32,
    /// `daysInPrison` after the gold cap (`:167-173`).
    pub days_in_prison: i32,
    /// Iterations of the split loop, i.e. `penaltyAmount / 40` before halving.
    pub steps: i32,
}

/// Whether the punishment is normal or severe (`:122-138`). Both thresholds
/// come from the negative legal reputation: `t1 = min(-rep, 75)` and
/// `t2 = min(-rep / 2, 75)`; with **both** `FailedRoll` the punishment is
/// [`PUNISHMENT_FINE_OR_PRISON`], otherwise [`PUNISHMENT_SEVERE`].
pub fn punishment_type(legal_rep: i16, rng: &mut DfRandom) -> i32 {
    let mut threshold1 = 0;
    let mut threshold2 = 0;
    if legal_rep < 0 {
        let deficit = -i32::from(legal_rep);
        threshold1 = deficit.min(PUNISHMENT_THRESHOLD_CAP);
        threshold2 = (deficit / 2).min(PUNISHMENT_THRESHOLD_CAP);
    }
    if failed_roll(threshold2, rng) && failed_roll(threshold1, rng) {
        PUNISHMENT_FINE_OR_PRISON
    } else {
        PUNISHMENT_SEVERE
    }
}

/// `penaltyAmount` (`:141-151`) **after** the clamps, before the `/ 40`.
///
/// The lower clamp is unreachable with the retail table: for `LegalRep >= 0`
/// the amount is `P·rep + Base >= Base`, and for `LegalRep < 0` it is
/// `Base + P·|rep| >= Base`; `Base > Min` in all 15 rows and the source tests
/// `> Max` first (`:150-151`), so `amount < Min` never runs. It is kept as the
/// source keeps it.
pub fn penalty_amount(legal_rep: i16, crime: Crimes) -> Option<i32> {
    let index = crime.court_index()?;
    let per_point = PENALTY_PER_LEGAL_REP[index];
    let base = BASE_PENALTY_AMOUNTS[index];
    let rep = i32::from(legal_rep);
    let mut amount = if legal_rep >= 0 {
        per_point * rep + base
    } else {
        base - per_point * rep
    };
    if amount > MAXIMUM_PENALTY_AMOUNTS[index] {
        amount = MAXIMUM_PENALTY_AMOUNTS[index];
    } else if amount < MINIMUM_PENALTY_AMOUNTS[index] {
        amount = MINIMUM_PENALTY_AMOUNTS[index];
    }
    Some(amount)
}

/// Caps the fine at the player's gold and turns the remainder into prison days
/// (`:167-173`).
pub fn cap_fine_to_gold(fine: i32, days_in_prison: i32, gold: i32) -> (i32, i32) {
    if gold < fine {
        (gold, days_in_prison + (fine - gold) / FINE_STEP)
    } else {
        (fine, days_in_prison)
    }
}

/// Full sentence of the court (`:141-173`), computing the punishment type
/// first (it draws from the same seeded PRNG, R-28-3). `None` for
/// [`Crimes::None`], which closes the court without reading the table.
pub fn sentence(
    legal_rep: i16,
    crime: Crimes,
    gold: i32,
    rng: &mut DfRandom,
) -> Option<Sentence> {
    let punishment_type = punishment_type(legal_rep, rng);
    let steps = penalty_amount(legal_rep, crime)? / FINE_STEP;
    let mut fine = 0;
    let mut days_in_prison = 0;
    for _ in 0..steps {
        if rng.rand() & 1 != 0 {
            fine += FINE_STEP;
        } else {
            days_in_prison += PRISON_DAYS_STEP;
        }
    }
    let (fine, days_in_prison) = cap_fine_to_gold(fine, days_in_prison, gold);
    Some(Sentence {
        punishment_type,
        fine,
        days_in_prison,
        steps,
    })
}

/// Digest of `(fine, days)` for the determinism block: FNV-1a 64 of the two
/// little-endian `i32` values. **Declared delta:** the spec names `xxh3`; the
/// `behavior` organ is dependency-free, so the same role (identity ×2) uses
/// the organ's FNV-1a 64, as M27 already declared.
pub fn sentence_digest(fine: i32, days_in_prison: i32) -> u64 {
    let mut bytes = Vec::with_capacity(8);
    bytes.extend_from_slice(&fine.to_le_bytes());
    bytes.extend_from_slice(&days_in_prison.to_le_bytes());
    fnv1a64(&bytes)
}

// ── court: pleading, debate and guild rescue ───────────────────────────────

/// The two ways of pleading not guilty (`:369-392`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plea {
    /// `Debate` — uses Etiquette.
    Debate,
    /// `Lie` — uses Streetwise.
    Lie,
}

impl Plea {
    /// Skill name the plea consumes (`:371-384`).
    pub const fn skill(self) -> &'static str {
        match self {
            Self::Debate => "Etiquette",
            Self::Lie => "Streetwise",
        }
    }
}

/// `chanceToGoFree = LegalRep + (skill + Personality) / 2`, clamped to
/// `[5, 95]` (`:386-395`).
pub fn chance_to_go_free(legal_rep: i16, skill: i16, personality: i16) -> i32 {
    let chance = i32::from(legal_rep) + (i32::from(skill) + i32::from(personality)) / 2;
    chance.clamp(MIN_CHANCE_TO_GO_FREE, MAX_CHANCE_TO_GO_FREE)
}

/// Outcome of pleading not guilty (`:397-427`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PleaOutcome {
    /// The plea succeeded: free to go, reputation raised.
    Free,
    /// Severe punishment (type 0).
    Banished,
    /// Execution (type 1; unreachable from the retail sentence).
    Executed,
    /// Fine/prison: the fine may be doubled or halved by the roll.
    FineModified {
        /// `roll < 25 ⇒ fine *= 2`.
        doubled: bool,
        /// `roll > 75 ⇒ fine >>= 1`.
        halved: bool,
    },
}

/// `Dice100.FailedRoll(chanceToGoFree)` then the punishment branch
/// (`:397-411`). The fine roll is `LegalRep + Dice100.Roll()`
/// (`:403-407`).
pub fn plea_outcome(
    chance: i32,
    punishment_type: i32,
    legal_rep: i16,
    rng: &mut DfRandom,
) -> PleaOutcome {
    if !failed_roll(chance, rng) {
        return PleaOutcome::Free;
    }
    match punishment_type {
        PUNISHMENT_SEVERE => PleaOutcome::Banished,
        PUNISHMENT_EXECUTION => PleaOutcome::Executed,
        _ => {
            let roll = i32::from(legal_rep) + roll_1_100(rng);
            PleaOutcome::FineModified {
                doubled: roll < PLEA_ROLL_DOUBLE_BELOW,
                halved: roll > PLEA_ROLL_HALVE_ABOVE,
            }
        }
    }
}

/// Guild that rescues the player before the normal sentence (`:176-218`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RescueGuild {
    /// Dark Brotherhood — Assault or Murder (`:180-199`).
    DarkBrotherhood,
    /// Thieves Guild — `crimeType <= 2 || crimeType == 11` (`:200-218`).
    ThievesGuild,
}

/// Membership and rank read from M27 (`GuildDirectory`); the rescue compares
/// `Rank >= Random.Range(0, 20)`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GuildRescue {
    /// Member of the Dark Brotherhood.
    pub dark_brotherhood_member: bool,
    /// Current rank there (`-1` when not a member, `:180-183`).
    pub dark_brotherhood_rank: i32,
    /// Member of the Thieves Guild.
    pub thieves_member: bool,
    /// Current rank there (`:201-204`).
    pub thieves_rank: i32,
}

/// Rescue check of `:176-218`. With `rank < 0` the comparison never passes
/// (R-28-6). The Dark Brotherhood is evaluated first and short-circuits.
pub fn rescue_for_crime(
    crime: Crimes,
    rescue: GuildRescue,
    rng: &mut DfRandom,
) -> Option<RescueGuild> {
    let violent = matches!(crime, Crimes::Assault | Crimes::Murder);
    if violent && rescue.dark_brotherhood_member {
        let draw = draw_inclusive(rng, 0, 19);
        if rescue.dark_brotherhood_rank >= draw {
            return Some(RescueGuild::DarkBrotherhood);
        }
    }
    let index = crime.court_index()?;
    let thieving = index <= 2 || index == 11;
    if thieving && rescue.thieves_member {
        let draw = draw_inclusive(rng, 0, 19);
        if rescue.thieves_rank >= draw {
            return Some(RescueGuild::ThievesGuild);
        }
    }
    None
}

// ── court: state machine (`:231-353`) ──────────────────────────────────────

/// Court states of `HandleCourtLogic` (`:231-353`); [`CourtState::code`] gives
/// the numeric state of the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CourtState {
    /// `0` — computes the punishment and asks guilty / not guilty.
    Starting,
    /// `1` — waiting for the button.
    AwaitingVerdict,
    /// `2` — found guilty (after a failed plea).
    FoundGuilty,
    /// `3` — serving the prison sentence.
    ServingSentence,
    /// `4` — banished.
    Banished,
    /// `5` — executed (unreachable from the retail sentence, `:278`).
    Executed,
    /// `6` — reposition at the location entrance.
    Repositioning,
    /// `100` — done.
    Done,
}

impl CourtState {
    /// Numeric state of the source.
    pub const fn code(self) -> u8 {
        match self {
            Self::Starting => 0,
            Self::AwaitingVerdict => 1,
            Self::FoundGuilty => 2,
            Self::ServingSentence => 3,
            Self::Banished => 4,
            Self::Executed => 5,
            Self::Repositioning => 6,
            Self::Done => 100,
        }
    }
}

/// One court session (`DaggerfallCourtWindow`): the sentence plus the visible
/// side effects the caller must apply (gold, prison, reputation, vitals,
/// flags). `CourtSession` never touches the player directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CourtSession {
    /// Current state.
    pub state: CourtState,
    /// Region whose legal reputation is on trial.
    pub region: usize,
    /// Crime under trial.
    pub crime: Crimes,
    /// Legal reputation at the moment the session opened.
    pub legal_rep: i16,
    /// Sentence; `None` when the crime is `None` or a guild rescued the player.
    pub sentence: Option<Sentence>,
    /// Guild that rescued the player, if any.
    pub rescued_by: Option<RescueGuild>,
    /// Gold the court takes (`DeductGoldAmount`, `:334`); 0 when nothing is paid.
    pub fine_paid: i32,
    /// Prison days still to serve (`daysInPrison`, halved on guilty).
    pub days_left: i32,
    /// Whether `RaiseReputationForDoingSentence` runs.
    pub raise_reputation: bool,
    /// Whether `FillVitalSigns` runs.
    pub refill_vitals: bool,
    /// Whether the player is repositioned at the location entrance.
    pub reposition: bool,
}

impl CourtSession {
    /// Opens a session (`state == 0`): computes the punishment, the sentence
    /// and the guild rescue, then waits for the verdict. A [`Crimes::None`]
    /// crime closes immediately without reading the table (`:109-113`).
    pub fn open(
        crime: Crimes,
        region: usize,
        legal_rep: i16,
        gold: i32,
        rescue: GuildRescue,
        rng: &mut DfRandom,
    ) -> Self {
        let mut session = Self {
            state: CourtState::Done,
            region,
            crime,
            legal_rep,
            sentence: None,
            rescued_by: None,
            fine_paid: 0,
            days_left: 0,
            raise_reputation: false,
            refill_vitals: false,
            reposition: false,
        };
        if crime == Crimes::None {
            return session;
        }
        let sentence = sentence(legal_rep, crime, gold, rng).expect("court table row");
        session.days_left = sentence.days_in_prison;
        session.sentence = Some(sentence);
        if let Some(guild) = rescue_for_crime(crime, rescue, rng) {
            session.rescued_by = Some(guild);
            session.raise_reputation = true;
            session.refill_vitals = true;
            session.state = CourtState::Done;
            return session;
        }
        session.state = CourtState::AwaitingVerdict;
        session
    }

    /// Guilty button (`:315-353`).
    pub fn answer_guilty(&mut self) {
        let Some(sentence) = self.sentence else {
            return;
        };
        self.state = match sentence.punishment_type {
            PUNISHMENT_SEVERE => {
                self.reposition = true;
                self.refill_vitals = true;
                CourtState::Banished
            }
            PUNISHMENT_EXECUTION => {
                self.reposition = true;
                CourtState::Executed
            }
            _ => {
                self.fine_paid = sentence.fine >> 1;
                self.days_left = sentence.days_in_prison >> 1;
                self.raise_reputation = true;
                self.reposition = true;
                if self.days_left > 0 {
                    CourtState::ServingSentence
                } else {
                    self.refill_vitals = true;
                    CourtState::Done
                }
            }
        };
    }

    /// Not-guilty button followed by the debate/lie outcome (`:355-427`).
    pub fn answer_not_guilty(&mut self, outcome: PleaOutcome) {
        self.state = match outcome {
            PleaOutcome::Free => {
                self.refill_vitals = true;
                self.raise_reputation = true;
                CourtState::Repositioning
            }
            PleaOutcome::Banished => {
                self.reposition = true;
                self.refill_vitals = true;
                CourtState::Banished
            }
            PleaOutcome::Executed => {
                self.reposition = true;
                CourtState::Executed
            }
            PleaOutcome::FineModified { doubled, halved } => {
                if let Some(sentence) = self.sentence.as_mut() {
                    if doubled {
                        sentence.fine *= 2;
                    } else if halved {
                        sentence.fine >>= 1;
                    }
                }
                CourtState::FoundGuilty
            }
        };
    }

    /// Advances the automatic states: `FoundGuilty` to serve or release
    /// (`:231-252`), `Executed` to the reposition (`:291-295`), and
    /// `Repositioning` to done.
    pub fn advance(&mut self) {
        match self.state {
            CourtState::FoundGuilty => {
                self.reposition = true;
                self.raise_reputation = true;
                if self.days_left > 0 {
                    self.state = CourtState::ServingSentence;
                } else {
                    self.refill_vitals = true;
                    self.state = CourtState::Done;
                }
            }
            CourtState::Executed => self.state = CourtState::Repositioning,
            CourtState::Repositioning => self.state = CourtState::Done,
            other => self.state = other,
        }
    }

    /// Applies the severe punishment flags of states 4 and 5 (`:262-290`).
    pub fn apply_severe_punishment(&self, region: &mut RegionLaw) {
        match self.state {
            CourtState::Banished => region.severe_punishment_flags |= SEVERE_PUNISHMENT_BANISHED,
            CourtState::Executed => region.severe_punishment_flags |= SEVERE_PUNISHMENT_EXECUTED,
            _ => {}
        }
    }
}

// ── surrender and guard response (`:2313-2332`, `EnemyAttack.cs:270-296`) ─

/// Result of [`surrender_to_city_guards`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurrenderOutcome {
    /// Health after the surrender: **always 1** (`SetHealth(1)`, R-28-5).
    pub health_after: i32,
    /// Whether the court opens.
    pub court: bool,
}

/// `SurrenderToCityGuards` (`:2313-2332`). Returns `None` when the player is
/// already dead (the source returns `false`). Note that the source sets health
/// to 1 **before** deciding, so a refused surrender still leaves 1 HP.
pub fn surrender_to_city_guards(
    legal_rep: i16,
    health: i32,
    voluntary: bool,
    rng: &mut DfRandom,
) -> Option<SurrenderOutcome> {
    if health <= 0 {
        return None;
    }
    if legal_rep < -20 && !voluntary {
        return Some(SurrenderOutcome {
            health_after: 1,
            court: false,
        });
    }
    if !(-20..=0).contains(&legal_rep) {
        return Some(SurrenderOutcome {
            health_after: 1,
            court: true,
        });
    }
    if (rng.rand() & 1) != 0 && !voluntary {
        return Some(SurrenderOutcome {
            health_after: 1,
            court: false,
        });
    }
    Some(SurrenderOutcome {
        health_after: 1,
        court: true,
    })
}

/// Effect of receiving a guard hit (`Game/EnemyAttack.cs:270-296,395-402`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuardHitResponse {
    /// `LowerRepForCrime()` runs because the crime is not `None` and the
    /// surrender dialogue was not shown yet.
    pub lower_legal_rep: bool,
    /// The surrender dialogue is asked once.
    pub show_surrender_dialogue: bool,
    /// The hit lands because the player cannot survive it or refused to
    /// surrender.
    pub apply_damage: bool,
}

/// Guard hit decision (`EnemyAttack.cs:270-296`): the reputation drop and the
/// dialogue happen at most once per crime; the damage always lands when the
/// player would die from it or declined the surrender.
pub fn guard_hit_response(
    crime: Crimes,
    dialogue_shown: bool,
    survives_hit: bool,
    surrender_declined: bool,
) -> GuardHitResponse {
    let first_hit = crime != Crimes::None && !dialogue_shown;
    GuardHitResponse {
        lower_legal_rep: first_hit,
        show_surrender_dialogue: first_hit,
        apply_damage: !survives_hit || surrender_declined,
    }
}

// ── guard spawn request (`SpawnCityGuards`, `:621-745`) ────────────────────

/// Input of [`guard_spawn_request`]. The NPC pool, vision rays and interior
/// doors belong to M26/M11; the caller supplies their outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GuardSpawnContext {
    /// `HowManyEnemiesOfType(Knight_CityWatch, false, true)`.
    pub active_guards: usize,
    /// `IsPlayerInsideDungeon` — guards never spawn there (`:626`).
    pub inside_dungeon: bool,
    /// Inside an open shop, tavern or residence (`:631`).
    pub inside_building: bool,
    /// `immediateSpawn` (`:644`).
    pub immediate: bool,
    /// NPCs active inside the 77.5 m radius (`:665`).
    pub npcs_in_radius: usize,
    /// A guard NPC saw the player (line of sight over 77.5 m / 95°).
    pub seen_by_guard: bool,
    /// A non-guard NPC saw the player (`:734-742`).
    pub seen_by_npc: bool,
}

/// What M28 asks M26 to spawn; M28 never creates guard entities (`:747-762`
/// is M26's contract).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GuardSpawnRequest {
    /// `Random.Range(2, 6)` guards at the lowest outer door (`:630-641`).
    Interior {
        /// Number of guards, `2..=5`.
        guards: usize,
        /// `radius + 0.1f` from the door (`:633-634`).
        door_offset_meters: f32,
    },
    /// Guards created from the nearby NPC pool, no countdown (`:644-688`).
    ImmediateFromPool {
        /// Replacement radius around the player (`:665`).
        radius_meters: f32,
    },
    /// `CreateFoeSpawner` fallback when no NPC could be replaced (`:685-688`).
    FoeSpawner {
        /// Number of guards, `2..=5`.
        guards: usize,
        /// Inner radius (`:687`).
        min_radius_meters: f32,
        /// Outer radius (`:687`).
        max_radius_meters: f32,
    },
    /// Guards arrive after a countdown bound to the location (`:737-742`).
    Countdown {
        /// `Random.Range(5, 10 + 1)` seconds.
        seconds: u32,
    },
}

/// Decides the spawn request of `SpawnCityGuards` (`:621-745`). `None` means
/// no guards are requested: inside a dungeon, with more than
/// [`MAX_ACTIVE_GUARD_SPAWNS`] active guards, or when nobody saw the player
/// and the spawn was not immediate.
pub fn guard_spawn_request(
    context: GuardSpawnContext,
    rng: &mut DfRandom,
) -> Option<GuardSpawnRequest> {
    if context.inside_dungeon || context.active_guards > MAX_ACTIVE_GUARD_SPAWNS {
        return None;
    }
    if context.inside_building {
        return Some(GuardSpawnRequest::Interior {
            guards: draw_inclusive(rng, INTERIOR_GUARD_MIN as i32, INTERIOR_GUARD_MAX as i32)
                as usize,
            door_offset_meters: INTERIOR_DOOR_OFFSET_METERS,
        });
    }
    if context.immediate {
        if context.npcs_in_radius > 0 {
            return Some(GuardSpawnRequest::ImmediateFromPool {
                radius_meters: GUARD_ATTENTION_RADIUS_METERS,
            });
        }
        return Some(GuardSpawnRequest::FoeSpawner {
            guards: draw_inclusive(rng, FOE_SPAWNER_GUARD_MIN as i32, FOE_SPAWNER_GUARD_MAX as i32)
                as usize,
            min_radius_meters: FOE_SPAWNER_MIN_RADIUS_METERS,
            max_radius_meters: FOE_SPAWNER_MAX_RADIUS_METERS,
        });
    }
    if context.seen_by_guard {
        return Some(GuardSpawnRequest::ImmediateFromPool {
            radius_meters: GUARD_ATTENTION_RADIUS_METERS,
        });
    }
    if context.seen_by_npc {
        return Some(GuardSpawnRequest::Countdown {
            seconds: draw_inclusive(
                rng,
                GUARD_COUNTDOWN_MIN_SECONDS as i32,
                GUARD_COUNTDOWN_MAX_SECONDS as i32,
            ) as u32,
        });
    }
    None
}

// ── prison (`:464-490`) ────────────────────────────────────────────────────

/// Prison clock and flags of `UpdatePrisonScreen` / `ReleaseFromPrison`
/// (`:464-490`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrisonState {
    /// `daysInPrison` used for the clock jump (`:474`).
    pub days_sentence: i32,
    /// `daysInPrisonLeft` shown to the player.
    pub days_left: i32,
    /// Seconds of game clock advanced by this state.
    pub clock_advanced: i64,
    /// `InPrison`.
    pub in_prison: bool,
    /// `PreventEnemySpawns` set when the sentence ends (`:470-471`).
    pub prevent_enemy_spawns: bool,
    /// `PreventNormalizingReputations` set when the sentence ends (`:472`).
    pub prevent_normalizing_reputations: bool,
}

impl PrisonState {
    /// Enters prison for `days` days (`state == 3`, `:253-261`).
    pub fn begin(days: i32) -> Self {
        Self {
            days_sentence: days,
            days_left: days.max(0),
            clock_advanced: 0,
            in_prison: days > 0,
            prevent_enemy_spawns: false,
            prevent_normalizing_reputations: false,
        }
    }

    /// One UI day tick (`:300-318`); returns `true` when the sentence just
    /// ended, at which point the clock advances `days_sentence * 1440 * 60`.
    pub fn tick_day(&mut self) -> bool {
        self.days_left = (self.days_left - 1).max(0);
        if self.days_left > 0 {
            return false;
        }
        self.prevent_enemy_spawns = true;
        self.prevent_normalizing_reputations = true;
        self.clock_advanced += i64::from(self.days_sentence) * SECONDS_PER_DAY;
        self.in_prison = false;
        true
    }

    /// `ReleaseFromPrison` (`:481-490`): advances 4 hours and returns the
    /// seconds added.
    pub fn release(&mut self) -> i64 {
        self.clock_advanced += RELEASE_SECONDS;
        self.in_prison = false;
        RELEASE_SECONDS
    }

    /// `prisonUpdateInterval` (`:54`, `:300-301`).
    pub const fn ui_interval(fast: bool) -> f32 {
        if fast {
            PRISON_UI_FAST_INTERVAL
        } else {
            PRISON_UI_INTERVAL
        }
    }
}

/// End of the sentence and release (`:470-490`): clears the crime and returns
/// the seconds the caller must advance after the prison jump.
pub fn finish_sentence(prison: &mut PrisonState, law: &mut PlayerLaw) -> i64 {
    law.crime = Crimes::None;
    law.in_prison = false;
    prison.release()
}

// ── crime sources (spec §1.3) ──────────────────────────────────────────────

/// The seven triggers that raise a crime (`spec §1.3`). The loan default is a
/// separate path because it never spawns guards ([`loan_default_crime`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrimeTrigger {
    /// Failed force of a **closed** door plus the 10 % of being seen.
    LockedDoorForce,
    /// Force of an **open** door plus the 10 %.
    OpenDoorForce,
    /// Failed pickpocket against a townsperson.
    Pickpocket,
    /// Killing a non-guard NPC with a weapon.
    WeaponKill,
    /// Hourly check with `LegalRep < -10`.
    CriminalConspiracy,
    /// Hourly check in a banished region.
    BanishedRegion,
    /// Quest verb `setplayercrime <crime>`.
    QuestVerb,
}

impl CrimeTrigger {
    /// Crime the trigger raises.
    pub const fn crime(self) -> Crimes {
        match self {
            Self::LockedDoorForce => Crimes::AttemptedBreakingAndEntering,
            Self::OpenDoorForce => Crimes::BreakingAndEntering,
            Self::Pickpocket => Crimes::Pickpocketing,
            Self::WeaponKill => Crimes::Murder,
            Self::CriminalConspiracy | Self::BanishedRegion => Crimes::CriminalConspiracy,
            Self::QuestVerb => Crimes::None,
        }
    }

    /// Whether guards spawn immediately (`SpawnCityGuards(immediate: true)`).
    pub const fn immediate_guards(self) -> bool {
        matches!(
            self,
            Self::LockedDoorForce | Self::OpenDoorForce | Self::Pickpocket | Self::WeaponKill
        )
    }

    /// Source citation.
    pub const fn source(self) -> &'static str {
        match self {
            Self::LockedDoorForce => "PlayerActivate.cs:571-579",
            Self::OpenDoorForce => "PlayerActivate.cs:620-626",
            Self::Pickpocket => "PlayerActivate.cs:1653-1658",
            Self::WeaponKill => "WeaponManager.cs:506-508",
            Self::CriminalConspiracy => "PlayerEntity.cs:498-504",
            Self::BanishedRegion => "PlayerEntity.cs:506-511",
            Self::QuestVerb => "SetPlayerCrime.cs:28,46-51,60",
        }
    }
}

/// The seven triggers of spec §1.3.
pub const CRIME_TRIGGERS: [CrimeTrigger; 7] = [
    CrimeTrigger::LockedDoorForce,
    CrimeTrigger::OpenDoorForce,
    CrimeTrigger::Pickpocket,
    CrimeTrigger::WeaponKill,
    CrimeTrigger::CriminalConspiracy,
    CrimeTrigger::BanishedRegion,
    CrimeTrigger::QuestVerb,
];

/// Hourly crime check (`PlayerEntity.cs:498-511`): `SuccessRoll(5)` with
/// `LegalRep < -10`, or `SuccessRoll(10)` in a region with
/// `SeverePunishmentFlags & 1`. Both paths raise `Criminal_Conspiracy` without
/// an immediate guard spawn.
pub fn hourly_crime(legal_rep: i16, severe_punishment_flags: u8, rng: &mut DfRandom) -> Option<Crimes> {
    if legal_rep < CRIMINAL_CONSPIRACY_LEGAL_REP_BELOW
        && success_roll(CRIMINAL_CONSPIRACY_CHANCE, rng)
    {
        return Some(Crimes::CriminalConspiracy);
    }
    if severe_punishment_flags & SEVERE_PUNISHMENT_BANISHED != 0
        && success_roll(BANISHED_CONSPIRACY_CHANCE, rng)
    {
        return Some(Crimes::CriminalConspiracy);
    }
    None
}

/// Quest verb validation (`SetPlayerCrime.cs:46-54`): an unknown crime is an
/// error for the caller (M29 raises it); the state assignment itself is
/// [`PlayerLaw::set_crime`].
pub fn quest_crime(argument: &str) -> Option<Crimes> {
    Crimes::from_name(argument)
}

/// `LoanChecker.OverdueLoan` (`:53-70`): after repaying from the account, a
/// loan that is still overdue and not previously defaulted raises
/// [`Crimes::LoanDefault`] **without guards**.
pub fn loan_default_crime(
    has_loan: bool,
    account_covers_loan: bool,
    has_defaulted: bool,
) -> Option<Crimes> {
    if has_loan && !account_covers_loan && !has_defaulted {
        Some(Crimes::LoanDefault)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Region 0 People faction (`type: 15`) with one ally and one enemy, so
    /// that the propagation of `change_reputation(..., propagate: true)` is
    /// observable.
    const PEOPLE_GRAPH: &str = "#77\ntype: 15\nregion: 1\nname: People\nally: 78\nenemy: 79\n#78\ntype: 2\nname: Ally\n#79\ntype: 2\nname: Enemy\n";

    fn people_graph() -> FactionGraph {
        FactionGraph::parse(PEOPLE_GRAPH, 0x1234).expect("valid fixture")
    }

    #[test]
    fn crime_values_and_losses_match_the_source() {
        assert_eq!(Crimes::ALL.len(), CRIME_KINDS);
        for (index, crime) in Crimes::ALL.iter().enumerate() {
            assert_eq!(crime.value() as usize, index);
            assert_eq!(Crimes::from_value(index as u8), Some(*crime));
            assert_eq!(Crimes::from_name(crime.name()), Some(*crime));
        }
        assert_eq!(Crimes::from_value(16), None);
        assert_eq!(Crimes::from_name("murder"), None, "case-sensitive enum lookup");
        assert_eq!(Crimes::Murder.reputation_loss(), 20);
        assert_eq!(Crimes::HighTreason.reputation_loss(), 75);
        assert_eq!(Crimes::Treason.reputation_loss(), 36);
        assert_eq!(Crimes::Vagrancy.reputation_loss(), 1);
        assert_eq!(Crimes::None.court_index(), None);
        assert_eq!(Crimes::AttemptedBreakingAndEntering.court_index(), Some(0));
        assert_eq!(Crimes::LoanDefault.court_index(), Some(14));
    }

    #[test]
    fn penalty_golden_vectors_gv_28_01_to_12() {
        let cases = [
            (Crimes::Trespassing, 0, 200, 5),
            (Crimes::Assault, 0, 1000, 25),
            (Crimes::Murder, 0, 10000, 250),
            (Crimes::Murder, -100, 11000, 275),
            (Crimes::Murder, 100, 11000, 275),
            (Crimes::HighTreason, 100, 2000, 50),
            (Crimes::HighTreason, -100, 2000, 50),
            (Crimes::Theft, -50, 200, 5),
            (Crimes::LoanDefault, 0, 100, 2),
            (Crimes::Vagrancy, 0, 100, 2),
            (Crimes::Pickpocketing, 30, 380, 9),
            (Crimes::Smuggling, -100, 1300, 32),
        ];
        for (crime, rep, want_amount, want_steps) in cases {
            let amount = penalty_amount(rep, crime).expect("table row");
            assert_eq!(amount, want_amount, "{crime:?} rep={rep}");
            assert_eq!(amount / FINE_STEP, want_steps, "{crime:?} rep={rep}");
        }
    }

    #[test]
    fn minimum_clamp_is_unreachable_with_the_retail_table() {
        for crime in Crimes::ALL {
            let Some(index) = crime.court_index() else {
                continue;
            };
            assert!(
                BASE_PENALTY_AMOUNTS[index] > MINIMUM_PENALTY_AMOUNTS[index],
                "{crime:?}: Base must exceed Min"
            );
            assert!(MAXIMUM_PENALTY_AMOUNTS[index] >= MINIMUM_PENALTY_AMOUNTS[index]);
            for rep in -100..=100 {
                let amount = penalty_amount(rep, crime).expect("table row");
                assert!(
                    amount >= MINIMUM_PENALTY_AMOUNTS[index],
                    "{crime:?} rep={rep} amount={amount}"
                );
            }
        }
    }

    #[test]
    fn legal_rep_clamps_at_minus_and_plus_100() {
        let mut region = RegionLaw::new();
        assert_eq!(region.set_legal_rep(-101), -100);
        assert_eq!(region.set_legal_rep(101), 100);
        assert_eq!(region.apply_legal_rep(-20), 80);
        assert_eq!(region.apply_legal_rep(-1000), -100);
        assert_eq!(region.apply_legal_rep(1000), 100);
    }

    #[test]
    fn sentence_is_deterministic_for_the_same_seed() {
        let first = sentence(-50, Crimes::Murder, 1_000_000, &mut DfRandom::srand(12345))
            .expect("table row");
        let second = sentence(-50, Crimes::Murder, 1_000_000, &mut DfRandom::srand(12345))
            .expect("table row");
        assert_eq!(first, second);
        assert_eq!(
            sentence_digest(first.fine, first.days_in_prison),
            sentence_digest(second.fine, second.days_in_prison)
        );
        assert_eq!(first.steps, 262, "floor(10500 / 40)");
    }

    #[test]
    fn split_identity_holds_before_halving_and_gold_cap() {
        let sentence =
            sentence(-50, Crimes::Murder, 1_000_000, &mut DfRandom::srand(7)).expect("table row");
        assert!(sentence.fine > 0 || sentence.days_in_prison > 0);
        assert_eq!(
            sentence.fine / FINE_STEP + sentence.days_in_prison / PRISON_DAYS_STEP,
            sentence.steps
        );
        assert_eq!(sentence.fine % FINE_STEP, 0);
        assert_eq!(sentence.days_in_prison % PRISON_DAYS_STEP, 0);
    }

    #[test]
    fn gold_cap_gv_28_25() {
        assert_eq!(cap_fine_to_gold(400, 0, 120), (120, 7));
        assert_eq!(cap_fine_to_gold(400, 9, 120), (120, 16));
        assert_eq!(cap_fine_to_gold(400, 0, 400), (400, 0));
    }

    #[test]
    fn none_crime_closes_the_court_without_a_sentence() {
        let session =
            CourtSession::open(Crimes::None, 0, -100, 0, GuildRescue::default(), &mut DfRandom::srand(1));
        assert_eq!(session.state, CourtState::Done);
        assert!(session.sentence.is_none());
        assert_eq!(penalty_amount(-100, Crimes::None), None);
    }

    #[test]
    fn lower_rep_for_crime_gv_28_13_14_15() {
        let mut law = PlayerLaw::new(1);
        let mut graph = people_graph();
        let delta = law
            .lower_rep_for_crime(0, Crimes::Murder, &mut graph)
            .expect("region");
        assert_eq!(delta.legal_rep_after, -20);
        assert_eq!(delta.people_delta, -10);
        assert_eq!(graph.reputation(77), -10);
        assert_eq!(graph.reputation(78), -5, "ally receives half");
        assert_eq!(graph.reputation(79), 5, "enemy receives the inverse");

        let mut law = PlayerLaw::new(1);
        let mut graph = people_graph();
        let delta = law
            .lower_rep_for_crime(0, Crimes::HighTreason, &mut graph)
            .expect("region");
        assert_eq!(delta.legal_rep_after, -75);
        assert_eq!(delta.people_delta, -37);
        assert_eq!(graph.reputation(77), -37);

        let mut law = PlayerLaw::new(1);
        let mut graph = people_graph();
        let delta = law
            .lower_rep_for_crime(0, Crimes::Vagrancy, &mut graph)
            .expect("region");
        assert_eq!(delta.legal_rep_after, -1);
        assert_eq!(delta.people_delta, 0);
        assert_eq!(graph.reputation(77), 0);
    }

    #[test]
    fn raise_rep_for_sentence_gv_28_16() {
        let mut law = PlayerLaw::new(1);
        let mut graph = people_graph();
        law.set_crime(Crimes::Murder, false);
        law.open_court(0, 0, GuildRescue::default(), &mut DfRandom::srand(1))
            .expect("region");
        assert_eq!(law.half_of_legal_rep_lost_from_crime, 10);
        let delta = law.raise_rep_for_sentence(0, &mut graph).expect("region");
        assert_eq!(delta.legal_rep_after, 9);
        assert_eq!(delta.people_delta, 4);
        assert_eq!(graph.reputation(77), 4);
        assert_eq!(graph.reputation(78), 2, "ally receives half");
        assert_eq!(graph.reputation(79), -2, "enemy receives the inverse");
    }

    #[test]
    fn set_crime_honours_the_racial_suppression() {
        let mut law = PlayerLaw::new(1);
        assert_eq!(law.set_crime(Crimes::Murder, false), Crimes::Murder);
        assert_eq!(law.set_crime(Crimes::Theft, true), Crimes::None);
        assert_eq!(law.crime, Crimes::None);
    }

    #[test]
    fn surrender_rules() {
        assert_eq!(
            surrender_to_city_guards(-30, 0, true, &mut DfRandom::srand(1)),
            None,
            "a dead player cannot surrender"
        );
        let refused = surrender_to_city_guards(-21, 10, false, &mut DfRandom::srand(1))
            .expect("alive");
        assert_eq!(refused, SurrenderOutcome { health_after: 1, court: false });
        assert_eq!(
            surrender_to_city_guards(10, 1, false, &mut DfRandom::srand(1)),
            Some(SurrenderOutcome { health_after: 1, court: true }),
            "legal reputation > 0 always opens the court"
        );
        let mut courts = 0;
        let mut refusals = 0;
        for seed in 0..64 {
            let outcome =
                surrender_to_city_guards(0, 10, false, &mut DfRandom::srand(seed)).expect("alive");
            assert_eq!(outcome.health_after, 1);
            if outcome.court {
                courts += 1;
            } else {
                refusals += 1;
            }
        }
        assert!(courts > 0 && refusals > 0, "legal 0 is a 50 % draw");
    }

    #[test]
    fn court_state_codes_match_the_source() {
        assert_eq!(CourtState::Starting.code(), 0);
        assert_eq!(CourtState::AwaitingVerdict.code(), 1);
        assert_eq!(CourtState::FoundGuilty.code(), 2);
        assert_eq!(CourtState::ServingSentence.code(), 3);
        assert_eq!(CourtState::Banished.code(), 4);
        assert_eq!(CourtState::Executed.code(), 5);
        assert_eq!(CourtState::Repositioning.code(), 6);
        assert_eq!(CourtState::Done.code(), 100);
    }

    fn session_with(punishment_type: i32, fine: i32, days: i32) -> CourtSession {
        CourtSession {
            state: CourtState::AwaitingVerdict,
            region: 0,
            crime: Crimes::Murder,
            legal_rep: -50,
            sentence: Some(Sentence {
                punishment_type,
                fine,
                days_in_prison: days,
                steps: fine / FINE_STEP + days / PRISON_DAYS_STEP,
            }),
            rescued_by: None,
            fine_paid: 0,
            days_left: days,
            raise_reputation: false,
            refill_vitals: false,
            reposition: false,
        }
    }

    #[test]
    fn guilty_halves_fine_and_days_and_serves() {
        let mut session = session_with(PUNISHMENT_FINE_OR_PRISON, 400, 12);
        session.answer_guilty();
        assert_eq!(session.state, CourtState::ServingSentence);
        assert_eq!(session.fine_paid, 200);
        assert_eq!(session.days_left, 6);
        assert!(session.raise_reputation && session.reposition);
        session.advance();
        assert_eq!(session.state, CourtState::ServingSentence);
    }

    #[test]
    fn guilty_without_prison_days_releases_with_reputation() {
        let mut session = session_with(PUNISHMENT_FINE_OR_PRISON, 400, 0);
        session.answer_guilty();
        assert_eq!(session.state, CourtState::Done);
        assert!(session.refill_vitals && session.raise_reputation);
    }

    #[test]
    fn severe_punishment_banishes_and_flags_the_region() {
        let mut session = session_with(PUNISHMENT_SEVERE, 0, 0);
        session.answer_guilty();
        assert_eq!(session.state, CourtState::Banished);
        assert!(session.reposition && session.refill_vitals);
        let mut region = RegionLaw::new();
        session.apply_severe_punishment(&mut region);
        assert!(region.banished());
        assert!(!region.executed());
    }

    #[test]
    fn not_guilty_paths() {
        let mut session = session_with(PUNISHMENT_FINE_OR_PRISON, 800, 9);
        session.answer_not_guilty(PleaOutcome::Free);
        assert_eq!(session.state, CourtState::Repositioning);
        session.advance();
        assert_eq!(session.state, CourtState::Done);
        assert!(session.raise_reputation);

        let mut session = session_with(PUNISHMENT_FINE_OR_PRISON, 800, 9);
        session.answer_not_guilty(PleaOutcome::FineModified {
            doubled: false,
            halved: true,
        });
        assert_eq!(session.state, CourtState::FoundGuilty);
        assert_eq!(session.sentence.as_ref().expect("sentence").fine, 400);
        assert_eq!(session.fine_paid, 0, "the debate path does not deduct gold");
        session.advance();
        assert_eq!(session.state, CourtState::ServingSentence);
    }

    #[test]
    fn guild_rescue_rules() {
        let rescue = GuildRescue {
            dark_brotherhood_member: true,
            dark_brotherhood_rank: 19,
            thieves_member: true,
            thieves_rank: 19,
        };
        assert_eq!(
            rescue_for_crime(Crimes::Murder, rescue, &mut DfRandom::srand(1)),
            Some(RescueGuild::DarkBrotherhood)
        );
        assert_eq!(
            rescue_for_crime(Crimes::Pickpocketing, rescue, &mut DfRandom::srand(1)),
            Some(RescueGuild::ThievesGuild)
        );
        let outsider = GuildRescue::default();
        assert_eq!(
            rescue_for_crime(Crimes::Murder, outsider, &mut DfRandom::srand(1)),
            None
        );
        let negative_rank = GuildRescue {
            dark_brotherhood_member: true,
            dark_brotherhood_rank: -1,
            thieves_member: true,
            thieves_rank: -1,
        };
        assert_eq!(
            rescue_for_crime(Crimes::Murder, negative_rank, &mut DfRandom::srand(1)),
            None,
            "rank -1 never rescues (R-28-6)"
        );
    }

    #[test]
    fn chance_to_go_free_gv_28_20_21_22() {
        assert_eq!(chance_to_go_free(0, 40, 50), 45);
        assert_eq!(chance_to_go_free(-100, 0, 0), 5);
        assert_eq!(chance_to_go_free(50, 100, 100), 95);
    }

    #[test]
    fn plea_outcome_reaches_all_branches() {
        let mut free = 0;
        let mut modified = 0;
        for seed in 0..128 {
            match plea_outcome(5, PUNISHMENT_FINE_OR_PRISON, -30, &mut DfRandom::srand(seed)) {
                PleaOutcome::Free => free += 1,
                PleaOutcome::FineModified { doubled, halved } => {
                    assert!(!(doubled && halved));
                    modified += 1;
                }
                other => panic!("unexpected outcome {other:?}"),
            }
        }
        assert!(free > 0 && modified > 0);
        assert_eq!(
            plea_outcome(5, PUNISHMENT_SEVERE, -30, &mut DfRandom::srand(3)),
            PleaOutcome::Banished
        );
    }

    #[test]
    fn prison_clock_gv_28_23_24() {
        let mut prison = PrisonState::begin(12);
        let mut ended = false;
        for _ in 0..12 {
            assert!(!ended);
            ended = prison.tick_day();
        }
        assert!(ended);
        assert_eq!(prison.days_left, 0);
        assert_eq!(prison.clock_advanced, 12 * 86_400);
        assert!(prison.prevent_enemy_spawns && prison.prevent_normalizing_reputations);
        assert!(!prison.in_prison);
        assert_eq!(prison.release(), 14_400);
        assert_eq!(prison.clock_advanced, 1_036_800 + 14_400);
        assert_eq!(PrisonState::ui_interval(false), 0.3);
        assert_eq!(PrisonState::ui_interval(true), 0.001);
    }

    #[test]
    fn finish_sentence_clears_the_crime() {
        let mut law = PlayerLaw::new(1);
        law.set_crime(Crimes::Murder, false);
        law.in_prison = true;
        let mut prison = PrisonState::begin(2);
        while !prison.tick_day() {}
        let added = finish_sentence(&mut prison, &mut law);
        assert_eq!(added, 14_400);
        assert_eq!(law.crime, Crimes::None);
        assert!(!law.in_prison);
    }

    #[test]
    fn crime_trigger_census_is_seven_and_loan_default_is_guard_free() {
        assert_eq!(CRIME_TRIGGERS.len(), 7);
        assert_eq!(
            CRIME_TRIGGERS
                .iter()
                .filter(|trigger| trigger.immediate_guards())
                .count(),
            4
        );
        assert_eq!(CrimeTrigger::WeaponKill.crime(), Crimes::Murder);
        assert_eq!(CrimeTrigger::BanishedRegion.crime(), Crimes::CriminalConspiracy);
        const { assert!(!LOAN_DEFAULT_SPAWNS_GUARDS) };
        assert_eq!(loan_default_crime(true, false, false), Some(Crimes::LoanDefault));
        assert_eq!(loan_default_crime(true, true, false), None);
        assert_eq!(loan_default_crime(true, false, true), None, "once per region");
        assert_eq!(loan_default_crime(false, false, false), None);
    }

    #[test]
    fn hourly_crime_needs_a_negative_rep_or_banishment() {
        let mut raised = 0;
        for seed in 0..512 {
            if hourly_crime(0, 0, &mut DfRandom::srand(seed)).is_some() {
                raised += 1;
            }
        }
        assert_eq!(raised, 0);
        let mut by_rep = 0;
        let mut by_ban = 0;
        for seed in 0..512 {
            if hourly_crime(-11, 0, &mut DfRandom::srand(seed)) == Some(Crimes::CriminalConspiracy) {
                by_rep += 1;
            }
            if hourly_crime(0, SEVERE_PUNISHMENT_BANISHED, &mut DfRandom::srand(seed))
                == Some(Crimes::CriminalConspiracy)
            {
                by_ban += 1;
            }
        }
        assert!(by_rep > 0 && by_rep < 512, "SuccessRoll(5)");
        assert!(by_ban > by_rep, "SuccessRoll(10) is more frequent");
    }

    #[test]
    fn quest_crime_validates_against_the_enum() {
        assert_eq!(quest_crime("Murder"), Some(Crimes::Murder));
        assert_eq!(quest_crime("LoanDefault"), Some(Crimes::LoanDefault));
        assert_eq!(quest_crime("Not_A_Crime"), None);
        assert_eq!(quest_crime("5"), None, "the verb pattern is [a-zA-Z_]+");
    }

    #[test]
    fn guard_spawn_requests_follow_the_source_gates() {
        let mut rng = DfRandom::srand(11);
        let dungeon = GuardSpawnContext {
            inside_dungeon: true,
            ..GuardSpawnContext::default()
        };
        assert_eq!(guard_spawn_request(dungeon, &mut rng), None);
        let crowded = GuardSpawnContext {
            active_guards: MAX_ACTIVE_GUARD_SPAWNS + 1,
            immediate: true,
            ..GuardSpawnContext::default()
        };
        assert_eq!(guard_spawn_request(crowded, &mut rng), None);
        let at_cap = GuardSpawnContext {
            active_guards: MAX_ACTIVE_GUARD_SPAWNS,
            immediate: true,
            npcs_in_radius: 3,
            ..GuardSpawnContext::default()
        };
        assert_eq!(
            guard_spawn_request(at_cap, &mut rng),
            Some(GuardSpawnRequest::ImmediateFromPool {
                radius_meters: GUARD_ATTENTION_RADIUS_METERS
            })
        );
        let indoor = GuardSpawnContext {
            inside_building: true,
            ..GuardSpawnContext::default()
        };
        match guard_spawn_request(indoor, &mut rng) {
            Some(GuardSpawnRequest::Interior { guards, door_offset_meters }) => {
                assert!((INTERIOR_GUARD_MIN..=INTERIOR_GUARD_MAX).contains(&guards));
                assert_eq!(door_offset_meters, INTERIOR_DOOR_OFFSET_METERS);
            }
            other => panic!("expected Interior, got {other:?}"),
        }
        let empty_street = GuardSpawnContext {
            immediate: true,
            ..GuardSpawnContext::default()
        };
        match guard_spawn_request(empty_street, &mut rng) {
            Some(GuardSpawnRequest::FoeSpawner { guards, .. }) => {
                assert!((FOE_SPAWNER_GUARD_MIN..=FOE_SPAWNER_GUARD_MAX).contains(&guards));
            }
            other => panic!("expected FoeSpawner, got {other:?}"),
        }
        let guard_sees = GuardSpawnContext {
            seen_by_guard: true,
            ..GuardSpawnContext::default()
        };
        assert!(matches!(
            guard_spawn_request(guard_sees, &mut rng),
            Some(GuardSpawnRequest::ImmediateFromPool { .. })
        ));
        let npc_sees = GuardSpawnContext {
            seen_by_npc: true,
            ..GuardSpawnContext::default()
        };
        match guard_spawn_request(npc_sees, &mut rng) {
            Some(GuardSpawnRequest::Countdown { seconds }) => {
                assert!((GUARD_COUNTDOWN_MIN_SECONDS..=GUARD_COUNTDOWN_MAX_SECONDS).contains(&seconds));
            }
            other => panic!("expected Countdown, got {other:?}"),
        }
        assert_eq!(guard_spawn_request(GuardSpawnContext::default(), &mut rng), None);
    }

    #[test]
    fn guard_hit_drops_reputation_once_and_lands_damage() {
        let first = guard_hit_response(Crimes::Murder, false, true, false);
        assert!(first.lower_legal_rep && first.show_surrender_dialogue);
        assert!(!first.apply_damage);
        let second = guard_hit_response(Crimes::Murder, true, true, false);
        assert!(!second.lower_legal_rep && !second.show_surrender_dialogue);
        let lethal = guard_hit_response(Crimes::Murder, true, false, false);
        assert!(lethal.apply_damage);
        let declined = guard_hit_response(Crimes::None, true, true, true);
        assert!(!declined.lower_legal_rep && declined.apply_damage);
    }

    #[test]
    fn on_pop_resets_state_without_touching_reputation() {
        let mut law = PlayerLaw::new(1);
        let mut region = RegionLaw::new();
        region.set_legal_rep(-40);
        *law.region_mut(0).expect("region") = region;
        law.arrested = true;
        law.in_prison = true;
        law.on_pop();
        assert!(!law.arrested && !law.in_prison);
        assert_eq!(law.region(0).expect("region").legal_rep, -40);
    }
}

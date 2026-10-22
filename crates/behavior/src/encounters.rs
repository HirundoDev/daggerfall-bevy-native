//! Random encounters (spec M25 §5, REQ-L99-058).
//!
//! Port of the observable behavior of `DFU:Assets/Scripts/Utility/
//! RandomEncounters.cs` @ `0038a1e6`: **45 active tables** of **20** enemies
//! (ids `MobileTypes`), the index mapping per context (dungeon / underwater /
//! climate × window / building) and the level range per `Dice100` roll
//! (`:1471-1526`).
//!
//! **Census (spec §5.1):** 45 active tables · 45 of 20 · 0 short · **1
//! commented out** of 11 entries (`// Cemetery - DF Unity version`, `:580-599`,
//! outside the array) · index **38** (`// Unused - Index38`, `:1133`) never
//! referenced · maximum used index **44**.
//!
//! **R-E (length clamp):** `:1516-1524` adds a clamp the classic did not have.
//! With the 45 tables (all of 20) it **never fires**; it is implemented and
//! tested on a synthetic table (`PROBE-ENCOUNTER-001` block A, E9).
//!
//! Pure: 0 Bevy, 0 I/O, 0 global RNG — rolls come in as parameters.

/// Number of active tables (indices 0..44).
pub const TABLE_COUNT: usize = 45;
/// Enemies per table (all active ones: 20).
pub const TABLE_LEN: usize = 20;
/// Entries of the only **commented-out** table (`Cemetery - DF Unity version`).
pub const COMMENTED_TABLE_ENTRIES: usize = 11;
/// Commented-out tables in the source (not part of the array).
pub const COMMENTED_TABLES: usize = 1;
/// Index declared `Unused` in the source (`:1133`) — never referenced.
pub const UNUSED_TABLE_INDEX: usize = 38;
/// Fixed table of `chooseUnderWaterEnemy` (`:1343-1344`).
pub const UNDERWATER_TABLE_INDEX: usize = 19;
/// Dungeon types `DFRegion.DungeonTypes` (0=Crypt … 18=Cemetery).
pub const DUNGEON_TYPE_COUNT: usize = 19;

/// Encounter tables (`EncounterTables`, `RandomEncounters.cs:26-1316`),
/// in index order, with the numeric `MobileTypes`.
pub const ENCOUNTER_TABLES: [[u16; TABLE_LEN]; TABLE_COUNT] = [
    // 0 · Crypt
    [0, 15, 3, 0, 15, 3, 19, 15, 6, 17, 18, 17, 17, 18, 18, 23, 23, 28, 30, 32],
    // 1 · Orc Stronghold
    [0, 3, 7, 12, 4, 7, 15, 7, 12, 5, 16, 21, 6, 21, 24, 20, 27, 20, 21, 24],
    // 2 · Human Stronghold
    [59, 51, 0, 4, 56, 48, 3, 44, 8, 60, 59, 51, 58, 24, 56, 23, 44, 21, 59, 28],
    // 3 · Prison
    [3, 0, 49, 15, 50, 6, 48, 6, 58, 53, 37, 53, 54, 17, 48, 36, 23, 18, 17, 50],
    // 4 · Desecrated Temple
    [3, 1, 47, 55, 46, 15, 13, 55, 19, 55, 21, 22, 23, 27, 29, 55, 25, 26, 34, 31],
    // 5 · Mine
    [0, 3, 4, 6, 15, 6, 48, 7, 12, 16, 53, 16, 59, 38, 36, 26, 53, 28, 48, 30],
    // 6 · Natural Cave
    [0, 3, 4, 5, 4, 6, 7, 9, 58, 13, 14, 16, 20, 21, 18, 59, 55, 58, 34, 32],
    // 7 · Coven
    [1, 3, 43, 46, 13, 48, 10, 37, 35, 36, 10, 27, 46, 44, 29, 26, 45, 43, 25, 31],
    // 8 · Vampire Haunt
    [0, 0, 3, 6, 9, 48, 15, 15, 18, 19, 23, 18, 28, 23, 28, 28, 30, 28, 30, 30],
    // 9 · Laboratory
    [1, 43, 46, 45, 17, 43, 46, 37, 45, 38, 35, 36, 22, 27, 46, 32, 43, 46, 32, 33],
    // 10 · Harpy Nest
    [0, 59, 3, 13, 3, 13, 15, 6, 50, 13, 20, 21, 48, 13, 51, 28, 29, 13, 29, 31],
    // 11 · Ruined Castle
    [0, 3, 59, 7, 15, 6, 12, 9, 60, 14, 17, 16, 60, 18, 23, 59, 60, 32, 33, 30],
    // 12 · Spider Nest
    [0, 3, 3, 6, 15, 2, 57, 6, 20, 51, 13, 6, 53, 18, 19, 6, 23, 6, 54, 32],
    // 13 · Giant Stronghold
    [0, 4, 5, 7, 16, 12, 20, 16, 14, 16, 24, 20, 21, 16, 27, 26, 34, 16, 24, 25],
    // 14 · Dragon's Den
    [0, 3, 8, 50, 20, 9, 60, 13, 6, 22, 46, 10, 48, 43, 60, 28, 45, 53, 33, 31],
    // 15 · Barbarian Stronghold
    [59, 58, 0, 3, 59, 8, 51, 56, 9, 58, 56, 14, 6, 59, 28, 51, 58, 30, 46, 33],
    // 16 · Volcanic Caves
    [3, 1, 5, 5, 58, 35, 13, 14, 16, 18, 20, 36, 35, 43, 36, 23, 27, 26, 31, 33],
    // 17 · Scorpion Nest
    [0, 3, 6, 15, 20, 53, 20, 17, 1, 18, 20, 22, 23, 47, 20, 48, 29, 20, 27, 31],
    // 18 · Cemetery (classic; the DFU variant is commented out in the source)
    [0, 0, 3, 0, 53, 3, 15, 3, 7, 56, 7, 1, 7, 1, 17, 19, 49, 51, 0, 0],
    // 19 · Underwater
    [11, 11, 15, 11, 11, 41, 41, 11, 17, 41, 11, 38, 11, 41, 42, 41, 42, 17, 23, 18],
    // 20 · Desert, in location, night
    [0, 3, 53, 54, 9, 51, 14, 59, 53, 52, 51, 50, 45, 43, 59, 53, 7, 28, 20, 54],
    // 21 · Desert, not in location, day
    [0, 5, 5, 8, 10, 34, 13, 16, 51, 20, 21, 54, 53, 59, 24, 44, 49, 51, 54, 60],
    // 22 · Desert, not in location, night
    [3, 3, 5, 6, 7, 48, 54, 9, 13, 14, 19, 20, 35, 24, 36, 59, 48, 28, 60, 34],
    // 23 · Mountain, in location, night
    [0, 3, 51, 7, 54, 44, 9, 60, 14, 59, 43, 46, 50, 53, 55, 56, 59, 58, 51, 54],
    // 24 · Mountain, not in location, day
    [4, 2, 51, 7, 10, 13, 12, 16, 60, 21, 55, 56, 58, 24, 36, 51, 57, 28, 60, 54],
    // 25 · Mountain, not in location, night
    [3, 4, 2, 8, 54, 57, 9, 34, 12, 16, 18, 21, 46, 22, 45, 23, 44, 28, 57, 60],
    // 26 · Rainforest, in location, night
    [0, 53, 0, 3, 48, 3, 13, 14, 49, 58, 51, 53, 59, 56, 54, 45, 48, 28, 59, 60],
    // 27 · Rainforest, not in location, day
    [0, 4, 5, 6, 53, 13, 51, 16, 57, 20, 59, 60, 53, 58, 57, 56, 52, 49, 43, 44],
    // 28 · Rainforest, not in location, night
    [3, 3, 5, 6, 44, 51, 9, 13, 14, 16, 18, 20, 9, 5, 23, 54, 27, 48, 59, 45],
    // 29 · Subtropical, in location, night
    [0, 50, 53, 55, 46, 49, 51, 59, 48, 53, 52, 57, 56, 28, 58, 59, 43, 44, 54, 60],
    // 30 · Subtropical, not in location, day
    [0, 1, 4, 5, 8, 10, 34, 16, 53, 20, 59, 46, 48, 51, 60, 45, 55, 53, 58, 48],
    // 31 · Subtropical, not in location, night
    [3, 4, 5, 6, 17, 9, 14, 13, 18, 20, 59, 48, 4, 58, 54, 45, 49, 48, 32, 54],
    // 32 · Swamp/woodlands, in location, night
    [0, 3, 50, 49, 51, 56, 9, 52, 14, 59, 45, 50, 55, 51, 48, 54, 53, 28, 60, 45],
    // 33 · Swamp/woodlands, not in location, day
    [0, 4, 2, 7, 8, 34, 12, 14, 16, 56, 21, 49, 58, 24, 45, 54, 57, 51, 60, 44],
    // 34 · Swamp/woodlands, not in location, night
    [3, 4, 6, 17, 48, 60, 9, 34, 14, 16, 57, 22, 44, 55, 49, 51, 57, 48, 60, 46],
    // 35 · Haunted woodlands, in location, night
    [3, 48, 50, 17, 51, 55, 14, 58, 18, 46, 59, 54, 23, 60, 45, 53, 51, 28, 58, 30],
    // 36 · Haunted woodlands, not in location, day
    [1, 4, 2, 6, 8, 10, 34, 13, 16, 46, 21, 22, 60, 24, 36, 45, 54, 56, 57, 60],
    // 37 · Haunted woodlands, not in location, night
    [3, 4, 6, 17, 15, 9, 34, 14, 16, 37, 18, 35, 6, 23, 25, 36, 27, 28, 30, 32],
    // 38 · Unused (never referenced by the mapping)
    [0, 0, 53, 0, 0, 3, 3, 17, 18, 0, 54, 3, 51, 0, 3, 0, 3, 0, 0, 28],
    // 39 · Default building
    [53, 59, 50, 59, 50, 49, 59, 52, 50, 51, 53, 59, 50, 48, 51, 59, 50, 53, 59, 51],
    // 40 · Guildhall
    [43, 1, 45, 47, 48, 44, 43, 46, 45, 47, 35, 44, 43, 46, 45, 48, 44, 43, 29, 45],
    // 41 · Temple
    [55, 55, 60, 55, 55, 55, 55, 60, 55, 55, 55, 55, 55, 60, 55, 55, 55, 55, 55, 60],
    // 42 · Palace, House1
    [59, 56, 49, 44, 60, 59, 60, 59, 56, 49, 44, 60, 59, 60, 59, 56, 44, 60, 59, 60],
    // 43 · House2
    [49, 59, 51, 53, 59, 44, 50, 51, 55, 43, 48, 52, 59, 49, 47, 46, 53, 28, 51, 59],
    // 44 · House3
    [0, 0, 50, 53, 49, 55, 49, 47, 51, 55, 50, 53, 49, 51, 57, 50, 53, 49, 55, 51],
];

/// Table by index (`None` outside `0..45`).
pub fn table(index: usize) -> Option<&'static [u16; TABLE_LEN]> {
    ENCOUNTER_TABLES.get(index)
}

/// Climate band from spec §5.1 (`MapsFile.Climates` collapsed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClimateBand {
    /// `Desert` + `Desert2` (224/225).
    Desert,
    /// `Mountain` (226).
    Mountain,
    /// `Rainforest` (227).
    Rainforest,
    /// `Subtropical` (229).
    Subtropical,
    /// `Swamp` + `MountainWoods` + `Woodlands` (228/230/231).
    SwampWoodlands,
    /// `HauntedWoodlands` (232).
    HauntedWoodlands,
}

impl ClimateBand {
    /// Column of the exterior mapping (`0..6`).
    pub const fn column(self) -> usize {
        match self {
            Self::Desert => 0,
            Self::Mountain => 1,
            Self::Rainforest => 2,
            Self::Subtropical => 3,
            Self::SwampWoodlands => 4,
            Self::HauntedWoodlands => 5,
        }
    }

    /// Band from the `MapsFile.Climates` index (223..232).
    ///
    /// `223` (`Ocean`) and any value outside `223..=232` have no exterior
    /// band (`None` ⇒ the mapping returns `MobileTypes.None` in the source).
    pub const fn from_climate_index(index: u8) -> Option<Self> {
        match index {
            224 | 225 => Some(Self::Desert),
            226 => Some(Self::Mountain),
            227 => Some(Self::Rainforest),
            229 => Some(Self::Subtropical),
            228 | 230 | 231 => Some(Self::SwampWoodlands),
            232 => Some(Self::HauntedWoodlands),
            _ => None,
        }
    }
}

/// Building type of the mapping `:1349-1366`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildingKind {
    /// Any unlisted building ⇒ table 39.
    Other,
    /// `GuildHall` ⇒ 40.
    GuildHall,
    /// `Temple` ⇒ 41.
    Temple,
    /// `Palace` or `House1` ⇒ 42.
    PalaceOrHouse1,
    /// `House2` ⇒ 43.
    House2,
    /// `House3` ⇒ 44.
    House3,
}

impl BuildingKind {
    /// Table index of the mapping (`:1349-1366`).
    pub const fn table_index(self) -> usize {
        match self {
            Self::Other => 39,
            Self::GuildHall => 40,
            Self::Temple => 41,
            Self::PalaceOrHouse1 => 42,
            Self::House2 => 43,
            Self::House3 => 44,
        }
    }
}

/// Player context that decides the table (`ChooseRandomEnemy`, `:1334`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncounterContext {
    /// Underwater ⇒ fixed table 19.
    Underwater,
    /// Inside a dungeon: `DungeonTypes` 0..18 ⇒ the same table.
    Dungeon {
        /// `(int)DFRegion.DungeonTypes` (0=Crypt … 18=Cemetery).
        dungeon_type: u8,
    },
    /// Inside a building.
    Building {
        /// Building type.
        kind: BuildingKind,
    },
    /// Exterior.
    Exterior {
        /// Climate band.
        climate: ClimateBand,
        /// Inside a location rectangle?
        in_location: bool,
        /// Is it daytime?
        is_day: bool,
    },
}

/// Exterior window of the mapping (`20..37`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExteriorWindow {
    /// In location, at night.
    InLocationNight,
    /// Out of location, during the day.
    OutOfLocationDay,
    /// Out of location, at night.
    OutOfLocationNight,
}

impl ExteriorWindow {
    /// Index base for the climate column (`20 + 3·column + offset`).
    pub const fn offset(self) -> usize {
        match self {
            Self::InLocationNight => 0,
            Self::OutOfLocationDay => 1,
            Self::OutOfLocationNight => 2,
        }
    }
}

/// Table index for a context (`None` when the source returns
/// `MobileTypes.None`: daytime inside a location).
pub fn table_index(context: EncounterContext) -> Option<usize> {
    match context {
        EncounterContext::Underwater => Some(UNDERWATER_TABLE_INDEX),
        EncounterContext::Dungeon { dungeon_type } => {
            (usize::from(dungeon_type) < DUNGEON_TYPE_COUNT).then_some(usize::from(dungeon_type))
        }
        EncounterContext::Building { kind } => Some(kind.table_index()),
        EncounterContext::Exterior {
            climate,
            in_location,
            is_day,
        } => {
            let window = match (in_location, is_day) {
                // `:1396-1401`: in location and daytime, there is no encounter.
                (true, true) => return None,
                (true, false) => ExteriorWindow::InLocationNight,
                (false, true) => ExteriorWindow::OutOfLocationDay,
                (false, false) => ExteriorWindow::OutOfLocationNight,
            };
            Some(20 + 3 * climate.column() + window.offset())
        }
    }
}

/// Tables referenced by the mapping (block B census).
///
/// Walks every reachable context: dungeons 0..18, underwater,
/// the 3 exterior windows × 6 climate bands and the 6 building types.
pub fn referenced_tables() -> [bool; TABLE_COUNT] {
    let mut referenced = [false; TABLE_COUNT];
    for dungeon_type in 0..DUNGEON_TYPE_COUNT as u8 {
        if let Some(index) = table_index(EncounterContext::Dungeon { dungeon_type }) {
            referenced[index] = true;
        }
    }
    if let Some(index) = table_index(EncounterContext::Underwater) {
        referenced[index] = true;
    }
    for climate in [
        ClimateBand::Desert,
        ClimateBand::Mountain,
        ClimateBand::Rainforest,
        ClimateBand::Subtropical,
        ClimateBand::SwampWoodlands,
        ClimateBand::HauntedWoodlands,
    ] {
        for (in_location, is_day) in [(true, false), (false, true), (false, false)] {
            if let Some(index) = table_index(EncounterContext::Exterior {
                climate,
                in_location,
                is_day,
            }) {
                referenced[index] = true;
            }
        }
    }
    for kind in [
        BuildingKind::Other,
        BuildingKind::GuildHall,
        BuildingKind::Temple,
        BuildingKind::PalaceOrHouse1,
        BuildingKind::House2,
        BuildingKind::House3,
    ] {
        if let Some(index) = table_index(EncounterContext::Building { kind }) {
            referenced[index] = true;
        }
    }
    referenced
}

/// Level range per roll (`RandomEncounterTable`, `:1471-1526`).
///
/// `roll > 80` ⇒ `[0, level+1]`; `roll > 95` ⇒ `[0, level+2]` if `level ≤ 5`
/// and `[0, 19]` otherwise; base branch `roll ≤ 80` ⇒ `[level−3, level+3]`. Then:
/// `min < 0` ⇒ `[0, 5]` and `max > 19` ⇒ `[14, 19]`.
pub fn level_range(player_level: i32, roll: i32) -> (i32, i32) {
    let (mut min, mut max) = if roll > 80 {
        if roll > 95 {
            if player_level <= 5 {
                (0, player_level + 2)
            } else {
                (0, 19)
            }
        } else {
            (0, player_level + 1)
        }
    } else {
        (player_level - 3, player_level + 3)
    };
    if min < 0 {
        min = 0;
        max = 5;
    }
    if max > 19 {
        min = 14;
        max = 19;
    }
    (min, max)
}

/// Result of the per-table-length clamp DFU added (`:1516-1524`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableClamp {
    /// The table covers the range: it is left untouched.
    Fits {
        /// `min` of the range.
        min: i32,
        /// `max` of the range.
        max: i32,
    },
    /// `max` was trimmed to `len − 1` and `min` to `max − 5`.
    Clamped {
        /// Trimmed `min`.
        min: i32,
        /// Trimmed `max`.
        max: i32,
    },
    /// `max < 5`: the source draws `Random.Range(0, max)` (int, `max`
    /// exclusive) for the `min` — the caller supplies the roll.
    NeedsDraw {
        /// Trimmed `max`.
        max: i32,
    },
}

/// Clamp by table length (`:1516-1524`), without consuming rolls.
pub fn clamp_to_table(len: usize, min: i32, max: i32) -> TableClamp {
    if max < len as i32 {
        return TableClamp::Fits { min, max };
    }
    let max = len as i32 - 1;
    if max >= 5 {
        TableClamp::Clamped { min: max - 5, max }
    } else {
        TableClamp::NeedsDraw { max }
    }
}

/// Picks an enemy from the table (full `ChooseRandomEnemy`, `:1471-1526`).
///
/// `draw(low, high)` must return an **inclusive** integer in `[low, high]`
/// (the equivalent of the source's `Random.Range`). `None` if the table is
/// empty.
pub fn choose_enemy(
    table: &[u16],
    player_level: i32,
    roll: i32,
    mut draw: impl FnMut(i32, i32) -> i32,
) -> Option<u16> {
    if table.is_empty() {
        return None;
    }
    let (min, max) = level_range(player_level, roll);
    let (min, max) = match clamp_to_table(table.len(), min, max) {
        TableClamp::Fits { min, max } => (min, max),
        TableClamp::Clamped { min, max } => (min, max),
        // `Random.Range(0, max)` int overload ⇒ `0..max-1` inclusive.
        TableClamp::NeedsDraw { max } => (draw(0, (max - 1).max(0)), max),
    };
    let index = draw(min, max).clamp(0, table.len() as i32 - 1) as usize;
    table.get(index).copied()
}

/// Denominators of the spawn rolls (spec §5.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnContext {
    /// In location, at night ⇒ `1/24` (`:1749-1756`).
    LocationNight,
    /// Wilderness at night ⇒ `1/24` (`:1775-1782`).
    WildernessNight,
    /// Wilderness during the day ⇒ `1/36` (`:1762-1769`).
    WildernessDay,
    /// Dungeon ⇒ `1/36` **only** with an alarm (`:1788-1797`).
    Dungeon,
}

impl SpawnContext {
    /// Roll divisor (`Random.Range(0, divisor) == 0`).
    pub const fn divisor(self) -> u32 {
        match self {
            Self::LocationNight | Self::WildernessNight => 24,
            Self::WildernessDay | Self::Dungeon => 36,
        }
    }

    /// Exact spawn probability (`1/divisor`).
    pub fn chance(self) -> f64 {
        1.0 / f64::from(self.divisor())
    }
}

/// Spawn roll for exterior/interior contexts
/// (`roll ∈ 0..divisor`, success on `roll == 0`).
pub fn roll_spawn(context: SpawnContext, roll: u32) -> bool {
    match context {
        // Without an alarm the source returns `1` (no spawn) without rolling.
        SpawnContext::Dungeon => false,
        _ => roll == 0,
    }
}

/// Dungeon spawn roll (`RollRandomSpawn_Dungeon`, `:1788-1797`).
///
/// Without `PlayerEntity.EnemyAlertActive` it returns `1` (no spawn); with an
/// alarm, `Random.Range(0, 36) == 0`.
pub fn roll_spawn_dungeon(alarm_active: bool, roll: u32) -> bool {
    alarm_active && roll == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn census_matches_the_spec() {
        assert_eq!(ENCOUNTER_TABLES.len(), TABLE_COUNT);
        assert!(ENCOUNTER_TABLES.iter().all(|table| table.len() == TABLE_LEN));
        let referenced = referenced_tables();
        let used: Vec<usize> = referenced
            .iter()
            .enumerate()
            .filter(|(_, hit)| **hit)
            .map(|(index, _)| index)
            .collect();
        assert_eq!(used.len(), TABLE_COUNT - 1);
        assert_eq!(*used.iter().max().unwrap(), 44);
        assert!(!referenced[UNUSED_TABLE_INDEX]);
        const { assert!(COMMENTED_TABLE_ENTRIES < TABLE_LEN) };
    }

    #[test]
    fn table_index_mapping_covers_every_window() {
        assert_eq!(
            table_index(EncounterContext::Underwater),
            Some(UNDERWATER_TABLE_INDEX)
        );
        assert_eq!(
            table_index(EncounterContext::Dungeon { dungeon_type: 0 }),
            Some(0)
        );
        assert_eq!(
            table_index(EncounterContext::Dungeon { dungeon_type: 18 }),
            Some(18)
        );
        assert_eq!(
            table_index(EncounterContext::Dungeon { dungeon_type: 19 }),
            None
        );
        // Desert: 20 in location at night, 21 outside in the day, 22 outside at night.
        for (climate, base) in [
            (ClimateBand::Desert, 20),
            (ClimateBand::HauntedWoodlands, 35),
        ] {
            assert_eq!(
                table_index(EncounterContext::Exterior {
                    climate,
                    in_location: true,
                    is_day: false
                }),
                Some(base)
            );
            assert_eq!(
                table_index(EncounterContext::Exterior {
                    climate,
                    in_location: false,
                    is_day: true
                }),
                Some(base + 1)
            );
            assert_eq!(
                table_index(EncounterContext::Exterior {
                    climate,
                    in_location: false,
                    is_day: false
                }),
                Some(base + 2)
            );
        }
        assert_eq!(
            table_index(EncounterContext::Exterior {
                climate: ClimateBand::Desert,
                in_location: true,
                is_day: true
            }),
            None
        );
        assert_eq!(ClimateBand::from_climate_index(223), None);
        assert_eq!(
            ClimateBand::from_climate_index(228),
            Some(ClimateBand::SwampWoodlands)
        );
        assert_eq!(
            ClimateBand::from_climate_index(232),
            Some(ClimateBand::HauntedWoodlands)
        );
        assert_eq!(
            table_index(EncounterContext::Building {
                kind: BuildingKind::GuildHall
            }),
            Some(40)
        );
        assert_eq!(
            table_index(EncounterContext::Building {
                kind: BuildingKind::Other
            }),
            Some(39)
        );
    }

    #[test]
    fn level_range_matches_goldens_e1_to_e8() {
        assert_eq!(level_range(5, 50), (2, 8)); // E1
        assert_eq!(level_range(10, 50), (7, 13)); // E2
        assert_eq!(level_range(2, 85), (0, 3)); // E3
        assert_eq!(level_range(5, 85), (0, 6)); // E4
        assert_eq!(level_range(5, 97), (0, 7)); // E5
        assert_eq!(level_range(8, 97), (0, 19)); // E6
        assert_eq!(level_range(1, 50), (0, 5)); // E7
        assert_eq!(level_range(19, 85), (14, 19)); // E8
    }

    #[test]
    fn table_clamp_matches_goldens_e9_e10() {
        // E9: synthetic table of 11 entries, max = 12 ⇒ max 10, min 5.
        assert_eq!(
            clamp_to_table(11, 0, 12),
            TableClamp::Clamped { min: 5, max: 10 }
        );
        // E10: table of 20, max = 19 ⇒ no clamp.
        assert_eq!(
            clamp_to_table(20, 14, 19),
            TableClamp::Fits { min: 14, max: 19 }
        );
        // `max < 5` branch (only reachable with a table < 6): caller roll.
        assert_eq!(clamp_to_table(4, 0, 4), TableClamp::NeedsDraw { max: 3 });
        // With the 45 active tables the clamp never fires.
        for table in ENCOUNTER_TABLES.iter() {
            for level in 0..=30 {
                for roll in 1..=100 {
                    let (min, max) = level_range(level, roll);
                    assert!(matches!(
                        clamp_to_table(table.len(), min, max),
                        TableClamp::Fits { .. }
                    ));
                }
            }
        }
    }

    #[test]
    fn choose_enemy_is_deterministic_and_bounded() {
        let table = ENCOUNTER_TABLES[19];
        let mut picks = Vec::new();
        for _ in 0..64 {
            picks.push(choose_enemy(&table, 10, 50, |low, high| low.max(high)).unwrap());
        }
        assert!(picks.iter().all(|id| table.contains(id)));
        // The E9 synthetic clamp picks within [5,10].
        let synthetic: Vec<u16> = (0..11).collect();
        let picked = choose_enemy(&synthetic, 20, 97, |low, high| {
            assert!(low >= 5 && high <= 10, "range {low}..={high}");
            low
        })
        .unwrap();
        assert_eq!(picked, 5);
        assert_eq!(choose_enemy(&[], 5, 50, |low, _| low), None);
    }

    #[test]
    fn spawn_probabilities_match_the_spec() {
        assert!((SpawnContext::LocationNight.chance() - 1.0 / 24.0).abs() < 1e-12);
        assert!((SpawnContext::WildernessDay.chance() - 1.0 / 36.0).abs() < 1e-12);
        assert!(roll_spawn(SpawnContext::LocationNight, 0));
        assert!(!roll_spawn(SpawnContext::LocationNight, 1));
        // Dungeon: never without an alarm; 1/36 with an alarm.
        assert!(!roll_spawn_dungeon(false, 0));
        assert!(roll_spawn_dungeon(true, 0));
        assert!(!roll_spawn_dungeon(true, 1));
    }
}

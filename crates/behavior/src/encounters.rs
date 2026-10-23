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
//! **`MobileTypes` ids (R-A):** the tables store the **enum ids** of the DFU
//! source, exactly as written there — monsters `0..=42` and humanoids
//! `128..=146` (`DaggerfallUnityEnums.cs:142-212`). 393 of the 900 cells are
//! humanoids. No id is ever remapped or clamped: every cell is resolved
//! through [`resolve_mobile`], which returns a typed [`MobileRef`] and an
//! [`EncounterTableError`] otherwise. [`validate_tables`] proves 900/900.
//!
//! A humanoid has **no `ENEMY###.CFG` record**: `MONSTER.BSA` holds 43 `ENEMY`
//! records = `CLASS.CFG` records of the 43 monsters (`MonsterFile.cs:46-47`),
//! while humanoid classes come from the career data of M20. [`MobileRef::Humanoid`]
//! therefore carries a *class id*, never an `ENEMY` record.
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

/// Encounter tables (`EncounterTables`, `RandomEncounters.cs:26-1328`),
/// in index order, with the **true** numeric `MobileTypes` ids
/// (`DaggerfallUnityEnums.cs:139-212`): monsters `0..=42`, humanoids
/// `128..=146`.
///
/// Every cell cites the `// <name> - IndexN` marker of its table in the DFU
/// source; the values are the enum ids of the `MobileTypes.<Name>` entries
/// listed there. **Census:** 45 tables × 20 = 900 cells, of which **393 are
/// humanoids** (`>= 128`) spread over **40 tables** (tables 0, 1, 13, 19 and
/// 37 are monster-only). [`validate_tables`] proves all 900 resolve.
pub const ENCOUNTER_TABLES: [[u16; TABLE_LEN]; TABLE_COUNT] = [
    // 0 · Crypt (`RandomEncounters.cs:30`)
    [0, 15, 3, 0, 15, 3, 19, 15, 6, 17, 18, 17, 17, 18, 18, 23, 23, 28, 30, 32],
    // 1 · Orc Stronghold (`RandomEncounters.cs:59`)
    [0, 3, 7, 12, 4, 7, 15, 7, 12, 5, 16, 21, 6, 21, 24, 20, 27, 20, 21, 24],
    // 2 · Human Stronghold (`RandomEncounters.cs:88`)
    [144, 136, 0, 4, 141, 133, 3, 129, 8, 145, 144, 136, 143, 24, 141, 23, 129, 21, 144, 28],
    // 3 · Prison (`RandomEncounters.cs:117`)
    [3, 0, 134, 15, 135, 6, 133, 6, 143, 138, 37, 138, 139, 17, 133, 36, 23, 18, 17, 135],
    // 4 · Desecrated Temple (`RandomEncounters.cs:146`)
    [3, 1, 132, 140, 131, 15, 13, 140, 19, 140, 21, 22, 23, 27, 29, 140, 25, 26, 34, 31],
    // 5 · Mine (`RandomEncounters.cs:175`)
    [0, 3, 4, 6, 15, 6, 133, 7, 12, 16, 138, 16, 144, 38, 36, 26, 138, 28, 133, 30],
    // 6 · Natural Cave (`RandomEncounters.cs:204`)
    [0, 3, 4, 5, 4, 6, 7, 9, 143, 13, 14, 16, 20, 21, 18, 144, 140, 143, 34, 32],
    // 7 · Coven (`RandomEncounters.cs:233`)
    [1, 3, 128, 131, 13, 133, 10, 37, 35, 36, 10, 27, 131, 129, 29, 26, 130, 128, 25, 31],
    // 8 · Vampire Haunt (`RandomEncounters.cs:262`)
    [0, 0, 3, 6, 9, 133, 15, 15, 18, 19, 23, 18, 28, 23, 28, 28, 30, 28, 30, 30],
    // 9 · Laboratory (`RandomEncounters.cs:291`)
    [1, 128, 131, 130, 17, 128, 131, 37, 130, 38, 35, 36, 22, 27, 131, 32, 128, 131, 32, 33],
    // 10 · Harpy Nest (`RandomEncounters.cs:320`)
    [0, 144, 3, 13, 3, 13, 15, 6, 135, 13, 20, 21, 133, 13, 136, 28, 29, 13, 29, 31],
    // 11 · Ruined Castle (`RandomEncounters.cs:349`)
    [0, 3, 144, 7, 15, 6, 12, 9, 145, 14, 17, 16, 145, 18, 23, 144, 145, 32, 33, 30],
    // 12 · Spider Nest (`RandomEncounters.cs:378`)
    [0, 3, 3, 6, 15, 2, 142, 6, 20, 136, 13, 6, 138, 18, 19, 6, 23, 6, 139, 32],
    // 13 · Giant Stronghold (`RandomEncounters.cs:407`)
    [0, 4, 5, 7, 16, 12, 20, 16, 14, 16, 24, 20, 21, 16, 27, 26, 34, 16, 24, 25],
    // 14 · Dragon's Den (`RandomEncounters.cs:436`)
    [0, 3, 8, 135, 20, 9, 145, 13, 6, 22, 131, 10, 133, 128, 145, 28, 130, 138, 33, 31],
    // 15 · Barbarian Stronghold (`RandomEncounters.cs:465`)
    [144, 143, 0, 3, 144, 8, 136, 141, 9, 143, 141, 14, 6, 144, 28, 136, 143, 30, 131, 33],
    // 16 · Volcanic Caves (`RandomEncounters.cs:494`)
    [3, 1, 5, 5, 143, 35, 13, 14, 16, 18, 20, 36, 35, 128, 36, 23, 27, 26, 31, 33],
    // 17 · Scorpion Nest (`RandomEncounters.cs:523`)
    [0, 3, 6, 15, 20, 138, 20, 17, 1, 18, 20, 22, 23, 132, 20, 133, 29, 20, 27, 31],
    // 18 · Cemetery (`RandomEncounters.cs:552`) — classic: the `Cemetery - DF
    // Unity version` variant is commented out (`:580-599`) and out of the array.
    [0, 0, 3, 0, 138, 3, 15, 3, 7, 141, 7, 1, 7, 1, 17, 19, 134, 136, 0, 0],
    // 19 · Underwater (`RandomEncounters.cs:601`)
    [11, 11, 15, 11, 11, 41, 41, 11, 17, 41, 11, 38, 11, 41, 42, 41, 42, 17, 23, 18],
    // 20 · Desert, in location, night (`RandomEncounters.cs:629`)
    [0, 3, 138, 139, 9, 136, 14, 144, 138, 137, 136, 135, 130, 128, 144, 138, 7, 28, 20, 139],
    // 21 · Desert, not in location, day (`RandomEncounters.cs:657`)
    [0, 5, 5, 8, 10, 34, 13, 16, 136, 20, 21, 139, 138, 144, 24, 129, 134, 136, 139, 145],
    // 22 · Desert, not in location, night (`RandomEncounters.cs:685`)
    [3, 3, 5, 6, 7, 133, 139, 9, 13, 14, 19, 20, 35, 24, 36, 144, 133, 28, 145, 34],
    // 23 · Mountain, in location, night (`RandomEncounters.cs:713`)
    [0, 3, 136, 7, 139, 129, 9, 145, 14, 144, 128, 131, 135, 138, 140, 141, 144, 143, 136, 139],
    // 24 · Mountain, not in location, day (`RandomEncounters.cs:741`)
    [4, 2, 136, 7, 10, 13, 12, 16, 145, 21, 140, 141, 143, 24, 36, 136, 142, 28, 145, 139],
    // 25 · Mountain, not in location, night (`RandomEncounters.cs:769`)
    [3, 4, 2, 8, 139, 142, 9, 34, 12, 16, 18, 21, 131, 22, 130, 23, 129, 28, 142, 145],
    // 26 · Rainforest, in location, night (`RandomEncounters.cs:797`)
    [0, 138, 0, 3, 133, 3, 13, 14, 134, 143, 136, 138, 144, 141, 139, 130, 133, 28, 144, 145],
    // 27 · Rainforest, not in location, day (`RandomEncounters.cs:825`)
    [0, 4, 5, 6, 138, 13, 136, 16, 142, 20, 144, 145, 138, 143, 142, 141, 137, 134, 128, 129],
    // 28 · Rainforest, not in location, night (`RandomEncounters.cs:853`)
    [3, 3, 5, 6, 129, 136, 9, 13, 14, 16, 18, 20, 9, 5, 23, 139, 27, 133, 144, 130],
    // 29 · Subtropical, in location, night (`RandomEncounters.cs:881`)
    [0, 135, 138, 140, 131, 134, 136, 144, 133, 138, 137, 142, 141, 28, 143, 144, 128, 129, 139, 145],
    // 30 · Subtropical, not in location, day (`RandomEncounters.cs:909`)
    [0, 1, 4, 5, 8, 10, 34, 16, 138, 20, 144, 131, 133, 136, 145, 130, 140, 138, 143, 133],
    // 31 · Subtropical, not in location, night (`RandomEncounters.cs:937`)
    [3, 4, 5, 6, 17, 9, 14, 13, 18, 20, 144, 133, 4, 143, 139, 130, 134, 133, 32, 139],
    // 32 · Swamp/woodlands, in location, night (`RandomEncounters.cs:965`)
    [0, 3, 135, 134, 136, 141, 9, 137, 14, 144, 130, 135, 140, 136, 133, 139, 138, 28, 145, 130],
    // 33 · Swamp/woodlands, not in location, day (`RandomEncounters.cs:993`)
    [0, 4, 2, 7, 8, 34, 12, 14, 16, 141, 21, 134, 143, 24, 130, 139, 142, 136, 145, 129],
    // 34 · Swamp/woodlands, not in location, night (`RandomEncounters.cs:1021`)
    [3, 4, 6, 17, 133, 145, 9, 34, 14, 16, 142, 22, 129, 140, 134, 136, 142, 133, 145, 131],
    // 35 · Haunted woodlands, in location, night (`RandomEncounters.cs:1049`)
    [3, 133, 135, 17, 136, 140, 14, 143, 18, 131, 144, 139, 23, 145, 130, 138, 136, 28, 143, 30],
    // 36 · Haunted woodlands, not in location, day (`RandomEncounters.cs:1077`)
    [1, 4, 2, 6, 8, 10, 34, 13, 16, 131, 21, 22, 145, 24, 36, 130, 139, 141, 142, 145],
    // 37 · Haunted woodlands, not in location, night (`RandomEncounters.cs:1105`)
    [3, 4, 6, 17, 15, 9, 34, 14, 16, 37, 18, 35, 6, 23, 25, 36, 27, 28, 30, 32],
    // 38 · Unused - Index38, never referenced (`RandomEncounters.cs:1133`)
    [0, 0, 138, 0, 0, 3, 3, 17, 18, 0, 139, 3, 136, 0, 3, 0, 3, 0, 0, 28],
    // 39 · Default building (`RandomEncounters.cs:1161`)
    [138, 144, 135, 144, 135, 134, 144, 137, 135, 136, 138, 144, 135, 133, 136, 144, 135, 138, 144, 136],
    // 40 · Guildhall (`RandomEncounters.cs:1189`)
    [128, 1, 130, 132, 133, 129, 128, 131, 130, 132, 35, 129, 128, 131, 130, 133, 129, 128, 29, 130],
    // 41 · Temple (`RandomEncounters.cs:1217`)
    [140, 140, 145, 140, 140, 140, 140, 145, 140, 140, 140, 140, 140, 145, 140, 140, 140, 140, 140, 145],
    // 42 · Palace, House1 (`RandomEncounters.cs:1245`)
    [144, 141, 134, 129, 145, 144, 145, 144, 141, 134, 129, 145, 144, 145, 144, 141, 129, 145, 144, 145],
    // 43 · House2 (`RandomEncounters.cs:1273`)
    [134, 144, 136, 138, 144, 129, 135, 136, 140, 128, 133, 137, 144, 134, 132, 131, 138, 28, 136, 144],
    // 44 · House3 (`RandomEncounters.cs:1301`)
    [0, 0, 135, 138, 134, 140, 134, 132, 136, 140, 135, 138, 134, 136, 142, 135, 138, 134, 140, 136],
];

/// Table by index (`None` outside `0..45`).
pub fn table(index: usize) -> Option<&'static [u16; TABLE_LEN]> {
    ENCOUNTER_TABLES.get(index)
}

/// Last monster `MobileTypes` id (`Lamia`, `DaggerfallUnityEnums.cs:187`).
pub const MONSTER_MOBILE_END: u16 = 42;
/// First humanoid `MobileTypes` id (`Mage = 128`, `DaggerfallUnityEnums.cs:190`).
pub const HUMANOID_MOBILE_START: u16 = 128;
/// Last humanoid `MobileTypes` id (`Knight_CityWatch`,
/// `DaggerfallUnityEnums.cs:208`).
pub const HUMANOID_MOBILE_END: u16 = 146;
/// Humanoid classes `128..=146` (`DaggerfallUnityEnums.cs:190-208`).
pub const HUMANOID_CLASS_COUNT: u8 = 19;

/// Typed `MobileTypes` reference of an encounter cell.
///
/// The distinction is data, not decoration: a monster has an
/// `ENEMY{mobile:03}.CFG` record in `MONSTER.BSA`, a humanoid does **not**
/// (`MonsterFile.cs:46-47`: the 43 `ENEMY` records are the `CLASS.CFG`
/// records); its class/level/HP come from the career data of M20.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MobileRef {
    /// Monster `0..=42`: `ENEMY{mobile:03}.CFG` (`record` = the same number,
    /// `MonsterFile.cs:159-183`).
    Monster {
        /// `MobileTypes` id.
        mobile: u16,
        /// `ENEMY###.CFG` record number (== `mobile`).
        record: usize,
    },
    /// Humanoid `128..=146`: **no** `ENEMY###.CFG`; the class comes from the
    /// career data (M20 boundary).
    Humanoid {
        /// `MobileTypes` id (`128..=146`).
        mobile: u16,
        /// Class slot `0..=18` (`mobile − 128`: `Mage` … `Knight_CityWatch`).
        class_id: u8,
    },
}

impl MobileRef {
    /// `MobileTypes` id of the cell (always the table value, never remapped).
    pub const fn mobile(self) -> u16 {
        match self {
            Self::Monster { mobile, .. } | Self::Humanoid { mobile, .. } => mobile,
        }
    }

    /// `ENEMY###.CFG` record — `Some` for monsters only (`MonsterFile.cs:46-47`).
    pub const fn enemy_record(self) -> Option<usize> {
        match self {
            Self::Monster { record, .. } => Some(record),
            Self::Humanoid { .. } => None,
        }
    }

    /// Class slot `0..=18` — `Some` for humanoids only.
    pub const fn class_id(self) -> Option<u8> {
        match self {
            Self::Humanoid { class_id, .. } => Some(class_id),
            Self::Monster { .. } => None,
        }
    }

    /// Is it a humanoid (M20 career data, no `ENEMY###.CFG`)?
    pub const fn is_humanoid(self) -> bool {
        matches!(self, Self::Humanoid { .. })
    }
}

/// A table cell that cannot be resolved to a `MobileTypes` id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncounterTableError {
    /// The id is neither a monster (`0..=42`) nor a humanoid (`128..=146`).
    ///
    /// No clamp and no remap: the value is reported exactly as stored.
    UnknownMobileType {
        /// Offending id.
        mobile: u16,
    },
    /// Table index outside `0..TABLE_COUNT`.
    TableOutOfBounds {
        /// Requested index.
        index: usize,
    },
    /// Cell index outside `0..TABLE_LEN`.
    CellOutOfBounds {
        /// Table index.
        table: usize,
        /// Requested cell.
        cell: usize,
    },
}

impl std::fmt::Display for EncounterTableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownMobileType { mobile } => write!(
                f,
                "MobileTypes {mobile} outside 0..=42 / 128..=146 (no clamp, no remap)"
            ),
            Self::TableOutOfBounds { index } => {
                write!(f, "table {index} outside 0..{}", TABLE_COUNT)
            }
            Self::CellOutOfBounds { table, cell } => write!(
                f,
                "cell {cell} outside 0..{TABLE_LEN} of table {table}"
            ),
        }
    }
}

impl std::error::Error for EncounterTableError {}

/// Resolves one `MobileTypes` id of a table cell (`RandomEncounters.cs`
/// `Enemies` arrays, `DaggerfallUnityEnums.cs:142-212`).
///
/// Accepts only the two bands the source uses; anything else is a typed
/// error — the value is never clamped into a neighbouring band.
pub fn resolve_mobile(mobile: u16) -> Result<MobileRef, EncounterTableError> {
    match mobile {
        0..=MONSTER_MOBILE_END => Ok(MobileRef::Monster {
            mobile,
            record: usize::from(mobile),
        }),
        HUMANOID_MOBILE_START..=HUMANOID_MOBILE_END => Ok(MobileRef::Humanoid {
            mobile,
            class_id: (mobile - HUMANOID_MOBILE_START) as u8,
        }),
        other => Err(EncounterTableError::UnknownMobileType { mobile: other }),
    }
}

/// Resolves the cell `(table, cell)` of [`ENCOUNTER_TABLES`].
pub fn resolve_cell(table: usize, cell: usize) -> Result<MobileRef, EncounterTableError> {
    let row = ENCOUNTER_TABLES
        .get(table)
        .ok_or(EncounterTableError::TableOutOfBounds { index: table })?;
    let mobile = *row
        .get(cell)
        .ok_or(EncounterTableError::CellOutOfBounds { table, cell })?;
    resolve_mobile(mobile)
}

/// Validates the whole constant: 45 × 20 = 900 cells, all resolvable.
pub fn validate_tables() -> Result<(), EncounterTableError> {
    for (table, row) in ENCOUNTER_TABLES.iter().enumerate() {
        for cell in 0..row.len() {
            resolve_cell(table, cell)?;
        }
    }
    Ok(())
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
/// (the equivalent of the source's `Random.Range`). `Ok(None)` if the table
/// is empty.
///
/// Every drawn cell goes through [`resolve_mobile`]: the result is a typed
/// [`MobileRef`] (monster → `ENEMY###.CFG`; humanoid → class id, no record),
/// and a cell outside the two `MobileTypes` bands is an
/// [`EncounterTableError`] instead of a raw id the caller could mis-index.
pub fn choose_enemy(
    table: &[u16],
    player_level: i32,
    roll: i32,
    mut draw: impl FnMut(i32, i32) -> i32,
) -> Result<Option<MobileRef>, EncounterTableError> {
    if table.is_empty() {
        return Ok(None);
    }
    let (min, max) = level_range(player_level, roll);
    let (min, max) = match clamp_to_table(table.len(), min, max) {
        TableClamp::Fits { min, max } => (min, max),
        TableClamp::Clamped { min, max } => (min, max),
        // `Random.Range(0, max)` int overload ⇒ `0..max-1` inclusive.
        TableClamp::NeedsDraw { max } => (draw(0, (max - 1).max(0)), max),
    };
    let index = draw(min, max).clamp(0, table.len() as i32 - 1) as usize;
    table.get(index).copied().map(resolve_mobile).transpose()
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
            picks.push(
                choose_enemy(&table, 10, 50, |low, high| low.max(high))
                    .unwrap()
                    .unwrap(),
            );
        }
        assert!(picks.iter().all(|pick| table.contains(&pick.mobile())));
        // The E9 synthetic clamp picks within [5,10].
        let synthetic: Vec<u16> = (0..11).collect();
        let picked = choose_enemy(&synthetic, 20, 97, |low, high| {
            assert!(low >= 5 && high <= 10, "range {low}..={high}");
            low
        })
        .unwrap()
        .unwrap();
        assert_eq!(picked, MobileRef::Monster { mobile: 5, record: 5 });
        assert_eq!(choose_enemy(&[], 5, 50, |low, _| low), Ok(None));
    }

    #[test]
    fn every_cell_resolves_to_a_true_mobile_type() {
        assert_eq!(validate_tables(), Ok(()));
        let mut cells = 0usize;
        let mut humanoids = 0usize;
        let mut tables_with_humanoids = 0usize;
        for (index, row) in ENCOUNTER_TABLES.iter().enumerate() {
            let mut table_humanoids = 0usize;
            for (cell, value) in row.iter().copied().enumerate() {
                let resolved = resolve_cell(index, cell).expect("cell resolves");
                assert_eq!(resolved.mobile(), value, "table {index} cell {cell}");
                cells += 1;
                if resolved.is_humanoid() {
                    humanoids += 1;
                    table_humanoids += 1;
                }
            }
            if table_humanoids > 0 {
                tables_with_humanoids += 1;
            }
        }
        assert_eq!(cells, TABLE_COUNT * TABLE_LEN);
        assert_eq!(cells, 900);
        assert_eq!(humanoids, 393);
        assert_eq!(tables_with_humanoids, 40);
        // Tables that are monster-only (checked cell by cell).
        let monster_only: Vec<usize> = ENCOUNTER_TABLES
            .iter()
            .enumerate()
            .filter(|(_, row)| row.iter().all(|mobile| *mobile <= MONSTER_MOBILE_END))
            .map(|(index, _)| index)
            .collect();
        assert_eq!(monster_only, vec![0, 1, 13, 19, 37]);
        // Every stored id is inside the two bands: the band check is the
        // inverse of `resolve_mobile` and covers ids the loop above would
        // have rejected.
        assert!(ENCOUNTER_TABLES.iter().flatten().all(|mobile| {
            *mobile <= MONSTER_MOBILE_END
                || (*mobile >= HUMANOID_MOBILE_START && *mobile <= HUMANOID_MOBILE_END)
        }));
    }

    #[test]
    fn humanoid_ids_match_the_dfu_source_per_table() {
        // Literal `MobileTypes.<Name>` ids of `RandomEncounters.cs`, read from
        // the source table by table (5 spot-checks, table 2 included).
        // 2 · Human Stronghold (`:88-115`).
        assert_eq!(
            ENCOUNTER_TABLES[2],
            [
                144, 136, 0, 4, 141, 133, 3, 129, 8, 145, 144, 136, 143, 24, 141, 23, 129, 21,
                144, 28
            ],
            "Warrior Rogue Rat GrizzlyBear Archer Nightblade GiantBat Spellsword Centaur Knight \
             Warrior Rogue Barbarian OrcWarlord Archer Wraith Spellsword OrcShaman Warrior Vampire"
        );
        // 7 · Coven (`:233-260`): Mage Spellsword Sorcerer Nightblade Battlemage...
        assert_eq!(
            ENCOUNTER_TABLES[7],
            [
                1, 3, 128, 131, 13, 133, 10, 37, 35, 36, 10, 27, 131, 129, 29, 26, 130, 128, 25,
                31
            ]
        );
        // 39 · Default building (`:1161-1188`): Thief Warrior Burglar...
        assert_eq!(
            ENCOUNTER_TABLES[39],
            [
                138, 144, 135, 144, 135, 134, 144, 137, 135, 136, 138, 144, 135, 133, 136, 144,
                135, 138, 144, 136
            ]
        );
        // 41 · Temple (`:1217-1244`): Monk ×20 except Knight at 2, 7, 13, 19.
        assert_eq!(
            ENCOUNTER_TABLES[41],
            [
                140, 140, 145, 140, 140, 140, 140, 145, 140, 140, 140, 140, 140, 145, 140, 140,
                140, 140, 140, 145
            ]
        );
        // 44 · House3 (`:1301-1328`): Rat Rat Burglar Thief Bard Monk...
        assert_eq!(
            ENCOUNTER_TABLES[44],
            [
                0, 0, 135, 138, 134, 140, 134, 132, 136, 140, 135, 138, 134, 136, 142, 135, 138,
                134, 140, 136
            ]
        );
    }

    #[test]
    fn resolution_rejects_ids_outside_the_two_bands() {
        for mobile in [43, 61, 127, 147, 0xFFFF] {
            assert_eq!(
                resolve_mobile(mobile),
                Err(EncounterTableError::UnknownMobileType { mobile }),
                "id {mobile} must not be clamped or remapped"
            );
        }
        assert_eq!(
            resolve_mobile(MONSTER_MOBILE_END).unwrap().enemy_record(),
            Some(42)
        );
        assert_eq!(
            resolve_mobile(HUMANOID_MOBILE_START),
            Ok(MobileRef::Humanoid {
                mobile: 128,
                class_id: 0
            })
        );
        assert_eq!(
            resolve_mobile(HUMANOID_MOBILE_END),
            Ok(MobileRef::Humanoid {
                mobile: 146,
                class_id: 18
            })
        );
        assert_eq!(
            resolve_cell(TABLE_COUNT, 0),
            Err(EncounterTableError::TableOutOfBounds { index: TABLE_COUNT })
        );
        assert_eq!(
            resolve_cell(0, TABLE_LEN),
            Err(EncounterTableError::CellOutOfBounds {
                table: 0,
                cell: TABLE_LEN
            })
        );
    }

    #[test]
    fn humanoids_have_no_enemy_record() {
        // `MonsterFile.cs:46-47`: the 43 `ENEMY###.CFG` are the monster
        // classes; a humanoid class comes from the career data (M20).
        for mobile in HUMANOID_MOBILE_START..=HUMANOID_MOBILE_END {
            let resolved = resolve_mobile(mobile).unwrap();
            assert_eq!(resolved.enemy_record(), None, "humanoid {mobile}");
            assert_eq!(resolved.class_id(), Some((mobile - 128) as u8));
            assert!(resolved.is_humanoid());
        }
        assert_eq!(HUMANOID_CLASS_COUNT as u16, HUMANOID_MOBILE_END - 127);
        for mobile in 0..=MONSTER_MOBILE_END {
            let resolved = resolve_mobile(mobile).unwrap();
            assert_eq!(resolved.enemy_record(), Some(usize::from(mobile)));
            assert_eq!(resolved.class_id(), None);
            assert!(!resolved.is_humanoid());
        }
        // A humanoid draw is typed, never a raw id the caller could index.
        let humanoid = choose_enemy(&[144; TABLE_LEN], 5, 50, |low, _| low)
            .unwrap()
            .unwrap();
        assert_eq!(
            humanoid,
            MobileRef::Humanoid {
                mobile: 144,
                class_id: 16
            }
        );
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

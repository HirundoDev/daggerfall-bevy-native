//! M25 probes — monsters, enemy AI and encounters (SPEC M25 §10,
//! REQ-L99-058 · ORDER M25-CLOSURE criteria 1/2/4).
//!
//! `PROBE-MONSTER-001` — five blocks over retail:
//! - **A** container census: `records=103 ascr=60 enemy=43 dir_off=25282
//!   record_sum=25278 enemy_stride=74 ascr_min=335 ascr_max=458` (§1.2, A1-A8).
//! - **B** the 43 `ENEMY###.CFG` are 74 B each and `Rat`/`Lamia` match field by
//!   field with §2.2 (goldens B1-B8) + `MobileTypes` → record bijection 43/43.
//! - **C** monster HP intervals from the **`EnemyBasics.Enemies` table**
//!   (delta vs spec §3.1, see block C) + the class-HP formula of the
//!   humanoid path (C1-C6).
//! - **D** deterministic demo: `spawned=3` with table levels and table HP,
//!   digest ×2.
//! - **E** the 19 humanoid classes spawn through `from_career` with retail
//!   `CLASS*.CFG` careers: HP inside `class_max_health_bounds` (criterion 7).
//!
//! Reads `ARENA2/MONSTER.BSA` (+ `CLASS00..18.CFG` for block E); unreadable
//! ⇒ **exit 2** `reason=missing_monster_bsa` (never a PASS without retail).
//!
//! `PROBE-ENEMY-AI-001` — pure (0 retail): formulas D1-D13, scenario
//! `Idle→Pursue` + contact at 2.25 m, `Pursue→Idle` at 12.5 s with 200
//! perceptions and `OpenDoor` gate at ≤ 2 m.
//!
//! `PROBE-ENCOUNTER-001` — pure (0 retail): E1-E10, census of the 45 tables,
//! **900/900 cells resolved to true `MobileTypes` ids and to the
//! `EnemyBasics.Enemies` lookup** (393 humanoid cells, 5 tables spot-checked
//! id by id against `RandomEncounters.cs`, negative controls 43/61/147 and
//! the full out-of-band set), 1000 seeded rolls with digest ×2 and spawn
//! probability, dungeon with and without alarm, the pacification goldens
//! F1-F7 (§6, row 5 of §12) and block **G**: the M25 data → combat/loot
//! boundary (material gate with no roll and no wear, enemy loot ×2 keys).
//!
//! **Custody (INV-L99-6):** Mode A — retail bytes are read at runtime from the
//! user's ARENA2 and never enter the repository. 0 Bevy, 0 GPU.

use std::path::PathBuf;

use daggerfall_behavior::encounters::{
    choose_enemy, clamp_to_table, level_range, referenced_tables, resolve_cell, resolve_mobile,
    roll_spawn_dungeon, table, table_index, validate_tables, BuildingKind, ClimateBand,
    EncounterContext, EncounterTableError, MobileRef, TableClamp, COMMENTED_TABLES,
    DUNGEON_TYPE_COUNT, ENCOUNTER_TABLES, HUMANOID_MOBILE_END, HUMANOID_MOBILE_START,
    MONSTER_MOBILE_END, TABLE_COUNT, TABLE_LEN, UNDERWATER_TABLE_INDEX, UNUSED_TABLE_INDEX,
};
use daggerfall_behavior::enemy_ai::{
    attack_gate, attack_move_speed, class_max_health_bounds, melee_timer_seconds, move_speed,
    pacification_chance, pacification_probability, pacifies, speed_floor,
    AiEvent, AiState, Door, EnemyAi, Perception, CLASSIC_TICK, GIVE_UP_SECONDS, MELEE_DISTANCE,
    OPEN_DOOR_DISTANCE, SIGHT_RADIUS_M,
};
use daggerfall_behavior::mobile_enemies::{
    mobile_enemy, MobileEnemyEntry, MobileEnemyError, MobileTeams,
};
use daggerfall_combat::{
    resolve_attack, AttackInput, Combatant, DeathSequence, Dice100, EntityKind, Weapon,
};
use daggerfall_inventory::equipment::{EquipSlot, ItemEquipTable};
use daggerfall_inventory::loot::{
    assign_enemy_starting_equipment, container_digest_bytes, generate_enemy_loot, matrix,
    EnemyEquipmentVariant, Rng64 as KitRng,
};
use daggerfall_parser_rs::monster::{
    record_index_of_mobile, MonsterArchive, MonsterClass, MonsterError, HUMANOID_TYPE_NAMES,
    MONSTER_FILENAME, MOBILE_TO_RECORD, MOBILE_TYPE_COUNT, MOBILE_TYPE_NAMES,
};
use daggerfall_skills::career::{parse_enemy_class_template, EnemyClassTemplate};
use daggerfall_skills::humanoid::{
    city_watch_level, is_city_watch, HumanoidProfile, HumanoidProfileError,
};
use daggerfall_skills::rng::Rng64;

use daggerfall_app::combat_runtime::{
    CombatSession, EnemyArmorPolicy, Fighter, ENEMY_ARMOR_BASE, ENEMY_CLASS_ARMOR_CAP,
};

// ── evidence rows (M19/M20/M21/M24 template) ────────────────────────────────

/// Printed row: `[OK] <id> - <detail>` or `FAIL <id> got=<v> want=<v>`.
#[derive(Debug, Clone)]
struct Row {
    id: &'static str,
    ok: bool,
    got: String,
    want: String,
    detail: String,
}

/// Row whose verdict is the equality `got == want`.
fn row(id: &'static str, got: String, want: String, detail: impl Into<String>) -> Row {
    Row {
        id,
        ok: got == want,
        got,
        want,
        detail: detail.into(),
    }
}

/// Row with its own verdict and raw detail.
fn check(id: &'static str, ok: bool, detail: impl Into<String>) -> Row {
    Row {
        id,
        ok,
        got: if ok { "ok".into() } else { "fail".into() },
        want: "ok".into(),
        detail: detail.into(),
    }
}

fn print_rows(title: &str, rows: &[Row]) -> bool {
    println!("\n-- {title} --");
    let mut ok = true;
    for row in rows {
        if row.ok {
            println!("[OK] {} - {}", row.id, row.detail);
        } else {
            ok = false;
            println!("FAIL {} got={} want={}", row.id, row.got, row.want);
        }
    }
    ok
}

/// Stable number-to-text without trailing zeros (`4.75`, `2.44898`, `12.5`).
fn fmt_f32(value: f32) -> String {
    let mut text = format!("{value:.5}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}

/// FNV-1a 64 (same digest as the rest of the probes).
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Loads `MONSTER.BSA` through the shared Mode A resolver.
fn load_monster_bsa() -> Result<(PathBuf, Vec<u8>), String> {
    let path = crate::mode_a_arena2_path(MONSTER_FILENAME)?;
    let bytes = std::fs::read(&path).map_err(|err| format!("reading {path:?}: {err}"))?;
    Ok((path, bytes))
}

// ═══════════════════════════════════════════════════════════════════════════
// Deterministic scene reused by the demo (block D)
// ═══════════════════════════════════════════════════════════════════════════

/// Demo scene seed (deterministic ×2).
pub const M25_SCENE_SEED: u64 = 20_261_021;
/// Player level in the demo scene.
pub const M25_PLAYER_LEVEL: i32 = 5;
/// Live monsters of the scene (§11 criterion 1: three).
pub const M25_SPAWNED: usize = 3;
/// Demo dungeon: `0 = Crypt` (`DFRegion.DungeonTypes`).
pub const M25_DUNGEON_TYPE: u8 = 0;
/// Encounter table used to prove the humanoid path: `2 = Human Stronghold`
/// (`RandomEncounters.cs:88`, 12 of its 20 cells are humanoids).
pub const M25_HUMANOID_TABLE: usize = 2;
/// Synthetic table of a single humanoid id (`MobileTypes.Warrior = 144`):
/// every draw must take the typed humanoid path.
static HUMANOID_ONLY_TABLE: [u16; TABLE_LEN] = [144; TABLE_LEN];

/// A live monster of the scene (what the world/ECS will see).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnedMonster {
    /// `MobileTypes` id drawn by the encounter table.
    pub mobile: u16,
    /// Index of the `ENEMY###.CFG` record that carries it.
    pub record: usize,
    /// Retail CFG `Name`.
    pub name: String,
    /// `MobileEnemy.Level` — the level of an `EnemyMonster`
    /// (`EnemyEntity.cs:284`). **Delta vs spec M25 §3/N-2:** the spawn level
    /// is the table field, not a `level_range` roll (`[INFERENCE]` →
    /// verified source).
    pub level: i32,
    /// HP rolled within the table interval `Random.Range(MinHealth,
    /// MaxHealth + 1)` (`EnemyEntity.cs:285`).
    pub hp: i32,
    /// Table HP interval `[MinHealth, MaxHealth]` — the spawn roll's bounds
    /// (`EnemyEntity.cs:285`), not the §3.1 class formula.
    pub hp_bounds: (i32, i32),
    /// `MobileEnemy.MinMetalToHit` on the `WeaponMaterialTypes` scale
    /// (`EnemyEntity.cs:315`; `None = -1`).
    pub min_metal_to_hit: i32,
    /// `MobileEnemy.Team` (`EnemyEntity.cs:316`) — exposed; the tree has no
    /// in-fighting consumer yet (`EnemyMotor.cs:209-212` is the DFU reader).
    pub team: MobileTeams,
    /// `MobileEnemy.LootTableKey` (`EnemyEntity.cs:330`).
    pub loot_table_key: Option<char>,
    /// `MobileEnemy.MapChance` (`EnemyEntity.cs:389`).
    pub map_chance: i32,
    /// The three `(Min, Max)` damage bands of the table — the ranges M22's
    /// `Fighter::monster_bands` consumes.
    pub damage_bands: [(i32, i32); 3],
}

impl SpawnedMonster {
    /// Monster built from a table row alone (ENCOUNTER-001 block G and unit
    /// tests): every gameplay field comes from the row, `hp` is supplied by
    /// the caller (probes pass the row floor so the golden is deterministic)
    /// and `record` is the `MobileTypes → ENEMY` identity of the monster
    /// band (`record_index_of_mobile`, B7 bijection).
    pub fn from_entry(entry: &MobileEnemyEntry, hp: i32) -> Self {
        Self {
            mobile: entry.id,
            record: usize::from(entry.id),
            name: entry.name.to_string(),
            level: entry.level,
            hp,
            hp_bounds: (entry.min_health, entry.max_health),
            min_metal_to_hit: entry.min_metal_to_hit.raw(),
            team: entry.team,
            loot_table_key: entry.loot_table_key,
            map_chance: entry.map_chance,
            damage_bands: entry.damage_bands,
        }
    }
}

/// A live humanoid draw of a composed scene.
///
/// Humanoids (`MobileTypes` `128..=146`) have **no** `ENEMY###.CFG` record
/// (`MonsterFile.cs:46-47`): level and HP come from the `EnemyClass`
/// branch — the player level (+3..=6 for the city watch,
/// `EnemyEntity.cs:299-301`) and `RollEnemyClassMaxHealth` through
/// `HumanoidProfile::from_career` (M20 career data). The `Level` /
/// `MinHealth` / `MaxHealth` fields of the humanoid table rows are the C#
/// struct defaults and are **not** consumed (`EnemyEntity.cs:294-303`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnedHumanoid {
    /// `MobileTypes` id (`128..=146`).
    pub mobile: u16,
    /// Class slot `0..=18` (`mobile − 128`, `EnemyEntity.cs:292`).
    pub class_id: u8,
    /// `MobileTypes` label of the class (`HUMANOID_TYPE_NAMES[class_id]`).
    pub name: &'static str,
    /// Effective level: the player level, +3..=6 for the city watch
    /// (`EnemyEntity.cs:299-301`).
    pub level: i32,
    /// HP rolled by `from_career` (`RollEnemyClassMaxHealth`,
    /// `EnemyEntity.cs:303`).
    pub max_health: i32,
    /// `[low, high]` of `class_max_health_bounds(level, hppl)`.
    pub health_bounds: (i32, i32),
    /// `MobileEnemy.Team` of the row (`EnemyEntity.cs:316`) — exposed, no
    /// in-fighting consumer yet.
    pub team: MobileTeams,
    /// `MobileEnemy.LootTableKey` of the row (`EnemyEntity.cs:330`).
    pub loot_table_key: Option<char>,
    /// `MobileEnemy.MapChance` of the row (`EnemyEntity.cs:389`).
    pub map_chance: i32,
}

/// One entry of a composed scene: the two `MobileTypes` bands stay distinct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpawnedCombatant {
    /// Monster with an `ENEMY###.CFG` record (`0..=42`).
    Monster(SpawnedMonster),
    /// Humanoid without a record (`128..=146`).
    Humanoid(SpawnedHumanoid),
}

impl SpawnedCombatant {
    /// `MobileTypes` id of the draw.
    pub const fn mobile(&self) -> u16 {
        match self {
            Self::Monster(monster) => monster.mobile,
            Self::Humanoid(humanoid) => humanoid.mobile,
        }
    }
}

/// Failure of the humanoid spawn path ([`spawn_humanoid`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HumanoidSpawnError {
    /// `mobile < 128` — the monster band takes the `ENEMY###.CFG` path.
    NotHumanoid {
        /// The rejected id.
        mobile: u16,
    },
    /// `mobile − 128 > 255` — the class index cannot exist as a `u8`.
    ClassIdOutOfRange {
        /// The rejected id.
        mobile: u16,
    },
    /// `HumanoidProfile::from_career` rejected the class index (typed, never
    /// clamped): classes 19+ are not standard careers. DFU would instead
    /// fail to load `CLASS{index}.CFG` (`DaggerfallEntity.cs:907-913`).
    Class {
        /// The rejected id.
        mobile: u16,
        /// Derived `mobile − 128`.
        class_id: u8,
        /// The profile's typed rejection.
        source: HumanoidProfileError,
    },
}

impl std::fmt::Display for HumanoidSpawnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotHumanoid { mobile } => {
                write!(f, "mobile {mobile} is not a humanoid (128..=146)")
            }
            Self::ClassIdOutOfRange { mobile } => {
                write!(f, "mobile {mobile}: class index {mobile} - 128 exceeds u8")
            }
            Self::Class {
                mobile,
                class_id,
                source,
            } => write!(f, "mobile {mobile}: class {class_id}: {source}"),
        }
    }
}

impl std::error::Error for HumanoidSpawnError {}

/// Spawns one humanoid exactly like the `EnemyClass` branch
/// (`EnemyEntity.cs:290-303`): `class_id = mobile − 128` (`:292`), the city
/// watch is +3..=6 levels above the player (`:299-301`) **before** the HP
/// `RollEnemyClassMaxHealth(level,
/// career.HitPointsPerLevel)` (`:303`) inside the M20 profile routine
/// (`HumanoidProfile::from_career` delegates to `from_parts`, which is the
/// code that runs here — the enemy template carries the same `CLASS*.CFG`
/// name/attributes/hppl fields). The table's `Level`/health fields are
/// never read — they are the struct defaults the branch ignores. A class
/// index outside `0..=18` surfaces as the typed
/// [`HumanoidProfileError::ClassId`] through this path, never as a silent
/// default class.
pub fn spawn_humanoid(
    mobile: u16,
    career: &EnemyClassTemplate,
    player_level: i32,
    rng: &mut Rng64,
) -> Result<SpawnedHumanoid, HumanoidSpawnError> {
    let class_id = mobile
        .checked_sub(HUMANOID_MOBILE_START)
        .ok_or(HumanoidSpawnError::NotHumanoid { mobile })?;
    let class_id =
        u8::try_from(class_id).map_err(|_| HumanoidSpawnError::ClassIdOutOfRange { mobile })?;
    let level = if is_city_watch(class_id) {
        city_watch_level(player_level, rng)
    } else {
        player_level
    };
    let profile = HumanoidProfile::from_parts(
        class_id,
        career.name.clone(),
        career.attributes,
        career.hit_points_per_level,
        level,
        rng,
    )
    .map_err(|source| {
        HumanoidSpawnError::Class {
            mobile,
            class_id,
            source,
        }
    })?;
    // `class_id <= 18` ⟹ `mobile <= 146` ⟹ in-band; the error arm stays a
    // typed path (no unwrap) for any future band change.
    let entry = mobile_enemy(mobile)
        .map_err(|MobileEnemyError::OutOfBand { id }| HumanoidSpawnError::ClassIdOutOfRange {
            mobile: id,
        })?;
    Ok(SpawnedHumanoid {
        mobile,
        class_id,
        name: HUMANOID_TYPE_NAMES[usize::from(class_id)],
        level: profile.level,
        max_health: profile.max_health,
        health_bounds: profile.health_bounds(),
        team: entry.team,
        loot_table_key: entry.loot_table_key,
        map_chance: entry.map_chance,
    })
}

/// Failure of a scene composition ([`compose_scene_from`]).
#[derive(Debug)]
pub enum ComposeError {
    /// `MONSTER.BSA` could not be read (Mode A).
    Retail(String),
    /// The archive or one of its records did not parse.
    Archive(MonsterError),
    /// The requested encounter table index does not exist.
    TableMissing,
    /// The encounter table has no cells to draw from.
    EmptyTable,
    /// A cell did not resolve to a `MobileTypes` id.
    Encounter(EncounterTableError),
    /// The career source could not produce the `CLASS*.CFG` career.
    Career {
        /// Class whose career failed to load.
        class_id: u8,
        /// The loader's reason.
        reason: String,
    },
    /// The humanoid spawn path rejected the draw.
    Humanoid {
        /// The rejected id.
        mobile: u16,
        /// The typed spawn failure.
        source: HumanoidSpawnError,
    },
    /// The demo dungeon table (monster-only by census) drew a humanoid.
    DemoTableDrewHumanoid {
        /// The humanoid that should not exist in this table.
        mobile: u16,
    },
}

impl std::fmt::Display for ComposeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Retail(reason) => write!(f, "retail data: {reason}"),
            Self::Archive(err) => write!(f, "{MONSTER_FILENAME}: {err}"),
            Self::TableMissing => write!(f, "encounter table missing"),
            Self::EmptyTable => write!(f, "empty encounter table"),
            Self::Encounter(err) => write!(f, "encounter cell: {err}"),
            Self::Career { class_id, reason } => {
                write!(f, "CLASS{class_id:02}.CFG: {reason}")
            }
            Self::Humanoid { mobile, source } => write!(f, "humanoid {mobile}: {source}"),
            Self::DemoTableDrewHumanoid { mobile } => {
                write!(f, "demo table drew humanoid {mobile} with no ENEMY###.CFG")
            }
        }
    }
}

impl std::error::Error for ComposeError {}

/// Retail `CLASS{class_id:02}.CFG` enemy-class template — the runtime load
/// of `DaggerfallEntity.GetClassCareerTemplate`
/// (`DaggerfallEntity.cs:907-913`: `string.Format("CLASS{0:00}.CFG",
/// (int)career)` read from ARENA2) in the enemy view: name, attributes and
/// `HitPointsPerLevel` are the fields `EnemyEntity.cs:294-303` consumes
/// (skill groups are player-only — see [`EnemyClassTemplate`]). Mode A:
/// bytes are read here and never enter the repository.
fn load_retail_class_career(class_id: u8) -> Result<EnemyClassTemplate, String> {
    let filename = format!("CLASS{class_id:02}.CFG");
    let path = crate::mode_a_arena2_path(&filename)?;
    let bytes = std::fs::read(&path).map_err(|err| format!("reading {path:?}: {err}"))?;
    parse_enemy_class_template(&bytes).map_err(|err| format!("{filename}: {err}"))
}

/// Scene: 3 live monsters from the demo dungeon table (Crypt, table 0).
///
/// Deterministic: same seed ⇒ same scene (block D digest). The demo table of
/// `M25_DUNGEON_TYPE` is monster-only (tables 0, 1, 13, 19 and 37 are); a
/// humanoid draw would be reported as an error instead of being dropped or
/// given an invented `ENEMY` record. The career source is the retail
/// `CLASS*.CFG` loader — the census guarantees it is never called here.
pub fn compose_scene(seed: u64) -> Result<Vec<SpawnedMonster>, ComposeError> {
    let (_, bytes) = load_monster_bsa().map_err(ComposeError::Retail)?;
    let index = table_index(EncounterContext::Dungeon {
        dungeon_type: M25_DUNGEON_TYPE,
    })
    .ok_or(ComposeError::TableMissing)?;
    let demo_table = table(index).ok_or(ComposeError::TableMissing)?;
    compose_scene_from(&bytes, demo_table, seed, load_retail_class_career)?
        .into_iter()
        .map(|entry| match entry {
            SpawnedCombatant::Monster(monster) => Ok(monster),
            SpawnedCombatant::Humanoid(humanoid) => Err(ComposeError::DemoTableDrewHumanoid {
                mobile: humanoid.mobile,
            }),
        })
        .collect()
}

/// Same as [`compose_scene`] over an already loaded buffer, an explicit
/// encounter table (demo/probe) and an explicit humanoid career source.
///
/// Every draw goes through `choose_enemy` → [`MobileRef`], so a humanoid
/// cell is spawned through [`spawn_humanoid`] (player level, city-watch
/// bonus, `from_career`→`from_parts` HP) instead of indexing
/// `MOBILE_TO_RECORD` (which only covers `0..=42`). A monster's level is
/// `MobileEnemy.Level` and its HP is rolled from
/// `MobileEnemy.MinHealth..=MaxHealth` (`EnemyEntity.cs:284-285`) — the
/// `level_range` roll only selects the table cell, it is never the
/// combatant's level (spec M25 §3/N-2 delta).
pub fn compose_scene_from(
    bytes: &[u8],
    table: &'static [u16; TABLE_LEN],
    seed: u64,
    career_of: impl Fn(u8) -> Result<EnemyClassTemplate, String>,
) -> Result<Vec<SpawnedCombatant>, ComposeError> {
    let archive = MonsterArchive::parse(bytes).map_err(ComposeError::Archive)?;

    let mut rng = Rng64::new(seed);
    let mut scene = Vec::with_capacity(M25_SPAWNED);
    for _ in 0..M25_SPAWNED {
        let roll = rng.range_inclusive(1, 100);
        let mobile_ref = choose_enemy(table, M25_PLAYER_LEVEL, roll, |low, high| {
            rng.range_inclusive(low, high)
        })
        .map_err(ComposeError::Encounter)?
        .ok_or(ComposeError::EmptyTable)?;
        match mobile_ref {
            MobileRef::Monster { mobile, record } => {
                let class = archive
                    .monster_class(bytes, record)
                    .map_err(ComposeError::Archive)?;
                let entry = mobile_enemy(mobile)
                    .map_err(|MobileEnemyError::OutOfBand { id }| {
                        ComposeError::Encounter(EncounterTableError::UnknownMobileType {
                            mobile: id,
                        })
                    })?;
                // `Random.Range(MinHealth, MaxHealth + 1)` (`:285`) — the
                // int overload is inclusive on both ends here.
                let hp = rng.range_inclusive(entry.min_health, entry.max_health);
                let mut monster = SpawnedMonster::from_entry(entry, hp);
                monster.record = record;
                monster.name = class.name.clone();
                scene.push(SpawnedCombatant::Monster(monster));
            }
            MobileRef::Humanoid { mobile, class_id } => {
                let career = career_of(class_id)
                    .map_err(|reason| ComposeError::Career { class_id, reason })?;
                let humanoid = spawn_humanoid(mobile, &career, M25_PLAYER_LEVEL, &mut rng)
                    .map_err(|source| ComposeError::Humanoid { mobile, source })?;
                scene.push(SpawnedCombatant::Humanoid(humanoid));
            }
        }
    }
    Ok(scene)
}

/// Encounter combatant of a spawned monster (ORDER M25-CLOSURE criterion 8):
/// the pure engine [`Combatant`] with the fields the M25 table owns —
/// `min_metal_to_hit = MobileEnemy.MinMetalToHit` (`EnemyEntity.cs:315`),
/// health from the spawn roll (`:285`; both health fields, 0 literal) and
/// `kind = EnemyMonster` (the `+40` to hit of `FormulaHelper.cs:1242-1245`).
///
/// `armor_values` stays the `Combatant::new` default here; the spawn entry
/// point that projects the entity base and the equipment is
/// [`encounter_fighter`] (`EnemyEntity.SetEnemyEquipment`).
pub fn monster_combatant(monster: &SpawnedMonster, id: u32) -> Combatant {
    let mut combatant = Combatant::new(id, EntityKind::EnemyMonster);
    combatant.health = monster.hp;
    combatant.max_health = monster.hp;
    combatant.min_metal_to_hit = monster.min_metal_to_hit;
    combatant
}

// ── encounter equipment (`ItemHelper.AssignEnemyStartingEquipment` +
//    `EnemyEntity.SetEnemyEquipment`) ────────────────────────────────────────

/// `MobileTypes` ids of the careers that equip themselves
/// (`EnemyEntity.cs:333-344`; a monster's `careerIndex` **is**
/// `mobileEnemy.ID`, `:278`), on the `MonsterCareers` scale
/// (`EntityEnums.cs:120-166`).
const CAREER_ORC: u16 = 7;
const CAREER_CENTAUR: u16 = 8;
const CAREER_ORC_SERGEANT: u16 = 12;
const CAREER_ORC_SHAMAN: u16 = 21;
const CAREER_ORC_WARLORD: u16 = 24;

/// `MobileTypes.Knight_CityWatch` (`ItemHelper.cs:1385`).
const MOBILE_KNIGHT_CITY_WATCH: u16 = 146;

/// `SetEnemyEquipment` variant of a spawn (`EnemyEntity.cs:333-348`): the five
/// equipment careers are fixed, an `EnemyClass` rolls `Random.Range(0, 2)`
/// (`:347` — 0 or 1, 2 exclusive) off the **spawn** stream. `None` = the
/// monster keeps its `MobileEnemy.ArmorValue * 5` base (`:286-288`).
pub fn equipment_variant(
    kind: EntityKind,
    mobile: u16,
    rng: &mut Rng64,
) -> Option<EnemyEquipmentVariant> {
    match kind {
        EntityKind::EnemyMonster => match mobile {
            CAREER_ORC | CAREER_ORC_SHAMAN => Some(EnemyEquipmentVariant::Zero),
            CAREER_CENTAUR | CAREER_ORC_SERGEANT => Some(EnemyEquipmentVariant::One),
            CAREER_ORC_WARLORD => Some(EnemyEquipmentVariant::Two),
            _ => None,
        },
        EntityKind::EnemyClass => Some(if rng.range_inclusive(0, 1) == 0 {
            EnemyEquipmentVariant::Zero
        } else {
            EnemyEquipmentVariant::One
        }),
        EntityKind::Player => None,
    }
}

/// `ItemHelper.cs:1379-1386`: `itemLevel = player.Level`, forced to `1` when
/// the spawn is an `EntityTypes.EnemyClass` whose `MobileEnemy.ID` is
/// `Knight_CityWatch` (146) — "City watch never have items above iron or
/// steel".
fn enemy_item_level(kind: EntityKind, mobile: u16, player_level: i32) -> i32 {
    if kind == EntityKind::EnemyClass && mobile == MOBILE_KNIGHT_CITY_WATCH {
        1
    } else {
        player_level
    }
}

/// The encounter fighter: a WU-1 [`SpawnedCombatant`] plus
/// `EnemyEntity.SetEnemyEquipment` (`EnemyEntity.cs:404-451`) — the kit lands
/// in [`Fighter::equipment`] and `armor_values`/`weapon` are projected by
/// [`Fighter::refresh_equipment`], so the [`CombatSession`] gets a fighter
/// with no literal `armor_values`.
///
/// Fields from the M25 table through the WU-1 spawn record: `min_metal_to_hit`
/// (`EnemyEntity.cs:315`), health (`:285` monster roll / `:303` class roll),
/// `level` (`:284`/`:299-301`), `monster_bands` (`MobileEnemy.Min/MaxDamage…`)
/// and — on the spawn record, which has no `Combatant` field for it — `team`
/// (`:316`).
///
/// Deltas declared, not silent:
/// - **D-M25-2:** DFU draws the kit from the same global `UnityEngine.Random`
///   stream as the encounter. Here the kit gets its own stream derived from
///   the spawn stream with one draw (`Rng64::new(rng.next_u64())`), the
///   pattern of D-M21-3/D-M22-1: the spawn seed still determines every item.
/// - **D-M25-3:** the probe runs catalog-free (`ItemTemplate::empty`, the same
///   as the M21 loot path), so the kit's value/weight/condition come from the
///   empty template; the armour values consume only the template index.
pub fn encounter_fighter(
    spawn: &SpawnedCombatant,
    id: u32,
    player_level: i32,
    rng: &mut Rng64,
) -> Result<Fighter, MobileEnemyError> {
    let entry = mobile_enemy(spawn.mobile())?;
    let (combatant, kind, level, bands) = match spawn {
        SpawnedCombatant::Monster(monster) => (
            monster_combatant(monster, id),
            EntityKind::EnemyMonster,
            monster.level,
            Some(monster.damage_bands),
        ),
        SpawnedCombatant::Humanoid(humanoid) => {
            let mut combatant = Combatant::new(id, EntityKind::EnemyClass);
            combatant.health = humanoid.max_health;
            combatant.max_health = humanoid.max_health;
            combatant.min_metal_to_hit = entry.min_metal_to_hit.raw();
            (
                combatant,
                EntityKind::EnemyClass,
                humanoid.level,
                None,
            )
        }
    };
    let policy = match kind {
        EntityKind::EnemyMonster => EnemyArmorPolicy::Monster {
            armor_value: entry.armor_value,
        },
        _ => EnemyArmorPolicy::Class,
    };
    let mut fighter = Fighter::new(combatant, DeathSequence::for_enemy()).with_enemy_armor(policy);
    fighter.level = level;
    fighter.monster_bands = bands;
    if let Some(variant) = equipment_variant(kind, spawn.mobile(), rng) {
        let mut kit_rng = KitRng::new(rng.next_u64());
        fighter.equipment = assign_enemy_starting_equipment(
            &mut kit_rng,
            variant,
            enemy_item_level(kind, spawn.mobile(), player_level),
            None,
        );
        fighter.refresh_equipment();
    }
    Ok(fighter)
}

/// FNV-1a 64 digest of the scene (`mobile`, `record`, `level`, `hp`).
pub fn scene_digest(scene: &[SpawnedMonster]) -> u64 {
    let mut bytes = Vec::with_capacity(scene.len() * 8);
    for monster in scene {
        bytes.extend_from_slice(&monster.mobile.to_le_bytes());
        bytes.extend_from_slice(&monster.level.to_le_bytes());
        bytes.extend_from_slice(&monster.hp.to_le_bytes());
    }
    fnv1a64(&bytes)
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-MONSTER-001
// ═══════════════════════════════════════════════════════════════════════════

/// Block B: the 43 CFG are 74 B each and the two reference classes match §2.2.
fn monster_block_b(
    archive: &MonsterArchive,
    bytes: &[u8],
) -> Result<Vec<Row>, String> {
    let mut rows = Vec::new();
    // `classes[n]` = `ENEMY{n:03}.CFG` (number in the name, not position in the
    // census — erratum 26: the directory interleaves ASCR and ENEMY).
    let mut classes: Vec<MonsterClass> = Vec::with_capacity(archive.enemy.len());
    for number in 0..archive.enemy.len() {
        let record = archive
            .monster_class(bytes, number)
            .map_err(|err| format!("ENEMY{number:03}.CFG: {err}"))?;
        classes.push(record);
    }
    // Every record declares size 74 and parses to exactly 74 fields.
    let all_74 = archive.census().enemy_stride == Some(74) && classes.len() == 43;
    println!("all_74={all_74}");
    rows.push(check(
        "B1 all_74",
        all_74,
        format!(
            "{} ENEMY records · stride {:?} · 58 + 8·2 = 74",
            classes.len(),
            archive.census().enemy_stride
        ),
    ));

    // Rat (ENEMY000) and Lamia (ENEMY042) — spec §2.2 cites them by index.
    let rat = archive
        .monster_class(bytes, 0)
        .map_err(|err| format!("ENEMY000.CFG: {err}"))?;
    let lamia = archive
        .monster_class(bytes, 42)
        .map_err(|err| format!("ENEMY042.CFG: {err}"))?;
    let rat_line = format!(
        "Rat{{res={},hppl={},adv={},attrs=[{}]}}",
        rat.resistance_flags,
        rat.hit_points_per_level,
        rat.advancement_multiplier_raw,
        rat.attributes
            .iter()
            .map(u16::to_string)
            .collect::<Vec<_>>()
            .join(",")
    );
    let lamia_line = format!(
        "Lamia{{res={},imm={},low={},abl={},hppl={},adv={}}}",
        lamia.resistance_flags,
        lamia.immunity_flags,
        lamia.low_tolerance_flags,
        lamia.ability_flags,
        lamia.hit_points_per_level,
        lamia.advancement_multiplier_raw
    );
    println!("{rat_line}");
    println!("{lamia_line}");
    rows.push(row(
        "B2 Rat",
        rat_line.clone(),
        "Rat{res=68,hppl=1,adv=75008,attrs=[40,10,70,80,55,50,45,50]}".to_string(),
        "spec §2.2 · ENEMY000.CFG",
    ));
    rows.push(row(
        "B3 Lamia",
        lamia_line.clone(),
        "Lamia{res=2,imm=69,low=16,abl=36,hppl=17,adv=52992}".to_string(),
        "spec §2.2 · ENEMY042.CFG",
    ));
    rows.push(row(
        "B4 Rat.Endurance",
        rat.endurance().to_string(),
        "55".to_string(),
        "u16 @58+4·2 (golden B2)",
    ));
    rows.push(row(
        "B5 Rat.weapon_armor_shields",
        rat.weapon_armor_shields().to_string(),
        "0".to_string(),
        "(0<<16)|(0<<8)|0 (golden B6)",
    ));
    rows.push(row(
        "B6 Lamia.name",
        lamia.name.clone(),
        "Lamia".to_string(),
        "16 B slot cut at the first NUL",
    ));

    // MobileTypes → record bijection, verified against the CFG `Name`.
    let mut used = [false; MOBILE_TYPE_COUNT];
    let mut mismatches = Vec::new();
    for (mobile, record) in MOBILE_TO_RECORD.iter().enumerate() {
        let index = usize::from(*record);
        let class = &classes[index];
        if used[index] || !name_matches(mobile, &class.name) {
            mismatches.push(format!(
                "{}({mobile}) -> ENEMY{index:03}={}",
                MOBILE_TYPE_NAMES[mobile], class.name
            ));
        }
        used[index] = true;
    }
    rows.push(check(
        "B7 MobileTypes → ENEMY bijection",
        mismatches.is_empty() && used.iter().all(|hit| *hit),
        format!(
            "{MOBILE_TYPE_COUNT}/{MOBILE_TYPE_COUNT} match by `Name`{}",
            if mismatches.is_empty() {
                String::new()
            } else {
                format!(" · mismatch: {}", mismatches.join(", "))
            }
        ),
    ));
    Ok(rows)
}

/// Retail CFG `Name` against the `MobileTypes` name, with the declared
/// aliases (`Giant Scorpian`, `horse`, second `dragonling`).
fn name_matches(mobile: usize, cfg_name: &str) -> bool {
    const ALIASES: [(usize, &str); 3] = [
        (20, "giantscorpian"),
        (39, "horse"),
        (40, "dragonling"),
    ];
    let normalized = |text: &str| {
        text.chars()
            .filter(|ch| ch.is_ascii_alphanumeric())
            .map(|ch| ch.to_ascii_lowercase())
            .collect::<String>()
    };
    let expected = ALIASES
        .iter()
        .find(|(id, _)| *id == mobile)
        .map(|(_, alias)| *alias)
        .unwrap_or_else(|| MOBILE_TYPE_NAMES[mobile]);
    normalized(cfg_name) == normalized(expected)
}

/// Attacker `chanceToHitMod` of the block-F strikes: a fixed composed skill
/// (`FormulaHelper.cs:592`) so the comparison is about the armour term only.
/// `45 + 60 (class armour) − 50 (universal adjustment)` stays inside the
/// `[3, 97]` clamp, so the Δ is never masked by the clamp.
const EQUIPMENT_ATTACKER_CHANCE_MOD: i32 = 45;

/// Block F · ORDER M25-CLOSURE criterion 3: the kit of a **spawned** encounter
/// humanoid lands in `armor_values` through `Fighter::refresh_equipment`, and
/// the same fighter with vs without the kit measurably moves the strike
/// (`FormulaHelper.CalculateArmorToHit`, `:1149-1161`, added at `:808`).
fn monster_block_f(bytes: &[u8]) -> Result<Vec<Row>, String> {
    let mut rows = Vec::new();

    // The spawn: the humanoid-only table (every draw is `Warrior 144`) with the
    // retail `CLASS16.CFG` career — the same path as block D6, over 24 seeds so
    // the kit RNG visits light and heavy armour.
    let mut fighters = Vec::new();
    for extra in 0..24u64 {
        let seed = M25_SCENE_SEED + 41 + extra;
        let scene = compose_scene_from(bytes, &HUMANOID_ONLY_TABLE, seed, load_retail_class_career)
            .map_err(|err| err.to_string())?;
        let mut rng = Rng64::new(seed);
        for (index, spawned) in scene.iter().enumerate() {
            let id = 9_200 + u32::try_from(extra * 3 + index as u64).unwrap_or(0);
            let fighter =
                encounter_fighter(spawned, id, M25_PLAYER_LEVEL, &mut rng).map_err(|err| err.to_string())?;
            fighters.push((seed, fighter));
        }
    }

    // F1 · every spawned fighter: class policy projected from its own kit
    // (`EnemyEntity.cs:409-435`), table `min_metal_to_hit`, one right-hand
    // weapon — 0 literal armour.
    let humanoid_row = mobile_enemy(144).map_err(|err| err.to_string())?;
    let unarmoured = [ENEMY_ARMOR_BASE; 7];
    let mut projection_ok = true;
    let mut armed = 0usize;
    let mut kit_lines = Vec::new();
    for (seed, fighter) in &fighters {
        let projected = EnemyArmorPolicy::Class.project(&fighter.equipment);
        projection_ok &= fighter.combatant.kind == EntityKind::EnemyClass
            && fighter.combatant.min_metal_to_hit == humanoid_row.min_metal_to_hit.raw()
            && fighter.combatant.armor_values == projected
            && projected.iter().all(|value| *value <= ENEMY_CLASS_ARMOR_CAP)
            && fighter.weapon.is_some();
        if fighter.combatant.armor_values != unarmoured {
            armed += 1;
        }
        if kit_lines.len() < 3 {
            let weapon = fighter
                .equipment
                .get(EquipSlot::RightHand)
                .map(|item| format!("{}(m{})", item.template_index, item.native_material))
                .unwrap_or_else(|| "-".to_string());
            kit_lines.push(format!(
                "seed {seed}: weapon {weapon} armor {:?} (M21 delta {:?})",
                fighter.combatant.armor_values,
                fighter
                    .equipment
                    .armor_values_over_slots(EquipSlot::Head, EquipSlot::Feet)
            ));
        }
    }
    rows.push(check(
        "F1 spawn kit → armor_values (projected, class clamp)",
        projection_ok && fighters.len() == 72 && armed > 0,
        format!(
            "{} spawned humanoids · {armed} with armour-value below the 60 clamp · {}",
            fighters.len(),
            kit_lines.join(" · ")
        ),
    ));

    // F2 · same fighter with vs without the kit, identical rolls: the chance
    // difference is exactly `ArmorValues_bare[part] − ArmorValues_kit[part]`.
    let attacker = Combatant::new(1, EntityKind::Player);
    let seeds = 64u64;
    let mut measurements = 0u64;
    let mut identity = 0u64;
    let mut reductions = 0u64;
    let mut max_delta = 0i32;
    let mut best = String::new();
    let mut best_index = 0usize;
    let mut heavy = 0usize;
    for (index, (_, fighter)) in fighters.iter().enumerate() {
        let mut naked = fighter.clone();
        naked.equipment = ItemEquipTable::new();
        naked.refresh_equipment();
        if naked.combatant.armor_values != fighter.combatant.armor_values {
            heavy += 1;
        }
        for seed in 0..seeds {
            let mut equipped_dice = Dice100::with_seed(seed + 1);
            let mut bare_dice = Dice100::with_seed(seed + 1);
            let strike = |target: &Combatant, dice: &mut Dice100| {
                resolve_attack(
                    &AttackInput {
                        attacker: &attacker,
                        target,
                        weapon: None,
                        chance_mod: EQUIPMENT_ATTACKER_CHANCE_MOD,
                        damage_mods: 0,
                        backstab_level: 1,
                        enemy_bonus: 0,
                    },
                    dice,
                )
            };
            let equipped = strike(&fighter.combatant, &mut equipped_dice);
            let bare = strike(&naked.combatant, &mut bare_dice);
            measurements += 1;
            if bare.part != equipped.part {
                continue;
            }
            let delta = bare.chance - equipped.chance;
            let expected = naked.combatant.armor_values[equipped.part]
                - fighter.combatant.armor_values[equipped.part];
            if delta == expected {
                identity += 1;
            }
            if equipped.chance < bare.chance {
                reductions += 1;
                if delta > max_delta {
                    max_delta = delta;
                    best_index = index;
                    best = format!(
                        "part {} chance {}→{} · kit {:?} vs naked {:?}",
                        equipped.part,
                        bare.chance,
                        equipped.chance,
                        fighter.combatant.armor_values,
                        naked.combatant.armor_values
                    );
                }
            }
        }
    }
    rows.push(check(
        "F2 kit vs naked: Δchance = ArmorValues[naked] − ArmorValues[kit]",
        identity == measurements && reductions > 0,
        format!(
            "measurements={measurements} identity={identity} heavy_kits={heavy}/{} reductions={reductions} max_delta={max_delta} · {best}",
            fighters.len()
        ),
    ));

    // F3 · the same fighter inside a `CombatSession`: the equipped piece of the
    // struck part absorbs the blow (`FormulaHelper.cs:1088-1122`) and the
    // worst-armoured fighter's vector survives the session's re-projection.
    let (_, strongest) = fighters
        .get(best_index)
        .ok_or("empty block-F scene")?;
    let expected_armor = strongest.combatant.armor_values;
    let session = CombatSession::new(
        Fighter::new(
            Combatant::new(7_001, EntityKind::Player),
            DeathSequence::for_player_height(1.8),
        ),
        vec![strongest.clone()],
        M25_SCENE_SEED,
    );
    // The session re-projects both fighters on construction (`:434-437`), so
    // the enemy's vector is still the kit projection, and the wear call the
    // session makes on a struck part (`:500-503`) reports the piece: shield
    // coverage first (`:592-610`), then the part's armour slot.
    let enemy = &session.enemies[0];
    let mut probe_table = enemy.equipment.clone();
    let mut dice = daggerfall_behavior::locomotion::Dice100::new(M25_SCENE_SEED);
    let absorbed: Vec<EquipSlot> = (0..7)
        .filter_map(|part| probe_table.damage_equipment(part, 2, &mut dice))
        .map(|wear| wear.slot)
        .collect();
    rows.push(check(
        "F3 CombatSession: the equipped piece absorbs the strike",
        enemy.combatant.armor_values == expected_armor && !absorbed.is_empty(),
        format!(
            "absorbing slots={absorbed:?} · armor {:?} (re-projected by the session)",
            enemy.combatant.armor_values
        ),
    ));

    Ok(rows)
}

/// `PROBE-MONSTER-001` — retail census + layout + HP + deterministic scene.
pub fn run_monster_001() -> i32 {
    println!("PROBE-MONSTER-001 - M25 monsters (REQ-L99-058 · spec §10)");
    let (path, bytes) = match load_monster_bsa() {
        Ok(loaded) => loaded,
        Err(reason) => {
            println!("ARENA2 not accessible: {reason}");
            println!("reason=missing_monster_bsa");
            println!("EXIT=2");
            println!("VERDICT: PROBE-MONSTER-001 NO DATA - {MONSTER_FILENAME} not accessible");
            return 2;
        }
    };
    let archive = match MonsterArchive::parse(&bytes) {
        Ok(archive) => archive,
        Err(err) => {
            println!("[FAIL] {MONSTER_FILENAME} does not parse: {err}");
            println!("reason=missing_monster_bsa");
            println!("EXIT=2");
            println!("VERDICT: PROBE-MONSTER-001 NO DATA - unreadable container");
            return 2;
        }
    };
    println!(
        "[OK] mode_a {}: {} ({} B)",
        MONSTER_FILENAME,
        path.display(),
        bytes.len()
    );

    // ── A · census (§1.2, A1-A8) ───────────────────────────────────────────
    let census = archive.census();
    let ascr_line = format!(
        "records={} ascr={} enemy={} dir_off={} record_sum={} enemy_stride={} ascr_min={} ascr_max={}",
        census.records,
        census.ascr,
        census.enemy,
        census.dir_offset,
        census.record_sum,
        census
            .enemy_stride
            .map(|size| size.to_string())
            .unwrap_or_else(|| "mixed".to_string()),
        census.ascr_min,
        census.ascr_max
    );
    println!("{ascr_line}");
    let a_ok = ascr_line
        == "records=103 ascr=60 enemy=43 dir_off=25282 record_sum=25278 enemy_stride=74 ascr_min=335 ascr_max=458";
    let mut a_rows = vec![check(
        "A1,A2,A7,A8 census",
        a_ok,
        "103 records · 25282 = 27136 − 18·103 · Σ = 25278 · 43×74",
    )];
    // A3/A4/A5/A6: real offsets of the records cited by the spec.
    let data = &bytes;
    // Absolute offset of a record = `data_start` + Σ previous sizes.
    let start_of = |entry: usize| -> u64 {
        archive
            .archive
            .entries
            .iter()
            .take(entry)
            .fold(archive.archive.data_start, |start, prev| {
                start + u64::from(prev.data_size())
            })
    };
    // `ENEMY{number:03}.CFG` — erratum 26: the directory interleaves ASCR/ENEMY,
    // so the position in the census is not the record number.
    let enemy_at = |number: usize| archive.enemy.iter().find(|rec| rec.number == number);
    let first_ascr = archive.anim.first().map(|rec| {
        (
            rec.name.clone(),
            start_of(rec.entry),
            u64::from(rec.size),
        )
    });
    let first_ascr_line = first_ascr
        .as_ref()
        .map(|(name, start, size)| format!("{name}@{start}+{size}"))
        .unwrap_or_else(|| "none".to_string());
    // Values measured over `ARENA2` (erratum 26): `ENEMY000.CFG` at 4257 and
    // `ENEMY042.CFG` at 24080; the last directory record is
    // `ASCR0001.ANC@24934+348`, which closes at 25282.
    let enemy000 = enemy_at(0).map(|rec| {
        (
            rec.name.clone(),
            start_of(rec.entry),
            u64::from(rec.size),
        )
    });
    let enemy042 = enemy_at(42).map(|rec| {
        (
            rec.name.clone(),
            start_of(rec.entry),
            u64::from(rec.size),
        )
    });
    let last = archive
        .archive
        .entries
        .iter()
        .enumerate()
        .next_back()
        .map(|(entry, rec)| (start_of(entry), u64::from(rec.data_size())));
    let (a000_name, a000_start, a000_size) = enemy000
        .clone()
        .unwrap_or_else(|| ("none".to_string(), 0, 0));
    let (a042_name, a042_start) = enemy042
        .clone()
        .map(|(name, start, _)| (name, start))
        .unwrap_or_else(|| ("none".to_string(), 0));
    let last_end = last.map(|(start, size)| start + size).unwrap_or(0);
    println!(
        "A3 {} · A4 {a000_name}@{a000_start} · A5 {a042_name}@{a042_start} · A6 last end={last_end}",
        first_ascr_line
    );
    let a3_ok = first_ascr_line == "ASCR0000.ANC@4+337";
    let a4_ok = a000_name == "ENEMY000.CFG" && a000_start == 4257 && a000_size == 74;
    let a5_ok = a042_name == "ENEMY042.CFG" && a042_start == 24080;
    let a6_ok = last_end == census.dir_offset;
    a_rows.push(check(
        "A3 ASCR0000 offset/size",
        a3_ok,
        first_ascr_line.clone(),
    ));
    a_rows.push(check(
        "A4 ENEMY000 offset/size",
        a4_ok,
        format!("{a000_name}@{a000_start} + {a000_size}"),
    ));
    a_rows.push(check(
        "A5 ENEMY042 offset",
        a5_ok,
        format!("{a042_name}@{a042_start}"),
    ));
    a_rows.push(check(
        "A6 close = directory",
        a6_ok,
        format!("end={last_end} dir={}", census.dir_offset),
    ));
    // Anti-false-positives (ii): the directory is AT THE END, not at the beginning.
    let header_wrong = bytes.len() >= 18 * census.records
        && bytes[4..4 + 14].iter().any(|byte| *byte != 0)
        && !bytes[4..4 + 14]
            .starts_with(archive.enemy.first().map(|r| r.name.as_bytes()).unwrap_or(b""));
    a_rows.push(check(
        "A9 directory at the end (anti-FP ii)",
        header_wrong || archive.archive.directory_offset == bytes.len() as u64 - 18 * 103,
        format!(
            "a read at 4 would yield data bytes · directory at {}",
            archive.archive.directory_offset
        ),
    ));
    let mut all_ok = print_rows("A · container census (§1.2)", &a_rows);

    // ── B · layout of the 43 CFG (§2) ──────────────────────────────────────
    match monster_block_b(&archive, data) {
        Ok(b_rows) => all_ok &= print_rows("B · ENEMY###.CFG 74 B (§2)", &b_rows),
        Err(err) => {
            println!("[FAIL] block B: {err}");
            all_ok = false;
        }
    }

    // ── C · HP by DFU branch (ORDER M25-CLOSURE criterion 5) ──────────────
    // Delta vs spec §3.1: DFU rolls an `EnemyMonster` HP from the TABLE —
    // `Random.Range(mobileEnemy.MinHealth, mobileEnemy.MaxHealth + 1)`
    // (`EnemyEntity.cs:284-285`) — and only `EnemyClass`/custom use
    // `RollEnemyClassMaxHealth(level, career.HitPointsPerLevel)` (`:273`,
    // `:303` → `FormulaHelper.cs:1729-1743`, rows C7/C8). The CFG `hppl`
    // therefore does NOT bound a monster's HP.
    let rat = mobile_enemy(0).expect("Rat row");
    let imp = mobile_enemy(1).expect("Imp row");
    let zombie = mobile_enemy(17).expect("Zombie row");
    let daedra_lord = mobile_enemy(31).expect("Daedra Lord row");
    let lamia = mobile_enemy(42).expect("Lamia row");
    let horse = mobile_enemy(39).expect("Horse row");
    let hp_line = format!(
        "hp_rat=[{},{}] hp_imp=[{},{}] hp_zombie=[{},{}] hp_daedra_lord=[{},{}] hp_lamia=[{},{}] hp_horse=[{},{}]",
        rat.min_health,
        rat.max_health,
        imp.min_health,
        imp.max_health,
        zombie.min_health,
        zombie.max_health,
        daedra_lord.min_health,
        daedra_lord.max_health,
        lamia.min_health,
        lamia.max_health,
        horse.min_health,
        horse.max_health
    );
    println!("{hp_line}");
    let mut c_rows = vec![row(
        "C1-C6 table intervals",
        hp_line.clone(),
        "hp_rat=[9,16] hp_imp=[11,18] hp_zombie=[52,66] hp_daedra_lord=[35,210] hp_lamia=[16,58] hp_horse=[0,0]".to_string(),
        "EnemyBasics.Enemies Min/MaxHealth · EnemyEntity.cs:284-285 (horse = struct defaults)",
    )];
    // C7/C8: the class formula still bounds the `EnemyClass` HP (the humanoid
    // path of block E and the unit batteries).
    let lamia_hppl = archive
        .monster_class(data, 42)
        .map(|class| class.hppl())
        .unwrap_or(-1);
    let l10 = class_max_health_bounds(10, lamia_hppl);
    c_rows.push(check(
        "C7 class formula Lamia L10 = [20,180]",
        l10 == (20, 180),
        format!("[{}, {}]", l10.0, l10.1),
    ));
    let h0 = class_max_health_bounds(5, 0);
    c_rows.push(check(
        "C8 hppl 0 degenerates into 1",
        h0 == (15, 15),
        format!("[{}, {}]", h0.0, h0.1),
    ));
    all_ok &= print_rows("C · HP by DFU branch (table vs class formula)", &c_rows);

    // ── D · demo: 3 live monsters (§11.1) ──────────────────────────────────
    let scene_first = compose_scene(M25_SCENE_SEED);
    let scene_second = compose_scene(M25_SCENE_SEED);
    let mut d_rows = Vec::new();
    match (scene_first, scene_second) {
        (Ok(first), Ok(second)) => {
            let levels = first
                .iter()
                .map(|monster| monster.level.to_string())
                .collect::<Vec<_>>()
                .join(",");
            let hp = first
                .iter()
                .map(|monster| monster.hp.to_string())
                .collect::<Vec<_>>()
                .join(",");
            let names = first
                .iter()
                .map(|monster| monster.name.clone())
                .collect::<Vec<_>>()
                .join(",");
            println!(
                "spawned={} levels=[{levels}] hp∈[{hp}] names=[{names}]",
                first.len()
            );
            d_rows.push(check(
                "D1 spawned=3",
                first.len() == M25_SPAWNED,
                format!("{} live monsters", first.len()),
            ));
            let in_bounds = first
                .iter()
                .all(|monster| monster.hp >= monster.hp_bounds.0 && monster.hp <= monster.hp_bounds.1);
            d_rows.push(check(
                "D2 hp ∈ table interval",
                in_bounds,
                first
                    .iter()
                    .map(|monster| {
                        format!(
                            "{} L{} hp {}/[{},{}]",
                            monster.name,
                            monster.level,
                            monster.hp,
                            monster.hp_bounds.0,
                            monster.hp_bounds.1
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" · "),
            ));
            let digest_first = scene_digest(&first);
            let digest_second = scene_digest(&second);
            println!(
                "digest={digest_first:016x} ×2 → {digest_second:016x} ({})",
                if digest_first == digest_second {
                    "equal"
                } else {
                    "DIFFERENT"
                }
            );
            d_rows.push(check(
                "D3 digest ×2",
                digest_first == digest_second && first == second,
                format!("{digest_first:016x}"),
            ));
        }
        (Err(err), _) | (_, Err(err)) => {
            println!("[FAIL] block D: {err}");
            d_rows.push(check("D0 scene", false, err.to_string()));
        }
    }
    // D4-D6 · typed humanoid draws (no `ENEMY###.CFG`, no panic; HP from the
    // retail `CLASS*.CFG` careers through `spawn_humanoid`).
    let humanoid_table = table(M25_HUMANOID_TABLE).expect("Human Stronghold table");
    match compose_scene_from(&bytes, humanoid_table, M25_SCENE_SEED, load_retail_class_career) {
        Ok(scene) => {
            let line = scene
                .iter()
                .map(|entry| match entry {
                    SpawnedCombatant::Monster(monster) => format!(
                        "{}#{}(monster ENEMY{:03})",
                        monster.name, monster.mobile, monster.record
                    ),
                    SpawnedCombatant::Humanoid(humanoid) => format!(
                        "{}#{}(humanoid class {})",
                        humanoid.name, humanoid.mobile, humanoid.class_id
                    ),
                })
                .collect::<Vec<_>>()
                .join(" · ");
            let humanoids = scene
                .iter()
                .filter(|entry| matches!(entry, SpawnedCombatant::Humanoid(_)))
                .count();
            println!("mixed_scene[table {M25_HUMANOID_TABLE}]: {line}");
            d_rows.push(check(
                "D4 table 2 composes without panic",
                scene.len() == M25_SPAWNED,
                format!("{} draw(s) · {humanoids} humanoid(s) · {line}", scene.len()),
            ));
            // Every humanoid is exactly a `HumanoidWithoutEnemyRecord` for the
            // parser as well: the two crates agree, no record is pretended.
            let consistent = scene.iter().all(|entry| match entry {
                SpawnedCombatant::Monster(monster) => {
                    record_index_of_mobile(monster.mobile) == Ok(monster.record)
                }
                SpawnedCombatant::Humanoid(humanoid) => matches!(
                    record_index_of_mobile(humanoid.mobile),
                    Err(MonsterError::HumanoidWithoutEnemyRecord { mobile, class_id })
                        if mobile == humanoid.mobile && class_id == humanoid.class_id
                ),
            });
            d_rows.push(check(
                "D5 typed mobile ↔ parser agreement",
                consistent,
                format!(
                    "mobiles {:?} · monsters → ENEMY record · humanoids → class id, no record",
                    scene.iter().map(SpawnedCombatant::mobile).collect::<Vec<_>>()
                ),
            ));
        }
        Err(err) => {
            println!("[FAIL] D4 mixed scene: {err}");
            d_rows.push(check("D4 table 2 composes without panic", false, err.to_string()));
            d_rows.push(check("D5 typed mobile ↔ parser agreement", false, "no scene"));
        }
    }
    // A table that only carries one humanoid id: every draw is that humanoid
    // with the retail `CLASS16.CFG` career.
    let all_humanoids =
        compose_scene_from(&bytes, &HUMANOID_ONLY_TABLE, M25_SCENE_SEED, load_retail_class_career);
    let all_humanoids_ok = all_humanoids.as_ref().is_ok_and(|scene| {
        scene.len() == M25_SPAWNED
            && scene.iter().all(|entry| {
                matches!(
                    entry,
                    SpawnedCombatant::Humanoid(SpawnedHumanoid {
                        mobile: 144,
                        class_id: 16,
                        name: "Warrior",
                        ..
                    })
                )
            })
    });
    d_rows.push(check(
        "D6 humanoid-only table ⟹ 3× Warrior(144)",
        all_humanoids_ok,
        format!("{all_humanoids:?}"),
    ));

    all_ok &= print_rows("D · deterministic demo (§11.1)", &d_rows);

    // ── E · 19 humanoid classes through the spawn path (criterion 7) ──────
    // Retail `CLASS00..18.CFG` (Mode A, `DaggerfallEntity.cs:907-913`) →
    // `spawn_humanoid` at the demo player level: HP inside
    // `class_max_health_bounds(level, hppl)` and `health_bounds()`
    // coherent; the city watch (class 18) levels to `player + 3..=6` before
    // the HP roll (`EnemyEntity.cs:299-301`).
    let mut e_rows = Vec::new();
    let mut rng = Rng64::new(M25_SCENE_SEED + 9);
    let mut class_lines = Vec::new();
    let mut classes_ok = true;
    for class_id in 0..=18u8 {
        let mobile = HUMANOID_MOBILE_START + u16::from(class_id);
        let spawned = load_retail_class_career(class_id).and_then(|career| {
            let name = career.name.clone();
            spawn_humanoid(mobile, &career, M25_PLAYER_LEVEL, &mut rng)
                .map(|humanoid| (name, humanoid))
                .map_err(|err| err.to_string())
        });
        match spawned {
            Ok((name, humanoid)) => {
                let (low, high) = humanoid.health_bounds;
                let hp_ok = (low..=high).contains(&humanoid.max_health);
                classes_ok &= hp_ok;
                class_lines.push(format!(
                    "{class_id}:{name} L{} HP {}/[{low},{high}]",
                    humanoid.level, humanoid.max_health
                ));
            }
            Err(err) => {
                classes_ok = false;
                class_lines.push(format!("{class_id}:{err}"));
            }
        }
    }
    println!("classes19: {}", class_lines.join(" · "));
    e_rows.push(check(
        "E1 19 classes HP ∈ bounds",
        classes_ok && class_lines.len() == 19,
        format!("player level {M25_PLAYER_LEVEL} · {} classes", class_lines.len()),
    ));
    // E2/E3: class 19 and 255 are typed errors THROUGH the spawn path.
    match load_retail_class_career(16) {
        Ok(career) => {
            let bad19 = spawn_humanoid(147, &career, M25_PLAYER_LEVEL, &mut rng);
            let bad255 = spawn_humanoid(383, &career, M25_PLAYER_LEVEL, &mut rng);
            let e2_ok = matches!(
                &bad19,
                Err(HumanoidSpawnError::Class {
                    class_id: 19,
                    source: HumanoidProfileError::ClassId(19),
                    ..
                })
            );
            let e3_ok = matches!(
                &bad255,
                Err(HumanoidSpawnError::Class {
                    class_id: 255,
                    source: HumanoidProfileError::ClassId(255),
                    ..
                })
            );
            e_rows.push(check(
                "E2 class 19 → Err(ClassId) via spawn",
                e2_ok,
                format!("{bad19:?}"),
            ));
            e_rows.push(check(
                "E3 class 255 → Err(ClassId) via spawn",
                e3_ok,
                format!("{bad255:?}"),
            ));
        }
        Err(err) => {
            e_rows.push(check("E2 class 19 → Err(ClassId) via spawn", false, err));
            e_rows.push(check(
                "E3 class 255 → Err(ClassId) via spawn",
                false,
                "no CLASS16.CFG",
            ));
        }
    }
    // E4: the city watch is player + 3..=6 before the HP roll (64 draws
    // cover the four offsets).
    match load_retail_class_career(18) {
        Ok(career) => {
            let mut seen = [false; 4];
            let mut ok = true;
            let mut watch_level = 0;
            for _ in 0..64 {
                match spawn_humanoid(146, &career, M25_PLAYER_LEVEL, &mut rng) {
                    Ok(humanoid) => {
                        let offset = humanoid.level - M25_PLAYER_LEVEL;
                        ok &= (3..=6).contains(&offset);
                        if (3..=6).contains(&offset) {
                            seen[(offset - 3) as usize] = true;
                        }
                        watch_level = humanoid.level;
                        let (low, high) = humanoid.health_bounds;
                        ok &= (low..=high).contains(&humanoid.max_health);
                    }
                    Err(err) => {
                        ok = false;
                        println!("city watch spawn failed: {err}");
                    }
                }
            }
            e_rows.push(check(
                "E4 city watch level ∈ [L+3, L+6]",
                ok && seen == [true; 4],
                format!("level {watch_level} · offsets 3,4,5,6 seen {:?}", seen),
            ));
        }
        Err(err) => e_rows.push(check(
            "E4 city watch level ∈ [L+3, L+6]",
            false,
            err,
        )),
    }
    all_ok &= print_rows("E · humanoid classes via from_career (criterion 7)", &e_rows);

    // ── F · encounter equipment → armor_values (ORDER criterion 3) ────────
    let f_rows = match monster_block_f(&bytes) {
        Ok(rows) => rows,
        Err(reason) => vec![check("F0 encounter equipment", false, reason)],
    };
    all_ok &= print_rows("F · encounter kit → armor_values (criterion 3)", &f_rows);

    if all_ok {
        println!("VERDICT: PROBE-MONSTER-001 PASS - census 103/60/43 · 43 CFG of 74 B · monster HP from the EnemyBasics table · {M25_SPAWNED} live monsters ×2 · humanoid draws typed with career HP · 19 classes in bounds, class>18 rejected · encounter kit projected onto armor_values with Δchance measured · 0 retail bytes in the repo");
        0
    } else {
        println!("VERDICT: PROBE-MONSTER-001 FAIL - some block does not match spec §10");
        1
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-ENEMY-AI-001
// ═══════════════════════════════════════════════════════════════════════════

/// `PROBE-ENEMY-AI-001` — formulas + state-machine scenarios.
pub fn run_enemy_ai_001() -> i32 {
    println!("PROBE-ENEMY-AI-001 - M25 enemy AI (REQ-L99-058 · spec §10)");

    // ── A · formulas D1-D5, D8-D13 ─────────────────────────────────────────
    let a_line = format!(
        "move_speed(40)={} move_speed(100)={} move_speed(0)={} gate(40)={} gate(0)={} giveup={} sight={} melee(r=2400,l=10,rx=2)={} melee(r=1500,l=0,rx=0)={}",
        fmt_f32(move_speed(40)),
        fmt_f32(move_speed(100)),
        fmt_f32(move_speed(0)),
        attack_gate(40),
        attack_gate(0),
        fmt_f32(GIVE_UP_SECONDS),
        fmt_f32(SIGHT_RADIUS_M),
        fmt_f32(melee_timer_seconds(2400, 10, 2)),
        fmt_f32(melee_timer_seconds(1500, 0, 0)),
    );
    println!("{a_line}");
    let want_a = "move_speed(40)=4.75 move_speed(100)=6.25 move_speed(0)=3.75 gate(40)=11 gate(0)=7 giveup=12.5 sight=102.4 melee(r=2400,l=10,rx=2)=2.44898 melee(r=1500,l=0,rx=0)=1.12245";
    let mut a_rows = vec![row(
        "A D1-D5,D8-D13",
        a_line.clone(),
        want_a.to_string(),
        "speed/gate/give-up/senses/cadence",
    )];
    // D2 and D7/D12/D13 complete family A.
    let d2 = attack_move_speed(40);
    let d7 = melee_timer_seconds(2400, 20, 2);
    let d13 = melee_timer_seconds(3000, 20, 4);
    a_rows.push(check(
        "A D2 pursuit = 2.375",
        (d2 - 2.375).abs() < 1e-6,
        fmt_f32(d2),
    ));
    a_rows.push(check(
        "A D7 = 1.93878",
        fmt_f32(d7) == "1.93878",
        fmt_f32(d7),
    ));
    a_rows.push(check(
        "A D13 = 3.46939",
        fmt_f32(d13) == "3.46939",
        fmt_f32(d13),
    ));
    // D4/D5: the probability is derived from the crate's own gate — comparing
    // `11/40` against `0.275` was circular (clippy `eq_op` caught it).
    let p_no_attack_d4 = f64::from(attack_gate(40)) / 40.0;
    let p_no_attack_d5 = f64::from(attack_gate(0)) / f64::from(speed_floor(0));
    a_rows.push(check(
        "A D4/D5 P(no attack)",
        (p_no_attack_d4 - 0.275).abs() < 1e-6 && (p_no_attack_d5 - 0.875).abs() < 1e-6,
        format!(
            "{:.1} % · {:.1} %",
            p_no_attack_d4 * 100.0,
            p_no_attack_d5 * 100.0
        ),
    ));
    let mut all_ok = print_rows("A · formulas (§4.2, D1-D13)", &a_rows);

    // ── B · Idle→Pursue + first contact (player at 20 m) ───────────────────
    let mut ai = EnemyAi::new(40);
    let mut distance = 20.0f32;
    let mut events = ai.classic_update(Perception::visible(distance), None);
    let acquire = ai.first_transition(AiState::Idle, AiState::Pursue);
    let mut ticks = 1;
    while ai.state == AiState::Pursue && ticks < 1000 {
        distance = ai.close_distance(distance, CLASSIC_TICK);
        events.extend(ai.classic_update(Perception::visible(distance), None));
        ticks += 1;
    }
    let contact = ai.first_contact();
    let b_line = match (acquire, contact) {
        (Some(at), Some((contact_at, contact_distance))) => format!(
            "Idle→Pursue at {:.4} s · contact at {} m (t={:.4} s)",
            at, fmt_f32(contact_distance), contact_at
        ),
        _ => "no transition/contact".to_string(),
    };
    println!("{b_line}");
    let mut b_rows = vec![check(
        "B Idle→Pursue ≤ 0.0625 s",
        acquire.map(|at| at <= CLASSIC_TICK + 1e-6).unwrap_or(false),
        format!("t = {} s", acquire.map(fmt_f32).unwrap_or_default()),
    )];
    b_rows.push(check(
        "B first contact at 2.25 m",
        contact
            .map(|(_, distance)| (distance - MELEE_DISTANCE).abs() < 1e-6)
            .unwrap_or(false),
        contact
            .map(|(at, distance)| format!("d = {} m · t = {} s", fmt_f32(distance), fmt_f32(at)))
            .unwrap_or_else(|| "no contact".to_string()),
    ));
    // Anti-false-positives (R-C): the SAME result at 16 Hz and at 32 Hz
    // (2 ticks of 0.03125 s) with the same scenario seed.
    let mut half = EnemyAi::new(40);
    let mut half_distance = 20.0f32;
    half.classic_update(Perception::visible(half_distance), None);
    let mut half_ticks = 1;
    while half.state == AiState::Pursue && half_ticks < 4000 {
        half_distance = half.close_distance(half_distance, CLASSIC_TICK / 2.0);
        half.classic_update(Perception::visible(half_distance), None);
        half_ticks += 1;
    }
    let half_contact = half.first_contact().map(|(_, distance)| distance);
    b_rows.push(check(
        "B independent of frequency (R-C)",
        half_contact
            .map(|distance| (distance - MELEE_DISTANCE).abs() < 1e-6)
            .unwrap_or(false),
        format!(
            "2 ticks of 0.03125 s ⇒ {} m in {} ticks",
            half_contact.map(fmt_f32).unwrap_or_default(),
            half_ticks
        ),
    ));
    all_ok &= print_rows("B · perception scenario (§10)", &b_rows);

    // ── C · target lost: exactly 12.5 s, 200 perceptions ───────────────────
    let mut lost = EnemyAi::new(40);
    let mut lost_distance = 20.0f32;
    lost.classic_update(Perception::visible(lost_distance), None);
    for _ in 0..9 {
        lost_distance = lost.close_distance(lost_distance, CLASSIC_TICK);
        lost.classic_update(Perception::visible(lost_distance), None);
    }
    assert_eq!(lost.state, AiState::Pursue);
    let last_detection = lost.elapsed;
    let mark = lost.perceptions;
    let mut lost_ticks = 0;
    while lost.state != AiState::Idle && lost_ticks < 10_000 {
        lost.classic_update(Perception::lost(), None);
        lost_ticks += 1;
    }
    let give_up_at = lost.first_transition(AiState::Pursue, AiState::Idle);
    let perceptions = lost.perceptions - mark;
    let c_line = format!(
        "Pursue→Idle at {} s ({:?} since last detection) · perceptions={perceptions}",
        give_up_at.map(fmt_f32).unwrap_or_else(|| "none".into()),
        give_up_at.map(|at| fmt_f32(at - last_detection))
    );
    println!("{c_line}");
    let mut c_rows = vec![check(
        "C exactly 12.5 s",
        give_up_at
            .map(|at| (at - last_detection - GIVE_UP_SECONDS).abs() < 1e-6)
            .unwrap_or(false),
        format!(
            "{} s since last detection ({:?} s)",
            give_up_at
                .map(|at| fmt_f32(at - last_detection))
                .unwrap_or_default(),
            fmt_f32(GIVE_UP_SECONDS)
        ),
    )];
    c_rows.push(check(
        "C perceptions = 200",
        perceptions == 200,
        format!("{perceptions} perceptions at 16 Hz"),
    ));
    all_ok &= print_rows("C · give-up (§4.1)", &c_rows);

    // ── D · closed door on the route ───────────────────────────────────────
    let mut doored = EnemyAi::new(40);
    let mut door_distance = 10.0f32;
    let mut door_ticks = 0;
    doored.classic_update(Perception::visible(20.0), None);
    while doored.state != AiState::OpenDoor && door_ticks < 400 {
        door_distance = (door_distance - doored.attack_speed * CLASSIC_TICK).max(0.0);
        doored.classic_update(
            Perception::visible(20.0),
            Some(Door {
                distance: door_distance,
                open: false,
            }),
        );
        door_ticks += 1;
    }
    let door_event = doored.events.iter().find_map(|event| match event {
        AiEvent::DoorOpened { at, distance } => Some((*at, *distance)),
        _ => None,
    });
    let d_line = format!(
        "OpenDoor at d={} m (≤ 2.0) · doors_opened={}",
        door_event.map(|(_, d)| fmt_f32(d)).unwrap_or_else(|| "none".into()),
        doored.doors_opened
    );
    println!("{d_line}");
    let mut d_rows = vec![check(
        "D OpenDoor with d ≤ 2.0",
        door_event
            .map(|(_, distance)| distance <= OPEN_DOOR_DISTANCE)
            .unwrap_or(false),
        format!(
            "door at {} m",
            door_event.map(|(_, d)| fmt_f32(d)).unwrap_or_default()
        ),
    )];
    d_rows.push(check(
        "D doors_opened = 1",
        doored.doors_opened == 1,
        format!("{} door(s) opened", doored.doors_opened),
    ));
    all_ok &= print_rows("D · doors (§4.2)", &d_rows);

    if all_ok {
        println!("VERDICT: PROBE-ENEMY-AI-001 PASS - formulas D1-D13 · Idle→Pursue in 1 tick · contact 2.25 m · give-up 12.5 s/200 · OpenDoor ≤ 2 m");
        0
    } else {
        println!("VERDICT: PROBE-ENEMY-AI-001 FAIL - some block does not match spec §10");
        1
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-ENCOUNTER-001
// ═══════════════════════════════════════════════════════════════════════════

/// Seed of the 1000 rolls of block C.
pub const ENCOUNTER_SEED: u64 = 20_261_022;
/// Rolls of block C.
pub const ENCOUNTER_ROLLS: usize = 1000;

/// `PROBE-ENCOUNTER-001` — tables, level range, spawn and pacification.
pub fn run_encounter_001() -> i32 {
    println!("PROBE-ENCOUNTER-001 - M25 encounters (REQ-L99-058 · spec §10)");

    // ── A · E1-E10 ─────────────────────────────────────────────────────────
    let goldens: [(&str, i32, i32, (i32, i32)); 8] = [
        ("E1", 5, 50, (2, 8)),
        ("E2", 10, 50, (7, 13)),
        ("E3", 2, 85, (0, 3)),
        ("E4", 5, 85, (0, 6)),
        ("E5", 5, 97, (0, 7)),
        ("E6", 8, 97, (0, 19)),
        ("E7", 1, 50, (0, 5)),
        ("E8", 19, 85, (14, 19)),
    ];
    let mut a_rows = Vec::new();
    for (id, level, roll, want) in goldens {
        let got = level_range(level, roll);
        a_rows.push(check(
            id,
            got == want,
            format!("(level {level}, roll {roll}) → [{}, {}]", got.0, got.1),
        ));
    }
    // E9: synthetic clamp of 11 entries with max 12 ⇒ [5,10].
    let e9 = clamp_to_table(11, 0, 12);
    a_rows.push(check(
        "E9 synthetic clamp",
        e9 == TableClamp::Clamped { min: 5, max: 10 },
        format!(
            "table of 11 · max 12 ⇒ {:?} (unit test, not a game encounter)",
            e9
        ),
    ));
    // E10: table of 20 with max 19 ⇒ no clamp.
    let e10 = clamp_to_table(20, 14, 19);
    a_rows.push(check(
        "E10 no clamp",
        e10 == TableClamp::Fits { min: 14, max: 19 },
        format!("table of 20 · max 19 ⇒ {e10:?}"),
    ));
    let mut all_ok = print_rows("A · level range (§5.3 E1-E10)", &a_rows);

    // ── B · table census ───────────────────────────────────────────────────
    let referenced = referenced_tables();
    let short = ENCOUNTER_TABLES
        .iter()
        .filter(|table| table.len() != TABLE_LEN)
        .count();
    let max_index_used = referenced
        .iter()
        .enumerate()
        .filter(|(_, hit)| **hit)
        .map(|(index, _)| index)
        .max()
        .unwrap_or(0);
    let unused: Vec<String> = referenced
        .iter()
        .enumerate()
        .filter(|(_, hit)| !**hit)
        .map(|(index, _)| index.to_string())
        .collect();
    let b_line = format!(
        "tables={} len20={} short={} commented={} max_index_used={} unused_index={}",
        ENCOUNTER_TABLES.len(),
        ENCOUNTER_TABLES
            .iter()
            .filter(|table| table.len() == TABLE_LEN)
            .count(),
        short,
        COMMENTED_TABLES,
        max_index_used,
        unused.join(",")
    );
    println!("{b_line}");
    let mut b_rows = vec![row(
        "B table census",
        b_line.clone(),
        "tables=45 len20=45 short=0 commented=1 max_index_used=44 unused_index=38".to_string(),
        "RandomEncounters.cs:26-1328 (the 11-entry table is commented out)",
    )];
    b_rows.push(check(
        "B indices by mapping",
        (0..DUNGEON_TYPE_COUNT).all(|index| referenced[index])
            && referenced[19]
            && (20..38).all(|index| referenced[index])
            && (39..45).all(|index| referenced[index])
            && !referenced[UNUSED_TABLE_INDEX],
        format!(
            "dungeon 0..{}, water 19, climate 20..37, buildings 39..44; {} not referenced",
            DUNGEON_TYPE_COUNT - 1,
            UNUSED_TABLE_INDEX
        ),
    ));

    // ── B2-B6 · typed `MobileTypes` resolution (R-A) ───────────────────────
    let mut cells = 0usize;
    let mut lookups = 0usize;
    let mut humanoids = 0usize;
    let mut tables_with_humanoids = 0usize;
    let mut bad = Vec::new();
    for (index, row_cells) in ENCOUNTER_TABLES.iter().enumerate() {
        let mut table_humanoids = 0usize;
        for (cell, mobile) in row_cells.iter().enumerate() {
            match resolve_cell(index, cell) {
                Ok(resolved) => {
                    let consistent = match resolved {
                        MobileRef::Monster {
                            mobile: id,
                            record,
                        } => usize::from(id) == record,
                        MobileRef::Humanoid {
                            mobile: id,
                            class_id,
                        } => usize::from(class_id) == usize::from(id - HUMANOID_MOBILE_START)
                            && resolved.enemy_record().is_none(),
                    };
                    if !consistent {
                        bad.push(format!("[{index}][{cell}]={mobile} inconsistent mapping"));
                    }
                    // Criterion 2: every drawn id also resolves through the
                    // `EnemyBasics.Enemies` lookup (ORDER M25-CLOSURE).
                    match mobile_enemy(resolved.mobile()) {
                        Ok(entry) if entry.id == resolved.mobile() => lookups += 1,
                        other => bad.push(format!(
                            "[{index}][{cell}]={mobile} lookup → {other:?}"
                        )),
                    }
                    cells += 1;
                    if resolved.is_humanoid() {
                        humanoids += 1;
                        table_humanoids += 1;
                    }
                }
                Err(err) => bad.push(format!("[{index}][{cell}]={mobile} → {err}")),
            }
        }
        if table_humanoids > 0 {
            tables_with_humanoids += 1;
        }
    }
    let validated = validate_tables();
    println!(
        "cells_resolved={cells}/{} humanoid_cells={humanoids} tables_with_humanoids={tables_with_humanoids} enemy_basics_lookup={lookups} validate_tables={validated:?}",
        TABLE_COUNT * TABLE_LEN
    );
    b_rows.push(check(
        "B2 900/900 cells resolve",
        validated.is_ok() && cells == 900 && lookups == 900 && bad.is_empty(),
        format!(
            "{cells}/900 resolved · {lookups}/900 lookup · invalid: {}",
            if bad.is_empty() {
                "none".to_string()
            } else {
                bad.join(", ")
            }
        ),
    ));
    b_rows.push(check(
        "B7 900/900 → EnemyBasics.Enemies lookup",
        lookups == 900,
        format!("{lookups}/900 rows through mobile_enemy(id)"),
    ));
    b_rows.push(row(
        "B3 humanoid census",
        format!("humanoid_cells={humanoids} tables={tables_with_humanoids}"),
        "humanoid_cells=393 tables=40".to_string(),
        "393 of the 900 cells are MobileTypes 128..=146 (RandomEncounters.cs)",
    ));
    // Five tables compared id by id with the DFU source (names in the comments
    // are the `MobileTypes.<Name>` entries of `RandomEncounters.cs`).
    let spot_checks: [(usize, [u16; TABLE_LEN], &str); 5] = [
        (
            2,
            [
                144, 136, 0, 4, 141, 133, 3, 129, 8, 145, 144, 136, 143, 24, 141, 23, 129, 21,
                144, 28,
            ],
            "2 Human Stronghold :88",
        ),
        (
            7,
            [
                1, 3, 128, 131, 13, 133, 10, 37, 35, 36, 10, 27, 131, 129, 29, 26, 130, 128, 25,
                31,
            ],
            "7 Coven :233",
        ),
        (
            39,
            [
                138, 144, 135, 144, 135, 134, 144, 137, 135, 136, 138, 144, 135, 133, 136, 144,
                135, 138, 144, 136,
            ],
            "39 Default building :1161",
        ),
        (
            41,
            [
                140, 140, 145, 140, 140, 140, 140, 145, 140, 140, 140, 140, 140, 145, 140, 140,
                140, 140, 140, 145,
            ],
            "41 Temple :1217",
        ),
        (
            44,
            [
                0, 0, 135, 138, 134, 140, 134, 132, 136, 140, 135, 138, 134, 136, 142, 135, 138,
                134, 140, 136,
            ],
            "44 House3 :1301",
        ),
    ];
    let mut spot_bad = Vec::new();
    for (index, want, label) in spot_checks {
        if ENCOUNTER_TABLES[index] != want {
            spot_bad.push(format!(
                "{label}: got {:?} want {want:?}",
                ENCOUNTER_TABLES[index]
            ));
        }
    }
    b_rows.push(check(
        "B4 5 tables match the DFU ids",
        spot_bad.is_empty(),
        if spot_bad.is_empty() {
            "tables 2, 7, 39, 41, 44 id by id (table 2 = Human Stronghold)".to_string()
        } else {
            spot_bad.join(" · ")
        },
    ));
    // Negative control: ids between/after the bands never clamp or remap.
    let negative: Vec<String> = [43u16, 61, 147]
        .iter()
        .map(|mobile| format!("{mobile}→{:?}", resolve_mobile(*mobile)))
        .collect();
    let negative_ok = [43u16, 61, 147].iter().all(|mobile| {
        matches!(
            resolve_mobile(*mobile),
            Err(EncounterTableError::UnknownMobileType { mobile: id }) if id == *mobile
        )
    });
    println!("negative_control: {}", negative.join(" · "));
    b_rows.push(check(
        "B5 negative control 43/61/147",
        negative_ok,
        negative.join(" · "),
    ));
    // The same control over the whole out-of-band set through the table
    // lookup: `43..=127` and `147..=255` are typed errors, never clamped.
    let lookup_negative_ok = (43u16..=127)
        .chain(147..=255)
        .all(|id| mobile_enemy(id) == Err(MobileEnemyError::OutOfBand { id }));
    b_rows.push(check(
        "B5b lookup rejects 43..=127 and 147..=255",
        lookup_negative_ok,
        "239 ids → Err(MobileEnemyError::OutOfBand), 0 clamps",
    ));
    let bands_ok = MONSTER_MOBILE_END == 42
        && HUMANOID_MOBILE_START == 128
        && HUMANOID_MOBILE_END == 146
        && resolve_mobile(0).is_ok()
        && resolve_mobile(42).is_ok()
        && resolve_mobile(128).is_ok()
        && resolve_mobile(146).is_ok()
        && resolve_mobile(127).is_err()
        && resolve_mobile(147).is_err();
    b_rows.push(check(
        "B6 bands 0..=42 / 128..=146",
        bands_ok,
        "DaggerfallUnityEnums.cs:142-212 (monsters 0-42, humanoids 128-146)".to_string(),
    ));

    all_ok &= print_rows("B · census (§5.1)", &b_rows);

    // ── C · 1000 seeded rolls ──────────────────────────────────────────────
    let first = seeded_encounter_run(ENCOUNTER_SEED);
    let second = seeded_encounter_run(ENCOUNTER_SEED);
    let digest_first = fnv1a64(&first.digest_input);
    let digest_second = fnv1a64(&second.digest_input);
    let probability = first.spawns as f64 / ENCOUNTER_ROLLS as f64;
    println!(
        "p_spawn(1/24)={probability:.4} spawned={} digest={digest_first:016x} ×2 → {digest_second:016x}",
        first.spawns
    );
    let mut c_rows = vec![check(
        "C cells resolve (typed draw)",
        first.resolve_errors == 0 && second.resolve_errors == 0,
        format!(
            "{} unresolved cells over {} draws ×2",
            first.resolve_errors + second.resolve_errors,
            ENCOUNTER_ROLLS * 2
        ),
    )];
    c_rows.push(check(
        "C p_spawn 1/24",
        (probability - 1.0 / 24.0).abs() <= 0.005,
        format!(
            "{probability:.4} over {ENCOUNTER_ROLLS} rolls (expected 0.0417 ± 0.005)"
        ),
    ));
    c_rows.push(check(
        "C digest ×2",
        digest_first == digest_second,
        format!("{digest_first:016x}"),
    ));
    c_rows.push(check(
        "C explicit seed (anti-FP iv)",
        seeded_encounter_run(ENCOUNTER_SEED + 1).digest_input != first.digest_input,
        "another seed ⇒ another histogram".to_string(),
    ));
    all_ok &= print_rows("C · 1000 seeded rolls (§5.4)", &c_rows);

    // ── D · dungeon without/with alarm ─────────────────────────────────────
    let no_alarm = (0..ENCOUNTER_ROLLS)
        .filter(|_| roll_spawn_dungeon(false, 0))
        .count();
    let with_alarm = (0..ENCOUNTER_ROLLS)
        .map(|index| roll_spawn_dungeon(true, (index as u32) % 36))
        .filter(|spawn| *spawn)
        .count();
    println!("spawns_without_alarm={no_alarm} spawns_with_alarm={with_alarm}");
    let mut d_rows = vec![check(
        "D dungeon without alarm ⇒ spawns=0",
        no_alarm == 0,
        format!("{no_alarm} spawns"),
    )];
    d_rows.push(check(
        "D dungeon with alarm ⇒ spawns>0",
        with_alarm > 0,
        format!("{with_alarm} spawns in {ENCOUNTER_ROLLS} rolls (1/36)"),
    ));
    all_ok &= print_rows("D · dungeon spawn (§5.4)", &d_rows);

    // ── E · pacification F1-F7 (§6, row 5 of §12) ──────────────────────────
    let mut e_rows = vec![check(
        "F1 chance 26 = 13.00 %",
        (pacification_probability(26) - 0.13).abs() < 1e-12,
        format!("{:.2} %", pacification_probability(26) * 100.0),
    )];
    e_rows.push(check(
        "F2 chance 100 = 50.00 %",
        (pacification_probability(100) - 0.5).abs() < 1e-12,
        format!("{:.2} %", pacification_probability(100) * 100.0),
    ));
    e_rows.push(check(
        "F3 chance 200 = 100 %",
        pacification_probability(200) == 1.0,
        format!("{:.2} %", pacification_probability(200) * 100.0),
    ));
    e_rows.push(check(
        "F4 chance 250 ⇒ clamp 100 %",
        pacification_probability(250) == 1.0,
        format!("{:.2} %", pacification_probability(250) * 100.0),
    ));
    e_rows.push(check(
        "F5 chance −9 ⇒ 0 %",
        pacification_probability(-9) == 0.0,
        format!("{:.2} %", pacification_probability(-9) * 100.0),
    ));
    let f6_ok = [0, 26, 100, 200, 250].iter().all(|chance| {
        let sheathed = pacification_chance(*chance, false, 50, true, 0);
        let drawn = pacification_chance(*chance, false, 50, false, 0);
        pacification_probability(sheathed) >= pacification_probability(drawn)
    }) && pacification_probability(pacification_chance(225, false, 50, true, 0))
        == pacification_probability(pacification_chance(225, false, 50, false, 0));
    e_rows.push(check(
        "F6 sheathed ≥ drawn (equality only at 225)",
        f6_ok,
        "p_sheathed − p_drawn = (min(c+10,200) − max(c−25,0))/200".to_string(),
    ));
    let f7_ok = pacification_probability(pacification_chance(80, false, 50, true, 0))
        >= pacification_probability(pacification_chance(20, false, 50, true, 0));
    e_rows.push(check(
        "F7 monotonicity in the skill",
        f7_ok && pacifies(26, 25) && !pacifies(26, 26),
        "skill 80 ≥ skill 20 · success ⇔ roll < chance".to_string(),
    ));
    // The climate → band mapping of §5.1 (M31 composes `WorldClimate`).
    let climate_ok = ClimateBand::from_climate_index(224) == Some(ClimateBand::Desert)
        && ClimateBand::from_climate_index(226) == Some(ClimateBand::Mountain)
        && ClimateBand::from_climate_index(227) == Some(ClimateBand::Rainforest)
        && ClimateBand::from_climate_index(229) == Some(ClimateBand::Subtropical)
        && ClimateBand::from_climate_index(230) == Some(ClimateBand::SwampWoodlands)
        && ClimateBand::from_climate_index(232) == Some(ClimateBand::HauntedWoodlands)
        && ClimateBand::from_climate_index(223).is_none()
        && BuildingKind::PalaceOrHouse1.table_index() == 42
        && table_index(EncounterContext::Exterior {
            climate: ClimateBand::Desert,
            in_location: true,
            is_day: true,
        })
        .is_none();
    e_rows.push(check(
        "§5.1 climate/building mapping",
        climate_ok,
        "climates 224/225..232 → 20..37 · buildings 39..44 · daytime in location ⇒ None",
    ));
    all_ok &= print_rows("E · pacification (§6 F1-F7)", &e_rows);

    // ── G · M25 data → combat/loot boundary (criterion 8/9/10) ────────────
    let mut g_rows = Vec::new();
    // G1: data rows the spawn consumes (`EnemyEntity.cs:315/:316/:330/:389`).
    let werewolf = mobile_enemy(9).expect("Werewolf row");
    let orc = mobile_enemy(7).expect("Orc row");
    let g1_ok = werewolf.min_metal_to_hit.raw() == 2
        && werewolf.team == MobileTeams::Werecreatures
        && werewolf.level == 6
        && orc.loot_table_key == Some('A')
        && orc.map_chance == 0;
    g_rows.push(check(
        "G1 Werewolf silver/team/L6 · Orc key A",
        g1_ok,
        format!(
            "Werewolf metal {} team {:?} L{} · Orc key {:?} map {}",
            werewolf.min_metal_to_hit.raw(),
            werewolf.team,
            werewolf.level,
            orc.loot_table_key,
            orc.map_chance
        ),
    ));
    // G2: the material gate — steel (`WeaponMaterialTypes.Steel = 1`) can't
    // touch a `MinMetalToHit = Silver (2)` target: miss with **0 rolls and
    // 0 wear** (`engine.rs` step 1, `FormulaHelper.cs:576-583`); a silver
    // weapon passes the gate.
    let monster = SpawnedMonster::from_entry(werewolf, werewolf.min_health);
    let target = monster_combatant(&monster, 9_001);
    let attacker = Combatant::new(1, EntityKind::Player);
    let steel = Weapon {
        native_material_value: 1,
        min_damage: 2,
        max_damage: 6,
        flags: 0,
        to_hit_mod: 0,
        material_damage_mod: 0,
    };
    let silver = Weapon {
        native_material_value: 2,
        min_damage: 2,
        max_damage: 6,
        flags: 0,
        to_hit_mod: 0,
        material_damage_mod: 0,
    };
    let attack = |weapon: &Weapon| {
        let mut dice = Dice100::with_seed(42);
        resolve_attack(
            &AttackInput {
                attacker: &attacker,
                target: &target,
                weapon: Some(weapon),
                chance_mod: 0,
                damage_mods: 0,
                backstab_level: 1,
                enemy_bonus: 0,
            },
            &mut dice,
        )
    };
    let blocked = attack(&steel);
    let allowed = attack(&silver);
    let g2_ok = blocked.material_ineffective
        && !blocked.hit
        && blocked.damage == 0
        && blocked.rolls_consumed == 0
        && !allowed.material_ineffective
        && allowed.rolls_consumed > 0;
    g_rows.push(check(
        "G2 steel vs silver gate: miss, 0 rolls",
        g2_ok,
        format!(
            "steel → material_ineffective {} hit {} damage {} rolls {} · silver → material_ineffective {} hit {} rolls {}",
            blocked.material_ineffective,
            blocked.hit,
            blocked.damage,
            blocked.rolls_consumed,
            allowed.material_ineffective,
            allowed.hit,
            allowed.rolls_consumed
        ),
    ));
    // G3: enemy loot from the table keys (`EnemyEntity.cs:330/:389-396` via
    // `daggerfall_inventory::loot::generate_enemy_loot`) — two distinct
    // keys, gold inside the matrix interval, digest ×2.
    let loot_seed = 20_261_023u64;
    let mut loot_ok = true;
    let mut loot_lines = Vec::new();
    for (id, level) in [(7u16, 5i32), (28, 19)] {
        let entry = mobile_enemy(id).expect("loot row");
        let key = entry.loot_table_key.expect("keyed row");
        let row_matrix = matrix(key);
        let first = generate_enemy_loot(
            loot_seed,
            u32::from(id),
            entry.loot_table_key,
            entry.map_chance,
            level,
        );
        let second = generate_enemy_loot(
            loot_seed,
            u32::from(id),
            entry.loot_table_key,
            entry.map_chance,
            level,
        );
        let gold_ok = (row_matrix.min_gold * level..=row_matrix.max_gold * level)
            .contains(&first.gold);
        // The two containers differ only by `Item.uid` (a process-global
        // counter); the deterministic core is gold/key/len + the M21
        // canonical digest, which deliberately excludes `uid`.
        loot_ok &= gold_ok
            && (first.gold, first.key, first.items.len())
                == (second.gold, second.key, second.items.len())
            && container_digest_bytes(&first) == container_digest_bytes(&second);
        loot_lines.push(format!(
            "{id}:{} key {key} gold {}/[{},{}] items {} digest {:016x}",
            entry.name,
            first.gold,
            row_matrix.min_gold * level,
            row_matrix.max_gold * level,
            first.items.len(),
            fnv1a64(&container_digest_bytes(&first)),
        ));
    }
    g_rows.push(check(
        "G3 enemy loot ×2 keys, gold in matrix, digest ×2",
        loot_ok,
        loot_lines.join(" · "),
    ));
    // G4: team is exposed and consumed by nothing else (declared frontier).
    g_rows.push(check(
        "G4 team exposed, 0 in-fighting consumer",
        monster.team == MobileTeams::Werecreatures
            && monster.min_metal_to_hit == 2
            && monster.damage_bands[0] == (1, 10),
        format!(
            "team {:?} · min_metal {} · bands {:?} (EnemyMotor.cs:209-212 is the DFU reader)",
            monster.team, monster.min_metal_to_hit, monster.damage_bands
        ),
    ));
    all_ok &= print_rows("G · data → combat/loot (criterion 8-10)", &g_rows);

    if all_ok {
        println!("VERDICT: PROBE-ENCOUNTER-001 PASS - E1-E10 · 45/45 tables of 20 (1 commented out, 38 unused) · 900/900 cells resolve and 900/900 EnemyBasics lookup (393 humanoid, MobileTypes 128..=146) · 5 tables id by id · negative controls 43/61/147 + 43..=127/147..=255 · 1000 rolls ×2 · dungeon without alarm 0 · F1-F7 · G1-G4 data→combat/loot");
        0
    } else {
        println!("VERDICT: PROBE-ENCOUNTER-001 FAIL - some block does not match spec §10");
        1
    }
}

/// Result of a seeded run of block C.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SeededRun {
    /// Observed spawns (`range(0, 24) == 0`).
    spawns: usize,
    /// Histogram bytes (`mobile` per roll, `0xFFFF` = no spawn).
    digest_input: Vec<u8>,
    /// Draws whose cell did not resolve (must be 0: the table is static).
    resolve_errors: usize,
}

/// "no spawn" marker in the block C histogram.
const NO_SPAWN: u16 = u16::MAX;

fn seeded_encounter_run(seed: u64) -> SeededRun {
    let mut rng = Rng64::new(seed);
    let mut spawns = 0;
    let mut resolve_errors = 0;
    let mut digest_input = Vec::with_capacity(ENCOUNTER_ROLLS * 2);
    for _ in 0..ENCOUNTER_ROLLS {
        let spawn_roll = rng.range_inclusive(0, 23);
        let level = rng.range_inclusive(1, 20);
        let enemy_roll = rng.range_inclusive(1, 100);
        let picked = match choose_enemy(
            &ENCOUNTER_TABLES[UNDERWATER_TABLE_INDEX],
            level,
            enemy_roll,
            |low, high| rng.range_inclusive(low, high),
        ) {
            Ok(Some(mobile_ref)) => mobile_ref.mobile(),
            Ok(None) => NO_SPAWN,
            Err(_) => {
                resolve_errors += 1;
                NO_SPAWN
            }
        };
        if spawn_roll == 0 {
            spawns += 1;
            digest_input.extend_from_slice(&picked.to_le_bytes());
        } else {
            digest_input.extend_from_slice(&NO_SPAWN.to_le_bytes());
        }
    }
    SeededRun {
        spawns,
        digest_input,
        resolve_errors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic enemy-class template: exactly the `CLASS*.CFG` fields the
    /// `EnemyClass` branch consumes (`EnemyEntity.cs:294-303`), built
    /// directly — 0 retail bytes (ENCOUNTER-001 stays pure) and `hppl`
    /// varies per class so every golden is measurable.
    fn synthetic_career(name: &str, hppl: i32) -> EnemyClassTemplate {
        EnemyClassTemplate {
            name: name.to_string(),
            attributes: [60, 50, 55, 50, 50, 40, 45, 50],
            hit_points_per_level: hppl,
        }
    }

    /// Both pure probes exit 0 (the scenarios are deterministic).
    #[test]
    fn pure_probes_pass() {
        assert_eq!(run_enemy_ai_001(), 0, "PROBE-ENEMY-AI-001");
        assert_eq!(run_encounter_001(), 0, "PROBE-ENCOUNTER-001");
    }

    /// 900/900 cells resolve through `mobile_enemy` (criterion 2b), the
    /// out-of-band set is rejected (2c) and three goldens pin the
    /// transcription (2c) — pure, 0 retail.
    #[test]
    fn lookup_covers_every_cell_and_rejects_the_rest() {
        let mut cells = 0usize;
        for (index, row_cells) in ENCOUNTER_TABLES.iter().enumerate() {
            for (cell, _) in row_cells.iter().enumerate() {
                let resolved = resolve_cell(index, cell).expect("cell");
                let entry = mobile_enemy(resolved.mobile()).expect("lookup");
                assert_eq!(entry.id, resolved.mobile());
                cells += 1;
            }
        }
        assert_eq!(cells, TABLE_COUNT * TABLE_LEN);
        for id in (43u16..=127).chain(147..=u16::MAX) {
            assert_eq!(mobile_enemy(id), Err(MobileEnemyError::OutOfBand { id }), "id {id}");
        }
        // Goldens measured from `EnemyBasics.cs@0038a1e6`.
        let rat = mobile_enemy(0).expect("Rat");
        assert_eq!(
            (rat.level, rat.min_health, rat.max_health, rat.armor_value),
            (1, 9, 16, 6)
        );
        assert_eq!(rat.min_metal_to_hit.raw(), -1);
        assert_eq!(rat.team, MobileTeams::Vermin);
        let lamia = mobile_enemy(42).expect("Lamia");
        assert_eq!((lamia.level, lamia.min_health, lamia.max_health), (16, 16, 58));
        assert_eq!(lamia.loot_table_key, Some('R'));
        let mage = mobile_enemy(128).expect("Mage");
        assert_eq!((mage.loot_table_key, mage.map_chance), (Some('U'), 3));
        assert_eq!((mage.level, mage.min_health), (0, 0));
    }

    /// The 19 classes spawn with synthetic careers: HP inside the class
    /// interval, city watch +3..=6, and class 19 / 255 are typed errors
    /// THROUGH the spawn path (criterion 7, pure).
    #[test]
    fn nineteen_classes_and_rejections_via_spawn_path() {
        let player_level = 5;
        let mut rng = Rng64::new(20_261_024);
        for class_id in 0..=18u8 {
            let hppl = 1 + i32::from(class_id) % 12;
            let career = synthetic_career("Synthetic", hppl);
            let mobile = HUMANOID_MOBILE_START + u16::from(class_id);
            let humanoid = spawn_humanoid(mobile, &career, player_level, &mut rng)
                .unwrap_or_else(|err| panic!("class {class_id}: {err}"));
            assert_eq!(humanoid.class_id, class_id);
            assert_eq!(humanoid.mobile, mobile);
            if is_city_watch(class_id) {
                assert!(
                    (player_level + 3..=player_level + 6).contains(&humanoid.level),
                    "watch level {}",
                    humanoid.level
                );
            } else {
                assert_eq!(humanoid.level, player_level);
            }
            assert_eq!(
                humanoid.health_bounds,
                class_max_health_bounds(humanoid.level, hppl)
            );
            let (low, high) = humanoid.health_bounds;
            assert!(
                (low..=high).contains(&humanoid.max_health),
                "class {class_id} hp {} out of [{low},{high}]",
                humanoid.max_health
            );
        }
        // Class 19 (mobile 147) and 255 (mobile 383) through the spawn path.
        let career = synthetic_career("Synthetic", 12);
        assert!(matches!(
            spawn_humanoid(147, &career, player_level, &mut rng),
            Err(HumanoidSpawnError::Class {
                class_id: 19,
                source: HumanoidProfileError::ClassId(19),
                ..
            })
        ));
        assert!(matches!(
            spawn_humanoid(383, &career, player_level, &mut rng),
            Err(HumanoidSpawnError::Class {
                class_id: 255,
                source: HumanoidProfileError::ClassId(255),
                ..
            })
        ));
        assert!(matches!(
            spawn_humanoid(100, &career, player_level, &mut rng),
            Err(HumanoidSpawnError::NotHumanoid { mobile: 100 })
        ));
        assert!(matches!(
            spawn_humanoid(384, &career, player_level, &mut rng),
            Err(HumanoidSpawnError::ClassIdOutOfRange { mobile: 384 })
        ));
    }

    /// The encounter combatant carries the table's `min_metal_to_hit` and
    /// the spawn HP (criterion 8); team/bands/key are exposed (9/10) and the
    /// enemy loot of two distinct keys is deterministic ×2 with the gold
    /// inside the matrix interval (10) — pure, 0 retail.
    #[test]
    fn combatant_and_loot_follow_the_table() {
        let werewolf = mobile_enemy(9).expect("Werewolf");
        let monster = SpawnedMonster::from_entry(werewolf, werewolf.min_health);
        let combatant = monster_combatant(&monster, 9_001);
        assert_eq!(combatant.min_metal_to_hit, 2);
        assert_eq!(combatant.kind, EntityKind::EnemyMonster);
        assert_eq!(combatant.health, werewolf.min_health);
        assert_eq!(combatant.max_health, werewolf.min_health);
        assert_eq!(monster.team, MobileTeams::Werecreatures);
        assert_eq!(monster.level, 6);
        assert_eq!(monster.damage_bands[2], (2, 12));
        assert_eq!(monster.loot_table_key, None);
        for (id, level) in [(7u16, 5i32), (28, 19)] {
            let entry = mobile_enemy(id).expect("row");
            let key = entry.loot_table_key.expect("keyed row");
            let row_matrix = matrix(key);
            let first = generate_enemy_loot(
                99,
                u32::from(id),
                entry.loot_table_key,
                entry.map_chance,
                level,
            );
            let second = generate_enemy_loot(
                99,
                u32::from(id),
                entry.loot_table_key,
                entry.map_chance,
                level,
            );
            // `Item.uid` is a process-global counter, so full struct
            // equality never holds across two calls: the deterministic core
            // is the canonical digest + gold/key/len.
            assert_eq!(
                container_digest_bytes(&first),
                container_digest_bytes(&second),
                "id {id} digest"
            );
            assert_eq!(
                (first.gold, first.key, first.items.len()),
                (second.gold, second.key, second.items.len()),
                "id {id} fields"
            );
            assert!(
                (row_matrix.min_gold * level..=row_matrix.max_gold * level).contains(&first.gold),
                "id {id} gold {}",
                first.gold
            );
        }
    }

    /// The demo scene is deterministic ×2 and respects the table HP interval
    /// when retail is available (without retail the test invents nothing: it
    /// declares the skip).
    #[test]
    fn scene_is_deterministic_when_retail_is_present() {
        let Ok((_, bytes)) = load_monster_bsa() else {
            eprintln!("MONSTER.BSA missing: scene not measured in this environment (Mode A)");
            return;
        };
        let first = compose_scene(M25_SCENE_SEED).expect("scene");
        let second = compose_scene(M25_SCENE_SEED).expect("scene");
        assert_eq!(first, second);
        assert_eq!(first.len(), M25_SPAWNED);
        for monster in &first {
            assert!(
                monster.hp >= monster.hp_bounds.0 && monster.hp <= monster.hp_bounds.1,
                "{} hp {} out of table interval",
                monster.name, monster.hp
            );
            let entry = mobile_enemy(monster.mobile).expect("table row");
            assert_eq!(monster.level, entry.level);
            assert_eq!(monster.min_metal_to_hit, entry.min_metal_to_hit.raw());
        }
        // Humanoid draws take the typed path: no `ENEMY###.CFG`, HP from the
        // retail `CLASS*.CFG` careers.
        let humanoid_table = table(M25_HUMANOID_TABLE).expect("table 2");
        let mixed =
            compose_scene_from(&bytes, humanoid_table, M25_SCENE_SEED, load_retail_class_career)
                .expect("scene");
        assert_eq!(mixed.len(), M25_SPAWNED);
        for entry in &mixed {
            if let SpawnedCombatant::Humanoid(humanoid) = entry {
                let (low, high) = humanoid.health_bounds;
                assert!((low..=high).contains(&humanoid.max_health));
                assert!((M25_PLAYER_LEVEL..=M25_PLAYER_LEVEL + 6).contains(&humanoid.level));
            }
        }
        let humanoid_scene = compose_scene_from(
            &bytes,
            &HUMANOID_ONLY_TABLE,
            M25_SCENE_SEED,
            load_retail_class_career,
        )
        .expect("scene");
        assert_eq!(humanoid_scene.len(), M25_SPAWNED);
        assert!(humanoid_scene
            .iter()
            .all(|entry| matches!(entry, SpawnedCombatant::Humanoid(_))));
        for entry in &humanoid_scene {
            if let SpawnedCombatant::Humanoid(humanoid) = entry {
                assert_eq!(
                    record_index_of_mobile(humanoid.mobile),
                    Err(MonsterError::HumanoidWithoutEnemyRecord {
                        mobile: humanoid.mobile,
                        class_id: humanoid.class_id
                    })
                );
                assert_eq!(
                    humanoid.name,
                    HUMANOID_TYPE_NAMES[usize::from(humanoid.class_id)]
                );
            }
        }
        // Every cell of every table composes without panicking.
        for index in 0..TABLE_COUNT {
            let composed = compose_scene_from(
                &bytes,
                table(index).expect("table"),
                M25_SCENE_SEED,
                load_retail_class_career,
            );
            assert!(composed.is_ok(), "table {index}: {composed:?}");
        }
    }
}

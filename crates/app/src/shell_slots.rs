//! **M-SHELL phase 2** (`CR-a34396f5`) — shell slot store `SAVE0..SAVE5`
//! on top of 🧠 Memory's **v3** save.
//!
//! The shell invents no format: each slot is a [`SaveGameV2`] document
//! (`DFL-SESSION 3`, Memory's contract `writer_version = 3`) written with
//! `save_to_path` and read back with `load_from_path`, which validates
//! header, checksum and sections. What the shell stores is what the session
//! knows about itself: **identity** (`character.name`) and the **session
//! clock** (`clock.hour`/`clock.minute`); vitals and inventory stay at 0
//! because this phase does not model them — the document is a record, not a
//! simulation in disguise.
//!
//! ## Directory: `DF_SAVE_DIR`
//!
//! The gate sets the variable per process and profile (`--out run1-saves`,
//! `run2-saves`, `ctrl-saves`): two processes with the same profile write the
//! same bytes. Without the variable there are **no slots**: `continue` publishes
//! `notice=no-save` and `save` refuses with one `M18 VE:` line — a game is
//! never fabricated in an implicit directory.
//!
//! ## Determinism
//!
//! The canonical document (`to_text`) carries no timestamps, pids or paths: the
//! `digest()` is FNV-1a 64 of the payload and `bytes` is the length of that text,
//! so the pair `(bytes, digest)` is identical between runs.

use std::path::{Path, PathBuf};

use daggerfall_heart::clock::MINUTES_PER_DAY;
use daggerfall_memory::savegame_v2::SaveGameV2;

/// Browser slots (`SAVE0..SAVE5`): the contract's literal `slots=6`.
pub const SLOT_COUNT: usize = 6;
/// File-name prefix of each slot.
pub const SLOT_PREFIX: &str = "SAVE";
/// Extension of the shell slots (canonical v3 text).
pub const SLOT_EXTENSION: &str = "dfl";
/// Version of the document the browser declares (`format=v3`).
pub const FORMAT: &str = "v3";
/// **Slot generation** — it is NOT the grammar version.
///
/// `FORMAT` is still `v3`: the file IS a `DFL-SESSION 3` document (the
/// `dun-save`/`dun-state`/`dun-elig` rows, `mplay-001` and
/// `mainquest_route::persist` pin it, and `scripts/shell-002.py` fixes
/// `format=v3` in five anchored expressions). What changes is **what the slot
/// declares**: generation 3 stored identity and session clock and left
/// `world.*`/`calendar.game_minutes` at the `SaveGameV2::new()` default;
/// generation 4 declares the **authoritative world state** (position,
/// orientation, site and absolute minute) in the same v3 fields.
pub const SLOT_GENERATION: u8 = 4;
/// Generation of a slot that does not declare world state (previous product).
pub const SLOT_GENERATION_WITHOUT_WORLD: u8 = 3;
/// Millimetres per metre: the integer unit the position travels in
/// (`world.pos_*_mm`), without depending on `f32` rounding between runs.
pub const MM_PER_M: f64 = 1000.0;
/// `map_id` de un mapa abierto (exterior): `SaveTree`/DFU `MapID` sin bloque.
pub const MAP_ID_OPEN_WORLD: i64 = 65535;
/// Level of the newly created character: creation hands out no experience.
pub const START_LEVEL: i64 = 1;
/// Races of the creation canon (the index is `face % 8`).
pub const RACES: [&str; 8] = [
    "breton",
    "redguard",
    "nord",
    "dark-elf",
    "high-elf",
    "wood-elf",
    "khajiit",
    "argonian",
];

/// What the load browser publishes about an occupied slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotFacts {
    /// Número de slot (`0..SLOT_COUNT`).
    pub index: usize,
    /// Character token (`character.name` of the document).
    pub character: String,
    /// Level declared by the document.
    pub level: i64,
    /// Session clock of the document in game minutes (`hour*60 + minute`).
    pub minutes: u64,
    /// Slot generation the document declares: `4` if it carries authoritative
    /// world state, `3` if its `world.*`/`calendar.game_minutes` are the default
    /// (the previous product's slot).
    pub fidelity: u8,
}

/// Result of a sweep over `SAVE0..SAVE5`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SaveStore {
    /// Valid slots, in slot-number order.
    pub slots: Vec<SlotFacts>,
    /// Ficheros presentes que no son documentos v3 válidos (no se cargan).
    pub rejected: usize,
}

impl SaveStore {
    /// Occupied slots (the `slots=N` of `load open=1`).
    pub fn count(&self) -> usize {
        self.slots.len()
    }

    /// Slot ocupado por número, si existe.
    pub fn get(&self, index: usize) -> Option<&SlotFacts> {
        self.slots.iter().find(|slot| slot.index == index)
    }

    /// Slot that `continue` loads: the **highest occupied number**.
    ///
    /// The v3 document stores no write instant (for determinism),
    /// so "the most recent" is a declared rule, not a measurement: the highest
    /// slot the shell wrote wins.
    pub fn newest(&self) -> Option<&SlotFacts> {
        self.slots.iter().max_by_key(|slot| slot.index)
    }
}

/// Slot directory of the process (`DF_SAVE_DIR`), or `None` without the variable.
pub fn save_dir() -> Option<PathBuf> {
    std::env::var_os("DF_SAVE_DIR").map(PathBuf::from)
}

/// Path of slot `index` inside `dir`.
pub fn slot_path_in(dir: &Path, index: usize) -> PathBuf {
    dir.join(format!("{SLOT_PREFIX}{index}.{SLOT_EXTENSION}"))
}

/// World state a generation 4 slot declares authoritative.
///
/// It is the transport unit between the window half (which measures it from
/// the ECS every frame) and the write door (ECS-free). The position travels in
/// **integer millimetres** — the unit of `WorldSection.pos_*_mm` — so the
/// round-trip does not depend on `f32` decimal rounding; `yaw`/`pitch` travel
/// in `f32` radians and are written as integer milliradians.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldSnapshot {
    /// Eye position in millimetres, world axes `(x, y, z)`.
    pub pos_mm: [i64; 3],
    /// Orientación horizontal en radianes (`M2Player.yaw`).
    pub yaw: f32,
    /// Tilt in radians (`M2Player.pitch`, already clamped by the engine).
    pub pitch: f32,
    /// `WorldSection.environment`: `1` exterior, `2` interior, `3` mazmorra.
    pub environment: i64,
    /// `WorldSection.map_id`: `65535` en mapa abierto, el bloque en interiores.
    pub map_id: i64,
    /// Región del sitio (`WorldSection.region_index`).
    pub region_index: i64,
    /// Localización del sitio (`WorldSection.location_index`).
    pub location_index: i64,
    /// Minuto absoluto del mundo (`calendar.game_minutes` = `GameClock::total_minutes`).
    pub world_minutes: u64,
}

impl WorldSnapshot {
    /// Snapshot from the ECS measurement (`f32` metres → integer millimetres).
    #[allow(clippy::too_many_arguments)]
    pub fn from_meters(
        pos: [f32; 3],
        yaw: f32,
        pitch: f32,
        environment: i64,
        map_id: i64,
        region_index: i64,
        location_index: i64,
        world_minutes: u64,
    ) -> Self {
        Self {
            pos_mm: [
                meters_to_mm(pos[0]),
                meters_to_mm(pos[1]),
                meters_to_mm(pos[2]),
            ],
            yaw,
            pitch,
            environment,
            map_id,
            region_index,
            location_index,
            world_minutes,
        }
    }

    /// Position in metres (`f32`) for the restored camera and `Locomotion`.
    pub fn pos_meters(&self) -> [f32; 3] {
        [
            mm_to_meters(self.pos_mm[0]),
            mm_to_meters(self.pos_mm[1]),
            mm_to_meters(self.pos_mm[2]),
        ]
    }

    /// Generation this snapshot declares (`3` if it lacks site or clock).
    pub fn fidelity(&self) -> u8 {
        if self.environment == 0 || self.world_minutes == 0 {
            SLOT_GENERATION_WITHOUT_WORLD
        } else {
            SLOT_GENERATION
        }
    }
}

/// `f32` metres → integer millimetres (rounded to nearest).
pub fn meters_to_mm(meters: f32) -> i64 {
    (f64::from(meters) * MM_PER_M).round() as i64
}

/// Milímetros enteros → metros `f32`.
pub fn mm_to_meters(mm: i64) -> f32 {
    (mm as f64 / MM_PER_M) as f32
}

/// What a document declares restorable, decided without ECS.
///
/// The decision is pure on purpose: `shell_continue`/`shell_load_slot` publish
/// `W-SAVE-001` and `m18_session_restored` applies the world, but **nobody invents**
/// position or clock when the document does not declare them (`fidelity == 3`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RestorePlan {
    /// Generación del documento (`3` o `4`).
    pub fidelity: u8,
    /// World to restore: `Some` only if the generation declares it.
    pub world: Option<WorldSnapshot>,
}

impl RestorePlan {
    /// Absent fields that make the slot generation 3 (notice text).
    pub fn missing(&self) -> &'static [&'static str] {
        if self.fidelity >= SLOT_GENERATION {
            &[]
        } else {
            &["world.environment", "calendar.game_minutes"]
        }
    }
}

/// Snapshot a document stored (even if it is the generation 3 default).
pub fn world_of(doc: &SaveGameV2) -> WorldSnapshot {
    WorldSnapshot {
        pos_mm: [doc.world.pos_x_mm, doc.world.pos_y_mm, doc.world.pos_z_mm],
        yaw: doc.world.yaw_mrad as f32 / 1000.0,
        pitch: doc.world.pitch_mrad as f32 / 1000.0,
        environment: doc.world.environment,
        map_id: doc.world.map_id,
        region_index: doc.world.region_index,
        location_index: doc.world.location_index,
        world_minutes: doc.calendar.game_minutes,
    }
}

/// Resume plan of a document: what is restored and what is missing.
pub fn restore_plan(doc: &SaveGameV2) -> RestorePlan {
    let world = world_of(doc);
    let fidelity = world.fidelity();
    RestorePlan {
        fidelity,
        world: if fidelity >= SLOT_GENERATION { Some(world) } else { None },
    }
}

/// v3 document of a freshly created/loaded session.
///
/// `minutes` are the game minutes of the session clock: `hour` stores them
/// in hours of the day (`0..24`) and `minute` the rest, so the round-trip
/// returns the same value within a game day.
///
/// `quest` is the main quest's state (`MQ-01`): it travels as a row of
/// `QuestsSection::states` (`MQ-01.state=<token>`), the shape v3 already
/// models — **0 version bump**. `Unstarted` writes no row (there is no state
/// to restore).
pub fn document(
    character: &str,
    face: usize,
    minutes: u64,
    quest: daggerfall_behavior::mainquest::State,
) -> SaveGameV2 {
    let mut doc = SaveGameV2::new();
    doc.clock.hour = ((minutes / 60) % 24) as i64;
    doc.clock.minute = (minutes % 60) as i64;
    doc.character.name = character.to_string();
    doc.character.level = START_LEVEL;
    doc.character.face_index = face as i64;
    doc.character.race = (face % RACES.len()) as i64;
    doc.character.gender = (face % 2) as i64;
    crate::mainquest_route::persist(&mut doc, quest);
    doc
}

/// Session clock a document declares (game minutes).
pub fn minutes_of(doc: &SaveGameV2) -> u64 {
    let hour = doc.clock.hour.clamp(0, 23) as u64;
    let minute = doc.clock.minute.clamp(0, 59) as u64;
    hour * 60 + minute
}

/// Browser facts from the document already read.
pub fn facts_of(index: usize, doc: &SaveGameV2) -> SlotFacts {
    SlotFacts {
        index,
        character: doc.character.name.clone(),
        level: doc.character.level,
        minutes: minutes_of(doc),
        fidelity: world_of(doc).fidelity(),
    }
}

/// Generation 4 v3 document: the identity and session clock of
/// [`document`] **plus** the authoritative world state.
///
/// The file is still a `DFL-SESSION 3` document (same grammar, no
/// bump): what it gains are the fields v3 already models and the product left
/// at the default — `world.*` and `calendar.game_minutes`.
pub fn document_with_world(
    character: &str,
    face: usize,
    minutes: u64,
    quest: daggerfall_behavior::mainquest::State,
    world: &WorldSnapshot,
) -> SaveGameV2 {
    let mut doc = document(character, face, minutes, quest);
    doc.world.pos_x_mm = world.pos_mm[0];
    doc.world.pos_y_mm = world.pos_mm[1];
    doc.world.pos_z_mm = world.pos_mm[2];
    doc.world.yaw_mrad = radians_to_mrad(world.yaw);
    doc.world.pitch_mrad = radians_to_mrad(world.pitch);
    doc.world.environment = world.environment;
    doc.world.map_id = world.map_id;
    doc.world.region_index = world.region_index;
    doc.world.location_index = world.location_index;
    doc.calendar.game_minutes = world.world_minutes;
    // `is_day` is derivable from the absolute minute (DFU `IsDay`: dawn at 6,
    // dusk at 18). It is written derived so the row does not lie.
    doc.calendar.is_day = i64::from(clock_hour_of(world.world_minutes) >= 6
        && clock_hour_of(world.world_minutes) < 18);
    doc
}

/// Radianes → milirradianes enteros (unidad de `WorldSection.yaw_mrad`).
pub fn radians_to_mrad(radians: f32) -> i64 {
    (f64::from(radians) * MM_PER_M).round() as i64
}

/// Hour of the day (0..23) of an absolute minute.
pub fn clock_hour_of(total_minutes: u64) -> u64 {
    (total_minutes % MINUTES_PER_DAY) / 60
}

/// Writes slot `index` with world state and returns `(bytes, digest)`.
pub fn write_in_with_world(
    dir: &Path,
    index: usize,
    character: &str,
    face: usize,
    minutes: u64,
    quest: daggerfall_behavior::mainquest::State,
    world: &WorldSnapshot,
) -> Result<(usize, u64), String> {
    if index >= SLOT_COUNT {
        return Err(format!("slot {index} fuera de SAVE0..SAVE5"));
    }
    let doc = document_with_world(character, face, minutes, quest, world);
    let path = slot_path_in(dir, index);
    doc.save_to_path(&path)
        .map_err(|err| format!("{}: {err:?}", path.display()))?;
    Ok((doc.to_text().len(), doc.digest()))
}

/// Writes slot `index` and returns `(bytes, digest)` of the canonical document.
pub fn write_in(
    dir: &Path,
    index: usize,
    character: &str,
    face: usize,
    minutes: u64,
    quest: daggerfall_behavior::mainquest::State,
) -> Result<(usize, u64), String> {
    if index >= SLOT_COUNT {
        return Err(format!("slot {index} fuera de SAVE0..SAVE5"));
    }
    let doc = document(character, face, minutes, quest);
    let path = slot_path_in(dir, index);
    doc.save_to_path(&path)
        .map_err(|err| format!("{}: {err:?}", path.display()))?;
    Ok((doc.to_text().len(), doc.digest()))
}

/// Lee el slot `index` (valida versión 3 y checksum).
pub fn read_in(dir: &Path, index: usize) -> Result<SaveGameV2, String> {
    let path = slot_path_in(dir, index);
    SaveGameV2::load_from_path(&path).map_err(|err| format!("{}: {err:?}", path.display()))
}

/// Barre `SAVE0..SAVE5` en orden; un fichero ilegible no es un slot.
pub fn scan_in(dir: &Path) -> SaveStore {
    let mut store = SaveStore::default();
    for index in 0..SLOT_COUNT {
        let path = slot_path_in(dir, index);
        if !path.is_file() {
            continue;
        }
        match SaveGameV2::load_from_path(&path) {
            Ok(doc) => store.slots.push(facts_of(index, &doc)),
            Err(_) => store.rejected += 1,
        }
    }
    store
}

/// Sweeps the process directory (`DF_SAVE_DIR`); without the variable, 0 slots.
pub fn scan() -> SaveStore {
    match save_dir() {
        Some(dir) => scan_in(&dir),
        None => SaveStore::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use daggerfall_behavior::mainquest::State;

    /// Own working directory per test (without touching `DF_SAVE_DIR`, which is
    /// global to the process).
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dfl-shell-slots-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// `_fixture` — the generation 4 slot declares the world and returns it
    /// **losslessly** on read-back: integer millimetres (including negatives and
    /// fractions of a metre), `yaw`/`pitch`, the world's absolute minute,
    /// `environment`, `map_id` and the site.
    #[test]
    fn world_snapshot_round_trips_through_the_slot_v4_fixture() {
        let dir = scratch("world-round-trip-fixture");
        // Fractions and negatives: 411.37 m, -0.25 m, 419.999 m; negative yaw.
        // `yaw`/`pitch` travel as integer milliradians (the unit of
        // `WorldSection.*_mrad`): the fixture uses values that land exactly on that
        // grid, and the non-exact case is checked separately with its bound.
        let world = WorldSnapshot::from_meters(
            [411.37, 1.6, -419.999],
            -1.570,
            0.123,
            1,
            MAP_ID_OPEN_WORLD,
            19,
            1234,
            210_169_081,
        );
        assert_eq!(world.fidelity(), SLOT_GENERATION);
        write_in_with_world(&dir, 2, "breton-m", 0, 116, State::Act1Hunt, &world).expect("write");
        let doc = read_in(&dir, 2).expect("read");
        let back = world_of(&doc);
        assert_eq!(back.pos_mm, world.pos_mm, "posición mm sin pérdida");
        assert_eq!(back.yaw, world.yaw, "yaw exacto en la rejilla mrad");
        assert_eq!(back.pitch, world.pitch);
        // Off the grid, quantization is 1 mrad (declared bound).
        let off_grid = WorldSnapshot { yaw: -std::f32::consts::FRAC_PI_2, ..world };
        assert_eq!(radians_to_mrad(off_grid.yaw), -1571);
        assert!((radians_to_mrad(off_grid.yaw) as f32 / 1000.0 - off_grid.yaw).abs() <= 0.0005);
        assert_eq!(back.world_minutes, 210_169_081, "minuto de mundo restaurado");
        assert_eq!(back.environment, 1);
        assert_eq!(back.map_id, MAP_ID_OPEN_WORLD);
        assert_eq!((back.region_index, back.location_index), (19, 1234));
        assert_eq!(doc.calendar.is_day, 0, "solar 18:01 = noche");
        let plan = restore_plan(&doc);
        assert_eq!(plan.fidelity, 4);
        assert!(plan.world.is_some(), "un slot v4 sí pide restaurar el mundo");
        assert!(plan.missing().is_empty());
    }

    /// `_fixture` — the product slot (generation 3, written by the old door
    /// `write_in`) does NOT declare a world: the plan says so with `fidelity=3`,
    /// names the absent fields and returns no snapshot (nothing to restore).
    #[test]
    fn product_slot_v3_declares_no_world_fixture() {
        let dir = scratch("product-slot-v3-fixture");
        write_in(&dir, 0, "breton-m", 0, 116, State::Act1Hunt).expect("write v3");
        let doc = read_in(&dir, 0).expect("read v3");
        let plan = restore_plan(&doc);
        assert_eq!(plan.fidelity, SLOT_GENERATION_WITHOUT_WORLD);
        assert_eq!(plan.missing(), &["world.environment", "calendar.game_minutes"]);
        assert!(plan.world.is_none(), "no se restaura un mundo que no existe");
        assert_eq!(facts_of(0, &doc).fidelity, 3);
        // The old door does not change: it keeps writing the document without a world.
        assert_eq!(doc.world.pos_x_mm, 0);
        assert_eq!(doc.calendar.game_minutes, 0);
    }

    /// The written slot reads back with the same token, level and clock, and the pair
    /// `(bytes, digest)` is stable between two identical writes.
    #[test]
    fn slot_round_trip_is_deterministic() {
        let dir = scratch("round-trip");
        let (bytes, digest) =
            write_in(&dir, 0, "breton-m", 0, 12, State::Act1Hunt).expect("write");
        assert!(bytes > 0);
        assert_eq!(
            bytes,
            document("breton-m", 0, 12, State::Act1Hunt).to_text().len()
        );
        let (bytes_again, digest_again) =
            write_in(&dir, 0, "breton-m", 0, 12, State::Act1Hunt).expect("rewrite");
        assert_eq!((bytes, digest), (bytes_again, digest_again));
        let doc = read_in(&dir, 0).expect("read");
        assert_eq!(doc.character.name, "breton-m");
        assert_eq!(doc.character.level, START_LEVEL);
        assert_eq!(minutes_of(&doc), 12);
        let store = scan_in(&dir);
        assert_eq!(store.count(), 1);
        assert_eq!(store.get(0), Some(&facts_of(0, &doc)));
        assert_eq!(store.newest().map(|slot| slot.index), Some(0));
        assert_eq!(store.rejected, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A file that is not a v3 document is not loaded as a slot, and `continue`
    /// over an empty directory has nothing to point at.
    #[test]
    fn corrupt_slots_are_rejected_and_empty_store_has_no_continue_target() {
        let dir = scratch("corrupt");
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(slot_path_in(&dir, 1), "no es un save\n").expect("write corrupt");
        let store = scan_in(&dir);
        assert_eq!(store.count(), 0);
        assert_eq!(store.rejected, 1);
        assert!(store.newest().is_none());
        assert!(read_in(&dir, 1).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `SLOT_COUNT` is the contract literal `slots=6` and the names fit
    /// `SAVE0..SAVE5`.
    #[test]
    fn store_covers_the_six_declared_slots() {
        assert_eq!(SLOT_COUNT, 6);
        let dir = Path::new("/tmp/df-saves");
        let names: Vec<String> = (0..SLOT_COUNT)
            .map(|index| {
                slot_path_in(dir, index)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default()
            })
            .collect();
        assert_eq!(names[0], "SAVE0.dfl");
        assert_eq!(names[5], "SAVE5.dfl");
    }
}

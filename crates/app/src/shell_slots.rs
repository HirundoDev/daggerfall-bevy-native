//! **M-SHELL fase 2** (`CR-a34396f5`) — store de slots `SAVE0..SAVE5` del shell
//! sobre el save **v3** de 🧠 Memoria.
//!
//! El shell no inventa formato: cada slot es un documento
//! [`SaveGameV2`] (`DFL-SESSION 3`, el `writer_version = 3` del contrato de
//! Memoria) escrito con `save_to_path` y releído con `load_from_path`, que
//! valida cabecera, checksum y secciones. Lo que el shell guarda es lo que la
//! sesión sabe de sí misma: **identidad** (`character.name`) y **reloj de
//! sesión** (`clock.hour`/`clock.minute`); las vitales y el inventario siguen
//! en 0 porque esta fase no los modela — el documento es un registro, no una
//! simulación disfrazada.
//!
//! ## Directorio: `DF_SAVE_DIR`
//!
//! El gate fija la variable por proceso y perfil (`--out run1-saves`,
//! `run2-saves`, `ctrl-saves`): dos procesos con el mismo perfil escriben los
//! mismos bytes. Sin la variable **no hay slots**: `continue` publica
//! `notice=no-save` y `save` se niega con una línea `M18 VE:` — nunca se
//! fabrica una partida en un directorio implícito.
//!
//! ## Determinismo
//!
//! El documento canónico (`to_text`) no lleva timestamps, pids ni rutas: el
//! `digest()` es FNV-1a 64 del payload y `bytes` es la longitud de ese texto,
//! así que el par `(bytes, digest)` es idéntico entre corridas.

use std::path::{Path, PathBuf};

use daggerfall_heart::clock::MINUTES_PER_DAY;
use daggerfall_memory::savegame_v2::SaveGameV2;

/// Slots del browser (`SAVE0..SAVE5`): el literal `slots=6` del contrato.
pub const SLOT_COUNT: usize = 6;
/// Prefijo del nombre de fichero de cada slot.
pub const SLOT_PREFIX: &str = "SAVE";
/// Extensión de los slots del shell (texto canónico v3).
pub const SLOT_EXTENSION: &str = "dfl";
/// Versión del documento que el browser declara (`format=v3`).
pub const FORMAT: &str = "v3";
/// **Generación del slot** — NO es la versión de la gramática.
///
/// `FORMAT` sigue siendo `v3`: el fichero ES un documento `DFL-SESSION 3` (las
/// filas `dun-save`/`dun-state`/`dun-elig`, `mplay-001` y
/// `mainquest_route::persist` lo pinchan, y `scripts/shell-002.py` fija
/// `format=v3` en cinco expresiones ancladas). Lo que cambia es **qué declara
/// el slot**: la generación 3 guardaba identidad y reloj de sesión y dejaba
/// `world.*`/`calendar.game_minutes` en el default de `SaveGameV2::new()`; la
/// generación 4 declara el **estado de mundo autoritativo** (posición,
/// orientación, sitio y minuto absoluto) en los mismos campos v3.
pub const SLOT_GENERATION: u8 = 4;
/// Generación de un slot que no declara estado de mundo (producto anterior).
pub const SLOT_GENERATION_WITHOUT_WORLD: u8 = 3;
/// Milímetros por metro: la unidad entera en la que viaja la posición
/// (`world.pos_*_mm`), sin depender del redondeo de `f32` entre corridas.
pub const MM_PER_M: f64 = 1000.0;
/// `map_id` de un mapa abierto (exterior): `SaveTree`/DFU `MapID` sin bloque.
pub const MAP_ID_OPEN_WORLD: i64 = 65535;
/// Nivel del personaje recién creado: la creación no reparte experiencia.
pub const START_LEVEL: i64 = 1;
/// Carreras del canon de creación (el índice es `face % 8`).
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

/// Lo que el browser de carga publica de un slot ocupado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotFacts {
    /// Número de slot (`0..SLOT_COUNT`).
    pub index: usize,
    /// Token del personaje (`character.name` del documento).
    pub character: String,
    /// Nivel declarado por el documento.
    pub level: i64,
    /// Reloj de sesión del documento en minutos de juego (`hour*60 + minute`).
    pub minutes: u64,
    /// Generación de slot que el documento declara: `4` si trae estado de mundo
    /// autoritativo, `3` si sus `world.*`/`calendar.game_minutes` son el default
    /// (el slot del producto anterior).
    pub fidelity: u8,
}

/// Resultado de un barrido de `SAVE0..SAVE5`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SaveStore {
    /// Slots válidos, en orden de número de slot.
    pub slots: Vec<SlotFacts>,
    /// Ficheros presentes que no son documentos v3 válidos (no se cargan).
    pub rejected: usize,
}

impl SaveStore {
    /// Slots ocupados (el `slots=N` del `load open=1`).
    pub fn count(&self) -> usize {
        self.slots.len()
    }

    /// Slot ocupado por número, si existe.
    pub fn get(&self, index: usize) -> Option<&SlotFacts> {
        self.slots.iter().find(|slot| slot.index == index)
    }

    /// Slot que carga `continue`: el **mayor número ocupado**.
    ///
    /// El documento v3 no guarda un instante de escritura (por determinismo),
    /// así que «el más reciente» es una regla declarada, no una medida: gana el
    /// slot más alto que el shell escribió.
    pub fn newest(&self) -> Option<&SlotFacts> {
        self.slots.iter().max_by_key(|slot| slot.index)
    }
}

/// Directorio de slots del proceso (`DF_SAVE_DIR`), o `None` sin la variable.
pub fn save_dir() -> Option<PathBuf> {
    std::env::var_os("DF_SAVE_DIR").map(PathBuf::from)
}

/// Ruta del slot `index` dentro de `dir`.
pub fn slot_path_in(dir: &Path, index: usize) -> PathBuf {
    dir.join(format!("{SLOT_PREFIX}{index}.{SLOT_EXTENSION}"))
}

/// Estado de mundo que un slot de generación 4 declara autoritativo.
///
/// Es la unidad de transporte entre el half de ventana (que lo mide de la ECS
/// cada frame) y la puerta de escritura (ECS-free). La posición viaja en
/// **milímetros enteros** — la unidad de `WorldSection.pos_*_mm` — para que el
/// round-trip no dependa del redondeo decimal de `f32`; `yaw`/`pitch` viajan en
/// `f32` radianes y se escriben en milirradianes enteros.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldSnapshot {
    /// Posición del ojo en milímetros, ejes `(x, y, z)` del mundo.
    pub pos_mm: [i64; 3],
    /// Orientación horizontal en radianes (`M2Player.yaw`).
    pub yaw: f32,
    /// Inclinación en radianes (`M2Player.pitch`, ya recortada por el motor).
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
    /// Snapshot desde la medida de la ECS (metros `f32` → milímetros enteros).
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

    /// Posición en metros (`f32`) para la cámara y el `Locomotion` restaurados.
    pub fn pos_meters(&self) -> [f32; 3] {
        [
            mm_to_meters(self.pos_mm[0]),
            mm_to_meters(self.pos_mm[1]),
            mm_to_meters(self.pos_mm[2]),
        ]
    }

    /// Generación que este snapshot declara (`3` si le falta sitio o reloj).
    pub fn fidelity(&self) -> u8 {
        if self.environment == 0 || self.world_minutes == 0 {
            SLOT_GENERATION_WITHOUT_WORLD
        } else {
            SLOT_GENERATION
        }
    }
}

/// Metros `f32` → milímetros enteros (redondeo al más cercano).
pub fn meters_to_mm(meters: f32) -> i64 {
    (f64::from(meters) * MM_PER_M).round() as i64
}

/// Milímetros enteros → metros `f32`.
pub fn mm_to_meters(mm: i64) -> f32 {
    (mm as f64 / MM_PER_M) as f32
}

/// Lo que un documento declara restaurable, decidido sin ECS.
///
/// La decisión es pura a propósito: `shell_continue`/`shell_load_slot` publican
/// `W-SAVE-001` y `m18_session_restored` aplica el mundo, pero **nadie inventa**
/// posición ni reloj cuando el documento no los declara (`fidelity == 3`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RestorePlan {
    /// Generación del documento (`3` o `4`).
    pub fidelity: u8,
    /// Mundo a restaurar: `Some` sólo si la generación lo declara.
    pub world: Option<WorldSnapshot>,
}

impl RestorePlan {
    /// Campos ausentes que hacen al slot de generación 3 (texto del aviso).
    pub fn missing(&self) -> &'static [&'static str] {
        if self.fidelity >= SLOT_GENERATION {
            &[]
        } else {
            &["world.environment", "calendar.game_minutes"]
        }
    }
}

/// Snapshot que un documento guardó (aunque sea el default de la generación 3).
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

/// Plan de reanudación de un documento: qué se restaura y qué falta.
pub fn restore_plan(doc: &SaveGameV2) -> RestorePlan {
    let world = world_of(doc);
    let fidelity = world.fidelity();
    RestorePlan {
        fidelity,
        world: if fidelity >= SLOT_GENERATION { Some(world) } else { None },
    }
}

/// Documento v3 de una sesión recién creada/cargada.
///
/// `minutes` son los minutos de juego del reloj de sesión: `hour` los guarda
/// en horas del día (`0..24`) y `minute` el resto, así que el round-trip
/// devuelve el mismo valor dentro de un día de juego.
///
/// `quest` es el estado de la quest principal (`MQ-01`): viaja como fila de
/// `QuestsSection::states` (`MQ-01.state=<token>`), la forma que el v3 ya
/// modela — **0 bump de versión**. `Unstarted` no escribe fila (no hay estado
/// que restaurar).
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

/// Reloj de sesión que declara un documento (minutos de juego).
pub fn minutes_of(doc: &SaveGameV2) -> u64 {
    let hour = doc.clock.hour.clamp(0, 23) as u64;
    let minute = doc.clock.minute.clamp(0, 59) as u64;
    hour * 60 + minute
}

/// Hechos del browser a partir del documento ya leído.
pub fn facts_of(index: usize, doc: &SaveGameV2) -> SlotFacts {
    SlotFacts {
        index,
        character: doc.character.name.clone(),
        level: doc.character.level,
        minutes: minutes_of(doc),
        fidelity: world_of(doc).fidelity(),
    }
}

/// Documento v3 de generación 4: la identidad y el reloj de sesión de
/// [`document`] **más** el estado de mundo autoritativo.
///
/// El fichero sigue siendo un documento `DFL-SESSION 3` (misma gramática, sin
/// bump): lo que gana son los campos que el v3 ya modela y el producto dejaba
/// en el default — `world.*` y `calendar.game_minutes`.
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
    // `is_day` es derivable del minuto absoluto (DFU `IsDay`: amanece a las 6,
    // anochece a las 18). Se escribe derivado para que la fila no mienta.
    doc.calendar.is_day = i64::from(clock_hour_of(world.world_minutes) >= 6
        && clock_hour_of(world.world_minutes) < 18);
    doc
}

/// Radianes → milirradianes enteros (unidad de `WorldSection.yaw_mrad`).
pub fn radians_to_mrad(radians: f32) -> i64 {
    (f64::from(radians) * MM_PER_M).round() as i64
}

/// Hora del día (0..23) de un minuto absoluto.
pub fn clock_hour_of(total_minutes: u64) -> u64 {
    (total_minutes % MINUTES_PER_DAY) / 60
}

/// Escribe el slot `index` con estado de mundo y devuelve `(bytes, digest)`.
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

/// Escribe el slot `index` y devuelve `(bytes, digest)` del documento canónico.
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

/// Barre el directorio del proceso (`DF_SAVE_DIR`); sin la variable, 0 slots.
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

    /// Directorio de trabajo propio por test (sin tocar `DF_SAVE_DIR`, que es
    /// global al proceso).
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dfl-shell-slots-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// `_fixture` — el slot de generación 4 declara el mundo y lo devuelve
    /// **sin pérdida** al releerlo: milímetros enteros (incluye negativos y
    /// fracciones de metro), `yaw`/`pitch`, minuto absoluto del mundo,
    /// `environment`, `map_id` y el sitio.
    #[test]
    fn world_snapshot_round_trips_through_the_slot_v4_fixture() {
        let dir = scratch("world-round-trip-fixture");
        // Fracciones y negativos: 411.37 m, -0.25 m, 419.999 m; yaw negativo.
        // `yaw`/`pitch` viajan en milirradianes enteros (la unidad de
        // `WorldSection.*_mrad`): el fixture usa valores que caen exactos en esa
        // rejilla, y el caso no exacto se comprueba aparte con su cota.
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
        // Fuera de la rejilla, la cuantización es de 1 mrad (cota declarada).
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

    /// `_fixture` — el slot de producto (generación 3, escrito por la puerta
    /// vieja `write_in`) NO declara mundo: el plan lo dice con `fidelity=3`,
    /// nombra los campos ausentes y no devuelve snapshot (nada que restaurar).
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
        // La puerta vieja no cambia: sigue escribiendo el documento sin mundo.
        assert_eq!(doc.world.pos_x_mm, 0);
        assert_eq!(doc.calendar.game_minutes, 0);
    }

    /// El slot escrito se relee con el mismo token, nivel y reloj, y el par
    /// `(bytes, digest)` es estable entre dos escrituras idénticas.
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

    /// Un fichero que no es un documento v3 no se carga como slot, y `continue`
    /// sobre un directorio vacío no tiene a qué apuntar.
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

    /// `SLOT_COUNT` es el literal `slots=6` del contrato y los nombres caben en
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

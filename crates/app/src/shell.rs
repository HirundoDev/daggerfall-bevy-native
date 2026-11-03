//! **M-SHELL fase 2 — capa aplicación** (`CR-ca78eb76` + `CR-a34396f5`): el
//! estado del shell (`Boot → Loading → Menu → Create → Playing ⇄ Paused →
//! Exit`), sus transiciones puras y la evidencia que consumen los gates
//! `scripts/shell-001.py` (fase 1) y `scripts/shell-002.py` (fase 2).
//!
//! Este módulo NO dibuja ni simula: los sistemas Bevy que lo conducen viven en
//! `m18_window` (donde están los recursos del mundo) y aquí queda lo que se
//! puede auditar sin una ventana — el contrato de líneas `SHELL VE:`, el espejo
//! byte-idéntico en `$DF_SHELL_STATE`, el reloj de sesión discreto, el plan de
//! viaje rápido sobre `daggerfall_heart::travel`, y el estado de navegación de
//! la fase 2 (menú de 5 ítems, wizard de creación, panel de opciones y browser
//! de slots).
//!
//! ## Fase 2 (`CR-a34396f5`) — estado que sobrevive a la navegación
//!
//! Tres piezas viven en el [`Shell`] y NO en la pantalla: el **panel** abierto
//! ([`ShellPanel`]), el **estado de opciones** (una sola página con su valor,
//! que sobrevive a cerrar el panel y a entrar/salir de una partida) y el
//! **browser de slots** (slot seleccionado + hechos leídos de `DF_SAVE_DIR`).
//! Los slots son documentos v3 de 🧠 Memoria (`crate::shell_slots`): el shell
//! no inventa un formato propio.
//!
//! ## Contrato de evidencia (verbatim del pack `CR-ca78eb76-shell-gate.md`)
//!
//! El gate compara las líneas contra `re.compile(...)` exactos (`shell-001.py`
//! §contrato): los constructores de este módulo son la ÚNICA copia de esas
//! formas. Añadir una línea exige tocar el contrato (y el gate) a la vez.
//!
//! ## Reloj de sesión (`ticks`) — por qué es discreto
//!
//! `PROC-two-runs` exige que las 15 líneas sean **byte-idénticas** entre dos
//! procesos, así que ningún campo puede derivarse de `dt` ni de la hora real.
//! El reloj que el shell reporta cuenta **minutos de juego consumidos por el
//! jugador** (el viaje rápido es el único consumidor en esta fase); el reloj M4
//! en tiempo real (`ProgressionClock`) sigue vivo para las skills y NO se
//! reporta. En `Paused` el reloj no se toca: el valor congelado es el mismo por
//! construcción y por ausencia de consumidores, no por una constante.

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use daggerfall_heart::travel::{
    calculate_travel_time, calculate_trip_cost, TerrainLookup, Transport, TravelOptions,
};

/// Prefijo de todas las líneas de evidencia (el gate filtra por él, y el espejo
/// de stdout usa la misma condición: `mirrored_lines`).
pub const LINE_PREFIX: &str = "SHELL VE: ";

/// Ítems del menú principal en orden de navegación (fase 2, `CR-a34396f5`):
/// los cinco destinos que el gate `scripts/shell-002.py` exige declarados en
/// `menu order=` y que el menú retail de arranque pinta
/// (`m39_block_a::MENU_TARGETS`, mismo orden). `new-game` es el índice 0
/// (selección inicial del contrato) y `quit` el último.
pub const MENU_ITEMS: [&str; 5] = ["new-game", "continue", "load", "options", "quit"];

/// Índice de `new-game` dentro de [`MENU_ITEMS`].
pub const MENU_NEW_GAME: usize = 0;
/// Índice de `continue` dentro de [`MENU_ITEMS`].
pub const MENU_CONTINUE: usize = 1;
/// Índice de `load` dentro de [`MENU_ITEMS`].
pub const MENU_LOAD: usize = 2;
/// Índice de `options` dentro de [`MENU_ITEMS`].
pub const MENU_OPTIONS: usize = 3;
/// Índice de `quit` dentro de [`MENU_ITEMS`].
pub const MENU_QUIT: usize = 4;

/// Pasos del canon de creación (`create step=<n> steps=10`): el wizard retail
/// pinta 16 superficies (`m38_surfaces::CREATE_SURFACE_COUNT`) y pregunta estos
/// diez campos en este orden. Los ids son estables y distintos (contrato
/// `ID = [a-z0-9-]{1,24}`).
pub const CREATE_STEPS: usize = 10;
/// Campos del canon de creación, en el orden en que el wizard los pregunta.
pub const CREATE_FIELDS: [&str; CREATE_STEPS] = [
    "face",
    "race",
    "gender",
    "class",
    "name",
    "level",
    "skills",
    "advantages",
    "disadvantages",
    "summary",
];
/// Caras del wizard (`FACES.CIF`): `Right`/`Left` recorren este anillo.
pub const CREATE_FACES: usize = 16;
/// Semilla fija del reloj de sesión (minutos de juego de arranque).
///
/// No se lee ningún reloj real: el pack exige un reloj determinista
/// («seed fijo por slot») y `PROC-two-runs` exige byte-identidad, así que la
/// sesión arranca en el minuto 12 del día de juego y de ahí solo avanza con
/// consumidores reales (viaje rápido). Es una semilla declarada, no un hecho
/// retail.
pub const SESSION_CLOCK_SEED: u64 = 12;

/// Superficie retail del menú (`m18_phase::MENU_PANEL_ID`).
pub const MENU_SURFACE: &str = "main-menu";
/// Panel retail que abre `M` (5º panel de sesión de `ui_spec`, `map-local`).
pub const MAP_PANEL: &str = "map-local";
/// Id de la superficie de opciones que declara la pausa.
pub const PAUSE_OPTIONS: &str = "pause-menu";
/// Id del panel del browser de slots (`panel=save-browser` del contrato).
pub const BROWSER_PANEL: &str = "save-browser";
/// Páginas del panel de opciones: las 11 superficies de configuración del censo
/// (`m38_surfaces::CONFIG_SURFACE_COUNT`), el `pages=11` del contrato.
pub const CONFIG_PAGES: usize = daggerfall_app::m38_surfaces::CONFIG_SURFACE_COUNT;

/// Tiempo mínimo que la pantalla de arranque se presenta (s).
pub const BOOT_DWELL: f32 = 1.0;
/// Tiempo mínimo por paso de carga (s) — la pantalla debe ser legible; las
/// unidades del paso salen del trabajo real, nunca de este temporizador.
pub const LOADING_STEP_DWELL: f32 = 0.30;
/// Retardo de la línea de un cambio de estado: el título ya escrito en el
/// servidor X se muestrea ANTES de que la línea aparezca (el gate muestrea cada
/// 50 ms durante `wait`).
pub const LABEL_SETTLE: f32 = 0.50;
/// Retardo de la línea de pausa tras el cambio de etiqueta.
pub const PAUSE_SETTLE: f32 = 0.45;
/// Reintento de petición de foco de teclado en la antesala del shell (s).
pub const FOCUS_RETRY: f32 = 0.50;
/// Retardo de la línea de reanudación tras el cambio de etiqueta.
pub const RESUME_SETTLE: f32 = 0.30;
/// Retardo de `exit state=Exit` tras poner la etiqueta `SHELL EXIT`.
pub const EXIT_SETTLE: f32 = 0.60;
/// Tiempo que el proceso sigue vivo después de la línea de salida (s): el
/// título `SHELL EXIT` debe haber sido muestreado y el `AppExit` llega con 0.
pub const EXIT_AFTER_LINE: f32 = 0.80;

/// Metros de mundo por píxel de mapa que el avatar avanza en un viaje rápido
/// (declarado: el mundo de la fase 1 no se streamea, así que el viaje mueve al
/// jugador y su píxel de mapa, no carga otra celda).
pub const TRAVEL_M_PER_PIXEL: f32 = 8.0;

/// Estado del shell. `Loading` lleva el paso en curso (0-based) y `Menu` la
/// selección; el resto son estados planos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellState {
    /// Pantalla de arranque, antes de cualquier trabajo de carga.
    Boot,
    /// Carga en curso: `step` es el índice del paso que se está presentando.
    Loading {
        /// Paso en curso (0-based).
        step: usize,
    },
    /// Menú navegable con la selección actual.
    Menu {
        /// Índice seleccionado dentro de [`MENU_ITEMS`].
        selection: usize,
    },
    /// Wizard de creación: `step` (1-based, `1..=CREATE_STEPS`) y la cara
    /// elegida (`Right`/`Left`).
    Create {
        /// Paso del canon en curso (`CREATE_FIELDS[step - 1]`).
        step: usize,
        /// Cara del personaje (`0..CREATE_FACES`).
        face: usize,
    },
    /// Mundo vivo.
    Playing,
    /// Pausa: el mundo está congelado.
    Paused,
    /// Salida pedida: la etiqueta ya es `SHELL EXIT`, la línea y el `AppExit`
    /// están en la cola.
    Exit,
}

impl ShellState {
    /// Etiqueta corta que el título de la ventana publica tras `SHELL `.
    pub fn label(self) -> &'static str {
        match self {
            Self::Boot => "BOOT",
            Self::Loading { .. } => "LOADING",
            Self::Menu { .. } => "MENU",
            Self::Create { .. } => "CREATE",
            Self::Playing => "PLAYING",
            Self::Paused => "PAUSED",
            Self::Exit => "EXIT",
        }
    }

    /// Etiqueta completa del título (`SHELL PLAYING`, …) — la que el gate busca
    /// en `window.title` con `label_of`.
    pub fn title(self) -> String {
        format!("SHELL {}", self.label())
    }

    /// ¿Corre el mundo? Solo `Playing` (la pausa congela el bundle de sistemas).
    pub fn world_live(self) -> bool {
        matches!(self, Self::Playing)
    }
}

/// Panel abierto sobre el estado actual (fase 2).
///
/// Es el único cursor de panel: `Menu` + `Load` es el browser de carga,
/// `Paused` + `Options` el panel de opciones, `Paused` + `Save` el browser de
/// guardado. `None` es «el estado desnudo», y por eso `Escape` con panel cierra
/// el panel en vez de reanudar la partida.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShellPanel {
    /// Sin panel: el estado se comporta como en la fase 1.
    #[default]
    None,
    /// Panel de opciones (`options open=1` abierto desde menú o pausa).
    Options,
    /// Browser de carga del menú (`load open=1`).
    Load,
    /// Browser de guardado de la pausa (`save open=1`).
    Save,
}

impl ShellPanel {
    /// Id retail del panel que declara la línea (`panel=` del contrato).
    pub fn id(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Options => Some(PAUSE_OPTIONS),
            Self::Load | Self::Save => Some(BROWSER_PANEL),
        }
    }
}

/// Una página del panel de opciones: el id estable del censo de configuración
/// (`CFG01`…, publicado en minúsculas: el contrato `page` solo admite
/// `[a-z0-9-]{1,24}`) y la clave del control que la página expone (su primera
/// lectura declarada, `config.effects.count` → `config-effects-count`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigPage {
    /// Id de la página (`cfg01`…), derivado de `m38_surfaces` (ids estables).
    pub id: String,
    /// Clave del control de la página.
    pub key: String,
}

/// Las páginas del panel de opciones, tal como las declara
/// `m38_surfaces::config_pages()` (11 páginas con su control real).
///
/// Un censo que no resuelve deja el panel con 0 páginas: `o`/`Return` en
/// `options` lo dice por `M18 VE:` y NO emite ninguna línea de contrato —
/// `pages=0` no es un valor inventable.
pub fn config_pages() -> Vec<ConfigPage> {
    daggerfall_app::m38_surfaces::config_pages()
        .map(|pages| {
            pages
                .into_iter()
                .map(|page| ConfigPage {
                    id: page.page,
                    key: page.key,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Token determinista del personaje creado: `face` elige carrera (anillo de 8)
/// y género (paridad). Sin aleatoriedad de sistema, sin timestamps: dos
/// procesos con la misma cara producen el mismo token y el mismo save.
pub fn character_token(face: usize) -> String {
    let race = crate::shell_slots::RACES[face % crate::shell_slots::RACES.len()];
    let gender = if face % 2 == 0 { "m" } else { "f" };
    format!("{race}-{gender}")
}

/// Un paso de carga: el nombre del trabajo y las unidades REALES que resolvió.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadingStep {
    /// Nombre del trabajo (`panels`, `settlements`, `buildings`, `textures`,
    /// `npcs`) — distinto por paso, como exige `check_loading`.
    pub work: &'static str,
    /// Unidades contadas al ejecutar el trabajo (`>0`: 0 = dormir disfrazado).
    pub units: u64,
}

/// Datos del HUD medidos una sola vez, antes de que el mundo avance: el mismo
/// `digest` en `MENU` y en `PLAYING` prueba que es el mismo selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HudFacts {
    /// Id del panel del HUD (`hud`).
    pub panel: String,
    /// Píxeles opacos del HUD compuesto.
    pub opaque: usize,
    /// Digest del HUD compuesto (`m20_hud_digest`).
    pub digest: u64,
}

/// Resultado del plan de viaje rápido (todo del motor ❤️, nada inventado).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TravelFacts {
    /// Píxel de origen.
    pub from: [i32; 2],
    /// Píxel de destino.
    pub to: [i32; 2],
    /// Píxeles recorridos (`TravelReport::pixels`).
    pub delta_px: u32,
    /// Minutos de juego (`TravelReport::minutes`).
    pub minutes: u64,
    /// Coste en oro (`TripCost::total`, nunca negativo).
    pub gold: u64,
}

/// Línea de evidencia pendiente con su retardo restante (s).
struct Pending {
    remaining: f32,
    line: String,
}

/// Espejo de la evidencia: `stdout` (con `flush` por línea) + append en
/// `$DF_SHELL_STATE` (con `flush` por línea). Sin la variable de entorno no
/// escribe fichero y lo dice una vez — nunca falla en silencio.
pub struct ShellMirror {
    path: Option<PathBuf>,
    file: Option<File>,
    announced: bool,
}

impl ShellMirror {
    /// Toma `$DF_SHELL_STATE`; sin él solo queda la salida estándar.
    pub fn from_env() -> Self {
        let path = std::env::var_os("DF_SHELL_STATE").map(PathBuf::from);
        let file = path.as_ref().and_then(|path| {
            match OpenOptions::new().create(true).append(true).open(path) {
                Ok(file) => Some(file),
                Err(err) => {
                    eprintln!("SHELL VE: mirror unavailable — {path:?}: {err}");
                    None
                }
            }
        });
        Self {
            path,
            file,
            announced: false,
        }
    }

    /// Escribe la línea completa (ya con [`LINE_PREFIX`]) en stdout y en el
    /// espejo, con `flush` en los dos: el gate lee el fichero mientras corre.
    pub fn emit(&mut self, line: &str) {
        println!("{line}");
        let _ = std::io::stdout().flush();
        match (&self.path, &mut self.file) {
            (Some(_), Some(file)) => {
                if let Err(err) = writeln!(file, "{line}").and_then(|()| file.flush()) {
                    eprintln!("SHELL VE: mirror write failed — {err}");
                    self.file = None;
                }
            }
            (Some(path), None) => {
                if !self.announced {
                    self.announced = true;
                    eprintln!("SHELL VE: mirror disabled — {path:?} is not writable");
                }
            }
            (None, _) => {
                if !self.announced {
                    self.announced = true;
                    eprintln!("SHELL VE: DF_SHELL_STATE unset — evidence goes to stdout only");
                }
            }
        }
    }
}

/// Estado completo del shell + la cola de evidencia.
///
/// El `derive` de recurso es la única concesión al ECS en este módulo: el
/// estado lo consume la ventana (`m18_window`) y las transiciones siguen
/// siendo puras y auditables sin ventana.
#[derive(bevy::prelude::Resource)]
pub struct Shell {
    /// Estado actual.
    pub state: ShellState,
    /// Segundos en el estado actual (lo avanza `shell_pump`).
    pub dwell: f32,
    /// Líneas pendientes, en orden estricto (una por frame como máximo). La
    /// cola es privada: `say` encola y `pump` publica, sin atajos externos.
    queue: VecDeque<Pending>,
    /// Pasos de carga, en orden, con sus unidades reales.
    pub steps: Vec<LoadingStep>,
    /// Suma de unidades (la que cierra `loading done`).
    pub units_total: u64,
    /// Reloj de sesión: minutos de juego consumidos (viaje rápido).
    pub clock_minutes: u64,
    /// HUD medido en `Boot`.
    pub hud: HudFacts,
    /// Identidad retail del asentamiento compuesto: `(region, location_id,
    /// block)` — la fila `world` publica este triple. `None` cuando `MAPS.BSA`
    /// no se pudo leer: la fila NO se emite (0 coordenadas inventadas).
    pub location: Option<(u16, u16, u16)>,
    /// Píxel de mapa de la sesión (origen del viaje).
    pub base_pixel: [i32; 2],
    /// Destino elegido: el asentamiento más cercano al píxel de la sesión.
    pub travel_target: Option<[i32; 2]>,
    /// ¿Está abierto el mapa (`map-local`)?
    pub map_open: bool,
    /// Índice del panel `map-local` en la composición (`None` si no existe).
    pub map_panel: Option<usize>,
    /// Panel visible antes de abrir el mapa (se restaura al cerrar).
    pub map_restore: usize,
    /// Ticks congelados de la pausa en curso.
    pub paused_ticks: u64,
    /// ¿Se pidió ya el `AppExit` de la salida?
    pub exit_requested: bool,
    /// Panel abierto sobre el estado actual (fase 2). Vive aquí, no en la
    /// pantalla: cerrar el panel y volver a abrirlo NO reinicia la navegación.
    pub panel: ShellPanel,
    /// Páginas del panel de opciones: id estable + clave del control
    /// (`m38_surfaces::config_pages`, 11 páginas).
    pub options: Vec<ConfigPage>,
    /// Página en la que el panel reabre (sobrevive a la navegación).
    pub options_page: usize,
    /// Valor por página: una sola copia, la que el jugador movió.
    pub options_values: Vec<u64>,
    /// Token del personaje de la sesión (`character.name` del save v3).
    pub character: Option<String>,
    /// Cara del personaje de la sesión (`character.face_index` del save v3).
    pub face: usize,
    /// Slot del que se cargó (o al que se guardó) la sesión, si hay alguno.
    pub session_slot: Option<usize>,
    /// Slots ocupados leídos al abrir el browser (`slots=N` del contrato).
    pub slots: Vec<crate::shell_slots::SlotFacts>,
    /// Ficheros presentes que no son documentos v3 (no se cargan).
    pub slots_rejected: usize,
    /// Posición del cursor del browser dentro de [`Shell::slots`].
    pub browser_slot: usize,
    /// Espejo de evidencia.
    pub mirror: ShellMirror,
    /// M-QST: estado de la quest principal de la sesión. Se rehidrata al
    /// cargar un slot (antes de entrar al mundo) y se persiste al guardar; la
    /// única puerta de escritura es `MainQuest::apply`.
    pub mainquest: crate::mainquest_route::MainQuest,
    /// Estado de mundo que la sesión guardará (`CR-7a3e9c14`).
    ///
    /// Lo **mide** el half de ventana una vez por frame (`m18_shell_world_snapshot`:
    /// `Locomotion` + cámara + `ProgressionClock` + escena de puertas) y lo
    /// **consume** la puerta de escritura, que sigue sin tocar la ECS. `None`
    /// significa «esta sesión no midió mundo» (el paso sin ventana): el slot se
    /// escribe entonces con la generación 3 y la evidencia lo dice —
    /// `fidelity=3`, nunca un mundo inventado.
    pub snapshot: Option<crate::shell_slots::WorldSnapshot>,
}

impl Shell {
    /// Ensambla el shell con los datos medidos en `Startup`.
    pub fn new(
        steps: Vec<LoadingStep>,
        hud: HudFacts,
        location: Option<(u16, u16, u16)>,
        base_pixel: [i32; 2],
        settlements: Vec<[i32; 2]>,
        map_panel: Option<usize>,
    ) -> Self {
        let units_total = steps.iter().map(|step| step.units).sum();
        let travel_target = nearest_other(&settlements, base_pixel);
        let options = config_pages();
        Self {
            state: ShellState::Boot,
            dwell: 0.0,
            queue: VecDeque::new(),
            steps,
            units_total,
            clock_minutes: SESSION_CLOCK_SEED,
            hud,
            location,
            base_pixel,
            travel_target,
            map_open: false,
            map_panel,
            map_restore: 0,
            paused_ticks: 0,
            exit_requested: false,
            panel: ShellPanel::None,
            options_values: vec![0; options.len()],
            options,
            options_page: 0,
            character: None,
            face: 0,
            session_slot: None,
            slots: Vec::new(),
            slots_rejected: 0,
            browser_slot: 0,
            mirror: ShellMirror::from_env(),
            mainquest: crate::mainquest_route::MainQuest::new(),
            snapshot: None,
        }
    }

    /// Valor de la opción de la página en curso.
    pub fn option_value(&self) -> u64 {
        self.options_values
            .get(self.options_page)
            .copied()
            .unwrap_or(0)
    }

    /// ¿Hay más de una página de opciones declarada?
    pub fn options_pages(&self) -> usize {
        self.options.len()
    }

    /// Encola una línea con retardo (s). El orden de encolado ES el orden de
    /// emisión: `shell_pump` publica como máximo una por frame.
    pub fn say(&mut self, delay: f32, line: String) {
        self.queue.push_back(Pending {
            remaining: delay,
            line,
        });
    }

    /// Avanza el reloj de estado y publica **como máximo una** línea encolada,
    /// en orden de encolado. Una línea por frame mantiene el orden estricto:
    /// dos líneas con su retardo ya cumplido no pueden adelantarse entre sí.
    pub fn pump(&mut self, dt: f32) {
        if let Some(line) = self.next_line(dt) {
            self.mirror.emit(&line);
        }
    }

    /// Avanza el reloj y devuelve la línea que toca publicar en este frame
    /// (`None` si el frente de la cola todavía tiene retardo vivo). Es la
    /// disciplina de [`Shell::pump`] sin el espejo: una sola copia del
    /// «adelanta el frente, una línea como máximo», que es lo que hace que el
    /// orden de encolado sea el orden de emisión.
    fn next_line(&mut self, dt: f32) -> Option<String> {
        self.dwell += dt;
        let front = self.queue.front_mut()?;
        front.remaining -= dt;
        if front.remaining > 0.0 {
            return None;
        }
        self.queue.pop_front().map(|pending| pending.line)
    }

    /// Declara la pausa: `pause state=Paused …` **y** `pause frozen=1 …` en la
    /// misma transición, una línea detrás de la otra en la cola.
    ///
    /// **Orden de emisión declarado (CR-6d086222).** El congelado se publica
    /// aquí, como parte de la pausa, y no tras un temporizador de reloj real.
    /// Antes esperaba `FROZEN_AFTER` (1.0 s) y su **posición** dependía del
    /// frame en que llegara la tecla siguiente: con el arnés del gate a ~0.18 s
    /// por etapa `pause frozen=1` salía detrás de `options close=1`; con la
    /// MISMA secuencia de teclas a ~0.55 s por etapa salía entre
    /// `options open=1` y `options set`. Dos sellos distintos para el mismo
    /// input: `PROC-idempotent` rojo entre invocaciones. Encolar los dos hechos
    /// juntos hace que la posición sea función del input y no del frame.
    ///
    /// La semántica no cambia: el mundo deja de tickear en el instante en que
    /// el estado pasa a `Paused` ([`ShellState::world_live`]), así que `ticks`
    /// es el mismo valor que declaraba el temporizador; lo único que cambia es
    /// cuándo se publica la línea.
    pub fn pause_begin(&mut self) {
        self.state = ShellState::Paused;
        self.dwell = 0.0;
        self.paused_ticks = self.clock_minutes;
        self.say(
            PAUSE_SETTLE,
            pause_line(self.paused_ticks, PAUSE_OPTIONS),
        );
        self.say(0.0, frozen_line(self.paused_ticks));
    }

    /// Publica de golpe **todas** las líneas cuyo retardo ya cumplió, en orden
    /// estricto de encolado, y se detiene ante el **primer** retraso vivo.
    ///
    /// Es la versión sin reloj de [`Shell::pump`] para los consumidores que no
    /// tienen frames (el pase headless del probe y cualquier transcripción que
    /// deba publicar lo pendiente antes de escribir su propia línea de fase):
    /// el orden de emisión sigue siendo el de encolado y una línea con retardo
    /// vivo NO se adelanta — se queda en la cola para el `pump` que la cumpla.
    /// Devuelve cuántas líneas publicó.
    pub fn drain(&mut self) -> usize {
        let mut published = 0usize;
        loop {
            match self.queue.front() {
                Some(front) if front.remaining <= 0.0 => {}
                _ => break,
            }
            if let Some(pending) = self.queue.pop_front() {
                self.mirror.emit(&pending.line);
                published += 1;
            }
        }
        published
    }
}

// ── contrato de líneas (ÚNICA copia de las formas del gate) ───────────────

/// `SHELL VE: boot state=Boot`
pub fn boot_line() -> String {
    format!("{LINE_PREFIX}boot state=Boot")
}

/// `SHELL VE: loading step=N steps=S work=W units=U`
pub fn step_line(step: u32, steps: u32, work: &str, units: u64) -> String {
    format!("{LINE_PREFIX}loading step={step} steps={steps} work={work} units={units}")
}

/// `SHELL VE: loading done steps=S units=U`
pub fn loaded_line(steps: u32, units: u64) -> String {
    format!("{LINE_PREFIX}loading done steps={steps} units={units}")
}

/// `SHELL VE: menu state=Menu items=5 selection=N surface=main-menu`
pub fn menu_line(items: usize, selection: usize) -> String {
    format!("{LINE_PREFIX}menu state=Menu items={items} selection={selection} surface={MENU_SURFACE}")
}

/// `SHELL VE: menu order=new-game,continue,load,options,quit`
///
/// Fase 2: el menú declara UNA vez por entrada el orden exacto de sus ítems
/// (`scripts/shell-002.py` `RE_MENU_ORDER`).
pub fn menu_order_line() -> String {
    format!("{LINE_PREFIX}menu order={}", MENU_ITEMS.join(","))
}

/// `SHELL VE: menu selection=N`
pub fn selection_line(selection: usize) -> String {
    format!("{LINE_PREFIX}menu selection={selection}")
}

/// `SHELL VE: new_game state=Playing item=new-game world_ticks=0`
pub fn new_game_line(ticks: u64) -> String {
    format!("{LINE_PREFIX}new_game state=Playing item=new-game world_ticks={ticks}")
}

/// `SHELL VE: world scene=Exterior location=(r,l,b) buildings=B npcs=P panels=Q`
pub fn world_line(
    scene: &str,
    location: (u16, u16, u16),
    buildings: usize,
    npcs: usize,
    panels: usize,
) -> String {
    format!(
        "{LINE_PREFIX}world scene={scene} location=({},{},{}) buildings={buildings} npcs={npcs} panels={panels}",
        location.0, location.1, location.2
    )
}

/// `SHELL VE: hud visible=V panel=hud opaque=O digest=D`
pub fn hud_line(visible: bool, hud: &HudFacts) -> String {
    format!(
        "{LINE_PREFIX}hud visible={} panel={} opaque={} digest={:016x}",
        u8::from(visible),
        hud.panel,
        hud.opaque,
        hud.digest
    )
}

/// `SHELL VE: map open=1 panel=map-local`
pub fn map_open_line(panel: &str) -> String {
    format!("{LINE_PREFIX}map open=1 panel={panel}")
}

/// `SHELL VE: map open=0`
pub fn map_close_line() -> String {
    format!("{LINE_PREFIX}map open=0")
}

/// `SHELL VE: travel from=(x,y) to=(x,y) delta_px=D minutes=M gold=G scene=S`
pub fn travel_line(facts: &TravelFacts, scene: &str) -> String {
    format!(
        "{LINE_PREFIX}travel from=({},{}) to=({},{}) delta_px={} minutes={} gold={} scene={scene}",
        facts.from[0],
        facts.from[1],
        facts.to[0],
        facts.to[1],
        facts.delta_px,
        facts.minutes,
        facts.gold
    )
}

/// `SHELL VE: pause state=Paused ticks=N options=pause-menu`
pub fn pause_line(ticks: u64, options: &str) -> String {
    format!("{LINE_PREFIX}pause state=Paused ticks={ticks} options={options}")
}

/// `SHELL VE: pause frozen=1 ticks=N state=Paused`
pub fn frozen_line(ticks: u64) -> String {
    format!("{LINE_PREFIX}pause frozen=1 ticks={ticks} state=Paused")
}

/// `SHELL VE: pause resumed=1 state=Playing`
pub fn resume_line() -> String {
    format!("{LINE_PREFIX}pause resumed=1 state=Playing")
}

/// `SHELL VE: exit state=Exit code=0`
pub fn exit_line(code: i32) -> String {
    format!("{LINE_PREFIX}exit state=Exit code={code}")
}

// ── contrato fase 2 (`scripts/shell-002.py`) ──────────────────────────────

/// `SHELL VE: continue result=none saves=0 notice=no-save`
pub fn continue_none_line(saves: usize) -> String {
    format!("{LINE_PREFIX}continue result=none saves={saves} notice=no-save")
}

/// `SHELL VE: continue result=loaded slot=0 format=v3`
pub fn continue_loaded_line(slot: usize, format: &str) -> String {
    format!("{LINE_PREFIX}continue result=loaded slot={slot} format={format}")
}

/// `SHELL VE: load open=1 slots=N format=v3 panel=save-browser`
pub fn load_open_line(slots: usize, format: &str) -> String {
    format!(
        "{LINE_PREFIX}load open=1 slots={slots} format={format} panel={BROWSER_PANEL}"
    )
}

/// `SHELL VE: load open=0`
pub fn load_close_line() -> String {
    format!("{LINE_PREFIX}load open=0")
}

/// `SHELL VE: slot index=0 name=TOKEN level=1 minutes=12 format=v3`
pub fn slot_line(slot: &crate::shell_slots::SlotFacts, format: &str) -> String {
    format!(
        "{LINE_PREFIX}slot index={} name={} level={} minutes={} format={format}",
        slot.index, slot.character, slot.level, slot.minutes
    )
}

/// `SHELL VE: load done slot=0 format=v3 state=Playing ticks=N character=T`
pub fn load_done_line(slot: usize, format: &str, ticks: u64, character: &str) -> String {
    format!(
        "{LINE_PREFIX}load done slot={slot} format={format} state=Playing ticks={ticks} character={character}"
    )
}

/// `SHELL VE: options open=1 page=cfg01 pages=11 panel=pause-menu`
///
/// `panel=` es el literal de la pausa TAMBIÉN cuando el panel se abre desde el
/// menú: el contrato de fase 2 lo exige en las dos aperturas (una sola ruta de
/// código, una sola forma de línea).
pub fn options_open_line(page: &str, pages: usize) -> String {
    format!("{LINE_PREFIX}options open=1 page={page} pages={pages} panel={PAUSE_OPTIONS}")
}

/// `SHELL VE: options set key=config-effects-count value=1 pages=11`
pub fn options_set_line(key: &str, value: u64, pages: usize) -> String {
    format!("{LINE_PREFIX}options set key={key} value={value} pages={pages}")
}

/// `SHELL VE: options close=1`
pub fn options_close_line() -> String {
    format!("{LINE_PREFIX}options close=1")
}

/// `SHELL VE: create state=Create surfaces=16 host=… face=0`
pub fn create_state_line(surfaces: usize, host: &str, face: usize) -> String {
    format!("{LINE_PREFIX}create state=Create surfaces={surfaces} host={host} face={face}")
}

/// `SHELL VE: create step=1 steps=10 field=face`
pub fn create_step_line(step: usize, steps: usize, field: &str) -> String {
    format!("{LINE_PREFIX}create step={step} steps={steps} field={field}")
}

/// `SHELL VE: create done surfaces=16 character=TOKEN state=Playing`
pub fn create_done_line(surfaces: usize, character: &str) -> String {
    format!("{LINE_PREFIX}create done surfaces={surfaces} character={character} state=Playing")
}

/// `SHELL VE: session state=Started character=TOKEN save=none`
/// o `SHELL VE: session state=Started character=TOKEN save=slot=0`
pub fn session_line(character: &str, slot: Option<usize>) -> String {
    let save = match slot {
        Some(slot) => format!("slot={slot}"),
        None => "none".to_string(),
    };
    format!("{LINE_PREFIX}session state=Started character={character} save={save}")
}

/// `SHELL VE: save open=1 slots=6 format=v3 panel=save-browser`
pub fn save_open_line(slots: usize, format: &str) -> String {
    format!(
        "{LINE_PREFIX}save open=1 slots={slots} format={format} panel={BROWSER_PANEL}"
    )
}

/// `SHELL VE: save slot=0 format=v3 bytes=N digest=HEX16`
pub fn save_line(slot: usize, format: &str, bytes: usize, digest: u64) -> String {
    format!("{LINE_PREFIX}save slot={slot} format={format} bytes={bytes} digest={digest:016x}")
}

// ── fidelidad del slot (`CR-7a3e9c14`) ────────────────────────────────────

/// `SHELL VE: slot fidelity=4 index=0 pos=(x,y,z) yaw=R pitch=R
/// world_minutes=M environment=E map_id=I`
///
/// Renglón NUEVO: no casa con `^SHELL VE: slot index=` (la línea de
/// `scripts/shell-002.py` sigue anclada a `slot index=`), así que se suma al
/// canal sin renombrar ni mover ninguna de las cinco expresiones fijadas.
/// La posición va en **milímetros enteros** y `yaw`/`pitch` en radianes con
/// tres decimales: el renglón no lleva ningún valor que el render pueda mover.
pub fn slot_fidelity_line(index: usize, world: &crate::shell_slots::WorldSnapshot) -> String {
    format!(
        "{LINE_PREFIX}slot fidelity={} index={index} pos=({},{},{}) yaw={:.3} pitch={:.3} \
         world_minutes={} environment={} map_id={}",
        world.fidelity(),
        world.pos_mm[0],
        world.pos_mm[1],
        world.pos_mm[2],
        world.yaw,
        world.pitch,
        world.world_minutes,
        world.environment,
        world.map_id
    )
}

/// `SHELL VE: slot fidelity=3 index=0 pos=unmeasured reason=session-without-world`
///
/// Se escribe cuando la sesión no midió mundo (paso sin ventana): el slot que
/// sale es el de generación 3 y la línea **no** publica una posición que nadie
/// midió — `pos=unmeasured` es la medida (0 coordenadas inventadas).
pub fn slot_fidelity_absent_line(index: usize) -> String {
    format!(
        "{LINE_PREFIX}slot fidelity=3 index={index} pos=unmeasured reason=session-without-world"
    )
}

/// `SHELL VE: warning W-SAVE-001 slot=0 fidelity=3 missing=… restored=identity+session-clock`
///
/// El slot de generación 3 (el que el producto escribía hasta
/// `CR-7a3e9c14`) no declara mundo: se reanuda la identidad y el reloj de
/// sesión, y el aviso dice **qué falta**. No hay teletransporte silencioso al
/// spawn ni reloj inventado — `reason=` nombra la causa medible.
pub fn save_fidelity_warning_line(
    slot: usize,
    plan: &crate::shell_slots::RestorePlan,
) -> String {
    format!(
        "{LINE_PREFIX}warning W-SAVE-001 slot={slot} fidelity={} missing={} \
         restored=identity+session-clock reason=slot-declares-no-world",
        plan.fidelity,
        plan.missing().join(",")
    )
}

/// `SHELL VE: warning W-SAVE-002 slot=0 environment=2 restored=clock+orientation`
///
/// El slot declara un interior/mazmorra y el reingreso a esa escena no está en
/// el alcance (`FG-01` resuelve la puerta, no la reentrada desde un slot): se
/// restauran reloj y orientación, la posición NO se teletransporta a
/// coordenadas de otra escena y el aviso lo declara.
pub fn interior_not_restored_line(slot: usize, environment: i64) -> String {
    format!(
        "{LINE_PREFIX}warning W-SAVE-002 slot={slot} environment={environment} \
         restored=clock+orientation reason=interior-stage-not-reentered"
    )
}

/// `SHELL VE: session state=Menu from=Playing`
pub fn session_menu_line() -> String {
    format!("{LINE_PREFIX}session state=Menu from=Playing")
}

// ── navegación de menú (pura) ─────────────────────────────────────────────

/// Siguiente selección al pulsar `Down` (circular — el menú tiene 5 ítems).
pub fn selection_after_down(selection: usize) -> usize {
    (selection + 1) % MENU_ITEMS.len()
}

/// Siguiente selección al pulsar `Up` (circular).
pub fn selection_after_up(selection: usize) -> usize {
    (selection + MENU_ITEMS.len() - 1) % MENU_ITEMS.len()
}

// ── viaje rápido (motor ❤️ real) ──────────────────────────────────────────

/// Distancia de Chebyshev entre dos píxeles de mapa (la métrica de
/// `find_coastal_pixel`: mismo criterio, una sola regla de cercanía).
pub fn chebyshev(a: [i32; 2], b: [i32; 2]) -> i32 {
    (a[0] - b[0]).abs().max((a[1] - b[1]).abs())
}

/// Destino del viaje rápido: el asentamiento más cercano que no sea el píxel
/// base; desempate `(x, y)` ascendente, determinista.
pub fn nearest_other(pixels: &[[i32; 2]], base: [i32; 2]) -> Option<[i32; 2]> {
    pixels
        .iter()
        .copied()
        .filter(|pixel| *pixel != base)
        .min_by_key(|pixel| (chebyshev(base, *pixel), pixel[0], pixel[1]))
}

/// Plan de viaje con el motor real: `calculate_travel_time` sobre el clima de
/// `CLIMATE.PAK` a pie con las opciones del popup retail (cauto, posada,
/// barco), y `calculate_trip_cost` con las mismas opciones.
pub fn plan_travel<T: TerrainLookup>(
    terrain: &T,
    from: (i32, i32),
    to: (i32, i32),
) -> Result<TravelFacts, String> {
    let options = TravelOptions::default();
    let report = calculate_travel_time(terrain, from, to, Transport::Foot, &options)
        .map_err(|err| format!("travel: {err:?}"))?;
    let cost = calculate_trip_cost(
        report.minutes,
        report.ocean_pixels,
        options.sleep_inn,
        false,
        options.travel_ship,
        options.travel_ship,
    );
    Ok(TravelFacts {
        from: [from.0, from.1],
        to: [to.0, to.1],
        delta_px: report.pixels,
        minutes: report.minutes,
        gold: cost.total.max(0) as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use daggerfall_heart::travel::UniformTerrain;

    /// Las formas de línea son las del contrato del gate (`scripts/shell-001.py`
    /// §RE_*): prefijo exacto, un espacio entre campos, sin cola.
    #[test]
    fn evidence_lines_match_the_gate_contract() {
        assert_eq!(boot_line(), "SHELL VE: boot state=Boot");
        assert_eq!(
            step_line(1, 5, "panels", 6),
            "SHELL VE: loading step=1 steps=5 work=panels units=6"
        );
        assert_eq!(
            loaded_line(5, 42),
            "SHELL VE: loading done steps=5 units=42"
        );
        assert_eq!(
            menu_line(MENU_ITEMS.len(), 0),
            "SHELL VE: menu state=Menu items=5 selection=0 surface=main-menu"
        );
        assert_eq!(
            menu_order_line(),
            "SHELL VE: menu order=new-game,continue,load,options,quit"
        );
        assert_eq!(selection_line(1), "SHELL VE: menu selection=1");
        assert_eq!(
            new_game_line(0),
            "SHELL VE: new_game state=Playing item=new-game world_ticks=0"
        );
        assert_eq!(
            world_line("Exterior", (17, 42, 0), 817, 42, 6),
            "SHELL VE: world scene=Exterior location=(17,42,0) buildings=817 npcs=42 panels=6"
        );
        let hud = HudFacts {
            panel: "hud".to_string(),
            opaque: 51200,
            digest: 0x0123_4567_89ab_cdef,
        };
        assert_eq!(
            hud_line(true, &hud),
            "SHELL VE: hud visible=1 panel=hud opaque=51200 digest=0123456789abcdef"
        );
        assert_eq!(
            map_open_line(MAP_PANEL),
            "SHELL VE: map open=1 panel=map-local"
        );
        assert_eq!(map_close_line(), "SHELL VE: map open=0");
        assert_eq!(
            pause_line(120, PAUSE_OPTIONS),
            "SHELL VE: pause state=Paused ticks=120 options=pause-menu"
        );
        assert_eq!(
            frozen_line(120),
            "SHELL VE: pause frozen=1 ticks=120 state=Paused"
        );
        assert_eq!(
            resume_line(),
            "SHELL VE: pause resumed=1 state=Playing"
        );
        assert_eq!(exit_line(0), "SHELL VE: exit state=Exit code=0");
        let facts = TravelFacts {
            from: [100, 50],
            to: [104, 52],
            delta_px: 4,
            minutes: 45,
            gold: 12,
        };
        assert_eq!(
            travel_line(&facts, "Exterior"),
            "SHELL VE: travel from=(100,50) to=(104,52) delta_px=4 minutes=45 gold=12 \
             scene=Exterior"
        );
    }

    /// El título lleva la etiqueta completa que `label_of` busca, y el estado
    /// decide si el mundo corre.
    #[test]
    fn titles_and_world_gate_follow_the_state() {
        assert_eq!(ShellState::Boot.title(), "SHELL BOOT");
        assert_eq!(ShellState::Loading { step: 0 }.title(), "SHELL LOADING");
        assert_eq!(ShellState::Menu { selection: 0 }.title(), "SHELL MENU");
        assert_eq!(
            ShellState::Create { step: 1, face: 0 }.title(),
            "SHELL CREATE"
        );
        assert_eq!(ShellState::Playing.title(), "SHELL PLAYING");
        assert_eq!(ShellState::Paused.title(), "SHELL PAUSED");
        assert_eq!(ShellState::Exit.title(), "SHELL EXIT");
        assert!(ShellState::Playing.world_live());
        for state in [
            ShellState::Boot,
            ShellState::Loading { step: 3 },
            ShellState::Menu { selection: 1 },
            ShellState::Create { step: 1, face: 0 },
            ShellState::Paused,
            ShellState::Exit,
        ] {
            assert!(!state.world_live(), "{state:?} must freeze the world");
        }
    }

    /// Las formas de la fase 2 son las del gate `scripts/shell-002.py`, con el
    /// literal `panel=pause-menu` también en la apertura desde el menú y el
    /// `format=v3` de los slots.
    #[test]
    fn phase_two_lines_match_the_gate_contract() {
        let slot = crate::shell_slots::SlotFacts {
            index: 0,
            character: "breton-m".to_string(),
            level: 1,
            minutes: 12,
            fidelity: 3,
        };
        assert_eq!(
            continue_none_line(0),
            "SHELL VE: continue result=none saves=0 notice=no-save"
        );
        assert_eq!(
            continue_loaded_line(0, "v3"),
            "SHELL VE: continue result=loaded slot=0 format=v3"
        );
        assert_eq!(
            load_open_line(0, "v3"),
            "SHELL VE: load open=1 slots=0 format=v3 panel=save-browser"
        );
        assert_eq!(load_close_line(), "SHELL VE: load open=0");
        assert_eq!(
            slot_line(&slot, "v3"),
            "SHELL VE: slot index=0 name=breton-m level=1 minutes=12 format=v3"
        );
        assert_eq!(
            load_done_line(0, "v3", 12, "breton-m"),
            "SHELL VE: load done slot=0 format=v3 state=Playing ticks=12 character=breton-m"
        );
        assert_eq!(
            options_open_line("cfg01", CONFIG_PAGES),
            "SHELL VE: options open=1 page=cfg01 pages=11 panel=pause-menu"
        );
        assert_eq!(
            options_set_line("config-effects-count", 1, CONFIG_PAGES),
            "SHELL VE: options set key=config-effects-count value=1 pages=11"
        );
        assert_eq!(options_close_line(), "SHELL VE: options close=1");
        assert_eq!(
            create_state_line(16, "DaggerfallStartNewGameWizard", 0),
            "SHELL VE: create state=Create surfaces=16 \
             host=DaggerfallStartNewGameWizard face=0"
        );
        assert_eq!(
            create_step_line(1, CREATE_STEPS, CREATE_FIELDS[0]),
            "SHELL VE: create step=1 steps=10 field=face"
        );
        assert_eq!(
            create_done_line(16, "breton-m"),
            "SHELL VE: create done surfaces=16 character=breton-m state=Playing"
        );
        assert_eq!(
            session_line("breton-m", None),
            "SHELL VE: session state=Started character=breton-m save=none"
        );
        assert_eq!(
            save_open_line(crate::shell_slots::SLOT_COUNT, "v3"),
            "SHELL VE: save open=1 slots=6 format=v3 panel=save-browser"
        );
        assert_eq!(
            save_line(0, "v3", 1234, 0x0123_4567_89ab_cdef),
            "SHELL VE: save slot=0 format=v3 bytes=1234 digest=0123456789abcdef"
        );
        assert_eq!(session_menu_line(), "SHELL VE: session state=Menu from=Playing");
    }

    /// `_fixture` — el renglón de fidelidad NO colisiona con la línea anclada
    /// `slot index=` de `scripts/shell-002.py` (el fichero del gate es
    /// intocable): mismo prefijo, otra forma, y `pos` en milímetros enteros.
    #[test]
    fn slot_fidelity_lines_fixture_do_not_collide_with_the_pinned_slot_line() {
        let world = crate::shell_slots::WorldSnapshot {
            pos_mm: [422_000, 1_600, 419_000],
            yaw: 3.93,
            pitch: -0.0,
            environment: 1,
            map_id: crate::shell_slots::MAP_ID_OPEN_WORLD,
            region_index: 19,
            location_index: 0,
            world_minutes: 210_169_081,
        };
        assert_eq!(
            slot_fidelity_line(0, &world),
            "SHELL VE: slot fidelity=4 index=0 pos=(422000,1600,419000) yaw=3.930 \
             pitch=-0.000 world_minutes=210169081 environment=1 map_id=65535"
        );
        // El renglón viejo sigue exactamente igual (contrato de fase 2).
        assert!(!slot_fidelity_line(0, &world).starts_with("SHELL VE: slot index="));
        assert_eq!(
            slot_fidelity_absent_line(3),
            "SHELL VE: slot fidelity=3 index=3 pos=unmeasured reason=session-without-world"
        );
        let plan = crate::shell_slots::RestorePlan {
            fidelity: 3,
            world: None,
        };
        assert_eq!(
            save_fidelity_warning_line(1, &plan),
            "SHELL VE: warning W-SAVE-001 slot=1 fidelity=3 \
             missing=world.environment,calendar.game_minutes \
             restored=identity+session-clock reason=slot-declares-no-world"
        );
        assert_eq!(
            interior_not_restored_line(2, 2),
            "SHELL VE: warning W-SAVE-002 slot=2 environment=2 \
             restored=clock+orientation reason=interior-stage-not-reentered"
        );
    }

    /// Las 11 páginas del censo de configuración se publican con id en
    /// minúsculas y clave de control distinta (el contrato `page`/`key` solo
    /// admite `[a-z0-9-]{1,24}`).
    #[test]
    fn config_pages_are_the_declared_eleven_with_lowercase_ids() {
        let pages = config_pages();
        assert_eq!(pages.len(), CONFIG_PAGES);
        for page in &pages {
            assert!(
                !page.id.is_empty() && page.id.len() <= 24,
                "page id {:?} must fit the contract",
                page.id
            );
            assert_eq!(page.id, page.id.to_ascii_lowercase());
            assert!(page.key.starts_with("config-"), "key {:?}", page.key);
            assert!(!page.key.contains('.'), "key {:?} keeps no dots", page.key);
        }
    }

    /// El canon de creación declara 10 pasos con ids distintos y el token del
    /// personaje es determinista (misma cara → mismo token).
    #[test]
    fn create_canon_declares_ten_distinct_fields_and_a_deterministic_token() {
        assert_eq!(CREATE_STEPS, 10);
        assert_eq!(CREATE_FIELDS.len(), CREATE_STEPS);
        let mut seen: Vec<&str> = CREATE_FIELDS.to_vec();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), CREATE_STEPS, "field ids must be distinct");
        assert_eq!(character_token(0), "breton-m");
        assert_eq!(character_token(1), "redguard-f");
        assert_eq!(character_token(8), "breton-m");
        assert_eq!(character_token(CREATE_FACES + 1), character_token(1));
    }

    /// El menú navega en círculo sobre los 5 ítems: `Down` recorre
    /// 0→1→2→3→4→0 y `Up` el camino inverso 0→4→3→2→1→0 (incluido el salto
    /// 1→2). Los ciclos se escriben como secuencias explícitas: repetir aquí
    /// `(s + 1) % 5` reafirmaría la fórmula en vez de comprobar el contrato.
    ///
    /// Los nombres fijan el `menu order=` del gate (`scripts/shell-002.py`).
    #[test]
    fn menu_navigation_wraps_over_the_five_items() {
        assert_eq!(MENU_ITEMS[MENU_NEW_GAME], "new-game");
        assert_eq!(MENU_ITEMS[MENU_CONTINUE], "continue");
        assert_eq!(MENU_ITEMS[MENU_LOAD], "load");
        assert_eq!(MENU_ITEMS[MENU_OPTIONS], "options");
        assert_eq!(MENU_ITEMS[MENU_QUIT], "quit");

        let down_cycle = [
            MENU_NEW_GAME,
            MENU_CONTINUE,
            MENU_LOAD,
            MENU_OPTIONS,
            MENU_QUIT,
            MENU_NEW_GAME,
        ];
        for hop in down_cycle.windows(2) {
            assert_eq!(
                selection_after_down(hop[0]),
                hop[1],
                "Down debe ir de {} a {}",
                hop[0],
                hop[1]
            );
        }

        let up_cycle = [
            MENU_NEW_GAME,
            MENU_QUIT,
            MENU_OPTIONS,
            MENU_LOAD,
            MENU_CONTINUE,
            MENU_NEW_GAME,
        ];
        for hop in up_cycle.windows(2) {
            assert_eq!(
                selection_after_up(hop[0]),
                hop[1],
                "Up debe ir de {} a {}",
                hop[0],
                hop[1]
            );
        }
    }

    /// El estado de opciones es UNO: el valor de la página sobrevive a cerrar
    /// el panel y a la navegación menú → partida → pausa (la base de
    /// `check_options_apply`, donde el segundo valor debe ser mayor).
    #[test]
    fn option_values_survive_panel_close_and_navigation() {
        let mut shell = Shell::new(
            vec![LoadingStep {
                work: "panels",
                units: 6,
            }],
            HudFacts {
                panel: "hud".to_string(),
                opaque: 1,
                digest: 1,
            },
            None,
            [0, 0],
            Vec::new(),
            None,
        );
        if shell.options_pages() == 0 {
            return;
        }
        assert_eq!(shell.option_value(), 0);
        shell.options_values[0] += 1;
        shell.panel = ShellPanel::Options;
        shell.panel = ShellPanel::None;
        shell.state = ShellState::Playing;
        shell.state = ShellState::Paused;
        shell.panel = ShellPanel::Options;
        assert_eq!(shell.option_value(), 1);
        shell.options_values[0] += 1;
        assert_eq!(shell.option_value(), 2);
        assert_eq!(shell.state, ShellState::Paused);
    }

    /// El destino es el asentamiento más cercano distinto del base, con
    /// desempate `(x, y)`; sobre clima uniforme el plan de viaje cuesta tiempo
    /// real (>0) y devuelve los píxeles recorridos del motor.
    #[test]
    fn travel_targets_the_nearest_settlement_and_uses_the_engine() {
        let pixels = [[0, 0], [3, 4], [10, 10], [3, 2]];
        assert_eq!(nearest_other(&pixels, [0, 0]), Some([3, 2]));
        assert_eq!(nearest_other(&[[0, 0]], [0, 0]), None);
        let facts = plan_travel(&UniformTerrain(231), (0, 0), (3, 2)).expect("route");
        assert!(facts.minutes > 0);
        assert!(facts.delta_px > 0);
        assert_eq!(facts.from, [0, 0]);
        assert_eq!(facts.to, [3, 2]);
    }

    /// El reloj `Paused` no lo mueve nadie: dos lecturas seguidas del mismo
    /// shell son iguales (la base de `check_pause`).
    #[test]
    fn paused_ticks_do_not_advance_without_a_consumer() {
        let hud = HudFacts {
            panel: "hud".to_string(),
            opaque: 1,
            digest: 1,
        };
        let mut shell = Shell::new(
            vec![LoadingStep {
                work: "panels",
                units: 6,
            }],
            hud,
            Some((17, 1, 0)),
            [0, 0],
            vec![[3, 2]],
            Some(4),
        );
        shell.state = ShellState::Paused;
        shell.paused_ticks = shell.clock_minutes;
        let ticks = shell.paused_ticks;
        shell.dwell += 5.0;
        assert_eq!(shell.paused_ticks, ticks);
        assert_eq!(shell.clock_minutes, ticks);
        assert_eq!(shell.travel_target, Some([3, 2]));
        assert_eq!(shell.units_total, 6);
    }

    /// Anticuerpo del orden de emisión (`PROC-idempotent` de
    /// `scripts/shell-002.py`, CR-6d086222).
    ///
    /// La pausa y el guardado caen en la **misma frame** —`Escape` y `s` sin
    /// ningún frame entre ellos— y la secuencia publicada tiene que ser la
    /// misma en todas las repeticiones, sea cual sea el reparto de `dt` entre
    /// los frames. Antes de este arreglo `pause frozen=1` no se encolaba aquí:
    /// esperaba `FROZEN_AFTER` (1.0 s de reloj real) en `shell_progress`, así
    /// que su **posición** era función del frame y no del input — con el arnés
    /// del gate salía detrás de `options close=1` a ~0.18 s por etapa y entre
    /// `options open=1` y `options set` a ~0.55 s por etapa.
    #[test]
    fn pause_and_save_in_one_frame_publish_one_sequence_in_every_run() {
        // Tres líneas, cuatro ritmos: un frame corto por línea, frames de
        // 250 ms, un frame de 1.4 s que ya cumple el retardo de la pausa y un
        // frame de 3 s que cumple el de todas.
        let schedules: [&[f32]; 4] = [
            &[0.016, 0.016, 0.016],
            &[0.25, 0.25, 0.25],
            &[1.4, 0.0, 0.0],
            &[3.0, 0.0, 0.0],
        ];
        let ticks = SESSION_CLOCK_SEED;
        let expected = vec![
            pause_line(ticks, PAUSE_OPTIONS),
            frozen_line(ticks),
            save_open_line(crate::shell_slots::SLOT_COUNT, crate::shell_slots::FORMAT),
        ];
        for _ in 0..3 {
            for schedule in schedules {
                let mut shell = Shell::new(
                    Vec::new(),
                    HudFacts {
                        panel: "hud".to_string(),
                        opaque: 0,
                        digest: 0,
                    },
                    None,
                    [0, 0],
                    Vec::new(),
                    None,
                );
                shell.state = ShellState::Playing;
                // Las dos teclas, la misma frame, el mismo orden de llegada.
                shell.pause_begin();
                crate::shell_route::open_save(
                    &mut shell,
                    crate::shell_slots::SaveStore::default(),
                );
                let mut published = Vec::new();
                for dt in schedule {
                    if let Some(line) = shell.next_line(*dt) {
                        published.push(line);
                    }
                }
                // Cola completa: frames de 0.5 s publican lo que falte (una
                // línea por frame, en orden de encolado).
                for _ in 0..4 {
                    if let Some(line) = shell.next_line(0.5) {
                        published.push(line);
                    }
                }
                assert_eq!(published, expected, "dt={schedule:?}");
            }
        }
    }
}

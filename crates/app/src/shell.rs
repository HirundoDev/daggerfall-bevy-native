//! **M-SHELL phase 2 — application layer** (`CR-ca78eb76` + `CR-a34396f5`): the
//! shell state (`Boot → Loading → Menu → Create → Playing ⇄ Paused →
//! Exit`), its pure transitions and the evidence the gates consume
//! `scripts/shell-001.py` (phase 1) and `scripts/shell-002.py` (phase 2).
//!
//! This module neither draws nor simulates: the Bevy systems that drive it live in
//! `m18_window` (where the world's resources are) and here stays what can be
//! audited without a window — the `SHELL VE:` line contract, the byte-identical
//! mirror in `$DF_SHELL_STATE`, the discrete session clock, the fast-travel plan
//! over `daggerfall_heart::travel`, and the phase 2 navigation state (5-item menu,
//! creation wizard, options panel and slot
//! browser).
//!
//! ## Phase 2 (`CR-a34396f5`) — state that survives navigation
//!
//! Three pieces live in the [`Shell`] and NOT on the screen: the open **panel**
//! ([`ShellPanel`]), the **options state** (a single page with its value, which
//! survives closing the panel and entering/leaving a game) and the **slot
//! browser** (selected slot + facts read from `DF_SAVE_DIR`).
//! The slots are 🧠 Memory v3 documents (`crate::shell_slots`): the shell
//! does not invent a format of its own.
//!
//! ## Evidence contract (verbatim from the `CR-ca78eb76-shell-gate.md` pack)
//!
//! The gate compares the lines against exact `re.compile(...)` patterns
//! (`shell-001.py` §contract): this module's constructors are the ONLY copy of those
//! shapes. Adding a line requires touching the contract (and the gate) at the same time.
//!
//! ## Session clock (`ticks`) — why it is discrete
//!
//! `PROC-two-runs` requires the 15 lines to be **byte-identical** between two
//! processes, so no field can be derived from `dt` or from wall-clock time.
//! The clock the shell reports counts **game minutes consumed by the
//! player** (fast travel is the only consumer in this phase); the real-time M4
//! clock (`ProgressionClock`) stays alive for the skills and is NOT
//! reported. In `Paused` the clock is not touched: the frozen value is the same by
//! construction and by absence of consumers, not by a constant.

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use daggerfall_heart::travel::{
    calculate_travel_time, calculate_trip_cost, TerrainLookup, Transport, TravelOptions,
};

/// Prefix of every evidence line (the gate filters by it, and the stdout
/// mirror uses the same condition: `mirrored_lines`).
pub const LINE_PREFIX: &str = "SHELL VE: ";

/// Main menu items in navigation order (phase 2, `CR-a34396f5`):
/// the five destinations the `scripts/shell-002.py` gate requires declared in
/// `menu order=` and that the retail startup menu paints
/// (`m39_block_a::MENU_TARGETS`, same order). `new-game` is index 0
/// (initial selection of the contract) and `quit` the last one.
pub const MENU_ITEMS: [&str; 5] = ["new-game", "continue", "load", "options", "quit"];

/// Index of `new-game` inside [`MENU_ITEMS`].
pub const MENU_NEW_GAME: usize = 0;
/// Index of `continue` inside [`MENU_ITEMS`].
pub const MENU_CONTINUE: usize = 1;
/// Index of `load` inside [`MENU_ITEMS`].
pub const MENU_LOAD: usize = 2;
/// Index of `options` inside [`MENU_ITEMS`].
pub const MENU_OPTIONS: usize = 3;
/// Index of `quit` inside [`MENU_ITEMS`].
pub const MENU_QUIT: usize = 4;

/// Canon creation steps (`create step=<n> steps=10`): the retail wizard
/// paints 16 surfaces (`m38_surfaces::CREATE_SURFACE_COUNT`) and asks these
/// ten fields in this order. The ids are stable and distinct (contract
/// `ID = [a-z0-9-]{1,24}`).
pub const CREATE_STEPS: usize = 10;
/// Canon creation fields, in the order the wizard asks them.
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
/// Wizard faces (`FACES.CIF`): `Right`/`Left` walk this ring.
pub const CREATE_FACES: usize = 16;
/// Fixed seed of the session clock (startup game minutes).
///
/// No real clock is read: the pack requires a deterministic clock
/// ("fixed seed per slot") and `PROC-two-runs` requires byte identity, so the
/// session starts at minute 12 of the game day and only advances with
/// real consumers (fast travel). It is a declared seed, not a retail fact.
/// retail.
pub const SESSION_CLOCK_SEED: u64 = 12;

/// Superficie retail del menú (`m18_phase::MENU_PANEL_ID`).
pub const MENU_SURFACE: &str = "main-menu";
/// Retail panel that `M` opens (5th session panel of `ui_spec`, `map-local`).
pub const MAP_PANEL: &str = "map-local";
/// Id of the options surface the pause declares.
pub const PAUSE_OPTIONS: &str = "pause-menu";
/// Id of the slot browser panel (`panel=save-browser` of the contract).
pub const BROWSER_PANEL: &str = "save-browser";
/// Pages of the options panel: the 11 configuration surfaces of the census
/// (`m38_surfaces::CONFIG_SURFACE_COUNT`), the contract's `pages=11`.
pub const CONFIG_PAGES: usize = daggerfall_app::m38_surfaces::CONFIG_SURFACE_COUNT;

/// Minimum time the boot screen is presented (s).
pub const BOOT_DWELL: f32 = 1.0;
/// Minimum time per loading step (s) — the screen must be legible; the step's
/// units come from the real work, never from this timer.
pub const LOADING_STEP_DWELL: f32 = 0.30;
/// Delay of a state-change line: the title already written to the X
/// server is sampled BEFORE the line appears (the gate samples every
/// 50 ms during `wait`).
pub const LABEL_SETTLE: f32 = 0.50;
/// Delay of the pause line after the label change.
pub const PAUSE_SETTLE: f32 = 0.45;
/// Retry of the keyboard-focus request in the shell antechamber (s).
pub const FOCUS_RETRY: f32 = 0.50;
/// Delay of the resume line after the label change.
pub const RESUME_SETTLE: f32 = 0.30;
/// Delay of `exit state=Exit` after setting the `SHELL EXIT` label.
pub const EXIT_SETTLE: f32 = 0.60;
/// Time the process stays alive after the exit line (s): the
/// `SHELL EXIT` title must have been sampled and `AppExit` arrives with 0.
pub const EXIT_AFTER_LINE: f32 = 0.80;

/// World metres per map pixel the avatar advances in one fast travel
/// (declared: the phase 1 world is not streamed, so travel moves the
/// player and their map pixel, it does not load another cell).
pub const TRAVEL_M_PER_PIXEL: f32 = 8.0;

/// Shell state. `Loading` carries the step in progress (0-based) and `Menu` the
/// selection; the rest are flat states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellState {
    /// Boot screen, before any loading work.
    Boot,
    /// Loading in progress: `step` is the index of the step being presented.
    Loading {
        /// Paso en curso (0-based).
        step: usize,
    },
    /// Navigable menu with the current selection.
    Menu {
        /// Índice seleccionado dentro de [`MENU_ITEMS`].
        selection: usize,
    },
    /// Creation wizard: `step` (1-based, `1..=CREATE_STEPS`) and the chosen
    /// face (`Right`/`Left`).
    Create {
        /// Paso del canon en curso (`CREATE_FIELDS[step - 1]`).
        step: usize,
        /// Cara del personaje (`0..CREATE_FACES`).
        face: usize,
    },
    /// Mundo vivo.
    Playing,
    /// Pause: the world is frozen.
    Paused,
    /// Exit requested: the label is already `SHELL EXIT`, the line and the `AppExit`
    /// are in the queue.
    Exit,
}

impl ShellState {
    /// Short label the window title publishes after `SHELL `.
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

    /// Full title label (`SHELL PLAYING`, ...) — the one the gate looks up
    /// in `window.title` with `label_of`.
    pub fn title(self) -> String {
        format!("SHELL {}", self.label())
    }

    /// Is the world running? Only `Playing` (the pause freezes the system bundle).
    pub fn world_live(self) -> bool {
        matches!(self, Self::Playing)
    }
}

/// Panel open over the current state (phase 2).
///
/// It is the only panel cursor: `Menu` + `Load` is the load browser,
/// `Paused` + `Options` the options panel, `Paused` + `Save` the save
/// browser. `None` is "the bare state", and that is why `Escape` with a panel closes
/// the panel instead of resuming the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShellPanel {
    /// Without a panel: the state behaves as in phase 1.
    #[default]
    None,
    /// Options panel (`options open=1` opened from menu or pause).
    Options,
    /// Menu load browser (`load open=1`).
    Load,
    /// Pause save browser (`save open=1`).
    Save,
}

impl ShellPanel {
    /// Retail id of the panel the line declares (`panel=` of the contract).
    pub fn id(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Options => Some(PAUSE_OPTIONS),
            Self::Load | Self::Save => Some(BROWSER_PANEL),
        }
    }
}

/// One page of the options panel: the stable id of the configuration census
/// (`CFG01`..., published in lowercase: the contract `page` only accepts
/// `[a-z0-9-]{1,24}`) and the control key the page exposes (its first
/// declared reading, `config.effects.count` → `config-effects-count`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigPage {
    /// Page id (`cfg01`...), derived from `m38_surfaces` (stable ids).
    pub id: String,
    /// Control key of the page.
    pub key: String,
}

/// The options panel pages, as declared by
/// `m38_surfaces::config_pages()` (11 pages with their real control).
///
/// A census that does not resolve leaves the panel with 0 pages: `o`/`Return` in
/// `options` says so through `M18 VE:` and emits NO contract line —
/// `pages=0` is not an inventable value.
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

/// Deterministic token of the created character: `face` chooses race (8 ring)
/// and gender (parity). No system randomness, no timestamps: two
/// processes with the same face produce the same token and the same save.
pub fn character_token(face: usize) -> String {
    let race = crate::shell_slots::RACES[face % crate::shell_slots::RACES.len()];
    let gender = if face % 2 == 0 { "m" } else { "f" };
    format!("{race}-{gender}")
}

/// One loading step: the job name and the REAL units it resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadingStep {
    /// Job name (`panels`, `settlements`, `buildings`, `textures`,
    /// `npcs`) — different per step, as `check_loading` requires.
    pub work: &'static str,
    /// Units counted when running the job (`>0`: 0 = sleeping in disguise).
    pub units: u64,
}

/// HUD data measured once, before the world advances: the same
/// `digest` in `MENU` and in `PLAYING` proves it is the same selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HudFacts {
    /// Id of the HUD panel (`hud`).
    pub panel: String,
    /// Píxeles opacos del HUD compuesto.
    pub opaque: usize,
    /// Digest del HUD compuesto (`m20_hud_digest`).
    pub digest: u64,
}

/// Result of the fast-travel plan (all from the ❤️ engine, nothing invented).
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

/// Pending evidence line with its remaining delay (s).
struct Pending {
    remaining: f32,
    line: String,
}

/// Evidence mirror: `stdout` (with per-line `flush`) + append to
/// `$DF_SHELL_STATE` (with per-line `flush`). Without the environment variable it does not
/// write a file and says so once — it never fails silently.
pub struct ShellMirror {
    path: Option<PathBuf>,
    file: Option<File>,
    announced: bool,
}

impl ShellMirror {
    /// Takes `$DF_SHELL_STATE`; without it only standard output remains.
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

    /// Writes the full line (already with [`LINE_PREFIX`]) to stdout and to the
    /// mirror, with `flush` on both: the gate reads the file while it runs.
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

/// Full shell state + the evidence queue.
///
/// The resource `derive` is the only concession to ECS in this module: the
/// state is consumed by the window (`m18_window`) and the transitions remain
/// pure and auditable without a window.
#[derive(bevy::prelude::Resource)]
pub struct Shell {
    /// Estado actual.
    pub state: ShellState,
    /// Segundos en el estado actual (lo avanza `shell_pump`).
    pub dwell: f32,
    /// Pending lines, in strict order (at most one per frame). The
    /// queue is private: `say` enqueues and `pump` publishes, with no outside shortcuts.
    queue: VecDeque<Pending>,
    /// Loading steps, in order, with their real units.
    pub steps: Vec<LoadingStep>,
    /// Sum of units (the one that closes `loading done`).
    pub units_total: u64,
    /// Session clock: game minutes consumed (fast travel).
    pub clock_minutes: u64,
    /// HUD medido en `Boot`.
    pub hud: HudFacts,
    /// Retail identity of the composed settlement: `(region, location_id,
    /// block)` — the `world` row publishes this triple. `None` when `MAPS.BSA`
    /// could not be read: the row is NOT emitted (0 invented coordinates).
    pub location: Option<(u16, u16, u16)>,
    /// Map pixel of the session (travel origin).
    pub base_pixel: [i32; 2],
    /// Chosen destination: the settlement closest to the session's pixel.
    pub travel_target: Option<[i32; 2]>,
    /// Is the map open (`map-local`)?
    pub map_open: bool,
    /// Index of the `map-local` panel in the composition (`None` if it does not exist).
    pub map_panel: Option<usize>,
    /// Visible panel before opening the map (restored on close).
    pub map_restore: usize,
    /// Frozen ticks of the pause in progress.
    pub paused_ticks: u64,
    /// Was the exit `AppExit` already requested?
    pub exit_requested: bool,
    /// Panel open over the current state (phase 2). It lives here, not on the
    /// screen: closing the panel and reopening it does NOT reset navigation.
    pub panel: ShellPanel,
    /// Options panel pages: stable id + control key
    /// (`m38_surfaces::config_pages`, 11 pages).
    pub options: Vec<ConfigPage>,
    /// Page the panel reopens on (survives navigation).
    pub options_page: usize,
    /// Value per page: a single copy, the one the player moved.
    pub options_values: Vec<u64>,
    /// Character token of the session (`character.name` of the v3 save).
    pub character: Option<String>,
    /// Character face of the session (`character.face_index` of the v3 save).
    pub face: usize,
    /// Slot the session was loaded from (or saved to), if any.
    pub session_slot: Option<usize>,
    /// Occupied slots read when opening the browser (`slots=N` of the contract).
    pub slots: Vec<crate::shell_slots::SlotFacts>,
    /// Ficheros presentes que no son documentos v3 (no se cargan).
    pub slots_rejected: usize,
    /// Browser cursor position inside [`Shell::slots`].
    pub browser_slot: usize,
    /// Espejo de evidencia.
    pub mirror: ShellMirror,
    /// M-QST: main quest state of the session. It is rehydrated when
    /// loading a slot (before entering the world) and persisted on save; the
    /// only write door is `MainQuest::apply`.
    pub mainquest: crate::mainquest_route::MainQuest,
    /// World state the session will save (`CR-7a3e9c14`).
    ///
    /// The window half **measures** it once per frame (`m18_shell_world_snapshot`:
    /// `Locomotion` + camera + `ProgressionClock` + door scene) and the
    /// write door **consumes** it, still without touching ECS. `None`
    /// means "this session did not measure a world" (the windowless pass): the slot is
    /// then written with generation 3 and the evidence says so —
    /// `fidelity=3`, never an invented world.
    pub snapshot: Option<crate::shell_slots::WorldSnapshot>,
}

impl Shell {
    /// Assembles the shell with the data measured in `Startup`.
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

    /// Value of the option of the current page.
    pub fn option_value(&self) -> u64 {
        self.options_values
            .get(self.options_page)
            .copied()
            .unwrap_or(0)
    }

    /// Is more than one options page declared?
    pub fn options_pages(&self) -> usize {
        self.options.len()
    }

    /// Enqueues a line with a delay (s). The enqueue order IS the emission
    /// order: `shell_pump` publishes at most one per frame.
    pub fn say(&mut self, delay: f32, line: String) {
        self.queue.push_back(Pending {
            remaining: delay,
            line,
        });
    }

    /// Advances the state clock and publishes **at most one** enqueued line,
    /// in enqueue order. One line per frame keeps the strict order:
    /// two lines whose delay has already elapsed cannot overtake each other.
    pub fn pump(&mut self, dt: f32) {
        if let Some(line) = self.next_line(dt) {
            self.mirror.emit(&line);
        }
    }

    /// Advances the clock and returns the line to publish this frame
    /// (`None` if the queue front still has a live delay). It is
    /// [`Shell::pump`]'s discipline without the mirror: a single copy of
    /// "advance the front, one line at most", which is what makes the
    /// enqueue order the emission order.
    fn next_line(&mut self, dt: f32) -> Option<String> {
        self.dwell += dt;
        let front = self.queue.front_mut()?;
        front.remaining -= dt;
        if front.remaining > 0.0 {
            return None;
        }
        self.queue.pop_front().map(|pending| pending.line)
    }

    /// Declares the pause: `pause state=Paused ...` **and** `pause frozen=1 ...` in the
    /// same transition, one line after the other in the queue.
    ///
    /// **Declared emission order (CR-6d086222).** The freeze is published
    /// here, as part of the pause, and not after a real-clock timer.
    /// It used to wait for `FROZEN_AFTER` (1.0 s) and its **position** depended on the
    /// frame in which the next key arrived: with the gate harness at ~0.18 s
    /// per stage `pause frozen=1` came out behind `options close=1`; with the
    /// SAME key sequence at ~0.55 s per stage it came out between
    /// `options open=1` and `options set`. Two distinct seals for the same
    /// input: `PROC-idempotent` red between invocations. Enqueuing both facts
    /// together makes the position a function of the input and not of the frame.
    ///
    /// The semantics do not change: the world stops ticking at the instant
    /// the state becomes `Paused` ([`ShellState::world_live`]), so `ticks`
    /// is the same value the timer declared; the only thing that changes is
    /// when the line is published.
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

    /// Publishes at once **all** lines whose delay has already elapsed, in strict
    /// enqueue order, and stops at the **first** live delay.
    ///
    /// It is the clock-free version of [`Shell::pump`] for consumers with no
    /// frames (the probe's headless pass and any transcript that must publish what is
    /// pending before writing its own phase line): the emission order remains the enqueue
    /// order and a line with a live delay is NOT overtaken — it stays in the queue for the
    /// `pump` that fulfils it. Returns how many lines it published.
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

// ── line contract (ONLY copy of the gate shapes) ──────────────────────────

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
/// Phase 2: the menu declares once per entry the exact order of its items
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
/// `panel=` is the pause literal ALSO when the panel opens from the
/// menu: the phase 2 contract requires it in both openings (a single code
/// path, a single line shape).
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
/// NEW row: it does not match `^SHELL VE: slot index=` (the
/// `scripts/shell-002.py` line stays anchored to `slot index=`), so it joins the
/// channel without renaming or moving any of the five pinned expressions.
/// The position travels in **integer millimetres** and `yaw`/`pitch` in radians with
/// three decimals: the row carries no value the renderer can move.
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
/// It is written when the session did not measure a world (windowless pass): the slot that
/// comes out is the generation 3 one and the line does **not** publish a position nobody
/// measured — `pos=unmeasured` is the measurement (0 invented coordinates).
pub fn slot_fidelity_absent_line(index: usize) -> String {
    format!(
        "{LINE_PREFIX}slot fidelity=3 index={index} pos=unmeasured reason=session-without-world"
    )
}

/// `SHELL VE: warning W-SAVE-001 slot=0 fidelity=3 missing=... restored=identity+session-clock`
///
/// The generation 3 slot (the one the product wrote until
/// `CR-7a3e9c14`) does not declare a world: identity and the session
/// clock are resumed, and the warning says **what is missing**. There is no silent teleport to
/// spawn and no invented clock — `reason=` names the measurable cause.
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
/// The slot declares an interior/dungeon and re-entry to that scene is out of
/// scope (`FG-01` resolves the door, not the re-entry from a slot): clock and
/// orientation are restored, the position is NOT teleported to
/// coordinates of another scene, and the warning declares it.
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

// ── menu navigation (pure) ────────────────────────────────────────────

/// Next selection on `Down` (circular — the menu has 5 items).
pub fn selection_after_down(selection: usize) -> usize {
    (selection + 1) % MENU_ITEMS.len()
}

/// Siguiente selección al pulsar `Up` (circular).
pub fn selection_after_up(selection: usize) -> usize {
    (selection + MENU_ITEMS.len() - 1) % MENU_ITEMS.len()
}

// ── viaje rápido (motor ❤️ real) ──────────────────────────────────────────

/// Chebyshev distance between two map pixels (the metric of
/// `find_coastal_pixel`: same criterion, a single proximity rule).
pub fn chebyshev(a: [i32; 2], b: [i32; 2]) -> i32 {
    (a[0] - b[0]).abs().max((a[1] - b[1]).abs())
}

/// Fast-travel destination: the nearest settlement that is not the base
/// pixel; tie-break by ascending `(x, y)`, deterministic.
pub fn nearest_other(pixels: &[[i32; 2]], base: [i32; 2]) -> Option<[i32; 2]> {
    pixels
        .iter()
        .copied()
        .filter(|pixel| *pixel != base)
        .min_by_key(|pixel| (chebyshev(base, *pixel), pixel[0], pixel[1]))
}

/// Travel plan with the real engine: `calculate_travel_time` over the
/// `CLIMATE.PAK` weather on foot with the retail popup options (cautious, inn,
/// boat), and `calculate_trip_cost` with the same options.
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

    /// The line shapes are the gate contract's (`scripts/shell-001.py`
    /// §RE_*): exact prefix, one space between fields, no tail.
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

    /// The title carries the full label `label_of` looks up, and the state
    /// decides whether the world runs.
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

    /// The phase 2 shapes are those of the `scripts/shell-002.py` gate, with the
    /// `panel=pause-menu` literal also on the opening from the menu and the
    /// `format=v3` of the slots.
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

    /// `_fixture` — the fidelity row does NOT collide with the anchored
    /// `slot index=` line of `scripts/shell-002.py` (the gate file is
    /// untouchable): same prefix, another shape, and `pos` in integer millimetres.
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
        // The old row stays exactly the same (phase 2 contract).
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

    /// The 11 pages of the configuration census are published with lowercase
    /// id and a distinct control key (the `page`/`key` contract only
    /// accepts `[a-z0-9-]{1,24}`).
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

    /// The creation canon declares 10 steps with distinct ids and the character
    /// token is deterministic (same face → same token).
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

    /// The menu navigates in a circle over the 5 items: `Down` walks
    /// 0→1→2→3→4→0 and `Up` the reverse path 0→4→3→2→1→0 (including the jump
    /// 1→2). The cycles are written as explicit sequences: repeating here
    /// `(s + 1) % 5` would reassert the formula instead of checking the contract.
    ///
    /// The names fix the gate's `menu order=` (`scripts/shell-002.py`).
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

    /// The options state is ONE: the page value survives closing
    /// the panel and the navigation menu → game → pause (the basis of
    /// `check_options_apply`, where the second value must be greater).
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

    /// The destination is the nearest settlement different from the base, with
    /// `(x, y)` tie-break; over uniform weather the travel plan costs real
    /// time (>0) and returns the engine's traversed pixels.
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

    /// Nobody moves the `Paused` clock: two consecutive reads of the same
    /// shell are equal (the basis of `check_pause`).
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

    /// Antibody for the emission order (`PROC-idempotent` of
    /// `scripts/shell-002.py`, CR-6d086222).
    ///
    /// The pause and the save land on the **same frame** —`Escape` and `s` with
    /// no frame between them— and the published sequence must be the
    /// same on every repetition, whatever the split of `dt` across
    /// frames. Before this fix `pause frozen=1` was not enqueued here:
    /// it waited for `FROZEN_AFTER` (1.0 s of wall clock) in `shell_progress`, so
    /// its **position** was a function of the frame and not of the input — with the gate
    /// harness it came out behind `options close=1` at ~0.18 s per stage and between
    /// `options open=1` and `options set` at ~0.55 s per stage.
    #[test]
    fn pause_and_save_in_one_frame_publish_one_sequence_in_every_run() {
        // Three lines, four rhythms: one short frame per line, 250 ms
        // frames, a 1.4 s frame that already fulfils the pause delay and a
        // 3 s frame that fulfils all of them.
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
                // Both keys, the same frame, the same arrival order.
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
                // Full queue: 0.5 s frames publish whatever is left (one
                // line per frame, in enqueue order).
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

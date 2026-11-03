//! **M-SHELL · rutas de sesión sin ventana** (`CR-a602cafc`): el tramo
//! `menú → creación → mundo → guardar → menú` del shell como funciones que
//! **no** dependen de la ventana, para que el gate pueda medirlo.
//!
//! ## Por qué existe este módulo
//!
//! Fase 2 dejó estas transiciones dentro de `m18_window` (los sistemas Bevy que
//! reciben las teclas y los recursos del mundo compuesto). El resultado medido
//! fue `P4 NOT-RUN` y `P9 NOT-RUN` en `scripts/mplay-001.py`: el instrumento
//! conduce el binario real **sin ventana**, así que las dos filas no tenían
//! quien las produjera — la ruta existía y no era medible.
//!
//! Aquí vive la ruta, con el mismo texto de evidencia que la ventana
//! (`M18 VE:`) y las mismas líneas de contrato (`SHELL VE:`), sin ECS: las
//! funciones reciben `&mut Shell` (+ la fase, la escena y el directorio de
//! slots) y devuelven los hechos que el consumidor imprime. `m18_window`
//! **llama** a estas funciones (un solo dueño de la ruta) y el probe de
//! creación las ejercita en un pase headless ([`headless_session_pass`]).
//!
//! ## Fronteras (lo que este módulo NO hace)
//!
//! - **Escena ausente ⇒ 0 world/hud.** [`enter_playing`] recibe
//!   `Option<&SessionScene>`: sin mundo compuesto (el probe headless no lo
//!   tiene) NO se publican `world` ni `hud` — un HUD inventado sería una
//!   captura que afirma un hecho que el producto no produjo.
//! - **Sin rutas en stdout.** [`headless_session_pass`] trabaja en
//!   `temp/dfl-session-pass-<pid>` y borra el directorio al terminar: el
//!   nombre temporal no entra en la evidencia (y el gate normaliza `/tmp`).
//! - **Cero decisión nueva.** Los textos, el token del personaje y el orden de
//!   las líneas son los de `shell`/`shell_slots`; este módulo solo compone.

use std::path::{Path, PathBuf};

use crate::m18_phase::AppPhase;
use crate::shell::{self, HudFacts, Shell, ShellState};
use crate::shell_slots::{self, SaveStore};

/// Superficies del wizard retail que `create done` declara (16).
pub const CREATE_SURFACES: usize = daggerfall_app::m38_surfaces::CREATE_SURFACE_COUNT;

/// Hechos del mundo compuesto que la línea `world` publica.
///
/// Los tres contadores son los del `DemoScene` real (`city.buildings`, `npcs`,
/// `panels`) y la etiqueta es la de `DoorWorld::scene_label`: la misma fuente
/// que usa el título de la ventana, así que la línea no puede divergir de lo
/// que hay en pantalla. La ubicación retail NO viaja aquí: es del [`Shell`]
/// (`shell.location`), que es donde la midió el `Startup`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionScene {
    /// Etiqueta de la escena compuesta (`Exterior`, `Interior(<block>)`).
    pub label: String,
    /// Edificios compuestos del asentamiento.
    pub buildings: usize,
    /// NPCs del censo compuesto.
    pub npcs: usize,
    /// Paneles de la capa 2D compuesta.
    pub panels: usize,
}

// ── creación de personaje (wizard del canon) ────────────────────────────────

/// `Right`/`Left` sobre el wizard: cara siguiente/anterior del anillo
/// (`shell::CREATE_FACES`) y repintado del paso en curso.
///
/// Devuelve la cara resultante. Fuera de `Create` no hace nada y devuelve la
/// cara guardada en el shell: la tecla no inventa un estado.
pub fn create_face(shell: &mut Shell, right: bool) -> usize {
    let ShellState::Create { step, face } = shell.state else {
        return shell.face;
    };
    let next_face = if right {
        (face + 1) % shell::CREATE_FACES
    } else {
        (face + shell::CREATE_FACES - 1) % shell::CREATE_FACES
    };
    shell.state = ShellState::Create {
        step,
        face: next_face,
    };
    shell.face = next_face;
    // Un cambio de cara repinta el paso en curso: la línea es consecuencia
    // causal de la tecla, no evidencia de entrada.
    shell.say(
        0.0,
        shell::create_step_line(step, shell::CREATE_STEPS, shell::CREATE_FIELDS[step - 1]),
    );
    next_face
}

/// `Down` sobre el wizard: paso siguiente del canon (se detiene en el último).
///
/// Devuelve el paso en curso. Fuera de `Create` devuelve 0 y no toca nada.
pub fn create_next_step(shell: &mut Shell) -> usize {
    let ShellState::Create { step, face } = shell.state else {
        return 0;
    };
    let next = if step < shell::CREATE_STEPS {
        step + 1
    } else {
        shell::CREATE_STEPS
    };
    shell.state = ShellState::Create { step: next, face };
    shell.face = face;
    shell.say(
        0.0,
        shell::create_step_line(next, shell::CREATE_STEPS, shell::CREATE_FIELDS[next - 1]),
    );
    next
}

/// Abre el wizard en el paso 1 con la cara 0 (el `Return` sobre `new-game`).
pub fn create_begin(shell: &mut Shell) {
    shell.state = ShellState::Create { step: 1, face: 0 };
    shell.face = 0;
    shell.panel = shell::ShellPanel::None;
    shell.dwell = 0.0;
    shell.say(
        0.0,
        shell::create_state_line(
            CREATE_SURFACES,
            daggerfall_app::m38_surfaces::CREATE_HOST,
            0,
        ),
    );
}

/// `Return` sobre el wizard: confirma el personaje y entra en el mundo.
///
/// Publica `create done`, la línea `session` de la primera sesión (sin slots en
/// el perfil) y delega en [`enter_playing`] con `new_game = true`, así que
/// frase y estado no pueden divergir. Devuelve el token del personaje; `None`
/// si la tecla llega fuera de `Create` (nada que confirmar).
///
/// `slots_present` es el número de slots ocupados del perfil: lo cuenta el
/// consumidor (`shell_slots::scan`), no este módulo — el directorio es del
/// proceso y aquí no se adivina.
pub fn confirm_create(
    shell: &mut Shell,
    phase: &mut AppPhase,
    scene: Option<&SessionScene>,
    slots_present: usize,
) -> Option<String> {
    let ShellState::Create { face, .. } = shell.state else {
        return None;
    };
    let character = shell::character_token(face);
    shell.character = Some(character.clone());
    shell.face = face;
    shell.panel = shell::ShellPanel::None;
    // M-QST: una partida nueva es una sesión nueva — la quest vuelve a
    // `Unstarted` para que el disparador de `enter_playing` pueda ofrecerla.
    // (Cargar/continuar NO pasa por aquí: rehidrata del documento.)
    shell.mainquest = crate::mainquest_route::MainQuest::new();
    // M-NEW (CR-178183d7): una partida nueva empieza de cero. El reloj de
    // sesión vuelve al arranque (`SESSION_CLOCK_SEED`; solo lo mueven el viaje
    // rápido y la carga), la pausa no arrastra minutos y el slot cargado deja
    // de estar atado a la sesión: si se sigue jugando, el siguiente guardado
    // elige el primer slot libre en vez de reescribir el que se había cargado.
    // `continue`/`load` NO pasan por aquí — [`apply_load`] es la única ruta que
    // conserva, y esa es la diferencia entre las dos entradas del menú.
    shell.clock_minutes = shell::SESSION_CLOCK_SEED;
    shell.paused_ticks = shell::SESSION_CLOCK_SEED;
    shell.session_slot = None;
    shell.say(
        0.0,
        shell::create_done_line(CREATE_SURFACES, &character),
    );
    let first_session = shell.session_slot.is_none();
    if first_session && slots_present == 0 {
        shell.say(0.0, shell::session_line(&character, None));
    }
    enter_playing(shell, phase, scene, true);
    Some(character)
}

// ── entrada al mundo ────────────────────────────────────────────────────────

/// Entra en `Playing` (creación, `continue` o carga): fase, mundo y HUD de una
/// sola vez, para que frase y estado no puedan divergir.
///
/// **Sin escena NO se publican `world` ni `hud`** — el consumidor headless no
/// tiene mundo compuesto y el HUD es un hecho medido, no un relleno. La línea
/// `new_game` sí se publica: la entrada al mundo es un hecho del shell.
///
/// La línea de fase incluye `phase=<LABEL>` (`phase=PLAYING`) y se imprime
/// **después** de drenar la cola: la evidencia del estado que la causó
/// (`create done`, `session`) queda antes de la frase, y el regreso al menú
/// (`back_to_menu`) siempre publica `menu state=Menu` después de esa frase.
pub fn enter_playing(
    shell: &mut Shell,
    phase: &mut AppPhase,
    scene: Option<&SessionScene>,
    new_game: bool,
) {
    let next = crate::m18_phase::phase_after_enter(*phase);
    if next != *phase {
        shell.drain();
        println!(
            "M18 VE: phase {} → {} (phase={}) — world systems live, the clock starts at the \
             menu's 0 minutes",
            phase.label(),
            next.label(),
            next.label()
        );
        *phase = next;
    }
    shell.state = ShellState::Playing;
    shell.panel = shell::ShellPanel::None;
    shell.map_open = false;
    shell.dwell = 0.0;
    let ticks = shell.clock_minutes;
    if new_game {
        shell.say(0.0, shell::new_game_line(ticks));
    }
    // M-QST: el disparador entra DESPUÉS de `state = Playing` y ANTES del
    // early-return de la escena ausente — con `scene = None` el pase headless
    // (y por tanto la medida) también ve la oferta. `offer` solo emite desde
    // `Unstarted`, así que `continue`/carga no re-ofrecen lo ya decidido.
    crate::mainquest_route::offer(shell);
    let Some(scene) = scene else {
        return;
    };
    if let Some(location) = shell.location {
        let line = shell::world_line(
            &scene.label,
            location,
            scene.buildings,
            scene.npcs,
            scene.panels,
        );
        shell.say(0.0, line);
    }
    let hud = shell.hud.clone();
    shell.say(0.0, shell::hud_line(true, &hud));
}

// ── slots: abrir el browser y escribir ──────────────────────────────────────

/// Abre el browser de guardado sobre el store ya barrido por el consumidor.
///
/// Devuelve los slots ocupados (el `slots.len()` que la ventana publica en su
/// línea `M18 VE:`), de modo que el llamante no recalcula el censo.
pub fn open_save(shell: &mut Shell, store: SaveStore) -> usize {
    let occupied = store.count();
    shell.slots = store.slots;
    shell.slots_rejected = store.rejected;
    shell.browser_slot = 0;
    shell.panel = shell::ShellPanel::Save;
    shell.say(
        0.0,
        shell::save_open_line(shell_slots::SLOT_COUNT, shell_slots::FORMAT),
    );
    occupied
}

/// Escribe el slot elegido (el primero libre, o el del cursor si no queda
/// ninguno) con el reloj de sesión real y refresca el censo del browser.
///
/// `dir` es el directorio de slots que resuelve el consumidor
/// (`shell_slots::save_dir()`): aquí no se adivina un directorio implícito.
/// Devuelve `(slot, bytes, digest)` del documento v3 escrito.
pub fn write_slot(shell: &mut Shell, dir: &Path) -> Result<(usize, usize, u64), String> {
    let Some(character) = shell.character.clone() else {
        return Err("no session character: nothing to write".to_string());
    };
    let occupied: Vec<usize> = shell.slots.iter().map(|slot| slot.index).collect();
    let index = (0..shell_slots::SLOT_COUNT)
        .find(|at| !occupied.contains(at))
        .unwrap_or(shell.browser_slot);
    // `CR-7a3e9c14`: si la sesión midió mundo, el slot lo declara (generación
    // 4, misma gramática v3); si no lo midió, el slot sigue siendo el de
    // generación 3 y la evidencia dice `fidelity=3` con su razón — la puerta
    // no inventa posición ni reloj de mundo.
    let written = match shell.snapshot {
        Some(world) => shell_slots::write_in_with_world(
            dir,
            index,
            &character,
            shell.face,
            shell.clock_minutes,
            shell.mainquest.state(),
            &world,
        ),
        None => shell_slots::write_in(
            dir,
            index,
            &character,
            shell.face,
            shell.clock_minutes,
            shell.mainquest.state(),
        ),
    };
    match written {
        Ok((bytes, digest)) => {
            shell.session_slot = Some(index);
            match shell.snapshot {
                Some(world) => shell.say(0.0, shell::slot_fidelity_line(index, &world)),
                None => shell.say(0.0, shell::slot_fidelity_absent_line(index)),
            }
            shell.say(
                0.0,
                shell::save_line(index, shell_slots::FORMAT, bytes, digest),
            );
            let store = shell_slots::scan_in(dir);
            shell.slots = store.slots;
            shell.slots_rejected = store.rejected;
            Ok((index, bytes, digest))
        }
        Err(err) => Err(format!("slot {index} not written: {err}")),
    }
}

// ── regreso al menú ─────────────────────────────────────────────────────────

/// `b` en la pausa: la sesión vuelve al menú con la selección en 0 (el ítem
/// `new-game`), sin redeclarar el conjunto de ítems (ya se declaró al entrar).
///
/// Publica `session state=Menu from=Playing` y el estado del menú: la frase de
/// fase que [`enter_playing`] imprimió queda **antes** de estas dos líneas por
/// construcción (la cola solo avanza hacia adelante), así que la secuencia
/// `PLAYING → menu` es la misma que la ventana muestra.
pub fn back_to_menu(shell: &mut Shell) {
    shell.state = ShellState::Menu {
        selection: shell::MENU_NEW_GAME,
    };
    shell.panel = shell::ShellPanel::None;
    shell.map_open = false;
    shell.dwell = 0.0;
    shell.say(0.0, shell::session_menu_line());
    shell.say(
        0.0,
        shell::menu_line(shell::MENU_ITEMS.len(), shell::MENU_NEW_GAME),
    );
}

// ── pase headless del recorrido de sesión ───────────────────────────────────

/// Directorio del pase: `temp/dfl-session-pass-<pid>` (borrado al terminar).
fn pass_dir() -> PathBuf {
    std::env::temp_dir().join(format!("dfl-session-pass-{}", std::process::id()))
}

/// Recorre `menú → creación → PLAYING → guardar → menú` sin ventana y publica
/// la **misma** evidencia que la ventana produce con las mismas teclas.
///
/// Es el pase que hace medible `P4` (`create done … state=Playing`) y `P9`
/// (`phase=PLAYING` antes de `menu state=Menu`) en un binario sin servidor
/// gráfico: el probe de creación lo llama al final de su medición y el gate
/// `scripts/mplay-001.py` lee las líneas de stdout.
///
/// Honestidad: el pase **no** inventa escena (0 `world`/`hud` — ver
/// [`enter_playing`]) ni escribe rutas; el directorio de slots es temporal y se
/// borra al final incluso si un paso falla.
pub fn headless_session_pass() -> Result<(), String> {
    let dir = pass_dir();
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .map_err(|err| format!("pass dir {}: {err}", dir.display()))?;
    }
    std::fs::create_dir_all(&dir)
        .map_err(|err| format!("pass dir {}: {err}", dir.display()))?;
    let result = session_pass(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// Cuerpo del pase (separado para que el `remove_dir_all` sea incondicional).
fn session_pass(dir: &Path) -> Result<(), String> {
    // Sin hechos del mundo: el shell headless no compone escena. Los campos que
    // NO se publican (hud/ubicación/asentamientos) quedan declarados como
    // ausentes, nunca rellenados con ceros que parezcan medidos.
    let mut shell = Shell::new(
        Vec::new(),
        HudFacts {
            panel: crate::m18_phase::HUD_PANEL_ID.to_string(),
            opaque: 0,
            digest: 0,
        },
        None,
        [0, 0],
        Vec::new(),
        None,
    );
    let mut phase = AppPhase::Menu;
    shell.state = ShellState::Menu {
        selection: shell::MENU_NEW_GAME,
    };
    // `Return` sobre `new-game`: abre el wizard.
    create_begin(&mut shell);
    // El canon avanza (el driver del gate confirma en el paso 2).
    create_next_step(&mut shell);
    let slots_present = shell_slots::scan_in(dir).count();
    let character = confirm_create(&mut shell, &mut phase, None, slots_present)
        .ok_or_else(|| "create: the wizard was not open".to_string())?;
    if shell.state != ShellState::Playing {
        return Err(format!("create: `{character}` did not enter Playing"));
    }
    // `Escape` (pausa) → `s` (browser) → `Return` (escribe el slot v3).
    shell.state = ShellState::Paused;
    shell.dwell = 0.0;
    shell.paused_ticks = shell.clock_minutes;
    open_save(&mut shell, shell_slots::scan_in(dir));
    let (slot, _bytes, _digest) = write_slot(&mut shell, dir)?;
    if shell.session_slot != Some(slot) {
        return Err(format!("save: slot {slot} not recorded in the session"));
    }
    // `b`: la sesión vuelve al menú.
    back_to_menu(&mut shell);
    shell.drain();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell() -> Shell {
        Shell::new(
            Vec::new(),
            HudFacts {
                panel: crate::m18_phase::HUD_PANEL_ID.to_string(),
                opaque: 0,
                digest: 0,
            },
            None,
            [0, 0],
            Vec::new(),
            None,
        )
    }

    #[test]
    fn drain_publishes_every_ready_line_and_stops_at_a_live_delay() {
        let mut shell = shell();
        shell.say(0.0, "SHELL VE: a".to_string());
        shell.say(0.0, "SHELL VE: b".to_string());
        shell.say(5.0, "SHELL VE: late".to_string());
        shell.say(0.0, "SHELL VE: blocked-behind-late".to_string());
        assert_eq!(shell.drain(), 2);
        assert_eq!(shell.drain(), 0, "the live delay is still live");
        // Una sola línea por `pump`: el orden estricto no cambia.
        shell.pump(5.0);
        assert_eq!(shell.drain(), 1, "the late line became ready");
    }

    #[test]
    fn create_route_advances_the_canon_and_confirms_the_token() {
        let mut shell = shell();
        let mut phase = AppPhase::Menu;
        create_begin(&mut shell);
        assert_eq!(
            shell.state,
            ShellState::Create { step: 1, face: 0 },
            "the wizard opens at step 1 with face 0"
        );
        assert_eq!(create_face(&mut shell, true), 1);
        assert_eq!(create_next_step(&mut shell), 2);
        assert_eq!(
            shell.state,
            ShellState::Create { step: 2, face: 1 },
            "the canon step and the face are independent"
        );
        let character = confirm_create(&mut shell, &mut phase, None, 0)
            .expect("the wizard was open");
        assert_eq!(character, shell::character_token(1));
        assert_eq!(phase, AppPhase::Playing);
        assert_eq!(shell.state, ShellState::Playing);
        assert_eq!(shell.character.as_deref(), Some(character.as_str()));
    }

    #[test]
    fn enter_playing_without_a_scene_publishes_no_world_or_hud() {
        let mut shell = shell();
        let mut phase = AppPhase::Menu;
        enter_playing(&mut shell, &mut phase, None, true);
        assert_eq!(phase, AppPhase::Playing);
        assert_eq!(shell.state, ShellState::Playing);
        // Solo `new_game` + la oferta de la quest (M-QST): ningún `world` ni
        // `hud` inventado. La oferta sale de aquí precisamente porque el
        // early-return de la escena ausente va DESPUÉS del disparador.
        assert_eq!(shell.mainquest.state(), daggerfall_behavior::mainquest::State::Offered);
        assert_eq!(shell.drain(), 2);
    }

    #[test]
    fn enter_playing_with_a_scene_publishes_world_and_hud() {
        let mut shell = shell();
        let mut phase = AppPhase::Menu;
        let scene = SessionScene {
            label: "Exterior".to_string(),
            buildings: 2,
            npcs: 3,
            panels: 4,
        };
        enter_playing(&mut shell, &mut phase, Some(&scene), false);
        // `world` exige ubicación retail: sin ella se publica solo el HUD,
        // más la oferta de la quest de la primera entrada (M-QST).
        assert_eq!(shell.drain(), 2);
        shell.location = Some((17, 50026, 0));
        let mut phase = AppPhase::Playing;
        enter_playing(&mut shell, &mut phase, Some(&scene), false);
        // La oferta no se repite (el estado ya no es `Unstarted`).
        assert_eq!(shell.mainquest.offers(), 1);
        assert_eq!(shell.drain(), 2, "world + hud");
    }

    #[test]
    fn back_to_menu_returns_to_new_game_and_publishes_the_two_lines() {
        let mut shell = shell();
        shell.state = ShellState::Paused;
        back_to_menu(&mut shell);
        assert_eq!(
            shell.state,
            ShellState::Menu {
                selection: shell::MENU_NEW_GAME
            }
        );
        assert_eq!(shell.drain(), 2, "session state=Menu + menu state=Menu");
    }

    #[test]
    fn the_headless_pass_runs_and_leaves_no_directory() {
        let dir = pass_dir();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(headless_session_pass().is_ok());
        assert!(!dir.exists(), "the pass removes its own directory");
    }
}

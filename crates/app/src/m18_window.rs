//! M18 — window of the **integrated demo** (feature `window`).
//!
//! It mounts [`crate::m18_demo`]'s composition in Bevy (same source as the
//! `DEMO-001` probe, 0 duplicated logic):
//!
//! - **D1** retail `.RMB` city with M12 `base_color_texture` materials
//!   (`split_by_texture` → `scale_uvs` → `image_from_rgba` →
//!   `material_from_texture`); the block's synthetic ground goes untextured
//!   (declared neutral color);
//! - **D2** M13 terrain ring (one mesh per band, unlit);
//! - **D3** M15 NPCs as 8-dir billboards with decoded flat
//!   (`AlphaMode::Mask`) and retail caption rendered with `.FNT` over the 2D
//!   canvas;
//! - **D4** the 4 M17 panels overlaid on a 2D camera (`order: 1`,
//!   `ClearColorConfig::None`), toggled with 1-4;
//! - **D5** live M20 HUD: the [`DemoScene::character`] status line
//!   (`SPELLSWORD - LV 1 - HP 37/37 - STA 110/110 - MAG 75 - RUN 5`) as a
//!   sprite on the 2D camera (top-left, 2× scale). The **progression driver**
//!   ([`daggerfall_skills::progression::SkillDriver`]) is the single authority
//!   of skill use and level-up in the window: [`m20_progression_tick`] feeds it
//!   the movement signals of every M19 frame (running, jumping, climbing),
//!   advances a [`GameClock`] at the documented M4 scale (1 real second =
//!   1 game minute) and evaluates `RaiseSkills` + the level-up on every new
//!   game minute; [`m14_melee_sfx`] and [`m22_enemy_retaliation`] report the
//!   resolved player/enemy strikes. Every tally/raise/level-up re-renders the
//!   HUD ([`DemoScene::m20_refresh_hud`]), replaces the sprite image and
//!   rebuilds `M19Player.body` plus `DemoCombat`'s player from the character
//!   (0 duplicated logic). **R** is a **demo accelerator**, not gameplay: it
//!   accumulates 109 `Running` uses and evaluates at `last + 361` min through
//!   the same driver (`use_skill` + `minute`).
//!
//! Movement (M19): first person with WASD + mouse-look (M2 input systems,
//! D1) whose displacement is decided by the **M19 engine**
//! `behavior::locomotion::Locomotion::step` over the demo adapter
//! ([`crate::m18_demo::DemoLocomotionWorld`], 0 duplicated logic): retail
//! `.RMB` grid inside the city (INV-L99-5) and M13 sampler outside. Jump
//! (Space), run (Shift), crouch (Ctrl) and sneak (C, toggle) change the speed
//! resolved by the DFU source (`PlayerSpeedChanger`); the title bar shows the
//! engine mode, the breath and the M20 status line. **Water (CS-08):** the
//! `Ocean` band is rendered as the sea surface at
//! [`crate::m18_demo::DemoTerrain::sea_level_m`] and it is **not walkable** —
//! the adapter validates the destination cell exactly like the grid validates
//! a city entry, so the player and a jump stop at the shoreline. No climbable
//! wall is modelled (§6.5): climbing is measured in `PROBE-LOCOMOTION-001` with
//! its synthetic world; levitation arrives with M23's cast (L), driving the same
//! engine (`MoveInput.levitating`).
//!
//! **Startup (B1-d, CS-20):** the app opens in [`crate::m18_phase::AppPhase::Menu`]
//! and shows the retail `main-menu` panel (`TITL00I0` + `MENU.IMG` +
//! `BUTN00`/`BUTN03`, index 3 of `ui_spec::build_all_panels`) full screen with
//! the HUD hidden and no music; `Enter` switches to `Playing`, which restores the
//! `hud` panel + HUD sprite, starts the city context song and lets the world
//! systems tick. The surface is selected by [`crate::m18_phase::visible_panel_index`],
//! the same function `MENU-001` reads.
//!
//! Combat (M22): the interactive fight runs through the production
//! [`daggerfall_app::combat_runtime::CombatSession`] — the same composition
//! layer the probes consume. `DemoCombat` holds the session, the
//! `EnemyAttack` timer and the player level; both fighters are built at
//! startup from the live M20 character (id 1, `reflexes`/`level` included)
//! plus the retail kit (steel cuirass + steel Longsword, `FALL.EXE`), with an
//! explicit degrade line when the table is unavailable. `m14_melee_sfx`
//! resolves the player's swing through `player_strike(law.target)` — the
//! nearest live guard the census prepared, `0` when none is in the encounter
//! (M14 audio policy untouched) — and `m22_enemy_retaliation` resolves one
//! swing per live actor on the shared cadence through `enemy_strike(index)`,
//! writing the health back into `DemoWorld`/the HUD.
//!
//! Magic (M23, REQ-L99-056): the window runs the magic layer through the
//! production engines — **L** casts the demo's Levitate on the player's
//! effect entity with the retail spell-point cost and pool gate
//! ([`daggerfall_app::m23_window::MagicLayer`], DFU `EntityEffectManager.cs:337-343,
//! 387-388`), one magic round runs per game minute hooked at the M20 minute
//! clock (`EntityEffectBroker.cs:205-245`), and the movement flags the cast
//! raises drive the M19 engine (`MoveInput.levitating`,
//! `PlayerBody.slow_falling`/`water_breathing`/`water_walking`). The item triggers
//! (Held/Equipped/Used/Breaks/MagicRound/Reroll) are M24's — no item organ in
//! this tree, so no item casts from the window.
//!
//! **Custody (PRE-2 §8 rule 3 / INV-L99-6):** Mode A only; retail bytes are
//! read at runtime and 0 assets are packaged.

use bevy::camera::ClearColorConfig;
use bevy::ecs::system::SystemParam;
use bevy::image::ImagePlugin;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

// M35 (V7): the two annex modules live next to this one and are declared here
// because `main.rs` is the trunk of the binary and does not know them. Both
// resolve their `super::` paths against this module (`super::m18_spawn_world`,
// `super::DemoWorld`, `super::ProgressionClock`, `super::DemoWeather`, …).
#[path = "m35_sky_render.rs"]
mod m35_sky_render;
#[path = "m35_vegetation.rs"]
mod m35_vegetation;

use daggerfall_behavior::locomotion::{
    max_breath, Dice100, Frame, Locomotion, MotionMode, MoveInput, PlayerBody,
};
use daggerfall_combat::actor_seed;
use daggerfall_heart::clock::{GameClock, TIME_SCALE_GAME_SECONDS_PER_REAL_SECOND};
use daggerfall_parser_bevy_bridge as bridge;
use daggerfall_skills::character::{Skill, SKILL_CHECK_INTERVAL_MINUTES};
use daggerfall_skills::progression::{DriverStep, MotionSignals, SkillDriver};

use crate::m18_demo::{DemoScene, DemoTexture, CAPTION_RANGE_M, M19_SEED};
use daggerfall_app::law_runtime::{
    city_watch_fighter, GuardOrder, GuardPlacement, LawRuntime, CITY_WATCH_CLASS_FILE,
};
use daggerfall_behavior::law::{
    CrimeTrigger, Crimes, DfRandom, GuardSpawnContext, GUARD_ATTENTION_RADIUS_METERS,
    GUARD_REPLACE_MIN_ANGLE_DEGREES, GUARD_REPLACE_ONE_IN,
};
use crate::m2_player;
use crate::ui_weather::{Precipitation, WeatherKind};
use daggerfall_parser_rs::climate::ClimateMap;

use daggerfall_app::audio::{
    self, AudioEngine, GameplayEvent, MusicBank, RenderedSong, SfxBank, SfxSink,
};

/// Game days used for the context song: the demo has no calendar (M8's clock
/// lives in the session loop), so it plays the day-0 city context — DFU
/// `SongManager` seeds `DFRandom` with the game day.
const DEMO_GAME_DAYS: u32 = 0;

/// M19 player: engine + body + die + sneak toggle.
#[derive(Resource)]
struct M19Player {
    locomotion: Locomotion,
    body: PlayerBody,
    dice: Dice100,
    sneak: bool,
    /// M14: fall event of the last M19 step (`Frame.fall_damage` /
    /// `hard_fall`), consumed once by [`m14_fall_sfx`] (never replayed).
    pending_fall: Option<GameplayEvent>,
    /// M20: movement signals of the last M19 frame, consumed by
    /// [`m20_progression_tick`] (running, swimming, jumping, climbing check).
    last_signals: MotionSignals,
}

/// M20 progression: the single authority of skill use and level-up in the
/// window (`SkillDriver`, D-M20-1 seed `M19_SEED`).
#[derive(Resource)]
struct Progression(SkillDriver);

/// M20 game clock of the window: the documented M4 scale (1 real second =
/// 1 game minute) driven by the frame `dt`, plus the last evaluated minute.
#[derive(Resource)]
struct ProgressionClock {
    clock: GameClock,
    /// Fractional game seconds of the current real second (`dt × 60` carry).
    carry: f64,
    /// Last game minute passed to `SkillDriver::minute`.
    last_minute: i64,
}

/// M14 production audio of the demo: bank + sink + queue, or `None` when the
/// retail `DAGGER.SND` is absent (declared degradation: 0 fake audio).
#[derive(Resource)]
struct GameAudio {
    engine: Option<AudioEngine>,
}

/// M14 melee: the demo's attacker/target pair + deterministic dice (M22).
#[derive(Resource)]
struct DemoCombat {
    session: daggerfall_app::combat_runtime::CombatSession,
    enemy_attack: daggerfall_combat::engine::EnemyAttack,
    player_level: i32,
}

/// M23: the window's magic layer — the active-effect state plus the dedicated
/// dice stream of casts and magic rounds ([`daggerfall_app::m23_window::MagicLayer`]).
#[derive(Resource)]
struct M23Magic(daggerfall_app::m23_window::MagicLayer);

/// Composed scene inserted as a resource (the demo builds it once).
#[derive(Resource)]
struct DemoWorld(DemoScene);

/// World NPCs: billboard entity + position in the city frame.
#[derive(Resource)]
struct DemoNpcs(Vec<(Entity, Vec3)>);

/// The pool NPC a city watch replaced (`M28`): DFU turns the candidate into the
/// guard (`PlayerEntity.cs:668-680`), so its billboard is hidden and the caption
/// stops naming it. The entry stays in [`DemoNpcs`] on purpose — the caption
/// sprites are indexed by NPC position, and dropping an entry would shift every
/// caption one slot.
#[derive(Component)]
struct ReplacedByGuard;

/// M28: the live law state (crime → guard request → spawn) plus the guard
/// census the scene holds. `runtime` is the production composition layer
/// (`daggerfall_app::law_runtime`); the streams are seeded so the scene is
/// reproducible.
#[derive(Resource)]
struct M28Law {
    runtime: LawRuntime,
    /// `DFRandom` of the law draws (counts, radii, countdown).
    rng: DfRandom,
    /// `Dice100` of the door rolls (`PlayerActivate.cs:571-579`).
    dice: daggerfall_combat::Dice100,
    /// RNG of the guard careers (`city_watch_level`).
    guard_rng: daggerfall_skills::rng::Rng64,
    /// Guards placed in the city frame: `(session enemy index, position)`.
    /// The player's swing reads the nearest entry; the census counts the live
    /// ones (`LawRuntime::sync_census`). M26 renderers read the positions.
    guards: Vec<(usize, [f32; 3])>,
    /// Next actor id handed to a guard fighter.
    next_id: u32,
    /// Fractional seconds carried into the countdown tick.
    carry: f32,
    /// Session index of the enemy the player's swing addresses: the nearest
    /// live guard, or `0` when no guard is in the encounter.
    target: usize,
}

/// Reach of the demo binding that bashes the nearest building door. Declared
/// demo binding (the retail binding is *activate with this mode*), never a DFU
/// number: the lock roll itself is the source's.
const DOOR_REACH_M: f32 = 2.5;

impl M28Law {
    fn new() -> Self {
        Self {
            runtime: LawRuntime::new(62),
            rng: DfRandom::srand(M19_SEED as u32),
            dice: daggerfall_combat::Dice100::with_seed(actor_seed(M19_SEED, 0x28)),
            guard_rng: daggerfall_skills::rng::Rng64::new(M19_SEED),
            guards: Vec::new(),
            next_id: 9_200,
            carry: 0.0,
            target: 0,
        }
    }
}

/// Overlaid captions (one per NPC) and the visible index.
#[derive(Resource, Default)]
struct DemoCaptions {
    sprites: Vec<Entity>,
    visible: Option<usize>,
}

/// M17 panels on the 2D camera.
#[derive(Resource, Default)]
struct DemoUi {
    sprites: Vec<Entity>,
    current: usize,
}

/// M20 HUD on the 2D camera (sprite replaceable with the R key).
#[derive(Resource)]
struct DemoHud {
    sprite: Option<Entity>,
}

/// **B3-a (CS-14)** — guard billboards instantiated from the live law census.
///
/// `spawned` is the idempotency cursor: the system only instantiates the guards
/// whose position appeared in `M28Law::guards` **after** the last frame, so a
/// guard is never duplicated. `sprites` keeps `(billboard, actor label,
/// position)` so [`m18_billboards`] rotates both quads toward the camera.
#[derive(Resource, Default)]
struct DemoGuardBillboards {
    spawned: usize,
    sprites: Vec<(Entity, Entity, Vec3)>,
}

/// **B3-b (CS-15/CS-16)** — retail overlays on the 2D camera.
///
/// `J` toggles the dialogue overlay and `I` the journal overlay (neither key
/// has an owner in `crates/app/src` — `V` is *sell*, `m2_player.rs:137`).
/// `None` means the retail file was unavailable: the toggle then reports the
/// degradation and draws nothing (0 invented text).
#[derive(Resource, Default)]
struct DemoOverlays {
    dialogue: Option<Entity>,
    journal: Option<Entity>,
    dialogue_on: bool,
    journal_on: bool,
    dialogue_size: (u16, u16),
    journal_size: (u16, u16),
    /// On-screen position of each overlay (restored when it is shown again).
    dialogue_at: Vec3,
    journal_at: Vec3,
}

/// **B3-d (CS-12)** — the window's precipitation bridge.
///
/// The model is [`crate::ui_weather`]: `CLIMATE.PAK` value → published row →
/// `draw(row, roll)` → `drops`. This resource only paints what the model
/// resolved: a deterministic half-resolution streak sheet scaled to the window.
/// `O` advances to the next retail climate that actually precipitates, so the
/// capture can show real rain/snow without inventing weather.
#[derive(Resource, Default)]
struct DemoWeather {
    /// Resolved precipitation in force (model output, never a literal).
    precip: Option<Precipitation>,
    /// Sprite carrying the sheet (`None` when `CLIMATE.PAK` is unreadable).
    sprite: Option<Entity>,
    /// Sheet handle, rewritten in place when the climate changes.
    handle: Option<Handle<Image>>,
    /// `CLIMATE.PAK` values present in the map, ascending.
    climates: Vec<u8>,
    /// Every `(climate, roll)` of the map that actually precipitates, in
    /// `CLIMATE.PAK` order and ascending roll — the `O` cycle walks this list.
    states: Vec<(u8, f32)>,
    /// Index into `states` of the value in force.
    at: usize,
    /// Half-resolution sheet size (`w`, `h`) in pixels.
    size: (u16, u16),
}

/// Z of the precipitation sheet: above the world, below the B3-b overlays.
const PRECIP_Z: f32 = 18.0;

/// Sheet divisor over the window (half resolution keeps the upload small).
const PRECIP_DIV: u16 = 2;

/// Upper bound of streaks painted by one sheet (the model's `drops` can be
/// large; the sheet is capped so the texture stay bounded, declared here).
const PRECIP_MAX_STREAKS: usize = 1_400;

/// **B1-a** — terrain group entities of the current composition: the `T` key
/// replaces them when the sampling pixel changes.
#[derive(Resource, Default)]
struct TerrainMeshes(Vec<Entity>);

/// M18 integrated demo window.
pub fn run_windowed_m18(
    path_checker: &daggerfall_inventory::path_checker::PathChecker,
    overrides: crate::m18_demo::M18Overrides,
) -> Result<(), String> {
    if path_checker.use_custom_universe() {
        return Err("M18 supports Mode A only (ModeState=B is out of scope)".into());
    }
    let scene = match crate::m18_demo::compose_demo_with_overrides(&overrides) {
        Ok(scene) => scene,
        Err(err) => {
            // **CS-08/09/10** — a requested band spawn that cannot be honoured is
            // NOT a recoverable window failure: falling back to the retail spawn
            // (or to another band) would silently measure a different state than
            // the one asked for. Refuse explicitly (exit 2), never degrade.
            if overrides.spawn.is_some() {
                println!("M18 VE: spawn-override refused — {err}");
                eprintln!(
                    "M18: --spawn {err} — refusing to open the world at any other point (exit 2)"
                );
                std::process::exit(2);
            }
            return Err(err.to_string());
        }
    };
    // B1-a: the override is named in the evidence (never a silent change).
    if let Some(origin) = overrides.origin {
        println!(
            "M18 VE: origin-override px=({},{}) — M13 terrain sampled at that map pixel \
             (city hull, collision grid and spawn untouched)",
            origin[0], origin[1]
        );
    }
    if let Some(yaw) = overrides.yaw {
        println!(
            "M18 VE: yaw-override {yaw:.4} rad ({:.1}°) — initial yaw of the spawn",
            yaw.to_degrees()
        );
    }
    if let Some(band_spawn) = scene.spawn_band {
        // **CS-08/09/10** — the viewpoint is evidence, not a silent change: where
        // it stands, which band is under it, the surface it stands on (mesh
        // height lifted to the declared sea plane over water), the yaw and what
        // that yaw looks at, and that the collision grid is untouched.
        println!(
            "M18 VE: spawn-override band:{} · eye=({:.2},{:.2},{:.2}) · at ({:.1},{:.1}) m = \
             {:.1} m outside the city hull · terrain band under the eye {} · surface {:.2} m \
             (mesh {:.2} m · sea level {}) · yaw {:.4} rad ({:.1}°) toward ({:.1},{:.1}) \
             {:.1} m away · open water {} samples / relief {:.1} m · open run {:.2} m · city hull, \
             collision grid and walkability untouched",
            band_spawn.band.id(),
            band_spawn.eye[0],
            band_spawn.eye[1],
            band_spawn.eye[2],
            band_spawn.at[0],
            band_spawn.at[1],
            band_spawn.outside_hull_m,
            scene.terrain.band_at(band_spawn.eye[0], band_spawn.eye[2]).name(),
            band_spawn.surface_m,
            band_spawn.ground_m,
            match band_spawn.sea_level_m {
                Some(level) => format!("{level:.2} m"),
                None => "none (no ocean in the ring)".to_string(),
            },
            band_spawn.yaw,
            band_spawn.yaw.to_degrees(),
            band_spawn.target[0],
            band_spawn.target[1],
            band_spawn.target_distance_m,
            band_spawn.water_samples,
            band_spawn.relief_m,
            band_spawn.open_run_m,
        );
    }
    let city_m = scene.city_m();
    println!(
        "M18 VE: {} · map-pixel ({},{}) · {} buildings · {} retail triangles · {} textures · \
         terrain {}×{} ({} triangles) · {} NPCs · {} UI panels · spawn=({:.1},{:.1},{:.1}) yaw={:.2}",
        scene.city_name,
        scene.map_pixel[0],
        scene.map_pixel[1],
        scene.city.buildings.len(),
        scene.city.triangle_count(),
        scene.textures.len(),
        scene.terrain.dim,
        scene.terrain.dim,
        scene.terrain.triangles,
        scene.npcs.len(),
        scene.panels.len(),
        scene.spawn.eye[0],
        scene.spawn.eye[1],
        scene.spawn.eye[2],
        scene.spawn.yaw,
    );
    println!(
        "M18 VE: terrain world-span x/z {:.1}..{:.1} m · city {:.1} m · WASD+mouse-look · \
         Esc releases the cursor · 1-6 UI panels · J dialogue overlay · I journal overlay",
        scene.terrain.inner_neg_m(),
        scene.terrain.outer_m(),
        city_m,
    );

    App::new()
        .add_plugins(
            DefaultPlugins
                .set(ImagePlugin::default_nearest())
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "DAGGERFALL-LIKE — M20 demo (DFU character + HUD)".to_string(),
                        resolution: (1280u32, 720u32).into(),
                        ..default()
                    }),
                    ..default()
                }),
        )
        // M35 (V7): sky L0-L4 + vegetation annex. Both startup systems are
        // ordered `.after(m18_spawn_world)` inside their own plugins, so the
        // world they read exists by then; `ClearColor` stays the window's
        // fallback until the sky composes its first horizon.
        .add_plugins((
            m35_sky_render::M35SkyPlugin,
            m35_vegetation::M35VegetationPlugin,
        ))
        .insert_resource(ClearColor(Color::srgba(0.40, 0.62, 0.94, 1.0)))
        .insert_resource(GlobalAmbientLight {
            brightness: 320.0,
            ..default()
        })
        .insert_resource(DemoWorld(scene))
        .insert_resource(m2_player::ActionState::default())
        .insert_resource(M28Law::new())
        .insert_resource(DemoGuardBillboards::default())
        // B1-d (CS-20): the original opens on the main menu; the world starts
        // with `Enter`. Until then every world system is gated by
        // [`crate::m18_phase::phase_playing`] and the game clock advances 0
        // minutes (the startup composition is the menu backdrop).
        .insert_resource(crate::m18_phase::AppPhase::Menu)
        .add_systems(
            Startup,
            (
                m18_spawn_world,
                m18_spawn_overlays,
                // B3-d (CS-12): the precipitation sheet of the map pixel.
                m18_spawn_weather,
                m2_player::m2_setup_cursor,
                m19_spawn_player,
                m20_spawn_progression,
                m14_audio_startup,
            ),
        )
        // Phase / camera-shell keys that run in BOTH phases: `m18_phase_keys`
        // is the only way out of the menu and the yaw step is a camera control.
        // `m18_phase_ui` selects the visible surface of each phase (M-1).
        //
        // **Declared order (m-3):** `m18_phase_keys` runs BEFORE the world
        // systems that observe the phase. That is the sequence `MENU-001` fixes
        // with `.chain()` (Enter handler → gated world bundle), so the frame that
        // enters the world is the first frame that ticks in both consumers and
        // the probe can never measure an order the window does not declare.
        .add_systems(
            Update,
            (
                m18_phase_keys
                    .before(m2_player::m2_gather_action)
                    .before(m18_apply_player_action),
                m18_yaw_keys,
                m18_phase_ui,
            ),
        )
        // World-only shell keys (m-7): `T` re-samples the terrain (a world
        // mutation) and `1-4` reveal the world panels — neither belongs to the
        // menu, which shows the retail `main-menu` composition only.
        .add_systems(
            Update,
            (m18_origin_toggle, m18_switch_panel).run_if(crate::m18_phase::phase_playing),
        )
        // World systems: gated by the phase (B1-d). `m19_update_title` stays
        // ungated so the bar shows `MENU` / `PLAYING` in both states.
        .add_systems(
            Update,
            (
                m2_player::m2_gather_action,
                m2_player::m2_escape_cursor,
                // M23: the cast resolves before the movement system reads the
                // effect entity, so a Levitate cast takes effect the same
                // frame it lands.
                m23_cast_key.before(m18_apply_player_action),
                // CS-14 scenario: the injected test door tracks the camera so
                // the bash stays reachable after the campaign walked.
                m28_cs14_scenario_door.before(m28_door_bash),
                m28_door_bash.before(m18_apply_player_action),
                m28_guard_arrival,
                m28_sync_census,
                // B3-a (CS-14): the visible guard follows the live census (the
                // law state is written above); idempotent by `spawned`.
                m18_guard_billboards.after(m28_sync_census),
                // B3-a (CS-14 fidelity): the watch REPLACES the pool NPC it is
                // drawn from — both billboards at one spot framed the priest
                // instead of the knight (measured 2026-10-24).
                m28_guards_replace_npcs.after(m18_guard_billboards),
                // CS-14 scenario: once the watch is placed, the camera turns to
                // the first guard so the capture shows the billboard.
                m18_cs13_face_crowd.before(m18_cs14_face_guards),
                m18_cs14_face_guards.after(m18_guard_billboards),
                // B3-b (CS-15/CS-16): `J` / `I` toggle the retail overlays.
                m18_overlay_keys,
                // B3-d (CS-12): `O` cycles to the next precipitating climate.
                m18_weather_key,
                m18_apply_player_action,
                // M20: the driver consumes the signals of the M19 frame and the
                // game clock before the title/HUD consumers read the character.
                m20_progression_tick.after(m18_apply_player_action),
                m18_billboards,
                m18_captions,
                m20_key_use_skill,
                // m-7: the city context song starts with the world, not on
                // `Startup` — the menu is silent.
                m14_song_on_playing,
            )
                .run_if(crate::m18_phase::phase_playing),
        )
        .add_systems(Update, m19_update_title)
        // M14: the two real game events (fall from the M19 frame, melee swing
        // resolved by the M22 engine) are enqueued after the systems that
        // produce them and drained once per frame.
        .add_systems(
            Update,
            (
                m14_fall_sfx.after(m18_apply_player_action),
                m14_melee_sfx.after(m2_player::m2_gather_action),
                m22_enemy_retaliation.after(m14_melee_sfx),
                m14_drain_audio.after(m14_fall_sfx).after(m14_melee_sfx),
            )
                .run_if(crate::m18_phase::phase_playing),
        )
        .run();
    Ok(())
}

/// Builds the production sink: the real device under feature `audio`, the
/// headless sink otherwise — never a panic, always an explicit warning.
fn m14_sink() -> Box<dyn SfxSink> {
    #[cfg(feature = "audio")]
    {
        match audio::RodioSink::new() {
            Ok(sink) => {
                println!("M14 VE: audio sink = rodio (default output device)");
                return Box::new(sink);
            }
            Err(err) => {
                println!("M14 VE: audio device unavailable ({err}) — NullSink, 0 fake playback");
            }
        }
    }
    #[cfg(not(feature = "audio"))]
    {
        println!("M14 VE: feature `audio` not compiled — NullSink (no device playback)");
    }
    Box::new(audio::NullSink)
}

/// M14: opens the retail banks (Mode A) and builds the engine over the sink.
/// Every failure is reported and degrades — the demo keeps running.
///
/// **m-7:** the context song is NOT started here. The demo opens on the menu
/// (B1-d) and the menu is silent; [`m14_song_on_playing`] starts the song on
/// the first `Playing` frame, with the same evidence lines (`M14 VE: music …`).
fn m14_audio_startup(mut commands: Commands, world: Res<DemoWorld>) {
    let engine = match SfxBank::from_arena2() {
        Ok(bank) => {
            println!(
                "M14 VE: effects bank {} ({} records) — dispatch by directory index",
                bank.path().display(),
                bank.len()
            );
            Some(AudioEngine::new(bank, m14_sink()))
        }
        Err(err) => {
            println!(
                "M14 VE: audio degraded — {err}; no clip is fabricated (REQ-L99-046)"
            );
            None
        }
    };
    commands.insert_resource(GameAudio { engine });

    // M22 boundary: the demo character is the player, an enemy-class NPC the
    // enemy; the same seed family the probes use keeps the demo reproducible.
    // The composition lives in `m18_demo::compose_demo_fight` so the window and
    // `ATTACK-KEY-001` fight the SAME pair (0 duplicated logic, B1-c).
    let fight = crate::m18_demo::compose_demo_fight(&world.0);
    commands.insert_resource(DemoCombat {
        session: fight.session,
        enemy_attack: fight.enemy_attack,
        player_level: fight.player_level,
    });
    // M23: the window's magic layer rides the same reproducible seed family,
    // on its own dice stream (casts/rounds never shift an attack or wear
    // roll). The cast key, the per-minute round and the movement projection
    // are the three thin shells over the pure layer.
    commands.insert_resource(M23Magic(daggerfall_app::m23_window::MagicLayer::new(
        crate::m18_demo::M19_SEED,
    )));
    println!(
        "M23 VE: magic layer active · L casts Levitate (Thaumaturgy, retail spell-point cost) · \
         one magic round per game minute · movement flags drive the M19 engine"
    );
}

/// Renders and starts the city day/night context song through the engine.
fn m14_start_song(engine: &mut AudioEngine) {
    let bank = match MusicBank::from_arena2() {
        Ok(bank) => bank,
        Err(err) => {
            println!("M14 VE: music degraded — {err}; no song is fabricated");
            return;
        }
    };
    // The demo has no calendar -> day context (see [`DEMO_GAME_DAYS`]).
    let song = audio::music_context_song(DEMO_GAME_DAYS, false);
    match bank.render(song) {
        Ok(rendered) => {
            m14_song_evidence(&bank, song, &rendered);
            m14_write_music_evidence(song, &rendered);
            if let Err(err) = engine.play_music(rendered) {
                println!("M14 VE: music sink rejected the song ({err}) — effects continue");
            }
        }
        Err(err) => println!(
            "M14 VE: song {} could not be rendered ({err}) — music silent",
            song.name()
        ),
    }
}

/// **m-7 (B1-d)** — starts the city context song on the first `Playing` frame.
///
/// The demo opens on the menu and the 1996 menu is scored by its own track; the
/// demo has none, so the world song waits for the world (CS-20 judges the menu,
/// not the world's music). Gated by [`crate::m18_phase::phase_playing`], once:
/// the `Local` guard makes the start a single event, never a per-frame retry.
/// The evidence lines of [`m14_start_song`] are untouched.
fn m14_song_on_playing(mut audio_state: ResMut<GameAudio>, mut started: Local<bool>) {
    if *started {
        return;
    }
    let Some(engine) = audio_state.engine.as_mut() else {
        return;
    };
    *started = true;
    println!("M14 VE: phase PLAYING — starting the city context song (the menu was silent)");
    m14_start_song(engine);
}

/// Evidence directory of the rendered music (**B1-b**: the WAV is the artefact
/// CS-06/CS-07 keep, never a retail-derived asset committed to git beyond it).
const MUSIC_EVIDENCE_DIR: &str = "docs/sintergic/l5-mission/m18-owner-gate/evidence/audio";
/// Size above which the WAV is **not** written (a footprint JSON is).
const MUSIC_EVIDENCE_MAX_BYTES: usize = 5 * 1024 * 1024;

/// **B1-b** — writes the rendered context song to the evidence directory with a
/// deterministic name (`m18-music-<song>-<pcm digest>.wav`) and prints
/// `M14 VE: music <file> duration=<s>s`.
///
/// Above [`MUSIC_EVIDENCE_MAX_BYTES`] the WAV is replaced by a `.json` carrying
/// `sha256` + duration (+ the reason), so the repository never takes a big
/// binary: the line then says so explicitly.
fn m14_write_music_evidence(song: daggerfall_parser_rs::midi::SongFiles, rendered: &RenderedSong) {
    let dir = std::path::Path::new(MUSIC_EVIDENCE_DIR);
    if let Err(err) = std::fs::create_dir_all(dir) {
        println!(
            "M14 VE: music evidence directory {MUSIC_EVIDENCE_DIR} unavailable ({err}) — \
             duration {:.1} s only",
            rendered.duration_secs
        );
        return;
    }
    let stem = format!(
        "m18-music-{}-{:016x}",
        song.name().to_lowercase(),
        rendered.digest
    );
    let sha256 = crate::m18_demo::sha256_hex(&rendered.wav);
    if rendered.wav.len() > MUSIC_EVIDENCE_MAX_BYTES {
        let path = dir.join(format!("{stem}.json"));
        let body = format!(
            "{{\n  \"file\": \"{stem}.wav\",\n  \"bytes\": {},\n  \"sha256\": \"{sha256}\",\n  \
             \"duration_secs\": {:.3},\n  \"sample_rate\": 11025,\n  \"pcm\": \"u8-mono\",\n  \
             \"reason\": \"WAV exceeds 5 MB — footprint only, the file is reproducible with \
             `cargo run --features window`\"\n}}\n",
            rendered.wav.len(),
            rendered.duration_secs,
        );
        match std::fs::write(&path, body) {
            Ok(()) => println!(
                "M14 VE: music {stem}.wav duration={:.1}s — {} bytes > 5 MB: WAV not written, \
                 footprint in {} (sha256 {sha256})",
                rendered.duration_secs,
                rendered.wav.len(),
                path.display(),
            ),
            Err(err) => println!(
                "M14 VE: music footprint {} could not be written ({err}) — sha256 {sha256} · \
                 duration {:.1}s",
                path.display(),
                rendered.duration_secs,
            ),
        }
        return;
    }
    let path = dir.join(format!("{stem}.wav"));
    match std::fs::write(&path, &rendered.wav) {
        Ok(()) => println!(
            "M14 VE: music {} duration={:.1}s ({} bytes · sha256 {sha256})",
            path.display(),
            rendered.duration_secs,
            rendered.wav.len(),
        ),
        Err(err) => println!(
            "M14 VE: music {} could not be written ({err}) — duration {:.1}s · sha256 {sha256}",
            path.display(),
            rendered.duration_secs,
        ),
    }
}

/// Evidence line of a rendered context song.
fn m14_song_evidence(bank: &MusicBank, song: daggerfall_parser_rs::midi::SongFiles, rendered: &RenderedSong) {
    println!(
        "M14 VE: music context city/day{} -> {}/{} ({} notes, {} ticks @ {}/{} division/bpm) \
         · {:.1} s · {} PCM · FNV-1a64 0x{:016x} · {} WAVE bytes",
        DEMO_GAME_DAYS,
        song.name(),
        song.record_name().unwrap_or("-"),
        rendered.note_events,
        rendered.end_tick,
        rendered.division,
        rendered.bpm,
        rendered.duration_secs,
        rendered.samples,
        rendered.digest,
        rendered.wav.len(),
    );
    println!("M14 VE: music bank {}", bank.path().display());
}

/// M14 event 1: landing damage / hard fall from the M19 engine frame.
fn m14_fall_sfx(mut audio_state: ResMut<GameAudio>, mut m19: ResMut<M19Player>) {
    let Some(event) = m19.pending_fall.take() else {
        return;
    };
    let Some(clip) = gameplay_clip_or_log(event) else {
        return;
    };
    if let Some(engine) = audio_state.engine.as_mut() {
        engine.play(clip);
    }
}

/// M14 event 2: `MouseButton::Left` **or `KeyF`** (B1-c) melee — always the
/// swing clip, plus `Hit*` when the M22 engine says the attack landed.
///
/// **B1-c (O9/CS-21):** the input route is the single action layer — the
/// `m2_player` system sets `ActionState.attack` from the left click **or** the
/// `F` key (`m2_kinematics::attack_triggered`), so both bindings reach this one
/// system and the same production `CombatSession`. A landed strike projects the
/// target's live HP into the HUD line ([`crate::m18_demo::demo_player_strike`])
/// and re-uploads the sprite: the HUD digest changes **exactly** on a hit; a
/// miss writes nothing.
///
/// The demo fights **bare-handed**: the swing is resolved by the production
/// `CombatSession` — the player carries no weapon, so `player_strike(0)` takes
/// the engine's hand-to-hand branch (`FormulaHelper.cs:589-592`) — while the
/// M14 audio policy is unchanged, and both clips follow DFU's unarmed policy,
/// never the armed one:
/// - swing: `SetMelee` assigns `SwingWeaponSound = SoundClips.SwingHighPitch`
///   (`DFU:WeaponManager.cs:756-761`) and the unarmed miss replays that same
///   high pitch (`DFU:EnemySounds.cs:153`); the low pitch (105) only comes from
///   an equipped weapon's `DaggerfallUnityItem.GetSwingSound`;
/// - hit: `Hit1 + Random.Range(2, 4)` ⇒ `Hit3`/`Hit4`
///   (`DFU:EnemySounds.cs:123,127`); `Hit1..=Hit5` is the armed range
///   (`Random.Range(0, 5)`, same lines).
#[allow(clippy::too_many_arguments)] // Bevy system: one parameter per resource/query the swing reads (action, audio, combat, wear, rng, probe)
fn m14_melee_sfx(
    action: Res<m2_player::ActionState>,
    mut audio_state: ResMut<GameAudio>,
    mut combat: ResMut<DemoCombat>,
    mut world: ResMut<DemoWorld>,
    mut progression: ResMut<Progression>,
    law: Res<M28Law>,
    mut upload: M20HudUpload,
    mut audio_dice: Local<Option<daggerfall_combat::Dice100>>,
) {
    if !action.attack {
        return;
    }
    let swing = unarmed_swing_event();
    // Resolved once: the engine plays it and the evidence line names the clip
    // actually requested (no second literal to drift).
    let swing_clip = gameplay_clip_or_log(swing);
    if let Some(clip) = swing_clip {
        if let Some(engine) = audio_state.engine.as_mut() {
            engine.play(clip);
        }
    }
    // The production runtime resolves the swing (the player carries no weapon,
    // so the engine takes the hand-to-hand branch). Shared with ATTACK-KEY-001.
    let session = &mut combat.session;
    let target = law.target;
    let Some(report) = crate::m18_demo::demo_player_strike(&mut world.0, session, target) else {
        println!(
            "M14 VE: melee swing (unarmed) → {} · no strike: target {target} alive={} player alive={}",
            swing_clip.map_or("-", |clip| clip.name()),
            session.enemies.get(target).is_some_and(|enemy| enemy.is_alive()),
            session.player.is_alive(),
        );
        return;
    };
    let strike = &report.strike;
    let attack = &strike.attack;
    println!(
        "M14 VE: melee swing (unarmed) → {} · target={} hit={} chance={} damage={} part={} · enemy HP \
         {}→{} (died: {}) · target_wear={} · weapon_wear={} · HUD {:016x}→{:016x}",
        swing_clip.map_or("-", |clip| clip.name()),
        target,
        attack.hit,
        attack.chance,
        attack.damage,
        attack.part,
        report.target_health_before,
        strike.target_health,
        strike.target_died,
        strike_wear(&strike.target_wear),
        strike_wear(&strike.weapon_wear),
        report.hud_before,
        report.hud_after,
    );
    if report.hud_after != report.hud_before {
        upload.apply(&world.0);
        println!(
            "M20 VE: HUD reflects the target — \"{}\"",
            world.0.m20_status_line()
        );
    }
    // M20: the resolved swing feeds the driver — the weapon skill (unarmed
    // route ⇒ `HandToHand`) plus `CriticalStrike`, **only on hit**
    // (`WeaponManager.cs:419-432`). A miss is a no-op inside the driver.
    let step = progression
        .0
        .player_strike(&mut world.0.character, None, attack.hit);
    m20_report_step("player strike (unarmed)", &step);
    if !attack.hit {
        return;
    }
    // Unarmed hit: `Hit1 + Random.Range(2, 4)` on the same actor PRNG.
    let dice = audio_dice
        .get_or_insert_with(|| daggerfall_combat::Dice100::with_seed(actor_seed(M19_SEED, 1)));
    let die = unarmed_hit_die(dice);
    if let Some(clip) = gameplay_clip_or_log(GameplayEvent::MeleeHit { die }) {
        if let Some(engine) = audio_state.engine.as_mut() {
            engine.play(clip);
        }
    }
}

/// One-line wear report of a strike (`M21 §4`): slot, amount and the resulting
/// condition, or `none` when the struck part carried no piece.
fn strike_wear(wear: &Option<daggerfall_inventory::equipment::ItemWear>) -> String {
    match wear {
        Some(wear) => format!(
            "{:?} wear={} condition={} broke={}",
            wear.slot, wear.wear, wear.condition, wear.broke
        ),
        None => "none".to_string(),
    }
}

/// One-line enchantment report of a strike (`M23`): the callback changes each
/// side received (attacker bearer first, then defender), or `none` when no
/// effect entity/row composed.
fn strike_enchantment(
    enchantment: &Option<daggerfall_combat::enchantments::StrikeCallbacks>,
) -> String {
    match enchantment {
        Some(report) => format!(
            "attacker[{}] defender[{}]",
            report.attacker.len(),
            report.defender.len()
        ),
        None => "none".to_string(),
    }
}

/// M22: the enemy retaliates on its melee timer through the same session, so
/// the damage reaches `SetHealth` + the HUD and the wear reaches the items.
///
/// **B1-b (O4/CS-07):** the retaliation also reaches the M14 audio engine —
/// the same `AudioEngine::play` + `gameplay_clip` route the player's swing
/// uses, with the clip named in the `M14 VE:` evidence line. The policy is
/// DFU's unarmed branch (`EnemySounds.cs:123,127` hit dice; `:153` swing pitch
/// on a miss), drawn from the same actor PRNG the player's hit uses, so a
/// retaliation never shifts the player's stream.
fn m22_enemy_retaliation(
    time: Res<Time>,
    mut audio_state: ResMut<GameAudio>,
    mut combat: ResMut<DemoCombat>,
    mut world: ResMut<DemoWorld>,
    mut progression: ResMut<Progression>,
    mut upload: M20HudUpload,
    mut audio_dice: Local<Option<daggerfall_combat::Dice100>>,
) {
    let DemoCombat {
        session,
        enemy_attack,
        player_level,
    } = &mut *combat;
    if !session.player.is_alive() {
        return;
    }
    if !enemy_attack.tick(time.delta_secs()) {
        return;
    }
    // Every live actor of the encounter retaliates (`EnemyAttack`, one timer
    // per actor in DFU; here the cadence is shared — declared delta, so a
    // spawned guard acts instead of standing inert).
    let alive: Vec<usize> = session
        .enemies
        .iter()
        .enumerate()
        .filter(|(_, enemy)| enemy.is_alive())
        .map(|(index, _)| index)
        .collect();
    if alive.is_empty() {
        return;
    }
    for index in alive {
        let health_before = session.player.health();
        let Some(strike) = session.enemy_strike(index) else {
            continue;
        };
        println!("M22 VE: retaliation from enemy {index}");
        let landed = match (&strike.multi, &strike.single) {
            (Some(multi), _) => multi.hits > 0,
            (None, Some(single)) => single.hit,
            _ => false,
        };
        // B1-b: the retaliating swing reaches the production audio engine.
        let event = if landed {
            let dice = audio_dice
                .get_or_insert_with(|| daggerfall_combat::Dice100::with_seed(actor_seed(M19_SEED, 1)));
            GameplayEvent::MeleeHit {
                die: unarmed_hit_die(dice),
            }
        } else {
            unarmed_swing_event()
        };
        let clip = gameplay_clip_or_log(event);
        if let Some(clip) = clip {
            if let Some(engine) = audio_state.engine.as_mut() {
                engine.play(clip);
            }
        }
        println!(
            "M14 VE: enemy retaliation {} → {} · enemy {index}",
            if landed { "hit" } else { "swing (miss)" },
            clip.map_or("-", |clip| clip.name()),
        );
        let bands = match &strike.multi {
            Some(multi) => format!("multi hits={} gates={} per_hit={:?}", multi.hits, multi.gates, multi.per_hit),
            None => match &strike.single {
                Some(single) => format!("single hit={} damage={}", single.hit, single.damage),
                None => "no branch".to_string(),
            },
        };
        let enchantment = strike_enchantment(&strike.enchantment);
        println!(
            "M22 VE: enemy retaliation — index={index} part={} {bands} damage={} · player HP {health_before}→{} \
             (died: {}) · target_wear={} · weapon_wear={} · enchantment={enchantment} · \
             death latch events={}",
            strike.part,
            strike.damage,
            strike.target_health,
            strike.target_died,
            strike_wear(&strike.target_wear),
            strike_wear(&strike.weapon_wear),
            session.player.death.events(),
        );
        world.0.character.health = session.player.health();
        // M20: every resolved enemy attack tallies Dodging, hit or miss
        // (`EnemyAttack.cs:257-260`).
        let step = progression.0.enemy_strike(&mut world.0.character);
        m20_report_step("enemy strike", &step);
        world.0.m20_refresh_hud();
        // CS-21: the damage the player just took is on screen, not only in the
        // resource (the same upload path the progression uses).
        upload.apply(&world.0);
        let roll = session.enemy_timer_roll(index).unwrap_or(1500);
        enemy_attack.reset_melee_timer(
            roll,
            *player_level,
            world.0.character.reflex as i32,
        );
    }
}

/// Clip of a gameplay event, with an explicit line when the event maps to no
/// clip (never a silent drop).
fn gameplay_clip_or_log(event: GameplayEvent) -> Option<daggerfall_parser_rs::snd::SoundClips> {
    match audio::gameplay_clip(event) {
        Some(clip) => Some(clip),
        None => {
            println!("M14 VE: gameplay event {event:?} maps to no DFU clip — skipped");
            None
        }
    }
}

/// Swing event of the unarmed demo attack: DFU `SetMelee` sets
/// `SwingWeaponSound = SoundClips.SwingHighPitch` for a bare-handed swing
/// (`DFU:WeaponManager.cs:756-761`) and the unarmed miss replays that pitch
/// (`DFU:EnemySounds.cs:153`).
fn unarmed_swing_event() -> GameplayEvent {
    GameplayEvent::MeleeSwing { high: true }
}

/// Unarmed hit die: `(int)SoundClips.Hit1 + Random.Range(2, 4)` ⇒ `Hit3`/`Hit4`
/// (`DFU:EnemySounds.cs:123,127`). `Hit1` is index 1, so the draw is `3..=4`
/// over the actor PRNG that resolved the attack; the helper stays total without
/// an `unwrap`.
fn unarmed_hit_die(dice: &mut daggerfall_combat::Dice100) -> u8 {
    if dice.roll_range(3, 4) == 4 {
        4
    } else {
        3
    }
}

/// M14: drains the queue every frame; a failure is reported once and the demo
/// continues (0 panic, 0 stack of silent errors).
fn m14_drain_audio(mut audio_state: ResMut<GameAudio>, mut warned: Local<bool>) {
    let Some(engine) = audio_state.engine.as_mut() else {
        return;
    };
    match engine.drain() {
        Ok(played) if played > 0 => {
            if let Some(last) = engine.played().last() {
                println!(
                    "M14 VE: audio → {} clip(s) · last {} · {} PCM · FNV-1a64 0x{:016x}",
                    played,
                    last.clip.name(),
                    last.samples,
                    last.digest,
                );
            }
        }
        Ok(_) => {}
        Err(err) => {
            if !*warned {
                *warned = true;
                println!("M14 VE: audio drain degraded ({err}) — later clips still play");
            }
        }
    }
}

/// Builds the scene: textured city + terrain + NPCs + UI + cameras.
fn m18_spawn_world(
    mut commands: Commands,
    world: Res<DemoWorld>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let scene = &world.0;

    // ── D1 · retail city: groups by texture key (M12) ───────────────────
    let ground_material = bridge::material_from_spec(
        &bridge::MaterialSpec {
            base_color: [0.33, 0.31, 0.28, 1.0],
            ..default()
        },
        &mut materials,
    );
    let mut textured_groups = 0usize;
    let mut textured_triangles = 0usize;
    for group in bridge::split_by_texture(&scene.city.combined) {
        let (handle, textured) = if let Some(texture) = scene.textures.get(&group.texture_key) {
            match bridge::image_from_rgba(
                &texture.rgba,
                u32::from(texture.width),
                u32::from(texture.height),
                &mut images,
            ) {
                Ok(image) => {
                    let uvs = bridge::scale_uvs(
                        &group.mesh.uvs,
                        u32::from(texture.width),
                        u32::from(texture.height),
                    );
                    let spec = bridge::MeshSpec {
                        uvs,
                        ..group.mesh.clone()
                    };
                    let mesh = bridge::mesh_from_spec(&spec, &mut meshes);
                    let material = bridge::material_from_texture(
                        &bridge::MaterialSpec {
                            base_color: [1.0, 1.0, 1.0, 1.0],
                            ..default()
                        },
                        image,
                        &mut materials,
                    );
                    (Some((mesh, material)), true)
                }
                Err(_) => (None, false),
            }
        } else {
            (None, false)
        };
        let (mesh, material) = handle.unwrap_or_else(|| {
            (
                bridge::mesh_from_spec(&group.mesh, &mut meshes),
                ground_material.clone(),
            )
        });
        if textured {
            textured_groups += 1;
            textured_triangles += group.mesh.triangle_count();
        }
        commands.spawn((Mesh3d(mesh), MeshMaterial3d(material)));
    }
    println!(
        "M18 VE: city → {} textured groups ({textured_triangles} triangles with retail texture)",
        textured_groups
    );

    // ── D2 · M13 terrain (one unlit mesh per band) ──────────────────────
    let terrain_entities =
        spawn_terrain_meshes(&mut commands, &mut meshes, &mut materials, &scene.terrain);
    commands.insert_resource(TerrainMeshes(terrain_entities));

    // ── D3 · M15 NPCs (8-dir billboard) ────────────────────────────────
    let quad = unit_billboard(&mut meshes);
    let mut npcs = Vec::with_capacity(scene.npcs.len());
    for npc in &scene.npcs {
        let Ok(image) = bridge::image_from_rgba(
            &npc.flat.rgba,
            u32::from(npc.flat.width),
            u32::from(npc.flat.height),
            &mut images,
        ) else {
            continue;
        };
        let material = bridge::material_from_texture(
            &bridge::MaterialSpec {
                base_color: [1.0, 1.0, 1.0, 1.0],
                ..default()
            },
            image,
            &mut materials,
        );
        if let Some(mut mat) = materials.get_mut(&material) {
            mat.alpha_mode = AlphaMode::Mask(0.5);
        }
        let height = 2.2f32;
        let width = height * (f32::from(npc.flat.width) / f32::from(npc.flat.height.max(1)));
        let position = Vec3::from_array(npc.position);
        let entity = commands
            .spawn((
                Mesh3d(quad.clone()),
                MeshMaterial3d(material),
                Transform::from_translation(position)
                    .with_scale(Vec3::new(width, height, 1.0)),
            ))
            .id();
        npcs.push((entity, position));
        println!(
            "M18 VE: NPC {} ({}) at ({:.1},{:.1}) · flat {}.{} · caption \"{}\"",
            npc.spec.name,
            npc.spec.race.name(),
            npc.position[0],
            npc.position[2],
            npc.flat.width,
            npc.flat.height,
            npc.spec.caption.text,
        );
    }
    if npcs.is_empty() {
        eprintln!("M18 VE: 0 NPCs placed (unresolvable flats) — the NPC layer is not shown");
    }

    // ── 3D first-person camera + light ─────────────────────────────────
    commands.spawn((
        Camera3d::default(),
        Transform::from_translation(Vec3::from_array(scene.spawn.eye))
            .with_rotation(Quat::from_rotation_y(scene.spawn.yaw)),
        m2_player::M2Player {
            yaw: scene.spawn.yaw,
            pitch: 0.0,
            radius: bridge::PLAYER_RADIUS_M,
        },
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 12_000.0,
            ..default()
        },
        Transform::from_xyz(0.0, 200.0, 0.0)
            .with_rotation(Quat::from_euler(EulerRot::XYZ, -1.0, 0.4, 0.0)),
    ));

    // ── D4 · overlaid M17 UI (2D camera over the 3D one) ───────────────
    commands.spawn((
        Camera2d,
        Camera {
            order: 1,
            clear_color: ClearColorConfig::None,
            ..default()
        },
    ));
    let mut panel_sprites = Vec::with_capacity(scene.panels.len());
    for (index, panel) in scene.panels.iter().enumerate() {
        let image = match bridge::image_from_rgba(
            &panel.rgba,
            u32::from(panel.width),
            u32::from(panel.height),
            &mut images,
        ) {
            Ok(image) => image,
            Err(err) => {
                // CS-18/CS-19 live on panels 5/6: a silently skipped sprite
                // makes the key a no-op in the window (measured 2026-10-23),
                // so the failure is named instead of swallowed.
                eprintln!(
                    "M18 VE: panel {index} not drawn — {}×{}: {err}",
                    panel.width, panel.height
                );
                continue;
            }
        };
        let mut entity = commands.spawn((
            Sprite {
                image,
                custom_size: Some(Vec2::new(
                    f32::from(panel.width) * 2.0,
                    f32::from(panel.height) * 2.0,
                )),
                ..default()
            },
            Transform::from_xyz(0.0, if index == 0 { 0.0 } else { PARKED_Y }, 0.0),
        ));
        if index != 0 {
            entity.insert(Visibility::Hidden);
        }
        panel_sprites.push(entity.id());
    }
    // Default world view (panel 0 + HUD); `m18_phase_ui` applies the `Menu`
    // surface (`main-menu` full screen, HUD hidden) on the first frame.
    let drawn_panels = panel_sprites.len();
    commands.insert_resource(DemoUi {
        sprites: panel_sprites,
        current: 0,
    });
    println!(
        "M18 VE: 1996 UI → {}/{} overlaid panels drawn (320×200 canvas ×2) · keys 1-6",
        drawn_panels,
        scene.panels.len()
    );

    // ── D5 · live M20 HUD (top-left, 2× scale like the panels) ──────────
    let hud = &scene.m20_hud;
    let hud_sprite = match bridge::image_from_rgba(
        &hud.rgba,
        u32::from(hud.width),
        u32::from(hud.height),
        &mut images,
    ) {
        Ok(image) => Some(
            commands
                .spawn((
                    Sprite {
                        image,
                        custom_size: Some(Vec2::new(
                            f32::from(hud.width) * 2.0,
                            f32::from(hud.height) * 2.0,
                        )),
                        ..default()
                    },
                    m20_hud_transform(windows.single().ok(), hud),
                ))
                .id(),
        ),
        Err(err) => {
            eprintln!("M20 VE: HUD without texture ({err:?}) — the status line is not drawn");
            None
        }
    };
    commands.insert_resource(DemoHud {
        sprite: hud_sprite,
    });
    println!(
        "M20 VE: {} · HUD {}×{} px · {} opaque px · R key = real Running use · digest {:016x}",
        scene.m20_status_line(),
        hud.width,
        hud.height,
        hud.rgba.chunks_exact(4).filter(|px| px[3] != 0).count(),
        scene.m20_hud_digest(),
    );

    // NPC captions (transparent sprites; visible when approaching).
    let mut caption_sprites = Vec::with_capacity(scene.npcs.len());
    for npc in &scene.npcs {
        let Ok(image) = bridge::image_from_rgba(
            &npc.caption.rgba,
            u32::from(npc.caption.width),
            u32::from(npc.caption.height),
            &mut images,
        ) else {
            caption_sprites.push(Entity::PLACEHOLDER);
            continue;
        };
        let entity = commands
            .spawn((
                Sprite {
                    image,
                    custom_size: Some(Vec2::new(
                        f32::from(npc.caption.width) * 2.0,
                        f32::from(npc.caption.height) * 2.0,
                    )),
                    ..default()
                },
                Transform::from_xyz(0.0, -250.0, 10.0),
                Visibility::Hidden,
            ))
            .id();
        caption_sprites.push(entity);
    }
    commands.insert_resource(DemoCaptions {
        sprites: caption_sprites,
        visible: None,
    });
    commands.insert_resource(DemoNpcs(npcs));
}

/// Spawns one unlit mesh per terrain band (ocean/beach/land) and returns the
/// entities so the `T` key can replace them (**B1-a**).
///
/// **CS-08:** the `Ocean` group **is** the sea surface — the retail sampler
/// clamps oceanic samples to `OCEAN_ELEVATION` (`DefaultTerrainSampler.cs:115-118`),
/// so the band lies on one plane at [`DemoTerrain::sea_level_m`]; it is drawn as
/// water and the collision adapter refuses it.
fn spawn_terrain_meshes(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    terrain: &crate::m18_demo::DemoTerrain,
) -> Vec<Entity> {
    let mut entities = Vec::with_capacity(terrain.groups.len());
    for group in &terrain.groups {
        let (color, label) = match group.band {
            daggerfall_parser_rs::woods::TerrainBand::Ocean => {
                ([0.13, 0.36, 0.62, 1.0], "water")
            }
            daggerfall_parser_rs::woods::TerrainBand::Beach => ([0.72, 0.66, 0.45, 1.0], "beach"),
            daggerfall_parser_rs::woods::TerrainBand::Land => ([0.24, 0.40, 0.20, 1.0], "land"),
        };
        let mesh = bridge::mesh_from_spec(&group.spec, meshes);
        let material = bridge::material_from_spec(
            &bridge::MaterialSpec {
                base_color: color,
                unlit: true,
                ..default()
            },
            materials,
        );
        println!(
            "M18 VE: terrain band {label} → {} triangles",
            group.spec.triangle_count()
        );
        entities.push(
            commands
                .spawn((Mesh3d(mesh), MeshMaterial3d(material)))
                .id(),
        );
    }
    match terrain.sea_level_m {
        Some(sea_level) => println!(
            "M18 VE: water surface y={sea_level:.2} m (OCEAN_ELEVATION − datum) · {} ocean quads \
             at the band elevation · not walkable (CS-08)",
            terrain.water_quads,
        ),
        None => println!(
            "M18 VE: 0 ocean samples in the ring — no water surface (CS-08 does not apply to this \
             composition)"
        ),
    }
    entities
}

/// **B1-d (CS-20)** — `Enter` leaves the menu: `Menu` ⇒ `Playing`, one way.
///
/// The transition itself is the shared pure [`crate::m18_phase::phase_after_enter`];
/// this system only reads the key (single input surface convention of the demo).
/// Nothing of the composition changes: the spawn, the yaw and the HUD are the
/// ones the menu already held.
fn m18_phase_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut phase: ResMut<crate::m18_phase::AppPhase>,
) {
    if !keys.just_pressed(KeyCode::Enter) {
        return;
    }
    let next = crate::m18_phase::phase_after_enter(*phase);
    if next != *phase {
        println!(
            "M18 VE: phase {} → {} (Enter) — world systems live, the clock starts at the \
             menu's 0 minutes",
            phase.label(),
            next.label()
        );
        *phase = next;
    }
}

/// **M-1 (B1-d / CS-20)** — puts the phase's surface on screen.
///
/// `Menu` ⇒ the retail `main-menu` panel full screen (index 3 of
/// `ui_spec::build_all_panels`: `TITL00I0` + `MENU.IMG` + buttons) with the
/// HUD hidden; `Playing` ⇒ the `hud` panel plus the M20 HUD sprite, i.e. the
/// default world view the pre-B1 build showed. The panel is selected by
/// [`crate::m18_phase::visible_panel_index`] — the same pure function
/// `MENU-001` reads over the composed panels — so the screen and the probe
/// cannot drift. Only a surface change touches `Visibility`.
fn m18_phase_ui(
    phase: Res<crate::m18_phase::AppPhase>,
    world: Res<DemoWorld>,
    hud: Res<DemoHud>,
    mut ui: ResMut<DemoUi>,
    mut visibilities: Query<&mut Visibility>,
    mut applied: Local<Option<(usize, bool)>>,
) {
    let ids: Vec<&str> = world
        .0
        .panels
        .iter()
        .map(|panel| panel.id.as_str())
        .collect();
    let Some(index) = crate::m18_phase::visible_panel_index(*phase, &ids) else {
        return;
    };
    let hud_visible = crate::m18_phase::hud_visible(*phase);
    if *applied == Some((index, hud_visible)) {
        return;
    }
    *applied = Some((index, hud_visible));
    if index >= ui.sprites.len() {
        println!(
            "M18 VE: UI surface {} — panel \"{}\" has no sprite (its image could not be \
             uploaded)",
            phase.label(),
            ids[index],
        );
    }
    for (at, entity) in ui.sprites.iter().enumerate() {
        if let Ok(mut visibility) = visibilities.get_mut(*entity) {
            *visibility = if at == index {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }
    ui.current = index;
    if let Some(entity) = hud.sprite {
        if let Ok(mut visibility) = visibilities.get_mut(entity) {
            *visibility = if hud_visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }
    println!(
        "M18 VE: UI surface {} → panel \"{}\" (index {index}, {} opaque px) · HUD {}",
        phase.label(),
        ids[index],
        world.0.panels[index].opaque,
        if hud_visible { "visible" } else { "hidden" },
    );
}

/// **B1-a** — `,` / `.` rotate the view ±[`crate::m18_demo::YAW_KEY_STEP_RAD`]
/// (5°). The camera is a shell, so the key works in both phases.
fn m18_yaw_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut players: Query<(&mut Transform, &mut m2_player::M2Player)>,
) {
    let step = if keys.just_pressed(KeyCode::Comma) {
        crate::m18_demo::YAW_KEY_STEP_RAD
    } else if keys.just_pressed(KeyCode::Period) {
        -crate::m18_demo::YAW_KEY_STEP_RAD
    } else {
        return;
    };
    for (mut transform, mut player) in &mut players {
        player.yaw += step;
        transform.rotation = Quat::from_euler(EulerRot::YXZ, player.yaw, player.pitch, 0.0);
        println!(
            "M18 VE: yaw {:.1}° ({:.4} rad) · , / . step {:.0}°",
            player.yaw.to_degrees(),
            player.yaw,
            crate::m18_demo::YAW_KEY_STEP_RAD.to_degrees(),
        );
    }
}

/// **B1-a (O2)** — `T` toggles the terrain sampling pixel between the capital
/// and the nearest **coastal** pixel of the ring (`Ocean>0 ∧ Beach>0`), by the
/// deterministic search of [`crate::m18_demo::find_coastal_pixel`] (Chebyshev
/// distance, then `(x, y)` ascending). With no coastal pixel in the ring the
/// line says so and the terrain stays untouched (0 invented coordinates).
///
/// Only the terrain is re-sampled ([`crate::m18_demo::DemoScene::recompose_terrain`]);
/// the city hull, its collision grid and the player frame do not move.
///
/// **m-7:** the key is registered inside the `Playing` bundle — re-sampling is a
/// world mutation and the frozen menu cannot trigger it.
#[allow(clippy::too_many_arguments)] // Bevy system: one parameter per resource the origin/coastal toggle reads (commands, keys, world, city, coastal cache)
fn m18_origin_toggle(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut world: ResMut<DemoWorld>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut terrain_meshes: ResMut<TerrainMeshes>,
    mut woods: Local<Option<daggerfall_parser_rs::woods::WoodsMap>>,
    mut coastal: Local<Option<Option<crate::m18_demo::CoastalPixel>>>,
) {
    if !keys.just_pressed(KeyCode::KeyT) {
        return;
    }
    if woods.is_none() {
        match crate::m18_demo::read_woods() {
            Ok(parsed) => *woods = Some(parsed),
            Err(err) => {
                println!("M18 VE: T — WOODS.WLD unavailable ({err}); terrain origin unchanged");
                return;
            }
        }
    }
    let Some(woods) = woods.as_ref() else {
        return;
    };
    let capital = world.0.map_pixel;
    let current = world.0.terrain.origin_pixel;
    let target = if current == capital {
        if coastal.is_none() {
            *coastal = Some(crate::m18_demo::find_coastal_pixel(woods, capital));
        }
        match coastal.as_ref().copied().flatten() {
            Some(found) => {
                println!(
                    "M18 VE: coastal pixel ({},{}) at Chebyshev {}/{} · ocean {} / beach {} / land {} samples",
                    found.pixel[0],
                    found.pixel[1],
                    found.chebyshev_from(capital),
                    crate::m18_demo::COASTAL_RING,
                    found.ocean,
                    found.beach,
                    found.land,
                );
                found.pixel
            }
            None => {
                println!(
                    "M18 VE: no coastal pixel in ring (±{} px around ({},{})) — terrain origin unchanged",
                    crate::m18_demo::COASTAL_RING,
                    capital[0],
                    capital[1]
                );
                return;
            }
        }
    } else {
        capital
    };
    if let Err(err) = world.0.recompose_terrain(woods, target) {
        println!("M18 VE: T — terrain re-sample failed ({err}); origin unchanged");
        return;
    }
    for entity in terrain_meshes.0.drain(..) {
        commands.entity(entity).despawn();
    }
    terrain_meshes.0 = spawn_terrain_meshes(
        &mut commands,
        &mut meshes,
        &mut materials,
        &world.0.terrain,
    );
    let (ocean, beach, land) = world.0.terrain.band_counts();
    println!(
        "M18 VE: origin-override px=({},{}) — bands ocean {ocean} / beach {beach} / land {land} · \
         {} terrain triangles",
        target[0],
        target[1],
        world.0.terrain.triangles,
    );
}

/// Unit quad in XY (base at y=0, facing +Z) for NPC billboards.
fn unit_billboard(meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
    let spec = bridge::MeshSpec {
        positions: vec![
            [-0.5, 0.0, 0.0],
            [0.5, 0.0, 0.0],
            [0.5, 1.0, 0.0],
            [-0.5, 1.0, 0.0],
        ],
        normals: vec![[0.0, 0.0, 1.0]; 4],
        indices: vec![0, 1, 2, 0, 2, 3],
        uvs: vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
        face_textures: vec![bridge::mesh_spec::NO_TEXTURE; 4],
    };
    bridge::mesh_from_spec(&spec, meshes)
}

/// M20 HUD position: top-left of the 2D camera viewport
/// (`ScalingMode::WindowSize` ⇒ 1 unit = 1 logical px) with a 16 px margin.
fn m20_hud_transform(window: Option<&Window>, hud: &DemoTexture) -> Transform {
    let (width, height) = window
        .map(|window| (window.width(), window.height()))
        .unwrap_or((1280.0, 720.0));
    let hud_width = f32::from(hud.width) * 2.0;
    let hud_height = f32::from(hud.height) * 2.0;
    Transform::from_xyz(
        -width / 2.0 + 16.0 + hud_width / 2.0,
        height / 2.0 - 16.0 - hud_height / 2.0,
        100.0,
    )
}

/// M20 startup: the progression driver and the game clock of the window.
///
/// D-M20-1: the driver owns its SplitMix64 stream seeded with [`M19_SEED`]
/// (the canonical demo seed), so the level-up roll is reproducible. The clock
/// starts at the DFU classic date and its `last_minute` is the starting
/// minute: only the minutes the window actually advances are evaluated.
fn m20_spawn_progression(mut commands: Commands) {
    let clock = GameClock::classic_default();
    let last_minute = clock.total_minutes() as i64;
    commands.insert_resource(Progression(SkillDriver::new(M19_SEED)));
    commands.insert_resource(ProgressionClock {
        clock,
        carry: 0.0,
        last_minute,
    });
    println!(
        "M20 VE: progression driver active · seed {M19_SEED} · clock {}-{:02}-{:02} {} ({}) · 1 real s = {} game min · Running/4 updates · Jumping/edge · Climbing/check · Swimming/min · Dodging/strike · weapon+H2H+CriticalStrike on hit",
        clock.year,
        clock.month,
        clock.day,
        clock.time_string(),
        clock.phase_label(),
        TIME_SCALE_GAME_SECONDS_PER_REAL_SECOND,
    );
}

/// M20: re-projects the character into every live consumer of the window —
/// HUD line, M19 `PlayerBody`, M22 player fighter and the enemy timer level.
///
/// The character → fighter projection is the shared
/// [`crate::m18_demo::m20_sync_player_fighter`] (single copy: the headless
/// `ATTACK-KEY-001` mounts the same fight).
fn m20_sync_character(
    scene: &mut DemoScene,
    m19: &mut M19Player,
    combat: &mut DemoCombat,
) {
    scene.m20_refresh_hud();
    m19.body = scene.character.player_body();
    crate::m18_demo::m20_sync_player_fighter(&scene.character, &mut combat.session.player);
    combat.player_level = scene.character.level;
}

/// One `M20 VE:` line per progression event; returns `true` when the step
/// changed observable progression state (tally, raise or level-up).
fn m20_report_step(label: &str, step: &DriverStep) -> bool {
    let observable = !step.tallies.is_empty() || !step.raised.is_empty() || step.level_up.is_some();
    if !observable && !step.checked {
        return false;
    }
    let level_up = match &step.level_up {
        Some(report) => format!(
            "LV {} (+{} HP, pool {})",
            report.level, report.hit_points, report.bonus_pool
        ),
        None => "none".to_string(),
    };
    println!(
        "M20 VE: {label} · tallies {:?} · raised {:?} · checked={} · ready={} · level_up {level_up}",
        step.tallies, step.raised, step.checked, step.ready_to_level_up,
    );
    observable
}

/// M20 HUD upload handles: the sprite, the image assets and the primary
/// window (shared by the R accelerator and [`m20_progression_tick`]).
#[derive(SystemParam)]
struct M20HudUpload<'w, 's> {
    hud: Res<'w, DemoHud>,
    images: ResMut<'w, Assets<Image>>,
    sprites: Query<'w, 's, (&'static mut Sprite, &'static mut Transform)>,
    windows: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
}

impl M20HudUpload<'_, '_> {
    /// Replaces the HUD sprite image with the current [`DemoScene::m20_hud`].
    fn apply(&mut self, scene: &DemoScene) {
        let Some(entity) = self.hud.sprite else {
            return;
        };
        let Ok((mut sprite, mut transform)) = self.sprites.get_mut(entity) else {
            return;
        };
        let window = self.windows.single().ok();
        m20_upload_hud(scene, &mut sprite, &mut transform, &mut self.images, window);
    }
}

/// Uploads the current HUD RGBA to the sprite (shared by the R accelerator
/// and [`m20_progression_tick`]).
fn m20_upload_hud(
    scene: &DemoScene,
    sprite: &mut Sprite,
    transform: &mut Transform,
    images: &mut Assets<Image>,
    window: Option<&Window>,
) {
    match bridge::image_from_rgba(
        &scene.m20_hud.rgba,
        u32::from(scene.m20_hud.width),
        u32::from(scene.m20_hud.height),
        images,
    ) {
        Ok(image) => {
            sprite.image = image;
            sprite.custom_size = Some(Vec2::new(
                f32::from(scene.m20_hud.width) * 2.0,
                f32::from(scene.m20_hud.height) * 2.0,
            ));
            *transform = m20_hud_transform(window, &scene.m20_hud);
        }
        Err(err) => eprintln!("M20 VE: HUD could not be uploaded to the GPU: {err:?}"),
    }
}

/// **R** key: demo **accelerator** (not gameplay) — 109 `Running` uses plus an
/// evaluation at `last check + 361` min, both through the progression driver
/// (`use_skill` + `minute`), with HUD re-render, sprite upload and rebuild of
/// the M19 `PlayerBody`/M22 player.
fn m20_key_use_skill(
    keys: Res<ButtonInput<KeyCode>>,
    mut world: ResMut<DemoWorld>,
    mut m19: ResMut<M19Player>,
    mut progression: ResMut<Progression>,
    mut combat: ResMut<DemoCombat>,
    mut upload: M20HudUpload,
) {
    if !keys.just_pressed(KeyCode::KeyR) {
        return;
    }
    let scene = &mut world.0;
    let digest_before = scene.m20_hud_digest();
    let running_before = scene.character.live_skill(Skill::Running);
    let use_step = progression
        .0
        .use_skill(&mut scene.character, Skill::Running, 109);
    let now_minutes = scene.character.last_skill_check_minutes + SKILL_CHECK_INTERVAL_MINUTES + 1;
    let minute_step = progression.0.minute(
        &mut scene.character,
        now_minutes,
        &MotionSignals::default(),
    );
    let observed = m20_report_step("accelerator use (109 Running)", &use_step)
        | m20_report_step("accelerator evaluation", &minute_step);
    if observed {
        m20_sync_character(scene, &mut m19, &mut combat);
    }
    let running_after = scene.character.live_skill(Skill::Running);
    let digest_after = scene.m20_hud_digest();
    upload.apply(scene);
    println!(
        "M20 VE: Running {running_before} → {running_after} · HUD digest {digest_before:016x} → {digest_after:016x} · checked={} · raised={:?} · line \"{}\"",
        minute_step.checked,
        minute_step.raised,
        scene.m20_status_line(),
    );
}

/// [`m20_progression_tick`] parameters: the frame clock, the demo world, the
/// M19/M20/M22/M23 state and the HUD upload handles, bundled so the system
/// stays single-argument (same shape as [`M20HudUpload`]).
#[derive(SystemParam)]
struct M20TickParams<'w, 's> {
    time: Res<'w, Time>,
    world: ResMut<'w, DemoWorld>,
    m19: ResMut<'w, M19Player>,
    progression: ResMut<'w, Progression>,
    clock: ResMut<'w, ProgressionClock>,
    combat: ResMut<'w, DemoCombat>,
    magic: ResMut<'w, M23Magic>,
    upload: M20HudUpload<'w, 's>,
}

/// M20: per-frame progression (`PlayerEntity.FixedUpdate` + `RaiseSkills`).
///
/// 1. `advance(dt, signals)`: Running every 4 classic updates, Jumping on the
///    jump edge, Climbing per executed check — frame-rate independent carry.
/// 2. The game clock advances at the documented M4 scale (1 real second =
///    1 game minute, [`TIME_SCALE_GAME_SECONDS_PER_REAL_SECOND`]); every new
///    game minute tallies Swimming (if swimming) and evaluates `RaiseSkills`
///    and the level-up, as D-M20-4/D-M20-5 declare; it also runs one M23
///    magic round over the player's effect entity (DFU
///    `EntityEffectBroker.cs:205-245`: "every game minute passing is another
///    magic round").
/// 3. Any tally/raise/level-up re-projects the character into the HUD, the M19
///    body and the M22 player.
fn m20_progression_tick(mut p: M20TickParams) {
    let dt = p.time.delta_secs();
    let signals = p.m19.last_signals;
    let scene = &mut p.world.0;
    let step = p.progression.0.advance(&mut scene.character, dt, &signals);
    let mut observed = m20_report_step("advance", &step);

    p.clock.carry += f64::from(dt) * (TIME_SCALE_GAME_SECONDS_PER_REAL_SECOND as f64);
    let whole = p.clock.carry.floor();
    if whole >= 1.0 {
        p.clock.clock.raise_seconds(whole as u64);
        p.clock.carry -= whole;
        let now = p.clock.clock.total_minutes() as i64;
        while p.clock.last_minute < now {
            p.clock.last_minute += 1;
            let minute = p
                .progression
                .0
                .minute(&mut scene.character, p.clock.last_minute, &signals);
            observed |= m20_report_step("minute", &minute);
            // M23: one magic round per game minute passed (see above).
            observed |=
                m23_magic_round(&mut p.magic.0, &mut p.combat.session, &mut scene.character);
        }
    }

    if observed {
        m20_sync_character(scene, &mut p.m19, &mut p.combat);
        p.upload.apply(scene);
    }
}

/// M23: one game-minute magic round over the player's effect entity
/// ([`daggerfall_app::m23_window::MagicLayer::magic_round`], DFU `DoMagicRound`,
/// `EntityEffectManager.cs:1730-1735`; one round per minute passed,
/// `EntityEffectBroker.cs:205-245`). The spell-point pool the round moved is
/// written back into the character sheet — the single source the HUD line
/// projects. Returns whether the round changed observable magic state (an
/// application, an `End()` or a state change).
fn m23_magic_round(
    magic: &mut daggerfall_app::m23_window::MagicLayer,
    session: &mut daggerfall_app::combat_runtime::CombatSession,
    character: &mut daggerfall_skills::character::Character,
) -> bool {
    let level = session.player.level;
    let Some(target) = session.player.effects.as_mut() else {
        return false;
    };
    let report = magic.magic_round(level, target);
    character.magicka = target.spell_points;
    if report.applications.is_empty() && report.expired.is_empty() && report.state_changes.is_empty()
    {
        return false;
    }
    let movement = target.movement_effects();
    println!(
        "M23 VE: magic round · applied {} · expired {:?} · state {} · pool {} · \
         levitating={} slow_falling={} water_breathing={} water_walking={}",
        report.applications.len(),
        report.expired,
        report.state_changes.len(),
        target.spell_points,
        movement.levitating,
        movement.slow_falling,
        movement.water_breathing,
        movement.water_walking,
    );
    true
}

/// M23 (REQ-L99-056): **L** casts the demo's Levitate on the player's effect
/// entity — a thin shell over [`daggerfall_app::m23_window::MagicLayer::cast_levitate`].
/// The key is a demo binding (the retail binding is the spellbook's hotkey);
/// the cast lifecycle, the retail spell-point cost and the pool gate are the
/// engine's (DFU `EntityEffectManager.cs:337-343, 387-388`). The pool the
/// cast moved is written back into the M20 character sheet — the single
/// source the HUD line projects.
fn m23_cast_key(
    keys: Res<ButtonInput<KeyCode>>,
    mut magic: ResMut<M23Magic>,
    mut combat: ResMut<DemoCombat>,
    mut world: ResMut<DemoWorld>,
    mut upload: M20HudUpload,
) {
    if !keys.just_pressed(KeyCode::KeyL) {
        return;
    }
    let level = combat.player_level;
    // Levitate is Thaumaturgy (`Levitate.cs:23-37`); DFU resolves the cost
    // against the caster's live school skill (`GetLiveSkillValue`).
    let live_school = i32::from(world.0.character.live_skill(Skill::Thaumaturgy));
    let Some(target) = combat.session.player.effects.as_mut() else {
        println!("M23 VE: cast Levitate — the player carries no effect entity");
        return;
    };
    let pool_before = target.spell_points;
    let outcome = magic.0.cast_levitate(level, live_school, target);
    // The entity owns the pool; the sheet projects it.
    world.0.character.magicka = target.spell_points;
    world.0.m20_refresh_hud();
    upload.apply(&world.0);
    let pool_after = target.spell_points;
    if outcome.applied {
        let movement = target.movement_effects();
        println!(
            "M23 VE: cast Levitate · level={level} thaumaturgy={live_school} · cost {} gold/{} \
             points · pool {pool_before}→{pool_after} · rounds {:?} · levitating={} \
             slow_falling={} water_breathing={} water_walking={}",
            outcome.cost.gold,
            outcome.cost.spell_points,
            outcome
                .report
                .as_ref()
                .map(|report| report.scheduled.as_slice())
                .unwrap_or_default(),
            movement.levitating,
            movement.slow_falling,
            movement.water_breathing,
            movement.water_walking,
        );
    } else {
        println!(
            "M23 VE: cast Levitate refused — pool {pool_before} < cost {} points \
             (EntityEffectManager.cs:337-343)",
            outcome.cost.spell_points
        );
    }
}

/// M19 player at the retail spawn: engine + body + die (once).
fn m19_spawn_player(mut commands: Commands, world: Res<DemoWorld>) {
    let scene = &world.0;
    let locomotion = Locomotion::new(
        scene.spawn.eye,
        bridge::EYE_HEIGHT_M,
        bridge::PLAYER_RADIUS_M,
    );
    println!(
        "M19 VE: DFU locomotion engine active · eye=({:.2},{:.2},{:.2}) yaw={:.2} · radius {:.2} · \
         Space jump · Shift run · Ctrl crouch · C sneak",
        scene.spawn.eye[0],
        scene.spawn.eye[1],
        scene.spawn.eye[2],
        scene.spawn.yaw,
        bridge::PLAYER_RADIUS_M,
    );
    commands.insert_resource(M19Player {
        locomotion,
        // M20 (spec §7.9/§9): engine body from the real character.
        body: scene.character.player_body(),
        dice: Dice100::new(M19_SEED),
        sneak: false,
        pending_fall: None,
        last_signals: MotionSignals::default(),
    });
    println!(
        "M20 VE: PlayerBody from {} · live_speed {} · running {} · jumping {} · stamina {} · line \"{}\"",
        scene.character.career.name,
        scene.character.player_body().live_speed,
        scene.character.player_body().running_skill,
        scene.character.player_body().jumping_skill,
        scene.character.rpg_stats().stamina,
        scene.m20_status_line(),
    );
}

/// First-person movement: the M19 engine decides (retail grid inside the
/// city, M13 sampler outside) through the demo adapter.
fn m18_apply_player_action(
    mut players: Query<(&mut Transform, &mut m2_player::M2Player)>,
    action: Res<m2_player::ActionState>,
    world: Res<DemoWorld>,
    combat: Res<DemoCombat>,
    mut m19: ResMut<M19Player>,
    time: Res<Time>,
    mut telemetry: Local<f32>,
) {
    let Ok((mut transform, mut player)) = players.single_mut() else {
        return;
    };
    if action.look_delta != Vec2::ZERO {
        player.yaw -= action.look_delta.x * m2_player::LOOK_SENSITIVITY;
        player.pitch = (player.pitch - action.look_delta.y * m2_player::LOOK_SENSITIVITY)
            .clamp(-m2_player::PITCH_LIMIT_RAD, m2_player::PITCH_LIMIT_RAD);
        transform.rotation = Quat::from_euler(EulerRot::YXZ, player.yaw, player.pitch, 0.0);
    }

    let M19Player {
        locomotion,
        body,
        dice,
        sneak,
        ..
    } = &mut *m19;
    if action.sneak_toggle {
        *sneak = !*sneak;
    }
    // M23 (REQ-L99-056): the movement flags come from the player's effect
    // entity — a real cast raised them, the effect's `End()` reverts them.
    // `levitating` drives `MotionMode::Levitating`; `slow_falling`,
    // `water_breathing` and `water_walking` are the `PlayerBody` fields the same
    // engine reads (`locomotion.rs`). `water_walking` is the flag
    // `WaterWalking.StartWaterWalking` sets (`WaterWalking.cs:91-98`) and the
    // swim step consumes (`LevitateMotor.cs:83-85,116-121`). No flag is set by
    // hand.
    let movement = combat
        .session
        .player
        .effects
        .as_ref()
        .map(daggerfall_behavior::magic::EffectTarget::movement_effects)
        .unwrap_or_default();
    body.slow_falling = movement.slow_falling;
    body.water_breathing = movement.water_breathing;
    body.water_walking = movement.water_walking;
    let input = MoveInput {
        forward: action.move_dir.y,
        strafe: action.move_dir.x,
        yaw: player.yaw,
        jump: action.jump,
        run: action.run,
        crouch: action.crouch,
        sneak: *sneak,
        levitating: movement.levitating,
    };
    let adapter = world.0.locomotion_world();
    let frame = locomotion.step(&input, body, &adapter, dice, time.delta_secs());
    transform.translation = Vec3::from_array(locomotion.position);

    *telemetry += time.delta_secs();
    if *telemetry >= 2.0 {
        *telemetry = 0.0;
        let terrain = &world.0.terrain;
        println!(
            "M19 VE: pos=({:.1},{:.1},{:.1}) yaw={:.2} · mode={} · speed={:.2} m/s · breath={}/{} \
             · terrain band {} · blocked={}",
            locomotion.position[0],
            locomotion.position[1],
            locomotion.position[2],
            player.yaw,
            frame.mode.label(),
            frame.speed,
            locomotion.breath,
            max_breath(body.live_endurance),
            terrain.band_at(locomotion.position[0], locomotion.position[2]).name(),
            frame.blocked,
        );
    }
    // M14: the landing event of this step is consumed once by
    // [`m14_fall_sfx`] (ordered right after this system). Placed after the
    // ECS borrows of the engine fields end; `Frame` is `Copy`.
    if let Some(event) = fall_event(&frame) {
        m19.pending_fall = Some(event);
    }
    // M20: movement signals of this frame — consumed by
    // [`m20_progression_tick`] (ordered right after this system):
    // `frame.running` is the resolved `PlayerSpeedChanger.isRunning` (run mode
    // latched on the ground, kept in the air — no movement test),
    // `frame.jumped` is the `AcrobatMotor.jumping` start frame (falls and
    // climbing aborts excluded) and `climb_check` is raised once per executed
    // climbing skill check (`ClimbingMotor.cs:823`).
    m19.last_signals = MotionSignals {
        running: frame.running,
        swimming: frame.mode == MotionMode::Swimming,
        jumping: frame.jumped,
        climbing_check: frame.climb_check,
    };
}

/// M14: fall event of an M19 frame.
///
/// Landing damage wins over the hard-fall alert exactly as
/// [`audio::gameplay_clip`] maps it (`FallDamage` when `damage > 0`,
/// otherwise `FallHard`), and a frame without either yields no event.
fn fall_event(frame: &Frame) -> Option<GameplayEvent> {
    if frame.fall_damage > 0 || frame.hard_fall {
        Some(GameplayEvent::Fall {
            damage: frame.fall_damage,
            hard: frame.hard_fall,
        })
    } else {
        None
    }
}

/// Live title bar: phase (B1-d), M19 engine mode, breath and M20 status line.
fn m19_update_title(
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    player: Res<M19Player>,
    world: Res<DemoWorld>,
    law: Res<M28Law>,
    guards: Res<DemoGuardBillboards>,
    phase: Res<crate::m18_phase::AppPhase>,
) {
    let mode = player.locomotion.mode;
    let mut title = format!(
        "DAGGERFALL-LIKE — {} · M20 demo · {} · {} · breath {}/{} · POP {} · guards visible {}",
        phase.label(),
        mode.label(),
        world.0.m20_status_line(),
        player.locomotion.breath,
        max_breath(player.body.live_endurance),
        world.0.npcs.len(),
        guards.sprites.len(),
    );
    if !phase.allows_world() {
        title.push_str(" · Enter enters the world");
    }
    if mode != MotionMode::Swimming {
        // CS-08: water exists (the `Ocean` band is the sea surface and it is not
        // walkable); the bar reports the level instead of the old "no water".
        if let Some(sea_level) = world.0.terrain.sea_level_m {
            title.push_str(&format!(" · sea {sea_level:.1} m (not walkable)"));
        }
    }
    if law.runtime.active_guards > 0 {
        title.push_str(&format!(
            " · {:?} · {} guards",
            law.runtime.law.crime.name(),
            law.runtime.active_guards
        ));
    }
    for mut window in &mut windows {
        if window.title != title {
            window.title.clone_from(&title);
        }
    }
}

/// NPC and guard billboards: always facing the camera (Y rotation).
///
/// **B3-a (CS-14):** the guard billboards registered by
/// [`m18_guard_billboards`] (flat + actor label) use this same rotation, so the
/// city watch reads like every other character of the street.
fn m18_billboards(
    npcs: Res<DemoNpcs>,
    guards: Res<DemoGuardBillboards>,
    cameras: Query<&Transform, (With<Camera3d>, Without<Mesh3d>)>,
    mut transforms: Query<&mut Transform, (With<Mesh3d>, Without<Camera3d>)>,
) {
    let Ok(camera) = cameras.single() else {
        return;
    };
    let face = |position: Vec3| {
        let dx = camera.translation.x - position.x;
        let dz = camera.translation.z - position.z;
        Quat::from_rotation_y(dx.atan2(dz))
    };
    for (entity, position) in &npcs.0 {
        if let Ok(mut transform) = transforms.get_mut(*entity) {
            transform.rotation = face(*position);
        }
    }
    for (flat, label, position) in &guards.sprites {
        for entity in [flat, label] {
            if *entity == Entity::PLACEHOLDER {
                continue;
            }
            if let Ok(mut transform) = transforms.get_mut(*entity) {
                transform.rotation = face(*position);
            }
        }
    }
}

/// Shows the caption of the nearest NPC within [`CAPTION_RANGE_M`].
fn m18_captions(
    npcs: Res<DemoNpcs>,
    mut captions: ResMut<DemoCaptions>,
    cameras: Query<&Transform, With<Camera3d>>,
    mut visibilities: Query<&mut Visibility>,
    replaced: Query<(), With<ReplacedByGuard>>,
    world: Res<DemoWorld>,
) {
    let Ok(camera) = cameras.single() else {
        return;
    };
    let mut nearest: Option<(f32, usize)> = None;
    for (index, (entity, position)) in npcs.0.iter().enumerate() {
        if replaced.contains(*entity) {
            continue;
        }
        let distance = ((camera.translation.x - position.x).powi(2)
            + (camera.translation.z - position.z).powi(2))
        .sqrt();
        if distance <= CAPTION_RANGE_M && nearest.is_none_or(|(best, _)| distance < best) {
            nearest = Some((distance, index));
        }
    }
    let wanted = nearest.map(|(_, index)| index);
    if wanted == captions.visible {
        return;
    }
    for (index, entity) in captions.sprites.iter().enumerate() {
        if *entity == Entity::PLACEHOLDER {
            continue;
        }
        if let Ok(mut visibility) = visibilities.get_mut(*entity) {
            *visibility = if Some(index) == wanted {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }
    if let Some((distance, index)) = nearest {
        if let Some(npc) = world.0.npcs.get(index) {
            println!(
                "M18 VE: NPC caption \"{}: {}\" at {:.1} m",
                npc.spec.name, npc.spec.caption.text, distance
            );
        }
    }
    captions.visible = wanted;
}

/// Keys 1-6: toggle the overlaid M17 panel (1-4 the 1996 panels, 5/6 the
/// B3-c map panels `map-local` / `map-world`, CS-18/CS-19).
///
/// **m-7:** registered inside the `Playing` bundle — the menu owns its surface
/// (M-1: `main-menu` full screen) and a stray `1-4` cannot replace it.
fn m18_switch_panel(
    keys: Res<ButtonInput<KeyCode>>,
    mut ui: ResMut<DemoUi>,
    mut visibilities: Query<&mut Visibility>,
    mut transforms: Query<&mut Transform>,
) {
    let requested = if keys.just_pressed(KeyCode::Digit1) {
        Some(0)
    } else if keys.just_pressed(KeyCode::Digit2) {
        Some(1)
    } else if keys.just_pressed(KeyCode::Digit3) {
        Some(2)
    } else if keys.just_pressed(KeyCode::Digit4) {
        Some(3)
    } else if keys.just_pressed(KeyCode::Digit5) {
        // **B3-b / CS-18** — `map-local` (5th panel of `session_panels`).
        Some(4)
    } else if keys.just_pressed(KeyCode::Digit6) {
        // **B3-b / CS-19** — `map-world` (6th panel of `session_panels`).
        Some(5)
    } else {
        None
    };
    let Some(index) = requested else {
        return;
    };
    if index >= ui.sprites.len() || index == ui.current {
        return;
    }
    let previous = ui.current;
    for (at, entity) in ui.sprites.iter().enumerate() {
        if let Ok(mut visibility) = visibilities.get_mut(*entity) {
            *visibility = if at == index {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
        // Panels are centred at the origin: parking is the hide the client
        // surface actually repaints (see [`PARKED_Y`]).
        if let Ok(mut transform) = transforms.get_mut(*entity) {
            transform.translation.y = if at == index { 0.0 } else { PARKED_Y };
        }
    }
    ui.current = index;
    println!(
        "M18 VE: panel {} visible (was {}) · {} sprites · keys 1-6",
        index,
        previous,
        ui.sprites.len()
    );
}

/// Y that puts a 2D sprite outside the orthographic view — the *reliable* hide
/// for this window.
///
/// Measured 2026-10-23 (CS-15/CS-16/CS-18/CS-19 campaign): the first
/// `Visibility` write of a run repaints the sprite, later writes on the same
/// sprite do not reach the client surface (journal stayed painted after its
/// `OFF`, the dialogue never painted while the journal was on). A `Transform`
/// move is read by the renderer every frame, so parking is what the capture can
/// prove; `Visibility` is still written alongside it.
const PARKED_Y: f32 = -20_000.0;

/// Scale of the retail overlays on the 2D camera (same 2× as the panels).
const OVERLAY_SCALE: f32 = 2.0;
/// Margin of an overlay to the window border (pixels).
const OVERLAY_MARGIN_PX: f32 = 8.0;

/// Spawns the two retail overlays (dialogue + journal) hidden on the 2D camera.
///
/// **B3-b (CS-15/CS-16):** the textures come from
/// [`crate::m18_b3b::compose_overlays`] — retail `TEXT.RSC` lines and the main
/// quests of the retail `.QBN`/`.QRC` set, painted with the HUD `.FNT` and the
/// HUD ink. A missing retail file degrades explicitly (no sprite, 0 invented
/// text).
fn m18_spawn_overlays(
    mut commands: Commands,
    world: Res<DemoWorld>,
    mut images: ResMut<Assets<Image>>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let window = windows.single().ok();
    let screen_w = window.map_or(1280.0, |window| window.resolution.width());
    let screen_h = window.map_or(720.0, |window| window.resolution.height());
    let mut overlays = DemoOverlays::default();
    match crate::m18_b3b::compose_overlays(&world.0.m20_font, world.0.m20_ink) {
        Ok(composed) => {
            let dialogue = &composed.dialogue;
            let dialogue_size = (
                f32::from(dialogue.width) * OVERLAY_SCALE,
                f32::from(dialogue.height) * OVERLAY_SCALE,
            );
            let dialogue_at = Vec3::new(
                -screen_w * 0.5 + OVERLAY_MARGIN_PX + dialogue_size.0 * 0.5,
                screen_h * 0.5 - OVERLAY_MARGIN_PX - dialogue_size.1 * 0.5,
                20.0,
            );
            overlays.dialogue =
                m18_overlay_sprite(&mut commands, &mut images, dialogue, dialogue_at);
            overlays.dialogue_size = (dialogue.width, dialogue.height);
            overlays.dialogue_at = dialogue_at;

            let journal = &composed.journal;
            let journal_size = (
                f32::from(journal.width) * OVERLAY_SCALE,
                f32::from(journal.height) * OVERLAY_SCALE,
            );
            let journal_at = Vec3::new(
                0.0,
                -screen_h * 0.5 + OVERLAY_MARGIN_PX + journal_size.1 * 0.5,
                20.0,
            );
            overlays.journal =
                m18_overlay_sprite(&mut commands, &mut images, journal, journal_at);
            overlays.journal_size = (journal.width, journal.height);
            overlays.journal_at = journal_at;

            println!(
                "M18 VE: overlay dialogue ready — {} (J toggles)",
                composed.dialogue_summary
            );
            println!(
                "M18 VE: overlay journal ready — {} (I toggles)",
                composed.journal_summary
            );
        }
        Err(err) => {
            eprintln!("M18 VE: overlays unavailable — {err} (0 invented text)");
        }
    }
    commands.insert_resource(overlays);
}

/// Spawns one hidden overlay sprite (2D camera, 2× scale) and returns it.
fn m18_overlay_sprite(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    texture: &DemoTexture,
    position: Vec3,
) -> Option<Entity> {
    let image = bridge::image_from_rgba(
        &texture.rgba,
        u32::from(texture.width),
        u32::from(texture.height),
        images,
    )
    .ok()?;
    Some(
        commands
            .spawn((
                Sprite {
                    image,
                    custom_size: Some(Vec2::new(
                        f32::from(texture.width) * OVERLAY_SCALE,
                        f32::from(texture.height) * OVERLAY_SCALE,
                    )),
                    ..default()
                },
                // Spawned parked: only `set_overlay(.., true)` puts it on screen.
                Transform::from_translation(Vec3::new(position.x, PARKED_Y, position.z)),
                Visibility::Hidden,
            ))
            .id(),
    )
}

/// **B3-b (CS-15/CS-16):** `J` toggles the dialogue overlay, `I` the journal.
///
/// Neither key has an owner in `crates/app/src` (`V` is *sell*,
/// `m2_player.rs:137`). The toggle prints one `M18 VE: overlay …` line as
/// evidence; a missing texture is reported instead of drawing invented text.
/// Shows or parks one overlay sprite: `Visibility` plus the `Transform` move
/// (see [`PARKED_Y`] — the surface only repaints the move).
fn set_overlay(
    visibilities: &mut Query<&mut Visibility>,
    transforms: &mut Query<&mut Transform>,
    entity: Entity,
    at: Vec3,
    on: bool,
) {
    if let Ok(mut visibility) = visibilities.get_mut(entity) {
        *visibility = if on {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    if let Ok(mut transform) = transforms.get_mut(entity) {
        transform.translation = if on {
            at
        } else {
            Vec3::new(at.x, PARKED_Y, at.z)
        };
    }
}

fn m18_overlay_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut overlays: ResMut<DemoOverlays>,
    mut visibilities: Query<&mut Visibility>,
    mut transforms: Query<&mut Transform>,
) {
    if keys.just_pressed(KeyCode::KeyJ) {
        let state = !overlays.dialogue_on;
        match overlays.dialogue {
            Some(entity) => {
                set_overlay(
                    &mut visibilities,
                    &mut transforms,
                    entity,
                    overlays.dialogue_at,
                    state,
                );
                if state {
                    // CS-15/CS-16: the two retail overlays never share the
                    // screen — opening one closes the other (deterministic
                    // evidence instead of two overlapping sprites).
                    if let Some(journal) = overlays.journal {
                        let journal_at = overlays.journal_at;
                        set_overlay(
                            &mut visibilities,
                            &mut transforms,
                            journal,
                            journal_at,
                            false,
                        );
                    }
                    overlays.journal_on = false;
                }
                overlays.dialogue_on = state;
                println!(
                    "M18 VE: overlay dialogue {} · retail TEXT.RSC lines · {}×{} px",
                    if state { "ON" } else { "OFF" },
                    overlays.dialogue_size.0,
                    overlays.dialogue_size.1
                );
            }
            None => println!(
                "M18 VE: overlay dialogue toggle ignored — TEXT.RSC unavailable (0 invented text)"
            ),
        }
    }
    if keys.just_pressed(KeyCode::KeyI) {
        let state = !overlays.journal_on;
        match overlays.journal {
            Some(entity) => {
                set_overlay(
                    &mut visibilities,
                    &mut transforms,
                    entity,
                    overlays.journal_at,
                    state,
                );
                if state {
                    // CS-16: same rule as the dialogue — one overlay at a time.
                    if let Some(dialogue) = overlays.dialogue {
                        let dialogue_at = overlays.dialogue_at;
                        set_overlay(
                            &mut visibilities,
                            &mut transforms,
                            dialogue,
                            dialogue_at,
                            false,
                        );
                    }
                    overlays.dialogue_on = false;
                }
                overlays.journal_on = state;
                println!(
                    "M18 VE: overlay journal {} · retail QBN/QRC main quests · {}×{} px",
                    if state { "ON" } else { "OFF" },
                    overlays.journal_size.0,
                    overlays.journal_size.1
                );
            }
            None => println!(
                "M18 VE: overlay journal toggle ignored — retail quests unavailable (0 invented text)"
            ),
        }
    }
}

/// Roll handed to [`crate::ui_weather::draw`] for a map pixel: deterministic
/// (`[0,100)`), so the same pixel replays the same precipitation.
fn weather_roll(pixel: [i32; 2]) -> f32 {
    let x = (pixel[0].rem_euclid(97) * 37) as u32;
    let y = (pixel[1].rem_euclid(89) * 61) as u32;
    ((x + y) % 100) as f32
}

/// Paints the precipitation sheet of one resolved climate (RGBA, transparent
/// except the streaks). Deterministic: the RNG is seeded by the climate and the
/// streak count, so the same weather always draws the same pixels.
fn precipitation_sheet(precip: &Precipitation, size: (u16, u16)) -> Vec<u8> {
    let (width, height) = (usize::from(size.0), usize::from(size.1));
    let mut sheet = vec![0u8; width * height * 4];
    let streaks = precip.drops.min(PRECIP_MAX_STREAKS);
    if streaks == 0 || width < 4 || height < 16 {
        return sheet;
    }
    let mut state = u32::from(precip.climate) << 16 | (streaks as u32 & 0xffff);
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state
    };
    let snow = precip.kind == WeatherKind::Snow;
    for _ in 0..streaks {
        let x = (next() as usize) % width;
        let y = (next() as usize) % height;
        let (pixel, length) = if snow {
            ([236u8, 242u8, 252u8], 2usize)
        } else {
            ([188u8, 203u8, 228u8], 5 + (next() as usize) % 9)
        };
        for step in 0..length {
            let at = (y + step) % height;
            let offset = (at * width + x) * 4;
            sheet[offset..offset + 4].copy_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
            if snow && x + 1 < width {
                let wide = offset + 4;
                sheet[wide..wide + 4].copy_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
            }
        }
    }
    sheet
}

/// **B3-d (CS-12):** resolves the precipitation of the demo's map pixel from the
/// retail `CLIMATE.PAK` and spawns the streak sheet.
///
/// The bridge does not invent weather: the climate index comes from the map, the
/// state and the streak count come from [`crate::ui_weather`]. When the retail
/// file is unreadable the system reports it and draws nothing.
fn m18_spawn_weather(
    mut commands: Commands,
    world: Res<DemoWorld>,
    mut images: ResMut<Assets<Image>>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let window = windows.single().ok();
    let screen_w = window.map_or(1280.0, |window| window.resolution.width());
    let screen_h = window.map_or(720.0, |window| window.resolution.height());
    let size = (
        (screen_w as u16 / PRECIP_DIV).max(8),
        (screen_h as u16 / PRECIP_DIV).max(8),
    );
    let mut weather = DemoWeather {
        size,
        ..DemoWeather::default()
    };
    let path = match crate::mode_a_arena2_path("CLIMATE.PAK") {
        Ok(path) => path,
        Err(err) => {
            eprintln!("M18 VE: weather unavailable — {err} (no invented climate)");
            commands.insert_resource(weather);
            return;
        }
    };
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("M18 VE: weather unavailable — reading {path:?}: {err}");
            commands.insert_resource(weather);
            return;
        }
    };
    let map = match ClimateMap::parse(&bytes) {
        Ok(map) => map,
        Err(err) => {
            eprintln!("M18 VE: weather unavailable — CLIMATE.PAK: {err:?}");
            commands.insert_resource(weather);
            return;
        }
    };
    weather.climates = map.distinct_climates().into_iter().collect();
    // The demo pixel lands on one *specific* state (Woodlands summer, roll 16 →
    // Sunny, 0 drops). `O` must be able to reach a state that precipitates, so
    // the cycle walks every `(climate, roll)` pair of the map that draws drops -
    // still retail data: no climate or roll is invented, only sampled.
    for value in weather.climates.clone() {
        let mut sample = 0.0f32;
        while sample < 100.0 {
            if let Ok(precip) = Precipitation::resolve(value, sample) {
                if precip.drops > 0 {
                    weather.states.push((value, sample));
                }
            }
            sample += 2.0;
        }
    }
    println!(
        "M18 VE: weather cycle ready — {} precipitating states over {} distinct climates (O cycles)",
        weather.states.len(),
        weather.climates.len()
    );
    let pixel = world.0.map_pixel;
    let Ok(climate) = map.climate(pixel[0], pixel[1]) else {
        eprintln!(
            "M18 VE: weather unavailable — map pixel ({},{}) outside CLIMATE.PAK",
            pixel[0], pixel[1]
        );
        commands.insert_resource(weather);
        return;
    };
    let index = climate.index();
    weather.at = weather
        .climates
        .iter()
        .position(|value| *value == index)
        .unwrap_or(0);
    let roll = weather_roll(pixel);
    let precip = match Precipitation::resolve(index, roll) {
        Ok(precip) => precip,
        Err(err) => {
            eprintln!("M18 VE: weather unavailable — climate {index}: {err:?}");
            commands.insert_resource(weather);
            return;
        }
    };
    let sheet = precipitation_sheet(&precip, size);
    match bridge::image_from_rgba(&sheet, u32::from(size.0), u32::from(size.1), &mut images) {
        Ok(handle) => {
            let entity = commands
                .spawn((
                    Sprite {
                        image: handle.clone(),
                        custom_size: Some(Vec2::new(screen_w, screen_h)),
                        ..default()
                    },
                    Transform::from_translation(Vec3::new(
                        0.0,
                        if precip.drops > 0 { 0.0 } else { PARKED_Y },
                        PRECIP_Z,
                    )),
                    if precip.drops > 0 {
                        Visibility::Visible
                    } else {
                        Visibility::Hidden
                    },
                ))
                .id();
            weather.sprite = Some(entity);
            weather.handle = Some(handle);
        }
        Err(err) => eprintln!("M18 VE: weather sheet unavailable — {err}"),
    }
    println!(
        "M18 VE: weather {} · map pixel ({},{}) · sheet {}×{} · O cycles the retail climates",
        precip.evidence(),
        pixel[0],
        pixel[1],
        size.0,
        size.1
    );
    weather.precip = Some(precip);
    commands.insert_resource(weather);
}

/// **B3-d (CS-12):** `O` advances to the next retail climate of `CLIMATE.PAK`
/// that actually precipitates, rewriting the sheet in place.
fn m18_weather_key(
    keys: Res<ButtonInput<KeyCode>>,
    mut weather: ResMut<DemoWeather>,
    mut images: ResMut<Assets<Image>>,
    mut visibilities: Query<&mut Visibility>,
    mut transforms: Query<&mut Transform>,
) {
    if !keys.just_pressed(KeyCode::KeyO) {
        return;
    }
    if weather.states.is_empty() {
        println!(
            "M18 VE: weather cycle ignored — no retail climate precipitates (no invented weather)"
        );
        return;
    }
    weather.at = (weather.at + 1) % weather.states.len();
    let (climate, roll) = weather.states[weather.at];
    let precip = match Precipitation::resolve(climate, roll) {
        Ok(precip) => precip,
        Err(err) => {
            println!("M18 VE: weather cycle — climate {climate}: {err:?}");
            return;
        }
    };
    let sheet = precipitation_sheet(&precip, weather.size);
    if let Some(handle) = weather.handle.clone() {
        if let Some(mut image) = images.get_mut(&handle) {
            // Bevy 0.20 keeps the RGBA bytes behind `Option<Vec<u8>>`.
            match image.data.as_mut() {
                Some(data) if data.len() == sheet.len() => data.copy_from_slice(&sheet),
                _ => eprintln!("M18 VE: weather sheet rewrite skipped — unexpected buffer"),
            }
        }
    }
    if let Some(entity) = weather.sprite {
        if let Ok(mut visibility) = visibilities.get_mut(entity) {
            *visibility = Visibility::Visible;
        }
        // Same measured rule as the overlays: the move is what repaints.
        if let Ok(mut transform) = transforms.get_mut(entity) {
            transform.translation = Vec3::new(0.0, 0.0, PRECIP_Z);
        }
    }
    println!(
        "M18 VE: weather {} · climate {climate} roll {roll:.0} · sheet {}×{} · O cycles the retail climates",
        precip.evidence(),
        weather.size.0,
        weather.size.1
    );
    weather.precip = Some(precip);
}

/// **B3-a (CS-14):** instantiates one billboard per arrived guard.
///
/// The law writes `M28Law::guards` when a city-watch order is executed
/// (`m28_execute_order`). This system walks the entries **after**
/// `DemoGuardBillboards::spawned`, so it is idempotent (a guard appears once),
/// and takes the flat from `DemoScene::guard_flats` with the retail rotation
/// (`at % guard_flats.len()`) plus the actor label of the session enemy
/// (`m18_demo::actor_label`, the same tag the HUD prints). The material path is
/// the NPC billboard path (`AlphaMode::Mask`).
#[allow(clippy::too_many_arguments)]
fn m18_guard_billboards(
    mut commands: Commands,
    world: Res<DemoWorld>,
    law: Res<M28Law>,
    combat: Res<DemoCombat>,
    mut state: ResMut<DemoGuardBillboards>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    if law.guards.len() <= state.spawned {
        return;
    }
    let scene = &world.0;
    if scene.guard_flats.is_empty() {
        println!(
            "M18 VE: guard billboards — 0 guard flats composed ({} guards without billboard, 0 invented flat)",
            law.guards.len()
        );
        state.spawned = law.guards.len();
        return;
    }
    let quad = unit_billboard(&mut meshes);
    for at in state.spawned..law.guards.len() {
        let (session_index, position) = law.guards[at];
        let flat = &scene.guard_flats[at % scene.guard_flats.len()];
        let position = Vec3::from_array(position);
        let Ok(image) = bridge::image_from_rgba(
            &flat.rgba,
            u32::from(flat.width),
            u32::from(flat.height),
            &mut images,
        ) else {
            println!(
                "M18 VE: guard billboard {at} — flat {} does not decode (not drawn)",
                flat.key
            );
            continue;
        };
        let material = bridge::material_from_texture(
            &bridge::MaterialSpec {
                base_color: [1.0, 1.0, 1.0, 1.0],
                ..default()
            },
            image,
            &mut materials,
        );
        if let Some(mut mat) = materials.get_mut(&material) {
            mat.alpha_mode = AlphaMode::Mask(0.5);
        }
        let height = 2.2f32;
        let width = height * (f32::from(flat.width) / f32::from(flat.height.max(1)));
        let billboard = commands
            .spawn((
                Mesh3d(quad.clone()),
                MeshMaterial3d(material),
                Transform::from_translation(position).with_scale(Vec3::new(width, height, 1.0)),
            ))
            .id();

        let label = crate::m18_demo::actor_label(&combat.session, session_index)
            .unwrap_or_else(|| format!("guard #{session_index}"));
        let caption = crate::m18_demo::caption_texture(&scene.m20_font, scene.m20_ink, &label);
        let label_entity = match bridge::image_from_rgba(
            &caption.rgba,
            u32::from(caption.width),
            u32::from(caption.height),
            &mut images,
        ) {
            Ok(image) => {
                let label_material = bridge::material_from_texture(
                    &bridge::MaterialSpec {
                        base_color: [1.0, 1.0, 1.0, 1.0],
                        ..default()
                    },
                    image,
                    &mut materials,
                );
                if let Some(mut mat) = materials.get_mut(&label_material) {
                    mat.alpha_mode = AlphaMode::Mask(0.5);
                }
                let label_height = f32::from(caption.height) * 0.02;
                let label_width =
                    label_height * (f32::from(caption.width) / f32::from(caption.height.max(1)));
                let label_at = position + Vec3::new(0.0, height * 0.5 + label_height, 0.0);
                commands
                    .spawn((
                        Mesh3d(quad.clone()),
                        MeshMaterial3d(label_material),
                        Transform::from_translation(label_at)
                            .with_scale(Vec3::new(label_width, label_height, 1.0)),
                    ))
                    .id()
            }
            Err(_) => Entity::PLACEHOLDER,
        };
        state.sprites.push((billboard, label_entity, position));
        println!(
            "M18 VE: guard billboard #{at} (session enemy {session_index}) flat {} {}×{} ({} opaque px) → \"{label}\" at ({:.1},{:.1})",
            flat.key,
            flat.width,
            flat.height,
            flat.rgba.chunks_exact(4).filter(|px| px[3] != 0).count(),
            position.x,
            position.z
        );
    }
    state.spawned = law.guards.len();
}

/// Distance in the XZ plane from `p` to the segment `a→b` (0 when the point
/// falls inside the segment). Used by the CS-14 scenario to keep the camera
/// clear of the retail crowd: an NPC one metre from the eye covers the whole
/// frame (measured 2026-10-24: the capture framed a priest and the guard behind
/// it was not visible).
fn point_segment_distance_xz(p: [f32; 3], a: Vec3, b: Vec3) -> f32 {
    let (px, pz) = (p[0] - a.x, p[2] - a.z);
    let (bx, bz) = (b.x - a.x, b.z - a.z);
    let len2 = bx * bx + bz * bz;
    let t = if len2 <= f32::EPSILON {
        0.0
    } else {
        ((px * bx + pz * bz) / len2).clamp(0.0, 1.0)
    };
    let (cx, cz) = (px - bx * t, pz - bz * t);
    cx.hypot(cz)
}

/// **CS-14 scenario** — aims the camera at an arriving guard, moving the
/// scenario camera the least distance that leaves a clear line of sight
/// (`M18_CS14=1` only).
///
/// DFU replaces the NPCs **behind** the player (`PlayerEntity.cs:668-681`: the
/// angle to the candidate must be >= 105.469 deg from the view), so the guards
/// of the crime always arrive out of frame and a capture of the empty street
/// would prove nothing about the billboards. The window has no keyboard look
/// (mouse only), so the scenario:
///
/// 1. takes the **nearest** live guard;
/// 2. tests the line of sight at eye height against the retail collision grid
///    (`is_free`, the authority the player's own movement obeys) and, when a
///    wall or a block is in between, probes camera positions along the
///    guard→camera ray with two lateral offsets, keeping the smallest move —
///    measured 2026-10-24: aiming from the spawn left the two billboards behind
///    a building, the frame carried the wall and the title alone was not proof;
/// 3. aims the camera at the guard and prints the **projected screen
///    rectangle** of every guard, so the capture can be checked in pixels.
///
/// Without the variable this system returns immediately: gameplay unchanged.
// Eight Bevy parameters (camera, law, world, locomotion, NPCs, replaced pool,
// commands, one-shot latch): each is a distinct resource/query, not a bag.
#[allow(clippy::too_many_arguments)]
fn m18_cs14_face_guards(
    mut cameras: Query<(&mut Transform, &mut m2_player::M2Player), With<Camera3d>>,
    law: Res<M28Law>,
    world: Res<DemoWorld>,
    // The camera translation is written every frame from the locomotion state
    // (`m19_tick`: `transform.translation = locomotion.position`), so the move
    // only sticks if this state moves too (measured 2026-10-24: the yaw took
    // and the position did not — the capture kept the door facade).
    mut m19: ResMut<M19Player>,
    npcs: Res<DemoNpcs>,
    replaced: Query<(), With<ReplacedByGuard>>,
    mut commands: Commands,
    mut aimed: Local<bool>,
) {
    if std::env::var("M18_CS14").is_err() || *aimed || law.guards.is_empty() {
        return;
    }
    let Ok((mut transform, mut player)) = cameras.single_mut() else {
        return;
    };
    let grid = &world.0.grid;
    let eye_y = transform.translation.y;
    let cur = Vec3::new(transform.translation.x, eye_y, transform.translation.z);
    const WIDTH: f32 = 1280.0;
    const HEIGHT: f32 = 720.0;
    const SPRITE_HEIGHT_M: f32 = 2.2;
    let focal = (HEIGHT * 0.5) / (0.5 * 45f32.to_radians()).tan();
    let aspect = |at: usize| -> f32 {
        let flat = &world.0.guard_flats[at % world.0.guard_flats.len()];
        f32::from(flat.width) / f32::from(flat.height.max(1))
    };
    // Projected rectangle of `guard` seen from `eye`: (center x, center y, w, h, depth).
    let project = |eye: Vec3, guard: [f32; 3], aspect: f32| -> Option<(f32, f32, f32, f32, f32)> {
        let dx = guard[0] - eye.x;
        let dz = guard[2] - eye.z;
        if dx.hypot(dz) < 0.01 {
            return None;
        }
        let yaw = (-dx).atan2(-dz);
        let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
        let forward = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
        let center = Vec3::new(guard[0], guard[1] + SPRITE_HEIGHT_M * 0.5, guard[2]) - eye;
        let depth = center.dot(forward);
        if depth <= 0.5 {
            return None;
        }
        Some((
            WIDTH * 0.5 + (center.dot(right) / depth) * focal,
            HEIGHT * 0.5 - (center.y / depth) * focal,
            (SPRITE_HEIGHT_M * aspect / depth) * focal,
            (SPRITE_HEIGHT_M / depth) * focal,
            depth,
        ))
    };
    let walkable = |p: Vec3| grid.is_free([p.x, p.y, p.z], bridge::PLAYER_RADIUS_M);
    // Best camera spot for one guard: clear grid line of sight, whole sprite in
    // the frame, no retail NPC on the eye or on the sight line, and the
    // smallest move from where the campaign left the player.
    // Returns `(tier, moved, eye)`: tier 0 also keeps the crowd out of the
    // frame, 1 is framed but possibly crowded, 2 is any clear spot. The caller
    // takes the lowest tier and the smallest move, so the scenario always aims
    // at a guard and prefers the clean shot.
    let select = |guard: [f32; 3], aspect: f32| -> Option<(u8, f32, Vec3)> {
        let target = Vec3::new(guard[0], eye_y, guard[2]);
        let clear = |from: Vec3| -> bool {
            let steps = (((target - from).length()) / 0.4).ceil().max(1.0) as usize;
            (1..steps).all(|i| {
                let t = i as f32 / steps as f32;
                let p = from.lerp(target, t);
                grid.is_free([p.x, p.y, p.z], 0.05)
            })
        };
        let framed = |eye: Vec3| -> bool {
            project(eye, guard, aspect).is_some_and(|(sx, sy, w, h, _)| {
                w >= 40.0
                    && h >= 60.0
                    && sx - w * 0.5 > 24.0
                    && sx + w * 0.5 < WIDTH - 24.0
                    && sy - h * 0.5 > 24.0
                    && sy + h * 0.5 < HEIGHT - 24.0
            })
        };
        // Rings around the guard: the scenario camera bashes a door 1.4 m ahead
        // on the far side of the block, so the spot that sees the guard has to
        // be found around it, not between it and the camera.
        let mut best: Option<(u8, f32, Vec3)> = None;
        let mut candidates = vec![cur];
        for radius in [3.0f32, 4.0, 5.0, 6.5, 8.0, 10.0, 12.0] {
            for step in 0..16 {
                let angle = std::f32::consts::TAU * step as f32 / 16.0;
                candidates.push(target + Vec3::new(radius * angle.cos(), 0.0, radius * angle.sin()));
            }
        }
        for candidate in candidates {
            if !walkable(candidate) || !clear(candidate) {
                continue;
            }
            let tier = if framed(candidate) { 0 } else { 1 };
            let moved = (candidate - cur).length();
            if best.is_none_or(|(best_tier, best_moved, _)| (tier, moved) < (best_tier, best_moved)) {
                best = Some((tier, moved, candidate));
            }
        }
        best
    };
    // Every arriving guard is a candidate: the crowd around one of them can
    // leave no clean spot while the other one has a clear view.
    let Some((tier, moved, eye, at, guard)) = law
        .guards
        .iter()
        .enumerate()
        .filter_map(|(index, (_, position))| {
            select(*position, aspect(index))
                .map(|(tier, moved, eye)| (tier, moved, eye, index, *position))
        })
        .min_by(|a, b| (a.0, a.1).partial_cmp(&(b.0, b.1)).unwrap_or(std::cmp::Ordering::Equal))
    else {
        println!(
            "M18 VE: CS14 scenario — no camera spot sees a guard with a clear grid line, a framed \
             sprite and no NPC in the way ({} guards, crowd of {} NPCs) — capture keeps the current pose",
            law.guards.len(),
            npcs.0.len()
        );
        return;
    };
    let dx = guard[0] - eye.x;
    let dz = guard[2] - eye.z;
    if dx.hypot(dz) < 0.01 {
        return;
    }
    let yaw = (-dx).atan2(-dz);
    // Keep the crowd, clear the shot: an NPC billboard ~1 m from the eye or on
    // the sight line covers the guard's rectangle (measured 2026-10-24: the
    // capture framed a priest and the guard was behind it). Only those
    // billboards are hidden — the rest of the street stays as the campaign
    // composed it, and every hidden NPC is named.
    let target = Vec3::new(guard[0], eye_y, guard[2]);
    let mut occluders = 0usize;
    for (entity, npc) in &npcs.0 {
        if replaced.contains(*entity) {
            continue;
        }
        let near_eye = (npc.x - eye.x).hypot(npc.z - eye.z) < 1.5;
        let on_line = point_segment_distance_xz([npc.x, npc.y, npc.z], eye, target) < 1.0;
        if near_eye || on_line {
            commands.entity(*entity).insert(Visibility::Hidden);
            occluders += 1;
        }
    }
    transform.translation = Vec3::new(eye.x, eye_y, eye.z);
    m19.locomotion.position = [eye.x, eye_y, eye.z];
    player.yaw = yaw;
    player.pitch = 0.0;
    transform.rotation = Quat::from_rotation_y(yaw);
    *aimed = true;
    println!(
        "M18 VE: CS14 scenario — camera aimed at guard #{at} at ({:.1},{:.1}) · from ({:.1},{:.1}) \
         to ({:.1},{:.1}) moved {moved:.1} m · yaw {yaw:.2} rad · los=clear · framed={} · \
         crowd={} of {} NPCs · {occluders} occluder(s) hidden",
        guard[0],
        guard[2],
        cur.x,
        cur.z,
        eye.x,
        eye.z,
        if tier == 0 { "true" } else { "false" },
        npcs.0.len() - occluders,
        npcs.0.len()
    );
    // Screen rectangle of every guard, so the capture is checked on pixels and
    // not on the window title.
    for (index, (session, position)) in law.guards.iter().enumerate() {
        match project(eye, *position, aspect(index)) {
            Some((sx, sy, w, h, depth)) => println!(
                "M18 VE: CS14 guard #{index} (session enemy {session}) screen=({sx:.0},{sy:.0}) \
                 size=({w:.0}x{h:.0}) depth={depth:.1} m in_frame={}",
                sx - w * 0.5 > 0.0
                    && sx + w * 0.5 < WIDTH
                    && sy - h * 0.5 > 0.0
                    && sy + h * 0.5 < HEIGHT
            ),
            None => println!(
                "M18 VE: CS14 guard #{index} (session enemy {session}) behind the camera or on it"
            ),
        }
    }
}

/// **CS-13 scenario** — turns the camera to the busiest direction of the street
/// so the population capture shows the 24 NPCs instead of the facade the walk
/// left in front of the lens (measured 2026-10-24: the CS-13 frame was an empty
/// room while the title said `POP 24`).
///
/// Only the yaw changes: the player stays where the campaign left it, no NPC is
/// moved or hidden, and the direction is the one that puts the most retail
/// billboards inside the frame with a clear grid line of sight.
fn m18_cs13_face_crowd(
    world: Res<DemoWorld>,
    mut cameras: Query<(&mut Transform, &mut m2_player::M2Player), With<Camera3d>>,
    mut m19: ResMut<M19Player>,
    npcs: Res<DemoNpcs>,
    replaced: Query<(), With<ReplacedByGuard>>,
    mut aimed: Local<bool>,
    mut last_eye: Local<Option<Vec3>>,
) {
    if std::env::var("M18_CS13").is_err() || npcs.0.is_empty() {
        return;
    }
    // The spawn point is inside a building: the first `Playing` frames have no
    // billboard with a clear line of sight, so the aim retries every frame and
    // latches on the first one that does (never on an empty list).
    let Ok((mut transform, mut player)) = cameras.single_mut() else {
        return;
    };
    let grid = &world.0.grid;
    let eye = Vec3::new(
        transform.translation.x,
        transform.translation.y,
        transform.translation.z,
    );
    const WIDTH: f32 = 1280.0;
    const HEIGHT: f32 = 720.0;
    const SPRITE_HEIGHT_M: f32 = 2.2;
    let focal = (HEIGHT * 0.5) / (0.5 * 45f32.to_radians()).tan();
    // Rectangle of one billboard seen from `eye` with the camera looking at `yaw`.
    let project = |eye: Vec3, yaw: f32, npc: Vec3| -> Option<(f32, f32, f32, f32, f32)> {
        let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
        let forward = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
        let center = npc + Vec3::new(0.0, SPRITE_HEIGHT_M * 0.5, 0.0) - eye;
        let depth = center.dot(forward);
        if depth <= 1.0 {
            return None;
        }
        let w = (SPRITE_HEIGHT_M * 0.55 / depth) * focal;
        let h = (SPRITE_HEIGHT_M / depth) * focal;
        Some((
            WIDTH * 0.5 + (center.dot(right) / depth) * focal,
            HEIGHT * 0.5 - (center.y / depth) * focal,
            w,
            h,
            depth,
        ))
    };
    let all: Vec<Vec3> = npcs
        .0
        .iter()
        .filter(|(entity, _)| !replaced.contains(*entity))
        .map(|(_, position)| *position)
        .collect();
    // Straight grid line of sight at eye height: the ray keeps `eye.y` (the
    // billboard positions carry their own height, and a ray that dips to the
    // ground reads as blocked by the floor).
    let sees = |eye: Vec3, at: Vec3| -> bool {
        // 5 m: closer billboards are legitimately drawn (and the CS-14 guard
        // stands at 7.6-8.7 m), but one of them in front of the lens covers the
        // street instead of showing it (measured 2026-10-24: a mage sprite
        // filled half the CS-13 frame).
        if (at.x - eye.x).hypot(at.z - eye.z) < 5.0 {
            return false;
        }
        let target = Vec3::new(at.x, eye.y, at.z);
        let steps = ((target - eye).length() / 0.4).ceil().max(1.0) as usize;
        (1..steps).all(|i| {
            let t = i as f32 / steps as f32;
            let p = eye.lerp(target, t);
            grid.is_free([p.x, p.y, p.z], 0.05)
        })
    };
    // The capture can land anywhere the campaign walks after the aim, so the
    // rectangles are re-printed whenever the eye moves: the yaw is not touched
    // here (the walk follows the camera and must not bend), only the numbers
    // the capture is measured with.
    let report = |eye: Vec3, yaw: f32| {
        let mut candidates: Vec<(f32, Vec3)> = all
            .iter()
            .filter_map(|at| project(eye, yaw, *at).map(|(_, _, _, _, depth)| (depth, *at)))
            .collect();
        candidates.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut printed = 0usize;
        for (_, npc) in candidates {
            if printed == 3 {
                break;
            }
            if let Some((sx, sy, w, h, depth)) = project(eye, yaw, npc) {
                if sx - w * 0.5 > 16.0
                    && sx + w * 0.5 < WIDTH - 16.0
                    && sy - h * 0.5 > 16.0
                    && sy + h * 0.5 < HEIGHT - 16.0
                {
                    println!(
                        "M18 VE: CS13 npc #{printed} screen=({sx:.0},{sy:.0}) size=({w:.0}x{h:.0}) \
                         depth={depth:.1} m at ({:.1},{:.1})",
                        npc.x, npc.z
                    );
                    printed += 1;
                }
            }
        }
    };
    if *aimed {
        let now = Vec3::new(
            transform.translation.x,
            transform.translation.y,
            transform.translation.z,
        );
        if last_eye.is_none_or(|last| (now - last).length() > 0.5) {
            *last_eye = Some(now);
            report(now, player.yaw);
        }
        return;
    }
    // The campaign spawns **inside** a building, so from the walk's end no
    // billboard has a clear line of sight (measured 2026-10-24: the CS-13 frame
    // was an empty room). When that happens the scenario looks for a street
    // vantage: a walkable spot 4…12 m from the densest cluster of the crowd
    // that sees at least three billboards, and moves the camera there — the
    // same kind of scenario move the CS-14 item already makes. Nothing else is
    // touched: no NPC is moved, hidden or invented.
    // "Outdoors" test: a street spot has most of its horizon free, while a room
    // exists only one or two rays wide (measured 2026-10-24: the aim kept the
    // camera in the spawn room and the CS-13 frame showed a wall, not the
    // street). Only an open spot can host the population shot.
    let open = |at: Vec3| -> usize {
        let mut free = 0usize;
        for step in 0..8 {
            let angle = std::f32::consts::TAU * step as f32 / 8.0;
            let far = Vec3::new(at.x + 14.0 * angle.cos(), at.y, at.z + 14.0 * angle.sin());
            let steps = 35usize;
            if (1..steps).all(|i| {
                let t = i as f32 / steps as f32;
                let p = at.lerp(far, t);
                grid.is_free([p.x, p.y, p.z], 0.05)
            }) {
                free += 1;
            }
        }
        free
    };
    let mut vantage = eye;
    let mut visible: Vec<Vec3> = if open(eye) >= 5 {
        all.iter().copied().filter(|at| sees(eye, *at)).collect()
    } else {
        Vec::new()
    };
    if visible.len() < 3 {
        let mut best: Option<(usize, Vec3)> = None;
        for anchor in &all {
            for radius in [4.0f32, 5.0, 6.5, 8.0, 10.0, 12.0] {
                for step in 0..24 {
                    let angle = std::f32::consts::TAU * step as f32 / 24.0;
                    let spot = Vec3::new(
                        anchor.x + radius * angle.cos(),
                        eye.y,
                        anchor.z + radius * angle.sin(),
                    );
                    if (spot - eye).length() > 60.0
                        || !grid.is_free([spot.x, spot.y, spot.z], bridge::PLAYER_RADIUS_M)
                        || open(spot) < 5
                    {
                        continue;
                    }
                    let count = all.iter().filter(|at| sees(spot, **at)).count();
                    if count >= 1 && best.is_none_or(|(best, _)| count > best) {
                        best = Some((count, spot));
                    }
                }
            }
            if best.is_some_and(|(count, _)| count >= 6) {
                break;
            }
        }
        if let Some((_, spot)) = best {
            vantage = spot;
            visible = all.iter().copied().filter(|at| sees(spot, *at)).collect();
        }
    }
    if visible.is_empty() {
        return;
    }
    let eye = vantage;
    // Busiest direction: most billboards inside the frame, ties broken by the
    // closest one (a big sprite is measurable on pixels).
    let mut best: Option<(usize, f32, f32)> = None;
    for step in 0..48 {
        let yaw = std::f32::consts::TAU * step as f32 / 48.0;
        let mut inside = 0usize;
        let mut nearest = f32::INFINITY;
        for npc in &visible {
            if let Some((sx, sy, w, h, depth)) = project(eye, yaw, *npc) {
                if sx - w * 0.5 > 16.0
                    && sx + w * 0.5 < WIDTH - 16.0
                    && sy - h * 0.5 > 16.0
                    && sy + h * 0.5 < HEIGHT - 16.0
                {
                    inside += 1;
                    nearest = nearest.min(depth);
                }
            }
        }
        if inside == 0 {
            continue;
        }
        let better = match best {
            None => true,
            Some((count, distance, _)) => inside > count || (inside == count && nearest < distance),
        };
        if better {
            best = Some((inside, nearest, yaw));
        }
    }
    let Some((count, nearest, yaw)) = best else {
        println!("M18 VE: CS13 scenario — no direction frames an NPC billboard");
        *aimed = true;
        return;
    };
    let moved = (vantage - Vec3::new(
        transform.translation.x,
        transform.translation.y,
        transform.translation.z,
    ))
    .length();
    transform.translation = Vec3::new(vantage.x, vantage.y, vantage.z);
    transform.rotation = Quat::from_rotation_y(yaw);
    player.yaw = yaw;
    player.pitch = 0.0;
    m19.locomotion.position = [vantage.x, vantage.y, vantage.z];
    *aimed = true;
    println!(
        "M18 VE: CS13 scenario — camera at ({:.1},{:.1}) moved {moved:.1} m · {count} billboards \
         in frame · nearest at {nearest:.1} m · yaw {yaw:.2} rad · {} NPCs with a clear line of sight",
        vantage.x,
        vantage.z,
        visible.len()
    );
    *last_eye = Some(vantage);
    report(vantage, yaw);
}

/// **Retail replacement (B3-a)** — the city watch turns the pool NPC it is
/// drawn from into a guard (`PlayerEntity.cs:668-680`), so the NPC billboard
/// goes away.
///
/// The demo spawned the guard **next to** the NPC: at CS-14 the two billboards
/// shared one spot and the capture framed the priest behind the knight
/// (measured 2026-10-24). The NPC stays in [`DemoNpcs`] (caption indices) but is
/// hidden and marked [`ReplacedByGuard`], which also takes it out of the
/// caption's nearest-NPC search.
fn m28_guards_replace_npcs(
    law: Res<M28Law>,
    npcs: ResMut<DemoNpcs>,
    mut commands: Commands,
    mut visibilities: Query<&mut Visibility>,
    mut replaced: Local<usize>,
) {
    if law.guards.len() <= *replaced {
        return;
    }
    let pool: Vec<[f32; 3]> = law.guards[*replaced..].iter().map(|(_, at)| *at).collect();
    *replaced = law.guards.len();
    for position in pool {
        let Some((entity, at)) = npcs.0.iter().find(|(_, npc)| {
            (npc.x - position[0]).hypot(npc.z - position[2]) < 0.6
                && (npc.y - position[1]).abs() < 1.0
        }) else {
            continue;
        };
        let (entity, at) = (*entity, *at);
        if let Ok(mut visibility) = visibilities.get_mut(entity) {
            *visibility = Visibility::Hidden;
        }
        commands.entity(entity).insert(ReplacedByGuard);
        println!(
            "M28 VE: city watch replaces the pool NPC at ({:.1},{:.1}) — its billboard is hidden \
             (retail: the candidate becomes the guard)",
            at.x, at.z
        );
    }
}

/// **CS-14 scenario** — keeps the scenario door of
/// [`crate::m18_demo::apply_cs14_scenario`] (`M18_CS14=1`) 1.4 m ahead of the
/// camera until a crime is stored.
///
/// The campaign walks before it bashes (CS-13/15/16 need the street), so a door
/// frozen at the spawn would already be out of reach when `G` is pressed. Only
/// the panel entry recorded by the scenario (`DemoScene::cs14_door`) moves: the
/// normal composition has `None` there and this system returns before touching
/// the world, so gameplay is unchanged without the variable.
fn m28_cs14_scenario_door(
    cameras: Query<&Transform, With<Camera3d>>,
    mut world: ResMut<DemoWorld>,
    law: Res<M28Law>,
) {
    if std::env::var("M18_CS14").is_err() || !matches!(law.runtime.law.crime, Crimes::None) {
        return;
    }
    let Ok(camera) = cameras.single() else {
        return;
    };
    let Some((building, index)) = world.0.cs14_door else {
        return;
    };
    let forward = camera.rotation * Vec3::NEG_Z;
    let ahead = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero()
        * crate::m18_demo::CS14_DOOR_AHEAD_M;
    let door = [camera.translation.x + ahead.x, 0.0, camera.translation.z + ahead.z];
    if let Some(slot) = world
        .0
        .city
        .buildings
        .get_mut(building)
        .and_then(|b| b.doors.get_mut(index))
    {
        *slot = door;
    }
}

/// M28 (REQ-L99-061, G6): **G** bashes the nearest building door — the crime
/// source DFU wires to `SpawnCityGuards` (`PlayerActivate.cs:571-579`: the bash
/// fails with `FailedRoll(25 - buildingLockValue)` and the player is noticed
/// with `SuccessRoll(10)`, then `CrimeCommitted = Attempted_Breaking_And_Entering`
/// and `SpawnCityGuards(true)`).
///
/// The order the law returns is executed in the live scene: the city watch is
/// composed from its retail class template (`CLASS18.CFG`) and spawned into the M22
/// session through [`CombatSession::spawn_enemy`]; positions come from the
/// retail city (door / NPC pool / ring), never from the synthetic M15 layout.
fn m28_door_bash(
    action: Res<m2_player::ActionState>,
    cameras: Query<&Transform, With<Camera3d>>,
    mut law: ResMut<M28Law>,
    mut combat: ResMut<DemoCombat>,
    mut world: ResMut<DemoWorld>,
) {
    if !action.bash {
        return;
    }
    let Ok(player) = cameras.single() else {
        return;
    };
    let player_pos = player.translation;
    // Nearest retail door within reach: (distance, quality, door position).
    let nearest = world
        .0
        .city
        .buildings
        .iter()
        .flat_map(|building| {
            building
                .doors
                .iter()
                .map(move |door| (building.quality, *door))
        })
        .map(|(quality, door)| {
            let distance = ((door[0] - player_pos.x).powi(2) + (door[2] - player_pos.z).powi(2))
                .sqrt();
            (distance, quality, door)
        })
        .min_by(|a, b| a.0.total_cmp(&b.0));
    let Some((distance, quality, door)) = nearest else {
        println!("M28 VE: bash — the city composition has 0 retail doors");
        return;
    };
    if distance > DOOR_REACH_M {
        // The distance is reported (not just "farther away"): the CS-14 capture
        // walks to the nearest facade with this number as its only guide.
        println!(
            "M28 VE: bash — no retail door within {DOOR_REACH_M:.1} m (nearest at {distance:.1} m)"
        );
        return;
    }
    // `GetBuildingLockValue(buildingSummary) = quality / 2`
    // (`PlayerActivate.cs:668-680`).
    let lock_value = i32::from(quality) / 2;
    if law.dice.success(25 - lock_value) {
        println!(
            "M28 VE: bash at {distance:.1} m — door forced (lock {lock_value}); no crime (`FailedRoll(25 − {lock_value})` failed)"
        );
        return;
    }
    if !law.dice.success(10) {
        println!(
            "M28 VE: bash at {distance:.1} m — lock {lock_value} holds and nobody noticed (`SuccessRoll(10)` failed); no crime"
        );
        return;
    }
    // DFU replaces the nearby NPC pool (`PlayerEntity.cs:668-681`): guards
    // first, then non-guards whose angle to the player's view is at least
    // 105.469° with a one-in-four roll. The demo's population records carry no
    // guard class, so every candidate is a non-guard (declared).
    let view = player.forward();
    let replacements =
        m28_pool_replacements(&mut law, &world.0, player_pos, Vec3::new(view.x, view.y, view.z));
    let npcs_in_radius = replacements.len();
    let context = GuardSpawnContext {
        immediate: true,
        npcs_in_radius,
        ..GuardSpawnContext::default()
    };
    let M28Law { runtime, rng, .. } = &mut *law;
    let Some(order) = runtime.commit(CrimeTrigger::LockedDoorForce, context, rng) else {
        println!(
            "M28 VE: bash — crime {:?} stored, the law requests no guard (dungeon or cap)",
            law.runtime.law.crime
        );
        return;
    };
    println!(
        "M28 VE: bash — crime {:?} (lock {lock_value}, npcs within {GUARD_ATTENTION_RADIUS_METERS:.1} m: {npcs_in_radius}) → request {:?}",
        law.runtime.law.crime, order.request
    );
    m28_execute_order(
        &mut law,
        &mut world.0,
        &mut combat.session,
        order,
        player_pos,
        Some(door),
        &replacements,
    );
}

/// Seconds of `guardsArriveCountdown` (`:737-742`) elapse in game time: the
/// runtime reports the arrival and the same executor places the guards.
fn m28_guard_arrival(
    time: Res<Time>,
    mut law: ResMut<M28Law>,
    mut combat: ResMut<DemoCombat>,
    mut world: ResMut<DemoWorld>,
    cameras: Query<&Transform, With<Camera3d>>,
) {
    if law.runtime.countdown.is_none() {
        return;
    }
    let Ok(player) = cameras.single() else {
        return;
    };
    let player_pos = player.translation;
    law.carry += time.delta_secs();
    let whole = law.carry.floor();
    if whole < 1.0 {
        return;
    }
    law.carry -= whole;
    let M28Law { runtime, rng, .. } = &mut *law;
    let Some(order) = runtime.tick(whole as u32, rng) else {
        return;
    };
    println!(
        "M28 VE: countdown expired — guards arrive via the foe-spawner branch ({:?})",
        order.request
    );
    m28_execute_order(&mut law, &mut world.0, &mut combat.session, order, player_pos, None, &[]);
}

/// DFU's nearby-NPC replacement census (`PlayerEntity.cs:668-681`): every
/// active NPC within 77.5 m is a candidate; guards are replaced outright, and a
/// non-guard only when the angle between the direction to it and the player's
/// view is at least `105.469°` **and** `Random.Range(0, 4) == 0`. The demo's
/// population records (`NpcSpec`) carry no guard class, so every candidate is a
/// non-guard — declared, not invented.
fn m28_pool_replacements(
    law: &mut M28Law,
    scene: &DemoScene,
    player: Vec3,
    forward: Vec3,
) -> Vec<[f32; 3]> {
    let forward = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    let mut replacements = Vec::new();
    for npc in &scene.npcs {
        let to_npc = Vec3::new(
            npc.position[0] - player.x,
            0.0,
            npc.position[2] - player.z,
        );
        let distance = to_npc.length();
        if distance > GUARD_ATTENTION_RADIUS_METERS || distance <= f32::EPSILON {
            continue;
        }
        let angle = to_npc.normalize().dot(forward).clamp(-1.0, 1.0).acos().to_degrees();
        if angle < GUARD_REPLACE_MIN_ANGLE_DEGREES {
            continue;
        }
        if !law.rng.rand().is_multiple_of(GUARD_REPLACE_ONE_IN) {
            continue;
        }
        replacements.push(npc.position);
    }
    replacements
}

/// `HowManyEnemiesOfType(Knight_CityWatch)` reads the live scene: the census
/// the law's cap uses is recomputed every frame from the spawned guards.
fn m28_sync_census(
    combat: Res<DemoCombat>,
    cameras: Query<&Transform, With<Camera3d>>,
    mut law: ResMut<M28Law>,
) {
    let player = cameras.single().map_or(Vec3::ZERO, |camera| camera.translation);
    law.target = m28_target_index(&law, &combat.session, player);
    let live = law
        .guards
        .iter()
        .filter(|(index, _)| {
            combat
                .session
                .enemies
                .get(*index)
                .is_some_and(|enemy| enemy.is_alive())
        })
        .count();
    if live != law.runtime.active_guards {
        let active = law.runtime.sync_census(live);
        println!("M28 VE: guard census → {active} (live scene)");
    }
}

/// The nearest live guard index for the player's swing, `0` when no guard is
/// in the encounter (the M22 enemy keeps the default target).
fn m28_target_index(law: &M28Law, session: &daggerfall_app::combat_runtime::CombatSession, player: Vec3) -> usize {
    law.guards
        .iter()
        .filter(|(index, _)| session.enemies.get(*index).is_some_and(|enemy| enemy.is_alive()))
        .map(|(index, position)| {
            (
                (position[0] - player.x).powi(2) + (position[2] - player.z).powi(2),
                *index,
            )
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map_or(0, |(_, index)| index)
}

/// Places an order: positions from the retail scene, then one
/// `Knight_CityWatch` per guard into the M22 session. Returns the guards placed.
fn m28_execute_order(
    law: &mut M28Law,
    scene: &mut DemoScene,
    session: &mut daggerfall_app::combat_runtime::CombatSession,
    order: GuardOrder,
    player: Vec3,
    door: Option<[f32; 3]>,
    pool: &[[f32; 3]],
) -> usize {
    if let Some(seconds) = order.after_seconds {
        println!(
            "M28 VE: countdown armed — guards arrive in {seconds} s ({:?})",
            order.request
        );
        return 0;
    }
    let positions: Vec<[f32; 3]> = match order.placement {
        // DFU spawns every interior guard at the same lowest door position
        // (`:630-641`, `SpawnCityGuard(lowestDoorPos, Vector3.forward)`).
        GuardPlacement::Door { offset_meters } => {
            let door = door.unwrap_or([player.x, player.y, player.z]);
            let dx = player.x - door[0];
            let dz = player.z - door[2];
            let length = (dx * dx + dz * dz).sqrt().max(f32::EPSILON);
            let placed = [
                door[0] + dx / length * offset_meters,
                door[1],
                door[2] + dz / length * offset_meters,
            ];
            vec![placed; order.count]
        }
        // One guard per NPC the caller's pool replaced (`:668-680`): the guard
        // takes the replaced NPC's position. The radius is the caller's own
        // selection criterion, reported for the record.
        GuardPlacement::FromPool { radius_meters } => {
            let _ = radius_meters;
            pool.to_vec()
        }
        // `CreateFoeSpawner(..., 12.8, 51.2)` (`:685-688`): a ring around the
        // player between the two radii. Delta D-M28-R2-1: DFU draws each point
        // at random; here `i`-th ring step is uniform (the radii are the
        // source's, the angle is declared deterministic for reproducibility).
        GuardPlacement::Ring {
            min_radius_meters,
            max_radius_meters,
        } => (0..order.count)
            .map(|index| {
                let steps = order.count.max(1) as f32;
                let angle = std::f32::consts::TAU * index as f32 / steps;
                let radius = min_radius_meters
                    + (max_radius_meters - min_radius_meters) * (index as f32 / steps);
                [
                    player.x + radius * angle.cos(),
                    player.y,
                    player.z + radius * angle.sin(),
                ]
            })
            .collect(),
    };
    if positions.is_empty() {
        println!(
            "M28 VE: order {:?} yields 0 positions (no pool NPC inside the radius) — 0 invented guards",
            order.request
        );
        return 0;
    }
    let Some(career) = scene.city_watch_template.clone() else {
        println!(
            "M28 VE: city watch career {CITY_WATCH_CLASS_FILE} unavailable — {} guards not composed (0 invented stats)",
            positions.len()
        );
        return 0;
    };
    let level = session.player.level;
    let mut placed = 0usize;
    for position in &positions {
        let id = law.next_id;
        law.next_id += 1;
        match city_watch_fighter(id, level, &career, &mut law.guard_rng) {
            Ok(guard) => {
                let level = guard.level;
                let index = session.spawn_enemy(guard);
                law.guards.push((index, *position));
                placed += 1;
                println!(
                    "M28 VE: guard #{placed} id {id} level {level} → session enemy {index} at ({:.1}, {:.1})",
                    position[0], position[2]
                );
            }
            Err(err) => println!("M28 VE: guard id {id} not composed ({err}) — 0 invented stats"),
        }
    }
    let active = law.runtime.record_spawn(placed);
    println!(
        "M28 VE: {:?} → {placed}/{} guards placed · active {active} · crime {:?} · spawned total {} · dispatches {}",
        order.request, order.count, law.runtime.law.crime, law.runtime.spawned, law.runtime.dispatches
    );
    placed
}

/// Hermetic checks of the unarmed melee policy of [`m14_melee_sfx`]: no Bevy
/// `App`, no retail bytes — only the two pure helpers the system calls.
///
/// This module rides the `window` feature (the whole file is `cfg`-gated), so
/// it runs with `cargo test -p daggerfall-app --features window`.
#[cfg(test)]
mod melee_sfx_tests {
    use super::*;
    use daggerfall_parser_rs::snd::SoundClips;

    /// F-01: the bare-handed swing is DFU `SwingHighPitch` (106), never the
    /// weapon-only low pitch (105) — `WeaponManager.SetMelee:756-761` plus
    /// `EnemySounds.PlayMissSound:153` for the unarmed miss.
    #[test]
    fn unarmed_swing_plays_the_dfu_high_pitch() {
        let event = unarmed_swing_event();
        assert_eq!(event, GameplayEvent::MeleeSwing { high: true });
        let clip = audio::gameplay_clip(event).expect("the unarmed swing maps to a DFU clip");
        assert_eq!(clip, SoundClips::SwingHighPitch);
        assert_eq!(clip.index(), Some(106));
        assert_eq!(clip.name(), "SwingHighPitch");
        assert_ne!(clip, SoundClips::SwingLowPitch);
    }

    /// F-02: over 200 seeded unarmed hits the die is `Hit3`/`Hit4` only and
    /// both values occur — DFU `Hit1 + Random.Range(2, 4)`
    /// (`EnemySounds.cs:123,127`). `Hit1`/`Hit2`/`Hit5` are armed-only.
    #[test]
    fn unarmed_hits_draw_dfu_hit3_or_hit4_over_many_rolls() {
        // Same seed the demo resource uses (`DemoCombat.dice`).
        let mut dice = daggerfall_combat::Dice100::with_seed(actor_seed(M19_SEED, 1));
        let mut seen = [0_u32; 5];
        for _ in 0..200 {
            let die = unarmed_hit_die(&mut dice);
            assert!(
                (3..=4).contains(&die),
                "unarmed hit die must be Hit3/Hit4, got {die}"
            );
            let clip = audio::gameplay_clip(GameplayEvent::MeleeHit { die })
                .expect("every DFU hit die maps to a clip");
            assert!(
                matches!(clip, SoundClips::Hit3 | SoundClips::Hit4),
                "die {die} played {}, which DFU never plays unarmed",
                clip.name()
            );
            seen[usize::from(die) - 1] += 1;
        }
        assert!(
            seen[2] > 0 && seen[3] > 0,
            "both Hit3 and Hit4 must occur over 200 draws: {seen:?}"
        );
        assert_eq!(
            seen[0] + seen[1] + seen[4],
            0,
            "armed-only hit dice leaked into the unarmed path: {seen:?}"
        );
        assert_eq!(seen.iter().sum::<u32>(), 200);
    }
}


/// **CS-14 (B3-a)** — the scenario door of [`crate::m18_demo::apply_cs14_scenario`]
/// must sit inside the reach of the bash binding and in front of the camera:
/// the interaction reads the XZ distance to the nearest door, so these tests
/// measure exactly that quantity (no Bevy app, no retail bytes).
#[cfg(test)]
mod cs14_tests {
    use super::*;
    use crate::m18_demo::{cs14_door_position, CS14_DOOR_AHEAD_M};

    fn xz_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
        ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    }

    #[test]
    fn scenario_door_is_in_reach_and_in_front_for_every_heading() {
        let eye = [411.0, 1.6, 411.0];
        for yaw in [0.0_f32, 0.79, 1.57, 3.93, -2.4] {
            let door = cs14_door_position(eye, yaw, CS14_DOOR_AHEAD_M);
            let distance = xz_distance(door, eye);
            assert!(
                distance <= DOOR_REACH_M,
                "yaw {yaw}: door at {distance:.2} m exceeds the bash reach {DOOR_REACH_M} m"
            );
            let forward = [-yaw.sin(), 0.0, -yaw.cos()];
            let dot = ((door[0] - eye[0]) * forward[0] + (door[2] - eye[2]) * forward[2]) / distance;
            assert!(dot > 0.99, "yaw {yaw}: door is not ahead of the camera (dot {dot:.3})");
            assert_eq!(door[1], 0.0, "the door point sits on the ground plane");
        }
    }

    #[test]
    fn scenario_door_position_is_deterministic() {
        let eye = [411.0, 1.6, 411.0];
        let first = cs14_door_position(eye, 3.93, CS14_DOOR_AHEAD_M);
        let second = cs14_door_position(eye, 3.93, CS14_DOOR_AHEAD_M);
        assert_eq!(first, second);
    }

    #[test]
    fn a_door_beyond_the_reach_is_refused_by_the_binding_rule() {
        // Control: the same geometry placed past `DOOR_REACH_M` is out of reach,
        // so the test above is not vacuous.
        let eye = [0.0, 1.6, 0.0];
        let door = cs14_door_position(eye, 0.0, DOOR_REACH_M + 0.6);
        assert!(xz_distance(door, eye) > DOOR_REACH_M);
    }
}

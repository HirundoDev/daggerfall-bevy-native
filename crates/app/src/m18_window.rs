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
//! engine mode, the breath and the M20 status line. The demo has no water
//! (§6.5): swimming is measured in `PROBE-LOCOMOTION-001` with its synthetic
//! world; levitation arrives with M23's cast (L), driving the same engine
//! (`MoveInput.levitating`).
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

use daggerfall_behavior::locomotion::{
    max_breath, Dice100, Frame, Locomotion, MotionMode, MoveInput, PlayerBody,
};
use daggerfall_combat::{actor_seed, Combatant, EntityKind};
use daggerfall_heart::clock::{GameClock, TIME_SCALE_GAME_SECONDS_PER_REAL_SECOND};
use daggerfall_parser_bevy_bridge as bridge;
use daggerfall_skills::character::{Attribute, Skill, SKILL_CHECK_INTERVAL_MINUTES};
use daggerfall_skills::progression::{DriverStep, MotionSignals, SkillDriver};

use crate::m18_demo::{DemoScene, DemoTexture, CAPTION_RANGE_M, M19_SEED};
use daggerfall_app::law_runtime::{
    city_watch_fighter, GuardOrder, GuardPlacement, LawRuntime, CITY_WATCH_CLASS_FILE,
};
use daggerfall_behavior::law::{
    CrimeTrigger, DfRandom, GuardSpawnContext, GUARD_ATTENTION_RADIUS_METERS,
    GUARD_REPLACE_MIN_ANGLE_DEGREES, GUARD_REPLACE_ONE_IN,
};
use crate::m2_player;

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

/// M18 integrated demo window.
pub fn run_windowed_m18(
    path_checker: &daggerfall_inventory::path_checker::PathChecker,
) -> Result<(), String> {
    if path_checker.use_custom_universe() {
        return Err("M18 supports Mode A only (ModeState=B is out of scope)".into());
    }
    let scene = crate::m18_demo::compose_demo().map_err(|err| err.to_string())?;
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
         Esc releases the cursor · 1-4 1996 UI panels",
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
        .insert_resource(ClearColor(Color::srgba(0.40, 0.62, 0.94, 1.0)))
        .insert_resource(GlobalAmbientLight {
            brightness: 320.0,
            ..default()
        })
        .insert_resource(DemoWorld(scene))
        .insert_resource(m2_player::ActionState::default())
        .insert_resource(M28Law::new())
        .add_systems(
            Startup,
            (
                m18_spawn_world,
                m2_player::m2_setup_cursor,
                m19_spawn_player,
                m20_spawn_progression,
                m14_audio_startup,
            ),
        )
        .add_systems(
            Update,
            (
                m2_player::m2_gather_action,
                m2_player::m2_escape_cursor,
                // M23: the cast resolves before the movement system reads the
                // effect entity, so a Levitate cast takes effect the same
                // frame it lands.
                m23_cast_key.before(m18_apply_player_action),
                m28_door_bash.before(m18_apply_player_action),
                m28_guard_arrival,
                m28_sync_census,
                m18_apply_player_action,
                // M20: the driver consumes the signals of the M19 frame and the
                // game clock before the title/HUD consumers read the character.
                m20_progression_tick.after(m18_apply_player_action),
                m19_update_title,
                m18_billboards,
                m18_captions,
                m18_switch_panel,
                m20_key_use_skill,
            ),
        )
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
            ),
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

/// M14: opens the retail banks (Mode A), builds the engine over the sink and
/// starts the context song. Every failure is reported and degrades — the demo
/// keeps running.
fn m14_audio_startup(mut commands: Commands, world: Res<DemoWorld>) {
    let engine = match SfxBank::from_arena2() {
        Ok(bank) => {
            println!(
                "M14 VE: effects bank {} ({} records) — dispatch by directory index",
                bank.path().display(),
                bank.len()
            );
            let mut engine = AudioEngine::new(bank, m14_sink());
            m14_start_song(&mut engine);
            Some(engine)
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
    let character = &world.0.character;
    let mut player_combatant = Combatant::new(1, EntityKind::Player);
    player_combatant.luck = character.live(Attribute::Luck);
    player_combatant.agility = character.live(Attribute::Agility);
    player_combatant.critical_strike = i32::from(character.live_skill(Skill::CriticalStrike));
    player_combatant.dodging = i32::from(character.live_skill(Skill::Dodging));
    player_combatant.hand_to_hand_skill = i32::from(character.live_skill(Skill::HandToHand));
    player_combatant.strength_modifier = character.damage_modifier();
    player_combatant.health = character.health;
    player_combatant.max_health = character.max_health;
    let mut player = daggerfall_app::combat_runtime::Fighter::new(
        player_combatant,
        daggerfall_combat::engine::DeathSequence::for_player_height(1.7),
    );
    // Unarmed ⇒ `GetLiveSkillValue(HandToHand)` (`FormulaHelper.cs:589-592`),
    // the same composition the demo and the probes use — and the same
    // projection the progression driver re-applies on every skill change.
    m20_sync_player_fighter(character, &mut player);
    // M23 boundary: the player's effect entity is the character sheet
    // (8 `DFCareer.Stats` attributes + live resources); the weapon's
    // enchantment row is M24's (no item in this kit carries one), so the
    // session hook stays armed and fires the moment a row exists.
    player = player.with_effects(player_effect_target(character), None);
    let mut enemy = daggerfall_app::combat_runtime::Fighter::new(
        Combatant::new(9_001, EntityKind::EnemyClass),
        daggerfall_combat::engine::DeathSequence::for_enemy(),
    );
    // NPC attributes are M25's (no sheet in this tree): the effect entity only
    // carries live resources until then.
    let enemy_health = enemy.combatant.health;
    enemy = enemy.with_effects(
        daggerfall_behavior::magic::EffectTarget::new(enemy_health, 0, [0; 8]),
        None,
    );
    // Fixture 0: no M25 class-skill composition exists in this tree, and this
    // swing only drives the audio policy (`m22_enemy_retaliation`).
    enemy.chance_mod = 0;
    // The kit is the retail table (steel cuirass + steel Longsword); an
    // unavailable table degrades to an unarmoured fight with the reason
    // printed — no item is ever invented.
    match crate::m18_demo::retail_item_templates() {
        Ok(templates) => {
            let (player_cuirass, enemy_sword) = crate::m18_demo::m22_kit(&templates);
            let (enemy_cuirass, _) = crate::m18_demo::m22_kit(&templates);
            println!(
                "M22 VE: kit from FALL.EXE — player cuirass (tpl {}, uid {}, condition {}) + \
                 enemy steel Longsword (uid {}, condition {}) + enemy cuirass (uid {})",
                player_cuirass.template_index,
                player_cuirass.uid,
                player_cuirass.current_condition,
                enemy_sword.uid,
                enemy_sword.current_condition,
                enemy_cuirass.uid,
            );
            let player_equip = player.equip(player_cuirass);
            let enemy_armor = enemy.equip(enemy_cuirass);
            let enemy_weapon = enemy.equip(enemy_sword);
            let equipped = matches!(
                player_equip,
                daggerfall_inventory::equipment::EquipResult::Equipped(_)
            ) && matches!(
                enemy_armor,
                daggerfall_inventory::equipment::EquipResult::Equipped(_)
            ) && matches!(
                enemy_weapon,
                daggerfall_inventory::equipment::EquipResult::Equipped(_)
            );
            println!(
                "M22 VE: equipped={equipped} · player armor_values {:?} · enemy weapon {:?}",
                player.equipment.armor_values(),
                enemy.weapon.as_ref().map(|weapon| (
                    weapon.min_damage,
                    weapon.max_damage,
                    weapon.to_hit_mod
                )),
            );
        }
        Err(reason) => println!(
            "M22 VE: kit degraded — {reason}; both combatants fight unarmoured (0 invented items)"
        ),
    }
    let player_level = player.level;
    commands.insert_resource(DemoCombat {
        session: daggerfall_app::combat_runtime::CombatSession::new(
            player,
            vec![enemy],
            actor_seed(crate::m18_demo::M19_SEED, 1),
        ),
        enemy_attack: daggerfall_combat::engine::EnemyAttack::new(),
        player_level,
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

/// M23 effect entity of the player, projected from the character sheet:
/// `health`/`magicka`/attributes are the live values M20 keeps, never literals.
fn player_effect_target(character: &daggerfall_skills::character::Character) -> daggerfall_behavior::magic::EffectTarget {
    use daggerfall_behavior::magic::EffectTarget;
    // `Character::live` takes the skills `Attribute`; both enums transcribe
    // the same `DFCareer.Stats` order the neutral target indexes by.
    const ATTRIBUTES: [Attribute; 8] = [
        Attribute::Strength,
        Attribute::Intelligence,
        Attribute::Willpower,
        Attribute::Agility,
        Attribute::Endurance,
        Attribute::Personality,
        Attribute::Speed,
        Attribute::Luck,
    ];
    let mut target = EffectTarget::new(
        character.health,
        character.magicka,
        ATTRIBUTES.map(|attribute| character.live(attribute)),
    );
    target.max_health = character.max_health;
    target.fatigue = character.fatigue;
    target
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

/// M14 event 2: left-click melee — always the swing clip, plus `Hit*` when the
/// M22 engine says the attack landed.
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
fn m14_melee_sfx(
    action: Res<m2_player::ActionState>,
    mut audio_state: ResMut<GameAudio>,
    mut combat: ResMut<DemoCombat>,
    mut world: ResMut<DemoWorld>,
    mut progression: ResMut<Progression>,
    law: Res<M28Law>,
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
    // so the engine takes the hand-to-hand branch).
    let session = &mut combat.session;
    let target = law.target;
    let Some(strike) = session.player_strike(target) else {
        println!(
            "M14 VE: melee swing (unarmed) → {} · no strike: target {target} alive={} player alive={}",
            swing_clip.map_or("-", |clip| clip.name()),
            session.enemies.get(target).is_some_and(|enemy| enemy.is_alive()),
            session.player.is_alive(),
        );
        return;
    };
    let attack = &strike.attack;
    let enemy_before = strike.target_health
        + if attack.damage > 0 {
            attack.damage
        } else {
            0
        };
    println!(
        "M14 VE: melee swing (unarmed) → {} · target={} hit={} chance={} damage={} part={} · enemy HP \
         {enemy_before}→{} (died: {}) · target_wear={} · weapon_wear={}",
        swing_clip.map_or("-", |clip| clip.name()),
        target,
        attack.hit,
        attack.chance,
        attack.damage,
        attack.part,
        strike.target_health,
        strike.target_died,
        strike_wear(&strike.target_wear),
        strike_wear(&strike.weapon_wear),
    );
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
fn m22_enemy_retaliation(
    time: Res<Time>,
    mut combat: ResMut<DemoCombat>,
    mut world: ResMut<DemoWorld>,
    mut progression: ResMut<Progression>,
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
    for group in &scene.terrain.groups {
        let (color, label) = match group.band {
            daggerfall_parser_rs::woods::TerrainBand::Ocean => {
                ([0.10, 0.22, 0.42, 1.0], "ocean")
            }
            daggerfall_parser_rs::woods::TerrainBand::Beach => {
                ([0.72, 0.66, 0.45, 1.0], "beach")
            }
            daggerfall_parser_rs::woods::TerrainBand::Land => {
                ([0.24, 0.40, 0.20, 1.0], "land")
            }
        };
        let mesh = bridge::mesh_from_spec(&group.spec, &mut meshes);
        let material = bridge::material_from_spec(
            &bridge::MaterialSpec {
                base_color: color,
                unlit: true,
                ..default()
            },
            &mut materials,
        );
        println!(
            "M18 VE: terrain band {label} → {} triangles",
            group.spec.triangle_count()
        );
        commands.spawn((Mesh3d(mesh), MeshMaterial3d(material)));
    }

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
        let Ok(image) = bridge::image_from_rgba(
            &panel.rgba,
            u32::from(panel.width),
            u32::from(panel.height),
            &mut images,
        ) else {
            continue;
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
            Transform::from_xyz(0.0, 0.0, 0.0),
        ));
        if index != 0 {
            entity.insert(Visibility::Hidden);
        }
        panel_sprites.push(entity.id());
    }
    commands.insert_resource(DemoUi {
        sprites: panel_sprites,
        current: 0,
    });
    println!(
        "M18 VE: 1996 UI → {} overlaid panels (320×200 canvas ×2) · keys 1-4",
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

/// M20: single character → M22 player projection (level, live skills,
/// attributes, health). Used at startup and on every progression change.
fn m20_sync_player_fighter(
    character: &daggerfall_skills::character::Character,
    player: &mut daggerfall_app::combat_runtime::Fighter,
) {
    player.combatant.luck = character.live(Attribute::Luck);
    player.combatant.agility = character.live(Attribute::Agility);
    player.combatant.critical_strike = i32::from(character.live_skill(Skill::CriticalStrike));
    player.combatant.dodging = i32::from(character.live_skill(Skill::Dodging));
    player.combatant.hand_to_hand_skill = i32::from(character.live_skill(Skill::HandToHand));
    player.combatant.strength_modifier = character.damage_modifier();
    player.combatant.health = character.health;
    player.combatant.max_health = character.max_health;
    player.reflexes = character.reflex as i32;
    player.level = character.level;
    // Unarmed ⇒ `GetLiveSkillValue(HandToHand)` (`FormulaHelper.cs:589-592`).
    player.chance_mod = i32::from(character.live_skill(Skill::HandToHand));
}

/// M20: re-projects the character into every live consumer of the window —
/// HUD line, M19 `PlayerBody`, M22 player fighter and the enemy timer level.
fn m20_sync_character(
    scene: &mut DemoScene,
    m19: &mut M19Player,
    combat: &mut DemoCombat,
) {
    scene.m20_refresh_hud();
    m19.body = scene.character.player_body();
    m20_sync_player_fighter(&scene.character, &mut combat.session.player);
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

/// Live title bar: M19 engine mode, breath and M20 status line.
fn m19_update_title(
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    player: Res<M19Player>,
    world: Res<DemoWorld>,
    law: Res<M28Law>,
) {
    let mode = player.locomotion.mode;
    let mut title = format!(
        "DAGGERFALL-LIKE — M20 demo · {} · {} · breath {}/{}",
        mode.label(),
        world.0.m20_status_line(),
        player.locomotion.breath,
        max_breath(player.body.live_endurance),
    );
    if mode != MotionMode::Swimming {
        title.push_str(" (no water)");
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

/// NPC billboards: always facing the camera (Y rotation).
fn m18_billboards(
    npcs: Res<DemoNpcs>,
    cameras: Query<&Transform, (With<Camera3d>, Without<Mesh3d>)>,
    mut transforms: Query<&mut Transform, (With<Mesh3d>, Without<Camera3d>)>,
) {
    let Ok(camera) = cameras.single() else {
        return;
    };
    for (entity, position) in &npcs.0 {
        if let Ok(mut transform) = transforms.get_mut(*entity) {
            let dx = camera.translation.x - position.x;
            let dz = camera.translation.z - position.z;
            transform.rotation = Quat::from_rotation_y(dx.atan2(dz));
        }
    }
}

/// Shows the caption of the nearest NPC within [`CAPTION_RANGE_M`].
fn m18_captions(
    npcs: Res<DemoNpcs>,
    mut captions: ResMut<DemoCaptions>,
    cameras: Query<&Transform, With<Camera3d>>,
    mut visibilities: Query<&mut Visibility>,
    world: Res<DemoWorld>,
) {
    let Ok(camera) = cameras.single() else {
        return;
    };
    let mut nearest: Option<(f32, usize)> = None;
    for (index, (_, position)) in npcs.0.iter().enumerate() {
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

/// Keys 1-4: toggle the overlaid M17 panel.
fn m18_switch_panel(
    keys: Res<ButtonInput<KeyCode>>,
    mut ui: ResMut<DemoUi>,
    mut visibilities: Query<&mut Visibility>,
) {
    let requested = if keys.just_pressed(KeyCode::Digit1) {
        Some(0)
    } else if keys.just_pressed(KeyCode::Digit2) {
        Some(1)
    } else if keys.just_pressed(KeyCode::Digit3) {
        Some(2)
    } else if keys.just_pressed(KeyCode::Digit4) {
        Some(3)
    } else {
        None
    };
    let Some(index) = requested else {
        return;
    };
    if index >= ui.sprites.len() || index == ui.current {
        return;
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
        .filter(|(distance, _, _)| *distance <= DOOR_REACH_M)
        .min_by(|a, b| a.0.total_cmp(&b.0));
    let Some((distance, quality, door)) = nearest else {
        println!(
            "M28 VE: bash — no retail door within {DOOR_REACH_M:.1} m (nearest building doors are farther away)"
        );
        return;
    };
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


//! M17 — window of the **1996 aesthetic UI** (REQ-L99-050): the 4 composed
//! panels (`ui_spec`) over the M7 read models, without Bevy default widgets
//! and without system fonts (C7). The text comes from the retail `.FNT`; the
//! images, from `.IMG/.CIF`.
//!
//! **Canvas:** exact 320×200 composed on CPU (`compose`) and presented with
//! integer ×2 nearest scaling + 40+40 letterbox (`present_nearest`, C1/C1b).
//! The window measures 640×480 (the full present).
//!
//! **Declared delta:** this window does NOT replace the owner's visual sign-off
//! (`docs/sintergic/game/programming/m17-owner-acceptance.md`,
//! `verdict: PENDING`): it is evidence that the retail panels paint over their
//! family palette; the side-by-side comparison with the original is M18/owner
//! acceptance (§8 spec).

use bevy::asset::RenderAssetUsages;
use bevy::image::ImagePlugin;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use daggerfall_parser_bevy_bridge::ui_spec::{
    compose, present_nearest, UiAssets, PRESENT_HEIGHT, PRESENT_WIDTH,
};

/// UI assets loaded in `Startup` (handles of the 4 presentations).
#[derive(Resource)]
struct M17Panels {
    current: usize,
}

/// Panel sprites (one per panel, toggled visibility).
#[derive(Resource)]
struct M17Sprites(Vec<Entity>);

/// Loads the panel assets from ARENA2 (Mode A read-only).
fn load_assets() -> Result<UiAssets, String> {
    // Single loader shared with PROBE-UI-001 and the M18 demo.
    let dir = crate::mode_a_arena2_dir()?;
    crate::ui_assets::load_ui_assets(&dir)
}

/// M17 window: opens the 4 retail panels presented at 640×480.
pub fn run_windowed_m17(
    path_checker: &daggerfall_inventory::path_checker::PathChecker,
) -> Result<(), String> {
    // Mode A only (milestone boundary), same as the M2-M8 windows.
    if path_checker.use_custom_universe() {
        return Err("M17 supports Mode A only (ModeState=B out of scope)".into());
    }
    let assets = load_assets()?;
    // Canonical M17 panels shared with the M18 demo: the same 4 panels from
    // the M7 read models, 0 duplicated data.
    let panels = crate::ui_assets::session_panels(&assets)?;

    let mut presentations: Vec<(String, Vec<u8>)> = Vec::new();
    for panel in &panels {
        let framebuffer = compose(std::slice::from_ref(panel), &assets)
            .map_err(|err| format!("{}: {err}", panel.id.as_str()))?;
        let present = present_nearest(&framebuffer);
        presentations.push((panel.id.as_str().to_string(), present.rgba));
    }

    println!(
        "M17 VE: 1996 UI window — {} retail panels (IMG/CIF/FNT from ARENA2) · canvas \
         {PRESENT_WIDTH}×{PRESENT_HEIGHT} (320×200 ×2 nearest + letterbox) · keys 1-4 \
         switch panel · 0 default widgets/fonts",
        presentations.len()
    );

    App::new()
        .add_plugins(DefaultPlugins.set(ImagePlugin::default_nearest()).set(
            WindowPlugin {
                primary_window: Some(Window {
                    title: "daggerfall-bevy-native M17 — 1996 UI (CR-778b1a85)".to_string(),
                    resolution: (PRESENT_WIDTH as u32, PRESENT_HEIGHT as u32).into(),
                    resizable: false,
                    ..default()
                }),
                ..default()
            },
        ))
        .insert_resource(ClearColor(Color::srgb(0.0, 0.0, 0.0)))
        .insert_resource(Presentations(presentations))
        .add_systems(Startup, m17_spawn)
        .add_systems(Update, m17_switch_panel)
        .run();
    Ok(())
}

/// Presentations pending upload to the GPU (bootstrap resource).
#[derive(Resource)]
struct Presentations(Vec<(String, Vec<u8>)>);

fn m17_spawn(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut pending: ResMut<Presentations>,
) {
    let mut sprites = Vec::with_capacity(pending.0.len());
    for (index, (_name, rgba)) in pending.0.drain(..).enumerate() {
        let image = Image::new(
            Extent3d {
                width: PRESENT_WIDTH as u32,
                height: PRESENT_HEIGHT as u32,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            rgba,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        let handle = images.add(image);
        let mut entity = commands.spawn((
            Sprite {
                image: handle,
                custom_size: Some(Vec2::new(PRESENT_WIDTH as f32, PRESENT_HEIGHT as f32)),
                ..default()
            },
            Transform::default(),
        ));
        if index != 0 {
            entity.insert(Visibility::Hidden);
        }
        sprites.push(entity.id());
    }
    commands.spawn(Camera2d);
    commands.insert_resource(M17Panels { current: 0 });
    commands.insert_resource(M17Sprites(sprites));
}

/// Switches panels with 1-4 (no widgets: switching is keyboard-driven).
fn m17_switch_panel(
    keys: Res<ButtonInput<KeyCode>>,
    mut panels: ResMut<M17Panels>,
    sprites: Res<M17Sprites>,
    mut query: Query<&mut Visibility>,
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
    if index >= sprites.0.len() || index == panels.current {
        return;
    }
    for (at, entity) in sprites.0.iter().enumerate() {
        if let Ok(mut visibility) = query.get_mut(*entity) {
            *visibility = if at == index {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }
    panels.current = index;
}

//! M35 V6 — nature billboards (spec §3, retail `TEXTURE.500..511`).
//!
//! **Scope.** The placement decision is the model's, not this module's:
//! [`place_nature`] takes the terrain samples and the retail location
//! rectangles and returns the billboards with their record, height scale and
//! sink offset. This module answers only the render questions the window asks:
//! *which* samples are rendered at all, *where* they land in the world the
//! window already draws, *which* sprite image each record maps to, and how the
//! quad reaches the framebuffer.
//!
//! **Sample set.** The samples are exactly the terrain ring the window draws
//! (`DemoTerrain::first_sample..+dim`), grouped by the map-pixel that owns each
//! global sample index with the same `div_euclid` / `rem_euclid` split the
//! `WOODS.WLD` sampler uses (`DefaultTerrainSampler`). A sample that the ring
//! does not draw never becomes a billboard, so no sprite can float outside the
//! mesh.
//!
//! **Rects.** The location footprints come from the retail `MAPS.BSA`
//! (`MAPNAMES.NNN` + `MAPTABLE.NNN`) with the bitfield layout of
//! `M18`/`m35_biome_probe` (`MapTableData`: 256-unit map units, `16` tiles per
//! block). A map-pixel whose whole area is inside the clearance-expanded
//! footprint (the capital at `(207, 213)`, `8 × 8` blocks) produces **no**
//! sample: skipping it is the same answer `place_nature` would give, only
//! without building a terrain patch that rule 2 would reject tile by tile.
//!
//! **Declared gap.** `TileClass` is [`TileClass::Unknown`] for every sample
//! (`GAP-TAX-11`, no tile classifier in the bridge yet), so rule 4 (the
//! probability gate) is **not applied** and the placement reports
//! `probability_gate_unavailable`. That is logged once, never turned into an
//! invented rejection.
//!
//! **Scale.** Retail sprite pixels are `MeshReader.GlobalScale` = `0.025`
//! world units = metres ([`M35_VEG_M_PER_UNIT`], `UNITS_PER_METER = 40`),
//! identical to the geometry path of the city.

use std::collections::BTreeMap;

use bevy::prelude::*;

use daggerfall_parser_bevy_bridge as bridge;
use daggerfall_parser_rs::bsa;
use daggerfall_parser_rs::climate::{WorldClimate, WorldSeason};
use daggerfall_parser_rs::maps::{parse_map_names, parse_map_table, MapTableEntry};
use daggerfall_parser_rs::nature::NatureAtlas;
use daggerfall_parser_rs::texture::Palette;
use daggerfall_parser_rs::woods::HEIGHTMAP_DIM;

use bridge::terrain_nature::{
    place_nature, NaturePlacement, NatureRect, NatureRules, NatureSample, TileClass,
    NATURE_CLEARANCE_TILES, WORLD_MAP_TILE_DIM,
};
use bridge::terrain_spec::{TerrainPatch, TerrainVertex, SAMPLE_SPACING_M};

use crate::m18_demo::{read_woods, DemoScene, NPC_SUBCLIMATE};

use super::m35_sky_render::M35SkyState;
use super::DemoWorld;

/// Retail sprite pixel → metre (`MeshReader.GlobalScale = 0.025`,
/// `UNITS_PER_METER = 40` in [`bridge::convert`]).
pub const M35_VEG_M_PER_UNIT: f32 = 0.025;

/// Distance from the spawn eye beyond which a billboard is not spawned (m).
///
/// The ring is `dim × 6.4 m` wide; the far end of a 129-sample ring is 819 m
/// away, where a 1–3 m sprite is sub-pixel. 300 m keeps the visible field
/// populated and bounds the entity count of the demo.
pub const M35_VEG_MAX_DISTANCE_M: f32 = 300.0;

/// Hard cap on spawned billboards of the annex (declared, not silent).
pub const M35_VEG_MAX_BILLBOARDS: usize = 2_000;

/// Determinism seed handed to [`NatureRules::new`].
pub const M35_VEG_SEED: u64 = 0x4d33_3556_4547_0001;

/// Tiles per map block (`MapFile.MapBlockSize = 4096` units / 256 units per
/// tile); the same value `parser_bevy_bridge::terrain_nature` documents.
pub const M35_VEG_TILES_PER_BLOCK: i32 = 16;

/// Alpha cutoff of the mask material (retail sprites are binary-masked).
pub const M35_VEG_ALPHA_CUTOFF: f32 = 0.5;

// ---------------------------------------------------------------------------
// Pure geometry: ring → pixels, tiles → world, steepness.
// ---------------------------------------------------------------------------

/// Global sample index → owning map-pixel offset and in-pixel tile.
///
/// The split is the sampler's own (`DS = 128`): `div_euclid`/`rem_euclid` so
/// negative indices land in the previous pixel instead of mirroring.
pub fn sample_owner(sample: i32, origin: i32) -> (i32, u16) {
    (
        origin + sample.div_euclid(WORLD_MAP_TILE_DIM),
        sample.rem_euclid(WORLD_MAP_TILE_DIM) as u16,
    )
}

/// Groups the drawn ring by owning map-pixel.
///
/// Iteration order is the ring's own (`z` outer, `x` inner) and the map is a
/// `BTreeMap`, so the placement order is deterministic across runs.
pub fn ring_samples(
    first_sample: i32,
    dim: usize,
    origin_pixel: [i32; 2],
) -> BTreeMap<[i32; 2], Vec<[u16; 2]>> {
    let mut out: BTreeMap<[i32; 2], Vec<[u16; 2]>> = BTreeMap::new();
    let end = first_sample + dim as i32;
    for sz in first_sample..end {
        for sx in first_sample..end {
            let (pixel_x, tile_x) = sample_owner(sx, origin_pixel[0]);
            let (pixel_z, tile_z) = sample_owner(sz, origin_pixel[1]);
            out.entry([pixel_x, pixel_z])
                .or_default()
                .push([tile_x, tile_z]);
        }
    }
    out
}

/// World `(x, z)` in metres of a tile inside a map-pixel offset from the
/// origin pixel.
///
/// The ring places global sample `s` at `s * SAMPLE_SPACING_M`
/// (`DemoTerrain::height_at` divides by the same spacing), so the world
/// position is the global sample index times the spacing.
pub fn world_xz(pixel_offset: [i32; 2], tile: [u16; 2]) -> [f32; 2] {
    let gx = pixel_offset[0] * WORLD_MAP_TILE_DIM + i32::from(tile[0]);
    let gz = pixel_offset[1] * WORLD_MAP_TILE_DIM + i32::from(tile[1]);
    [
        gx as f32 * SAMPLE_SPACING_M,
        gz as f32 * SAMPLE_SPACING_M,
    ]
}

/// Steepness in degrees from the five sampled heights of a centre vertex and
/// its four grid neighbours (`[centre, +x, -x, +z, -z]`).
///
/// Central difference per axis, divided by the grid spacing and converted to
/// degrees; the maximum of both axes is the measured steepness. A clamped edge
/// (neighbour replaced by the centre) contributes `0` instead of a fake wall.
pub fn steepness_deg(heights: [f32; 5], spacing_m: f32) -> f32 {
    let dx = (heights[1] - heights[2]).abs() / (2.0 * spacing_m);
    let dz = (heights[3] - heights[4]).abs() / (2.0 * spacing_m);
    dx.max(dz).atan().to_degrees()
}

/// Heights of the four grid neighbours of `tile` inside `patch`, clamped to
/// the patch on every side (`[centre, +x, -x, +z, -z]`).
pub fn patch_neighbour_heights(patch: &TerrainPatch, tile: [u16; 2]) -> [f32; 5] {
    let last = patch.dim() as i32 - 1;
    let x = i32::from(tile[0]);
    let z = i32::from(tile[1]);
    let at = |ix: i32, iz: i32| {
        let cx = ix.clamp(0, last) as usize;
        let cz = iz.clamp(0, last) as usize;
        patch.vertex(cx, cz).height_m
    };
    [at(x, z), at(x + 1, z), at(x - 1, z), at(x, z + 1), at(x, z - 1)]
}

// ---------------------------------------------------------------------------
// Pure placement helpers.
// ---------------------------------------------------------------------------

/// Sprite size in metres from the retail record and the height scale of the
/// model (`width × 0.025`, `height × 0.025 × scale`).
pub fn sprite_size(width_px: u16, height_px: u16, scale: f32) -> [f32; 2] {
    [
        f32::from(width_px) * M35_VEG_M_PER_UNIT,
        f32::from(height_px) * M35_VEG_M_PER_UNIT * scale,
    ]
}

/// Clamps a billboard count to the annex cap [`M35_VEG_MAX_BILLBOARDS`].
pub fn clamp_billboards(count: usize) -> usize {
    count.min(M35_VEG_MAX_BILLBOARDS)
}

/// `true` when every tile of the map-pixel is inside the clearance-expanded
/// union of `rects` — i.e. `place_nature` would reject every sample.
pub fn pixel_cleared(rects: &[NatureRect], clearance: i32) -> bool {
    if rects.is_empty() {
        return false;
    }
    let expanded: Vec<NatureRect> = rects.iter().map(|rect| rect.expanded(clearance)).collect();
    (0..WORLD_MAP_TILE_DIM).all(|z| {
        (0..WORLD_MAP_TILE_DIM).all(|x| {
            expanded
                .iter()
                .any(|rect| rect.contains([x as u16, z as u16]))
        })
    })
}

/// Map-pixel and tile rectangle of one `MAPTABLE` entry.
///
/// Bitfield layout of `MapTableData` (`m18_probe`/`m35_biome_probe`): the
/// longitude lives in the low 24 bits of `LatitudeType`, the latitude in the
/// low 24 bits of `LongitudeType`, both in 256-unit map units (`128` units per
/// diagonal tile of a 128-tile pixel); one map block is `4096` units = `16`
/// tiles.
pub fn rect_of(entry: &MapTableEntry) -> ([i32; 2], NatureRect) {
    let longitude = (entry.latitude_type & 0x01ff_ffff) >> 8;
    let latitude = (entry.longitude_type & 0x00ff_ffff) >> 8;
    let pixel = [
        (longitude / 128) as i32,
        499 - (latitude / 128) as i32,
    ];
    let width_blocks = ((entry.longitude_type >> 24) & 0x0f) as i32;
    let height_blocks = ((entry.longitude_type >> 28) & 0x0f) as i32;
    let offset_x = (longitude % 128) as i32;
    let offset_z = (WORLD_MAP_TILE_DIM - 1) - (latitude % 128) as i32;
    let tiles_x = width_blocks * M35_VEG_TILES_PER_BLOCK;
    let tiles_z = height_blocks * M35_VEG_TILES_PER_BLOCK;
    let rect = NatureRect {
        min: [offset_x.max(0), (offset_z - tiles_z + 1).max(0)],
        max: [
            (offset_x + tiles_x - 1).min(WORLD_MAP_TILE_DIM - 1),
            offset_z.min(WORLD_MAP_TILE_DIM - 1),
        ],
    };
    (pixel, rect)
}

/// FNV-1a digest of a billboard list (tile, record, height scale bits), so the
/// demo can print one stable number per run.
pub fn position_digest(entries: &[PlannedVeg]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    for entry in entries {
        let mut feed = |byte: u8| {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(PRIME);
        };
        for lane in [
            entry.tile[0] as u32,
            entry.tile[1] as u32,
            entry.record as u32,
            entry.scale.to_bits(),
        ] {
            for byte in lane.to_le_bytes() {
                feed(byte);
            }
        }
    }
    hash
}

// ---------------------------------------------------------------------------
// ECS payload.
// ---------------------------------------------------------------------------

/// One billboard ready to be spawned: world ground position, retail record and
/// sprite size. Season changes reuse it, because the season only swaps the
/// archive (`NaturePlacement::archive_for`), never the positions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlannedVeg {
    /// In-pixel tile (the placement identity).
    pub tile: [u16; 2],
    /// Record inside the atlas (`0..32`).
    pub record: usize,
    /// Height scale of the model.
    pub scale: f32,
    /// Ground position in the window's world space (m).
    pub position: [f32; 3],
    /// Sprite size in metres (`width`, `height`).
    pub size: [f32; 2],
}

/// Counters of one placement pass — every rejection of the model is carried
/// through instead of being swallowed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VegReport {
    /// Map-pixels that owned at least one drawn sample.
    pub pixels: usize,
    /// Map-pixels skipped because their whole area is cleared.
    pub pixels_cleared: usize,
    /// Samples handed to `place_nature`.
    pub samples: usize,
    /// Billboards accepted by the model.
    pub billboards: usize,
    /// Rule 1 rejections (ocean/beach).
    pub rejected_below_beach: usize,
    /// Rule 2 rejections (location clearance).
    pub rejected_clearance: usize,
    /// Rule 3 rejections (steepness).
    pub rejected_steepness: usize,
    /// Rule 4 rejections (only for known tile classes).
    pub rejected_probability: usize,
    /// Samples that reached rule 4 without a tile class.
    pub samples_without_class: usize,
    /// `true` when the probability gate was not measurable.
    pub probability_gate_unavailable: bool,
    /// Map-pixels whose placement hit the batch cap.
    pub capped_pixels: usize,
    /// Billboards dropped by distance culling or the annex cap.
    pub culled: usize,
    /// Nature archive actually loaded (`TEXTURE.{archive}`).
    pub archive: u32,
}

/// Vegetation state of the annex: the plan, the spawned entities and the
/// season they were spawned for.
#[derive(Resource, Default)]
pub struct M35VegetationState {
    /// Season the current entity set was built for.
    pub season: Option<WorldSeason>,
    /// `true` once the plan was built (an empty plan is legal).
    pub planned: bool,
    /// Billboards of the current plan.
    pub plan: Vec<PlannedVeg>,
    /// Parent entities currently alive.
    pub spawned: Vec<Entity>,
    /// Deterministic digest of the plan.
    pub digest: u64,
    /// Archive of the last spawn.
    pub archive: u32,
    /// Last camera yaw the billboards were turned to.
    pub last_yaw: f32,
}

/// Marks the parent entity of one billboard.
#[derive(Component, Debug, Clone, Copy)]
pub struct M35VegBillboard;

/// Plugin of the vegetation annex.
pub struct M35VegetationPlugin;

impl Plugin for M35VegetationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<M35VegetationState>()
            // The world (and `M35SkyState` of the sky plugin) exist here.
            .add_systems(Startup, m35_spawn_vegetation.after(super::m18_spawn_world))
            .add_systems(Update, (m35_vegetation_season, m35_vegetation_face_camera));
    }
}

// ---------------------------------------------------------------------------
// Startup: plan + spawn.
// ---------------------------------------------------------------------------

/// Rewrites the billboards whenever the season changed (the archive is the
/// only thing a season swaps); the first run also builds the plan.
fn m35_vegetation_season(
    mut commands: Commands,
    world: Res<DemoWorld>,
    sky: Option<Res<M35SkyState>>,
    mut state: ResMut<M35VegetationState>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let season = sky
        .as_deref()
        .map(|sky| sky.season)
        .unwrap_or(WorldSeason::Summer);
    if state.season == Some(season) {
        return;
    }
    spawn_for_season(
        &mut commands,
        &world.0,
        season,
        &mut state,
        &mut images,
        &mut meshes,
        &mut materials,
    );
}

fn m35_spawn_vegetation(
    commands: Commands,
    world: Res<DemoWorld>,
    sky: Option<Res<M35SkyState>>,
    state: ResMut<M35VegetationState>,
    images: ResMut<Assets<Image>>,
    meshes: ResMut<Assets<Mesh>>,
    materials: ResMut<Assets<StandardMaterial>>,
) {
    m35_vegetation_season(commands, world, sky, state, images, meshes, materials);
}

fn spawn_for_season(
    commands: &mut Commands,
    scene: &DemoScene,
    season: WorldSeason,
    state: &mut M35VegetationState,
    images: &mut Assets<Image>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    for entity in state.spawned.drain(..) {
        commands.entity(entity).despawn();
    }
    state.season = Some(season);

    if !state.planned {
        let started = std::time::Instant::now();
        let (plan, report) = build_plan(scene, season);
        state.plan = plan;
        state.digest = position_digest(&state.plan);
        state.planned = true;
        info!(
            "M35 VE6: plan={} billboards · pixels={} cleared={} samples={} · rejected \
             below_beach={} clearance={} steepness={} probability={} · without_class={} \
             gate_unavailable={} capped_pixels={} culled={} · archive={} digest={:016x} · {} ms",
            state.plan.len(),
            report.pixels,
            report.pixels_cleared,
            report.samples,
            report.rejected_below_beach,
            report.rejected_clearance,
            report.rejected_steepness,
            report.rejected_probability,
            report.samples_without_class,
            report.probability_gate_unavailable,
            report.capped_pixels,
            report.culled,
            report.archive,
            state.digest,
            started.elapsed().as_millis()
        );
        if report.probability_gate_unavailable {
            info!(
                "M35 VE6: GAP-TAX-11 — {} samples reached the probability gate with \
                 TileClass::Unknown; the gate is NOT applied (declared gap, no invented \
                 rejection)",
                report.samples_without_class
            );
        }
    }

    let (plan, archive) = (state.plan.clone(), state.archive);
    let archive = if archive == 0 {
        NaturePlacement::archive_for(climate_of_the_demo(), season)
    } else {
        archive
    };
    state.archive = archive;

    let Some(records) = load_atlas(archive) else {
        return;
    };
    let quad = bridge::mesh_from_spec(&billboard_spec(), meshes);
    // One material per record actually used: retail sprites are alpha-masked.
    let mut cache: BTreeMap<usize, Handle<StandardMaterial>> = BTreeMap::new();
    let mut spawned = 0usize;
    for entry in &plan {
        let Some(record) = records.get(entry.record) else {
            continue;
        };
        let material = match cache.get(&entry.record) {
            Some(handle) => handle.clone(),
            None => {
                let image = match bridge::image_from_rgba(
                    &record.rgba,
                    u32::from(record.width),
                    u32::from(record.height),
                    images,
                ) {
                    Ok(handle) => handle,
                    Err(reason) => {
                        warn!("M35 VE6: record {} image: {reason:?}", entry.record);
                        continue;
                    }
                };
                let handle = materials.add(StandardMaterial {
                    base_color: Color::WHITE,
                    base_color_texture: Some(image),
                    unlit: true,
                    cull_mode: None,
                    alpha_mode: AlphaMode::Mask(M35_VEG_ALPHA_CUTOFF),
                    ..default()
                });
                cache.insert(entry.record, handle.clone());
                handle
            }
        };
        let parent = commands
            .spawn((
                M35VegBillboard,
                Transform::from_xyz(entry.position[0], entry.position[1], entry.position[2]),
                Visibility::Inherited,
            ))
            .id();
        commands.entity(parent).with_children(|children| {
            children.spawn((
                Mesh3d(quad.clone()),
                MeshMaterial3d(material),
                Transform::from_xyz(0.0, entry.size[1] * 0.5, 0.0).with_scale(Vec3::new(
                    entry.size[0],
                    entry.size[1],
                    1.0,
                )),
            ));
        });
        state.spawned.push(parent);
        spawned += 1;
    }
    info!(
        "M35 VE6: spawned={spawned} season={season:?} archive={archive} images={} digest={:016x}",
        cache.len(),
        state.digest
    );
}

/// Turns every billboard to the camera, but only when the camera yaw changed.
fn m35_vegetation_face_camera(
    mut state: ResMut<M35VegetationState>,
    cameras: Query<&Transform, (With<Camera3d>, Without<M35VegBillboard>)>,
    mut parents: Query<&mut Transform, With<M35VegBillboard>>,
) {
    let Some(camera) = cameras.iter().next() else {
        return;
    };
    let (yaw, _, _) = camera.rotation.to_euler(EulerRot::YXZ);
    if (yaw - state.last_yaw).abs() < f32::EPSILON {
        return;
    }
    state.last_yaw = yaw;
    let rotation = Quat::from_rotation_y(yaw);
    for mut transform in &mut parents {
        transform.rotation = rotation;
    }
}

// ---------------------------------------------------------------------------
// Planning.
// ---------------------------------------------------------------------------

/// Climate of the demo capital, resolved the same way the city NPCs do
/// (`NPC_SUBCLIMATE`). A value outside the table keeps the model's own
/// `Temperate` default instead of inventing a climate.
pub fn climate_of_the_demo() -> WorldClimate {
    // `231` is Woodlands, the `CLIMATE.PAK` value of the retail capital; the
    // fallback is the same variant, so an out-of-range table never invents a
    // different climate.
    WorldClimate::from_index(NPC_SUBCLIMATE).unwrap_or(WorldClimate::Woodlands)
}

/// Builds the billboard plan of the drawn ring for `season`.
pub fn build_plan(scene: &DemoScene, season: WorldSeason) -> (Vec<PlannedVeg>, VegReport) {
    let mut report = VegReport {
        archive: NaturePlacement::archive_for(climate_of_the_demo(), season),
        ..default()
    };
    let ring = ring_samples(
        scene.terrain.first_sample,
        scene.terrain.dim,
        scene.terrain.origin_pixel,
    );
    let rects = match load_location_rects() {
        Ok(rects) => rects,
        Err(reason) => {
            warn!("M35 VE6: MAPS.BSA rects unavailable ({reason}); only the city clearance applies");
            Vec::new()
        }
    };
    let woods = match read_woods() {
        Ok(woods) => woods,
        Err(reason) => {
            warn!("M35 VE6: WOODS.WLD unavailable ({reason:?}); 0 billboards");
            return (Vec::new(), report);
        }
    };
    let rules = NatureRules::new(M35_VEG_SEED, climate_of_the_demo());
    let eye = scene.spawn.eye;
    let origin = scene.terrain.origin_pixel;
    let mut plan: Vec<PlannedVeg> = Vec::new();

    for (pixel, tiles) in &ring {
        let local: Vec<NatureRect> = rects
            .iter()
            .filter(|(rect_pixel, _)| rect_pixel == pixel)
            .map(|(_, rect)| *rect)
            .collect();
        if pixel_cleared(&local, NATURE_CLEARANCE_TILES) {
            report.pixels_cleared += 1;
            continue;
        }
        let patch = match TerrainPatch::build_with_dim(&woods, pixel[0], pixel[1], HEIGHTMAP_DIM) {
            Ok(patch) => patch,
            Err(reason) => {
                warn!("M35 VE6: patch {pixel:?} failed ({reason}); pixel skipped");
                continue;
            }
        };
        let mut samples = Vec::with_capacity(tiles.len());
        for tile in tiles {
            let vertex: TerrainVertex = patch.vertex(usize::from(tile[0]), usize::from(tile[1]));
            let steepness = steepness_deg(patch_neighbour_heights(&patch, *tile), SAMPLE_SPACING_M);
            samples.push(NatureSample::from_vertex(
                *tile,
                &vertex,
                steepness,
                TileClass::Unknown,
            ));
        }
        report.pixels += 1;
        report.samples += samples.len();
        let placement = place_nature(&rules, *pixel, season, &samples, &local);
        report.billboards += placement.billboards.len();
        report.rejected_below_beach += placement.rejected_below_beach;
        report.rejected_clearance += placement.rejected_clearance;
        report.rejected_steepness += placement.rejected_steepness;
        report.rejected_probability += placement.rejected_probability;
        report.samples_without_class += placement.samples_without_class;
        report.probability_gate_unavailable |= placement.probability_gate_unavailable;
        if placement.capped {
            report.capped_pixels += 1;
        }
        let offset = [pixel[0] - origin[0], pixel[1] - origin[1]];
        let records_here = load_atlas(report.archive);
        let atlas_records = records_here.as_deref();
        for billboard in placement.billboards {
            let xz = world_xz(offset, billboard.tile);
            if (xz[0] - eye[0]).hypot(xz[1] - eye[2]) > M35_VEG_MAX_DISTANCE_M {
                report.culled += 1;
                continue;
            }
            let vertex = patch.vertex(usize::from(billboard.tile[0]), usize::from(billboard.tile[1]));
            let ground = scene.terrain.height_at(xz[0], xz[1]) + (billboard.sink_y - vertex.height_m);
            let record = usize::from(billboard.record);
            let Some(image) = atlas_records.and_then(|records| records.get(record)) else {
                continue;
            };
            plan.push(PlannedVeg {
                tile: billboard.tile,
                record,
                scale: billboard.scale,
                position: [xz[0], ground, xz[1]],
                size: sprite_size(image.width, image.height, billboard.scale),
            });
        }
    }
    if plan.len() > clamp_billboards(plan.len()) {
        report.culled += plan.len() - M35_VEG_MAX_BILLBOARDS;
        plan.truncate(M35_VEG_MAX_BILLBOARDS);
    }
    (plan, report)
}

// ---------------------------------------------------------------------------
// Retail assets.
// ---------------------------------------------------------------------------

/// Retail palette (`PAL.PAL`) of the window path.
fn load_palette() -> Option<Palette> {
    let path = match crate::mode_a_arena2_path("PAL.PAL") {
        Ok(path) => path,
        Err(reason) => {
            warn!("M35 VE6: PAL.PAL path: {reason}");
            return None;
        }
    };
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(reason) => {
            warn!("M35 VE6: PAL.PAL read {}: {reason}", path.display());
            return None;
        }
    };
    match Palette::parse(&bytes) {
        Ok(palette) => Some(palette),
        Err(reason) => {
            warn!("M35 VE6: PAL.PAL parse: {reason}");
            None
        }
    }
}

/// Records of `TEXTURE.{archive}` (one nature atlas, 32 records).
fn load_atlas(archive: u32) -> Option<Vec<daggerfall_parser_rs::nature::NatureRecord>> {
    let name = format!("TEXTURE.{archive}");
    let path = match crate::mode_a_arena2_path(&name) {
        Ok(path) => path,
        Err(reason) => {
            warn!("M35 VE6: {name} path: {reason}");
            return None;
        }
    };
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(reason) => {
            warn!("M35 VE6: {name} read {}: {reason}", path.display());
            return None;
        }
    };
    let palette = load_palette()?;
    match NatureAtlas::parse(&bytes, &palette) {
        Ok(atlas) => Some(atlas.records),
        Err(reason) => {
            warn!("M35 VE6: {name} parse: {reason:?}");
            None
        }
    }
}

/// Location rectangles of every generated region, keyed by map-pixel.
fn load_location_rects() -> Result<Vec<([i32; 2], NatureRect)>, String> {
    let path = crate::mode_a_arena2_path("MAPS.BSA")?;
    let data = std::fs::read(&path).map_err(|reason| format!("{}: {reason}", path.display()))?;
    let archive = bsa::parse(&data).map_err(|reason| format!("{reason:?}"))?;
    let mut out = Vec::new();
    for (index, entry) in archive.entries.iter().enumerate() {
        let bsa::Entry::Named { name, .. } = entry else {
            continue;
        };
        let Some(suffix) = name.strip_prefix("MAPNAMES.") else {
            continue;
        };
        let names_bytes = bsa::entry_data(&archive, &data, index)
            .map_err(|reason| format!("{name}: {reason:?}"))?;
        let Ok(names) = parse_map_names(names_bytes) else {
            continue;
        };
        let table_name = format!("MAPTABLE.{suffix}");
        let Some(table_index) = archive.entries.iter().position(
            |candidate| matches!(candidate, bsa::Entry::Named { name, .. } if *name == table_name),
        ) else {
            continue;
        };
        let table_bytes = bsa::entry_data(&archive, &data, table_index)
            .map_err(|reason| format!("{table_name}: {reason:?}"))?;
        let Ok(table) = parse_map_table(table_bytes, names.len()) else {
            continue;
        };
        for entry in table {
            out.push(rect_of(&entry));
        }
    }
    Ok(out)
}

/// Unit quad of one billboard, same convention as the sky quads
/// (`v = 1` at the bottom, retail textures are top-down).
fn billboard_spec() -> bridge::MeshSpec {
    bridge::MeshSpec {
        positions: vec![
            [-0.5, -0.5, 0.0],
            [0.5, -0.5, 0.0],
            [0.5, 0.5, 0.0],
            [-0.5, 0.5, 0.0],
        ],
        normals: vec![[0.0, 0.0, 1.0]; 4],
        indices: vec![0, 1, 2, 0, 2, 3],
        uvs: vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
        face_textures: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(min: [i32; 2], max: [i32; 2]) -> NatureRect {
        NatureRect { min, max }
    }

    fn veg(tile: [u16; 2], record: usize, x: f32, z: f32) -> PlannedVeg {
        PlannedVeg {
            tile,
            record,
            scale: 1.0,
            position: [x, 0.0, z],
            size: [1.0, 2.0],
        }
    }

    #[test]
    fn sample_owner_splits_like_the_sampler() {
        assert_eq!(sample_owner(0, 207), (207, 0));
        assert_eq!(sample_owner(127, 207), (207, 127));
        assert_eq!(sample_owner(128, 207), (208, 0));
        assert_eq!(sample_owner(-1, 207), (206, 127));
        assert_eq!(sample_owner(-32, 207), (206, 96));
        assert_eq!(sample_owner(-128, 207), (206, 0));
        assert_eq!(sample_owner(-129, 207), (205, 127));
    }

    #[test]
    fn ring_samples_covers_the_drawn_ring_only() {
        // A 3-sample ring centred on the origin pixel: the `-1` row and column
        // spill into the neighbouring pixel (tile 127), so nine samples land in
        // exactly four pixels.
        let ring = ring_samples(-1, 3, [207, 213]);
        let total: usize = ring.values().map(Vec::len).sum();
        assert_eq!(total, 9);
        assert_eq!(ring.len(), 4);
        assert_eq!(ring[&[206, 212]], vec![[127, 127]]);
        assert_eq!(ring[&[206, 213]], vec![[127, 0], [127, 1]]);
        assert_eq!(ring[&[207, 212]], vec![[0, 127], [1, 127]]);
        assert_eq!(ring[&[207, 213]], vec![[0, 0], [1, 0], [0, 1], [1, 1]]);
    }

    #[test]
    fn world_xz_maps_the_global_sample_grid() {
        let xz = world_xz([0, 0], [0, 0]);
        assert_eq!(xz, [0.0, 0.0]);
        let xz = world_xz([0, 0], [128, 64]);
        assert!((xz[0] - 128.0 * SAMPLE_SPACING_M).abs() < 1e-3);
        assert!((xz[1] - 64.0 * SAMPLE_SPACING_M).abs() < 1e-3);
        // The previous pixel side is negative, like the ring.
        let xz = world_xz([-1, -1], [96, 127]);
        assert!((xz[0] + 32.0 * SAMPLE_SPACING_M).abs() < 1e-3);
        assert!((xz[1] + 1.0 * SAMPLE_SPACING_M).abs() < 1e-3);
    }

    #[test]
    fn steepness_of_a_flat_surface_is_zero_and_a_slope_matches_the_angle() {
        assert_eq!(steepness_deg([200.0; 5], SAMPLE_SPACING_M), 0.0);
        let rise = SAMPLE_SPACING_M; // 45 degrees on the x axis
        let steep = steepness_deg([0.0, rise, -rise, 0.0, 0.0], SAMPLE_SPACING_M);
        assert!((steep - 45.0).abs() < 1e-3, "…");
        // The z axis alone is enough to reject: 82.4 degrees.
        let steep = steepness_deg([0.0, 0.0, 0.0, 5.0 * SAMPLE_SPACING_M, -5.0 * SAMPLE_SPACING_M], SAMPLE_SPACING_M);
        assert!(steep > 78.0 && steep < 79.0, "{steep}");
    }

    #[test]
    fn sprite_size_uses_the_global_scale_and_the_height_scale() {
        let size = sprite_size(20, 40, 1.0);
        assert!((size[0] - 0.5).abs() < 1e-6);
        assert!((size[1] - 1.0).abs() < 1e-6);
        assert!((sprite_size(20, 40, 0.5)[1] - 0.5).abs() < 1e-6);
        assert_eq!(
            sprite_size(32, 32, 1.0),
            [32.0 * M35_VEG_M_PER_UNIT, 32.0 * M35_VEG_M_PER_UNIT]
        );
    }

    #[test]
    fn billboards_are_capped_at_the_annex_limit() {
        assert_eq!(clamp_billboards(0), 0);
        assert_eq!(clamp_billboards(129), 129);
        assert_eq!(clamp_billboards(M35_VEG_MAX_BILLBOARDS), M35_VEG_MAX_BILLBOARDS);
        assert_eq!(
            clamp_billboards(M35_VEG_MAX_BILLBOARDS + 1),
            M35_VEG_MAX_BILLBOARDS
        );
    }

    #[test]
    fn a_fully_covered_pixel_is_cleared() {
        assert!(!pixel_cleared(&[], NATURE_CLEARANCE_TILES));
        assert!(!pixel_cleared(&[rect([0, 0], [10, 10])], NATURE_CLEARANCE_TILES));
        // The capital footprint: 8 x 8 blocks at offset (0, 127) covers the
        // whole pixel once the clearance is expanded.
        assert!(pixel_cleared(
            &[rect([0, 0], [127, 127])],
            NATURE_CLEARANCE_TILES
        ));
        // A 3-block city in one corner does not.
        assert!(!pixel_cleared(&[rect([0, 0], [47, 47])], 4));
    }

    #[test]
    fn rect_of_reads_the_retail_bitfield() {
        // `MapTableData` packs the longitude in the low 24 bits of
        // `LatitudeType` and the latitude plus the block sizes in the top
        // nibbles of `LongitudeType`.
        let entry = MapTableEntry {
            map_id: 0,
            // Longitude 1: pixel 0, offset x 1.
            latitude_type: 1u32 << 8,
            // Latitude 2: pixel 499, offset z 127 - 2 = 125.
            // Width 1 block, height 1 block (16 tiles each).
            longitude_type: (2u32 << 8) | (1u32 << 24) | (1u32 << 28),
            flavor: 0,
            services: 0,
        };
        let (pixel, rect) = rect_of(&entry);
        assert_eq!(pixel, [0, 499]);
        // z runs from 125 down 16 tiles, x from 1 up 16 tiles.
        assert_eq!(rect.min, [1, 110]);
        assert_eq!(rect.max, [16, 125]);
    }

    #[test]
    fn position_digest_is_stable_and_sensitive() {
        let plan = vec![veg([1, 2], 3, 0.0, 0.0), veg([4, 5], 6, 10.0, 0.0)];
        assert_eq!(position_digest(&plan), position_digest(&plan.clone()));
        let mut other = plan.clone();
        other[1].record = 7;
        assert_ne!(position_digest(&plan), position_digest(&other));
        assert_eq!(position_digest(&[]), 0xcbf2_9ce4_8422_2325);
    }

    #[test]
    fn the_archive_follows_climate_and_season() {
        let climate = climate_of_the_demo();
        let summer = NaturePlacement::archive_for(climate, WorldSeason::Summer);
        let winter = NaturePlacement::archive_for(climate, WorldSeason::Winter);
        assert!((500..=511).contains(&summer), "{summer}");
        assert!((500..=511).contains(&winter), "{winter}");
        assert_ne!(summer, winter);
    }
}

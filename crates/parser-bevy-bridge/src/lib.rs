//! daggerfall-parser-bevy-bridge — parser↔Bevy bridge (M1).
//!
//! Contract: addendum PRE-2 §8 (`game-process-contract-addendum.md`)
//! and PRE-3 decisions (`bevy-bridge-matrix.md` §10.1 — `MeshSpec`
//! is kept as a pure intermediate type).
//!
//! Position in the DAG: `app → bridge → daggerfall_parser_rs`; the parser
//! remains a 0-dependency clean-room leaf (AB-11) — this crate depends on
//! it ONLY through its public API, 0 modification of it (PARITY-FIRST).
//!
//! Contract interfaces:
//! - `BsaSource` (in) → [`bsa_source::parse_bsa`] + [`bsa_source::mesh_spec_at`]
//!   (opening through the parser's public API; custody resolved by the app);
//! - `MeshRecord` (in) → [`convert::mesh_record_to_spec`] → [`MeshSpec`]
//!   (internal, pure — testable without Bevy, rule 4);
//! - [`MeshSpec`] → [`bevy_assets::mesh_from_spec`] → `Handle<Mesh>` (out);
//! - [`bevy_assets::MaterialSpec`] → `Handle<StandardMaterial>` (out).
//! - `TextureRecord → Handle<Image>`: **resolved in M12**
//!   (DoD §c): [`bevy_assets::image_from_rgba`] + [`bevy_assets::scale_uvs`]
//!   + [`bevy_assets::split_by_texture`] + [`bevy_assets::material_from_texture`]
//!     (byte decoding in `daggerfall_parser_rs::texture`, 0 bytes
//!     packaged — INV-L99-6).
//!
//! M2: additive new interfaces, pure without Bevy:
//! - [`CollisionGrid`] → [`collision_grid`] — voxel collision grid derived
//!   from `MeshSpec` (INV-L99-5; grid-vs-geometry decision in the M2 note);
//! - [`run_view_probe`] → [`view_probe`] — PROBE-VIEW-001 core
//!   (INV-L99-5, Anti-Pantalla: runtime invariant, not screenshot-only).
//!
//! M3: additive new interfaces, pure without Bevy:
//! - [`city_spec`] — deterministic city layout (64 m blocks + 8-16 m
//!   streets) and combined floor+blocks+landmarks ARCH3D fit-to-lot mesh
//!   (street source: MISSING-SPEC GRIDCITY → synthetic over ARCH3D,
//!   delta documented in the M3 note);
//! - [`city_probe`] → [`run_city_probe`] — PROBE-CITY-001 core
//!   (street walkable end-to-end without traversing + block collision,
//!   Anti-Pantalla; reuses `run_view_probe` as generic layer);
//! - [`solid_convertible_blocks`] (BSA) + [`landmark_is_solid`] (city_spec) —
//!   SINGLE selection of convertible **and solid** landmarks (4 faces without
//!   a gap for the player sphere at eye height in the fit-to-lot WORLD),
//!   shared by the CLI and the runtime test (§5a).
//!
//! M4: additive new interfaces, pure without Bevy:
//! - [`interior_spec`] — navigable synthetic shop interior (enclosure
//!   with door gap + floor + solid props) inside the M3 city's perimeter
//!   street; it is the probe's **block A** (reference geometry 0 I/O).
//!   The premise "DFON1.BSA absent" was FALSE (WI-3): the
//!   real interior is [`interior_retail`] (`.RDB` + ARCH3D, M11), which is
//!   what the M4 window renders (`build_city_mesh` intact, PARITY-FIRST);
//! - [`interior_probe`] → [`run_interior_probe`] — PROBE-INTERIOR-001 core
//!   (street→interior walking through the door without traversing walls,
//!   Anti-Pantalla; reuses `run_view_probe` as generic layer).
//!
//! M5: additive new interfaces, pure without Bevy:
//! - [`commerce_spec`] — merchant NPC as a **solid** AABB box (AB-06:
//!   0 non-solid props) + raycast interaction
//!   ([`interact_target`], the same `CollisionGrid::raycast` as the probes);
//!   `march_axis`/[`March`] of `interior_probe` are `pub` so the
//!   commerce probe can reuse the street→door→interior march.
//!
//! M10: additive new interfaces, pure without Bevy:
//! - [`city_retail`] — city composition from real `.RMB` Map Block Records
//!   (the seed is no longer the layout source, DoD §a): each
//!   BuildingDataList[i] resolves to (type, doors, ARCH3D model) at its
//!   real position; [`RetailCity::canonical_bytes`]/[`RetailCity::digest`]
//!   for the PROBE-SEED-001 determinism probe (real ext).
//!
//! M11: additive new interfaces, pure without Bevy:
//! - [`interior_retail`] — navigable interior assembled from real
//!   `.RDB` records (DoD §b, WI-2/WI-3): Model→ARCH3D + Light + Flat with
//!   Daggerfall Angles rotations (`YRot/XRot/ZRot`) and scale
//!   `UNITS_PER_METER = 40`; the door is a **real opening** of the mesh
//!   (flanked by walls, 4-side sweep); [`RetailInterior`]
//!   [`RetailInterior::canonical_bytes`]/[`RetailInterior::digest`] for
//!   determinism; [`run_retail_interior_probe`] = real variant of
//!   PROBE-INTERIOR-001 (M2 generic layer + 7 RDB checks).
//!
//! M15: additive new interfaces, pure without Bevy:
//! - [`npc_spec`] — deterministic NPC population (race by subclimate, name
//!   bank per region with `DFRandom.srand(nameSeed)`, `faceIndex`, 8-dir
//!   flats from `TEXTURE.256`, retail caption from `TEXT.RSC`, crime guards
//!   and `create npc at` / `place npc`), DoD §f / REQ-L99-047.
//!
//! Bridge rules (PRE-2 §8): 0 mode logic · 0 persistence —
//! runtime-only conversion (INV-L99-6) · 0 winit/input types.

pub mod bevy_assets;
pub mod bsa_source;
pub mod city_probe;
pub mod city_retail;
pub mod city_spec;
pub mod collision_grid;
pub mod commerce_spec;
pub mod convert;
pub mod interior_probe;
pub mod interior_retail;
pub mod interior_spec;
pub mod mesh_spec;
pub mod npc_spec;
pub mod terrain_spec;
pub mod ui_spec;
pub mod view_probe;

pub use bevy_assets::{
    image_from_rgba, material_from_spec, material_from_texture, mesh_from_spec, scale_uvs,
    split_by_texture, MaterialSpec, TextureAssetError, TextureGroup,
};
pub use bsa_source::{
    entry_data, mesh_spec_at, parse_bsa, solid_convertible_blocks, BsaArchive, BsaError, Entry,
    MeshSpecAtError,
};
pub use city_probe::{run_city_probe, synthetic_city, CityProbeReport};
pub use city_retail::{
    build_retail_city, RetailBuilding, RetailCity, RETAIL_BLOCK_M, ROTATION_DIVISOR,
};
pub use city_spec::{
    build_city_mesh, landmark_is_solid, street_spawn, CityLayout, CityMesh, CITY_BLOCK_M,
    CITY_STREET_MAX_M, CITY_STREET_MIN_M,
};
pub use collision_grid::{
    first_geometry_hit, CellStatus, CoherenceReport, CollisionGrid, RayHit, SpawnPoint, StepOutcome,
    DEFAULT_CELL_M,
};
pub use commerce_spec::{
    interact_target, merchant_world, shop_merchant, InteractHit, MerchantProp, INTERACT_RANGE_M,
};
pub use convert::{mesh_record_to_spec, ConvertError};
pub use interior_probe::{
    march_axis, run_interior_probe, synthetic_world, world_from_landmarks, InteriorProbeReport,
    March,
};
pub use interior_retail::{
    build_retail_interior, retail_world, run_retail_interior_probe, RetailFlat, RetailInterior,
    RetailInteriorReport, RetailLight, RetailModel, Side, ENTER_DEPTH_M, RDB_SIDE_M,
    STREET_MARGIN_M,
};
pub use interior_spec::{
    build_shop_mesh, world_mesh, ShopMesh, DOOR_HEIGHT_M, DOOR_Z_MAX, DOOR_Z_MIN,
    FIXTURE_MIN_H_M, SHOP_HEIGHT_M, SHOP_X_MAX, SHOP_Z_MAX, SHOP_Z_MIN,
};
pub use mesh_spec::MeshSpec;
pub use npc_spec::{
    create_npc_at, place_npc, region, spawn_city_guards, subclimate_race, Caption, CaptionSource,
    FlatRef, GuardSpec, NpcError, NpcSpec, NpcWorld, Occupation, Place, Placement, Population,
    PopulationRequest, QuestPerson, QuestRegistry, Race, RegionInfo, SiteLink, CITY_POPULATION,
    FACE_COUNT, FLAT_DIRECTIONS, GUARDS_PER_SPAWN, GUARD_CRIME_THRESHOLD, OCCUPATIONS,
    PERSON_FLAT_ARCHIVE, PLACE_NPC_DEFAULT_MARKER, REGIONS,
};
pub use terrain_spec::{
    TerrainError, TerrainPatch, TerrainVertex, WalkReport, EYE_HEIGHT_TOLERANCE_CELLS, PATCH_DIM,
    PATCH_SPAN_M, SAMPLE_SPACING_M, TERRAIN_TILE_M, WALK_STEPS, WALK_STRIDE_M, WALK_SUBSAMPLES,
};
pub use ui_spec::{
    assert_no_generic_widgets, build_all_panels, build_hud_panel, build_inventory_panel,
    build_main_menu_panel, build_map_panels, build_quests_panel, canvas_rgba, compose,
    family_histogram, fit_text, palette_compliance, palette_hash, present_delta,
    present_interpolated_pixels, present_nearest, Framebuffer, FramebufferPalette, HudPanelData,
    InventoryPanelData, PresentFrame, PresentMode, QuestPanelData, UiAnchor, UiAsset, UiAssets,
    UiNode, UiNodeKind, UiPanel, UiPanelId, UiRect, UiSpecError, BUTTON_CELL, CANVAS_HEIGHT,
    CANVAS_WIDTH, DEFAULT_FONT, DEFAULT_INK, FONT_FILES, LETTERBOX, MAP_LOCAL_ASSET,
    MAP_WORLD_ASSET, MENU_CLIP_HEIGHT, PRESENT_HEIGHT, PRESENT_SCALE, PRESENT_WIDTH,
};
pub use view_probe::{run_view_probe, ProbeCheck, ViewProbeReport, EYE_HEIGHT_M, PLAYER_RADIUS_M};

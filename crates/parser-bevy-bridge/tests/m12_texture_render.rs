//! M12 render test (DoD §c / WI-4) — the retail texture chain reaches a real `App`'s assets.
//!
//! This is the missing proof of M12 (MAJOR of the campaign-final-reaudit):
//! the chain `TEXTURE.* bytes → TextureArchive → decode_frame → Palette::to_rgba
//! → bridge::image_from_rgba → Handle<Image> → material_from_texture` produces a
//! `StandardMaterial` whose `base_color_texture` is the decoded image, and the
//! mesh of that group carries `Mesh::ATTRIBUTE_UV_0` in `[0,1]` — without UVs
//! the material could point at the texture and the geometry would still sample
//! one texel (the original M12 hole).
//!
//! Custody (INV-L99-6): the synthetic fixture below is built byte by byte here
//! (0 retail bytes in git, 0 `include_bytes!`). The retail-gated test reads the
//! user's ARENA2 at runtime and skips explicitly when it is absent.

use std::collections::HashMap;
use std::path::PathBuf;

use bevy::asset::{AssetApp, AssetPlugin};
use bevy::image::{ImageAddressMode, ImageSampler};
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;

use daggerfall_parser_bevy_bridge as bridge;
use daggerfall_parser_rs::texture::{self, Palette, TextureArchive};

// ───────────────────────── synthetic fixture ─────────────────────────

/// Synthetic `TEXTURE.*` file (100% own bytes) with the retail layout
/// documented in `crates/daggerfall_parser_rs/src/texture.rs`:
/// `TextureFileHeader 26 B` + `RecordHeader 20 B` + `TextureRecord 28 B` at
/// `RecordPosition`, with the data of a single-frame record interleaved in
/// 256 B rows (`pos + DataOffset + row·256`).
///
/// One record: `2×2` px, palette indices `[0,1] / [2,3]`, `FrameCount = 1`,
/// `Compression = 0` (UncompressedSingleFrame).
fn synthetic_texture_file() -> Vec<u8> {
    const W: usize = 2;
    const H: usize = 2;
    const COUNT: u16 = 1;
    let header_len = texture::TEXTURE_FILE_HEADER_LEN + COUNT as usize * texture::RECORD_HEADER_LEN;
    let position = header_len;
    let data_offset = texture::TEXTURE_RECORD_LEN as u32;
    let base = position + texture::TEXTURE_RECORD_LEN;
    let file_len = base + H * texture::ROW_CHUNK;

    let mut file = vec![0u8; file_len];
    // TextureFileHeader: RecordCount u16 LE + 24 B ASCII name.
    file[0..2].copy_from_slice(&COUNT.to_le_bytes());
    let name = b"SYNTH M12 RENDER";
    file[2..2 + name.len()].copy_from_slice(name);

    // RecordHeader (20 B): Unknown1 u16 · RecordPosition i32 · Unknown2 u16 ·
    // Unknown3 u32 · NullValue i64 = 0.
    let at = texture::TEXTURE_FILE_HEADER_LEN;
    file[at..at + 2].copy_from_slice(&0x1200u16.to_le_bytes());
    file[at + 2..at + 6].copy_from_slice(&(position as u32).to_le_bytes());
    file[at + 6..at + 8].copy_from_slice(&0x3400u16.to_le_bytes());
    file[at + 8..at + 12].copy_from_slice(&0u32.to_le_bytes());
    // NullValue stays 0.

    // TextureRecord (28 B): OffsetX/Y · Width · Height · Compression ·
    // RecordSize · DataOffset (from the TextureRecord start) · IsNormal ·
    // FrameCount · Unknown1 · XScale/YScale.
    let rec = position;
    file[rec + 4..rec + 6].copy_from_slice(&(W as i16).to_le_bytes());
    file[rec + 6..rec + 8].copy_from_slice(&(H as i16).to_le_bytes());
    file[rec + 8..rec + 10].copy_from_slice(&0u16.to_le_bytes()); // Compression
    file[rec + 10..rec + 14]
        .copy_from_slice(&((texture::TEXTURE_RECORD_LEN + W * H) as u32).to_le_bytes());
    file[rec + 14..rec + 18].copy_from_slice(&data_offset.to_le_bytes());
    file[rec + 18..rec + 20].copy_from_slice(&192u16.to_le_bytes()); // IsNormal
    file[rec + 20..rec + 22].copy_from_slice(&1u16.to_le_bytes()); // FrameCount
    file[rec + 22..rec + 24].copy_from_slice(&71u16.to_le_bytes());
    file[rec + 24..rec + 26].copy_from_slice(&256i16.to_le_bytes());
    file[rec + 26..rec + 28].copy_from_slice(&256i16.to_le_bytes());

    // Pixels: row 0 = [0,1], row 1 = [2,3] (each row in its own 256 B chunk).
    let row0 = base;
    file[row0..row0 + W].copy_from_slice(&[0u8, 1]);
    let row1 = base + texture::ROW_CHUNK;
    file[row1..row1 + W].copy_from_slice(&[2u8, 3]);
    file
}

/// Synthetic `PalFile` (768 B = 256 × RGB). Index 0 = black/transparent
/// (`Palette::to_rgba` sets alpha 0 for index 0); indices 1..=3 are the four
/// distinct colors of the golden RGBA vector.
fn synthetic_palette_file() -> Vec<u8> {
    let mut colors = [0u8; texture::PAL_FILE_LEN];
    colors[0..3].copy_from_slice(&[0, 0, 0]);
    colors[3..6].copy_from_slice(&[10, 20, 30]);
    colors[6..9].copy_from_slice(&[40, 50, 60]);
    colors[9..12].copy_from_slice(&[70, 80, 90]);
    colors.to_vec()
}

/// Golden RGBA of the fixture, read straight from the four palette indices
/// `0,1,2,3` through `Palette::to_rgba` (index 0 → alpha 0).
const SYNTH_GOLDEN_RGBA: [u8; 16] = [
    0, 0, 0, 0, // index 0 (transparent)
    10, 20, 30, 255, // index 1
    40, 50, 60, 255, // index 2
    70, 80, 90, 255, // index 3
];

// ───────────────────────── headless App ─────────────────────────

/// Headless `App` (0 window, 0 GPU): `MinimalPlugins` + `AssetPlugin` and the
/// two asset stores the chain writes into.
fn headless_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()));
    app.init_asset::<Image>();
    app.init_asset::<StandardMaterial>();
    app.init_asset::<Mesh>();
    app
}

/// Runtime decision of the render path: only valid RGBA bytes produce a
/// textured material; the error short-circuits before any material is added
/// (this is the function the negative controls falsify).
fn render_material(
    rgba: &[u8],
    width: u32,
    height: u32,
    app: &mut App,
) -> Result<Handle<StandardMaterial>, bridge::TextureAssetError> {
    let texture = {
        let mut images = app.world_mut().resource_mut::<Assets<Image>>();
        bridge::image_from_rgba(rgba, width, height, &mut images)?
    };
    let mut materials = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
    Ok(bridge::material_from_texture(
        &bridge::MaterialSpec::default(),
        texture,
        &mut materials,
    ))
}

// ───────────────────────── 1. real path to Bevy assets ─────────────────────────

#[test]
fn synthetic_texture_chain_reaches_bevy_image_and_material() {
    // 1. Real decode route: parse → decode_frame → Palette::to_rgba.
    let file = synthetic_texture_file();
    let archive = TextureArchive::parse(&file).expect("synthetic TEXTURE file parses");
    assert_eq!(archive.record_count, 1);
    assert_eq!(archive.name, "SYNTH M12 RENDER");
    let decoded = texture::decode_frame(&file, &archive, 0, 0).expect("record 0 decodes");
    assert_eq!((decoded.width, decoded.height), (2, 2));
    assert_eq!(decoded.pixels, vec![0, 1, 2, 3], "palette indices golden");
    let palette = Palette::parse(&synthetic_palette_file()).expect("768 B PalFile parses");
    let rgba = palette.to_rgba(&decoded.pixels);
    assert_eq!(rgba.len(), decoded.pixels.len() * 4);

    // 2. Bridge into a runtime App (headless).
    let mut app = headless_app();
    let image_handle = {
        let mut images = app.world_mut().resource_mut::<Assets<Image>>();
        bridge::image_from_rgba(&rgba, decoded.width as u32, decoded.height as u32, &mut images)
            .expect("RGBA of the decoded record is accepted")
    };
    let material_handle = {
        let mut materials = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
        bridge::material_from_texture(
            &bridge::MaterialSpec::default(),
            image_handle.clone(),
            &mut materials,
        )
    };

    // 3. The Image is the decoded texture, pixel by pixel.
    let images = app.world().resource::<Assets<Image>>();
    let image = images.get(&image_handle).expect("image lives in Assets<Image>");
    assert_eq!(
        (image.width(), image.height()),
        (decoded.width as u32, decoded.height as u32),
        "Image dimensions == decoded TextureRecord"
    );
    assert_eq!(
        image.texture_descriptor.format,
        TextureFormat::Rgba8UnormSrgb,
        "palette colors are sRGB"
    );
    match &image.sampler {
        ImageSampler::Descriptor(sampler) => {
            assert_eq!(sampler.address_mode_u, ImageAddressMode::Repeat, "U tiles");
            assert_eq!(sampler.address_mode_v, ImageAddressMode::Repeat, "V tiles");
        }
        other => panic!("expected an ImageSampler::Descriptor, got {other:?}"),
    }
    let data = image.data.as_deref().expect("CPU-side RGBA bytes");
    assert_eq!(data.len(), SYNTH_GOLDEN_RGBA.len());
    assert_eq!(data, SYNTH_GOLDEN_RGBA, "4 golden pixels (RGBA literal)");

    // 4. The material points at exactly that image (texture is the color source).
    let materials = app.world().resource::<Assets<StandardMaterial>>();
    let material = materials
        .get(&material_handle)
        .expect("material lives in Assets<StandardMaterial>");
    assert_eq!(
        material.base_color_texture.as_ref(),
        Some(&image_handle),
        "base_color_texture == the decoded texture handle"
    );
    assert!(material.double_sided);
    assert!(material.cull_mode.is_none());
}

// ───────────────────────── 2. mesh carries UV_0 ─────────────────────────

#[test]
fn textured_group_mesh_carries_scaled_uv0() {
    let key = bridge::mesh_spec::texture_key(2, 0); // 2000 (archive 2, record 0)
    assert_eq!(key, 2000);

    let positions = vec![
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.5, 0.0, 0.0],
        [0.5, 1.0, 0.0],
    ];
    let normals = vec![[0.0, 0.0, 1.0]; 6];
    let indices = vec![0, 1, 2, 3, 4, 5];
    // UVs in texture pixel units (DF `÷16` convention already applied by
    // `convert`); a 2×2 texture means pixel 2 == the far edge.
    let uvs_px = vec![
        [0.0, 0.0],
        [2.0, 0.0],
        [0.0, 2.0],
        [2.0, 2.0],
        [1.0, 0.0],
        [1.0, 2.0],
    ];
    let spec = bridge::MeshSpec {
        positions,
        normals,
        indices,
        uvs: uvs_px,
        face_textures: vec![key; 6],
    };
    assert!(spec.texture_alignment_valid());

    // Split → one group keyed 2000; scale its UVs to [0,1].
    let mut groups = bridge::split_by_texture(&spec);
    assert_eq!(groups.len(), 1, "one texture key = one group");
    assert_eq!(groups[0].texture_key, key);
    assert_eq!(groups[0].mesh.positions.len(), 6);
    assert_eq!(groups[0].mesh.indices, vec![0, 1, 2, 3, 4, 5]);
    let group = &mut groups[0];
    group.mesh.uvs = bridge::scale_uvs(&group.mesh.uvs, 2, 2);

    let mut app = headless_app();
    {
        let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
        let handle = bridge::mesh_from_spec(&group.mesh, &mut meshes);
        let mesh = meshes.get(&handle).expect("mesh lives in Assets<Mesh>");

        let got = match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
            Some(VertexAttributeValues::Float32x2(uvs)) => uvs.clone(),
            other => panic!("textured mesh must carry ATTRIBUTE_UV_0, got {other:?}"),
        };
        let expected = vec![
            [0.0, 0.0],
            [1.0, 0.0],
            [0.0, 1.0],
            [1.0, 1.0],
            [0.5, 0.0],
            [0.5, 1.0],
        ];
        assert_eq!(got, expected, "scaled UVs reach the Mesh exactly");
        assert!(
            got.iter().flatten().all(|c| (0.0..=1.0).contains(c)),
            "normalized UVs inside [0,1]: {got:?}"
        );
        assert_eq!(mesh.count_vertices(), 6);
    }

    // Synthetic meshes (empty `uvs`) keep the exact pre-M12 vertex layout:
    // no UV attribute is inserted, so the 826 baseline tests see no delta.
    let synthetic = bridge::MeshSpec::new(
        group.mesh.positions.clone(),
        group.mesh.normals.clone(),
        group.mesh.indices.clone(),
    );
    let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
    let flat_handle = bridge::mesh_from_spec(&synthetic, &mut meshes);
    let flat_mesh = meshes.get(&flat_handle).expect("synthetic mesh");
    assert!(
        flat_mesh.attribute(Mesh::ATTRIBUTE_UV_0).is_none(),
        "empty uvs => no ATTRIBUTE_UV_0"
    );
    assert_eq!(flat_mesh.count_vertices(), 6);
}

// ───────────────────────── 3. negative controls ─────────────────────────

/// Why the controls cannot pass vacuously:
/// - `render_material` is the ONLY path from bytes to a textured material; if
///   `image_from_rgba` accepted a truncated buffer (or dimensions 0), the
///   `Err` assertions below turn red — they are not assertions about a
///   separate helper.
/// - "0 textured material in that path" is measured on the material itself
///   (`base_color_texture == None`) and on the whole store, not on a counter
///   of assets.
/// - The fallback is distinguishable from a textured material: flat color
///   (not WHITE) + no texture (a textured material is `Some(handle)`).
#[test]
fn negative_controls_reject_bad_texture_paths() {
    let mut app = headless_app();
    let golden = SYNTH_GOLDEN_RGBA;

    // (a) Truncated RGBA (one byte short) → exact DataLengthMismatch.
    let truncated = &golden[..golden.len() - 1];
    let err = render_material(truncated, 2, 2, &mut app)
        .expect_err("truncated RGBA must be rejected, never silently padded");
    assert_eq!(
        err,
        bridge::TextureAssetError::DataLengthMismatch {
            expected: 16,
            got: 15
        }
    );
    // (a2) Extra bytes are rejected too (no silent truncation).
    let mut too_long = golden.to_vec();
    too_long.push(0);
    assert_eq!(
        render_material(&too_long, 2, 2, &mut app).unwrap_err(),
        bridge::TextureAssetError::DataLengthMismatch {
            expected: 16,
            got: 17
        }
    );
    // The flat fallback of that path: no texture, base color not WHITE.
    let flat_handle = {
        let mut materials = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
        bridge::material_from_spec(&bridge::MaterialSpec::default(), &mut materials)
    };
    let materials = app.world().resource::<Assets<StandardMaterial>>();
    let flat = materials.get(&flat_handle).expect("flat material");
    assert_eq!(
        flat.base_color_texture, None,
        "the error path yields NO textured material"
    );
    assert_ne!(flat.base_color, Color::WHITE, "flat color is not the tint-white");
    assert!(
        materials.iter().all(|(_, m)| m.base_color_texture.is_none()),
        "no textured material exists after the two rejected buffers"
    );

    // (b) Dimensions 0 → EmptyDimensions (both axes).
    assert_eq!(
        render_material(&golden, 0, 2, &mut app).unwrap_err(),
        bridge::TextureAssetError::EmptyDimensions {
            width: 0,
            height: 2
        }
    );
    assert_eq!(
        render_material(&golden, 2, 0, &mut app).unwrap_err(),
        bridge::TextureAssetError::EmptyDimensions {
            width: 2,
            height: 0
        }
    );

    // (c) Texture key absent from the map → no textured material.
    let image_handle = render_material(&golden, 2, 2, &mut app).expect("valid buffer renders");
    let textures: HashMap<u32, Handle<Image>> = HashMap::from([
        (bridge::mesh_spec::texture_key(2, 0), {
            let materials = app.world().resource::<Assets<StandardMaterial>>();
            materials
                .get(&image_handle)
                .and_then(|m| m.base_color_texture.clone())
                .expect("the valid buffer produced a texture handle")
        }),
    ]);
    assert!(textures.contains_key(&2000));
    assert!(
        !textures.contains_key(&bridge::mesh_spec::texture_key(3, 0)),
        "key (3,0) is absent from the texture map"
    );
    let missing_key_material = {
        let mut materials = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
        match textures.get(&bridge::mesh_spec::texture_key(3, 0)) {
            Some(handle) => bridge::material_from_texture(
                &bridge::MaterialSpec::default(),
                handle.clone(),
                &mut materials,
            ),
            None => bridge::material_from_spec(&bridge::MaterialSpec::default(), &mut materials),
        }
    };
    let materials = app.world().resource::<Assets<StandardMaterial>>();
    let missing = materials.get(&missing_key_material).expect("fallback material");
    assert_eq!(
        missing.base_color_texture, None,
        "a group whose key is absent from the map gets no textured material"
    );
}

// ───────────────────────── 4. retail-gated real route ─────────────────────────

fn arena2_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("DF_ARENA2") {
        return Some(PathBuf::from(dir));
    }
    std::env::var("HOME")
        .ok()
        .map(|home| PathBuf::from(home).join("Games/Daggerfall/app/ARENA2"))
}

/// Four RGBA pixels of `TEXTURE.007` record 0, pinned from a real ARENA2 run
/// (see the comment at the assertion). Layout of the indices in the decoded
/// 64×64 image: `(x,y) = (0,0), (1,0), (0,1), (1,1)`.
const RETAIL_007_R0_GOLDEN_RGBA: [[u8; 4]; 4] = [
    [196, 154, 105, 255],
    [227, 180, 144, 255],
    [207, 152, 118, 255],
    [213, 174, 128, 255],
];

/// Missing retail runtime: skip by default, FAIL when the caller demands the
/// retail proof (`DF_REQUIRE_RETAIL` set to anything but empty/`0`).
///
/// A green suite without ARENA2 is indistinguishable from a suite that proved
/// the retail render, so a run that requires the proof can ask for it: the
/// skip then becomes a panic with the literal reason (QA MINOR-3).
fn skip_retail(reason: String) {
    let required = matches!(
        std::env::var("DF_REQUIRE_RETAIL").as_deref(),
        Ok(value) if !value.is_empty() && value != "0"
    );
    if required {
        panic!("DF_REQUIRE_RETAIL is set but the retail runtime is unavailable: {reason}");
    }
    eprintln!("SKIP m12_texture_render: {reason}");
}

/// Same route as the synthetic test, over the real `TEXTURE.007` + `PAL.PAL`
/// (Mode A, INV-L99-6: the bytes stay on the user's disk). Without ARENA2 the
/// test returns explicitly (skip, never a false PASS) — or fails, if the run
/// asked for the retail proof via `DF_REQUIRE_RETAIL`.
#[test]
fn retail_texture_007_record0_matches_pinned_pixels() {
    let Some(dir) = arena2_dir() else {
        skip_retail("$HOME is not set".to_string());
        return;
    };
    let texture_bytes = match std::fs::read(dir.join("TEXTURE.007")) {
        Ok(bytes) => bytes,
        Err(err) => {
            skip_retail(format!("runtime missing ({dir:?}/TEXTURE.007): {err}"));
            return;
        }
    };
    let palette_bytes = match std::fs::read(dir.join("PAL.PAL")) {
        Ok(bytes) => bytes,
        Err(err) => {
            skip_retail(format!("runtime missing ({dir:?}/PAL.PAL): {err}"));
            return;
        }
    };

    let archive = TextureArchive::parse(&texture_bytes).expect("retail TEXTURE.007 parses");
    assert_eq!(archive.record_count, 4, "TEXTURE.007 golden: 4 records");
    let decoded = texture::decode_frame(&texture_bytes, &archive, 0, 0)
        .expect("retail TEXTURE.007 record 0 decodes");
    assert_eq!((decoded.width, decoded.height), (64, 64));
    let palette = Palette::parse(&palette_bytes).expect("retail PAL.PAL (ColFile) parses");
    let rgba = palette.to_rgba(&decoded.pixels);

    let mut app = headless_app();
    let image_handle = {
        let mut images = app.world_mut().resource_mut::<Assets<Image>>();
        bridge::image_from_rgba(&rgba, decoded.width as u32, decoded.height as u32, &mut images)
            .expect("retail RGBA is accepted")
    };
    let material_handle = {
        let mut materials = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
        bridge::material_from_texture(
            &bridge::MaterialSpec::default(),
            image_handle.clone(),
            &mut materials,
        )
    };

    let images = app.world().resource::<Assets<Image>>();
    let image = images.get(&image_handle).expect("image in Assets<Image>");
    assert_eq!(
        (image.width(), image.height()),
        (decoded.width as u32, decoded.height as u32)
    );
    assert_eq!(image.texture_descriptor.format, TextureFormat::Rgba8UnormSrgb);
    let data = image.data.as_deref().expect("CPU-side RGBA bytes");
    assert_eq!(data.len(), 64 * 64 * 4);

    // Pins taken from a real 2026-10-22 run of this test over
    // $HOME/Games/Daggerfall/app/ARENA2 (GOG retail): the four pixels are
    // read from `image.data` at byte offsets 0, 4, 256, 260 and copied
    // verbatim below (0 retail bytes in git — only these 16 numbers).
    let mut got = [[0u8; 4]; 4];
    for (i, at) in [0usize, 4, 256, 260].into_iter().enumerate() {
        got[i].copy_from_slice(&data[at..at + 4]);
    }
    eprintln!("TEXTURE.007 record 0 pinned pixels: {got:?}");
    assert_eq!(
        got, RETAIL_007_R0_GOLDEN_RGBA,
        "retail 4 px golden (RGBA literal)"
    );

    let materials = app.world().resource::<Assets<StandardMaterial>>();
    let material = materials.get(&material_handle).expect("material");
    assert_eq!(material.base_color_texture.as_ref(), Some(&image_handle));
}

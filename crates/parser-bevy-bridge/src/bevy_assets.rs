//! `MeshSpec → Bevy` — runtime-only handles (PRE-2 §8 out: `MeshAsset`,
//! `MaterialSpec → StandardMaterial`).
//!
//! Bridge rules (PRE-2 §8): 0 persistence — everything that leaves here
//! lives in the `World`'s `Assets<T>` at runtime; nothing is packaged into
//! the binary (INV-L99-6, BUILD-CONTENT-SCAN-001). 0 winit/input types
//! (rule 5, D1): this module touches neither window nor input.
//!
//! The M1 material is neutral double-sided PBR: `double_sided` + no culling
//! guarantees block visibility without depending on the winding of the source
//! record (`convert` flat normals); roughness/metallic are the contract's
//! PBR parameters (record normals/roughness = M5).
//!
//! **M12 (DoD §c, WI-4):** the retail texture pipeline closes the
//! `TextureRecord → Handle<Image>` chain (previously "deferred" in `lib.rs`):
//! [`image_from_rgba`] (bytes decoded with the palette → `Handle<Image>`)
//! + [`scale_uvs`] (UVs in pixels → `[0,1]`) + [`split_by_texture`] (mesh
//!   → groups by texture key) + [`material_from_texture`] (material with
//!   `base_color_texture` — replaces the flat color). 0 retail bytes live
//!   here: everything arrives decoded at runtime (INV-L99-6).

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::mesh_spec::MeshSpec;

/// [`MeshSpec`] → `Handle<Mesh>` (`TriangleList` indexado).
pub fn mesh_from_spec(spec: &MeshSpec, meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, spec.positions.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, spec.normals.clone());
    // M12 (DoD §c): retail UVs in `[0,1]` (already normalized by
    // [`scale_uvs`]) must reach the `Mesh` — without `ATTRIBUTE_UV_0` the
    // textured material samples one texel and the texture never shows.
    // Synthetic meshes (`uvs` empty) keep the exact M1 vertex layout.
    if !spec.uvs.is_empty() {
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, spec.uvs.clone());
    }
    mesh.insert_indices(Indices::U32(spec.indices.clone()));
    meshes.add(mesh)
}

/// Error of the retail texture pipeline (M12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextureAssetError {
    /// Non-positive dimensions in the image to create.
    EmptyDimensions { width: u32, height: u32 },
    /// The RGBA bytes do not cover `width × height × 4`.
    DataLengthMismatch { expected: usize, got: usize },
}

impl std::fmt::Display for TextureAssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyDimensions { width, height } => {
                write!(f, "non-positive texture dimensions: {width}x{height}")
            }
            Self::DataLengthMismatch { expected, got } => {
                write!(f, "RGBA bytes {got} do not cover the declared size ({expected} expected)")
            }
        }
    }
}

impl std::error::Error for TextureAssetError {}

/// Decoded RGBA bytes → `Handle<Image>` (M12: `TextureRecord → Handle<Image>`).
///
/// `rgba` is `width × height` pixels of 4 bytes in RGBA order with a
/// top-left origin (the same origin as the raw bytes decoded with the
/// palette — see `daggerfall_parser_rs::texture::Palette::to_rgba`).
/// Format `Rgba8UnormSrgb` (palette colors are sRGB) and sampler
/// with `Repeat` in U/V: Daggerfall UVs tile outside `[0,1]`
/// (same behavior as DFU). **0 persistence** — the `Handle` lives in the
/// `World`'s `Assets<Image>` at runtime (PRE-2 §8 rule).
pub fn image_from_rgba(
    rgba: &[u8],
    width: u32,
    height: u32,
    images: &mut Assets<Image>,
) -> Result<Handle<Image>, TextureAssetError> {
    if width == 0 || height == 0 {
        return Err(TextureAssetError::EmptyDimensions { width, height });
    }
    let expected = width as usize * height as usize * 4;
    if rgba.len() != expected {
        return Err(TextureAssetError::DataLengthMismatch {
            expected,
            got: rgba.len(),
        });
    }
    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba.to_vec(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        ..default()
    });
    Ok(images.add(image))
}

/// Normalizes UVs from pixel units to `[0,1]` by dividing by the texture
/// size (M12: `DFU:MeshReader.cs:723` `dfPoint.U / sz.x`).
///
/// Output outside `[0,1]` = legitimate tiling (the [`image_from_rgba`]
/// sampler is `Repeat`). Sizes 0 are treated as 1 to avoid
/// producing `inf` (size validation lives in [`image_from_rgba`]).
pub fn scale_uvs(uvs: &[[f32; 2]], width: u32, height: u32) -> Vec<[f32; 2]> {
    let w = width.max(1) as f32;
    let h = height.max(1) as f32;
    uvs.iter().map(|[u, v]| [u / w, v / h]).collect()
}

/// A group of mesh geometry that shares ONE texture key.
#[derive(Debug, Clone, PartialEq)]
pub struct TextureGroup {
    /// [`crate::mesh_spec::texture_key`] key (`NO_TEXTURE` = synthetic).
    pub texture_key: u32,
    /// Re-mapped sub-mesh (local indices, uniform `face_textures`).
    pub mesh: MeshSpec,
}

/// Splits a [`MeshSpec`] into groups by texture key (M12).
///
/// Iterates the triangle soup in order (`indices` in triples) and remaps
/// vertices per group: the 3 vertices of a triangle share a key (M12
/// invariant of [`MeshSpec`]). Output sorted by ascending key
/// (`BTreeMap`) ⇒ deterministic. Geometry without aligned UVs/`face_textures`
/// (synthetic mesh) is grouped under [`crate::mesh_spec::NO_TEXTURE`] — no
/// vertex is discarded, the caller decides the material of that group.
pub fn split_by_texture(spec: &MeshSpec) -> Vec<TextureGroup> {
    if spec.face_textures.len() != spec.positions.len() || spec.uvs.len() != spec.positions.len() {
        return Vec::new();
    }
    #[derive(Default)]
    struct GroupBuilder {
        positions: Vec<[f32; 3]>,
        normals: Vec<[f32; 3]>,
        uvs: Vec<[f32; 2]>,
        indices: Vec<u32>,
        /// Source vertex → local index (per-group remap).
        map: std::collections::HashMap<u32, u32>,
    }

    let mut groups: std::collections::BTreeMap<u32, GroupBuilder> =
        std::collections::BTreeMap::new();
    for tri in spec.indices.chunks_exact(3) {
        let key = spec.face_textures[tri[0] as usize];
        debug_assert!(
            tri.iter().all(|idx| spec.face_textures[*idx as usize] == key),
            "triangle with mixed texture keys (invariant M12 broken)"
        );
        let group = groups.entry(key).or_default();
        for idx in tri {
            let local = match group.map.get(idx) {
                Some(local) => *local,
                None => {
                    let local = group.positions.len() as u32;
                    group.positions.push(spec.positions[*idx as usize]);
                    group.normals.push(spec.normals[*idx as usize]);
                    group.uvs.push(spec.uvs[*idx as usize]);
                    group.map.insert(*idx, local);
                    local
                }
            };
            group.indices.push(local);
        }
    }
    groups
        .into_iter()
        .map(|(texture_key, group)| {
            let vertices = group.uvs.len();
            TextureGroup {
                texture_key,
                mesh: MeshSpec {
                    positions: group.positions,
                    normals: group.normals,
                    indices: group.indices,
                    uvs: group.uvs,
                    face_textures: vec![texture_key; vertices],
                },
            }
        })
        .collect()
}

/// Neutral PBR parameters (contract PRE-2 §8 `MaterialSpec → StandardMaterial`).
///
/// Default values: light matte stone — readable under directional +
/// ambient light without a texture (textures = M2+).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaterialSpec {
    /// Base RGBA color (linear).
    pub base_color: [f32; 4],
    /// Perceptual PBR roughness (0 = mirror, 1 = matte).
    pub perceptual_roughness: f32,
    /// PBR metallic (0 = dielectric).
    pub metallic: f32,
    /// Constant lighting without PBR shading (M3: unlit sky and clouds).
    ///
    /// M3 addition: default `false` = byte-identical M1/M2 PBR material;
    /// `true` = flat color ignoring lights (required so
    /// the sky/clouds look "clear day" with the scene's directional light
    /// pointing downward).
    pub unlit: bool,
}

impl Default for MaterialSpec {
    fn default() -> Self {
        Self {
            base_color: [0.62, 0.58, 0.50, 1.0],
            perceptual_roughness: 0.92,
            metallic: 0.0,
            unlit: false,
        }
    }
}

/// [`MaterialSpec`] → `Handle<StandardMaterial>` (double-sided, no culling).
pub fn material_from_spec(
    spec: &MaterialSpec,
    materials: &mut Assets<StandardMaterial>,
) -> Handle<StandardMaterial> {
    standard_material(spec, None, materials)
}

/// [`MaterialSpec`] + retail texture → `Handle<StandardMaterial>` (M12).
///
/// Same PBR material as [`material_from_spec`] with `base_color_texture`
/// assigned: the base color acts as a tint over the texture (WHITE =
/// texture as-is). Replaces the flat-color material of the windows
/// for meshes with real UVs/`face_textures` (DoD §c, WI-4).
pub fn material_from_texture(
    spec: &MaterialSpec,
    texture: Handle<Image>,
    materials: &mut Assets<StandardMaterial>,
) -> Handle<StandardMaterial> {
    standard_material(spec, Some(texture), materials)
}

/// Single `StandardMaterial` constructor (0 parameter duplication).
fn standard_material(
    spec: &MaterialSpec,
    texture: Option<Handle<Image>>,
    materials: &mut Assets<StandardMaterial>,
) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: Color::srgba(
            spec.base_color[0],
            spec.base_color[1],
            spec.base_color[2],
            spec.base_color[3],
        ),
        base_color_texture: texture,
        perceptual_roughness: spec.perceptual_roughness,
        metallic: spec.metallic,
        unlit: spec.unlit,
        double_sided: true,
        cull_mode: None,
        ..default()
    })
}

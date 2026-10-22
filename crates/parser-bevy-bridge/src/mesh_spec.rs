//! `MeshSpec` — Bevy-independent normalized geometry.
//!
//! Contract: addendum PRE-2 §8 (internal `MeshSpec` interface) + decision
//! PRE-3 §10.1 (bevy-bridge-matrix): collapsing into the direct
//! `MeshRecord → Handle<Mesh>` contract was REJECTED; `MeshSpec` is
//! KEPT as a pure containment type, testable without Bevy (AB-11,
//! parser clean-room intact).
//!
//! Coordinate convention (delta documented in M1, criterion (g)):
//! - Daggerfall: Y up, left-handed; Bevy: Y up, right-handed.
//!   Conversion: `(x, y, z)_df → (x, y, -z)_bevy`.
//! - Scale (M10-FIX3): `MeshSpec` is already in meters;
//!   ARCH3D vertices = raw ÷ 256 (`Arch3dFile.cs:39` pointDivisor) ÷ 40
//!   (`MeshReader.cs:58` GlobalScale) = raw ÷ 10240; RMB positions =
//!   raw ÷ 40 (`convert::UNITS_PER_METER`). (The M1 note documented
//!   "1 m = 64 units" — latent parity bug fixed in M10-FIX3.)
//! - Topology: flat-shaded triangle soup (per-face normals), no
//!   smoothing — emulates Daggerfall's flat shading without depending on the
//!   Normal section of the spec (flagged "not entirely confirmed" in
//!   dfbsa.txt).
//! - UVs/textures (M12, DoD §c / WI-4): [`MeshSpec::uvs`] +
//!   [`MeshSpec::face_textures`] carry the retail texture per vertex
//!   (decoding in `convert`, see its header); synthetic meshes
//!   leave both arrays empty (no texture = neutral PBR material).

/// "No texture" key in [`MeshSpec::face_textures`] (synthetic).
pub const NO_TEXTURE: u32 = u32::MAX;

/// Texture key per `(archive, record)` pair — DFU `GetTextureIndices`
/// convention (`archive = id / 1000`, `record = id % 1000`): the
/// ARENA2 file is `TEXTURE.{archive:03}` and the sub-image is `record`.
pub const fn texture_key(archive: u16, record: u16) -> u32 {
    archive as u32 * 1000 + record as u32
}

/// Inverse of [`texture_key`] `(archive, record)`.
pub const fn texture_key_parts(key: u32) -> (u16, u16) {
    ((key / 1000) as u16, (key % 1000) as u16)
}

/// Normalized mesh geometry: triangles with flat per-face normals.
///
/// `positions`/`normals`/`indices` have the same length; `indices`
/// indexes consecutive triples (`TriangleList`).
///
/// **Invariant M12:** `uvs`/`face_textures` are either empty (synthetic mesh,
/// no texture) or aligned 1:1 with `positions` (one value per vertex; the
/// 3 vertices of a triangle share `face_textures` because the soup is
/// flat and the fan comes from ONE ARCH3D plane).
#[derive(Debug, Clone, PartialEq)]
pub struct MeshSpec {
    /// Vertices in meters, Bevy handedness (right), Y up.
    pub positions: Vec<[f32; 3]>,
    /// Unit per-vertex normals (flat: 3 vertices per triangle).
    pub normals: Vec<[f32; 3]>,
    /// Indices in triples (`TriangleList`).
    pub indices: Vec<u32>,
    /// Per-vertex UV in texture **pixel units** (DF UV ÷ 16,
    /// `DFU:Arch3dFile.cs` `textureDivisor = 16.0f`); empty = no UV.
    /// The final normalization to `[0,1]` divides by the texture size
    /// (`DFU:MeshReader.cs:723` `dfPoint.U / sz.x`) — see
    /// [`crate::bevy_assets::scale_uvs`].
    pub uvs: Vec<[f32; 2]>,
    /// Texture key ([`texture_key`]) per vertex; [`NO_TEXTURE`] on
    /// synthetic vertices; empty = no texture data.
    pub face_textures: Vec<u32>,
}

impl MeshSpec {
    /// Synthetic mesh without texture (empty `uvs`/`face_textures`) — the
    /// constructor for all geometry that does not come from ARCH3D.
    pub fn new(positions: Vec<[f32; 3]>, normals: Vec<[f32; 3]>, indices: Vec<u32>) -> Self {
        Self {
            positions,
            normals,
            indices,
            uvs: Vec::new(),
            face_textures: Vec::new(),
        }
    }

    /// Do the texture arrays respect the invariant (empty or aligned)?
    pub fn texture_alignment_valid(&self) -> bool {
        (self.uvs.is_empty() || self.uvs.len() == self.positions.len())
            && (self.face_textures.is_empty()
                || self.face_textures.len() == self.positions.len())
    }

    /// Number of triangles in the mesh.
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Bounding box `(min, max)` of the vertices; `None` if there are no vertices.
    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let mut iter = self.positions.iter();
        let first = *iter.next()?;
        let mut min = first;
        let mut max = first;
        for p in iter {
            for axis in 0..3 {
                min[axis] = min[axis].min(p[axis]);
                max[axis] = max[axis].max(p[axis]);
            }
        }
        Some((min, max))
    }

    /// Center of the bounding box; `None` if there are no vertices.
    pub fn center(&self) -> Option<[f32; 3]> {
        let (min, max) = self.bounds()?;
        Some([
            (min[0] + max[0]) * 0.5,
            (min[1] + max[1]) * 0.5,
            (min[2] + max[2]) * 0.5,
        ])
    }

    /// Approximate radius: half the diagonal of the bounding box.
    pub fn bounding_radius(&self) -> Option<f32> {
        let (min, max) = self.bounds()?;
        let d = [
            max[0] - min[0],
            max[1] - min[1],
            max[2] - min[2],
        ];
        Some((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() * 0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_triangle() -> MeshSpec {
        MeshSpec::new(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![[0.0, 0.0, 1.0]; 3],
            vec![0, 1, 2],
        )
    }

    #[test]
    fn triangle_count_and_bounds() {
        let spec = one_triangle();
        assert_eq!(spec.triangle_count(), 1);
        let (min, max) = spec.bounds().expect("bounds");
        assert_eq!(min, [0.0, 0.0, 0.0]);
        assert_eq!(max, [1.0, 1.0, 0.0]);
        let center = spec.center().expect("center");
        assert_eq!(center, [0.5, 0.5, 0.0]);
        let r = spec.bounding_radius().expect("radius");
        assert!((r - (2.0f32.sqrt() * 0.5)).abs() < 1e-6);
    }

    #[test]
    fn empty_spec_has_no_bounds() {
        let spec = MeshSpec::new(Vec::new(), Vec::new(), Vec::new());
        assert!(spec.bounds().is_none());
        assert!(spec.center().is_none());
        assert!(spec.bounding_radius().is_none());
        assert_eq!(spec.triangle_count(), 0);
        assert!(spec.uvs.is_empty() && spec.face_textures.is_empty());
        assert!(spec.texture_alignment_valid());
    }

    #[test]
    fn texture_key_roundtrip_dfu_convention() {
        // DFU GetTextureIndices convention: archive = id/1000, record = id%1000.
        assert_eq!(texture_key(70, 5), 70_005);
        assert_eq!(texture_key_parts(70_005), (70, 5));
        assert_eq!(texture_key_parts(texture_key(474, 127)), (474, 127));
        assert_ne!(texture_key(1, 0), NO_TEXTURE);
    }

    #[test]
    fn synthetic_spec_has_no_texture_arrays() {
        let spec = one_triangle();
        assert!(spec.uvs.is_empty());
        assert!(spec.face_textures.is_empty());
    }

    #[test]
    fn texture_alignment_detects_misaligned_arrays() {
        let mut spec = one_triangle();
        spec.uvs = vec![[0.0, 0.0]; 2]; // misaligned (positions = 3)
        assert!(!spec.texture_alignment_valid());
        spec.uvs = vec![[0.0, 0.0]; 3];
        spec.face_textures = vec![texture_key(1, 2); 3];
        assert!(spec.texture_alignment_valid());
    }
}

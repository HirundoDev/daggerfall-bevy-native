//! `MeshRecord → MeshSpec` conversion — 0 Bevy dependency.
//!
//! Contract: addendum PRE-2 §8 (`MeshRecord` in, internal `MeshSpec`
//! interface; headless-testable, rule 4). The bridge uses ONLY the parser's
//! public API (`parse_mesh_header` + `compute_sections` + record bytes
//! via `bsa::entry_data`); the parser is not modified (AB-11).
//!
//! Decoding spec: UESP `dfbsa.txt` (S1, sha256
//! `73921ac5208f874c8b6bde972d8765c66be81d1a2b7fc87f8b491096acc911cc`,
//! pinned in `docs/sintergic/inputs/2026-10-18-fase1-input-manifest.md`),
//! "Plane Data Record Section":
//! - plane record = `8 + PlanePointCount*8` bytes; `ppc ∈ 0x00..=0x18`;
//! - `ppc = 0` → plane without visible geometry (collision); it is skipped;
//! - sub-record per point (8 bytes): `PointOffset u32 LE` + `TextureU/V i16`;
//! - **v2.7/v2.6**: `PointOffset ÷ 12` = point index (confirmed in the
//!   spec, except the anomalous object `0x13274C6`);
//! - **v2.5**: `PointOffset ÷ 4` (spec: "unconfirmed") — verified here
//!   against the retail build by the runtime test (all indices < point_count).
//!
//! Documented deltas:
//! - Flat normals derived from the geometry (Normal section of the spec
//!   marked "not entirely confirmed"; spec normal decoding
//!   = M2+).
//! - UVs/textures: RESOLVED in M12 (DoD §c / WI-4): each
//!   plane contributes its `Texture` (u16 bitfield: `archive = tb >> 7`,
//!   `record = tb & 0x7F` — `DFU:Arch3dFile.cs:615-624`) and its `TextureU/V`
//!   per point → [`MeshSpec::uvs`] (pixel units, DF UV ÷ 16 —
//!   `DFU:Arch3dFile.cs:40` `textureDivisor = 16.0f`,
//!   `Arch3dFile.cs:947-948`) + [`MeshSpec::face_textures`]. The linear
//!   model of UESP Appendix E is replicated as in DFU
//!   (`Arch3dFile.WritePlane`/`FaceUVTool`): absolute `UV[0], UV[1]` delta of
//!   `UV[0], UV[2]` delta of `UV[1]+UV[0]`, points ≥3 through the affine map of the
//!   basis (p1−p0, p2−p0) — our re-derivation from the linear
//!   statement, 0 third-party code copied (clean-room AB-11). The sign of V
//!   is NOT negated: DFU negates V for Unity's bottom-left origin; Bevy
//!   images use the top-left origin of the raw data (same
//!   origin as the decoded palette).

use daggerfall_parser_rs::mesh::{
    compute_sections, parse_mesh_header, MeshError, MeshVersion, SectionError,
};

use crate::mesh_spec::{texture_key, MeshSpec};

/// Native Daggerfall units per meter — DFU convention for RMB/block
/// positions: `DFU:MeshReader.cs:58` `GlobalScale = 0.025f` (RMB
/// coordinates are multiplied by 0.025 ⇒ 40 units = 1 m; consistent with
/// `DFU:RMBLayout.cs:34` `RMBSide = 4096 × GlobalScale = 102.4 m`).
///
/// **M10-FIX3:** the value frozen in M1 was `64.0` — a latent parity bug,
/// not a regression (the DFU spec mandates 40 u/m). ARCH3D
/// vertices are also 16.8 fixed point: see [`ARCH3D_POINT_DIVISOR`].
///
/// `pub(crate)`: [`crate::city_retail`] uses this SAME scale for the RMB
/// positions (0 duplicated scale constants in the crate).
pub(crate) const UNITS_PER_METER: f32 = 40.0;

/// 16.8 fixed point of the ARCH3D vertices: `DFU:Arch3dFile.cs:39`
/// `pointDivisor = 256.0f` — `WritePoint` (`Arch3dFile.cs:941`,
/// "Vector coordinates are divided by 256.0f") divides each raw coordinate
/// by 256 **before** `MeshReader.LoadVertices` (`MeshReader.cs:717`)
/// applies `GlobalScale`. Vertex in meters = `raw / 256 / 40 = raw / 10240`
/// (numeric check: record 212 with raw `Size` (106496, 86528, 172032)
/// ⇒ 10.4 × 8.4 × 16.8 m = realistic retail house).
const ARCH3D_POINT_DIVISOR: f32 = 256.0;

/// Raw ARCH3D vertex units per meter (`256 × 40 = 10240`, exact
/// in f32) — the only extra factor of the format compared to [`UNITS_PER_METER`].
const ARCH3D_UNITS_PER_METER: f32 = ARCH3D_POINT_DIVISOR * UNITS_PER_METER;

/// Divisor of the Daggerfall texture coordinates to pixel units:
/// `DFU:Arch3dFile.cs:40` `textureDivisor = 16.0f` (`WritePoint` doc:
/// "texture coordinates are divided by 16.0f"). Consistent with UESP Appendix
/// E: 1024 DF-UV = 64 px ⇒ `NU = DFUV / (16 × width_px)`.
const TEXTURE_UV_DIVISOR: f32 = 16.0;

/// Bound of a "packed" coordinate (DFU `UVunpack`: outside
/// (−14336, 14336) the coordinate carries a multiple of 8192 that must be
/// subtracted). −7168 is the only known exception in range (same DFU note).
const UV_PACKED_MIN: i32 = -14336;
const UV_PACKED_MAX: i32 = 14336;
/// Packed exception within range (DFU `UVunpack`: `u != -7168`).
const UV_PACKED_EXCEPTION: i32 = -7168;
/// Packing step of the texture coordinates (DFU `UVunpack`).
const UV_PACK_STEP: i32 = 8192;

/// Unpacks a Daggerfall texture coordinate (DFU `UVunpack` rule,
/// observed retail behavior): only coordinates outside (−14336, 14336) —
/// or exactly −7168 — are "packed"; the nearest multiple of 8192 is
/// subtracted from them.
fn uv_unpack(u: i32) -> i32 {
    if u > UV_PACKED_MIN && u < UV_PACKED_MAX && u != UV_PACKED_EXCEPTION {
        return u;
    }
    // Nearest multiple of 8192 (ties upward, like DFU).
    let next_mult = (((u - 1) >> 13) + 1) << 13;
    let prev_mult = next_mult - UV_PACK_STEP;
    let mult = if u - prev_mult < next_mult - u { prev_mult } else { next_mult };
    u - mult
}

/// Conversion error of an ARCH3D record to [`MeshSpec`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConvertError {
    /// The 64-byte header does not parse (delegated from the parser).
    Mesh(MeshError),
    /// The header sections exceed the record (delegated from the parser).
    Section(SectionError),
    /// `PointOffset` of a sub-record is not a multiple of the version divisor.
    PointOffsetMisaligned { plane_index: usize, point_offset: u32, divisor: u32 },
    /// Derived point index outside the `[0, point_count)` range.
    PointIndexOutOfRange { plane_index: usize, point_index: u32, point_count: u32 },
    /// The record produced not a single visible triangle.
    NoVisibleGeometry { point_count: u32, plane_count: u32 },
}

impl std::fmt::Display for ConvertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mesh(e) => write!(f, "invalid mesh header: {e}"),
            Self::Section(e) => write!(f, "invalid sections: {e}"),
            Self::PointOffsetMisaligned { plane_index, point_offset, divisor } => write!(
                f,
                "plane {plane_index}: PointOffset {point_offset} is not a multiple of {divisor}"
            ),
            Self::PointIndexOutOfRange { plane_index, point_index, point_count } => write!(
                f,
                "plane {plane_index}: point index {point_index} >= {point_count}"
            ),
            Self::NoVisibleGeometry { point_count, plane_count } => write!(
                f,
                "record without visible geometry ({point_count} points, {plane_count} planes)"
            ),
        }
    }
}

impl std::error::Error for ConvertError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Mesh(e) => Some(e),
            Self::Section(e) => Some(e),
            _ => None,
        }
    }
}

impl From<MeshError> for ConvertError {
    fn from(e: MeshError) -> Self {
        Self::Mesh(e)
    }
}

impl From<SectionError> for ConvertError {
    fn from(e: SectionError) -> Self {
        Self::Section(e)
    }
}

fn i32_le(record: &[u8], at: usize) -> i32 {
    i32::from_le_bytes([record[at], record[at + 1], record[at + 2], record[at + 3]])
}

fn u32_le(record: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([record[at], record[at + 1], record[at + 2], record[at + 3]])
}

fn i16_le(record: &[u8], at: usize) -> i16 {
    i16::from_le_bytes([record[at], record[at + 1]])
}

fn u16_le(record: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([record[at], record[at + 1]])
}

/// Absolute UVs (pixel units) of the `ppc` points of a plane:
/// absolute UV[0], UV[1] delta of UV[0], UV[2] delta of UV[1]+UV[0]
/// (UESP Appendix E, DFU behavior `Arch3dFile.WritePlane`/
/// `FaceUVTool.ComputeFaceUVCoordinates`); points ≥3 come from the affine
/// map over the basis (p1−p0, p2−p0) — clean-room re-derivation of the
/// linear Appendix E model. `raw_uv` are the raw `TextureU/V` per
/// point (≥3 entries); `positions` are the plane points in the same order.
fn plane_uvs(raw_uv: &[[i16; 2]], positions: &[[f32; 3]]) -> Vec<[f32; 2]> {
    let unpack = |raw: [i16; 2]| -> [f32; 2] {
        [
            uv_unpack(raw[0] as i32) as f32 / TEXTURE_UV_DIVISOR,
            uv_unpack(raw[1] as i32) as f32 / TEXTURE_UV_DIVISOR,
        ]
    };
    let uv0 = unpack(raw_uv[0]);
    let uv1_raw = unpack(raw_uv[1]);
    let uv2_raw = unpack(raw_uv[2]);
    let uv1 = [uv0[0] + uv1_raw[0], uv0[1] + uv1_raw[1]];
    let uv2 = [uv1[0] + uv2_raw[0], uv1[1] + uv2_raw[1]];
    let mut out = vec![[0.0f32; 2]; raw_uv.len()];
    out[0] = uv0;
    out[1] = uv1;
    out[2] = uv2;
    // Affine basis (p1−p0, p2−p0) for the fan points ≥3 (plane guaranteed
    // non-degenerate: only triangles with a computable normal are pushed).
    let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let e1 = sub(positions[1], positions[0]);
    let e2 = sub(positions[2], positions[0]);
    let e1e1 = dot(e1, e1);
    let e2e2 = dot(e2, e2);
    let e1e2 = dot(e1, e2);
    let det = e1e1 * e2e2 - e1e2 * e1e2;
    for k in 3..raw_uv.len() {
        if det.abs() <= f32::EPSILON {
            out[k] = uv2;
            continue;
        }
        let w = sub(positions[k], positions[0]);
        let we1 = dot(w, e1);
        let we2 = dot(w, e2);
        let alpha = (we1 * e2e2 - we2 * e1e2) / det;
        let beta = (we2 * e1e1 - we1 * e1e2) / det;
        out[k] = [
            uv0[0] + alpha * (uv1[0] - uv0[0]) + beta * (uv2[0] - uv0[0]),
            uv0[1] + alpha * (uv1[1] - uv0[1]) + beta * (uv2[1] - uv0[1]),
        ];
    }
    out
}

/// `PointOffset → point index` divisor per version (dfbsa.txt).
fn point_index_divisor(version: MeshVersion) -> u32 {
    match version {
        // v2.5: ÷4 (spec "unconfirmed"; verified against retail at runtime).
        MeshVersion::V2_5 => 4,
        // v2.6/v2.7: ÷12 (spec "confirmed for all v2.7 objects except one").
        MeshVersion::V2_6 | MeshVersion::V2_7 => 12,
    }
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// Normalizes; `None` if the vector is degenerate (length ~0).
fn try_normalize(v: [f32; 3]) -> Option<[f32; 3]> {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len <= f32::EPSILON {
        None
    } else {
        Some([v[0] / len, v[1] / len, v[2] / len])
    }
}

/// Converts an ARCH3D.BSA record (header + mesh bytes, via the parser's
/// public API) into a normalized [`MeshSpec`] (meters, Bevy handedness).
///
/// Bounds-checks: the section offsets are validated by `compute_sections`
/// (parser, already tested); here every derived `PointOffset` is re-validated.
pub fn mesh_record_to_spec(record: &[u8]) -> Result<MeshSpec, ConvertError> {
    let header = parse_mesh_header(record)?;
    let sections = compute_sections(&header, record)?;
    let divisor = point_index_divisor(header.version);

    // Points (12 B: 3×i32 LE, 16.8 fixed point) → meters + Bevy handedness
    // (y negated, z negated). Scale: `raw / 256 (Arch3dFile.WritePoint) / 40
    // (GlobalScale)`. M10-FIX3b: the raw ARCH3D Y is
    // Y-DOWN (measured: records 125/131/138/212/430/444/744-752 all with
    // Y ∈ [−S, 0]) and DFU negates it on load (`MeshReader.cs:717`
    // `new Vector3(dfPoint.X, -dfPoint.Y, dfPoint.Z) * scale`); without the
    // negation the retail city ended up half-buried (bounds Y
    // [−14.9, 4.4] → readability 0/8 in PROBE-RMB-001).
    let mut points: Vec<[f32; 3]> = Vec::with_capacity(header.point_count as usize);
    for i in 0..header.point_count as usize {
        let at = header.point_offset as usize + i * 12;
        let x = i32_le(record, at) as f32 / ARCH3D_UNITS_PER_METER;
        let y = -(i32_le(record, at + 4) as f32) / ARCH3D_UNITS_PER_METER;
        let z = -(i32_le(record, at + 8) as f32) / ARCH3D_UNITS_PER_METER;
        points.push([x, y, z]);
    }

    // Planes → flat-shaded triangle soup (fan (0, k, k+1)) with UVs and
    // per-plane texture (M12).
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut face_textures: Vec<u32> = Vec::new();

    let mut offset = header.plane_offset as usize;
    for plane_index in 0..header.plane_count as usize {
        let ppc = record[offset] as usize;
        if ppc == 0 {
            // Collision plane without visible geometry (dfbsa.txt: ppc 0x00).
            offset += 8;
            continue;
        }

        // Plane texture: u16 bitfield after ppc+Unknown1
        // (`DFU:Arch3dFile.cs:615-624`: `archive = tb >> 7` (9 bits),
        // `record = tb & 0x7F` (7 bits) → TEXTURE.{archive:03} file).
        let texture_tb = u16_le(record, offset + 2);
        let texture = texture_key(texture_tb >> 7, texture_tb & 0x7F);

        // Plane vertices, with a bounds-check of every derived PointOffset.
        let mut plane_points: Vec<[f32; 3]> = Vec::with_capacity(ppc);
        let mut raw_uv: Vec<[i16; 2]> = Vec::with_capacity(ppc);
        for k in 0..ppc {
            let sub_at = offset + 8 + k * 8;
            let point_offset_raw = u32_le(record, sub_at);
            if !point_offset_raw.is_multiple_of(divisor) {
                return Err(ConvertError::PointOffsetMisaligned {
                    plane_index,
                    point_offset: point_offset_raw,
                    divisor,
                });
            }
            let point_index = point_offset_raw / divisor;
            if point_index >= header.point_count {
                return Err(ConvertError::PointIndexOutOfRange {
                    plane_index,
                    point_index,
                    point_count: header.point_count,
                });
            }
            plane_points.push(points[point_index as usize]);
            raw_uv.push([i16_le(record, sub_at + 4), i16_le(record, sub_at + 6)]);
        }

        // Plane UV in pixel units (only planes with ≥3 points
        // emit triangles; ppc 1..2 does not need UVs).
        let plane_uv = if ppc >= 3 {
            Some(plane_uvs(&raw_uv, &plane_points))
        } else {
            None
        };

        // Fan triangulation with a flat normal per face (degenerate triangles
        // are skipped: 0 invented normals).
        for k in 1..ppc.saturating_sub(1) {
            let a = plane_points[0];
            let b = plane_points[k];
            let c = plane_points[k + 1];
            let Some(normal) = try_normalize(cross(sub(b, a), sub(c, a))) else {
                continue;
            };
            let base = positions.len() as u32;
            positions.push(a);
            positions.push(b);
            positions.push(c);
            normals.push(normal);
            normals.push(normal);
            normals.push(normal);
            if let Some(puv) = &plane_uv {
                uvs.push(puv[0]);
                uvs.push(puv[k]);
                uvs.push(puv[k + 1]);
            }
            face_textures.push(texture);
            face_textures.push(texture);
            face_textures.push(texture);
            indices.push(base);
            indices.push(base + 1);
            indices.push(base + 2);
        }

        offset += 8 + ppc * 8;
    }

    // The plane walk replicates the size formula of `compute_sections`.
    debug_assert_eq!(offset as u64, sections.planes_end, "plane walk != compute_sections");

    if positions.is_empty() {
        return Err(ConvertError::NoVisibleGeometry {
            point_count: header.point_count,
            plane_count: header.plane_count,
        });
    }

    let spec = MeshSpec {
        positions,
        normals,
        indices,
        uvs,
        face_textures,
    };
    debug_assert!(spec.texture_alignment_valid(), "uvs/face_textures misaligned");
    Ok(spec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use daggerfall_parser_rs::mesh::MESH_HEADER_LEN;

    /// Sub-record of a synthetic plane: `(raw PointOffset, TextureU, TextureV)`.
    type PlaneSub = (u32, i16, i16);
    /// Synthetic plane: `(points per face, Texture bitfield, sub-records)`.
    type PlaneFixture<'a> = (u8, u16, &'a [PlaneSub]);

    /// Builds a synthetic record with full control: raw points,
    /// per-plane texture (u16 bitfield) and `TextureU/V` per sub-record.
    fn build_record_full(
        version: &[u8; 4],
        points: &[[i32; 3]],
        planes: &[PlaneFixture<'_>],
    ) -> Vec<u8> {
        let point_count = points.len() as u32;
        let point_offset = MESH_HEADER_LEN as u32;
        let points_bytes = point_count * 12;
        let plane_offset = point_offset + points_bytes;
        let planes_bytes: u32 = planes
            .iter()
            .map(|(ppc, _, _)| 8 + *ppc as u32 * 8)
            .sum();
        let normal_offset = plane_offset + planes_bytes;

        let mut r = vec![0u8; MESH_HEADER_LEN];
        r[0..4].copy_from_slice(version);
        r[4..8].copy_from_slice(&point_count.to_le_bytes());
        r[8..12].copy_from_slice(&(planes.len() as u32).to_le_bytes());
        r[24..28].copy_from_slice(&120u32.to_le_bytes()); // Data1Offset (non-zero)
        r[48..52].copy_from_slice(&point_offset.to_le_bytes());
        r[52..56].copy_from_slice(&normal_offset.to_le_bytes());
        r[60..64].copy_from_slice(&plane_offset.to_le_bytes());
        // The header is only the first MESH_HEADER_LEN bytes: the point
        // block starts at `point_offset`, outside the header — the buffer
        // is extended up to the start of the planes before writing them.
        r.resize(plane_offset as usize, 0);

        // Raw points (16.8 fixed point: 10240 raw = 1 m).
        for (i, p) in points.iter().enumerate() {
            let at = (point_offset + i as u32 * 12) as usize;
            r[at..at + 4].copy_from_slice(&p[0].to_le_bytes());
            r[at + 4..at + 8].copy_from_slice(&p[1].to_le_bytes());
            r[at + 8..at + 12].copy_from_slice(&p[2].to_le_bytes());
        }

        // Planes: PointOffsets + TextureU/V + texture bitfield.
        for (ppc, texture_tb, subs) in planes {
            let mut plane = vec![0u8; 8 + *ppc as usize * 8];
            plane[0] = *ppc;
            plane[2..4].copy_from_slice(&texture_tb.to_le_bytes());
            for (k, (raw, u, v)) in subs.iter().enumerate() {
                let at = 8 + k * 8;
                plane[at..at + 4].copy_from_slice(&raw.to_le_bytes());
                plane[at + 4..at + 6].copy_from_slice(&u.to_le_bytes());
                plane[at + 6..at + 8].copy_from_slice(&v.to_le_bytes());
            }
            r.extend_from_slice(&plane);
        }
        r
    }

    /// Builds a simple synthetic record (texture 0, UVs 0): points by
    /// index and PointOffset offsets in the given order.
    fn build_record(version: &[u8; 4], point_count: u32, planes: &[(u8, &[u32])]) -> Vec<u8> {
        let points: Vec<[i32; 3]> = (0..point_count)
            .map(|i| {
                let (x, z) = match i {
                    0 => (0i32, 0i32),
                    1 => (20_480, 0),
                    2 => (0, 20_480),
                    _ => (40_960, 0), // collinear with 0 and 1 on the X axis (for the degenerate test)
                };
                [x, 0, z]
            })
            .collect();
        let full: Vec<(u8, u16, Vec<PlaneSub>)> = planes
            .iter()
            .map(|(ppc, offs)| (*ppc, 0u16, offs.iter().map(|o| (*o, 0, 0)).collect()))
            .collect();
        let borrowed: Vec<PlaneFixture<'_>> = full
            .iter()
            .map(|(ppc, tb, subs)| (*ppc, *tb, subs.as_slice()))
            .collect();
        build_record_full(version, &points, &borrowed)
    }

    fn canonical_offsets_v27() -> Vec<u8> {
        build_record(b"v2.7", 3, &[(3, &[0, 12, 24])])
    }

    #[test]
    fn converts_canonical_v27_triangle() {
        let record = canonical_offsets_v27();
        let spec = mesh_record_to_spec(&record).expect("convert");
        assert_eq!(spec.triangle_count(), 1);
        assert_eq!(spec.indices, vec![0, 1, 2]);
        // Scale: raw 20480 = 2 m (÷256 pointDivisor ÷40 GlobalScale);
        // handedness: z negated (0 → 0).
        assert_eq!(spec.positions[1], [2.0, 0.0, 0.0]);
        assert_eq!(spec.positions[2], [0.0, 0.0, -2.0]);
        // Flat normal points up (winding (b-a)×(c-a) = +Y here).
        assert_eq!(spec.normals[0], [0.0, 1.0, 0.0]);
        // unit normals
        for n in &spec.normals {
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            assert!((len - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn v25_uses_divisor_4() {
        // Same geometry, v2.5 offsets (÷4): 0, 4, 8.
        let record = build_record(b"v2.5", 3, &[(3, &[0, 4, 8])]);
        let spec = mesh_record_to_spec(&record).expect("convert v2.5");
        assert_eq!(spec.triangle_count(), 1);
        assert_eq!(spec.positions[1], [2.0, 0.0, 0.0]);
    }

    #[test]
    fn rejects_misaligned_point_offset() {
        let record = build_record(b"v2.7", 3, &[(3, &[0, 13, 24])]);
        let err = mesh_record_to_spec(&record).expect_err("misaligned");
        assert!(matches!(
            err,
            ConvertError::PointOffsetMisaligned { plane_index: 0, point_offset: 13, divisor: 12 }
        ));
    }

    #[test]
    fn rejects_out_of_range_point_index() {
        let record = build_record(b"v2.7", 3, &[(3, &[0, 12, 36])]);
        let err = mesh_record_to_spec(&record).expect_err("out of range");
        assert!(matches!(
            err,
            ConvertError::PointIndexOutOfRange { point_index: 3, point_count: 3, .. }
        ));
    }

    #[test]
    fn collision_plane_produces_no_visible_geometry() {
        // A single plane with ppc=0 → no triangles.
        let record = build_record(b"v2.7", 3, &[(0, &[])]);
        let err = mesh_record_to_spec(&record).expect_err("no geometry");
        assert!(matches!(
            err,
            ConvertError::NoVisibleGeometry { point_count: 3, plane_count: 1 }
        ));
    }

    #[test]
    fn skips_degenerate_plane_but_keeps_valid_one() {
        // Degenerate plane 0 (points 0,1,3 collinear on X) + valid plane.
        let record = build_record(b"v2.7", 4, &[(3, &[0, 12, 36]), (3, &[0, 12, 24])]);
        let spec = mesh_record_to_spec(&record).expect("convert");
        // Only the second plane contributes a non-degenerate triangle.
        assert_eq!(spec.triangle_count(), 1);
    }

    #[test]
    fn rejects_bad_header() {
        let short = [0u8; 63];
        let err = mesh_record_to_spec(&short).expect_err("short");
        assert!(matches!(err, ConvertError::Mesh(_)));
    }
}

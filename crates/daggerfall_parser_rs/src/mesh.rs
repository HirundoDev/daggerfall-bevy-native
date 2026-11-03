//! 3D records of ARCH3D.BSA — clean-room from UESP `dfbsa.txt`
//! ("3D Object Records", "3D Object Header", "Plane Data Record Section" sections).
//!
//! A record contains a fixed 64-byte header followed by data sections
//! (points, planes, normals, …) whose offsets the header itself carries:
//!
//! ```text
//! [3D Object Header (64 bytes)][Point Data][Plane Data][Normal?][Offset1][Offset2 …]
//! ```
//! - Each point is 3 signed longs (X, Y, Z) = 12 bytes.
//! - Each plane (polygon) is `8 + PlanePointCount*8` bytes; `PlanePointCount`
//!   is byte 0 of the plane record (0x00..0x18).
//! - `NormalOffset` points to the byte right after the end of the planes section
//!   (dfbsa.txt: "Always non-zero (confirmed)").
//!
//! Parity note: dfbsa.txt documents that "most 3D objects have this format,
//! some have the sections mixed up slightly" — that is why every offset access
//! is bounds-checked against the complete record, and exact section
//! equalities are verified by sampling in the runtime tests, not
//! as a parsing requirement.

/// Size of the 3D Object header (dfbsa.txt: "always the first 64 bytes").
pub const MESH_HEADER_LEN: usize = 64;
/// Base size of each plane record (8 + PlanePointCount*8).
pub const PLANE_RECORD_BASE_LEN: u32 = 8;

/// Versions observed in ARCH3D.BSA (dfbsa.txt: v2.7 majority; 135× v2.6; 9× v2.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeshVersion {
    V2_5,
    V2_6,
    V2_7,
}

impl MeshVersion {
    pub fn parse(raw: &[u8; 4]) -> Option<Self> {
        match raw {
            b"v2.5" => Some(Self::V2_5),
            b"v2.6" => Some(Self::V2_6),
            b"v2.7" => Some(Self::V2_7),
            _ => None,
        }
    }
}

/// 3D Object header (64 bytes, little-endian) — one ARCH3D.BSA record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeshHeader {
    pub version: MeshVersion,
    pub point_count: u32,
    pub plane_count: u32,
    pub unknown1: u32,
    pub data1_offset: u32,
    pub data2_offset: u32,
    pub num_data2_records: u32,
    pub unknown3: u16,
    pub unknown4: u16,
    pub null_value3: u32,
    pub point_offset: u32,
    pub normal_offset: u32,
    pub unknown6: u32,
    pub plane_offset: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MeshError {
    /// Record shorter than the 64-byte header.
    TooShort { len: usize },
    /// `Version[4]` outside {v2.5, v2.6, v2.7}.
    BadVersion { raw: [u8; 4] },
    /// Header bytes 16-23 must be 8×0x00 (NullValue1, "confirmed" in dfbsa.txt).
    NullValue1NotZero { bytes: [u8; 8] },
    /// Header bytes 44-47 must be 0x00000000 (NullValue4, "confirmed" in dfbsa.txt).
    NullValue4NotZero { raw: u32 },
    /// Planes section that runs past the record (bounds-check).
    PlaneSectionOutOfBounds { plane_index: usize, offset: u32 },
    /// Header sections invalid (delegated from [`compute_sections`]).
    Sections(SectionError),
}

impl std::fmt::Display for MeshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort { len } => write!(f, "3D record too short: {len} < 64 bytes"),
            Self::BadVersion { raw } => write!(f, "unknown version: {raw:?}"),
            Self::NullValue1NotZero { bytes } => write!(f, "non-null NullValue1: {bytes:?}"),
            Self::NullValue4NotZero { raw } => write!(f, "non-null NullValue4: 0x{raw:08X}"),
            Self::PlaneSectionOutOfBounds { plane_index, offset } => {
                write!(f, "plane record {plane_index} at offset {offset} outside the record")
            }
            Self::Sections(e) => write!(f, "invalid sections: {e}"),
        }
    }
}

impl std::error::Error for MeshError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sections(e) => Some(e),
            _ => None,
        }
    }
}

impl From<SectionError> for MeshError {
    fn from(e: SectionError) -> Self {
        Self::Sections(e)
    }
}

fn u32_le(record: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([record[at], record[at + 1], record[at + 2], record[at + 3]])
}

/// Parses the header of an ARCH3D.BSA 3D record.
///
/// Validates the fields marked "confirmed" in dfbsa.txt: NullValue1 (8×0x00),
/// NullValue4 (0) and version ∈ {v2.5, v2.6, v2.7}. The remaining fields are
/// extracted without imposing ranges (dfbsa.txt reports observed ranges, not
/// invariants).
pub fn parse_mesh_header(record: &[u8]) -> Result<MeshHeader, MeshError> {
    if record.len() < MESH_HEADER_LEN {
        return Err(MeshError::TooShort { len: record.len() });
    }
    let raw_version = [record[0], record[1], record[2], record[3]];
    let version = MeshVersion::parse(&raw_version).ok_or(MeshError::BadVersion { raw: raw_version })?;

    let null1 = [record[16], record[17], record[18], record[19], record[20], record[21], record[22], record[23]];
    if null1 != [0; 8] {
        return Err(MeshError::NullValue1NotZero { bytes: null1 });
    }
    let null4 = u32_le(record, 44);
    if null4 != 0 {
        return Err(MeshError::NullValue4NotZero { raw: null4 });
    }

    Ok(MeshHeader {
        version,
        point_count: u32_le(record, 4),
        plane_count: u32_le(record, 8),
        unknown1: u32_le(record, 12),
        data1_offset: u32_le(record, 24),
        data2_offset: u32_le(record, 28),
        num_data2_records: u32_le(record, 32),
        unknown3: u16::from_le_bytes([record[36], record[37]]),
        unknown4: u16::from_le_bytes([record[38], record[39]]),
        null_value3: u32_le(record, 40),
        point_offset: u32_le(record, 48),
        normal_offset: u32_le(record, 52),
        unknown6: u32_le(record, 56),
        plane_offset: u32_le(record, 60),
    })
}

/// Data section offsets of a 3D record, bounds-checked against
/// the actual record size (never trust the header alone).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeshSections {
    pub points_end: u64,
    pub planes_end: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SectionError {
    /// A header offset points past the record.
    OffsetOutOfBounds { field: &'static str, offset: u32, len: usize },
    /// The point count (12 bytes each) exceeds the record.
    PointDataOutOfBounds { point_count: u32, end: u64, len: usize },
    /// Not enough bytes for the next plane record.
    PlaneTruncated { plane_index: usize, offset: u64, len: usize },
}

impl std::fmt::Display for SectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OffsetOutOfBounds { field, offset, len } => {
                write!(f, "{field} offset = {offset} beyond the record ({len} bytes)")
            }
            Self::PointDataOutOfBounds { point_count, end, len } => {
                write!(f, "point data out of bounds: {point_count} pts -> {end} > {len}")
            }
            Self::PlaneTruncated { plane_index, offset, len } => {
                write!(f, "plane record {plane_index} truncated at offset {offset} (record: {len} bytes)")
            }
        }
    }
}

impl std::error::Error for SectionError {}

/// Computes the end offsets of the point and plane sections using the
/// size formula from dfbsa.txt, bounds-checked against the record.
pub fn compute_sections(header: &MeshHeader, record: &[u8]) -> Result<MeshSections, SectionError> {
    let len = record.len() as u64;
    for (field, offset) in [("point_offset", header.point_offset), ("plane_offset", header.plane_offset)] {
        if u64::from(offset) > len {
            return Err(SectionError::OffsetOutOfBounds { field, offset, len: record.len() });
        }
    }

    let points_end = u64::from(header.point_offset) + u64::from(header.point_count) * 12;
    if points_end > len {
        return Err(SectionError::PointDataOutOfBounds {
            point_count: header.point_count,
            end: points_end,
            len: record.len(),
        });
    }

    let mut offset = u64::from(header.plane_offset);
    for plane_index in 0..header.plane_count as usize {
        if offset + 1 > len {
            return Err(SectionError::PlaneTruncated { plane_index, offset, len: record.len() });
        }
        let plane_point_count = u64::from(record[offset as usize]);
        offset += PLANE_RECORD_BASE_LEN as u64 + plane_point_count * 8;
        if offset > len {
            return Err(SectionError::PlaneTruncated { plane_index, offset, len: record.len() });
        }
    }

    Ok(MeshSections { points_end, planes_end: offset })
}

// ── facade doors embedded in the model (FG-01, `GAP-FG01-02`) ──────────────
//
// The retail `.RMB` does **not** carry building doors: the 9005 sub-records of the
// datum declare `doors_in_exterior = 0` (census `fg01-door-census.py`). The
// facade doors live in the building's **3D model**, as planes with base texture
// archive 74 — `DFU:MeshReader.cs:614`
// (`public const int BuildingDoors = 74;`) and `:645-648` (archive reduction).

/// Texture file whose planes are building doors
/// (`DFU:MeshReader.cs:614`).
pub const BUILDING_DOORS_ARCHIVE: u16 = 74;

/// Base texture archive of a plane texture bitfield (`tb >> 7`), applying the
/// reduction `DFU:MeshReader.cs:645-648`: only when the archive is `> 100` and
/// it is neither `331` nor `156` (`archive -= archive / 100 * 100`).
pub fn base_texture_archive(raw_archive: u16) -> u16 {
    if raw_archive > 100 && raw_archive != 331 && raw_archive != 156 {
        raw_archive - (raw_archive / 100) * 100
    } else {
        raw_archive
    }
}

/// One plane of a 3D record as the data carries it (`Arch3dFile.cs:607-663`):
/// point count, texture bitfield and the raw point offsets of its vertices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelPlane {
    /// `PlanePointCount` (byte 0 of the plane record).
    pub plane_point_count: u8,
    /// `Texture` bitfield (`archive << 7 | record`), **raw**.
    pub texture: u16,
    /// Raw `PointOffset` of each vertex (already `PointCount`-divided? no: the
    /// record stores offsets and the divisor is the *reader*'s
    /// (`MeshReader.cs:681-710`), applied by the caller).
    pub points: Vec<u32>,
}

/// Reads every plane of a 3D record: `(plane_point_count, texture, points)`,
/// the exact triple the reference reader iterates (`Arch3dFile.cs:607-663`).
pub fn model_planes(record: &[u8]) -> Result<Vec<ModelPlane>, MeshError> {
    let header = parse_mesh_header(record)?;
    // Validates the plane section size before indexing it.
    compute_sections(&header, record)?;
    let mut planes = Vec::with_capacity(header.plane_count as usize);
    let mut offset = header.plane_offset as usize;
    for plane_index in 0..header.plane_count as usize {
        let plane_point_count = record[offset];
        let texture = u16::from_le_bytes([record[offset + 2], record[offset + 3]]);
        let mut points = Vec::with_capacity(plane_point_count as usize);
        for k in 0..plane_point_count as usize {
            let at = offset + PLANE_RECORD_BASE_LEN as usize + k * 8;
            let raw = u32_le(record, at);
            if raw as usize > record.len() {
                return Err(MeshError::PlaneSectionOutOfBounds { plane_index, offset: raw });
            }
            points.push(raw);
        }
        planes.push(ModelPlane {
            plane_point_count,
            texture,
            points,
        });
        offset += PLANE_RECORD_BASE_LEN as usize + plane_point_count as usize * 8;
    }
    Ok(planes)
}

/// A door plane of a 3D record: the three vertices `GetStaticDoors` uses
/// (`Vert0`, `Vert1`, `Vert2`) plus its texture cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelDoor {
    /// Index of the plane in the record (evidence).
    pub plane_index: usize,
    /// Base texture archive (== [`BUILDING_DOORS_ARCHIVE`] for a door).
    pub texture_archive: u16,
    /// Texture record inside that archive.
    pub texture_record: u16,
    /// Raw point offsets of `Vert0`, `Vert1`, `Vert2` (`PlanePointCount >= 3`).
    pub verts: [u32; 3],
}

/// Every **door plane** of a 3D record, in record order
/// (`DFU:MeshReader.cs:645-710` + `GameObjectHelper.GetStaticDoors`).
///
/// A plane counts as a door when its base texture archive is
/// [`BUILDING_DOORS_ARCHIVE`]. Planes with fewer than three points cannot
/// carry `Vert0/Vert1/Vert2` and are not doors (they are reported by
/// [`model_planes`], never invented here).
pub fn model_doors(record: &[u8]) -> Result<Vec<ModelDoor>, MeshError> {
    let mut doors = Vec::new();
    for (plane_index, plane) in model_planes(record)?.into_iter().enumerate() {
        let raw_archive = plane.texture >> 7;
        let archive = base_texture_archive(raw_archive);
        if archive != BUILDING_DOORS_ARCHIVE || plane.plane_point_count < 3 {
            continue;
        }
        doors.push(ModelDoor {
            plane_index,
            texture_archive: archive,
            texture_record: plane.texture & 0x7F,
            verts: [plane.points[0], plane.points[1], plane.points[2]],
        });
    }
    Ok(doors)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a synthetic 64-byte header with 1 plane of 3 points.
    fn build_header(version: &[u8; 4], normal_offset: u32) -> Vec<u8> {
        let mut h = vec![0u8; MESH_HEADER_LEN];
        h[0..4].copy_from_slice(version);
        h[4..8].copy_from_slice(&3u32.to_le_bytes()); // PointCount
        h[8..12].copy_from_slice(&1u32.to_le_bytes()); // PlaneCount
        h[24..28].copy_from_slice(&120u32.to_le_bytes()); // Data1Offset (non-zero)
        h[48..52].copy_from_slice(&64u32.to_le_bytes()); // PointOffset (0x40)
        h[52..56].copy_from_slice(&normal_offset.to_le_bytes()); // NormalOffset
        h[60..64].copy_from_slice(&100u32.to_le_bytes()); // PlaneOffset
        h
    }

    fn build_full_record() -> Vec<u8> {
        // header + 3 points (36 B, 64..100) + plane ppc=3 (8+24=32 B, 100..132)
        let mut r = build_header(b"v2.7", 132);
        r.extend_from_slice(&[0u8; 36]);
        let mut plane = vec![0u8; 32];
        plane[0] = 3; // PlanePointCount
        r.extend_from_slice(&plane);
        r
    }

    #[test]
    fn parses_confirmed_header_fields() {
        let record = build_full_record();
        let header = parse_mesh_header(&record).expect("parse");
        assert_eq!(header.version, MeshVersion::V2_7);
        assert_eq!(header.point_count, 3);
        assert_eq!(header.plane_count, 1);
        assert_eq!(header.point_offset, 64);
        assert_eq!(header.plane_offset, 100);
        assert_eq!(header.normal_offset, 132);
    }

    #[test]
    fn rejects_bad_version_and_nonzero_nulls() {
        let mut record = build_full_record();
        record[0..4].copy_from_slice(b"v9.9");
        assert!(matches!(parse_mesh_header(&record), Err(MeshError::BadVersion { .. })));

        let mut record = build_full_record();
        record[16] = 1;
        assert!(matches!(parse_mesh_header(&record), Err(MeshError::NullValue1NotZero { .. })));

        let mut record = build_full_record();
        record[44] = 1;
        assert!(matches!(parse_mesh_header(&record), Err(MeshError::NullValue4NotZero { raw: 1 })));

        let short = [0u8; MESH_HEADER_LEN - 1];
        assert!(matches!(parse_mesh_header(&short), Err(MeshError::TooShort { len: 63 })));
    }

    #[test]
    fn computes_sections_from_size_formula() {
        let record = build_full_record();
        let header = parse_mesh_header(&record).expect("parse");
        let sections = compute_sections(&header, &record).expect("sections");
        assert_eq!(sections.points_end, 64 + 3 * 12);
        assert_eq!(sections.planes_end, 100 + 8 + 3 * 8);
        // The equality NormalOffset == end of planes is the canonical case in dfbsa.txt.
        assert_eq!(sections.planes_end as u32, header.normal_offset);
    }

    #[test]
    fn rejects_truncated_plane_section() {
        let mut record = build_full_record();
        record.truncate(110); // the 3-point plane (up to 132) gets cut off
        let header = parse_mesh_header(&record).expect("parse");
        assert!(matches!(
            compute_sections(&header, &record),
            Err(SectionError::PlaneTruncated { plane_index: 0, .. })
        ));
    }

    /// Record with 4 points (64..112) and 2 planes (112..):
    /// plane 0 = ppc 4, door texture (archive 74, record 5); plane 1 = ppc 3,
    /// wall texture (archive 1, record 7).
    fn build_door_record() -> Vec<u8> {
        let planes_off = 112u32;
        let planes_len = (8 + 4 * 8) + (8 + 3 * 8);
        let mut h = vec![0u8; MESH_HEADER_LEN];
        h[0..4].copy_from_slice(b"v2.7");
        h[4..8].copy_from_slice(&4u32.to_le_bytes());
        h[8..12].copy_from_slice(&2u32.to_le_bytes());
        h[24..28].copy_from_slice(&120u32.to_le_bytes());
        h[48..52].copy_from_slice(&64u32.to_le_bytes());
        h[52..56].copy_from_slice(&(planes_off + planes_len).to_le_bytes());
        h[60..64].copy_from_slice(&planes_off.to_le_bytes());
        let mut r = h;
        for _ in 0..4 {
            r.extend_from_slice(&[0u8; 12]);
        }
        // Plane 0: door.
        r.push(4);
        r.push(0);
        r.extend_from_slice(&((74u16 << 7) | 5).to_le_bytes());
        r.extend_from_slice(&[0u8; 4]); // rest of the plane header (8 B)
        for index in [0u32, 12, 24, 36] {
            r.extend_from_slice(&index.to_le_bytes());
            r.extend_from_slice(&[0u8; 4]);
        }
        // Plane 1: wall.
        r.push(3);
        r.push(0);
        r.extend_from_slice(&((1u16 << 7) | 7).to_le_bytes());
        r.extend_from_slice(&[0u8; 4]);
        for index in [0u32, 12, 24] {
            r.extend_from_slice(&index.to_le_bytes());
            r.extend_from_slice(&[0u8; 4]);
        }
        r
    }

    #[test]
    fn reads_planes_with_point_count_and_texture() {
        let record = build_door_record();
        let planes = model_planes(&record).expect("planes");
        assert_eq!(planes.len(), 2);
        assert_eq!(planes[0].plane_point_count, 4);
        assert_eq!(planes[0].texture, (74u16 << 7) | 5);
        assert_eq!(planes[0].points, vec![0, 12, 24, 36]);
        assert_eq!(planes[1].plane_point_count, 3);
    }

    #[test]
    fn extracts_only_archive_74_planes_as_doors() {
        let record = build_door_record();
        let doors = model_doors(&record).expect("doors");
        assert_eq!(doors.len(), 1, "one door plane in the record");
        assert_eq!(doors[0].plane_index, 0);
        assert_eq!(doors[0].texture_archive, BUILDING_DOORS_ARCHIVE);
        assert_eq!(doors[0].texture_record, 5);
        assert_eq!(doors[0].verts, [0, 12, 24]);
    }

    #[test]
    fn reduces_base_archive_only_when_the_rule_says_so() {
        // `MeshReader.cs:645-648`.
        assert_eq!(base_texture_archive(74), 74);
        assert_eq!(base_texture_archive(331), 331);
        assert_eq!(base_texture_archive(156), 156);
        assert_eq!(base_texture_archive(100), 100);
        assert_eq!(base_texture_archive(174), 74);
        assert_eq!(base_texture_archive(374), 74);
        assert_eq!(base_texture_archive(1234), 34);
    }
}

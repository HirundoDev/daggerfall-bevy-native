//! Read-only runtime tests over the real ARCH3D.BSA.
//!
//! - Outside the repo: `~/Games/Daggerfall/app/ARENA2` (or `$DF_ARENA2`); 0
//!   Bethesda bytes are copied into the repo (F0 guardrails intact).
//! - Spec goldens: UESP `dfbsa.txt` (sha256 pinned in the input_manifest):
//!   count 0x280B=10251, size range 212..81394, NullValue1/NullValue4
//!   "confirmed", versions v2.5/v2.6/v2.7, plane = 8 + ppc*8.
//! - Spec↔retail runtime deltas (GOG 1.9.3.2649) documented as parity
//!   findings P-1..P-3 (see input_manifest): dfbsa.txt (2002) counts
//!   135×v2.6 + 9×v2.5, the retail file carries 134×v2.6 + 8×v2.5; 14
//!   duplicate IDs; 7 records whose end of planes ≠ NormalOffset ("sections
//!   mixed up slightly").
//! - If the runtime is not present, the tests are skipped with an explicit
//!   message (they are not stubs: the logic runs fully when data is available).

use std::collections::HashSet;
use std::path::PathBuf;

/// Expected retail observation (verified with an independent probe in this lane).
const RETAIL_V26: usize = 134;
const RETAIL_V25: usize = 8;
const RETAIL_UNIQUE_IDS: usize = 10237;
/// Indices (directory order) whose end of planes differs from NormalOffset.
const RETAIL_PLANE_OUTLIERS: [usize; 7] = [4147, 4184, 4722, 6313, 7614, 8195, 8238];

fn arena2_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("DF_ARENA2") {
        return Some(PathBuf::from(dir));
    }
    std::env::var("HOME").ok().map(|home| PathBuf::from(home).join("Games/Daggerfall/app/ARENA2"))
}

fn load_arch3d() -> Option<Vec<u8>> {
    let path = match arena2_dir() {
        Some(dir) => dir.join("ARCH3D.BSA"),
        None => {
            eprintln!("SKIP arch3d_runtime: $HOME not defined and $DF_ARENA2 absent");
            return None;
        }
    };
    match std::fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(err) => {
            eprintln!("SKIP arch3d_runtime: runtime absent ({path:?}): {err}");
            None
        }
    }
}

#[test]
fn arch3d_header_matches_spec_golden() {
    let Some(data) = load_arch3d() else { return };
    let archive = daggerfall_parser_rs::bsa::parse(&data).expect("ARCH3D.BSA must parse");

    // dfbsa.txt §ARCH3D.BSA: "A typical, 4 byte, BSA header indicating 0x280B records (10251)".
    assert_eq!(archive.entries.len(), 10251, "DirectoryCount != 0x280B (dfbsa.txt golden)");
    // Retail uses the numeric 0x0200 directory (long RecordID + long DataSize);
    // DOS 8.3 names live in FILENAME.BSA (name resolution = later phase).
    assert_eq!(archive.directory_type, daggerfall_parser_rs::bsa::DirectoryType::Number8);
    // Layout: data at offset 4, directory at the end — parse invariant (dfbsa.txt).
    assert_eq!(archive.data_start, 4);
    assert_eq!(archive.directory_offset + 10251 * 8, data.len() as u64);

    // dfbsa.txt: "Records range from 212 to 81394 bytes in size" — exact golden.
    let min = archive.entries.iter().map(|e| e.data_size()).min().unwrap();
    let max = archive.entries.iter().map(|e| e.data_size()).max().unwrap();
    assert_eq!((min, max), (212, 81394), "size range != dfbsa.txt spec");
}

#[test]
fn arch3d_entries_have_ids_and_valid_sizes() {
    let Some(data) = load_arch3d() else { return };
    let archive = daggerfall_parser_rs::bsa::parse(&data).expect("parse");

    let mut ids = HashSet::with_capacity(archive.entries.len());
    let mut total: u64 = 4;
    for (i, entry) in archive.entries.iter().enumerate() {
        let daggerfall_parser_rs::bsa::Entry::Numbered { record_id, data_size } = entry else {
            panic!("entry {i} should be Numbered in ARCH3D.BSA (type 0x0200)");
        };
        assert!(*data_size > 0, "size 0 in entry {i} (id {record_id})");
        ids.insert(*record_id);
        total += u64::from(*data_size);
    }
    // The last data record ends exactly where the directory starts.
    assert_eq!(total, archive.directory_offset);
    // Finding P-2 (parity): retail does NOT have unique IDs — 14 duplicates out of 10251.
    assert_eq!(ids.len(), RETAIL_UNIQUE_IDS, "unique IDs != 10237 (observed retail)");
    assert!(ids.len() < archive.entries.len(), "retail should have duplicate IDs (P-2)");
}

#[test]
fn arch3d_mesh_records_parse_with_confirmed_invariants() {
    let Some(data) = load_arch3d() else { return };
    let archive = daggerfall_parser_rs::bsa::parse(&data).expect("parse");

    // dfbsa.txt marks "confirmed": NullValue1=8×0x00, NullValue4=0,
    // version ∈ {v2.5,v2.6,v2.7} — all 10251 records are verified (parsing would panic).
    let mut versions = [0usize; 3]; // [v2.5, v2.6, v2.7]
    let mut planes_match_normal = 0usize;
    let mut plane_outliers: Vec<usize> = Vec::new();
    let mut section_anomalies: Vec<(usize, String)> = Vec::new();

    for (i, entry) in archive.entries.iter().enumerate() {
        let record = daggerfall_parser_rs::bsa::entry_data(&archive, &data, i).expect("entry bounds");
        let header = daggerfall_parser_rs::mesh::parse_mesh_header(record)
            .unwrap_or_else(|e| panic!("entry {i} (id {}): {e}", entry_id(entry)));
        match header.version {
            daggerfall_parser_rs::mesh::MeshVersion::V2_5 => versions[0] += 1,
            daggerfall_parser_rs::mesh::MeshVersion::V2_6 => versions[1] += 1,
            daggerfall_parser_rs::mesh::MeshVersion::V2_7 => versions[2] += 1,
        }
        match daggerfall_parser_rs::mesh::compute_sections(&header, record) {
            Ok(sections) if sections.planes_end as u32 == header.normal_offset => {
                planes_match_normal += 1
            }
            Ok(_) => plane_outliers.push(i),
            Err(e) => section_anomalies.push((i, e.to_string())),
        }
    }

    // Observed retail distribution (independent probe in this lane).
    // Finding P-1 (parity): dfbsa.txt (2002) declares 135×v2.6 and 9×v2.5 —
    // retail GOG carries 134 and 8 (delta 1+1); the rest is v2.7 in both cases.
    assert_eq!(versions, [RETAIL_V25, RETAIL_V26, 10251 - RETAIL_V25 - RETAIL_V26]);

    // Finding P-3 (parity): 7 records whose end of planes does not match
    // NormalOffset (dfbsa.txt: "some have the sections mixed up slightly");
    // bounds-check 0 failures across the whole file.
    assert!(
        section_anomalies.is_empty(),
        "records with sections out of bounds: {section_anomalies:?}"
    );
    assert_eq!(
        plane_outliers, RETAIL_PLANE_OUTLIERS,
        "plane outliers != observed list (P-3)"
    );
    assert_eq!(
        planes_match_normal,
        10251 - RETAIL_PLANE_OUTLIERS.len(),
        "planes_end==NormalOffset compliance"
    );
    eprintln!(
        "versions={versions:?} planes_match={planes_match_normal}/10251 outliers={plane_outliers:?}"
    );
}

fn entry_id(entry: &daggerfall_parser_rs::bsa::Entry) -> u32 {
    match entry {
        daggerfall_parser_rs::bsa::Entry::Numbered { record_id, .. } => *record_id,
        daggerfall_parser_rs::bsa::Entry::Named { name, .. } => {
            panic!("unexpected named entry: {name}")
        }
    }
}

//! `dungeon_grid` — dungeon layout assembled from a `MAPDITEM` entry
//! (M34-D / G-N1: **geometry only** — no combat, no loot, no AI).
//!
//! **Why it exists:** a dungeon location is a *grid* of `.RDB` cells
//! (`DungeonBlock` list) plus the doors/markers inside those cells. The
//! parser gives the layout ([`daggerfall_parser_rs::maps::parse_ditem`]);
//! this module places it in meters and resolves the cell records through a
//! caller-supplied closure, so it stays **pure** (0 I/O, rule 4): the app
//! owns BSA custody and passes the bytes.
//!
//! Sources of the math and of every constant (DoD §b declaration):
//! - `DFU:Assets/Scripts/Utility/RDBLayout.cs` (upstream MIT, **cited**;
//!   the pinned corpus does not include it — same regime as `interior_retail`
//!   and `.RMB` in M10), `RDBSide = 2048 × MeshReader.GlobalScale`
//!   (`GlobalScale = 1/40` ⇒ **51.2 m**, see [`RDB_SIDE_M`]).
//! - Block↔object composition: `GetModelMatrix` ⇒
//!   `position = (X·2048 + px, −py, Z·2048 + pz) × GlobalScale`, then the
//!   df→Bevy z-negation already used by [`crate::interior_retail`] ⇒
//!   `x = (X·2048 + px)/40`, `y = −py/40`, `z = −(Z·2048 + pz)/40`.
//!   The dedupe below is invariant to that sign.
//! - Editor flats: `TextureReader.EditorFlatsTextureArchive = 199`
//!   (`DFU:API/TextureReader.cs:34`); `record 10 = StartMarkers`,
//!   `record 8 = EnterMarkers` (`RDBLayout.AddFlats`, same indices as
//!   `DaggerfallDungeon.EnumerateDebuggerMarkers`: `enterMarkerIndex = 8`,
//!   `startMarkerIndex = 10`).
//! - Markers are read **only from the starting block**
//!   (`DaggerfallDungeon.FindMarkers`, `:398-434`): DFU's `enterMarker` is
//!   `null` when that block has no `Enter` flat, and this module returns
//!   `None` in exactly that case (measured in retail: 775/4232 dungeons).
//! - Action doors: `RDBLayout.IsActionDoor` — `description ∈ {DOR, DDR, NEW,
//!   CAV}` and `ModelIdNum ≠ redBrickDoorModelID (72100)`.
//! - Overlap: `DaggerfallDungeon.RemoveOverlappingDoors`
//!   (`:355-395`) — greedy in hierarchy order (block list → object order),
//!   `Vector3.Distance < 1.4` ⇒ the door is **removed**.
//!
//! **Declared delta vs DFU:** the overlap registry is seeded by DFU with the
//! static `DungeonExit` door (`DoorTypes.DungeonExit`) of the starting block.
//! This engine has no static door entities yet, so the registry starts empty:
//! [`DungeonGrid::doors_removed`] counts overlaps **between action doors
//! only**. It is not a DFU-equal door count, and it is reported as such.

use std::fmt::Write as _;

use daggerfall_parser_rs::maps::{DungeonBlock, LocationDungeon};
use daggerfall_parser_rs::rdb::{self, RdbBlock, RdbResource};

use crate::convert::UNITS_PER_METER;
use crate::interior_retail::RDB_SIDE_M;

/// Native side of an `.RDB` cell (`DFU:RDBLayout.cs`, `RDBSide/GobalScale`).
pub const RDB_SIDE_UNITS: i32 = 2048;
/// `UNITS_PER_METER` as `f32` for the placement math (`GlobalScale = 1/40`).
const UNITS_PER_METER_F: f32 = UNITS_PER_METER;

/// Editor flat archive (`DFU:API/TextureReader.cs:34`).
pub const EDITOR_FLATS_ARCHIVE: u16 = 199;
/// `Enter` marker record (`RDBLayout.AddFlats` / `DaggerfallDungeon.cs:446`).
pub const ENTER_MARKER_RECORD: u16 = 8;
/// `Start` marker record (`RDBLayout.AddFlats` / `DaggerfallDungeon.cs:447`).
pub const START_MARKER_RECORD: u16 = 10;
/// Overlap tolerance in meters (`DaggerfallDungeon.RemoveOverlappingDoors`).
pub const DOOR_OVERLAP_TOLERANCE_M: f32 = 1.4;
/// Red-brick door model excluded from the action-door set
/// (`RDBLayout.IsActionDoor`).
pub const RED_BRICK_DOOR_MODEL_ID: u32 = 72_100;
/// `ModelReferenceList.Description` values of an action door
/// (`RDBLayout.IsActionDoor`).
pub const ACTION_DOOR_DESCRIPTIONS: [[u8; 3]; 4] = [*b"DOR", *b"DDR", *b"NEW", *b"CAV"];

/// One `DungeonBlock` placed on the world grid.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedBlock {
    /// `.RDB` record name (`{letter}{number:07}.RDB`).
    pub name: String,
    /// Grid X (`DungeonBlock.X`, cells).
    pub x: i8,
    /// Grid Z (`DungeonBlock.Z`, cells).
    pub z: i8,
    /// Cell origin in meters (Bevy frame).
    pub origin_m: [f32; 3],
    /// `DungeonBlock.IsStartingBlock`.
    pub is_starting_block: bool,
    /// The record was resolved **and** parsed (models/flats available).
    pub parsed: bool,
}

/// An action door kept after the overlap pass.
#[derive(Debug, Clone, PartialEq)]
pub struct DungeonDoor {
    /// Index into [`DungeonGrid::blocks`].
    pub block: usize,
    /// Position in meters (Bevy frame).
    pub position_m: [f32; 3],
    /// Object offset inside the `.RDB` record (deterministic identity).
    pub object_offset: usize,
    /// `ModelReferenceList.Description` (`DOR`/`DDR`/`NEW`/`CAV`).
    pub description: [u8; 3],
    /// ARCH3D model id of the door.
    pub model_id: u32,
}

/// Dungeon assembled from one `MAPDITEM` entry + its `.RDB` cells.
#[derive(Debug, Clone, PartialEq)]
pub struct DungeonGrid {
    /// Blocks in `MAPDITEM` order (`DungeonBlock[i]` = `blocks[i]`).
    pub blocks: Vec<PlacedBlock>,
    /// Minimum grid X among blocks.
    pub min_x: i8,
    /// Maximum grid X among blocks.
    pub max_x: i8,
    /// Minimum grid Z among blocks.
    pub min_z: i8,
    /// Maximum grid Z among blocks.
    pub max_z: i8,
    /// Action doors kept, in hierarchy order (block → object).
    pub doors: Vec<DungeonDoor>,
    /// Action doors dropped by the 1.4 m overlap rule.
    pub doors_removed: usize,
    /// `Start` marker of the starting block (first one, DFU `[0]`).
    pub start_marker: Option<[f32; 3]>,
    /// `Enter` marker of the starting block; `None` = DFU `enterMarker` null.
    pub enter_marker: Option<[f32; 3]>,
    /// Records absent from the archive (0 in retail, measured).
    pub missing_blocks: Vec<String>,
    /// Records present but rejected by `rdb::parse` (name, error).
    pub unparsed_blocks: Vec<(String, String)>,
}

impl DungeonGrid {
    /// Extent in meters: `(width_x, depth_z)` of the occupied grid.
    pub fn extent_m(&self) -> (f32, f32) {
        (
            f32::from(self.max_x - self.min_x + 1) * RDB_SIDE_M,
            f32::from(self.max_z - self.min_z + 1) * RDB_SIDE_M,
        )
    }

    /// Canonical text rendering (deterministic; feeds the probe digest).
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "grid {} {} {} {}\nblocks {}",
            self.min_x,
            self.max_x,
            self.min_z,
            self.max_z,
            self.blocks.len()
        );
        for (i, block) in self.blocks.iter().enumerate() {
            let _ = writeln!(
                out,
                "b{i} {} {} {} {:.3} {:.3} {:.3} start={} parsed={}",
                block.name,
                block.x,
                block.z,
                block.origin_m[0],
                block.origin_m[1],
                block.origin_m[2],
                u8::from(block.is_starting_block),
                u8::from(block.parsed)
            );
        }
        let _ = writeln!(out, "doors {} removed {}", self.doors.len(), self.doors_removed);
        for (i, door) in self.doors.iter().enumerate() {
            let _ = writeln!(
                out,
                "d{i} blk={} at={:.3},{:.3},{:.3} off={} desc={} model={}",
                door.block,
                door.position_m[0],
                door.position_m[1],
                door.position_m[2],
                door.object_offset,
                String::from_utf8_lossy(&door.description),
                door.model_id
            );
        }
        let marker = |m: Option<[f32; 3]>| match m {
            Some(p) => format!("{:.3},{:.3},{:.3}", p[0], p[1], p[2]),
            None => "none".to_string(),
        };
        let _ = writeln!(
            out,
            "markers start={} enter={} missing={} unparsed={}",
            marker(self.start_marker),
            marker(self.enter_marker),
            self.missing_blocks.len(),
            self.unparsed_blocks.len()
        );
        out.into_bytes()
    }

    /// FNV-1a 64 of [`Self::canonical_bytes`].
    pub fn digest(&self) -> u64 {
        daggerfall_parser_rs::rdi::fnv1a64(&self.canonical_bytes())
    }
}

/// Cell origin in meters (Bevy frame).
pub fn block_origin_m(x: i8, z: i8) -> [f32; 3] {
    [
        (f32::from(x) * RDB_SIDE_UNITS as f32) / UNITS_PER_METER_F,
        0.0,
        canonical_zero(-((f32::from(z) * RDB_SIDE_UNITS as f32) / UNITS_PER_METER_F)),
    ]
}

/// `-0.0 → +0.0`: the z reflection must not leak a signed zero into the
/// canonical text (`-0.000` would make the digest depend on the sign bit).
fn canonical_zero(value: f32) -> f32 {
    value + 0.0
}

/// Object position inside its block, in meters (Bevy frame;
/// `DFU:RDBLayout.cs GetModelMatrix` + the df→Bevy z-negation).
pub fn object_position_m(x: i8, z: i8, position: [i32; 3]) -> [f32; 3] {
    let origin_x = i32::from(x) * RDB_SIDE_UNITS;
    let origin_z = i32::from(z) * RDB_SIDE_UNITS;
    [
        (origin_x + position[0]) as f32 / UNITS_PER_METER_F,
        -(position[1] as f32) / UNITS_PER_METER_F,
        -((origin_z + position[2]) as f32) / UNITS_PER_METER_F,
    ]
}

/// `RDBLayout.IsActionDoor`: description in the action set and model ≠ 72100.
pub fn is_action_door(description: [u8; 3], model_id: u32) -> bool {
    ACTION_DOOR_DESCRIPTIONS.contains(&description) && model_id != RED_BRICK_DOOR_MODEL_ID
}

/// `(archive, record)` of a flat, validated against the editor archive.
pub fn is_editor_marker(archive: u16, record: u16, wanted: u16) -> bool {
    archive == EDITOR_FLATS_ARCHIVE && record == wanted
}

/// Greedy overlap removal in hierarchy order (DFU `RemoveOverlappingDoors`):
/// a door closer than [`DOOR_OVERLAP_TOLERANCE_M`] to an already kept door is
/// dropped. Returns `(kept, removed)`.
pub fn dedupe_doors(candidates: Vec<DungeonDoor>) -> (Vec<DungeonDoor>, usize) {
    let limit = DOOR_OVERLAP_TOLERANCE_M * DOOR_OVERLAP_TOLERANCE_M;
    let mut kept: Vec<DungeonDoor> = Vec::with_capacity(candidates.len());
    let mut removed = 0usize;
    for door in candidates {
        let overlaps = kept.iter().any(|other| {
            let d = [
                door.position_m[0] - other.position_m[0],
                door.position_m[1] - other.position_m[1],
                door.position_m[2] - other.position_m[2],
            ];
            d[0] * d[0] + d[1] * d[1] + d[2] * d[2] < limit
        });
        if overlaps {
            removed += 1;
        } else {
            kept.push(door);
        }
    }
    (kept, removed)
}

/// First flat of `record` in object order (DFU `[0]` of the marker list).
fn first_marker(block: &RdbBlock, record: u16) -> Option<[i32; 3]> {
    block.objects.iter().find_map(|object| match object.resource {
        RdbResource::Flat(flat)
            if is_editor_marker(flat.texture_archive(), flat.texture_record(), record) =>
        {
            Some(object.position)
        }
        _ => None,
    })
}

/// Action doors of one parsed block, in object order.
fn block_doors(block: &RdbBlock, x: i8, z: i8, block_index: usize) -> Vec<DungeonDoor> {
    let mut out = Vec::new();
    for object in &block.objects {
        let RdbResource::Model(model) = object.resource else {
            continue;
        };
        let Some(reference) = block.model_references.get(model.model_index as usize) else {
            continue;
        };
        let Some(model_id) = reference.model_id else {
            continue;
        };
        if !is_action_door(reference.description, model_id) {
            continue;
        }
        out.push(DungeonDoor {
            block: block_index,
            position_m: object_position_m(x, z, object.position),
            object_offset: object.offset,
            description: reference.description,
            model_id,
        });
    }
    out
}

/// Assembles a dungeon grid from one `MAPDITEM` entry.
///
/// `resolve(name)` returns the `.RDB` bytes of `name` (the app resolves them
/// through BLOCKS.BSA). Missing records are recorded, never fabricated.
pub fn assemble_dungeon<'a, F>(dungeon: &LocationDungeon, mut resolve: F) -> DungeonGrid
where
    F: FnMut(&str) -> Option<&'a [u8]>,
{
    let mut grid = DungeonGrid {
        blocks: Vec::with_capacity(dungeon.blocks.len()),
        min_x: 0,
        max_x: 0,
        min_z: 0,
        max_z: 0,
        doors: Vec::new(),
        doors_removed: 0,
        start_marker: None,
        enter_marker: None,
        missing_blocks: Vec::new(),
        unparsed_blocks: Vec::new(),
    };
    let mut candidates: Vec<DungeonDoor> = Vec::new();
    for (index, block) in dungeon.blocks.iter().enumerate() {
        if index == 0 {
            grid.min_x = block.x;
            grid.max_x = block.x;
            grid.min_z = block.z;
            grid.max_z = block.z;
        } else {
            grid.min_x = grid.min_x.min(block.x);
            grid.max_x = grid.max_x.max(block.x);
            grid.min_z = grid.min_z.min(block.z);
            grid.max_z = grid.max_z.max(block.z);
        }
        let mut placed = PlacedBlock {
            name: block.name.clone(),
            x: block.x,
            z: block.z,
            origin_m: block_origin_m(block.x, block.z),
            is_starting_block: block.is_starting_block,
            parsed: false,
        };
        let Some(bytes) = resolve(&block.name) else {
            grid.missing_blocks.push(block.name.clone());
            grid.blocks.push(placed);
            continue;
        };
        match rdb::parse(bytes) {
            Ok(rdb) => {
                placed.parsed = true;
                candidates.extend(block_doors(&rdb, block.x, block.z, index));
                // Markers exist only in the starting block (DFU FindMarkers).
                if block.is_starting_block {
                    if let Some(position) = first_marker(&rdb, START_MARKER_RECORD) {
                        grid.start_marker = Some(object_position_m(block.x, block.z, position));
                    }
                    if let Some(position) = first_marker(&rdb, ENTER_MARKER_RECORD) {
                        grid.enter_marker = Some(object_position_m(block.x, block.z, position));
                    }
                }
            }
            Err(error) => grid.unparsed_blocks.push((block.name.clone(), error.to_string())),
        }
        grid.blocks.push(placed);
    }
    let (kept, removed) = dedupe_doors(candidates);
    grid.doors = kept;
    grid.doors_removed = removed;
    grid
}

/// `DungeonBlock` name of the block at `index` (helper for probes/tests).
pub fn block_name(block: &DungeonBlock) -> &str {
    &block.name
}

#[cfg(test)]
mod tests {
    use super::*;
    use daggerfall_parser_rs::maps::{DungeonHeader, LocationRecordElement, LocationDungeon};

    fn door(at: [f32; 3]) -> DungeonDoor {
        DungeonDoor {
            block: 0,
            position_m: at,
            object_offset: 0,
            description: *b"DOR",
            model_id: 72_101,
        }
    }

    fn dungeon(blocks: Vec<DungeonBlock>) -> LocationDungeon {
        LocationDungeon {
            record: LocationRecordElement {
                location_id: 26_219,
                exterior_location_id: 26_219,
                location_name: "Daggerfall".to_string(),
            },
            header: DungeonHeader {
                null_value1: 0,
                unknown1: 0,
                unknown2: 0,
                block_count: blocks.len() as u16,
            },
            blocks,
        }
    }

    fn block(name: &str, x: i8, z: i8, start: bool) -> DungeonBlock {
        DungeonBlock {
            x,
            z,
            number: 0,
            is_starting_block: start,
            block_index: 0,
            name: name.to_string(),
        }
    }

    #[test]
    fn side_is_fifty_one_point_two_meters() {
        assert!((RDB_SIDE_M - 51.2).abs() < f32::EPSILON);
        assert_eq!(block_origin_m(1, 0), [51.2, 0.0, 0.0]);
        // df Z is negated (handedness), as in `interior_retail`.
        assert_eq!(block_origin_m(0, 1), [0.0, 0.0, -51.2]);
        assert_eq!(block_origin_m(-1, -1), [-51.2, 0.0, 51.2]);
    }

    #[test]
    fn object_position_adds_block_origin_and_negates_y_and_z() {
        let position = object_position_m(1, -2, [400, 80, -120]);
        assert!((position[0] - (2048.0 + 400.0) / 40.0).abs() < 1e-3);
        assert!((position[1] + 2.0).abs() < 1e-3);
        assert!((position[2] - (4096.0 + 120.0) / 40.0).abs() < 1e-3);
    }

    #[test]
    fn action_door_rules_match_dfu() {
        assert!(is_action_door(*b"DOR", 72_101));
        assert!(is_action_door(*b"DDR", 0));
        assert!(is_action_door(*b"NEW", 0));
        assert!(is_action_door(*b"CAV", 0));
        // Red-brick door model is rejected even with a door description.
        assert!(!is_action_door(*b"DOR", RED_BRICK_DOOR_MODEL_ID));
        assert!(!is_action_door(*b"COL", 72_101));
        assert!(!is_action_door([0xff, 0xff, 0xff], 1));
    }

    #[test]
    fn editor_markers_use_archive_199_and_records_8_10() {
        assert!(is_editor_marker(199, 8, ENTER_MARKER_RECORD));
        assert!(is_editor_marker(199, 10, START_MARKER_RECORD));
        // Archive 213 has the same record numbers for other content.
        assert!(!is_editor_marker(213, 8, ENTER_MARKER_RECORD));
        assert!(!is_editor_marker(199, 11, ENTER_MARKER_RECORD));
    }

    #[test]
    fn dedupe_drops_doors_within_tolerance_in_order() {
        let (kept, removed) = dedupe_doors(vec![
            door([0.0, 0.0, 0.0]),
            door([1.0, 0.0, 0.0]),   // 1.0 < 1.4 → removed
            door([1.4, 0.0, 0.0]),   // exactly the tolerance → kept
            door([0.0, 0.0, -1.39]), // overlaps the first → removed
            door([1.4, 0.0, 0.5]),   // 0.5 from the kept 1.4 door → removed
        ]);
        assert_eq!(removed, 3);
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[1].position_m[0], 1.4);
    }

    #[test]
    fn dedupe_is_invariant_to_the_z_reflection() {
        let a = dedupe_doors(vec![door([0.0, 0.0, 0.0]), door([0.0, 0.0, 1.3])]);
        let b = dedupe_doors(vec![door([0.0, 0.0, 0.0]), door([0.0, 0.0, -1.3])]);
        assert_eq!(a.1, b.1);
        assert_eq!(a.0.len(), b.0.len());
    }

    #[test]
    fn assemble_without_records_reports_missing_and_keeps_extent() {
        let layout = dungeon(vec![
            block("N0000006.RDB", 0, 0, true),
            block("W0000050.RDB", 1, -2, false),
            block("L0000011.RDB", -3, 2, false),
        ]);
        let grid = assemble_dungeon(&layout, |_| None);
        assert_eq!(grid.blocks.len(), 3);
        assert!(!grid.blocks[0].parsed);
        assert_eq!(grid.missing_blocks.len(), 3);
        assert!(grid.doors.is_empty());
        assert_eq!((grid.min_x, grid.max_x, grid.min_z, grid.max_z), (-3, 1, -2, 2));
        assert_eq!(grid.extent_m(), (5.0 * RDB_SIDE_M, 5.0 * RDB_SIDE_M));
        // No record → no markers (never fabricated).
        assert_eq!(grid.start_marker, None);
        assert_eq!(grid.enter_marker, None);
    }

    #[test]
    fn canonical_bytes_are_stable() {
        let layout = dungeon(vec![block("N0000006.RDB", 0, 0, true)]);
        let a = assemble_dungeon(&layout, |_| None);
        let b = assemble_dungeon(&layout, |_| None);
        assert_eq!(a.canonical_bytes(), b.canonical_bytes());
        assert_eq!(a.digest(), b.digest());
        let text = String::from_utf8(a.canonical_bytes()).expect("utf8");
        assert!(text.contains("grid 0 0 0 0"));
        assert!(text.contains("b0 N0000006.RDB 0 0 0.000 0.000 0.000 start=1 parsed=0"));
        assert!(text.contains("markers start=none enter=none missing=1 unparsed=0"));
    }

    #[test]
    fn block_name_returns_the_record_name() {
        let b = block("M0000001.RDB", 0, 0, false);
        assert_eq!(block_name(&b), "M0000001.RDB");
    }
}

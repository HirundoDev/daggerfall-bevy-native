//! `interior_retail` (app) — retail custody of the **dungeon grid** (M34-D /
//! G-N1): ARENA2 `MAPS.BSA` (`MAPNAMES`/`MAPPITEM`/`MAPDITEM`) + `BLOCKS.BSA`
//! → [`daggerfall_parser_bevy_bridge::dungeon_grid::assemble_dungeon`].
//!
//! **Name collision, stated on purpose:** this is *not*
//! `daggerfall_parser_bevy_bridge::interior_retail`, which builds the
//! geometry of ONE `.RDB` interior. This module is the app-side
//! **source-resolution** layer for dungeon *locations*: it owns BSA custody
//! (Mode A: the user's ARENA2, 0 packaged bytes — INV-L99-6), feeds
//! [`daggerfall_parser_rs::maps::parse_ditem`] and the pure grid assembly,
//! and contains **no business logic** (M34 frontier: geometry only — no
//! formulas, no AI, no loot).
//!
//! Consumers: `PROBE-DUNGEON-001` (`crate::dungeon_probe`). Nothing else
//! re-implements the MAPDITEM index: [`RetailSource::dungeons`] is the single
//! enumeration of dungeon locations.

use std::collections::HashMap;

use daggerfall_parser_bevy_bridge::dungeon_grid::{assemble_dungeon, DungeonGrid};
use daggerfall_parser_rs::{bsa, maps};

use crate::m10_probe::named_entry;
use crate::m18_probe::region_suffixes;

/// One dungeon location with its parsed `MAPDITEM` layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetailDungeon {
    /// Region slot index (`MAPNAMES.<nnn>` suffix).
    pub region_index: u16,
    /// Position inside the region's `MAPNAMES` (index order).
    pub location_index: usize,
    /// Location name (`MAPNAMES`).
    pub location_name: String,
    /// `LocationId` of the MAPPITEM header (the lookup key in MAPDITEM).
    pub location_id: u16,
    /// Parsed dungeon layout (`DFU:API/MapsFile.cs:1275-1343`).
    pub layout: maps::LocationDungeon,
}

/// Region/location census of the dungeon index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DungeonCensus {
    /// Region slots with data (not empty).
    pub regions: usize,
    /// Locations enumerated across those regions.
    pub locations: usize,
    /// Locations whose MAPDITEM entry exists (dungeons).
    pub dungeons: usize,
    /// Locations without a MAPDITEM entry (towns, etc.).
    pub without_dungeon: usize,
    /// `MAPS.BSA` size in bytes.
    pub maps_bytes: usize,
    /// `BLOCKS.BSA` size in bytes.
    pub blocks_bytes: usize,
}

/// ARENA2 custody + the single enumeration of dungeon locations.
pub struct RetailSource {
    maps_archive: bsa::BsaArchive,
    maps_data: Vec<u8>,
    blocks_archive: bsa::BsaArchive,
    blocks_data: Vec<u8>,
    blocks_index: HashMap<String, usize>,
}

impl RetailSource {
    /// Loads `MAPS.BSA` + `BLOCKS.BSA` from the user's ARENA2
    /// (`crate::mode_a_arena2_path`, `DF_APP_ROOT` overrides it).
    pub fn load() -> Result<Self, String> {
        let maps_data = read_arena2("MAPS.BSA")?;
        let blocks_data = read_arena2("BLOCKS.BSA")?;
        let maps_archive = bsa::parse(&maps_data).map_err(|e| format!("MAPS.BSA: {e}"))?;
        let blocks_archive = bsa::parse(&blocks_data).map_err(|e| format!("BLOCKS.BSA: {e}"))?;
        let mut blocks_index = HashMap::new();
        for (index, entry) in blocks_archive.entries.iter().enumerate() {
            if let bsa::Entry::Named { name, .. } = entry {
                blocks_index.insert(name.to_ascii_uppercase(), index);
            }
        }
        Ok(Self {
            maps_archive,
            maps_data,
            blocks_archive,
            blocks_data,
            blocks_index,
        })
    }

    /// `MAPS.BSA` size in bytes (evidence of the measured source).
    pub fn maps_bytes(&self) -> usize {
        self.maps_data.len()
    }

    /// `BLOCKS.BSA` size in bytes (evidence of the measured source).
    pub fn blocks_bytes(&self) -> usize {
        self.blocks_data.len()
    }

    /// Enumerates every dungeon location in index order (region, location)
    /// and parses its MAPDITEM layout.
    ///
    /// A region whose `MAPDITEM.<nnn>` record is absent has **0 dungeons**
    /// (there is nothing to place); a present record that fails to parse is
    /// an error, never a silent skip.
    pub fn dungeons(&self) -> Result<(DungeonCensus, Vec<RetailDungeon>), String> {
        let mut census = DungeonCensus {
            maps_bytes: self.maps_data.len(),
            blocks_bytes: self.blocks_data.len(),
            ..DungeonCensus::default()
        };
        let mut out: Vec<RetailDungeon> = Vec::new();
        for (region_index, suffix) in region_suffixes(&self.maps_archive) {
            let names_record = named_entry(&self.maps_archive, &self.maps_data, &format!("MAPNAMES.{suffix}"))?;
            let pitem_record = named_entry(&self.maps_archive, &self.maps_data, &format!("MAPPITEM.{suffix}"))?;
            let ditem_record = named_entry(&self.maps_archive, &self.maps_data, &format!("MAPDITEM.{suffix}"))
                .unwrap_or(&[]);
            if names_record.is_empty() || pitem_record.is_empty() {
                // Empty region slots are not index regions (M18 PROBE-MAP-001
                // measured 62 slots, 61 with locations).
                continue;
            }
            let names = maps::parse_map_names(names_record)
                .map_err(|e| format!("MAPNAMES.{suffix}: {e}"))?;
            if names.is_empty() {
                continue;
            }
            let locations = maps::parse_pitem(pitem_record, names.len())
                .map_err(|e| format!("MAPPITEM.{suffix}: {e}"))?;
            if locations.len() != names.len() {
                return Err(format!(
                    "region {region_index}: names={} pitem={}",
                    names.len(),
                    locations.len()
                ));
            }
            census.regions += 1;
            for (location_index, location) in locations.iter().enumerate() {
                census.locations += 1;
                match maps::parse_ditem(ditem_record, location.location_id)
                    .map_err(|e| format!("MAPDITEM.{suffix}[{location_index}]: {e}"))?
                {
                    Some(layout) => {
                        census.dungeons += 1;
                        out.push(RetailDungeon {
                            region_index,
                            location_index,
                            location_name: location.name.clone(),
                            location_id: location.location_id,
                            layout,
                        });
                    }
                    None => census.without_dungeon += 1,
                }
            }
        }
        Ok((census, out))
    }

    /// Assembles the grid of one layout (pure geometry; missing block records
    /// are reported inside the grid, never fabricated).
    pub fn assemble(&self, layout: &maps::LocationDungeon) -> DungeonGrid {
        assemble_dungeon(layout, |name| self.block_record(name))
    }

    /// `.RDB` bytes of `name` from BLOCKS.BSA (case-insensitive).
    fn block_record(&self, name: &str) -> Option<&[u8]> {
        let index = *self.blocks_index.get(&name.to_ascii_uppercase())?;
        bsa::entry_data(&self.blocks_archive, &self.blocks_data, index).ok()
    }
}

/// Reads one file of the user's ARENA2, resolved at runtime.
fn read_arena2(file: &str) -> Result<Vec<u8>, String> {
    let path = crate::mode_a_arena2_path(file)?;
    std::fs::read(&path).map_err(|e| format!("{} ({e})", path.display()))
}

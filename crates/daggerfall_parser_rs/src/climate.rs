//! `CLIMATE.PAK` / `POLITIC.PAK` — climate and politics per map-pixel, nature
//! per season (M13, DoD §e / WI-6 · REQ-L99-044).
//!
//! Spec anchors (`reference/dfu-spec/` corpus, tag `v1.1.1`,
//! commit `0038a1e61999ad2c73fb1f33b706cae9a88ec66f`):
//!
//! | Fact | Anchor |
//! |---|---|
//! | PAK layout: 500 `u32` offsets + RLE rows `(u16 count, u8 value)` of 1001 | `PakFile.cs:110-135` |
//! | `pakWidthValue = 1001`, `pakHeightValue = 500` | `PakFile.cs:28-31` |
//! | `GetClimateIndex` adds +1 to X to align with the heightmap | `MapsFile.cs:891-896` |
//! | `GetPoliticIndex` (same PAK, same +1) | `MapsFile.cs:904-907` |
//! | `GetWorldClimateSettings`: climate 223..232 → ground/nature/sky/people | `MapsFile.cs:440-532` |
//! | `Climates` enum (Ocean 223 … HauntedWoodlands 232) | `MapsFile.cs:289-301` |
//! | `DilateCoastalClimate` (Moore 8, ocean = 223, 2 passes at startup) | `TerrainHelper.cs:383-410` + `:416-437` |
//! | `GetNatureArchive(NatureSet, Season)` (+1 in winter for 4 sets) | `ClimateSwaps.cs:389-446` |
//! | `FromAPITextureSet` (texture set → nature set) | `ClimateSwaps.cs:354-378` |
//! | nature sets 500..511 (`Nature_*`) | `DFLocation.cs:251-263` |
//! | `GetWorldClimateSettings` by `worldClimate` on the terrain | `StreamingWorld.cs:1266-1272` |
//!
//! **Empirical layout** (measured 2026-10-20 over the files held in
//! `ASSET-REGISTRY.json`):
//!
//! | File | bytes | sha256 (prefix) | offsets | end of last row |
//! |---|---|---|---|---|
//! | `CLIMATE.PAK` | 14,771 | `60324030…` | 500 × u32, first 2000, last 14,750 | 14,771 (0 trailing) |
//! | `POLITIC.PAK` | 19,556 | — | 500 × u32, first 2000, last 19,541 | 19,556 (0 trailing) |
//!
//! `CLIMATE.PAK` contains exactly the 10 values of the `Climates` enum
//! (223..232); any other value is a **typed error**
//! ([`ClimateError::ClimateOutOfRange`]), never a silent default.
//!
//! ## 61 regions
//!
//! [`REGION_COUNT`] = 61 is the project's canonical figure (UESP
//! `dfbsa.txt` → MAPS.BSA "61 regions, 15,251 locations"; 00c
//! REQ-L99-051 + DoD §a): the index regions are enumerable by index
//! `0..61` with [`regions`]. The physical slot 61 of `MAPS.BSA`
//! (`MapFileReader.RegionCount = recordCount / 4 = 62`) is a deviation
//! **declared** in the HH-1, not a parser invention.

use std::collections::BTreeSet;

/// Width of one PAK row (`PakFile.cs:28`).
pub const PAK_WIDTH: usize = 1001;

/// Height of the PAK grid (`PakFile.cs:31`).
pub const PAK_HEIGHT: usize = 500;

/// Offset table of a PAK: `500 × u32`.
pub const PAK_OFFSET_TABLE_LEN: usize = PAK_HEIGHT * 4;

/// Ocean climate (`MapsFile.Climates.Ocean`).
pub const OCEAN_CLIMATE: u8 = 223;

/// Coastal dilation passes DFU applies at startup
/// (`StreamingWorld.cs:1680`).
pub const COASTAL_DILATION_PASSES: usize = 2;

/// Enumerable regions of the world index (UESP `dfbsa.txt` / 00c
/// REQ-L99-051).
pub const REGION_COUNT: usize = 61;

/// Region indexes `0..61` (enumerable, stable).
pub fn regions() -> impl Iterator<Item = u8> {
    0..REGION_COUNT as u8
}

/// PAK parse error (climate/politics).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PakError {
    /// File shorter than the offset table.
    TooShort { len: usize, need: usize },
    /// Non-monotonic offsets (`PakFile.cs` walks them in order).
    OffsetsNotMonotonic { row: usize, previous: u32, current: u32 },
    /// Row that starts before the table or past the file.
    RowStartOutOfBounds { row: usize, offset: u32, len: usize },
    /// RLE row that exhausts the file before covering 1001 values.
    RowTruncated { row: usize, at: usize },
    /// RLE run that overruns the row (> 1001 values).
    RowOverrun { row: usize, got: usize, want: usize },
    /// Leftover bytes at the end (0 trailing allowed).
    TrailingBytes { pos: usize, len: usize },
}

impl std::fmt::Display for PakError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort { len, need } => {
                write!(f, "PAK too short ({len} < {need} bytes)")
            }
            Self::OffsetsNotMonotonic {
                row,
                previous,
                current,
            } => write!(
                f,
                "PAK: row {row} offset ({current}) < previous row ({previous})"
            ),
            Self::RowStartOutOfBounds { row, offset, len } => write!(
                f,
                "PAK: row {row} starts at {offset}, outside the file ({len} B)"
            ),
            Self::RowTruncated { row, at } => {
                write!(f, "PAK: row {row} truncated at byte {at}")
            }
            Self::RowOverrun { row, got, want } => {
                write!(f, "PAK: row {row} overruns ({got} > {want} values)")
            }
            Self::TrailingBytes { pos, len } => write!(
                f,
                "PAK: {len} B unconsumed after the last row (pos {pos}) — 0 trailing allowed"
            ),
        }
    }
}

impl std::error::Error for PakError {}

/// Decompressed PAK: `1001 × 500` values (each row is a series of runs
/// `(u16 count, u8 value)`, `PakFile.cs:110-135`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PakMap {
    buffer: Vec<u8>,
}

impl PakMap {
    /// Decompresses a `.PAK` requiring **exact** consumption of the file.
    pub fn parse(bytes: &[u8]) -> Result<Self, PakError> {
        if bytes.len() < PAK_OFFSET_TABLE_LEN {
            return Err(PakError::TooShort {
                len: bytes.len(),
                need: PAK_OFFSET_TABLE_LEN,
            });
        }
        let mut buffer = vec![0u8; PAK_WIDTH * PAK_HEIGHT];
        let mut pos = 0usize;
        let mut previous = 0u32;
        for row in 0..PAK_HEIGHT {
            let at = row * 4;
            let offset = u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
            if row > 0 && offset < previous {
                return Err(PakError::OffsetsNotMonotonic {
                    row,
                    previous,
                    current: offset,
                });
            }
            previous = offset;
            let mut reader = offset as usize;
            if reader < PAK_OFFSET_TABLE_LEN || reader >= bytes.len() {
                return Err(PakError::RowStartOutOfBounds {
                    row,
                    offset,
                    len: bytes.len(),
                });
            }
            let buffer_pos = PAK_WIDTH * row;
            let mut row_pos = 0usize;
            while row_pos < PAK_WIDTH {
                if reader + 3 > bytes.len() {
                    return Err(PakError::RowTruncated { row, at: reader });
                }
                let count = u16::from_le_bytes([bytes[reader], bytes[reader + 1]]) as usize;
                let value = bytes[reader + 2];
                reader += 3;
                row_pos += count;
                if row_pos > PAK_WIDTH {
                    return Err(PakError::RowOverrun {
                        row,
                        got: row_pos,
                        want: PAK_WIDTH,
                    });
                }
                let start = buffer_pos + row_pos - count;
                buffer[start..buffer_pos + row_pos].fill(value);
            }
            pos = pos.max(reader);
        }
        if pos != bytes.len() {
            return Err(PakError::TrailingBytes {
                pos,
                len: bytes.len() - pos,
            });
        }
        Ok(Self { buffer })
    }

    /// PAK value at `(x, y)` with the `PakFile.cs:162-172` validation
    /// (`-1` in DFU; `None` here).
    pub fn value(&self, x: usize, y: usize) -> Option<u8> {
        if x >= PAK_WIDTH || y >= PAK_HEIGHT {
            return None;
        }
        Some(self.buffer[y * PAK_WIDTH + x])
    }

    /// Complete buffer (1001 × 500, row-major).
    pub fn buffer(&self) -> &[u8] {
        &self.buffer
    }

    /// Mutable buffer (coastal dilation modifies the runtime copy).
    pub fn buffer_mut(&mut self) -> &mut [u8] {
        &mut self.buffer
    }
}

/// Climate/nature domain error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClimateError {
    /// `CLIMATE.PAK` value outside `223..=232` (`MapsFile.Climates`).
    ClimateOutOfRange { value: u8 },
    /// `NatureSet` without a known archive.
    UnknownNatureSet { value: u32 },
}

impl std::fmt::Display for ClimateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ClimateOutOfRange { value } => write!(
                f,
                "climate {value} out of range (223..=232, MapsFile.Climates)"
            ),
            Self::UnknownNatureSet { value } => {
                write!(f, "unknown nature set {value} (500..=511)")
            }
        }
    }
}

impl std::error::Error for ClimateError {}

/// Default climate base when a map-pixel does not declare a valid one
/// (`MapsFile.DefaultClimate = 231`).
pub const DEFAULT_CLIMATE: u8 = 231;

/// Climate map (`CLIMATE.PAK`) with the `MapsFile.cs:891-896` indexing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClimateMap {
    pak: PakMap,
}

impl ClimateMap {
    /// Parses `CLIMATE.PAK` (14,771 B retail, see module).
    pub fn parse(bytes: &[u8]) -> Result<Self, PakError> {
        Ok(Self {
            pak: PakMap::parse(bytes)?,
        })
    }

    /// `GetClimateIndex(mapPixelX, mapPixelY)` — adds +1 to X
    /// (`MapsFile.cs:891-896`). `None` if outside the grid.
    pub fn climate_index(&self, map_pixel_x: i32, map_pixel_y: i32) -> Option<u8> {
        if map_pixel_x < 0 || map_pixel_y < 0 {
            return None;
        }
        self.pak
            .value(map_pixel_x as usize + 1, map_pixel_y as usize)
    }

    /// Typed climate; value outside `Climates` ⇒ [`ClimateError`].
    pub fn climate(&self, map_pixel_x: i32, map_pixel_y: i32) -> Result<WorldClimate, ClimateError> {
        match self.climate_index(map_pixel_x, map_pixel_y) {
            Some(value) => WorldClimate::from_index(value),
            None => Err(ClimateError::ClimateOutOfRange { value: u8::MAX }),
        }
    }

    /// Distinct values present in the map (sorted).
    pub fn distinct_climates(&self) -> BTreeSet<u8> {
        self.pak.buffer().iter().copied().collect()
    }

    /// Dilates land climate into the ocean over the Moore neighbourhood
    /// (`TerrainHelper.cs:383-410` + `TransferLandToOcean` `:416-437`).
    /// Returns how many ocean pixels gained land climate.
    pub fn dilate_coastal(&mut self, passes: usize) -> usize {
        let mut changed = 0usize;
        for _ in 0..passes {
            let source = self.pak.buffer().to_vec();
            for y in 1..PAK_HEIGHT - 1 {
                for x in 1..PAK_WIDTH - 1 {
                    // `srcOffset = srcY * 1001 + (srcX + 1)` — the +1 on X is
                    // already applied in the PAK grid.
                    let src = source[y * PAK_WIDTH + x];
                    if src == OCEAN_CLIMATE {
                        continue;
                    }
                    for (dx, dy) in MOORE_NEIGHBOURHOOD {
                        let nx = (x as i32 + dx) as usize;
                        let ny = (y as i32 + dy) as usize;
                        let dst = ny * PAK_WIDTH + nx;
                        if source[dst] == OCEAN_CLIMATE {
                            self.pak.buffer_mut()[dst] = src;
                            changed += 1;
                        }
                    }
                }
            }
        }
        changed
    }

    /// Direct access to the PAK grid (for dilation tests/probes).
    pub fn raw_value(&self, x: usize, y: usize) -> Option<u8> {
        self.pak.value(x, y)
    }
}

/// Moore neighbourhood of 8 (offsets), in the order of
/// `TerrainHelper.cs:400-407`.
pub const MOORE_NEIGHBOURHOOD: [(i32, i32); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

/// Political map (`POLITIC.PAK`) with the `MapsFile.cs:904-907` indexing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoliticMap {
    pak: PakMap,
}

impl PoliticMap {
    /// Parses `POLITIC.PAK` (19,556 B retail).
    pub fn parse(bytes: &[u8]) -> Result<Self, PakError> {
        Ok(Self {
            pak: PakMap::parse(bytes)?,
        })
    }

    /// `GetPoliticIndex(mapPixelX, mapPixelY)` — adds +1 to X.
    pub fn politic_index(&self, map_pixel_x: i32, map_pixel_y: i32) -> Option<u8> {
        if map_pixel_x < 0 || map_pixel_y < 0 {
            return None;
        }
        self.pak
            .value(map_pixel_x as usize + 1, map_pixel_y as usize)
    }

    /// Distinct political values (faction per map-pixel), sorted.
    pub fn distinct_politics(&self) -> BTreeSet<u8> {
        self.pak.buffer().iter().copied().collect()
    }
}

/// `MapsFile.Climates` enum (`MapsFile.cs:289-301`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WorldClimate {
    /// 223 — ocean.
    Ocean,
    /// 224 — desert.
    Desert,
    /// 225 — Dak'fron desert.
    Desert2,
    /// 226 — mountain.
    Mountain,
    /// 227 — rainforest.
    Rainforest,
    /// 228 — swamp.
    Swamp,
    /// 229 — subtropical.
    Subtropical,
    /// 230 — mountain woods.
    MountainWoods,
    /// 231 — woodlands (default climate).
    Woodlands,
    /// 232 — haunted woodlands.
    HauntedWoodlands,
}

impl WorldClimate {
    /// Converts the raw `CLIMATE.PAK` value (`223..=232`).
    pub fn from_index(value: u8) -> Result<Self, ClimateError> {
        Ok(match value {
            223 => Self::Ocean,
            224 => Self::Desert,
            225 => Self::Desert2,
            226 => Self::Mountain,
            227 => Self::Rainforest,
            228 => Self::Swamp,
            229 => Self::Subtropical,
            230 => Self::MountainWoods,
            231 => Self::Woodlands,
            232 => Self::HauntedWoodlands,
            other => return Err(ClimateError::ClimateOutOfRange { value: other }),
        })
    }

    /// Raw value in `CLIMATE.PAK`.
    pub fn index(self) -> u8 {
        match self {
            Self::Ocean => 223,
            Self::Desert => 224,
            Self::Desert2 => 225,
            Self::Mountain => 226,
            Self::Rainforest => 227,
            Self::Swamp => 228,
            Self::Subtropical => 229,
            Self::MountainWoods => 230,
            Self::Woodlands => 231,
            Self::HauntedWoodlands => 232,
        }
    }

    /// Stable name for probes.
    pub fn name(self) -> &'static str {
        match self {
            Self::Ocean => "Ocean",
            Self::Desert => "Desert",
            Self::Desert2 => "Desert2",
            Self::Mountain => "Mountain",
            Self::Rainforest => "Rainforest",
            Self::Swamp => "Swamp",
            Self::Subtropical => "Subtropical",
            Self::MountainWoods => "MountainWoods",
            Self::Woodlands => "Woodlands",
            Self::HauntedWoodlands => "HauntedWoodlands",
        }
    }

    /// `MapsFile.GetWorldClimateSettings(this)` (`MapsFile.cs:440-532`).
    pub fn settings(self) -> ClimateSettings {
        let (climate_type, ground_archive, nature_texture_set, sky_base, people) = match self {
            Self::Ocean => ("Swamp", 402, 504, 24, "Breton"),
            Self::Desert | Self::Desert2 => ("Desert", 2, 503, 8, "Redguard"),
            Self::Mountain => ("Mountain", 102, 510, 0, "Nord"),
            Self::Rainforest => ("Swamp", 402, 500, 24, "Redguard"),
            Self::Swamp => ("Swamp", 402, 502, 24, "Breton"),
            Self::Subtropical => ("Desert", 2, 501, 24, "Breton"),
            Self::MountainWoods => ("Temperate", 102, 506, 16, "Breton"),
            Self::Woodlands => ("Temperate", 302, 504, 16, "Breton"),
            Self::HauntedWoodlands => ("Temperate", 302, 508, 16, "Breton"),
        };
        ClimateSettings {
            world_climate: self,
            climate_type,
            ground_archive,
            nature_texture_set,
            nature_set: NatureSet::from_texture_set(nature_texture_set),
            sky_base,
            people,
        }
    }
}

/// `DFLocation.ClimateSettings` reduced to what the terrain uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClimateSettings {
    /// World climate that produced these settings.
    pub world_climate: WorldClimate,
    /// `ClimateBaseType` (`Temperate`/`Desert`/`Mountain`/`Swamp`).
    pub climate_type: &'static str,
    /// Ground archive (`GetGroundArchive`, `ClimateSwaps.cs:454-...`).
    pub ground_archive: u32,
    /// Nature texture set (`DFLocation.ClimateTextureSet`, 500..511).
    pub nature_texture_set: u32,
    /// Equivalent nature set (`ClimateSwaps.FromAPITextureSet`).
    pub nature_set: NatureSet,
    /// `SkyBase` of the climate.
    pub sky_base: u32,
    /// Dominant race of the region (`FactionRaces`).
    pub people: &'static str,
}

/// `ClimateSwaps.ClimateNatureSets`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NatureSet {
    /// 500 — rainforest.
    RainForest,
    /// 501 — subtropical.
    SubTropical,
    /// 502 — swamp.
    Swamp,
    /// 503 — desert.
    Desert,
    /// 504 — temperate woodland.
    TemperateWoodland,
    /// 506 — woodland hills.
    WoodlandHills,
    /// 508 — haunted woodlands.
    HauntedWoodlands,
    /// 510 — mountains.
    Mountains,
}

impl NatureSet {
    /// `ClimateSwaps.FromAPITextureSet` (`ClimateSwaps.cs:354-378`); the
    /// snow variants (505/507/509/511) collapse to their base set.
    pub fn from_texture_set(value: u32) -> Self {
        match value {
            500 => Self::RainForest,
            501 => Self::SubTropical,
            502 => Self::Swamp,
            503 => Self::Desert,
            504 | 505 => Self::TemperateWoodland,
            506 | 507 => Self::WoodlandHills,
            508 | 509 => Self::HauntedWoodlands,
            510 | 511 => Self::Mountains,
            _ => Self::TemperateWoodland,
        }
    }

    /// Base `DFLocation.ClimateTextureSet` (without snow).
    pub fn texture_set(self) -> u32 {
        match self {
            Self::RainForest => 500,
            Self::SubTropical => 501,
            Self::Swamp => 502,
            Self::Desert => 503,
            Self::TemperateWoodland => 504,
            Self::WoodlandHills => 506,
            Self::HauntedWoodlands => 508,
            Self::Mountains => 510,
        }
    }

    /// `true` if the set has a winter archive
    /// (`ClimateSwaps.cs:427-444`).
    pub fn supports_winter(self) -> bool {
        matches!(
            self,
            Self::TemperateWoodland | Self::WoodlandHills | Self::HauntedWoodlands | Self::Mountains
        )
    }

    /// Stable name for probes.
    pub fn name(self) -> &'static str {
        match self {
            Self::RainForest => "RainForest",
            Self::SubTropical => "SubTropical",
            Self::Swamp => "Swamp",
            Self::Desert => "Desert",
            Self::TemperateWoodland => "TemperateWoodland",
            Self::WoodlandHills => "WoodlandHills",
            Self::HauntedWoodlands => "HauntedWoodlands",
            Self::Mountains => "Mountains",
        }
    }
}

/// `ClimateSwaps.ClimateSeason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Season {
    /// Summer (base archive).
    Summer,
    /// Winter (+1 only on sets with snow).
    Winter,
    /// Rain (does not affect the nature archive).
    Rain,
}

impl Season {
    /// World season (`DaggerfallDateTime.Seasons`) — the DFU terrain
    /// only distinguishes winter (`StreamingWorld.cs:1267-1271`).
    pub fn from_world_season(season: WorldSeason) -> Self {
        match season {
            WorldSeason::Winter => Self::Winter,
            _ => Self::Summer,
        }
    }
}

/// `DaggerfallDateTime.Seasons` (the terrain only uses winter).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldSeason {
    /// Spring.
    Spring,
    /// Summer.
    Summer,
    /// Autumn.
    Autumn,
    /// Winter.
    Winter,
}

/// `ClimateSwaps.GetNatureArchive(natureSet, climateSeason)`
/// (`ClimateSwaps.cs:389-446`): the set's 500..511 archive, `+1` in winter
/// only for the four sets with snow.
pub fn nature_archive(set: NatureSet, season: Season) -> u32 {
    let mut archive = set.texture_set();
    if season == Season::Winter && set.supports_winter() {
        archive += 1;
    }
    archive
}

/// `GetNatureArchive(ClimateTextureSet, Seasons)` — variant starting from the
/// texture set stored in `ClimateSettings` (`ClimateSwaps.cs:379-387`).
pub fn nature_archive_for_texture_set(texture_set: u32, season: Season) -> u32 {
    nature_archive(NatureSet::from_texture_set(texture_set), season)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic PAK: 500 rows, each with a single run of 1001 values.
    fn synthetic_pak(value: u8) -> Vec<u8> {
        let mut bytes = vec![0u8; PAK_OFFSET_TABLE_LEN];
        for row in 0..PAK_HEIGHT {
            let offset = (PAK_OFFSET_TABLE_LEN + row * 3) as u32;
            let at = row * 4;
            bytes[at..at + 4].copy_from_slice(&offset.to_le_bytes());
            bytes.extend_from_slice(&(PAK_WIDTH as u16).to_le_bytes());
            bytes.push(value);
        }
        bytes
    }

    #[test]
    fn parse_rejects_trailing_bytes() {
        let mut bytes = synthetic_pak(231);
        bytes.push(0);
        assert_eq!(
            PakMap::parse(&bytes).unwrap_err(),
            PakError::TrailingBytes {
                pos: PAK_OFFSET_TABLE_LEN + PAK_HEIGHT * 3,
                len: 1
            }
        );
    }

    #[test]
    fn parse_rejects_non_monotonic_offsets() {
        let mut bytes = synthetic_pak(231);
        bytes[4..8].copy_from_slice(&10u32.to_le_bytes());
        assert_eq!(
            PakMap::parse(&bytes).unwrap_err(),
            PakError::OffsetsNotMonotonic {
                row: 1,
                previous: 2000,
                current: 10
            }
        );
    }

    #[test]
    fn parse_rejects_row_overrun() {
        let mut bytes = synthetic_pak(231);
        // First run of row 0: count = 1002 > 1001.
        let at = PAK_OFFSET_TABLE_LEN;
        bytes[at..at + 2].copy_from_slice(&1002u16.to_le_bytes());
        assert_eq!(
            PakMap::parse(&bytes).unwrap_err(),
            PakError::RowOverrun {
                row: 0,
                got: 1002,
                want: 1001
            }
        );
    }

    #[test]
    fn climate_index_applies_the_x_offset() {
        let bytes = synthetic_pak(226);
        let climate = ClimateMap::parse(&bytes).expect("valid PAK");
        // `GetClimateIndex` reads (x + 1, y) — `MapsFile.cs:891-896`.
        assert_eq!(climate.climate_index(0, 0), Some(climate.raw_value(1, 0).unwrap()));
        assert_eq!(climate.climate(41, 17).unwrap(), WorldClimate::Mountain);
        assert_eq!(climate.climate_index(-1, 0), None);
    }

    #[test]
    fn climate_rejects_out_of_range_values() {
        let climate = ClimateMap::parse(&synthetic_pak(200)).expect("valid PAK");
        assert_eq!(
            climate.climate(0, 0).unwrap_err(),
            ClimateError::ClimateOutOfRange { value: 200 }
        );
    }

    #[test]
    fn dilation_fills_coastal_ocean_with_land_climate() {
        // Every row ocean except row 250, with a single land column
        // at PAK position x = 1.
        let mut rows: Vec<Vec<u8>> = Vec::with_capacity(PAK_HEIGHT);
        for row in 0..PAK_HEIGHT {
            let mut bytes = Vec::new();
            if row == 250 {
                bytes.extend_from_slice(&1u16.to_le_bytes());
                bytes.push(OCEAN_CLIMATE);
                bytes.extend_from_slice(&1u16.to_le_bytes());
                bytes.push(231);
                bytes.extend_from_slice(&(PAK_WIDTH as u16 - 2).to_le_bytes());
                bytes.push(OCEAN_CLIMATE);
            } else {
                bytes.extend_from_slice(&(PAK_WIDTH as u16).to_le_bytes());
                bytes.push(OCEAN_CLIMATE);
            }
            rows.push(bytes);
        }
        let mut bytes = vec![0u8; PAK_OFFSET_TABLE_LEN];
        let mut offset = PAK_OFFSET_TABLE_LEN as u32;
        for (row, body) in rows.iter().enumerate() {
            let at = row * 4;
            bytes[at..at + 4].copy_from_slice(&offset.to_le_bytes());
            offset += body.len() as u32;
        }
        for body in &rows {
            bytes.extend_from_slice(body);
        }
        let mut climate = ClimateMap::parse(&bytes).expect("valid PAK");
        assert_eq!(climate.raw_value(1, 250), Some(231));
        assert_eq!(climate.raw_value(2, 250), Some(OCEAN_CLIMATE));
        let changed = climate.dilate_coastal(1);
        assert_eq!(changed, 8, "the complete Moore neighbourhood of 1 land pixel");
        assert_eq!(climate.raw_value(2, 250), Some(231));
        assert_eq!(climate.raw_value(0, 249), Some(231));
    }

    #[test]
    fn nature_archive_matches_climate_swaps() {
        // `ClimateSwaps.cs:389-446`.
        assert_eq!(nature_archive(NatureSet::TemperateWoodland, Season::Summer), 504);
        assert_eq!(nature_archive(NatureSet::TemperateWoodland, Season::Winter), 505);
        assert_eq!(nature_archive(NatureSet::WoodlandHills, Season::Winter), 507);
        assert_eq!(nature_archive(NatureSet::HauntedWoodlands, Season::Winter), 509);
        assert_eq!(nature_archive(NatureSet::Mountains, Season::Winter), 511);
        // Sets without snow: winter does not change the archive.
        assert_eq!(nature_archive(NatureSet::RainForest, Season::Winter), 500);
        assert_eq!(nature_archive(NatureSet::Desert, Season::Winter), 503);
        assert_eq!(nature_archive(NatureSet::Swamp, Season::Rain), 502);
    }

    #[test]
    fn climate_settings_match_the_world_table() {
        // `MapsFile.cs:440-532`.
        let desert = WorldClimate::Desert.settings();
        assert_eq!(desert.ground_archive, 2);
        assert_eq!(desert.nature_texture_set, 503);
        assert_eq!(desert.sky_base, 8);
        assert_eq!(desert.people, "Redguard");
        let woods = WorldClimate::Woodlands.settings();
        assert_eq!(woods.nature_set, NatureSet::TemperateWoodland);
        assert_eq!(woods.ground_archive, 302);
        let mountain = WorldClimate::Mountain.settings();
        assert_eq!(mountain.people, "Nord");
        assert_eq!(mountain.sky_base, 0);
    }

    #[test]
    fn region_indices_are_enumerable() {
        assert_eq!(regions().count(), REGION_COUNT);
        assert_eq!(regions().last(), Some(60));
    }

    #[test]
    fn world_climate_roundtrips_223_to_232() {
        for value in 223u8..=232 {
            let climate = WorldClimate::from_index(value).expect("Climates enum value");
            assert_eq!(climate.index(), value);
        }
        assert!(WorldClimate::from_index(222).is_err());
        assert!(WorldClimate::from_index(233).is_err());
    }
}

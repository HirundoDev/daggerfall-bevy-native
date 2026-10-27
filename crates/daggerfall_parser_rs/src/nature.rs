//! Nature texture atlases `TEXTURE.500..=511` (M35 / V1, `PROBE-BIOME-001`
//! block D).
//!
//! Normative source:
//! `docs/sintergic/game/programming/m35-vegetation-night-sky-spec-CR-f716455c.md`
//! — §2 (data census: 12 archives × 32 records × 1 frame = 384 sprites,
//! RGBA8 5.77 MiB without mips / 7.69 MiB with mips) and §3.3 rule 1 (one
//! atlas per archive, `alpha(palette index 0) = 0` as the **only**
//! transparency assumption).
//!
//! Custody (INV-L99-6): this module decodes bytes the caller already owns. It
//! never reads a file and never embeds retail data — the archives are opened
//! at runtime and 0 retail byte enters git.
//!
//! The decoder is the existing one ([`crate::texture::decode_frame`] frame 0,
//! [`crate::texture::Palette::to_rgba`]); this module adds the RGBA8
//! normalisation the spec demands (index 0 forced to alpha 0), the per-record
//! measurement the probe reports, and an LRU atlas cache.

use crate::texture::{decode_frame, Palette, TextureArchive, TextureError};

/// Nature archives: `TEXTURE.500..=511` (spec §2 first row, 12 files).
pub const NATURE_ARCHIVES: [u32; 12] = [500, 501, 502, 503, 504, 505, 506, 507, 508, 509, 510, 511];

/// Records per nature archive (spec §2: `32 records × 1 frame`).
pub const RECORDS_PER_ARCHIVE: usize = 32;

/// Sprites of the complete set: `12 × 32 = 384` (spec §2, `384/384` in §6.1 D).
pub const NATURE_SPRITE_COUNT: usize = NATURE_ARCHIVES.len() * RECORDS_PER_ARCHIVE;

/// One mebibyte in bytes (spec §0: VRAM in MiB, `1 MiB = 1 048 576 B`).
pub const MIB_BYTES: u64 = 1_048_576;

/// RGBA8 bytes per pixel: one `u8` per channel, 4 channels (spec §0).
pub const RGBA8_BYTES_PER_PIXEL: u64 = 4;

/// Atlas VRAM allowed by the frame budget, in MiB (spec §3.3:
/// "VRAM ≤ 7,7 MiB +5 %").
pub const NATURE_VRAM_BUDGET_MIB: f64 = 7.7;

/// Margin applied on top of [`NATURE_VRAM_BUDGET_MIB`] (spec §3.3: `+5 %`).
pub const NATURE_VRAM_BUDGET_MARGIN: f64 = 1.05;

/// Atlas VRAM budget in bytes: `7.7 × 1 048 576 × 1.05 = 8 477 736 B`.
///
/// The integer is the floor of the floating-point product declared in
/// [`NATURE_VRAM_BUDGET_MIB`] × [`MIB_BYTES`] × [`NATURE_VRAM_BUDGET_MARGIN`]
/// (see `vram_budget_matches_the_declared_product` in the tests). Two other
/// numbers of spec §2 are recorded next to it, never conflated with it:
/// `6 047 684 B` (5.77 MiB) is the measured RGBA8 total **without** mips and
/// `7.69 MiB` is the same total **with** mips.
pub const NATURE_VRAM_BUDGET_BYTES: u64 = 8_477_736;

/// Spec §2 measured RGBA8 total **without** mips (`6 047 684 B` = 5.77 MiB).
pub const NATURE_RGBA8_NO_MIPS_BYTES: u64 = 6_047_684;

/// Spec §2 measured RGBA8 total **with** mips (`7.69 MiB`).
pub const NATURE_RGBA8_WITH_MIPS_MIB: f64 = 7.69;

/// Nature atlas error: a wrapped texture error plus the census/cache
/// invariants of this module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NatureError {
    /// Underlying `TEXTURE.*` parse or decode failure.
    Texture(TextureError),
    /// The file does not declare exactly [`RECORDS_PER_ARCHIVE`] records
    /// (spec §2 census: 32 records per nature archive).
    RecordCountMismatch {
        /// Records actually declared by `TextureFileHeader.RecordCount`.
        found: usize,
        /// Records the spec census requires.
        expected: usize,
    },
    /// Archive identifier outside [`NATURE_ARCHIVES`].
    ArchiveOutOfRange {
        /// Identifier the caller asked for.
        archive: u32,
    },
    /// A cache with capacity 0 can never serve an atlas.
    ZeroCapacity,
}

impl std::fmt::Display for NatureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Texture(err) => write!(f, "texture error: {err}"),
            Self::RecordCountMismatch { found, expected } => write!(
                f,
                "nature archive declares {found} records, spec census requires {expected}"
            ),
            Self::ArchiveOutOfRange { archive } => {
                write!(f, "archive TEXTURE.{archive} is outside 500..=511")
            }
            Self::ZeroCapacity => write!(f, "nature atlas cache with capacity 0"),
        }
    }
}

impl std::error::Error for NatureError {}

impl From<TextureError> for NatureError {
    fn from(err: TextureError) -> Self {
        Self::Texture(err)
    }
}

/// One decoded nature sprite: RGBA8 pixels plus the transparency measurement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NatureRecord {
    /// Record index inside the archive (`0..RECORDS_PER_ARCHIVE`).
    pub index: usize,
    /// Sprite width in pixels.
    pub width: u16,
    /// Sprite height in pixels.
    pub height: u16,
    /// `width × height` RGBA8 pixels, row-major.
    pub rgba: Vec<u8>,
    /// Pixels whose palette index is 0 before the RGBA conversion. Reported,
    /// never hidden: the probe prints it per archive so a file with an
    /// unexpected transparent area is visible instead of silent.
    pub transparent_pixels: usize,
}

impl NatureRecord {
    /// Pixels of the sprite (`width × height`).
    pub fn pixel_count(&self) -> usize {
        self.width as usize * self.height as usize
    }

    /// VRAM cost of this sprite in bytes (`width × height × 4`).
    pub fn vram_bytes(&self) -> u64 {
        self.pixel_count() as u64 * RGBA8_BYTES_PER_PIXEL
    }

    /// `true` when every palette-index-0 pixel ended up with alpha 0 and no
    /// other pixel did (spec §3.3 rule 1, the single transparency
    /// assumption). The check is exact: the number of fully transparent RGBA
    /// pixels must equal the number of index-0 pixels.
    pub fn alpha0_is_transparent(&self) -> bool {
        let transparent = self.rgba.chunks_exact(4).filter(|px| px[3] == 0).count();
        transparent == self.transparent_pixels
    }
}

/// One nature archive decoded to RGBA8: 32 records, ready for instancing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NatureAtlas {
    /// File name as declared by `TextureFileHeader` (e.g. `TEXTURE.500`).
    pub name: String,
    /// Decoded records in file order (canonical iteration order).
    pub records: Vec<NatureRecord>,
}

impl NatureAtlas {
    /// Parses `file_bytes` (one `TEXTURE.500..511`) and decodes **all**
    /// records at frame 0 into RGBA8.
    ///
    /// `alpha = 0` is forced for every palette-index-0 pixel after
    /// [`Palette::to_rgba`], so the transparency assumption of spec §3.3
    /// holds independently of the palette implementation.
    pub fn parse(file_bytes: &[u8], palette: &Palette) -> Result<Self, NatureError> {
        let archive = TextureArchive::parse(file_bytes)?;
        if archive.records.len() != RECORDS_PER_ARCHIVE {
            return Err(NatureError::RecordCountMismatch {
                found: archive.records.len(),
                expected: RECORDS_PER_ARCHIVE,
            });
        }
        let mut records = Vec::with_capacity(RECORDS_PER_ARCHIVE);
        for index in 0..archive.records.len() {
            let decoded = decode_frame(file_bytes, &archive, index, 0)?;
            let mut rgba = palette.to_rgba(&decoded.pixels);
            let mut transparent_pixels = 0usize;
            for (palette_index, pixel) in decoded.pixels.iter().zip(rgba.chunks_exact_mut(4)) {
                if *palette_index == 0 {
                    pixel[3] = 0;
                    transparent_pixels += 1;
                }
            }
            records.push(NatureRecord {
                index,
                width: decoded.width,
                height: decoded.height,
                rgba,
                transparent_pixels,
            });
        }
        Ok(Self {
            name: archive.name,
            records,
        })
    }

    /// Records decoded (`32` for a spec-conformant archive).
    pub fn record_count(&self) -> usize {
        self.records.len()
    }

    /// Sprites contributed by this archive (one frame per record).
    pub fn sprite_count(&self) -> usize {
        self.records.len()
    }

    /// One record by index, `None` when out of range.
    pub fn record(&self, index: usize) -> Option<&NatureRecord> {
        self.records.get(index)
    }

    /// VRAM of the whole archive in bytes: `Σ width × height × 4`.
    pub fn vram_bytes(&self) -> u64 {
        self.records.iter().map(NatureRecord::vram_bytes).sum()
    }

    /// Total palette-index-0 pixels of the archive (probe reporting).
    pub fn transparent_pixels(&self) -> usize {
        self.records
            .iter()
            .map(|record| record.transparent_pixels)
            .sum()
    }

    /// `true` when every record satisfies [`NatureRecord::alpha0_is_transparent`]
    /// (spec §3.3 rule 1 / `PROBE-BIOME-001` block D).
    pub fn alpha0_is_transparent(&self) -> bool {
        self.records.iter().all(NatureRecord::alpha0_is_transparent)
    }
}

/// LRU cache of decoded nature atlases keyed by archive number.
///
/// Counters (`hits`/`misses`/`evictions`) are part of the contract: the probe
/// prints them to prove the "0 reloads in the same frame" criterion of V1.
#[derive(Debug, Clone)]
pub struct NatureAtlasCache {
    capacity: usize,
    /// Least recently used first, most recently used last.
    entries: Vec<(u32, NatureAtlas)>,
    hits: u64,
    misses: u64,
    evictions: u64,
}

impl NatureAtlasCache {
    /// Cache of `capacity` atlases (capacity 0 is rejected by [`Self::get`]).
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: Vec::with_capacity(capacity),
            hits: 0,
            misses: 0,
            evictions: 0,
        }
    }

    /// Declared capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Atlases currently resident.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` when nothing is resident.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Successful lookups of a resident archive.
    pub fn hits(&self) -> u64 {
        self.hits
    }

    /// Lookups that had to call the loader.
    pub fn misses(&self) -> u64 {
        self.misses
    }

    /// Atlases dropped to make room.
    pub fn evictions(&self) -> u64 {
        self.evictions
    }

    /// VRAM of the resident atlases in bytes.
    pub fn loaded_bytes(&self) -> u64 {
        self.entries
            .iter()
            .map(|(_, atlas)| atlas.vram_bytes())
            .sum()
    }

    /// `true` when `archive` is resident (does not touch the LRU order).
    pub fn contains(&self, archive: u32) -> bool {
        self.entries.iter().any(|(key, _)| *key == archive)
    }

    /// Resident archives, least recently used first.
    pub fn resident(&self) -> impl Iterator<Item = u32> + '_ {
        self.entries.iter().map(|(archive, _)| *archive)
    }

    /// Returns the atlas of `archive`, loading it with `load` on a miss.
    ///
    /// Rejects identifiers outside [`NATURE_ARCHIVES`] with
    /// [`NatureError::ArchiveOutOfRange`] and a zero capacity with
    /// [`NatureError::ZeroCapacity`] **before** calling `load`, so an invalid
    /// request never touches the disk.
    pub fn get(
        &mut self,
        archive: u32,
        load: impl FnOnce() -> Result<NatureAtlas, NatureError>,
    ) -> Result<&NatureAtlas, NatureError> {
        if !NATURE_ARCHIVES.contains(&archive) {
            return Err(NatureError::ArchiveOutOfRange { archive });
        }
        if self.capacity == 0 {
            return Err(NatureError::ZeroCapacity);
        }
        if let Some(position) = self.entries.iter().position(|(key, _)| *key == archive) {
            self.hits += 1;
            let entry = self.entries.remove(position);
            self.entries.push(entry);
            return self.most_recent();
        }
        self.misses += 1;
        let atlas = load()?;
        if self.entries.len() >= self.capacity {
            self.entries.remove(0);
            self.evictions += 1;
        }
        self.entries.push((archive, atlas));
        self.most_recent()
    }

    /// Most recently used entry (`None` only for an empty cache).
    fn most_recent(&self) -> Result<&NatureAtlas, NatureError> {
        match self.entries.last() {
            Some((_, atlas)) => Ok(atlas),
            None => Err(NatureError::ZeroCapacity),
        }
    }
}

impl Default for NatureAtlasCache {
    /// Default capacity: the 12 archives of one complete set.
    fn default() -> Self {
        Self::new(NATURE_ARCHIVES.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::texture::{
        RECORD_HEADER_LEN, ROW_CHUNK, TEXTURE_FILE_HEADER_LEN, TEXTURE_RECORD_LEN,
    };

    /// Own fixture (0 retail bytes — INV-L99-6): `count` records of `w × h`
    /// pixels, all rows interleaved in 256 B chunks exactly like the retail
    /// single-frame scheme (`[row0 rec0][row0 rec1]…[row1 rec0]…`).
    ///
    /// Pixel value of record `i` at row `r`/column `c`:
    /// `(i as u8).wrapping_add(r + c)`, so record 0 holds index 0 in its first
    /// pixel and record `i > 0` holds index ≥ 1 there.
    fn synthetic_archive(count: usize, w: usize, h: usize) -> Vec<u8> {
        let header_len = TEXTURE_FILE_HEADER_LEN + count * RECORD_HEADER_LEN;
        let data_start = header_len + count * TEXTURE_RECORD_LEN;
        let file_len = data_start + h * ROW_CHUNK;
        let mut file = vec![0u8; file_len];
        file[0..2].copy_from_slice(&(count as u16).to_le_bytes());
        let name = b"SYNTH NAT";
        file[2..2 + name.len()].copy_from_slice(name);
        for i in 0..count {
            let at = TEXTURE_FILE_HEADER_LEN + i * RECORD_HEADER_LEN;
            let position = header_len + i * TEXTURE_RECORD_LEN;
            // Record i occupies bytes [i·w, (i+1)·w) of every 256 B chunk.
            let data_at = data_start + i * w;
            file[at..at + 2].copy_from_slice(&(0x1200u16 + i as u16).to_le_bytes());
            file[at + 2..at + 6].copy_from_slice(&(position as u32).to_le_bytes());
            file[at + 6..at + 8].copy_from_slice(&(0x3400u16 + i as u16).to_le_bytes());
            file[at + 8..at + 12].copy_from_slice(&0u32.to_le_bytes());
            let rec = position;
            file[rec + 4..rec + 6].copy_from_slice(&(w as i16).to_le_bytes());
            file[rec + 6..rec + 8].copy_from_slice(&(h as i16).to_le_bytes());
            file[rec + 8..rec + 10].copy_from_slice(&0u16.to_le_bytes());
            file[rec + 10..rec + 14]
                .copy_from_slice(&((TEXTURE_RECORD_LEN + w * h) as u32).to_le_bytes());
            file[rec + 14..rec + 18].copy_from_slice(&((data_at - position) as u32).to_le_bytes());
            file[rec + 18..rec + 20].copy_from_slice(&192u16.to_le_bytes());
            file[rec + 20..rec + 22].copy_from_slice(&1u16.to_le_bytes());
            file[rec + 22..rec + 24].copy_from_slice(&71u16.to_le_bytes());
            for r in 0..h {
                let base = data_at + r * ROW_CHUNK;
                for c in 0..w {
                    file[base + c] = (i as u8).wrapping_add((r + c) as u8);
                }
            }
        }
        file
    }

    /// Own 768 B palette fixture: index `i` maps to `(i, 2i, 3i)` RGB.
    fn synthetic_palette() -> Palette {
        let mut bytes = Vec::with_capacity(768);
        for i in 0..256u32 {
            bytes.push(i as u8);
            bytes.push((2 * i) as u8);
            bytes.push((3 * i) as u8);
        }
        Palette::parse(&bytes).expect("synthetic palette")
    }

    #[test]
    fn census_declares_twelve_archives_of_thirty_two_records() {
        assert_eq!(
            NATURE_ARCHIVES,
            [500, 501, 502, 503, 504, 505, 506, 507, 508, 509, 510, 511]
        );
        assert_eq!(NATURE_ARCHIVES.len(), 12);
        assert_eq!(RECORDS_PER_ARCHIVE, 32);
        assert_eq!(NATURE_SPRITE_COUNT, 384, "12 × 32 = 384 sprites");
    }

    #[test]
    fn vram_budget_matches_the_declared_product() {
        let product =
            (NATURE_VRAM_BUDGET_MIB * MIB_BYTES as f64 * NATURE_VRAM_BUDGET_MARGIN).floor() as u64;
        assert_eq!(NATURE_VRAM_BUDGET_BYTES, product);
        assert_eq!(NATURE_VRAM_BUDGET_BYTES, 8_477_736);
        // The spec §2 measured totals stay distinct from the budget.
        assert_eq!(NATURE_RGBA8_NO_MIPS_BYTES, 6_047_684);
        let no_mips = NATURE_RGBA8_NO_MIPS_BYTES;
        let budget = NATURE_VRAM_BUDGET_BYTES;
        assert!(no_mips < budget, "measured RGBA8 total fits the budget");
        assert!((NATURE_RGBA8_WITH_MIPS_MIB - 7.69).abs() < f64::EPSILON);
    }

    #[test]
    fn parses_all_records_to_rgba8() {
        let file = synthetic_archive(RECORDS_PER_ARCHIVE, 2, 2);
        let atlas = NatureAtlas::parse(&file, &synthetic_palette()).expect("synthetic atlas");
        assert_eq!(atlas.name, "SYNTH NAT");
        assert_eq!(atlas.record_count(), RECORDS_PER_ARCHIVE);
        assert_eq!(atlas.sprite_count(), 32);
        let first = atlas.record(0).expect("record 0");
        assert_eq!((first.index, first.width, first.height), (0, 2, 2));
        assert_eq!(first.rgba.len(), 2 * 2 * 4);
        // Palette index 0 → RGB (0,0,0) with the forced alpha 0.
        assert_eq!(&first.rgba[0..4], &[0, 0, 0, 0]);
        assert_eq!(first.pixel_count(), 4);
        assert_eq!(atlas.vram_bytes(), 32 * 4 * 4);
        assert_eq!(atlas.record(32).map(|r| r.index), None);
    }

    #[test]
    fn alpha_of_palette_index_zero_is_zero_and_others_are_opaque() {
        let file = synthetic_archive(RECORDS_PER_ARCHIVE, 2, 2);
        let atlas = NatureAtlas::parse(&file, &synthetic_palette()).expect("synthetic atlas");
        // Record 0: pixel (0,0) is index 0 → alpha 0.
        let record0 = atlas.record(0).expect("record 0");
        assert_eq!(record0.rgba[3], 0, "index 0 alpha");
        assert!(record0.transparent_pixels >= 1);
        // Record 1: first pixel is index 1 → RGB (1,2,3), alpha 255.
        let record1 = atlas.record(1).expect("record 1");
        assert_eq!(
            &record1.rgba[0..4],
            &[1, 2, 3, 255],
            "index 1 colour + alpha"
        );
        assert!(atlas.alpha0_is_transparent());
        assert!(atlas
            .records
            .iter()
            .all(NatureRecord::alpha0_is_transparent));
    }

    #[test]
    fn record_or_archive_out_of_range_is_an_error() {
        let palette = synthetic_palette();
        // 31 records: below the spec census.
        let short = synthetic_archive(RECORDS_PER_ARCHIVE - 1, 2, 1);
        assert_eq!(
            NatureAtlas::parse(&short, &palette),
            Err(NatureError::RecordCountMismatch {
                found: RECORDS_PER_ARCHIVE - 1,
                expected: RECORDS_PER_ARCHIVE,
            })
        );
        // 33 records: above the spec census.
        let long = synthetic_archive(RECORDS_PER_ARCHIVE + 1, 2, 1);
        assert!(matches!(
            NatureAtlas::parse(&long, &palette),
            Err(NatureError::RecordCountMismatch { found: 33, .. })
        ));
        // Archive identifiers outside 500..=511 never reach the loader.
        let mut cache = NatureAtlasCache::new(2);
        let mut loaded = false;
        let error = cache
            .get(499, || {
                loaded = true;
                Err(NatureError::ZeroCapacity)
            })
            .err();
        assert_eq!(error, Some(NatureError::ArchiveOutOfRange { archive: 499 }));
        assert!(!loaded, "invalid archive must not call the loader");
        assert_eq!(cache.misses(), 0);
        // Zero capacity is rejected, not panicking.
        let mut empty = NatureAtlasCache::new(0);
        assert_eq!(
            empty.get(500, || Err(NatureError::ZeroCapacity)).err(),
            Some(NatureError::ZeroCapacity)
        );
    }

    #[test]
    fn lru_counts_hits_misses_and_evictions() {
        let file = synthetic_archive(RECORDS_PER_ARCHIVE, 2, 2);
        let palette = synthetic_palette();
        let mut cache = NatureAtlasCache::new(2);
        assert!(cache.is_empty());
        for archive in [500u32, 501] {
            let atlas = cache
                .get(archive, || NatureAtlas::parse(&file, &palette))
                .expect("load");
            assert_eq!(atlas.record_count(), RECORDS_PER_ARCHIVE);
        }
        assert_eq!((cache.hits(), cache.misses(), cache.evictions()), (0, 2, 0));
        assert_eq!(cache.loaded_bytes(), 2 * 32 * 4 * 4);
        // Third archive evicts the least recently used one (500).
        cache
            .get(502, || NatureAtlas::parse(&file, &palette))
            .expect("load");
        assert_eq!((cache.hits(), cache.misses(), cache.evictions()), (0, 3, 1));
        assert!(!cache.contains(500));
        assert_eq!(cache.resident().collect::<Vec<_>>(), vec![501, 502]);
        // The evicted archive is a miss again…
        cache
            .get(500, || NatureAtlas::parse(&file, &palette))
            .expect("load");
        assert_eq!((cache.hits(), cache.misses()), (0, 4));
        // …and the second request for it is a hit.
        cache
            .get(500, || NatureAtlas::parse(&file, &palette))
            .expect("hit");
        assert_eq!((cache.hits(), cache.misses()), (1, 4));
        assert_eq!(cache.evictions(), 2);
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn default_cache_holds_one_complete_set() {
        let cache = NatureAtlasCache::default();
        assert_eq!(cache.capacity(), NATURE_ARCHIVES.len());
        assert!(cache.is_empty());
    }
}

//! `WOODS.WLD` — world height map and terrain sampler (M13,
//! DoD §e / WI-6 · REQ-L99-044).
//!
//! Spec anchors (corpus `reference/dfu-spec/`, tag `v1.1.1`,
//! commit `0038a1e61999ad2c73fb1f33b706cae9a88ec66f`):
//!
//! | Fact | Anchor |
//! |---|---|
//! | 8-`u32` header + 28 null `u32` (144 B) | `WoodsFile.cs:383-393` |
//! | `Position = header.HeightMapOffset; ReadBytes(500000)` | `WoodsFile.cs:437-438` |
//! | heightmap 1000 × 500 = 500 000 B | `WoodsFile.cs:30-38` |
//! | sampling `Buffer[y * 1000 + x]` with clamp | `WoodsFile.cs:221-233` |
//! | offset table `dataOffsets[1000*500]` (2 000 000 B) | `WoodsFile.cs:45-50` |
//! | `GetLargeMapData` = `5×5` at `offset + 22` | `WoodsFile.cs:288-311` |
//! | `GetLargeHeightMapValuesRange` (inverts Y, 3×3 per pixel) | `WoodsFile.cs:317-346` |
//! | sampler constants (`baseHeightScale 8`, `noiseMapScale 4`, `extraNoiseScale 10`, ocean `3.4×8`, beach `5.0×8`, max `1539`) | `DefaultTerrainSampler.cs:22-45` |
//! | bicubic formula base+noise and ocean clamp | `DefaultTerrainSampler.cs:92-120` |
//! | `CubicInterpolator` | `TerrainHelper.cs:539-549` |
//! | `height/climate/politic` per map-pixel | `TerrainHelper.cs:62-64` |
//! | `HeightmapDimension` default 129 | `TerrainSampler.cs:96` |
//!
//! ## Empirical layout of `WOODS.WLD` (26 001 168 B)
//!
//! Measured on the file held in `ASSET-REGISTRY.json`
//! (`size_bytes 26001168`, sha256 `2842ab5e…`), 2026-10-20:
//!
//! ```text
//! [0, 144)               header: OffsetSize=2 000 000 · Width=1000 · Height=500 ·
//!                        NullValue1=0 · DataSection1Offset=2 000 144 · Unknown1=1 ·
//!                        Unknown2=22 · HeightMapOffset=2 001 168 · 28 × u32 = 0
//! [144, 2 000 144)       dataOffsets table (500 000 × u32 LE, absolute offsets)
//! [2 000 144, 2 001 168) DataSection1 (1024 B, no known use)
//! [2 001 168, 2 501 168) heightmap 500 × 1000 (500 000 B)
//! [2 501 168, 26 001 168) per-map-pixel blocks (5×5 at block+22)
//! ```
//!
//! Exact invariant: the parser consumes **exactly** `26 001 168` bytes
//! (0 trailing) and rejects any other size with
//! [`WoodsError::SizeMismatch`] — it never guesses the layout.
//!
//! ## `extraNoiseScale` term — bit-exact port
//!
//! `DefaultTerrainSampler.cs:108-114` adds
//! `(lowFreq * highFreq) * extraNoiseScale`, where both factors are
//! `TerrainHelper.GetNoise(...)` (`TerrainHelper.cs:551-568`) over
//! `Mathf.PerlinNoise`:
//!
//! ```text
//! noisex = mapPixelX * (hDim - 1) + x
//! noisey = (MapsFile.MaxMapPixelY - mapPixelY) * (hDim - 1) + y
//! lowFreq  = Clamp01(PerlinNoise(noisex * 0.3, noisey * 0.3) * 0.5)
//! highFreq = Clamp01(PerlinNoise(noisex * 0.9, noisey * 0.9) * 0.5)
//! ```
//!
//! `Mathf.PerlinNoise` is not opaque: its native binding is
//! `PerlinNoise::NoiseNormalized` (`Runtime/Export/Math.txt:2335-2338` of the
//! Unity 4.3.1f1 tree, `http://git.warmcat.org/Unity431f1`) over
//! `Runtime/Terrain/PerlinNoise.cpp` + `PerlinNoise.h`, and the algorithm has
//! not changed between 4.x and the versions used by DFU (the documented value
//! `Mathf.PerlinNoise(0,0) = 0.4652731` is reproduced: `0.69 / 1.483`,
//! `0x3eee3846`). [`mathf_perlin_noise`] is the literal f32 port (same
//! permutation table, same operation order, no FMA), and
//! [`terrain_helper_get_noise`] replicates `GetNoise`. Reference goldens
//! generated with **`PerlinNoise.cpp` itself compiled** (not with the port):
//! 0 ULP difference.
//!
//! Magnitude of the term: each `GetNoise` returns `Perlin ∈ [0,1] × 0.5`, so
//! `lowFreq × highFreq ∈ [0, 0.25]` and the contribution is `[0, 2.5]` sampler
//! units (`[0, 0.1624 %]` of `maxTerrainHeight`), not `[0,10]` as the
//! original HH-1 declared (corrected in HH-1 together with this port).

/// Heightmap width in bytes (`WoodsFile.cs:30`).
pub const MAP_WIDTH: usize = 1000;

/// Heightmap height in bytes (`WoodsFile.cs:33`).
pub const MAP_HEIGHT: usize = 500;

/// Length of the height buffer: `1000 × 500` (`WoodsFile.cs:36`).
pub const HEIGHTMAP_LEN: usize = MAP_WIDTH * MAP_HEIGHT;

/// `WOODS.WLD` header: 8 `u32` + 28 null `u32` (`WoodsFile.cs:383-393`).
pub const HEADER_LEN: usize = 8 * 4 + 28 * 4;

/// `dataOffsets` table: one `u32` per map-pixel (`WoodsFile.cs:45-50`).
pub const OFFSET_TABLE_LEN: usize = HEIGHTMAP_LEN * 4;

/// Official retail file size (`ASSET-REGISTRY.json` → `WOODS.WLD`).
pub const OFFICIAL_SIZE: usize = 26_001_168;

/// `DefaultTerrainSampler.cs:24` — base height scale.
pub const BASE_HEIGHT_SCALE: f32 = 8.0;

/// `DefaultTerrainSampler.cs:25` — large noise map scale.
pub const NOISE_MAP_SCALE: f32 = 4.0;

/// `DefaultTerrainSampler.cs:26` — extra noise scale (Unity Perlin).
pub const EXTRA_NOISE_SCALE: f32 = 10.0;

/// `PerlinNoise.h:6-16` — offset of `NoiseNormalized` (`value + 0.69`).
pub const NOISE_NORMALIZE_OFFSET: f32 = 0.69;

/// `PerlinNoise.h:6-16` — ceiling of the normalized range (`0.793 + 0.69`).
pub const NOISE_NORMALIZE_RANGE: f32 = 0.793 + 0.69;

/// `DefaultTerrainSampler.cs:27` — `scaledOceanElevation = 3.4 × 8 = 27.2`.
pub const OCEAN_ELEVATION: f32 = 3.4 * BASE_HEIGHT_SCALE;

/// `DefaultTerrainSampler.cs:28` — `scaledBeachElevation = 5.0 × 8 = 40.0`.
pub const BEACH_ELEVATION: f32 = 5.0 * BASE_HEIGHT_SCALE;

/// `DefaultTerrainSampler.cs:31` — maximum terrain height.
pub const MAX_TERRAIN_HEIGHT: f32 = 1539.0;

/// `TerrainSampler.cs:96` — default heightmap dimension (129).
pub const HEIGHTMAP_DIM: usize = 129;

/// Sampler constants, in DoD §e order (for the probe).
pub const HEIGHT_CONSTANTS: [(&str, f32); 7] = [
    ("baseHeightScale", BASE_HEIGHT_SCALE),
    ("noiseMapScale", NOISE_MAP_SCALE),
    ("extraNoiseScale", EXTRA_NOISE_SCALE),
    ("oceanElevation", OCEAN_ELEVATION),
    ("beachElevation", BEACH_ELEVATION),
    ("maxTerrainHeight", MAX_TERRAIN_HEIGHT),
    ("heightmapDimension", HEIGHTMAP_DIM as f32),
];

/// Terrain band derived from the scaled height (sampler units).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerrainBand {
    /// `scaled < OceanElevation` (the sampler clamps it to 27.2).
    Ocean,
    /// `OceanElevation ≤ scaled ≤ BeachElevation`.
    Beach,
    /// `scaled > BeachElevation`.
    Land,
}

impl TerrainBand {
    /// Band of a raw scaled height (before clamping).
    pub fn of(scaled_height: f32) -> Self {
        if scaled_height < OCEAN_ELEVATION {
            Self::Ocean
        } else if scaled_height <= BEACH_ELEVATION {
            Self::Beach
        } else {
            Self::Land
        }
    }

    /// `true` if the terrain is walkable (beach and land, not ocean).
    pub fn is_walkable(self) -> bool {
        !matches!(self, Self::Ocean)
    }

    /// Stable name for probes/reports.
    pub fn name(self) -> &'static str {
        match self {
            Self::Ocean => "ocean",
            Self::Beach => "beach",
            Self::Land => "land",
        }
    }
}

/// `WOODS.WLD` parse error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WoodsError {
    /// Size different from the declared one (0 trailing bytes allowed).
    SizeMismatch { len: usize, expected: usize },
    /// Header width/height different from the 1000 × 500 standard.
    BadDimensions { width: u32, height: u32 },
    /// `OffsetSize` different from `500 000 × 4`.
    BadOffsetSize { offset_size: u32 },
    /// `HeightMapOffset` outside the file.
    HeightMapOutOfBounds { offset: u32, need: usize, len: usize },
    /// The offset table does not fit between the header and the heightmap.
    OffsetTableOverlap {
        offset_table_end: usize,
        height_map_offset: usize,
    },
    /// Map-pixel offset outside the file (5×5 block unreachable).
    BlockOutOfBounds {
        x: usize,
        y: usize,
        offset: u32,
        need: usize,
        len: usize,
    },
}

impl std::fmt::Display for WoodsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SizeMismatch { len, expected } => write!(
                f,
                "WOODS.WLD: size {len} ≠ {expected} declared bytes (0 trailing allowed)"
            ),
            Self::BadDimensions { width, height } => {
                write!(f, "WOODS.WLD: header {width}×{height} ≠ 1000×500")
            }
            Self::BadOffsetSize { offset_size } => write!(
                f,
                "WOODS.WLD: OffsetSize={offset_size} ≠ {OFFSET_TABLE_LEN} (500 000 × u32)"
            ),
            Self::HeightMapOutOfBounds { offset, need, len } => write!(
                f,
                "WOODS.WLD: HeightMapOffset={offset} + {need} B outside the file ({len} B)"
            ),
            Self::OffsetTableOverlap {
                offset_table_end,
                height_map_offset,
            } => write!(
                f,
                "WOODS.WLD: offset table ends at {offset_table_end} > HeightMapOffset {height_map_offset}"
            ),
            Self::BlockOutOfBounds {
                x,
                y,
                offset,
                need,
                len,
            } => write!(
                f,
                "WOODS.WLD: block ({x},{y}) at offset {offset} + {need} B outside the file ({len} B)"
            ),
        }
    }
}

impl std::error::Error for WoodsError {}

/// 144 B header of `WOODS.WLD` (`WoodsFile.cs:68-82`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WoodsHeader {
    /// Size of the offset table in bytes (`2 000 000`).
    pub offset_size: u32,
    /// Heightmap width (`1000`).
    pub width: u32,
    /// Heightmap height (`500`).
    pub height: u32,
    /// `NullValue1` (spec: 0).
    pub null_value1: u32,
    /// Start of `DataSection1` (unused).
    pub data_section1_offset: u32,
    /// `Unknown1` (retail: 1).
    pub unknown1: u32,
    /// `Unknown2` (retail: 22).
    pub unknown2: u32,
    /// Absolute offset of the 500 000 B heightmap.
    pub height_map_offset: u32,
}

fn u32_le(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// Parsed `WOODS.WLD`: header + offset table + file bytes.
///
/// The layout is NOT guessed: any deviation from the declared
/// size/invariants returns [`WoodsError`] (0 panics, 0 out-of-range reads).
#[derive(Debug, Clone)]
pub struct WoodsMap {
    header: WoodsHeader,
    /// Absolute heightmap offset (cache of `header.height_map_offset`).
    height_map_offset: usize,
    offsets: Vec<u32>,
    file: Vec<u8>,
}

impl WoodsMap {
    /// Parses `WOODS.WLD` requiring **exactly** [`OFFICIAL_SIZE`] bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, WoodsError> {
        if bytes.len() != OFFICIAL_SIZE {
            return Err(WoodsError::SizeMismatch {
                len: bytes.len(),
                expected: OFFICIAL_SIZE,
            });
        }
        let header = WoodsHeader {
            offset_size: u32_le(bytes, 0),
            width: u32_le(bytes, 4),
            height: u32_le(bytes, 8),
            null_value1: u32_le(bytes, 12),
            data_section1_offset: u32_le(bytes, 16),
            unknown1: u32_le(bytes, 20),
            unknown2: u32_le(bytes, 24),
            height_map_offset: u32_le(bytes, 28),
        };
        if header.width as usize != MAP_WIDTH || header.height as usize != MAP_HEIGHT {
            return Err(WoodsError::BadDimensions {
                width: header.width,
                height: header.height,
            });
        }
        if header.offset_size as usize != OFFSET_TABLE_LEN {
            return Err(WoodsError::BadOffsetSize {
                offset_size: header.offset_size,
            });
        }
        let offset_table_end = HEADER_LEN + OFFSET_TABLE_LEN;
        let height_map_offset = header.height_map_offset as usize;
        if height_map_offset < offset_table_end {
            return Err(WoodsError::OffsetTableOverlap {
                offset_table_end,
                height_map_offset,
            });
        }
        if height_map_offset + HEIGHTMAP_LEN > bytes.len() {
            return Err(WoodsError::HeightMapOutOfBounds {
                offset: header.height_map_offset,
                need: HEIGHTMAP_LEN,
                len: bytes.len(),
            });
        }
        let mut offsets = Vec::with_capacity(HEIGHTMAP_LEN);
        for i in 0..HEIGHTMAP_LEN {
            offsets.push(u32_le(bytes, HEADER_LEN + i * 4));
        }
        Ok(Self {
            header,
            height_map_offset,
            offsets,
            file: bytes.to_vec(),
        })
    }

    /// 144 B header as read.
    pub fn header(&self) -> WoodsHeader {
        self.header
    }

    /// Raw height buffer (500 000 B, `WoodsFile.cs:221-233`).
    pub fn height_map(&self) -> &[u8] {
        &self.file[self.height_map_offset..self.height_map_offset + HEIGHTMAP_LEN]
    }

    /// Raw height of a map-pixel with the exact clamp of
    /// `WoodsFile.cs:221-233`: `x ∈ [0, 999]`, `y ∈ [0, 499]`.
    pub fn height_map_value(&self, x: i32, y: i32) -> u8 {
        let x = x.clamp(0, MAP_WIDTH as i32 - 1) as usize;
        let y = y.clamp(0, MAP_HEIGHT as i32 - 1) as usize;
        self.height_map()[y * MAP_WIDTH + x]
    }

    /// `dim × dim` range of raw heights starting at `(x, y)` (`WoodsFile.cs:262-278`).
    pub fn height_map_range_1dim(&self, x: i32, y: i32, dim: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(dim * dim);
        for row in 0..dim as i32 {
            for col in 0..dim as i32 {
                out.push(self.height_map_value(x + col, y + row));
            }
        }
        out
    }

    /// `5×5` block of a map-pixel read at `offset + 22`
    /// (`WoodsFile.cs:288-311`). `block[y][x]` = DFU's `data[x, y]`.
    pub fn large_map_data(&self, x: i32, y: i32) -> Result<[[u8; 5]; 5], WoodsError> {
        let cx = x.clamp(0, MAP_WIDTH as i32 - 1) as usize;
        let cy = y.clamp(0, MAP_HEIGHT as i32 - 1) as usize;
        let offset = self.offsets[cy * MAP_WIDTH + cx];
        let start = offset as usize + 22;
        let need = 25;
        if (offset as usize) < (self.height_map_offset + HEIGHTMAP_LEN)
            || start + need > self.file.len()
        {
            return Err(WoodsError::BlockOutOfBounds {
                x: cx,
                y: cy,
                offset,
                need,
                len: self.file.len(),
            });
        }
        let mut block = [[0u8; 5]; 5];
        for (row, values) in block.iter_mut().enumerate() {
            let at = start + row * 5;
            values.copy_from_slice(&self.file[at..at + 5]);
        }
        Ok(block)
    }

    /// Large heightmap range with the unknown grid removed and Y
    /// inverted (`WoodsFile.cs:317-346`): `(dim·3)²` values in the order of
    /// `lhm[y * lDim + x] = lhm2[x, y]` (`DefaultTerrainSampler.cs:141-146`),
    /// that is, `out[Y * (dim·3) + X]`.
    pub fn large_height_range(&self, x: i32, y: i32, dim: usize) -> Result<Vec<u8>, WoodsError> {
        let len = dim * 3;
        let mut out = vec![0u8; len * len];
        self.fill_large_range(x, y, dim, &mut out)?;
        Ok(out)
    }

    /// Fills `out` (`(dim·3)²`) without allocating memory: same routine as
    /// [`Self::large_height_range`], used by the sampler per sample.
    fn fill_large_range(
        &self,
        x: i32,
        y: i32,
        dim: usize,
        out: &mut [u8],
    ) -> Result<(), WoodsError> {
        let len = dim * 3;
        debug_assert_eq!(out.len(), len * len);
        for by in 0..dim as i32 {
            for bx in 0..dim as i32 {
                // `GetLargeMapData(mapPixelX + x, mapPixelY - y)` — `WoodsFile.cs:337`.
                let src = self.large_map_data(x + bx, y - by)?;
                for (row_idx, row) in src.iter().enumerate().skip(1).take(3) {
                    // `dstData[.., startY + iy - offsety] = srcData[ix, 4 - iy]`:
                    // the block row is `y = 4 - iy` ⇒ `iy = 4 - row_idx`.
                    let iy = 4 - row_idx;
                    let dst_y = by as usize * 3 + (iy - 1);
                    for (ix, value) in row.iter().enumerate().skip(1).take(3) {
                        // `srcData[x, y]` ⇒ x = ix (column); `src[row][col]`
                        // of [`Self::large_map_data`] is `row = y`, `col = x`.
                        let dst_x = bx as usize * 3 + (ix - 1);
                        out[dst_y * len + dst_x] = *value;
                    }
                }
            }
        }
        Ok(())
    }

    /// Height sample of the whole cell `(gx, gy)` of the map-pixel's
    /// `129 × 129` grid — full `DefaultTerrainSampler.cs:83-123`,
    /// **including** the `extraNoise` term with [`mathf_perlin_noise`].
    ///
    /// DFU's sampler is only evaluated at integer indices (`x`, `y` of the
    /// `IJobParallelFor`); exposing continuous subpixels would invent a Perlin
    /// term with no defined coordinate, so the API is the grid index.
    pub fn sample_grid(&self, map_x: i32, map_y: i32, gx: usize, gy: usize) -> HeightSample {
        self.sample_at(map_x, map_y, gx as i32, gy as i32)
    }

    /// Body of `GenerateSamplesJob` for the integer index `(gx, gy)`.
    fn sample_at(&self, map_x: i32, map_y: i32, gx: i32, gy: i32) -> HeightSample {
        let hd_minus_1 = (HEIGHTMAP_DIM - 1) as f32;
        let x = gx as f32;
        let y = gy as f32;
        // `float div = (HeightmapDimension - 1) / 3f;` — `DefaultTerrainSampler.cs:131`.
        let div = hd_minus_1 / 3.0;
        let ix_f = (x / div).floor();
        let iy_f = (y / div).floor();
        let sfrac_x = x / hd_minus_1;
        let sfrac_y = y / hd_minus_1;
        let frac_x = (x - ix_f * div) / div;
        let frac_y = (y - iy_f * div) / div;
        let ix = ix_f as i32;
        let iy = iy_f as i32;

        // ── base: bicubic over the small 4×4 heightmap ────────────────────
        // `shm = GetHeightMapValuesRange1Dim(mx-2, my-2, 4)`; `JobA.Idx(c, r, 4)`
        // = `r*4 + c` ⇒ x1 walks row 3 … x4 row 0.
        let mut shm = [[0f32; 4]; 4];
        for (row, values) in shm.iter_mut().enumerate() {
            for (col, value) in values.iter_mut().enumerate() {
                *value =
                    self.height_map_value((map_x - 2) + col as i32, (map_y - 2) + row as i32) as f32;
            }
        }
        let x1 = cubic_interpolator(shm[3][0], shm[3][1], shm[3][2], shm[3][3], sfrac_x);
        let x2 = cubic_interpolator(shm[2][0], shm[2][1], shm[2][2], shm[2][3], sfrac_x);
        let x3 = cubic_interpolator(shm[1][0], shm[1][1], shm[1][2], shm[1][3], sfrac_x);
        let x4 = cubic_interpolator(shm[0][0], shm[0][1], shm[0][2], shm[0][3], sfrac_x);
        let base_height = cubic_interpolator(x1, x2, x3, x4, sfrac_y);

        // ── noise: bicubic over the large 9×9 heightmap ───────────────────
        let mut lhm = [0u8; 81];
        // The file was already fully validated in `parse`: the table offsets
        // are inside the file, so the fill cannot fail.
        // (`fill_large_range` returns `Result` because of the public contract
        // of `large_height_range`; an error here would only be possible with a
        // file manipulated *after* the parse, a case the type prevents.)
        if self.fill_large_range(map_x - 1, map_y, 3, &mut lhm).is_err() {
            // Block out of range ⇒ minimum height (ocean), never panic.
            return HeightSample {
                scaled_height: OCEAN_ELEVATION,
                normalized: OCEAN_ELEVATION / MAX_TERRAIN_HEIGHT,
                band: TerrainBand::Ocean,
            };
        }
        let at = |xx: i32, yy: i32| -> f32 { lhm[(yy * 9 + xx) as usize] as f32 };
        let mut rows = [0f32; 4];
        for (j, row) in rows.iter_mut().enumerate() {
            let yy = iy + j as i32;
            *row = cubic_interpolator(
                at(ix, yy),
                at(ix + 1, yy),
                at(ix + 2, yy),
                at(ix + 3, yy),
                frac_x,
            );
        }
        let noise_height = cubic_interpolator(rows[0], rows[1], rows[2], rows[3], frac_y);

        // `scaledHeight += (lowFreq * highFreq) * extraNoiseScale` —
        // `DefaultTerrainSampler.cs:108-114` with the bit-exact port.
        let extra_noise = extra_noise_term(map_x, map_y, gx, gy);
        let mut scaled_height = 0.0f32;
        scaled_height += base_height * BASE_HEIGHT_SCALE;
        scaled_height += noise_height * NOISE_MAP_SCALE;
        scaled_height += extra_noise * EXTRA_NOISE_SCALE;
        let band = TerrainBand::of(scaled_height);
        // `if (scaledHeight < scaledOceanElevation) scaledHeight = scaledOceanElevation`,
        // `Mathf.Clamp01(scaledHeight / maxTerrainHeight)` — `DefaultTerrainSampler.cs:115-118`.
        let clamped = scaled_height.max(OCEAN_ELEVATION);
        HeightSample {
            scaled_height: clamped,
            normalized: (clamped / MAX_TERRAIN_HEIGHT).clamp(0.0, 1.0),
            band,
        }
    }
}

/// Height of one sampler sample (`DefaultTerrainSampler.cs:118-120`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeightSample {
    /// Scaled height after the ocean clamp (sampler units).
    pub scaled_height: f32,
    /// `scaled / MaxTerrainHeight` clamped to `[0, 1]`.
    pub normalized: f32,
    /// Terrain band (ocean/beach/land) of the height **before** clamping.
    pub band: TerrainBand,
}

/// Cubic interpolator from `TerrainHelper.cs:539-549` (Horner form, the one
/// DFU marks as "Faster").
pub fn cubic_interpolator(v0: f32, v1: f32, v2: f32, v3: f32, frac: f32) -> f32 {
    let a = (v3 - v2) - (v0 - v1);
    let b = (v0 - v1) - a;
    let c = v2 - v0;
    let d = v1;
    a * (frac * frac * frac) + b * (frac * frac) + c * frac + d
}

/// Permutation table from `PerlinNoise.cpp:20-48` (Ken Perlin's original
/// duplicated to 512 entries).
const MATHF_PERLIN_PERM: [u8; 512] = [
    151, 160, 137, 91, 90, 15, 131, 13, 201, 95, 96, 53, 194, 233, 7, 225, 140, 36, 103, 30, 69, 142,
    8, 99, 37, 240, 21, 10, 23, 190, 6, 148, 247, 120, 234, 75, 0, 26, 197, 62, 94, 252, 219, 203,
    117, 35, 11, 32, 57, 177, 33, 88, 237, 149, 56, 87, 174, 20, 125, 136, 171, 168, 68, 175, 74,
    165, 71, 134, 139, 48, 27, 166, 77, 146, 158, 231, 83, 111, 229, 122, 60, 211, 133, 230, 220,
    105, 92, 41, 55, 46, 245, 40, 244, 102, 143, 54, 65, 25, 63, 161, 1, 216, 80, 73, 209, 76, 132,
    187, 208, 89, 18, 169, 200, 196, 135, 130, 116, 188, 159, 86, 164, 100, 109, 198, 173, 186, 3,
    64, 52, 217, 226, 250, 124, 123, 5, 202, 38, 147, 118, 126, 255, 82, 85, 212, 207, 206, 59, 227,
    47, 16, 58, 17, 182, 189, 28, 42, 223, 183, 170, 213, 119, 248, 152, 2, 44, 154, 163, 70, 221,
    153, 101, 155, 167, 43, 172, 9, 129, 22, 39, 253, 19, 98, 108, 110, 79, 113, 224, 232, 178, 185,
    112, 104, 218, 246, 97, 228, 251, 34, 242, 193, 238, 210, 144, 12, 191, 179, 162, 241, 81, 51,
    145, 235, 249, 14, 239, 107, 49, 192, 214, 31, 181, 199, 106, 157, 184, 84, 204, 176, 115, 121,
    50, 45, 127, 4, 150, 254, 138, 236, 205, 93, 222, 114, 67, 29, 24, 72, 243, 141, 128, 195, 78,
    66, 215, 61, 156, 180, 151, 160, 137, 91, 90, 15, 131, 13, 201, 95, 96, 53, 194, 233, 7, 225,
    140, 36, 103, 30, 69, 142, 8, 99, 37, 240, 21, 10, 23, 190, 6, 148, 247, 120, 234, 75, 0, 26,
    197, 62, 94, 252, 219, 203, 117, 35, 11, 32, 57, 177, 33, 88, 237, 149, 56, 87, 174, 20, 125,
    136, 171, 168, 68, 175, 74, 165, 71, 134, 139, 48, 27, 166, 77, 146, 158, 231, 83, 111, 229,
    122, 60, 211, 133, 230, 220, 105, 92, 41, 55, 46, 245, 40, 244, 102, 143, 54, 65, 25, 63, 161,
    1, 216, 80, 73, 209, 76, 132, 187, 208, 89, 18, 169, 200, 196, 135, 130, 116, 188, 159, 86,
    164, 100, 109, 198, 173, 186, 3, 64, 52, 217, 226, 250, 124, 123, 5, 202, 38, 147, 118, 126,
    255, 82, 85, 212, 207, 206, 59, 227, 47, 16, 58, 17, 182, 189, 28, 42, 223, 183, 170, 213, 119,
    248, 152, 2, 44, 154, 163, 70, 221, 153, 101, 155, 167, 43, 172, 9, 129, 22, 39, 253, 19, 98,
    108, 110, 79, 113, 224, 232, 178, 185, 112, 104, 218, 246, 97, 228, 251, 34, 242, 193, 238, 210,
    144, 12, 191, 179, 162, 241, 81, 51, 145, 235, 249, 14, 239, 107, 49, 192, 214, 31, 181, 199,
    106, 157, 184, 84, 204, 176, 115, 121, 50, 45, 127, 4, 150, 254, 138, 236, 205, 93, 222, 114,
    67, 29, 24, 72, 243, 141, 128, 195, 78, 66, 215, 61, 156, 180,
];

/// `fade(t) = t³(t(6t − 15) + 10)` — `PerlinNoise.cpp:4`, same f32 order.
#[inline]
fn perlin_fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// `lerp(t, a, b) = a + t(b − a)` — `PerlinNoise.cpp:5`.
#[inline]
fn perlin_lerp(t: f32, a: f32, b: f32) -> f32 {
    a + t * (b - a)
}

/// `grad2(hash, x, y)` — `PerlinNoise.cpp:14-19`, literal.
#[inline]
fn perlin_grad2(hash: i32, x: f32, y: f32) -> f32 {
    let h = hash & 15;
    let u = if h < 8 { x } else { y };
    let v = if h < 4 {
        y
    } else if h == 12 || h == 14 {
        x
    } else {
        0.0
    };
    (if (h & 1) == 0 { u } else { -u }) + (if (h & 2) == 0 { v } else { -v })
}

/// `Mathf.PerlinNoise(x, y)` bit-exact — `PerlinNoise::NoiseNormalized`
/// (`PerlinNoise.h:6-16`) over `PerlinNoise::Noise` (`PerlinNoise.cpp:52-73`),
/// binding in `Runtime/Export/Math.txt:2335-2338` of the Unity 4.3.1f1 tree.
///
/// Reproduces the algorithm without reinterpreting it: `Abs` of both
/// arguments, integer truncation, `& 255`, `fade(min(frac, 1))`, the 512-entry
/// permutation table and `grad2`; finally `(noise + 0.69) / (0.793 + 0.69)`.
/// Sanity anchor published by Unity: `mathf_perlin_noise(0.0, 0.0)`
/// returns `0.4652731` (`0x3eee3846`), the value Unity documents in its
/// forums for any 4.x–2020.x build.
///
/// It does not use `mul_add`: the IEEE order of each operation is part of the
/// contract.
pub fn mathf_perlin_noise(x: f32, y: f32) -> f32 {
    let x = x.abs();
    let y = y.abs();

    let floor_x = x as i32;
    let floor_y = y as i32;

    let big_x = (floor_x & 255) as usize;
    let big_y = floor_y & 255;
    let fx = x - floor_x as f32;
    let fy = y - floor_y as f32;

    let u = perlin_fade(fx.min(1.0));
    let v = perlin_fade(fy.min(1.0));

    let a = MATHF_PERLIN_PERM[big_x] as i32 + big_y;
    let aa = MATHF_PERLIN_PERM[a as usize] as i32;
    let ab = MATHF_PERLIN_PERM[a as usize + 1] as i32;
    let b = MATHF_PERLIN_PERM[big_x + 1] as i32 + big_y;
    let ba = MATHF_PERLIN_PERM[b as usize] as i32;
    let bb = MATHF_PERLIN_PERM[b as usize + 1] as i32;

    let res = perlin_lerp(
        v,
        perlin_lerp(
            u,
            perlin_grad2(MATHF_PERLIN_PERM[aa as usize] as i32, fx, fy),
            perlin_grad2(MATHF_PERLIN_PERM[ba as usize] as i32, fx - 1.0, fy),
        ),
        perlin_lerp(
            u,
            perlin_grad2(MATHF_PERLIN_PERM[ab as usize] as i32, fx, fy - 1.0),
            perlin_grad2(MATHF_PERLIN_PERM[bb as usize] as i32, fx - 1.0, fy - 1.0),
        ),
    );
    (res + NOISE_NORMALIZE_OFFSET) / NOISE_NORMALIZE_RANGE
}

/// `TerrainHelper.GetNoise` (`TerrainHelper.cs:551-568`) literal: sums
/// `octaves` octaves of `Mathf.PerlinNoise(seed + x·freq, seed + y·freq)`
/// multiplied by `amplitude`, doubling the frequency and multiplying the
/// amplitude by `persistance`, with a final `Clamp01`.
pub fn terrain_helper_get_noise(
    x: i32,
    y: i32,
    frequency: f32,
    amplitude: f32,
    persistence: f32,
    octaves: u32,
    seed: i32,
) -> f32 {
    let mut final_value = 0.0f32;
    let mut frequency = frequency;
    let mut amplitude = amplitude;
    for _ in 0..octaves {
        final_value +=
            mathf_perlin_noise(seed as f32 + x as f32 * frequency, seed as f32 + y as f32 * frequency)
                * amplitude;
        frequency *= 2.0;
        amplitude *= persistence;
    }
    // `Mathf.Clamp01`.
    final_value.clamp(0.0, 1.0)
}

/// `lowFreq × highFreq` term of `DefaultTerrainSampler.cs:108-114` for the
/// whole cell `(gx, gy)` of map-pixel `(map_x, map_y)`.
///
/// Exact job coordinates: `noisex = mapPixelX·128 + x`,
/// `noisey = (MaxMapPixelY − mapPixelY)·128 + y` with `MaxMapPixelY = 500`
/// (`MapsFile.cs:45-46`). Range `[0, 0.25]`: both `GetNoise` carry
/// `amplitude = 0.5` and a single octave ⇒ the scaled contribution is
/// `[0, 2.5]`.
pub fn extra_noise_term(map_x: i32, map_y: i32, gx: i32, gy: i32) -> f32 {
    let step = HEIGHTMAP_DIM as i32 - 1;
    let noise_x = map_x * step + gx;
    let noise_y = (MAP_HEIGHT as i32 - map_y) * step + gy;
    let low_frequency = terrain_helper_get_noise(noise_x, noise_y, 0.3, 0.5, 0.5, 1, 0);
    let high_frequency = terrain_helper_get_noise(noise_x, noise_y, 0.9, 0.5, 0.5, 1, 0);
    low_frequency * high_frequency
}


#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic fixture with the exact layout (26 001 168 B): valid header,
    /// offset table with one 5×5 block per map-pixel (47 B stride as in
    /// retail) and a flat heightmap with a known value.
    fn synthetic_woods(height: u8) -> Vec<u8> {
        let mut bytes = vec![0u8; OFFICIAL_SIZE];
        let height_map_offset = 2_001_168u32;
        bytes[0..4].copy_from_slice(&(OFFSET_TABLE_LEN as u32).to_le_bytes());
        bytes[4..8].copy_from_slice(&(MAP_WIDTH as u32).to_le_bytes());
        bytes[8..12].copy_from_slice(&(MAP_HEIGHT as u32).to_le_bytes());
        bytes[16..20].copy_from_slice(&2_000_144u32.to_le_bytes());
        bytes[20..24].copy_from_slice(&1u32.to_le_bytes());
        bytes[24..28].copy_from_slice(&22u32.to_le_bytes());
        bytes[28..32].copy_from_slice(&height_map_offset.to_le_bytes());
        for i in 0..HEIGHTMAP_LEN {
            let offset = 2_501_168u32 + (i as u32) * 47;
            let at = HEADER_LEN + i * 4;
            bytes[at..at + 4].copy_from_slice(&offset.to_le_bytes());
            let block = offset as usize;
            for b in 0..47usize {
                bytes[block + b] = height;
            }
        }
        let start = height_map_offset as usize;
        for b in bytes[start..start + HEIGHTMAP_LEN].iter_mut() {
            *b = height;
        }
        bytes
    }

    #[test]
    fn parse_rejects_wrong_size_with_typed_error() {
        let bytes = synthetic_woods(10);
        assert_eq!(
            WoodsMap::parse(&bytes[..bytes.len() - 1]).unwrap_err(),
            WoodsError::SizeMismatch {
                len: OFFICIAL_SIZE - 1,
                expected: OFFICIAL_SIZE
            }
        );
        assert_eq!(
            WoodsMap::parse(&[bytes.clone(), vec![0u8]].concat()).unwrap_err(),
            WoodsError::SizeMismatch {
                len: OFFICIAL_SIZE + 1,
                expected: OFFICIAL_SIZE
            }
        );
    }

    #[test]
    fn parse_rejects_bad_header_invariants() {
        let mut bytes = synthetic_woods(10);
        bytes[0..4].copy_from_slice(&7u32.to_le_bytes());
        assert_eq!(
            WoodsMap::parse(&bytes).unwrap_err(),
            WoodsError::BadOffsetSize { offset_size: 7 }
        );

        let mut bytes = synthetic_woods(10);
        bytes[4..8].copy_from_slice(&999u32.to_le_bytes());
        assert_eq!(
            WoodsMap::parse(&bytes).unwrap_err(),
            WoodsError::BadDimensions {
                width: 999,
                height: 500
            }
        );
    }

    #[test]
    fn flat_heightmap_samples_the_two_scales() {
        let map = WoodsMap::parse(&synthetic_woods(10)).expect("valid fixture");
        let sample = map.sample_grid(500, 250, 64, 64);
        // base 10 × 8 + noise 10 × 4 = 120 + extraNoise × 10.
        // Reference golden (PerlinNoise.cpp compiled): 0x42f1daa8.
        assert_eq!(sample.scaled_height.to_bits(), 0x42f1daa8);
        assert_eq!(sample.band, TerrainBand::Land);
        assert!((sample.normalized - 120.927_06 / MAX_TERRAIN_HEIGHT).abs() < 1e-6);
    }

    #[test]
    fn ocean_clamps_to_ocean_elevation() {
        let map = WoodsMap::parse(&synthetic_woods(1)).expect("valid fixture");
        let sample = map.sample_grid(10, 10, 64, 64);
        assert_eq!(sample.scaled_height, OCEAN_ELEVATION);
        assert_eq!(sample.band, TerrainBand::Ocean);
        assert!(!sample.band.is_walkable());
    }

    #[test]
    fn beach_band_is_bounded_by_the_two_constants() {
        // Flat 3 ⇒ 3×8 + 3×4 = 36 ∈ (27.2, 40] ⇒ beach (extraNoise ≤ 2.5).
        let map = WoodsMap::parse(&synthetic_woods(3)).expect("valid fixture");
        let sample = map.sample_grid(0, 0, 0, 0);
        assert!(sample.scaled_height >= OCEAN_ELEVATION);
        assert!(sample.scaled_height <= BEACH_ELEVATION);
        assert_eq!(sample.band, TerrainBand::Beach);
        assert!(sample.band.is_walkable());

        // Flat 4 ⇒ 48 > 40 ⇒ land.
        let map = WoodsMap::parse(&synthetic_woods(4)).expect("valid fixture");
        assert_eq!(map.sample_grid(0, 0, 0, 0).band, TerrainBand::Land);
    }

    #[test]
    fn large_height_range_uses_the_offset_table() {
        let map = WoodsMap::parse(&synthetic_woods(7)).expect("valid fixture");
        // The fixture fills the block with 7: the large range has no
        // gaps (0) if the offset table and the Y inversion are correct.
        let range = map.large_height_range(500, 250, 3).expect("valid range");
        assert_eq!(range.len(), 81);
        assert!(range.iter().all(|v| *v == 7));
        let block = map.large_map_data(500, 250).expect("valid block");
        assert_eq!(block[0][0], 7);
    }

    #[test]
    fn same_coordinate_is_byte_identical() {
        let map = WoodsMap::parse(&synthetic_woods(9)).expect("valid fixture");
        let a = map.sample_grid(321, 210, 37, 61);
        let b = map.sample_grid(321, 210, 37, 61);
        assert_eq!(a.scaled_height.to_bits(), 0x42d81440);
        assert_eq!(a.scaled_height.to_bits(), b.scaled_height.to_bits());
        assert_eq!(a.normalized.to_bits(), b.normalized.to_bits());
    }

    /// `Mathf.PerlinNoise` against compiled `PerlinNoise.cpp` (Unity 4.3.1f1
    /// tree): f32 inputs and outputs by bits ⇒ 0 ULP, not a tolerance.
    #[test]
    fn mathf_perlin_noise_matches_unity_reference_bits() {
        // (x_bits, y_bits, expected_bits) — generated with the original C++.
        const GOLDENS: [(u32, u32, u32); 9] = [
            (0x0000_0000, 0x0000_0000, 0x3eee_3846), // (0,0) = 0.4652731 (Unity anchor)
            (0x3f00_0000, 0x3f00_0000, 0x3e97_e886), // (0.5,0.5)
            (0x3f80_0000, 0x3f80_0000, 0x3eee_3846), // (1,1) = (0,0) after truncation
            (0xba83_126f, 0x4049_0fdb, 0x3ef4_9c14), // (-0.001, π) ⇒ Abs
            (0x43a7_2800, 0x4228_0000, 0x3f38_b503), // (334.3125,42)
            (0x4199_999a, 0x4696_2667, 0x3f16_13f7), // lowFreq of (0,0)
            (0x4266_6666, 0x4761_3999, 0x3f03_36f1), // highFreq of (0,0)
            (0x4696_2667, 0x4616_4ccd, 0x3f15_62f7), // lowFreq of (500,250)
            (0x4761_3999, 0x46e1_7333, 0x3f22_ae75), // highFreq of (500,250)
        ];
        for (x_bits, y_bits, expected) in GOLDENS {
            let value = mathf_perlin_noise(f32::from_bits(x_bits), f32::from_bits(y_bits));
            assert_eq!(
                value.to_bits(),
                expected,
                "Mathf.PerlinNoise({}, {}) = {value}",
                f32::from_bits(x_bits),
                f32::from_bits(y_bits)
            );
        }
    }

    /// `TerrainHelper.GetNoise` against the C++ reference (same bits).
    #[test]
    fn terrain_helper_get_noise_matches_dfu_reference_bits() {
        // (x, y, frequency, amplitude, persistence, octaves, expected_bits).
        const GOLDENS: [(i32, i32, f32, f32, f32, u32, u32); 5] = [
            (64064, 32064, 0.3, 0.5, 0.5, 1, 0x3e95_62f7),
            (64064, 32064, 0.9, 0.5, 0.5, 1, 0x3ea2_ae75),
            (128128, 64000, 0.3, 0.5, 0.5, 1, 0x3e6e_3846),
            (128128, 64000, 0.9, 0.5, 0.5, 1, 0x3e8f_7003),
            (12345, 54321, 0.7, 1.0, 0.5, 3, 0x3f1f_0314),
        ];
        for (x, y, frequency, amplitude, persistence, octaves, expected) in GOLDENS {
            assert_eq!(
                terrain_helper_get_noise(x, y, frequency, amplitude, persistence, octaves, 0)
                    .to_bits(),
                expected,
                "GetNoise({x}, {y}, f={frequency}, a={amplitude}, p={persistence}, o={octaves})"
            );
        }
    }

    /// The `lowFreq × highFreq` term is bounded by `0.5 × 0.5 = 0.25`
    /// (⇒ `[0, 2.5]` units), not by `[0,1]` (⇒ `[0,10]`).
    #[test]
    fn extra_noise_term_matches_reference_and_is_bounded() {
        // Golden of (500,250) at the grid centre: 0x3dbddcdc.
        assert_eq!(extra_noise_term(500, 250, 64, 64).to_bits(), 0x3dbd_dcdc);
        let mut max = 0.0f32;
        for map_y in (0..500).step_by(5) {
            for map_x in (0..1000).step_by(7) {
                let value = extra_noise_term(map_x, map_y, 64, 64);
                assert!((0.0..=0.25).contains(&value), "value {value} outside [0,0.25]");
                max = max.max(value);
            }
        }
        assert!(max * EXTRA_NOISE_SCALE <= 2.5, "maximum {max} exceeds 0.25");
    }

    #[test]
    fn cubic_interpolator_hits_its_knots() {
        assert!((cubic_interpolator(1.0, 2.0, 3.0, 4.0, 0.0) - 2.0).abs() < 1e-6);
        assert!((cubic_interpolator(1.0, 2.0, 3.0, 4.0, 1.0) - 3.0).abs() < 1e-6);
    }

    #[test]
    fn heights_constants_match_the_spec() {
        assert_eq!(BASE_HEIGHT_SCALE, 8.0);
        assert_eq!(NOISE_MAP_SCALE, 4.0);
        assert_eq!(EXTRA_NOISE_SCALE, 10.0);
        assert_eq!(NOISE_NORMALIZE_OFFSET, 0.69);
        assert_eq!(NOISE_NORMALIZE_RANGE, 1.483);
        assert_eq!(OCEAN_ELEVATION, 27.2);
        assert_eq!(BEACH_ELEVATION, 40.0);
        assert_eq!(MAX_TERRAIN_HEIGHT, 1539.0);
        assert_eq!(HEIGHTMAP_DIM, 129);
    }
}

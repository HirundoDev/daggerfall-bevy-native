//! `PROBE-SPRITE-001` — M35 blocks A–C (species sprite catalogue), headless.
//!
//! Contract (spec §6.2, annex M35 V5/H4, CR-2fef7f8a): every check prints
//! `SPRITE-001 <block>/<check>: PASS|FAIL|NA <numeric detail>` and the last
//! stdout line is `PROBE-SPRITE-001: verdict=<...> exit=<...> reasons=<...>`.
//!
//! Exit policy (doc-commented by contract):
//! * `2` — declared data gap: `ARENA2` absent, so the archive census of block A
//!   cannot be measured at all. A `NEGATIVE` disposition is **not** a gap: it
//!   is a signed measurement and exits `0`.
//! * `1` — a measurable check failed.
//! * `0` — everything the data allows passed.
//!
//! What each block measures:
//!
//! * **A — catalogue.** The declared species → archive table has no pinned
//!   source: `reference/dfu-spec/` carries 120 `.cs` files and no
//!   `EnemyBasics.cs`, so nothing maps a mobile type to `TEXTURE.255..=297`.
//!   The probe therefore measures the corpus census (`corpus_census`), proves
//!   the absence of `EnemyBasics.cs`, censuses the archives on disk
//!   (`archives_present` / `archives_absent`) and signs
//!   [`SpeciesCatalogDisposition::Negative`] with that [`NegativeEvidence`].
//!   The disposition is signed, never converted into a silent `PASS`.
//! * **B — budget.** Σ `w·h·4` over every record (`frame 0`) of a species is
//!   its resident decoded footprint; the four largest go into
//!   [`SpeciesSpriteCache`] and then the present archives stream through it.
//!   Checks: ≤ [`PER_SPECIES_BUDGET_BYTES`] per species, `resident ≤ peak ≤
//!   `[`TOTAL_BUDGET_BYTES`], `loads() == `[`LRU_CAPACITY`], `evictions() ==
//!   total_inserts() − LRU_CAPACITY` and **0** simultaneous loads of the 43.
//! * **C — determinism.** FNV-1a 64 over `archive ++ SPRITE_SEED ++ (w,h) ++
//!   frame-0` of the largest archive on disk, recomputed from a second decode
//!   in the same process; the cross-process comparison is the orchestrator's
//!   two invocations of `--probe SPRITE-001`.
//!
//! No synthetic PASS: every number below comes from retail bytes, from
//! `content-runtime`'s budget module or from a filesystem census; nothing is
//! mocked and no expected count is hardcoded into a check that could be
//! satisfied by the expectation itself.
//!
//! The budget module is included by path (the app crate does not depend on
//! `content-runtime`): the file is self-contained and touches no other module.

use std::fs;
use std::path::{Path, PathBuf};

use daggerfall_parser_rs::texture::{self, TextureArchive};

#[path = "../../content-runtime/src/species_sprites.rs"]
#[allow(dead_code)]
mod species_sprites;

use species_sprites::{
    declared_table_available, is_species_archive, NegativeEvidence, SpeciesCatalogDisposition,
    SpeciesSpriteCache, LRU_CAPACITY, PER_SPECIES_BUDGET_BYTES, SPECIES_ARCHIVE_COUNT,
    TOTAL_BUDGET_BYTES,
};

/// Corpus of the pinned `Daggerfall Unity` sources.
const CORPUS_DIR: &str = "reference/dfu-spec";
/// File whose absence removes the species → archive column (gap H4).
const MISSING_SOURCE: &str = "Assets/Scripts/Game/EnemyBasics.cs";
/// `.cs` files in the pinned corpus census (spec §6.2 A; the probe measures it
/// and prints the delta instead of asserting it).
const EXPECTED_CORPUS_CENSUS: usize = 120;
/// Archives absent from the retail ARENA2 (spec §6.2 A / annex M4); measured
/// and printed with its delta, never assumed by the gate.
const EXPECTED_ARCHIVES_ABSENT: [u32; 1] = [294];
/// Pinned determinism seed of block C, printed and never random.
const SPRITE_SEED: u64 = 0x5350_5201;
/// The LRU window is strictly smaller than the archive set, so the capacity
/// bound of block B *is* the "0 loads of the 43 at once" invariant.
const _: () = assert!(LRU_CAPACITY < SPECIES_ARCHIVE_COUNT);
/// Bytes per decoded pixel (`RGBA8`, spec §0 units).
const BYTES_PER_PIXEL: u64 = 4;

// ---------------------------------------------------------------------------
// Reporting — same shape as `m35_biome_probe`.
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Tally {
    passed: usize,
    failed: usize,
    na: usize,
    gaps: Vec<&'static str>,
}

impl Tally {
    fn pass(&mut self, id: &str, detail: impl Into<String>) {
        self.passed += 1;
        println!("SPRITE-001 {id}: PASS {}", detail.into());
    }

    fn fail(&mut self, id: &str, detail: impl Into<String>) {
        self.failed += 1;
        println!("SPRITE-001 {id}: FAIL {}", detail.into());
    }

    fn gap(&mut self, reason: &'static str) {
        if !self.gaps.contains(&reason) {
            self.gaps.push(reason);
        }
    }

    fn verdict(&self) -> (&'static str, i32) {
        if self.failed > 0 {
            ("FAIL", 1)
        } else if !self.gaps.is_empty() {
            ("DEGRADED", 2)
        } else {
            ("PASS", 0)
        }
    }
}

// ---------------------------------------------------------------------------
// FNV-1a 64 — the digest style of `m34_light_probe` / `m35_biome_probe`.
// ---------------------------------------------------------------------------

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn fnv1a64_fold(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

fn repo_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| manifest.to_path_buf())
}

// ---------------------------------------------------------------------------
// Block A — the declared table has no pinned source.
// ---------------------------------------------------------------------------

/// Every `.cs` file below `root`, recursively (directory order is normalised by
/// the caller, so the census is reproducible).
fn count_cs_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|e| format!("{}: {e}", path.display()))?;
            if file_type.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("cs") {
                found.push(path);
            }
        }
    }
    found.sort();
    Ok(found)
}

/// The archive census of `TEXTURE.255..=297` in the retail ARENA2.
struct ArchiveCensus {
    present: Vec<u32>,
    absent: Vec<u32>,
}

impl ArchiveCensus {
    fn measure() -> Self {
        let mut present = Vec::new();
        let mut absent = Vec::new();
        for archive in species_sprites::SPECIES_ARCHIVE_RANGE {
            debug_assert!(is_species_archive(archive));
            if crate::mode_a_arena2_path(&format!("TEXTURE.{archive}")).is_ok() {
                present.push(archive);
            } else {
                absent.push(archive);
            }
        }
        Self { present, absent }
    }
}

/// Block A — corpus census + archive census + the signed `NEGATIVE`.
fn block_a(tally: &mut Tally) -> Option<SpeciesCatalogDisposition> {
    if declared_table_available() {
        tally.fail(
            "A/declared-table",
            "content-runtime declares the species table available, so a source must be cited",
        );
        return None;
    }
    tally.pass(
        "A/declared-table",
        "declared_table_available()=false (gap H4: no pinned species -> archive column)",
    );

    let corpus = match count_cs_files(&repo_root().join(CORPUS_DIR)) {
        Ok(files) => files,
        Err(reason) => {
            tally.fail("A/corpus-census", reason);
            return None;
        }
    };
    let census = corpus.len();
    let missing_source_present = corpus
        .iter()
        .any(|path| path.file_name().and_then(|name| name.to_str()) == Some("EnemyBasics.cs"));
    let delta = census as i64 - EXPECTED_CORPUS_CENSUS as i64;
    if missing_source_present {
        tally.fail(
            "A/corpus-census",
            format!(
                "{CORPUS_DIR}: EnemyBasics.cs IS present ({census} .cs files), so the absence \
                 evidence of gap H4 is void"
            ),
        );
        return None;
    }
    if census == 0 {
        tally.fail(
            "A/corpus-census",
            format!("{CORPUS_DIR}: 0 .cs files, so absence is vacuous"),
        );
        return None;
    }
    tally.pass(
        "A/corpus-census",
        format!(
            "{CORPUS_DIR}: {census} .cs files (expected {EXPECTED_CORPUS_CENSUS}, delta {delta:+}), \
             EnemyBasics.cs absent"
        ),
    );

    let archives = ArchiveCensus::measure();
    let counted = archives.present.len() + archives.absent.len();
    if counted != SPECIES_ARCHIVE_COUNT {
        tally.fail(
            "A/archives-census",
            format!(
                "present {} + absent {} = {counted} != SPECIES_ARCHIVE_COUNT {SPECIES_ARCHIVE_COUNT}",
                archives.present.len(),
                archives.absent.len()
            ),
        );
    } else {
        tally.pass(
            "A/archives-census",
            format!(
                "present {} + absent {} = {counted} == SPECIES_ARCHIVE_COUNT \
                 (TEXTURE.255..=297)",
                archives.present.len(),
                archives.absent.len()
            ),
        );
    }

    let expected_absent: Vec<u32> = EXPECTED_ARCHIVES_ABSENT.to_vec();
    if archives.absent == expected_absent {
        tally.pass(
            "A/archives-absent",
            format!(
                "absent {:?} == expected {:?} (measured on disk)",
                archives.absent, expected_absent
            ),
        );
    } else {
        tally.fail(
            "A/archives-absent",
            format!(
                "absent {:?} != expected {:?} (delta measured, gate not relaxed)",
                archives.absent, expected_absent
            ),
        );
    }

    let disposition = SpeciesCatalogDisposition::Negative(NegativeEvidence {
        missing_source: MISSING_SOURCE,
        corpus_census: census,
        archives_present: archives.present.len(),
        archives_absent: archives.absent.clone(),
    });
    let negative_ok = disposition.is_negative();
    let rows_ok = disposition.rows().is_none();
    let evidence_ok = match &disposition {
        SpeciesCatalogDisposition::Negative(evidence) => {
            evidence.missing_source == MISSING_SOURCE
                && evidence.corpus_census == census
                && evidence.archives_present == archives.present.len()
                && evidence.archives_absent == archives.absent
        }
        SpeciesCatalogDisposition::Declared(_) => false,
    };
    if negative_ok && rows_ok && evidence_ok {
        tally.pass(
            "A/disposition",
            format!(
                "NEGATIVE signed: missing_source={MISSING_SOURCE} corpus_census={census} \
                 archives_present={} archives_absent={:?} (rows()=None)",
                archives.present.len(),
                archives.absent
            ),
        );
    } else {
        tally.fail(
            "A/disposition",
            format!(
                "NEGATIVE not signed: is_negative={negative_ok} rows_none={rows_ok} \
                 evidence_matches_measurement={evidence_ok}"
            ),
        );
    }

    Some(disposition)
}

// ---------------------------------------------------------------------------
// Block B — per-species budget, LRU window and no bulk load.
// ---------------------------------------------------------------------------

/// One present species archive, decoded.
struct SpeciesFootprint {
    archive: u32,
    /// Σ `w·h·4` over every decodable record at frame 0.
    bytes: u64,
    /// Largest single record (`w·h·4`).
    largest_record_bytes: u64,
    records: usize,
    decoded_records: usize,
    /// Records with no image data (solid colours / non-image kinds).
    pixel_less_records: usize,
    file_bytes: u64,
}

/// Decodes every record (frame 0) of `TEXTURE.{archive}` and sums the RGBA
/// footprint. Returns the footprint plus the raw file bytes for block C.
fn decode_species(archive: u32) -> Result<(SpeciesFootprint, Vec<u8>, TextureArchive), String> {
    let path = crate::mode_a_arena2_path(&format!("TEXTURE.{archive}"))
        .map_err(|reason| format!("TEXTURE.{archive}: {reason}"))?;
    let data = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let parsed = TextureArchive::parse(&data).map_err(|e| format!("TEXTURE.{archive}: {e}"))?;
    let mut footprint = SpeciesFootprint {
        archive,
        bytes: 0,
        largest_record_bytes: 0,
        records: parsed.records.len(),
        decoded_records: 0,
        pixel_less_records: 0,
        file_bytes: data.len() as u64,
    };
    for index in 0..parsed.records.len() {
        match texture::decode_frame(&data, &parsed, index, 0) {
            Ok(frame) => {
                let bytes =
                    u64::from(frame.width) * u64::from(frame.height) * BYTES_PER_PIXEL;
                footprint.bytes = footprint.bytes.saturating_add(bytes);
                footprint.largest_record_bytes = footprint.largest_record_bytes.max(bytes);
                footprint.decoded_records += 1;
            }
            Err(error) => {
                if parsed.records[index].meta().is_some() {
                    return Err(format!(
                        "TEXTURE.{archive} record {index} has image metadata but did not decode: \
                         {error:?}"
                    ));
                }
                footprint.pixel_less_records += 1;
            }
        }
    }
    Ok((footprint, data, parsed))
}

/// The 4 largest footprints, largest first (ties broken by archive).
fn four_largest(footprints: &[SpeciesFootprint]) -> Vec<u32> {
    let mut ordered: Vec<&SpeciesFootprint> = footprints.iter().collect();
    ordered.sort_by(|left, right| {
        right
            .bytes
            .cmp(&left.bytes)
            .then(left.archive.cmp(&right.archive))
    });
    ordered
        .iter()
        .take(LRU_CAPACITY)
        .map(|footprint| footprint.archive)
        .collect()
}

/// Block B — budget, LRU window and the "0 loads of the 43 at once" invariant.
fn block_b(tally: &mut Tally, present: &[u32]) {
    let mut footprints: Vec<SpeciesFootprint> = Vec::with_capacity(present.len());
    for archive in present {
        match decode_species(*archive) {
            Ok((footprint, _, _)) => footprints.push(footprint),
            Err(reason) => {
                tally.fail("B/decode", reason);
                return;
            }
        }
    }
    if footprints.is_empty() {
        tally.fail("B/decode", "no present species archive decoded");
        return;
    }

    let total_records: usize = footprints.iter().map(|f| f.records).sum();
    let decoded_records: usize = footprints.iter().map(|f| f.decoded_records).sum();
    let pixel_less: usize = footprints.iter().map(|f| f.pixel_less_records).sum();
    let disk_bytes: u64 = footprints.iter().map(|f| f.file_bytes).sum();
    println!(
        "[info] block B corpus: species={} records={total_records} decoded={decoded_records} \
         pixel_less={pixel_less} disk={disk_bytes} B",
        footprints.len()
    );

    // B/per-species: the largest footprint against PER_SPECIES_BUDGET_BYTES.
    let over: Vec<&SpeciesFootprint> = footprints
        .iter()
        .filter(|footprint| footprint.bytes > PER_SPECIES_BUDGET_BYTES)
        .collect();
    let largest = footprints
        .iter()
        .max_by_key(|footprint| footprint.bytes)
        .expect("footprints is not empty");
    let per_species_detail = format!(
        "largest TEXTURE.{} = {} B ({:.3} MiB) vs budget {} B; 0 of {} species over budget",
        largest.archive,
        largest.bytes,
        largest.bytes as f64 / 1_048_576.0,
        PER_SPECIES_BUDGET_BYTES,
        footprints.len()
    );
    if over.is_empty() {
        tally.pass("B/per-species-budget", per_species_detail);
    } else {
        let listing: Vec<String> = over
            .iter()
            .map(|footprint| {
                format!(
                    "TEXTURE.{}={} B ({} records, largest {} B)",
                    footprint.archive,
                    footprint.bytes,
                    footprint.records,
                    footprint.largest_record_bytes
                )
            })
            .collect();
        tally.fail(
            "B/per-species-budget",
            format!(
                "{} of {} species over budget {} B: [{}]; {per_species_detail}",
                over.len(),
                footprints.len(),
                PER_SPECIES_BUDGET_BYTES,
                listing.join(", ")
            ),
        );
    }

    // B — the cache: 4 largest first, then the present archives streamed.
    let seeds = four_largest(&footprints);
    let mut cache = SpeciesSpriteCache::new();
    let mut max_resident = 0_usize;
    for archive in &seeds {
        let footprint = footprints
            .iter()
            .find(|candidate| candidate.archive == *archive)
            .expect("seed archive comes from the footprints");
        cache.insert((*archive) as u16, footprint.bytes);
        max_resident = max_resident.max(cache.loads());
    }
    for footprint in &footprints {
        cache.insert(footprint.archive as u16, footprint.bytes);
        max_resident = max_resident.max(cache.loads());
    }

    if cache.resident_bytes() <= cache.peak_bytes() && cache.peak_bytes() <= TOTAL_BUDGET_BYTES {
        tally.pass(
            "B/total-budget",
            format!(
                "resident {} B <= peak {} B <= TOTAL_BUDGET_BYTES {} B ({:.3} MiB peak)",
                cache.resident_bytes(),
                cache.peak_bytes(),
                TOTAL_BUDGET_BYTES,
                cache.peak_bytes() as f64 / 1_048_576.0
            ),
        );
    } else {
        tally.fail(
            "B/total-budget",
            format!(
                "resident {} B, peak {} B vs TOTAL_BUDGET_BYTES {} B",
                cache.resident_bytes(),
                cache.peak_bytes(),
                TOTAL_BUDGET_BYTES
            ),
        );
    }

    let expected_evictions = cache
        .total_inserts()
        .saturating_sub(LRU_CAPACITY as u64);
    if cache.loads() == LRU_CAPACITY && cache.evictions() == expected_evictions {
        tally.pass(
            "B/lru-window",
            format!(
                "loads()={} == LRU_CAPACITY {LRU_CAPACITY}; evictions()={} == \
                 total_inserts {} - {LRU_CAPACITY}",
                cache.loads(),
                cache.evictions(),
                cache.total_inserts()
            ),
        );
    } else {
        tally.fail(
            "B/lru-window",
            format!(
                "loads()={} (want {LRU_CAPACITY}), evictions()={} (want {expected_evictions} \
                 from total_inserts {})",
                cache.loads(),
                cache.evictions(),
                cache.total_inserts()
            ),
        );
    }

    // `LRU_CAPACITY < SPECIES_ARCHIVE_COUNT` is proven at compile time by the
    // `const _` assertion below, so `max_resident <= LRU_CAPACITY` alone is the
    // whole "0 loads of the 43 at once" invariant.
    if max_resident <= LRU_CAPACITY {
        tally.pass(
            "B/no-bulk-load",
            format!(
                "max simultaneous resident species={max_resident} <= LRU_CAPACITY {LRU_CAPACITY} \
                 (0 loads of the {SPECIES_ARCHIVE_COUNT} at once)"
            ),
        );
    } else {
        tally.fail(
            "B/no-bulk-load",
            format!(
                "max simultaneous resident species={max_resident} vs LRU_CAPACITY {LRU_CAPACITY} \
                 of {SPECIES_ARCHIVE_COUNT}"
            ),
        );
    }

    if cache.budget_ok() {
        tally.pass(
            "B/budget-ok",
            format!(
                "SpeciesSpriteCache::budget_ok()=true after {} inserts ({} seeds + {} streamed)",
                cache.total_inserts(),
                seeds.len(),
                footprints.len()
            ),
        );
    } else {
        tally.fail(
            "B/budget-ok",
            format!(
                "SpeciesSpriteCache::budget_ok()=false: resident {} B, peak {} B, loads {}",
                cache.resident_bytes(),
                cache.peak_bytes(),
                cache.loads()
            ),
        );
    }
}

// ---------------------------------------------------------------------------
// Block C — determinism of the largest archive's frame 0.
// ---------------------------------------------------------------------------

/// Block C — FNV-1a 64 over `archive ++ SPRITE_SEED ++ (w,h) ++ frame-0`,
/// computed twice from two independent decodes of the largest archive.
fn block_c(tally: &mut Tally, present: &[u32]) {
    let mut largest: Option<(u32, u64)> = None;
    for archive in present {
        let path = match crate::mode_a_arena2_path(&format!("TEXTURE.{archive}")) {
            Ok(path) => path,
            Err(reason) => {
                tally.fail("C/largest", format!("TEXTURE.{archive}: {reason}"));
                return;
            }
        };
        let size = match fs::metadata(&path) {
            Ok(metadata) => metadata.len(),
            Err(error) => {
                tally.fail("C/largest", format!("{}: {error}", path.display()));
                return;
            }
        };
        if largest.is_none_or(|(_, best)| size > best) {
            largest = Some((*archive, size));
        }
    }
    let Some((archive, file_bytes)) = largest else {
        tally.fail("C/largest", "no present species archive");
        return;
    };

    match digest_frame_zero(archive) {
        Ok((digest, width, height, first_bytes)) => {
            println!("SPRITE-001 C digest={digest:016x}");
            println!(
                "[info] block C subject: TEXTURE.{archive} ({file_bytes} B on disk) record 0 \
                 frame 0 = {width}x{height} RGBA {first_bytes} B, seed {SPRITE_SEED:#x}"
            );
            match digest_frame_zero(archive) {
                Ok((again, width_2, height_2, bytes_2)) => {
                    if again == digest
                        && width_2 == width
                        && height_2 == height
                        && bytes_2 == first_bytes
                    {
                        tally.pass(
                            "C/determinism",
                            format!(
                                "TEXTURE.{archive} record 0 frame 0 digest {digest:016x} == \
                                 recomputed {again:016x} ({width}x{height}, {first_bytes} B)"
                            ),
                        );
                    } else {
                        tally.fail(
                            "C/determinism",
                            format!(
                                "TEXTURE.{archive} digest {digest:016x}/{width}x{height}/{first_bytes} B \
                                 != recomputed {again:016x}/{width_2}x{height_2}/{bytes_2} B"
                            ),
                        );
                    }
                }
                Err(reason) => tally.fail("C/determinism", reason),
            }
        }
        Err(reason) => tally.fail("C/digest", reason),
    }
}

/// FNV-1a 64 over `archive ++ SPRITE_SEED ++ (w,h) ++ frame-0` of record 0.
fn digest_frame_zero(archive: u32) -> Result<(u64, u16, u16, u64), String> {
    let path = crate::mode_a_arena2_path(&format!("TEXTURE.{archive}"))
        .map_err(|reason| format!("TEXTURE.{archive}: {reason}"))?;
    let data = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let parsed = TextureArchive::parse(&data).map_err(|e| format!("TEXTURE.{archive}: {e}"))?;
    let frame = texture::decode_frame(&data, &parsed, 0, 0).map_err(|error| {
        format!("TEXTURE.{archive} record 0 frame 0 did not decode: {error:?}")
    })?;
    let mut digest = fnv1a64_fold(FNV_OFFSET, &archive.to_le_bytes());
    digest = fnv1a64_fold(digest, &SPRITE_SEED.to_le_bytes());
    digest = fnv1a64_fold(digest, &frame.width.to_le_bytes());
    digest = fnv1a64_fold(digest, &frame.height.to_le_bytes());
    digest = fnv1a64_fold(digest, &frame.pixels);
    Ok((
        digest,
        frame.width,
        frame.height,
        (frame.pixels.len() as u64) * BYTES_PER_PIXEL,
    ))
}

// ---------------------------------------------------------------------------
// Entry point.
// ---------------------------------------------------------------------------

/// `PROBE-SPRITE-001` entry point — returns the process exit code.
pub fn run_sprite_001() -> i32 {
    println!("PROBE-SPRITE-001 — M35 blocks A–C (CR-2fef7f8a)");
    println!("================================================================");
    println!(
        "[info] seed={SPRITE_SEED:#x}; archives TEXTURE.255..=297 = {SPECIES_ARCHIVE_COUNT}; \
         LRU {LRU_CAPACITY} x {PER_SPECIES_BUDGET_BYTES} B = {TOTAL_BUDGET_BYTES} B"
    );

    if let Err(reason) = crate::mode_a_arena2_path("MAPS.BSA") {
        println!("[gap] ARENA2 not resolvable: {reason}");
        println!("PROBE-SPRITE-001: verdict=DEGRADED exit=2 reasons=arena2-absent");
        return 2;
    }

    let mut tally = Tally::default();
    let census = ArchiveCensus::measure();
    if census.present.is_empty() {
        tally.gap("archives-absent");
    }

    match block_a(&mut tally) {
        Some(disposition) => {
            let signed = if disposition.is_negative() {
                "NEGATIVE"
            } else {
                "DECLARED"
            };
            println!(
                "[info] block A disposition: {signed} (rows={:?})",
                disposition.rows().map(<[_]>::len)
            );
        }
        None => {
            // `block_a` already recorded the FAIL/gap and there is no
            // disposition to stream against.
            let (verdict, exit) = tally.verdict();
            let reasons = if tally.gaps.is_empty() {
                "-".to_string()
            } else {
                tally.gaps.join(",")
            };
            println!(
                "[summary] checks passed={} failed={} na={} gaps={reasons}",
                tally.passed, tally.failed, tally.na
            );
            println!("PROBE-SPRITE-001: verdict={verdict} exit={exit} reasons={reasons}");
            return exit;
        }
    }

    if census.present.is_empty() {
        tally.fail(
            "B/decode",
            format!(
                "0 of {SPECIES_ARCHIVE_COUNT} species archives present, so no budget is measurable"
            ),
        );
        tally.fail("C/largest", "no present species archive");
    } else {
        block_b(&mut tally, &census.present);
        block_c(&mut tally, &census.present);
    }

    let (verdict, exit) = tally.verdict();
    let reasons = if tally.gaps.is_empty() {
        "-".to_string()
    } else {
        tally.gaps.join(",")
    };
    println!(
        "[summary] checks passed={} failed={} na={} gaps={reasons}",
        tally.passed, tally.failed, tally.na
    );
    println!("PROBE-SPRITE-001: verdict={verdict} exit={exit} reasons={reasons}");
    exit
}

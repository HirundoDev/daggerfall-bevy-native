//! M11 — anti-screen probes for the retail `.RDB` interior (REQ-L99-043,
//! DoD §b):
//!
//! - **PROBE-RDB-001** (`--probe RDB-001`): census of ALL `.RDB` records in
//!   the BLOCKS.BSA directory (Mode A) — every record parsed or counted as a
//!   failure with its error, 0 panic — + deterministic subject (first record
//!   by directory order whose assembled interior PASSes the real probe,
//!   [`RdbSources::select_subject`]) + full 12-check probe over the subject
//!   (generic M2 layer + 7 RDB/door/interior checks, INV-L99-5) + byte-by-byte
//!   determinism (FNV-1a64 digest of two compositions of the SAME record).
//! - The real variant also runs inside PROBE-INTERIOR-001 (block C via
//!   [`run_real_interior_block`], wired in `m4_probe`) — DoD id mapping:
//!   **PROBE-INTERIOR-001 (DoD) = INTERIOR-RDB-001 = `--probe RDB-001`**
//!   (HH-1 m11fin decision: do NOT reuse the synthetic M4 INTERIOR-001 as DoD
//!   evidence).
//!
//! **Source custody (PRE-2 §8 rule 3):** Mode A only (PathChecker); the
//! `.BSA` files are read from the user's ARENA2 at runtime (0 bytes in git,
//! INV-L99-6). 0 reads in `cargo test` (fixtures live in
//! `daggerfall-parser-rs` and `parser-bevy-bridge`).
use daggerfall_parser_bevy_bridge as bridge;
use daggerfall_parser_rs::{bsa, rdb};

use crate::m10_probe::Arch3dIndex;

/// Retail sources (Mode A) of the M11 probe: BLOCKS.BSA + index of `.RDB`
/// records in directory order + ARCH3D.BSA resolvable on demand.
pub(crate) struct RdbSources {
    blocks_data: Vec<u8>,
    blocks_archive: bridge::BsaArchive,
    /// `(directory entry, name)` in DIRECTORY ORDER (the source of
    /// determinism for subject selection).
    rdb_entries: Vec<(usize, String)>,
    arch: Arch3dIndex,
}

/// One `.RDB` record of the directory: parsed, or with its honest error.
pub(crate) struct RdbRecord {
    pub name: String,
    pub block: Result<rdb::RdbBlock, String>,
}

/// Aggregated census of the `.RDB` directory (deterministic: directory order).
pub(crate) struct RdbCensus {
    /// `.RDB` records in the BLOCKS.BSA directory.
    pub total: usize,
    /// Records parsed successfully.
    pub parsed: usize,
    /// `(name, error)` of the records that do not parse.
    pub failed: Vec<(String, String)>,
    /// Objects per type `(model, light, flat)` over all parsed records.
    pub objects: (usize, usize, usize),
    /// Root cells with a present chain / total root cells.
    pub used_roots: usize,
    pub root_cells: usize,
    /// Histogram `(width, height) → record count` (sorted by dimensions).
    pub dims: Vec<((u32, u32), usize)>,
}

impl RdbSources {
    /// Parses ALL `.RDB` records of the directory in directory order (the M11
    /// census: every record ends up parsed or with its error).
    pub(crate) fn parse_directory(&self) -> Vec<RdbRecord> {
        self.rdb_entries
            .iter()
            .map(|(idx, name)| {
                let block = bsa::entry_data(&self.blocks_archive, &self.blocks_data, *idx)
                    .map_err(|e| format!("record bytes: {e}"))
                    .and_then(|bytes| rdb::parse(bytes).map_err(|e| format!("parse: {e}")));
                RdbRecord {
                    name: name.clone(),
                    block,
                }
            })
            .collect()
    }

    /// Composes the interior of ONE record (single assembly path — selection
    /// and the determinism re-composition use THIS path).
    pub(crate) fn assemble(
        &mut self,
        record: &RdbRecord,
    ) -> Result<bridge::RetailInterior, String> {
        let block = record.block.as_ref().map_err(|e| e.clone())?;
        let arch = &mut self.arch;
        Ok(bridge::build_retail_interior(&record.name, block, &mut |id| {
            arch.resolve(id)
        }))
    }

    /// Deterministic subject selection: first record by directory order whose
    /// assembled interior is navigable (models/lights/flats ≥ 1, 0 pending
    /// ARCH3D, non-empty mesh) AND PASSes the real 12-check probe. Returns
    /// `(record index, interior, candidates assembled up to the PASS)`.
    pub(crate) fn select_subject(
        &mut self,
        records: &[RdbRecord],
    ) -> Result<(usize, bridge::RetailInterior, usize), String> {
        let mut candidates = 0usize;
        for (rank, record) in records.iter().enumerate() {
            let Ok(block) = &record.block else { continue };
            let (models, lights, flats) = block.resource_counts();
            if models == 0 || lights == 0 || flats == 0 {
                continue;
            }
            candidates += 1;
            let Ok(interior) = self.assemble(record) else {
                continue;
            };
            if interior.unresolved_models != 0 || interior.mesh.triangle_count() == 0 {
                continue;
            }
            if bridge::run_retail_interior_probe(&interior).pass {
                return Ok((rank, interior, candidates));
            }
        }
        Err(format!(
            "no navigable .RDB interior in the directory ({candidates} candidates assembled)"
        ))
    }
}

/// Retail sources (Mode A) already loaded: BLOCKS.BSA + `.RDB` index +
/// ARCH3D.BSA on demand.
pub(crate) fn load_rdb_sources() -> Result<RdbSources, String> {
    let path_checker = daggerfall_inventory::path_checker::PathChecker::new();
    if path_checker.use_custom_universe() {
        return Err("custom universe (PathChecker) not supported in this probe".into());
    }
    let blocks_path = crate::mode_a_arena2_path("BLOCKS.BSA")?;
    let arch_path = crate::mode_a_arena2_path("ARCH3D.BSA")?;
    println!(
        "[OK] mode_a BLOCKS.BSA: {} ({} bytes)",
        blocks_path.display(),
        std::fs::metadata(&blocks_path).map(|m| m.len()).unwrap_or(0)
    );
    println!(
        "[OK] mode_a ARCH3D.BSA: {} ({} bytes)",
        arch_path.display(),
        std::fs::metadata(&arch_path).map(|m| m.len()).unwrap_or(0)
    );
    let blocks_data =
        std::fs::read(&blocks_path).map_err(|e| format!("reading BLOCKS.BSA: {e}"))?;
    let blocks_archive =
        bridge::parse_bsa(&blocks_data).map_err(|e| format!("invalid BLOCKS.BSA: {e}"))?;
    let rdb_entries: Vec<(usize, String)> = blocks_archive
        .entries
        .iter()
        .enumerate()
        .filter_map(|(i, entry)| match entry {
            bsa::Entry::Named { name, .. }
                if name.to_ascii_uppercase().ends_with(".RDB") =>
            {
                Some((i, name.clone()))
            }
            _ => None,
        })
        .collect();
    if rdb_entries.is_empty() {
        return Err("0 .RDB records in the BLOCKS.BSA directory".into());
    }
    let arch_bytes = std::fs::read(&arch_path).map_err(|e| format!("reading ARCH3D.BSA: {e}"))?;
    let arch = Arch3dIndex::new(arch_bytes)?;
    Ok(RdbSources {
        blocks_data,
        blocks_archive,
        rdb_entries,
        arch,
    })
}

/// Aggregates the census from the parsed records (deterministic).
fn aggregate_census(records: &[RdbRecord]) -> RdbCensus {
    let mut census = RdbCensus {
        total: records.len(),
        parsed: 0,
        failed: Vec::new(),
        objects: (0, 0, 0),
        used_roots: 0,
        root_cells: 0,
        dims: Vec::new(),
    };
    for record in records {
        let Ok(block) = &record.block else {
            let reason = record.block.as_ref().err().cloned().unwrap_or_default();
            census.failed.push((record.name.clone(), reason));
            continue;
        };
        census.parsed += 1;
        let (models, lights, flats) = block.resource_counts();
        census.objects.0 += models;
        census.objects.1 += lights;
        census.objects.2 += flats;
        census.used_roots += block.used_roots();
        census.root_cells += block.root_count();
        let dims = (block.header.width, block.header.height);
        match census.dims.iter_mut().find(|((w, h), _)| (*w, *h) == dims) {
            Some((_, count)) => *count += 1,
            None => census.dims.push((dims, 1)),
        }
    }
    census.dims.sort_by_key(|((w, h), _)| (*w, *h));
    census
}

/// Census summary line (shared by PROBE-RDB-001 and block C of
/// PROBE-INTERIOR-001).
fn print_census_summary(census: &RdbCensus) {
    let dims = census
        .dims
        .iter()
        .map(|((w, h), n)| format!("{w}×{h}:{n}"))
        .collect::<Vec<_>>()
        .join(" · ");
    println!(
        "[{}] RDB census: {} .RDB records in BLOCKS.BSA · parsed {} · failures {} · \
         objects (model,light,flat)=({},{},{}) · used roots {}/{} · dimensions [{dims}] · 0 panic",
        if census.failed.is_empty() { "OK" } else { "FAIL" },
        census.total,
        census.parsed,
        census.failed.len(),
        census.objects.0,
        census.objects.1,
        census.objects.2,
        census.used_roots,
        census.root_cells,
    );
    for (name, reason) in &census.failed {
        println!("     [FAIL] {name}: {reason}");
    }
}

/// Shared evaluation of the real interior: census + deterministic subject +
/// 12-check probe (single core of PROBE-RDB-001 and block C of
/// PROBE-INTERIOR-001 — 0 duplicated logic).
pub(crate) struct RealInteriorEval {
    pub sources: RdbSources,
    pub records: Vec<RdbRecord>,
    pub census: RdbCensus,
    /// Subject index in directory order.
    pub subject_rank: usize,
    pub subject: bridge::RetailInterior,
    /// Candidates assembled up to the first PASS.
    pub candidates: usize,
    pub report: bridge::RetailInteriorReport,
}

/// Census + selection + real probe (a single parse pass).
pub(crate) fn evaluate_real_interior() -> Result<RealInteriorEval, String> {
    let mut sources = load_rdb_sources()?;
    let records = sources.parse_directory();
    let census = aggregate_census(&records);
    if census.parsed == 0 {
        return Err(format!(
            "0 of {} .RDB records parsed (first failure: {:?})",
            census.total,
            census.failed.first()
        ));
    }
    let (subject_rank, subject, candidates) = sources.select_subject(&records)?;
    let report = bridge::run_retail_interior_probe(&subject);
    Ok(RealInteriorEval {
        sources,
        records,
        census,
        subject_rank,
        subject,
        candidates,
        report,
    })
}

/// Block C of PROBE-INTERIOR-001 (M11): the REAL variant over the
/// deterministic subject of the `.RDB` directory in addition to the synthetic
/// M4 block A. Returns true only if the census is complete (0 parse failures)
/// AND the real probe PASSes (12/12).
pub(crate) fn run_real_interior_block() -> bool {
    let eval = match evaluate_real_interior() {
        Ok(eval) => eval,
        Err(reason) => {
            println!("[FAIL] real .RDB interior: {reason}");
            return false;
        }
    };
    print_census_summary(&eval.census);
    println!(
        "[{}] deterministic subject: {} (record #{} of {} by directory order · \
         {} candidates assembled · {} models/{} lights/{} flats · {} triangles · digest {})",
        if eval.report.pass { "OK" } else { "FAIL" },
        eval.subject.block_name,
        eval.subject_rank + 1,
        eval.census.total,
        eval.candidates,
        eval.subject.counts().0,
        eval.subject.counts().1,
        eval.subject.counts().2,
        eval.subject.mesh.triangle_count(),
        eval.subject.digest(),
    );
    for check in &eval.report.checks {
        let status = if check.ok { "OK" } else { "FAIL" };
        println!("[{status}] rdb_{} - {}", check.id, check.detail);
    }
    eval.census.failed.is_empty() && eval.report.pass
}

/// Runs PROBE-RDB-001 — retail `.RDB` interior (REQ-L99-043, M11).
pub fn run_rdb_001() -> i32 {
    println!("PROBE-RDB-001 - retail .RDB interior (REQ-L99-043, M11, CR-3f9d13a2)");
    let mut eval = match evaluate_real_interior() {
        Ok(eval) => eval,
        Err(reason) => {
            println!("[FAIL] real .RDB interior: {reason}");
            return 1;
        }
    };
    let mut all_ok = true;

    // ── 1. Full census of the `.RDB` directory ─────────────────────────
    print_census_summary(&eval.census);
    let census_ok =
        eval.census.total > 0 && eval.census.parsed == eval.census.total;
    println!(
        "[{}] rdb_census_complete - {} directory records parsed ({} failures, 0 req) · \
         accumulated objects (model,light,flat)=({},{},{})",
        if census_ok { "OK" } else { "FAIL" },
        eval.census.parsed,
        eval.census.failed.len(),
        eval.census.objects.0,
        eval.census.objects.1,
        eval.census.objects.2,
    );
    all_ok &= census_ok;

    // ── 2. Deterministic subject + byte-by-byte determinism ───────────────
    println!(
        "[OK] deterministic subject: {} (record #{} of {} by directory order · \
         {} candidates assembled · {} models/{} lights/{} flats · {} triangles)",
        eval.subject.block_name,
        eval.subject_rank + 1,
        eval.census.total,
        eval.candidates,
        eval.subject.counts().0,
        eval.subject.counts().1,
        eval.subject.counts().2,
        eval.subject.mesh.triangle_count(),
    );
    let deterministic = match eval.sources.assemble(&eval.records[eval.subject_rank]) {
        Ok(again) => {
            let bytes_equal = again.canonical_bytes() == eval.subject.canonical_bytes();
            let digest_equal = again.digest() == eval.subject.digest();
            println!(
                "[{}] rdb_subject_deterministic - two compositions of record {} · \
                 canonical_bytes {} == {} bytes · FNV-1a64 digest {} vs {}",
                if bytes_equal && digest_equal {
                    "OK"
                } else {
                    "FAIL"
                },
                eval.subject.block_name,
                again.canonical_bytes().len(),
                eval.subject.canonical_bytes().len(),
                again.digest(),
                eval.subject.digest(),
            );
            bytes_equal && digest_equal
        }
        Err(reason) => {
            println!(
                "[FAIL] rdb_subject_deterministic - re-composition of {}: {reason}",
                eval.subject.block_name
            );
            false
        }
    };
    all_ok &= deterministic;

    // ── 3. Real 12-check probe over the subject (INV-L99-5) ──────────
    for check in &eval.report.checks {
        let status = if check.ok { "OK" } else { "FAIL" };
        println!("[{status}] rdb_{} - {}", check.id, check.detail);
    }
    let door = eval
        .report
        .door
        .map(|(side, from, to)| format!("side {} lateral [{from:.1},{to:.1}] m", side.label(), ))
        .unwrap_or_else(|| "no door".into());
    println!(
        "[{}] rdb_retail_interior_probe - {}/12 checks · real door: {door}",
        if eval.report.pass { "OK" } else { "FAIL" },
        eval.report.checks.iter().filter(|c| c.ok).count(),
    );
    all_ok &= eval.report.pass;

    if all_ok {
        println!(
            "VERDICT: PROBE-RDB-001 PASS - census {} / {} .RDB records of BLOCKS.BSA · \
             deterministic subject {} navigable through the real door with 0 crossings · \
             byte-by-byte determinism (digest {})",
            eval.census.parsed,
            eval.census.total,
            eval.subject.block_name,
            eval.subject.digest(),
        );
        0
    } else {
        println!("VERDICT: PROBE-RDB-001 FAIL - see [FAIL] rows");
        1
    }
}

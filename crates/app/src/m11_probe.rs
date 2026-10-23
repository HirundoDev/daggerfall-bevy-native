//! M11 — anti-screen probes for the retail `.RDB` interior (REQ-L99-043,
//! DoD §b):
//!
//! - **PROBE-RDB-001** (`--probe RDB-001`): census of ALL `.RDB` records in
//!   the BLOCKS.BSA directory (Mode A) — every record parsed or counted as a
//!   failure with its error, 0 panic — + census of the 187 companion `.RDI`
//!   records (`rdi_census_complete`, [`rdi`]) + deterministic subject (first
//!   record by directory order whose assembled interior PASSes the real
//!   probe, [`RdbSources::select_subject`]) + full 12-check probe over the
//!   subject (generic M2 layer + 7 RDB/door/interior checks, INV-L99-5) +
//!   byte-by-byte determinism (FNV-1a64 digest of two compositions of the
//!   SAME record) + the published search magnitude (`rdb_assembly_coverage`:
//!   how many of the 187 records were assembled vs how many PASSed, both
//!   taken from the real counters — never from a literal).
//!   The subject search is **bounded**: at most 96 assembled candidates, a
//!   90 s wall-clock stop (`[FAIL] search budget exceeded …`) and
//!   per-candidate work caps — every
//!   `[SKIP]`/`[OK]` line names the record and the reason, so an honest FAIL
//!   is one diagnosable verdict in bounded time instead of a hang.
//!   In the retail installation the search assembles **71** of the 187
//!   `.RDB` records and exactly **1** passes the 12 checks (subject
//!   `N0000055.RDB`); the magnitude is measured, not asserted.
//! - **`.RDI` consumption:** each `.RDB` candidate must have a companion
//!   `.RDI` that validates (fixed 512 B + flags ∈ {0,1} + the 1:1 base-name
//!   pairing) BEFORE it is assembled — a candidate without one is rejected
//!   with `[SKIP] candidate N …: no valid companion .RDI (…)` and counted in
//!   `rdi_rejected`. The flag count of the companion is printed on the
//!   selection lines, so the record is consumed by the selection route
//!   instead of being loaded and forgotten. The flag *semantics* stay
//!   undocumented (see [`rdi`]); nothing is inferred from them.
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
use std::collections::{BTreeMap, BTreeSet};

use daggerfall_parser_bevy_bridge as bridge;
use daggerfall_parser_rs::{bsa, rdb, rdi};

use crate::m10_probe::Arch3dIndex;

/// Retail sources (Mode A) of the M11 probe: BLOCKS.BSA + index of `.RDB`
/// records in directory order + the parsed `.RDI` companions + ARCH3D.BSA
/// resolvable on demand.
pub(crate) struct RdbSources {
    blocks_data: Vec<u8>,
    blocks_archive: bridge::BsaArchive,
    /// `(directory entry, name)` in DIRECTORY ORDER (the source of
    /// determinism for subject selection).
    rdb_entries: Vec<(usize, String)>,
    /// `(directory entry, name)` of every `.RDI` in directory order (the raw
    /// census source; the parsed values live in [`Self::rdi`]).
    rdi_entries: Vec<(usize, String)>,
    /// Parsed `.RDI` records keyed by base name in UPPERCASE (`N0000055`),
    /// with the parser reason preserved for the invalid ones.
    rdi: BTreeMap<String, Result<rdi::RdiFlags, String>>,
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

/// Aggregated census of the `.RDI` directory (deterministic: directory order
/// for failures, name order for the digest).
pub(crate) struct RdiCensus {
    /// `.RDI` records in the BLOCKS.BSA directory.
    pub total: usize,
    /// Records that satisfy the fixed length + the Boolean invariant.
    pub parsed: usize,
    /// `(directory index, name, parser reason)` of the invalid records.
    pub failed: Vec<(usize, String, String)>,
    /// `.RDB` base names without a companion `.RDI` (sorted).
    pub rdb_without_rdi: Vec<String>,
    /// `.RDI` base names without a companion `.RDB` (sorted).
    pub rdi_without_rdb: Vec<String>,
    /// Flags set (`!= 0`) over every parsed record.
    pub active_flags: usize,
    /// All-zero records over every parsed record.
    pub empty_records: usize,
    /// FNV-1a64 folded over the raw bytes of every parsed record in name
    /// order (0 allocations).
    pub digest: u64,
}

/// Real counters of one subject search (published as `rdb_assembly_coverage`
/// — the magnitude is what was measured, never a literal).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SearchStats {
    /// Candidates that entered assembly (companion `.RDI` validated).
    pub attempted: usize,
    /// `assemble` calls that returned an interior.
    pub assembled: usize,
    /// `assemble` calls that returned an error.
    pub assemble_errors: usize,
    /// Records with no model, no light or no flat (skipped before assembly).
    pub without_objects: usize,
    /// Assembled interiors with pending ARCH3D models or an empty mesh.
    pub not_navigable: usize,
    /// Interiors rejected by the per-candidate work caps.
    pub over_work_cap: usize,
    /// Candidates that assembled but failed the 12-check probe.
    pub probe_failed: usize,
    /// Candidates rejected because their companion `.RDI` is missing or
    /// invalid.
    pub rdi_rejected: usize,
    /// Candidates that passed the 12-check probe.
    pub passed: usize,
}

/// Result of [`RdbSources::select_subject`]: the subject, its directory rank
/// and the counters of the walk that found it.
pub(crate) struct SubjectSearch {
    /// Subject index in directory order.
    pub rank: usize,
    pub interior: bridge::RetailInterior,
    pub stats: SearchStats,
}

/// Deterministic cap of the subject search: maximum candidates (records with
/// models/lights/flats ≥ 1) assembled and probed. The retail directory of
/// BLOCKS.BSA PASSes at the 71st assembled candidate of 187 records (measured
/// — see `rdb_assembly_coverage`: 71 assembled, 1 PASS), so 96 keeps a 25%
/// margin and still bounds the walk if the installation changes.
pub(crate) const SEARCH_MAX_CANDIDATES: usize = 96;

/// Wall-clock ceiling of the subject search. It only STOPS the search with an
/// explicit `[FAIL] search budget exceeded` line — it never changes a PASS
/// criterion: any interior that PASSes the 12 checks inside the budget is a
/// real PASS (the retail installation needs ~18 s for 71 candidates).
const SEARCH_BUDGET: std::time::Duration = std::time::Duration::from_secs(90);

/// Deterministic work caps of ONE candidate probe (rejected with an explicit
/// reason before probing). The retail subject tops out at 3,173 triangles
/// inside a 71×48×71 m box (≈0.25 M grid cells); the caps are 2–3 orders of
/// magnitude above that and exist so a malformed record cannot turn a single
/// probe into unbounded grid / march work.
const MAX_CANDIDATE_TRIANGLES: usize = 100_000;
const MAX_CANDIDATE_SPAN_M: f32 = 1024.0;
const MAX_CANDIDATE_CELLS: f32 = 16_000_000.0;

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
    /// companion `.RDI` validates AND whose assembled interior is navigable
    /// (models/lights/flats ≥ 1, 0 pending ARCH3D, non-empty mesh, inside the
    /// per-candidate work caps) AND PASSes the real 12-check probe.
    ///
    /// The companion `.RDI` gate runs BEFORE the record is assembled: a
    /// missing or invalid companion is an explicit `[SKIP]` with its reason
    /// and is counted in [`SearchStats::rdi_rejected`] (0 in the retail
    /// installation — the 187 `.RDB` all have a valid `.RDI`), so the gate is
    /// exercised on every candidate instead of being declared.
    ///
    /// **Bounded search (P0 hang guard):** the walk stops after
    /// `SEARCH_MAX_CANDIDATES` assembled candidates or `SEARCH_BUDGET`
    /// (whichever comes first) and returns an explicit FAIL. Every assembled
    /// candidate prints one `[SKIP]` / `[OK]` line with its reason, so a FAIL
    /// is diagnosable record by record. The returned [`SearchStats`] are the
    /// MEASURED counters of the walk (they feed `rdb_assembly_coverage`).
    pub(crate) fn select_subject(
        &mut self,
        records: &[RdbRecord],
    ) -> Result<SubjectSearch, String> {
        let started = std::time::Instant::now();
        let mut stats = SearchStats::default();
        for (rank, record) in records.iter().enumerate() {
            let Ok(block) = &record.block else { continue };
            let (models, lights, flats) = block.resource_counts();
            if models == 0 || lights == 0 || flats == 0 {
                stats.without_objects += 1;
                continue;
            }
            if stats.attempted >= SEARCH_MAX_CANDIDATES {
                return Err(format!(
                    "search candidate cap reached ({SEARCH_MAX_CANDIDATES} assembled, no PASS: \
                     {} failed the 12-check probe · {} not navigable · {} over the work caps · \
                     {} assembly errors · {} records without model/light/flat) — per-candidate \
                     reasons above",
                    stats.probe_failed,
                    stats.not_navigable,
                    stats.over_work_cap,
                    stats.assemble_errors,
                    stats.without_objects,
                ));
            }
            if started.elapsed() >= SEARCH_BUDGET {
                return Err(format!(
                    "search budget exceeded after {} candidates ({:.1} s > {} s, no PASS: {} \
                     failed the 12-check probe · {} not navigable · {} over the work caps · {} \
                     assembly errors) — per-candidate reasons above",
                    stats.attempted,
                    started.elapsed().as_secs_f64(),
                    SEARCH_BUDGET.as_secs(),
                    stats.probe_failed,
                    stats.not_navigable,
                    stats.over_work_cap,
                    stats.assemble_errors,
                ));
            }
            // `.RDI` gate: the companion must exist and pass the parser
            // (512 B + Boolean invariant) before any assembly work.
            let ordinal = stats.attempted + stats.rdi_rejected + 1;
            let rdi_active = match self.rdi.get(&base_key(&record.name)) {
                Some(Ok(flags)) => flags.active_flags(),
                Some(Err(reason)) => {
                    stats.rdi_rejected += 1;
                    println!(
                        "[SKIP] candidate {ordinal} {}: no valid companion .RDI ({reason})",
                        record.name
                    );
                    continue;
                }
                None => {
                    stats.rdi_rejected += 1;
                    println!(
                        "[SKIP] candidate {ordinal} {}: no valid companion .RDI (missing .RDI \
                         record)",
                        record.name
                    );
                    continue;
                }
            };
            stats.attempted += 1;
            let interior = match self.assemble(record) {
                Ok(interior) => {
                    stats.assembled += 1;
                    interior
                }
                Err(reason) => {
                    stats.assemble_errors += 1;
                    println!(
                        "[SKIP] candidate {} {}: assembly error: {reason}",
                        stats.attempted, record.name
                    );
                    continue;
                }
            };
            if interior.unresolved_models != 0 || interior.mesh.triangle_count() == 0 {
                stats.not_navigable += 1;
                println!(
                    "[SKIP] candidate {} {}: not navigable ({} models · {} pending ARCH3D · {} \
                     triangles)",
                    stats.attempted,
                    record.name,
                    interior.models.len(),
                    interior.unresolved_models,
                    interior.mesh.triangle_count()
                );
                continue;
            }
            if let Some(reason) = work_cap_violation(&interior) {
                stats.over_work_cap += 1;
                println!(
                    "[SKIP] candidate {} {}: {reason}",
                    stats.attempted, record.name
                );
                continue;
            }
            let report = bridge::run_retail_interior_probe(&interior);
            if report.pass {
                stats.passed += 1;
                println!(
                    "[OK] candidate {} {}: 12/12 checks · {} models/{} lights/{} flats · {} \
                     triangles · .RDI {rdi_active} active flags",
                    stats.attempted,
                    record.name,
                    interior.counts().0,
                    interior.counts().1,
                    interior.counts().2,
                    interior.mesh.triangle_count(),
                );
                return Ok(SubjectSearch {
                    rank,
                    interior,
                    stats,
                });
            }
            stats.probe_failed += 1;
            let failed = report
                .checks
                .iter()
                .filter(|check| !check.ok)
                .map(|check| check.id)
                .collect::<Vec<_>>()
                .join(", ");
            println!(
                "[SKIP] candidate {} {}: {}/12 checks · failed [{failed}] · .RDI {rdi_active} \
                 active flags",
                stats.attempted,
                record.name,
                report.checks.iter().filter(|check| check.ok).count(),
            );
        }
        Err(format!(
            "no navigable .RDB interior in the directory ({} candidates assembled · {} failed \
             the 12-check probe · {} not navigable · {} over the work caps · {} assembly errors · \
             {} records without model/light/flat · {} rejected by the companion .RDI gate)",
            stats.assembled,
            stats.probe_failed,
            stats.not_navigable,
            stats.over_work_cap,
            stats.assemble_errors,
            stats.without_objects,
            stats.rdi_rejected,
        ))
    }

    /// Active flags of the companion `.RDI` of `name` (subject lines) —
    /// `None` when it is missing or invalid.
    pub(crate) fn rdi_companion_active(&self, name: &str) -> Option<usize> {
        self.rdi
            .get(&base_key(name))
            .and_then(|parsed| parsed.as_ref().ok())
            .map(rdi::RdiFlags::active_flags)
    }
}

/// Deterministic work caps of ONE candidate probe
/// ([`MAX_CANDIDATE_TRIANGLES`], [`MAX_CANDIDATE_SPAN_M`],
/// [`MAX_CANDIDATE_CELLS`]): `Some(reason)` when the assembled interior is too
/// large to probe with bounded work.
fn work_cap_violation(interior: &bridge::RetailInterior) -> Option<String> {
    let triangles = interior.mesh.triangle_count();
    if triangles > MAX_CANDIDATE_TRIANGLES {
        return Some(format!(
            "work cap: {triangles} triangles > {MAX_CANDIDATE_TRIANGLES}"
        ));
    }
    let (min, max) = interior.mesh_bounds()?;
    let span = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
    if let Some(axis) = (0..3).find(|axis| span[*axis] > MAX_CANDIDATE_SPAN_M) {
        return Some(format!(
            "work cap: span {:.1} m on axis {axis} > {MAX_CANDIDATE_SPAN_M} m",
            span[axis]
        ));
    }
    let cells = span[0] * span[1] * span[2];
    if cells > MAX_CANDIDATE_CELLS {
        return Some(format!(
            "work cap: footprint {cells:.0} m³ > {MAX_CANDIDATE_CELLS} grid cells (1 m)"
        ));
    }
    None
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
    // `.RDI` companions: same directory-order pattern as `.RDB`, each record
    // validated by the parser (512 B + Boolean invariant) with the reason
    // preserved for the invalid ones.
    let rdi_entries: Vec<(usize, String)> = blocks_archive
        .entries
        .iter()
        .enumerate()
        .filter_map(|(i, entry)| match entry {
            bsa::Entry::Named { name, .. } if name.to_ascii_uppercase().ends_with(".RDI") => {
                Some((i, name.clone()))
            }
            _ => None,
        })
        .collect();
    let mut rdi: BTreeMap<String, Result<rdi::RdiFlags, String>> = BTreeMap::new();
    for (index, name) in &rdi_entries {
        let parsed = bsa::entry_data(&blocks_archive, &blocks_data, *index)
            .map_err(|e| format!("record bytes: {e}"))
            .and_then(|bytes| rdi::RdiFlags::parse(bytes).map_err(|e| e.to_string()));
        rdi.insert(base_key(name), parsed);
    }
    let arch_bytes = std::fs::read(&arch_path).map_err(|e| format!("reading ARCH3D.BSA: {e}"))?;
    let arch = Arch3dIndex::new(arch_bytes)?;
    Ok(RdbSources {
        blocks_data,
        blocks_archive,
        rdb_entries,
        rdi_entries,
        rdi,
        arch,
    })
}

/// Base key of a BLOCKS.BSA record name: the stem (before the last `.`) in
/// UPPERCASE. This is the pairing key between `.RDB` and `.RDI`
/// (`N0000055.RDB` and `n0000055.rdi` → `N0000055`).
fn base_key(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, _)) => stem.to_ascii_uppercase(),
        None => name.to_ascii_uppercase(),
    }
}

/// Orphans of the `.RDB` ↔ `.RDI` base-name pairing, in both directions and
/// ordered (pure: no source access, so it is testable without retail bytes).
fn pair_orphans(rdb_names: &[String], rdi_names: &[String]) -> (Vec<String>, Vec<String>) {
    let rdb: BTreeSet<String> = rdb_names.iter().map(|name| base_key(name)).collect();
    let rdi: BTreeSet<String> = rdi_names.iter().map(|name| base_key(name)).collect();
    let rdb_without_rdi = rdb
        .difference(&rdi)
        .map(|key| format!("{key}.RDB"))
        .collect();
    let rdi_without_rdb = rdi
        .difference(&rdb)
        .map(|key| format!("{key}.RDI"))
        .collect();
    (rdb_without_rdi, rdi_without_rdb)
}

/// `rdi_census_complete` PASS criterion: every `.RDI` of the directory parsed,
/// no failure, no orphan in either direction, and a non-empty directory.
fn rdi_census_complete(census: &RdiCensus) -> bool {
    census.total > 0
        && census.total == census.parsed
        && census.failed.is_empty()
        && census.rdb_without_rdi.is_empty()
        && census.rdi_without_rdb.is_empty()
}

/// Aggregates the `.RDI` census (deterministic: directory order for failures,
/// name order for the folded digest — 0 allocation beyond the report vectors).
fn aggregate_rdi_census(
    rdi_entries: &[(usize, String)],
    rdi: &BTreeMap<String, Result<rdi::RdiFlags, String>>,
    rdb_names: &[String],
) -> RdiCensus {
    let mut census = RdiCensus {
        total: rdi_entries.len(),
        parsed: 0,
        failed: Vec::new(),
        rdb_without_rdi: Vec::new(),
        rdi_without_rdb: Vec::new(),
        active_flags: 0,
        empty_records: 0,
        digest: rdi::FNV1A64_OFFSET_BASIS,
    };
    for (index, name) in rdi_entries {
        match rdi.get(&base_key(name)) {
            Some(Ok(flags)) => {
                census.parsed += 1;
                census.active_flags += flags.active_flags();
                census.empty_records += usize::from(flags.is_empty());
            }
            Some(Err(reason)) => census.failed.push((*index, name.clone(), reason.clone())),
            None => census
                .failed
                .push((*index, name.clone(), "record not parsed".to_string())),
        }
    }
    for flags in rdi.values().filter_map(|parsed| parsed.as_ref().ok()) {
        census.digest = rdi::fnv1a64_update(census.digest, flags.as_bytes());
    }
    let rdi_names: Vec<String> = rdi_entries.iter().map(|(_, name)| name.clone()).collect();
    let (rdb_without_rdi, rdi_without_rdb) = pair_orphans(rdb_names, &rdi_names);
    census.rdb_without_rdi = rdb_without_rdi;
    census.rdi_without_rdb = rdi_without_rdb;
    census
}

/// Prints `rdi_census_complete` plus one `[FAIL]` row per invalid record and
/// per orphan. Returns the PASS verdict (never SKIP: an invalid `.RDI` is a
/// FAIL with its index and reason).
fn print_rdi_census(census: &RdiCensus, rdb_total: usize) -> bool {
    let ok = rdi_census_complete(census);
    let paired = rdb_total.saturating_sub(census.rdb_without_rdi.len());
    println!(
        "[{}] rdi_census_complete - {} .RDI records parsed ({} failures) · {} B fixed · flags ∈ \
         {{0,1}} · pairs {paired}/{rdb_total} ({} .RDB without .RDI · {} .RDI without .RDB) · {} \
         active flags · {} empty · digest {:016x}",
        if ok { "OK" } else { "FAIL" },
        census.parsed,
        census.failed.len(),
        rdi::RDI_RECORD_LEN,
        census.rdb_without_rdi.len(),
        census.rdi_without_rdb.len(),
        census.active_flags,
        census.empty_records,
        census.digest,
    );
    for (index, name, reason) in &census.failed {
        println!("[FAIL] .RDI #{index} {name}: {reason}");
    }
    for name in census.rdb_without_rdi.iter().chain(&census.rdi_without_rdb) {
        println!("[FAIL] unpaired {name}");
    }
    ok
}

/// `rdb_assembly_coverage` PASS criterion (pure: no source access, so the
/// reconciliation itself is testable without retail bytes).
///
/// Reconciliation: every attempted candidate is either an assembly error or
/// lands in exactly one of the four buckets printed on that line — so the
/// attempted count is fully accounted for (no hidden remainder). With
/// `assemble_errors == 0` both equalities collapse to
/// `attempted == passed + failed + not navigable + over caps`.
fn coverage_reconciles(stats: &SearchStats) -> bool {
    stats.attempted == stats.assembled + stats.assemble_errors
        && stats.assembled
            == stats.passed + stats.probe_failed + stats.not_navigable + stats.over_work_cap
        && stats.attempted >= 2
        && stats.passed == 1
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
    pub rdi_census: RdiCensus,
    /// Subject index in directory order.
    pub subject_rank: usize,
    pub subject: bridge::RetailInterior,
    /// Real counters of the subject search (magnitude evidence).
    pub stats: SearchStats,
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
    let rdb_names: Vec<String> = records.iter().map(|record| record.name.clone()).collect();
    let rdi_census = aggregate_rdi_census(&sources.rdi_entries, &sources.rdi, &rdb_names);
    let search = sources.select_subject(&records)?;
    let report = bridge::run_retail_interior_probe(&search.interior);
    let SubjectSearch {
        rank,
        interior,
        stats,
    } = search;
    Ok(RealInteriorEval {
        sources,
        records,
        census,
        rdi_census,
        subject_rank: rank,
        subject: interior,
        stats,
        report,
    })
}

/// Block C of PROBE-INTERIOR-001 (M11): the REAL variant over the
/// deterministic subject of the `.RDB` directory in addition to the synthetic
/// M4 block A. Returns true only if the census is complete (0 parse failures),
/// the `.RDI` census is complete (0 failures, 0 orphans) AND the real probe
/// PASSes (12/12).
pub(crate) fn run_real_interior_block() -> bool {
    let eval = match evaluate_real_interior() {
        Ok(eval) => eval,
        Err(reason) => {
            println!("[FAIL] real .RDB interior: {reason}");
            return false;
        }
    };
    print_census_summary(&eval.census);
    let rdi_ok = print_rdi_census(&eval.rdi_census, eval.census.total);
    let subject_rdi = eval
        .sources
        .rdi_companion_active(&eval.subject.block_name)
        .map_or_else(|| "missing".to_string(), |active| active.to_string());
    println!(
        "[{}] deterministic subject: {} (record #{} of {} by directory order · \
         {} candidates assembled · {} models/{} lights/{} flats · {} triangles · .RDI {} active \
         flags · digest {})",
        if eval.report.pass { "OK" } else { "FAIL" },
        eval.subject.block_name,
        eval.subject_rank + 1,
        eval.census.total,
        eval.stats.attempted,
        eval.subject.counts().0,
        eval.subject.counts().1,
        eval.subject.counts().2,
        eval.subject.mesh.triangle_count(),
        subject_rdi,
        eval.subject.digest(),
    );
    for check in &eval.report.checks {
        let status = if check.ok { "OK" } else { "FAIL" };
        println!("[{status}] rdb_{} - {}", check.id, check.detail);
    }
    eval.census.failed.is_empty() && rdi_ok && eval.report.pass
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

    // ── 2. Census of the companion `.RDI` directory ───────────────────────
    let rdi_ok = print_rdi_census(&eval.rdi_census, eval.census.total);
    all_ok &= rdi_ok;

    // ── 3. Published magnitude of the subject search (measured counters) ──
    let stats = eval.stats;
    let coverage_ok = coverage_reconciles(&stats);
    let percent = 100.0 * stats.assembled as f64 / eval.census.total.max(1) as f64;
    println!(
        "[{}] rdb_assembly_coverage - {}/{} .RDB records assembled ({percent:.1}% · {} attempted \
         · {} assembly errors) · not navigable {} · over caps {} · 12-check FAILED {} · RDI \
         rejected {} · without objects {} · 12-check PASS {}/{} (subject #{} {})",
        if coverage_ok { "OK" } else { "FAIL" },
        stats.assembled,
        eval.census.total,
        stats.attempted,
        stats.assemble_errors,
        stats.not_navigable,
        stats.over_work_cap,
        stats.probe_failed,
        stats.rdi_rejected,
        stats.without_objects,
        stats.passed,
        eval.census.total,
        eval.subject_rank + 1,
        eval.subject.block_name,
    );
    all_ok &= coverage_ok;

    // ── 4. Deterministic subject + byte-by-byte determinism ───────────────
    let subject_rdi = eval
        .sources
        .rdi_companion_active(&eval.subject.block_name)
        .map_or_else(|| "missing".to_string(), |active| active.to_string());
    println!(
        "[OK] deterministic subject: {} (record #{} of {} by directory order · \
         {} candidates assembled · {} models/{} lights/{} flats · {} triangles · .RDI {} active \
         flags)",
        eval.subject.block_name,
        eval.subject_rank + 1,
        eval.census.total,
        stats.attempted,
        eval.subject.counts().0,
        eval.subject.counts().1,
        eval.subject.counts().2,
        eval.subject.mesh.triangle_count(),
        subject_rdi,
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

    // ── 5. Real 12-check probe over the subject (INV-L99-5) ──────────
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

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| (*name).to_string()).collect()
    }

    #[test]
    fn pair_orphans_reports_nothing_on_a_perfect_pairing() {
        let rdb = names(&["N0000055.RDB", "n0000060.rdb"]);
        let rdi = names(&["N0000055.RDI", "n0000060.rdi"]);
        assert_eq!(pair_orphans(&rdb, &rdi), (Vec::new(), Vec::new()));
    }

    #[test]
    fn pair_orphans_reports_rdb_without_rdi() {
        let rdb = names(&["N0000055.RDB", "N0000060.RDB"]);
        let rdi = names(&["N0000055.RDI"]);
        assert_eq!(
            pair_orphans(&rdb, &rdi),
            (
                vec!["N0000060.RDB".to_string()],
                Vec::<String>::new()
            )
        );
    }

    #[test]
    fn pair_orphans_reports_rdi_without_rdb() {
        let rdb = names(&["N0000055.RDB"]);
        let rdi = names(&["N0000055.RDI", "n0000060.rdi"]);
        assert_eq!(
            pair_orphans(&rdb, &rdi),
            (
                Vec::<String>::new(),
                vec!["N0000060.RDI".to_string()]
            )
        );
    }

    /// Retail shape of the subject search (`rdb001.log`): 71 of the 187
    /// `.RDB` records assembled, exactly 1 passed the 12 checks.
    fn retail_stats() -> SearchStats {
        SearchStats {
            attempted: 71,
            assembled: 71,
            passed: 1,
            probe_failed: 70,
            ..SearchStats::default()
        }
    }

    /// `.RDI` census with the given coverage counts and no failure/orphan.
    fn synthetic_rdi_census(total: usize, parsed: usize) -> RdiCensus {
        RdiCensus {
            total,
            parsed,
            failed: Vec::new(),
            rdb_without_rdi: Vec::new(),
            rdi_without_rdb: Vec::new(),
            active_flags: 0,
            empty_records: 0,
            digest: rdi::FNV1A64_OFFSET_BASIS,
        }
    }

    /// Synthetic 512-byte record whose flags at `offsets` are set.
    fn synthetic_record(offsets: &[usize]) -> rdi::RdiFlags {
        let mut bytes = [0u8; rdi::RDI_RECORD_LEN];
        for offset in offsets {
            bytes[*offset] = 1;
        }
        rdi::RdiFlags::parse(&bytes)
            .unwrap_or_else(|err| panic!("synthetic record rejected by the parser: {err}"))
    }

    #[test]
    fn coverage_reconciliation_accepts_the_retail_shape() {
        let stats = retail_stats();
        assert!(
            coverage_reconciles(&stats),
            "the measured retail shape (attempted 71 = assembled 71, assembled 71 = 1 passed \
             + 70 failed) must reconcile, got {stats:?}"
        );
    }

    #[test]
    fn coverage_reconciliation_rejects_a_bucket_leak() {
        let leaked = SearchStats {
            probe_failed: 69,
            not_navigable: 0,
            ..retail_stats()
        };
        assert!(
            !coverage_reconciles(&leaked),
            "a failed candidate that escaped every bucket (1 + 69 + 0 + 0 = 70, not 71) must \
             FAIL the line, got {leaked:?}"
        );
        let double_counted = SearchStats {
            assemble_errors: 1,
            ..retail_stats()
        };
        assert!(
            !coverage_reconciles(&double_counted),
            "an attempt counted both as assembled and as an assembly error (71 != 71 + 1) must \
             FAIL the line, got {double_counted:?}"
        );
    }

    #[test]
    fn coverage_reconciliation_rejects_attempted_mismatch() {
        let mismatch = SearchStats {
            attempted: 72,
            ..retail_stats()
        };
        assert!(
            !coverage_reconciles(&mismatch),
            "72 attempted != 71 assembled + 0 assembly errors must FAIL the line, got {mismatch:?}"
        );
    }

    #[test]
    fn rdi_census_complete_rejects_an_empty_directory() {
        let census = synthetic_rdi_census(0, 0);
        assert!(
            !rdi_census_complete(&census),
            "an empty BLOCKS.BSA directory must FAIL instead of passing vacuously"
        );
    }

    #[test]
    fn rdi_census_complete_accepts_a_full_pairing() {
        let retail = synthetic_rdi_census(187, 187);
        assert!(
            rdi_census_complete(&retail),
            "187/187 parsed with no orphan in either direction must PASS"
        );
        let minimal = synthetic_rdi_census(1, 1);
        assert!(
            rdi_census_complete(&minimal),
            "1/1 parsed with no orphan in either direction must PASS"
        );
    }

    #[test]
    fn rdi_census_complete_rejects_an_unpaired_rdi() {
        let mut census = synthetic_rdi_census(1, 1);
        census.rdi_without_rdb = vec!["N0000060.RDI".to_string()];
        assert!(
            !rdi_census_complete(&census),
            "a `.RDI` without its `.RDB` must FAIL even when every record parsed"
        );
    }

    #[test]
    fn aggregate_rdi_census_propagates_an_invalid_record_with_its_index() {
        const REASON: &str = "RDI record of 511 bytes != the fixed 512 bytes";
        let entries = vec![(0, "A.RDI".to_string()), (1, "B.RDI".to_string())];
        let valid = synthetic_record(&[7, 511]);
        let valid_digest = rdi::fnv1a64(valid.as_bytes());
        let mut parsed = BTreeMap::new();
        parsed.insert("A".to_string(), Ok(valid));
        parsed.insert("B".to_string(), Err(REASON.to_string()));
        let census = aggregate_rdi_census(&entries, &parsed, &names(&["A.RDB", "B.RDB"]));

        assert_eq!(census.total, 2, "both directory records must be censused");
        assert_eq!(census.parsed, 1, "only the valid record is parsed");
        assert_eq!(
            census.failed,
            vec![(1, "B.RDI".to_string(), REASON.to_string())],
            "the invalid record must be reported with its directory index and parser reason"
        );
        assert_eq!(
            census.active_flags, 2,
            "both set flags of A must be counted"
        );
        assert_eq!(census.empty_records, 0, "A is not an all-zero record");
        assert_eq!(
            census.digest, valid_digest,
            "the digest must fold only the parsed records"
        );
        assert!(
            !print_rdi_census(&census, 2),
            "S3: the verdict chain (aggregate -> print) must return FAIL for an invalid record"
        );
    }

    #[test]
    fn aggregate_rdi_census_digest_changes_with_one_flag() {
        let base = synthetic_record(&[7]);
        let base_digest = rdi::fnv1a64(base.as_bytes());
        let flipped = synthetic_record(&[7, 511]);
        let flipped_digest = rdi::fnv1a64(flipped.as_bytes());
        let entries = vec![(0, "A.RDI".to_string())];
        let rdb_names = names(&["A.RDB"]);
        let mut left = BTreeMap::new();
        left.insert("A".to_string(), Ok(base));
        let mut right = BTreeMap::new();
        right.insert("A".to_string(), Ok(flipped));
        let left_census = aggregate_rdi_census(&entries, &left, &rdb_names);
        let right_census = aggregate_rdi_census(&entries, &right, &rdb_names);

        assert_eq!(left_census.active_flags, 1);
        assert_eq!(right_census.active_flags, 2);
        assert_eq!(left_census.digest, base_digest);
        assert_eq!(right_census.digest, flipped_digest);
        assert_ne!(
            left_census.digest, right_census.digest,
            "one flipped flag must change the folded digest (the digest is not a constant)"
        );
    }
}

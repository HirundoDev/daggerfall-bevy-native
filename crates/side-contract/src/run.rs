//! The five evaluators (DESIGN-v1 §10, F6-B pack §8): every mode ends as
//! exactly one canonical `side-contract-verdict` row on stdout plus a
//! `phase:` trace on stderr, with the exit code of §10.5.
//!
//! Cut points follow the precedence of §10.3: within `verify`, the manifest
//! (P1) stops everything, the retail scan (P10) stops before the per-file walk
//! so an injected retail file reports `bundle_retail_violation`, and only then
//! are P2/P1 cross-check findings collected. In `check`/`sync`, P1/P2 cut
//! before the lock is even read. `internal_error` aborts at its origin and
//! never combines.

use std::path::PathBuf;

use serde_json::json;

use crate::bundle::{self, GenErrKind, VerifyReport};
use crate::fsutil::io_detail;
use crate::lockfile::{self, Lock, LockOutcome};
use crate::semver::{classify, SemVer};
use crate::sideio::{self, DiffEntry};
use crate::taxonomy::Cause;
use crate::verdict::{FilesCounts, Obs, Row};
use crate::Stop;

/// Parsed and mode-validated arguments of one evaluator.
pub struct EvalArgs {
    pub command: &'static str,
    // generate
    pub contract_root: PathBuf,
    pub out: PathBuf,
    pub main_root: PathBuf,
    pub verify_determinism: bool,
    // verify / check / sync / diff
    pub bundle: PathBuf,
    // check / sync / diff
    pub side: PathBuf,
    // check / sync / diff
    pub json: bool,
    // check / sync
    pub allow_downgrade: bool,
    // sync
    pub dry_run: bool,
    pub force: bool,
}

/// What one evaluator produced: exit code, its single row, and the two streams.
pub struct Outcome {
    pub exit: u8,
    pub row: Option<Row>,
    pub trace: Vec<String>,
    pub extra: Vec<String>,
}

fn finish(row: Row, trace: Vec<String>, extra: Vec<String>) -> Outcome {
    let exit = row.exit_code() as u8;
    Outcome {
        exit,
        row: Some(row),
        trace,
        extra,
    }
}

fn ok_row(command: &'static str, detail: String) -> Row {
    let mut row = Row::from_obs(command, Vec::new());
    row.detail = detail;
    row
}

fn gen_detail(kind: GenErrKind) -> String {
    match kind {
        GenErrKind::Internal(detail) => detail,
        GenErrKind::Failed(obs) => obs.detail,
    }
}

/// Fills the bundle-side fields of a row from a verified report.
fn apply_bundle(row: &mut Row, report: &VerifyReport) {
    row.contract_version = report.contract_version.clone();
    row.bundle_sha256 = report.bundle_sha256.clone();
}

// ---------------------------------------------------------------------------
// --generate
// ---------------------------------------------------------------------------

fn cmd_generate(args: &EvalArgs) -> Outcome {
    let mut trace = Vec::new();
    let built = match bundle::build_bundle(&args.contract_root, &args.main_root, &mut trace) {
        Ok(built) => built,
        Err(err) => {
            let mut row = match err.kind {
                GenErrKind::Internal(detail) => Row::internal("generate", detail),
                GenErrKind::Failed(obs) => Row::from_obs("generate", vec![obs]),
            };
            row.contract_version = err.contract_version;
            return finish(row, trace, Vec::new());
        }
    };

    if let Err(e) = bundle::write_bundle(&args.out, &built) {
        let mut row = Row::internal("generate", io_detail(&e));
        row.contract_version = Some(built.contract_version);
        return finish(row, trace, Vec::new());
    }
    trace.push(format!(
        "phase: bundle escrito ({} files)",
        built.files.len()
    ));

    if args.verify_determinism {
        let out2 = PathBuf::from(format!("{}.verify-2", args.out.display()));
        let second =
            match bundle::build_bundle(&args.contract_root, &args.main_root, &mut Vec::new()) {
                Err(err) => Err(err),
                Ok(second) => match bundle::write_bundle(&out2, &second) {
                    Ok(()) => Ok(second),
                    Err(e) => Err(crate::bundle::GenErr {
                        contract_version: Some(second.contract_version.clone()),
                        kind: GenErrKind::Internal(io_detail(&e)),
                    }),
                },
            };
        match second {
            Err(err) => {
                let mut row = Row::internal(
                    "generate",
                    format!(
                        "determinismo roto: segunda corrida falló: {}",
                        gen_detail(err.kind)
                    ),
                );
                row.contract_version = Some(built.contract_version.clone());
                row.bundle_sha256 = Some(built.bundle_sha256.clone());
                return finish(row, trace, Vec::new());
            }
            Ok(second) => {
                let identical = second.bundle_sha256 == built.bundle_sha256
                    && second.files == built.files
                    && second.contract_json == built.contract_json;
                if identical {
                    let _ = std::fs::remove_dir_all(&out2);
                    trace.push(
                        "phase: determinismo verificado (2 corridas byte-idénticas)".to_string(),
                    );
                } else {
                    let mut row = Row::internal(
                        "generate",
                        format!(
                            "determinismo roto: principal {} != verify-2 {}",
                            built.bundle_sha256, second.bundle_sha256
                        ),
                    );
                    row.contract_version = Some(built.contract_version.clone());
                    row.bundle_sha256 = Some(built.bundle_sha256.clone());
                    // both trees are kept on purpose (§8.4).
                    return finish(row, trace, Vec::new());
                }
            }
        }
    }

    let mut row = ok_row(
        "generate",
        format!(
            "{} files, {} anchors",
            built.files.len(),
            built.anchors_count
        ),
    );
    row.contract_version = Some(built.contract_version);
    row.bundle_sha256 = Some(built.bundle_sha256);
    row.files.checked = built.files.len();
    finish(row, trace, Vec::new())
}

// ---------------------------------------------------------------------------
// --verify
// ---------------------------------------------------------------------------

fn cmd_verify(args: &EvalArgs) -> Outcome {
    let mut trace = Vec::new();
    match bundle::verify_bundle(&args.bundle, &mut trace) {
        Err(Stop::Internal(detail)) => finish(Row::internal("verify", detail), trace, Vec::new()),
        Ok(report) => {
            let mut row = Row::from_obs("verify", report.obs.clone());
            if row.cause.is_none() {
                row.detail = report.ok_detail.clone();
            }
            apply_bundle(&mut row, &report);
            row.files = report.files;
            finish(row, trace, Vec::new())
        }
    }
}

// ---------------------------------------------------------------------------
// shared side-state evaluation (check / sync / diff)
// ---------------------------------------------------------------------------

/// `lock < bundle` with the §9.2 hint for the row detail.
fn stale_detail(lock: &SemVer, bundle: &SemVer, class: &'static str) -> String {
    let hint = match class {
        "patch" => "patch: re-sync",
        "minor" => "minor: re-sync sin revalidar",
        _ => "major: re-sync + revalidar probes y preview",
    };
    format!(
        "lock {} < bundle {} ({})",
        lock.to_string_version(),
        bundle.to_string_version(),
        hint
    )
}

/// Version comparison of P5/P6: `lock_ahead` (suppressed by
/// `--allow-downgrade`, surfaced in `note` instead) and `lock_stale` with its
/// observed SemVer class.
fn compare_versions(
    lock_version: &str,
    bundle_version: &str,
    allow_downgrade: bool,
) -> (Vec<Obs>, Option<String>) {
    let lock = SemVer::parse(lock_version).expect("lock version validated");
    let bundle = SemVer::parse(bundle_version).expect("bundle version validated");
    if lock == bundle {
        return (Vec::new(), None);
    }
    if lock > bundle {
        if allow_downgrade {
            let note = format!(
                "lock {} > bundle {} (downgrade permitido)",
                lock.to_string_version(),
                bundle.to_string_version()
            );
            return (Vec::new(), Some(note));
        }
        let obs = Obs::new(
            Cause::LockAhead,
            format!(
                "lock {} > bundle {} (major: no degradar sin --allow-downgrade)",
                lock.to_string_version(),
                bundle.to_string_version()
            ),
        );
        return (vec![obs], None);
    }
    let class = classify(lock, bundle).class();
    let obs = Obs::with_class(Cause::LockStale, class, stale_detail(&lock, &bundle, class));
    (vec![obs], None)
}

/// Everything `check`/`diff`/`sync` need to know about the side, in cut order.
struct SideState {
    row: Row,
    trace: Vec<String>,
    lock: Option<Lock>,
}

fn evaluate_side_state(args: &EvalArgs, command: &'static str) -> SideState {
    let mut trace = Vec::new();
    let side_id = sideio::read_side_id(&args.side);

    // P1/P2/P10 cut before the lock is read.
    let report = match bundle::verify_bundle(&args.bundle, &mut trace) {
        Err(Stop::Internal(detail)) => {
            let mut row = Row::internal(command, detail);
            row.side_id = side_id;
            return SideState {
                row,
                trace,
                lock: None,
            };
        }
        Ok(report) => report,
    };
    if !report.obs.is_empty() {
        let mut row = Row::from_obs(command, report.obs.clone());
        apply_bundle(&mut row, &report);
        row.files = report.files;
        row.side_id = side_id;
        return SideState {
            row,
            trace,
            lock: None,
        };
    }

    // P3/P4: the lock.
    let lock_outcome = match lockfile::read_lock(&args.side.join("contract.lock")) {
        Err(Stop::Internal(detail)) => {
            let mut row = Row::internal(command, detail);
            apply_bundle(&mut row, &report);
            row.side_id = side_id;
            return SideState {
                row,
                trace,
                lock: None,
            };
        }
        Ok(outcome) => outcome,
    };
    let lock = match lock_outcome {
        LockOutcome::Missing => {
            trace.push("phase: contract.lock ausente".to_string());
            let mut row = Row::from_obs(
                command,
                vec![Obs::new(
                    Cause::LockMissing,
                    "sin contract.lock: el side nunca se sincronizó (--sync)",
                )],
            );
            apply_bundle(&mut row, &report);
            row.side_id = side_id;
            return SideState {
                row,
                trace,
                lock: None,
            };
        }
        LockOutcome::Invalid(reason) => {
            trace.push("phase: contract.lock inválido".to_string());
            let mut row = Row::from_obs(
                command,
                vec![Obs::new(
                    Cause::LockInvalid,
                    format!("contract.lock inválido: {reason}"),
                )],
            );
            apply_bundle(&mut row, &report);
            row.side_id = side_id;
            return SideState {
                row,
                trace,
                lock: None,
            };
        }
        LockOutcome::Present(lock) => {
            trace.push("phase: contract.lock leído".to_string());
            lock
        }
    };

    // P5/P6: versions.
    let mut obs: Vec<Obs> = Vec::new();
    let bundle_version = report.contract_version.clone().expect("manifest validated");
    let (version_obs, note) = compare_versions(
        &lock.contract_version,
        &bundle_version,
        args.allow_downgrade,
    );
    obs.extend(version_obs);
    trace.push("phase: versiones comparadas".to_string());

    // P7/P8/P9: read-only walk of the copy.
    let scan = match sideio::scan_side(&args.side, &lock) {
        Err(Stop::Internal(detail)) => {
            let mut row = Row::internal(command, detail);
            apply_bundle(&mut row, &report);
            row.lock_contract_version = Some(lock.contract_version.clone());
            row.lock_bundle_sha256 = Some(lock.bundle_sha256.clone());
            row.side_id = side_id;
            return SideState {
                row,
                trace,
                lock: Some(lock),
            };
        }
        Ok(scan) => scan,
    };
    trace.push(format!(
        "phase: .side-contract/ verificado ({} archivos)",
        lock.files.len()
    ));
    obs.extend(scan.obs.clone());

    let mut row = Row::from_obs(command, obs);
    if row.cause.is_none() {
        row.detail = match note {
            Some(note) => note,
            None => format!("sin drift: {} archivos verificados", lock.files.len()),
        };
    }
    apply_bundle(&mut row, &report);
    row.lock_contract_version = Some(lock.contract_version.clone());
    row.lock_bundle_sha256 = Some(lock.bundle_sha256.clone());
    row.side_id = side_id;
    row.files = scan.files;
    SideState {
        row,
        trace,
        lock: Some(lock),
    }
}

fn cmd_check(args: &EvalArgs) -> Outcome {
    let state = evaluate_side_state(args, "check");
    finish(state.row, state.trace, Vec::new())
}

fn cmd_diff(args: &EvalArgs) -> Outcome {
    let state = evaluate_side_state(args, "diff");
    let mut row = state.row;
    let mut extra = Vec::new();
    let mut files_detail = Vec::new();

    if let Some(lock) = &state.lock {
        match sideio::diff_entries(&args.side, lock) {
            Err(Stop::Internal(detail)) => {
                let mut internal = Row::internal("diff", detail);
                internal.side_id = row.side_id.clone();
                return finish(internal, state.trace, Vec::new());
            }
            Ok(entries) => {
                for entry in &entries {
                    extra.push(format!(
                        "diff: {} esperado {} actual {}",
                        entry.path,
                        entry.expected.as_deref().unwrap_or("-"),
                        entry.actual.as_deref().unwrap_or("-")
                    ));
                }
                files_detail = entries.iter().map(diff_entry_value).collect();
            }
        }
    }
    if args.json {
        row.files_detail = Some(files_detail);
    }
    finish(row, state.trace, extra)
}

fn diff_entry_value(entry: &DiffEntry) -> serde_json::Value {
    json!({
        "actual": entry.actual,
        "expected": entry.expected,
        "path": entry.path,
    })
}

// ---------------------------------------------------------------------------
// --sync
// ---------------------------------------------------------------------------

fn cmd_sync(args: &EvalArgs) -> Outcome {
    let mut trace = Vec::new();
    let side_id = sideio::read_side_id(&args.side);

    // 1) verify the bundle internally first (P1/P2/P10 cut, no --force helps).
    let report = match bundle::verify_bundle(&args.bundle, &mut trace) {
        Err(Stop::Internal(detail)) => {
            let mut row = Row::internal("sync", detail);
            row.side_id = side_id;
            return finish(row, trace, Vec::new());
        }
        Ok(report) => report,
    };
    if !report.obs.is_empty() {
        let mut row = Row::from_obs("sync", report.obs.clone());
        apply_bundle(&mut row, &report);
        row.files = report.files;
        row.side_id = side_id;
        return finish(row, trace, Vec::new());
    }

    // 1b) read the previous lock; `--force` is the only way past a broken one.
    let lock_outcome = match lockfile::read_lock(&args.side.join("contract.lock")) {
        Err(Stop::Internal(detail)) => {
            let mut row = Row::internal("sync", detail);
            apply_bundle(&mut row, &report);
            row.side_id = side_id;
            return finish(row, trace, Vec::new());
        }
        Ok(outcome) => outcome,
    };
    let mut lock: Option<Lock> = None;
    match lock_outcome {
        LockOutcome::Present(value) => {
            trace.push("phase: contract.lock leído".to_string());
            lock = Some(value);
        }
        LockOutcome::Missing => {
            trace.push("phase: contract.lock ausente (primera sincronización)".to_string());
        }
        LockOutcome::Invalid(reason) => {
            if args.force {
                trace.push("phase: contract.lock inválido, --force lo reemplaza".to_string());
            } else {
                let mut row = Row::from_obs(
                    "sync",
                    vec![Obs::new(
                        Cause::LockInvalid,
                        format!("contract.lock inválido: {reason}"),
                    )],
                );
                apply_bundle(&mut row, &report);
                row.side_id = side_id;
                return finish(row, trace, Vec::new());
            }
        }
    }

    // 2) the check logic: `file_drift` / `file_unexpected` need `--force`,
    // `lock_ahead` needs `--allow-downgrade`; `lock_stale` and `file_missing`
    // are exactly what sync repairs, so they never block.
    let mut obs: Vec<Obs> = Vec::new();
    let mut files = FilesCounts::default();
    let mut note: Option<String> = None;
    if let Some(lock) = &lock {
        let bundle_version = report.contract_version.clone().expect("manifest validated");
        let (version_obs, version_note) = compare_versions(
            &lock.contract_version,
            &bundle_version,
            args.allow_downgrade,
        );
        obs.extend(version_obs);
        note = version_note;
        let scan = match sideio::scan_side(&args.side, lock) {
            Err(Stop::Internal(detail)) => {
                let mut row = Row::internal("sync", detail);
                apply_bundle(&mut row, &report);
                row.side_id = side_id;
                return finish(row, trace, Vec::new());
            }
            Ok(scan) => scan,
        };
        files = scan.files;
        obs.extend(scan.obs);
        trace.push("phase: precondiciones evaluadas".to_string());
    } else {
        trace.push("phase: precondiciones evaluadas".to_string());
    }

    // §10.6 step 2: without `--force` a mutated or unknown copy aborts with the
    // `--check` row (`wrote_side: false`) so nobody's edits die silently.
    // `--force` is the escape hatch of the only writer mode: `file_drift` and
    // `file_unexpected` stop blocking and the old copy disappears whole in the
    // step-4 replacement. `lock_ahead` is NOT lifted by `--force`: that one
    // belongs to `--allow-downgrade`.
    let mut forced_files = 0usize;
    let blocking = obs.iter().any(|o| {
        if matches!(o.cause, Cause::FileDrift | Cause::FileUnexpected) {
            if args.force {
                forced_files += 1;
                return false;
            }
            return true;
        }
        matches!(o.cause, Cause::LockAhead)
    });
    if forced_files > 0 {
        trace.push(format!(
            "phase: {forced_files} archivo(s) con drift/inesperado forzado(s) por --force"
        ));
    }
    if blocking {
        let mut row = Row::from_obs("sync", obs);
        apply_bundle(&mut row, &report);
        if let Some(lock) = &lock {
            row.lock_contract_version = Some(lock.contract_version.clone());
            row.lock_bundle_sha256 = Some(lock.bundle_sha256.clone());
        }
        row.side_id = side_id;
        row.files = files;
        return finish(row, trace, Vec::new());
    }

    // --dry-run: report exactly what would be written, touch nothing.
    if args.dry_run {
        let mut paths: Vec<String> = report
            .manifest_files
            .iter()
            .map(|entry| format!(".side-contract/{}", entry.path))
            .collect();
        paths.push(".side-contract/CONTRACT.json".to_string());
        paths.push("contract.lock".to_string());
        let mut row = ok_row("sync", format!("dry-run: {}", paths.join(" ")));
        apply_bundle(&mut row, &report);
        if let Some(lock) = &lock {
            row.lock_contract_version = Some(lock.contract_version.clone());
            row.lock_bundle_sha256 = Some(lock.bundle_sha256.clone());
        }
        row.side_id = side_id;
        row.files = FilesCounts {
            checked: report.manifest_files.len(),
            ..FilesCounts::default()
        };
        return finish(row, trace, Vec::new());
    }
    let _ = note; // a stale/allowed-ahead note never blocks and is not reported on success

    // 3) + 4) write the tree, 5) commit the lock last.
    let (payload, contract_json) = match bundle::read_payload(&args.bundle, &report.manifest_files)
    {
        Err(Stop::Internal(detail)) => {
            let mut row = Row::internal("sync", detail);
            apply_bundle(&mut row, &report);
            row.side_id = side_id;
            return finish(row, trace, Vec::new());
        }
        Ok(payload) => payload,
    };
    if let Err(e) = sideio::write_tree(&args.side, &payload, &contract_json) {
        let mut row = Row::internal("sync", io_detail(&e));
        apply_bundle(&mut row, &report);
        row.side_id = side_id;
        return finish(row, trace, Vec::new());
    }
    trace.push("phase: árbol escrito y reemplazado (.side-contract)".to_string());

    let bundle_version = report.contract_version.clone().expect("manifest validated");
    let bundle_sha = report.bundle_sha256.clone().expect("manifest hashed");
    let lock_files = sideio::manifest_to_lock_files(&report.manifest_files);
    let lock_text = lockfile::render_lock(&bundle_version, &bundle_sha, &lock_files);
    if let Err(e) = sideio::commit_lock(&args.side, &lock_text) {
        // The tree was already replaced: say so honestly (wrote_side), and the
        // next --check will see old-lock-vs-new-tree — never a false OK.
        let mut row = Row::internal("sync", io_detail(&e));
        apply_bundle(&mut row, &report);
        row.lock_contract_version = lock.as_ref().map(|l| l.contract_version.clone());
        row.lock_bundle_sha256 = lock.as_ref().map(|l| l.bundle_sha256.clone());
        row.side_id = side_id;
        row.wrote_side = true;
        return finish(row, trace, Vec::new());
    }
    trace.push("phase: contract.lock escrito (commit point)".to_string());

    let mut row = ok_row(
        "sync",
        format!(
            "{} archivos + contract.lock escritos",
            report.manifest_files.len()
        ),
    );
    apply_bundle(&mut row, &report);
    row.lock_contract_version = Some(bundle_version);
    row.lock_bundle_sha256 = Some(bundle_sha);
    row.side_id = side_id;
    row.files = FilesCounts {
        checked: report.manifest_files.len(),
        ..FilesCounts::default()
    };
    row.wrote_side = true;
    finish(row, trace, Vec::new())
}

/// Dispatch over the five evaluators (the `command` enum of schema 1.0).
pub fn run(args: &EvalArgs) -> Outcome {
    match args.command {
        "generate" => cmd_generate(args),
        "verify" => cmd_verify(args),
        "check" => cmd_check(args),
        "sync" => cmd_sync(args),
        "diff" => cmd_diff(args),
        other => unreachable!("unknown evaluator {other}"),
    }
}

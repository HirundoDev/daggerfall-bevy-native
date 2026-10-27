//! Probe evaluation — plan §4.3 step 4 with the OD-8 (b) contract:
//! argv without a shell, a `{seed}` placeholder, a hard `timeout_secs`,
//! `expected_sha256 = sha256(stdout_bytes)` and **two** runs that must agree.
//!
//! Order of checks (first failure stops, taxonomy v1):
//! set of names in block == `probes.required`, then per probe: command
//! declared → `{seed}` present → block fields (seed, runs, argv, digest
//! format) → execute twice → determinism → hash equality.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::manifest::{Manifest, ProbeCommand};
use crate::taxonomy::Cause;

/// Runs declared in the machine block must equal this (OD-8 (b): 2 corridas).
pub const MANDATED_RUNS: i64 = 2;

/// Placeholder the gate substitutes with the block's `seed`.
pub const SEED_PLACEHOLDER: &str = "{seed}";

/// A typed step-4 failure.
#[derive(Debug, Clone)]
pub struct ProbeFailure {
    pub cause: Cause,
    pub detail: String,
}

/// Run every required probe twice and return `probe name -> sha256(stdout)`.
pub fn run_all(
    manifest: &Manifest,
    block: &Value,
    side_root: &Path,
) -> Result<BTreeMap<String, String>, ProbeFailure> {
    check_contract(manifest, block)?;

    let mut hashes = BTreeMap::new();
    for name in &manifest.probes.required {
        let command = manifest.probes.commands.get(name).ok_or_else(|| ProbeFailure {
            cause: Cause::ProbeCommandUndefined,
            detail: format!("probe {name:?} declarada en probes.required sin probes.commands"),
        })?;
        if !command.argv.iter().any(|a| a.contains(SEED_PLACEHOLDER)) {
            return Err(ProbeFailure {
                cause: Cause::ProbeSeedContractMissing,
                detail: format!(
                    "probe {name:?}: argv sin placeholder {SEED_PLACEHOLDER} (OD-8 (b))"
                ),
            });
        }
        let entry = block_entry(block, name).expect("contract checked names");
        let seed = entry.get("seed").and_then(|s| s.as_str()).unwrap_or("");
        if seed.is_empty() {
            return Err(ProbeFailure {
                cause: Cause::ProbeSeedContractMissing,
                detail: format!("probe {name:?}: bloque machine sin `seed`"),
            });
        }
        let hash = run_probe(name, command, seed, side_root)?;
        let expected = entry
            .get("expected_sha256")
            .and_then(|h| h.as_str())
            .unwrap_or_default();
        if hash != expected {
            return Err(ProbeFailure {
                cause: Cause::ProbeHashMismatch,
                detail: format!(
                    "probe {name:?}: sha256(stdout) {hash} != expected_sha256 {expected}"
                ),
            });
        }
        hashes.insert(name.clone(), hash);
    }
    Ok(hashes)
}

/// Block/manifest agreement: same name set, `runs == 2`, identical argv and a
/// 64-hex `expected_sha256`.
fn check_contract(manifest: &Manifest, block: &Value) -> Result<(), ProbeFailure> {
    let declared: BTreeSet<String> = manifest.probes.required.iter().cloned().collect();
    let blocked: BTreeSet<String> = block_names(block);
    if declared != blocked {
        let missing: Vec<_> = declared.difference(&blocked).cloned().collect();
        let extra: Vec<_> = blocked.difference(&declared).cloned().collect();
        return Err(ProbeFailure {
            cause: Cause::ProbeReportMismatch,
            detail: format!(
                "bloque no refleja probes.required (faltan: {:?}; sobran: {:?})",
                missing, extra
            ),
        });
    }

    for name in &manifest.probes.required {
        let entry = block_entry(block, name).expect("name sets already match");
        if let Some(command) = manifest.probes.commands.get(name) {
            let block_argv: Vec<String> = entry
                .get("command")
                .and_then(|c| c.as_array())
                .map(|a| {
                    a.iter()
                        .map(|v| v.as_str().unwrap_or_default().to_string())
                        .collect()
                })
                .unwrap_or_default();
            if block_argv != command.argv {
                return Err(ProbeFailure {
                    cause: Cause::ProbeReportMismatch,
                    detail: format!(
                        "probe {name:?}: command del bloque difiere del manifest ({block_argv:?} != {:?})",
                        command.argv
                    ),
                });
            }
        }
        let runs = entry.get("runs").and_then(|r| r.as_i64());
        if runs != Some(MANDATED_RUNS) {
            return Err(ProbeFailure {
                cause: Cause::ProbeReportMismatch,
                detail: format!(
                    "probe {name:?}: runs={runs:?} en el bloque (OD-8 (b) exige {MANDATED_RUNS})"
                ),
            });
        }
        let expected = entry.get("expected_sha256").and_then(|h| h.as_str());
        if !is_sha256_hex(expected) {
            return Err(ProbeFailure {
                cause: Cause::ProbeReportMismatch,
                detail: format!(
                    "probe {name:?}: expected_sha256 ausente o no hexadecimal de 64"
                ),
            });
        }
        // A manifest command without {seed} breaks the OD-8 (b) contract even
        // when the block itself is consistent.
        if let Some(command) = manifest.probes.commands.get(name) {
            if !command.argv.iter().any(|a| a.contains(SEED_PLACEHOLDER)) {
                return Err(ProbeFailure {
                    cause: Cause::ProbeSeedContractMissing,
                    detail: format!(
                        "probe {name:?}: argv sin placeholder {SEED_PLACEHOLDER} (OD-8 (b))"
                    ),
                });
            }
        }
    }
    Ok(())
}

fn block_names(block: &Value) -> BTreeSet<String> {
    block
        .get("probes")
        .and_then(|p| p.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|e| e.get("name").and_then(|n| n.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn block_entry<'a>(block: &'a Value, name: &str) -> Option<&'a Value> {
    block
        .get("probes")?
        .as_array()?
        .iter()
        .find(|e| e.get("name").and_then(|n| n.as_str()) == Some(name))
}

fn is_sha256_hex(value: Option<&str>) -> bool {
    match value {
        Some(h) => h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()),
        None => false,
    }
}

/// Substitute `{seed}` in every argv element (no shell, no interpretation).
pub fn substitute_seed(argv: &[String], seed: &str) -> Vec<String> {
    argv.iter().map(|a| a.replace(SEED_PLACEHOLDER, seed)).collect()
}

/// Execute one probe twice and compare both digests.
fn run_probe(
    name: &str,
    command: &ProbeCommand,
    seed: &str,
    side_root: &Path,
) -> Result<String, ProbeFailure> {
    let args = substitute_seed(&command.argv, seed);
    let timeout = Duration::from_secs(command.timeout_secs);

    let first = execute_once(name, &args, side_root, timeout)?;
    let second = execute_once(name, &args, side_root, timeout)?;
    if first != second {
        return Err(ProbeFailure {
            cause: Cause::ProbeNondeterministic,
            detail: format!(
                "probe {name:?}: dos corridas con hashes distintos ({} != {})",
                hex(&first),
                hex(&second)
            ),
        });
    }
    Ok(hex(&first))
}

/// One run: spawn argv[0] directly, drain stdout on a side thread, kill on
/// timeout. `side_root` is the child's cwd so relative fixture paths resolve.
fn execute_once(
    name: &str,
    args: &[String],
    side_root: &Path,
    timeout: Duration,
) -> Result<[u8; 32], ProbeFailure> {
    let program = &args[0];
    let mut child = Command::new(program)
        .args(&args[1..])
        .current_dir(side_root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| ProbeFailure {
            cause: Cause::ProbeCommandUndefined,
            detail: format!("probe {name:?}: no se pudo ejecutar {program:?}: {e}"),
        })?;

    let mut stdout = child.stdout.take().expect("stdout piped");
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = reader.join();
                    return Err(ProbeFailure {
                        cause: Cause::ProbeTimeout,
                        detail: format!(
                            "probe {name:?}: excedió timeout_secs={}",
                            timeout.as_secs()
                        ),
                    });
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err(ProbeFailure {
                    cause: Cause::ProbeTimeout,
                    detail: format!("probe {name:?}: error esperando el proceso: {e}"),
                });
            }
        }
    };

    let out = reader.join().unwrap_or_default();
    if !status.success() {
        return Err(ProbeFailure {
            cause: Cause::ProbeNonzeroExit,
            detail: format!("probe {name:?}: exit {status}"),
        });
    }
    let mut hasher = Sha256::new();
    hasher.update(&out);
    let digest = hasher.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&digest);
    Ok(arr)
}

fn hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// sha256 of arbitrary bytes as lowercase hex (shared with evidence hashing).
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn manifest(argv: Vec<String>) -> Manifest {
        let raw = format!(
            r#"
schema_version = "1.3"
[side]
id = "f4b-unit"
version = "0.1.0"
display_name = "x"
owner = "owner"
kind = "plugin"
[compat]
core = "^0.1"
bevy = "0.20"
manifest = "1.3"
[contract]
version = "1.0.0"
sha256 = "8b8b16c62d314d623a8493386ea67cb750c001683600f7622b2b02c5495bb604"
[plugin]
entry_crate = "x"
plugin_type = "XPlugin"
load_order = 1
[capabilities]
grants = []
[reads]
resources = []
events = []
[writes]
paths = []
state = "side-owned"
[probes]
required = ["smoke"]
[probes.commands.smoke]
argv = [{}]
timeout_secs = 5
[preview]
argv = ["f4b-preview-side", "--socket", "{{socket}}", "--instance", "{{instance}}"]
instance = "preview"
timeout_secs = 5
[readiness]
state = "ready"
report = "readiness-report.md"
"#,
            argv.iter()
                .map(|a| format!("{:?}", a))
                .collect::<Vec<_>>()
                .join(", ")
        );
        toml::from_str(&raw).unwrap()
    }

    fn block(command: Vec<serde_json::Value>, runs: i64, seed: Option<&str>) -> Value {
        json!({
            "schema": "readiness-probes",
            "probes": [{
                "name": "smoke",
                "command": command,
                "seed": seed.unwrap_or("s1"),
                "expected_sha256": "0".repeat(64),
                "runs": runs
            }]
        })
    }

    #[test]
    fn seed_substitution_replaces_every_occurrence() {
        let argv = vec!["--seed".into(), "{seed}".into(), "x-{seed}".into()];
        let out = substitute_seed(&argv, "42");
        assert_eq!(out, vec!["--seed", "42", "x-42"]);
    }

    #[test]
    fn name_set_mismatch_is_report_mismatch() {
        let m = manifest(vec!["printf".into(), "{seed}".into()]);
        let b = json!({"probes": []});
        let err = check_contract(&m, &b).unwrap_err();
        assert_eq!(err.cause, Cause::ProbeReportMismatch);
    }

    #[test]
    fn runs_must_be_two() {
        let m = manifest(vec!["printf".into(), "{seed}".into()]);
        let b = block(vec![json!("printf"), json!("{seed}")], 1, None);
        let err = check_contract(&m, &b).unwrap_err();
        assert_eq!(err.cause, Cause::ProbeReportMismatch);
        assert!(err.detail.contains("runs"), "{}", err.detail);
    }

    #[test]
    fn argv_mismatch_is_report_mismatch() {
        let m = manifest(vec!["printf".into(), "{seed}".into()]);
        let b = block(vec![json!("printf"), json!("other-{seed}")], 2, None);
        let err = check_contract(&m, &b).unwrap_err();
        assert_eq!(err.cause, Cause::ProbeReportMismatch);
    }

    #[test]
    fn bad_digest_format_is_report_mismatch() {
        let m = manifest(vec!["printf".into(), "{seed}".into()]);
        let b = json!({"probes": [{
            "name": "smoke",
            "command": ["printf", "{seed}"],
            "seed": "s1",
            "expected_sha256": "zz",
            "runs": 2
        }]});
        let err = check_contract(&m, &b).unwrap_err();
        assert_eq!(err.cause, Cause::ProbeReportMismatch);
    }

    #[test]
    fn empty_seed_in_block_is_seed_contract() {
        let m = manifest(vec!["printf".into(), "{seed}".into()]);
        let b = json!({"probes": [{
            "name": "smoke",
            "command": ["printf", "{seed}"],
            "seed": "",
            "expected_sha256": "0".repeat(64),
            "runs": 2
        }]});
        let err = run_all(&m, &b, Path::new(".")).unwrap_err();
        assert_eq!(err.cause, Cause::ProbeSeedContractMissing);
    }

    #[test]
    fn missing_placeholder_is_seed_contract() {
        let m = manifest(vec!["printf".into(), "no-placeholder".into()]);
        let b = block(vec![json!("printf"), json!("no-placeholder")], 2, None);
        let err = check_contract(&m, &b).unwrap_err();
        assert_eq!(err.cause, Cause::ProbeSeedContractMissing);
    }

    #[test]
    fn deterministic_probe_hashes_stdout() {
        let m = manifest(vec!["printf".into(), "ok-{seed}".into()]);
        let expected = sha256_hex(b"ok-s1");
        let b = json!({"probes": [{
            "name": "smoke",
            "command": ["printf", "ok-{seed}"],
            "seed": "s1",
            "expected_sha256": expected,
            "runs": 2
        }]});
        let hashes = run_all(&m, &b, Path::new(".")).expect("probe determinista");
        assert_eq!(hashes["smoke"], expected);
    }

    #[test]
    fn wrong_hash_is_hash_mismatch() {
        let m = manifest(vec!["printf".into(), "ok-{seed}".into()]);
        let b = block(vec![json!("printf"), json!("ok-{seed}")], 2, None);
        let err = run_all(&m, &b, Path::new(".")).unwrap_err();
        assert_eq!(err.cause, Cause::ProbeHashMismatch);
    }

    #[test]
    fn nonzero_exit_is_typed() {
        let m = manifest(vec!["false".into(), "{seed}".into()]);
        let b = block(vec![json!("false"), json!("{seed}")], 2, None);
        let err = run_all(&m, &b, Path::new(".")).unwrap_err();
        assert_eq!(err.cause, Cause::ProbeNonzeroExit);
    }

    #[test]
    fn nondeterministic_probe_is_typed() {
        let m = manifest(vec!["date".into(), "+%N-{seed}".into()]);
        let b = block(vec![json!("date"), json!("+%N-{seed}")], 2, None);
        let err = run_all(&m, &b, Path::new(".")).unwrap_err();
        assert_eq!(err.cause, Cause::ProbeNondeterministic);
    }

    #[test]
    fn missing_binary_is_command_undefined() {
        let m = manifest(vec!["f4a-definitely-not-a-binary".into(), "{seed}".into()]);
        let b = block(
            vec![json!("f4a-definitely-not-a-binary"), json!("{seed}")],
            2,
            None,
        );
        let err = run_all(&m, &b, Path::new(".")).unwrap_err();
        assert_eq!(err.cause, Cause::ProbeCommandUndefined);
    }
}

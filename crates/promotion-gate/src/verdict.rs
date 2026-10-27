//! The verdict row: one JSON object used both as the gate's stdout and as the
//! append-only promotion-ledger row (plan §4.3 step 7, §9 F4 (b)).
//!
//! Exactly one row is appended per gate run. The ledger is written **only by
//! the main** (`--ledger`, default
//! `docs/sintergic/promotion-gate/promotion-ledger.jsonl`): a side can read it
//! but never writes it.
//!
//! Schema `1.1` (F4-B) adds the two step-6 evidence blocks (`preview`,
//! `main_baseline`) and makes `PASS` constructible: a `PASS` row cannot exist
//! without both, because [`Verdict::pass`] demands them.
//!
//! Schema `1.2` (F6-D) adds the step-0 `contract` block (DESIGN-v1 §12.2):
//! `{lock_version, bundle_sha256, drift_class}`, `null` only when the gate
//! failed before it could read the lock (`gate_internal_error`).

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::contract::ContractEvidence;
use crate::taxonomy::{
    Cause, EXIT_INCOMPLETE_INTERNAL, EXIT_PASS, TAXONOMY_VERSION, VERDICT_SCHEMA,
    VERDICT_SCHEMA_VERSION,
};

/// Hashes that make a verdict auditable: what was read, and what the probes printed.
#[derive(Debug, Clone, Serialize, Default)]
pub struct EvidenceHashes {
    /// sha256 of the raw `side-project.toml` bytes, when readable.
    pub manifest_sha256: Option<String>,
    /// sha256 of the raw `readiness-report.md` bytes, when readable.
    pub report_sha256: Option<String>,
    /// probe name -> sha256 of its stdout (both mandated runs agree).
    pub probes: BTreeMap<String, String>,
}

/// Step-6a evidence: what the preview published over the runtime control socket.
#[derive(Debug, Clone, Serialize)]
pub struct PreviewEvidence {
    /// Instance id the preview was asked to serve.
    pub instance: String,
    /// `status.api` the preview negotiated (`1.0`).
    pub api: String,
    /// `status.lifecycle` observed after composition.
    pub lifecycle: String,
    /// Permission bits of the published socket (`0o600` = 384).
    pub socket_mode: u32,
    /// `status.loaded_sides`, ascending.
    pub loaded_sides: Vec<String>,
    /// sha256 of the exact `status` response line the gate accepted.
    pub status_sha256: String,
    /// sha256 of everything the preview printed to stdout before the handshake.
    pub stdout_sha256: String,
    /// Milliseconds from spawn to the accepted `status`.
    pub startup_ms: u64,
}

/// Step-6b evidence: the main's baseline surface, as declared and as measured.
#[derive(Debug, Clone, Serialize)]
pub struct BaselineEvidence {
    /// Declared baseline files, in declaration order.
    pub files: Vec<String>,
    /// sha256 of the declaration bytes (`main-baseline.toml`).
    pub declaration_sha256: String,
    /// sha256 over `path\n<sha256 of the working file>\n` for every declared file.
    ///
    /// Deterministic pin: the same main tree always yields the same digest.
    pub surface_sha256: String,
}

/// Both step-6 blocks. A `PASS` row carries exactly one of these.
#[derive(Debug, Clone, Serialize)]
pub struct StepSixEvidence {
    pub preview: PreviewEvidence,
    pub main_baseline: BaselineEvidence,
}

impl StepSixEvidence {
    pub fn into_parts(self) -> (PreviewEvidence, BaselineEvidence) {
        (self.preview, self.main_baseline)
    }
}

/// One promotion-gate execution. Serialized identically to stdout and to the ledger.
#[derive(Debug, Clone, Serialize)]
pub struct Verdict {
    pub schema: String,
    pub schema_version: String,
    pub taxonomy_version: String,
    /// Version of the `daggerfall-promotion-gate` crate that produced the row.
    pub gate_version: String,
    /// RFC 3339 UTC timestamp of the run.
    pub ts: String,
    /// `git rev-parse HEAD` of the main workspace; `null` only with `gate_internal_error`.
    pub main_head: Option<String>,
    pub side_id: Option<String>,
    pub side_version: Option<String>,
    /// `git rev-parse HEAD` of the side workspace (only when the side has its own `.git`).
    pub side_commit: Option<String>,
    /// `PASS` | `FAIL` | `INCOMPLETE`.
    pub verdict: String,
    /// Stable cause id from the taxonomy; `null` only when verdict is `PASS`.
    pub cause: Option<String>,
    /// §4.3 step where the evaluation stopped (`null` for `gate_internal_error`).
    pub failed_step: Option<u8>,
    /// One-line human explanation (never a substitute for `cause`).
    pub detail: String,
    pub evidence_hashes: EvidenceHashes,
    /// Step-0 evidence (schema 1.2): the side's lock vs the main's published
    /// contract. `null` only when the gate failed before reading the lock.
    pub contract: Option<ContractEvidence>,
    /// Step 6a evidence, when a preview ran (also on `main_baseline_failed`).
    pub preview: Option<PreviewEvidence>,
    /// Step 6b evidence, present only when the main baseline was measured.
    pub main_baseline: Option<BaselineEvidence>,
    /// Whether this row reached the ledger (always `true` in the ledger itself;
    /// `false` only on the stdout copy when the append failed).
    pub ledger_written: bool,
}

impl Verdict {
    /// Build a row for a cause; the verdict word and exit code come from the taxonomy.
    pub fn from_cause(
        cause: Cause,
        main_head: Option<String>,
        side_id: Option<String>,
        side_version: Option<String>,
        side_commit: Option<String>,
        detail: impl Into<String>,
        evidence: EvidenceHashes,
    ) -> Self {
        Verdict {
            schema: VERDICT_SCHEMA.to_string(),
            schema_version: VERDICT_SCHEMA_VERSION.to_string(),
            taxonomy_version: TAXONOMY_VERSION.to_string(),
            gate_version: env!("CARGO_PKG_VERSION").to_string(),
            ts: now_rfc3339(),
            main_head,
            side_id,
            side_version,
            side_commit,
            verdict: cause.verdict().to_string(),
            cause: Some(cause.id().to_string()),
            failed_step: cause.step(),
            detail: detail.into(),
            evidence_hashes: evidence,
            contract: None,
            preview: None,
            main_baseline: None,
            ledger_written: true,
        }
    }

    /// Build the `PASS` row. The only constructor that does **not** take a
    /// cause, and it cannot exist without the step-6 evidence.
    pub fn pass(
        main_head: Option<String>,
        side_id: Option<String>,
        side_version: Option<String>,
        side_commit: Option<String>,
        detail: impl Into<String>,
        evidence: EvidenceHashes,
        step_six: StepSixEvidence,
    ) -> Self {
        let (preview, main_baseline) = step_six.into_parts();
        Verdict {
            schema: VERDICT_SCHEMA.to_string(),
            schema_version: VERDICT_SCHEMA_VERSION.to_string(),
            taxonomy_version: TAXONOMY_VERSION.to_string(),
            gate_version: env!("CARGO_PKG_VERSION").to_string(),
            ts: now_rfc3339(),
            main_head,
            side_id,
            side_version,
            side_commit,
            verdict: "PASS".to_string(),
            cause: None,
            failed_step: None,
            detail: detail.into(),
            evidence_hashes: evidence,
            contract: None,
            preview: Some(preview),
            main_baseline: Some(main_baseline),
            ledger_written: true,
        }
    }

    /// Attach step-0 contract evidence to any row (schema 1.2). The gate calls
    /// this on every path after the baseline: a row is complete when it can say
    /// which contract the side composed against.
    pub fn with_contract(mut self, contract: Option<ContractEvidence>) -> Self {
        self.contract = contract;
        self
    }

    /// Attach step-6a evidence to a failing row (the preview ran, the baseline did not pass).
    pub fn with_preview(mut self, preview: PreviewEvidence) -> Self {
        self.preview = Some(preview);
        self
    }

    /// Exit code for this verdict (plan/PROMOTION-GATE-v3 table).
    ///
    /// Derived from the taxonomy, so a cause can never carry two exit codes.
    pub fn exit_code(&self) -> i32 {
        match self.cause.as_deref() {
            None => EXIT_PASS,
            Some(id) => match Cause::from_id(id) {
                Some(cause) => cause.exit_code(),
                // A row whose cause is not in this taxonomy was not produced by
                // this build: refuse to claim PASS or FAIL for it.
                None => EXIT_INCOMPLETE_INTERNAL,
            },
        }
    }

    /// Compact single-line JSON, the exact bytes appended to the ledger.
    pub fn to_json_line(&self) -> String {
        serde_json::to_string(self).expect("verdict serializes")
    }

    /// Append this row to `path` (creating parent directories). Returns the
    /// error instead of panicking: a ledger that cannot be written is reported,
    /// never silently dropped.
    pub fn append_to(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
            }
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| format!("open {}: {e}", path.display()))?;
        file.write_all(self.to_json_line().as_bytes())
            .and_then(|_| file.write_all(b"\n"))
            .map_err(|e| format!("write {}: {e}", path.display()))?;
        file.flush().map_err(|e| format!("flush {}: {e}", path.display()))
    }
}

/// RFC 3339 UTC timestamp with second precision, computed without extra crates.
pub fn now_rfc3339() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    unix_to_rfc3339(secs as i64)
}

/// Civil date/time formatting from a unix timestamp (days algorithm, UTC only).
pub fn unix_to_rfc3339(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Howard Hinnant's civil_from_days, valid for any Gregorian date.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{h:02}:{min:02}:{s:02}Z",
        m = m,
        d = d,
        h = rem / 3600,
        min = (rem % 3600) / 60,
        s = rem % 60,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::taxonomy::{Cause, EXIT_FAIL};

    fn preview() -> PreviewEvidence {
        PreviewEvidence {
            instance: "preview".into(),
            api: "1.0".into(),
            lifecycle: "running".into(),
            socket_mode: 0o600,
            loaded_sides: vec!["f4b-preview-pass".into()],
            status_sha256: "aa".into(),
            stdout_sha256: "dd".into(),
            startup_ms: 12,
        }
    }

    fn baseline() -> BaselineEvidence {
        BaselineEvidence {
            files: vec!["crates/game-core/src/lib.rs".into()],
            declaration_sha256: "bb".into(),
            surface_sha256: "cc".into(),
        }
    }

    #[test]
    fn rfc3339_known_values() {
        assert_eq!(unix_to_rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(unix_to_rfc3339(1_792_912_746), "2026-10-25T07:19:06Z");
    }

    #[test]
    fn fail_row_carries_every_ordered_field() {
        let v = Verdict::from_cause(
            Cause::PreviewFailed,
            Some("cc2771a".into()),
            Some("f4b-x".into()),
            Some("0.1.0".into()),
            None,
            "detalle",
            EvidenceHashes::default(),
        );
        let json: serde_json::Value = serde_json::from_str(&v.to_json_line()).unwrap();
        for key in [
            "ts",
            "gate_version",
            "main_head",
            "side_id",
            "side_version",
            "side_commit",
            "verdict",
            "cause",
            "failed_step",
            "detail",
            "evidence_hashes",
            "contract",
            "preview",
            "main_baseline",
            "schema",
            "schema_version",
            "taxonomy_version",
            "ledger_written",
        ] {
            assert!(json.get(key).is_some(), "missing field {key}");
        }
        assert_eq!(json["verdict"], "FAIL");
        assert_eq!(json["cause"], "preview_failed");
        assert_eq!(json["failed_step"], 6);
        assert_eq!(json["taxonomy_version"], TAXONOMY_VERSION);
        assert_eq!(json["schema_version"], "1.2");
        assert_eq!(json["contract"], serde_json::Value::Null);
        assert_eq!(json["preview"], serde_json::Value::Null);
        assert_eq!(v.exit_code(), EXIT_FAIL);
    }

    #[test]
    fn with_contract_attaches_the_step_zero_block() {
        let v = Verdict::from_cause(
            Cause::ContractStale,
            Some("cc2771a".into()),
            Some("f4b-x".into()),
            Some("0.1.0".into()),
            None,
            "detalle",
            EvidenceHashes::default(),
        )
        .with_contract(Some(ContractEvidence {
            lock_version: Some("1.0.0".into()),
            bundle_sha256: Some("8b8b16c6".into()),
            drift_class: Some("major".into()),
        }));
        let json: serde_json::Value = serde_json::from_str(&v.to_json_line()).unwrap();
        assert_eq!(json["contract"]["lock_version"], "1.0.0");
        assert_eq!(json["contract"]["bundle_sha256"], "8b8b16c6");
        assert_eq!(json["contract"]["drift_class"], "major");
        assert_eq!(json["failed_step"], 0);
    }

    #[test]
    fn pass_row_has_no_cause_and_carries_step_six() {
        let v = Verdict::pass(
            Some("cc2771a".into()),
            Some("f4b-preview-pass".into()),
            Some("0.1.0".into()),
            None,
            "preview compone",
            EvidenceHashes::default(),
            StepSixEvidence {
                preview: preview(),
                main_baseline: baseline(),
            },
        );
        let json: serde_json::Value = serde_json::from_str(&v.to_json_line()).unwrap();
        assert_eq!(json["verdict"], "PASS");
        assert_eq!(json["cause"], serde_json::Value::Null);
        assert_eq!(json["failed_step"], serde_json::Value::Null);
        assert_eq!(json["preview"]["lifecycle"], "running");
        assert_eq!(json["preview"]["socket_mode"], 384);
        assert_eq!(json["main_baseline"]["files"][0], "crates/game-core/src/lib.rs");
        assert_eq!(v.exit_code(), EXIT_PASS);
    }

    #[test]
    fn unknown_cause_never_yields_pass() {
        let mut v = Verdict::from_cause(
            Cause::PreviewTimeout,
            None,
            None,
            None,
            None,
            "x",
            EvidenceHashes::default(),
        );
        v.cause = Some("from_the_future".into());
        assert_eq!(v.exit_code(), EXIT_INCOMPLETE_INTERNAL);
    }

    #[test]
    fn preview_evidence_survives_a_baseline_failure() {
        let v = Verdict::from_cause(
            Cause::MainBaselineFailed,
            Some("cc2771a".into()),
            Some("f4b-x".into()),
            Some("0.1.0".into()),
            None,
            "baseline",
            EvidenceHashes::default(),
        )
        .with_preview(preview());
        let json: serde_json::Value = serde_json::from_str(&v.to_json_line()).unwrap();
        assert_eq!(json["preview"]["instance"], "preview");
        assert_eq!(json["main_baseline"], serde_json::Value::Null);
        assert_eq!(v.exit_code(), EXIT_FAIL);
    }

    #[test]
    fn append_creates_parent_and_row() {
        let dir = std::env::temp_dir().join(format!("f4b-verdict-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let ledger = dir.join("nested/promotion-ledger.jsonl");
        let v = Verdict::from_cause(
            Cause::AllowlistViolation,
            None,
            None,
            None,
            None,
            "x",
            EvidenceHashes::default(),
        );
        v.append_to(&ledger).unwrap();
        v.append_to(&ledger).unwrap();
        let body = std::fs::read_to_string(&ledger).unwrap();
        assert_eq!(body.lines().count(), 2);
        for line in body.lines() {
            assert!(serde_json::from_str::<serde_json::Value>(line).is_ok());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn append_to_directory_errors() {
        let dir = std::env::temp_dir().join(format!("f4b-verdict-dir-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let v = Verdict::from_cause(
            Cause::GateInternalError,
            None,
            None,
            None,
            None,
            "x",
            EvidenceHashes::default(),
        );
        assert!(v.append_to(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

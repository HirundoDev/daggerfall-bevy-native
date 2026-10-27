//! The `side-contract-verdict` 1.0 row (DESIGN-v1 §10.2) and its canonical
//! serialization.
//!
//! The row is built **always** through `serde_json::Value`/`json!`: `serde_json`'s
//! default `Map` is a `BTreeMap`, so keys come out lexicographically sorted at
//! every level, compact, `/` unescaped, non-ASCII raw, integers plain, and the
//! line always ends in exactly one `\n`. No `serde` derive ordering ever leaks
//! into the output. There is deliberately **no `ts`** (§10.2): a fixture can
//! compare the whole row byte for byte.

use serde_json::{json, Value};

use crate::taxonomy::{Cause, TAXONOMY_VERSION, VERDICT_SCHEMA, VERDICT_SCHEMA_VERSION};

/// One observed cause with its (possibly dynamic) drift class and a
/// deterministic human detail.
#[derive(Debug, Clone)]
pub struct Obs {
    pub cause: Cause,
    pub class: &'static str,
    pub detail: String,
}

impl Obs {
    /// Observation with the cause's static default class.
    pub fn new(cause: Cause, detail: impl Into<String>) -> Obs {
        Obs {
            cause,
            class: cause.default_class(),
            detail: detail.into(),
        }
    }

    /// Observation with an explicit class (used by `lock_stale`).
    pub fn with_class(cause: Cause, class: &'static str, detail: impl Into<String>) -> Obs {
        Obs {
            cause,
            class,
            detail: detail.into(),
        }
    }
}

/// Sort observations by precedence; the first one is the primary cause.
pub fn sort_by_precedence(obs: &mut [Obs]) {
    obs.sort_by_key(|o| o.cause.precedence());
}

/// `files` counters of the row.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FilesCounts {
    pub checked: usize,
    pub drifted: usize,
    pub missing: usize,
    pub unexpected: usize,
}

/// One `side-contract-verdict` 1.0 row.
#[derive(Debug, Clone)]
pub struct Row {
    /// One of the five evaluators: `generate` | `verify` | `check` | `sync` | `diff`.
    pub command: &'static str,
    pub contract_version: Option<String>,
    pub bundle_sha256: Option<String>,
    pub lock_contract_version: Option<String>,
    pub lock_bundle_sha256: Option<String>,
    pub side_id: Option<String>,
    /// `OK` | `DRIFT` | `FAIL` | `INCOMPLETE`.
    pub verdict: &'static str,
    /// Primary cause by precedence; `None` **only** in `OK`.
    pub cause: Option<&'static str>,
    /// All observed causes, ordered by precedence.
    pub causes: Vec<&'static str>,
    pub drift_class: &'static str,
    pub blocking: bool,
    pub files: FilesCounts,
    pub wrote_side: bool,
    pub detail: String,
    /// `diff --json` only: per-file expected/actual digests, ordered by path.
    pub files_detail: Option<Vec<Value>>,
}

impl Row {
    /// Builds the row from ordered observations (empty ⇒ `OK`).
    pub fn from_obs(command: &'static str, mut obs: Vec<Obs>) -> Row {
        sort_by_precedence(&mut obs);
        let primary = obs.first();
        let (verdict, cause, drift_class) = match primary {
            None => ("OK", None, "none"),
            Some(o) => (o.cause.verdict(), Some(o.cause.id()), o.class),
        };
        let causes = obs.iter().map(|o| o.cause.id()).collect();
        let detail = match primary {
            None => "sin observaciones".to_string(),
            Some(o) => o.detail.clone(),
        };
        Row {
            command,
            contract_version: None,
            bundle_sha256: None,
            lock_contract_version: None,
            lock_bundle_sha256: None,
            side_id: None,
            verdict,
            cause,
            causes,
            drift_class,
            blocking: Cause::class_blocks(drift_class),
            files: FilesCounts::default(),
            wrote_side: false,
            detail,
            files_detail: None,
        }
    }

    /// The row for an aborted evaluation: `internal_error` never combines with
    /// any other cause and always lands INCOMPLETE/4.
    pub fn internal(command: &'static str, detail: impl Into<String>) -> Row {
        Row::from_obs(command, vec![Obs::new(Cause::InternalError, detail.into())])
    }

    /// Exit code of the row (§10.5): 0 OK, 2 FAIL|DRIFT, 4 INCOMPLETE.
    pub fn exit_code(&self) -> i32 {
        match self.verdict {
            "OK" => crate::taxonomy::EXIT_OK,
            "INCOMPLETE" => crate::taxonomy::EXIT_INCOMPLETE_INTERNAL,
            _ => crate::taxonomy::EXIT_FAIL,
        }
    }

    /// The row as a canonical JSON value (keys sorted at every level).
    pub fn to_value(&self) -> Value {
        let mut row = json!({
            "schema": VERDICT_SCHEMA,
            "schema_version": VERDICT_SCHEMA_VERSION,
            "taxonomy_version": TAXONOMY_VERSION,
            "tool_version": env!("CARGO_PKG_VERSION"),
            "command": self.command,
            "contract_version": self.contract_version,
            "bundle_sha256": self.bundle_sha256,
            "lock_contract_version": self.lock_contract_version,
            "lock_bundle_sha256": self.lock_bundle_sha256,
            "side_id": self.side_id,
            "verdict": self.verdict,
            "cause": self.cause,
            "causes": self.causes,
            "drift_class": self.drift_class,
            "blocking": self.blocking,
            "files": {
                "checked": self.files.checked,
                "drifted": self.files.drifted,
                "missing": self.files.missing,
                "unexpected": self.files.unexpected,
            },
            "wrote_side": self.wrote_side,
            "detail": self.detail,
        });
        if let Some(files_detail) = &self.files_detail {
            row["files_detail"] = Value::Array(files_detail.clone());
        }
        row
    }

    /// Exactly one compact canonical JSON line plus a final `\n`.
    pub fn to_json_line(&self) -> String {
        let mut line = serde_json::to_string(&self.to_value()).expect("row serializes");
        line.push('\n');
        line
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_obs_is_ok_without_cause() {
        let row = Row::from_obs("check", vec![]);
        assert_eq!(row.verdict, "OK");
        assert_eq!(row.cause, None);
        assert!(row.causes.is_empty());
        assert_eq!(row.drift_class, "none");
        assert!(!row.blocking);
        assert_eq!(row.exit_code(), 0);
    }

    #[test]
    fn primary_is_the_first_by_precedence() {
        let obs = vec![
            Obs::new(Cause::FileDrift, "bytes distintos en x"),
            Obs::new(Cause::LockStale, "lock viejo"),
            Obs::new(Cause::FileMissing, "falta y"),
        ];
        let row = Row::from_obs("check", obs);
        assert_eq!(row.cause, Some("lock_stale"));
        assert_eq!(row.causes, vec!["lock_stale", "file_missing", "file_drift"]);
        assert_eq!(row.verdict, "DRIFT");
        assert_eq!(row.exit_code(), 2);
        assert_eq!(row.detail, "lock viejo");
    }

    #[test]
    fn classes_drive_blocking() {
        let mut row = Row::from_obs(
            "check",
            vec![Obs::with_class(Cause::LockStale, "patch", "d")],
        );
        assert!(!row.blocking);
        row = Row::from_obs(
            "check",
            vec![Obs::with_class(Cause::LockStale, "major", "d")],
        );
        assert!(row.blocking);
        row = Row::from_obs("check", vec![Obs::new(Cause::FileDrift, "d")]);
        assert!(row.blocking);
        assert_eq!(row.drift_class, "tamper");
    }

    #[test]
    fn internal_row_is_incomplete_4_and_alone() {
        let row = Row::internal("sync", "error de IO (PermissionDenied)");
        assert_eq!(row.verdict, "INCOMPLETE");
        assert_eq!(row.cause, Some("internal_error"));
        assert_eq!(row.causes, vec!["internal_error"]);
        assert_eq!(row.exit_code(), 4);
        assert!(row.blocking);
    }

    #[test]
    fn json_line_is_canonical_compact_and_newline_terminated() {
        let mut row = Row::from_obs("verify", vec![]);
        row.contract_version = Some("1.0.0".to_string());
        row.bundle_sha256 = Some("a".repeat(64));
        row.files = FilesCounts {
            checked: 10,
            drifted: 0,
            missing: 0,
            unexpected: 0,
        };
        row.detail = "10 files verificados".to_string();
        let line = row.to_json_line();
        assert!(line.ends_with('\n'));
        assert!(!line.ends_with("\n\n"));
        assert!(!line.contains(": "), "no insignificant spaces: {line}");
        assert!(!line.contains("/\\u"), "slash never escaped");
        // one final newline, parseable, and byte-stable across serializations
        let body = &line[..line.len() - 1];
        let value: Value = serde_json::from_str(body).expect("parses");
        assert_eq!(serde_json::to_string(&value).unwrap(), body);
        // lexicographic key order at the top level
        let keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(|k| k.as_str())
            .collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        assert_eq!(keys, sorted);
        // every key of §10.2 is always present
        for key in [
            "schema",
            "schema_version",
            "taxonomy_version",
            "tool_version",
            "command",
            "contract_version",
            "bundle_sha256",
            "lock_contract_version",
            "lock_bundle_sha256",
            "side_id",
            "verdict",
            "cause",
            "causes",
            "drift_class",
            "blocking",
            "files",
            "wrote_side",
            "detail",
        ] {
            assert!(
                value.as_object().unwrap().contains_key(key),
                "missing {key}"
            );
        }
        assert!(!value.as_object().unwrap().contains_key("ts"), "no ts ever");
        let files = value.get("files").unwrap().as_object().unwrap();
        assert_eq!(
            files.keys().collect::<Vec<_>>(),
            ["checked", "drifted", "missing", "unexpected"]
        );
    }

    #[test]
    fn no_ts_host_or_path_in_detail_is_the_callers_contract() {
        // The row itself carries no clock/host fields; only `detail` is free
        // text and every producer in `run.rs` keeps it path-relative.
        let line = Row::internal("generate", "error de IO (NotFound)").to_json_line();
        assert!(!line.contains("\"ts\""));
        assert!(!line.contains("/home/"));
        assert!(!line.contains("/tmp/"));
    }
}

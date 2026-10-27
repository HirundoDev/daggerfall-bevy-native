//! Closed, versioned drift taxonomy of `side-contract` (DESIGN-v1 §10.3).
//!
//! The list is **closed**: a cause that is not in [`Cause::ALL`] cannot be
//! emitted, and adding a cause is a taxonomy version bump. Precedence
//! (P1..P11) decides which cause is *primary* when several fire; `internal_error`
//! never combines with anything — it aborts at its origin.
//!
//! Verdict/exit pairing follows §10.5: `0` OK, `1` usage (never a row),
//! `2` FAIL|DRIFT, `3` reserved and never emitted, `4` INCOMPLETE.

/// Version of the closed drift cause list. Bump on any add/remove/rename.
pub const TAXONOMY_VERSION: &str = "1";

/// `schema` discriminator of a verdict row.
pub const VERDICT_SCHEMA: &str = "side-contract-verdict";

/// Version of the verdict row schema. Bump on any field change.
pub const VERDICT_SCHEMA_VERSION: &str = "1.0";

/// Exit code: verdict `OK`.
pub const EXIT_OK: i32 = 0;
/// Exit code: CLI usage error (no verdict row is produced).
pub const EXIT_USAGE: i32 = 1;
/// Exit code: verdict `FAIL` or `DRIFT`.
pub const EXIT_FAIL: i32 = 2;
/// Exit code: verdict `INCOMPLETE` caused by `internal_error`.
pub const EXIT_INCOMPLETE_INTERNAL: i32 = 4;

// Exit code `3` is reserved and never emitted, for coherence with the
// promotion gate (`PROMOTION-GATE-v2.md` §Códigos de salida): a stale script
// interpreting `3` must not be able to confuse states across tools.

/// The twelve (and only) causes `side-contract` can emit: 11 drift causes in
/// precedence order plus `internal_error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    /// P1 (verify/check/generate): `CONTRACT.json` absent, non-canonical,
    /// unknown/missing field, `files[]` containing itself, order/type violations.
    BundleManifestInvalid,
    /// P2 (verify/check): digest mismatch, listed file absent, extra file, or
    /// an R1/R2/R3 violation (`\r`, 0 or 2 final `\n`, BOM).
    BundleTampered,
    /// P3 (check): the side has no `contract.lock` (never synchronized).
    LockMissing,
    /// P4 (check): the lock is unreadable TOML, wrong `schema_version` /
    /// `bundle_schema`, missing field, unsorted `files[]` or malformed sha.
    LockInvalid,
    /// P5 (check): lock ahead of the bundle (never downgraded without `--allow-downgrade`).
    LockAhead,
    /// P6 (check): lock older than the bundle; class = SemVer diff.
    LockStale,
    /// P7 (check): a lock path has no bytes under `.side-contract/`.
    FileMissing,
    /// P8 (check): bytes differ from the digest the lock records.
    FileDrift,
    /// P9 (check): a file under `.side-contract/` the lock does not record.
    FileUnexpected,
    /// P10 (generate/verify): a packaged path matches an `[exclusions]` glob.
    BundleRetailViolation,
    /// P11 (generate): an anchor no longer matches, a closed block is
    /// unreadable, or a derived item is not cited in its coverage chapter.
    ContractSourceDrift,
    /// All phases: I/O, permissions or invalid UTF-8 — the tool could not
    /// evaluate (also a broken `--verify-determinism` run). Never combined.
    InternalError,
}

impl Cause {
    /// Every cause of taxonomy v1, in precedence order (P1..P11 + internal).
    pub const ALL: [Cause; 12] = [
        Cause::BundleManifestInvalid,
        Cause::BundleTampered,
        Cause::LockMissing,
        Cause::LockInvalid,
        Cause::LockAhead,
        Cause::LockStale,
        Cause::FileMissing,
        Cause::FileDrift,
        Cause::FileUnexpected,
        Cause::BundleRetailViolation,
        Cause::ContractSourceDrift,
        Cause::InternalError,
    ];

    /// Stable machine id of the cause (snake_case).
    pub fn id(self) -> &'static str {
        match self {
            Cause::BundleManifestInvalid => "bundle_manifest_invalid",
            Cause::BundleTampered => "bundle_tampered",
            Cause::LockMissing => "lock_missing",
            Cause::LockInvalid => "lock_invalid",
            Cause::LockAhead => "lock_ahead",
            Cause::LockStale => "lock_stale",
            Cause::FileMissing => "file_missing",
            Cause::FileDrift => "file_drift",
            Cause::FileUnexpected => "file_unexpected",
            Cause::BundleRetailViolation => "bundle_retail_violation",
            Cause::ContractSourceDrift => "contract_source_drift",
            Cause::InternalError => "internal_error",
        }
    }

    /// Precedence P1..P11 (`internal_error` = 12, but it aborts at origin and
    /// is therefore never combined with the others).
    pub fn precedence(self) -> u8 {
        match self {
            Cause::BundleManifestInvalid => 1,
            Cause::BundleTampered => 2,
            Cause::LockMissing => 3,
            Cause::LockInvalid => 4,
            Cause::LockAhead => 5,
            Cause::LockStale => 6,
            Cause::FileMissing => 7,
            Cause::FileDrift => 8,
            Cause::FileUnexpected => 9,
            Cause::BundleRetailViolation => 10,
            Cause::ContractSourceDrift => 11,
            Cause::InternalError => 12,
        }
    }

    /// Verdict word emitted with this cause (§10.3).
    pub fn verdict(self) -> &'static str {
        match self {
            Cause::InternalError => "INCOMPLETE",
            Cause::LockMissing
            | Cause::LockAhead
            | Cause::LockStale
            | Cause::FileMissing
            | Cause::FileDrift
            | Cause::FileUnexpected => "DRIFT",
            _ => "FAIL",
        }
    }

    /// Exit code emitted with this cause (§10.5).
    pub fn exit_code(self) -> i32 {
        match self {
            Cause::InternalError => EXIT_INCOMPLETE_INTERNAL,
            _ => EXIT_FAIL,
        }
    }

    /// Static `drift_class` of the cause; `lock_stale` is dynamic and the
    /// evaluator always supplies its observed class explicitly.
    pub fn default_class(self) -> &'static str {
        match self {
            Cause::BundleTampered | Cause::FileDrift => "tamper",
            Cause::LockAhead => "major",
            Cause::LockStale => "unknown",
            Cause::InternalError => "unknown",
            _ => "unknown",
        }
    }

    /// `blocking` is false only for `none`, `patch` and `minor` (§10.2).
    pub fn class_blocks(class: &str) -> bool {
        !matches!(class, "none" | "patch" | "minor")
    }

    /// Reverse lookup by stable id (used by `--explain` and the tests).
    pub fn from_id(id: &str) -> Option<Cause> {
        Cause::ALL.iter().copied().find(|c| c.id() == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_has_twelve_unique_ids() {
        let mut ids: Vec<&str> = Cause::ALL.iter().map(|c| c.id()).collect();
        ids.sort_unstable();
        let mut dedup = ids.clone();
        dedup.dedup();
        assert_eq!(ids.len(), 12);
        assert_eq!(ids, dedup);
    }

    #[test]
    fn precedence_is_strictly_increasing_over_all() {
        let prec: Vec<u8> = Cause::ALL.iter().map(|c| c.precedence()).collect();
        let mut sorted = prec.clone();
        sorted.sort_unstable();
        assert_eq!(prec, sorted);
        assert_eq!(prec, (1..=12).collect::<Vec<u8>>());
    }

    #[test]
    fn internal_is_incomplete_exit_4_and_never_combines() {
        assert_eq!(Cause::InternalError.verdict(), "INCOMPLETE");
        assert_eq!(Cause::InternalError.exit_code(), 4);
        for cause in Cause::ALL {
            if cause != Cause::InternalError {
                assert_ne!(cause.exit_code(), 4);
                assert_ne!(cause.verdict(), "INCOMPLETE");
            }
        }
    }

    #[test]
    fn drift_causes_exit_2_and_ok_never_is_a_cause() {
        for cause in Cause::ALL {
            if cause != Cause::InternalError {
                assert_eq!(cause.exit_code(), 2, "{}", cause.id());
            }
        }
        assert!(Cause::from_id("lock_stale").is_some());
        assert!(Cause::from_id("no_existe").is_none());
    }

    #[test]
    fn blocking_rule() {
        assert!(!Cause::class_blocks("none"));
        assert!(!Cause::class_blocks("patch"));
        assert!(!Cause::class_blocks("minor"));
        assert!(Cause::class_blocks("major"));
        assert!(Cause::class_blocks("tamper"));
        assert!(Cause::class_blocks("unknown"));
    }
}

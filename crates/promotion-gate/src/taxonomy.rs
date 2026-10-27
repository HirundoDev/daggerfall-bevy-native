//! Closed, versioned cause taxonomy of the promotion gate (plan
//! `I1-INFRA-1-TRUST-DUAL-SIDE-PROJECTS.md` §4.3, §9 F4).
//!
//! The list is **closed**: a cause that is not in [`Cause::ALL`] cannot be
//! emitted, and adding a cause is a taxonomy version bump. Every cause names
//! the §4.3 step that can produce it, so a verdict row always says *where* the
//! evaluation stopped.
//!
//! ## History
//!
//! * **v1** (F4-A): 16 causes; step 6 had exactly one entry,
//!   `preview_not_implemented`, because the preview did not exist yet.
//! * **v2** (F4-B): step 6 is implemented, so `preview_not_implemented` is
//!   **retired** (unreachable, not renamed silently) and replaced by the four
//!   causes the real preview can produce: `preview_failed`,
//!   `preview_timeout`, `runtime_status_unavailable` and `main_baseline_failed`.
//!   19 causes, one fixture each.
//! * **v3** (F6-D): step 0 (DESIGN-v1 §12.1) validates `<side>/contract.lock`
//!   against the main's published releases before step 1, adding the four
//!   `contract_*` causes. 23 causes, one fixture each.

/// Version of the closed cause list. Bump on any add/remove/rename.
pub const TAXONOMY_VERSION: &str = "3";

/// `schema` discriminator of a verdict row (stdout and ledger are the same object).
pub const VERDICT_SCHEMA: &str = "promotion-verdict";

/// Version of the verdict row schema. Bump on any field change.
///
/// 1.1 → 1.2 (F6-D): adds the `contract` evidence block (step 0,
/// DESIGN-v1 §12.2 `{lock_version, bundle_sha256, drift_class}`).
pub const VERDICT_SCHEMA_VERSION: &str = "1.2";

/// Exit code: verdict `PASS`.
pub const EXIT_PASS: i32 = 0;
/// Exit code: CLI usage error (no verdict row is produced).
pub const EXIT_USAGE: i32 = 1;
/// Exit code: verdict `FAIL` (side rejected, typed cause recorded).
pub const EXIT_FAIL: i32 = 2;
/// Exit code: verdict `INCOMPLETE` caused by `gate_internal_error` (the gate could not evaluate).
pub const EXIT_INCOMPLETE_INTERNAL: i32 = 4;

// Exit code `3` is **retired** in taxonomy v2: it meant
// `preview_not_implemented` (F4-A) and no verdict emits it any more. It is
// deliberately not reused, so a stale script that treats `3` as "preview
// pending" cannot mistake a v2 failure for it.

/// The twenty-three (and only) causes the gate can emit, taxonomy v3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    /// Step 0: `contract.lock` absent, unreadable, or not a proof this gate can read (DESIGN-v1 §12.1).
    ContractLockMissing,
    /// Step 0: the lock's `contract_version` differs in **major** from the main's published contract.
    ContractStale,
    /// Step 0: the lock's `bundle_sha256` is not in any release record of the main.
    ContractUnknown,
    /// Step 0: the side's `.side-contract/` copy no longer matches the lock (`bundle_tampered`/`file_drift`).
    ContractTampered,
    /// Step 1: `side-project.toml` unreadable, unparseable or violating a §2 validation.
    ManifestInvalid,
    /// Step 1: manifest valid but `readiness.state != "ready"`.
    SideNotReady,
    /// Step 2: `compat.core` does not contain the current `game-core` version.
    CoreIncompatible,
    /// Step 2: `compat.bevy` differs from the main workspace Bevy version.
    BevyMismatch,
    /// Step 3: `readiness-report.md` missing a mandatory §4.2 section (or empty risk section).
    ReadinessSectionMissing,
    /// Step 3: the OD-8 (b) machine-readable probe block is absent or unreadable.
    ReadinessMachineBlockMissing,
    /// Step 4: a probe in `probes.required` has no `probes.commands` entry, or its `argv[0]` cannot be spawned.
    ProbeCommandUndefined,
    /// Step 4: the seed contract is incomplete (argv without `{seed}`, or block entry without `seed`).
    ProbeSeedContractMissing,
    /// Step 4: a probe run exceeded its `timeout_secs`.
    ProbeTimeout,
    /// Step 4: a probe run exited non-zero.
    ProbeNonzeroExit,
    /// Step 4: the two mandated runs produced different `sha256(stdout)`.
    ProbeNondeterministic,
    /// Step 4: computed `sha256(stdout)` differs from the declared `expected_sha256`.
    ProbeHashMismatch,
    /// Step 4: the machine block does not mirror the manifest (names, argv, runs, digest format).
    ProbeReportMismatch,
    /// Step 5: a C1 row of the side ledger records a write outside its `repo_root`.
    AllowlistViolation,
    /// Step 6: the preview program could not be launched, died before publishing its socket, or never composed.
    PreviewFailed,
    /// Step 6: the preview did not publish its socket and answer `status` inside `preview.timeout_secs`.
    PreviewTimeout,
    /// Step 6: the socket answered, but not with a valid v1 `status` of *this* instance with the side loaded.
    RuntimeStatusUnavailable,
    /// Step 6: the main's baseline surface declared in `main-baseline.toml` differs from `HEAD`.
    MainBaselineFailed,
    /// The gate itself could not establish its baseline (main identity, ledger I/O).
    GateInternalError,
}

impl Cause {
    /// Every cause of taxonomy v3, in step order.
    pub const ALL: [Cause; 23] = [
        Cause::ContractLockMissing,
        Cause::ContractStale,
        Cause::ContractUnknown,
        Cause::ContractTampered,
        Cause::ManifestInvalid,
        Cause::SideNotReady,
        Cause::CoreIncompatible,
        Cause::BevyMismatch,
        Cause::ReadinessSectionMissing,
        Cause::ReadinessMachineBlockMissing,
        Cause::ProbeCommandUndefined,
        Cause::ProbeSeedContractMissing,
        Cause::ProbeTimeout,
        Cause::ProbeNonzeroExit,
        Cause::ProbeNondeterministic,
        Cause::ProbeHashMismatch,
        Cause::ProbeReportMismatch,
        Cause::AllowlistViolation,
        Cause::PreviewFailed,
        Cause::PreviewTimeout,
        Cause::RuntimeStatusUnavailable,
        Cause::MainBaselineFailed,
        Cause::GateInternalError,
    ];

    /// Stable machine id of the cause (snake_case, as listed in plan/§4.3 orders).
    pub fn id(self) -> &'static str {
        match self {
            Cause::ContractLockMissing => "contract_lock_missing",
            Cause::ContractStale => "contract_stale",
            Cause::ContractUnknown => "contract_unknown",
            Cause::ContractTampered => "contract_tampered",
            Cause::ManifestInvalid => "manifest_invalid",
            Cause::SideNotReady => "side_not_ready",
            Cause::CoreIncompatible => "core_incompatible",
            Cause::BevyMismatch => "bevy_mismatch",
            Cause::ReadinessSectionMissing => "readiness_section_missing",
            Cause::ReadinessMachineBlockMissing => "readiness_machine_block_missing",
            Cause::ProbeCommandUndefined => "probe_command_undefined",
            Cause::ProbeSeedContractMissing => "probe_seed_contract_missing",
            Cause::ProbeTimeout => "probe_timeout",
            Cause::ProbeNonzeroExit => "probe_nonzero_exit",
            Cause::ProbeNondeterministic => "probe_nondeterministic",
            Cause::ProbeHashMismatch => "probe_hash_mismatch",
            Cause::ProbeReportMismatch => "probe_report_mismatch",
            Cause::AllowlistViolation => "allowlist_violation",
            Cause::PreviewFailed => "preview_failed",
            Cause::PreviewTimeout => "preview_timeout",
            Cause::RuntimeStatusUnavailable => "runtime_status_unavailable",
            Cause::MainBaselineFailed => "main_baseline_failed",
            Cause::GateInternalError => "gate_internal_error",
        }
    }

    /// The §4.3 step that emits the cause; `None` for gate-internal failures
    /// (they are not attributable to a step of the side).
    pub fn step(self) -> Option<u8> {
        match self {
            Cause::ContractLockMissing
            | Cause::ContractStale
            | Cause::ContractUnknown
            | Cause::ContractTampered => Some(0),
            Cause::ManifestInvalid | Cause::SideNotReady => Some(1),
            Cause::CoreIncompatible | Cause::BevyMismatch => Some(2),
            Cause::ReadinessSectionMissing | Cause::ReadinessMachineBlockMissing => Some(3),
            Cause::ProbeCommandUndefined
            | Cause::ProbeSeedContractMissing
            | Cause::ProbeTimeout
            | Cause::ProbeNonzeroExit
            | Cause::ProbeNondeterministic
            | Cause::ProbeHashMismatch
            | Cause::ProbeReportMismatch => Some(4),
            Cause::AllowlistViolation => Some(5),
            Cause::PreviewFailed
            | Cause::PreviewTimeout
            | Cause::RuntimeStatusUnavailable
            | Cause::MainBaselineFailed => Some(6),
            Cause::GateInternalError => None,
        }
    }

    /// Verdict word emitted with this cause.
    pub fn verdict(self) -> &'static str {
        match self {
            Cause::GateInternalError => "INCOMPLETE",
            _ => "FAIL",
        }
    }

    /// Exit code emitted with this cause.
    pub fn exit_code(self) -> i32 {
        match self {
            Cause::GateInternalError => EXIT_INCOMPLETE_INTERNAL,
            _ => EXIT_FAIL,
        }
    }

    /// Reverse lookup by stable id (used by tests and tooling).
    pub fn from_id(id: &str) -> Option<Cause> {
        Cause::ALL.iter().copied().find(|c| c.id() == id)
    }
}

/// The three verdicts of the gate. `PASS` requires every §4.3 step, including
/// the step-6 preview composition and the main baseline check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerdictKind {
    Pass,
    Fail,
    Incomplete,
}

impl VerdictKind {
    pub fn word(self) -> &'static str {
        match self {
            VerdictKind::Pass => "PASS",
            VerdictKind::Fail => "FAIL",
            VerdictKind::Incomplete => "INCOMPLETE",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taxonomy_ids_are_unique_and_twenty_three() {
        let mut ids: Vec<&str> = Cause::ALL.iter().map(|c| c.id()).collect();
        let before = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(before, 23);
        assert_eq!(ids.len(), 23);
        assert_eq!(before, Cause::ALL.len());
    }

    #[test]
    fn taxonomy_ids_round_trip() {
        for cause in Cause::ALL {
            assert_eq!(Cause::from_id(cause.id()), Some(cause));
        }
        assert_eq!(Cause::from_id("not_a_cause"), None);
        // Retired in v2: no alias, no silent resurrection.
        assert_eq!(Cause::from_id("preview_not_implemented"), None);
    }

    #[test]
    fn step_mapping_matches_plan_4_3() {
        assert_eq!(Cause::ContractLockMissing.step(), Some(0));
        assert_eq!(Cause::ContractStale.step(), Some(0));
        assert_eq!(Cause::ContractUnknown.step(), Some(0));
        assert_eq!(Cause::ContractTampered.step(), Some(0));
        assert_eq!(Cause::ManifestInvalid.step(), Some(1));
        assert_eq!(Cause::SideNotReady.step(), Some(1));
        assert_eq!(Cause::CoreIncompatible.step(), Some(2));
        assert_eq!(Cause::BevyMismatch.step(), Some(2));
        assert_eq!(Cause::ReadinessSectionMissing.step(), Some(3));
        assert_eq!(Cause::ReadinessMachineBlockMissing.step(), Some(3));
        assert_eq!(Cause::ProbeReportMismatch.step(), Some(4));
        assert_eq!(Cause::AllowlistViolation.step(), Some(5));
        assert_eq!(Cause::PreviewFailed.step(), Some(6));
        assert_eq!(Cause::PreviewTimeout.step(), Some(6));
        assert_eq!(Cause::RuntimeStatusUnavailable.step(), Some(6));
        assert_eq!(Cause::MainBaselineFailed.step(), Some(6));
        assert_eq!(Cause::GateInternalError.step(), None);
    }

    #[test]
    fn verdict_and_exit_codes() {
        assert_eq!(Cause::PreviewFailed.verdict(), "FAIL");
        assert_eq!(Cause::PreviewTimeout.verdict(), "FAIL");
        assert_eq!(Cause::RuntimeStatusUnavailable.verdict(), "FAIL");
        assert_eq!(Cause::MainBaselineFailed.verdict(), "FAIL");
        assert_eq!(Cause::GateInternalError.verdict(), "INCOMPLETE");
        assert_eq!(Cause::GateInternalError.exit_code(), EXIT_INCOMPLETE_INTERNAL);
        for cause in Cause::ALL {
            if cause == Cause::GateInternalError {
                assert_eq!(cause.exit_code(), EXIT_INCOMPLETE_INTERNAL);
            } else {
                assert_eq!(cause.verdict(), "FAIL");
                assert_eq!(cause.exit_code(), EXIT_FAIL);
            }
            // No cause produces PASS, and none reuses the retired code 3.
            assert_ne!(cause.exit_code(), EXIT_PASS);
            assert_ne!(cause.exit_code(), 3);
        }
        assert_ne!(EXIT_FAIL, EXIT_INCOMPLETE_INTERNAL);
    }

    #[test]
    fn taxonomy_version_is_three_and_step_six_is_four_causes() {
        assert_eq!(TAXONOMY_VERSION, "3");
        let step_six: Vec<&str> = Cause::ALL
            .iter()
            .filter(|c| c.step() == Some(6))
            .map(|c| c.id())
            .collect();
        assert_eq!(
            step_six,
            vec![
                "preview_failed",
                "preview_timeout",
                "runtime_status_unavailable",
                "main_baseline_failed"
            ]
        );
    }
}

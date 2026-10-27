//! Gate orchestration — plan `I1-INFRA-1-TRUST-DUAL-SIDE-PROJECTS.md` §4.3 +
//! DESIGN-v1 §12.1: **step 0** (contract.lock), steps 1-6 and 7 are executable
//! in one command (F4-B completed step 6; F6-D added step 0).
//!
//! First failure stops (§4.3), and every run — pass or fail — appends exactly
//! one row to the promotion ledger (step 7).
//!
//! Step 6 runs in a fixed order: **6a preview** (the side's declared preview
//! composes and answers the runtime `status`) then **6b main baseline** (the
//! main's declared surface still equals `HEAD`). That order is deliberate: the
//! composition evidence is attached to the row even when the baseline fails, and
//! a preview failure never pays for the baseline read.

use std::path::PathBuf;

use crate::allowlist;
use crate::baseline;
use crate::contract::{self, ContractEvidence};
use crate::manifest::{self, LoadedManifest};
use crate::main_identity::{self, MainIdentity};
use crate::preview;
use crate::probes;
use crate::report;
use crate::taxonomy::Cause;
use crate::verdict::{BaselineEvidence, EvidenceHashes, PreviewEvidence, StepSixEvidence, Verdict};

/// Default ledger location, relative to `main_root` (plan §9 F4 "ledger de
/// promociones"; append-only, written only by the main).
pub const DEFAULT_LEDGER: &str = "docs/sintergic/promotion-gate/promotion-ledger.jsonl";

/// Everything the gate needs to run once.
#[derive(Debug, Clone)]
pub struct GateConfig {
    /// Side workspace holding `side-project.toml`.
    pub side_root: PathBuf,
    /// Main workspace whose identity is the baseline.
    pub main_root: PathBuf,
    /// Append-only promotion ledger (one row per execution).
    pub ledger: PathBuf,
    /// Directory scanned for fleet uniqueness; defaults to the side's parent.
    pub fleet_dir: PathBuf,
}

impl GateConfig {
    pub fn new(side_root: PathBuf, main_root: PathBuf) -> Self {
        let fleet_dir = side_root
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| side_root.clone());
        GateConfig {
            ledger: main_root.join(DEFAULT_LEDGER),
            side_root,
            main_root,
            fleet_dir,
        }
    }
}

/// Result of one gate execution: the row, the human trace (stderr) and whether
/// the row reached the ledger.
#[derive(Debug)]
pub struct RunOutcome {
    pub verdict: Verdict,
    pub trace: Vec<String>,
    pub ledger_error: Option<String>,
}

/// Row-building context: what is known at the point of failure.
#[derive(Debug, Default)]
struct Ctx {
    main_head: Option<String>,
    side_id: Option<String>,
    side_version: Option<String>,
    side_commit: Option<String>,
    contract: Option<ContractEvidence>,
    /// Step-0 non-blocking WARN (lock patch/minor, DESIGN-v1 §12.1): surfaced
    /// in `detail` of whatever row this context builds — the trace keeps it too.
    contract_warn: Option<String>,
    evidence: EvidenceHashes,
}

impl Ctx {
    fn adopt(&mut self, loaded: &LoadedManifest) {
        self.side_id = Some(loaded.manifest.side.id.clone());
        self.side_version = Some(loaded.manifest.side.version.clone());
    }

    /// `detail` + the step-0 WARN when one was observed (§12.1: WARN en `detail`).
    fn with_contract_warn(&self, detail: impl Into<String>) -> String {
        let detail = detail.into();
        match &self.contract_warn {
            Some(warn) => format!("{detail} · WARN step 0: {warn}"),
            None => detail,
        }
    }

    fn verdict(&self, cause: Cause, detail: impl Into<String>) -> Verdict {
        Verdict::from_cause(
            cause,
            self.main_head.clone(),
            self.side_id.clone(),
            self.side_version.clone(),
            self.side_commit.clone(),
            self.with_contract_warn(detail),
            self.evidence.clone(),
        )
        .with_contract(self.contract.clone())
    }

    /// The `PASS` row: impossible to build without both step-6 evidence blocks.
    fn pass(&self, detail: impl Into<String>, step_six: StepSixEvidence) -> Verdict {
        Verdict::pass(
            self.main_head.clone(),
            self.side_id.clone(),
            self.side_version.clone(),
            self.side_commit.clone(),
            self.with_contract_warn(detail),
            self.evidence.clone(),
            step_six,
        )
        .with_contract(self.contract.clone())
    }
}

/// Execute the gate: baseline, step 0 (contract), steps 1-6, then exactly one ledger row (step 7).
pub fn run_gate(cfg: &GateConfig) -> RunOutcome {
    let mut trace: Vec<String> = Vec::new();
    let mut ctx = Ctx::default();

    // Baseline before any step: without it no row is complete.
    let identity: MainIdentity = match main_identity::gather(&cfg.main_root) {
        Ok(id) => {
            trace.push(format!(
                "main: HEAD {} · game-core {} · bevy {}.{}",
                &id.head[..7.min(id.head.len())],
                id.core_version,
                id.bevy.major,
                id.bevy.minor
            ));
            id
        }
        Err(e) => {
            return finish(cfg, ctx.verdict(Cause::GateInternalError, e), trace);
        }
    };
    ctx.main_head = Some(identity.head.clone());
    ctx.side_commit = side_commit(&cfg.side_root);
    trace.push(format!(
        "side: {}",
        ctx.side_commit
            .as_ref()
            .map(|c| format!("commit {}", &c[..7.min(c.len())]))
            .unwrap_or_else(|| "sin .git propio".into())
    ));

    // Step 0 (DESIGN-v1 §12.1): the side's contract.lock still names the
    // contract this main published — before the side's own manifest is read.
    match contract::check(&cfg.side_root, &cfg.main_root) {
        Ok(ok) => {
            let block = &ok.evidence;
            let version = block.lock_version.as_deref().unwrap_or("?");
            let bundle = block.bundle_sha256.as_deref().unwrap_or("?");
            match &ok.warn {
                Some(warn) => trace.push(format!(
                    "step 0: OK — contract.lock v{version} bundle {} (WARN: {warn})",
                    &bundle[..12.min(bundle.len())]
                )),
                None => trace.push(format!(
                    "step 0: OK — contract.lock v{version} bundle {} vigente",
                    &bundle[..12.min(bundle.len())]
                )),
            }
            ctx.contract = Some(ok.evidence);
            ctx.contract_warn = ok.warn;
        }
        Err(failure) => {
            trace.push(format!("step 0: FAIL — {}", failure.detail));
            let evidence = Some(failure.evidence);
            return finish(
                cfg,
                ctx.verdict(failure.cause, failure.detail)
                    .with_contract(evidence),
                trace,
            );
        }
    }

    // Step 1: manifest valid (§2) and readiness.state == ready (§4.3.1).
    let loaded = match manifest::parse(&cfg.side_root) {
        Ok(loaded) => {
            ctx.evidence.manifest_sha256 = Some(probes::sha256_hex(&loaded.raw));
            ctx.adopt(&loaded);
            loaded
        }
        Err(e) => {
            trace.push(format!("step 1: FAIL — {e}"));
            return finish(cfg, ctx.verdict(Cause::ManifestInvalid, e), trace);
        }
    };
    if let Err(e) = manifest::validate(&loaded, &cfg.side_root, &cfg.fleet_dir) {
        trace.push(format!("step 1: FAIL — {e}"));
        return finish(cfg, ctx.verdict(Cause::ManifestInvalid, e), trace);
    }
    if let Err(e) = manifest::require_ready(&loaded) {
        trace.push(format!("step 1: FAIL — {e}"));
        return finish(cfg, ctx.verdict(Cause::SideNotReady, e), trace);
    }
    trace.push("step 1: OK — manifest schema v1.3 válido y readiness.state=ready".into());

    // Step 2: compat pins (§4.3.2).
    if let Err((cause, e)) = manifest::check_compat(&loaded, &identity) {
        trace.push(format!("step 2: FAIL — {e}"));
        return finish(cfg, ctx.verdict(cause, e), trace);
    }
    trace.push(format!(
        "step 2: OK — compat.core contiene {} y compat.bevy coincide con el main",
        identity.core_version
    ));

    // Step 3: report with every §4.2 section + the OD-8 (b) machine block (§4.3.3).
    let report = match report::load(&cfg.side_root, &loaded.manifest.readiness.report) {
        Ok(text) => {
            ctx.evidence.report_sha256 = Some(probes::sha256_hex(text.as_bytes()));
            text
        }
        Err(e) => {
            trace.push(format!("step 3: FAIL — {e}"));
            return finish(cfg, ctx.verdict(Cause::ReadinessSectionMissing, e), trace);
        }
    };
    if let Err(e) = report::check_sections(&report) {
        trace.push(format!("step 3: FAIL — {e}"));
        return finish(cfg, ctx.verdict(Cause::ReadinessSectionMissing, e), trace);
    }
    let block = match report::machine_block(&report) {
        Ok(block) => block,
        Err(e) => {
            trace.push(format!("step 3: FAIL — {e}"));
            return finish(
                cfg,
                ctx.verdict(Cause::ReadinessMachineBlockMissing, e),
                trace,
            );
        }
    };
    trace.push("step 3: OK — 8 secciones §4.2 + bloque machine-readable".into());

    // Step 4: declared probes exist, run twice, same hash (§4.3.4).
    match probes::run_all(&loaded.manifest, &block, &cfg.side_root) {
        Ok(hashes) => {
            for (name, hash) in &hashes {
                trace.push(format!("step 4: OK — {name} -> {hash}"));
            }
            ctx.evidence.probes = hashes;
        }
        Err(failure) => {
            trace.push(format!("step 4: FAIL — {}", failure.detail));
            return finish(cfg, ctx.verdict(failure.cause, failure.detail), trace);
        }
    }

    // Step 5: 0 allowlist violations in the side's C1 ledger (§4.3.5, §6.1-6.2).
    match allowlist::check(&cfg.side_root) {
        Ok(violations) if violations.is_empty() => {
            trace.push("step 5: OK — 0 violaciones de allowlist".into());
        }
        Ok(violations) => {
            let detail = format!(
                "{} violación(es) de allowlist: {}",
                violations.len(),
                violations.join("; ")
            );
            trace.push(format!("step 5: FAIL — {detail}"));
            return finish(cfg, ctx.verdict(Cause::AllowlistViolation, detail), trace);
        }
        Err(e) => {
            // Unreadable audit trail is an unauditable write: fail closed.
            let detail = format!("ledger C1 ilegible: {e}");
            trace.push(format!("step 5: FAIL — {detail}"));
            return finish(cfg, ctx.verdict(Cause::AllowlistViolation, detail), trace);
        }
    }

    // Step 6a: the declared preview composes — argv (never a shell), a private
    // socket, and a v1 `status` of *this* instance with the side loaded.
    let preview_evidence: PreviewEvidence =
        match preview::run(&loaded.manifest.preview, &cfg.side_root, &loaded.manifest.side.id) {
            Ok(evidence) => {
                trace.push(format!(
                    "step 6a: OK — preview {} compuesto en {} ms (instance {} · {} side(s))",
                    loaded.manifest.preview.argv[0],
                    evidence.startup_ms,
                    evidence.instance,
                    evidence.loaded_sides.len()
                ));
                evidence
            }
            Err(failure) => {
                trace.push(format!("step 6a: FAIL — {}", failure.detail));
                return finish(cfg, ctx.verdict(failure.cause, failure.detail), trace);
            }
        };

    // Step 6b: the main's baseline surface still equals HEAD.
    let baseline_evidence: BaselineEvidence = match baseline::check(&cfg.main_root) {
        Ok(evidence) => {
            trace.push(format!(
                "step 6b: OK — baseline del main intacto ({} archivo(s), surface {})",
                evidence.files.len(),
                &evidence.surface_sha256[..12.min(evidence.surface_sha256.len())]
            ));
            evidence
        }
        Err(detail) => {
            trace.push(format!("step 6b: FAIL — {detail}"));
            return finish(
                cfg,
                ctx.verdict(Cause::MainBaselineFailed, detail)
                    .with_preview(preview_evidence),
                trace,
            );
        }
    };

    // Steps 1-6 passed: the only path to PASS, and it carries both blocks.
    let detail = format!(
        "pasos 0-6 OK: contract.lock vigente, manifest v1.3, compat, report, \
probes deterministas, allowlist, preview compuesto ({}) y baseline del main intacto",
        loaded.manifest.preview.argv[0]
    );
    finish(
        cfg,
        ctx.pass(detail, StepSixEvidence { preview: preview_evidence, main_baseline: baseline_evidence }),
        trace,
    )
}

/// Side commit: only the side's *own* `.git` counts, so a side nested inside
/// another checkout cannot borrow that checkout's HEAD.
fn side_commit(side_root: &std::path::Path) -> Option<String> {
    if !side_root.join(".git").exists() {
        return None;
    }
    let out = std::process::Command::new("git")
        .args(["-C", &side_root.display().to_string(), "rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let head = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!head.is_empty()).then_some(head)
}

/// Step 7: append exactly one row; a ledger that cannot be written is
/// reported on stdout (`ledger_written=false`) instead of silently dropped.
fn finish(cfg: &GateConfig, mut verdict: Verdict, mut trace: Vec<String>) -> RunOutcome {
    trace.push(format!("ledger: {}", cfg.ledger.display()));
    let ledger_error = match verdict.append_to(&cfg.ledger) {
        Ok(()) => {
            trace.push("step 7: OK — 1 fila append-only en el ledger".into());
            None
        }
        Err(e) => {
            verdict.ledger_written = false;
            trace.push(format!("step 7: ERROR — no se pudo escribir el ledger: {e}"));
            Some(e)
        }
    };
    RunOutcome { verdict, trace, ledger_error }
}

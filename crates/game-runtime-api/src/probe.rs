//! The diagnostic probe catalogue.
//!
//! `probes.run` is the only diagnostic entry point and it takes a **name**.
//! The wire type system has no command, path or argument parameter anywhere,
//! and the name grammar (`^[A-Z][A-Z0-9_]{0,15}-[0-9]{3}$`, the shape the main's
//! `--probe` catalogue already uses) makes `/bin/sh`, `../bin/sh`, `bash -c id`
//! and even a trailing space unrepresentable. Registering and running are
//! separate privileges: the embedding binary owns the registry.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::error::{RpcError, RuntimeError};

/// Largest accepted prefix of a probe name, in bytes.
pub const MAX_PROBE_PREFIX: usize = 16;

/// A validated probe name such as `ALL-001` or `SKILLS-001`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProbeName(String);

impl ProbeName {
    /// Parses `^[A-Z][A-Z0-9_]{0,15}-[0-9]{3}$`.
    ///
    /// Any violation - including a lowercase prefix, whitespace, a slash, an
    /// absolute path or a shell snippet - is [`RpcError::InvalidProbeName`].
    pub fn parse(raw: &str) -> Result<Self, RpcError> {
        match ProbeName::validate(raw) {
            Ok(()) => Ok(ProbeName(raw.to_string())),
            Err(reason) => Err(RpcError::InvalidProbeName {
                raw: raw.to_string(),
                reason,
            }),
        }
    }

    /// The canonical text of the name.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn validate(raw: &str) -> Result<(), &'static str> {
        let (prefix, digits) = raw
            .split_once('-')
            .ok_or("probe name must be `<PREFIX>-<NNN>`")?;
        if digits.len() != 3 || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err("probe name suffix must be exactly three ASCII digits");
        }
        let prefix_bytes = prefix.as_bytes();
        if prefix_bytes.is_empty() || prefix_bytes.len() > MAX_PROBE_PREFIX {
            return Err("probe name prefix must be 1..=16 characters");
        }
        if !prefix_bytes[0].is_ascii_uppercase() {
            return Err("probe name prefix must start with an uppercase letter");
        }
        if !prefix_bytes
            .iter()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || *b == b'_')
        {
            return Err("probe name prefix may only contain A-Z, 0-9 and _");
        }
        Ok(())
    }
}

impl fmt::Display for ProbeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The three verdicts a probe can report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeStatus {
    /// The invariant holds.
    Pass,
    /// The invariant is violated.
    Fail,
    /// The probe could not run (environment absent, precondition unmet).
    Skip,
}

impl ProbeStatus {
    /// Stable lowercase token used on the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            ProbeStatus::Pass => "pass",
            ProbeStatus::Fail => "fail",
            ProbeStatus::Skip => "skip",
        }
    }
}

impl fmt::Display for ProbeStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A probe verdict plus its deterministic, human-readable detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeOutcome {
    /// The verdict.
    pub status: ProbeStatus,
    /// What the probe observed.
    pub detail: String,
}

impl ProbeOutcome {
    /// Builds an outcome from a verdict and a detail string.
    pub fn new(status: ProbeStatus, detail: impl Into<String>) -> Self {
        ProbeOutcome {
            status,
            detail: detail.into(),
        }
    }

    /// A `pass` outcome.
    pub fn pass(detail: impl Into<String>) -> Self {
        ProbeOutcome::new(ProbeStatus::Pass, detail)
    }

    /// A `fail` outcome.
    pub fn fail(detail: impl Into<String>) -> Self {
        ProbeOutcome::new(ProbeStatus::Fail, detail)
    }

    /// A `skip` outcome.
    pub fn skip(detail: impl Into<String>) -> Self {
        ProbeOutcome::new(ProbeStatus::Skip, detail)
    }
}

/// What `probes.list` publishes about one registered probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeSpec {
    /// The validated name.
    pub name: ProbeName,
    /// One-line description of the invariant the probe checks.
    pub description: String,
}

/// A registered probe body: deterministic, synchronous and side-effect free.
pub type ProbeFn = Arc<dyn Fn() -> ProbeOutcome + Send + Sync + 'static>;

struct Entry {
    description: String,
    run: ProbeFn,
}

/// The name-to-function catalogue behind `probes.list` and `probes.run`.
///
/// Names are unique and sorted; registration is the embedding binary's job,
/// the wire can only look names up. The lock is never held while a probe body
/// runs.
pub struct ProbeRegistry {
    entries: Mutex<BTreeMap<String, Entry>>,
}

impl ProbeRegistry {
    /// Creates an empty catalogue.
    pub fn new() -> Self {
        ProbeRegistry {
            entries: Mutex::new(BTreeMap::new()),
        }
    }

    /// Registers a probe under a validated name.
    ///
    /// A name that violates the grammar is [`RuntimeError::InvalidProbeName`];
    /// a second registration of the same name is
    /// [`RuntimeError::DuplicateProbe`] - the catalogue never silently replaces
    /// what an operator already saw in `probes.list`.
    pub fn register(
        &self,
        name: &str,
        description: &str,
        run: ProbeFn,
    ) -> Result<(), RuntimeError> {
        let parsed = match ProbeName::parse(name) {
            Ok(parsed) => parsed,
            Err(RpcError::InvalidProbeName { raw, reason }) => {
                return Err(RuntimeError::InvalidProbeName { raw, reason })
            }
            Err(other) => {
                return Err(RuntimeError::InvalidProbeName {
                    raw: name.to_string(),
                    reason: other.code(),
                })
            }
        };
        let mut entries = self.lock();
        if entries.contains_key(parsed.as_str()) {
            return Err(RuntimeError::DuplicateProbe {
                name: parsed.as_str().to_string(),
            });
        }
        entries.insert(
            parsed.as_str().to_string(),
            Entry {
                description: description.to_string(),
                run,
            },
        );
        Ok(())
    }

    /// Every registered probe, sorted by name.
    pub fn list(&self) -> Vec<ProbeSpec> {
        self.lock()
            .iter()
            .map(|(name, entry)| ProbeSpec {
                name: ProbeName(name.clone()),
                description: entry.description.clone(),
            })
            .collect()
    }

    /// Runs a registered probe by exact name.
    ///
    /// The grammar is checked first, so a path-like name is always
    /// [`RpcError::InvalidProbeName`] and never a lookup; a well-formed but
    /// unregistered name is [`RpcError::ProbeUnknown`].
    pub fn run(&self, name: &str) -> Result<ProbeOutcome, RpcError> {
        let parsed = ProbeName::parse(name)?;
        let probe = {
            let entries = self.lock();
            entries.get(parsed.as_str()).map(|entry| entry.run.clone())
        };
        match probe {
            Some(run) => Ok(run()),
            None => Err(RpcError::ProbeUnknown {
                name: parsed.as_str().to_string(),
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, BTreeMap<String, Entry>> {
        // A panic in a probe body must not take the catalogue down with it: the
        // map itself is consistent, so recovering from poisoning is safe here.
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Default for ProbeRegistry {
    fn default() -> Self {
        ProbeRegistry::new()
    }
}

impl fmt::Debug for ProbeRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProbeRegistry")
            .field("names", &self.list().iter().map(|s| s.name.to_string()).collect::<Vec<_>>())
            .finish()
    }
}

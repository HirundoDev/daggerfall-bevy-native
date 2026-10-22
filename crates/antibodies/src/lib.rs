//! Antibodies organ — invariants and guardrails (F0 scaffold).
//!
//! Placeholder until runtime guards land (e.g. "no Bethesda bytes tracked
//! by git" is enforced today by `.gitignore` + the negative verification in
//! DATA-CLASSIFICATION.md).

/// Canonical organ id (Sintergic organ map, ADR-FW-104).
pub const ORGAN: &str = "antibodies";

/// Human-readable organ name.
pub fn organ_name() -> &'static str {
    ORGAN
}

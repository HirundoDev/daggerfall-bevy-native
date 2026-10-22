//! `BsaSource` — opening of the .BSA file through the parser's public API.
//!
//! Contract PRE-2 §8: the `BsaSource` interface (in, parser→bridge) is the
//! .BSA file opened by `daggerfall_parser_rs` — inventory custody,
//! only under ModeState=A. The bridge does NOT decide the mode (rule 1): the caller
//! (app/perception) resolves `ModeState` and hands over the already-custodied bytes.
//!
//! DAG: `app → bridge → daggerfall_parser_rs` — this module is the ONLY
//! gate of the open-parse-convert flow; the app does not depend on the
//! parser directly (AB-11, PRE-2 §8).

pub use daggerfall_parser_rs::bsa::{entry_data, parse as parse_bsa, BsaArchive, BsaError, Entry};

use crate::convert::{mesh_record_to_spec, ConvertError};
use crate::mesh_spec::MeshSpec;

/// Error of [`mesh_spec_at`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MeshSpecAtError {
    /// Record `index` exceeds the BSA buffer (delegated from the parser).
    Bsa(BsaError),
    /// The record does not convert to visible geometry (delegated from the converter).
    Convert(ConvertError),
}

impl std::fmt::Display for MeshSpecAtError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bsa(e) => write!(f, "record outside the BSA: {e}"),
            Self::Convert(e) => write!(f, "conversion failed: {e}"),
        }
    }
}

impl std::error::Error for MeshSpecAtError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Bsa(e) => Some(e),
            Self::Convert(e) => Some(e),
        }
    }
}

impl From<BsaError> for MeshSpecAtError {
    fn from(e: BsaError) -> Self {
        Self::Bsa(e)
    }
}

impl From<ConvertError> for MeshSpecAtError {
    fn from(e: ConvertError) -> Self {
        Self::Convert(e)
    }
}

/// Reads record `index` (directory order) and converts it into a [`MeshSpec`].
pub fn mesh_spec_at(
    archive: &BsaArchive,
    data: &[u8],
    index: usize,
) -> Result<MeshSpec, MeshSpecAtError> {
    let record = entry_data(archive, data, index)?;
    Ok(mesh_record_to_spec(record)?)
}

/// First `max` records of ARCH3D.BSA that convert to visible geometry
/// (≥1 triangle) **and are solid** ([`crate::city_spec::landmark_is_solid`]), in
/// directory order — deterministic selection of the city landmarks
/// (sha-pinned S1 input; M3/M4/M5/M6 + the runtime test `probe_city_001`).
///
/// **SINGLE shared function** for the CLI (`convertible_blocks` in
/// `crates/app/src/main.rs` → `--probe CITY-001/INTERIOR-001/...`) and the
/// test `crates/app/tests/probe_city_001_runtime.rs`: one implementation
/// both consume, with no divergent duplicated logic (guardrail
/// §5a.2). Returns `(record_id, spec)` so the caller can keep the
/// evidence of the selected ids.
///
/// The solidity filter exists because the M3 checks (block collision +
/// flank readability) are calibrated with CLOSED landmarks: the first
/// convertible (`44005`) has an open 11 m passage along the `+X` axis at
/// `z=40` that the flank ray and the `r=0.3` march thread through
/// (HH-1 evidence `m10-yneg-partial`); 0 relaxation of checks.
pub fn solid_convertible_blocks(
    data: &[u8],
    max: usize,
) -> Result<Vec<(u32, MeshSpec)>, String> {
    let archive = parse_bsa(data).map_err(|e| format!("invalid ARCH3D.BSA: {e}"))?;
    let mut solid = Vec::new();
    for (index, entry) in archive.entries.iter().enumerate() {
        if solid.len() >= max {
            break;
        }
        let Entry::Numbered { record_id, .. } = entry else {
            continue;
        };
        let Ok(spec) = mesh_spec_at(&archive, data, index) else {
            continue;
        };
        if spec.triangle_count() > 0 && crate::city_spec::landmark_is_solid(&spec) {
            solid.push((*record_id, spec));
        }
    }
    if solid.is_empty() {
        return Err(
            "no ARCH3D.BSA record converts to visible solid geometry".into(),
        );
    }
    Ok(solid)
}

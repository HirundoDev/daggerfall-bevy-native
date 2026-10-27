//! Strict `M.m.p` SemVer parsing and diff classification (DESIGN-v1 §9.2),
//! implemented here because the crate is forbidden to depend on `semver`.
//!
//! "Strict" means: exactly three dot-separated numeric components, ASCII
//! digits only, no leading zeros (`0` alone is fine), no pre-release, no build
//! metadata. Anything else is rejected by the caller (`bundle_manifest_invalid`
//! in a manifest, `lock_invalid` in a lock, `internal_error` in `contract.toml`).

use std::cmp::Ordering;

/// A strict `major.minor.patch` triple.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemVer {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

/// How two contract versions differ (§9.2 bump classes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionDiff {
    /// Same version: no drift.
    None,
    /// Same major and minor, different patch.
    Patch,
    /// Same major, different minor.
    Minor,
    /// Different major.
    Major,
}

impl VersionDiff {
    /// `drift_class` of a `lock_stale` observation.
    pub fn class(self) -> &'static str {
        match self {
            VersionDiff::None => "none",
            VersionDiff::Patch => "patch",
            VersionDiff::Minor => "minor",
            VersionDiff::Major => "major",
        }
    }
}

fn parse_component(raw: &str) -> Option<u64> {
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if raw.len() > 1 && raw.starts_with('0') {
        return None; // strict SemVer forbids leading zeros
    }
    raw.parse::<u64>().ok()
}

impl SemVer {
    /// Parses a strict `M.m.p` string; `None` for anything else.
    pub fn parse(raw: &str) -> Option<SemVer> {
        let mut parts = raw.split('.');
        let major = parse_component(parts.next()?)?;
        let minor = parse_component(parts.next()?)?;
        let patch = parse_component(parts.next()?)?;
        if parts.next().is_some() {
            return None;
        }
        Some(SemVer {
            major,
            minor,
            patch,
        })
    }

    /// The dotted form (round-trips with [`SemVer::parse`]).
    pub fn to_string_version(self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl PartialOrd for SemVer {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SemVer {
    fn cmp(&self, other: &Self) -> Ordering {
        self.major
            .cmp(&other.major)
            .then(self.minor.cmp(&other.minor))
            .then(self.patch.cmp(&other.patch))
    }
}

/// Classifies `old` vs `new` (§9.2): equal → none; different major → major;
/// different minor → minor; otherwise patch.
pub fn classify(old: SemVer, new: SemVer) -> VersionDiff {
    if old == new {
        VersionDiff::None
    } else if old.major != new.major {
        VersionDiff::Major
    } else if old.minor != new.minor {
        VersionDiff::Minor
    } else {
        VersionDiff::Patch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_strict_triples() {
        let v = SemVer::parse("1.0.0").expect("strict triple");
        assert_eq!((v.major, v.minor, v.patch), (1, 0, 0));
        assert_eq!(
            SemVer::parse("0.1.97").unwrap().to_string_version(),
            "0.1.97"
        );
        assert_eq!(
            SemVer::parse("10.20.30").unwrap(),
            SemVer {
                major: 10,
                minor: 20,
                patch: 30
            }
        );
    }

    #[test]
    fn rejects_non_strict_forms() {
        for bad in [
            "",
            "1",
            "1.0",
            "1.0.0.0",
            "1.0.x",
            "v1.0.0",
            "01.0.0",
            "1.0.01",
            "1.-2.0",
            "1.0.0-beta",
            "1.0.0+build",
            " 1.0.0",
            "1.0.0 ",
        ] {
            assert!(SemVer::parse(bad).is_none(), "must reject {bad:?}");
        }
    }

    #[test]
    fn ordering_is_component_wise() {
        assert!(SemVer::parse("1.0.0").unwrap() < SemVer::parse("1.0.1").unwrap());
        assert!(SemVer::parse("1.0.9").unwrap() < SemVer::parse("1.1.0").unwrap());
        assert!(SemVer::parse("1.9.9").unwrap() < SemVer::parse("2.0.0").unwrap());
        assert_eq!(
            SemVer::parse("1.2.3").unwrap(),
            SemVer::parse("1.2.3").unwrap()
        );
    }

    #[test]
    fn classify_matches_bump_matrix() {
        let a = SemVer::parse("1.0.0").unwrap();
        assert_eq!(
            classify(a, SemVer::parse("1.0.0").unwrap()),
            VersionDiff::None
        );
        assert_eq!(
            classify(a, SemVer::parse("1.0.1").unwrap()),
            VersionDiff::Patch
        );
        assert_eq!(
            classify(a, SemVer::parse("1.0.9").unwrap()),
            VersionDiff::Patch
        );
        assert_eq!(
            classify(a, SemVer::parse("1.1.0").unwrap()),
            VersionDiff::Minor
        );
        assert_eq!(
            classify(a, SemVer::parse("1.9.0").unwrap()),
            VersionDiff::Minor
        );
        assert_eq!(
            classify(a, SemVer::parse("2.0.0").unwrap()),
            VersionDiff::Major
        );
        assert_eq!(
            classify(SemVer::parse("1.5.0").unwrap(), a),
            VersionDiff::Minor
        );
        assert_eq!(VersionDiff::None.class(), "none");
        assert_eq!(VersionDiff::Patch.class(), "patch");
        assert_eq!(VersionDiff::Minor.class(), "minor");
        assert_eq!(VersionDiff::Major.class(), "major");
    }
}

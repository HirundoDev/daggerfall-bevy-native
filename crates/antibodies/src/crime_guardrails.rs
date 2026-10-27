//! M28 · Crime and law guardrails (SPEC-M28-CRIME-LAW; REQ-L99-061).
//!
//! The `behavior` organ computes the sentence; this module states the
//! invariants every observable result must satisfy, so a wrong number is a
//! typed violation instead of a plausible-looking panel. The organ stays
//! dependency-free: the bounds are the source constants
//! (`PlayerEntity.cs:2247-2256`, `:2263-2281`; `DaggerfallCourtWindow.cs:59-62,
//! :148-173, :474, :484`), restated here as the antibody, not imported.
//!
//! Scope: values that cross the court boundary (regional legal reputation,
//! crime value, fine/days shape, gold debit, prison clock, guard quota). The
//! formula itself is not duplicated — a guardrail that recomputed the sentence
//! would pass by construction.

/// `legalRepMin` (`PlayerEntity.cs:2247-2248`).
pub const LEGAL_REP_MIN: i16 = -100;
/// `legalRepMax` (`:2249-2253`).
pub const LEGAL_REP_MAX: i16 = 100;
/// Highest `Crimes` value (`LoanDefault = 15`, `:2281`).
pub const CRIME_MAX_VALUE: u8 = 15;
/// `penaltyAmount /= 40` (`DaggerfallCourtWindow.cs:148`).
pub const FINE_STEP: i32 = 40;
/// `daysInPrison += 3` per split iteration (`:160`).
pub const PRISON_DAYS_STEP: i32 = 3;
/// `daysInPrison * 1440 * 60` (`:474`).
pub const SECONDS_PER_DAY: i64 = 1440 * 60;
/// `RaiseTime(240 * 60)` (`:484`).
pub const RELEASE_SECONDS: i64 = 240 * 60;
/// `maxActiveGuardSpawns = 5` (`PlayerEntity.cs:623`).
pub const MAX_ACTIVE_GUARD_SPAWNS: usize = 5;

/// Typed invariant violation of the law domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LawViolation {
    /// A regional legal reputation left `[-100, 100]`.
    LegalRepOutOfBounds {
        /// Observed value.
        value: i16,
    },
    /// A crime value outside `0..=15`.
    CrimeValueUnknown {
        /// Observed value.
        value: u8,
    },
    /// Negative fine or negative prison days.
    NegativeSentence {
        /// Observed fine.
        fine: i32,
        /// Observed prison days.
        days: i32,
    },
    /// The court charged more gold than the player owns.
    FineExceedsGold {
        /// Observed fine.
        fine: i32,
        /// Available gold.
        gold: i32,
    },
    /// `fine / 40 + days / 3` differs from the split iterations while the
    /// gold cap did not bind.
    StepsMismatch {
        /// Expected iterations.
        steps: i32,
        /// Observed fine.
        fine: i32,
        /// Observed prison days.
        days: i32,
    },
    /// The court table was consumed for `Crimes::None`, which closes the court
    /// before reading it (`DaggerfallCourtWindow.cs:109-113`).
    CourtTableWithoutCrime {
        /// Observed fine.
        fine: i32,
        /// Observed prison days.
        days: i32,
    },
    /// The prison clock does not match `days * 86400` (+ 14400 on release).
    PrisonClockMismatch {
        /// Expected seconds.
        expected: i64,
        /// Observed seconds.
        observed: i64,
    },
    /// A guard spawn was requested with more than five active guards.
    GuardQuotaExceeded {
        /// Observed active guards.
        active: usize,
    },
}

/// Validated shape of a court sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SentenceShape {
    /// Split iterations before the halving (`penaltyAmount / 40`).
    pub steps: i32,
    /// Fine after the gold cap.
    pub fine: i32,
    /// `daysInPrison` after the gold cap.
    pub days: i32,
}

/// A regional legal reputation must stay inside `[-100, 100]`.
pub fn check_legal_rep(value: i16) -> Result<i16, LawViolation> {
    if (LEGAL_REP_MIN..=LEGAL_REP_MAX).contains(&value) {
        Ok(value)
    } else {
        Err(LawViolation::LegalRepOutOfBounds { value })
    }
}

/// A crime value must resolve to a `Crimes` member.
pub fn check_crime(value: u8) -> Result<u8, LawViolation> {
    if value <= CRIME_MAX_VALUE {
        Ok(value)
    } else {
        Err(LawViolation::CrimeValueUnknown { value })
    }
}

/// Invariants of a court sentence: non-negative values, the fine never above
/// the player's gold, no table for `Crimes::None`, and — only when the
/// gold cap did not bind — the split identity `fine/40 + days/3 = steps`.
pub fn check_sentence(
    steps: i32,
    fine: i32,
    days: i32,
    gold: i32,
    crime_value: u8,
) -> Result<SentenceShape, LawViolation> {
    check_crime(crime_value)?;
    if fine < 0 || days < 0 {
        return Err(LawViolation::NegativeSentence { fine, days });
    }
    if fine > gold {
        return Err(LawViolation::FineExceedsGold { fine, gold });
    }
    if crime_value == 0 {
        return Err(LawViolation::CourtTableWithoutCrime { fine, days });
    }
    if gold > fine && fine / FINE_STEP + days / PRISON_DAYS_STEP != steps {
        return Err(LawViolation::StepsMismatch { steps, fine, days });
    }
    Ok(SentenceShape { steps, fine, days })
}

/// Prison clock: the sentence jump is `days * 1440 * 60` and the release adds
/// `240 * 60`. Returns the expected seconds.
pub fn check_prison_clock(
    days: i32,
    advanced: i64,
    released: bool,
) -> Result<i64, LawViolation> {
    let mut expected = i64::from(days) * SECONDS_PER_DAY;
    if released {
        expected += RELEASE_SECONDS;
    }
    if advanced == expected {
        Ok(expected)
    } else {
        Err(LawViolation::PrisonClockMismatch {
            expected,
            observed: advanced,
        })
    }
}

/// Guards only spawn while at most five city guards are active
/// (`PlayerEntity.cs:623-626`).
pub fn check_guard_quota(active: usize) -> Result<usize, LawViolation> {
    if active <= MAX_ACTIVE_GUARD_SPAWNS {
        Ok(active)
    } else {
        Err(LawViolation::GuardQuotaExceeded { active })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legal_rep_bounds_are_typed() {
        assert_eq!(check_legal_rep(-100), Ok(-100));
        assert_eq!(check_legal_rep(100), Ok(100));
        assert_eq!(
            check_legal_rep(-101),
            Err(LawViolation::LegalRepOutOfBounds { value: -101 })
        );
        assert_eq!(
            check_legal_rep(101),
            Err(LawViolation::LegalRepOutOfBounds { value: 101 })
        );
    }

    #[test]
    fn crime_values_are_typed() {
        assert_eq!(check_crime(0), Ok(0));
        assert_eq!(check_crime(15), Ok(15));
        assert_eq!(
            check_crime(16),
            Err(LawViolation::CrimeValueUnknown { value: 16 })
        );
    }

    #[test]
    fn sentence_shape_accepts_the_golden_vector_and_rejects_tampering() {
        assert_eq!(
            check_sentence(5, 80, 9, 1000, 2),
            Ok(SentenceShape {
                steps: 5,
                fine: 80,
                days: 9
            })
        );
        assert_eq!(
            check_sentence(5, 80, 57, 1000, 2),
            Err(LawViolation::StepsMismatch {
                steps: 5,
                fine: 80,
                days: 57
            })
        );
        assert_eq!(
            check_sentence(0, 0, 0, 0, 0),
            Err(LawViolation::CourtTableWithoutCrime { fine: 0, days: 0 })
        );
        assert_eq!(
            check_sentence(2, 120, 0, 100, 15),
            Err(LawViolation::FineExceedsGold {
                fine: 120,
                gold: 100
            })
        );
        assert_eq!(
            check_sentence(2, -40, 3, 100, 15),
            Err(LawViolation::NegativeSentence {
                fine: -40,
                days: 3
            })
        );
        assert_eq!(
            check_sentence(2, 100, 61, 100, 15),
            Ok(SentenceShape {
                steps: 2,
                fine: 100,
                days: 61
            }),
            "the gold cap breaks the identity, so it is not checked"
        );
    }

    #[test]
    fn prison_clock_matches_the_source() {
        assert_eq!(check_prison_clock(12, 1_036_800, false), Ok(1_036_800));
        assert_eq!(check_prison_clock(12, 1_051_200, true), Ok(1_051_200));
        assert_eq!(
            check_prison_clock(12, 1_036_800, true),
            Err(LawViolation::PrisonClockMismatch {
                expected: 1_051_200,
                observed: 1_036_800
            })
        );
    }

    #[test]
    fn guard_quota_is_five() {
        assert_eq!(check_guard_quota(5), Ok(5));
        assert_eq!(
            check_guard_quota(6),
            Err(LawViolation::GuardQuotaExceeded { active: 6 })
        );
    }
}

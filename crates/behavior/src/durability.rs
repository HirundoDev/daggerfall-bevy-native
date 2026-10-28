//! Durability and repair — SPEC M21 §4.
//!
//! Every constant and formula is transcribed from Daggerfall Unity, pinned at
//! `Interkarma/daggerfall-unity@2343305d1d83ccc0de57a81e3b1e61188a997e34`
//! (`reference/dfu-spec/CLASSIFICATION.md` §1; MIT); local mirror
//! `reference/dfu-spec/Assets/Scripts/Game/Formulas/FormulaHelper.cs`, sha256
//! `b7ba3b2dfdf3e10826b6c5532d593e35125731afac5fd2524ddae6c1c3a2a09d`:
//!
//! - wear per physical hit: `FormulaHelper.cs:1123-1138` (amount `:1130`,
//!   20 % re-roll `:1131-1132`, applied `:1135`/`:1137`)
//! - shop cost: `FormulaHelper.cs:1884-1899` — clamp `:1890-1893`, regional
//!   `:1895`, quality multiplier `:1896`, return `:1898`
//! - repair cost: `FormulaHelper.cs:1901-1922` (`:1911-1914` value + clamp,
//!   `:1916` calls `CalculateCost`, `:1919` guild discount, `:1921` return)
//! - repair time: `FormulaHelper.cs:1924-1933` (`:1930-1932`),
//!   `DaggerfallDateTime.cs:42`
//! - breakage / left-for-repair state: `DaggerfallUnityItem.cs:1170-1182`
//!   (`LowerCondition`) and `ItemRepairData.cs:54-93` — **context not verified
//!   locally**: neither file is part of the local sparse mirror
//!   (`CLASSIFICATION.md` §7), so their line ranges cite upstream behaviour
//!   only; 0 constant, 0 arithmetic and 0 probe value in this module derives
//!   from them (the engine reproduces subtract → clamp at 0 → break).
//!
//! **Frontier (not invented here):** `ApplyRegionalPriceAdjustment`
//! (`FormulaHelper.cs:2026-2045`) is the identity in this engine — the
//! identity branch is the source's own (`:2036-2039`, no current location);
//! the priced branch (`:2041-2044`, `cost * RegionData.PriceAdjustment / 1000`
//! floored at 1) needs retail save data absent from this repo, so it is
//! **NOT-COMPARABLE** and stays unmodelled (regional prices are M30 economy).
//! The guild discount (`guild.ReducedRepairCost`, `FormulaHelper.cs:1919`) is
//! M27. Neither is modelled, neither is faked. The wear roll uses the caller's
//! explicit [`Dice100`]: 0 global state.

use crate::locomotion::Dice100;

/// Chance of the "minimum wear" re-roll (`FormulaHelper.cs:1131-1132`):
/// `if ((amount == 0) && Dice100.SuccessRoll(20)) amount = 1;`.
pub const WEAR_REROLL_CHANCE: i32 = 20;

/// One classic day in seconds (`DaggerfallDateTime.cs:42`:
/// `SecondsPerDay = SecondsPerHour * HoursPerDay` = 3600 · 24).
pub const MIN_REPAIR_SECONDS: i64 = 86_400;

/// Wear dealt by a physical hit of `damage` points
/// (`FormulaHelper.cs:1130`: `(10 * damage + 50) / 100`).
///
/// Negative damage deals no wear (the caller only applies wear when
/// `damage > 0`, `FormulaHelper.cs:1090`).
pub fn wear_amount(damage: i32) -> i32 {
    if damage < 0 {
        return 0;
    }
    ((10 * i64::from(damage) + 50) / 100) as i32
}

/// [`wear_amount`] plus the 20 % re-roll that turns a zero into one point
/// (`FormulaHelper.cs:1131-1132`). The re-roll happens even for `damage == 0`,
/// exactly as in the source.
pub fn wear_amount_rolled(damage: i32, dice: &mut Dice100) -> i32 {
    let amount = wear_amount(damage);
    if amount == 0 && dice.success(WEAR_REROLL_CHANCE) {
        1
    } else {
        amount
    }
}

/// Result of applying wear to a condition counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WearOutcome {
    /// Zero wear and the item was already above zero: nothing changed.
    Unchanged,
    /// The item lost `amount` points and still has `condition` left.
    Worn { amount: i32, condition: i32 },
    /// The item reached zero or below: `current` was clamped to `0`
    /// (`DaggerfallUnityItem.cs:1170-1182`, `ItemBreaks`).
    Broke { amount: i32 },
}

/// Applies [`wear_amount_rolled`] to `current` and reports the outcome.
///
/// Mirrors `LowerCondition` (`DaggerfallUnityItem.cs:1170-1182`): subtract,
/// clamp at zero, and break when the result is `<= 0`. An item that is already
/// at zero breaks again (the source also re-triggers `ItemBreaks`).
pub fn apply_wear(current: &mut i32, damage: i32, dice: &mut Dice100) -> WearOutcome {
    let amount = wear_amount_rolled(damage, dice);
    if amount == 0 && *current > 0 {
        return WearOutcome::Unchanged;
    }
    *current = current.saturating_sub(amount);
    if *current <= 0 {
        *current = 0;
        WearOutcome::Broke { amount }
    } else {
        WearOutcome::Worn {
            amount,
            condition: *current,
        }
    }
}

/// Shop-specific cost multiplier — DFU `CalculateCost(baseValue, shopQuality,
/// conditionPercentage = -1)` (`FormulaHelper.cs:1884-1899`):
/// `c = baseValue`, `c = max(c, 1)` (`:1890-1893`);
/// `c = ApplyRegionalPriceAdjustment(c)` (`:1895`, identity here — the source's
/// own no-location branch `:2036-2039`); `c = 2 * (c * (quality - 10) / 100 + c)`
/// (`:1896`), integer truncation toward zero; return `:1898`.
///
/// **`conditionPercentage` is declared and never read** in the body: `:1884`
/// takes it and `:1888` forwards it to the override only. The engine models
/// the non-override path and therefore takes no condition argument — an
/// explicit NOT-COMPARABLE, not an omission.
///
/// The intermediate product is evaluated in `i64` and clamped to `i32` so an
/// absurd input cannot panic in debug builds; every retail-scale input hits
/// the plain `int` result.
pub fn calculate_cost(cost: i32, quality: i32) -> i32 {
    let base = i64::from(cost.max(1));
    let quality = i64::from(quality);
    let adjusted = 2 * (base * (quality - 10) / 100 + base);
    adjusted.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

/// Repair cost — DFU `CalculateItemRepairCost` (`FormulaHelper.cs:1901-1922`):
/// `0` when already repaired (`:1908-1909`); `c = 10 * value / 100`,
/// `c = max(c, 1)` (`:1911-1914`); then the shelf stage
/// `c = CalculateCost(c, quality)` (`:1916`); guild discount `:1919` (M27,
/// unmodelled); return `:1921`.
pub fn repair_cost(value: i32, condition: i32, max_condition: i32, quality: i32) -> i32 {
    if condition == max_condition {
        return 0;
    }
    let cost = (10 * i64::from(value) / 100).max(1) as i32;
    calculate_cost(cost, quality)
}

/// Repair time in seconds for `damage` missing condition points (D-M21-1):
/// `damage * 86400 / 1000`, truncated, floored at one day —
/// `CalculateItemRepairTime` (`FormulaHelper.cs:1924-1933`: damage `:1930`,
/// product `:1931`, floor `:1932`).
///
/// `i64` arithmetic is a **declared delta**, not a transcription:
/// `damage * 86400` overflows `int32` above `damage = 24855` and C#'s
/// unchecked wrap makes DFU return one day. The engine returns the real value
/// instead of inheriting the runtime accident.
pub fn repair_time_seconds_for_damage(damage: i64) -> i64 {
    (damage.saturating_mul(MIN_REPAIR_SECONDS) / 1000).max(MIN_REPAIR_SECONDS)
}

/// Repair time for a condition pair (`FormulaHelper.cs:1930-1932`):
/// `damage = max - condition`, then [`repair_time_seconds_for_damage`].
pub fn repair_time_seconds(condition: i32, max_condition: i32) -> i64 {
    repair_time_seconds_for_damage(i64::from(max_condition) - i64::from(condition))
}

/// Days left until a ready timestamp (`ItemRepairData.cs:88-93`):
/// `ceil(seconds / 86400)`, never negative.
pub fn days_remaining(seconds: i64) -> i64 {
    if seconds <= 0 {
        0
    } else {
        // Stable ceiling division: `div_ceil` was still unstable (`E0658`)
        // on the toolchain this crate builds with. `seconds > 0` here, so the
        // quotient-plus-remainder form matches `div_ceil` exactly and cannot
        // overflow the numerator.
        seconds / MIN_REPAIR_SECONDS + i64::from(seconds % MIN_REPAIR_SECONDS != 0)
    }
}

/// "Left for repair" record (`ItemRepairData.cs:23-93`).
///
/// The source keeps `timeStarted` in world seconds and treats `0` as "not left
/// for repair"; [`RepairData::new`] stores the supplied start verbatim, so a
/// start of `0` means "not being repaired" exactly like the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepairData {
    /// World second the item was left (`ItemRepairData.cs:64-68`).
    pub time_started: i64,
    /// Repair duration in seconds (`RepairTime`, `ItemRepairData.cs:29`).
    pub repair_seconds: i64,
}

impl RepairData {
    /// Records a repair left at `now` taking `seconds` world seconds.
    pub fn new(now: i64, seconds: i64) -> Self {
        Self {
            time_started: now,
            repair_seconds: seconds,
        }
    }

    /// Ready timestamp (`GetTimeDone`, `ItemRepairData.cs:33-36`).
    pub fn ready_at(&self) -> i64 {
        self.time_started + self.repair_seconds
    }

    /// `IsRepairFinished` (`ItemRepairData.cs:54-57`): only a started repair
    /// can be finished, and only once `ready_at() <= now`.
    pub fn is_ready(&self, now: i64) -> bool {
        self.time_started > 0 && now >= self.ready_at()
    }

    /// `DaysUntilRepaired` (`ItemRepairData.cs:82-93`).
    pub fn days_remaining(&self, now: i64) -> i64 {
        days_remaining(self.ready_at() - now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dice(seed: u64) -> Dice100 {
        Dice100::new(seed)
    }

    // ── GV-DUR-01..06: wear_amount ──────────────────────────────────────

    #[test]
    fn golden_wear_amounts() {
        assert_eq!(wear_amount(4), 0, "GV-DUR-01");
        assert_eq!(wear_amount(5), 1, "GV-DUR-02");
        assert_eq!(wear_amount(9), 1, "GV-DUR-03");
        assert_eq!(wear_amount(10), 1, "GV-DUR-04");
        assert_eq!(wear_amount(15), 2, "GV-DUR-05");
        assert_eq!(wear_amount(25), 3, "GV-DUR-06");
    }

    #[test]
    fn negative_and_zero_damage_deal_no_base_wear() {
        assert_eq!(wear_amount(-3), 0);
        assert_eq!(wear_amount(0), 0);
    }

    // ── re-roll: 0 -> 1 with 20 % chance ────────────────────────────────

    #[test]
    fn zero_wear_is_rerolled_at_twenty_percent() {
        let mut saw_zero = false;
        let mut saw_one = false;
        for seed in 1..=2_000 {
            let amount = wear_amount_rolled(4, &mut dice(seed));
            assert!(amount == 0 || amount == 1, "re-roll stays binary");
            saw_zero |= amount == 0;
            saw_one |= amount == 1;
        }
        assert!(saw_zero && saw_one, "both outcomes reachable");
    }

    #[test]
    fn non_zero_wear_ignores_the_dice() {
        for seed in 1..=64 {
            assert_eq!(wear_amount_rolled(5, &mut dice(seed)), 1);
            assert_eq!(wear_amount_rolled(25, &mut dice(seed)), 3);
        }
    }

    // ── GV-DUR-07/08: apply_wear ────────────────────────────────────────

    #[test]
    fn golden_apply_wear() {
        let mut d = dice(7);
        let mut condition = 2;
        assert_eq!(
            apply_wear(&mut condition, 5, &mut d),
            WearOutcome::Worn {
                amount: 1,
                condition: 1
            },
            "GV-DUR-07"
        );
        assert_eq!(condition, 1);

        let mut d = dice(7);
        let mut condition = 1;
        assert_eq!(
            apply_wear(&mut condition, 5, &mut d),
            WearOutcome::Broke { amount: 1 },
            "GV-DUR-08"
        );
        assert_eq!(condition, 0);
    }

    #[test]
    fn zero_amount_with_live_condition_is_unchanged_only_when_the_roll_fails() {
        // Find one seed whose first roll fails SuccessRoll(20) and one that
        // succeeds, then check both branches with damage 4.
        let mut failing = None;
        let mut succeeding = None;
        for seed in 1..=1_000 {
            let mut d = dice(seed);
            if d.roll() < WEAR_REROLL_CHANCE {
                succeeding.get_or_insert(seed);
            } else {
                failing.get_or_insert(seed);
            }
        }
        let mut d = dice(failing.unwrap());
        let mut condition = 10;
        assert_eq!(apply_wear(&mut condition, 4, &mut d), WearOutcome::Unchanged);
        assert_eq!(condition, 10);

        let mut d = dice(succeeding.unwrap());
        let mut condition = 10;
        assert_eq!(
            apply_wear(&mut condition, 4, &mut d),
            WearOutcome::Worn {
                amount: 1,
                condition: 9
            }
        );
    }

    // ── GV-DUR-09..11: repair cost ─────────────────────────────────────

    #[test]
    fn golden_repair_costs() {
        assert_eq!(repair_cost(45, 25, 50, 10), 8, "GV-DUR-09");
        assert_eq!(repair_cost(45, 25, 50, 15), 8, "GV-DUR-10");
        assert_eq!(repair_cost(5000, 25, 50, 20), 1100, "GV-DUR-11");
    }

    #[test]
    fn repaired_items_cost_nothing() {
        assert_eq!(repair_cost(45, 50, 50, 10), 0, "GV-DUR-15");
        // `FormulaHelper.cs:1908-1909` compares `condition == max`, not `>=`: an
        // over-max condition still prices like any unrepaired item (4 → 8 gp).
        assert_eq!(repair_cost(45, 51, 50, 10), 8, "condition > max is priced");
    }

    #[test]
    fn minimum_cost_is_one_before_quality() {
        // value 0 -> c = 0 -> max(c, 1) = 1 -> CalculateCost(1, 10) = 2.
        assert_eq!(repair_cost(0, 1, 10, 10), 2);
        assert_eq!(calculate_cost(0, 10), 2, "CalculateCost clamps to 1");
        assert_eq!(calculate_cost(1, 5), 2, "q<10 on 1 gp truncates to 1");
        assert_eq!(calculate_cost(100, 0), 180, "2*(100*-10/100+100)");
    }

    // ── GV-DUR-12..14 + D-M21-1: repair time ───────────────────────────

    #[test]
    fn golden_repair_times() {
        assert_eq!(repair_time_seconds(25, 50), 86_400, "GV-DUR-12");
        assert_eq!(repair_time_seconds(1000, 2000), 86_400, "GV-DUR-13");
        assert_eq!(repair_time_seconds(999, 2000), 86_486, "GV-DUR-14");
    }

    #[test]
    fn damage_beyond_i32_product_does_not_wrap() {
        // D-M21-1: `damage * 86400` overflows int32 above 24855. The engine
        // computes in i64 and returns `damage * 86400 / 1000`.
        //
        // Spec erratum (declared in the M21 lane yield): GV-DUR-17 quotes
        // `2_831_155_200`, which is `32768 * 86400` *before* the `/1000`
        // division of `FormulaHelper.cs:1931`. The formula's real result is
        // 2_831_155 s. The engine follows the formula; the probe must use the
        // corrected value.
        assert_eq!(repair_time_seconds_for_damage(32_768), 2_831_155);
        assert!(2_831_155 > i64::from(i32::MAX) / 1000);
    }

    #[test]
    fn repair_time_is_floored_at_one_day_even_for_negative_damage() {
        assert_eq!(repair_time_seconds(60, 50), 86_400);
        assert_eq!(repair_time_seconds_for_damage(-5), 86_400);
    }

    // ── GV-DUR-16 + RepairData ─────────────────────────────────────────

    #[test]
    fn days_remaining_rounds_up() {
        assert_eq!(days_remaining(216_000), 3, "GV-DUR-16 (2.5 days)");
        assert_eq!(days_remaining(86_400), 1);
        assert_eq!(days_remaining(86_401), 2);
        assert_eq!(days_remaining(0), 0);
        assert_eq!(days_remaining(-1), 0);
    }

    #[test]
    fn repair_data_tracks_readiness() {
        let repair = RepairData::new(1_000, 86_400);
        assert_eq!(repair.ready_at(), 87_400);
        assert!(!repair.is_ready(1_000));
        assert!(!repair.is_ready(87_399));
        assert!(repair.is_ready(87_400));
        assert!(repair.is_ready(200_000));
        assert_eq!(repair.days_remaining(1_000), 1);
        assert_eq!(repair.days_remaining(87_400), 0);

        let not_left = RepairData::new(0, 86_400);
        assert!(!not_left.is_ready(1_000_000), "timeStarted == 0 is never ready");
    }
}

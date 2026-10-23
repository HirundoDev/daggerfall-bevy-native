//! M23 → M22 seam: the item enchantment callbacks of a **resolved** strike.
//!
//! DFU runs the item callbacks once the damage is known, and **before** the
//! health is removed: `WeaponManager.cs:618` (player strike) and
//! `EnemyAttack.cs:267,382` (enemy strike) call
//! `EntityEffectManager.DoItemEnchantmentPayloads(Strikes, weapon, …,
//! damage)`, whose return value may change the damage
//! (`WeaponManager.cs:619-621`); the defender's artifacts get
//! `EnchantmentPayloadFlags.None` out of band from
//! `FormulaHelper.CalculateWeaponAttackDamage` (`:704-719`, only when the
//! struck entity is the player — the Ring of Namira path,
//! `EnchantmentPayloadFlags.cs`).
//!
//! The M22 formula stays pure (`resolve_attack`, frozen): this function is the
//! call the app composes right after it, so effects are applied from the game's
//! strike step and not only from the probe.
//!
//! Entities: `run_enchantment` receives the two entities of the callback, so
//! DFU's split between `sourceEntity` and `targetEntity` is preserved — on a
//! weapon `Strikes` the bearer is the **attacker** and the struck entity is the
//! **defender**; on the out-of-band `Struck` the bearer is the **wearer** and
//! the struck entity is the **attacker** (`RingOfNamira.cs:50-72` reflects the
//! damage back at whoever struck).

use daggerfall_behavior::magic::{
    run_enchantment, EffectChange, EffectDice, EffectProperties, EffectTarget, Trigger,
};

/// What a resolved strike triggered, split by entity.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StrikeCallbacks {
    /// Changes on the attacker: the weapon's bearer-scoped payloads
    /// (`VampiricEffect` heals the bearer, `MaceOfMolagBal` raises it) and the
    /// `Struck` requests its victim's artifact emits at it
    /// (`Request::ReflectDamage`).
    pub attacker: Vec<EffectChange>,
    /// Changes on the defender: the weapon's `targetEntity` payloads (drain,
    /// transfer, curse) and the wear on the defender's worn artifact.
    pub defender: Vec<EffectChange>,
}

impl StrikeCallbacks {
    /// Every change, attacker first.
    pub fn changes(&self) -> impl Iterator<Item = &EffectChange> {
        self.attacker.iter().chain(self.defender.iter())
    }

    /// Whether the strike triggered nothing observable.
    pub fn is_empty(&self) -> bool {
        self.attacker.is_empty() && self.defender.is_empty()
    }
}

/// Runs the two out-of-band callbacks of a resolved strike:
///
/// 1. the attacker's weapon `Strikes` payload (`trigger` = [`Trigger::Strikes`]),
/// 2. the defender's worn artifact `Struck` payload ([`Trigger::Struck`]).
///
/// `event_amount` is the resolved `damage` (DFU's `sourceDamage`), passed to
/// the payloads that consume it ([`daggerfall_behavior::magic::Payload::TriggerAmount`]).
/// An item without an enchantment is `None` — the corresponding callback is
/// simply not broadcast, as in DFU (no effect manager callback without a
/// template).
pub fn strike_callbacks(
    attacker_weapon: Option<&'static EffectProperties>,
    defender_worn: Option<&'static EffectProperties>,
    attacker: &mut EffectTarget,
    defender: &mut EffectTarget,
    damage: i32,
    dice: &mut impl EffectDice,
) -> StrikeCallbacks {
    let mut callbacks = StrikeCallbacks::default();

    if let Some(weapon) = attacker_weapon {
        let report = run_enchantment(weapon, Trigger::Strikes, damage, attacker, defender, dice);
        callbacks.attacker.extend(report.bearer);
        callbacks.defender.extend(report.struck);
    }

    if let Some(worn) = defender_worn {
        // The wearer is the bearer of its own artifact; the entity it struck
        // back at is the `targetEntity`.
        let report = run_enchantment(worn, Trigger::Struck, damage, defender, attacker, dice);
        callbacks.defender.extend(report.bearer);
        callbacks.attacker.extend(report.struck);
    }

    callbacks
}

#[cfg(test)]
mod tests {
    use super::*;
    use daggerfall_behavior::magic::{effect_by_key, EffectTarget, Request};

    /// Dice that answer nothing: the artifact rows declare no magnitude roll
    /// that consumes a `Range` value on these paths.
    struct NoDice;

    impl EffectDice for NoDice {
        fn range_inclusive(&mut self, _low: i32, _high: i32) -> i32 {
            0
        }

        fn d100(&mut self) -> i32 {
            0
        }
    }

    fn target() -> EffectTarget {
        let mut target = EffectTarget::new(100, 100, [50; 8]);
        target.health = 50;
        target.spell_points = 50;
        target.fatigue = target.max_fatigue() / 2;
        target.item.max_condition = 100;
        target.item.condition = 50;
        target.item.magnitude = 5;
        target
    }

    #[test]
    fn a_vampiric_weapon_heals_the_attacker_the_resolved_damage() {
        // `VampiricEffect.WhenStrikes`: `sourceEntity.CurrentHealth += sourceDamage`
        // (`VampiricEffect.cs:112-128`); `EnchantmentParam.Value` = the worn
        // item's magnitude.
        let weapon = effect_by_key("VampiricEffect").expect("registered row");
        let mut attacker = target();
        let mut defender = target();
        let callbacks = strike_callbacks(
            Some(weapon),
            None,
            &mut attacker,
            &mut defender,
            12,
            &mut NoDice,
        );
        assert_eq!(attacker.health, 62, "the bearer heals the strike damage");
        assert_eq!(defender.health, 50, "the struck entity is untouched");
        assert_eq!(callbacks.attacker.len(), 1);
        assert!(callbacks.defender.is_empty());
    }

    #[test]
    fn an_unenchanted_strike_does_nothing() {
        let mut attacker = target();
        let mut defender = target();
        let callbacks = strike_callbacks(None, None, &mut attacker, &mut defender, 12, &mut NoDice);
        assert!(callbacks.is_empty());
        assert_eq!(attacker.observe(), target().observe());
        assert_eq!(defender.observe(), target().observe());
    }

    #[test]
    fn the_wearers_artifact_reflects_the_damage_at_the_attacker() {
        // `Ring_of_Namira`: `OnStruck` → `Request::ReflectDamage`, a world
        // action the owner consumes (`RingOfNamira.cs:50-72`,
        // `FormulaHelper.cs:721-722`).
        let worn = effect_by_key("Ring_of_Namira").expect("registered row");
        let mut attacker = target();
        let mut defender = target();
        let callbacks = strike_callbacks(
            None,
            Some(worn),
            &mut attacker,
            &mut defender,
            9,
            &mut NoDice,
        );
        assert_eq!(
            attacker.request_count(Request::ReflectDamage),
            1,
            "the request lands on the attacker, not on the wearer"
        );
        assert_eq!(defender.request_count(Request::ReflectDamage), 0);
        assert_eq!(callbacks.attacker.len(), 1);
    }

    #[test]
    fn a_health_leech_weapon_charges_the_bearer_not_the_victim() {
        // `HealthLeech.cs:88-89`: the strike cost (`leechWeaponAmount = 8`)
        // leaves the **bearer**. A strike trigger carries a `targetEntity`, so
        // the row wraps it in `Bearer(..)`.
        let weapon = effect_by_key("HealthLeech").expect("registered row");
        let mut attacker = target();
        let mut defender = target();
        let callbacks = strike_callbacks(
            Some(weapon),
            None,
            &mut attacker,
            &mut defender,
            10,
            &mut NoDice,
        );
        assert!(!callbacks.attacker.is_empty(), "the bearer pays the leech");
        assert_eq!(attacker.health, 50 - 8, "the bearer pays 8 health");
        assert_eq!(defender.health, 50, "the victim is untouched");
    }

    #[test]
    fn a_draining_weapon_drains_the_defender_and_buffs_the_bearer() {
        // `Mace_of_Molag_Bal` strikes (`MaceOfMolagBalEffect.cs:80-160`): the
        // victim loses spell points and strength, and the bearer's half
        // (`Bearer(..)`: max/current strength + max magicka) lands on the
        // attacker, never on the victim.
        let weapon = effect_by_key("Mace_of_Molag_Bal").expect("registered row");
        let mut attacker = target();
        let mut defender = target();
        let callbacks = strike_callbacks(
            Some(weapon),
            None,
            &mut attacker,
            &mut defender,
            10,
            &mut NoDice,
        );
        assert!(!callbacks.defender.is_empty(), "the victim is drained");
        assert_eq!(defender.spell_points, 45, "victim loses EnchantmentParam.Value SP");
        assert!(defender.attribute_damage[0] > 0, "victim loses strength");
        assert_eq!(
            defender.effective_attribute(daggerfall_behavior::magic::Attribute::Strength),
            50 - 5,
            "the victim is not buffed"
        );
        assert!(attacker.max_magicka_mod > 0, "the bearer's max magicka rises");
        assert!(
            attacker.effective_attribute(daggerfall_behavior::magic::Attribute::Strength) > 50,
            "the bearer's strength rises"
        );
        assert_eq!(attacker.attribute_damage[0], 0, "the bearer keeps its strength");
    }
}

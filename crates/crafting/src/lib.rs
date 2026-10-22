//! Crafting organ — item enchanting and potion alchemy (SPEC M24).
//!
//! M24 does not invent effects: it consumes M23's effect registry
//! (`daggerfall_behavior::magic::effect_by_key`) and M21's item instances
//! (`daggerfall_inventory`). What this organ defines is (a) the **enchantment
//! budget** of an item, (b) the **accounting** of powers/side-effects and their
//! gold cost, (c) the **effect-key contract** (§3.4) and (d) the **recipe
//! matching** for potions by the hash of sorted ingredient ids.
//!
//! Three modules, matching the fixed design (HH-1):
//!
//! - [`magic_def`] — the retail `MAGIC.DEF` parser (62-byte little-endian
//!   records: name + type/group/groupIndex + 10 enchantments + uses + value +
//!   material). Artifacts and regular magic items both live here; the item
//!   maker never offers artifacts (their budget is fixed by the file, §2.1).
//! - [`enchanting`] — the `EnchantmentTypes` registry (real `EnchantCost`
//!   values, §2.3), the power/side-effect classification by the **sign** of
//!   the cost, the point/gold accounting and the three gates
//!   (`NotEnoughGold` / `BeyondLimit` / `NoRoomInItem`, §2.4).
//! - [`alchemy`] — the 20 classic recipes (§3.2) with price, effect key,
//!   variant, ingredients, texture and effect settings; brewing is a hash
//!   lookup over sorted ingredient ids (§3.3).
//!
//! Pure: 0 Bevy, 0 I/O. Retail bytes are read by the caller (Mode A) and fed
//! in; the formulas and tables are the organ's own. The API is the surface
//! M25 (monsters) and M30 (trade) will consume: artifact templates from
//! [`magic_def`] and potion prices from [`alchemy`].

pub mod alchemy;
pub mod enchanting;
pub mod magic_def;

/// Canonical organ id (Sintergic organ map, ADR-FW-104).
pub const ORGAN: &str = "crafting";

/// Human-readable organ name.
pub fn organ_name() -> &'static str {
    ORGAN
}

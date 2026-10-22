//! Behavior organ — gameplay rules and domain logic (F0 scaffold).
//!
//! M7: [`quest`] — `QuestLog` (active quests within a session), source of the
//! PD4 T7 Inventory/Quests read model (REQ-L99-040, declared minimum
//! contract). The remaining rules arrive with their own lanes.
//!
//! M19: [`locomotion`] — player body and locomotion (REQ-L99-052): DFU
//! movement speeds, jump/gravity/fall, climbing, levitation, swimming with
//! air, carry weight and the stealth/theft/lockpick formulas. Pure: 0 Bevy,
//! 0 I/O; the world enters as a trait.
//!
//! M21: [`durability`] — wear per physical hit, breakage, repair cost and
//! time, and the "left for repair" state (spec M21 §4). Pure: the die enters
//! as a parameter (`Dice100`), 0 global RNG.
//!
//! M22: [`combat`] — pure combat formulas: hit chance (clamp `[3,97]`,
//! `−50/+40` adjustments and D-M22-2 bio-gate), damage, cadence `/980` and
//! `set_health` (spec M22 §1-§4). Pure: rolls are injected, 0 global RNG
//! (the per-actor PRNG lives in `daggerfall-combat`, D-M22-1).
//!
//! M23: [`magic`] — spells: effect model (22 templates), cost via formulas B
//! (§3.1) and C (§3.2), `MagicState`, `Spellbook`, `SpellMaker` and cast
//! animation (spec M23 §2-§5). Pure: 0 Bevy, 0 I/O; the die enters as a
//! parameter.
//!
//! M24: [`crafting`] — pure enchantment and alchemy formulas: potency per
//! material (weapon/armor, `Mathf.FloorToInt`) and recipe hash over sorted
//! ids with `int` wrap (spec M24 §2-§3). Pure: 0 Bevy, 0 I/O; the accounting
//! budget and the recipe table live in the `daggerfall-crafting` organ.
//!
//! M25: [`enemy_ai`] — observable enemy AI (spec §3-§4: states
//! `Idle/Pursue/Attack/Retreat/OpenDoor`, senses 102.4 m / 25 m / 180°,
//! `GiveUpTimer` 12.5 s, door at ≤ 2 m, class HP and pacification §6) and
//! [`encounters`] — 45 tables of 20 enemies, context mapping (dungeon /
//! water / weather × time band / building) and deterministic spawn (spec §5).
//! Pure: 0 Bevy, 0 I/O, 0 global RNG; rolls enter as parameters.
//!
//! M26: [`npc_ai`] — city population (caps §1: 24/48/72/96, 100 ms tick),
//! 1.6 m-per-cell navgrid and a deterministic `SeekingTile ↔ MovingForward`
//! routine with 1/32 guards (spec §2); [`dialogue`] — greeting by reaction
//! (7209/7208/7207/7206), topics by faction/locality, knowledge seeded by
//! (NPC, topic) and responses as `TEXT.RSC` IDs (spec §3). Pure: 0 Bevy,
//! 0 I/O, 0 prose.
//!
//! M27: [`factions`] — complete `FACTION.TXT` (366 retail factions),
//! reputation changes with propagation to allies and a deterministic digest;
//! [`guild`] — guilds and temples: membership by `GuildGroup`, ranks
//! (`rankReq*` table + 28-day cadence), service access, benefits
//! (Arkay/Akatosh/Kynareth/Stendarr/Fighters), training (720 min, cap 50,
//! cost 100/400 × level), healing (250 × disease, store quality, holidays)
//! and donation (`2·gold/|rep|+1`, without propagation). Pure: 0 Bevy,
//! 0 I/O, 0 global RNG (rolls enter as parameters).

pub mod combat;
pub mod crafting;
pub mod dialogue;
mod determinism;
pub mod durability;
pub mod encounters;
pub mod enemy_ai;
pub mod factions;
pub mod guild;
pub mod locomotion;
pub mod magic;
pub mod npc_ai;
pub mod quest;

/// Canonical organ id (Sintergic organ map, ADR-FW-104).
pub const ORGAN: &str = "behavior";

/// Human-readable organ name.
pub fn organ_name() -> &'static str {
    ORGAN
}

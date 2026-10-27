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
//! `GiveUpTimer` 12.5 s, door at ≤ 2 m, class HP and pacification §6);
//! [`encounters`] — 45 tables of 20 enemies, context mapping (dungeon /
//! water / weather × time band / building) and deterministic spawn (spec §5);
//! and [`mobile_enemies`] — the 62 rows of `EnemyBasics.Enemies` (`:234` @
//! `0038a1e6`) with the typed `mobile_enemy(id)` lookup (ORDER M25-CLOSURE).
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
//!
//! M28: [`law`] — crime, law and courts (REQ-L99-061): the 15 crimes and their
//! legal-reputation loss, the per-region `LegalRep` (±100) with the People
//! faction propagation, the court tables (P/Base/Min/Max), `punishmentType`,
//! the fine/prison split with the gold cap, surrender, the debate/lie plea,
//! guild rescue, the court state machine, the prison clock, the seven crime
//! triggers and the loan default. The guard spawn is **requested** here and
//! owned by M26; guard entities are never created by this organ. Pure:
//! 0 Bevy, 0 I/O, 0 global RNG (a seeded [`law::DfRandom`] enters).
//!
//! M29: [`quests`] — playable quest machine (REQ-L99-062, SPEC-M29-QUESTS):
//! 10 Hz [`quests::QuestMachine`], `Quest.Update` order, the five task kinds
//! with the always-on primary/secondary rule and `Headless` born triggered
//! (`Task.cs:181-185`), `PersistUntil`, per-action completion
//! (`Task.cs:207-209`: `PlaySound`/`PcAt` repeat), named clocks
//! (`60..=604800`, text forms, `ceil(s/86400)`, a `&16` clock requesting the
//! `2.5×` round trip from M31), local/global boolean variables, journal steps
//! and notes, `EndQuest` after two ticks with the `±5/−2` reputation as a
//! typed request to M27, display-only messages kept out of the journal with a
//! drainable mailbox, the `TaskSaveData_v1` shape (task
//! `global_var`/`has_trigger_conditions` plus `ActionSaveData_v1` actions so a
//! restored quest keeps executing), the UID/tombstone fields and the
//! complete opcode coverage table (70 rows: 62 documented types used in
//! retail, 7 NEGATIVE, `0x3E` documented but unused). The 87-byte record is
//! decoded only at `+0` (type) and `+81` (message id); every other byte stays
//! opaque. Pure: 0 Bevy, 0 I/O, 0 global RNG, 0 parser dependency; effects
//! crossing the organ leave as [`quests::QuestRequest`].

//! M30: [`economy`] — shops, inns, banking and transport (REQ-L99-063,
//! SPEC-M30): the nine `storeBuysItemType` buildings with their 36
//! `(building, group)` pairs and five trade modes, `CalculateCost` +
//! `ApplyRegionalPriceAdjustment` with the 250..4000 drift, the 53 retail
//! holidays, repair/identify quotes, `CalculateRoomCost`, the per-region
//! bank ledger (deposit/withdraw/LOC/loans `+10 %`/houses/ships) with the
//! five loan events and the overdue default as a typed [`economy::LoanCrime`],
//! and the four transport modes with their DFU speeds. The haggling stage
//! stays in the Skills organ (`calculate_trade_price`, M5); the `app`
//! composes both. Pure: 0 Bevy, 0 I/O, 0 global RNG (rolls enter as
//! parameters).

//! M38: [`nav_graph`] — dungeon navigation graph + A* pathfinding (F-AUD-02):
//! per-block nodes on the 1.6 m lattice (`CELL_METERS`), quantized levels,
//! integer edge costs, `PathResult::{Found,Partial,Unreachable,NoGraph}` and a
//! deterministic `path_digest` (design `f-aud-01-02-streaming-navmesh` §3, D-M38-1).
//! Pure: 0 Bevy, 0 I/O, 0 RNG, **0 dependencies** (I-2); the bridge's
//! `nav_build` extracts the graph from `RdbBlock` + `CollisionGrid`.

pub mod combat;
pub mod crafting;
pub mod dialogue;
mod determinism;
pub mod durability;
pub mod economy;
pub mod encounters;
pub mod enemy_ai;
pub mod factions;
pub mod guild;
pub mod law;
pub mod locomotion;
pub mod magic;
pub mod mobile_enemies;
pub mod nav_graph;
pub mod npc_ai;
pub mod quest;
pub mod quests;

/// Canonical organ id (Sintergic organ map, ADR-FW-104).
pub const ORGAN: &str = "behavior";

/// Human-readable organ name.
pub fn organ_name() -> &'static str {
    ORGAN
}

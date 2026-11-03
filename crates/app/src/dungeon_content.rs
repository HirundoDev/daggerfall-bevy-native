//! M44 lane D (`CR-bd972066-59e6-4768-9985-de75f69f6798`) — dungeon **content**:
//! the descent into the assembled retail dungeon plus the loot/farm evidence of
//! `scripts/dun-content-001.py` (the ten `DUN VE:` families of its block D).
//!
//! Owners reused, none duplicated: geometry = [`crate::interior_retail`]
//! (`RetailSource::assemble` — the same bytes `m18_window::m44_interior_grid`
//! reads), loot = `daggerfall_inventory::loot` (M21 matrices + the enemy chain,
//! + [`generate_boss_loot`]), clock = `daggerfall_heart::clock` (`GameClock`,
//! the same `classic_default` origin the window advances), canal =
//! [`crate::dun_canal`].
//!
//! Declared limitations (measured, never silent):
//! * the assembled `DungeonGrid` exposes blocks/doors/markers but **not** a
//!   container census per block ⇒ the three source containers are tagged real
//!   grid facts (`0x43…` door chest, `0x44…` door drop, `0x42…` starting
//!   block): real identity, deterministic, never a synthesised id;
//! * the demo has no collision mesh for `DungeonGrid` (`CollisionGrid` is built
//!   from a `MeshSpec` only) ⇒ the descent publishes the retail `Start` marker
//!   and does **not** teleport the avatar (a teleport without collision is a
//!   fall-through regression, and a second geometry path is forbidden by M44).
//!
//! Publication happens **before** the `FG01 VE: interact → interior` line: the
//! gate driver terminates the process as soon as it reads that line
//! (`scripts/dun-content-001.py:477-494`, no settle window), so anything
//! emitted after it is a race against the kill.

// Sin `window` no hay mundo de puertas que consumir (`main.rs`: `mod m18_doors`
// es `#[cfg(feature = "window")]`) ⇒ `descend` no existe y el resto del
// contrato (las diez familias, el loot y el reloj) se compila igual, sin
// llamante por diseño. Mismo criterio que `shell*` en `main.rs`.
#![cfg_attr(not(feature = "window"), allow(dead_code))]

use std::collections::HashMap;
use std::fmt::Write as _;

use daggerfall_heart::clock::GameClock;
use daggerfall_inventory::loot::{
    container_digest_bytes, dungeon_key, generate_boss_loot, generate_container,
    generate_container_with_counts, generate_enemy_loot, LootContainer,
};
use daggerfall_parser_bevy_bridge::dungeon_grid::DungeonGrid;

// M44 (`CR-03390267`): el mundo de puertas es tipo de la **ventana**. El
// contrato de contenido no depende de él: el build por defecto (`probes`, sin
// `window`) compila las diez familias sin `DoorWorld`.
#[cfg(feature = "window")]
use crate::m18_doors::DoorWorld;

// ── contract (mirrors `scripts/dun-content-001.py` block D) ────────────────

/// Points of the declared roll space (`ROLL_SPACE`, gate-side).
pub const ROLL_SPACE: u32 = 48;
/// Clock arithmetic is exact minutes (`total_minutes`), so the tolerance is 0.
pub const TOLERANCE_TICKS: u64 = 0;
/// Character level of the descent: a level-1 character enters the dungeon.
pub const LEVEL: i32 = 1;
/// The three dungeon families of the contract (Crypt `K`, Mine `M`, Nest `L`).
pub const LOOT_TABLES: [(char, i32); 6] = [
    ('K', 1),
    ('K', 10),
    ('M', 1),
    ('M', 10),
    ('L', 1),
    ('L', 10),
];
/// The 12 vectors: both ends of the declared roll space, per family and level.
pub const LOOT_VECTORS: [(char, i32, u32); 12] = [
    ('K', 1, 0),
    ('K', 1, 47),
    ('K', 10, 0),
    ('K', 10, 47),
    ('M', 1, 0),
    ('M', 1, 47),
    ('M', 10, 0),
    ('M', 10, 47),
    ('L', 1, 0),
    ('L', 1, 47),
    ('L', 10, 0),
    ('L', 10, 47),
];
/// Loot sources, in contract order.
pub const SOURCES: [&str; 3] = ["Chest", "Drop", "Boss"];
/// Minutes until each source respawns (Chest, Drop, Boss).
pub const RESPAWN_MINUTES: [u64; 3] = [20, 40, 30];
/// Entry cells of the loot-state matrix.
pub const ENTRY_LOCATIONS: [u64; 3] = [0, 1, 2];
/// Epochs of the loot-state matrix.
pub const ENTRY_EPOCHS: [u64; 3] = [1, 2, 3];
/// The five anti-farm rules: `(rule, slug, metric, required value)`.
pub const ANTIFARM: [(u8, &str, &str, u32); 5] = [
    (1, "no-reloot-same-epoch", "grants", 1),
    (2, "no-respawn-while-inside", "respawns", 0),
    (3, "boss-single-loot-per-epoch", "grants", 1),
    (4, "container-lockout-cross-player", "grants", 1),
    (5, "loot-not-scaled-by-entries", "grants_per_entry", 1),
];

// ── digest / identity ──────────────────────────────────────────────────────

/// FNV-1a 64 (the hash the parser crate uses; local copy, 0 new dependency).
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// FNV-1a over `tag|part…` — deterministic evidence identity.
fn evidence_of(tag: &str, parts: &[u64]) -> u64 {
    let mut text = String::from(tag);
    for part in parts {
        let _ = write!(text, "|{part}");
    }
    fnv1a64(text.as_bytes())
}

/// Evidence identity of one generated container at one level.
///
/// `container_digest_bytes` carries the container id + items but **not** the
/// gold nor the level, so a level-1 and a level-10 roll of the same container
/// could collide whenever the gold draw is 0. The identity therefore binds
/// `(container bytes, gold, level)`: two levels are two different pieces of
/// evidence, structurally. (Declared deviation from the M21 canonical digest,
/// which stays untouched in `loot.rs`.)
pub fn evidence_digest(container: &LootContainer, level: i32) -> u64 {
    let mut bytes = container_digest_bytes(container);
    bytes.extend_from_slice(&container.gold.to_le_bytes());
    bytes.extend_from_slice(&level.to_le_bytes());
    fnv1a64(&bytes)
}

/// Container identity of a loot-table row (level-specific, deterministic).
fn table_container_id(key: char, level: i32) -> u32 {
    0x4C00_0000 ^ ((key as u32) << 8) ^ (level as u32)
}

/// Container identity of a vector row (family × level × roll).
fn vector_container_id(key: char, level: i32, roll: u32) -> u32 {
    0x4C01_0000 ^ ((key as u32) << 12) ^ ((level as u32) << 6) ^ roll
}

/// Per-family seed; the level is **not** part of it.
///
/// The level does enter the container identity (`table_container_id`) and
/// `derive_rng` folds the container id into the stream (`loot.rs:239-242`),
/// so every level draws from its **own** stream: level 1 and level 10 are two
/// different rolls, never one roll scaled. What the level scales is the gold
/// *range* — `range_inclusive(min_gold, max_gold) * level` (`loot.rs:716-719`),
/// i.e. the result lands in `[min*level, max*level]`. Measured on the six
/// rows at levels 1/5/10: K 2/25/70, L 2/35/170, M 14/45/10 — M's gold *falls*
/// from level 1 to level 10, which one shared draw multiplied by the level
/// cannot produce.
fn table_seed(key: char) -> u64 {
    0x4C4F_4F54_0000_0000 ^ u64::from(key as u32)
}

/// Seed of one vector point (declared index inside the roll space).
fn vector_seed(key: char, roll: u32) -> u64 {
    table_seed(key) ^ (u64::from(roll) << 32)
}

/// Identity of one dungeon instance: `(location, epoch, slot)`.
fn instance_id(location: u64, epoch: u64, slot: u8) -> u64 {
    evidence_of("instance", &[location, epoch, u64::from(slot)])
}

/// Tagged real grid facts: the three source containers of one dungeon.
///
/// The high tag byte keeps the three identities apart for every grid; the low
/// bits carry the real door/block identity the grid exposes.
pub fn source_containers(grid: &DungeonGrid) -> [u32; 3] {
    let first_door = grid.doors.first().map(|door| door.object_offset).unwrap_or(0);
    let last_door = grid.doors.last().map(|door| door.object_offset).unwrap_or(0);
    let start_block = grid
        .blocks
        .iter()
        .position(|block| block.is_starting_block)
        .unwrap_or(0);
    [
        0x4300_0000 ^ ((grid.doors.len() as u32) << 16) ^ (first_door as u32),
        0x4400_0000 ^ ((grid.blocks.len() as u32) << 16) ^ (last_door as u32),
        0x4200_0000 ^ (start_block as u32),
    ]
}

// ── anti-farm ledger ───────────────────────────────────────────────────────

/// One loot grant: who took which container, in which epoch and instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grant {
    /// Container that yielded the loot.
    pub container: u32,
    /// Epoch the grant belongs to.
    pub epoch: u64,
    /// Player that asked.
    pub player: u32,
    /// Instance the player is in.
    pub instance: u64,
}

/// Farm state of one dungeon: grants, loot clock and entry count.
///
/// A container yields loot once per `(epoch, instance)`, whoever asks; the
/// entry count never multiplies loot; a respawn only fires when nobody is
/// inside.
#[derive(Debug, Default)]
pub struct FarmLedger {
    grants: Vec<Grant>,
    looted_at: HashMap<u32, u64>,
    entries: u32,
}

impl FarmLedger {
    /// Registers one entry into the dungeon.
    pub fn enter(&mut self) {
        self.entries += 1;
    }

    /// Entry count so far.
    pub fn entries(&self) -> u32 {
        self.entries
    }

    /// Asks for loot. `true` = granted (first ask of the container in this
    /// epoch **and** instance), `false` = lockout.
    pub fn loot(&mut self, container: u32, epoch: u64, player: u32, instance: u64, now: u64) -> bool {
        let taken = self
            .grants
            .iter()
            .any(|grant| grant.container == container && grant.epoch == epoch && grant.instance == instance);
        if taken {
            return false;
        }
        self.grants.push(Grant {
            container,
            epoch,
            player,
            instance,
        });
        self.looted_at.insert(container, now);
        true
    }

    /// Total grants of this state.
    pub fn grants(&self) -> usize {
        self.grants.len()
    }

    /// Grants of one container in one `(epoch, instance)` cell.
    pub fn grants_for(&self, container: u32, epoch: u64, instance: u64) -> u32 {
        self.grants
            .iter()
            .filter(|grant| {
                grant.container == container && grant.epoch == epoch && grant.instance == instance
            })
            .count() as u32
    }

    /// Cells `(container, epoch, instance)` granted to two or more players.
    pub fn shared(&self) -> usize {
        let mut by_cell: HashMap<(u32, u64, u64), Vec<u32>> = HashMap::new();
        for grant in &self.grants {
            by_cell
                .entry((grant.container, grant.epoch, grant.instance))
                .or_default()
                .push(grant.player);
        }
        by_cell
            .values()
            .filter(|players| {
                let mut sorted = (*players).clone();
                sorted.sort_unstable();
                sorted.dedup();
                sorted.len() >= 2
            })
            .count()
    }

    /// Minutes until `container` respawns, counted from `now`; `None` = never
    /// looted in this state.
    pub fn respawn_in(&self, container: u32, now: u64, cooldown: u64) -> Option<u64> {
        self.looted_at
            .get(&container)
            .map(|at| at.saturating_add(cooldown).saturating_sub(now))
    }

    /// Does the respawn fire now? Never while a player is inside.
    pub fn respawn_due(&self, container: u32, now: u64, cooldown: u64, player_inside: bool) -> bool {
        if player_inside {
            return false;
        }
        self.respawn_in(container, now, cooldown) == Some(0)
    }
}

/// Grants present in both states (cell identity intersection).
fn shared_state(left: &FarmLedger, right: &FarmLedger) -> usize {
    left.grants
        .iter()
        .filter(|grant| {
            right.grants.iter().any(|other| {
                other.container == grant.container
                    && other.epoch == grant.epoch
                    && other.instance == grant.instance
            })
        })
        .count()
}

// ── content of one dungeon ─────────────────────────────────────────────────

/// What the descent recorded about the dungeon it entered.
#[derive(Debug, Clone)]
pub struct DungeonEntry {
    /// Digest of the assembled grid (`DungeonGrid::digest`).
    pub grid_digest: u64,
    /// Blocks of the assembled grid.
    pub blocks: usize,
    /// Action doors kept by the grid.
    pub doors: usize,
    /// `locationIndex` of the retail dungeon (drives the loot key).
    pub location_index: usize,
    /// Retail location name (`MAPNAMES`).
    pub location_name: String,
    /// Loot key of the dungeon (`dungeon_key(location_index)`).
    pub key: char,
    /// Retail `Start` marker (published, never teleported to — see module doc).
    pub start: Option<[f32; 3]>,
}

impl DungeonEntry {
    /// One-line evidence of the descent (carries the `scene=Dungeon` literal).
    pub fn line(&self) -> String {
        let start = match self.start {
            Some(marker) => format!("({:.2},{:.2})", marker[0], marker[2]),
            None => "absent".to_string(),
        };
        format!(
            "DUN VE: dungeon descent location={} name=\"{}\" block={} doors={} start={} \
             containers={} key={} level={} scene=Dungeon",
            self.location_index,
            self.location_name,
            self.blocks,
            self.doors,
            start,
            SOURCES.len(),
            self.key,
            LEVEL,
        )
    }
}

/// The six loot-table rows, one per `(family, level)`.
pub fn loot_table_lines() -> Vec<String> {
    LOOT_TABLES
        .iter()
        .map(|&(key, level)| {
            let container = generate_container(
                table_seed(key),
                table_container_id(key, level),
                key,
                level,
            );
            format!(
                "DUN VE: loot id={key}-{level} key={key} level={level} rolls={ROLL_SPACE} \
                 digest={:016x}",
                evidence_digest(&container, level)
            )
        })
        .collect()
}

/// The twelve vectors: two public entry points of the library must agree on
/// every point of the declared roll space.
pub fn loot_vector_lines() -> Vec<String> {
    LOOT_VECTORS
        .iter()
        .map(|&(key, level, roll)| {
            let id = vector_container_id(key, level, roll);
            let seed = vector_seed(key, roll);
            let expected = evidence_digest(&generate_container(seed, id, key, level), level);
            let actual = evidence_digest(&generate_container_with_counts(seed, id, key, level).0, level);
            format!(
                "DUN VE: loot_vector id={key}-{level}-{roll} key={key} level={level} roll={roll} \
                 expected={expected:016x} actual={actual:016x}"
            )
        })
        .collect()
}

/// One [`LootContainer`] per source, over the real grid digest as seed.
pub fn source_containers_of(grid: &DungeonGrid) -> [(u32, LootContainer); 3] {
    let ids = source_containers(grid);
    let key = dungeon_key(0).unwrap_or(LOOT_TABLES[0].0);
    let seed = grid.digest();
    [
        (ids[0], generate_container(seed, ids[0], key, LEVEL)),
        // A drop has no loot-table key: the enemy chain with no map roll.
        (ids[1], generate_enemy_loot(seed, ids[1], None, 0, LEVEL)),
        (ids[2], generate_boss_loot(seed, ids[2], key, LEVEL)),
    ]
}

/// The three source rows plus the distinctness row.
pub fn source_lines(grid: &DungeonGrid) -> Vec<String> {
    let containers = source_containers_of(grid);
    let mut digests: Vec<u64> = Vec::with_capacity(SOURCES.len());
    let mut out = Vec::with_capacity(SOURCES.len() + 1);
    for (index, source) in SOURCES.iter().enumerate() {
        let (_, container) = &containers[index];
        let digest = evidence_digest(container, LEVEL);
        digests.push(digest);
        out.push(format!(
            "DUN VE: source id={source} source={source} digest={digest:016x} rolls={ROLL_SPACE}"
        ));
    }
    let mut unique = digests.clone();
    unique.sort_unstable();
    unique.dedup();
    out.push(format!(
        "DUN VE: source_distinct id=sources sources={} distinct={} shared={}",
        SOURCES.len(),
        unique.len(),
        digests.len() - unique.len()
    ));
    out
}

/// The three respawn rows, measured on the heart clock.
pub fn respawn_lines(grid: &DungeonGrid, now: u64) -> Vec<String> {
    let containers = source_containers_of(grid);
    let mut out = Vec::with_capacity(SOURCES.len());
    for (index, source) in SOURCES.iter().enumerate() {
        let (id, _) = &containers[index];
        let cooldown = RESPAWN_MINUTES[index];
        let mut ledger = FarmLedger::default();
        ledger.loot(*id, 1, 0, instance_id(0, 1, 0), now);
        let minutes = ledger.respawn_in(*id, now, cooldown).unwrap_or(cooldown);
        out.push(format!(
            "DUN VE: respawn id={source} source={source} minutes={minutes} \
             tolerance_ticks={TOLERANCE_TICKS}"
        ));
    }
    out
}

/// The five anti-farm rules, each one measured on its own state.
pub fn antifarm_lines(grid: &DungeonGrid, now: u64) -> Vec<String> {
    let [chest, drop, boss] = source_containers(grid);
    let mut out = Vec::with_capacity(ANTIFARM.len());
    for (number, slug, metric, required) in ANTIFARM {
        let (container, measured) = match number {
            1 => {
                let mut ledger = FarmLedger::default();
                let first = ledger.loot(chest, 1, 0, 1, now);
                let second = ledger.loot(chest, 1, 0, 1, now);
                debug_assert!(first && !second, "rule 1: re-loot granted");
                (chest, ledger.grants() as u32)
            }
            2 => {
                let mut ledger = FarmLedger::default();
                ledger.loot(drop, 1, 0, 1, now);
                let deadline = now + RESPAWN_MINUTES[1];
                let due = ledger.respawn_due(drop, deadline, RESPAWN_MINUTES[1], true);
                debug_assert!(
                    ledger.respawn_due(drop, deadline, RESPAWN_MINUTES[1], false),
                    "rule 2: respawn must fire once the player is out"
                );
                (drop, u32::from(due))
            }
            3 => {
                let mut ledger = FarmLedger::default();
                ledger.loot(boss, 1, 0, 1, now);
                ledger.loot(boss, 1, 1, 1, now);
                (boss, ledger.grants() as u32)
            }
            4 => {
                let mut ledger = FarmLedger::default();
                ledger.loot(chest, 1, 0, 1, now);
                ledger.loot(chest, 1, 1, 1, now);
                debug_assert_eq!(ledger.shared(), 0, "rule 4: no cell shared across players");
                (chest, ledger.grants() as u32)
            }
            _ => {
                let mut ledger = FarmLedger::default();
                for _ in 0..3 {
                    ledger.enter();
                    ledger.loot(chest, 1, 0, 1, now);
                }
                // One container touched by three entries: grants stay 1, so the
                // entry count never multiplies loot.
                debug_assert_eq!(ledger.entries(), 3);
                (chest, ledger.grants() as u32)
            }
        };
        debug_assert_eq!(measured, required, "rule {number} ({slug}) out of contract");
        out.push(format!(
            "DUN VE: antifarm id=r{number} rule={number} name={slug} container={container} \
             epoch=1 {metric}={measured} verdict=ok"
        ));
    }
    out
}

/// The nine loot-state cells: entering a cell grants nothing.
pub fn entry_lines() -> Vec<String> {
    let mut out = Vec::with_capacity(ENTRY_LOCATIONS.len() * ENTRY_EPOCHS.len());
    for location in ENTRY_LOCATIONS {
        for epoch in ENTRY_EPOCHS {
            let mut ledger = FarmLedger::default();
            ledger.enter();
            let instance = instance_id(location, epoch, 0);
            let looted = ledger.grants_for(location as u32, epoch, instance);
            out.push(format!(
                "DUN VE: entry id={location}-{epoch} location={location} epoch={epoch} \
                 looted={looted} evidence={:016x}",
                evidence_of("entry", &[location, epoch, u64::from(looted)])
            ));
        }
    }
    out
}

/// Two instances of the same cell: isolated state, no shared grants.
pub fn instance_lines(container: u32, now: u64) -> Vec<String> {
    let (location, epoch) = (0_u64, 1_u64);
    let first = instance_id(location, epoch, 0);
    let second = instance_id(location, epoch, 1);
    let mut left = FarmLedger::default();
    let mut right = FarmLedger::default();
    left.loot(container, epoch, 0, first, now);
    right.loot(container, epoch, 0, second, now);
    let shared = shared_state(&left, &right);
    let isolated = u32::from(shared == 0 && first != second);
    vec![
        format!(
            "DUN VE: instance slot=a instance={first:016x} location={location} epoch={epoch} \
             isolated={isolated}"
        ),
        format!(
            "DUN VE: instance slot=b instance={second:016x} location={location} epoch={epoch} \
             isolated={isolated}"
        ),
        format!("DUN VE: instance_isolation a={first:016x} b={second:016x} shared={shared}"),
    ]
}

/// The three adversarial trials: declared scenario, measured grants/shared.
pub fn adversarial_lines(container: u32, now: u64) -> Vec<String> {
    // (id, players, containers, instances, epochs)
    const TRIALS: [(&str, u32, u32, u32, u32); 3] = [
        ("ADV-01", 2, 1, 1, 1),
        ("ADV-02", 2, 1, 1, 2),
        ("ADV-03", 2, 1, 2, 1),
    ];
    let mut out = Vec::with_capacity(TRIALS.len());
    for (id, players, containers, instances, epochs) in TRIALS {
        let mut ledger = FarmLedger::default();
        for epoch in 1..=u64::from(epochs) {
            for slot in 0..instances {
                let instance = instance_id(0, epoch, slot as u8);
                for player in 0..players {
                    ledger.loot(container, epoch, player, instance, now);
                }
            }
        }
        out.push(format!(
            "DUN VE: adversarial id={id} players={players} containers={containers} \
             instances={instances} epochs={epochs} grants={} shared={}",
            ledger.grants(),
            ledger.shared()
        ));
    }
    out
}

/// The whole block D of the canal, in contract order, for one assembled grid.
pub fn content_lines(grid: &DungeonGrid, entry: &DungeonEntry) -> Vec<String> {
    // The origin of the M4 clock — the same `classic_default` the window
    // advances from (`m18_window.rs:2530`). Only differences are published.
    let now = GameClock::classic_default().total_minutes();
    let [chest, _, _] = source_containers(grid);
    let mut lines = Vec::with_capacity(48);
    lines.push(entry.line());
    lines.extend(loot_table_lines());
    lines.extend(loot_vector_lines());
    lines.extend(source_lines(grid));
    lines.extend(respawn_lines(grid, now));
    lines.extend(antifarm_lines(grid, now));
    lines.extend(entry_lines());
    lines.extend(instance_lines(chest, now));
    lines.extend(adversarial_lines(chest, now));
    lines
}

/// Assembles the retail dungeon and returns its entry record + published block.
fn assemble() -> Option<(DungeonEntry, Vec<String>)> {
    let retail = crate::interior_retail::RetailSource::load().ok()?;
    let (_, dungeons) = retail.dungeons().ok()?;
    let dungeon = dungeons.first()?;
    let grid = retail.assemble(&dungeon.layout);
    let entry = DungeonEntry {
        grid_digest: grid.digest(),
        blocks: grid.blocks.len(),
        doors: grid.doors.len(),
        location_index: dungeon.location_index,
        location_name: dungeon.location_name.clone(),
        key: dungeon_key(dungeon.location_index).unwrap_or(LOOT_TABLES[0].0),
        start: grid.start_marker,
    };
    let lines = content_lines(&grid, &entry);
    Some((entry, lines))
}

/// `E` on the interior: descend into the assembled retail dungeon, publish the
/// content block once, and keep the entry in the door world.
///
/// Idempotent per [`DoorWorld`]: a second press never re-publishes (the gate's
/// families have exact cardinality).
#[cfg(feature = "window")]
pub fn descend(doors: &mut DoorWorld) -> bool {
    if doors.dungeon.is_some() {
        return false;
    }
    match assemble() {
        Some((entry, lines)) => {
            for line in &lines {
                crate::dun_canal::emit(line);
            }
            doors.dungeon = Some(entry);
            true
        }
        None => {
            crate::dun_canal::emit(
                "DUN VE: dungeon descent unavailable — ARENA2/MAPS.BSA/BLOCKS.BSA not \
                 accessible (never a silent 0)",
            );
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use daggerfall_inventory::loot::{generate_container, matrix};

    #[test]
    fn level_is_an_input_of_the_loot_table() {
        for (key, level) in LOOT_TABLES {
            let other = if level == 1 { 10 } else { 1 };
            let here = generate_container(table_seed(key), table_container_id(key, level), key, level);
            let there = generate_container(table_seed(key), table_container_id(key, other), key, other);
            assert_ne!(
                evidence_digest(&here, level),
                evidence_digest(&there, other),
                "family {key}: level {level} and {other} must not share an identity"
            );
        }
    }

    #[test]
    fn gold_scales_with_level_on_its_own_draw() {
        // `GenerateRandomLoot` multiplies *this* container's draw by the level:
        // `Range(MinGold, MaxGold) * level` (`LootTables.cs:193-198`). The
        // container identity is level-specific (`table_container_id`) and the
        // stream is seeded from it (`derive_rng`), so level 10 never replays
        // level 1's roll: `gold(10) == gold(1) * 10` was a test assumption, not
        // a product contract — measured K 2 → 70, M 14 → 10, L 2 → 170. What
        // the product does hold is the matrix range scaled by the level, which
        // is also what the level-5 golden is built on.
        for (key, _) in LOOT_TABLES {
            let table = matrix(key);
            assert!(table.max_gold >= table.min_gold);
            for level in [1, 5, 10] {
                let container =
                    generate_container(table_seed(key), table_container_id(key, level), key, level);
                assert!(
                    (table.min_gold * level..=table.max_gold * level).contains(&container.gold),
                    "family {key} level {level}: gold {} outside [{}, {}]",
                    container.gold,
                    table.min_gold * level,
                    table.max_gold * level
                );
                assert_eq!(
                    container.gold % level,
                    0,
                    "family {key} level {level}: the level is an exact multiplier"
                );
                assert!(
                    container.gold > 0,
                    "family {key} level {level}: a positive matrix always pays gold"
                );
            }
        }
    }

    #[test]
    fn evidence_digest_is_stable() {
        let container = generate_container(7, 9, 'K', 3);
        assert_eq!(evidence_digest(&container, 3), evidence_digest(&container, 3));
        let mut other = container.clone();
        other.gold += 1;
        assert_ne!(evidence_digest(&container, 3), evidence_digest(&other, 3));
    }

    #[test]
    fn vectors_agree_on_both_entry_points() {
        let lines = loot_vector_lines();
        assert_eq!(lines.len(), LOOT_VECTORS.len());
        for line in &lines {
            let (expected, actual) = line
                .split_once("expected=")
                .and_then(|(_, rest)| rest.split_once(" actual="))
                .expect("vector line carries both halves");
            assert_eq!(expected, actual, "vector {line}");
        }
    }

    #[test]
    fn rule_one_rejects_the_repeat() {
        let mut ledger = FarmLedger::default();
        assert!(ledger.loot(5, 1, 0, 1, 0));
        assert!(!ledger.loot(5, 1, 0, 1, 0));
        assert_eq!(ledger.grants(), 1);
    }

    #[test]
    fn rule_two_holds_the_respawn_while_inside() {
        let mut ledger = FarmLedger::default();
        ledger.loot(5, 1, 0, 1, 100);
        assert!(!ledger.respawn_due(5, 120, 20, true));
        assert!(ledger.respawn_due(5, 120, 20, false));
        assert_eq!(ledger.respawn_in(5, 110, 20), Some(10));
    }

    #[test]
    fn rule_five_ignores_the_entry_count() {
        let mut ledger = FarmLedger::default();
        for _ in 0..3 {
            ledger.enter();
            ledger.loot(5, 1, 0, 1, 0);
        }
        assert_eq!(ledger.entries(), 3);
        assert_eq!(ledger.grants(), 1);
    }

    #[test]
    fn cross_player_lockout_shares_nothing() {
        let mut ledger = FarmLedger::default();
        ledger.loot(5, 1, 0, 1, 0);
        ledger.loot(5, 1, 1, 1, 0);
        assert_eq!(ledger.grants(), 1);
        assert_eq!(ledger.shared(), 0);
    }

    #[test]
    fn entries_grant_nothing() {
        for line in entry_lines() {
            assert!(line.contains("looted=0"), "{line}");
        }
    }

    #[test]
    fn instances_are_isolated() {
        let lines = instance_lines(5, 0);
        assert_eq!(lines.len(), 3);
        assert!(lines[0].contains("isolated=1") && lines[1].contains("isolated=1"));
        assert!(lines[2].ends_with("shared=0"));
    }

    #[test]
    fn adversarial_grants_match_the_contract() {
        let lines = adversarial_lines(5, 0);
        assert!(lines[0].ends_with("grants=1 shared=0"), "{}", lines[0]);
        assert!(lines[1].ends_with("grants=2 shared=0"), "{}", lines[1]);
        assert!(lines[2].ends_with("grants=2 shared=0"), "{}", lines[2]);
    }
}

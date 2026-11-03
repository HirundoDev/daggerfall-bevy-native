//! `dungeon_elig` (app, `probes`) — the M44 **lane C** entry-eligibility
//! evidence (`CR-bd972066-59e6-4768-9985-de75f69f6798`).
//!
//! **What it measures.** Three facts of the container protocol, published
//! through the single canal of the cycle ([`crate::dun_canal`]) and persisted
//! in the `dungeon_elig` / `dungeon_loot` families of the real save:
//!
//! 1. **the eligibility token** — `fnv1a64("dungeon-elig-v1|<seed>|<location>|
//!    <container>|<player>|<epoch>")`, computed by [`token`]. No literal of the
//!    contract's goldens lives here: the three vectors of `§4.3` are produced
//!    by the function from their campaign seeds (see the tests) and the probe
//!    publishes the tokens it computed, never a typed hex string. A new epoch
//!    changes the token because the epoch is part of the preimage.
//! 2. **the content of a container** — [`container_digest`] re-derives the
//!    digest from the `dungeon_loot` rows of that container *every* time it is
//!    observed, so two visits inside one epoch publish the same bytes because
//!    the persisted content is the same, not because a variable was reused.
//! 3. **the loot lockout** — [`Lockout`] opens when the container is looted
//!    (`until = looted_minutes + LOCKOUT_MAX_MINUTES`) and it is `Locked`
//!    strictly before that deadline and `Open` at it. The deadline is a
//!    number of game minutes, never a wall clock.
//!
//! **Frontier:** this module owns the `elig`, `container` and `lockout` lines
//! and the `dungeon_elig` / `dungeon_loot` rows. The state machine, the boss
//! register and the `load` row belong to [`crate::dungeon_state`], which
//! composes the tail through [`elig_rows`] and [`loot_rows`].

use daggerfall_memory::dungeon_v3::{DungeonEligRow, DungeonLootRow, LootSource};
use daggerfall_parser_rs::rdi::fnv1a64;

use crate::dun_canal;

/// Version of the token preimage (`TOKEN_PREFIX` of the block-C contract).
pub const TOKEN_PREFIX: &str = "dungeon-elig-v1";

/// Longest a container stays locked after being looted, in game minutes
/// (`LOCKOUT_MAX_MINUTES` of the contract).
pub const LOCKOUT_MAX_MINUTES: u64 = 60;

/// The dungeon the session walks — `sample[0]` of `PROBE-DUNGEON-001`, the
/// same location the state machine and the save persist.
pub const LOCATION: u16 = 26_219;

/// The container the session loots (it is the shared, open one of
/// [`crate::dungeon_state::container_views`]).
pub const LOOT_CONTAINER: u32 = 7;

/// The instance epoch the artifact can hold: the v3 grammar starts the epoch
/// at `1` (minute `0` is its "never" sentinel), so a row of epoch `0` is a
/// token the contract pins but the save cannot carry.
pub const SESSION_EPOCH: u32 = 1;

/// Game minute the container spawned (its loot row).
pub const SPAWN_MINUTES: u64 = 120;

/// Game minute it was looted. `0` is the "never looted" sentinel of the v3
/// grammar, so a looted row can never carry it.
pub const LOOTED_MINUTES: u64 = 120;

/// Campaign seed of each dungeon location. The seed is the identity of the
/// run the token belongs to; it never travels in a published line — the token
/// derived from it does, which is what makes the golden reproducible.
pub const CAMPAIGN_SEEDS: &[(u16, u64)] = &[(26_219, 12_345_678_901_234_567), (50_015, 42)];

/// The three vectors the contract pins, as `(location, container, player,
/// epoch)`. Their seeds are resolved through [`seed_for`] and their tokens
/// through [`token_hex`]: a drift in either one is caught by the tests.
pub const GOLDEN_ROWS: &[(u16, u32, u8, u32)] = &[
    (26_219, 0, 1, 0),
    (26_219, 0, 1, 1),
    (50_015, 3, 2, 5),
];

/// The campaign seed of `location`, if the session walks it.
pub fn seed_for(location: u16) -> Option<u64> {
    CAMPAIGN_SEEDS
        .iter()
        .find(|(id, _seed)| *id == location)
        .map(|(_id, seed)| *seed)
}

/// The 8 big-endian bytes of the eligibility token.
///
/// FNV-1a 64 (offset basis `0xcbf2_9ce4_8422_2325`, prime `0x1000_0000_01b3`,
/// mod 2^64) over the canonical preimage. Big endian because the save writes
/// `token` as 16 lowercase hex digits of the big-endian value.
pub fn token(seed: u64, location: u16, container: u32, player: u8, epoch: u32) -> [u8; 8] {
    let preimage = format!("{TOKEN_PREFIX}|{seed}|{location}|{container}|{player}|{epoch}");
    fnv1a64(preimage.as_bytes()).to_be_bytes()
}

/// The 16 lowercase hex digits a canal line publishes for that token.
pub fn token_hex(seed: u64, location: u16, container: u32, player: u8, epoch: u32) -> String {
    format!(
        "{:016x}",
        u64::from_be_bytes(token(seed, location, container, player, epoch))
    )
}

/// The `dungeon_loot` rows of the session — one row per container, ascending
/// by `(location, container, epoch)` as the v3 tail requires.
pub fn loot_rows() -> Vec<DungeonLootRow> {
    let mut rows = vec![DungeonLootRow {
        location_id: LOCATION,
        container_id: LOOT_CONTAINER,
        source: LootSource::Chest,
        spawn_minutes: SPAWN_MINUTES,
        looted_minutes: LOOTED_MINUTES,
        looted: true,
        epoch: SESSION_EPOCH,
    }];
    rows.sort_by_key(|row| (row.location_id, row.container_id, row.epoch));
    rows
}

/// The `dungeon_elig` rows of the session — the contract vectors whose epoch
/// the artifact can hold, with their tokens computed (never typed), ascending
/// by `(location, container, player, epoch)`.
///
/// The epoch-`0` vector is published by [`publish`] but **not** persisted: the
/// v3 grammar refuses `epoch=0`, so persisting it would require faking the
/// epoch and the artifact would then describe a session that never happened.
///
/// `seen = false` in every row: the tokens belong to players that have not
/// observed the container yet, which is what makes them still eligible.
pub fn elig_rows() -> Vec<DungeonEligRow> {
    let mut rows: Vec<DungeonEligRow> = GOLDEN_ROWS
        .iter()
        .filter(|(_, _, _, epoch)| *epoch >= SESSION_EPOCH)
        .map(|&(location, container, player, epoch)| DungeonEligRow {
            token: token(
                seed_for(location).expect("golden location without a campaign seed"),
                location,
                container,
                player,
                epoch,
            ),
            location_id: location,
            container_id: container,
            player_id: player,
            epoch,
            seen: false,
        })
        .collect();
    rows.sort_by_key(|row| {
        (
            row.location_id,
            row.container_id,
            row.player_id,
            row.epoch,
        )
    });
    rows
}

/// FNV-1a 64 of the canonical listing of one container's persisted loot: the
/// `"<source>,<spawn_minutes>,<looted_minutes>"` string of every `dungeon_loot`
/// row of `(location, container)`, **sorted**, each followed by `\n`.
///
/// Re-derived from [`loot_rows`] on every call and filtered by
/// `(location, container)` — the epoch is *not* part of the filter, exactly
/// like the reader of the artifact: a container that looting changed would
/// publish a different digest, which is the point.
pub fn container_digest(location: u16, container: u32) -> u64 {
    let mut listing: Vec<String> = loot_rows()
        .iter()
        .filter(|row| row.location_id == location && row.container_id == container)
        .map(|row| {
            format!(
                "{},{},{}",
                row.source.as_str(),
                row.spawn_minutes,
                row.looted_minutes
            )
        })
        .collect();
    listing.sort();
    let body: String = listing.iter().map(|line| format!("{line}\n")).collect();
    fnv1a64(body.as_bytes())
}

/// A loot lockout of one container for one player, in game minutes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lockout {
    /// Dungeon location of the container.
    pub location: u16,
    /// Container the lockout belongs to.
    pub container: u32,
    /// Player that cannot loot it yet.
    pub player: u8,
    /// Game minute at which the lockout expires.
    pub until_minutes: u64,
}

impl Lockout {
    /// The lockout a loot opens: the container stays locked for
    /// [`LOCKOUT_MAX_MINUTES`] game minutes after `looted_minutes`.
    pub fn after_loot(location: u16, container: u32, player: u8, looted_minutes: u64) -> Self {
        Self {
            location,
            container,
            player,
            until_minutes: looted_minutes + LOCKOUT_MAX_MINUTES,
        }
    }

    /// The state the lockout reports at `retry_minutes`: `Locked` strictly
    /// before the deadline, `Open` from it on.
    pub fn state_at(&self, retry_minutes: u64) -> &'static str {
        if retry_minutes < self.until_minutes {
            "Locked"
        } else {
            "Open"
        }
    }
}

/// Publishes the eligibility section through the canal: the three golden
/// tokens, the two visits of the looted container and the lockout with its
/// expiry. Deterministic end to end — no clock, no RNG, no map iteration.
pub fn publish() {
    // R10 — the tokens of the contract, derived from their seeds here.
    for &(location, container, player, epoch) in GOLDEN_ROWS {
        let seed = seed_for(location).expect("golden location without a campaign seed");
        dun_canal::emit(&format!(
            "DUN VE: elig token={} location={location} container={container} player={player} epoch={epoch}",
            token_hex(seed, location, container, player, epoch)
        ));
    }

    // R11 — the same container observed twice in one epoch. The digest is
    // re-derived per visit: equality is a measurement of the persisted
    // content, not a variable printed twice.
    let epoch = loot_rows()[0].epoch;
    for visit in 1..=2u8 {
        let digest = container_digest(LOCATION, LOOT_CONTAINER);
        dun_canal::emit(&format!(
            "DUN VE: container location={LOCATION} container={LOOT_CONTAINER} epoch={epoch} \
             visit={visit} content_digest={digest:016x}"
        ));
    }

    // R13 — the lockout the loot row opens, observed while it bites and once
    // it has expired. Both observations are the same `(location, container,
    // player)` and the reported state comes from the deadline arithmetic.
    let lockout = Lockout::after_loot(LOCATION, LOOT_CONTAINER, 2, LOOTED_MINUTES);
    for retry_minutes in [LOOTED_MINUTES, lockout.until_minutes] {
        dun_canal::emit(&format!(
            "DUN VE: lockout location={} container={} player={} until_minutes={} \
             retry_minutes={retry_minutes} state={}",
            lockout.location,
            lockout.container,
            lockout.player,
            lockout.until_minutes,
            lockout.state_at(retry_minutes)
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three vectors of `§4.3`, produced by the function from their seeds:
    /// `(seed, location, container, player, epoch) -> token`.
    #[test]
    fn golden_tokens_are_computed_from_their_seeds() {
        let cases = [
            (12_345_678_901_234_567u64, 26_219u16, 0u32, 1u8, 0u32, "6fe1574c48403ecd"),
            (12_345_678_901_234_567u64, 26_219, 0, 1, 1, "6fe1564c48403d1a"),
            (42u64, 50_015, 3, 2, 5, "a809ee2515123280"),
        ];
        for (seed, location, container, player, epoch, want) in cases {
            assert_eq!(
                token_hex(seed, location, container, player, epoch),
                want,
                "token drift for seed={seed} location={location} container={container} \
                 player={player} epoch={epoch}"
            );
        }
    }

    /// The published goldens resolve their seed through the campaign table —
    /// the same path `publish` walks at runtime.
    #[test]
    fn golden_rows_resolve_their_campaign_seed() {
        for &(location, _container, _player, _epoch) in GOLDEN_ROWS {
            assert!(
                seed_for(location).is_some(),
                "golden location {location} has no campaign seed"
            );
        }
    }

    /// A new epoch invalidates the token of the previous one (R12).
    #[test]
    fn new_epoch_changes_the_token() {
        let seed = seed_for(LOCATION).expect("seed");
        assert_ne!(
            token_hex(seed, LOCATION, 0, 1, 0),
            token_hex(seed, LOCATION, 0, 1, 1)
        );
    }

    /// The content digest is the same on two visits and it moves when the
    /// persisted content moves: the digest is a measurement, not a constant.
    #[test]
    fn content_digest_is_idempotent_and_bites() {
        let empty = container_digest(LOCATION, LOOT_CONTAINER);
        assert_eq!(empty, container_digest(LOCATION, LOOT_CONTAINER));
        assert_ne!(empty, container_digest(LOCATION, LOOT_CONTAINER + 1));
    }

    /// The lockout is bounded by `LOCKOUT_MAX_MINUTES` and it expires at its
    /// deadline: `Locked` one minute before, `Open` at it.
    #[test]
    fn lockout_is_bounded_and_expires() {
        let lockout = Lockout::after_loot(LOCATION, LOOT_CONTAINER, 2, LOOTED_MINUTES);
        assert_eq!(
            lockout.until_minutes - LOOTED_MINUTES,
            LOCKOUT_MAX_MINUTES,
            "the lockout is not bounded by the contract"
        );
        assert_eq!(lockout.state_at(LOOTED_MINUTES), "Locked");
        assert_eq!(lockout.state_at(lockout.until_minutes - 1), "Locked");
        assert_eq!(lockout.state_at(lockout.until_minutes), "Open");
        assert_eq!(lockout.state_at(lockout.until_minutes + 1), "Open");
    }
}

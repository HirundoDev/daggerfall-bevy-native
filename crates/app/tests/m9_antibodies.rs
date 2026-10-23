//! M9 antibodies: the falsifiable controls behind
//! PROBE-CHUNK-001, PROBE-SCHEDULE-001 and PROBE-MODB-ISOLATION-001.
//!
//! Every test carries a real assertion — no test asserts a tautology. The
//! two "naive" controls ([`chunk_naive_order_digest_differs`],
//! [`schedule_priority_mutation_is_detected`]) prove that the measured
//! invariants are not accidental: a naive order digest DOES change between
//! load orders, and a mutated priority table DOES change the emitted order.

use std::path::PathBuf;

use daggerfall_heart::clock::GameClock;
use daggerfall_heart::scheduler::{two_tick_log, SchedulePhase, Scheduler, TIME_ADVANCED};
use daggerfall_inventory::path_checker::PathChecker;
use daggerfall_memory::chunk_content::{
    chunk_digest, generate_chunk, set_digest, ChunkSet, CELL_COUNT,
};
use daggerfall_memory::chunks::ChunkCoord;
use daggerfall_memory::data_source::{DataSource, DataSourceError};

const SEED: u64 = 0x5EED_1234_ABCD_0001;
const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn coords() -> [ChunkCoord; 7] {
    [
        ChunkCoord::new(0, 0),
        ChunkCoord::new(1, 0),
        ChunkCoord::new(0, 1),
        ChunkCoord::new(1, 1),
        ChunkCoord::new(2, 3),
        ChunkCoord::new(3, 1),
        ChunkCoord::new(2, 2),
    ]
}

fn reversed<T: Copy>(items: &[T]) -> Vec<T> {
    items.iter().rev().copied().collect()
}

/// FNV-1a over the per-chunk digests in load order (deliberately naive).
fn naive_order_digest(seed: u64, load_order: &[ChunkCoord]) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    for coord in load_order {
        for byte in chunk_digest(seed, *coord).to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    hash
}

fn build_set(seed: u64, order: &[ChunkCoord]) -> ChunkSet {
    let mut set = ChunkSet::new();
    for coord in order {
        set.load(seed, *coord);
    }
    set
}

#[test]
fn chunk_order_invariance_two_orders() {
    let forward = coords();
    let backward = reversed(&forward);
    let set_a = build_set(SEED, &forward);
    let set_b = build_set(SEED, &backward);

    for coord in forward {
        assert_eq!(
            set_a.per_chunk_digest(coord),
            set_b.per_chunk_digest(coord),
            "per-chunk digest must not depend on load order at {coord:?}"
        );
    }
    assert_eq!(set_a.set_digest(), set_b.set_digest());
    assert_eq!(set_a.coords(), set_b.coords());
    // Free function, both orders, equal to the set value.
    assert_eq!(set_digest(SEED, &forward), set_a.set_digest());
    assert_eq!(set_digest(SEED, &backward), set_a.set_digest());
    // A third, non-trivial permutation.
    let permuted = vec![forward[3], forward[0], forward[6], forward[2], forward[5], forward[1], forward[4]];
    assert_eq!(set_digest(SEED, &permuted), set_a.set_digest());
}

#[test]
fn chunk_mutation_is_detected() {
    let order = coords();
    let mut set = build_set(SEED, &order);
    let target = ChunkCoord::new(2, 3);
    let before_set = set.set_digest();
    let before_target = set.per_chunk_digest(target).expect("loaded");
    let neighbours: Vec<(ChunkCoord, u64)> = order
        .iter()
        .filter(|coord| **coord != target)
        .map(|coord| (*coord, set.per_chunk_digest(*coord).expect("loaded")))
        .collect();

    assert!(set.damage(target, 4));
    assert_ne!(set.per_chunk_digest(target).expect("loaded"), before_target);
    assert_ne!(set.set_digest(), before_set);
    for (coord, digest) in neighbours {
        assert_eq!(
            set.per_chunk_digest(coord).expect("loaded"),
            digest,
            "neighbour {coord:?} must stay intact"
        );
    }
    // Damaging an absent chunk is refused and changes nothing.
    assert!(!set.damage(ChunkCoord::new(9, 9), 1));
}

#[test]
fn chunk_seed_change_is_detected() {
    let order = coords();
    for coord in order {
        assert_ne!(
            chunk_digest(SEED, coord),
            chunk_digest(SEED + 1, coord),
            "a different seed must change the digest at {coord:?}"
        );
    }
    assert_ne!(set_digest(SEED, &order), set_digest(SEED + 1, &order));
    // Generation itself differs, not only the hash.
    assert_ne!(generate_chunk(SEED, order[0]), generate_chunk(SEED + 1, order[0]));
    assert_eq!(generate_chunk(SEED, order[0]).heights.len(), CELL_COUNT);
}

#[test]
fn chunk_naive_order_digest_differs() {
    let forward = coords();
    let backward = reversed(&forward);
    let naive_forward = naive_order_digest(SEED, &forward);
    let naive_backward = naive_order_digest(SEED, &backward);
    // The naive control DOES depend on load order: it hashes the per-chunk
    // digests in the order the chunks were loaded.
    assert_ne!(naive_forward, naive_backward);
    // The keyed set digest does not: `set_digest` sorts + deduplicates the
    // coordinates and XORs one contribution per chunk, each rotated by an
    // amount derived from its own coordinate. This assertion measures the
    // free function (the ChunkSet method is compared to it in
    // `chunk_order_invariance_two_orders`), so it is not a consequence of the
    // container being sorted by construction.
    assert_eq!(set_digest(SEED, &forward), set_digest(SEED, &backward));
}

#[test]
fn schedule_two_runs_same_order() {
    let first = two_tick_log([0, 1, 2]);
    let second = two_tick_log([0, 1, 2]);
    assert_eq!(first, second);
    assert_eq!(first.len(), 2);
    assert_eq!(
        first[0],
        vec![
            (SchedulePhase::Simulation, TIME_ADVANCED),
            (SchedulePhase::Input, "MoveIntent"),
            (SchedulePhase::Input, "UseIntent"),
            (SchedulePhase::Simulation, "CollisionResolve"),
            (SchedulePhase::Simulation, "DamageApply"),
            (SchedulePhase::Presentation, "CameraUpdate"),
        ]
    );
    // The clock advances 600 s per tick (10 game minutes, classic start).
    assert_eq!(first[1], vec![(SchedulePhase::Simulation, TIME_ADVANCED)]);

    let mut clock = GameClock::classic_default();
    let mut scheduler = Scheduler::new();
    scheduler.schedule(SchedulePhase::Simulation, "CollisionResolve");
    let events = scheduler.tick(&mut clock, 600);
    assert_eq!(events[0].name, TIME_ADVANCED);
    assert_eq!(events[0].phase, SchedulePhase::Simulation);
    assert_eq!(clock.time_string(), "12:10:00");
    assert_eq!(events[1].seq, 0);
    assert_eq!(events[1].name, "CollisionResolve");
}

#[test]
fn schedule_priority_mutation_is_detected() {
    let default = two_tick_log([0, 1, 2]);
    let mutated = two_tick_log([1, 0, 2]);
    assert_ne!(default, mutated, "mutated priorities must change the order");
    // Tick 1 differs; the cadence event still comes first in both.
    assert_ne!(default[0], mutated[0]);
    assert_eq!(default[0][0].1, TIME_ADVANCED);
    assert_eq!(mutated[0][0].1, TIME_ADVANCED);
    assert_eq!(
        mutated[0][1],
        (SchedulePhase::Simulation, "CollisionResolve")
    );

    // Insertion order decides within the same priority.
    let mut forward = Scheduler::new();
    forward.schedule(SchedulePhase::Presentation, "A");
    forward.schedule(SchedulePhase::Presentation, "B");
    let mut inverted = Scheduler::new();
    inverted.schedule(SchedulePhase::Presentation, "B");
    inverted.schedule(SchedulePhase::Presentation, "A");
    let mut clock = GameClock::classic_default();
    let forward_names: Vec<&str> = forward
        .tick(&mut clock, 0)
        .into_iter()
        .map(|event| event.name)
        .collect();
    let inverted_names: Vec<&str> = inverted
        .tick(&mut clock, 0)
        .into_iter()
        .map(|event| event.name)
        .collect();
    assert_eq!(forward_names, vec!["A", "B"]);
    assert_eq!(inverted_names, vec!["B", "A"]);
}

/// Scratch directory removed on drop.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    /// Scratch root: the OS temp dir, with a fallback under `target/` for a
    /// full tmpfs (`ENOSPC`).
    fn candidates(tag: &str) -> [PathBuf; 2] {
        let name = format!("df-m9-test-{tag}-{}", std::process::id());
        [
            std::env::temp_dir().join(&name),
            PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/m9-scratch"))
                .join(&name),
        ]
    }

    fn create(tag: &str) -> Self {
        let mut last_error = None;
        for path in Self::candidates(tag) {
            let _ = std::fs::remove_dir_all(&path);
            // A full tmpfs still accepts `create_dir_all`; probe a real write.
            let writable = std::fs::create_dir_all(path.join("ARENA2"))
                .and_then(|()| std::fs::write(path.join(".writable"), b"m9"));
            match writable {
                Ok(()) => {
                    let _ = std::fs::remove_file(path.join(".writable"));
                    return Self { path };
                }
                Err(error) => {
                    let _ = std::fs::remove_dir_all(&path);
                    last_error = Some(format!("create {}: {error}", path.display()));
                }
            }
        }
        panic!("no scratch candidate: {last_error:?}");
    }

    fn arena2(&self) -> PathBuf {
        self.path.join("ARENA2")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn synthetic_payload(len: usize) -> Vec<u8> {
    (0..len).map(|index| (index % 251) as u8).collect()
}

#[test]
fn modb_isolation_zero_reads_custom_and_positive_retail() {
    // ── Mode B: decoy exists, yet 0 reads and 1 censused violation ───────
    let custom = Scratch::create("custom");
    let decoy = custom.arena2().join("MAPS.BSA");
    std::fs::write(&decoy, synthetic_payload(3072)).expect("write decoy");
    assert!(decoy.is_file(), "decoy must exist: the block is policy");

    let path_checker = PathChecker::with_custom_universe(&custom.path);
    assert!(path_checker.use_custom_universe());

    let source = DataSource::custom(&custom.path);
    assert!(source.is_custom());
    let world_coords = [
        ChunkCoord::new(0, 0),
        ChunkCoord::new(1, 0),
        ChunkCoord::new(0, 1),
        ChunkCoord::new(1, 1),
    ];
    let world = source.generate_world(SEED, &world_coords);
    assert_eq!(world.len(), world_coords.len());
    assert!(world
        .iter()
        .all(|chunk| chunk.heights.len() == CELL_COUNT && chunk.tiles.len() == CELL_COUNT));
    let generated_census = source.census();
    assert_eq!(
        generated_census.generated(),
        world_coords.len(),
        "the data source itself must count the produced chunks"
    );
    assert_eq!(generated_census.opens(), 0, "generation performs no I/O");
    assert_eq!(generated_census.bsa_opens(), 0);
    drop(generated_census);

    let error = source.open("MAPS.BSA").expect_err("custom open must fail");
    assert!(matches!(error, DataSourceError::IsolationViolation(_)));
    let census = source.census();
    assert_eq!(census.opens(), 0);
    assert_eq!(census.bsa_opens(), 0);
    assert_eq!(census.violations(), 1);
    assert_eq!(census.attempts().len(), 1);
    drop(census);

    // ── Mode A: a real file open IS counted (scratch corpus) ─────────────
    let retail = Scratch::create("retail");
    std::fs::write(retail.arena2().join("MAPS.BSA"), synthetic_payload(3072))
        .expect("write scratch MAPS.BSA");
    let retail_source = DataSource::retail(&retail.path);
    assert!(!retail_source.is_custom());
    let bytes = retail_source.open("MAPS.BSA").expect("retail read");
    assert_eq!(bytes.len(), 3072);
    let retail_census = retail_source.census();
    assert!(retail_census.opens() > 0);
    assert!(retail_census.bsa_opens() > 0);
    assert_eq!(retail_census.violations(), 0);
    drop(retail_census);
    // Also cover the declared retail root contract: passing the ARENA2 dir
    // itself still resolves (same file, case-insensitive).
    let arena2_source = DataSource::retail(retail.arena2());
    assert_eq!(
        arena2_source.open("maps.bsa").expect("case-insensitive").len(),
        3072
    );
    assert_eq!(arena2_source.census().bsa_opens(), 1);
}

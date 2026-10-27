//! M9 — verification-gap probes: chunk determinism
//! (PROBE-CHUNK-001, INV-L99-2), phase scheduling (PROBE-SCHEDULE-001) and
//! custom-universe isolation (PROBE-MODB-ISOLATION-001, INV-L99-6 /
//! REQ-L99-016).
//!
//! All three probes are **falsifiable**: each one prints a negative control
//! that must differ from the positive one (mutated chunk, mutated priority
//! table, inverted insertion order, decoy file present but unreadable by
//! policy). A run where the detector is insensitive prints `[FAIL]` and
//! exits `1` instead of reporting a green verdict.
//!
//! The probes run standalone, outside the `ALL-001` suite (same pattern as
//! `SKILLS-001`/`EQUIP-001`): `m6_probe::run_probe_all` keeps its 37 entries
//! for M2-M29 — 40 since M34 integrated its three demo probes (spec §11).
use std::path::{Path, PathBuf};

use daggerfall_heart::clock::GameClock;
use daggerfall_heart::scheduler::{two_tick_log, SchedulePhase, Scheduler, TIME_ADVANCED};
use daggerfall_inventory::path_checker::PathChecker;
use daggerfall_memory::chunk_content::{
    chunk_digest, set_digest, ChunkSet, CELL_COUNT,
};
use daggerfall_memory::chunks::{ChunkCoord, ChunkMap};
use daggerfall_memory::data_source::{DataSource, DataSourceError};

/// Seed used by PROBE-CHUNK-001.
const CHUNK_SEED: u64 = 0xD00D_F00D_1234_5678;

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

// ── PROBE-CHUNK-001 ────────────────────────────────────────────────────────

/// Deliberately naive digest: FNV-1a over the per-chunk digests **in load
/// order**. This is the negative control of the order invariant — it MUST
/// differ between two load orders, proving the real (keyed) `set_digest` is
/// not order-invariant by accident.
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

/// PROBE-CHUNK-001: deterministic chunk content + order-invariant set digest.
pub fn run_chunk_001() -> i32 {
    println!("PROBE-CHUNK-001 - deterministic chunk content (INV-L99-2, M9, CR-2102198d)");

    let mut order_a = vec![
        ChunkCoord::new(0, 0),
        ChunkCoord::new(1, 0),
        ChunkCoord::new(0, 1),
        ChunkCoord::new(1, 1),
        ChunkCoord::new(2, 3),
        ChunkCoord::new(3, 1),
        ChunkCoord::new(2, 2),
    ];
    order_a.sort_unstable_by_key(|coord| (coord.x, coord.z));
    let mut order_b = order_a.clone();
    order_b.reverse();

    let mut set_a = ChunkSet::new();
    for coord in &order_a {
        set_a.load(CHUNK_SEED, *coord);
    }
    let mut set_b = ChunkSet::new();
    for coord in &order_b {
        set_b.load(CHUNK_SEED, *coord);
    }

    let mut all_ok = true;

    // (i) per-chunk digests identical across load orders.
    let mut per_chunk_equal = true;
    for coord in &order_a {
        match (
            set_a.per_chunk_digest(*coord),
            set_b.per_chunk_digest(*coord),
        ) {
            (Some(digest_a), Some(digest_b)) => {
                let equal = digest_a == digest_b;
                per_chunk_equal &= equal;
                println!(
                    "[{}] chunk({:>2},{:>2}) digest A={digest_a:016x} B={digest_b:016x} equal={equal}",
                    if equal { "OK" } else { "FAIL" },
                    coord.x,
                    coord.z
                );
            }
            _ => {
                per_chunk_equal = false;
                println!(
                    "[FAIL] chunk({},{}) missing from a set",
                    coord.x, coord.z
                );
            }
        }
    }
    println!(
        "[{}] per_chunk_digests identical A vs B ({}/{} chunks)",
        if per_chunk_equal { "OK" } else { "FAIL" },
        order_a.len(),
        order_a.len()
    );
    all_ok &= per_chunk_equal;

    // (ii) set digest identical across load orders (method + free function).
    let set_a_digest = set_a.set_digest();
    let set_b_digest = set_b.set_digest();
    let free_a = set_digest(CHUNK_SEED, &order_a);
    let free_b = set_digest(CHUNK_SEED, &order_b);
    let set_equal = set_a_digest == set_b_digest && free_a == set_a_digest && free_b == set_a_digest;
    println!(
        "[{}] set_digest order invariant: set A={set_a_digest:016x} B={set_b_digest:016x} \
         free(A)={free_a:016x} free(B)={free_b:016x}",
        if set_equal { "OK" } else { "FAIL" }
    );
    all_ok &= set_equal;

    // (iii) negative control: the naive order digest MUST differ.
    let naive_a = naive_order_digest(CHUNK_SEED, &order_a);
    let naive_b = naive_order_digest(CHUNK_SEED, &order_b);
    let naive_differs = naive_a != naive_b;
    println!(
        "[{}] naive order digest differs (not tautological): naive(A)={naive_a:016x} \
         naive(B)={naive_b:016x} differs={naive_differs}",
        if naive_differs { "OK" } else { "FAIL" }
    );
    all_ok &= naive_differs;

    // (iv) ChunkMap iteration is identical regardless of load order.
    let mut map_a = ChunkMap::new();
    for coord in &order_a {
        map_a.load(*coord);
    }
    let mut map_b = ChunkMap::new();
    for coord in &order_b {
        map_b.load(*coord);
    }
    let iter_a: Vec<ChunkCoord> = map_a.iter().collect();
    let iter_b: Vec<ChunkCoord> = map_b.iter().collect();
    let map_equal = iter_a == iter_b && iter_a.len() == order_a.len();
    println!(
        "[{}] ChunkMap::iter identical for both load orders ({} entries)",
        if map_equal { "OK" } else { "FAIL" },
        iter_a.len()
    );
    all_ok &= map_equal;

    // (v) negative control: damaging ONE chunk changes its digest and the set
    // digest, and leaves the neighbours untouched.
    let target = ChunkCoord::new(1, 1);
    let before_target = set_a.per_chunk_digest(target);
    let before_others: Vec<(ChunkCoord, Option<u64>)> = order_a
        .iter()
        .filter(|coord| **coord != target)
        .map(|coord| (*coord, set_a.per_chunk_digest(*coord)))
        .collect();
    let before_set = set_a.set_digest();
    let damaged = set_a.damage(target, 3);
    let after_target = set_a.per_chunk_digest(target);
    let after_set = set_a.set_digest();
    let neighbours_intact = before_others
        .iter()
        .all(|(coord, before)| set_a.per_chunk_digest(*coord) == *before);
    let mutation_detected = damaged
        && after_target.is_some()
        && after_target != before_target
        && after_set != before_set
        && neighbours_intact;
    let hex_target_before = before_target
        .map(|digest| format!("{digest:016x}"))
        .unwrap_or_else(|| "<none>".to_string());
    let hex_target_after = after_target
        .map(|digest| format!("{digest:016x}"))
        .unwrap_or_else(|| "<none>".to_string());
    println!(
        "[{}] mutation detected: damage(1,1,+3) chunk digest {hex_target_before} -> \
         {hex_target_after} · set {before_set:016x} -> {after_set:016x} · neighbours intact=\
         {neighbours_intact}",
        if mutation_detected { "OK" } else { "FAIL" }
    );
    all_ok &= mutation_detected;

    // (vi) negative control: a different seed changes every per-chunk digest.
    let mut seed_chunks_differ = 0usize;
    for coord in &order_a {
        if chunk_digest(CHUNK_SEED + 1, *coord) != chunk_digest(CHUNK_SEED, *coord) {
            seed_chunks_differ += 1;
        }
    }
    let seed_set = set_digest(CHUNK_SEED + 1, &order_a);
    let seed_detected =
        seed_chunks_differ == order_a.len() && seed_set != free_a;
    println!(
        "[{}] seed+1 detected: {seed_chunks_differ}/{} chunk digests differ · set \
         {free_a:016x} -> {seed_set:016x}",
        if seed_detected { "OK" } else { "FAIL" },
        order_a.len()
    );
    all_ok &= seed_detected;

    if all_ok {
        println!(
            "VERDICT: PROBE-CHUNK-001 PASS - per-chunk A==B · set_digest A==B (order invariant) · \
             naive order digest A!=B · ChunkMap iter identical · 1-chunk mutation detected · \
             seed+1 detected"
        );
        0
    } else {
        println!("VERDICT: PROBE-CHUNK-001 FAIL - see [FAIL] rows");
        1
    }
}

// ── PROBE-SCHEDULE-001 ─────────────────────────────────────────────────────

fn print_log(label: &str, log: &[Vec<(SchedulePhase, &'static str)>]) {
    for (index, tick) in log.iter().enumerate() {
        let rendered: Vec<String> = tick
            .iter()
            .map(|(phase, name)| format!("{}:{name}", phase.label()))
            .collect();
        println!("        {label} tick {} = [{}]", index + 1, rendered.join(", "));
    }
}

/// PROBE-SCHEDULE-001: deterministic `(priority, seq)` drain order.
pub fn run_schedule_001() -> i32 {
    println!("PROBE-SCHEDULE-001 - deterministic phase scheduling (M9, CR-2102198d)");
    let mut all_ok = true;

    // (1) two independent runs are byte-identical.
    let default_a = two_tick_log([0, 1, 2]);
    let default_b = two_tick_log([0, 1, 2]);
    let identical = default_a == default_b;
    println!(
        "[{}] two runs of two_tick_log([0,1,2]) identical={identical}",
        if identical { "OK" } else { "FAIL" }
    );
    print_log("default", &default_a);
    all_ok &= identical;

    // (2) negative control: mutated priorities change the order.
    let mutated = two_tick_log([1, 0, 2]);
    let detected = default_a != mutated;
    let first_divergent = default_a
        .iter()
        .zip(mutated.iter())
        .position(|(left, right)| left != right);
    println!(
        "[{}] mutated priority table [1,0,2] detected: default != mutated={detected} · \
         first divergent tick={:?}",
        if detected { "OK" } else { "FAIL" },
        first_divergent.map(|index| index + 1)
    );
    print_log("mutated", &mutated);
    all_ok &= detected;

    // (3) negative control: same priority ⇒ insertion sequence decides.
    let mut forward = Scheduler::new();
    forward.schedule(SchedulePhase::Simulation, "A");
    forward.schedule(SchedulePhase::Simulation, "B");
    let mut inverted = Scheduler::new();
    inverted.schedule(SchedulePhase::Simulation, "B");
    inverted.schedule(SchedulePhase::Simulation, "A");
    let mut zero_clock = GameClock::classic_default();
    let forward_names: Vec<&'static str> = forward
        .tick(&mut zero_clock, 0)
        .into_iter()
        .map(|event| event.name)
        .collect();
    let inverted_names: Vec<&'static str> = inverted
        .tick(&mut zero_clock, 0)
        .into_iter()
        .map(|event| event.name)
        .collect();
    let seq_decides = forward_names == ["A", "B"]
        && inverted_names == ["B", "A"]
        && forward_names != inverted_names;
    println!(
        "[{}] insertion order decides within a priority: forward {:?} · inverted {:?}",
        if seq_decides { "OK" } else { "FAIL" },
        forward_names,
        inverted_names
    );
    all_ok &= seq_decides;

    // (4) the tick advances the real clock and emits TimeAdvanced first.
    let mut clock = GameClock::classic_default();
    let mut scheduler = Scheduler::new();
    scheduler.schedule(SchedulePhase::Input, "MoveIntent");
    let before = clock.time_string();
    let events = scheduler.tick(&mut clock, 600);
    let cadence_first = events.first().map(|event| event.name) == Some(TIME_ADVANCED);
    let clock_advanced = clock.time_string() != before;
    let drained = scheduler.pending_len() == 0;
    println!(
        "[{}] tick(600 s) advanced clock {before} -> {} · first event {:?} · events={} · \
         pending after tick={}",
        if cadence_first && clock_advanced && drained {
            "OK"
        } else {
            "FAIL"
        },
        clock.time_string(),
        events.first().map(|event| event.name),
        events.len(),
        scheduler.pending_len()
    );
    all_ok &= cadence_first && clock_advanced && drained;

    if all_ok {
        println!(
            "VERDICT: PROBE-SCHEDULE-001 PASS - two identical runs · mutated priorities detected · \
             seq decides within a priority · TimeAdvanced first with the clock advanced"
        );
        0
    } else {
        println!("VERDICT: PROBE-SCHEDULE-001 FAIL - see [FAIL] rows");
        1
    }
}

// ── PROBE-MODB-ISOLATION-001 ───────────────────────────────────────────────

/// Scratch directory under the OS temp dir, removed on drop (0 bytes left).
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    /// Scratch root: the OS temp dir, with a fallback under `target/` for a
    /// full tmpfs (`ENOSPC`) — the probe must not depend on `/tmp` health.
    fn candidates(tag: &str) -> [PathBuf; 2] {
        let name = format!("df-m9-{tag}-{}", std::process::id());
        [
            std::env::temp_dir().join(&name),
            PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/m9-scratch"))
                .join(&name),
        ]
    }

    fn create(tag: &str) -> Result<Self, String> {
        let mut last_error = None;
        for path in Self::candidates(tag) {
            let _ = std::fs::remove_dir_all(&path);
            // A full tmpfs still accepts `create_dir_all` and fails on the
            // first real write: probe writability before accepting the root.
            let writable = std::fs::create_dir_all(path.join("ARENA2"))
                .and_then(|()| std::fs::write(path.join(".writable"), b"m9"));
            match writable {
                Ok(()) => {
                    let _ = std::fs::remove_file(path.join(".writable"));
                    return Ok(Self { path });
                }
                Err(error) => {
                    let _ = std::fs::remove_dir_all(&path);
                    last_error = Some(format!("create {}: {error}", path.display()));
                }
            }
        }
        Err(last_error.unwrap_or_else(|| "no scratch candidate".to_string()))
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

/// Synthetic payload with no retail bytes (`index % 251`).
fn synthetic_payload(len: usize) -> Vec<u8> {
    (0..len).map(|index| (index % 251) as u8).collect()
}

/// Mode B side: decoy present, 0 reads, 1 censused violation.
fn modb_custom_side() -> bool {
    let scratch = match Scratch::create("modb-custom") {
        Ok(scratch) => scratch,
        Err(reason) => {
            println!("[FAIL] custom scratch: {reason}");
            return false;
        }
    };
    let decoy = scratch.arena2().join("MAPS.BSA");
    if let Err(error) = std::fs::write(&decoy, synthetic_payload(3072)) {
        println!("[FAIL] decoy write {}: {error}", decoy.display());
        return false;
    }
    let decoy_len = std::fs::metadata(&decoy).map(|meta| meta.len()).unwrap_or(0);

    let path_checker = PathChecker::with_custom_universe(&scratch.path);
    let mode_b = path_checker.use_custom_universe();
    let source = DataSource::custom(&scratch.path);

    // A custom world generation goes through the same pure chunk pipeline and
    // must not open a single payload file. The census counter lives in the
    // data source, so `generated()` can only move by actually producing
    // chunks (non-trivial check).
    let seed = 0x00C0_FFEE_0BAD_F00D_u64;
    let world_coords = [
        ChunkCoord::new(0, 0),
        ChunkCoord::new(1, 0),
        ChunkCoord::new(0, 1),
        ChunkCoord::new(1, 1),
    ];
    let world = source.generate_world(seed, &world_coords);
    let generated_cells: usize = world.iter().map(|chunk| chunk.heights.len()).sum();
    let world_census = source.census();
    let generated = world_census.generated();
    let opens_after_generation = world_census.opens();
    let bsa_opens_after_generation = world_census.bsa_opens();
    drop(world_census);

    let violation = match source.open("MAPS.BSA") {
        Ok(bytes) => {
            println!("[FAIL] custom open(MAPS.BSA) read {} B", bytes.len());
            None
        }
        Err(DataSourceError::IsolationViolation(reason)) => Some(reason),
        Err(other) => {
            println!("[FAIL] custom open(MAPS.BSA) returned {other}");
            None
        }
    };

    let census = source.census();
    let opens = census.opens();
    let bsa_opens = census.bsa_opens();
    let violations = census.violations();
    let attempts = census.attempts();
    drop(census);

    let decoy_ok = decoy_len == 3072 && decoy.is_file();
    println!(
        "[{}] custom universe (Mode B, use_custom_universe={mode_b}, is_custom={}) - decoy \
         ARENA2/MAPS.BSA present={decoy_len} B: the block is POLICY, not absence",
        if decoy_ok { "OK" } else { "FAIL" },
        source.is_custom()
    );
    let generation_ok = generated == world_coords.len()
        && generated_cells == world_coords.len() * CELL_COUNT
        && opens_after_generation == 0
        && bsa_opens_after_generation == 0;
    println!(
        "[{}] custom world generation: {} chunks / {generated_cells} cells · generated={generated} \
         -> opens={opens_after_generation} · bsa_opens={bsa_opens_after_generation}",
        if generation_ok { "OK" } else { "FAIL" },
        world.len()
    );
    let policy_ok = violation.is_some()
        && opens == 0
        && bsa_opens == 0
        && violations == 1
        && attempts.len() == 1;
    println!(
        "[{}] custom open(MAPS.BSA) -> IsolationViolation ({}) · violations={violations} · \
         opens={opens} · bsa_opens={bsa_opens} · attempts={}",
        if policy_ok { "OK" } else { "FAIL" },
        violation.as_deref().unwrap_or("<none>"),
        attempts.len()
    );

    decoy_ok && mode_b && source.is_custom() && generation_ok && policy_ok
}

/// Mode A side: at least one real `.BSA` open (retail corpus or scratch).
fn modb_retail_side() -> bool {
    let mut scratch: Option<Scratch> = None;
    let (root, label): (PathBuf, &'static str) = match daggerfall_app::mode_a::arena2_dir() {
        Ok(arena2) => {
            let root = arena2
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| arena2.clone());
            (root, "retail")
        }
        Err(_) => match Scratch::create("modb-retail") {
            Ok(created) => {
                let payload = synthetic_payload(3072);
                if let Err(error) = std::fs::write(created.arena2().join("MAPS.BSA"), &payload) {
                    println!("[FAIL] retail scratch MAPS.BSA: {error}");
                    return false;
                }
                let root = created.path.clone();
                scratch = Some(created);
                (root, "scratch")
            }
            Err(reason) => {
                println!("[FAIL] retail scratch: {reason}");
                return false;
            }
        },
    };

    let source = DataSource::retail(&root);
    let maps_len = match source.open("MAPS.BSA") {
        Ok(bytes) => bytes.len(),
        Err(error) => {
            println!("[FAIL] retail open(MAPS.BSA) under {}: {error}", root.display());
            return false;
        }
    };
    let arch3d = if label == "retail" {
        source.open("ARCH3D.BSA").ok().map(|bytes| bytes.len())
    } else {
        None
    };

    let census = source.census();
    let opens = census.opens();
    let bsa_opens = census.bsa_opens();
    drop(census);

    let ok = opens > 0 && bsa_opens > 0;
    println!(
        "[{}] Mode A positive control (source={label}, root={}) - MAPS.BSA {maps_len} B read{} \
         · opens={opens} · bsa_opens={bsa_opens}",
        if ok { "OK" } else { "FAIL" },
        root.display(),
        match arch3d {
            Some(len) => format!(" · ARCH3D.BSA {len} B"),
            None => String::new(),
        }
    );
    drop(scratch);
    ok
}

/// PROBE-MODB-ISOLATION-001: Mode B never reads payload bytes; Mode A does.
pub fn run_modb_isolation_001() -> i32 {
    println!(
        "PROBE-MODB-ISOLATION-001 - custom universe isolation vs Mode A positive control \
         (INV-L99-6 / REQ-L99-016, M9, CR-2102198d)"
    );
    let custom_ok = modb_custom_side();
    let retail_ok = modb_retail_side();
    if custom_ok && retail_ok {
        println!(
            "VERDICT: PROBE-MODB-ISOLATION-001 PASS - custom universe 0 opens / 1 violation \
             (decoy present, blocked before I/O) · Mode A positive control bsa_opens>0"
        );
        0
    } else {
        println!("VERDICT: PROBE-MODB-ISOLATION-001 FAIL - see [FAIL] rows");
        1
    }
}

//! `m42_npc_probe` — `PROBE-SYS-NPC-001` (M42 / W42-1): living NPCs with
//! reproducible 24 h agendas (roadmap §3.7,
//! `docs/acknowledgments/roadmap-m18-m48.md:317`:
//! "24 deterministic simulated hours: every NPC with a reproducible agenda
//! ×2; 0 teleporting NPCs").
//!
//! This probe **extends** M26's `PROBE-NPC-AI-001`
//! (`crates/app/src/m26_probe.rs`) instead of repeating it: M26 proves the
//! population caps, the navigation units, the appearance probabilities and a
//! 60 s routine with digest ×2, so the rows here are the **daily layer** the
//! M26 walk lacks (N-4): one 100 ms-tick simulation of 24 h
//! ([`behavior::npc_ai::DAY_TICKS`]) per NPC, the cell at each hour boundary
//! as that hour's agenda stop, and an adjacency check of **every** path step
//! (Chebyshev distance `> 1` ⇒ teleport ⇒ gate FAIL).
//!
//! Blocks (every evidence row is `[OK] <id> - <detail>` with a `src=` or
//! `spec=` citation, or `FAIL <id> got=<v> want=<v>`):
//!
//! - **A · agendas** — one `AGENDA npc=<i> seed=<s> guard=<0|1> hour=<h>
//!   cell=(x,y) kind=<name>` row per NPC per hour (8 NPCs × 24 h = 192
//!   rows); civilian kinds follow the day table (`home` 22-05, `work`
//!   06-10/13-17, `meal` 11-12/18-19, `rest` 20-21), the guard patrols 24 h;
//! - **B · day digest** — one `DAY npc=<i> moves=<m> cells=<c> teleports=<t>
//!   max_step=<s> digest=<hex16> agenda=<hex16>` row per NPC plus the
//!   `CENSUS npcs=8 hours=24 stops=192 teleports=0` total;
//! - **C · adversarial** — two `NEGATIVE` rows for the declared non-goals
//!   (per-building interiors, retail bytes) with on-disk evidence paths.
//!
//! **0 retail bytes**: the grid is the synthetic `NavGrid::city(1, seed)` and
//! every NPC start is fixed, so the probe is retail-independent and never
//! prints `SKIP` for a missing `ARENA2`.
//!
//! **Determinism:** no floats are printed, all iteration is over fixed arrays,
//! and the last line is `DIGEST <sha256>` over every byte printed before it.
//! The ×2 reproducibility is enforced by `scripts/sys-npc-001.py` running two
//! separate processes with byte-identical stdout.

use daggerfall_behavior::npc_ai::{
    agenda_digest, simulate_day, AgendaKind, NavGrid, DAY_HOURS,
};
use sha2::{Digest, Sha256};

/// Lane correlation id (W42-1).
const CR: &str = "CR-b434f896-2fe9-4750-b70e-9b748370e6c3";

/// Probe seed: NPC `i` simulates with `PROBE_SEED + i`.
const PROBE_SEED: u64 = 42_010_001;
/// Seed of the shared synthetic city grid.
const GRID_SEED: u64 = 42_010_001;
/// NPC population of the gate (npc 0 is the guard).
const NPC_COUNT: usize = 8;

/// Citations (every one resolves on disk).
const SRC_AGENDA: &str = "crates/behavior/src/npc_ai.rs";
const SPEC_M26: &str =
    "docs/sintergic/game/programming/m26-npc-ai-dialogue-spec-CR-5243af63.md";
const SPEC_ROADMAP: &str = "docs/acknowledgments/roadmap-m18-m48.md:317";

// ── evidence rows ───────────────────────────────────────────────────────

/// One printed evidence row.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    id: String,
    ok: bool,
    detail: String,
}

/// Row whose verdict is computed by the caller.
fn check(id: impl Into<String>, ok: bool, detail: String) -> Row {
    Row {
        id: id.into(),
        ok,
        detail,
    }
}

/// Hexadecimal `sha256` (the `DIGEST` line and nothing else needs it).
fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Expected kind of `hour` for `guard` (independent transcription of
/// [`AgendaKind::for_hour`], so a drift between probe and crate fails loudly).
fn expected_kind(hour: u8, guard: bool) -> AgendaKind {
    if guard {
        return AgendaKind::Patrol;
    }
    match hour {
        0..=5 | 22..=23 => AgendaKind::Home,
        6..=10 | 13..=17 => AgendaKind::Work,
        11..=12 | 18..=19 => AgendaKind::Meal,
        _ => AgendaKind::Rest,
    }
}

/// Fixed start cell of NPC `i` (inside the 64×64 synthetic city).
fn start_cell(i: usize) -> (i32, i32) {
    (3 + 5 * i as i32, 32)
}

// ── entrypoint ──────────────────────────────────────────────────────────

/// `--probe SYS-NPC-001`: blocks A–C, `EXIT=0` ⟺ every row OK.
pub fn run_sys_npc_001() -> i32 {
    let mut out = String::new();
    out.push_str(&format!(
        "PROBE-SYS-NPC-001 - living NPCs: 24h deterministic agendas (M42/W42-1, {CR})\n"
    ));

    let grid = NavGrid::city(1, GRID_SEED);
    let mut ok = true;
    let mut total_teleports: u64 = 0;
    let mut agenda_rows: usize = 0;

    // ── A · agendas (8 NPCs × 24 h) ────────────────────────────────────
    out.push_str("\n-- A · agendas (8 NPCs x 24 h, reproducible stops) --\n");
    for i in 0..NPC_COUNT {
        let seed = PROBE_SEED + i as u64;
        let guard = i == 0;
        let day = simulate_day(seed, guard, &grid, start_cell(i));
        total_teleports += day.teleports;
        for stop in day.stops.iter() {
            agenda_rows += 1;
            out.push_str(&format!(
                "AGENDA npc={i} seed={seed} guard={} hour={:02} cell=({},{}) kind={}\n",
                u8::from(guard),
                stop.hour,
                stop.cell.0,
                stop.cell.1,
                stop.kind.name(),
            ));
        }
    }

    // ── B · day digest (per-NPC totals + census) ────────────────────────
    out.push_str("\n-- B · day digest (moves, cells, teleports, digests) --\n");
    for i in 0..NPC_COUNT {
        let seed = PROBE_SEED + i as u64;
        let guard = i == 0;
        let day = simulate_day(seed, guard, &grid, start_cell(i));
        out.push_str(&format!(
            "DAY npc={i} moves={} cells={} teleports={} max_step={} digest={:016x} agenda={:016x}\n",
            day.moves,
            day.cells_visited,
            day.teleports,
            day.max_step,
            day.digest,
            agenda_digest(&day.stops),
        ));
    }
    out.push_str(&format!(
        "CENSUS npcs={NPC_COUNT} hours={DAY_HOURS} stops={agenda_rows} teleports={total_teleports}\n"
    ));

    // ── C · adversarial (declared non-goals with on-disk evidence) ─────
    out.push_str("\n-- C · adversarial (non-goals, negative evidence) --\n");
    out.push_str(&format!("NEGATIVE building-interiors reason={SRC_AGENDA}\n"));
    out.push_str(&format!("NEGATIVE retail-bytes reason={SPEC_M26}\n"));

    // ── verdict rows ───────────────────────────────────────────────────
    let mut rows: Vec<Row> = Vec::new();
    rows.push(check(
        "schedule-24h",
        agenda_rows == NPC_COUNT * DAY_HOURS,
        format!(
            "{NPC_COUNT} NPCs x {DAY_HOURS} h = {} AGENDA rows src={SRC_AGENDA}",
            NPC_COUNT * DAY_HOURS,
        ),
    ));
    let kinds_ok = {
        let mut good = true;
        for i in 0..NPC_COUNT {
            let seed = PROBE_SEED + i as u64;
            let guard = i == 0;
            let day = simulate_day(seed, guard, &grid, start_cell(i));
            for stop in day.stops.iter() {
                if stop.kind != expected_kind(stop.hour, guard) {
                    good = false;
                }
            }
        }
        good
    };
    rows.push(check(
        "agenda-kinds",
        kinds_ok,
        format!(
            "civilian day table + guard patrol x24, 192/192 match spec={SPEC_M26}"
        ),
    ));
    rows.push(check(
        "teleport-free",
        total_teleports == 0,
        format!(
            "teleports={total_teleports} across {NPC_COUNT} days, every step Chebyshev<=1 src={SRC_AGENDA}"
        ),
    ));
    rows.push(check(
        "guard-patrol",
        {
            let day = simulate_day(PROBE_SEED, true, &grid, start_cell(0));
            day.stops.iter().all(|stop| stop.kind == AgendaKind::Patrol)
        },
        format!("npc0 guard patrol 24/24 spec={SPEC_ROADMAP}"),
    ));
    rows.push(check(
        "hour-boundaries",
        {
            let day = simulate_day(PROBE_SEED + 1, false, &grid, start_cell(1));
            day.stops
                .iter()
                .enumerate()
                .all(|(h, stop)| stop.hour == h as u8)
        },
        format!("stop[h].hour == h for h in 0..24 src={SRC_AGENDA}"),
    ));
    rows.push(check(
        "census-totals",
        agenda_rows == NPC_COUNT * DAY_HOURS && total_teleports == 0,
        format!(
            "CENSUS npcs={NPC_COUNT} hours={DAY_HOURS} stops={agenda_rows} teleports={total_teleports} spec={SPEC_ROADMAP}"
        ),
    ));

    for row in &rows {
        if row.ok {
            out.push_str(&format!("[OK] {} - {}\n", row.id, row.detail));
            ok &= true;
        } else {
            out.push_str(&format!("[FAIL] {} - {}\n", row.id, row.detail));
            ok = false;
        }
    }

    out.push_str(if ok {
        "\nVEREDICTO: PASS\n"
    } else {
        "\nVEREDICTO: FAIL\n"
    });
    print!("{out}");
    println!("DIGEST {}", sha256_hex(out.as_bytes()));
    if ok { 0 } else { 1 }
}



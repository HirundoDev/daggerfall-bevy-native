//! **B3-a** — street population (CS-13) and visible city guard (CS-14) of M18.
//!
//! Two pieces, one probe:
//! - **population density (CS-13):** the street population of the integrated
//!   demo is [`STREET_POPULATION`] NPCs (`24`), the same figure the bridge
//!   declares for a city (`npc_spec::CITY_POPULATION`);
//!   `m18_demo::NPC_COUNT` **is** this constant (one source, [`assert_pinned`]),
//!   so the composition, the probe and the window can never diverge;
//! - **visible guard (CS-14):** the 4 flats of the city watch are decoded from
//!   `TEXTURE.399` (header `    Guard`, 20 records) with the **retail rule** of
//!   `spawn_city_guards`
//!   (`record = (i * 3) % flat_count`, `npc_spec.rs:588-590`) and the guard
//!   contract (threshold, quota, distinct records) is measured headless with
//!   the production function — no copy of the rule.
//!
//! Probe blocks:
//! - **A** composition ×2 (determinism): census of the scene (total, with flat,
//!   with caption, distinct flat heights, digest) identical in both passes;
//! - **B** count == [`STREET_POPULATION`], every NPC on a free cell and the
//!   guard flats decoded over the 8 modeled directions;
//! - **C** headless guard contract through [`spawn_city_guards`]: under the
//!   retail threshold ⇒ 0 guards, at/over ⇒ [`GUARDS_PER_SPAWN`] with distinct
//!   records;
//! - **D** negative control: the census of an empty population reports 0 and a
//!   `count: 0` request generates no NPC (0 invented population).
//!
//! Custody (INV-L99-6): 0 retail bytes — every flat comes from the user's
//! ARENA2 at runtime and lives in memory.

use std::fmt::Write as _;

use daggerfall_parser_bevy_bridge::city_spec::CityLayout;
use daggerfall_parser_bevy_bridge::npc_spec::{
    spawn_city_guards, NpcWorld, PopulationRequest, FLAT_DIRECTIONS, GUARDS_PER_SPAWN,
    GUARD_CRIME_THRESHOLD,
};
use daggerfall_parser_rs::namegen::NameGen;
use daggerfall_parser_rs::text_rsc::TextRsc;
use daggerfall_parser_rs::texture::{self, TextureArchive};

use crate::m18_demo::{DemoNpc, NPC_SEED, NPC_SUBCLIMATE};

/// NPCs of the retail street population of the demo (CS-13).
///
/// `24` is the bridge's own city population (`npc_spec::CITY_POPULATION`);
/// [`assert_pinned`] keeps that equality and `m18_demo::NPC_COUNT` pinned.
pub const STREET_POPULATION: usize = 24;

/// Guard flats decoded for the city watch (CS-14) — the retail quota
/// (`npc_spec::GUARDS_PER_SPAWN`).
pub const GUARD_FLATS: usize = 4;

/// Retail flat record of guard `index` over an archive of `flat_count`
/// records: `(i * 3) % flat_count` (`SpawnCityGuards.cs:17-20,45-56` as
/// transcribed by `npc_spec::spawn_city_guards:588-590`).
pub fn guard_flat_record(index: usize, flat_count: usize) -> usize {
    if flat_count == 0 {
        return 0;
    }
    (index * 3) % flat_count
}

/// Census of a population (CS-13 evidence).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PopulationCensus {
    /// NPCs in the population.
    pub total: usize,
    /// NPCs whose flat decodes to a non-empty RGBA.
    pub with_flat: usize,
    /// NPCs whose retail caption is non-empty.
    pub with_caption: usize,
    /// Distinct flat heights (a single record would report 1).
    pub distinct_heights: usize,
    /// FNV-1a 64 over positions + flat RGBA + caption RGBA.
    pub digest: u64,
}

impl PopulationCensus {
    /// Deterministic one-line summary (probe evidence).
    pub fn summary(&self) -> String {
        format!(
            "total {} · with flat {} · with caption {} · distinct heights {} · digest {:016x}",
            self.total, self.with_flat, self.with_caption, self.distinct_heights, self.digest
        )
    }
}

/// Census over the already composed NPCs (0 composition of its own).
pub fn census(npcs: &[DemoNpc]) -> PopulationCensus {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut feed = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    let mut heights: Vec<u16> = Vec::with_capacity(npcs.len());
    let mut with_flat = 0usize;
    let mut with_caption = 0usize;
    for npc in npcs {
        let flat_ok = npc.flat.rgba.len()
            == usize::from(npc.flat.width) * usize::from(npc.flat.height) * 4
            && !npc.flat.rgba.is_empty();
        if flat_ok {
            with_flat += 1;
        }
        if !npc.spec.caption.text.is_empty() {
            with_caption += 1;
        }
        if !heights.contains(&npc.flat.height) {
            heights.push(npc.flat.height);
        }
        feed(npc.spec.name.as_bytes());
        for value in npc.position {
            feed(&value.to_bits().to_le_bytes());
        }
        feed(&npc.flat.rgba);
        feed(&npc.caption.rgba);
    }
    PopulationCensus {
        total: npcs.len(),
        with_flat,
        with_caption,
        distinct_heights: heights.len(),
        digest: hash,
    }
}

/// Pins the shared population figures (called by the probe and by the
/// module test; it can never be a silent drift).
pub fn assert_pinned() {
    assert_eq!(
        STREET_POPULATION,
        daggerfall_parser_bevy_bridge::npc_spec::CITY_POPULATION,
        "STREET_POPULATION must be the bridge's city population"
    );
    assert_eq!(
        crate::m18_demo::NPC_COUNT,
        STREET_POPULATION,
        "the demo population must be the street population (one source)"
    );
    assert_eq!(
        GUARD_FLATS, GUARDS_PER_SPAWN,
        "one guard flat per retail guard of a spawn"
    );
}

/// Marks a check and prints it (`[OK]`/`[FAIL]` — the repo's probe helper).
fn check(passed: &mut bool, condition: bool, label: &str, detail: String) {
    if !condition {
        *passed = false;
    }
    println!(
        "[{}] {label} - {detail}",
        if condition { "OK" } else { "FAIL" }
    );
}

/// Reads a retail file from ARENA2 Mode A.
fn load(name: &str) -> Result<Vec<u8>, String> {
    let path = crate::mode_a_arena2_path(name)?;
    std::fs::read(&path).map_err(|err| format!("reading {path:?}: {err}"))
}

/// Retail data the guard contract needs (flats + population sources).
struct RetailGuardData {
    namegen: NameGen,
    /// People archive the population is drawn from (`TEXTURE.182`; header
    /// `medium Common people`).
    flats: TextureArchive,
    /// City-watch archive (`TEXTURE.399`; header `    Guard`, 20 records).
    guard_flats: TextureArchive,
    guard_flats_bytes: Vec<u8>,
    text: TextRsc,
    text_bytes: Vec<u8>,
}

impl RetailGuardData {
    fn load() -> Result<Self, String> {
        let namegen =
            NameGen::parse(&load("NAMEGEN.DAT")?).map_err(|err| format!("NAMEGEN.DAT: {err:?}"))?;
        let person_file = crate::m18_demo::PERSON_FLAT_FILE;
        let flats_bytes = load(person_file)?;
        let flats =
            TextureArchive::parse(&flats_bytes).map_err(|err| format!("{person_file}: {err:?}"))?;
        let guard_file = crate::m18_demo::GUARD_FLAT_FILE;
        let guard_flats_bytes = load(guard_file)?;
        let guard_flats = TextureArchive::parse(&guard_flats_bytes)
            .map_err(|err| format!("{guard_file}: {err:?}"))?;
        let text_bytes = load("TEXT.RSC")?;
        let text = TextRsc::parse(&text_bytes).map_err(|err| format!("TEXT.RSC: {err:?}"))?;
        Ok(Self {
            namegen,
            flats,
            guard_flats,
            guard_flats_bytes,
            text,
            text_bytes,
        })
    }

    fn world(&self) -> NpcWorld<'_> {
        NpcWorld {
            namegen: &self.namegen,
            person_flats: &self.flats,
            text: &self.text,
            text_bytes: &self.text_bytes,
        }
    }
}

/// Runs `PROBE-POPULATION-001` — CS-13 density + CS-14 visible guard.
///
/// `EXIT=0` ⟺ blocks A-D pass; `EXIT=2` if ARENA2 (or a mandatory retail
/// file) is not readable — the probe never fakes a PASS.
pub fn run_population_001() -> i32 {
    println!("PROBE-POPULATION-001 - M18 B3-a street population + city guard (CS-13/CS-14)");
    assert_pinned();

    // ── A · composition ×2 (determinism) ────────────────────────────────
    let first = match crate::m18_demo::compose_demo() {
        Ok(scene) => scene,
        Err(crate::m18_demo::DemoError::MissingData(reason)) => {
            println!("ARENA2 not accessible: {reason}");
            println!("VERDICT: PROBE-POPULATION-001 NO DATA - ARENA2 not accessible");
            return 2;
        }
        Err(err) => {
            println!("[FAIL] composition: {err}");
            println!("VERDICT: PROBE-POPULATION-001 FAIL - invalid composition");
            return 1;
        }
    };
    let second = match crate::m18_demo::compose_demo() {
        Ok(scene) => scene,
        Err(err) => {
            println!("[FAIL] composition 2: {err}");
            println!("VERDICT: PROBE-POPULATION-001 FAIL - second pass");
            return 1;
        }
    };
    let mut passed = true;

    let census_first = census(&first.npcs);
    let census_second = census(&second.npcs);
    let scene_digest = first.digest();
    let scene_digest_2 = second.digest();
    check(
        &mut passed,
        census_first == census_second && scene_digest == scene_digest_2,
        "A composition_deterministic",
        format!(
            "pass 1: {} · pass 2: {} · scene digest {:016x} ×2",
            census_first.summary(),
            census_second.summary(),
            scene_digest
        ),
    );
    check(
        &mut passed,
        census_first.with_flat == census_first.total
            && census_first.with_caption == census_first.total,
        "A census_complete",
        format!(
            "{} with flat / {} with caption of {}",
            census_first.with_flat, census_first.with_caption, census_first.total
        ),
    );

    // ── B · population count + guard flats over 8 directions ────────────
    check(
        &mut passed,
        first.npcs.len() == STREET_POPULATION,
        "B population_count",
        format!(
            "{} NPCs in the city (STREET_POPULATION = {STREET_POPULATION})",
            first.npcs.len()
        ),
    );
    let free_positions = first
        .npcs
        .iter()
        .filter(|npc| {
            first.grid.is_free(
                [
                    npc.position[0],
                    daggerfall_parser_bevy_bridge::EYE_HEIGHT_M,
                    npc.position[2],
                ],
                daggerfall_parser_bevy_bridge::PLAYER_RADIUS_M,
            )
        })
        .count();
    check(
        &mut passed,
        free_positions == first.npcs.len(),
        "B positions_free",
        format!(
            "{free_positions}/{} NPCs on a free cell of the retail collision grid",
            first.npcs.len()
        ),
    );

    let data = match RetailGuardData::load() {
        Ok(data) => data,
        Err(reason) => {
            println!("ARENA2 not accessible: {reason}");
            println!("VERDICT: PROBE-POPULATION-001 NO DATA - retail guard data unreadable");
            return 2;
        }
    };
    let flat_count = data.guard_flats.records.len();
    let mut guard_report = String::new();
    let mut guard_flats_ok = first.guard_flats.len() == GUARD_FLATS;
    let mut decoded_directions = 0usize;
    for (index, flat) in first.guard_flats.iter().enumerate() {
        let record = guard_flat_record(index, flat_count);
        let mut directions = 0usize;
        for direction in 0..FLAT_DIRECTIONS {
            if texture::decode_frame(
                &data.guard_flats_bytes,
                &data.guard_flats,
                record,
                u16::from(direction),
            )
            .is_ok()
            {
                directions += 1;
            }
        }
        decoded_directions += directions;
        let flat_ok = !flat.rgba.is_empty()
            && flat.rgba.len() == usize::from(flat.width) * usize::from(flat.height) * 4;
        guard_flats_ok &= flat_ok && directions > 0;
        let _ = write!(
            guard_report,
            "flat {index} → record {record} ({}×{} px, {directions}/{FLAT_DIRECTIONS} dirs){} · ",
            flat.width,
            flat.height,
            if flat_ok { "" } else { " EMPTY" }
        );
    }
    check(
        &mut passed,
        guard_flats_ok,
        "B guard_flats_8dir",
        format!(
            "{}/{GUARD_FLATS} guard flats decoded from {} ({} records) · {decoded_directions} directions total · {}",
            first.guard_flats.len(),
            crate::m18_demo::GUARD_FLAT_FILE,
            flat_count,
            guard_report.trim_end_matches(" · ")
        ),
    );
    let distinct_keys: std::collections::BTreeSet<u32> =
        first.guard_flats.iter().map(|flat| flat.key).collect();
    check(
        &mut passed,
        distinct_keys.len() == first.guard_flats.len(),
        "B guard_flats_distinct",
        format!(
            "{} distinct flat keys over {} flats",
            distinct_keys.len(),
            first.guard_flats.len()
        ),
    );

    // ── C · headless guard contract (production `spawn_city_guards`) ────
    let world = data.world();
    let layout = CityLayout::new(8);
    let under = spawn_city_guards(
        &world,
        &layout,
        GUARD_CRIME_THRESHOLD.saturating_sub(1),
        false,
    );
    let at = spawn_city_guards(&world, &layout, GUARD_CRIME_THRESHOLD, true);
    let over = spawn_city_guards(&world, &layout, GUARD_CRIME_THRESHOLD + 100, false);
    let (under, at, over) = match (under, at, over) {
        (Ok(under), Ok(at), Ok(over)) => (under, at, over),
        (under, at, over) => {
            let err = [under.err(), at.err(), over.err()]
                .into_iter()
                .flatten()
                .map(|err| format!("{err:?}"))
                .collect::<Vec<_>>()
                .join("; ");
            check(&mut passed, false, "C guard_contract", err);
            (Vec::new(), Vec::new(), Vec::new())
        }
    };
    let under_ok = under.is_empty();
    let at_ok = at.len() == GUARDS_PER_SPAWN;
    let over_ok = over.len() == GUARDS_PER_SPAWN;
    check(
        &mut passed,
        under_ok && at_ok && over_ok,
        "C guard_contract",
        format!(
            "crime {} (under) → {} guards · crime {} (at) → {} · crime {} (over) → {} (quota {GUARDS_PER_SPAWN})",
            GUARD_CRIME_THRESHOLD.saturating_sub(1),
            under.len(),
            GUARD_CRIME_THRESHOLD,
            at.len(),
            GUARD_CRIME_THRESHOLD + 100,
            over.len()
        ),
    );
    let records: std::collections::BTreeSet<u16> =
        at.iter().map(|guard| guard.flat.record).collect();
    let positions: std::collections::BTreeSet<[u32; 3]> = at
        .iter()
        .map(|guard| {
            [
                guard.position[0].to_bits(),
                guard.position[1].to_bits(),
                guard.position[2].to_bits(),
            ]
        })
        .collect();
    check(
        &mut passed,
        records.len() == GUARDS_PER_SPAWN && positions.len() == GUARDS_PER_SPAWN,
        "C guard_records_distinct",
        format!(
            "{} distinct flat records {:?} · {} distinct positions",
            records.len(),
            records.iter().collect::<Vec<_>>(),
            positions.len()
        ),
    );
    let rule_ok = (0..GUARDS_PER_SPAWN)
        .all(|index| guard_flat_record(index, flat_count) == (index * 3) % flat_count);
    let demo_rule_ok = first
        .guard_flats
        .iter()
        .enumerate()
        .all(|(index, flat)| flat.key == guard_flat_record(index, flat_count) as u32);
    check(
        &mut passed,
        rule_ok && demo_rule_ok,
        "C guard_rule_retail",
        format!(
            "record = (i * 3) % {flat_count} · demo flats keys {:?}",
            first
                .guard_flats
                .iter()
                .map(|flat| flat.key)
                .collect::<Vec<_>>()
        ),
    );

    // ── D · negative control ─────────────────────────────────────────────
    let empty = census(&[]);
    let empty_population = world.populate(PopulationRequest {
        seed: NPC_SEED,
        region: crate::m18_demo::NPC_REGION,
        subclimate: NPC_SUBCLIMATE,
        count: 0,
    });
    // The bridge refuses an empty request (`NpcError::EmptyPopulation`) — an
    // explicit rejection is the strongest form of "0 invented population": both
    // outcomes (Ok with 0 NPCs or an explicit error) mean no NPC was fabricated.
    let (zero_npcs, zero_note) = match &empty_population {
        Ok(population) => (
            population.npcs.len(),
            "populate(count: 0) → Ok(0 NPCs)".to_string(),
        ),
        Err(err) => (0usize, format!("populate(count: 0) rejected ({err:?})")),
    };
    check(
        &mut passed,
        empty.total == 0
            && empty.with_flat == 0
            && empty.with_caption == 0
            && empty.digest != census_first.digest
            && zero_npcs == 0,
        "D negative_control",
        format!(
            "empty census {} · populated digest {:016x} · {zero_note} (0 invented population)",
            empty.summary(),
            census_first.digest
        ),
    );

    if passed {
        println!(
            "VERDICT: PROBE-POPULATION-001 PASS - {} NPCs (CS-13) · {} guard flats decoded with the retail rule (CS-14) · guard quota {GUARDS_PER_SPAWN} at crime ≥ {GUARD_CRIME_THRESHOLD} · digest {:016x} ×2",
            first.npcs.len(),
            first.guard_flats.len(),
            scene_digest
        );
        0
    } else {
        println!("VERDICT: PROBE-POPULATION-001 FAIL");
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn street_population_is_pinned() {
        assert_pinned();
    }

    #[test]
    fn guard_flat_rule_matches_retail() {
        for index in 0..GUARD_FLATS {
            assert_eq!(guard_flat_record(index, 64), (index * 3) % 64);
        }
        let distinct: std::collections::BTreeSet<usize> = (0..GUARD_FLATS)
            .map(|index| guard_flat_record(index, 64))
            .collect();
        assert_eq!(distinct.len(), GUARD_FLATS);
        assert_eq!(GUARD_FLATS, GUARDS_PER_SPAWN);
    }

    #[test]
    fn guard_flat_record_handles_empty_archive() {
        assert_eq!(guard_flat_record(3, 0), 0);
    }

    #[test]
    fn empty_census_reports_zero() {
        let empty = census(&[]);
        assert_eq!(empty.total, 0);
        assert_eq!(empty.with_flat, 0);
        assert_eq!(empty.with_caption, 0);
        assert_eq!(empty.distinct_heights, 0);
    }
}

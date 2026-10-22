//! M15 — anti-screen probe for the NPC population (`--probe NPC-001`,
//! DoD §f / WI-7 / REQ-L99-047).
//!
//! **PROBE-NPC-001** verifies, over the user's real ARENA2 (Mode A, 0 retail
//! bytes in git — INV-L99-6):
//!
//! 1. `NAMEGEN.DAT` parsed (8,378 B ⇒ 11 banks / 785 parts, H1);
//! 2. **18 golden name vectors** (seed + bank + gender ⇒ exact string)
//!    derived from the DFU chain `srand(seed)` + `NameHelper.FullName`
//!    (`NameHelper.cs:188-289`), and same seed ×2 ⇒ identical string;
//! 3. monster bank and Nord bank (H3: suffix not pinned) return a **typed
//!    error**, never a synthetic name;
//! 4. deterministic population: same seed ×2 ⇒ **same digest** (FNV-1a64 of
//!    the canonical form); race (subclimate) and bank (region) as independent
//!    axes;
//! 5. 0 NPCs without a flat resolvable against `TEXTURE.256` (15 image
//!    records) and direction 0…7 modelled (H2: the 8-dir geometry is
//!    `NEGATIVE`);
//! 6. **≥4 retail captions** from `TEXT.RSC`: the id's text matches the file
//!    byte-by-byte (ids 4442/4446/4653/4657);
//! 7. guards: 0 below the crime threshold, full quota from it upwards (contract
//!    of `SpawnCityGuards.cs:17-20,45-56`; threshold = M15 decision, H4);
//! 8. `create npc at` / `place npc`: reserved site, placement at a valid
//!    coordinate, and rejection of unknown symbol / individual-at-home /
//!    coordinate outside the city (`PlaceNpc.cs:76-105`).
//!
//! Without `NAMEGEN.DAT`, `TEXTURE.256` or `TEXT.RSC` the probe exits with
//! **exit 2** and the literal reason: 0 invented names/captions.

use daggerfall_parser_bevy_bridge::npc_spec::{
    create_npc_at, place_npc, region, spawn_city_guards, CaptionSource, NpcWorld, Place,
    PopulationRequest, QuestPerson, QuestRegistry, CITY_POPULATION, FLAT_DIRECTIONS,
    GUARD_CRIME_THRESHOLD, GUARDS_PER_SPAWN, PERSON_FLAT_ARCHIVE, PLACE_NPC_DEFAULT_MARKER,
};
use daggerfall_parser_bevy_bridge::CityLayout;
use daggerfall_parser_rs::namegen::{BankType, Gender, NameGen, NameGenError};
use daggerfall_parser_rs::text_rsc::TextRsc;
use daggerfall_parser_rs::texture::{RecordKind, TextureArchive};

/// Name golden: `srand(seed)` + `FullName(bank, gender)` ⇒ string.
struct GoldenName {
    seed: u32,
    bank: BankType,
    gender: Gender,
    name: &'static str,
}

/// 18 vectors from spec M15 §7.2/§7.3, re-verified against `NAMEGEN.DAT` in
/// this lane (18/18 match; 0 copied by eye).
/// The first 13 are population rows; the last 5 are banks unreachable by
/// region (`§7.3`).
const GOLDEN: [GoldenName; 18] = [
    GoldenName { seed: 1000, bank: BankType::Breton, gender: Gender::Male, name: "Agrore Wicking" },
    GoldenName { seed: 1000, bank: BankType::Breton, gender: Gender::Female, name: "Chrystanna Wicking" },
    GoldenName { seed: 12345, bank: BankType::Breton, gender: Gender::Male, name: "Gondyrick Kinghouse" },
    GoldenName { seed: 0, bank: BankType::Redguard, gender: Gender::Female, name: "Bojja" },
    GoldenName { seed: 424242, bank: BankType::Redguard, gender: Gender::Male, name: "Tercom" },
    GoldenName { seed: 424242, bank: BankType::Redguard, gender: Gender::Female, name: "Tercba" },
    GoldenName { seed: 1, bank: BankType::Redguard, gender: Gender::Male, name: "C'ozom" },
    GoldenName { seed: 999, bank: BankType::Redguard, gender: Gender::Female, name: "Chird-e" },
    GoldenName { seed: 777, bank: BankType::Breton, gender: Gender::Male, name: "Andane Moorwing" },
    GoldenName { seed: 888_888, bank: BankType::Breton, gender: Gender::Female, name: "Chrystyna Copperhouse" },
    GoldenName { seed: 20_261_020, bank: BankType::Breton, gender: Gender::Male, name: "Perore Gaerhouse" },
    GoldenName { seed: 314_159, bank: BankType::Redguard, gender: Gender::Female, name: "Saochtha" },
    GoldenName { seed: 99_991, bank: BankType::Redguard, gender: Gender::Male, name: "Fajer" },
    GoldenName { seed: 777, bank: BankType::DarkElf, gender: Gender::Male, name: "Nisseth Grulen" },
    GoldenName { seed: 4242, bank: BankType::HighElf, gender: Gender::Female, name: "Psytha Stormaire" },
    GoldenName { seed: 31_415, bank: BankType::WoodElf, gender: Gender::Male, name: "Karodell Shadyvale" },
    GoldenName { seed: 99_991, bank: BankType::Khajiit, gender: Gender::Female, name: "Satima Soloni" },
    GoldenName { seed: 20_261_020, bank: BankType::Imperial, gender: Gender::Male, name: "Caligerius Theocles" },
];

/// Canonical probe seed (lane date, deterministic).
const POP_SEED: u32 = 20_261_020;
/// Canonical region (17 = Daggerfall → Breton bank).
const POP_REGION: u8 = 17;
/// Canonical subclimate (231 = Woodlands → `People = Breton`).
const POP_SUBCLIMATE: u8 = 231;

/// Reads a file from ARENA2 Mode A or returns the reason.
fn load(name: &str) -> Result<Vec<u8>, String> {
    let path = crate::mode_a_arena2_path(name)?;
    std::fs::read(&path).map_err(|err| format!("reading {path:?}: {err}"))
}

/// Runs PROBE-NPC-001 — NPC population (M15).
pub fn run_npc_001() -> i32 {
    println!(
        "PROBE-NPC-001 - deterministic NPC population (M15, CR-46a43aba, \
         REQ-L99-047 · DoD §f / WI-7)"
    );

    // ── 1. Mode A custody: the 3 retail data files must exist ────────────
    let namegen_bytes = match load("NAMEGEN.DAT") {
        Ok(bytes) => bytes,
        Err(reason) => {
            println!("ARENA2 not accessible (NAMEGEN.DAT): {reason}");
            return 2;
        }
    };
    let texture_bytes = match load("TEXTURE.256") {
        Ok(bytes) => bytes,
        Err(reason) => {
            println!("ARENA2 not accessible (TEXTURE.256): {reason}");
            return 2;
        }
    };
    let rsc_bytes = match load("TEXT.RSC") {
        Ok(bytes) => bytes,
        Err(reason) => {
            println!("ARENA2 not accessible (TEXT.RSC): {reason}");
            return 2;
        }
    };

    let mut all_ok = true;

    // ── 2. NAMEGEN.DAT (H1: measured binary layout) ──────────────────────
    let namegen = match NameGen::parse(&namegen_bytes) {
        Ok(namegen) => namegen,
        Err(err) => {
            println!("[FAIL] namegen_parse - {err}");
            println!("VERDICT: PROBE-NPC-001 FAIL - NAMEGEN.DAT does not parse");
            return 1;
        }
    };
    let parts = namegen.parts_total();
    let header_ok = namegen_bytes.len() == 8_378 && parts == 785;
    println!(
        "[{}] namegen_parse - NAMEGEN.DAT {} B · 11 banks · {} parts (retail expected 8,378 B / 785)",
        if header_ok { "OK" } else { "FAIL" },
        namegen_bytes.len(),
        parts,
    );
    all_ok &= header_ok;
    for bank in BankType::CITY_REACHABLE {
        println!(
            "      bank {}: {} sets · {} parts",
            bank.name(),
            namegen.bank(bank).sets.len(),
            namegen.bank(bank).parts_total(),
        );
    }

    // ── 3. Name goldens + determinism ─────────────────────────────
    let mut matched = 0usize;
    for golden in &GOLDEN {
        let got = namegen.full_name(golden.bank, golden.gender, golden.seed);
        let ok = got.as_deref() == Ok(golden.name);
        if ok {
            matched += 1;
        } else {
            println!(
                "[FAIL] name_golden seed={} {} {} - got {got:?} want {:?}",
                golden.seed,
                golden.bank.name(),
                golden.gender.name(),
                golden.name,
            );
        }
        all_ok &= ok;
    }
    println!(
        "[{}] name_golden - {matched}/{} exact vectors (seed+bank+gender ⇒ name)",
        if matched == GOLDEN.len() { "OK" } else { "FAIL" },
        GOLDEN.len(),
    );

    let repeat_a = namegen.full_name(BankType::Breton, Gender::Female, 1000);
    let repeat_b = namegen.full_name(BankType::Breton, Gender::Female, 1000);
    let repeat_ok = repeat_a == repeat_b && repeat_a.is_ok();
    println!(
        "[{}] name_same_seed_twice - {repeat_a:?} ×2 identical",
        if repeat_ok { "OK" } else { "FAIL" },
    );
    all_ok &= repeat_ok;

    let monster = namegen.full_name(BankType::Monster1, Gender::Male, 1);
    let nord = namegen.full_name(BankType::Nord, Gender::Female, 1);
    let errors_ok = monster == Err(NameGenError::UnsupportedBank { bank: 8 })
        && nord == Err(NameGenError::NordSuffixUnavailable);
    println!(
        "[{}] name_invalid_banks - Monster1 → {monster:?} · Nord → {nord:?} (0 panic, 0 synthetic name)",
        if errors_ok { "OK" } else { "FAIL" },
    );
    all_ok &= errors_ok;

    // ── 4. Person flats (TEXTURE.256) ───────────────────────────────
    let flats = match TextureArchive::parse(&texture_bytes) {
        Ok(archive) => archive,
        Err(err) => {
            println!("[FAIL] flats_parse - TEXTURE.256: {err:?}");
            println!("VERDICT: PROBE-NPC-001 FAIL - TEXTURE.256 does not parse");
            return 1;
        }
    };
    let images = flats
        .records
        .iter()
        .filter(|header| matches!(header.kind, RecordKind::Image(_)))
        .count();
    let flats_ok = flats.records.len() == 15 && images == 15 && flats.name.contains("Imp");
    println!(
        "[{}] flats_archive - TEXTURE.256 (file {}) · {} records · {images} image · 8-dir modelled 0…{}",
        if flats_ok { "OK" } else { "FAIL" },
        PERSON_FLAT_ARCHIVE,
        flats.records.len(),
        FLAT_DIRECTIONS - 1,
    );
    all_ok &= flats_ok;

    // ── 5. TEXT.RSC (retail captions) ───────────────────────────────────
    let text = match TextRsc::parse(&rsc_bytes) {
        Ok(text) => text,
        Err(err) => {
            println!("[FAIL] text_rsc_parse - {err}");
            println!("VERDICT: PROBE-NPC-001 FAIL - TEXT.RSC does not parse");
            return 1;
        }
    };
    let rsc_ok = text.record_count() == 1408 && text.header_length == 8_454;
    println!(
        "[{}] text_rsc_parse - TEXT.RSC {} B · header {} · {} records",
        if rsc_ok { "OK" } else { "FAIL" },
        rsc_bytes.len(),
        text.header_length,
        text.record_count(),
    );
    all_ok &= rsc_ok;

    let mut captions_retail = 0usize;
    for occupation in daggerfall_parser_bevy_bridge::npc_spec::OCCUPATIONS {
        let raw_ok = text
            .record_raw(&rsc_bytes, occupation.text_id)
            .is_some_and(|raw| raw.last() == Some(&0xFE));
        let got = text.record_ascii(&rsc_bytes, occupation.text_id);
        let ok = raw_ok && got == Some(occupation.text);
        if ok {
            captions_retail += 1;
        } else {
            println!(
                "[FAIL] caption_{} - TEXT.RSC[{}] = {got:?} (expected {:?})",
                occupation.key, occupation.text_id, occupation.text,
            );
        }
        all_ok &= ok;
    }
    let captions_ok = captions_retail >= 4;
    println!(
        "[{}] captions_retail - {captions_retail}/4 TEXT.RSC records with exact retail text (4442 priest · 4446 assassin · 4653 mage · 4657 merchant)",
        if captions_ok { "OK" } else { "FAIL" },
    );
    all_ok &= captions_ok;

    // ── 6. Deterministic population ───────────────────────────────────────
    let world = NpcWorld {
        namegen: &namegen,
        person_flats: &flats,
        text: &text,
        text_bytes: &rsc_bytes,
    };
    let request = PopulationRequest {
        seed: POP_SEED,
        region: POP_REGION,
        subclimate: POP_SUBCLIMATE,
        count: CITY_POPULATION,
    };
    let population = match world.populate(request) {
        Ok(population) => population,
        Err(err) => {
            println!("[FAIL] population - {err}");
            println!("VERDICT: PROBE-NPC-001 FAIL - population not generable");
            return 1;
        }
    };
    let population_again = world.populate(request).expect("same request");
    let digest = population.digest();
    let digest_again = population_again.digest();
    let deterministic = digest == digest_again && population == population_again;
    println!(
        "[{}] population_deterministic - seed {POP_SEED} ×2 ⇒ digest 0x{digest:016x} == 0x{digest_again:016x} · {} NPCs",
        if deterministic { "OK" } else { "FAIL" },
        population.npcs.len(),
    );
    all_ok &= deterministic;

    let other = world
        .populate(PopulationRequest {
            seed: POP_SEED + 1,
            ..request
        })
        .expect("alternate seed");
    let seed_sensitive = other.digest() != digest;
    println!(
        "[{}] population_seed_sensitive - seed {} ⇒ digest 0x{:016x} ≠ 0x{digest:016x}",
        if seed_sensitive { "OK" } else { "FAIL" },
        POP_SEED + 1,
        other.digest(),
    );
    all_ok &= seed_sensitive;

    let region_ok = region(POP_REGION as usize)
        .is_some_and(|info| info.name == "Daggerfall" && info.bank == BankType::Breton);
    let axes_ok = population
        .npcs
        .iter()
        .all(|npc| npc.name_bank == BankType::Breton && npc.race == daggerfall_parser_bevy_bridge::npc_spec::Race::Breton);
    let mountain = world
        .populate(PopulationRequest {
            subclimate: 226,
            ..request
        })
        .expect("Mountain subclimate");
    let nord_axis_ok = mountain.npcs.iter().all(|npc| {
        npc.race == daggerfall_parser_bevy_bridge::npc_spec::Race::Nord
            && npc.name_bank == BankType::Breton
    });
    let axes_ok = region_ok && axes_ok && nord_axis_ok;
    println!(
        "[{}] population_axes - region 17 Daggerfall ⇒ Breton bank · subclimate 231 ⇒ Breton race · subclimate 226 ⇒ Nord race with Breton bank (independent axes)",
        if axes_ok { "OK" } else { "FAIL" },
    );
    all_ok &= axes_ok;

    let mut flats_resolved = 0usize;
    for npc in &population.npcs {
        let resolved = world.resolve_flat(npc.flat.record, npc.flat.direction);
        let ok = resolved.as_ref() == Ok(&npc.flat) && npc.flat.frames > 0 && !npc.name.is_empty();
        if ok {
            flats_resolved += 1;
        } else {
            println!("[FAIL] npc_{} - invalid flat/name: {:?}", npc.index, npc.flat);
        }
        all_ok &= ok;
    }
    println!(
        "[{}] population_flats - {flats_resolved}/{} NPCs with resolvable flat + name",
        if flats_resolved == population.npcs.len() { "OK" } else { "FAIL" },
        population.npcs.len(),
    );

    let retail_captions = population
        .npcs
        .iter()
        .filter(|npc| npc.caption.source == CaptionSource::TextRscOccupation)
        .filter(|npc| {
            npc.caption
                .text_id
                .and_then(|id| text.record_ascii(&rsc_bytes, id))
                .is_some_and(|retail| retail == npc.caption.text)
        })
        .count();
    let captions_npc_ok = retail_captions >= 4;
    println!(
        "[{}] population_captions - {retail_captions}/{} NPCs with caption == retail text from their id",
        if captions_npc_ok { "OK" } else { "FAIL" },
        population.npcs.len(),
    );
    all_ok &= captions_npc_ok;

    for npc in population.npcs.iter().take(6) {
        println!(
            "      #{:02} {:<22} race {:<8} bank {:<8} {:<6} face {} · flat {}.{} ({} frames, dir {}) · {}:{} \"{}\"",
            npc.index,
            npc.name,
            npc.race.name(),
            npc.name_bank.name(),
            npc.gender.name(),
            npc.face_index,
            npc.flat.archive,
            npc.flat.record,
            npc.flat.frames,
            npc.flat.direction,
            npc.occupation.key,
            npc.caption
                .text_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "-".into()),
            npc.caption.text,
        );
    }

    // ── 7. Crime guards ───────────────────────────────────────────
    let city = CityLayout::new(3);
    let no_crime = spawn_city_guards(&world, &city, 0, false).expect("crime 0");
    let below = spawn_city_guards(&world, &city, GUARD_CRIME_THRESHOLD - 1, true).expect("below threshold");
    let at_threshold =
        spawn_city_guards(&world, &city, GUARD_CRIME_THRESHOLD, true).expect("at threshold");
    let guards_ok = no_crime.is_empty()
        && below.is_empty()
        && at_threshold.len() == GUARDS_PER_SPAWN
        && at_threshold.iter().all(|guard| {
            guard.position[0] >= 0.0
                && guard.position[0] <= city.total_m()
                && guard.position[2] >= 0.0
                && guard.position[2] <= city.total_m()
        });
    println!(
        "[{}] guards_by_crime - crime 0 ⇒ {} · crime {} ⇒ {} · crime {} ⇒ {} guards (threshold {})",
        if guards_ok { "OK" } else { "FAIL" },
        no_crime.len(),
        GUARD_CRIME_THRESHOLD - 1,
        below.len(),
        GUARD_CRIME_THRESHOLD,
        at_threshold.len(),
        GUARD_CRIME_THRESHOLD,
    );
    all_ok &= guards_ok;

    // ── 8. create npc at / place npc ────────────────────────────────────
    let site = create_npc_at("aPlace", 0);
    let mut registry = QuestRegistry::new();
    registry.add_person(QuestPerson {
        symbol: "anNPC".into(),
        is_individual_npc: false,
        is_individual_at_home: false,
    });
    registry.add_person(QuestPerson {
        symbol: "homeNPC".into(),
        is_individual_npc: true,
        is_individual_at_home: true,
    });
    registry.add_place(Place {
        symbol: "aPlace".into(),
        position: [city.gap_center(1), 0.0, city.gap_center(1)],
    });
    registry.add_place(Place {
        symbol: "farPlace".into(),
        position: [city.total_m() * 10.0, 0.0, 0.0],
    });
    let placed = place_npc(
        &registry,
        "anNPC",
        "aPlace",
        PLACE_NPC_DEFAULT_MARKER,
        &city,
    );
    let rejected_symbol = place_npc(&registry, "nope", "aPlace", -1, &city);
    let rejected_home = place_npc(&registry, "homeNPC", "aPlace", -1, &city);
    let rejected_far = place_npc(&registry, "anNPC", "farPlace", -1, &city);
    let placement_ok = site == Ok(daggerfall_parser_bevy_bridge::npc_spec::SiteLink { site_index: 0 })
        && placed.as_ref().is_ok_and(|placement| {
            !placement.is_hidden
                && placement.marker == PLACE_NPC_DEFAULT_MARKER
                && placement.position[0] >= 0.0
                && placement.position[0] <= city.total_m()
        })
        && matches!(
            rejected_symbol,
            Err(daggerfall_parser_bevy_bridge::npc_spec::NpcError::UnknownNpcSymbol(_))
        )
        && matches!(
            rejected_home,
            Err(daggerfall_parser_bevy_bridge::npc_spec::NpcError::IndividualAtHome(_))
        )
        && matches!(
            rejected_far,
            Err(daggerfall_parser_bevy_bridge::npc_spec::NpcError::OutsideCity(_))
        );
    println!(
        "[{}] quest_placement - create npc at ⇒ SiteLink {:?} · place npc ⇒ {:?} · unknown/atHome/outside ⇒ rejected",
        if placement_ok { "OK" } else { "FAIL" },
        site.map(|link| link.site_index),
        placed.as_ref().map(|placement| placement.position),
    );
    all_ok &= placement_ok;

    if all_ok {
        println!(
            "VERDICT: PROBE-NPC-001 PASS - NAMEGEN.DAT {parts} parts · {matched}/{} goldens · population {}/{CITY_POPULATION} (digest 0x{digest:016x} ×2, 0 without flat) · {captions_retail}/4 TEXT.RSC retail captions · guards ≥{GUARD_CRIME_THRESHOLD} · place/create OK",
            GOLDEN.len(),
            flats_resolved,
        );
        0
    } else {
        println!("VERDICT: PROBE-NPC-001 FAIL - see [FAIL] rows");
        1
    }
}

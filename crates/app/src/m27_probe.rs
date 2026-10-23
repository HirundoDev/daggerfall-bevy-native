//! M27 probes — Guilds and factions: reputation, ranks and benefits
//! (SPEC-M27-FACTIONS §7, REQ-L99-060).
//!
//! `PROBE-FACTION-001` — complete `FACTION.TXT`:
//! - **A** GV-27-01..09 formulas over a synthetic fixture (ids 77/980, 1-based
//!   → 0-based region, `flat` `182<<7+23`, single `flat` copy,
//!   `rulerPowerBonus` `draw%51+20`).
//! - **B** determinism: parse ×2 with `DFRandom::srand(0x1234)` ⇒
//!   identical `records`/`unique_ids`/`resolver` and digest.
//! - **C** `DISK:FACTION.TXT` census: bytes, blocks, `type[7]`, `ggroup[11]`,
//!   `ggroup[17]`.
//! - **D** `GetGuildGroup`: shop 510 ⇒ `None`; Fighters/Mages ⇒ 11/10.
//!
//! `PROBE-GUILD-001` — guild rules:
//! - **A** GV-27-11..20 (rank and eligibility).
//! - **B** GV-27-21..35 (training, cure, `CalculateCost`, donation,
//!   Akatosh/Julianos/Fighters gating).
//! - **C** GV-27-36..42 (propagation with `PROP_TREE`, digest ×2).
//! - **D** Training disabled with `rank=-1` and enabled with `rank=0`.
//!
//! `M27-DEMO-001` — §9 demo over the retail `FACTION.TXT`: propagation
//! `±half` to ally/enemy, joining the Mages Guild (id 40, ggroup 10), two
//! promotions (rank 2 unlocks the library, rank 9 teleport ⇒ services 5→10)
//! and measured benefit (training, reward, repair, cure, donation),
//! with graph digest ×2.
//!
//! **Declared delta:** the digest is FNV-1a 64 (the `behavior` organ is
//! dependency-free); spec §7.1 names `xxh3` — same role (identity ×2),
//! different function. Mode A: 0 retail bytes in the repository.

use daggerfall_behavior::factions::{
    FactionGraph, FactionType, FACTION_FILENAME, PROVINCE_TYPE, RESOLVER_ID_START, SHOP_FACTION_ID,
};
use daggerfall_behavior::guild::{
    calculate_cost, can_access_service, can_access_library, cure_price, day_since_zero,
    guild, GuildDef, GuildDirectory, GuildKind, GuildService, Holiday, Deity,
    calculate_new_rank, donation_chance, donate, is_eligible, reduced_cure_cost,
    services_visible, training_price, training_window,
    alter_reward, reduced_repair_cost,
};
use daggerfall_parser_rs::dfrandom::DfRandom;

// ── evidence rows (M19/M25/M26 template) ────────────────────────────────────

/// Printed row: `[OK] <id> - <detail>` or `FAIL <id> got=<v> want=<v>`.
struct Row {
    id: &'static str,
    ok: bool,
    got: String,
    want: String,
    detail: String,
}

fn row(id: &'static str, got: String, want: impl Into<String>, detail: impl Into<String>) -> Row {
    let want = want.into();
    Row {
        id,
        ok: got == want,
        got,
        want,
        detail: detail.into(),
    }
}

fn check(id: &'static str, ok: bool, detail: impl Into<String>) -> Row {
    Row {
        id,
        ok,
        got: if ok { "ok".into() } else { "fail".into() },
        want: "ok".into(),
        detail: detail.into(),
    }
}

fn print_rows(title: &str, rows: &[Row]) -> bool {
    println!("\n-- {title} --");
    let mut ok = true;
    for row in rows {
        if row.ok {
            println!("[OK] {} - {}", row.id, row.detail);
        } else {
            ok = false;
            println!("FAIL {} got={} want={}", row.id, row.got, row.want);
        }
    }
    ok
}

/// Loads a Mode A ARENA2 file through the shared resolver, which matches the
/// source's case-insensitive open (DFU `FactionFile.cs:740-741`): the GOG
/// installation ships `faction.txt` where the corpus declares `FACTION.TXT`,
/// and both resolve to the same file.
fn load(name: &str) -> Result<(std::path::PathBuf, Vec<u8>), String> {
    let path = crate::mode_a_arena2_path(name)?;
    let bytes = std::fs::read(&path).map_err(|err| format!("read {path:?}: {err}"))?;
    Ok((path, bytes))
}

/// §7 GV-27-01..09 fixture: `#77` repeated (⇒ 980), region 18 and -1, flats.
const FACTION_FIXTURE: &str = "#77\ntype: 2\nname: Alpha\nregion: 18\nflat: 182 23\n\t#77\ntype: 2\nname: AlphaDup\nregion: -1\n#78\ntype: 2\nname: Beta\nregion: -1\nflat: 175 2\n";

/// Propagation tree of `factions.rs` (§4): root 10, child 11 (A),
/// grandchild 12 (B), ally 21 (L), enemy 20 (E).
const PROP_TREE: &str = "#10\ntype: 2\nname: Root\n\t#11\ntype: 2\nname: A\nally: 21\nenemy: 20\n\t#12\ntype: 2\nname: B\n#21\ntype: 2\nname: L\n#20\ntype: 2\nname: E\n";

fn parse(text: &str) -> FactionGraph {
    FactionGraph::parse(text, 0x1234).expect("valid fixture")
}

/// Skill values by **position** in `guild_skills` (notation `{22,4}`).
fn skills_at(def: &GuildDef, values: &[(usize, i16)], rest: i16) -> [i16; 35] {
    let mut skills = [rest; 35];
    for &(position, value) in values {
        skills[def.guild_skills[position]] = value;
    }
    skills
}

fn propagation_line(graph: &FactionGraph) -> String {
    format!(
        "R={} A={} B={} L={} E={}",
        graph.reputation(10),
        graph.reputation(11),
        graph.reputation(12),
        graph.reputation(21),
        graph.reputation(20),
    )
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-FACTION-001
// ═══════════════════════════════════════════════════════════════════════════

/// `PROBE-FACTION-001` — faction graph + retail census.
pub fn run_faction_001() -> i32 {
    println!("PROBE-FACTION-001 - FACTION.TXT: graph + census (M27, CR-989281d9, REQ-L99-060 · spec §7.1)");

    // ── A · formulas GV-27-01..09 ──────────────────────────────────────────
    let fixture = parse(FACTION_FIXTURE);
    let alpha = fixture.get(77).expect("77 present");
    let beta = fixture.get(78).expect("78 present");
    let mut rows = Vec::new();
    rows.push(row(
        "A1 first #77 keeps id",
        format!("id={} flat1={}", alpha.id, alpha.flat1),
        "id=77 flat1=23319",
        "GV-27-01: 182<<7+23 = 23319",
    ));
    rows.push(row(
        "A2 duplicate #77 resolves",
        format!("id={} dup={}", fixture.get(980).map_or(-1, |f| f.id), fixture.duplicate_ids()),
        "id=980 dup=1",
        format!("GV-27-02: resolver starts at {RESOLVER_ID_START}"),
    ));
    rows.push(row(
        "A3 region 18 -> 17",
        format!("{}", alpha.region),
        "17",
        "GV-27-03: 1-based → 0-based",
    ));
    rows.push(row(
        "A4 region -1 preserved",
        format!("{}", beta.region),
        "-1",
        "GV-27-04: not decremented",
    ));
    rows.push(row(
        "A5 flat unpack",
        format!("{}", alpha.flat1),
        "23319",
        "GV-27-05: 182<<7 + 23",
    ));
    rows.push(row(
        "A6 single flat copies",
        format!("{}=={}", beta.flat1, beta.flat2),
        "22402==22402",
        "GV-27-06: 175<<7 + 2",
    ));
    rows.push(row(
        "A7 ruler bonus draw 0",
        format!("{}", daggerfall_behavior::factions::ruler_power_bonus_from_draw(0)),
        "20",
        "GV-27-07: 0%51+20",
    ));
    rows.push(row(
        "A8 ruler bonus draw 50",
        format!("{}", daggerfall_behavior::factions::ruler_power_bonus_from_draw(50)),
        "70",
        "GV-27-08: 50%51+20",
    ));
    rows.push(row(
        "A9 ruler bonus parsed",
        format!(
            "{} {}",
            fixture.get(77).map_or(-1, |f| f.ruler_power_bonus),
            fixture.get(78).map_or(-1, |f| f.ruler_power_bonus)
        ),
        "50 24",
        "fixture bonus with the u64 LCG of `DFRandom::srand(0x1234)`",
    ));
    let mut all_ok = print_rows("A · formulas (GV-27-01..09)", &rows);

    // ── B · determinism ×2 ─────────────────────────────────────────────────
    let Ok((path, bytes)) = load(FACTION_FILENAME) else {
        println!("EXIT=2 FACTION.TXT unreadable in ARENA2 (Mode A)");
        return 2;
    };
    let text = match std::str::from_utf8(&bytes) {
        Ok(text) => text,
        Err(err) => {
            println!("EXIT=2 FACTION.TXT is not UTF-8: {err}");
            return 2;
        }
    };
    let first = parse(text);
    let second = parse(text);
    let unique = first.len() - first.duplicate_ids();
    let resolver = if first.duplicate_ids() == 0 {
        RESOLVER_ID_START
    } else {
        RESOLVER_ID_START + first.duplicate_ids() as i32 - 1
    };
    let b_line = format!(
        "records={} unique_ids={} resolver={} digest={:016x}",
        first.len(),
        unique,
        resolver,
        first.digest()
    );
    println!("{b_line}");
    let mut rows = vec![row(
        "B determinism x2",
        b_line,
        format!(
            "records=366 unique_ids=365 resolver=980 digest={:016x}",
            second.digest()
        ),
        "same DFRandom(0x1234) sequence ⇒ same graph byte-by-byte",
    )];
    rows.push(check(
        "B digest identical",
        first.digest() == second.digest(),
        format!("digest1={:016x} digest2={:016x}", first.digest(), second.digest()),
    ));
    all_ok &= print_rows("B · determinism (spec §7.1 B)", &rows);

    // ── C · retail census ──────────────────────────────────────────────────
    let provinces = first.find_factions(Some(PROVINCE_TYPE), None, None, None).len();
    let ggroup11 = first.find_factions(None, None, Some(11), None).len();
    let ggroup17 = first.find_factions(None, None, Some(17), None).len();
    let people = first
        .find_factions(Some(FactionType::People as i32), None, None, None)
        .len();
    let c_line = format!(
        "DISK:{} bytes={} blocks={} type[7]={} type[15]={} ggroup[11]={} ggroup[17]={}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("FACTION.TXT"),
        bytes.len(),
        first.len(),
        provinces,
        people,
        ggroup11,
        ggroup17,
    );
    println!("{c_line}");
    let rows = vec![
        row(
            "C census blocks",
            format!("bytes={} blocks={}", bytes.len(), first.len()),
            "bytes=74221 blocks=366",
            "§1.4: retail file census",
        ),
        row(
            "C census types",
            format!("type[7]={} type[15]={}", provinces, people),
            "type[7]=46 type[15]=44",
            "GV-27-10",
        ),
        row(
            "C census ggroups",
            format!("ggroup[11]={} ggroup[17]={}", ggroup11, ggroup17),
            "ggroup[11]=5 ggroup[17]=51",
            "§7.1 C",
        ),
    ];
    all_ok &= print_rows("C · DISK:FACTION.TXT census", &rows);

    // ── D · GetGuildGroup ─────────────────────────────────────────────────
    let rows = vec![
        row(
            "D shop 510 has no guild",
            format!("{}", first.guild_group(SHOP_FACTION_ID)),
            "-1",
            "GuildManager.cs:266-268",
        ),
        row(
            "D fighters group",
            format!("{}", first.guild_group(41)),
            "11",
            "The_Fighters_Guild ⇒ FightersGuild",
        ),
        row(
            "D mages group",
            format!("{}", first.guild_group(40)),
            "10",
            "The_Mages_Guild ⇒ MagesGuild",
        ),
    ];
    all_ok &= print_rows("D · GetGuildGroup", &rows);

    if all_ok {
        println!("\nVERDICT: PROBE-FACTION-001 PASS");
        0
    } else {
        println!("\nVERDICT: PROBE-FACTION-001 FAIL");
        1
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-GUILD-001
// ═══════════════════════════════════════════════════════════════════════════

/// `PROBE-GUILD-001` — rank, services, propagation.
pub fn run_guild_001() -> i32 {
    println!("PROBE-GUILD-001 - guilds: rank/gating/propagation (M27, CR-989281d9, REQ-L99-060 · spec §7.2)");

    let fighters = guild(GuildKind::Fighters);
    let mages = guild(GuildKind::Mages);
    let akatosh = guild(GuildKind::Temple(Deity::Akatosh));
    let julianos = guild(GuildKind::Temple(Deity::Julianos));
    let arkay = guild(GuildKind::Temple(Deity::Arkay));

    // ── A · rank GV-27-11..20 ──────────────────────────────────────────────
    let mut rows = Vec::new();
    let cases: [(&str, i32, i32, [i16; 35]); 8] = [
        ("A1 GV-27-11", 0, -1, skills_at(&fighters, &[], 0)),
        ("A2 GV-27-12", 0, -1, skills_at(&fighters, &[(0, 40)], 0)),
        ("A3 GV-27-13", 0, 0, skills_at(&fighters, &[(0, 22), (1, 4)], 0)),
        ("A4 GV-27-14", 25, 2, skills_at(&fighters, &[], 100)),
        ("A5 GV-27-15", 90, 9, skills_at(&fighters, &[], 100)),
        ("A6 GV-27-16", -1, -1, skills_at(&fighters, &[], 100)),
        ("A7 GV-27-17", 50, -1, skills_at(&mages, &[(0, 20), (1, 20)], 0)),
        ("A8 GV-27-18", 50, 0, skills_at(&mages, &[(0, 22), (1, 4)], 0)),
    ];
    for (id, reputation, want, skills) in cases {
        let got = calculate_new_rank(reputation, &skills, &fighters_or(&mages, id));
        rows.push(row(id, format!("rank={got}"), format!("rank={want}"), "rankReq* table"));
    }
    rows.push(row(
        "A9 GV-27-19",
        format!("eligible={}", is_eligible(0, &skills_at(&fighters, &[(0, 22), (1, 4)], 0), &fighters)),
        "eligible=true",
        "high>0 and low+high>1",
    ));
    rows.push(row(
        "A10 GV-27-20",
        format!("eligible={}", is_eligible(-1, &skills_at(&fighters, &[(0, 100), (1, 100)], 0), &fighters)),
        "eligible=false",
        "rep>=0 required",
    ));
    let mut all_ok = print_rows("A · rank and eligibility", &rows);

    // ── B · services GV-27-21..35 ──────────────────────────────────────────
    let training_reject = training_window(&fighters, 0, daggerfall_behavior::guild::skill::AXE, 51, 5, 900, 0);
    let don_roll = {
        let mut rng = DfRandom::new();
        rng.srand(7);
        rng.rand() % 100
    };
    let b_counts = format!(
        "train_price(5)=500/2000 cure(2,arkay5)={} cost(500,20)={} chance(100,0)={} chance(100,100)={} chance(1000,50)={} roll7={}",
        reduced_cure_cost(&arkay, 5, 500),
        calculate_cost(500, 20),
        donation_chance(100, 0),
        donation_chance(100, 100),
        donation_chance(1000, 50),
        don_roll,
    );
    println!("{b_counts}");
    let mut rows = vec![
        row(
            "B1 GV-27-21/22",
            format!("{} / {}", training_price(5, true), training_price(5, false)),
            "500 / 2000",
            "100·level as a member, 400·level as a non-member",
        ),
        row(
            "B2 GV-27-23",
            format!("{training_reject:?}"),
            "Err(TooSkilled { value: 51 })",
            "skill 51 > cap 50",
        ),
        row(
            "B3 GV-27-24/25/26",
            format!(
                "{} {} {}",
                reduced_cure_cost(&arkay, 0, 500),
                reduced_cure_cost(&arkay, 5, 500),
                reduced_cure_cost(&arkay, 9, 500)
            ),
            "500 250 48",
            "((10-rank)<<8)/10 · base >> 8",
        ),
        row(
            "B4 GV-27-27",
            format!("{}", reduced_cure_cost(&fighters, 9, 500)),
            "500",
            "no discount outside Arkay",
        ),
        row(
            "B5 GV-27-28",
            format!("{}", calculate_cost(500, 20)),
            "1100",
            "2·(500·10/100 + 500)",
        ),
        row(
            "B6 GV-27-29/30/31",
            format!(
                "{} {} {}",
                donation_chance(100, 0),
                donation_chance(100, 100),
                donation_chance(1000, 50)
            ),
            "201 3 41",
            "2·gold/max(|rep|,1)+1",
        ),
        row(
            "B7 GV-27-32/33",
            format!(
                "{} {}",
                can_access_service(&akatosh, 4, GuildService::BuyPotions),
                can_access_service(&akatosh, 3, GuildService::BuyPotions)
            ),
            "true false",
            "Akatosh buyPotions=4",
        ),
        row(
            "B8 GV-27-34",
            format!("{}", can_access_service(&julianos, 3, GuildService::BuyMagicItems)),
            "true",
            "Julianos buyMagic=3",
        ),
        row(
            "B9 GV-27-35",
            format!("{}", can_access_service(&fighters, -1, GuildService::Training)),
            "false",
            "Training requires IsMember",
        ),
        row(
            "B10 donation applies",
            format!("{} {}", donate(50, 100, 0).applied, donate(50, 100, 0).reputation),
            "true 51",
            "chance 5 > roll 0; propagation is measured in the §9.4 demo",
        ),
    ];
    rows.push(row(
        "B11 training quote",
        format!(
            "{:?}",
            training_window(&fighters, 0, daggerfall_behavior::guild::skill::AXE, 50, 4, 720, 0)
        ),
        "Ok(TrainingQuote { price: 400, hours: 3, fatigue: 180 })",
        "member level 4 ⇒ 400 gp, 3 h, fatigue 180",
    ));
    all_ok &= print_rows("B · services (GV-27-21..35)", &rows);

    // ── C · propagation GV-27-36..42 ───────────────────────────────────────
    let mut rows = Vec::new();
    let mut g36 = parse(PROP_TREE);
    g36.change_reputation(11, 10, true);
    rows.push(row("C1 GV-27-36", propagation_line(&g36), "R=10 A=10 B=5 L=5 E=-5", "+10 propagates"));
    let mut g37 = parse(PROP_TREE);
    g37.change_reputation(11, 5, true);
    rows.push(row("C2 GV-27-37", propagation_line(&g37), "R=5 A=5 B=2 L=2 E=-2", "5/2=2 truncated"));
    let mut g38 = parse(PROP_TREE);
    g38.change_reputation(11, -10, true);
    rows.push(row("C3 GV-27-38", propagation_line(&g38), "R=-10 A=-10 B=-5 L=-5 E=5", "symmetric sign"));
    let mut g39 = parse(PROP_TREE);
    g39.change_reputation(11, 10, false);
    rows.push(row("C4 GV-27-39", propagation_line(&g39), "R=0 A=10 B=0 L=0 E=0", "no propagation"));
    let mut g40 = parse(PROP_TREE);
    g40.set_reputation(11, 95);
    g40.change_reputation(11, 10, false);
    rows.push(row("C5 GV-27-40", format!("A={}", g40.reputation(11)), "A=100", "upper clamp"));
    let g41_before = parse(PROP_TREE);
    let mut g41 = parse(PROP_TREE);
    let changed = g41.change_reputation(999, 10, true);
    rows.push(row(
        "C6 GV-27-41",
        format!("changed={changed} digest_equal={}", g41.digest() == g41_before.digest()),
        "changed=false digest_equal=true",
        "non-existent id",
    ));
    let mut g42 = parse(PROP_TREE);
    g42.set_reputation(20, 4);
    g42.change_repute_with(11, -5);
    rows.push(row("C7 GV-27-42", propagation_line(&g42), "R=-5 A=-5 B=-2 L=-2 E=6", "quest verb −5"));
    let mut d1 = parse(PROP_TREE);
    d1.change_reputation(11, 7, true);
    let mut d2 = parse(PROP_TREE);
    d2.change_reputation(11, 7, true);
    rows.push(row(
        "C8 digest tree x2",
        format!("{:016x}", d1.digest()),
        format!("{:016x}", d2.digest()),
        "same operation ⇒ same digest",
    ));
    all_ok &= print_rows("C · propagation (GV-27-36..42)", &rows);

    // ── D · guild menu gating ──────────────────────────────────────────────
    let rows = vec![
        row(
            "D1 rank -1 training disabled",
            format!("{}", can_access_service(&fighters, -1, GuildService::Training)),
            "false",
            "guild staff: without membership, no training offered",
        ),
        row(
            "D2 rank 0 training enabled",
            format!("{}", can_access_service(&fighters, 0, GuildService::Training)),
            "true",
            "member from rank 0",
        ),
        row(
            "D3 visible services",
            format!(
                "rank-1={} rank0={} rank9={}",
                services_visible(&fighters, -1).len(),
                services_visible(&fighters, 0).len(),
                services_visible(&fighters, 9).len()
            ),
            "rank-1=5 rank0=7 rank9=7",
            "PurchaseSpells/Identify/Donate/CureDisease/Quests always",
        ),
        row(
            "D4 mages library gate",
            format!(
                "rank1={} rank2={}",
                can_access_library(&mages, 1),
                can_access_library(&mages, 2)
            ),
            "rank1=false rank2=true",
            "MagesGuild.cs:124-127",
        ),
    ];
    all_ok &= print_rows("D · gating", &rows);

    if all_ok {
        println!("\nVERDICT: PROBE-GUILD-001 PASS");
        0
    } else {
        println!("\nVERDICT: PROBE-GUILD-001 FAIL");
        1
    }
}

/// Selects the definition used by each rank case of table A.
fn fighters_or(mages: &GuildDef, id: &str) -> GuildDef {
    if id.starts_with("A7") || id.starts_with("A8") {
        *mages
    } else {
        guild(GuildKind::Fighters)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// M27-DEMO-001
// ═══════════════════════════════════════════════════════════════════════════

/// Retail factions used by the demo (§1.4 / §9).
const MAGES_FACTION_ID: i32 = 40;
/// §9.4 witness (probe C census): faction 10 with ally 106 and enemy 25.
const PROP_FACTION_ID: i32 = 10;
const FIGHTERS_FACTION_ID: i32 = 41;

/// `M27-DEMO-001` — §9: graph, propagation, ranks, services and benefit.
pub fn run_demo_001() -> i32 {
    println!("M27-DEMO-001 - guilds: join + rank + measured benefit (M27, CR-989281d9 · spec §9)");

    let Ok((_, bytes)) = load(FACTION_FILENAME) else {
        println!("EXIT=2 FACTION.TXT unreadable in ARENA2 (Mode A)");
        return 2;
    };
    let Ok(text) = std::str::from_utf8(&bytes) else {
        println!("EXIT=2 FACTION.TXT is not UTF-8");
        return 2;
    };
    let mut graph = parse(text);
    let mages = guild(GuildKind::Mages);

    // §9.4 — witness fixed by the probe C census: faction 10 declares
    // ally 106 and enemy 25, both outside its tree (⇒ exact ±half delta).
    let (prop_id, ally_id, enemy_id) = (
        PROP_FACTION_ID,
        graph.get(PROP_FACTION_ID).expect("retail faction 10").ally[0],
        graph.get(PROP_FACTION_ID).expect("retail faction 10").enemy[0],
    );
    let propagated = graph.change_repute_with(prop_id, 10);
    let panel_rep = graph.reputation(prop_id);
    let ally_rep = graph.reputation(ally_id);
    let enemy_rep = graph.reputation(enemy_id);
    println!(
        "loaded_factions={} fighters_ggroup={} faction={} rep={} ally={}={} enemy={}={} digest={:016x}",
        graph.len(),
        graph.guild_group(FIGHTERS_FACTION_ID),
        prop_id,
        panel_rep,
        ally_id,
        ally_rep,
        enemy_id,
        enemy_rep,
        graph.digest()
    );

    // §9.2/§9.3 — join ⇒ rank 0; rank 2 unlocks the library; rank 9 teleport.
    let day_zero = day_since_zero(405, 1);
    let mut directory = GuildDirectory::new();
    let joined = directory.join(&mages, day_zero);
    let services_before = services_visible(&mages, joined.rank).len();
    let library_rank0 = can_access_library(&mages, joined.rank);
    let skills = [100i16; 35];
    let promotion2 = directory.update(&mages, 20, &skills, day_zero + 28);
    let rank2 = directory.rank(mages.group).unwrap_or(-1);
    let library_rank2 = can_access_library(&mages, rank2);
    let promotion9 = directory.update(&mages, 90, &skills, day_zero + 56);
    let rank = directory.rank(mages.group).unwrap_or(-1);
    let services_after = services_visible(&mages, rank).len();
    let teleport_rank9 = can_access_service(&mages, rank, GuildService::Teleport);

    // Measured benefit (same formulas as the retail guilds).
    let price_member = training_price(5, true);
    let price_outsider = training_price(5, false);
    let reward = alter_reward(rank, 1000);
    let repair = reduced_repair_cost(rank, 1000);
    let arkay = guild(GuildKind::Temple(Deity::Arkay));
    let cure = cure_price(2, &arkay, 5, 20, Holiday::None);
    let mut rng = DfRandom::new();
    rng.srand(7);
    let donation_roll = rng.rand() % 100;
    let donation_amount = 10_000;
    let donation_chance = donation_chance(donation_amount, 90);
    let donated = donate(90, donation_amount, donation_roll);

    println!(
        "mages_faction={MAGES_FACTION_ID} join_rank={} promotions={:?}/{:?} rank={} visible_services: before={services_before}, after={services_after} training_price={price_member}/{price_outsider} reward={reward} repair={repair} cure2_arkay5={cure} donation{donation_amount}gp chance={donation_chance} roll={donation_roll} rep={}",
        joined.rank, promotion2, promotion9, rank, donated.reputation
    );

    let mut all_ok = print_rows(
        "demo",
        &[
            row(
                "demo graph loaded",
                format!("{} zero_groups={}", graph.len(), graph.guild_group(0)),
                "366 zero_groups=-1",
                "§9.1: 366 factions loaded; faction 0 has no guild",
            ),
            row(
                "demo panel rep",
                format!("{panel_rep}"),
                "10",
                "§9.1: numeric faction reputation after the change",
            ),
            row(
                "demo propagation",
                format!("{ally_rep} {enemy_rep}"),
                "5 -5",
                "§9.4: `change repute with` +10 ⇒ ally +half, enemy -half",
            ),
            check(
                "demo change accepted",
                propagated,
                "the faction exists and the change is applied (factions with a resolved id)",
            ),
            row(
                "demo witness",
                format!("{prop_id} {ally_id} {enemy_id}"),
                "10 106 25",
                "§9.4: retail faction with ally and enemy declared outside its tree",
            ),
            row(
                "demo join",
                format!("rank={}", joined.rank),
                "rank=0",
                "Join ⇒ rank 0 (Guild.cs:304-308)",
            ),
            row(
                "demo promotions",
                format!("{promotion2:?} {promotion9:?}"),
                "Some(Promotion(2)) Some(Promotion(9))",
                "28 days per step; rep 20 ⇒ rank 2, rep 90 ⇒ rank 9",
            ),
            row(
                "demo library",
                format!("{library_rank0} {library_rank2}"),
                "false true",
                "§9.3: Mages rank 2 unlocks the library",
            ),
            row(
                "demo services",
                format!("{services_before} {services_after}"),
                "5 10",
                "§9.3: the service list changes size when the rank rises",
            ),
            row(
                "demo teleport",
                format!("{teleport_rank9}"),
                "true",
                "§9.3: Mages rank 8 unlocks teleport",
            ),
            check(
                "demo training benefit",
                price_member < price_outsider,
                format!("{price_member} < {price_outsider} ⇒ saving {}", price_outsider - price_member),
            ),
            check(
                "demo rank scaling",
                reward > 1000 && repair < 1000,
                format!("reward {reward} > 1000, repair {repair} < 1000"),
            ),
            check(
                "demo cure measured",
                cure == 550,
                format!("cure 2 diseases Arkay rank 5 quality 20 = {cure}"),
            ),
            row(
                "demo donation",
                format!("{} {}", donated.applied, donated.reputation),
                "true 91",
                "pure donation: only moves the donor's rep (Donation.cs:60-70)",
            ),
            check(
                "demo digest x2",
                {
                    let a = parse(text).digest();
                    let b = parse(text).digest();
                    a == b
                },
                "the same file produces the same digest",
            ),
        ],
    );

    // The directory must never be empty after two promotions.
    all_ok &= directory.len() == 1;

    if all_ok {
        println!("\nVERDICT: M27-DEMO-001 PASS");
        0
    } else {
        println!("\nVERDICT: M27-DEMO-001 FAIL");
        1
    }
}

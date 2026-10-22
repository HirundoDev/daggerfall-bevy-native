//! M16 — Anti-Pantalla probes for text (`TEXT.RSC`) and original quests
//! (`.QBN`/`.QRC`), DoD §g / WI-8 / REQ-L99-048 + REQ-L99-049.
//!
//! **PROBE-TEXT-RSC-001** (block A) verifies against the user's real ARENA2
//! (Mode A, 0 retail bytes in git — INV-L99-6):
//!
//! 1. `TEXT.RSC` (353 393 B) parses: `H=8454` ⇒ **1408** records, `0xFFFF`
//!    and declared size after the table; 1408/1408 records tokenized with
//!    **0 errors** and `record_ascii` (M15 API) intact;
//! 2. the **13 golden vectors** from spec M16 §3 (offset, length, hex prefix
//!    and exact tokens V1-V10; structure of V11-V13), including the
//!    self-checks (a)-(d): `header[475]`, contiguity 26773/26792/26808/
//!    26825, `header[9999]+len == 353393` and `0xFB` consuming **1** byte;
//! 3. file statistics (18 501 tokens; 4145 `Center`, 2309 `Left`,
//!    2496 `SubrecordSeparator`, 20 `PositionPrefix` in 6 records, 0 `0xFA`),
//!    with `0xFF` variants (494 records with ≥1 separator);
//! 4. determinism: same input ×2 ⇒ same FNV-1a64 digest of the render;
//! 5. books (`books/BOK*.TXT`): same token flow without a table header
//!    (`0xF6` end of page) — ≥1 book resolved with retail text.
//!
//! **PROBE-QUEST-ORIGINAL-001** (blocks B+C) verifies:
//!
//! 1. **247/247** `.QBN` and **244/244** `.QRC` parse (0 panic, 0 error);
//!    6407 opcodes as opaque 87 B containers (semantics = H4);
//! 2. table of the **82** verbs (17 triggers + 65 actions);
//! 3. end-to-end ingestion: **34** main quests (`name[0] == 'S'`, all
//!    with `.QRC`) + deterministic side quest sampling (1 per cell
//!    `byte0 × byte2`, lexicographic order) in `QuestLog`, with messages
//!    resolved from the `.QRC` (route A) — 0 invented string;
//! 4. golden vectors Q1-Q6 from spec §7.4 (bytes, records, ids, macros);
//! 5. lore: `RUMOR.DAT` (3506 B, 31 records) with ≥1 resolved rumor.
//!
//! Without `TEXT.RSC`, the quests or `RUMOR.DAT`, the probe exits with
//! **exit 2** and the literal reason: 0 invented text.

use std::collections::BTreeMap;
use std::path::Path;

use daggerfall_behavior::quest::{
    main_quest_names, sample_side_quests, QuestCounts, QuestFamily, QuestLog, QuestMessage,
    QuestRecord,
};
use daggerfall_parser_rs::quest::{
    Qbn, Qrc, RumorFile, VerbKind, QUEST_ACTION_COUNT, QUEST_TRIGGER_COUNT, QUEST_VERBS,
};
use daggerfall_parser_rs::text_rsc::{Book, TextRsc, Token, TokenFormat};
use sha2::{Digest, Sha256};

/// Header golden vector: `(id, offset, len, sha256 of the prefix)`.
///
/// The prefix is the retail content (`min(len, 64)` B) **hashed**: the probe
/// does not reproduce a single byte of the game text (INV-L99-6).
struct GoldenRecord {
    id: u16,
    offset: u32,
    len: usize,
    sha256: &'static str,
}

/// 13 vectors from spec M16 §3 (`sha256` of the prefix: full record in
/// V1-V3, 64 B for the rest — long vectors are additionally validated by
/// token structure).
const GOLDEN: [GoldenRecord; 13] = [
    GoldenRecord { id: 475, offset: 26_773, len: 19, sha256: "47d2a0a2d48a40eb62fcafd89657db83efaca6b178f1186adf36df2c2a94e1cb" },
    GoldenRecord { id: 476, offset: 26_792, len: 16, sha256: "d4da7f0108c732471b7fddff956ec44307e4e9baf44e99892d8467bd4b1a4a54" },
    GoldenRecord { id: 477, offset: 26_808, len: 17, sha256: "cf1c52e9a4fb3fb38de8717596dd2c0a4bbc608e47e4172998be1a13a131fbda" },
    GoldenRecord {
        id: 1000,
        offset: 61_603,
        len: 71,
        sha256: "f6fba4842ce07a5bdc0b2b68011ee5846faef31b86883c74abdbf606e08f2183",
    },
    GoldenRecord {
        id: 1500,
        offset: 114_399,
        len: 136,
        sha256: "b7e574a88ee26278b1f92be44573a70d2f5299a1a02a41e53ab942e5239d7b72",
    },
    GoldenRecord {
        id: 0,
        offset: 8_456,
        len: 284,
        sha256: "6e22a05d1f37fbc4a7702b6106d7426d0ed88ecf489ba2a162b58e2f3d0a01d0",
    },
    GoldenRecord {
        id: 1,
        offset: 8_740,
        len: 233,
        sha256: "7a8dd25e2d68b541948e979575095292c0999ad6f66fd6b54c878234e6d589dd",
    },
    GoldenRecord {
        id: 2,
        offset: 8_973,
        len: 229,
        sha256: "11532bf8b354c8d3918ff62f8d7a4d5d99f963fd735773e0c61b1bb4d3be59a6",
    },
    GoldenRecord {
        id: 3,
        offset: 9_202,
        len: 233,
        sha256: "62f44c4921f7fe75e96b35e2bf87403b607b3c506d07fba1d3e85c6616f87b76",
    },
    GoldenRecord {
        id: 4,
        offset: 9_435,
        len: 301,
        sha256: "2d4a78d67f37a33150cf7949ff2d6dac9a75721895c3c84f0107129dc2c75277",
    },
    GoldenRecord {
        id: 2000,
        offset: 131_536,
        len: 362,
        sha256: "babf5e6557b58dad89d6769649b2267a3fdb433be4cf2004b4c3bdb23d97db76",
    },
    GoldenRecord {
        id: 2400,
        offset: 154_732,
        len: 731,
        sha256: "9c06d8d0e1613873fe289c43716d1b5f6142d746d4598801dbaa7bccbfbf6379",
    },
    GoldenRecord {
        id: 8722,
        offset: 319_336,
        len: 661,
        sha256: "445dc187cefdbc11c7b94cb5fb438fe739684088640440d065d502d57122a238",
    },
];

/// M16 §7.4 spec quest golden vector.
struct GoldenQuest {
    name: &'static str,
    /// `(bytes, SectionRecordCount[0..9])` of the `.QBN`, if present.
    qbn: Option<(usize, [u16; 10])>,
    /// `(bytes, records, min id, max id)` of the `.QRC`.
    qrc: (usize, usize, u16, u16),
    /// Minimum expected macros (`%m` and `_v_`).
    macros: &'static [(&'static str, usize)],
}

const GOLDEN_QUESTS: [GoldenQuest; 6] = [
    GoldenQuest {
        name: "A0C00Y00",
        qbn: Some((4_399, [10, 0, 0, 4, 1, 0, 1, 4, 44, 16])),
        qrc: (3_973, 24, 1000, 1031),
        macros: &[("%pcf", 2), ("%qdt", 1), ("%ra", 1), ("_qgiver_", 7)],
    },
    GoldenQuest {
        name: "S0000001",
        qbn: Some((4_156, [3, 0, 0, 6, 3, 0, 1, 0, 42, 20])),
        qrc: (9_612, 25, 1000, 1040),
        macros: &[
            ("%pcf", 4),
            ("%pcn", 2),
            ("%qdt", 2),
            ("%god", 1),
            ("%oth", 1),
            ("%pct", 1),
            ("%di", 3),
        ],
    },
    GoldenQuest {
        name: "S0000010",
        qbn: Some((2_942, [2, 0, 0, 2, 3, 0, 3, 1, 29, 12])),
        qrc: (3_429, 12, 1000, 1022),
        macros: &[("%pcf", 3), ("%pcn", 1), ("%qdt", 1), ("%ra", 3)],
    },
    GoldenQuest {
        name: "_BRISIEN",
        qbn: Some((2_763, [3, 0, 0, 1, 2, 0, 5, 0, 27, 8])),
        qrc: (6_578, 18, 1001, 1025),
        macros: &[("%pcf", 3), ("%pcn", 5), ("%qdt", 2)],
    },
    GoldenQuest {
        name: "$CUREVAM",
        qbn: Some((5_219, [2, 0, 0, 3, 2, 0, 3, 4, 54, 20])),
        qrc: (5_709, 26, 1000, 1052),
        macros: &[("%pcf", 1), ("%pcn", 5), ("%qdt", 4), ("%reg", 5), ("%vam", 11)],
    },
    GoldenQuest {
        name: "M0B40Y04",
        qbn: None,
        qrc: (2_151, 13, 1000, 1080),
        macros: &[],
    },
];

/// Justification tokens (assert sugar).
fn center() -> Token {
    Token::formatting(TokenFormat::JustifyCenter)
}

/// `JustifyLeft`.
fn left() -> Token {
    Token::formatting(TokenFormat::JustifyLeft)
}

/// `PositionPrefix`.
fn pos(column: u8) -> Token {
    Token::with_parameter(TokenFormat::PositionPrefix, column)
}

/// `Text` **without content**: expectations compare format and parameter,
/// never the game text (INV-L99-6). Content is verified by `sha256`
/// in `GOLDEN`.
fn t() -> Token {
    Token::text(String::new())
}

/// `sha256` hexadecimal of a slice.
fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Marks the check and prints it (`[OK]`/`[FAIL]`).
fn check(passed: &mut bool, condition: bool, label: &str, detail: String) {
    if !condition {
        *passed = false;
    }
    println!(
        "[{}] {label} - {detail}",
        if condition { "OK" } else { "FAIL" }
    );
}

/// Reads a file from ARENA2 Mode A.
fn load(name: &str) -> Result<Vec<u8>, String> {
    let path = crate::mode_a_arena2_path(name)?;
    std::fs::read(&path).map_err(|err| format!("reading {path:?}: {err}"))
}

/// Sorted list `(stem, bytes)` of the files with the given extension.
fn collect(dir: &Path, extension: &str) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut files = Vec::new();
    let entries = std::fs::read_dir(dir).map_err(|err| format!("reading {dir:?}: {err}"))?;
    for entry in entries {
        let path = entry
            .map_err(|err| format!("entry of {dir:?}: {err}"))?
            .path();
        let matches = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case(extension));
        if !matches {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| format!("non-UTF-8 file name: {path:?}"))?
            .to_string();
        let bytes = std::fs::read(&path).map_err(|err| format!("reading {path:?}: {err}"))?;
        files.push((stem, bytes));
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(files)
}

/// FNV-1a 64 (deterministic digest with no dependencies).
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Canonical digest of the render of ALL records (id + tokens).
fn render_digest(rsc: &TextRsc, file: &[u8]) -> Result<u64, String> {
    let mut canonical = Vec::new();
    for (index, record) in rsc.records.iter().enumerate() {
        canonical.extend_from_slice(&record.id.to_le_bytes());
        let tokens = rsc
            .record_tokens_by_index(file, index)
            .map_err(|err| format!("record {index}: {err}"))?;
        canonical.extend_from_slice(format!("{tokens:?}").as_bytes());
    }
    Ok(fnv1a64(&canonical))
}

/// Runs PROBE-TEXT-RSC-001 — block A (`TEXT.RSC` + books) and chains
/// block B/C (`run_quest_original_001`).
pub fn run_text_rsc_001() -> i32 {
    println!(
        "PROBE-TEXT-RSC-001 - tokenized retail text (M16, \
         REQ-L99-048 · DoD §g / WI-8)"
    );
    let file = match load("TEXT.RSC") {
        Ok(bytes) => bytes,
        Err(reason) => {
            println!("ARENA2 not accessible (TEXT.RSC): {reason}");
            return 2;
        }
    };
    let mut ok = true;
    block_text(&file, &mut ok);
    block_books(&mut ok);
    if !ok {
        println!("VERDICT: PROBE-TEXT-RSC-001 FAIL - block A (TEXT.RSC/books)");
        return 1;
    }

    let quest_code = run_quest_original_001();
    if quest_code != 0 {
        println!("VERDICT: PROBE-TEXT-RSC-001 FAIL - block B/C exit {quest_code}");
        return 1;
    }
    println!("VERDICT: PROBE-TEXT-RSC-001 PASS");
    0
}

/// Block A: header, tokens, goldens, statistics and determinism.
fn block_text(file: &[u8], ok: &mut bool) {
    let rsc = match TextRsc::parse(file) {
        Ok(rsc) => rsc,
        Err(err) => {
            check(ok, false, "text_rsc_parse", format!("{err}"));
            return;
        }
    };
    check(
        ok,
        rsc.record_count() == 1408,
        "records",
        format!(
            "{} records (H={}, retail 1408)",
            rsc.record_count(),
            rsc.header_length
        ),
    );
    check(
        ok,
        rsc.header_length == 8454,
        "header_length",
        format!("H={} (retail 8454)", rsc.header_length),
    );
    check(
        ok,
        rsc.header_terminator(file) == Some(0xFFFF),
        "header_terminator",
        format!("{:?} (retail 0xFFFF)", rsc.header_terminator(file)),
    );
    check(
        ok,
        rsc.declared_file_size(file) == Some(file.len() as u32),
        "declared_file_size",
        format!(
            "declared {:?} / actual {} B",
            rsc.declared_file_size(file),
            file.len()
        ),
    );
    // Record 475 is identified by length + sha256 (18 characters):
    // 0 bytes of retail text in the repo (INV-L99-6).
    let record_475 = rsc.record_ascii(file, 475);
    let record_475_fingerprint = record_475.map(|text| {
        (
            text.chars().count(),
            sha256_hex(text.as_bytes()),
        )
    });
    check(
        ok,
        record_475_fingerprint.as_ref().map(|(chars, hash)| (*chars, hash.as_str()))
            == Some((18, "9541862f85df50127cffd61e1354bb9dd652e1024be1307990fd118ca5d54eae")),
        "record_ascii_m15",
        format!(
            "id 475 = {} chars · sha256 {:?} (M15 API intact)",
            record_475.map(|text| text.chars().count()).unwrap_or(0),
            record_475_fingerprint.map(|(_, hash)| hash)
        ),
    );

    // ── A2: golden vectors (header + tokens) ────────────────────────────
    for golden in &GOLDEN {
        let index = rsc.index_of(golden.id);
        let offset = index.map(|index| rsc.records[index].offset);
        let length = index.and_then(|index| rsc.record_len(file, index));
        let prefix_len = golden.len.min(64);
        let prefix_sha256 = offset
            .map(|offset| offset as usize)
            .and_then(|offset| file.get(offset..offset + prefix_len))
            .map(sha256_hex)
            .unwrap_or_default();
        check(
            ok,
            offset == Some(golden.offset) && length == Some(golden.len) && prefix_sha256 == golden.sha256,
            "golden",
            format!(
                "V id {}: offset {:?} (retail {}), len {:?} (retail {}), sha256({prefix_len} B) {}",
                golden.id,
                offset,
                golden.offset,
                length,
                golden.len,
                if prefix_sha256 == golden.sha256 {
                    "OK".to_string()
                } else {
                    format!("{prefix_sha256} != {}", golden.sha256)
                }
            ),
        );
    }
    golden_self_checks(&rsc, file, ok);
    golden_tokens(&rsc, file, ok);

    // ── A3: file statistics ─────────────────────────────────────────────
    let mut tokens_total = 0usize;
    let mut text_count = 0usize;
    let mut center_count = 0usize;
    let mut left_count = 0usize;
    let mut position_count = 0usize;
    let mut position_records = 0usize;
    let mut separator_count = 0usize;
    let mut records_with_separator = 0usize;
    let mut cursor_positioners = 0usize;
    let mut font_prefix = 0usize;
    let mut other_fa = 0usize;
    let mut token_errors = Vec::new();
    for index in 0..rsc.record_count() {
        match rsc.record_tokens_by_index(file, index) {
            Ok(tokens) => {
                tokens_total += tokens.len();
                let mut separators = 0usize;
                let mut positions = 0usize;
                for token in &tokens {
                    match token.format {
                        TokenFormat::Text => text_count += 1,
                        TokenFormat::JustifyCenter => center_count += 1,
                        TokenFormat::JustifyLeft => left_count += 1,
                        TokenFormat::PositionPrefix => positions += 1,
                        TokenFormat::SubrecordSeparator => separators += 1,
                        TokenFormat::InputCursorPositioner => cursor_positioners += 1,
                        TokenFormat::FontPrefix => font_prefix += 1,
                        TokenFormat::Other(0xFA) => other_fa += 1,
                        _ => {}
                    }
                }
                position_count += positions;
                if positions > 0 {
                    position_records += 1;
                }
                if separators > 0 {
                    records_with_separator += 1;
                }
                separator_count += separators;
            }
            Err(err) => token_errors.push(format!("record {index}: {err}")),
        }
    }
    check(
        ok,
        token_errors.is_empty(),
        "tokens_all_records",
        format!(
            "{}/{} records tokenized · {} tokens · 0 errors",
            rsc.record_count() - token_errors.len(),
            rsc.record_count(),
            tokens_total
        ),
    );
    check(
        ok,
        tokens_total == 18_501
            && text_count == 9_528
            && center_count == 4_145
            && left_count == 2_309
            && position_count == 20
            && position_records == 6
            && separator_count == 2_496
            && records_with_separator == 494
            && cursor_positioners == 3
            && font_prefix == 0
            && other_fa == 0,
        "token_stats",
        format!(
            "Text {text_count} · Center {center_count} · Left {left_count} · Pos {position_count} \
             ({position_records} records) · SubSep {separator_count} ({records_with_separator} \
             records) · Cursor {cursor_positioners} · Font {font_prefix} · 0xFA {other_fa}"
        ),
    );
    check(
        ok,
        separator_count + rsc.record_count() == 3_904,
        "variants",
        format!(
            "{} variants (records + separators; retail 3904)",
            separator_count + rsc.record_count()
        ),
    );

    let mut ids: Vec<u16> = rsc.records.iter().map(|record| record.id).collect();
    ids.sort_unstable();
    let below_1000 = ids.iter().filter(|id| **id < 1000).count();
    let from_8000 = ids.iter().filter(|id| **id >= 8000).count();
    let shared = rsc
        .records
        .windows(2)
        .filter(|pair| pair[0].offset == pair[1].offset)
        .count();
    check(
        ok,
        ids.first() == Some(&0)
            && ids.last() == Some(&9999)
            && below_1000 == 265
            && from_8000 == 161
            && shared == 12,
        "id_stats",
        format!(
            "ids {}..{} · <1000 {below_1000} · ≥8000 {from_8000} · shared offsets {shared}",
            ids.first().unwrap_or(&0),
            ids.last().unwrap_or(&0)
        ),
    );

    // ── A4: determinism (same input ×2 ⇒ same digest) ──────────────────
    let digest_a = render_digest(&rsc, file);
    let digest_b = render_digest(&rsc, file);
    check(
        ok,
        matches!((&digest_a, &digest_b), (Ok(a), Ok(b)) if a == b),
        "determinism",
        format!(
            "FNV-1a64 render ×2 = {:?} / {:?}",
            digest_a.map(|hash| format!("{hash:016x}")),
            digest_b.map(|hash| format!("{hash:016x}"))
        ),
    );

    // First rendered line of record 0 (readable evidence).
    let rendered = rsc.record_text(file, 0).unwrap_or_default();
    check(
        ok,
        rendered.starts_with("STRENGTH\n Strength governs"),
        "render_sample",
        format!("id 0: {:?}", rendered.lines().take(2).collect::<Vec<_>>()),
    );
}

/// Self-checks (a)-(d) from spec M16 §3.
fn golden_self_checks(rsc: &TextRsc, file: &[u8], ok: &mut bool) {
    let offset = |id: u16| rsc.index_of(id).map(|index| rsc.records[index].offset);
    check(
        ok,
        offset(475) == Some(26_773) && offset(476) == Some(26_773 + 19),
        "self_check_a",
        format!(
            "header[475]={:?} · header[476]-26773={:?}",
            offset(475),
            offset(476).map(|value| value as i64 - 26_773)
        ),
    );
    let contiguous = match (offset(475), offset(476), offset(477)) {
        (Some(first), Some(second), Some(third)) => {
            let index = rsc.index_of(477).unwrap_or(0);
            let third_len = rsc.record_len(file, index).unwrap_or(0) as u32;
            second == first + 19
                && third == second + 16
                && third.checked_add(third_len) == Some(26_825)
        }
        _ => false,
    };
    check(
        ok,
        contiguous,
        "self_check_b",
        format!(
            "contiguity {:?} → +19 → {:?} → +16 → {:?} (+len) → 26825 (id 478 absent)",
            offset(475),
            offset(476),
            offset(477)
        ),
    );
    let last_end = rsc
        .index_of(9999)
        .and_then(|index| {
            rsc.records[index]
                .offset
                .checked_add(rsc.record_len(file, index)? as u32)
        });
    check(
        ok,
        last_end == Some(353_393) && file.len() == 353_393,
        "self_check_c",
        format!(
            "header[9999].offset+len={last_end:?} · file {} B",
            file.len()
        ),
    );

    // (d) `0xFB` consumes exactly 1 B: with 2 B, `%dam`,
    // `modifier...`, `hand-to-hand...` and `kilograms...` from V6 would disappear.
    let tokens = rsc.record_tokens(file, 0).unwrap_or_default();
    let parameters: Vec<Option<u8>> = tokens
        .iter()
        .filter(|token| token.format == TokenFormat::PositionPrefix)
        .map(|token| token.parameter)
        .collect();
    // Expected structure after each `Pos`: a `Text` token (content is
    // verified by `sha256` in `GOLDEN`; the format suffices here).
    let after: Vec<TokenFormat> = tokens
        .windows(2)
        .filter(|pair| pair[0].format == TokenFormat::PositionPrefix)
        .map(|pair| pair[1].format)
        .collect();
    check(
        ok,
        parameters == vec![Some(20), Some(40), Some(40), Some(20), Some(40)]
            && after == vec![TokenFormat::Text; 5],
        "self_check_d",
        format!("Pos ×5 with 1 B parameter: {parameters:?} · following tokens {after:?}"),
    );
}

/// V1-V13 vectors token by token (V1-V10 exact; V11-V13 structural).
fn golden_tokens(rsc: &TextRsc, file: &[u8], ok: &mut bool) {
    let expect = |id: u16, expected: &[Token], ok: &mut bool| {
        let actual = rsc.record_tokens(file, id);
        // **Structural** comparison: format + parameter, never the text.
        let passed = match actual.as_deref() {
            Ok(tokens) => {
                tokens.len() == expected.len()
                    && tokens
                        .iter()
                        .zip(expected)
                        .all(|(actual, expected)| {
                            actual.format == expected.format
                                && actual.parameter == expected.parameter
                        })
            }
            Err(_) => false,
        };
        check(
            ok,
            passed,
            "golden_tokens",
            format!(
                "id {id}: {} tokens (expected {}){}",
                actual.as_ref().map(|tokens| tokens.len()).unwrap_or(0),
                expected.len(),
                if passed {
                    String::new()
                } else {
                    format!(
                        " — actual {:?}",
                        actual.as_ref().map(|tokens| tokens
                            .iter()
                            .map(|token| token.format)
                            .collect::<Vec<_>>())
                    )
                }
            ),
        );
    };

    expect(475, &[t()], ok);
    expect(476, &[t()], ok);
    expect(477, &[t()], ok);
    expect(
        1000,
        &[
            t(),
            center(),
            t(),
            center(),
            t(),
            center(),
            t(),
            center(),
        ],
        ok,
    );
    expect(
        1500,
        &[
            t(),
            center(),
            t(),
            center(),
            t(),
            center(),
            t(),
            center(),
            t(),
            center(),
        ],
        ok,
    );
    expect(
        0,
        &[
            t(),
            center(),
            t(),
            left(),
            t(),
            left(),
            t(),
            left(),
            t(),
            pos(20),
            t(),
            pos(40),
            t(),
            left(),
            t(),
            pos(40),
            t(),
            left(),
            t(),
            pos(20),
            t(),
            pos(40),
            t(),
            left(),
        ],
        ok,
    );
    expect(
        1,
        &[
            t(),
            center(),
            t(),
            left(),
            t(),
            left(),
            t(),
            left(),
            t(),
            pos(20),
            t(),
            left(),
        ],
        ok,
    );
    expect(
        2,
        &[
            t(),
            center(),
            t(),
            left(),
            t(),
            left(),
            t(),
            left(),
            t(),
            pos(20),
            t(),
            pos(40),
            t(),
            left(),
        ],
        ok,
    );
    expect(
        3,
        &[
            t(),
            center(),
            t(),
            left(),
            t(),
            left(),
            t(),
            left(),
            t(),
            left(),
            t(),
            pos(20),
            t(),
            pos(40),
            t(),
            left(),
        ],
        ok,
    );
    expect(
        4,
        &[
            t(),
            center(),
            t(),
            left(),
            t(),
            left(),
            t(),
            left(),
            t(),
            pos(20),
            t(),
            pos(40),
            t(),
            left(),
            t(),
            pos(40),
            t(),
            left(),
            t(),
            pos(20),
            t(),
            pos(40),
            t(),
            left(),
        ],
        ok,
    );

    // V11: 16 tokens, all justifications `Center`.
    let v11 = rsc.record_tokens(file, 2000).unwrap_or_default();
    check(
        ok,
        v11.len() == 16
            && v11.iter().all(|token| {
                matches!(token.format, TokenFormat::Text | TokenFormat::JustifyCenter)
            })
            && v11
                .iter()
                .filter(|token| token.format == TokenFormat::JustifyCenter)
                .count()
                == 8,
        "golden_v11",
        format!(
            "id 2000: {} tokens · {} Center · 0 Left",
            v11.len(),
            v11.iter()
                .filter(|token| token.format == TokenFormat::JustifyCenter)
                .count()
        ),
    );

    // V12: 36 tokens, `0xFF` presente ⇒ 2 variantes.
    let v12 = rsc.record_tokens(file, 2400).unwrap_or_default();
    let v12_variants = rsc
        .record_variants(file, 2400)
        .map(|variants| variants.len())
        .unwrap_or(0);
    // Structural comparison: format + parameter, never the game text (INV-L99-6).
    let kind = |tokens: &[Token], i: usize| tokens.get(i).map(|token| (token.format, token.parameter));
    check(
        ok,
        v12.len() == 36
            && v12_variants == 2
            && kind(&v12, 0) == Some((TokenFormat::Text, None))
            && kind(&v12, 1).map(|k| k.0) == Some(TokenFormat::JustifyCenter)
            && kind(&v12, 2).map(|k| k.0) == Some(TokenFormat::SubrecordSeparator),
        "golden_v12",
        format!(
            "id 2400: {} tokens · {v12_variants} variants (SubSep) · head {:?}",
            v12.len(),
            &v12[..v12.len().min(3)]
        ),
    );

    // V13: 38 tokens, 18 `Left`, 0 `Center`, 2 variants.
    let v13 = rsc.record_tokens(file, 8722).unwrap_or_default();
    let v13_left = v13
        .iter()
        .filter(|token| token.format == TokenFormat::JustifyLeft)
        .count();
    let v13_center = v13
        .iter()
        .filter(|token| token.format == TokenFormat::JustifyCenter)
        .count();
    check(
        ok,
        v13.len() == 38
            && v13_left == 19
            && v13_center == 0
            && rsc.record_variants(file, 8722).map(|v| v.len()) == Ok(2),
        "golden_v13",
        format!(
            "id 8722: {} tokens · {v13_left} Left · {v13_center} Center · 2 variants",
            v13.len()
        ),
    );
}

/// Block A5: books `BOK*.TXT` (same token flow, `0xF6` end of page).
fn block_books(ok: &mut bool) {
    // Title and author are verified by `sha256`: not a single byte of the book
    // text is reproduced in the repo (INV-L99-6). The `sha256` is as demanding
    // as the previous literal comparison.
    let books = [
        (
            "books/BOK10000.TXT",
            "981861172d117679dca1430671f24736574eb740267ed4edbc3ce0dcece458f4",
            "a77cd1baf364eaec1296e89df88802a0abe2d196a453d519c23e6b0d78ec1387",
            4usize,
        ),
        (
            "books/BOK00000.TXT",
            "9becbeffb0c52370bdfa671fdddb944e5ba95469d08f3661102853b11e8c84ef",
            "f4ed00ae347917bca3d013fa6edf4e9c56ffcf1aa6a6d1637f719d8dd60b2d96",
            9usize,
        ),
    ];
    for (path, title_sha256, author_sha256, pages) in books {
        let bytes = match load(path) {
            Ok(bytes) => bytes,
            Err(reason) => {
                check(ok, false, "book_load", format!("{path}: {reason}"));
                continue;
            }
        };
        match Book::parse(&bytes) {
            Ok(book) => {
                let title_measured = sha256_hex(book.title.as_bytes());
                let author_measured = sha256_hex(book.author.as_bytes());
                let first = book.page_text(0).unwrap_or_default();
                let resolved = !first.is_empty();
                check(
                    ok,
                    title_measured == title_sha256
                        && author_measured == author_sha256
                        && book.pages.len() == pages
                        && resolved,
                    "book",
                    format!(
                        "{path} ({} B): title sha256 {title_measured} · author sha256 \
                         {author_measured} · {} pages · page 0 {} chars",
                        bytes.len(),
                        book.pages.len(),
                        first.chars().count()
                    ),
                );
            }
            Err(err) => check(ok, false, "book_parse", format!("{path}: {err}")),
        }
    }
    let truncated = load("books/BOK10000.TXT").unwrap_or_default();
    let truncated = truncated.get(..100).unwrap_or_default();
    check(
        ok,
        Book::parse(truncated).is_err(),
        "book_error",
        "truncated book (<128 B) ⇒ typed error, 0 panic".to_string(),
    );
}

/// Runs PROBE-QUEST-ORIGINAL-001 — blocks B (QBN/QRC) and C (lore).
pub fn run_quest_original_001() -> i32 {
    println!(
        "PROBE-QUEST-ORIGINAL-001 - retail QBN/QRC quests + 82 verbs (M16, \
         REQ-L99-049 · DoD §g / WI-8)"
    );
    let dir = match crate::mode_a_arena2_dir() {
        Ok(dir) => dir,
        Err(reason) => {
            println!("ARENA2 not accessible (quests): {reason}");
            return 2;
        }
    };
    let qbn_files = match collect(&dir, "QBN") {
        Ok(files) => files,
        Err(reason) => {
            println!("ARENA2 not accessible (QBN): {reason}");
            return 2;
        }
    };
    let qrc_files = match collect(&dir, "QRC") {
        Ok(files) => files,
        Err(reason) => {
            println!("ARENA2 not accessible (QRC): {reason}");
            return 2;
        }
    };
    let mut ok = true;

    // ── B1: 247/247 QBN, 0 error, 6407 opcodes ──────────────────────────
    let mut opcodes = 0usize;
    let mut debug_info = 0usize;
    let mut text_variables = 0usize;
    let mut qbn_errors: Vec<String> = Vec::new();
    let mut reserved_bad = 0usize;
    let mut containers: BTreeMap<&str, Qbn> = BTreeMap::new();
    for (name, bytes) in &qbn_files {
        match Qbn::parse(bytes) {
            Ok(container) => {
                if container.quest_id != 0
                    || container.faction_id != 0
                    || container.resource_id != 0
                    || container.resource_filename != [0u8; 9]
                    || container.null2 != 0
                {
                    reserved_bad += 1;
                }
                if container.opcode_containers(bytes).len() != container.opcode_count() as usize {
                    qbn_errors.push(format!("{name}: truncated opcodes"));
                }
                opcodes += container.opcode_count() as usize;
                if container.has_debug_info != 0 {
                    debug_info += 1;
                }
                if container.text_variable_records > 0 {
                    text_variables += 1;
                }
                containers.insert(name.as_str(), container);
            }
            Err(err) => qbn_errors.push(format!("{name}: {err}")),
        }
    }
    check(
        &mut ok,
        qbn_files.len() == 247 && qbn_errors.is_empty(),
        "qbn_parse",
        format!(
            "{}/{} QBN · {} errors{}",
            qbn_files.len() - qbn_errors.len(),
            qbn_files.len(),
            qbn_errors.len(),
            if qbn_errors.is_empty() {
                String::new()
            } else {
                format!(" — {:?}", &qbn_errors[..qbn_errors.len().min(3)])
            }
        ),
    );
    check(
        &mut ok,
        opcodes == 6_407 && debug_info == 7 && text_variables == 7 && reserved_bad == 0,
        "qbn_container",
        format!(
            "opcodes {opcodes} (retail 6407) · HasDebugInfo≠0 {debug_info} (7) · \
             TextVariables {text_variables} (7) · reserved header ≠0 {reserved_bad}"
        ),
    );

    // ── B2: 244/244 QRC, 3680 records ───────────────────────────────────
    let mut qrc_errors: Vec<String> = Vec::new();
    let mut qrc_records = 0usize;
    let mut id_min = u16::MAX;
    let mut id_max = 0u16;
    let mut duplicate_files = 0usize;
    let mut decreasing_files = 0usize;
    let mut first_offset_anomalies: Vec<String> = Vec::new();
    let mut texts: BTreeMap<&str, Qrc> = BTreeMap::new();
    let mut qrc_raw: BTreeMap<&str, &[u8]> = BTreeMap::new();
    for (name, bytes) in &qrc_files {
        match Qrc::parse(bytes) {
            Ok(qrc) => {
                qrc_records += qrc.record_count();
                let mut seen = std::collections::BTreeSet::new();
                for record in &qrc.text.records {
                    id_min = id_min.min(record.id);
                    id_max = id_max.max(record.id);
                    seen.insert(record.id);
                }
                if seen.len() != qrc.record_count() {
                    duplicate_files += 1;
                }
                if qrc.decreasing_ids > 0 {
                    decreasing_files += 1;
                }
                let header_end = qrc.text.header_length as usize + 2;
                if let Some(first) = qrc.text.records.first() {
                    if first.offset as usize != header_end {
                        first_offset_anomalies.push(format!(
                            "{name}: {} != H+2 ({header_end})",
                            first.offset
                        ));
                    }
                }
                texts.insert(name.as_str(), qrc);
                qrc_raw.insert(name.as_str(), bytes.as_slice());
            }
            Err(err) => qrc_errors.push(format!("{name}: {err}")),
        }
    }
    check(
        &mut ok,
        qrc_files.len() == 244 && qrc_errors.is_empty(),
        "qrc_parse",
        format!(
            "{}/{} QRC · {} errors{}",
            qrc_files.len() - qrc_errors.len(),
            qrc_files.len(),
            qrc_errors.len(),
            if qrc_errors.is_empty() {
                String::new()
            } else {
                format!(" — {:?}", &qrc_errors[..qrc_errors.len().min(3)])
            }
        ),
    );
    check(
        &mut ok,
        qrc_records == 3_680
            && id_min == 1000
            && id_max == 2011
            && duplicate_files == 17
            && decreasing_files == 12,
        "qrc_records",
        format!(
            "records {qrc_records} (retail 3680) · ids {id_min}..{id_max} · \
             files with repeated ids {duplicate_files} (retail 17) · non-monotonic {decreasing_files} (measured 12)"
        ),
    );
    check(
        &mut ok,
        first_offset_anomalies.len() == 1 && first_offset_anomalies[0].starts_with("B0C00Y06"),
        "qrc_first_offset",
        format!(
            "first offset == H+2 except 1 measured padding: {first_offset_anomalies:?}"
        ),
    );

    // ── B3: 82 verbs (17 + 65) ──────────────────────────────────────────
    let triggers = QUEST_VERBS
        .iter()
        .filter(|verb| verb.kind == VerbKind::Trigger)
        .count();
    let actions = QUEST_VERBS
        .iter()
        .filter(|verb| verb.kind == VerbKind::Action)
        .count();
    check(
        &mut ok,
        QUEST_VERBS.len() == 82
            && triggers == QUEST_TRIGGER_COUNT
            && actions == QUEST_ACTION_COUNT,
        "verbs",
        format!(
            "{} verbs · {triggers} triggers + {actions} actions · e.g. `{}` / `{}`",
            QUEST_VERBS.len(),
            QUEST_VERBS[0].verb,
            QUEST_VERBS[81].verb
        ),
    );

    // ── B4: filename families ───────────────────────────────────────────
    let names: Vec<String> = qbn_files.iter().map(|(name, _)| name.clone()).collect();
    let family = |family: QuestFamily| {
        names
            .iter()
            .filter(|name| QuestFamily::of(name) == family)
            .count()
    };
    check(
        &mut ok,
        family(QuestFamily::Main) == 34
            && family(QuestFamily::Starting) == 2
            && family(QuestFamily::Cure) == 2
            && family(QuestFamily::Standard) == 209,
        "families",
        format!(
            "main {} · starting {} · cure {} · standard {} (retail 34/2/2/209)",
            family(QuestFamily::Main),
            family(QuestFamily::Starting),
            family(QuestFamily::Cure),
            family(QuestFamily::Standard)
        ),
    );

    // ── B5: main quest ingestion (34/34) + side quest sampling ──────────
    let main = main_quest_names(&names);
    let main_with_qrc = main
        .iter()
        .filter(|name| texts.contains_key(name.as_str()))
        .count();
    check(
        &mut ok,
        main.len() == 34 && main_with_qrc == 34,
        "main_chain",
        format!(
            "{} main quests · {} with QRC (retail 34/34)",
            main.len(),
            main_with_qrc
        ),
    );

    let sample = sample_side_quests(&names);
    let cells: std::collections::BTreeSet<(u8, u8)> = names
        .iter()
        .filter(|name| QuestFamily::of(name) != QuestFamily::Main)
        .map(|name| {
            let bytes = name.as_bytes();
            (
                bytes.first().copied().unwrap_or(0),
                bytes.get(2).copied().unwrap_or(0),
            )
        })
        .collect();
    check(
        &mut ok,
        sample.len() >= 10 && sample.len() == cells.len(),
        "side_sampling",
        format!(
            "{} side quests sampled · {} byte0×byte2 cells (42 standard + 3 from `$`/`_` \
             measured) · e.g. {:?}",
            sample.len(),
            cells.len(),
            &sample[..sample.len().min(5)]
        ),
    );

    let mut log = QuestLog::new();
    let mut ingest_errors: Vec<String> = Vec::new();
    let mut messages_total = 0usize;
    let mut quests_with_messages = 0usize;
    for name in main.iter().chain(sample.iter()) {
        let qrc = texts
            .get(name.as_str())
            .zip(qrc_raw.get(name.as_str()))
            .map(|(qrc, bytes)| (qrc, *bytes));
        match build_record(name, containers.get(name.as_str()), qrc) {
            Ok(record) => {
                if !record.messages.is_empty() {
                    quests_with_messages += 1;
                }
                messages_total += record.messages.len();
                if !log.ingest(record) {
                    ingest_errors.push(format!("{name}: duplicate"));
                }
            }
            Err(err) => ingest_errors.push(format!("{name}: {err}")),
        }
    }
    let expected_ingested = main.len() + sample.len();
    check(
        &mut ok,
        ingest_errors.is_empty() && log.len() == expected_ingested,
        "ingest",
        format!(
            "{} quests in QuestLog ({} main + {} side) · {messages_total} messages resolved · \
             {quests_with_messages} quests with messages · {} errors{}",
            log.len(),
            main.len(),
            sample.len(),
            quests_with_messages,
            ingest_errors.len()
        ),
    );

    // ── B6: real retail messages, 0 invented string ─────────────────────
    let mut invalid_messages = 0usize;
    let mut empty_messages = 0usize;
    let mut replacement_messages = 0usize;
    let mut non_ascii_messages = 0usize;
    for record in log.records() {
        for message in &record.messages {
            let empty = message.text.is_empty();
            let replacement = message.text.contains('\u{FFFD}');
            let non_ascii = message
                .text
                .chars()
                .any(|ch| ch != '\n' && (ch.is_control() || !ch.is_ascii()));
            if replacement || non_ascii {
                invalid_messages += 1;
                empty_messages += usize::from(empty);
                replacement_messages += usize::from(replacement);
                non_ascii_messages += usize::from(non_ascii);
            } else if empty {
                // Retail record containing only separators: legitimate
                // empty (60 in retail), not an invented string.
                empty_messages += 1;
            }
        }
    }
    let a0 = log.message("A0C00Y00", 1000).unwrap_or_default();
    check(
        &mut ok,
        invalid_messages == 0
            && empty_messages == 60
            && a0.starts_with("My name is _qgiver_.")
            && log.records().iter().all(|record| !record.title.is_empty()),
        "messages",
        format!(
            "0 invalid (replacement {replacement_messages} · non-ASCII {non_ascii_messages}) · \
             legitimate empty {empty_messages} (measured 60) · A0C00Y00#1000 = {:?} · non-empty titles",
            a0.lines().next().unwrap_or_default()
        ),
    );
    let first_active = log.active().first().map(|quest| quest.id.clone());
    let completed = first_active
        .as_deref()
        .and_then(|id| log.complete(id))
        .is_some();
    check(
        &mut ok,
        completed && log.records().len() == expected_ingested,
        "questlog_api",
        format!(
            "M7 API intact: complete({first_active:?}) = {completed} · history {} records",
            log.records().len()
        ),
    );

    // ── B7: golden vectors Q1-Q6 ────────────────────────────────────────
    for golden in &GOLDEN_QUESTS {
        let qbn_bytes = qbn_files
            .iter()
            .find(|(name, _)| name == golden.name)
            .map(|(_, bytes)| bytes);
        let qrc_bytes = qrc_files
            .iter()
            .find(|(name, _)| name == golden.name)
            .map(|(_, bytes)| bytes);
        let mut detail = String::new();
        let mut passed = true;
        match (golden.qbn, qbn_bytes) {
            (Some((size, counts)), Some(bytes)) => {
                let container = Qbn::parse(bytes);
                let ok_qbn = container
                    .as_ref()
                    .map(|container| bytes.len() == size && container.section_counts == counts)
                    == Ok(true);
                passed &= ok_qbn;
                detail.push_str(&format!(
                    "QBN {}/{} B · counts {:?}{}",
                    bytes.len(),
                    size,
                    container.as_ref().map(|c| c.section_counts),
                    if ok_qbn { "" } else { " FAIL" }
                ));
            }
            (None, None) => detail.push_str("QBN absent (expected)"),
            _ => {
                passed = false;
                detail.push_str("QBN expected/actual mismatch");
            }
        }
        match qrc_bytes {
            Some(bytes) => {
                let parsed = Qrc::parse(bytes);
                let ok_qrc = parsed
                    .as_ref()
                    .map(|qrc| {
                        qrc.record_count() == golden.qrc.1
                            && qrc.text.records.first().map(|r| r.id) == Some(golden.qrc.2)
                            && qrc.text.records.last().map(|r| r.id) == Some(golden.qrc.3)
                    })
                    == Ok(true);
                passed &= ok_qrc && bytes.len() == golden.qrc.0;
                detail.push_str(&format!(
                    " · QRC {}/{} B · {} recs (retail {})",
                    bytes.len(),
                    golden.qrc.0,
                    parsed.as_ref().map(|qrc| qrc.record_count()).unwrap_or(0),
                    golden.qrc.1
                ));
            }
            None => {
                passed = false;
                detail.push_str(" · QRC absent");
            }
        }
        let qrc = texts
            .get(golden.name)
            .zip(qrc_raw.get(golden.name))
            .map(|(qrc, bytes)| (qrc, *bytes));
        if let Ok(record) = build_record(golden.name, containers.get(golden.name), qrc) {
            let macros = record.macros();
            for (name, expected) in golden.macros {
                let actual = macros.get(*name).copied().unwrap_or(0);
                if actual != *expected {
                    passed = false;
                    detail.push_str(&format!(" · {name}={actual} (retail {expected})"));
                }
            }
            detail.push_str(&format!(
                " · {} messages · {} distinct macros",
                record.messages.len(),
                macros.len()
            ));
        } else {
            passed = false;
            detail.push_str(" · record not constructible");
        }
        check(&mut ok, passed, "golden_quest", format!("{}: {detail}", golden.name));
    }

    // ── C: lore (RUMOR.DAT) ─────────────────────────────────────────────
    block_lore(&mut ok);

    if ok {
        println!(
            "VERDICT: PROBE-QUEST-ORIGINAL-001 PASS (247/247 QBN · 244/244 QRC · 82 verbs \
             · {} quests in history · {} active)",
            log.records().len(),
            log.len()
        );
        0
    } else {
        println!("VERDICT: PROBE-QUEST-ORIGINAL-001 FAIL");
        1
    }
}

/// Builds the projection of a retail quest (`QBN` container + `QRC` text).
fn build_record(
    name: &str,
    qbn: Option<&Qbn>,
    qrc: Option<(&Qrc, &[u8])>,
) -> Result<QuestRecord, String> {
    let counts = match qbn {
        Some(container) => QuestCounts {
            items: container.section_counts[0],
            npcs: container.section_counts[3],
            locations: container.section_counts[4],
            timers: container.section_counts[6],
            mobs: container.section_counts[7],
            opcodes: container.section_counts[8],
            states: container.section_counts[9],
            text_variables: container.text_variable_records,
        },
        None => QuestCounts::default(),
    };
    let mut messages = Vec::new();
    if let Some((qrc, bytes)) = qrc {
        for index in 0..qrc.record_count() {
            let id = qrc.text.records[index].id;
            let text = qrc
                .text
                .record_text_by_index(bytes, index)
                .map_err(|err| format!("{name} id {id}: {err}"))?;
            messages.push(QuestMessage { id, text });
        }
    }
    Ok(QuestRecord::new(name, counts, messages))
}

/// Block C: `RUMOR.DAT` (3506 B, 31 records) and ≥1 resolved rumor.
fn block_lore(ok: &mut bool) {
    let bytes = match load("RUMOR.DAT") {
        Ok(bytes) => bytes,
        Err(reason) => {
            check(ok, false, "rumor_load", format!("RUMOR.DAT: {reason}"));
            return;
        }
    };
    let rumors = match RumorFile::parse(&bytes) {
        Ok(rumors) => rumors,
        Err(err) => {
            check(ok, false, "rumor_parse", format!("{err}"));
            return;
        }
    };
    let first = rumors.rumors.first();
    let mut types = BTreeMap::new();
    let mut flags = BTreeMap::new();
    let mut flags8_types = std::collections::BTreeSet::new();
    for rumor in &rumors.rumors {
        *types.entry(rumor.kind).or_insert(0usize) += 1;
        *flags.entry(rumor.flags).or_insert(0usize) += 1;
        if rumor.flags == 8 {
            flags8_types.insert(rumor.kind);
        }
    }
    check(
        ok,
        bytes.len() == 3_506
            && rumors.rumors.len() == 31
            && first.map(|rumor| (rumor.faction1, rumor.faction2, rumor.kind, rumor.region, rumor.flags))
                == Some((508, 810, 27, 0, 1))
            && first.map(|rumor| rumor.text_length) == Some(107)
            && first.map(|rumor| rumor.time_limit) == Some(566_670)
            && rumors.rumors.iter().all(|rumor| {
                rumor.time_limit == 566_670
                    && rumor.unknown == 0
                    && rumor.reserved == [0, 0, 0]
                    && rumor.quest_name == [0u8; 16]
            }),
        "rumor_file",
        format!(
            "{} B · {} records · header0 F1/F2/Type/Region/Flags = {:?} · TimeLimit 566670 ×31",
            bytes.len(),
            rumors.rumors.len(),
            first.map(|rumor| (rumor.faction1, rumor.faction2, rumor.kind, rumor.region, rumor.flags))
        ),
    );
    check(
        ok,
        types == BTreeMap::from([(4u8, 1usize), (7, 3), (12, 5), (26, 10), (27, 12)])
            && flags == BTreeMap::from([(1u8, 26usize), (8, 5)])
            && flags8_types == std::collections::BTreeSet::from([12u8]),
        "rumor_histogram",
        format!("Type {types:?} · Flags {flags:?} · Flags=8 ⇒ Type {flags8_types:?}"),
    );
    // Record 0 is verified by length + sha256 + number of lines: the retail
    // text is not reproduced in the repo (INV-L99-6).
    let text = rumors.text(0).unwrap_or_default();
    let text_sha256 = sha256_hex(text.as_bytes());
    check(
        ok,
        text.chars().count() == 106
            && text.lines().count() == 2
            && text_sha256 == "15edf61b2edfce962d06428899990f3bdcc41f60e823a5260758380635aef460",
        "rumor_text",
        format!(
            "record 0: {} chars · {} lines · sha256 {text_sha256}",
            text.chars().count(),
            text.lines().count()
        ),
    );
    let mut region_min = u8::MAX;
    let mut region_max = 0u8;
    for rumor in &rumors.rumors {
        region_min = region_min.min(rumor.region);
        region_max = region_max.max(rumor.region);
    }
    check(
        ok,
        region_min == 0 && region_max == 61,
        "rumor_regions",
        format!("RegionID {region_min}..{region_max} (62 regions)"),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lowercase hex representation (test-internal expectations only).
    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn fnv_digest_is_stable_and_sensitive() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), fnv1a64(b"a"));
        assert_ne!(fnv1a64(b"a"), fnv1a64(b"b"));
    }

    #[test]
    fn hex_matches_lowercase_encoding() {
        assert_eq!(hex(&[0x00, 0xfe, 0x7f]), "00fe7f");
    }
}

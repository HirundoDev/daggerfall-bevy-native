//! MEASUREMENT INSTRUMENT (temporary, CR-f1b5045b): dumps every record of the
//! people billboard archive (`TEXTURE.182`, `medium Common people`) with the
//! engine's own decoder so each record can be classified from the **pixels**
//! (clothed / nude, gender, race) instead of from an invented list.
//!
//! Run (retail data required, 0 retail bytes in git):
//! ```text
//! DF_FLAT_DUMP_DIR=/tmp/flats cargo test -p daggerfall-parser-bevy-bridge \
//!   --test person_flats_inventory -- --ignored --nocapture
//! ```
//! Writes `inventory.tsv` (one row per record/frame) and `rec{index}_f{frame}.rgba`
//! (`u32 LE width · u32 LE height · RGBA rows`), 0 other side effect.

use std::path::{Path, PathBuf};

use daggerfall_parser_bevy_bridge::npc_spec::{self, NpcWorld, PopulationRequest};
use daggerfall_parser_rs::namegen::NameGen;
use daggerfall_parser_rs::text_rsc::TextRsc;
use daggerfall_parser_rs::texture::{self, Palette, TextureArchive};

const PEOPLE_FILE: &str = "TEXTURE.182";

fn arena2_dir() -> Option<PathBuf> {
    let dir = match std::env::var("DF_FLAT_ARENA2") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => {
            let home = std::env::var("HOME").ok()?;
            PathBuf::from(home).join("Games/Daggerfall/app/ARENA2")
        }
    };
    dir.is_dir().then_some(dir)
}

fn read(dir: &Path, name: &str) -> Vec<u8> {
    std::fs::read(dir.join(name)).unwrap_or_else(|err| panic!("{name}: {err}"))
}

#[test]
#[ignore = "measurement instrument: needs the retail ARENA2 and writes evidence dumps"]
fn dump_people_flats() {
    let out = std::env::var("DF_FLAT_DUMP_DIR").expect("DF_FLAT_DUMP_DIR");
    let out = PathBuf::from(out);
    std::fs::create_dir_all(&out).expect("dump dir");
    let Some(dir) = arena2_dir() else {
        eprintln!("SKIP: no ARENA2 (set DF_FLAT_ARENA2)");
        return;
    };
    let file = read(&dir, PEOPLE_FILE);
    let palette = Palette::parse(&read(&dir, "PAL.PAL")).expect("PAL.PAL");
    let archive = TextureArchive::parse(&file).expect(PEOPLE_FILE);

    let mut tsv = String::from(
        "record\tkind\tframes\twidth\theight\tcompression\trecord_size\toffset_x\toffset_y\tdecoded\n",
    );
    for (index, header) in archive.records.iter().enumerate() {
        match header.kind {
            texture::RecordKind::Image(meta) => {
                let frames = meta.frame_count.max(1);
                for frame in 0..frames {
                    let decoded =
                        texture::decode_frame(&file, &archive, index, frame).expect("decode");
                    let rgba = palette.to_rgba(&decoded.pixels);
                    let mut dump = Vec::with_capacity(8 + rgba.len());
                    dump.extend_from_slice(&u32::from(decoded.width).to_le_bytes());
                    dump.extend_from_slice(&u32::from(decoded.height).to_le_bytes());
                    dump.extend_from_slice(&rgba);
                    std::fs::write(out.join(format!("rec{index:03}_f{frame:02}.rgba")), &dump)
                        .expect("write dump");
                }
                tsv.push_str(&format!(
                    "{index}\timage\t{frames}\t{}\t{}\t{}\t{}\t{}\t{}\tyes\n",
                    meta.width,
                    meta.height,
                    meta.compression,
                    meta.record_size,
                    meta.offset_x,
                    meta.offset_y
                ));
            }
            texture::RecordKind::Solid { palette_index } => {
                tsv.push_str(&format!(
                    "{index}\tsolid\t0\t0\t0\t0\t0\t0\t0\tno(palette {palette_index})\n"
                ));
            }
            texture::RecordKind::NoImage => {
                tsv.push_str(&format!("{index}\tnoimage\t0\t0\t0\t0\t0\t0\t0\tno\n"));
            }
        }
    }
    std::fs::write(out.join("inventory.tsv"), tsv).expect("write inventory");
    eprintln!(
        "dumped {} records of {PEOPLE_FILE} ({}) into {}",
        archive.records.len(),
        archive.name.trim(),
        out.display()
    );
}

/// Prints the exact `(record, gender, direction, frame)` the demo population
/// draws, with the demo's own request (`m18_demo::{NPC_SEED, NPC_REGION,
/// NPC_SUBCLIMATE, NPC_COUNT}` = 20_261_020 / 17 / 231 / 24) — the repro anchor
/// of the "nude NPC in the street" report.
#[test]
#[ignore = "measurement instrument: needs the retail ARENA2"]
fn dump_demo_population() {
    let Some(dir) = arena2_dir() else {
        eprintln!("SKIP: no ARENA2 (set DF_FLAT_ARENA2)");
        return;
    };
    let namegen = NameGen::parse(&read(&dir, "NAMEGEN.DAT")).expect("NAMEGEN.DAT");
    let flats_bytes = read(&dir, PEOPLE_FILE);
    let flats = TextureArchive::parse(&flats_bytes).expect(PEOPLE_FILE);
    let text_bytes = read(&dir, "TEXT.RSC");
    let text = TextRsc::parse(&text_bytes).expect("TEXT.RSC");
    let world = NpcWorld {
        namegen: &namegen,
        person_flats: &flats,
        text: &text,
        text_bytes: &text_bytes,
    };
    // Same request the integrated demo issues (m18_demo:2208-2216).
    let population = world
        .populate(PopulationRequest {
            seed: 20_261_020,
            region: 17,
            subclimate: 231,
            count: 24,
        })
        .expect("population");
    println!("index\trecord\tframes\tdirection\tframe\tgender");
    for npc in &population.npcs {
        let frame = u16::from(npc.flat.direction).min(npc.flat.frames.saturating_sub(1));
        println!(
            "{}\t{}\t{}\t{}\t{}\t{:?}",
            npc.index, npc.flat.record, npc.flat.frames, npc.flat.direction, frame, npc.gender
        );
    }
    println!("digest=0x{:016x}", population.digest());
    println!("flat_count={}", npc_spec::PERSON_FLAT_ARCHIVE);
}

/// **Antibody on the retail archive (CR-f1b5045b).** Proves, against the
/// user's own `TEXTURE.182`, that:
/// 1. the demo population (`NPC_SEED/NPC_REGION/NPC_SUBCLIMATE`, 24 draws)
///    contains **no** `TOPLESS_RECORDS` index — the choke point in
///    `NpcWorld::resolve_flat` is what makes this hold;
/// 2. every measured substitute of `TOPLESS_SUBSTITUTE` exists in the file as
///    an `Image` with at least one frame and decodes to real pixels (the
///    destination is a billboard, never a solid/no-image record);
/// 3. the featured woman (`m18_demo::FEATURED_FLAT_RECORD` = 50) exists as an
///    `Image` of 4 frames that decodes — the billboard the demo appends as the
///    25th cell is real.
///
/// "Dressed" and "female" are the pixel classification of the report
/// (`docs/sintergic/CR-f1b5045b-npc-flats-182.md`: 2 VLM passes + 3×/6×/10×
/// montages); this test pins the **existence and decodability** those claims
/// rest on.
#[test]
#[ignore = "measurement instrument: needs the retail ARENA2"]
fn reclassification_holds_on_the_retail_archive() {
    let Some(dir) = arena2_dir() else {
        eprintln!("SKIP: no ARENA2 (set DF_FLAT_ARENA2)");
        return;
    };
    let flats_bytes = read(&dir, PEOPLE_FILE);
    let flats = TextureArchive::parse(&flats_bytes).expect(PEOPLE_FILE);
    let palette = Palette::parse(&read(&dir, "PAL.PAL")).expect("PAL.PAL");

    // 1 · no topless record can leave the choke point.
    let namegen = NameGen::parse(&read(&dir, "NAMEGEN.DAT")).expect("NAMEGEN.DAT");
    let text_bytes = read(&dir, "TEXT.RSC");
    let text = TextRsc::parse(&text_bytes).expect("TEXT.RSC");
    let world = NpcWorld {
        namegen: &namegen,
        person_flats: &flats,
        text: &text,
        text_bytes: &text_bytes,
    };
    let population = world
        .populate(PopulationRequest {
            seed: 20_261_020,
            region: 17,
            subclimate: 231,
            count: 24,
        })
        .expect("population");
    for npc in &population.npcs {
        assert!(
            !npc_spec::TOPLESS_RECORDS.contains(&npc.flat.record),
            "index {} resolved to the topless record {}",
            npc.index,
            npc.flat.record
        );
        assert!(npc.flat.frames > 0, "index {} without frames", npc.index);
    }

    // 2 · every substitute is a decodable image.
    for (from, to) in npc_spec::TOPLESS_SUBSTITUTE {
        assert!(npc_spec::TOPLESS_RECORDS.contains(&from));
        assert!(!npc_spec::TOPLESS_RECORDS.contains(&to));
        let header = flats
            .records
            .get(to as usize)
            .unwrap_or_else(|| panic!("substitute record {to} missing"));
        let meta = match header.kind {
            texture::RecordKind::Image(meta) => meta,
            other => panic!("substitute record {to} is not an image: {other:?}"),
        };
        assert!(meta.frame_count >= 1, "substitute {to} without frames");
        let image = texture::decode_frame(&flats_bytes, &flats, to as usize, 0)
            .unwrap_or_else(|err| panic!("substitute {to}: {err:?}"));
        assert!(image.width > 0 && image.height > 0);
        let rgba = palette.to_rgba(&image.pixels);
        assert_eq!(
            rgba.len(),
            usize::from(image.width) * usize::from(image.height) * 4
        );
        println!(
            "substitute {from}→{to}: {}×{} · {} frames · {} B RGBA",
            image.width,
            image.height,
            meta.frame_count,
            rgba.len()
        );
    }

    // 3 · the featured woman exists and decodes.
    const FEATURED: u16 = 50;
    assert!(!npc_spec::TOPLESS_RECORDS.contains(&FEATURED));
    assert_eq!(npc_spec::clothed_flat_record(FEATURED), FEATURED);
    let header = flats.records.get(FEATURED as usize).expect("featured record");
    let meta = match header.kind {
        texture::RecordKind::Image(meta) => meta,
        other => panic!("featured record {FEATURED} is not an image: {other:?}"),
    };
    assert!(meta.frame_count >= 1);
    let image = texture::decode_frame(&flats_bytes, &flats, FEATURED as usize, 0)
        .expect("featured frame 0");
    println!(
        "featured {}: {}×{} · {} frames · decoded {}×{} ({})",
        FEATURED,
        image.width,
        image.height,
        meta.frame_count,
        image.width,
        image.height,
        picture::sha(&palette.to_rgba(&image.pixels)),
    );
}

/// FNV-1a 64 of a byte slice, so the decoded art can be pinned without
/// committing a single retail pixel.
mod picture {
    pub fn sha(bytes: &[u8]) -> String {
        // 0 dependencies (the bridge crate has no hasher of its own here).
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        format!("fnv1a64 0x{hash:016x} · {} B", bytes.len())
    }
}

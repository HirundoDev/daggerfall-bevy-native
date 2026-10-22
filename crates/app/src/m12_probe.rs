//! M12 — anti-screen probe for the retail visual layer (`TEXTURE.*`,
//! DoD §c / WI-4).
//!
//! **PROBE-TEXTURE-001** (`--probe TEXTURE-001`): decodes at runtime 3 retail
//! files from the user's ARENA2 (`TEXTURE.002`, `TEXTURE.007`,
//! `TEXTURE.256`) × their 4 rotations (records 0..3 of the rotation
//! quadruplet) and compares the **FNV-1a 64** digest of the decoded pixels
//! against the 12 pinned constants ([`GOLDEN`]) — byte-by-byte regression of
//! the 4 decoders in `daggerfall_parser_rs::texture` (single-frame,
//! multiframe, image RLE and record RLE) over **real** data.
//!
//! **Source custody (INV-L99-6):** Mode A only ([`crate::
//! mode_a_arena2_path`]); the retail bytes are read from the user's ARENA2 at
//! runtime and NEVER enter the repository — only 12 `u64` live here. Without
//! an accessible ARENA2 the probe exits with **exit 2** and the literal
//! reason, never with a false PASS.
//!
//! Checks (all binary, 0 fuzzy thresholds):
//! 1. the 3 files parse (`TextureArchive::parse`) and declare ≥ 4 records;
//! 2. `width`/`height`/`FNV-1a64(pixels)` of each rotation == constant;
//! 3. the 4 rotations of a file differ from each other (the rotation
//!    quadruplet is not a duplicate);
//! 4. the retail palette `PAL.PAL` (776 B, `ColHeader` `0xB123`) parses and
//!    its `to_rgba` honours index-0 transparency — the pixels→color chain
//!    that feeds `bevy_assets::image_from_rgba`.
use daggerfall_parser_rs::texture::{self, Palette, TextureArchive};

/// Golden of a retail texture: file + 4 rotations
/// `(width, height, FNV-1a64 of the palette indices)`.
struct GoldenTexture {
    file: &'static str,
    rotations: [(u16, u16, u64); 4],
}

/// Constants pinned from ARENA2 on 2026-10-20 (M12).
/// 12 digests: `TEXTURE.002` (56 records), `TEXTURE.007` (4 records) and
/// `TEXTURE.256` (15 records), records 0..3 of each one.
const GOLDEN: [GoldenTexture; 3] = [
    GoldenTexture {
        file: "TEXTURE.002",
        rotations: [
            (64, 64, 0x02f5_dafc_4bbd_9a06),
            (64, 64, 0x175f_0b43_40d4_683d),
            (64, 64, 0xe502_6aef_3532_c134),
            (64, 64, 0x448d_775e_9780_8530),
        ],
    },
    GoldenTexture {
        file: "TEXTURE.007",
        rotations: [
            (64, 64, 0xe270_acea_3a0e_a76d),
            (64, 64, 0xf98c_a0bf_4c9f_8e59),
            (64, 64, 0x0321_833f_c04b_e70a),
            (64, 64, 0x9634_9a83_bd8d_8243),
        ],
    },
    GoldenTexture {
        file: "TEXTURE.256",
        rotations: [
            (172, 129, 0x8d9e_1f07_b745_34c0),
            (90, 111, 0xd17a_58e5_2b25_7f8e),
            (42, 90, 0x944c_9909_e33e_95c8),
            (75, 68, 0x95e4_d81b_8157_15c1),
        ],
    },
];

/// FNV-1a 64 (deterministic, 0 dependencies) over the palette indices.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Runs PROBE-TEXTURE-001 — retail `TEXTURE.*` textures (M12).
pub fn run_texture_001() -> i32 {
    println!(
        "PROBE-TEXTURE-001 - retail TEXTURE.* textures (M12, CR-46a43aba, \
         REQ-L99-042/043 · DoD §c / WI-4)"
    );

    // ── 1. Mode A custody: the 3 files + the palette must exist ────
    let mut sources: Vec<(&'static str, String, Vec<u8>)> = Vec::new();
    for golden in &GOLDEN {
        let path = match crate::mode_a_arena2_path(golden.file) {
            Ok(path) => path,
            Err(reason) => {
                println!("ARENA2 not accessible: {reason}");
                return 2;
            }
        };
        match std::fs::read(&path) {
            Ok(bytes) => sources.push((golden.file, path.display().to_string(), bytes)),
            Err(err) => {
                println!("ARENA2 not accessible: reading {path:?}: {err}");
                return 2;
            }
        }
    }
    let palette_path = match crate::mode_a_arena2_path("PAL.PAL") {
        Ok(path) => path,
        Err(reason) => {
            println!("ARENA2 not accessible: {reason}");
            return 2;
        }
    };
    let palette_bytes = match std::fs::read(&palette_path) {
        Ok(bytes) => bytes,
        Err(err) => {
            println!("ARENA2 not accessible: reading {palette_path:?}: {err}");
            return 2;
        }
    };

    let mut all_ok = true;
    let mut diffs = 0usize;
    let mut digests_matched = 0usize;

    // ── 2. 3 files × 4 rotations == 12 constants ───────────────────
    for (golden, (file, path, bytes)) in GOLDEN.iter().zip(&sources) {
        let archive = match TextureArchive::parse(bytes) {
            Ok(archive) => archive,
            Err(err) => {
                println!("[FAIL] texture_{file}_parse - {path}: {err:?}");
                all_ok = false;
                continue;
            }
        };
        let parse_ok = archive.record_count >= 4 && archive.records.len() >= 4;
        println!(
            "[{}] texture_{file}_archive - {} · {} records · {} B · 4 rotations required",
            if parse_ok { "OK" } else { "FAIL" },
            file,
            archive.record_count,
            bytes.len(),
        );
        all_ok &= parse_ok;

        let mut seen: Vec<u64> = Vec::with_capacity(4);
        for (rotation, (want_w, want_h, want_fnv)) in golden.rotations.iter().enumerate() {
            let decoded = texture::decode_frame(bytes, &archive, rotation, 0);
            let (got_w, got_h, got_fnv, pixels) = match decoded {
                Ok(img) => (img.width, img.height, fnv1a64(&img.pixels), img.pixels.len()),
                Err(err) => {
                    println!(
                        "[FAIL] texture_{file}_r{rotation} - decode_frame({rotation}): {err:?}"
                    );
                    all_ok = false;
                    continue;
                }
            };
            let digest_ok = got_w == *want_w && got_h == *want_h && got_fnv == *want_fnv;
            if digest_ok {
                digests_matched += 1;
            } else {
                diffs += 1;
            }
            println!(
                "[{}] texture_{file}_r{rotation} - {file} record {rotation} · {got_w}×{got_h} px \
                 ({pixels} indices) · FNV-1a64 0x{got_fnv:016x} vs golden 0x{want_fnv:016x}",
                if digest_ok { "OK" } else { "FAIL" },
            );
            all_ok &= digest_ok;
            seen.push(got_fnv);
        }

        seen.sort_unstable();
        seen.dedup();
        let distinct = seen.len() == 4;
        println!(
            "[{}] texture_{file}_rotations_distinct - 4 rotations with distinct digest: {} \
             unique",
            if distinct { "OK" } else { "FAIL" },
            seen.len(),
        );
        all_ok &= distinct;
    }

    // ── 3. Consumable retail palette (pixels→RGBA8 chain) ──────────────
    match Palette::parse(&palette_bytes) {
        Ok(palette) => {
            let decoded = sources
                .iter()
                .find(|(file, _, _)| *file == "TEXTURE.256")
                .and_then(|(file, _, bytes)| {
                    TextureArchive::parse(bytes)
                        .ok()
                        .and_then(|archive| texture::decode_frame(bytes, &archive, 0, 0).ok())
                        .map(|img| (file, img))
                });
            let palette_ok = match decoded {
                Some((_, img)) => {
                    let rgba = palette.to_rgba(&img.pixels);
                    let transparent = img.pixels.iter().filter(|&&px| px == 0).count();
                    let rgba_transparent = rgba.chunks_exact(4).filter(|c| c[3] == 0).count();
                    let alpha_ok = rgba.len() == img.pixels.len() * 4
                        && transparent == rgba_transparent;
                    println!(
                        "[{}] palette_pal_255 - PAL.PAL {} B · 256 colors · RGBA8 {} B for \
                         {} indices · indices 0 => alpha 0: {} == {}",
                        if alpha_ok { "OK" } else { "FAIL" },
                        palette_bytes.len(),
                        rgba.len(),
                        img.pixels.len(),
                        transparent,
                        rgba_transparent,
                    );
                    alpha_ok
                }
                None => {
                    println!("[FAIL] palette_pal_255 - no decoded TEXTURE.256 image");
                    false
                }
            };
            all_ok &= palette_ok;
        }
        Err(err) => {
            println!("[FAIL] palette_pal_255 - PAL.PAL: {err:?}");
            all_ok = false;
        }
    }

    println!(
        "[{}] texture_digests_12 - {digests_matched}/12 FNV-1a64 digests equal to the \
         constant · {diffs} discrepancies",
        if digests_matched == 12 && diffs == 0 {
            "OK"
        } else {
            "FAIL"
        },
    );

    if all_ok {
        println!(
            "VERDICT: PROBE-TEXTURE-001 PASS - 12/12 FNV-1a64 digests (TEXTURE.002/007/256 × \
             4 rotations) over retail data decoded at runtime · PAL.PAL palette \
             consumable (index 0 => alpha 0) · 0 retail bytes in git (INV-L99-6)"
        );
        0
    } else {
        println!("VERDICT: PROBE-TEXTURE-001 FAIL - see [FAIL] rows");
        1
    }
}

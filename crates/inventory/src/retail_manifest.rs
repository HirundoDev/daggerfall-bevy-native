//! Canonical retail manifest of the engine (data contract §2, §4.2, §4.3).
//!
//! One list, one name-resolution rule. The resolver ([`crate::data_root`])
//! scores candidates with this manifest, [`crate::data_root::DataRoot::verify`]
//! validates the files in it, and every reader (preflight, UI assets, audio,
//! probes) opens names through the same case-insensitive resolution — the
//! contract measured two competing rules (§1.6/R4: the resolver was
//! case-insensitive while the preflight and `ui_assets` used `dir.join`, so an
//! installation with lowercase entries passed L0 and failed L1).
//!
//! **D11 (`TEXTURE.256`):** the contract's core list carried `TEXTURE.256`
//! (§2.1, 14 files) while the demo reads `TEXTURE.182` (`PERSON_FLAT_FILE`) and
//! `TEXTURE.399` (`GUARD_FLAT_FILE`); `TEXTURE.256` would only be opened if a
//! city texture record referenced file 256 (§2.3.1). It is therefore **not**
//! required: a trimmed installation without it is complete for this engine.
//! The consequence is declared: the candidate score ceiling is
//! [`CORE_REQUIRED`] + `FALL.EXE` = **14**, exactly the §4.2 threshold, so the
//! threshold did not move when the file left the list.

use std::path::{Path, PathBuf};

/// Mandatory retail files inside `ARENA2` (L1, `E-DATA-003`).
///
/// §2.1 core list **minus `TEXTURE.256`** (D11, see the module doc): the demo
/// consumes `TEXTURE.182` (person flats) and `TEXTURE.399` (city watch), which
/// are the archives it actually reads.
pub const CORE_REQUIRED: &[&str] = &[
    "MAPS.BSA",
    "BLOCKS.BSA",
    "ARCH3D.BSA",
    "WOODS.WLD",
    "NAMEGEN.DAT",
    "TEXTURE.182",
    "TEXTURE.399",
    "TEXT.RSC",
    "PAL.PAL",
    "FMAP_PAL.COL",
    "ART_PAL.COL",
    "CLASS01.CFG",
    "BIO.DAT",
];

/// Flat archive of the common people the demo wanders (60 records).
pub const PERSON_FLAT_FILE: &str = "TEXTURE.182";
/// Flat archive of the city watch (20 records).
pub const GUARD_FLAT_FILE: &str = "TEXTURE.399";
/// Character class of the demo's real character (74 B).
pub const M20_CLASS_FILE: &str = "CLASS01.CFG";
/// Biography of the demo's real character.
pub const M20_BIO_FILE: &str = "BIO.DAT";

/// Assets consumed by the 4 UI panels (headerless `RCI`/`CIF` included).
pub const PANEL_ASSETS: [&str; 43] = [
    "MAIN00I0.IMG",
    "MAIN02I0.IMG",
    "MAIN03I0.IMG",
    "COMPASS.IMG",
    "ITEM00I0.IMG",
    "INVE00I0.IMG",
    "INVE03I0.IMG",
    "INVE06I0.IMG",
    "INVE07I0.IMG",
    "INVE08I0.IMG",
    "INVE09I0.IMG",
    "INVE10I0.IMG",
    "INVE11I0.IMG",
    "INVE12I0.IMG",
    "INVE13I0.IMG",
    "INVE14I0.IMG",
    "ITEM01I0.IMG",
    "BOOK00I0.IMG",
    "SCBG00I0.IMG",
    "SCBG01I0.IMG",
    "SCBG02I0.IMG",
    "SCBG03I0.IMG",
    "SCBG04I0.IMG",
    "SCBG05I0.IMG",
    "SCBG06I0.IMG",
    "SCBG07I0.IMG",
    "SCBG08I0.IMG",
    "SCRL04I0.IMG",
    "SCRL05I0.IMG",
    "SCRL07I0.IMG",
    "SCRL08I0.IMG",
    "SCRL09I0.IMG",
    "SCRL10I0.IMG",
    "SCRL11I0.IMG",
    "SCRL12I0.IMG",
    "SCRL13I0.IMG",
    "SCRL14I0.IMG",
    "TITL00I0.IMG",
    "MENU.IMG",
    "BUTN00I0.IMG",
    "BUTN03I0.IMG",
    // **B3-c** — the two map screens (raw 320×200 RCI). Their palette index
    // range is resolved per asset (`image::ASSET_RANGE_DERIVATIONS`), not by
    // widening the `Map` family floor.
    "MAP100I0.IMG",
    "TRAV0I00.IMG",
];

/// Runtime `.FNT` fonts (C7b: 0 system fonts).
///
/// Mirrors `daggerfall_parser_bevy_bridge::ui_spec::FONT_FILES` (the renderer's
/// own list, out of this lane's scope); a test in `daggerfall-app` asserts the
/// two lists are equal, so the mirror cannot drift silently.
pub const FONT_FILES: [&str; 5] = [
    "FONT0000.FNT",
    "FONT0001.FNT",
    "FONT0002.FNT",
    "FONT0003.FNT",
    "FONT0004.FNT",
];

/// Optional retail files (L1 warns, never aborts; §2.2).
pub const OPTIONAL_WARN: &[&str] = &["DAGGER.SND", "MIDI.BSA", "CLIMATE.PAK", "CLASS18.CFG"];

/// Install-root file (never inside `ARENA2`): item/class tables (§2.1 root).
pub const FALL_EXE: &str = "FALL.EXE";

/// Both accepted spellings of the data directory (§4.1 normalization).
pub const ARENA2_NAMES: [&str; 2] = ["ARENA2", "arena2"];

/// Score ceiling of §4.2: every [`CORE_REQUIRED`] file plus `FALL.EXE` in the
/// install root. A candidate below this is listed, never chosen.
pub const MAX_SCORE: usize = CORE_REQUIRED.len() + 1;

/// Case-insensitive resolution of `name` inside `dir` (§1.1 step 5).
///
/// An exact-case match wins (deterministic precedence over any variant); then
/// the directory is scanned for entries equal under `to_ascii_lowercase()`.
/// Directory order is not stable across filesystems, so candidates are sorted
/// and the first one wins: two case variants resolve to the same file on every
/// run. Directories are rejected (`is_file`).
pub fn find_child_ci(dir: &Path, name: &str) -> Option<PathBuf> {
    let exact = dir.join(name);
    if exact.is_file() {
        return Some(exact);
    }
    let wanted = name.to_ascii_lowercase();
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|found| found.to_str())
                .is_some_and(|found| found.to_ascii_lowercase() == wanted)
        })
        .filter(|path| path.is_file())
        .collect();
    candidates.sort();
    candidates.into_iter().next()
}

/// True when `dir` is an install root: it has `ARENA2` (or `arena2`) with
/// `ARCH3D.BSA` inside (L0, §4.3).
pub fn looks_like_data_root(dir: &Path) -> bool {
    arena2_dir(dir).is_some()
}

/// The `ARENA2`/`arena2` child of `dir` when it holds `ARCH3D.BSA` (L0).
pub fn arena2_dir(dir: &Path) -> Option<PathBuf> {
    ARENA2_NAMES
        .iter()
        .map(|name| dir.join(name))
        .find(|path| path.join("ARCH3D.BSA").is_file())
}

/// §4.2 score of a candidate root: `1` per [`CORE_REQUIRED`] file present
/// (case-insensitive) plus `1` for `FALL.EXE` in the root itself.
///
/// Returns `None` when the candidate does not pass L0, so an absent directory
/// is not scored as if it were an empty installation.
pub fn score(root: &Path) -> Option<usize> {
    let arena2 = arena2_dir(root)?;
    let files = CORE_REQUIRED
        .iter()
        .filter(|name| find_child_ci(&arena2, name).is_some())
        .count();
    let root_file = usize::from(find_child_ci(root, FALL_EXE).is_some());
    Some(files + root_file)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after the Unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "df-manifest-{tag}-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    /// The case-insensitive rule is one implementation: a lowercase entry
    /// resolves for the declared name and an exact match still wins.
    #[test]
    fn lowercase_entries_resolve_case_insensitively() {
        let dir = scratch("ci");
        std::fs::write(dir.join("text.rsc"), b"x").expect("write lowercase");
        let found = find_child_ci(&dir, "TEXT.RSC").expect("lowercase entry resolves");
        assert_eq!(found.file_name().unwrap(), "text.rsc");
        std::fs::write(dir.join("MAPS.BSA"), b"y").expect("write exact");
        let found = find_child_ci(&dir, "MAPS.BSA").expect("exact entry resolves");
        assert_eq!(found.file_name().unwrap(), "MAPS.BSA");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A directory named like the file is not the file.
    #[test]
    fn directories_are_not_files() {
        let dir = scratch("dirs");
        std::fs::create_dir_all(dir.join("MAPS.BSA")).expect("create dir");
        assert!(find_child_ci(&dir, "MAPS.BSA").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// L0 accepts both spellings and the score counts presence, not content.
    #[test]
    fn score_counts_required_files_and_fall_exe() {
        let root = scratch("score");
        let arena2 = root.join("arena2");
        std::fs::create_dir_all(&arena2).expect("create arena2");
        std::fs::write(arena2.join("ARCH3D.BSA"), b"x").expect("write ARCH3D");
        assert_eq!(score(&root), Some(1), "only ARCH3D present, no FALL.EXE");
        std::fs::write(root.join("FALL.EXE"), b"x").expect("write FALL");
        for name in CORE_REQUIRED {
            std::fs::write(arena2.join(name), b"x").expect("write core");
        }
        assert_eq!(score(&root), Some(MAX_SCORE));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// An absent directory is not a candidate at all (§4.2: L0 first).
    #[test]
    fn absent_directory_is_not_scored() {
        assert_eq!(score(Path::new("/nonexistent/df-root")), None);
    }
}

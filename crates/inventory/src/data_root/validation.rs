//! Phased validation of a resolved root (§4.3).
//!
//! | Phase | Cost | Checks | Failure |
//! |---|---|---|---|
//! | L0 | 2 `stat` | `ARENA2`/`arena2` + `ARCH3D.BSA` | `E-DATA-002` (done by [`crate::data_root::DataRoot::resolve`]) |
//! | L1 | 1 `stat` per mandatory file | 13 core (D11) + 43 images + 5 fonts + `FALL.EXE` | `E-DATA-003` |
//! | L2 | full read (~85 MB) + headers | BSA, `WOODS.WLD` exact, COL/IMG/FNT, typed `TEXT.RSC`, `NAMEGEN.DAT`, `FALL.EXE` item table | `E-DATA-004` naming file and reason |
//! | L3 | sha256 | against `ASSET-REGISTRY.json` | `W-DATA-005`; hard **only** with `--strict-data` (D5) |
//!
//! L2 runs **before the window exists** (the product path calls it, and the M18
//! preflight runs it once per process), so a truncated `WOODS.WLD` is reported
//! as `E-DATA-004`, not as a panic or a black window.
//!
//! Optional files (`DAGGER.SND`, `MIDI.BSA`, `CLIMATE.PAK`, `CLASS18.CFG`) are
//! validated **when present** and never abort: a broken one is `W-DATA-006`.
//! Mods are D6-D10 (another lane), so `W-DATA-007` is not emitted here.

use std::path::{Path, PathBuf};

use crate::data_root::{hex, sha256, DataRoot, DataRootError, Warning};
use crate::items::parse_item_templates;
use crate::retail_manifest::{
    CORE_REQUIRED, FALL_EXE, FONT_FILES, OPTIONAL_WARN, PANEL_ASSETS,
};

/// The contract's phases. L0 happens inside resolution, so it has no verify
/// entry point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase {
    /// Presence only (1 `stat` per mandatory file).
    L1,
    /// Structure: full read plus headers.
    L2,
    /// Identity: sha256 against the reference registry.
    L3,
}

impl Phase {
    /// Label for the `--verify-data` report.
    pub fn label(&self) -> &'static str {
        match self {
            Self::L1 => "L1 presence",
            Self::L2 => "L2 structure",
            Self::L3 => "L3 identity",
        }
    }
}

/// A successful validation up to `phase`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Report {
    /// Phase that was reached.
    pub phase: Option<Phase>,
    /// Files examined (presence checks plus structural reads).
    pub checked: usize,
    /// Non-fatal findings (`W-DATA-00x`), already deduplicated.
    pub warnings: Vec<Warning>,
    /// §4.6 fingerprint of the mandatory set.
    pub identity: Option<[u8; 32]>,
}

/// The full picture, for `--verify-data`: nothing aborts, everything is listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Audit {
    /// Install root that was audited.
    pub root: PathBuf,
    /// Origin label recorded by the resolver.
    pub origin: String,
    /// Mandatory files that are absent (`E-DATA-003`).
    pub missing: Vec<String>,
    /// Mandatory files present but unusable, with the reason (`E-DATA-004`).
    pub invalid: Vec<(String, String)>,
    /// Identity mismatches (`W-DATA-005`, fatal under `--strict-data`).
    pub mismatches: Vec<(String, String)>,
    /// Non-fatal findings.
    pub warnings: Vec<Warning>,
    /// Fingerprint of the mandatory set.
    pub identity: [u8; 32],
    /// Reference registry that was used, when found.
    pub registry: Option<PathBuf>,
    /// Files examined.
    pub checked: usize,
}

impl Audit {
    /// `true` when the installation passes L1+L2 (L3 only matters in strict).
    pub fn passes(&self, strict: bool) -> bool {
        self.missing.is_empty() && self.invalid.is_empty() && (!strict || self.mismatches.is_empty())
    }

    /// Multi-line report for `--verify-data`.
    pub fn render(&self, strict: bool) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "data root: {}\norigin: {}\n",
            self.root.display(),
            self.origin
        ));
        out.push_str(&format!(
            "files checked: {}/{} mandatory\n",
            self.checked,
            mandatory_count()
        ));
        out.push_str(&format!("fingerprint: {}\n", hex(&self.identity)));
        out.push_str(&format!("L1: {}\n", if self.missing.is_empty() { "PASS" } else { "FAIL" }));
        for rel in &self.missing {
            out.push_str(&format!("  E-DATA-003: missing {rel}\n"));
        }
        out.push_str(&format!("L2: {}\n", if self.invalid.is_empty() { "PASS" } else { "FAIL" }));
        for (rel, reason) in &self.invalid {
            out.push_str(&format!("  E-DATA-004: {rel} {reason}\n"));
        }
        out.push_str(&format!(
            "L3: {}\n",
            match (&self.registry, self.mismatches.is_empty()) {
                (None, _) => "SKIPPED (no ASSET-REGISTRY.json)".to_string(),
                (Some(_), true) => "PASS".to_string(),
                (Some(_), false) => format!(
                    "{} ({} mismatches{})",
                    "FAIL",
                    self.mismatches.len(),
                    if strict { "" } else { ", warnings only" }
                ),
            }
        ));
        for (rel, reason) in &self.mismatches {
            out.push_str(&format!("  W-DATA-005: {rel} {reason}\n"));
        }
        for warning in &self.warnings {
            out.push_str(&format!("  {}: {}\n", warning.code, warning.message));
        }
        out.push_str(if self.passes(strict) {
            "RESULT: PASS\n"
        } else {
            "RESULT: FAIL\n"
        });
        out
    }
}

/// Total mandatory files after D11: 13 core + 43 images + 5 fonts + `FALL.EXE`.
pub fn mandatory_count() -> usize {
    CORE_REQUIRED.len() + PANEL_ASSETS.len() + FONT_FILES.len() + 1
}

/// Gate used by the product: runs `phase` and returns the first hard failure.
pub fn verify(root: &DataRoot, phase: Phase) -> Result<Report, DataRootError> {
    let audit = audit(root, phase);
    if let Some(rel) = audit.missing.first() {
        return Err(DataRootError::Missing {
            root: root.base().to_path_buf(),
            rel: rel.clone(),
        });
    }
    if let Some((rel, reason)) = audit.invalid.first() {
        return Err(DataRootError::Invalid {
            root: root.base().to_path_buf(),
            rel: rel.clone(),
            reason: reason.clone(),
        });
    }
    if root.strict() {
        if let Some((rel, reason)) = audit.mismatches.first() {
            return Err(DataRootError::Invalid {
                root: root.base().to_path_buf(),
                rel: rel.clone(),
                reason: reason.clone(),
            });
        }
    }
    Ok(Report {
        phase: Some(phase),
        checked: audit.checked,
        warnings: audit.warnings,
        identity: Some(audit.identity),
    })
}

/// Runs every phase up to `phase`, collecting all findings.
pub fn audit(root: &DataRoot, phase: Phase) -> Audit {
    let mut audit = Audit {
        root: root.base().to_path_buf(),
        origin: root.origin().label(),
        missing: Vec::new(),
        invalid: Vec::new(),
        mismatches: Vec::new(),
        // The root's own warnings (ignored config, dropped remembered root…)
        // are announced once per process by the `mode_a` facade; repeating
        // them here duplicated every line in the `--verify-data` log.
        warnings: Vec::new(),
        identity: [0u8; 32],
        registry: root.registry().map(Path::to_path_buf),
        checked: 0,
    };

    // L1 — presence (§4.3). Order is the manifest order, so the first missing
    // file named in `E-DATA-003` is stable and greppable.
    let mut rels: Vec<String> = CORE_REQUIRED
        .iter()
        .chain(PANEL_ASSETS.iter())
        .chain(FONT_FILES.iter())
        .map(|name| root.rel(name))
        .collect();
    rels.push(FALL_EXE.to_string());
    for rel in &rels {
        if root.resolve_relative(rel).is_ok() {
            audit.checked += 1;
        } else {
            audit.missing.push(rel.clone());
        }
    }
    for name in OPTIONAL_WARN {
        let rel = root.rel(name);
        match root.resolve_relative(&rel) {
            Ok(_) => {}
            Err(_) => audit.warnings.push(Warning {
                code: "W-DATA-006",
                message: optional_disabled(name, &rel),
            }),
        }
    }

    if phase >= Phase::L2 && audit.missing.is_empty() {
        structural(root, &mut audit);
    }

    if phase >= Phase::L3 {
        identity(root, &mut audit);
    }

    audit.identity = root.fingerprint();
    audit
}

fn optional_disabled(name: &str, rel: &str) -> String {
    match name {
        "DAGGER.SND" => format!("audio disabled: {rel} missing."),
        "MIDI.BSA" => format!("music disabled: {rel} missing."),
        "CLIMATE.PAK" => format!("travel climate unavailable: {rel} missing."),
        "CLASS18.CFG" => format!("city watch unavailable: {rel} missing."),
        other => format!("optional file {other} missing ({rel})."),
    }
}

/// The structural catalogue of §2.3. One check per file kind, executed in
/// manifest order so a failure report is deterministic.
enum Check {
    Bsa,
    SoundBank,
    Woods,
    NameGen,
    Texture,
    TextRsc,
    Palette,
    Class74,
    Bio,
    Image,
    Font,
    ItemTable,
    Climate,
}

fn structural(root: &DataRoot, audit: &mut Audit) {
    // ORDER OF ELEMENTS IS THE REPORT ORDER: core first, then panels, fonts,
    // FALL.EXE, then the optional files.
    let mut checks: Vec<(String, Check)> = Vec::new();
    checks.push((root.rel("MAPS.BSA"), Check::Bsa));
    checks.push((root.rel("BLOCKS.BSA"), Check::Bsa));
    checks.push((root.rel("ARCH3D.BSA"), Check::Bsa));
    checks.push((root.rel("WOODS.WLD"), Check::Woods));
    checks.push((root.rel("NAMEGEN.DAT"), Check::NameGen));
    checks.push((root.rel("TEXTURE.182"), Check::Texture));
    checks.push((root.rel("TEXTURE.399"), Check::Texture));
    checks.push((root.rel("TEXT.RSC"), Check::TextRsc));
    checks.push((root.rel("PAL.PAL"), Check::Palette));
    checks.push((root.rel("FMAP_PAL.COL"), Check::Palette));
    checks.push((root.rel("ART_PAL.COL"), Check::Palette));
    checks.push((root.rel("CLASS01.CFG"), Check::Class74));
    checks.push((root.rel("BIO.DAT"), Check::Bio));
    for name in PANEL_ASSETS {
        checks.push((root.rel(name), Check::Image));
    }
    for name in FONT_FILES {
        checks.push((root.rel(name), Check::Font));
    }
    checks.push((FALL_EXE.to_string(), Check::ItemTable));
    for name in OPTIONAL_WARN {
        let rel = root.rel(name);
        let check = match *name {
            "DAGGER.SND" => Check::SoundBank,
            "MIDI.BSA" => Check::Bsa,
            "CLIMATE.PAK" => Check::Climate,
            // `CLASS18.CFG` is the only remaining optional file: 74-byte record.
            _ => Check::Class74,
        };
        checks.push((rel, check));
    }

    for (rel, check) in checks {
        let optional = OPTIONAL_WARN.iter().any(|name| root.rel(name) == rel);
        if optional && root.resolve_relative(&rel).is_err() {
            continue; // reported as W-DATA-006 by L1
        }
        let bytes = match root.read(&rel) {
            Ok(bytes) => bytes,
            Err(err) => {
                if optional {
                    audit.warnings.push(Warning {
                        code: "W-DATA-006",
                        message: format!("{} disabled: {rel} unreadable ({reason}).", check.label(), reason = err.message()),
                    });
                } else {
                    audit
                        .invalid
                        .push((rel, format!("cannot be read ({})", err.message())));
                }
                continue;
            }
        };
        audit.checked += 1;
        match check.run(&bytes) {
            Ok(()) => {}
            Err(reason) => {
                if optional {
                    audit.warnings.push(Warning {
                        code: "W-DATA-006",
                        message: format!(
                            "{} disabled: {rel} fails header ({reason}); continuing.",
                            check.label()
                        ),
                    });
                } else {
                    audit.invalid.push((rel, reason));
                }
            }
        }
    }
}

impl Check {
    fn label(&self) -> &'static str {
        match self {
            Self::Bsa => "archive",
            Self::SoundBank => "audio",
            Self::Woods => "world map",
            Self::NameGen => "name generation",
            Self::Texture => "textures",
            Self::TextRsc => "text resources",
            Self::Palette => "palette",
            Self::Class74 => "class",
            Self::Bio => "biography",
            Self::Image => "UI image",
            Self::Font => "font",
            Self::ItemTable => "item table",
            Self::Climate => "climate",
        }
    }

    fn run(&self, bytes: &[u8]) -> Result<(), String> {
        match self {
            Self::Bsa => daggerfall_parser_rs::bsa::parse(bytes)
                .map(|_| ())
                .map_err(|err| format!("does not parse: {err}")),
            Self::SoundBank => daggerfall_parser_rs::snd::SoundBank::parse(bytes)
                .map(|_| ())
                .map_err(|err| format!("does not parse: {err}")),
            Self::Woods => {
                let expected = daggerfall_parser_rs::woods::OFFICIAL_SIZE;
                if bytes.len() != expected {
                    return Err(format!(
                        "is {} bytes; expected {expected}. {}",
                        bytes.len(),
                        if bytes.len() < expected {
                            "The file is truncated."
                        } else {
                            "The file is longer than the reference size."
                        }
                    ));
                }
                daggerfall_parser_rs::woods::WoodsMap::parse(bytes)
                    .map(|_| ())
                    .map_err(|err| format!("does not parse: {err}"))
            }
            Self::NameGen => daggerfall_parser_rs::namegen::NameGen::parse(bytes)
                .map(|_| ())
                .map_err(|err| format!("does not parse: {err}")),
            Self::Texture => daggerfall_parser_rs::texture::TextureArchive::parse(bytes)
                .map(|_| ())
                .map_err(|err| format!("does not parse: {err}")),
            Self::TextRsc => daggerfall_parser_rs::text_rsc::TextRsc::parse(bytes)
                .map(|_| ())
                .map_err(|err| format!("does not parse: {err}")),
            Self::Palette => daggerfall_parser_rs::texture::Palette::parse(bytes)
                .map(|_| ())
                .map_err(|err| format!("does not parse: {err}")),
            Self::Class74 => {
                // `CLASSnn.CFG` is a fixed 74-byte record (`career.rs:223-226`).
                if bytes.len() == 74 {
                    Ok(())
                } else {
                    Err(format!("is {} bytes; expected 74.", bytes.len()))
                }
            }
            Self::Bio => {
                // `BIO.DAT` parsing is infallible by design (`bio.rs:71`): the
                // structural contract is "it has content".
                if bytes.is_empty() {
                    Err("is empty.".into())
                } else {
                    Ok(())
                }
            }
            Self::Image => daggerfall_parser_rs::image::decode_img_file(bytes)
                .map(|_| ())
                .map_err(|err| format!("does not parse: {err}")),
            Self::Font => daggerfall_parser_rs::image::FntFile::parse(bytes)
                .map(|_| ())
                .map_err(|err| format!("does not parse: {err}")),
            Self::ItemTable => parse_item_templates(bytes)
                .map(|_| ())
                .map_err(|err| format!("does not parse: {err}")),
            Self::Climate => daggerfall_parser_rs::climate::ClimateMap::parse(bytes)
                .map(|_| ())
                .map_err(|err| format!("does not parse: {err}")),
        }
    }
}

/// L3 — identity against `ASSET-REGISTRY.json` (§4.3 D5). Warn by default.
fn identity(root: &DataRoot, audit: &mut Audit) {
    let Some(registry) = root.registry() else {
        audit.warnings.push(Warning {
            code: "W-DATA-009",
            message: "identity not checked: ASSET-REGISTRY.json not found (pass \
                      DF_ASSET_REGISTRY=<path> to compare hashes)."
                .into(),
        });
        return;
    };
    let Ok(text) = std::fs::read_to_string(registry) else {
        audit.warnings.push(Warning {
            code: "W-DATA-009",
            message: format!("identity not checked: {} unreadable.", registry.display()),
        });
        return;
    };
    for (rel, bytes) in mandatory_snapshot(root) {
        let name = rel.rsplit('/').next().unwrap_or(&rel).to_string();
        let Some(expected) = registry_sha256(&text, &name) else {
            continue;
        };
        let actual = hex(&sha256(&bytes));
        if actual != expected.to_ascii_lowercase() {
            audit.mismatches.push((
                rel.clone(),
                format!(
                    "does not match the reference (patched/other version): registry {expected}, \
                     file {actual}. Continuing."
                ),
            ));
        }
    }
}

fn mandatory_snapshot(root: &DataRoot) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    for name in CORE_REQUIRED {
        let rel = root.rel(name);
        if let Ok(bytes) = root.read(&rel) {
            out.push((rel, bytes));
        }
    }
    out
}

/// Minimal scan of the reference registry: `"name": "<file>"` followed by the
/// record's `"sha256": "<hex>"`. The registry is a flat array of records, so
/// the next hash after a name is that record's hash.
fn registry_sha256(text: &str, file: &str) -> Option<String> {
    let needle = format!("\"name\": \"{file}\"");
    let start = text.find(&needle)?;
    let rest = &text[start + needle.len()..];
    let hash_at = rest.find("\"sha256\"")?;
    let after = &rest[hash_at..];
    let colon = after.find(':')?;
    let quoted = after[colon..].find('"')?;
    let value = &after[colon + quoted + 1..];
    let end = value.find('"')?;
    Some(value[..end].to_string())
}

/// `--verify-data` entry point: full audit, never aborts.
pub fn audit_full(root: &DataRoot) -> Audit {
    audit(root, Phase::L3)
}

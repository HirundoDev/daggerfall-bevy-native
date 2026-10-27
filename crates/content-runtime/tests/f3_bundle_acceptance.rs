//! F3 acceptance suite - plan `I1` §9 F3, criteria (d) and invariants r1-r4 of
//! §3.4.
//!
//! Every test is named after the criterion it proves, and every assertion message
//! states the property, not merely the comparison. Fixtures live under
//! `std::env::temp_dir()` with a unique suffix; no test reads the clock, the
//! network or any environment variable besides the temp directory.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use daggerfall_content_api::SideId;
use daggerfall_content_runtime::{
    detect_executable, Bundle, BundleBuilder, BundleError, BundlePath, Manifest, PublishOutcome,
    Version, COMPAT_ENGINE_KEY, ENGINE_PIN, ERROR_CODES, MANIFEST_FILE, MANIFEST_HASH_FILE,
    PAYLOAD_DIR, SCHEMA_VERSION_LABEL,
};

// ---------------------------------------------------------------- fixtures ---

/// A self-deleting unique directory.
struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "dfc1-content-runtime-{label}-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        TempDir(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn side(raw: &str) -> SideId {
    SideId::parse(raw).expect("valid side id")
}

fn v(raw: &str) -> Version {
    Version::parse(raw).expect("valid version")
}

/// Bytes that are plainly data: a tiny PNG header plus opaque bytes.
fn image_bytes() -> Vec<u8> {
    vec![
        0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x01, 0x02,
    ]
}

/// The reference bundle: side `alpha`, version `1.0.0`, core pin `0.1.0`, two
/// data files (12 + 11 bytes).
fn sample_bundle() -> Bundle {
    let mut builder = BundleBuilder::new(side("alpha"), v("1.0.0"), v("0.1.0"));
    builder
        .add_file("alpha/data/items.json", b"{\"items\":[]}".to_vec())
        .expect("a JSON data file is allowed");
    builder
        .add_file("alpha/assets/icon.png", image_bytes())
        .expect("a PNG data file is allowed");
    builder.build()
}

fn bundle_dir(root: &Path) -> PathBuf {
    root.join("alpha").join("1.0.0")
}

fn publish_sample(root: &Path) -> String {
    match sample_bundle().publish(root).expect("publish the sample") {
        PublishOutcome::Published { manifest_sha256 } => manifest_sha256,
        PublishOutcome::AlreadyPublished => panic!("expected a fresh publication"),
    }
}

/// The stable code of an error, i.e. the first token of its `Display` output.
fn code(error: &BundleError) -> String {
    error
        .to_string()
        .split(':')
        .next()
        .unwrap_or_default()
        .to_string()
}

/// Every file under `dir`, keyed by relative path.
fn snapshot(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(dir: &Path, prefix: &str, out: &mut BTreeMap<String, Vec<u8>>) {
        let mut paths: Vec<PathBuf> = fs::read_dir(dir)
            .expect("read dir")
            .map(|entry| entry.expect("dir entry").path())
            .collect();
        paths.sort();
        for path in paths {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .expect("utf-8 name")
                .to_string();
            let relative = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if path.is_dir() {
                walk(&path, &relative, out);
            } else {
                out.insert(relative, fs::read(&path).expect("read file"));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, "", &mut out);
    out
}

// ------------------------------------------------------ (d1) offline verify --

#[test]
fn d1_offline_verification_and_tamper_detection() {
    let root = TempDir::new("d1-roundtrip");
    let published = publish_sample(root.path());
    let dir = bundle_dir(root.path());

    let opened = Bundle::open(&dir).expect("a published bundle opens offline");
    let report = opened.verify().expect("a published bundle verifies offline");
    assert_eq!(
        report.entries, 2,
        "the sample bundle carries exactly two entries"
    );
    assert_eq!(report.bytes, 23, "12 + 11 payload bytes are accounted for");
    assert_eq!(
        report.manifest_sha256, published,
        "the manifest digest survives a publish/open round trip"
    );
    assert_eq!(opened.manifest_sha256(), published);

    let payload_file = dir
        .join(PAYLOAD_DIR)
        .join("alpha")
        .join("data")
        .join("items.json");
    let mut bytes = fs::read(&payload_file).expect("payload file exists");
    bytes[0] ^= 0xFF;
    fs::write(&payload_file, &bytes).expect("tamper the payload");
    let error = Bundle::open(&dir).expect_err("a flipped payload byte must be refused");
    assert_eq!(
        code(&error),
        "entry_hash_mismatch",
        "a flipped payload byte must fail the recorded digest"
    );
    assert!(matches!(error, BundleError::EntryHashMismatch { .. }));

    let root_extra = TempDir::new("d1-extra");
    publish_sample(root_extra.path());
    let dir_extra = bundle_dir(root_extra.path());
    fs::write(
        dir_extra.join(PAYLOAD_DIR).join("alpha").join("extra.bin"),
        b"not in the manifest",
    )
    .expect("write an extra payload file");
    let error = Bundle::open(&dir_extra).expect_err("an undeclared file must be refused");
    assert_eq!(
        code(&error),
        "entry_unexpected",
        "a payload file absent from the manifest is a smuggled entry"
    );
    assert!(matches!(error, BundleError::EntryUnexpected { .. }));

    let root_missing = TempDir::new("d1-missing");
    publish_sample(root_missing.path());
    let dir_missing = bundle_dir(root_missing.path());
    fs::remove_file(
        dir_missing
            .join(PAYLOAD_DIR)
            .join("alpha")
            .join("data")
            .join("items.json"),
    )
    .expect("delete a declared payload file");
    let error = Bundle::open(&dir_missing).expect_err("a missing payload file must be refused");
    assert_eq!(
        code(&error),
        "entry_missing",
        "an entry without a file is an incomplete bundle"
    );
    assert!(matches!(error, BundleError::EntryMissing { .. }));

    let root_manifest = TempDir::new("d1-manifest");
    publish_sample(root_manifest.path());
    let dir_manifest = bundle_dir(root_manifest.path());
    let manifest_path = dir_manifest.join(MANIFEST_FILE);
    let mut text = fs::read_to_string(&manifest_path).expect("read manifest");
    text.push('\n');
    fs::write(&manifest_path, text).expect("tamper the manifest");
    let error = Bundle::open(&dir_manifest).expect_err("a tampered manifest must be refused");
    assert_eq!(
        code(&error),
        "manifest_mismatch",
        "manifest.sha256 must pin the exact manifest bytes"
    );
    assert!(matches!(error, BundleError::ManifestMismatch { .. }));
}

// ----------------------------------------------- (d2) zero executable code --

#[test]
fn d2_zero_executable_code_by_magic_and_extension() {
    let magic_samples: &[(&str, Vec<u8>)] = &[
        (
            "renamed-elf.png",
            vec![0x7F, b'E', b'L', b'F', 0x02, 0x01, 0x01, 0x00],
        ),
        (
            "renamed-macho.png",
            vec![0xCF, 0xFA, 0xED, 0xFE, 0x07, 0x00, 0x00, 0x01],
        ),
        (
            "renamed-wasm.png",
            vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00],
        ),
        (
            "renamed-dos.png",
            vec![b'M', b'Z', 0x90, 0x00, 0x03],
        ),
        ("renamed-script.png", b"#!/bin/sh\necho hi\n".to_vec()),
    ];
    for (name, bytes) in magic_samples {
        let path = format!("alpha/assets/{name}");
        let mut builder = BundleBuilder::new(side("alpha"), v("1.0.0"), v("0.1.0"));
        let error = builder
            .add_file(&path, bytes.clone())
            .expect_err("executable bytes must be refused even under an image name");
        assert_eq!(
            code(&error),
            "executable_payload",
            "`{name}` is an executable payload regardless of its extension"
        );
        match &error {
            BundleError::ExecutablePayload { path: rejected, .. } => {
                assert_eq!(rejected, &path, "the error must name the offending path");
            }
            other => panic!("expected executable_payload, got {other:?}"),
        }
    }

    for name in [
        "tool.exe",
        "lib/thing.so",
        "mod/mod.wasm",
        "TOOL.EXE",
        "s.bin.so",
    ] {
        let path = format!("alpha/{name}");
        let mut builder = BundleBuilder::new(side("alpha"), v("1.0.0"), v("0.1.0"));
        let error = builder
            .add_file(&path, b"harmless looking data".to_vec())
            .expect_err("an executable extension must be refused even with data bytes");
        assert_eq!(
            code(&error),
            "executable_payload",
            "`{name}` declares executable content by extension"
        );
    }

    // `.bin` is data, and the magic table still catches a real binary under it.
    let blob = BundlePath::new(&side("alpha"), "alpha/assets/blob.bin").expect("valid path");
    assert!(
        detect_executable(b"plain data", &blob).is_none(),
        "`.bin` must stay usable for data files"
    );
    assert!(
        detect_executable(&[0x7F, b'E', b'L', b'F'], &blob).is_some(),
        "a real ELF under `.bin` must still be caught by magic"
    );

    let root = TempDir::new("d2-data");
    let published = publish_sample(root.path());
    assert_eq!(
        published.len(),
        64,
        "a data-only bundle publishes and yields a 64-character manifest digest"
    );
}

// ------------------------------------------------------ (r1) immutability --

#[test]
fn r1_immutability_conflict_and_idempotent_republish() {
    let root = TempDir::new("r1");
    let published = publish_sample(root.path());
    let dir = bundle_dir(root.path());
    let before = snapshot(&dir);
    let manifest_mtime = fs::metadata(dir.join(MANIFEST_FILE))
        .expect("manifest metadata")
        .modified()
        .expect("manifest mtime");

    let mut builder = BundleBuilder::new(side("alpha"), v("1.0.0"), v("0.1.0"));
    builder
        .add_file("alpha/data/items.json", b"{\"items\":[1]}".to_vec())
        .expect("valid data");
    builder
        .add_file("alpha/assets/icon.png", b"different bytes".to_vec())
        .expect("valid data");
    let conflicting = builder.build();
    let error = conflicting
        .publish(root.path())
        .expect_err("the same (side, version) must not be rewritten");
    assert_eq!(
        code(&error),
        "version_conflict",
        "publishing different content under a taken (side, version) is a conflict"
    );
    match &error {
        BundleError::VersionConflict {
            existing,
            incoming,
            side: conflict_side,
            version,
        } => {
            assert_eq!(
                existing, &published,
                "the conflict must report the published digest"
            );
            assert_ne!(existing, incoming, "the two digests must differ");
            assert_eq!(conflict_side.as_str(), "alpha");
            assert_eq!(*version, v("1.0.0"));
        }
        other => panic!("expected version_conflict, got {other:?}"),
    }
    assert_eq!(
        snapshot(&dir),
        before,
        "a refused publication must leave every published byte untouched"
    );

    let outcome = sample_bundle()
        .publish(root.path())
        .expect("republishing identical content succeeds");
    assert_eq!(
        outcome,
        PublishOutcome::AlreadyPublished,
        "identical content is idempotent, not an error"
    );
    assert_eq!(
        snapshot(&dir),
        before,
        "an idempotent republication must not rewrite the tree"
    );
    let manifest_mtime_after = fs::metadata(dir.join(MANIFEST_FILE))
        .expect("manifest metadata")
        .modified()
        .expect("manifest mtime");
    assert_eq!(
        manifest_mtime_after, manifest_mtime,
        "an idempotent republication must not touch manifest.json"
    );
}

// ------------------------------------------ (r2) namespacing, 0 collisions --

#[test]
fn r2_namespacing_and_side_isolation() {
    let invalid_paths = [
        "other/data.json",
        "/abs/x",
        "../x",
        "alpha/../x",
        "alpha//x",
        "alpha",
        "alpha/data file.json",
        "alpha/back\\slash.json",
        "",
    ];
    for raw in invalid_paths {
        let mut builder = BundleBuilder::new(side("alpha"), v("1.0.0"), v("0.1.0"));
        let error = builder
            .add_file(raw, b"data".to_vec())
            .expect_err("a path outside the side namespace must be refused");
        assert_eq!(
            code(&error),
            "invalid_path",
            "`{raw}` is not a namespaced relative path"
        );
        assert!(matches!(error, BundleError::InvalidPath { .. }));
    }

    let mut builder = BundleBuilder::new(side("alpha"), v("1.0.0"), v("0.1.0"));
    builder
        .add_file("alpha/data.json", b"one".to_vec())
        .expect("first add");
    let error = builder
        .add_file("alpha/data.json", b"two".to_vec())
        .expect_err("a path may only be declared once");
    assert_eq!(code(&error), "duplicate_path");
    assert!(matches!(error, BundleError::DuplicatePath { .. }));

    let mut wrong_side = BundleBuilder::new(side("beta"), v("2.0.0"), v("0.1.0"));
    let error = wrong_side
        .add_file("alpha/shared/data.json", b"x".to_vec())
        .expect_err("beta must not address alpha's namespace");
    assert_eq!(code(&error), "invalid_path");

    let root = TempDir::new("r2-sides");
    let mut alpha_builder = BundleBuilder::new(side("alpha"), v("1.0.0"), v("0.1.0"));
    alpha_builder
        .add_file("alpha/shared/data.json", b"alpha payload".to_vec())
        .expect("alpha namespaced file");
    let mut beta_builder = BundleBuilder::new(side("beta"), v("1.0.0"), v("0.1.0"));
    beta_builder
        .add_file("beta/shared/data.json", b"beta payload".to_vec())
        .expect("beta namespaced file");

    let alpha_hash = match alpha_builder
        .build()
        .publish(root.path())
        .expect("publish alpha")
    {
        PublishOutcome::Published { manifest_sha256 } => manifest_sha256,
        PublishOutcome::AlreadyPublished => panic!("alpha is a fresh side"),
    };
    let beta_hash = match beta_builder
        .build()
        .publish(root.path())
        .expect("publish beta")
    {
        PublishOutcome::Published { manifest_sha256 } => manifest_sha256,
        PublishOutcome::AlreadyPublished => panic!("beta is a fresh side"),
    };
    assert_ne!(
        alpha_hash, beta_hash,
        "the same relative tail under two sides must produce two distinct bundles"
    );

    let alpha_dir = root.path().join("alpha").join("1.0.0");
    let beta_dir = root.path().join("beta").join("1.0.0");
    assert_eq!(
        fs::read(alpha_dir.join("payload/alpha/shared/data.json")).expect("alpha payload"),
        b"alpha payload".to_vec(),
        "alpha keeps its own file"
    );
    assert_eq!(
        fs::read(beta_dir.join("payload/beta/shared/data.json")).expect("beta payload"),
        b"beta payload".to_vec(),
        "beta keeps its own file"
    );
    assert!(
        Bundle::open(&alpha_dir).is_ok() && Bundle::open(&beta_dir).is_ok(),
        "both sides remain independently verifiable under one root"
    );
}

// --------------------------------------------- (r3) version and core pin ---

#[test]
fn r3_version_semantics_and_exact_core_pin() {
    let bundle = sample_bundle();
    bundle.check_core(&v("0.1.0")).expect("the exact pin matches");
    let error = bundle
        .check_core(&v("0.1.1"))
        .expect_err("there is no range: any other core is refused");
    assert_eq!(code(&error), "incompatible_core");
    match error {
        BundleError::IncompatibleCore { required, actual } => {
            assert_eq!(
                required,
                v("0.1.0"),
                "the error reports what the bundle requires"
            );
            assert_eq!(actual, v("0.1.1"), "the error reports what the caller runs");
        }
        other => panic!("expected incompatible_core, got {other:?}"),
    }

    for raw in [
        "1",
        "1.0",
        "01.0.0",
        "1.0.0-alpha",
        "1..0",
        "1.0.0.0",
        "",
        "1.0.-1",
    ] {
        let error = Version::parse(raw).expect_err("a malformed version must be refused");
        assert_eq!(
            code(&error),
            "invalid_version",
            "`{raw}` is not a strict major.minor.patch version"
        );
        assert!(matches!(error, BundleError::InvalidVersion { .. }));
    }
    assert!(
        Version::parse("4294967296.0.0").is_err(),
        "a component beyond u32 is invalid"
    );

    assert!(
        Version::parse("0.9.10").expect("parse") < Version::parse("0.10.0").expect("parse"),
        "ordering is numeric, not lexicographic"
    );
    for raw in ["0.0.0", "0.1.0", "0.9.10", "1.0.0", "4294967295.0.0"] {
        let parsed = Version::parse(raw).expect("valid version");
        assert_eq!(parsed.to_string(), raw, "Display round-trips `{raw}`");
        assert_eq!(
            Version::parse(&parsed.to_string()).expect("reparse"),
            parsed,
            "parse(Display(v)) == v for `{raw}`"
        );
    }

    let mut builder = BundleBuilder::new(side("alpha"), v("1.0.0"), v("0.2.0"));
    builder
        .add_file("alpha/data.json", b"{}".to_vec())
        .expect("valid data");
    let other_core = builder.build();
    assert_eq!(
        code(&other_core
            .check_core(&v("0.1.0"))
            .expect_err("mismatched pin")),
        "incompatible_core",
        "the pin comes from the bundle, not from a default"
    );
    assert_eq!(
        SCHEMA_VERSION_LABEL, "1.0",
        "the published schema version is pinned"
    );
    assert_eq!(ENGINE_PIN, "0.20.0", "the engine pin is fixed by plan 7.3");
}

// ------------------------------------- (r4) deterministic canonical form ---

#[test]
fn r4_deterministic_canonical_manifest() {
    assert_eq!(
        sample_bundle().manifest_sha256(),
        sample_bundle().manifest_sha256(),
        "two independently built identical bundles hash identically"
    );

    let mut reordered = BundleBuilder::new(side("alpha"), v("1.0.0"), v("0.1.0"));
    reordered
        .add_file("alpha/assets/icon.png", image_bytes())
        .expect("icon");
    reordered
        .add_file("alpha/data/items.json", b"{\"items\":[]}".to_vec())
        .expect("items");
    assert_eq!(
        reordered.build().manifest_sha256(),
        sample_bundle().manifest_sha256(),
        "insertion order must not change the manifest digest"
    );

    let mut builder = BundleBuilder::new(side("alpha"), v("1.0.0"), v("0.1.0"));
    builder
        .add_file("alpha/data.json", b"{}".to_vec())
        .expect("one small file");
    let bundle = builder.build();
    let expected = concat!(
        "{\"bundle_id\":\"alpha@1.0.0\",",
        "\"compat\":{\"bevy\":\"0.20.0\",\"core\":\"0.1.0\"},",
        "\"counts\":{\"bytes\":2,\"entries\":1},",
        "\"entries\":[{\"path\":\"alpha/data.json\",",
        "\"sha256\":\"44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a\",",
        "\"size\":2}],",
        "\"schema_version\":\"1.0\",",
        "\"side\":\"alpha\",",
        "\"version\":\"1.0.0\"}"
    );
    assert_eq!(
        String::from_utf8(bundle.manifest().canonical_bytes()).expect("utf-8 manifest"),
        expected,
        "the canonical manifest is compact JSON with sorted keys and sorted entries"
    );
    assert_eq!(
        bundle.entries().len(),
        1,
        "one entry is recorded for the one file"
    );
    assert_eq!(
        bundle.entries()[0].path.as_str(),
        "alpha/data.json",
        "entries carry their namespaced path"
    );

    let root = TempDir::new("r4-disk");
    publish_sample(root.path());
    let on_disk = fs::read(bundle_dir(root.path()).join(MANIFEST_FILE)).expect("read manifest");
    assert_eq!(
        on_disk,
        sample_bundle().manifest().canonical_bytes(),
        "manifest.json on disk is exactly the canonical form"
    );
    let hash_file = fs::read_to_string(bundle_dir(root.path()).join(MANIFEST_HASH_FILE))
        .expect("read manifest.sha256");
    assert_eq!(
        hash_file,
        format!("{}\n", sample_bundle().manifest_sha256()),
        "manifest.sha256 is the lowercase hex digest plus one newline"
    );

    // The canonical form is checked, not assumed.
    let canonical = bundle.manifest().canonical_bytes();
    let mut spaced = Vec::with_capacity(canonical.len() + 1);
    spaced.push(canonical[0]);
    spaced.push(b' ');
    spaced.extend_from_slice(&canonical[1..]);
    let error = Manifest::parse(&spaced).expect_err("non-canonical bytes must be refused");
    assert_eq!(
        code(&error),
        "manifest_mismatch",
        "a manifest must be byte-identical to its canonical form"
    );
}

// ------------------------------------ manifest validation and error codes ---

#[test]
fn manifest_parse_enforces_schema_side_and_canonical_form() {
    let empty = |schema: &str, side_id: &str, version: &str| {
        format!(
            concat!(
                "{{\"bundle_id\":\"{side}@{version}\",",
                "\"compat\":{{\"{engine}\":\"{pin}\",\"core\":\"0.1.0\"}},",
                "\"counts\":{{\"bytes\":0,\"entries\":0}},",
                "\"entries\":[],",
                "\"schema_version\":\"{schema}\",",
                "\"side\":\"{side}\",",
                "\"version\":\"{version}\"}}"
            ),
            engine = COMPAT_ENGINE_KEY,
            pin = ENGINE_PIN,
            side = side_id,
            schema = schema,
            version = version,
        )
    };

    let parsed = Manifest::parse(empty("1.0", "alpha", "1.0.0").as_bytes())
        .expect("a canonical empty manifest parses");
    assert_eq!(parsed.entry_count(), 0);
    assert_eq!(parsed.byte_count(), 0);
    assert_eq!(parsed.bundle_id(), "alpha@1.0.0");

    let error = Manifest::parse(empty("2.0", "alpha", "1.0.0").as_bytes())
        .expect_err("an unknown schema version must be refused");
    assert_eq!(code(&error), "unsupported_schema");
    assert!(matches!(error, BundleError::UnsupportedSchema { .. }));

    let error = Manifest::parse(empty("1.0", "base", "1.0.0").as_bytes())
        .expect_err("a reserved side id must be refused");
    assert_eq!(
        code(&error),
        "invalid_side",
        "`base` is reserved by the content-api side grammar"
    );

    let error = Manifest::parse(empty("1.0", "alpha", "1.0").as_bytes())
        .expect_err("a malformed version must be refused");
    assert_eq!(code(&error), "invalid_version");
}

#[test]
fn error_codes_are_exact_and_display_prefixed() {
    let expected = [
        "invalid_side",
        "invalid_version",
        "invalid_path",
        "duplicate_path",
        "executable_payload",
        "version_conflict",
        "unsupported_schema",
        "manifest_mismatch",
        "entry_hash_mismatch",
        "entry_missing",
        "entry_unexpected",
        "incompatible_core",
        "io",
    ];
    assert_eq!(
        ERROR_CODES,
        &expected[..],
        "the published code list is closed and ordered"
    );

    let samples = [
        BundleError::InvalidSide {
            raw: "X".to_string(),
            reason: "reason",
        },
        BundleError::InvalidVersion {
            raw: "1".to_string(),
            reason: "reason",
        },
        BundleError::InvalidPath {
            raw: "../x".to_string(),
            reason: "reason",
        },
        BundleError::DuplicatePath {
            path: "alpha/x".to_string(),
        },
        BundleError::ExecutablePayload {
            path: "alpha/x.png".to_string(),
            reason: "reason",
        },
        BundleError::VersionConflict {
            side: side("alpha"),
            version: v("1.0.0"),
            existing: "a".to_string(),
            incoming: "b".to_string(),
        },
        BundleError::UnsupportedSchema {
            declared: "2.0".to_string(),
            supported: SCHEMA_VERSION_LABEL,
        },
        BundleError::ManifestMismatch {
            expected: "a".to_string(),
            actual: "b".to_string(),
        },
        BundleError::EntryHashMismatch {
            path: "alpha/x".to_string(),
            expected: "a".to_string(),
            actual: "b".to_string(),
        },
        BundleError::EntryMissing {
            path: "alpha/x".to_string(),
        },
        BundleError::EntryUnexpected {
            path: "alpha/x".to_string(),
        },
        BundleError::IncompatibleCore {
            required: v("0.1.0"),
            actual: v("0.2.0"),
        },
        BundleError::Io {
            operation: "op",
            message: "message".to_string(),
        },
    ];
    assert_eq!(
        samples.len(),
        ERROR_CODES.len(),
        "every published code has a Display sample"
    );
    for (error, expected_code) in samples.iter().zip(expected) {
        let display = error.to_string();
        assert!(
            display.starts_with(expected_code),
            "`{display}` must start with its stable code `{expected_code}`"
        );
        assert_eq!(code(error), expected_code);
    }
}

// ------------------------- payload root and payload entry are not symlinks ---

/// A tampered bundle cannot point `payload/` somewhere else: `Bundle::open`
/// reads the payload root's own file type (`symlink_metadata`), not the
/// symlink-followed one, so a symlinked root is `io`.
#[test]
fn payload_root_symlink_is_refused() {
    let root = TempDir::new("payload-root-symlink");
    publish_sample(root.path());
    let dir = bundle_dir(root.path());
    let real_payload = dir.join("payload-real");
    fs::rename(dir.join(PAYLOAD_DIR), &real_payload).expect("move the real payload aside");
    std::os::unix::fs::symlink(&real_payload, dir.join(PAYLOAD_DIR))
        .expect("point payload/ at another directory");

    let error = Bundle::open(&dir).expect_err("a symlinked payload root must be refused");
    assert_eq!(
        code(&error),
        "io",
        "a payload root that is not a real directory is an I/O refusal"
    );
    match &error {
        BundleError::Io { operation, message } => {
            assert_eq!(
                *operation, "payload root",
                "the refusal must name the payload root it inspected"
            );
            assert!(
                message.contains("payload"),
                "the refusal must name the path it refused, got `{message}`"
            );
        }
        other => panic!("expected `BundleError::Io` for a symlinked payload root, got {other:?}"),
    }
}

/// The pre-existing rule for entries inside the payload still holds: a symlink
/// where a file should be is `io`, even if it points at bytes that would verify.
#[test]
fn payload_symlink_inside_is_still_refused() {
    let root = TempDir::new("payload-entry-symlink");
    publish_sample(root.path());
    let dir = bundle_dir(root.path());
    let entry = dir
        .join(PAYLOAD_DIR)
        .join("alpha")
        .join("data")
        .join("items.json");
    let outside = TempDir::new("payload-entry-outside");
    let outside_file = outside.path().join("items.json");
    fs::write(&outside_file, b"{\"items\":[]}").expect("write identical bytes outside");
    fs::remove_file(&entry).expect("remove the payload entry");
    std::os::unix::fs::symlink(&outside_file, &entry).expect("link the entry outside the bundle");

    let error = Bundle::open(&dir).expect_err("a symlinked payload entry must be refused");
    assert_eq!(code(&error), "io", "a symlink is not a payload entry");
    assert!(matches!(error, BundleError::Io { .. }));
}

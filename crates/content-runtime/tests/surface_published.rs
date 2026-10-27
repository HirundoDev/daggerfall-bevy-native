//! Anti-drift guard (plan `I1` §9 F3): the published document, the stable error
//! codes, the on-disk layout, the executable-detection table and the crate's own
//! sources are one contract. Editing any side alone breaks this test.

use std::fs;

use daggerfall_content_api::SideId;
use daggerfall_content_runtime::{
    BundleBuilder, Version, COMPAT_ENGINE_KEY, ERROR_CODES, EXECUTABLE_EXTENSIONS, MANIFEST_FILE,
    MANIFEST_HASH_FILE, PAYLOAD_DIR, SCHEMA_VERSION_LABEL,
};

const CONTRACT_MD: &str =
    include_str!("../../../docs/sintergic/content-runtime/CONTENT-RUNTIME-v1.md");

/// Every published code, layout string, rule heading, magic entry and refused
/// extension appears in the normative document, and the document pins the schema
/// version this crate compiles.
#[test]
fn published_document_matches_code_surface() {
    assert_eq!(SCHEMA_VERSION_LABEL, "1.0");
    assert!(
        CONTRACT_MD.contains("schema_version: 1.0"),
        "the normative document must pin schema_version: 1.0"
    );
    assert!(
        CONTRACT_MD.contains("$id: dfc1:content-runtime:v1"),
        "the normative document must pin its $id"
    );

    for code in ERROR_CODES {
        assert!(
            CONTRACT_MD.contains(&format!("`{code}`")),
            "error code `{code}` is missing from CONTENT-RUNTIME-v1.md"
        );
    }

    for needle in [
        MANIFEST_FILE,
        MANIFEST_HASH_FILE,
        PAYLOAD_DIR,
        "<root>/<side>/<version>/manifest.json",
        "<root>/<side>/<version>/manifest.sha256",
        "<root>/<side>/<version>/payload/",
        "## 1. Immutable",
        "## 2. Offline-verifiable",
        "## 3. Namespaced",
        "## 4. Zero executable code",
        "compat.core",
        "0.20.0",
    ] {
        assert!(
            CONTRACT_MD.contains(needle),
            "the normative document must publish `{needle}`"
        );
    }

    for magic in [
        "7F 45 4C 46",
        "FE ED FA CE",
        "FE ED FA CF",
        "CE FA ED FE",
        "CF FA ED FE",
        "CA FE BA BE",
        "BE BA FE CA",
        "CA FE BA BF",
        "4D 5A",
        "00 61 73 6D",
        "23 21",
    ] {
        assert!(
            CONTRACT_MD.contains(magic),
            "the executable-detection table must publish the magic `{magic}`"
        );
    }

    for extension in EXECUTABLE_EXTENSIONS {
        assert!(
            CONTRACT_MD.contains(&format!("`.{extension}`")),
            "the executable-detection table must publish the extension `.{extension}`"
        );
    }
    assert!(
        !EXECUTABLE_EXTENSIONS.contains(&"bin"),
        "`.bin` must stay a legitimate data extension"
    );
}

/// The canonical manifest example in the document is byte-identical to what the
/// crate emits for the same bundle.
#[test]
fn published_manifest_example_is_the_canonical_output() {
    let side = SideId::parse("alpha").expect("valid side");
    let version = Version::parse("1.0.0").expect("valid version");
    let core = Version::parse("0.1.0").expect("valid version");
    let mut builder = BundleBuilder::new(side, version, core);
    builder
        .add_file("alpha/data.json", b"{}".to_vec())
        .expect("a data file");
    let bundle = builder.build();
    let canonical = String::from_utf8(bundle.manifest().canonical_bytes()).expect("utf-8");
    assert!(
        CONTRACT_MD.contains(&canonical),
        "CONTENT-RUNTIME-v1.md must show the canonical manifest byte-for-byte:\n{canonical}"
    );
}

/// Structural evidence for rules 2 and 4: these sources contain no network
/// primitive and no process execution, the crate declares no engine, network or
/// database dependency, and the crate root keeps the lint contract.
///
/// The "zero engine dependency" claim is a property of the build graph, so it
/// is measured in `Cargo.toml`. `manifest.rs` publishes the engine pin under its
/// real key and is therefore allowed to name the engine.
#[test]
fn sources_declare_no_network_or_process_execution() {
    let source_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut checked = 0_usize;
    let mut paths: Vec<_> = fs::read_dir(&source_dir)
        .expect("read src")
        .map(|entry| entry.expect("dir entry").path())
        .collect();
    paths.sort();
    for path in paths {
        if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
            continue;
        }
        let text = fs::read_to_string(&path).expect("read source file");
        for needle in ["std::net", "Tcp", "Udp", "Command::new", "std::process"] {
            assert!(
                !text.contains(needle),
                "{} must not contain `{needle}` (zero network, zero code execution)",
                path.display()
            );
        }
        checked += 1;
    }
    assert!(
        checked >= 7,
        "the source scan must cover every module, found {checked}"
    );

    let lib = fs::read_to_string(source_dir.join("lib.rs")).expect("read lib.rs");
    assert!(
        lib.contains("#![forbid(unsafe_code)]"),
        "the crate root must forbid unsafe code"
    );
    assert!(
        lib.contains("#![warn(missing_docs)]"),
        "the crate root must warn on missing documentation"
    );

    let manifest = include_str!("../Cargo.toml");
    for section in ["[dev-dependencies]", "[build-dependencies]", "[target."] {
        assert!(
            !manifest.contains(section),
            "the bundle contract must not declare `{section}`"
        );
    }
    for dependency in ["sha2", "serde_json", "daggerfall-content-api"] {
        assert!(
            manifest.contains(dependency),
            "the declared dependency `{dependency}` is missing"
        );
    }
    // The "zero engine / zero network / zero database" claim lives in the build
    // graph, so `Cargo.toml` is the artifact that proves it - not a source scan.
    for forbidden in ["bevy", "tokio", "axum", "sqlx", "serde_derive"] {
        assert!(
            !manifest.contains(forbidden),
            "Cargo.toml must not declare `{forbidden}`: this crate owns no engine, runtime, web or database"
        );
    }
}

/// The engine pin is published under its literal key, and the "zero engine"
/// claim is measured where it is actually true: the crate's dependency table.
///
/// `manifest.rs` once assembled the key as `concat!("be", "vy")` so the source
/// scan above would not see the engine's name. That made the code contort
/// around a test and left the real claim - no engine, network or database in
/// the build graph - unmeasured.
#[test]
fn publish_key_is_a_literal_not_an_evasion() {
    assert_eq!(
        COMPAT_ENGINE_KEY, "bevy",
        "the compat key is the published name, verbatim"
    );

    let manifest_source = fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("manifest.rs"),
    )
    .expect("read manifest.rs");
    assert!(
        manifest_source.contains("pub const COMPAT_ENGINE_KEY: &str = \"bevy\";"),
        "manifest.rs must declare the published key as one literal"
    );
    assert!(
        !manifest_source.contains("concat!"),
        "the engine key must not be assembled to hide it from a source scan"
    );

    let mut builder = BundleBuilder::new(
        SideId::parse("alpha").expect("valid side"),
        Version::parse("1.0.0").expect("valid version"),
        Version::parse("0.1.0").expect("valid version"),
    );
    builder
        .add_file("alpha/data.json", b"{}".to_vec())
        .expect("a data file");
    let canonical =
        String::from_utf8(builder.build().manifest().canonical_bytes()).expect("utf-8");
    assert!(
        canonical.contains("\"compat\":{\"bevy\":\"0.20.0\","),
        "the canonical manifest publishes the literal engine key, got:\n{canonical}"
    );
}

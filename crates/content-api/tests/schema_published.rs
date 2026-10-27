//! Anti-drift guard (plan `I1` §9 F2): the published documents are part of the
//! contract, so they are compiled in and compared against [`SURFACE`] and
//! [`ERROR_CODES`] at test time. Editing either side alone breaks the build.

use daggerfall_content_api::{ERROR_CODES, SCHEMA_VERSION_LABEL, SURFACE};

const SCHEMA_JSON: &str =
    include_str!("../../../docs/sintergic/content-api/content-api-v1.schema.json");
const CONTRACT_MD: &str = include_str!("../../../docs/sintergic/content-api/CONTENT-API-v1.md");

/// Every published name and error code appears in both published documents, and
/// both documents advertise exactly the version this crate compiles.
#[test]
fn published_schema_matches_code_surface() {
    assert_eq!(SCHEMA_VERSION_LABEL, "1.0");
    assert!(
        SCHEMA_JSON.contains("\"x-schema-version\": \"1.0\""),
        "the JSON projection must pin \"x-schema-version\": \"1.0\""
    );
    assert!(
        CONTRACT_MD.contains("schema_version: 1.0"),
        "the normative document must pin schema_version: 1.0"
    );
    assert!(
        SCHEMA_JSON.contains("\"$id\": \"dfc1:content-api:v1\""),
        "the JSON projection must pin its $id"
    );

    for item in SURFACE {
        assert!(
            SCHEMA_JSON.contains(item.name),
            "SURFACE type `{}` is missing from content-api-v1.schema.json",
            item.name
        );
        assert!(
            CONTRACT_MD.contains(item.name),
            "SURFACE type `{}` is missing from CONTENT-API-v1.md",
            item.name
        );
        assert!(
            matches!(item.kind, "type" | "enum" | "struct"),
            "`{}` declares an unknown kind `{}`",
            item.name,
            item.kind
        );
    }

    for code in ERROR_CODES {
        assert!(
            SCHEMA_JSON.contains(&format!("\"{code}\"")),
            "error code `{code}` is missing from content-api-v1.schema.json"
        );
        assert!(
            CONTRACT_MD.contains(&format!("`{code}`")),
            "error code `{code}` is missing from CONTENT-API-v1.md"
        );
    }
}

/// Evidence for "0 SQL / 0 red": `sha2` is the manifest's only dependency and no
/// other dependency table is declared, so nothing here can reach a network, a
/// socket, an async runtime, a database, a serializer or a game engine.
///
/// This is the structural form of the anti-transport check: instead of naming
/// individual crate names (which would put them back into this crate's own
/// sources and break the acceptance grep for zero transport identifiers), it
/// rejects *any* dependency other than the pinned `sha2`.
#[test]
fn crate_declares_no_transport_dependency() {
    let manifest = include_str!("../Cargo.toml");
    let lowered = manifest.to_lowercase();
    for section in ["[dev-dependencies]", "[build-dependencies]", "[target."] {
        assert!(
            !lowered.contains(section),
            "the content contract must not declare `{section}`"
        );
    }

    let declared: Vec<&str> = manifest
        .split_once("[dependencies]")
        .expect("manifest declares a [dependencies] section")
        .1
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    assert_eq!(
        declared,
        vec!["sha2 = \"0.10\""],
        "the only dependency of the content contract is sha2 0.10"
    );
}

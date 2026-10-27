//! T1-T8 matrix of the F6-B pack (§9) run against the **real binary**
//! (`std::process::Command` is allowed here, as in the promotion gate's
//! `gate_fixtures.rs`). Every fixture lives in its own temporary directory:
//! the only test that reads the repository is T1, and it is read-only.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

// ---------------------------------------------------------------------------
// harness
// ---------------------------------------------------------------------------

fn run(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_side-contract"))
        .args(args)
        .output()
        .expect("spawn side-contract");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn run_env(args: &[&str], envs: &[(&str, &str)]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_side-contract"))
        .args(args)
        .envs(envs.iter().copied())
        .output()
        .expect("spawn side-contract");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Parses the single canonical row an evaluator must print.
fn row(stdout: &str) -> Value {
    assert!(
        stdout.ends_with('\n') && !stdout.ends_with("\n\n"),
        "stdout debe terminar en exactamente 1 \\n: {stdout:?}"
    );
    let body = &stdout[..stdout.len() - 1];
    assert!(
        !body.contains('\n'),
        "stdout debe ser 1 sola fila: {stdout:?}"
    );
    serde_json::from_str(body).unwrap_or_else(|e| panic!("fila no parsea: {e}: {body}"))
}

/// Runs an evaluator and asserts exit + verdict + cause.
fn assert_case(name: &str, args: &[&str], code: i32, verdict: &str, cause: Option<&str>) -> Value {
    let (got, stdout, stderr) = run(args);
    assert_eq!(got, code, "{name}: exit {got} != {code}; stderr={stderr}");
    assert!(!stdout.is_empty(), "{name}: stdout vacío con fila esperada");
    let v = row(&stdout);
    assert_eq!(v["verdict"], verdict, "{name}: {v}");
    assert_eq!(v["cause"], Value::from(cause), "{name}: {v}");
    v
}

fn p(path: &Path) -> String {
    path.display().to_string()
}

/// Fresh, empty, uniquely named temp directory for one test.
fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sc-matrix-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("tmp dir");
    dir
}

// ---------------------------------------------------------------------------
// fixture: a whole synthetic contract-root (also its own --main-root)
// ---------------------------------------------------------------------------

fn fixture_contract(version: &str) -> String {
    format!(
        r#"schema_version = "1"
contract_version = "{version}"

[identity]
engine_version_source = "Cargo.toml"
bevy_source = "Cargo.toml"
msrv_source = "crates/game-core/Cargo.toml"

[[chapter]]
path = "coverage.md"
class = "chapter"

[[anchor]]
id = "fixture.schema-label"
path = "crates/content-api/src/schema.rs"
symbol = "SCHEMA_VERSION_LABEL"
line = 1
expect = "pub const SCHEMA_VERSION_LABEL: &str = \"1.0\";"

[[anchor]]
id = "fixture.surface"
path = "crates/content-api/src/schema.rs"
symbol = "SURFACE"
line = 3
expect = "pub const SURFACE: &[SurfaceItem] = &["

[exclusions]
globs = ["**/*.BSA", "**/*.VID", "**/*.pal", "**/arena2/**", "**/ARENA2/**"]
policy = "DATA-CLASSIFICATION.md:16-27"

[coverage]
surface = "coverage.md"
error_codes = "coverage.md"
runtime_methods = "coverage.md"
gate_causes = "coverage.md"
"#
    )
}

/// Writes the synthetic main workspace + authored source + `contract.toml`.
fn write_fixture(root: &Path, version: &str) {
    let schema_rs = "pub const SCHEMA_VERSION_LABEL: &str = \"1.0\";\n\npub const SURFACE: &[SurfaceItem] = &[\n    SurfaceItem { name: \"Demo\", kind: \"struct\" },\n];\n";
    fs::create_dir_all(root.join("crates/content-api/src")).unwrap();
    fs::create_dir_all(root.join("crates/game-runtime-api/src")).unwrap();
    fs::create_dir_all(root.join("crates/promotion-gate/src")).unwrap();
    fs::create_dir_all(root.join("crates/game-core")).unwrap();
    fs::create_dir_all(root.join("src/chapters")).unwrap();

    fs::write(
        root.join("Cargo.toml"),
        "[workspace.package]\nversion = \"0.1.0\"\n\n[workspace.dependencies]\nbevy = { version = \"0.20.0\", default-features = false }\n",
    )
    .unwrap();
    fs::write(
        root.join("crates/game-core/Cargo.toml"),
        "[package]\nname = \"game-core\"\nrust-version = \"1.97.1\"\n",
    )
    .unwrap();
    fs::write(root.join("crates/content-api/src/schema.rs"), schema_rs).unwrap();
    fs::write(
        root.join("crates/content-api/src/error.rs"),
        "pub const ERROR_CODES: &[&str] = &[\n    \"demo_error\",\n];\n",
    )
    .unwrap();
    fs::write(
        root.join("crates/game-runtime-api/src/protocol.rs"),
        "pub const STATUS: &str = \"status\";\npub const STOP: &str = \"stop\";\n\npub const ALL: &[&str] = &[\n    STATUS,\n    STOP,\n];\n",
    )
    .unwrap();
    fs::write(
        root.join("crates/promotion-gate/src/taxonomy.rs"),
        "pub const TAXONOMY_VERSION: &str = \"1\";\npub const VERDICT_SCHEMA_VERSION: &str = \"1.0\";\n\npub enum Cause {\n    Alpha,\n    Beta,\n}\n\nimpl Cause {\n    pub const ALL: [Cause; 2] = [\n        Cause::Alpha,\n        Cause::Beta,\n    ];\n\n    pub fn id(self) -> &'static str {\n        match self {\n            Cause::Alpha => \"alpha\",\n            Cause::Beta => \"beta\",\n        }\n    }\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("crates/promotion-gate/src/manifest.rs"),
        "pub const MANIFEST_SCHEMA: &str = \"1.2\";\n",
    )
    .unwrap();
    fs::write(
        root.join("src/README.md"),
        "# fixture\n\nREADME del fixture.\n",
    )
    .unwrap();
    fs::write(
        root.join("src/chapters/coverage.md"),
        "# Cobertura\n\nSuperficies: Demo. Errores: demo_error. Métodos: status, stop. Causas: alpha, beta.\n",
    )
    .unwrap();
    fs::write(root.join("contract.toml"), fixture_contract(version)).unwrap();
}

/// Fresh fixture + successful `--generate`; returns `(root, bundle, sha)`.
fn fresh_bundle(tag: &str) -> (PathBuf, PathBuf, String) {
    let root = tmp(tag);
    let out = root.join("bundle");
    write_fixture(&root, "1.0.0");
    let (code, stdout, stderr) = run(&[
        "--generate",
        "--contract-root",
        &p(&root),
        "--out",
        &p(&out),
        "--main-root",
        &p(&root),
    ]);
    assert_eq!(code, 0, "generate: {stderr}");
    let sha = row(&stdout)["bundle_sha256"]
        .as_str()
        .expect("bundle_sha256")
        .to_string();
    (root, out, sha)
}

/// A side synchronized against a bundle (exit 0), with a readable identity.
fn synced_side(tag: &str, bundle: &Path) -> PathBuf {
    let side = tmp(&format!("{tag}-side"));
    fs::write(
        side.join("side-project.toml"),
        "[side]\nid = \"matrix-side\"\n",
    )
    .unwrap();
    let (code, _, stderr) = run(&["--sync", "--bundle", &p(bundle), "--side", &p(&side)]);
    assert_eq!(code, 0, "sync base: {stderr}");
    side
}

/// Rewrites `CONTRACT.json` through a closure; serde_json re-serialization is
/// canonical (sorted keys), so a surviving failure is a real validator hit.
fn edit_manifest(bundle: &Path, edit: impl FnOnce(&mut Value)) {
    let path = bundle.join("CONTRACT.json");
    let mut value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    edit(&mut value);
    fs::write(&path, serde_json::to_string(&value).unwrap()).unwrap();
}

fn walk_files(dir: &Path, prefix: &str, out: &mut Vec<String>) {
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().into_owned();
        let rel = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if entry.path().is_dir() {
            walk_files(&entry.path(), &rel, out);
        } else {
            out.push(rel);
        }
    }
}

// ---------------------------------------------------------------------------
// T1 — the real contract.toml against the real repository (read-only)
// ---------------------------------------------------------------------------

#[test]
fn t1_real_contract_resolves_36_of_36_anchors() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let text = fs::read_to_string(root.join("docs/sintergic/side-contract/contract.toml"))
        .expect("contract.toml real");
    let cfg = daggerfall_side_contract::contract::ContractCfg::parse(&text)
        .expect("contract.toml real parsea");
    assert_eq!(cfg.schema_version, "1");
    assert_eq!(cfg.contract_version, "2.1.1");
    assert_eq!(cfg.chapters.len(), 7);
    assert_eq!(cfg.anchors.len(), 36);
    assert_eq!(cfg.exclusions.globs.len(), 5);
    let mut seen = std::collections::BTreeSet::new();
    for anchor in &cfg.anchors {
        assert!(
            seen.insert(anchor.id.as_str()),
            "id duplicado {}",
            anchor.id
        );
        let bytes =
            fs::read(root.join(&anchor.path)).unwrap_or_else(|e| panic!("{}: {e}", anchor.path));
        let src = String::from_utf8(bytes).expect("fuente UTF-8");
        let lines: Vec<&str> = src.split('\n').collect();
        let start = (anchor.line - 1) as usize;
        let count = anchor.expect.split('\n').count();
        assert!(
            start + count <= lines.len(),
            "{}: span fuera de rango",
            anchor.id
        );
        assert_eq!(
            lines[start..start + count].join("\n"),
            anchor.expect,
            "anchor {} no casa",
            anchor.id
        );
    }
    // The total is already fixed above (`cfg.anchors.len()`); here we only
    // require the set of resolved ids to cover exactly the declared anchors.
    assert_eq!(seen.len(), cfg.anchors.len());
}

// ---------------------------------------------------------------------------
// T2 — R1-R10: byte rules, paths, no environment, locale, determinism
// ---------------------------------------------------------------------------

fn assert_tampered(name: &str, _root: &Path, bundle: &Path) {
    let v = assert_case(
        name,
        &["--verify", "--bundle", &p(bundle)],
        2,
        "FAIL",
        Some("bundle_tampered"),
    );
    assert_eq!(v["drift_class"], "tamper", "{name}");
}

#[test]
fn t2_r1_cr_is_bundle_tampered() {
    let (root, bundle, _) = fresh_bundle("t2-cr");
    let path = bundle.join("chapters/coverage.md");
    let text = fs::read_to_string(&path).unwrap();
    fs::write(&path, text.replace('\n', "\r\n")).unwrap();
    assert_tampered("r1-cr", &root, &bundle);
}

#[test]
fn t2_r3_bom_is_bundle_tampered() {
    let (root, bundle, _) = fresh_bundle("t2-bom");
    let path = bundle.join("README.md");
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend(fs::read(&path).unwrap());
    fs::write(&path, bytes).unwrap();
    assert_tampered("r3-bom", &root, &bundle);
}

#[test]
fn t2_r2_zero_final_newlines_is_bundle_tampered() {
    let (root, bundle, _) = fresh_bundle("t2-0nl");
    let path = bundle.join("chapters/coverage.md");
    let mut bytes = fs::read(&path).unwrap();
    while bytes.last() == Some(&b'\n') {
        bytes.pop();
    }
    fs::write(&path, bytes).unwrap();
    assert_tampered("r2-0nl", &root, &bundle);
}

#[test]
fn t2_r2_two_final_newlines_is_bundle_tampered() {
    let (root, bundle, _) = fresh_bundle("t2-2nl");
    let path = bundle.join("chapters/coverage.md");
    let mut bytes = fs::read(&path).unwrap();
    bytes.push(b'\n');
    fs::write(&path, bytes).unwrap();
    assert_tampered("r2-2nl", &root, &bundle);
}

#[test]
fn t2_unsorted_files_is_bundle_manifest_invalid() {
    let (_root, bundle, _) = fresh_bundle("t2-order");
    edit_manifest(&bundle, |v| {
        v["files"].as_array_mut().unwrap().reverse();
    });
    assert_case(
        "files-desordenado",
        &["--verify", "--bundle", &p(&bundle)],
        2,
        "FAIL",
        Some("bundle_manifest_invalid"),
    );
}

#[test]
fn t2_absolute_path_is_bundle_manifest_invalid() {
    let (_root, bundle, _) = fresh_bundle("t2-abs");
    edit_manifest(&bundle, |v| {
        v["files"][0]["path"] = json_str("/etc/passwd");
    });
    assert_case(
        "path-absoluto",
        &["--verify", "--bundle", &p(&bundle)],
        2,
        "FAIL",
        Some("bundle_manifest_invalid"),
    );
}

#[test]
fn t2_parent_traversal_path_is_bundle_manifest_invalid() {
    let (_root, bundle, _) = fresh_bundle("t2-dotdot");
    edit_manifest(&bundle, |v| {
        v["files"][1]["path"] = json_str("../escape.md");
    });
    assert_case(
        "path-punto-punto",
        &["--verify", "--bundle", &p(&bundle)],
        2,
        "FAIL",
        Some("bundle_manifest_invalid"),
    );
}

fn json_str(raw: &str) -> Value {
    Value::String(raw.to_string())
}

#[test]
fn t2_bundle_carries_no_timestamps_hosts_or_home_paths() {
    let (_root, bundle, _) = fresh_bundle("t2-env");
    let mut files = Vec::new();
    walk_files(&bundle, "", &mut files);
    assert_eq!(
        files.len(),
        5,
        "README + 1 capítulo + 2 facts + CONTRACT.json"
    );
    let forbidden = [
        "\"ts\"",
        "created_at",
        "timestamp",
        "\"host\"",
        "hostname",
        "\"pid\"",
        "\"user\"",
        "\"cwd\"",
        "/home/",
        "/tmp/",
        "$HOME",
        "$TMPDIR",
    ];
    for rel in &files {
        let text = fs::read_to_string(bundle.join(rel)).unwrap();
        for bad in forbidden {
            assert!(!text.contains(bad), "{rel} contiene {bad:?}");
        }
    }
}

#[test]
fn t2_locale_does_not_change_a_single_byte() {
    let root_a = tmp("t2-locale-a");
    write_fixture(&root_a, "1.0.0");
    let out_a = root_a.join("bundle");
    let root_b = tmp("t2-locale-b");
    write_fixture(&root_b, "1.0.0");
    let out_b = root_b.join("bundle");

    let args_a = [
        "--generate",
        "--contract-root",
        &p(&root_a),
        "--out",
        &p(&out_a),
        "--main-root",
        &p(&root_a),
    ];
    let args_b = [
        "--generate",
        "--contract-root",
        &p(&root_b),
        "--out",
        &p(&out_b),
        "--main-root",
        &p(&root_b),
    ];
    let (code_a, stdout_a, _) = run_env(&args_a, &[("LC_ALL", "C"), ("LANG", "C")]);
    let (code_b, stdout_b, _) = run_env(
        &args_b,
        &[("LC_ALL", "tr_TR.UTF-8"), ("LANG", "tr_TR.UTF-8")],
    );
    assert_eq!((code_a, code_b), (0, 0));
    assert_eq!(stdout_a, stdout_b, "la fila cambia con el locale");
    assert_eq!(
        fs::read(out_a.join("CONTRACT.json")).unwrap(),
        fs::read(out_b.join("CONTRACT.json")).unwrap(),
        "CONTRACT.json cambia con el locale"
    );
}

#[test]
fn t2_two_generations_are_byte_identical_and_verify_determinism_cleans_up() {
    let (root, _bundle, sha_a) = fresh_bundle("t2-det");
    let out_b = root.join("bundle-b");
    let (code, stdout, stderr) = run(&[
        "--generate",
        "--contract-root",
        &p(&root),
        "--out",
        &p(&out_b),
        "--main-root",
        &p(&root),
    ]);
    assert_eq!(code, 0, "{stderr}");
    let row_b = row(&stdout);
    let sha_b = row_b["bundle_sha256"].as_str().unwrap();
    assert_eq!(sha_a, sha_b, "2 corridas dan distinto sha");

    let out_c = root.join("bundle-c");
    let (code, _, stderr) = run(&[
        "--generate",
        "--contract-root",
        &p(&root),
        "--out",
        &p(&out_c),
        "--main-root",
        &p(&root),
        "--verify-determinism",
    ]);
    assert_eq!(code, 0, "--verify-determinism: {stderr}");
    assert!(
        !root.join("bundle-c.verify-2").exists(),
        "verify-2 debió borrarse tras confirmar igualdad"
    );
}

// ---------------------------------------------------------------------------
// T3 — B5 matrix against the real binary: OK + causes + internal_error
// ---------------------------------------------------------------------------

#[test]
fn t3_ok_fixture_generate_sync_check() {
    let (_root, bundle, sha) = fresh_bundle("t3-ok");
    let side = tmp("t3-ok-side");
    fs::write(
        side.join("side-project.toml"),
        "[side]\nid = \"matrix-side\"\n",
    )
    .unwrap();
    assert_case(
        "sync",
        &["--sync", "--bundle", &p(&bundle), "--side", &p(&side)],
        0,
        "OK",
        None,
    );
    let v = assert_case(
        "check",
        &["--check", "--bundle", &p(&bundle), "--side", &p(&side)],
        0,
        "OK",
        None,
    );
    assert_eq!(v["side_id"], "matrix-side");
    assert_eq!(v["lock_bundle_sha256"], json_str(&sha));
    assert_eq!(v["wrote_side"], false);
    assert_eq!(v["files"]["checked"], 4);
    assert!(side.join("contract.lock").exists());
}

#[test]
fn t3_bundle_manifest_invalid_unknown_field() {
    let (_root, bundle, _) = fresh_bundle("t3-manifest");
    edit_manifest(&bundle, |v| {
        v["surprise"] = Value::Bool(true);
    });
    assert_case(
        "manifest-desconocido",
        &["--verify", "--bundle", &p(&bundle)],
        2,
        "FAIL",
        Some("bundle_manifest_invalid"),
    );
}

#[test]
fn t3_bundle_tampered_digest_mismatch() {
    let (_root, bundle, _) = fresh_bundle("t3-tamper");
    let path = bundle.join("chapters/coverage.md");
    let mut bytes = fs::read(&path).unwrap();
    bytes.extend_from_slice(b"X");
    fs::write(&path, bytes).unwrap();
    assert_case(
        "digest-distinto",
        &["--verify", "--bundle", &p(&bundle)],
        2,
        "FAIL",
        Some("bundle_tampered"),
    );
}

#[test]
fn t3_lock_missing_on_unsynchronized_side() {
    let (_root, bundle, _) = fresh_bundle("t3-nolock");
    let side =
        std::env::temp_dir().join(format!("sc-matrix-{}-t3-nolock-absent", std::process::id()));
    let _ = fs::remove_dir_all(&side);
    let v = assert_case(
        "check-sin-lock",
        &["--check", "--bundle", &p(&bundle), "--side", &p(&side)],
        2,
        "DRIFT",
        Some("lock_missing"),
    );
    assert_eq!(v["files"]["checked"], 0);
    assert!(!side.exists(), "check nunca crea el side");
}

#[test]
fn t3_lock_invalid_toml() {
    let (_root, bundle, _) = fresh_bundle("t3-badlock");
    let side = synced_side("t3-badlock", &bundle);
    fs::write(side.join("contract.lock"), "esto no es TOML [[\n").unwrap();
    assert_case(
        "check-lock-ilegible",
        &["--check", "--bundle", &p(&bundle), "--side", &p(&side)],
        2,
        "FAIL",
        Some("lock_invalid"),
    );
}

#[test]
fn t3_lock_ahead_major() {
    let (_root, bundle, _) = fresh_bundle("t3-ahead");
    let side = synced_side("t3-ahead", &bundle);
    let lock = side.join("contract.lock");
    let text = fs::read_to_string(&lock).unwrap();
    fs::write(&lock, text.replace("1.0.0", "9.9.9")).unwrap();
    let v = assert_case(
        "check-ahead",
        &["--check", "--bundle", &p(&bundle), "--side", &p(&side)],
        2,
        "DRIFT",
        Some("lock_ahead"),
    );
    assert_eq!(v["drift_class"], "major");
    assert_eq!(v["blocking"], true);
    // --allow-downgrade suppresses the cause and reports it in detail.
    let v = assert_case(
        "check-ahead-permitido",
        &[
            "--check",
            "--bundle",
            &p(&bundle),
            "--side",
            &p(&side),
            "--allow-downgrade",
        ],
        0,
        "OK",
        None,
    );
    assert!(v["detail"]
        .as_str()
        .unwrap()
        .contains("downgrade permitido"));
}

#[test]
fn t3_lock_stale_patch_is_not_blocking() {
    let (root, bundle_a, _) = fresh_bundle("t3-stale");
    let side = synced_side("t3-stale", &bundle_a);
    // MAIN advances with a patch bump; the side keeps the old lock.
    fs::write(root.join("contract.toml"), fixture_contract("1.0.1")).unwrap();
    let bundle_b = root.join("bundle-b");
    let (code, _, stderr) = run(&[
        "--generate",
        "--contract-root",
        &p(&root),
        "--out",
        &p(&bundle_b),
        "--main-root",
        &p(&root),
    ]);
    assert_eq!(code, 0, "{stderr}");
    let v = assert_case(
        "check-stale",
        &["--check", "--bundle", &p(&bundle_b), "--side", &p(&side)],
        2,
        "DRIFT",
        Some("lock_stale"),
    );
    assert_eq!(v["drift_class"], "patch");
    assert_eq!(v["blocking"], false);
}

#[test]
fn t3_file_missing() {
    let (_root, bundle, _) = fresh_bundle("t3-missing");
    let side = synced_side("t3-missing", &bundle);
    fs::remove_file(side.join(".side-contract/README.md")).unwrap();
    let v = assert_case(
        "check-falta",
        &["--check", "--bundle", &p(&bundle), "--side", &p(&side)],
        2,
        "DRIFT",
        Some("file_missing"),
    );
    assert_eq!(v["files"]["missing"], 1);
}

#[test]
fn t3_file_drift() {
    let (_root, bundle, _) = fresh_bundle("t3-drift");
    let side = synced_side("t3-drift", &bundle);
    let path = side.join(".side-contract/README.md");
    let mut bytes = fs::read(&path).unwrap();
    bytes.extend_from_slice(b"X");
    fs::write(&path, bytes).unwrap();
    let v = assert_case(
        "check-drift",
        &["--check", "--bundle", &p(&bundle), "--side", &p(&side)],
        2,
        "DRIFT",
        Some("file_drift"),
    );
    assert_eq!(v["drift_class"], "tamper");
    assert_eq!(v["blocking"], true);
}

#[test]
fn t3_file_unexpected() {
    let (_root, bundle, _) = fresh_bundle("t3-extra");
    let side = synced_side("t3-extra", &bundle);
    fs::write(side.join(".side-contract/rogue.txt"), "hola\n").unwrap();
    let v = assert_case(
        "check-sobra",
        &["--check", "--bundle", &p(&bundle), "--side", &p(&side)],
        2,
        "DRIFT",
        Some("file_unexpected"),
    );
    assert_eq!(v["files"]["unexpected"], 1);
}

#[test]
fn t3_internal_error_non_utf8_lock_is_exit_4() {
    let (_root, bundle, _) = fresh_bundle("t3-internal");
    let side = synced_side("t3-internal", &bundle);
    let lock = side.join("contract.lock");
    let mut bytes = fs::read(&lock).unwrap();
    bytes.push(0xFF);
    fs::write(&lock, bytes).unwrap();
    let v = assert_case(
        "check-lock-no-utf8",
        &["--check", "--bundle", &p(&bundle), "--side", &p(&side)],
        4,
        "INCOMPLETE",
        Some("internal_error"),
    );
    assert_eq!(v["causes"], json_str_array(&["internal_error"]));
}

fn json_str_array(items: &[&str]) -> Value {
    Value::Array(items.iter().map(|s| json_str(s)).collect())
}

// ---------------------------------------------------------------------------
// T4 — --check is read-only and never creates a missing side
// ---------------------------------------------------------------------------

#[test]
fn t4_check_never_writes_the_side() {
    let (_root, bundle, _) = fresh_bundle("t4-ro");
    let side = synced_side("t4-ro", &bundle);
    let before = daggerfall_side_contract::sideio::snapshot_tree(&side);
    let (code, stdout, _) = run(&["--check", "--bundle", &p(&bundle), "--side", &p(&side)]);
    assert_eq!(code, 0);
    assert_eq!(row(&stdout)["verdict"], "OK");
    let after = daggerfall_side_contract::sideio::snapshot_tree(&side);
    assert_eq!(before, after, "--check modificó el side");
    assert_eq!(
        before.len(),
        7,
        "identidad + lock + 4 copiados + CONTRACT.json"
    );
}

#[test]
fn t4_missing_side_is_lock_missing_and_nothing_is_created() {
    let (_root, bundle, _) = fresh_bundle("t4-absent");
    let side =
        std::env::temp_dir().join(format!("sc-matrix-{}-t4-never-created", std::process::id()));
    let _ = fs::remove_dir_all(&side);
    assert_case(
        "check-side-inexistente",
        &["--check", "--bundle", &p(&bundle), "--side", &p(&side)],
        2,
        "DRIFT",
        Some("lock_missing"),
    );
    assert!(!side.exists(), "--check creó el side");
}

// ---------------------------------------------------------------------------
// T5 — atomicity: an old lock against a new tree can never read OK
// ---------------------------------------------------------------------------

#[test]
fn t5a_new_tree_with_old_lock_never_reads_ok() {
    let (root, bundle_a, _) = fresh_bundle("t5a");
    let side = synced_side("t5a", &bundle_a);
    let old_lock = fs::read(side.join("contract.lock")).unwrap();
    // Sync a newer bundle, then restore the old lock: the "crash before the
    // commit point" state.
    fs::write(root.join("contract.toml"), fixture_contract("1.0.1")).unwrap();
    let bundle_b = root.join("bundle-b");
    let (code, _, stderr) = run(&[
        "--generate",
        "--contract-root",
        &p(&root),
        "--out",
        &p(&bundle_b),
        "--main-root",
        &p(&root),
    ]);
    assert_eq!(code, 0, "{stderr}");
    let (code, _, stderr) = run(&["--sync", "--bundle", &p(&bundle_b), "--side", &p(&side)]);
    assert_eq!(code, 0, "{stderr}");
    fs::write(side.join("contract.lock"), old_lock).unwrap();
    let v = assert_case(
        "check-estado-crash",
        &["--check", "--bundle", &p(&bundle_b), "--side", &p(&side)],
        2,
        "DRIFT",
        Some("lock_stale"),
    );
    assert_eq!(v["cause"], "lock_stale", "{v}");
    assert!(v["causes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c == "file_drift"));
}

#[test]
fn t5b_unwritable_lock_makes_sync_exit_4_and_check_stays_red() {
    use std::os::unix::fs::PermissionsExt;

    let (root, bundle_a, _) = fresh_bundle("t5b");
    let side = synced_side("t5b", &bundle_a);
    let lock = side.join("contract.lock");
    fs::set_permissions(&lock, fs::Permissions::from_mode(0o444)).unwrap();

    fs::write(root.join("contract.toml"), fixture_contract("1.0.1")).unwrap();
    let bundle_b = root.join("bundle-b");
    let (code, _, stderr) = run(&[
        "--generate",
        "--contract-root",
        &p(&root),
        "--out",
        &p(&bundle_b),
        "--main-root",
        &p(&root),
    ]);
    assert_eq!(code, 0, "{stderr}");
    let v = assert_case(
        "sync-lock-solo-lectura",
        &["--sync", "--bundle", &p(&bundle_b), "--side", &p(&side)],
        4,
        "INCOMPLETE",
        Some("internal_error"),
    );
    assert_eq!(v["wrote_side"], true, "el árbol sí fue reemplazado");

    fs::set_permissions(&lock, fs::Permissions::from_mode(0o644)).unwrap();
    let after = assert_case(
        "check-tras-fallo",
        &["--check", "--bundle", &p(&bundle_b), "--side", &p(&side)],
        2,
        "DRIFT",
        Some("lock_stale"),
    );
    assert_ne!(after["verdict"], "OK");
}

// ---------------------------------------------------------------------------
// T5c-T5e — §10.6: `--force` is the escape hatch of the only writer mode
// ---------------------------------------------------------------------------

/// Both paths of `file_drift` (§10.6 step 2): without `--force` the sync
/// aborts with the `--check` row and touches nothing; with `--force` it
/// overwrites the mutated copy and the side reads OK again.
#[test]
fn t5c_sync_force_resolves_file_drift() {
    let (_root, bundle, _) = fresh_bundle("t5c-drift");
    let side = synced_side("t5c-drift", &bundle);
    let path = side.join(".side-contract/README.md");
    let pristine = fs::read(&path).unwrap();
    let mut mutated = pristine.clone();
    mutated.extend_from_slice(b"X");
    fs::write(&path, &mutated).unwrap();

    let v = assert_case(
        "sync-drift-sin-force",
        &["--sync", "--bundle", &p(&bundle), "--side", &p(&side)],
        2,
        "DRIFT",
        Some("file_drift"),
    );
    assert_eq!(v["blocking"], true);
    assert_eq!(v["wrote_side"], false);
    assert_eq!(
        fs::read(&path).unwrap(),
        mutated,
        "sin --force no se destruyen ediciones ajenas"
    );

    let v = assert_case(
        "sync-drift-force",
        &[
            "--sync",
            "--force",
            "--bundle",
            &p(&bundle),
            "--side",
            &p(&side),
        ],
        0,
        "OK",
        None,
    );
    assert_eq!(v["wrote_side"], true);
    assert_eq!(
        fs::read(&path).unwrap(),
        pristine,
        "--force debe restaurar la copia del bundle"
    );
    assert_case(
        "check-tras-force",
        &["--check", "--bundle", &p(&bundle), "--side", &p(&side)],
        0,
        "OK",
        None,
    );
}

/// Both paths of `file_unexpected`: the rogue file blocks a plain sync and
/// disappears whole once `--force` takes the replacement.
#[test]
fn t5d_sync_force_drops_unexpected_files() {
    let (_root, bundle, _) = fresh_bundle("t5c-extra");
    let side = synced_side("t5c-extra", &bundle);
    let rogue = side.join(".side-contract/rogue.txt");
    fs::write(&rogue, "hola\n").unwrap();

    let v = assert_case(
        "sync-extra-sin-force",
        &["--sync", "--bundle", &p(&bundle), "--side", &p(&side)],
        2,
        "DRIFT",
        Some("file_unexpected"),
    );
    assert_eq!(v["wrote_side"], false);
    assert!(rogue.exists(), "sin --force el archivo sigue ahí");

    let v = assert_case(
        "sync-extra-force",
        &[
            "--sync",
            "--force",
            "--bundle",
            &p(&bundle),
            "--side",
            &p(&side),
        ],
        0,
        "OK",
        None,
    );
    assert_eq!(v["wrote_side"], true);
    assert!(!rogue.exists(), "--force reemplaza el árbol completo");
    assert_case(
        "check-tras-force-extra",
        &["--check", "--bundle", &p(&bundle), "--side", &p(&side)],
        0,
        "OK",
        None,
    );
}

/// `--force` never lifts `lock_ahead`: that one is `--allow-downgrade`'s
/// (§10.6 step 2 keeps its own gate per cause).
#[test]
fn t5e_force_does_not_lift_lock_ahead() {
    let (_root, bundle, _) = fresh_bundle("t5c-ahead");
    let side = synced_side("t5c-ahead", &bundle);
    let lock = side.join("contract.lock");
    let text = fs::read_to_string(&lock).unwrap();
    fs::write(&lock, text.replace("1.0.0", "9.9.9")).unwrap();

    assert_case(
        "sync-ahead-force",
        &[
            "--sync",
            "--force",
            "--bundle",
            &p(&bundle),
            "--side",
            &p(&side),
        ],
        2,
        "DRIFT",
        Some("lock_ahead"),
    );
    assert_case(
        "sync-ahead-allow-downgrade",
        &[
            "--sync",
            "--allow-downgrade",
            "--bundle",
            &p(&bundle),
            "--side",
            &p(&side),
        ],
        0,
        "OK",
        None,
    );
}

// ---------------------------------------------------------------------------
// T6 — B9: retail data never enters a bundle nor survives verification
// ---------------------------------------------------------------------------

#[test]
fn t6_generate_rejects_retail_and_verify_rejects_injected_retail() {
    // (a) generate: a retail file in the contract-root aborts the packaging.
    let root = tmp("t6-gen");
    write_fixture(&root, "1.0.0");
    fs::write(root.join("data.BSA"), b"RETAIL").unwrap();
    let out = root.join("bundle");
    let v = assert_case(
        "generate-retail",
        &[
            "--generate",
            "--contract-root",
            &p(&root),
            "--out",
            &p(&out),
            "--main-root",
            &p(&root),
        ],
        2,
        "FAIL",
        Some("bundle_retail_violation"),
    );
    assert!(v["detail"].as_str().unwrap().contains("data.BSA"));
    assert!(!out.exists(), "generate escribió pese al retail");

    // (b) verify: a retail file injected into a valid bundle aborts first.
    let (root2, bundle, _) = fresh_bundle("t6-verify");
    fs::create_dir_all(bundle.join("levels/ARENA2")).unwrap();
    fs::write(bundle.join("levels/ARENA2/map.BSA"), b"RETAIL").unwrap();
    let v = assert_case(
        "verify-retail",
        &["--verify", "--bundle", &p(&bundle)],
        2,
        "FAIL",
        Some("bundle_retail_violation"),
    );
    assert!(v["detail"].as_str().unwrap().contains("map.BSA"));
    let _ = root2;
}

// ---------------------------------------------------------------------------
// T7 — B10: an uncited derived item blocks the generation
// ---------------------------------------------------------------------------

#[test]
fn t7_uncited_surface_item_is_contract_source_drift() {
    let root = tmp("t7");
    write_fixture(&root, "1.0.0");
    let schema = root.join("crates/content-api/src/schema.rs");
    let text = fs::read_to_string(&schema).unwrap();
    fs::write(
        &schema,
        text.replace(
            "];",
            "    SurfaceItem { name: \"Ghost\", kind: \"struct\" },\n];",
        ),
    )
    .unwrap();
    let out = root.join("bundle");
    let v = assert_case(
        "generate-ghost",
        &[
            "--generate",
            "--contract-root",
            &p(&root),
            "--out",
            &p(&out),
            "--main-root",
            &p(&root),
        ],
        2,
        "FAIL",
        Some("contract_source_drift"),
    );
    let detail = v["detail"].as_str().unwrap();
    assert!(
        detail.contains("coverage: sin citar en") && detail.contains("Ghost"),
        "detail sin la lista ordenada: {detail}"
    );
}

// ---------------------------------------------------------------------------
// T8 — the verdict row is byte-reproducible
// ---------------------------------------------------------------------------

#[test]
fn t8_check_row_is_byte_identical_across_runs() {
    let (_root, bundle, _) = fresh_bundle("t8");
    let side = synced_side("t8", &bundle);
    let args = ["--check", "--bundle", &p(&bundle), "--side", &p(&side)];
    let (code_a, stdout_a, _) = run(&args);
    let (code_b, stdout_b, _) = run(&args);
    assert_eq!((code_a, code_b), (0, 0));
    assert_eq!(
        stdout_a, stdout_b,
        "la fila difiere entre corridas idénticas"
    );
}

// ---------------------------------------------------------------------------
// usage surface: exit 1 without a row, help/explain without a row
// ---------------------------------------------------------------------------

#[test]
fn usage_errors_exit_1_with_empty_stdout_and_help_explain_have_no_row() {
    // unknown flag
    let (code, stdout, stderr) = run(&["--verify", "--bundle", "/nonexistent", "--frobnicate"]);
    assert_eq!(code, 1);
    assert!(stdout.is_empty(), "uso: stdout debe quedar vacío");
    assert!(stderr.contains("Uso:"), "uso: falta la usage en stderr");

    // mode absent
    let (code, stdout, _) = run(&[]);
    assert_eq!(code, 1);
    assert!(stdout.is_empty());

    // two modes
    let (code, stdout, _) = run(&["--verify", "--check", "--bundle", "b", "--side", "s"]);
    assert_eq!(code, 1);
    assert!(stdout.is_empty());

    // unknown cause
    let (code, stdout, _) = run(&["--explain", "--cause", "no_existe"]);
    assert_eq!(code, 1);
    assert!(stdout.is_empty());

    // --help: usage on stdout, exit 0, no row
    let (code, stdout, stderr) = run(&["--help"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("Uso:"));
    assert!(stderr.is_empty());
    assert!(!stdout.contains("\"schema\""), "help sin fila");

    // --explain: text on stdout, exit 0, no row
    let (code, stdout, _) = run(&["--explain", "--cause", "lock_stale"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("cause: lock_stale"));
    assert!(stdout.contains("Qué hacer:"));
    assert!(!stdout.contains("\"schema\""), "explain sin fila");

    // every exit stays inside 0..=4 and 3 is never emitted
    let (code, _, _) = run(&["--verify", "--bundle", "/definitely/missing"]);
    assert_eq!(code, 2, "bundle inexistente => FAIL");
}

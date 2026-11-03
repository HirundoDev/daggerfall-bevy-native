//! Tests of the single resolver. Everything runs on synthetic installations
//! built with `HOME` injection: the retail tree is never touched, and the
//! auto-detection rule is measured against the same candidate list the real
//! machine uses.

use super::*;
use crate::retail_manifest::{CORE_REQUIRED, FONT_FILES, PANEL_ASSETS};

struct Scratch {
    dir: PathBuf,
}

impl Scratch {
    fn new(tag: &str) -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after the Unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "df-dataroot-{tag}-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        Self { dir }
    }

    fn path(&self) -> &Path {
        &self.dir
    }

    fn env(&self) -> Env {
        Env {
            home: Some(self.dir.join("home")),
            xdg_config_home: Some(self.dir.join("config")),
            xdg_state_home: Some(self.dir.join("state")),
            ..Env::default()
        }
    }

    /// A complete-looking installation: `ARENA2` (either spelling) with every
    /// mandatory file plus `FALL.EXE`. Contents are dummies — L1 is presence.
    fn install(&self, root: &Path, arena2: &str) -> PathBuf {
        let dir = root.join(arena2);
        std::fs::create_dir_all(&dir).expect("create ARENA2");
        for name in CORE_REQUIRED {
            std::fs::write(dir.join(name), b"x").expect("write core");
        }
        for name in PANEL_ASSETS {
            std::fs::write(dir.join(name), b"x").expect("write panel");
        }
        for name in FONT_FILES {
            std::fs::write(dir.join(name), b"x").expect("write font");
        }
        std::fs::write(root.join(FALL_EXE), b"x").expect("write FALL.EXE");
        root.to_path_buf()
    }

    /// A partial installation: L0 passes, the score does not reach the ceiling.
    fn partial_install(&self, root: &Path) -> PathBuf {
        let dir = root.join("ARENA2");
        std::fs::create_dir_all(&dir).expect("create ARENA2");
        std::fs::write(dir.join("ARCH3D.BSA"), b"x").expect("write ARCH3D");
        std::fs::write(dir.join("MAPS.BSA"), b"x").expect("write MAPS");
        root.to_path_buf()
    }

    fn write_config(&self, env: &Env, body: &str) -> PathBuf {
        let path = env.config_path().expect("config path");
        std::fs::create_dir_all(path.parent().unwrap()).expect("create config dir");
        std::fs::write(&path, body).expect("write config");
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn options(env: Env) -> ResolveOptions {
    ResolveOptions {
        explicit: None,
        strict: false,
        env,
    }
}

/// §4.1 rank 1: the CLI wins over every other rank.
#[test]
fn cli_wins_over_config_env_and_detection() {
    let scratch = Scratch::new("precedence");
    let cli_root = scratch.install(&scratch.path().join("cli/app"), "ARENA2");
    let config_root = scratch.install(&scratch.path().join("config-copy"), "ARENA2");
    let env_root = scratch.install(&scratch.path().join("env-copy"), "ARENA2");
    let home = scratch.path().join("home");
    std::fs::create_dir_all(home.join(".steam/steam/steamapps/common")).expect("home");
    let detected = scratch.install(
        &home.join(".steam/steam/steamapps/common/Daggerfall"),
        "ARENA2",
    );
    let mut env = scratch.env();
    env.home = Some(home);
    env.app_root = Some(env_root.clone());
    scratch.write_config(
        &env,
        &format!("schema = 1\n[data]\nroot = \"{}\"\n", config_root.display()),
    );
    let root = DataRoot::resolve_with(&ResolveOptions {
        explicit: Some(cli_root.clone()),
        strict: false,
        env,
    })
    .expect("resolves");
    assert_eq!(root.base(), cli_root.as_path());
    assert_eq!(root.origin(), &Origin::Cli);
    assert_ne!(root.base(), detected.as_path());
}

/// §4.1 rank 1 failure is fatal: the next rank is **not** tried even when it
/// would resolve (falsifier `a`).
#[test]
fn invalid_explicit_root_never_falls_through() {
    let scratch = Scratch::new("no-fallthrough");
    let bad = scratch.path().join("not-a-game");
    std::fs::create_dir_all(&bad).expect("create bad root");
    let good = scratch.install(&scratch.path().join("good"), "ARENA2");
    let mut env = scratch.env();
    env.app_root = Some(good);
    let error = DataRoot::resolve_with(&ResolveOptions {
        explicit: Some(bad.clone()),
        strict: false,
        env,
    })
    .expect_err("an invalid explicit root is fatal");
    assert_eq!(error.code(), "E-DATA-002");
    assert!(matches!(error, DataRootError::RootRejected { origin: Origin::Cli, .. }));
    let message = error.player_message();
    assert!(message.starts_with("E-DATA-002: --data-root="), "{message}");
    assert!(message.contains("not-a-game"), "{message}");
    assert!(message.contains("ARENA2/ARCH3D.BSA"), "{message}");
}

/// §4.1 normalization: `<root>/ARENA2` is accepted and the parent is used.
#[test]
fn arena2_basename_is_normalized_to_parent() {
    let scratch = Scratch::new("normalize");
    let root = scratch.install(&scratch.path().join("game/app"), "ARENA2");
    let root = DataRoot::resolve_with(&ResolveOptions {
        explicit: Some(root.join("ARENA2")),
        strict: false,
        env: scratch.env(),
    })
    .expect("resolves from the ARENA2 child");
    assert_eq!(root.base(), scratch.path().join("game/app").as_path());
    assert_eq!(root.arena2_dir().file_name().unwrap(), "ARENA2");

    let lower = scratch.install(&scratch.path().join("game/lower"), "arena2");
    let root = DataRoot::resolve_with(&ResolveOptions {
        explicit: Some(lower.join("arena2")),
        strict: false,
        env: scratch.env(),
    })
    .expect("resolves from a lowercase child");
    assert_eq!(root.base(), scratch.path().join("game/lower").as_path());
    assert_eq!(root.rel("MAPS.BSA"), "arena2/MAPS.BSA");
    assert!(root.resolve_relative("ARENA2/MAPS.BSA").is_ok(), "spelling does not matter");
}

/// §4.1 rank 2: the remembered root is used when the CLI is silent.
#[test]
fn config_root_is_used_when_cli_is_silent() {
    let scratch = Scratch::new("config");
    let remembered = scratch.install(&scratch.path().join("remembered"), "ARENA2");
    let env = scratch.env();
    scratch.write_config(
        &env,
        &format!(
            "schema = 1\n[data]\nroot = \"{}\"\nstrict = false\n",
            remembered.display()
        ),
    );
    let root = DataRoot::resolve_with(&options(env)).expect("resolves from config");
    assert_eq!(root.base(), remembered.as_path());
    assert_eq!(root.origin(), &Origin::Config);
}

/// §4.1 rank 3: `DF_APP_ROOT` is honoured and its failure is fatal.
#[test]
fn app_root_env_is_honoured_and_fatal_when_invalid() {
    let scratch = Scratch::new("app-root");
    let root = scratch.install(&scratch.path().join("env"), "ARENA2");
    let mut env = scratch.env();
    env.app_root = Some(root.clone());
    let resolved = DataRoot::resolve_with(&options(env.clone())).expect("resolves");
    assert_eq!(resolved.base(), root.as_path());
    assert_eq!(resolved.origin(), &Origin::EnvAppRoot);

    env.app_root = Some(scratch.path().join("missing"));
    let error = DataRoot::resolve_with(&options(env)).expect_err("fatal");
    assert_eq!(error.code(), "E-DATA-002");
    assert!(error.player_message().contains("DF_APP_ROOT"));
}

/// §4.1 rank 4: the legacy aliases still work, normalized and deprecated.
#[test]
fn legacy_env_is_a_deprecated_alias_of_rank_four() {
    let scratch = Scratch::new("legacy");
    let root = scratch.install(&scratch.path().join("legacy"), "ARENA2");
    let mut env = scratch.env();
    env.legacy_arena2 = vec![("DF_ARENA2".into(), root.join("ARENA2"))];
    let resolved = DataRoot::resolve_with(&options(env)).expect("resolves");
    assert_eq!(resolved.base(), root.as_path());
    assert!(matches!(resolved.origin(), Origin::EnvLegacy(name) if name == "DF_ARENA2"));
    assert!(
        resolved
            .warnings()
            .iter()
            .any(|warning| warning.code == "W-DATA-008"),
        "the deprecation warning must be surfaced"
    );
}

/// §4.2: one valid candidate is chosen.
#[test]
fn auto_detection_picks_the_only_valid_candidate() {
    let scratch = Scratch::new("detect-one");
    let env = scratch.env();
    std::fs::create_dir_all(env.home.as_ref().unwrap().join("Games/Daggerfall")).expect("home");
    let install = scratch.install(
        &env.home.as_ref().unwrap().join("Games/Daggerfall/app"),
        "ARENA2",
    );
    let root = DataRoot::resolve_with(&options(env)).expect("resolves");
    assert_eq!(root.base(), install.as_path());
    assert_eq!(root.origin(), &Origin::AutoDetected);
}

/// §4.2: a tie lists the candidates and refuses (falsifier `b`).
#[test]
fn auto_detection_refuses_to_choose_between_valid_candidates() {
    let scratch = Scratch::new("detect-two");
    let env = scratch.env();
    let home = env.home.clone().unwrap();
    let steam = home.join(".steam/steam/steamapps/common/Daggerfall");
    let gog = home.join("GOG Games/Daggerfall");
    scratch.install(&steam, "ARENA2");
    scratch.install(&gog, "ARENA2");
    let error = DataRoot::resolve_with(&options(env)).expect_err("ambiguous");
    assert_eq!(error.code(), "E-DATA-001");
    assert!(matches!(error, DataRootError::Ambiguous { .. }));
    let message = error.player_message();
    assert!(message.contains("refusing to guess"), "{message}");
    assert!(message.contains(&steam.display().to_string()), "{message}");
    assert!(message.contains(&gog.display().to_string()), "{message}");
    assert!(message.contains("Pass --data-root"), "{message}");
    assert_eq!(error.candidates().len(), 2);
}

/// §4.2: nothing to find lists what was searched.
#[test]
fn auto_detection_without_candidates_lists_the_search() {
    let scratch = Scratch::new("detect-none");
    let env = scratch.env();
    let error = DataRoot::resolve_with(&options(env.clone())).expect_err("nothing found");
    assert_eq!(error.code(), "E-DATA-001");
    assert!(matches!(error, DataRootError::NoDataRoot { .. }));
    let message = error.player_message();
    assert!(message.contains("no Daggerfall data folder found"), "{message}");
    assert!(message.contains("1. "), "numbered list: {message}");
    assert!(
        message.contains(&env.historic_default().unwrap().display().to_string()),
        "the historic default is one of the listed candidates: {message}"
    );
}

/// §4.2: a candidate below the score ceiling is listed, never chosen.
#[test]
fn auto_detection_lists_a_below_threshold_candidate() {
    let scratch = Scratch::new("detect-weak");
    let env = scratch.env();
    let weak = scratch.partial_install(&env.home.as_ref().unwrap().join("dosbox/daggerfall"));
    let error = DataRoot::resolve_with(&options(env)).expect_err("below threshold");
    assert_eq!(error.code(), "E-DATA-001");
    match &error {
        DataRootError::BelowThreshold { best, .. } => assert!(*best < MAX_SCORE),
        other => panic!("expected BelowThreshold, got {other:?}"),
    }
    let message = error.player_message();
    assert!(message.contains(&weak.display().to_string()), "{message}");
    assert!(message.contains(&format!("/{MAX_SCORE}")), "{message}");
}

/// A candidate whose data directory is a real `ARENA2` child is detected, and
/// a `Daggerfall/app` layout (the historic default) is one more candidate.
#[test]
fn candidate_with_arena2_child_is_detected() {
    let scratch = Scratch::new("detect-child");
    let env = scratch.env();
    let home = env.home.clone().unwrap();
    std::fs::create_dir_all(home.join("GOG Games")).expect("home");
    let gog = scratch.install(&home.join("GOG Games/Daggerfall"), "ARENA2");
    let root = DataRoot::resolve_with(&options(env)).expect("resolves");
    assert_eq!(root.base(), gog.as_path());
}

/// §4.5: a corrupt config is ignored with a warning, left untouched, and the
/// next rank decides (falsifier `c`).
#[test]
fn corrupt_config_is_ignored_and_not_rewritten() {
    let scratch = Scratch::new("config-corrupt");
    let env = scratch.env();
    let root = scratch.install(&env.home.as_ref().unwrap().join("dosbox/daggerfall"), "ARENA2");
    let body = "schema = 1\n[data\nroot = \"/nope\"\n";
    let path = scratch.write_config(&env, body);
    let resolved = DataRoot::resolve_with(&options(env)).expect("resolves via detection");
    assert_eq!(resolved.base(), root.as_path());
    assert_eq!(resolved.origin(), &Origin::AutoDetected);
    assert!(
        resolved
            .warnings()
            .iter()
            .any(|warning| warning.code == "W-DATA-001"),
        "{:?}",
        resolved.warnings()
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), body, "never rewritten");
}

/// §4.5: an unknown schema is ignored with a warning and not rewritten.
#[test]
fn unknown_schema_is_ignored_and_not_rewritten() {
    let scratch = Scratch::new("schema");
    let env = scratch.env();
    let root = scratch.install(&env.home.as_ref().unwrap().join("dosbox/daggerfall"), "ARENA2");
    let body = "schema = 99\n[data]\nroot = \"/nowhere\"\n";
    let path = scratch.write_config(&env, body);
    let resolved = DataRoot::resolve_with(&options(env)).expect("resolves via detection");
    assert_eq!(resolved.base(), root.as_path());
    assert!(resolved.warnings().iter().any(|w| w.code == "W-DATA-001"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), body);
}

/// Falsifier `c`, hardest half: a corrupt config with **nothing** else to
/// resolve. The warning must travel with the error — announcing it only on the
/// happy path explains nothing to the player who is about to be told that no
/// data folder was found.
#[test]
fn corrupt_config_warning_travels_with_the_failure() {
    let scratch = Scratch::new("config-corrupt-nothing");
    let env = scratch.env();
    let body = "schema = [[[\n";
    let path = scratch.write_config(&env, body);
    let error = DataRoot::resolve_with(&options(env)).expect_err("nothing to resolve");
    assert_eq!(error.code(), "E-DATA-001");
    let message = error.player_message();
    assert!(message.contains("W-DATA-001"), "{message}");
    assert!(message.contains("E-DATA-001"), "{message}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), body, "never rewritten");
}

/// A remembered root that rotted is ignored with `W-DATA-002`, the file is not
/// rewritten, and the next rank decides (falsifier `d`).
#[test]
fn stale_remembered_root_is_ignored_and_kept_as_is() {
    let scratch = Scratch::new("stale");
    let env = scratch.env();
    let detected = scratch.install(&env.home.as_ref().unwrap().join("dosbox/daggerfall"), "ARENA2");
    let stale = scratch.path().join("unplugged-drive/Daggerfall/app");
    let body = format!("schema = 1\n[data]\nroot = \"{}\"\n", stale.display());
    let path = scratch.write_config(&env, &body);
    let resolved = DataRoot::resolve_with(&options(env)).expect("resolves via detection");
    assert_eq!(resolved.base(), detected.as_path());
    assert_eq!(resolved.origin(), &Origin::AutoDetected);
    let warning = resolved
        .warnings()
        .iter()
        .find(|warning| warning.code == "W-DATA-002")
        .expect("W-DATA-002 for the stale root");
    assert!(warning.message.contains("unplugged-drive"), "{warning:?}");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        body,
        "a stale remembered root is never rewritten by a failed resolution"
    );
}

/// A stale remembered root is mentioned in `E-DATA-001` when nothing else
/// resolves, so the player learns why the folder they chose is gone.
#[test]
fn stale_remembered_root_is_reported_when_nothing_resolves() {
    let scratch = Scratch::new("stale-none");
    let env = scratch.env();
    let stale = scratch.path().join("gone/Daggerfall/app");
    scratch.write_config(
        &env,
        &format!("schema = 1\n[data]\nroot = \"{}\"\n", stale.display()),
    );
    let error = DataRoot::resolve_with(&options(env)).expect_err("nothing found");
    let message = error.player_message();
    assert!(message.contains("(remembered)"), "{message}");
    assert!(message.contains(&stale.display().to_string()), "{message}");
}

/// Persistence only happens when asked, and only for a root that validated.
#[test]
fn remember_writes_an_atomic_config() {
    let scratch = Scratch::new("remember");
    let env = scratch.env();
    let root = scratch.install(&scratch.path().join("game/app"), "ARENA2");
    let resolved = DataRoot::resolve_with(&ResolveOptions {
        explicit: Some(root.clone()),
        strict: true,
        env: env.clone(),
    })
    .expect("resolves");
    let path = resolved.remember(&env).expect("remembers");
    let state = config_file::load(&path);
    let file = state.loaded().expect("config loads");
    assert_eq!(file.root.as_deref(), Some(root.as_path()));
    assert_eq!(file.strict, Some(true));
    let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
        .collect();
    assert!(leftovers.is_empty(), "no temporary file may survive");
}

/// `require` is L1 over `ARENA2`-relative names and reports the first gap.
#[test]
fn require_reports_the_first_missing_mandatory_file() {
    let scratch = Scratch::new("require");
    let root = scratch.install(&scratch.path().join("game/app"), "ARENA2");
    std::fs::remove_file(root.join("ARENA2/MAPS.BSA")).expect("remove MAPS");
    let resolved = DataRoot::resolve_with(&ResolveOptions {
        explicit: Some(root),
        strict: false,
        env: scratch.env(),
    })
    .expect("L0 still passes");
    let error = resolved.require(&["MAPS.BSA"]).expect_err("incomplete");
    assert_eq!(error.code(), "E-DATA-003");
    assert!(error.player_message().contains("missing ARENA2/MAPS.BSA."));
}

/// L1 passes on a complete synthetic tree; L2 runs and rejects dummy content,
/// which proves L2 is executed (not skipped) on presence-complete trees.
#[test]
fn l1_passes_and_l2_is_executed_on_a_complete_tree() {
    let scratch = Scratch::new("phases");
    let root = scratch.install(&scratch.path().join("game/app"), "ARENA2");
    let resolved = DataRoot::resolve_with(&ResolveOptions {
        explicit: Some(root),
        strict: false,
        env: scratch.env(),
    })
    .expect("resolves");
    let report = resolved.verify(Phase::L1).expect("L1 passes");
    assert_eq!(report.checked, crate::data_root::validation::mandatory_count());
    assert_eq!(report.phase, Some(Phase::L1));
    let error = resolved.verify(Phase::L2).expect_err("dummy content fails L2");
    assert_eq!(error.code(), "E-DATA-004");
    assert!(matches!(error, DataRootError::Invalid { .. }));
}

/// `open` reports provenance: the absolute path and the digest of the bytes.
#[test]
fn open_reports_layer_path_and_digest() {
    let scratch = Scratch::new("open");
    let root = scratch.install(&scratch.path().join("game/app"), "ARENA2");
    std::fs::write(root.join("ARENA2/MAPS.BSA"), b"retail-ish").expect("write");
    let resolved = DataRoot::resolve_with(&ResolveOptions {
        explicit: Some(root.join("ARENA2")),
        strict: false,
        env: scratch.env(),
    })
    .expect("resolves");
    let (bytes, provenance) = resolved.open("ARENA2/maps.bsa").expect("opens");
    assert_eq!(bytes, b"retail-ish");
    assert_eq!(provenance.layer, LayerId::Base);
    assert_eq!(provenance.path, resolved.arena2_dir().join("MAPS.BSA"));
    assert_eq!(provenance.sha256, sha256(b"retail-ish"));
    assert_eq!(provenance.sha256_hex().len(), 64);
}

/// The fingerprint is deterministic and content-sensitive (§4.6).
#[test]
fn fingerprint_is_stable_and_content_sensitive() {
    let scratch = Scratch::new("fingerprint");
    let root = scratch.install(&scratch.path().join("game/app"), "ARENA2");
    let resolved = DataRoot::resolve_with(&ResolveOptions {
        explicit: Some(root.clone()),
        strict: false,
        env: scratch.env(),
    })
    .expect("resolves");
    let first = resolved.fingerprint();
    assert_eq!(first, resolved.fingerprint(), "cached and deterministic");
    let again = DataRoot::resolve_with(&ResolveOptions {
        explicit: Some(root.clone()),
        strict: false,
        env: scratch.env(),
    })
    .expect("resolves");
    assert_eq!(first, again.fingerprint(), "stable across instances");
    std::fs::write(root.join("ARENA2/MAPS.BSA"), b"y").expect("mutate");
    let changed = DataRoot::resolve_with(&ResolveOptions {
        explicit: Some(root),
        strict: false,
        env: scratch.env(),
    })
    .expect("resolves");
    assert_ne!(first, changed.fingerprint());
}

/// The resolver never leaves the installation.
#[test]
fn relative_paths_cannot_escape_the_root() {
    let scratch = Scratch::new("escape");
    let root = scratch.install(&scratch.path().join("game/app"), "ARENA2");
    let resolved = DataRoot::from_arena2(&root.join("ARENA2")).expect("builds");
    assert!(resolved.resolve_relative("../MAPS.BSA").is_err());
    assert!(resolved.resolve_relative("ARENA2/../FALL.EXE").is_err());
    assert!(resolved.resolve_relative("/etc/passwd").is_err());
    assert_eq!(resolved.base(), root.as_path());
    assert!(resolved.resolve_relative("FALL.EXE").is_ok());
}

/// L3 compares hashes against the reference registry, warning by default and
/// failing only under `--strict-data` (D5).
#[test]
fn l3_warns_by_default_and_fails_when_strict() {
    let scratch = Scratch::new("l3");
    let root = scratch.install(&scratch.path().join("game/app"), "ARENA2");
    let registry = scratch.path().join("ASSET-REGISTRY.json");
    let good = hex(&sha256(b"x"));
    std::fs::write(
        &registry,
        format!(
            "{{\"assets\": [{{\"name\": \"MAPS.BSA\", \"sha256\": \"{good}\"}}, \
             {{\"name\": \"WOODS.WLD\", \"sha256\": \"{}\"}}]}}",
            "0".repeat(64)
        ),
    )
    .expect("write registry");
    let mut env = scratch.env();
    env.asset_registry = Some(registry);
    let lenient = DataRoot::resolve_with(&ResolveOptions {
        explicit: Some(root.clone()),
        strict: false,
        env: env.clone(),
    })
    .expect("resolves");
    let audit = validation::audit_full(&lenient);
    assert_eq!(audit.mismatches.len(), 1, "only WOODS.WLD differs");
    assert!(audit.mismatches[0].0.ends_with("WOODS.WLD"));
    assert_eq!(audit.registry.as_deref(), env.asset_registry.as_deref());
    // The structural failures of this synthetic tree are the reason `passes`
    // is false here; identity is measured separately: a mismatch alone only
    // matters under `--strict-data`.
    let mut identity_only = audit.clone();
    identity_only.missing.clear();
    identity_only.invalid.clear();
    assert!(identity_only.passes(false), "a mismatch is a warning by default");
    assert!(!identity_only.passes(true));

    let strict = DataRoot::resolve_with(&ResolveOptions {
        explicit: Some(root),
        strict: true,
        env,
    })
    .expect("resolves");
    // L2 fails first on the dummy bytes; the strict identity error is only
    // reachable on a structurally valid tree, which this fixture is not.
    assert_eq!(strict.verify(Phase::L2).unwrap_err().code(), "E-DATA-004");
    assert!(strict.strict());
    assert!(audit.render(false).contains("W-DATA-005"));
    assert!(audit.render(false).contains("warnings only"));
}

//! `side-contract` binary — a deliberately thin CLI over the library
//! (DESIGN-v1 §10.1). Argument parsing is manual (no `clap`).
//!
//! Contract of the surface:
//! - the five evaluators (`--generate`/`--verify`/`--check`/`--sync`/`--diff`)
//!   print **exactly one** canonical `side-contract-verdict` 1.0 JSON row on
//!   stdout and a `phase:` trace on stderr;
//! - `--help` prints the usage on stdout and exits 0 without a row; a usage
//!   error (unknown/missing/inadmissible flag, absent or duplicated mode,
//!   non-empty `--out`, unknown `--cause`) prints the usage on **stderr**,
//!   leaves stdout **empty** and exits 1 without a row;
//! - `--explain --cause <code>` prints the cause text on stdout and exits 0
//!   without a row (the `command` enum of schema 1.0 only admits evaluators);
//! - exits: `0` OK · `1` usage · `2` FAIL|DRIFT · `3` reserved, never emitted ·
//!   `4` INCOMPLETE.
//!
//! B8 boundary: the only `std::process` item in `src/` is `ExitCode` below —
//! no `Command`/`Child`/`Stdio`/`spawn` anywhere in this crate's library.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use daggerfall_side_contract::explain::explain;
use daggerfall_side_contract::run::{run, EvalArgs};
use daggerfall_side_contract::taxonomy::{Cause, EXIT_USAGE};

const USAGE: &str = "\
side-contract — contrato de side para daggerfall-bevy-native (DESIGN-v1 §10)

Uso:
  side-contract --generate  --contract-root <dir> --out <dir> [--main-root <dir>] [--verify-determinism]
  side-contract --verify    --bundle <dir>
  side-contract --check     --bundle <dir> --side <dir> [--json] [--allow-downgrade]
  side-contract --sync      --bundle <dir> --side <dir> [--dry-run] [--force] [--allow-downgrade] [--json]
  side-contract --diff      --bundle <dir> --side <dir> [--json]
  side-contract --explain   --cause <code>
  side-contract --help

Los cinco modos evaluadores imprimen exactamente una fila JSON
`side-contract-verdict` 1.0 en stdout y una traza `phase:` en stderr.
Códigos de salida: 0 OK · 1 uso · 2 FAIL|DRIFT · 3 reservado · 4 INCOMPLETE.
";

const MODES: [&str; 7] = [
    "--generate",
    "--verify",
    "--check",
    "--sync",
    "--diff",
    "--explain",
    "--help",
];
const VALUE_FLAGS: [&str; 6] = [
    "--contract-root",
    "--out",
    "--main-root",
    "--bundle",
    "--side",
    "--cause",
];
const BOOL_FLAGS: [&str; 5] = [
    "--verify-determinism",
    "--json",
    "--allow-downgrade",
    "--dry-run",
    "--force",
];

/// What the arguments asked for: text output or one evaluator.
enum Action {
    Help,
    Explain(Cause),
    Eval(EvalArgs),
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let code = match parse(&argv) {
        Ok(Action::Help) => {
            print!("{USAGE}");
            0
        }
        Ok(Action::Explain(cause)) => {
            print!("{}", explain(cause));
            0
        }
        Ok(Action::Eval(eval)) => {
            let outcome = run(&eval);
            for line in &outcome.trace {
                eprintln!("{line}");
            }
            for line in &outcome.extra {
                eprintln!("{line}");
            }
            if let Some(row) = &outcome.row {
                print!("{}", row.to_json_line());
            }
            outcome.exit as i32
        }
        Err(message) => {
            eprintln!("{message}");
            eprint!("{USAGE}");
            EXIT_USAGE
        }
    };
    let _ = std::io::stdout().flush();
    // Only exit codes 0..=4 ever leave this binary (§10.5; 3 is reserved).
    ExitCode::from(code.clamp(0, 4) as u8)
}

/// Parses the command line, rejecting anything the mode does not admit.
fn parse(argv: &[String]) -> Result<Action, String> {
    if argv.is_empty() {
        return Err("faltan argumentos".to_string());
    }
    let mut modes: Vec<&str> = Vec::new();
    let mut values: BTreeMap<&str, &str> = BTreeMap::new();
    let mut bools: Vec<&str> = Vec::new();

    let mut it = argv.iter();
    while let Some(arg) = it.next() {
        let flag = arg.as_str();
        if MODES.contains(&flag) {
            if modes.contains(&flag) {
                return Err(format!("modo repetido: {flag}"));
            }
            modes.push(flag);
        } else if VALUE_FLAGS.contains(&flag) {
            let Some(value) = it.next() else {
                return Err(format!("{flag} sin valor"));
            };
            if value.starts_with("--") {
                return Err(format!("{flag} sin valor"));
            }
            if values.insert(flag, value.as_str()).is_some() {
                return Err(format!("bandera repetida: {flag}"));
            }
        } else if BOOL_FLAGS.contains(&flag) {
            if bools.contains(&flag) {
                return Err(format!("bandera repetida: {flag}"));
            }
            bools.push(flag);
        } else {
            return Err(format!("argumento desconocido: {flag}"));
        }
    }

    if modes.len() != 1 {
        return Err(
            "se requiere exactamente un modo: --generate, --verify, --check, --sync, \
             --diff, --explain o --help"
                .to_string(),
        );
    }
    let mode = modes[0];
    reject_unknown(mode, &values, &bools)?;

    match mode {
        "--help" => Ok(Action::Help),
        "--explain" => {
            let cause = required(&values, "--cause")?;
            match Cause::from_id(cause) {
                Some(cause) => Ok(Action::Explain(cause)),
                None => Err(format!(
                    "cause desconocida: {cause} (use --help para ver los modos; \
                     los códigos son los de la taxonomía cerrada v1)"
                )),
            }
        }
        "--generate" => {
            let contract_root = required(&values, "--contract-root")?;
            let out = required(&values, "--out")?;
            if out_is_nonempty(out) {
                return Err(format!(
                    "--out existe y no está vacío: {out} (borre o elija otro destino)"
                ));
            }
            Ok(Action::Eval(EvalArgs {
                command: "generate",
                contract_root: contract_root.into(),
                out: out.into(),
                main_root: values
                    .get("--main-root")
                    .map(|value| (*value).into())
                    .unwrap_or_else(|| ".".into()),
                verify_determinism: bools.contains(&"--verify-determinism"),
                bundle: Default::default(),
                side: Default::default(),
                json: false,
                allow_downgrade: false,
                dry_run: false,
                force: false,
            }))
        }
        "--verify" => Ok(Action::Eval(EvalArgs {
            command: "verify",
            bundle: required(&values, "--bundle")?.into(),
            ..base_args("verify")
        })),
        "--check" => Ok(Action::Eval(EvalArgs {
            command: "check",
            bundle: required(&values, "--bundle")?.into(),
            side: required(&values, "--side")?.into(),
            json: bools.contains(&"--json"),
            allow_downgrade: bools.contains(&"--allow-downgrade"),
            ..base_args("check")
        })),
        "--sync" => Ok(Action::Eval(EvalArgs {
            command: "sync",
            bundle: required(&values, "--bundle")?.into(),
            side: required(&values, "--side")?.into(),
            json: bools.contains(&"--json"),
            allow_downgrade: bools.contains(&"--allow-downgrade"),
            dry_run: bools.contains(&"--dry-run"),
            force: bools.contains(&"--force"),
            ..base_args("sync")
        })),
        "--diff" => Ok(Action::Eval(EvalArgs {
            command: "diff",
            bundle: required(&values, "--bundle")?.into(),
            side: required(&values, "--side")?.into(),
            json: bools.contains(&"--json"),
            ..base_args("diff")
        })),
        other => Err(format!("modo desconocido: {other}")),
    }
}

/// The `EvalArgs` defaults for the evaluators that only fill some fields.
fn base_args(command: &'static str) -> EvalArgs {
    EvalArgs {
        command,
        contract_root: Default::default(),
        out: Default::default(),
        main_root: Default::default(),
        verify_determinism: false,
        bundle: Default::default(),
        side: Default::default(),
        json: false,
        allow_downgrade: false,
        dry_run: false,
        force: false,
    }
}

/// Rejects value/bool flags the mode does not admit (usage error).
fn reject_unknown(mode: &str, values: &BTreeMap<&str, &str>, bools: &[&str]) -> Result<(), String> {
    let (allowed_values, allowed_bools): (&[&str], &[&str]) = match mode {
        "--generate" => (
            &["--contract-root", "--out", "--main-root"],
            &["--verify-determinism"],
        ),
        "--verify" => (&["--bundle"], &[]),
        "--check" => (&["--bundle", "--side"], &["--json", "--allow-downgrade"]),
        "--sync" => (
            &["--bundle", "--side"],
            &["--json", "--allow-downgrade", "--dry-run", "--force"],
        ),
        "--diff" => (&["--bundle", "--side"], &["--json"]),
        "--explain" => (&["--cause"], &[]),
        "--help" => (&[], &[]),
        other => return Err(format!("modo desconocido: {other}")),
    };
    for flag in values.keys() {
        if !allowed_values.contains(flag) {
            return Err(format!("bandera inadmisible para {mode}: {flag}"));
        }
    }
    for flag in bools {
        if !allowed_bools.contains(flag) {
            return Err(format!("bandera inadmisible para {mode}: {flag}"));
        }
    }
    Ok(())
}

/// A flag that needs a value must have exactly one.
fn required<'a>(values: &'a BTreeMap<&'a str, &'a str>, flag: &str) -> Result<&'a str, String> {
    values
        .get(flag)
        .copied()
        .ok_or_else(|| format!("falta {flag} <dir>"))
}

/// `--out` must be absent or an empty directory (usage, exit 1): generating
/// over an existing bundle would silently mix two contracts.
fn out_is_nonempty(path: &str) -> bool {
    let path = Path::new(path);
    match std::fs::metadata(path) {
        Ok(meta) if meta.is_dir() => std::fs::read_dir(path)
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(true),
        Ok(_) => true,
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_documents_every_mode() {
        for mode in MODES {
            assert!(USAGE.contains(mode), "usage sin {mode}");
        }
        assert!(USAGE.ends_with('\n'));
    }

    #[test]
    fn empty_argv_is_a_usage_error() {
        assert!(parse(&[]).is_err());
    }

    #[test]
    fn absent_or_duplicated_mode_is_a_usage_error() {
        let only_flag: Vec<String> = ["--json".to_string()].into_iter().collect();
        assert!(parse(&only_flag).is_err());
        let two_modes: Vec<String> = ["--verify", "--check", "--bundle", "b", "--side", "s"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(parse(&two_modes).is_err());
    }

    #[test]
    fn unknown_missing_and_repeated_flags_are_usage_errors() {
        let unknown: Vec<String> = ["--verify", "--bundle", "b", "--frobnicate"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(parse(&unknown).is_err());
        let missing: Vec<String> = vec!["--verify".to_string()];
        assert!(parse(&missing).is_err());
        let repeated: Vec<String> = ["--verify", "--bundle", "b", "--bundle", "c"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(parse(&repeated).is_err());
    }

    #[test]
    fn inadmissible_flag_for_the_mode_is_a_usage_error() {
        // --dry-run belongs to sync, not check
        let bad: Vec<String> = ["--check", "--bundle", "b", "--side", "s", "--dry-run"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(parse(&bad).is_err());
        // --help admits no other flag
        let help_bad: Vec<String> = vec!["--help".to_string(), "--json".to_string()];
        assert!(parse(&help_bad).is_err());
    }

    #[test]
    fn unknown_cause_is_a_usage_error() {
        let bad: Vec<String> = ["--explain", "--cause", "no_existe"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(parse(&bad).is_err());
        let good: Vec<String> = ["--explain", "--cause", "lock_stale"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(matches!(
            parse(&good),
            Ok(Action::Explain(Cause::LockStale))
        ));
    }

    #[test]
    fn main_root_defaults_to_dot() {
        let argv: Vec<String> = ["--generate", "--contract-root", "cr", "--out", "o"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        match parse(&argv) {
            Ok(Action::Eval(eval)) => {
                assert_eq!(eval.command, "generate");
                assert_eq!(eval.main_root, Path::new("."));
                assert!(!eval.verify_determinism);
            }
            other => {
                let _ = other;
                panic!("expected generate eval");
            }
        }
    }
}

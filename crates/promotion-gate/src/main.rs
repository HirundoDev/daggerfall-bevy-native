//! `promotion-gate` CLI: evaluate one side against plan §4.3 + DESIGN-v1 §12.1
//! in a single command. stdout carries exactly one verdict row (JSON); stderr
//! carries the step trace; the exit code is the verdict (`0` PASS, `1` usage,
//! `2` FAIL, `4` INCOMPLETE). The normative contract is
//! `docs/sintergic/promotion-gate/PROMOTION-GATE-v3.md` (v1 and v2 are superseded).

use std::path::PathBuf;
use std::process::ExitCode;

use daggerfall_promotion_gate::{run_gate, GateConfig, DEFAULT_LEDGER};

const USAGE: &str = "\
promotion-gate — gate de promoción I1 (paso 0 + plan §4.3 pasos 1-7, F4-A + F4-B + F6-D)

Uso:
  promotion-gate --side <dir> [--main-root <dir>] [--ledger <file>] [--fleet-dir <dir>]

Opciones:
  --side <dir>      workspace del side (contiene side-project.toml) [obligatorio]
  --main-root <dir> workspace main: HEAD, game-core, bevy, releases (default: .)
  --ledger <file>   ledger append-only de promociones
                    (default: <main-root>/docs/sintergic/promotion-gate/promotion-ledger.jsonl)
  --fleet-dir <dir> directorio para la unicidad de side.id (default: padre de --side)
  -h, --help        esta ayuda

Códigos de salida: 0 PASS · 1 uso · 2 FAIL · 4 INCOMPLETE (gate_internal_error)
Contrato normativo: docs/sintergic/promotion-gate/PROMOTION-GATE-v3.md
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut side: Option<PathBuf> = None;
    let mut main_root = PathBuf::from(".");
    let mut ledger: Option<PathBuf> = None;
    let mut fleet: Option<PathBuf> = None;

    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "--side" | "--main-root" | "--ledger" | "--fleet-dir" => {
                let Some(value) = it.next() else {
                    eprintln!("error: {arg} requiere un valor\n\n{USAGE}");
                    return usage();
                };
                let path = PathBuf::from(value);
                match arg.as_str() {
                    "--side" => side = Some(path),
                    "--main-root" => main_root = path,
                    "--ledger" => ledger = Some(path),
                    "--fleet-dir" => fleet = Some(path),
                    _ => unreachable!(),
                }
            }
            other => {
                eprintln!("error: argumento desconocido {other:?}\n\n{USAGE}");
                return usage();
            }
        }
    }

    let Some(side) = side else {
        eprintln!("error: falta --side <dir>\n\n{USAGE}");
        return usage();
    };

    let mut config = GateConfig::new(side, main_root);
    if let Some(ledger) = ledger {
        config.ledger = ledger;
    }
    if let Some(fleet) = fleet {
        config.fleet_dir = fleet;
    }
    if config.ledger.as_os_str().is_empty() {
        config.ledger = PathBuf::from(DEFAULT_LEDGER);
    }

    let outcome = run_gate(&config);
    for line in &outcome.trace {
        eprintln!("{line}");
    }
    if let Some(err) = &outcome.ledger_error {
        eprintln!("ADVERTENCIA: fila NO materializada en el ledger: {err}");
    }
    println!("{}", outcome.verdict.to_json_line());

    let code = outcome.verdict.exit_code();
    ExitCode::from(code.clamp(0, 255) as u8)
}

fn usage() -> ExitCode {
    ExitCode::from(daggerfall_promotion_gate::taxonomy::EXIT_USAGE as u8)
}

# The launcher

*Case file split from [`docs/ACKNOWLEDGMENTS.md`](../ACKNOWLEDGMENTS.md): the body was moved verbatim; only relative link targets and heading levels were rebased.*

`scripts/launch.sh` is the single entry point for running, probing and building. It resolves the
mode, validates features against `crates/app/Cargo.toml`, checks the data root, and never copies or
mutates game data.

| Mode | Build | Window | Typical use |
|---|---|---|---|
| `complete` | `--release` | yes (unless `--headless`) | full demo |
| `preview` (default) | debug | no | fast iteration, CI |
| `pristine` | debug | never | parity baseline (zero opt-in layers) |

| Flag | Effect |
|---|---|
| `--probe <ID>` | run one behaviour probe and exit with its code (`ALL-001`, `RMB-001`, `TERRAIN-001`, …) |
| `--headless` | force no window; no display required |
| `--window` | request the `window` feature (rejected with `pristine`, incompatible with `--headless`) |
| `--features <csv>` | explicit app features; only features declared in the manifest are accepted (`modern`/`mods` are reserved and rejected) |
| `--data-root <dir>` | export `DF_APP_ROOT` for this run |
| `--no-build` | run an existing binary |
| `--build-only` | build and exit |
| `--print-plan` | print the resolved plan with zero side effects |
| `--timeout <sec>` | hard run timeout (default 900 s; build default 1800 s) |

```bash
# Full behaviour-probe suite through the launcher
./scripts/launch.sh --probe ALL-001

# Inspect what would run, without building or executing anything
./scripts/launch.sh --mode complete --print-plan

# Plain cargo, if you prefer
cargo test --workspace
cargo run -p daggerfall-app --features window
cargo run -p daggerfall-app -- --probe ALL-001
```

Exit codes: `0` success · `1` build/binary/feature failure · `2` invalid usage · `3` window
requested without a display · `124` timeout · other codes come from the probe itself.

Every row above is a verified behaviour, not an intention: the launcher's mode and flag handling is
covered by 11 checks (all three modes, feature rejection, unknown flags and exit codes). The probe
count grows with each closed milestone; `ALL-001` prints the current total.

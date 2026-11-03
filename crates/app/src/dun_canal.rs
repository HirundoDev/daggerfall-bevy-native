//! `dun_canal` (app, `probes`) — the **single output channel** of the dungeon
//! instrumentation (M44, `CR-bd972066-59e6-4768-9985-de75f69f6798`).
//!
//! **Why it exists:** the operator-facing gate
//! (`scripts/dun-geo-001.py`) reads the retained state file named by
//! `$DF_DUN_STATE` *after* the probe has run. A line that only reaches stdout
//! is invisible to it, and a line that only reaches the file is invisible to
//! the two-run byte comparison. Both consumers are served by one call:
//! [`emit`] prints the line **and** appends it to `$DF_DUN_STATE` (absent
//! variable ⇒ stdout only, never a silent failure).
//!
//! **Determinism (the gate compares two processes byte by byte):** nothing
//! here reads a clock, a RNG or the environment except the *presence* of
//! `DF_DUN_STATE`; the content of a line is owned by its publisher.
//!
//! **One call site for the whole cycle:** [`emit_all`] publishes the three
//! sections in a fixed order — geometry ([`crate::dun_geometry`]), state
//! ([`crate::dungeon_state`]) and eligibility ([`crate::dungeon_elig`]). The
//! probes call it once, so the sibling lanes of this cycle add evidence
//! without ever touching the probe or this channel.

use std::io::Write as _;

/// Prints `line` and appends it (flushed) to `$DF_DUN_STATE` when the
/// variable is set. A file that cannot be opened is reported on stdout — the
/// evidence is never dropped in silence.
pub fn emit(line: &str) {
    println!("{line}");
    let Some(path) = std::env::var_os("DF_DUN_STATE") else {
        return;
    };
    let opened = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path);
    match opened {
        Ok(mut file) => {
            let _ = writeln!(file, "{line}");
            let _ = file.flush();
        }
        Err(err) => println!("DUN VE: canal — {path:?} not writable ({err}), stdout only"),
    }
}

/// Publishes every dungeon section, in fixed order: geometry → state →
/// eligibility. The order is part of the contract: two runs must produce the
/// same bytes.
pub fn emit_all() {
    crate::dun_geometry::publish();
    crate::dungeon_state::publish();
    crate::dungeon_elig::publish();
}

# Export limits

The public export is the published tree, not the engineering record. The record
(`docs/sintergic/`, `.sintergic/`, `.omp/`, `.opencode/`, `promotion-ledger`) stays in the
private workspace, so the exported code still *names* a few internal paths. This page declares
what is missing, what the shipped code does when it meets the absence, and where the metric
that counts such references is printed. Every row is anchored to `file:line` in this tree and describes the
behaviour of the code as published — nothing here is a promise about a future state.

The measurement and the failure policy live in `tools/public-export/verify-export.py` (pass E),
the tolerances in `tools/public-export/internal-refs-allowlist.txt`, and the exclusions in
`tools/public-export/exclude.txt`.

Every figure on this page names the commit it was measured from. A release is identified by its
name and the commit it was cut from — `v0.3.1-alpha` was cut from content commit `9f9cc3b` — and
never by a figure printed on this page: pass E is printed by
`python3 tools/public-export/verify-export.py --export <dir>` over the finished tree, at the moment
of the export, and the line is not repeated here because this page travels inside the export and is
part of the surface that pass measures. Writing the count into the page moves the count.

The measurements quoted below belong to earlier, immutable commits and stay attributed to them: the
cut of `673b8a92f2ec7dbe9663d443fd91e91c75cd50b6`, exported with
`python3 tools/public-export/export-public.py --release-commit 673b8a9`, produced commit
`30fa584c47e18b5483837965cea891dcb2291810` and tree `08560a768acd5884cf092e9a2f82d41e1bae39da`
twice, byte for byte, and every row below that carries it stays attributed to it. The commit that
carries these sentences publishes no figure of itself.

## 0. What this page does not measure

Pass E prints one line, over the finished tree, at the moment of the export:

```
E_internal_refs: files=<n> lines=<n> functional=<n> non_functional=<n> allowlisted=<n> occurrences=<n> unallowlisted=[…] stale=[…] verdict=PASS|FAIL
```

The two bracketed lists are the failure lists of that pass: a functional line with no tolerance row
is `unallowlisted`, and a tolerance row whose line disappeared, moved or stopped being functional is
`stale`. Both are empty on a PASS.

That line is not reproduced here with values. This page travels inside the export, so it is part of
the surface pass E measures: a count written into it moves the count, and a figure of the tree that
carries the page is refuted by the page the moment the page is edited. The authority is the
verifier's output for the export being cut; the release is identified by its name and by the commit
it was cut from, both carried by the export report of that run and by `git`.

The tests of the exported tree are measured the same way, by running the workspace inside the
export. §1 names the test files the export omits and the commit at which their `#[test]` attributes
and the executed counts were measured.

## 1. Tests excluded from the export

Four test files are excluded by their **exact path**, never by a `tests/` wildcard: every other
test file ships and is expected to compile and pass in this tree.

| File | `#[test]` | Why it cannot ship |
|---|---:|---|
| `crates/side-contract/tests/matrix.rs` | 33 | reads `docs/sintergic/side-contract/contract.toml` (`matrix.rs:247`); the contract corpus is not published, so the test panics here. |
| `crates/content-api/tests/schema_published.rs` | 2 | `include_str!` of the content-api contract (`:8`, `:9`); the test target does not compile here. |
| `crates/content-runtime/tests/surface_published.rs` | 4 | `include_str!` of the content-runtime contract (`:14`); the test target does not compile here. |
| `crates/game-runtime-api/tests/surface_published.rs` | 4 | `include_str!` of the game-runtime-api contract (`:14`); the test target does not compile here. |

Three promotion-gate test files are also absent, as before this change: they drive fixtures that
live in the private contract corpus (12 `#[test]` in total, under `crates/promotion-gate/tests/`).

`#[test]` attributes measured in the tree on the `673b8a9` cut, same command on both sides:

```
grep -rn --include=*.rs -E '^[[:space:]]*#\[(tokio::)?test\]' crates | wc -l
```

| Tree | `#[test]` |
|---|---:|
| private workspace | 1604 |
| this export | 1549 |
| delta | 55 |

The delta is fully explained by excluded files — 43 from the four files above (33 + 2 + 4 + 4)
and 12 from the promotion-gate fixtures. No test attribute lives outside `crates/`, and no file
shared by both trees loses one.

A test attribute is not always a test: `crates/promotion-gate/tests/gate_fixtures.rs` declares one
`#[test]` inside a `macro_rules!` template that is instantiated once per fixture, so its 5 literal
attributes run 28 tests. The executed counts below were measured at commit `673b8a9` with the same
command on both sides (`cargo test --workspace`, after `cargo test --no-run --workspace` compiled
every target in the export with exit 0):

| Tree | Tests reported | Test binaries |
|---|---:|---:|
| private workspace (clean detached worktree) | 1,602 passed / 0 failed / 3 ignored | 66 |
| this export | 1,524 passed / 0 failed / 3 ignored | 59 |
| delta | 78 | 7 |

The 78 absent tests are the 43 of the four files above (each literal attribute is one test) plus the
35 of the three promotion-gate test files (2 + 28 + 5): 55 literal attributes that expand to 78
executed cases, and no test binary shared by both trees reports a different count.

## 2. Runtime limits

The shipped code refers to internal paths in four ways: it reads a main-owned record, it writes
into an internal directory, it prints the path as evidence, or it only mentions it in a docstring
or a help text. Only the first two change behaviour.

### `promotion-gate` — reads main-owned records

| Reference | Activated by | Behaviour here |
|---|---|---|
| `crates/promotion-gate/src/gate.rs:29` | any run without `--ledger` (`GateConfig::new` joins it to `--main-root`, default `.`, at `gate.rs:51`) | the ledger path resolves to `docs/sintergic/promotion-gate/promotion-ledger.jsonl`; `Verdict::append_to` (`verdict.rs:230`) creates the directory and **writes** the row, so a run materialises an internal directory in the tree. If the tree is read-only the row is skipped and the run prints `ADVERTENCIA: fila NO materializada en el ledger: <err>` on stderr (`main.rs:85`) with the verdict unchanged. |
| `crates/promotion-gate/src/main.rs:78` | `--ledger ""` (an explicitly empty value) | the empty path is discarded and the default constant is used **relative to the current directory**, not to `--main-root`; otherwise identical to the row above. |
| `crates/promotion-gate/src/contract.rs:44` | step 0, for a side whose `contract.lock` cites a bundle | the release directory is read with `read_dir`; a missing directory is silent (`contract.rs:248`), the release list stays empty and the gate ends with `contract_unknown` (exit 2): no release record of the main can verify the locked contract. |
| `crates/promotion-gate/src/baseline.rs:35` | every run: step 6b (`gate.rs:305`) | the baseline declaration is main-owned and absent here, so the read fails and the gate ends with `main_baseline_failed` (exit 2). The gate therefore cannot pass in this tree without a declared baseline. |
| `crates/promotion-gate/src/allowlist.rs:12` | every run: the write-allowlist step over `--side` | a missing directory returns an empty audit (`allowlist.rs:18-21`) — it **degrades** to "nothing recorded, nothing to violate"; an unreadable existing directory aborts the step with an error. |

`crates/promotion-gate/src/main.rs:22` and `:27` are `--help` text only, and
`crates/promotion-gate/src/contract.rs:486` is a doc comment inside a `#[cfg(test)]` module.

### The windowed demo — writes its own evidence

`crates/app/src/m18_window.rs:689` is activated by a windowed run that renders a context song.
`create_dir_all` failing prints `M14 VE: music evidence directory ... unavailable (<err>) —
duration <s> s only` and returns (`m18_window.rs:702`): the run **degrades**, the music still
plays, no panic. On a writable tree the directory is created and the WAV is written (above 5 MB
a `sha256` footprint in `.json` instead).

### Operator scripts — private inputs and outputs

| Reference | Activated by | Behaviour here |
|---|---|---|
| `scripts/closure-gate.py:27` | any run without `--register` | reads the closure register: a missing file raises `FileNotFoundError`, so the script stops instead of reporting a closure verdict. |
| `scripts/ui-audit-001.py:46` `:47` `:48` `:50` `:52` | any run with the default inputs | the corpus, census, spec, audit and transcript documents are internal: the first read raises `FileNotFoundError`. |
| `scripts/verify-m18-owner-pack.py:15` | any run | `open()` on the gate evidence at `:17` raises `FileNotFoundError`. |
| `scripts/npc-flats-capture.py:592` | any run without `--out` | `os.makedirs(..., exist_ok=True)` at `:394` **creates** the internal evidence directory in the tree and writes the capture there. |

`scripts/ci/language-check.py:75` (`EXEMPT_PREFIX`), `scripts/ci/trademark-check.sh:70` and
`:81`, and the export tooling's own patterns (`verify-export.py:212`, `:290`, `:405`,
`export-public.py:86`) only compare strings: they classify or count a path, they never open it.
`scripts/ci/test-language-check.sh:122` writes its fixture under `$TMP`. The probe evidence lines
`crates/app/src/dungeon_probe.rs:252` and `crates/app/src/m34_automap.rs:526` print the path;
the docstrings in `scripts/ui-base-001.py`, `scripts/ui-config-001.py`, `scripts/ui-create-001.py`,
`scripts/ui-input-001.py`, `scripts/ui-jugable-001.py`, `scripts/closure-gate.py:6`,
`scripts/ci/language-check.py:38` and the comment at
`crates/parser-bevy-bridge/tests/person_flats_inventory.rs:155` never execute.

## 3. Internal-reference metric

Pass E of `tools/public-export/verify-export.py` counts every file, line and occurrence that
mentions an internal path, splits the lines into functional and non-functional, and fails with the
number of offending references when a functional line is not tolerated by
`internal-refs-allowlist.txt`. The metric is printed on every run, PASS included, and the pass
cannot be skipped.

Reproduce it on an exported tree:

```
python3 tools/public-export/verify-export.py --export <exported-tree>
```

Reported on the export of commit `673b8a9` (export commit
`30fa584c47e18b5483837965cea891dcb2291810`, tree `08560a768acd5884cf092e9a2f82d41e1bae39da`), the
verifier returned verdict **PASS**: every functional line of that tree was carried by a row of the
tolerance file, and both failure lists were empty. Its `E_internal_refs` line is not restated here,
by the same rule as §0: the page travels inside the export.

The tolerance file holds one row per functional line, each with its reason; the remaining lines that
mention an internal path are prose, comments and the exclusion/tolerance data itself. A functional
line added without a row fails the verifier with the number of offending references, and a row whose
line disappears, moves or stops being functional is reported as stale and also fails: the count
cannot be lowered by editing the allowlist, only by removing the reference from the code. The
`673b8a9` run stays attributed to that commit and is not inherited by a later tree.

# Translation status

Snapshot: **2026-10-21**. Scope: the documents published in this repository. Priorities are defined
in [`POLICY.md`](POLICY.md#when-a-translation-is-produced): **P0** entry door, **P1** published file
still in Spanish, **P2** nice to have, **—** not a translation target.

**Summary: 15 published markdown documents · 0 stale · 1 local-only translation (not published).**
The public export ships English only and the tree is English: the sweep reports **0 Spanish prose
blocks** on the exportable surface (`bash scripts/ci/language-check.sh`, §E). Translations are written
by hand, on demand; the push is gated by the export gate, which re-measures the exported tree.

## A. Project documentation — English canonical (P0)

| File | Lines | Translation | Status |
|---|---:|:---:|:---:|
| `README.md` | 400 | — | current |
| `ARCHITECTURE.md` | 250 | — | current |
| `CONTRIBUTING.md` | 358 | — | current |
| `docs/ACKNOWLEDGMENTS.md` (index) | 45 | — | current |
| `docs/acknowledgments/credits.md` | 54 | — | current |
| `docs/acknowledgments/legal-and-trademarks.md` | 38 | — | current |
| `docs/acknowledgments/why-not-dfu-or-daggerfalljs.md` | 63 | — | current |
| `docs/acknowledgments/parity-verification.md` | 71 | — | current |
| `docs/acknowledgments/status.md` | 66 | — | current |
| `docs/acknowledgments/roadmap.md` | 23 | — | current |
| `docs/acknowledgments/launcher.md` | 45 | — | current |
| `docs/acknowledgments/crates-and-determinism.md` | 40 | — | current |
| `docs/i18n/POLICY.md` | 144 | — | current |
| `docs/i18n/STATUS.md` (this file) | — | — | current |
| `DATA-CLASSIFICATION.md` | 47 | local `es` only (not published) | current |

Canonical English prose on the published surface: **1,919 lines**, the sum of §A and §C. `LICENSE` and `NOTICE` are legal text and are not translation targets.

## B. Third-party corpus — verbatim, never translated

`reference/dfu-spec/`: **123 tracked files** — 119 `.cs` (Daggerfall Unity, MIT, © 2009–2023
Daggerfall Workshop), 2 `.txt` and the corpus licence. English, byte-preserved, quoted as an
executable specification. Its own `LICENSE.txt` governs it; this policy does not touch it.

Two project-owned records sit next to the corpus and are part of the published surface, in English:
`PROVENANCE.md` (the source-class pin: verification protocol, pin table, verified anchors, declared
gaps, clean-room rule) and `CLASSIFICATION.md` (origin and chain of custody, the per-file map with
sha256 and byte counts, and the system legend).

## C. Custody records — translated, claims unchanged

| Item | Lines | Note |
|---|---:|---|
| `reference/dfu-spec/PROVENANCE.md` | 84 | English translation of the original Spanish custody record. Only internal process identifiers were dropped; upstream commits, line references, byte counts, hashes and clean-room rules are as recorded. |
| `reference/dfu-spec/CLASSIFICATION.md` | 191 | English translation of the original Spanish classification record: 113 mapping rows with sha256 and byte counts, copied verbatim from the original table and verified row by row. |

Both files carry the fidelity note at the top. The Spanish originals remain in the repository
history.

## D. Local-only translations — never published

| File | Lines | Canonical | Status | Visibility |
|---|---:|---|---|---|
| `DATA-CLASSIFICATION.es.md` | 50 | `DATA-CLASSIFICATION.md` @ `261a1b5` | current | **local-only**: excluded from the export, not linked from any published file |

The tree is English: every document in §A–§C was translated **by hand, file by file**; no automated
translator is used, and the working record kept outside the published surface is not a translation
target. What is translated is prose, not code: identifiers, paths, probe IDs, commands and commit
hashes stay byte-identical. The export gate must still report 0 Spanish files in the published tree
before anything is published.

## E. Code-surface residual - measured

The published tree is not code-free: Rust sources, `Cargo.toml` files, the scripts and the export
tooling ship with the documentation. Measured at this snapshot with the language gate
(`bash scripts/ci/language-check.sh`), which reads comments, Python docstrings and manifest
descriptions over `git ls-files` minus the export exclusions:

- **0 Spanish prose blocks** on the exportable surface, and 0 accented characters inside comments,
  docstrings or descriptions.
- **1 file carries accented characters**, `tools/public-export/export-public.py`, and they are
  pattern data — the Spanish accent character class — not prose. The Spanish word list lives in
  `scripts/ci/language-check.py`, which the export gate now imports instead of keeping a second
  detector, so the private tree and the exported tree are measured by one rule. The vendored
  corpus (`reference/dfu-spec/**`) is exempt from the gate and is English.

An earlier snapshot counted 61 files and 2,484 accented characters in code comments. That count is
superseded: the code comments are English, and the gate is what keeps them that way. It fails on
prose, so it reads `#`, `//` and `/* */` comments, Python docstrings and `description` values;
string literals are not read, because a data string is not a sentence and reading them would flag
the fixtures of `scripts/ci/test-language-check.sh` and `tools/public-export/test-export-public.py`,
which exist to prove the gate rejects Spanish.

The export gate re-measures the exported tree and remains the last barrier: publication is no longer
held back by language on the measured surface, and a file that reaches the export in Spanish is a
defect even when the CI gate passed on the private tree.

## Staleness register

| Translation | Canonical | `canonical-commit` | Status |
|---|---|---|---|
| `DATA-CLASSIFICATION.es.md` | `DATA-CLASSIFICATION.md` | `261a1b5` | current (local-only, not published) |

The check is manual: `git log -1 --format=%h -- DATA-CLASSIFICATION.md` must still match the
`canonical-commit` in the translation's header. Any later change to the canonical makes the
translation `stale` until it is re-translated and the header bumped.

## Reproducing this inventory

```bash
wc -l README.md ARCHITECTURE.md CONTRIBUTING.md DATA-CLASSIFICATION*.md \
      docs/ACKNOWLEDGMENTS.md docs/acknowledgments/*.md docs/i18n/*.md
git ls-files 'reference/' | wc -l                 # third-party corpus (123)
git log -1 --format=%h -- DATA-CLASSIFICATION.md  # canonical commit of the only translation
# section E: Spanish prose blocks on the exportable surface (0) and its size
bash scripts/ci/language-check.sh
bash scripts/ci/language-check.sh --list-surface | wc -l
# section E: files carrying accented characters in the exported code surface (1, pattern data)
git ls-files crates scripts tools Cargo.toml | grep -v '^tools/public-export/CHECKLIST.md$' | xargs grep -lP '[\x{e1}\x{e9}\x{ed}\x{f3}\x{fa}\x{f1}\x{c1}\x{c9}\x{cd}\x{d3}\x{da}\x{d1}\x{bf}\x{a1}]' | wc -l
```

Counts are a snapshot: a pull request that adds a document updates this table in the same change.

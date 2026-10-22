# Translation status

Snapshot: **2026-10-21**. Scope: the documents published in this repository. Priorities are defined
in [`POLICY.md`](POLICY.md#when-a-translation-is-produced): **P0** entry door, **P1** published file
still in Spanish, **P2** nice to have, **—** not a translation target.

**Summary: 15 published markdown documents · 0 stale · 1 local-only translation (not published).**
The public export ships English only. The Spanish translation of the tree is **in progress** (manual,
no scripts) and the push is **blocked until the sweep reports 0 Spanish files in the published tree**.

## A. Project documentation — English canonical (P0)

| File | Lines | Translation | Status |
|---|---:|:---:|:---:|
| `README.md` | 400 | — | current |
| `ARCHITECTURE.md` | 250 | — | current |
| `CONTRIBUTING.md` | 328 | — | current |
| `docs/ACKNOWLEDGMENTS.md` (index) | 45 | — | current |
| `docs/acknowledgments/credits.md` | 54 | — | current |
| `docs/acknowledgments/legal-and-trademarks.md` | 38 | — | current |
| `docs/acknowledgments/why-not-dfu-or-daggerfalljs.md` | 63 | — | current |
| `docs/acknowledgments/parity-verification.md` | 71 | — | current |
| `docs/acknowledgments/status.md` | 66 | — | current |
| `docs/acknowledgments/roadmap.md` | 23 | — | current |
| `docs/acknowledgments/launcher.md` | 45 | — | current |
| `docs/acknowledgments/crates-and-determinism.md` | 40 | — | current |
| `docs/i18n/POLICY.md` | 125 | — | current |
| `docs/i18n/STATUS.md` (this file) | — | — | current |
| `DATA-CLASSIFICATION.md` | 47 | local `es` only (not published) | current |

Canonical English prose on the published surface: **1,904 lines**, the sum of §A and §C. `LICENSE` and `NOTICE` are legal text and are not translation targets.

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
| `reference/dfu-spec/PROVENANCE.md` | 118 | English translation of the original Spanish custody record. Only internal process identifiers were dropped; upstream commits, line references, byte counts, hashes and clean-room rules are as recorded. |
| `reference/dfu-spec/CLASSIFICATION.md` | 191 | English translation of the original Spanish classification record: 113 mapping rows with sha256 and byte counts, copied verbatim from the original table and verified row by row. |

Both files carry the fidelity note at the top. The Spanish originals remain in the repository
history.

## D. Local-only translations — never published

| File | Lines | Canonical | Status | Visibility |
|---|---:|---|---|---|
| `DATA-CLASSIFICATION.es.md` | 50 | `DATA-CLASSIFICATION.md` @ `261a1b5` | current | **local-only**: excluded from the export, not linked from any published file |

Translating the tree to English is **in progress** and done by hand, file by file: there is no
automated sweep yet. Sections A–C list what has been translated so far; the working record kept
outside the published surface is not a translation target. The unfinished part is a condition for the
push, not for the documents already published: the export gate must report 0 Spanish files in the
published tree before anything is published.

## E. Code-surface residual - measured, still open

The published tree is not code-free: Rust sources, `Cargo.toml` files and the export scripts ship
with the documentation, and some of their comments are still Spanish. Measured at this snapshot with
the export gate's rule - the accented Latin characters Spanish uses (U+00E1, U+00E9, U+00ED, U+00F3,
U+00FA, U+00F1 and their capitals, plus U+00BF and U+00A1) - over `git ls-files` minus the export
exclusions: **61 files, 2,484 accented characters**, all in code: `crates/**` (55), `scripts/**` (3),
`tools/public-export/**` (2 exported files) and one `Cargo.toml`. The vendored corpus
(`reference/dfu-spec/**`) contains none.

The export report counts **59** rather than 61 for the same tree: its scanner skips
`tools/public-export/`, because the export tooling is not product. 61 is the published-surface number.

Translation of those comments is **in progress, by hand**; no script rewrites them. Until this
section reads 0, the push stays blocked, and no file in this repository claims the code is English.

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
# section E: files with accented characters in the exported code surface
git ls-files crates scripts tools Cargo.toml | grep -v '^tools/public-export/CHECKLIST.md$' | xargs grep -lP '[\x{e1}\x{e9}\x{ed}\x{f3}\x{fa}\x{f1}\x{c1}\x{c9}\x{cd}\x{d3}\x{da}\x{d1}\x{bf}\x{a1}]' | wc -l
```

Counts are a snapshot: a pull request that adds a document updates this table in the same change.

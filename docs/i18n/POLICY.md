# Language policy

Status: **active**, decided with the publication of the repository. Applies to documentation and
repository text. It does not apply to code identifiers, which are English by convention already.

**Decision: English is the canonical language of this repository.** The public export ships English
only; `*.es.md` translations are local-only and excluded from it. The export gate rejects Spanish in
the published tree. Spanish is a *derived* surface, produced on demand and never a source of truth.
Prose is written in English **when it is created** — a file is English from its first commit, never
translated into English after the fact.

## Why English is canonical

- The Daggerfall modding and reverse-engineering community works in English: UESP's format
  documentation, Daggerfall Unity's source and every BSA discussion this engine cites are English.
  A public repository whose entry door is English can be checked against its sources without a
  translation hop.
- One source of truth. Two "canonical" copies of the same paragraph drift; a derived copy can be
  verified against the canonical one, and can be marked stale when it falls behind.
- Contributors can review the thing that actually matters (claims, evidence, commands) without
  maintaining two versions of every probe result.

This is a documentation decision, not a statement about anyone's language: it is the language in
which the project's evidence can be checked against its sources.

## Scope: what is translated and what is not

| Layer | Paths | Language | Translated? |
|---|---|---|---|
| **Project documentation** — the entry door and everything a reader lands on | `README.md`, `ARCHITECTURE.md`, `CONTRIBUTING.md`, `docs/ACKNOWLEDGMENTS.md`, `docs/acknowledgments/**`, `docs/i18n/**`, `DATA-CLASSIFICATION.md` | English canonical | Derived `*.es.md` are **local-only** and excluded from the export |
| **Third-party corpus** | `reference/dfu-spec/**` (Daggerfall Unity source, MIT) | English, verbatim | **No.** Vendored text is never edited, including its language. |

Consequences a reader should expect:

- **The invariant:** the public export ships English only; `*.es.md` translations are local-only and
  excluded; the export gate rejects Spanish in the published tree. Spanish prose that reaches the
  published tree is a defect to fix by translating that file into English — never by shipping a
  translation alongside it.

## Enforcement

The invariant has two barriers, both scoped to the exportable surface (everything
`tools/public-export/exclude.txt` does not exclude):

- **CI gate.** `bash scripts/ci/language-check.sh`, wired into `scripts/ci/run-ci.sh` and the
  `guardrails` workflow, fails when a prose block on that surface reads as Spanish. What it reads
  and how the density decision is made are documented in
  [`CONTRIBUTING.md`](../../CONTRIBUTING.md#the-english-gate); its regression probe is
  `scripts/ci/test-language-check.sh`.
- **Publication gate.** `tools/public-export/export-public.py` re-checks the exported tree and
  stays the last barrier: a file that reaches it in Spanish is a defect even when the CI gate
  passed on the private tree.

A line the gate reports is fixed by writing that prose in English, not by shipping a translation
next to it.

## Naming convention

- A translation lives **next to** its canonical file, with the language tag before the extension:
  `README.md` → `README.es.md`; `docs/i18n/POLICY.md` → `docs/i18n/POLICY.es.md`;
  `docs/acknowledgments/status.md` → `docs/acknowledgments/status.es.md`. Translations are
  **local-only**: they are never linked from a published file and never enter the export.
- Language tags are ISO 639-1 (`es`, and `de`, `fr`, `pt-BR` if they ever exist). No regional
  variants unless a real contributor insists on one.
- **No mirrored directory tree** (`docs/es/`, `docs/de/`) and no bilingual files
  (`README.es-en.md`, side-by-side documents). A mirrored tree duplicates navigation and rots
  silently; side-by-side files are unreviewable in a diff.

## When a translation is produced

- **On demand.** When a reader actually needs it, or when a contributor offers it. Not preemptively,
  not as a batch.
- **Never in bulk, never machine-only.** A published translation must be readable prose: commands,
  probe identifiers, paths, required numbers and code comments stay byte-identical to the canonical
  file (see [Review](#review-of-a-translation)). An unedited machine translation is not accepted.
- **Priority order** is recorded in [`STATUS.md`](STATUS.md): P0 (entry door — English, nothing
  pending), P1 (public-facing file still in Spanish — none left), P2 (nice to have; the engineering
  record is explicitly out of scope). Nothing is scheduled today.

## Who translates and who approves

- Any contributor may propose a translation as a pull request against `main`. The owner (or a
  maintainer acting for the owner) merges it.
- The translator is credited in the commit message (`translation: README.md → README.es.md (es)`), not
  by adding a name to the document unless they ask.
- Review requires someone able to read both languages. If no such reviewer is available, the
  translation is not merged: a stale translation nobody can verify is worse than no translation.

## Staleness: how it is marked

Every translated file starts with a machine-readable header, immediately after the title:

```html
<!-- translation: es | canonical: README.md | canonical-commit: 97f852e | translated-at: 2026-10-21 | status: current -->
```

- `canonical-commit` is the commit where the canonical file was last materially changed **when the
  translation was made**: `git log -1 --format=%h -- README.md`.
- If `git log -1 --format=%h -- <canonical>` no longer matches `canonical-commit`, the translation is
  **stale**. The staleness is recorded by setting `status: stale` — and the file keeps serving as a
  translation until it is updated, so the reader sees the marker rather than a 404.
- A stale translation is fixed by re-translating the changed hunks and bumping `canonical-commit` and
  `translated-at`; it is never "patched" into looking current while the canonical moved.
- If a translation is more than one milestone behind, it is deleted rather than kept as a misleading
  artifact.
- **There is no automated staleness check today**; it is enforced by the review of any pull request
  that touches a canonical file with a translation (the diff shows the header commit). A CI check is
  possible and deliberately not added yet: with a single translation it would cover one file, and the
  manual check is one `git log` away.

## Review of a translation

The diff is checked for these invariants, in this order:

1. **Not translated:** code blocks, commands, flags, paths, file names, crate/crate-member names,
   probe IDs (`PROBE-MAP-001`, `ALL-001`), commit hashes, numbers and units,
   error codes, and the contents of tables that are actual test results.
2. **Translated:** headings, prose, table headers and descriptive cells, link text.
3. **Links:** relative targets keep working from the translated file (same directory, so they are
   identical); where a translated counterpart exists for a linked document, the translated file links
   to the counterpart, otherwise to the canonical file.
4. **Claims:** no claim may be added, softened or strengthened in translation. If the canonical text
   contradicts the evidence, the fix belongs in the canonical file, in a separate change.

## Adding a translation — mechanical steps

1. Copy the canonical file to `<name>.<lang>.md`.
2. Translate the prose; leave items listed in [Review](#review-of-a-translation) untouched.
3. Add the header with `canonical`, `canonical-commit` (`git log -1 --format=%h -- <canonical>`),
   `translated-at` and `status: current`.
4. Add a row to [`STATUS.md`](STATUS.md) with language, priority and status, marked **local-only**.
5. Do **not** link it from the canonical file: the export ships English only, so a published link
   would advertise a file the reader cannot reach.

## Current state

Translations are written **by hand, on demand, without scripts**: files are translated one at a
time, claims unchanged, and no automated translator is used. One derived file exists today —
`DATA-CLASSIFICATION.es.md` of `DATA-CLASSIFICATION.md`, local-only — and nothing is stale.

The sweep condition is met at this snapshot: the CI gate of [Enforcement](#enforcement) reports
**0 Spanish prose blocks** on the exportable surface, and the code comments, Python docstrings and
manifest descriptions are English. The published markdown surface is English, the two gates keep it
that way, and the export gate re-measures the exported tree at publication. The inventory of
documents, their language, their priority and the measurement are in [`STATUS.md`](STATUS.md).

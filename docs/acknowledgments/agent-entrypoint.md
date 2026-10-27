# Agent entry point

> **Who this is for.** Anyone — human or autonomous agent — picking up this project without the
> conversation that produced the plan. It states where the truth lives, what is actually done, which
> gate comes next, what is forbidden, and how to continue without inventing scope.

## 1. Source of truth

| Question | Authoritative answer |
|:---|:---|
| What is planned, and at what cost? | **[Roadmap M18–M48 + E8](roadmap-m18-m48.md)** — the single canonical plan. It is the only document allowed to state sequencing or estimation for M18–M48 |
| Which milestone actually closed? | the **milestone tags** (`M<n>-YYYY-MM-DD`) and the probe suites in the repository — a closure without a tag is technical work, not an accepted milestone |
| What is the acceptance bar per milestone? | the plan's gate for that milestone (section 3 of the roadmap) plus the definition of done it cites |
| What does the game claim publicly? | the [README](../../README.md) and [Status](status.md); a claim there without a probe behind it is a bug in the documentation |
| Where is the historical record? | the repository history and the dated documents; **history is frozen**, it is not edited to look current |

**Rule of reading.** A state claim wins over a plan claim; a measurement wins over both. If two documents
disagree about a milestone's state, the one bound to a commit with a probe wins, and the other is
corrected rather than reconciled by prose.

## 2. Real state (measured)

| Milestone | State | What it means in practice |
|:---|:---|:---|
| **M18** | **PARTIAL** | technical closure passed, the 21 acceptance rows were measured, the **owner signature and tag are pending**. Not the same as DONE |
| **M19–M29** | **DONE** | 11 milestones, each with probes and a QA verdict. "DONE" here means technical closure at the declared bar, not a signed release |
| **M30** | **FROZEN** | economy and banking: a written specification, **no code**, frozen by owner decision |
| **M31–M33** | **SPEC-ONLY** | travel/weather/calendar, conditions/death/rest and total savegame: one written specification each, not implemented |
| **M34** | **DONE** (technical) | end-to-end integration and presentation: closed 2026-10-26 — `ALL-001` **40/40**, QA **0.94** (0 critical / 0 major), owner rows 12/12. No tag. **Closes Plan A** |
| **M35** | **PARTIAL** | biomes, 2D vegetation, sky — delivered 2026-10-26 for the `0.1.0` surface: `BIOME-001` 25 passed / 0 failed / **1 declared NA** (`GAP-TAX-11`: the corpus carries no tile-class data) and `SPRITE-001` 11/11. **Optional annex**, not a gate for Plan A |
| **M36–M37** | **DONE** (technical) | UI-0 audit and the seven-axis UI layer: closed 2026-10-27 — `UI-AUDIT-001` A+B+C `VERDICT: PASS` in two separate processes with identical stdout (**85 = 78 surfaces (27 + 51) + 7 infrastructure rows**, measured **4 implemented / 47 partial / 34 absent**, rule **35 PATCH · 50 REWRITE**) and `UI-BASE-001` **9/9** + `UI-INPUT-001` **10/10** `VERDICT: PASS` over **3 pilot windows**. No tag |
| **M38–M48** | **NOT STARTED** | Plan B, remaining (E1–E7). Specifications exist *in the plan*; no code, no dispatch, no tag |
| **E8** | **NOT STARTED, not estimable** | a **separate program** after M48, with its own genesis contract. It is not "M49" |

Public surface: tags up to **`v0.2.0-alpha`** only. No milestone tag exists for M18 or later.

## 3. Closure rule (the only one)

Every milestone and every work package closes the same way:

`probe PASS → printable demo/evidence → QA at or above the declared bar (0 critical / 0 major) →
commit with a named pathspec → closure record`

Two consequences that are frequently violated:

1. **A milestone with technical closure is not an accepted milestone.** Acceptance (signature, tag) is a
   separate act and belongs to the owner.
2. **The plan is not a checklist of intentions.** A work package without a named probe does not exist.

**Plan A vs Plan B cut.** Plan A closes at **M34 PASS** (49.95 u strict) — measured on 2026-10-26, so
Plan A is **delivered at technical level**; the owner acceptance (signature, tag) that would make it an
*accepted* milestone is still pending, and no tag exists. **M35** is an optional, non-blocking annex
delivered for the `0.1.0` surface with one declared `NA`; it is not a gate for closing Plan A. The
multiplier base (51.85 u) includes M35 for continuity of the historical series, and the roadmap declares
both figures so the two readings never merge.

## 4. Next gate — M38, and only with an explicit go

The next gate is **M38 (UI-2)**: the 16 creation windows, the 11 configuration pages and the menu family,
built on the layer M37 landed. It requires an explicit go from the owner before any dispatch, with its
requirements addendum. Absence of a go is a hard stop, not an invitation.

**M36 and M37 are closed at technical level (2026-10-27), with no tag.** M36 produced what used to be the
blocker: the executable census of the 85 original windows with a measured state per row (`UI-AUDIT-001`,
blocks A+B+C, `VERDICT: PASS`), the 85×9 fit matrix, the patch-vs-rewrite decision (35 PATCH ·
50 REWRITE), the UI↔game compatibility contract (seven axes) and the E1 re-estimate (21.81–49.95 u).
M37 turned that contract into code — `crates/parser-bevy-bridge/src/ui_compat/` — exercised by 3 pilot
windows under `UI-BASE-001` (9/9) and `UI-INPUT-001` (10/10), `VERDICT: PASS` with byte-identical stdout
in two processes. Nothing in M36–M37 remains dispatched, and neither carries a tag.

## 5. Limits (hard)

| Limit | Statement |
|:---|:---|
| **No code without a go** | Plan B is a written plan. Writing product code for M36–M48 before the milestone's go violates the plan |
| **No retail assets** | No original game data, textures, sounds, fonts or executables in the repository, in any form. The engine is **BYOA**: the user points it at their own copy. Content scans must stay at zero |
| **No secrets** | No keys, tokens or credentials in the repository, in tests, in fixtures or in documentation |
| **No new milestone** | The ending is **M48 + E8**. E8 is a separate program with a new genesis contract — never "M49", never "M48b", never "phase 2" |
| **No dates** | `u` is volume, not time. Do not convert it into days, weeks or months, and do not promise a "v1.0" date |
| **No silent parity claim** | Nothing may be described as "visually identical", "feature complete" or "release quality" without the probe and the signature that back it |
| **No closure by prose** | A milestone closes with its gate and its record, never with a status sentence |

## 6. Routes — where things live

| Need | Where |
|:---|:---|
| the plan and its gates | [Roadmap M18–M48 + E8](roadmap-m18-m48.md) |
| stage-by-stage public state | [Roadmap](roadmap.md) and [Status](status.md) |
| how to build and run, and the data policy | [README](../../README.md) and [Bring your own data](../../README.md#bring-your-own-data-byoa) |
| architecture and crate boundaries | [Architecture](../../ARCHITECTURE.md) |
| contribution rules and the English gate | [Contributing](../../CONTRIBUTING.md) |
| data classification and legal posture | [Data classification](../../DATA-CLASSIFICATION.md) |
| upstream reference corpus (MIT) | `reference/dfu-spec/` |

## 7. How to continue

1. **Read the plan, not the chat.** Section 2 tells you the state; section 3 tells you the work; section 5
   tells you the dependencies; section 10 tells you what a closure means.
2. **Pick the next unblocked gate, never a random milestone.** The critical path is depth → dungeon →
   online → platform → modern; the UI runs in parallel. The audit (M36) and the layer (M37) are closed,
   so **M38** is the next unblocked window gate.
3. **Write the specification with golden vectors before the code.** A stretch without golden vectors is
   where estimates are least reliable (see the risk factors in section 6.3 of the plan).
4. **Name the probe in the first commit.** If you cannot name the probe, you do not yet know what you are
   building.
5. **Declare residuals instead of hiding them.** A declared gap with an owner is the plan working.
6. **Do not rewrite history.** The plan supersedes earlier documents as *planning*; their measured
   history stays intact.
7. **When measurement contradicts the plan, the measurement wins** — re-estimate the next milestone and
   record the delta. Never adjust the band to fit the sum.

---

*Public, sanitized edition of the canonical roadmap. Documentation only: no code, no milestone tag, no
claim of delivery. Status as measured at the publication date of this document; the plan's internal
engineering record holds the per-row evidence behind each figure.*



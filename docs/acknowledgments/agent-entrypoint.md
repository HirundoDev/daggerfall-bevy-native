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
| **M18** | **DONE** at technical level · `PARTIAL-TAG-PENDING` | gate rows and the acceptance signature exist; the release tag does not. The 21 acceptance rows are **signed by delegation on 2026-10-26** — **8 PASS · 9 NC-ACEPTA · 4 NOT-COMPARABLE · 0 FAIL** (`NOT-COMPARABLE` is a declared class, not a downgraded pass). The pending row is `T-RELEASE-TAG`, the tag is **none**, and the comparison against the *original* game is still the owner's own act. Not the same as "looks like 1996" |
| **M19–M33** | **DONE** | 15 milestones, each with probes and a QA verdict. "DONE" here means technical closure at the declared bar, not a signed release |
| **M30–M33** | **DONE** (technical) | economy and banking (M30, closed 2026-10-23), travel with weather and calendar (M31, 2026-10-23), conditions/death/rest (M32, 2026-10-23) and total persistence (M33, 2026-10-26), each with its organ in `crates/` and its probes. Their consumer today is the probe, not the demo window. No tags |
| **M34** | **DONE** at technical level · `PARTIAL-TECHNICAL-CLOSURE` | end-to-end integration and presentation: closed 2026-10-26 — `ALL-001` **40/40**, QA **0.94** (0 critical / 0 major), owner rows 12/12. One named row stays open: **`AUDIO-RT-001-DEVICE`** — `AUDIO-RT-001` reproduces `PASS(degraded)` (`[SKIP-DEVICE]`, binary built without the `audio` feature), so the device branch of the audio is the owner's decision. `UI-FULL-001` counts the **34** non-playable surfaces as `N/A-engine` entries of its own census and measures only the **51** playable windows (**51/51**): its `PASS` neither depends on those 34 nor certifies them, nor the device branch. No tag |
| **M35** | **PARTIAL** | biomes, 2D vegetation, sky — delivered 2026-10-26 for the `0.1.0` surface: `BIOME-001` 25 passed / 0 failed / **1 declared NA** (`GAP-TAX-11`: the corpus carries no tile-class data) and `SPRITE-001` 11/11. The declared gap is **deferred to M44**. **Optional annex**, not a gate for Plan A |
| **M36–M38** | **DONE** (technical) | UI-0 audit, the seven-axis UI layer and the 27 creation/configuration surfaces: closed 2026-10-27 — `UI-AUDIT-001` A+B+C `VERDICT: PASS` in two separate processes with identical stdout (**85 = 78 surfaces (27 + 51) + 7 infrastructure rows**, measured **4 implemented / 47 partial / 34 absent**, rule **35 PATCH · 50 REWRITE**), `UI-BASE-001` **9/9** + `UI-INPUT-001` **10/10** over **3 pilot windows**, and `UI-CREATE-001` (16 surfaces) + `UI-CONFIG-001` (11 surfaces) `VERDICT: PASS` in two processes with identical digests. No tag |
| **M39** | **DONE** at gate level | the 51 interactive playable windows: technical closure measured at HEAD `5803297` — `UI-JUGABLE-001` passes its **9/9** gate rules in two processes with byte-identical stdout (51/51 surfaces, 8/8 blocks, 7/7 axes; AXIS A5 `focus-rows=153` ×2, 4/4 falsifiers red) — and the closure report at `e249004` publishes `requirements_coverage 0.857` with **K7** (owner side-by-side) as the single open clause. Not signed, no tag, and the windows are not yet the running demo window |
| **M40–M48** | **AUSENTE** | Plan B, remaining (E1–E7). Specifications exist *in the plan*; no code, no dispatch, no tag. M46 also carries `T-M46-GO` (an explicit owner go) |
| **E8** | **AUSENTE** | a **separate program** after M48, with its own genesis contract. It is not "M49" |

Public surface: tags up to **`v0.3.0-alpha`** (published 2026-10-27), with `v0.3.1-alpha` in
preparation as a documentation correction. No milestone tag exists from M18 on; M18's acceptance
signature is recorded by delegation on 2026-10-26.

## 3. Closure rule (the only one)

Every milestone and every work package closes the same way:

`probe PASS → printable demo/evidence → QA at or above the declared bar (0 critical / 0 major) →
commit with a named pathspec → closure record`

Two consequences that are frequently violated:

1. **A milestone with technical closure is not an accepted milestone.** Acceptance (signature, tag) is a
   separate act and belongs to the owner.
2. **The plan is not a checklist of intentions.** A work package without a named probe does not exist.

**Plan A vs Plan B cut.** Plan A closes when M34's outstanding row **`AUDIO-RT-001-DEVICE`** passes
(49.95 u strict) — measured on 2026-10-26, so Plan A is **delivered at technical level except that
row**: M34 is `PARTIAL-TECHNICAL-CLOSURE` today, and the owner acceptance (signature, tag) that would
make it an *accepted* milestone is still pending, and no tag exists. **M35** is an optional,
non-blocking annex delivered for the `0.1.0` surface with one declared `NA` (`GAP-TAX-11`, deferred to
M44); it is not a gate for closing Plan A. The
multiplier base (51.85 u) includes M35 for continuity of the historical series, and the roadmap declares
both figures so the two readings never merge.

## 4. The next gate — M40, and only with an explicit go

**M36, M37, M38 and M39 are closed at gate level (2026-10-27), with no tag.** M36 produced what used to
be the blocker: the executable census of the 85 original windows with a measured state per row
(`UI-AUDIT-001`, blocks A+B+C, `VERDICT: PASS`), the 85×9 fit matrix, the patch-vs-rewrite decision
(35 PATCH · 50 REWRITE), the UI↔game compatibility contract (seven axes) and the E1 re-estimate
(21.81–49.95 u). M37 turned that contract into code — `crates/parser-bevy-bridge/src/ui_compat/` —
exercised by 3 pilot windows under `UI-BASE-001` (9/9) and `UI-INPUT-001` (10/10), `VERDICT: PASS` with
byte-identical stdout in two processes. M38 (UI-2) delivered the 16 creation windows and the 11
configuration pages, measured by `UI-CREATE-001` (16 surfaces, 12 checks) and `UI-CONFIG-001` (11
surfaces, 12 checks) — `VERDICT: PASS`, exit 0, two processes, identical digests — with requirement
coverage **0.857 (6/7)**: **K7** (owner side-by-side acceptance) is the only open clause — **K2** (the
composed `FACES.CIF` face) is closed, with its handoff recording `status: DONE` — so the surfaces are
measured, not accepted, and not yet composed into the running demo window. Nothing in M36–M38 remains
dispatched, and none carries a tag.

**M39 (UI-3) is `DONE` at gate level and is not signed.** The surface registry, the block grammar, the
A–H runner and probe are in the tree since `85979b9`: the runner replays the blocks in two processes,
re-hashes the 51 captures and drives the three falsifiers, and `UI-JUGABLE-001` prints `VERDICT: PASS`
on its 9 gate rules (9/9 PASS ×2, AXIS A5 `focus-rows=153` ×2, 4/4 falsifiers red, at HEAD `5803297`).
The closure report — measured at `e249004`, kept at
docs/sintergic/l5-mission/m39-ui3-mission-report-CR-aecd1cef.md — publishes `requirements_coverage
0.857`, with K7 as the single open clause. What remains is the owner's K7 signature and the tag, not a
report. Absence of a go is a hard stop, not an invitation, and an unsigned gate is not an accepted
milestone.

**M40 — the UI gate at 85/85 with side-by-side, input and accessibility — is the next gate, and only
with an explicit go.** No M40 work is dispatched today.

## 5. Limits (hard)

| Limit | Statement |
|:---|:---|
| **No code without a go** | Plan B is a written plan. M40–M48 are `AUSENTE`: writing product code for any of them before the milestone's go violates the plan |
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
   online → platform → modern; the UI runs in parallel. The audit (M36), the layer (M37), the 27
   surfaces (M38) and the 51 playable windows (M39, technical closure at coverage 0.857) are closed, so
   **M40** (the UI gate: 85/85 with side-by-side, input and accessibility) is the next gate; M39's only
   remainder is the owner's signature.
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



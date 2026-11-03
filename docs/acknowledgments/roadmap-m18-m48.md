# Roadmap — M18 to M48, plus E8

> Commit shas on this page name commits of the private engineering history unless the object
> resolves in your clone (`git cat-file -t <sha>`); the published history is grafted, so a
> source-tree sha does not resolve here.

> **What this is.** The complete, single plan of the program up to its declared ending:
> **Plan A** (M18–M35 — foundation and vertical slice: M18 DONE at technical level, verdict
> `PARTIAL-TAG-PENDING`, signed by delegation on 2026-10-26 with the comparison against the original and
> the tag still the owner's; M19–M33 DONE at gate level, acceptance PENDING and no tags; M34 DONE at
> technical level, verdict `PARTIAL-TECHNICAL-CLOSURE` with the named row `AUDIO-RT-001-DEVICE`;
> M35 `PARTIAL`, with `GAP-TAX-11` deferred to M44) plus **Plan B**
> (M36–M48 — the epics E1–E7 turned into milestones with an atomic work breakdown; M36–M42 DONE with no
> tags, M43 in flight, M44 next and M45–M48 `AUSENTE`) plus **`M-SHELL`** (the application and the session
> — the 13 capabilities the coverage matrix of 2026-10-29 measures as unreachable from the binary,
> 17.00 u, `PLANNED`) plus **`M-PLAY`** (the closing milestone
> that connects the whole game, gated by the complete playthrough probe) plus
> **E8** (a deferred expansion horizon, a **separate program** after M48, never "M49", `AUSENTE`).
>
> **What this is not.** It implements nothing, signs no milestone, creates no tag, promises no
> dates and rewrites no history. Every milestone below is described in the present tense of a
> plan, not of a delivery.
>
> **Unit of work.** `u` is the program's volume unit, on the same scale as the Plan A records.
> It is **not** a duration and it is never converted into a date. `u` figures carry a range;
> the nominal is the centre, the high bound is the realistic scenario for a stretch without
> golden references.

**Status legend.** `DONE` = probe passed and QA at or above its bar (0 critical / 0 major); in public prose
it means **gate-level technical closure**, never owner acceptance.
`PARTIAL` = pieces delivered, closure criterion unmet. `PARTIAL-TAG-PENDING` = technical closure reached and
acceptance signature recorded, annotated tag outstanding. `PARTIAL-TECHNICAL-CLOSURE` = technical closure
reached on every row that can be measured, with one named row still open. `AUSENTE` = no code, no dispatch,
no tag. These are the closure register's own verdicts (checked by `scripts/closure-gate.py` →
`VERDICT: PASS`, 0 prose↔register contradictions and falsifiers 3/3 red).

---

## 0. Status at a glance

| Block | Milestones | State |
|:---|:---|:---|
| **Active path — sequence in force (2026-10-29)** | **M43 → M44 → M-SHELL → M-PLAY** | **M43 (SYS-3) in flight** under `CR-39d656e1` (register verdict still `AUSENTE`; no dispatch of M44 until it closes) → **M44 (DUN-1) next**, whole and unblocked: the dungeons of the original, content and state (layers A/B/C) → **M-SHELL**, the application and the session: the 13 capabilities (boot, loading screen, menu, new game, character, world load, session, slots, HUD, map, fast travel, pause/options, exit) that the coverage matrix measures as **not reachable from the binary** (17.00 u, §3.9b; `PLANNED`) → **M-PLAY**, the milestone that connects the whole game and whose only acceptance condition is the complete playthrough probe of the playthrough probe contract (`P1..P9`, `P5` = a real dungeon of the original ⇒ hard dependency on M44; `P1`/`P8`/`P9` observe the shell ⇒ hard dependency on M-SHELL) |
| **Blocked behind M-PLAY** | **M45, M46, M48 (`DEFERRED`)** and **M47 (`OPTIONAL`)** | M45, M46, M48 y M47 no se abren hasta que M-PLAY pase su probe de partida completa de punta a punta; M44 no se difiere ni se bloquea. |
| DONE at gate level | M19–M33 (15 milestones) | **DONE** (each with its probes and QA score; acceptance PENDING, no tags) |
| DONE at technical level, tag pending | M18 | **DONE at technical level · `PARTIAL-TAG-PENDING`** (acceptance signature recorded 2026-10-26 by delegation: 8 PASS · 9 NC-ACEPTA · 4 NOT-COMPARABLE · 0 FAIL; row `T-RELEASE-TAG` pending; the comparison against the original and the tag remain the owner's) |
| DONE at technical level, one named row open | M34 | **DONE at technical level · `PARTIAL-TECHNICAL-CLOSURE`** (`AUDIO-RT-001-DEVICE`: `AUDIO-RT-001` reproduces `PASS(degraded)` with `[SKIP-DEVICE]` in a binary built without the `audio` feature, so the device branch is not promoted — owner decision on the audio branch) |
| PARTIAL | M35 (annex) | **PARTIAL** (`BIOME-001` 25/25 + 1 NA: `GAP-TAX-11`, deferred to M44; `SPRITE-001` 11/11) |
| DONE at gate level | M36–M40 (5 milestones) | **DONE** (technical closure 2026-10-27; M39 coverage 0.857 with K7 open; M40 `UI-FULL-001` 85/85 on 8/8 rules; acceptance PENDING, no tags) |
| `AUSENTE` | M43–M48 + E8 (7 milestones) | **AUSENTE** (no code, no dispatch, no tag; M43 is the one **in flight** and M46 carries `T-M46-GO`; M45/M46/M48 are `DEFERRED`, M47 `OPTIONAL`) |
| Separate program | E8 | **AUSENTE** (separate program with its own genesis contract; never "M49") |

Recount: **41 DONE · 1 PARTIAL · 1 PARTIAL-TAG-PENDING · 1 PARTIAL-TECHNICAL-CLOSURE · 7 AUSENTE**
(51 milestones, 100 gate rows; `program_closure: OPEN`, `closure-gate.py` → `VERDICT: PASS`).
**Corrected 2026-10-29 (`CR-359b5d8e`):** `EXPECTED_IDS` was extended, so `M-SHELL` (section 3.9b) and
`M-PLAY` (section 3.9a) **are** rows of the register — both `AUSENTE`, with their gates in `missing_rows`
— and the register is now **53 milestones · 107 rows**; its single recount includes them. The gate's
verdict today is `FAIL` with 19 errors, **all** in `M43` rows written by another CR (`CR-1b68cf36`) —
none attributable to this extension.
M19–M33 are DONE at gate level — acceptance PENDING, no tags — and M30–M33 closed at technical level
between 2026-10-23 and 2026-10-26 (probe-backed organs; the probe, not the demo window, is their consumer).
M34 is DONE at technical level on 2026-10-26 (`ALL-001`
**40/40**, QA **0.94** with 0 critical / 0 major, owner rows 12/12), with verdict
`PARTIAL-TECHNICAL-CLOSURE` and the named row `AUDIO-RT-001-DEVICE` still open; M35 was delivered the same
day with one declared `NA` (`GAP-TAX-11`, deferred to M44) and stays `PARTIAL`. M36, M37 and M38 closed at technical level on 2026-10-27: `UI-AUDIT-001` blocks A+B+C
print `VERDICT: PASS` in two separate processes with identical stdout (`85 = 78 surfaces + 7
infrastructure`, measured **4 / 47 / 34**), `UI-BASE-001` **9/9** plus `UI-INPUT-001` **10/10** print
`VERDICT: PASS` with byte-identical stdout in two processes, and `UI-CREATE-001` (16 surfaces) plus
`UI-CONFIG-001` (11 surfaces) print `VERDICT: PASS` in two processes with identical digests. M18's 21
acceptance rows were signed by delegation on 2026-10-26 (0 FAIL: 8 PASS / 9 NC-ACEPTA / 4
NOT-COMPARABLE); M39 is DONE at gate level, coverage 0.857 — its A–H runner and probe are
in the tree since `85979b9`, `UI-JUGABLE-001` passes its 8 rules in two processes, and the closure
report is measured at `e249004` — with K7 the single open clause. No milestone tag exists from M18 on.

---

## 1. What "100%" means — four definitions that are never mixed

The original defect in the definition of done was reading "100% of the data DoD" as "100% of the
game"; the symmetric error would be selling "100% original" as "the full vision". This roadmap keeps
them apart.

### 1.1 The four definitions

| Definition | Milestones | What it includes | Size |
|:---|:---|:---|:---|
| **Plan A** (foundation + slice) — **planned scope, not delivery** | **M18–M35** | retail data, 49 systems at slice depth, presentation UI, E2E and save. State today: **M18 DONE at technical level (`PARTIAL-TAG-PENDING`; acceptance signature recorded 2026-10-26 by delegation) · M19–M33 DONE at gate level · M34 DONE at technical level (`PARTIAL-TECHNICAL-CLOSURE`, row `AUDIO-RT-001-DEVICE` open) · M35 PARTIAL (`GAP-TAX-11` deferred to M44)** | **51.85 u** with the M35 annex · **49.95 u** without it |
| **100% original** | **M36–M44** | full UI (E1) + real system depth (E2) + dungeon content (E3) | **2.8–5.6×** Plan A |
| **Retail-ready** | **M36–M44 + M47** | 100% original + platform and distribution (E6) | **3.0–6.0×** |
| **Full vision** | **M36–M48** | E1–E7 | **4.9–9.8×** |
| **E8** (deferred expansion horizon) | **E8** | new program, new genesis contract | **not estimable** |

**`M-SHELL` and `M-PLAY` are charged outside the E1–E7 multiply.** The four definitions above are the
Plan A records' envelopes for the epics E1–E7 and they are **not** re-cut by this re-baseline. `M-PLAY` (section 3.9a)
is a **closing milestone in its own right** — the fix set that connects what is already built plus the
instrument that proves it — and its `u` enters the **active path** totals of §3.15 as a separate line.
`M-SHELL` (section 3.9b) is the same kind of line for the same reason: it is the **composition layer**
over surfaces already priced inside E1 (the 16 creation windows and 11 configuration pages of M38, the
startup/menu/save windows of M39 block A, the M33 save, the M34 map), so charging it inside the
multiplier would count those surfaces twice. `100% original` therefore reads, today: **M36–M44**
(the E-envelope, 219.00 u nominal) **+ M-PLAY**, and the composition layer **M-SHELL** (17.00 u) on the
same footing; the multipliers of §1.2 stay untouched because no new epic and no new multiplier is
invented here.

### 1.2 Multiplier arithmetic (declared base)

Base **B = 51.85 u** = the cost of the M18–M35 program **including** the M35 annex. The multipliers are
the ones fixed by the Plan A records: `E1 0.3–0.6×` · `E2 2–4×` · `E3 0.5–1×` · `E4 0.5–1×` ·
`E5 1–2×` · `E6 0.2–0.4×` · `E7 0.4–0.8×`.

- `100% original` = E1+E2+E3 = **2.8–5.6×** ⇒ **146.40–290.55 u** (nominal 219.00).
- `Retail-ready` = +E6 ⇒ **156.40–311.55 u** = **3.02–6.01×**, declared **3.0–6.0×**.
- `Full vision` = E1..E7 ⇒ **253.40–508.55 u** = **4.89–9.81×**, declared **4.9–9.8×**.

### 1.3 Granularity: M36–M48 are epic-sized milestones

Plan A has **18 rows** and **51.85 u**, i.e. **2.88 u per row**. Keeping that granularity, Plan B would
be **50–101 milestones** for 100% original and **88–177** for the full vision. **M36–M48 are therefore
epic-sized and variable** (2.0 u the smallest, 78.0 u the largest), not 2 u modules. A reader who counts
"13 modules" and expects the pace of M19–M29 is off by an order of magnitude. `M-PLAY` sits **on top** of
the 13 epics as a closing milestone of the same tramo (section 3.9a): it is the only one whose size is
**provisional by construction**, because it is quoted from the open defect inventory rather than from an
E-envelope.

### 1.4 Measured corrections of premise

Five status claims circulated before this plan; each is kept as intent and corrected where measurement
says otherwise. None of them reduces scope; all of them prevent a false gate.

| # | Premise | Measured | Consequence |
|:-:|:---|:---|:---|
| **C-1** | "M18–M32 are DONE" | **M18 DONE at technical level · `PARTIAL-TAG-PENDING`** (acceptance signature recorded 2026-10-26 by delegation — 8 PASS · 9 NC-ACEPTA · 4 NOT-COMPARABLE · 0 FAIL; the comparison against the original and the tag are the owner's); **M19–M33 DONE at gate level** (15 milestones, acceptance PENDING, no tags); **M34 DONE at technical level · `PARTIAL-TECHNICAL-CLOSURE`** (named row `AUDIO-RT-001-DEVICE`); **M35 PARTIAL** (`GAP-TAX-11` deferred to M44); **M36–M40 DONE at gate level** (technical closure 2026-10-27; M39 coverage 0.857, K7 open; M40 `UI-FULL-001` 85/85; acceptance PENDING, no tags); **M41–M48 not started** (no code, no dispatch, no tag); **E8 a separate program, not started** | the matrix marks M18 `DONE` with its signature source; Plan B does **not** depend on any signature |
| **C-2** | "UI is 51/85 (60%) delivered" | 51/85 is a census ratio (playable windows within the 85), not a delivery. Delivered today: **6 static panels** and a shell with a registry and a stack | `UI-AUDIT-001` (M36, 2026-10-27) measured the census row by row: **4 implemented / 47 partial / 34 absent**; the M40 gate is per **interactive flow**, not per "panel present". M38 (27 surfaces), M39 (51) and M40 (the gate over all 85) have closed at gate level since, so those 34 rows are certified — 16 + 11 with per-surface evidence, 7 by layer contract — not pending |
| **C-3** | "34 are missing" | 34 = 16 character-creation + 11 configuration + 7 infrastructure | **M38 reverses part of that `N/A`**: the 11 configuration pages enter with **functional** parity (not pixel). The 7 infrastructure entries stay out: they are base classes, factories and constants, not windows |
| **C-4** | "Gate: 85/85 interactive windows" | 7 of the 85 are not windows | the gate is stated exactly: **78/78 interactive windows + 7/7 classified infrastructure + 0 rows without a verdict = 85/85 resolved**. Measured 2026-10-27: 85/85 surfaces with per-surface evidence, 0 rows without a verdict. |
| **C-5** | "Dialogue only in trade, no exterior–interior transition" | the dialogue **UI consumer** does not exist; the dialogue **system** does (M26 DONE); the exterior→interior transition is measured | the premise describes the public `0.1.0` surface; M36 treats it as a hypothesis to measure, not a fact of HEAD |
| **C-6** | "13.1 u/day" | correct as a measured reference for work **with golden references** | used only as an order of magnitude; **zero dates** |

---

## 2. Canonical matrix — M18 to M48 plus E8

| M | Epic | Scope | State | `u` remaining |
|:-:|:---|:---|:---|--:|
| **M18** | slice | presentation parity + map closure | **DONE at technical level · `PARTIAL-TAG-PENDING`** — acceptance signature recorded 2026-10-26 by delegation; original-side comparison and annotated tag pending (`T-RELEASE-TAG`) | 5.60 |
| **M19** | slice | locomotion and body | **DONE** | 0 |
| **M20** | slice | character and progression | **DONE** | 0 |
| **M21** | slice | items, equipment, loot | **DONE** | 0 |
| **M22** | slice | combat | **DONE** | 0 |
| **M23** | slice | magic | **DONE** | 0 |
| **M24** | slice | enchanting and alchemy | **DONE** | 0 |
| **M25** | slice | monsters and encounters | **DONE** | 0 |
| **M26** | slice | living NPCs and dialogue | **DONE** | 0 |
| **M27** | slice | guilds and factions | **DONE** | 0 |
| **M28** | slice | crime, law and courts | **DONE** | 0 |
| **M29** | slice | playable quests | **DONE** | 0 |
| **M30** | slice | economy and banking | **DONE** (technical closure 2026-10-23, no tag) | — |
| **M31** | slice | travel, weather, calendar | **DONE** (technical closure 2026-10-23, no tag) | — |
| **M32** | slice | conditions, death, rest | **DONE** (technical closure 2026-10-23, no tag) | — |
| **M33** | slice | total persistence (save v2) | **DONE** (technical closure 2026-10-26, no tag) | 1.65 |
| **M34** | slice | end-to-end integration + presentation | **DONE at technical level · `PARTIAL-TECHNICAL-CLOSURE`** (technical closure 2026-10-26, no tag; named row `AUDIO-RT-001-DEVICE` open — owner decision on the audio branch) | 5.85 |
| **M35** | slice (annex) | biomes, 2D vegetation, sky | **PARTIAL** (delivered; 1 NA declared `GAP-TAX-11`, deferred to M44; non-blocking, `0.1.0`) | 1.90 |
| **M36** | **UI-0** | audit of the 85 windows + patch/rewrite decision + compatibility contract + E1 re-estimate | **DONE** (technical closure 2026-10-27, no tag) | 2.00 |
| **M37** | **UI-1** | UI compatibility layer (7 axes) | **DONE** (technical closure 2026-10-27, no tag) | 4.50 |
| **M38** | **UI-2** | 16 creation + 11 configuration + menus (27) | **DONE** (technical closure 2026-10-27, no tag) | 6.50 |
| **M39** | **UI-3** | 51 interactive playable windows | **DONE at gate level** (technical closure 2026-10-27; coverage 0.857, K7 open; no tag) | 7.50 |
| **M40** | **UI-4** | UI gate 78/78 + side-by-side + input + accessibility | **DONE at gate level** (technical closure 2026-10-27; `UI-FULL-001` 85/85, 8/8 rules; side-by-side = owner's K7; no tag) | 3.50 |
| **M41** | **SYS-1** | depth: combat, magic, items, stats, resistances | **DONE** | 63.00 |
| **M42** | **SYS-2** | depth: NPCs, guilds, crime and law, quests | **DONE** (technical closure, gate-rows; 8 GAP-M42 cerrados con evidencia re-ejecutada; acceptance y tag = owner) | 42.00 |
| **M43** | **SYS-3** | depth: economy, travel, weather, calendar, conditions | **AUSENTE** — **in flight** under `CR-39d656e1`; first of the sequence, no tag | 51.00 |
| **M44** | **DUN-1** | dungeon content and state: loot/respawn/bosses/instances (the geometry is M34's, measured by `PROBE-DUNGEON-001`) | **AUSENTE** — **next** in the sequence, whole and unblocked; its close is the hard precondition of `M-PLAY` (`P5`) and it **precedes `M-SHELL`**: `W44-5` writes save **v3**, the format the save slots are built on | 39.00 |
| **M-SHELL** | **SHELL** | the application and the session — the 13 capabilities of the capability census: boot, loading screen, main menu, new game, character creation/selection, world load, session/saved game, save/load slots, HUD, map, fast travel, pause/options, exit | **PLANNED** — gate: `SHELL-001` (the composed sequence, 13 rows × 2 processes with byte-identical stdout + 13/13 negative controls) plus `SHELL-002..006` per work package; not a row of the closure register (G-N13) | 17.00 (12.92–21.93) |
| **M-PLAY** | **PLAY** | connect the whole game: doors and transitions, world↔building↔dungeon, combat↔inventory↔stats, save/load, UI hooks — the work list is `the private engineering recordplayability/FUNCTIONAL-GAPS.md` | **PLANNED** — gate: `PLAYTHROUGH-PROBE` (`P1..P9`) of the playthrough probe contract; not a row of the closure register (the register's `EXPECTED_IDS` is fixed M0..M48 + E8, so a machine row here would be `SCHEMA-IDS`) | 6.50 (**provisional**, 3.90–9.10) |
| **M45** | **E4 MODS** | declarative mod pipeline | **`DEFERRED`** — register verdict stays `AUSENTE`; reason: width (mods) without moving functional parity of the original; blocked behind `M-PLAY` | 39.00 |
| **M46** | **E5 ONLINE** | optional online: lobby, instances, authority | **`DEFERRED`** — register verdict stays `AUSENTE` with `blocker: OWNER-GO`; reason: width (online) without moving functional parity; blocked behind `M-PLAY` | 78.00 |
| **M47** | **E6 PLATFORM** | packaging, multi-OS CI, sustained perf, accessibility | **`OPTIONAL`** — register verdict stays `AUSENTE`; entry condition: the `M-PLAY` probe passes end to end **and** the owner asks for distribution (gate `PLATFORM-001..003`); blocked behind `M-PLAY` | 15.50 |
| **M48** | **E7 MODERN** | opt-in modern layer (pristine/modern) | **`DEFERRED`** — register verdict stays `AUSENTE`; reason: width (modern render) without moving functional parity; blocked behind `M-PLAY` | 31.00 |
| **E8** | **deferred expansion horizon** | separate program, **after M48**, new genesis contract | **AUSENTE / NOT ESTIMABLE** | — |

**Reading note.** The 31 rows M18–M48 are the program; **`M-SHELL` is the 32nd row and `M-PLAY` the 33rd,
and both are register rows since 2026-10-29** (`CR-359b5d8e` extended `EXPECTED_IDS`; G-N12/G-N13 no
longer apply — the register carries them as `AUSENTE` with their gates in `missing_rows`):
`M-SHELL` is the application/session layer (section 3.9b) and `M-PLAY` the closing milestone of the tramo
(section 3.9a), inserted as `M44 → M-SHELL → M-PLAY` before M45. **E8 is not row 34** and not an "M49": it is a
program with its own genesis contract, its own definition of done and its own budget. The `u` buckets of
the re-baseline of 2026-10-29 are: **active 113.50** (`M43 + M44 + M-SHELL + M-PLAY`), **deferred 148.00**
(`M45 + M46 + M48`), **optional 15.50** (`M47`) — §3.15 has the arithmetic.

**Closed slice block.** M30–M33 closed at technical level — economy and banking on 2026-10-23, travel
with weather and calendar on 2026-10-23, conditions/death/rest on 2026-10-23 and the total savegame on
2026-10-26 — each with its module in `crates/`, its probe and its closure recorded; M34 and M35 are
delivered and their state is the one in §0, with M34's named row `AUDIO-RT-001-DEVICE` and M35's
`GAP-TAX-11` still open. Their consumer today is the probe: none of the four is
surfaced in the running demo window yet — that surfacing belongs to the M39/M40 stretch, and M39's
runner and probe are in the tree since `85979b9` with its gate DONE at gate level. `—` in the `u`
column means the remaining envelope of that block was not re-measured in this edition; the Plan B
figures in §3 stand.

---

## 3. Atomic work breakdown for Plan B

**Closure rule for every work package (inherited):** `probe PASS → printable demo/evidence → QA at or
above its bar (0 critical / 0 major) → commit with a named pathspec → closure record`.
A work package without a probe is not a work package.

> Plan A is **not duplicated** here. Its atomic work breakdown stays the reference for M18's rows and for
> M33 while it is open (M34 and M35 are closed at technical level); this document cites it and only
> summarises it in sections 2 and 6.

### 3.1 M36 — UI-0: measure before touching anything (2.00 u; range 1.60–2.40)

**Goal:** replace estimation with measurement. An executable census of the 85 windows, a fit matrix
against the real substrate (Bevy 0.20 + ECS + the UI shell + the M33 save v2), a **patch vs rewrite**
decision with a falsifiable rule, a UI↔game compatibility contract, and a **re-estimate of E1**.

| WP | Deliverable | Exact acceptance | Probe / gate | Deps | u |
|:--|:---|:---|:---|:---|--:|
| **W36-1** | Executable census of the **85** windows: name, base class, category (16/11/7/51), read model, assets, measured state, system it depends on | 85/85 rows with a verdict, no `unmeasured` cells; exact sum 16+11+7+51=85 | `UI-AUDIT-001` block A | — | 0.55 |
| **W36-2** | 85×9 fit matrix against the substrate: focus, typed modal outcome, scrolling lists, text input, drag and drop, paper doll/portraits, sprite animation, pointer, gamepad | every cell `fits` / `fits-with-extension` / `does-not-fit` with substrate evidence | `UI-AUDIT-001` block B | W36-1 | 0.45 |
| **W36-3** | **PATCH \| REWRITE** decision with a falsifiable rule: PATCH if ≥70% of nodes and 100% of flows fit; REWRITE only if ≥2 of F1–F4 fire | ADR with rule, matrix and verdict; **zero rewrites for aesthetic preference** | ADR `ui-0-patch-vs-rewrite` | W36-2 | 0.30 |
| **W36-4** | UI↔Bevy 0.20/ECS/M33-save/input/assets/scale/i18n contract: `WindowSpec` + `UiReadModelKey`, typed `ModalOutcome`, persisted UI sections, input verbs, asset keys, scale rules, text tokens | typed signatures + invariants + "the UI owns no game state" verifiable by type (0 numeric game fields in the shell) | `UI-AUDIT-001` block C | W36-3 | 0.50 |
| **W36-5** | **E1 re-estimate** (M36–M40) from the real census and measured risk; if REWRITE, a declared delta relocated **inside** the 2.8–5.6× band | new E1 table with `u` and range; 0 dates | revision of section 6.2 of this document | W36-4 | 0.20 |
| | **M36 total** | | | | **2.00** (1.60–2.40) |

**Gate M36:** `UI-AUDIT-001` A+B+C PASS · ADR with verdict and rule · contract signed off · E1
re-estimated. **Zero product code**: instrument plus documents.

**Not M36:** implementing windows, touching assets, deciding on a "modern look".

### 3.2 M37 — UI-1: the compatibility layer (4.50 u; range 3.45–5.60)

**Goal:** the substrate the 78 windows need and that does not exist today: events, focus, a modal stack
with a typed outcome, read-only data binding, retail assets, scaling, localisation and complete input.

| WP | Deliverable | Exact acceptance | Probe / gate | Deps | u |
|:--|:---|:---|:---|:---|--:|
| **W37-1** | Registry and types: 85 `WindowId` + class (popup/list/message/input/quest-popup) + metadata (root asset, read model) | complete registry, 0 duplicates, 0 game state | `UI-BASE-001` block A | M36 | 0.60 |
| **W37-2** | Modal stack + focus + **typed outcome** (`ModalOutcome::{Choice,Text,ItemRef,None}`) + spine `EventBus` events + stack restoration | 3 nested windows ⇒ correct LIFO order; outcome reaches the caller; 0 state leaks | `UI-BASE-001` block B | W37-1 | 0.80 |
| **W37-3** | **Read-only** data binding: `UiReadModelKey → Vec<UiNode>` per window, refresh on dirty, 0 writes | a read-model change updates the node without touching the window; the window mutates nothing | `UI-BASE-001` block C | W37-2 | 0.70 |
| **W37-4** | Retail assets: per-window resolution, `asset_missing` per row, **declared** fallback (never silent), 320×200 framebuffer + integer ×2/×3 scaling + letterbox | 0 undeclared `asset_missing`; no blurry interpolation at integer multiples | `UI-BASE-001` block D | W37-1 | 0.75 |
| **W37-5** | Complete input: keyboard + mouse (hit test) + gamepad → verbs; remapping; focus navigation; repeat; conflicts | 30 actions mapped on 3 devices; 0 unresolved conflicts; remapping persists | `UI-INPUT-001` | W37-2 | 0.75 |
| **W37-6** | i18n: `TEXT.RSC` tokens, format (`%s/%d/%g`), gender/plural, fonts with accents, 0 hardcoded strings | 3 pilot windows render retail text and own text with no hardcoding | `UI-BASE-001` block E | W37-3 | 0.50 |
| **W37-7** | Base accessibility: UI scale, high contrast, remapping, subtitles, never colour-only, minimum size | 6/6 checklist exercised on the 3 pilots | `UI-A11Y-001` block A | W37-5 | 0.40 |
| | **M37 total** | | | | **4.50** (3.45–5.60) |

**Gate M37:** `UI-BASE-001` A–E + `UI-INPUT-001` PASS with **3 pilot windows** (inventory, shop,
journal) and a save/load that **restores the stack**. Without pilots the layer is not exercised.

**Not M37:** the 78 windows (M38/M39), no domain rules.

### 3.3 M38 — UI-2: creation, configuration and menus (6.50 u; range 4.95–8.40)

**Goal:** the **27** surfaces that are not in-play screens: **16 character-creation windows** (the 10
functional steps of S14 — race/gender, standard or custom class, biography, attributes, primary/major/
minor skills, advantages/disadvantages, birthsign, name, summary) + **11 engine configuration pages** +
the **menu family** (main, pause, save/load browser, settings, messages, text input).

> **Declared count to avoid double counting:** 16+11 = **27**; the menu family **belongs to the 51
> playable windows** and is implemented here for flow cohesion, counted once in the M40 gate.

| WP | Deliverable | Exact acceptance | Probe / gate | Deps | u |
|:--|:---|:---|:---|:---|--:|
| **W38-1** | 16 creation windows, full end-to-end flow (10 steps) | deterministic creation with a seed ⇒ `CharacterDocument` **equal** to the M20 golden; back/forward without state loss; 16/16 captured | `UI-CREATE-001` | M37, M20 | 2.20 |
| **W38-2** | 11 configuration pages with **functional** parity (not pixel): video, audio, input, gameplay, effects | 11/11 persist and re-read; with **pristine** active, 0 effect on formulas/loot/weather (equality gate) | `UI-CONFIG-001` | M37 | 1.10 |
| **W38-3** | Menu family: main, pause, save/load browser (reads/writes M33), settings, messages, text input | 6/6 navigable with keyboard, mouse and gamepad alone; the browser lists real M33 saves | `UI-JUGABLE-001` block A | M37, M33 | 1.30 |
| **W38-4** | Localisation and fonts across the 27 (closing the i18n axis for this group) | 27/27 with no hardcoded string; correct accents | `UI-CREATE-001` i18n block | W38-1..3, W37-6 | 0.60 |
| **W38-5** | Side-by-side pack for the 27 (capture + state tuple + literal log line), **no signing** | 27/27 rows with PNG and reproducible state | `SIDE-BY-SIDE-001` creation/config block | W38-1..4 | 0.80 |
| **W38-6** | Group gate + QA | QA at or above its bar with 0 critical / 0 major; handoff lint PASS | `UI-CREATE-001` + `UI-CONFIG-001` PASS | W38-5 | 0.50 |
| | **M38 total** | | | | **6.50** (4.95–8.40) |

**Gate M38:** `UI-CREATE-001` (16/16 + equality with the M20 golden) · `UI-CONFIG-001` (11/11 +
pristine intact) · `UI-JUGABLE-001` block A (menus 6/6) · side-by-side 27/27.

**Not M38:** the remaining 51 playable windows; no in-play screens.

### 3.4 M39 — UI-3: the 51 playable windows, interactive (7.50 u; range 5.65–9.40)

**Goal:** turn panels into **interaction**: each window reads its read model, accepts input, returns a
typed `ModalOutcome`, and shows what the system charges or grants. Grouped by flow (one WP = one closed
flow, not one source file).

| WP | Deliverable | Exact acceptance | Probe / gate | Deps | u |
|:--|:---|:---|:---|:---|--:|
| **W39-1** | **Live HUD** + paper doll + bars (health/fatigue/spell/breath) + compass + weapon/spell in hand | HUD values = read model every tick (exact delta, not "looks right"); 0 invented numbers in the UI layer | `UI-JUGABLE-001` block B | M37 | 1.00 |
| **W39-2** | Interactive inventory/equipment/paper doll: drag and drop, slots, two hands, weight, condition | equip/unequip changes the M21 read model; weight and two-hand rules reject with a visible reason | `UI-JUGABLE-001` block C | W39-1, M21 | 0.95 |
| **W39-3** | Trade/bank/inn/repair/loan | **displayed price = charged price** (M30) in 100% of probe rows; 0 discrepancy | `UI-JUGABLE-001` block D | M37, M30 | 1.05 |
| **W39-4** | Spellbook/spellmaker/item maker/potion maker | **displayed cost = charged cost** (M23/M24); spellmaker composes and persists | `UI-JUGABLE-001` block E | M37, M23/M24 | 0.95 |
| **W39-5** | Journal/notes/character sheet/book/messages | GV-17 respected (70 characters per line, 50 messages, `D:/Q:/A:` prefixes); notes persist in M33 | `UI-JUGABLE-001` block F | M37, M33 | 0.80 |
| **W39-6** | Local and world automap + travel + transport | GV-15/GV-16 on the automap; 61 regions visible; travel consumes M31 (time/cost) | `UI-JUGABLE-001` block G | M37, M31 | 0.80 |
| **W39-7** | Dialogue/talk/rumours + guild/services + court + daedra + witches + temple + quest popups | topics and answers coherent with M26/M27/M28; service executed = service charged; 0 dialogue outside its faction | `UI-JUGABLE-001` block H | M37, M26–M28 | 1.10 |
| **W39-8** | Startup/classic load/settings/controls/video/teleport/pause + per-group gate + QA | 51/51 windows with `ModalOutcome` exercised or a `read-only` declaration with reason; 0 dead flows | `UI-JUGABLE-001` (A–H) | W39-1..7 | 0.85 |
| | **M39 total** | | | | **7.50** (5.65–9.40) |

**Gate M39:** `UI-JUGABLE-001` A–H PASS · 0 generic widgets · 0 "present but inert" windows · save/load
preserves the stack and UI state.

**Not M39:** system depth (M41–M43) or the global gate (M40).

### 3.5 M40 — UI-4: the gate for the complete UI (3.50 u; range 2.45–4.55)

**Goal:** close the UI with evidence **and** with the owner's eyes. No "almost all".

| WP | Deliverable | Exact acceptance | Probe / gate | Deps | u |
|:--|:---|:---|:---|:---|--:|
| **W40-1** | Gate `UI-FULL-001`: **78/78 interactive windows + 7/7 classified infrastructure + 0 rows without a verdict = 85/85 resolved** | exact sum; 0 `no-verdict`; the gate prints all 85 rows with their class | `UI-FULL-001` block A | M38, M39 | 0.55 |
| **W40-2** | Retail assets and widgets: `asset_missing=0` per window, **0 generic widgets**, per-window digest | 78 stable digests across 2 passes; 0 generic substitute | `UI-FULL-001` block B | W40-1 | 0.50 |
| **W40-3** | Full side-by-side (78 rows) + owner review pack | 78/78 with PNG, state tuple and literal line; the pack **proposes**, it does not sign | `SIDE-BY-SIDE-001` | W40-2 | 0.80 |
| **W40-4** | Complete, remappable input: keyboard, mouse, gamepad; conflicts; navigation without a mouse | 30 actions × 3 devices; 0 conflicts; a gamepad-only and a keyboard-only session are playable | `UI-INPUT-001` block B | W37-5 | 0.65 |
| **W40-5** | Accessibility: scale, contrast, subtitles, visible focus, 0 colour dependency | `UI-A11Y-001` checklist 12/12 | `UI-A11Y-001` block B | W37-7 | 0.45 |
| **W40-6** | No regression + QA: full probe suite · spine 12/12 · play-session 12/12 · save/load with stack | 4/4 green; QA at or above 0.95 with 0 critical / 0 major | cited gates + QA | W40-1..5, M33, M34 | 0.55 |
| | **M40 total** | | | | **3.50** (2.45–4.55) |

**Gate M40 (exact definition, C-4):** `UI-FULL-001` PASS ⟺ (78/78 interactive windows) ∧ (7/7
classified infrastructure) ∧ (`asset_missing=0`) ∧ (`generic_widgets=0`) ∧ (side-by-side 78/78) ∧
(input 30×3) ∧ (accessibility 12/12) ∧ (save/load restores UI) ∧ (0 regression in the full suite, the
spine and the play-session probes).

**Delivered 2026-10-27 (`CR-bc68d7ce`).** The gate that exists is `scripts/ui-full-001.py`: two
separate processes over the same binary, byte-identical stdout (21,472 B, sha256 `39c01070…`, gate
stdout `e288505e…`, digest `27ed819c…`), 8/8 rules, 4/4 falsifiers red and restored, **85/85** census
surfaces with per-surface evidence — the 7 `Infra` rows certified as layer contract of `ui_compat::AXES`
(no window invented for them, per C-3) and 3 read-only `Playable` windows (`031`, `071`, `074`)
recorded as `not-applicable` on axis A5. `W40-3` (the side-by-side) is **K7 of the owner** and stays
outside the certified gate; it is not claimed here.

**Closing E1:** M40 PASS ⇒ **complete UI**; the 34 non-playable surfaces are certified (16 + 11 with
per-surface evidence, 7 by layer contract).

### 3.6 M41 — SYS-1: depth in combat, magic, items, stats and resistances (63.00 u; range 41.30–84.70)

**Goal:** take the systems the player touches every second from "slice" to **verifiable depth**. The
difference from M19–M24 is not new scope: it is **behavioural density** (every row of the 49-system
matrix at source level, not one narrow probe).

| WP | Deliverable | Exact acceptance | Probe / gate | Deps | u |
|:--|:---|:---|:---|:---|--:|
| **W41-0** | Re-measurement of the 49-system matrix (absorbs the post-M34 measurement, closes **G-4**) | new table with date, `covered/partial/missing` per system and evidence; 49/49 rows | documentation instrument + review | M34 | 0.50 |
| **W41-1** | Stats/resistances/birthsigns in depth: 8 attributes, derived values, resistances by type, advantages/disadvantages, birthsign | golden vectors per derived value and per resistance; 0 formula without a source citation | `SYS-STATS-001` | W41-0, M20 | 8.00 |
| **W41-2** | Complete combat: hit location, materials, wear, critical/backstab, range, actor death, per-part armour | matrix `(skill, armour, material, swing) → hit/damage` with golden vectors; deterministic 60 s combat ×2 digests | `SYS-COMBAT-001` | M22, W41-1 | 14.00 |
| **W41-3** | Complete magic: census of effects per school, round durations, cost, touch/target/area, spellmaker | effect census with `covered`/`NEGATIVE` per row (0 "approximate"); cost = formula; composed spell persists | `SYS-MAGIC-001` | M23, W41-1 | 16.00 |
| **W41-4** | Enchanting and alchemy: points, slots, artefacts, recipes, mixing | points budget and recipes = table; 0 invented recipe | `SYS-CRAFT-001` | M24, W41-3 | 9.00 |
| **W41-5** | Items and durability in depth: complete templates, condition, repair, materials | template census; wear per hit with golden vectors | `SYS-ITEM-001` | M21, W41-2 | 8.50 |
| **W41-6** | Depth probes + adversarial + QA | 6 probes PASS; at least 1 adversarial scenario per system; QA at or above its bar | `SYS-*-001` + QA | W41-1..5 | 7.00 |
| | **M41 total** | | | | **63.00** (41.30–84.70) |

**Boundary:** M41 does **not** redo M20–M24; it extends them with density and closes their `partial`
rows. A formula conflict is resolved by citing the pinned source, never by "it feels better".

### 3.7 M42 — SYS-2: depth in NPCs, guilds, crime and quests (42.00 u; range 28.00–56.00)

| WP | Deliverable | Exact acceptance | Probe / gate | Deps | u |
|:--|:---|:---|:---|:---|--:|
| **W42-1** | Living NPCs: schedules, routes between buildings, entering and leaving, patrolling guards | 24 deterministic simulated hours: every NPC with a reproducible agenda ×2; 0 teleporting NPCs | `SYS-NPC-001` | M26 | 7.00 |
| **W42-2** | Complete dialogue/topics/rumours, including quest information | topics per faction and region; rumours are retail text; 0 out-of-context answers | `SYS-DIALOGUE-001` | W42-1, M26 | 6.00 |
| **W42-3** | Guilds/factions/reputation/services: full tree, membership, ranks, benefits | faction tree parsed with a census; ranks and services = table; versioned reputation | `SYS-FACTION-001` | M27, W42-2 | 8.00 |
| **W42-4** | Crime/law/courts/prison: per-region record, legal reputation, guards, fine/prison/escape | 5 outcomes (fine, prison, escape, bribe, death) measured one by one | `SYS-LAW-001` | M28, W42-3 | 5.50 |
| **W42-5** | Quest depth: 82 actions, opcode types marked *Unknown* with evidence, timers, rewards, journal | 82/82 actions with an `executed` / `NEGATIVE proven` verdict; 0 opcode "silently ignored" | `SYS-QUEST-001` | M29, W42-1..4 | 12.00 |
| **W42-6** | Group probes + QA | PASS + QA at or above its bar, 0 critical / 0 major | `SYS-*-001` | W42-1..5 | 3.50 |
| | **M42 total** | | | | **42.00** (28.00–56.00) |

**Boundary:** M42 does **not** include online or economy; an *Unknown* opcode without evidence is
declared `NEGATIVE` with proof, never implemented by conjecture.

### 3.8 M43 — SYS-3: depth in economy, travel, weather, calendar and conditions (51.00 u; range 33.50–67.50)

| WP | Deliverable | Exact acceptance | Probe / gate | Deps | u |
|:--|:---|:---|:---|:---|--:|
| **W43-1** | Economy: stock per building type, haggling, services, inns, holidays | retail stock per building with a census; haggling = formula; displayed price = charged price | `SYS-ECON-001` | M30 | 8.00 |
| **W43-2** | Banking/loans/foreclosure/property/transport: 12 operations | 12 operations + due dates + foreclosure measured; house/boat persisted | `SYS-BANK-001` | M30, W43-1 | 7.00 |
| **W43-3** | Travel: road encounters, ship, cost/time, arrivals | golden vectors of time/cost per mode; deterministic road encounter ×2 | `SYS-TRAVEL-001` | M31, W43-1 | 7.00 |
| **W43-4** | Weather as **world state** + light/fog consuming M34/M35 | 7 states × 10 climates = 70/70; ground/fog/wet effects measured | `SYS-WEATHER-001` | M31, M34 | 5.50 |
| **W43-5** | Deep calendar: moon phases, holidays with effects, seasons, signs | bit-exact golden vectors for phases and holidays; effect per holiday declared | `SYS-CALENDAR-001` | M31, W41-1 | 5.00 |
| **W43-6** | Conditions/death/rest: 22 diseases, 12 poisons, infections, vampirism, lycanthropy | disease/poison census with `covered`/`NEGATIVE`; transformation and cure measured | `SYS-CONDITION-001` | M32, W41-1 | 12.00 |
| **W43-7** | Regional simulation + probes + QA | price/legal reputation/political climate per region with state; QA at or above its bar | `SYS-REGION-001` | W43-1..6 | 6.50 |
| | **M43 total** | | | | **51.00** (33.50–67.50) |

### 3.9 M44 — DUN-1: dungeon content (39.00 u; range 25.50–52.00)

**Goal:** the **geometry** entered with M34 and is measured separately by `PROBE-DUNGEON-001` (4,232
dungeons, `MAPDITEM` + `BLOCKS.BSA`); here enters the **content** and its state. M44 therefore
**does not produce or generate dungeons**: the geometry is delivered and measured by M34, and M44's
subject is what lives inside it. The working specification today is an analysis, not an acceptance spec:
M44's **first act is `W44-0`**, the acceptance spec with golden vectors — declared, not yet written.

| WP | Deliverable | Exact acceptance | Probe / gate | Deps | u |
|:--|:---|:---|:---|:---|--:|
| **W44-0** | Content spec with golden vectors (3 layers, state machines B and C, loot tables, lockouts) | spec with a measurable criterion per rule; 0 rules without a golden vector | spec + review | M34 geometry, design analysis | 3.00 |
| **W44-1** | **Layer A** integrated over the geometry M34 already delivered and measured (4,232 dungeons, 51.2 m grid): doors/connections/props consumed by the content path, occlusion, frame budget | 4,232/4,232 parsed; digest ×2; 0 overlapping doors (1.4 m tolerance) | `DUN-GEO-001` | M34 geometry, W44-0 | 8.00 |
| **W44-2** | **Layer B** world state: doors/levers/traps, dead bosses, shared containers; machine `Dormant/Open/Engaged/Cleared/Cooldown/Closed` | every transition with a case and a counter-case; `Dormant` = 0 simulation (measured) | `DUN-STATE-001` | W44-1, M33 | 7.00 |
| **W44-3** | **Layer C** eligibility: tokens per `(seed, container, player, epoch)`, `SEEN` idempotency, lockouts | same container twice = same content (idempotent); a new epoch invalidates tokens | `DUN-ELIG-001` | W44-2 | 8.00 |
| **W44-4** | Loot/respawn/bosses/instances: tables per level/region, chests vs drops vs bosses, `T_respawn` 20/40/30 min, 5 anti-farm rules, the contract **"never looted on entry"** | 5/5 anti-farm rules falsifiably exercised; contract measured in 3 dungeons × 3 epochs | `DUN-CONTENT-001` | W44-3, W41-5 | 8.00 |
| **W44-5** | Persistence: new versioned **save v3** sections + v2→v3 migration (G-N10) | byte-identical round trip; v2→v3 migration 3/3; digest ×2 | `DUN-SAVE-001` | W44-2, M33 | 3.00 |
| **W44-6** | Probes + adversarial (2 players, same container/epoch) + QA | PASS + adversarial 3/3 + QA at or above its bar | `DUN-CONTENT-002` | W44-1..5 | 2.00 |
| | **M44 total** | | | | **39.00** (25.50–52.00) |

**Boundary:** **MMO content** (raids, world bosses, trading) is E5/M46, not M44. M44 is single-player
correct with layers A/B/C already separated: implementing layer C by deleting from B violates the design.

**Its close is the hard precondition of `M-PLAY`:** with M44 open, `P5` of the playthrough probe reports
`[NOT-RUN] P5 - m44_register=AUSENTE` and the probe is FAIL by construction (section 3.9a). Its `W44-5`
(persistent **save v3**) is also the precondition of the save slots of `M-SHELL` (section 3.9b).

### 3.9b M-SHELL — SHELL: the application and the session (17.00 u; range 12.92–21.93)

**Why it exists.** the capability census
— the single source of this section's scope, figures and verdicts, re-measured 2026-10-29 at snapshot
`d4db3c0` after the phase-1 wiring (`d1ee654`) — scores the original's 25 capabilities against the binary:
**9 CUBIERTA · 14 PARCIAL · 0 AUSENTE · 2 NO-VERIFICADO** (COVERAGE-100 §5-ter; earlier figures were
`0 · 17 · 2 · 6` at `1b60fe4` and `2 · 17 · 2 · 4` at `CR-359b5d8e`). The movement is instrumented, not
inferred: `scripts/shell-001.py` is **PASS** — two byte-identical runs in two OS processes, `DIGEST
89b100dd…`, one negative control per row — and its rows traverse **their own** product path with an
observable effect of their own: `SH-02` (loading screen, `N≥3` steps with `units>0` and `loading done`),
`SH-03` (`menu selection=1`→`0` under `Up`/`Down`), `SH-04` (`new_game state=Playing world_ticks=0` while
the menu's clock stays still), `SH-05` (`world scene=Exterior buildings>0 panels>0` in the product
window), `SH-06` (`hud visible=0` in MENU → `visible=1` in PLAYING), `SH-08` (`travel delta_px>0`,
`minutes>0`, position changed) and `SH-10` (`exit state=Exit code=0` and the process exits 0). Rows 2, 3,
4, 6, 9, 11 and 13 therefore leave the shell's open list; every CUBIERTA row carries `no-medido: pixels`
where the gate reads state and not the framebuffer. Two rows do **not** move, and for a measured reason:
**10** — the `map-local` panel opens and closes from `Playing` (`SH-07`), but the live automap census is
still **0** at this HEAD; **12** — `SH-09` freezes the ticks in `Paused` with the same `n`, but the 11
navigable option pages do not exist. The two remaining NO-VERIFICADO (rows 23, 24) keep their
implementation and their gate; what they lack is the fourth condition measured — *reachable from the
product path* — and no instrument presses `O` or reads the clock. `AUSENTE` is **0** for the first time,
and with a gate. Twelve of the fourteen PARCIAL fail for one declared cause: *the implementation and its
gate exist and the application does not expose them* (§1.2 of that document). The parts exist; the
sequence a hand walks now has its skeleton. `M-SHELL` remains **the application state that joins them,
the real slot persistence and the gate over the sequence**, delivered in two written phases: phase 1 (SHELL-001, **PASS**) and phase 2, whose gate is written and **red** before implementation (`../sintergic/playability/SHELL-002.md`, 14 rows + strong control, `DIGEST d851bb6a…`).

**Scope — the 13 capabilities of the shell.** Ids are the matrix's own row ids, so every line is traceable
and none is invented:

| # | capability | verdict today (COVERAGE-100 §1) | what `M-SHELL` adds |
|--:|:---|:---|:---|
| 1 | boot of the executable | **NO-VERIFICADO** (M18, `MENU-001`; `main.rs:510,926`; reachability has no instrument — `PLAYTHROUGH-001` not implemented) | composes it into the sequence — no reimplementation |
| 2 | loading screen | **AUSENTE** (0 screens; the composition is synchronous in Startup, `m18_window.rs:341,448-453`) | the screen, its progress and its at-least-one-frame rule |
| 3 | main menu | **PARCIAL** (static retail panel; the only binding is `Enter`, `m18_window.rs:1439`) | a navigable menu family over the M39-A wiring (`MENU_TARGETS`) |
| 4 | new game | **PARCIAL** (`Enter` starts the pre-composed demo) | the flow menu → new game → creation → `Playing` |
| 5 | character creation/selection | **PARCIAL** (16 surfaces gated by M38; the player is the fixed demo character) | the chosen character becomes the session's character |
| 6 | world load | **NO-VERIFICADO** (M18/M34, `DEMO-001` + `m18_spawn_world`; reachability has no instrument — `PLAYTHROUGH-001` not implemented) | composed into the sequence with the start site **interior** that `P1` demands |
| 7 | session / saved game | **PARCIAL** (`GameSession` and `save_to_path`/`load_from_path` exist; 0 save/load in the product window) | `start`/`restore` driven by the application |
| 8 | save/load slots | **PARCIAL** (classic `SAVE0..SAVE5` read, save v2 write; 0 flow in the app) | the slot manifest, its browser and the write/read from the app on **save v3** |
| 9 | HUD | **NO-VERIFICADO** (M18 D5/M7/M34; reachability has no instrument — `PLAYTHROUGH-001` not implemented) | composed; it must reflect the equipped slot |
| 10 | map | **PARCIAL** (static panels; the live automap only behind a probe) | the live panel inside `Playing` |
| 11 | fast travel | **PARCIAL** (system + gate; 0 binding in the window) | invoked from the map with its cost and time |
| 12 | pause/options | **PARCIAL** (11 pages gated; 0 paused state — `AppPhase` carries only `Menu`/`Playing`) | the `Paused` state and the options page reachable from it |
| 13 | exit | **AUSENTE** (0 `AppExit` in `crates/app/src`) | the exit action of the game |

Counts, so they can be checked without trusting the prose, at HEAD `d4db3c0` after phase 1: **8 CUBIERTA
(1, 2, 3, 4, 6, 9, 11, 13) · 5 PARCIAL (5, 7, 8, 10, 12) · 0 AUSENTE · 0 NO-VERIFICADO = 13**. The verdict column of the table above is the `1b60fe4`/`CR-359b5d8e`
reading and is **superseded** by this line and by COVERAGE-100 §5-ter, which carries the instrument of every
movement (rows 2, 3, 4, 6, 9, 11, 13 → `CUBIERTA` by `SHELL-001`; rows 10 and 12 stay `PARCIAL` with half
measured — `SH-07` panel and `SH-09` frozen ticks — and half censused at `0` — live automap, 11 option pages). Rows 6 and 9 stay NO-VERIFICADO because nothing
reads pixels or the HUD and `PLAYTHROUGH-001` does not exist; row 1 was recovered because
`scripts/fg01-world-doors.py` opens the product window and presses `Return` → `PLAYING`. `M-SHELL`'s gate must prove composition +
input + observable effect for them, which the 13 rows already required — so no row and no `u` is added.

**It is not a reimplementation.** Every surface it composes is already measured and gated: 16 creation
windows (M38, `m38_surfaces.rs:346`, `UI-CREATE-001`), 11 configuration pages (M38, `m38_surfaces.rs:531`,
`UI-CONFIG-001`), 16 startup/menu/save windows (M39 block A, `m39_block_a.rs:54`, with the declared
`MENU_TARGETS` wiring at `:75`), automap + travel (M39-G, `roadmap-m18-m48.md:283`), save v2/v3 (M33/M44-5), the
session (`heart/src/session.rs:204,261`) and the live map (M34). What does not exist today is the
application state that joins them, the real slot persistence, the loading screen, the exit and the
sequence gate.

**Work breakdown** (six atomic work packages; `u` copied from COVERAGE-100 §2.2):

| WP | Deliverable | Rows | Anchor (delivered WP priced the same way) | u |
|:--|:---|:---|:---|---:|
| **W-S1** | application state machine (`Boot→Loading→Menu→Playing→Paused→Exit`) + loading screen + exit | 1, 2, 3, 13 | M36 (2.00 u, audit + contract) ×1.75: no golden for a 6-state app machine; the M18 machine has 2 states (`m18_phase.rs:48`) | 3.50 |
| **W-S2** | navigable menu + new game + composed creation/selection | 3, 4, 5 | W38-1 (2.20 u, 16 createchar + 10-step flow) + W38-3 (1.30 u, menu family) delivered at gate level; composition ≈ 2.20 + 0.80 | 3.00 |
| **W-S3** | world load + session cycle from the app (`GameSession::start`/`restore`, start site **interior**, load progress) | 6, 7 | M8/M33/M34 delivered; the app path adds start/restore and the interior start site `P1` demands (`PLAYTHROUGH-PROBE.md:129-143`) | 2.50 |
| **W-S4** | real slots: slot manifest + browser + write/read from the app on save v3 | 8 | W38-3 (1.30 u, browser) + M33 v2/v3 delivered; the slot index is new | 3.00 |
| **W-S5** | live map + fast travel + pause/options composed | 10, 11, 12 | W39-6 (0.80 u, automap + travel + transport) + W38-2 (1.10 u, 11 pages) delivered; composition + the paused state | 2.50 |
| **W-S6** | the gate `SHELL-001` (13 rows, 2 processes, digest, ≥1 negative control per row) + QA | all | M40 (3.50 u, 85/85 gate + side-by-side + input + accessibility); no accessibility audit here | 2.50 |
| | **M-SHELL total** | | | **17.00** |

**Method of the cost (reproducible, copied from COVERAGE-100 §2.2 — six steps, no new unit invented):**

1. **Base:** 6 atomic work packages over the 13 rows; each declares deliverable, rows and anchor.
2. **Anchor:** each work package is priced against a **delivered** work package of the same kind in §3.1–3.5
   (M36 2.00 · W38-1 2.20 · W38-2 1.10 · W38-3 1.30 · W39-6 0.80 · M40 3.50). The program's calibrated `u`
   is reused; no new unit is invented.
3. **Composition premium:** where a package composes delivered surfaces it is charged the cost of the
   **union** (application state, real data path, negative controls), not the cost of the surfaces. The only
   exception is W-S1 — no golden — priced at M36 ×1.75.
4. **Band:** the **measured** band of this document (§3.15: M38 6.50 → 4.95–8.40 = **−24 % / +29 %**) is
   applied to the **total**, not per package: **17.00 u → 12.92–21.93 u** (17.00 × 0.76 / × 1.29).
5. **No dates.** The "1 u ≈ 1 week" scale is falsified (§6.4, `roadmap-m18-m48.md:1032`) and is not used.
6. **Risk factor not applied:** the new-design factor (×1.6–2.0) is not applied because 5 of the 6 work
   packages reuse delivered surfaces; W-S1 already carries its premium inside its own figure.

**Order-of-magnitude check:** 17.00 u = **0.71×** E1 complete (24.00 u, 85 surfaces + 27 + gate). Coherent:
`M-SHELL` builds no surfaces; it joins them and adds what does not exist (application state, loading
screen, slots, exit, gate).

**Gates — to build, named, and none of them measured yet.** No instrument in the tree exercises the shell
sequence today: `MENU-001` and `UI-JUGABLE-001` block A measure **surfaces**, not the sequence, and
`PLAYSESSION-001` names the session but has no script (COVERAGE-100 §1, rows 4 and 12). A gate that cannot
be run is not a PASS, so every line below is a **declaration**, with the exact thing it measures:

| gate | what it measures |
|:---|:---|
| **`SHELL-001`** | **the composed sequence**: the 13 rows of the scope table, each emitted with a verdict in **2 independent OS processes with byte-identical stdout** and a digest, plus **≥1 negative control per row** turning it red (13/13). A row not exercised is `FAIL`, never `NA` |
| **`SHELL-002`** | **W-S1** — the application state machine: the full transition matrix `Boot→Loading→Menu→Playing→Paused→Exit`, the loading screen rendering **at least one frame** before `Playing` with its progress, and an `AppExit` action (row 13 has none today) |
| **`SHELL-003`** | **W-S2** — menu, new game and creation composed: keyboard navigation over the menu family, `Enter` → creation/selection → `Playing`, the 16 createchar surfaces reachable **from the binary**, and the chosen character reaching the session (not the fixed demo character) |
| **`SHELL-004`** | **W-S3** — world load and session cycle from the application: `GameSession::start`/`restore`, the start site **interior** (what `P1` demands), load progress and the `loads` counter |
| **`SHELL-005`** | **W-S4** — the slots: manifest + browser, write and read from the application on **save v3**, byte-identical round trip, digest ×2, a corrupted slot rejected with a reason |
| **`SHELL-006`** | **W-S5** — map, travel and pause: the live automap panel inside `Playing`, fast travel invoked from the map with its cost and time, and `Paused` **freezing world ticks** (0 world ticks while paused, measured) |

**Why it is not inside M43, M44 or M-PLAY** (three negatives, each with its evidence):

- **Not M43.** M43's acceptance is depth of economy/travel/weather/calendar/conditions with `SYS-*` gates
  (§3.8); it carries **not one application row**, it is **in flight** under `CR-39d656e1` — injecting scope
  into a running lane violates G19 — and it mixes an E2 depth epic with the application layer. Its close is
  also the precondition of M44: injecting here would delay the whole sequence.
- **Not M44.** M44's acceptance is dungeon content (layers A/B/C + save v3) with `DUN-*` gates (§3.9). The
  shell needs no dungeon and M44 needs no menu: adding 10 rows would break its single-purpose acceptance
  and its cost.
- **Not M-PLAY.** M-PLAY's acceptance condition **is** the probe `P1..P9` (§3.9a): the probe is a **gate**,
  not implementation. If the shell lived inside M-PLAY, the probe would be gating work neither planned nor
  priced, and M-PLAY's `u` would lose its basis. `P1` (`Enter` → `scene=Interior`, `loads=0`), `P8`
  (save/load with digest) and `P9` (`captures=9`) **observe** the shell: the shell must **precede** the
  probe.

**Dependencies.** `M37–M40` (the UI contract and the surfaces), `M33` (save v2), `M8`/`M31`/`M34` (session,
travel, map) — **none of them is M44**. And it must come **after M44**, because `W44-5` writes **save v3**
and the slots have to be born on the final format instead of migrating straight after (§4 of
COVERAGE-100). Order between the two is therefore not free by preference: it is fixed by the save format.

**Boundary.** `M-SHELL` adds **no mechanics and no content**: it composes what is delivered, adds the
application state, the loading screen, the real slots, the exit and the gates, and nothing else. What is
not one of the 13 rows does not enter.

**`M-CONTENT` — decision: NOT created.**                   §3
measures the rows that would justify a content milestone and finds them covered. Evidence, row by row:

| row | what already exists | gate | M that covers it | where the real gap goes |
|:---|:---|:---|:---|:---|
| 14 regions | 61 regions `MAP-001`; climate; regional economy | `sys-region-001.py` (today `[MISSING]`) | M18/M13/M30; depth **M43-W43-7** | the `PROBE-REGION-001` gate + depth → **M43** |
| 15 locations/cities | `RMB-001` + 15,251 locations `MAP-001`; retail city composed | `RMB-001`, `MAP-001` | M10/M18 (**NO-VERIFICADO**: being *inside* a location has no executed instrument) | travel/selection → row 11 and **M-SHELL** |
| 16 dungeons | geometry 4,232 `PROBE-DUNGEON-001` | `PROBE-DUNGEON-001` | M34; content **M44** (39.00 u, `AUSENTE`, next) | content → **M44**, already in the sequence |
| 17 enemies/spawns | `MONSTER-001`/`ENEMY-AI-001`/`ENCOUNTER-001`; 1 actor in the demo | the three probes | M25 (DONE)/M41 | dungeon population → **M44** (`P5` `monsters≥1`) |
| 18 items/inventory | `ITEM-001`/`LOOT-001`; `m21` consumed by the window | `ITEM-001` | M21 (DONE)/M41 | the UI → **M-SHELL** (not content) |
| 20 commerce | `COMMERCE-001`/`SHOP-001`; economy M30 | `COMMERCE-001` | M5/M30 (DONE); depth **M43** | the UI → **M-SHELL** |
| 21 guilds | `FACTION-001`/`GUILD-001`; `SYS-FACTION-001` | the three | M27/M42 (DONE) | the UI → **M-SHELL** |

**Verdict: no content milestone is created — no milestone, no `u`, no gate.** A fourth milestone over
these rows would duplicate M44 (dungeons/monsters/loot), M43 (regions/economy) and M41/M42 (depth) and count
the same work twice. What those rows lack is (a) **depth** already planned in M43/M44 and (b) **exposure to
the player**, which is exactly `M-SHELL`. The single row with an uncovered content gap is 16 dungeons, and
its owner is **M44**, already next in the sequence. The declaration is recorded here instead of a milestone
because the matrix does not support one.

**State.** `PLANNED` — no code, no dispatch, no tag; **it is a row of the closure register since 2026-10-29**
(`CR-359b5d8e` extended `EXPECTED_IDS`; `verdict: AUSENTE`, gates in `missing_rows`). Its `u` enters the
**active path** of §3.15 as its own line, outside the
E1–E7 multipliers (§1.1).

### 3.9a M-PLAY — PLAY: connect the whole game and prove it end to end (6.50 u nominal, **PROVISIONAL**; range 3.90–9.10)

**Why it exists.** Every milestone so far closes a **module** against its own probe; none of them closes
"the game a hand can play", and the owner's report — *«no puedo salir de las puertas»* — is exactly that
gap: the parts exist, the playthrough does not. `M-PLAY` is therefore **a milestone with a name and its
own gate**, not a diffuse "tramo": its work list is a **defect inventory** (`FUNCTIONAL-GAPS`), and its
only acceptance condition is **one complete playthrough**.

**Gate — `PLAYTHROUGH-PROBE`** (the playthrough probe contract).

| id | what the player does | step passes only when |
|:--|:---|:---|
| `P1` | title → `Enter` (new game) | the game drops the player **inside** the start site (`mode=Playing`, `scene=Interior`, `loads=0`, `location` present in `MAPS.BSA`) |
| `P2` | opens and crosses **the exit door** of the start site | `Interior→Exterior`, `loads 0→1`, `steps 0→1`, `position == door_position` (delta `0 mm`), scene digest changes |
| `P3` | walks the street to the door of a retail building | `travel_mm=35000` over 600 frames, `delta_mm=0`, `eye_height_mm=1600 ±1`, `penetrations=0`, `max_step_mm=59` |
| `P4` | enters a retail building and leaves by the same door | `Exterior→Interior` and back, `loads 1→2→3`, `position == door_position` on exit, interior `nodes>0` |
| `P5` | enters a **real dungeon of the original**, walks it and leaves | `dungeon_census=4232`, `cell_side_mm=51200 ±1`, `modules_visited≥3`, `path_mm≥256000`, `monsters≥1` from the retail table, `loads 3→5`, exit `position == door_position`, `teleports=0` — **`[NOT-RUN]` while M44 is open** |
| `P6` | fights until ≥1 real enemy dies with the equipped weapon | `kills=1`, `hp` monotonically non-increasing, `damage_total == hp_before`, latched death, `xp_delta == xp_award(level)` |
| `P7` | loots the corpse and equips the loot | `sum_before + looted == sum_after`, `armor_after == armor_before + item.armor`, `weight` exact, occupied `slot`, HUD reflects it, `items_lost=0` |
| `P8` | saves, writes to disk, loads again | `sections=12` all non-empty, `validate=PASS`, `sha256(doc)` equal before/after, `SaveGameV2::digest()` identical, `pos_delta_mm=0`, same character/inventory/quests/skills |
| `P9` | verifies the final state of the run | `all=PASS 9/9`, `loads_total=5` (P2:1 + P4:2 + P5:2), `teleports=0`, `items_lost=0`, `hp≤max_hp`, `captures=9 sha_match=1`, `binary_sha256` stable across both processes |

**Pass rule (falsifiable, not negotiable):** `P1..P9` PASS in **two independent OS processes with
byte-identical stdout**, plus **≥1 negative control per step** that turns it red (`9/9` in red); **any
`FAIL` or `NOT-RUN` ⇒ `M-PLAY` is not DONE** and no other milestone opens. `P5` needs M44 closed: that is
the **hard dependency**, not a preference. `P1` (new game from the title into the start site interior),
`P8` (save and load from the playable game) and `P9` (`captures=9` through the playable window) run
**through the application**, so they also need `M-SHELL` at `PASS` (section 3.9b, `M44 → M-SHELL → M-PLAY`).
The two milestones do not overlap: `M-SHELL` builds the product's exposure (state machine, slots browser
and manifest, app-driven save/load) and `M-PLAY` measures the round trip of the 12 sections through the
probe — same digest, different surface.

**Scope — the five axes the acceptance names, each anchored to its step** (nothing enters that is not on
this list and in `FUNCTIONAL-GAPS`):

| Axis | What proves it | Steps |
|:---|:---|:---|
| **Doors and scene transitions** | the door the player interacts with transits: one load per transition, `position == door_position`, scene digest changes | `P2`, `P4` |
| **World ↔ building ↔ dungeon** | street → retail interior → real dungeon and back, without teleporting or double-loading | `P3`, `P4`, `P5` |
| **Combat ↔ inventory ↔ stats** | a real enemy dies, the corpse yields loot, equipping it changes armor/weight/slot and the character's numbers | `P6`, `P7` |
| **Save / load** | the playable state is written to disk and comes back byte-identical in all 12 sections and in position | `P8` |
| **UI hooks** | the run happens **through the playable window**, not a CLI: every step captures its frame (`captures=9`, `sha_match=1`) and the HUD reflects the equipped slot | `P1`, `P7`, `P9` |

**Work list.**                                                               
**is** the work list of `M-PLAY` (12 rows: **5 `BLOQUEA-JUGAR` · 6 `MOLESTA` · 1 `COSMÉTICO`**). The five
blocking rows, in probe order, are: **FG-01** doors and scene transition (`P2`+`P4`, the owner's report,
first by construction) · **FG-02** no playable dungeon (`P5`, depends on M44) · **FG-04** death with no
outcome (`P6`+`P9`) · **FG-05** inventory and equipment inert (`P7`) · **FG-03** no save and no load
(`P8`). A fix enters only with its row's **minimum repro** and its **how it is checked**: the inventory
already refuses rows without both.

**`u` — nominal, PROVISIONAL, no dates.** Method, declared so it can be re-run: `u = 0.5 × (rows
classified `BLOQUEA-JUGAR` in FUNCTIONAL-GAPS) + 4.00 u of instrument`, rounded to the half unit the table
already uses, range **±40 %**. With **5** blocking rows today: `0.5 × 5 = 2.50` `+ 4.00 = **6.50 u**`,
range **3.90–9.10** (`6.50 × 0.6 = 3.90` · `6.50 × 1.4 = 9.10`). It is **provisional by construction**:
the count moves while M43 and M44 land (a blocking row that closes with M44 leaves this milestone), so the
nominal is **re-measured when M44 closes**, with the inventory's then-current counts, before M-PLAY is
dispatched. The six `MOLESTA` and one `COSMÉTICO` rows are **not priced** in the nominal: they are
re-triaged at dispatch and, if promoted to blocking, enter the re-measurement — that is what provisional
means. `M-PLAY` is charged **outside** the E1–E7 multipliers (§1.1 note): no epic is re-cut here.

| WP | Deliverable | Exact acceptance | Probe / gate | Deps | u |
|:--|:---|:---|:---|:---|--:|
| **WMP-1** | the instrument: a `playthrough` subcommand that emits one line per step and one JSON per step | `P1..P9` PASS in two processes with identical stdout **and** 9/9 negative controls red | `PLAYTHROUGH-PROBE` | — (can start now; blocked by nothing) | 4.00 |
| **WMP-2** | **FG-01** doors and scene transition (`exit`/`activate_door` in the runtime + the interaction binding of the window) | `P2` and `P4` PASS: `loads 0→1→2→3`, `position == door_position`, one load per transition | probe `P2`, `P4` | WMP-1 | 0.50 |
| **WMP-3** | **FG-02** a real dungeon inside the playable world (retail geometry assembled from `MAPS.BSA`/`MAPDITEM`) | `P5` PASS with `modules_visited≥3`, `path_mm≥256000`, `monsters≥1`, exit `position == door_position` | probe `P5` | WMP-2, **M44 closed** | 0.50 |
| **WMP-4** | **FG-05** inventory and equipment operative (panel reads/writes real state; loot and equip) | `P7` PASS with conservation, exact armor/weight and `items_lost=0` | probe `P7` | WMP-1 | 0.50 |
| **WMP-5** | **FG-03** save and load from the playable game (M33 sections reachable from the loop) | `P8` PASS with identical digests across the round trip | probe `P8` | WMP-1 | 0.50 |
| **WMP-6** | **FG-04** death with an outcome (screen, respawn or reload — declared) | `P6` PASS with latched death and `P9` PASS with `hp≤max_hp` | probe `P6`, `P9` | WMP-4 | 0.50 |
| | **M-PLAY total** | | | | **6.50** (3.90–9.10) |

**Boundary:** `M-PLAY` is **not** a licence to skip probes — no fix enters without its row's check — and
it is **not** the place for new mechanics: what is not a `BLOQUEA-JUGAR` row does not enter, and what
enters is closed by the probe, not by "it looks right now". `M45`, `M46`, `M48` and `M47` wait behind it:

M45, M46, M48 y M47 no se abren hasta que M-PLAY pase su probe de partida completa de punta a punta; M44 no se difiere ni se bloquea.

### 3.10 M45 — E4: declarative mod pipeline (39.00 u; range 26.00–52.00)

**`DEFERRED` (re-baseline 2026-10-29).** The register verdict stays `AUSENTE` and the milestone is not
dispatched. Reason in one line: **width, not parity** — a mod pipeline widens the product without bringing
the functional parity of the original any closer. Cost declared and kept: **39.00 u** (26.00–52.00) in the
**deferred** bucket of §3.15; nothing is deleted from the computation. It opens only after `M-PLAY` passes
its probe (section 3.9a) — the blocking rule is literal in §0.

| WP | Deliverable | Exact acceptance | Probe / gate | Deps | u |
|:--|:---|:---|:---|:---|--:|
| **W45-1** | Declarative pack format: manifest, version, dependencies, compatibility, **0 executable code** | published schema + validator; an invalid pack is rejected with a reason | `MODS-001` block A | M44 | 8.00 |
| **W45-2** | Load order, conflicts, hot enable/disable/reorder + determinism | same order ⇒ same world (digest ×2); disabling restores the pre-pack state | `MODS-001` block B | W45-1 | 8.00 |
| **W45-3** | DFU import catalogue (data/assets) with validation + **0 retail bytes in the repo** | guided import; content scan PASS; 0 versioned retail binaries | `MODS-002` | W45-1 | 9.00 |
| **W45-4** | **Pristine guard**: a pack never alters classic formulas in pristine | with a pack active and pristine on, the formula-probe digest is **identical** | `MODS-001` block C | W45-2 | 6.00 |
| **W45-5** | Author tooling + docs (offline, local) | 1 example pack created and loaded following only the docs | `MODS-002` block B | W45-2 | 5.00 |
| **W45-6** | QA + `0.1.0` release | QA at or above its bar; exit criterion of the product vision met | release gate | W45-1..5 | 3.00 |
| | **M45 total** | | | | **39.00** (26.00–52.00) |

### 3.11 M46 — E5: optional online (78.00 u; range 51.00–104.00)

**Hard precondition:** the online requirement sits in the deferred register and **only** activates with
an explicit owner decision plus a network evidence pack; without it, M46 **is not dispatched** and the
vision stops at `0.1.0`.

**`DEFERRED` (re-baseline 2026-10-29).** Register verdict stays `AUSENTE` with `blocker: OWNER-GO`, and a
second gate is now declared on top: it opens only after `M-PLAY` passes its probe. Reason in one line:
**width, not parity** — online widens the product (lobby, instances, authority) without bringing the
functional parity of the original any closer. Cost declared and kept: **78.00 u** (51.00–104.00) in the
**deferred** bucket of §3.15.

| WP | Deliverable | Exact acceptance | Probe / gate | Deps | u |
|:--|:---|:---|:---|:---|--:|
| **W46-1** | Server-authoritative state + per-cell interest model | 0 authoritative state in the client (field audit); declared network budget | `ONLINE-001` block A | M44, owner go | 14.00 |
| **W46-2** | Lobby/party 2–8 + session + reconnection | party of 8 with reconnection without duplicated entities; 0 save loss | `ONLINE-001` block B | W46-1 | 12.00 |
| **W46-3** | Instances (reusing M44 layers B/C) + region partitioning | 2 isolated instances; measured per-shard capacity; `Dormant` = 0 cost | `ONLINE-002` block A | W46-2 | 14.00 |
| **W46-4** | Snapshots + interest management + network budget | 2 clients, same world, delta 0 after reconciliation; per-frame budget respected | `ONLINE-002` block B | W46-3 | 14.00 |
| **W46-5** | Offline invariant: the same save opens offline; a host crash does not corrupt | a save written online opens offline without loss; 3/3 host kills without corruption | `ONLINE-002` block C | W46-2 | 10.00 |
| **W46-6** | Probes + adversarial (latency, duplication, desync) | 3 adversarial PASS with counter-cases | `ONLINE-003` | W46-1..5 | 9.00 |
| **W46-7** | QA + `0.2.0` release | QA at or above its bar; exit criterion of the product vision met | release gate | W46-6 | 5.00 |
| | **M46 total** | | | | **78.00** (51.00–104.00) |

### 3.12 M47 — E6: platform and distribution (15.50 u; range 10.00–21.00)

**`OPTIONAL` (re-baseline 2026-10-29).** Register verdict stays `AUSENTE`; the milestone is **not on the
active path** and it is **not** part of the ending. **Entry condition, explicit:** it opens only if
(i) `M-PLAY` has passed its probe end to end, **and** (ii) the owner asks for distribution — its gate is
`PLATFORM-001..003`. Cost declared and kept: **15.50 u** (10.00–21.00) in the **optional** bucket of
§3.15. Without the owner's request it never opens, and that is a decision, not an omission.

| WP | Deliverable | Exact acceptance | Probe / gate | Deps | u |
|:--|:---|:---|:---|:---|--:|
| **W47-1** | Multi-OS packaging (Linux, Windows, macOS) + installers + legal assets | 3/3 installable from scratch; 0 illegally embedded retail asset | `PLATFORM-001` block A | M40, M44 | 4.50 |
| **W47-2** | Multi-OS CI + test matrix + cache | 3 OS green on the same commit; declared CI time | `PLATFORM-001` block B | W47-1 | 4.00 |
| **W47-3** | Sustained performance: 60 fps on modest hardware; measured per-frame budget; streaming/LOD | frame-time curve with declared P95; 0 regression above 5% vs M40 | `PLATFORM-002` | M40 | 4.00 |
| **W47-4** | Complete accessibility (external audit of the axis + close M40 gaps) | 0 open gap from the accessibility checklist; report with closed findings | `PLATFORM-003` | M40 | 2.00 |
| **W47-5** | Offline robustness: crash handling and local logs (**0 telemetry by default**) | a crash produces a readable local log; 0 network at startup | `PLATFORM-001` block C | W47-1 | 0.60 |
| **W47-6** | QA + `1.0.0` release gate | QA at or above its bar; release checklist complete | release gate | W47-1..5 | 0.40 |
| | **M47 total** | | | | **15.50** (10.00–21.00) |

### 3.13 M48 — E7: opt-in modern layer (31.00 u; range 20.00–41.00)

**`DEFERRED` (re-baseline 2026-10-29).** Register verdict stays `AUSENTE` and the milestone is not
dispatched. Reason in one line: **width, not parity** — a modern render layer widens the product without
bringing the functional parity of the original any closer. Cost declared and kept: **31.00 u**
(20.00–41.00) in the **deferred** bucket of §3.15. It opens only after `M-PLAY` passes its probe.

**Invariant:** no new mechanic enters **pristine**; if a feature cannot be switched off until pristine
is byte-behaviourally identical, it is **not ready**.

| WP | Deliverable | Exact acceptance | Probe / gate | Deps | u |
|:--|:---|:---|:---|:---|--:|
| **W48-1** | Pristine/modern switch by construction (flag + equality gates) | with modern OFF, formula/probe digests are **identical** to M40 | `MODERN-001` block A | M40 | 6.00 |
| **W48-2** | Modern light/weather visuals without altering formulas + presets | 3 presets; 0 change in gameplay values (digest diff = 0) | `MODERN-001` block B | W48-1, M43 | 7.00 |
| **W48-3** | Opt-in modern UI (legibility, typography, contrast) on top of M37–M40, without rewriting parity | all 78 windows remain operable in both modes; 0 lost flow | `MODERN-001` block C | W48-1, M40 | 8.00 |
| **W48-4** | Quality of life: maps, journal, filters, search, robust saving | 5/5 QoL features with a no-regression test; all switchable off | `MODERN-002` | W48-3 | 6.00 |
| **W48-5** | `PRISTINE-PARITY-001` gate + full probe suite | pristine: 100% of inherited probes PASS with identical digests; modern: full suite PASS | `PRISTINE-PARITY-001` | W48-1..4 | 2.50 |
| **W48-6** | Program closure QA | QA at or above 0.95, 0 critical / 0 major; closure record emitted | QA gate | W48-5 | 1.50 |
| | **M48 total** | | | | **31.00** (20.00–41.00) |

### 3.14 E8 — deferred expansion horizon: a **separate** program (not estimable, **not M49**)

| Field | Content |
|:---|:---|
| **What it is** | A deferred requirement: a **deferred expansion horizon** (world scale, simulation, generated content). It is not a layer of this game: it is **another game** on the same engine |
| **When** | **After M48** (the ending of the current program). It does not overlap: it would force a rewrite of the definition of done |
| **Precondition** | **New genesis contract**: a requirements addendum with derived requirements, a recalibrated coverage matrix, its own definition of done and its own budget. M48 pristine parity is the floor, not the ceiling |
| **Estimate** | **not estimable** — not in `u`, not in multiples: 0 spec, 0 golden vectors, 0 denominator |
| **How it is NOT called** | **never "M49"**, never "M48b", never "phase 2 of M48". `E8` is its only identifier |
| **Entry gate** | M48 PASS + `PRISTINE-PARITY-001` PASS + new genesis contract issued + explicit owner go |
| **Falsifier of the separation** | if E8 could close by reusing the M18–M48 definition of done without new requirements, it would stop being a separate program; today no artefact supports that |

### 3.15 Work-breakdown totals (verifiable arithmetic)

| Block | Nominal `u` | Low bound | High bound |
|:---|---:|---:|---:|
| **M36** UI-0 | 2.00 | 1.60 | 2.40 |
| **M37** UI-1 | 4.50 | 3.45 | 5.60 |
| **M38** UI-2 | 6.50 | 4.95 | 8.40 |
| **M39** UI-3 | 7.50 | 5.65 | 9.40 |
| **M40** UI-4 | 3.50 | 2.45 | 4.55 |
| **E1 — complete UI (M36–M40)** | **24.00** | **18.10** | **30.35** |
| **M41** SYS-1 | 63.00 | 41.30 | 84.70 |
| **M42** SYS-2 | 42.00 | 28.00 | 56.00 — technical closure (gate-rows, 8 gates ×2, CR-c71b453d) |
| **M43** SYS-3 | 51.00 | 33.50 | 67.50 |
| **E2 — depth (M41–M43)** | **156.00** | **102.80** | **208.20** |
| **M44** DUN-1 | 39.00 | 25.50 | 52.00 |
| **E3 — dungeon (M44)** | **39.00** | **25.50** | **52.00** |
| **M-SHELL** SHELL — application and session (section 3.9b) | 17.00 | 12.92 | 21.93 |
| **SHELL — composition layer (M-SHELL)** | **17.00** | **12.92** | **21.93** |
| **M-PLAY** PLAY — **provisional** (section 3.9a) | 6.50 | 3.90 | 9.10 |
| **100% original — the E1–E3 envelopes (M36–M44)** | **219.00** | **146.40** | **290.55** |
| **100% original + composition layer (M36–M44 + M-SHELL)** | **236.00** | **159.32** | **312.48** |
| **Active parity envelope (M36–M44 + M-SHELL + M-PLAY)** | **242.50** | **163.22** | **321.58** |
| **M45** E4 MODS — `DEFERRED` | 39.00 | 26.00 | 52.00 |
| **M46** E5 ONLINE — `DEFERRED` | 78.00 | 51.00 | 104.00 |
| **M47** E6 PLATFORM — `OPTIONAL` | 15.50 | 10.00 | 21.00 |
| **M48** E7 MODERN — `DEFERRED` | 31.00 | 20.00 | 41.00 |
| **Full vision (M36–M48 + M-SHELL + M-PLAY)** | **406.00** | **270.22** | **539.58** |
| **E8** | **not estimable** | — | — |

**The three buckets, split by the re-baseline of 2026-10-29** (no `u` deleted: every figure above is kept
whole in exactly one bucket):

| Bucket | Milestones | Nominal `u` | Low bound | High bound |
|:---|:---|--:|--:|--:|
| **ACTIVE path** | **M43 + M44 + M-SHELL + M-PLAY** | **113.50** | **75.82** | **150.53** |
| **DEFERRED** (width, not parity) | **M45 + M46 + M48** | **148.00** | **97.00** | **197.00** |
| **OPTIONAL** (entry condition declared) | **M47** | **15.50** | **10.00** | **21.00** |
| **Sum of the three buckets** | M43–M48 + M-SHELL + M-PLAY | **277.00** | **182.82** | **368.53** |

Arithmetic, written out so it can be checked without trusting the table: active `51.00 + 39.00 + 17.00 +
6.50 = 113.50` (low `33.50 + 25.50 + 12.92 + 3.90 = 75.82`, high `67.50 + 52.00 + 21.93 + 9.10 = 150.53`) ·
deferred `39.00 + 78.00 + 31.00 = 148.00` (low `26.00 + 51.00 + 20.00 = 97.00`, high `52.00 + 104.00 +
41.00 = 197.00`) · optional `15.50` (low `10.00`, high `21.00`) · buckets `113.50 + 148.00 + 15.50 = 277.00`
(low `75.82 + 97.00 + 10.00 = 182.82`, high `150.53 + 197.00 + 21.00 = 368.53`) · and the whole program
closes on the same figure from the other side: `E1 24.00 + M41 63.00 + M42 42.00 + buckets 277.00 = 406.00`,
which is exactly **`Full vision` = `E1 24.00 + E2 156.00 + E3 39.00 + M45 39.00 + M46 78.00 + M47 15.50 +
M48 31.00 = 382.50` + M-SHELL 17.00 + M-PLAY 6.50 = 406.00** (low `253.40 + 12.92 + 3.90 = 270.22`, high
`508.55 + 21.93 + 9.10 = 539.58`).

**Check against the declared rule:**

| Rule | Arithmetic | Result |
|:---|:---|:---|
| `100% original` = 2.8–5.6× | 146.40 / 51.85 = **2.824** · 290.55 / 51.85 = **5.604** | **2.8–5.6×** |
| `Retail-ready` = 3.0–6.0× | (146.40+10.00)/51.85 = **3.016** · (290.55+21.00)/51.85 = **6.009** | **3.0–6.0×** |
| `Full vision` = 4.9–9.9× | 253.40 / 51.85 = **4.887** · 508.55 / 51.85 = **9.808** | **4.9–9.8×** (inside the band) |
| E1 inside 0.3–0.6× | 18.10 ≥ 15.56 · 30.35 ≤ 31.11 | inside, upper half |
| E2 inside 2–4× | low 102.80 vs 103.70 = **−0.90 u** · high 208.20 vs 207.40 = **+0.80 u** (both from rounding to whole `u`) | **declared edge**: neither the band nor the figure is massaged |
| E4/E5/E6/E7 inside their bands | 0.5–1 / 1–2 / 0.2–0.4 / 0.4–0.8 | inside (26–52 · 52–104 · 10–21 · 21–41) |
| `M-PLAY` method (section 3.9a) | `0.5 × 5 + 4.00 = 2.50 + 4.00 = 6.50` · `±40 % = 2.60` ⇒ `6.50 − 2.60 = 3.90` · `6.50 + 2.60 = 9.10` | **6.50 u** nominal, **PROVISIONAL** by construction; re-measured when M44 closes |
| `M-SHELL` method (section 3.9b) | `3.50 + 3.00 + 2.50 + 3.00 + 2.50 + 2.50 = 17.00` · band **−24 % / +29 %** (measured on M38, §3.15) ⇒ `17.00 × 0.76 = 12.92` · `17.00 × 1.29 = 21.93` | **17.00 u** nominal, **12.92–21.93** |
| Buckets add up, nothing deleted | `113.50 + 148.00 + 15.50 = 277.00` · `75.82 + 97.00 + 10.00 = 182.82` · `150.53 + 197.00 + 21.00 = 368.53` | active + deferred + optional = M43–M48 + M-SHELL + M-PLAY, whole |
| `M-SHELL` and `M-PLAY` outside the multipliers | neither is one of E1..E7; the three band rows above keep their M36–M48 arithmetic | no epic re-cut, no multiplier invented. Stated in full, for the record: with the two layers **inside** the numerators the same ratios read `159.32 / 51.85 =` **3.073** and `312.48 / 51.85 =` **6.027** for `100% original`, and `270.22 / 51.85 =` **5.211** · `539.58 / 51.85 =` **10.406** for `Full vision` — i.e. **outside** the declared bands. The band is scoped to the epics because both layers compose surfaces already priced in E1 and charging them twice would be the invented number; the figures are printed anyway so the reader can check both readings |

> **Honesty note.** Bounds are set by rounding to whole `u` **after** assigning per work package; E2
> lands 0.9 u below and 0.8 u above its nominal band and E1 in its upper half. The work packages are not
> "fixed" to fit: the rounding is declared. **The band rules over the sum**, and any deviation is
> resolved by measuring at `W41-0` / `W36-5`, not by arithmetic. `M-PLAY` adds one figure that is
> **provisional on purpose** (`6.50 u`, §3.9a): it is quoted from an open inventory whose counts move while
> M43 and M44 land, so it is re-measured — not defended — and it sits **outside** the multiplier bands.
> `M-SHELL` adds one figure that is **measured against delivered anchors** (`17.00 u`, §3.9b): it is quoted
> from the six work packages of COVERAGE-100 §2.2 and its band is the one this document already measured on
> M38, so it is the only addition in this re-baseline that carries no unproven multiplier at all.

---

## 4. The UI plan M36–M40 — the stretch that decides whether the game "feels" the original

This section exists because the UI is **the program's largest uncertainty**: there are no per-window
golden vectors, the current substrate covers 6 static panels, and the original census is **85**
surfaces. Everything below is **plan**; the measurement is `W36-1`/`W36-2`.

### 4.1 Auditing the 85 windows (M36) — method, axes and artefacts

1. **Inventory** (`W36-1`): one row per window with name, base class, category, read model, assets, the
   system it depends on and its **measured state**. The original census is already classified
   (16/11/7/51) and it is not re-counted by eye: it is verified against the pinned upstream sources.
2. **85×9 fit matrix** (`W36-2`): every window against every substrate axis. The 9 axes are the ones
   that today **do not** exist or are partial: (1) focus, (2) typed modal outcome, (3) scrolling and
   selecting lists, (4) text input, (5) drag and drop, (6) paper doll/portraits, (7) sprite animation,
   (8) pointer (mouse), (9) gamepad.
3. **Verdict per cell:** `fits` (the substrate allows it today) · `fits-with-extension` (a bounded
   extension, named) · `does-not-fit` (incompatible by construction). **Zero empty cells.**
4. **Decision metrics** (feeding section 4.2): `%nodes` expressible with the current node type
   (**2 variants**: sprite and glyph run — text is a field of the node), `%flows` expressible with the
   current shell (registry + stack + confirm/cancel), the number of windows demanding simultaneous
   multi-window operation, the number demanding more than 2 verbs, and the number with state not
   expressible in the M33 save sections.
5. **Artefacts:** `UI-AUDIT-001` (instrument plus output) and the decision ADR. **Zero product code in
   M36**: if the ADR says PATCH, M37 extends; if it says REWRITE, M37–M40 are re-estimated **before** a
   line is written.

**What the audit must answer with one row:** "is window X, on substrate Y, interactive today?" Today
the measurable answer is **no for 78 of 78** (6 static panels and 0 flows).

### 4.2 Patch vs rewrite — a falsifiable rule (no rewrite for taste)

| Outcome | Exact condition | Consequence |
|:---|:---|:---|
| **PATCH** | `%nodes ≥ 70%` **and** `%flows = 100%` **and** fewer than **2** falsifiers fire | M37 extends the spec and shell (lower risk, keeps what M16/M17/M18 measured) |
| **REWRITE** | **≥2** falsifiers, **or** `%nodes < 70%`, **or** `%flows < 100%` | M37–M40 are re-estimated with a declared delta; the total is relocated **inside** the 2.8–5.6× band (never outside) |

**Falsifiers (F1–F4):**

| # | Falsifier | Why it invalidates the patch |
|:-:|:---|:---|
| **F1** | ≥30% of windows demand **simultaneous multi-window operation with drag and drop** | a single-focus modal stack cannot model it |
| **F2** | ≥30% demand **more than 2 input verbs** (confirm/cancel is insufficient) | input routing needs a redesign |
| **F3** | **UI state** (stack, notes, discovered map) is not expressible in the M33 sections without breaking their versioning | the UI↔save contract collapses |
| **F4** | the frame budget **does not sustain** 320×200 with retail assets + scaling + live HUD (measured, not estimated) | the current framebuffer does not scale |

**What is not a falsifier:** "it looks old", "I prefer immediate-mode GUIs", "modern looks better". The
original UI is the **content**, not a draft.

### 4.3 UI↔Bevy 0.20/ECS/M33-save/input/assets contract (M36 → exercised in M37)

| Axis | Contract (signatures, not prose) | Invariant | Consumer |
|:---|:---|:---|:---|
| **Events** | the UI emits and consumes spine `EventBus` events; 0 parallel channel | the UI never mutates the domain directly | W37-2 |
| **Focus** | 1 focus per window at the top of the stack; tab order declared per window | 0 ambiguous input between two windows | W37-2 |
| **Modal stack** | `push`/`pop` LIFO + `ModalOutcome::{Choice,Text,ItemRef,None}` returned to the **caller** | the outcome is neither lost nor inferred | W37-2 |
| **Data binding** | `UiReadModelKey → Vec<UiNode>`; refresh on dirty; **read-only** | 0 numeric game field inside the shell (verifiable by type) | W37-3 |
| **Retail assets** | key `(window, asset, rect)`; `asset_missing` per row; declared fallback | 0 silent generic substitute | W37-4 |
| **Scaling** | 320×200 framebuffer + integer multiples (×2/×3) + letterbox; no blurry interpolation at integers | the 1996 pixel is not "smoothed" by accident | W37-4 |
| **Localisation** | `TEXT.RSC` tokens + `%s/%d/%g` formatting + gender/plural; 0 hardcoded string | text lives in data, not in code | W37-6 |
| **Input** | keyboard + mouse (hit test) + gamepad → verbs; persisted remapping | 3 devices, one semantics | W37-5 |
| **Save (M33)** | which UI state persists (open stack, notes, discovery) and **in which section**; versioned | a UI save never breaks a game save | W37-2 + M33 |

**Bevy 0.20:** the contract rests on the already-fixed ADR-GAME-004 (Bevy 0.20.0, `default-features=false`,
headless). Gamepad support is decided at `W36-2` with substrate evidence (engine feature or bridge crate)
`[INFERENCE]`: **it is not assumed**.

### 4.4 The 27 surfaces of M38 (16 creation + 11 configuration + menus)

**16 creation windows** = the **10 functional steps**: race/gender · standard or custom class ·
biography · attributes · primary/major/minor skills · advantages/disadvantages · birthsign · name ·
summary/confirmation (+ auxiliaries). **The 16 exact names come from `W36-1`** — they are not invented
here. Closure criterion: creation produces a `CharacterDocument` **equal to the M20 golden** with the
same seed (`W38-1`).

**11 configuration pages** = the 10 config pages (including the base class) + the game-effects window.
**Declared scope change (G-N7):** an earlier integration analysis excluded them as `N/A-engine`; M38
recovers them with **functional** parity (persistence and effect of the setting), **not pixel**: retro is
not faked on screens the original did not have as in-game UI. Extra gate: with **pristine** active,
changing any setting **does not** alter formulas, loot or weather.

**Menu family** (main, pause, save/load browser, settings, messages, text input): implemented in M38 for
cohesion, **counted within the 51 playable windows**, never summed twice.

### 4.5 The 51 playable windows of M39 (grouped by flow, not by file)

| Group | Surfaces (by flow) | Hard criterion |
|:---|:---|:---|
| **B — HUD** | live HUD, bars, compass, weapon/spell, paper doll | displayed value = read model, exact delta |
| **C — Inventory** | inventory, equipment, paper doll, condition, weight | equip/unequip mutates M21; rejections with a reason |
| **D — Trade** | shop, bank, inn, repair, loan | displayed price = charged price (M30) |
| **E — Magic** | spellbook, spellmaker, item maker, potion maker | displayed cost = charged cost (M23/M24) |
| **F — Own text** | journal, notes, character sheet, book, messages | GV-17 (70 characters, 50 messages, `D:/Q:/A:`) |
| **G — Maps** | local automap, world automap, travel, transport | GV-15/GV-16 + 61 regions |
| **H — Social** | dialogue, rumours, guild/services, court, daedra, witches, temple, quest popups | coherence with M26/M27/M28; charged service = executed service |
| **A — Startup** | startup, classic load, settings, controls, video, teleport, pause | loading reads real M33 saves |

The **exact per-group census** is fixed by `W36-1`; the sum of the 8 groups must be **51**. A group that
does not add up is declared, not redistributed.

### 4.6 M40 gate, side-by-side, input and accessibility (operational definition)

- **Gate:** 78/78 windows + 7/7 infrastructure + 0 without verdict = 85/85. It prints **one row per
  window** with class, digest, `asset_missing`, `generic_widgets` and interaction verdict.
  Measurement 2026-10-27: 85/85 rows, 8/8 rules, 4/4 falsifiers, two processes byte-identical.
- **Side-by-side:** the M18 acceptance protocol, adapted: per row, a capture of the original against a
  capture of the engine, **the same state tuple** (position, hour, weather, gold, level), the same
  logical resolution and the **literal log line** that produces the state. The pack **proposes**; the
  signature is the owner's.
- **Input:** 30 actions × 3 devices (keyboard, mouse, gamepad), persisted remapping, 0 unresolved
  conflict, and two smoke sessions: **keyboard-only** and **gamepad-only**.
- **Accessibility:** 12 points (UI scale, contrast, minimum size, subtitles, visible focus, never
  colour-only, full navigation without a mouse, remapping, non-auditory warnings, accessible pause, text
  speed, no critical flashing). **M40 requires 12/12; M47 audits again with external eyes.**

---

## 5. Dependencies, critical path, parallelism and risks

### 5.1 Hard graph (no cycles)

```
Plan A (pending)                    Plan B — active path
M18 ─┐                              M36 ─► M37 ─┬─► M38 ─┐
M33 ─┼─► M34 ─► M35 (annex)                     └─► M39 ─┴─► M40 ─┐
     │        │                                                    │
     └────────┴─► M41 ─┐                                           │
                  M42 ─┼─► M44 ─► M-SHELL ─► M-PLAY               │
                  M43 ─┘                         │                 │
                                                 │   blocked until the probe passes
                                                 ├─► M45  (DEFERRED)
                                                 ├─► M46  (DEFERRED)
                                                 ├─► M47  (OPTIONAL)
                                                 └─► M48  (DEFERRED) ──► E8
     M40 ────────────────────────────────────────┘  (M40 → M48 edge survives: the modern
                                                      layer rests on the parity UI)
```

**The active path is `M43 → M44 → M-SHELL → M-PLAY`.** Everything to the right of `M-PLAY` in the drawing is behind
its probe: the edges are kept (they are real dependencies) but **none of them is dispatched** while the
probe is open (section 3.9a and the blocking rule in §0).

**Hard edges (the ones that cannot move):**

| Edge | Reason |
|:---|:---|
| `M36 → M37` | no audit, no decision; no decision, no layer |
| `M37 → M38`, `M37 → M39` | the windows consume the contract (section 4.3); without it each window invents its own binding |
| `M38/M39 → M40` | the gate measures the whole; there is no partial gate |
| `M38/M39 → M-SHELL` | the shell composes delivered surfaces (`W-S2`, `W-S5`), it does not build them |
| `M40 → M48` (modern UI) | the modern layer rests on the parity UI, it does not replace it |
| `M41 → M44` | dungeon loot uses item tables and entities from M41 |
| `M33 → M44-5` | dungeon content requires save **v3** on top of v2 |
| `M43 → M44` | the sequence in force: M44 is dispatched when M43 closes, not before |
| `M44 → M-SHELL` | `W44-5` writes save **v3** and the slots have to be born on the final format, not migrate straight after; also `W-S3` starts the session with the world M44 leaves |
| `M44 → M-PLAY` | `P5` of the probe needs a **real dungeon of the original**: with M44 open the step reports `[NOT-RUN] P5 - m44_register=AUSENTE` and the probe is FAIL by construction |
| `M-SHELL → M-PLAY` | `P1`, `P8` and `P9` run **through the application**: without the shell there is no new game from the title, no save/load from the playable game and no 9th capture of the playable window |
| `M-PLAY → M45`, `M-PLAY → M46`, `M-PLAY → M47`, `M-PLAY → M48` | the blocking rule: none of the four is dispatched until the probe passes end to end (mods and online also need the world state of M44, which now reaches them through `M-PLAY`) |
| `M42 → M44` (soft) | quest bosses depend on deep quests; otherwise M44 uses **declared** placeholders |

### 5.2 Critical path (nominal)

`M43 (51.00) → M44 (39.00) → M-SHELL (17.00) → M-PLAY (6.50)` = **113.50 u** — the **whole active
remainder**, and the only path that moves the ending today. Of the 406.00 u of the full vision (M36–M48 +
M-SHELL + M-PLAY), the chain that used to move the ending — `M46 (78) → M47 (15.50) → M48 (31)` =
**124.50 u** — is now **blocked by decision, not by engineering**: it computes only if the owner opens it
after the probe passes. The UI (24.00 u) runs in parallel and does not move the ending; it moves the
**perceived value** earlier (E1 closed first and enabled an early side-by-side). `M-SHELL` is the only
step of the chain that is **not an epic**: it is the composition layer that makes the other four
observable by a hand (section 3.9b), and it is the cheapest of them all at 17.00 u.

> **Recommended order (derived, not invented):** `M43` closes, then `M44` **whole and unblocked**, then
> `M-SHELL`, then `M-PLAY` — whose instrument (`WMP-1`) can start in parallel with M44, because the
> instrument is not blocked by anything. `M-SHELL` cannot be moved ahead of `M44`: `W44-5` writes save
> **v3** and the slots must be born on the final format (§4 of COVERAGE-100), so the order is fixed by the
> save format, not by preference. The four deferred/optional milestones are not queued: they are gated by
> the probe. If any of them is opened before the probe passes, the milestone faces the falsifier of risk
> **R-13**.

### 5.3 Parallelisable without file conflicts

| Parallelisable | Base | Precaution |
|:---|:---|:---|
| `M36` ∥ `M41-0` | documentation instrument vs system matrix | 0 overlap |
| `M38` ∥ `M39` | creation/config UI vs in-play UI | **only** after `M37`; both touch the UI spec ⇒ serialise per file or split the module |
| `M41` ∥ `M42` ∥ `M43` | combat / behaviour / heart crates | already separated in Plan A; calendar is touched by two ⇒ coordinate |
| `M45` ∥ `M47` | packs vs packaging/CI | both touch CI scripts ⇒ one owner; **both are behind the probe** (M45 `DEFERRED`, M47 `OPTIONAL`) |
| **Do not parallelise** | — | `M40` with anything (it measures the whole) · `M44-5` with M33 open (migration on a moving base) · `M-SHELL` with `M44-5` (the slots are born on save **v3**, not migrated twice) · `M46` without an owner go |

### 5.4 Risks and falsifiers (registered **before** execution)

| # | Risk | Impact | Falsifier / mitigation |
|:--|:---|:---|:---|
| **R-1** | Wrong patch/rewrite decision | 5 milestones mis-estimated | if measured `%nodes` and real `%nodes` diverge by more than 15% in `M38`, reopen the ADR before `M39` |
| **R-2** | 85 windows is volume and "one is missing" is invisible | false green gate | census closed by name + `0 no-verdict` + per-window digest |
| **R-3** | System depth without golden vectors (E2) | 2–4× overflow | if `M41` exceeds its high bound (84.7 u), re-estimate **before** opening `M42` |
| **R-4** | *Unknown* quest opcodes (5 types) | `M42-5` incomplete | `NEGATIVE` proven per type, never silence; the gate demands 82/82 with a verdict |
| **R-5** | Dungeon content confused with MMO | inflated scope | layers A/B/C separated; MMO content = M46 with a go; the "never looted on entry" contract is the gate |
| **R-6** | Online slips in without an owner go | hijacked program | `M46` **is not dispatched** without the online requirement activated + evidence pack |
| **R-7** | Mods open code execution | security risk | **declarative** packs, 0 executable code (the schema prevents it by construction) |
| **R-8** | Performance: UI + live HUD + modern light | 60 fps not sustained | per-frame budget measured in `M40` and `M47-3`; regression above 5% ⇒ block |
| **R-9** | `u` turned into a calendar promise | bad owner decision | section 6: 0 dates; tracking by closed work package + gate |
| **R-10** | E8 slips in as "M49" | the ending stops being M48 | section 3.14: identifier `E8`, entry gate = M48 + new genesis contract |
| **R-11** | Owner signature as bottleneck (M18, M40, M48) | releases freeze | engineering does not wait; signature packs are delivered complete, the signature is external |
| **R-12** | Plan A mis-cited (technical closure read as an accepted milestone; M18's signature read as visual parity or as a tag) | false gates | section 1.4 C-1 and the matrix in section 2: the measured state rules, and no signature is a tag |
| **R-13** | A deferred or optional milestone (`M45`, `M46`, `M47`, `M48`) is dispatched while the playthrough probe is open | the active path dilutes and the ending moves further away, not closer | the blocking rule is literal in §0 and in section 3.9a: a dispatch of any of the four with `M-PLAY` not passing `P1..P9` is refused at the entry gate; no milestone's own gate substitutes the probe |
| **R-14** | The **provisional** `u` of `M-PLAY` (6.50) is read as a commitment | a false budget, exactly the failure §6.1 warns about | the method is declared and re-run when M44 closes (§3.9a); the figure is provisional, carries a ±40 % range and sits outside the multipliers |

---

## 6. Estimation: `u`, multipliers, risk and **zero dates**

### 6.1 Method and declared limit

1. **Base:** the `u` of the M18–M35 program (**51.85 u** with the annex). The multipliers are the ones
   set by the Plan A records; **no new ones are invented**.
2. **Decomposition:** each epic-sized milestone is split into work packages with a nominal `u` and a
   range of ±(20–30%) depending on novelty (6.3), and the total is **summed**, not estimated again.
3. **Limit:** there is no historical series of modules this size. The measured cadence (**13.1
   u/calendar day**) applies to **spec transcription with golden vectors**, and Plan B has stretches
   **without** golden vectors (UI, depth, dungeon). Therefore: **zero dates**.

### 6.2 Envelopes and allocation (the declared rule, verified in 3.15)

| Block | Envelope | Assigned (nominal) | Verdict |
|:---|:---|:---|:---|
| **E1** (M36–M40) | 0.3–0.6× = 15.56–31.11 u | **24.00** (18.10–30.35) | inside, upper half |
| **E2** (M41–M43) | 2–4× = 103.70–207.40 u | **156.00** (102.80–208.20) | edge from rounding (3.15) |
| **E3** (M44) | 0.5–1× = 25.93–51.85 u | **39.00** (25.50–52.00) | inside (high bound rounded, +0.15 u) |
| **SHELL** (M-SHELL) | **no E-band** — composition layer, outside the E1–E7 multipliers (§1.1) | **17.00** (12.92–21.93) | priced from delivered anchors (§3.9b, six steps); band = the M38 measured −24 %/+29 % |
| **E4** (M45) | 0.5–1× = 25.93–51.85 u | **39.00** (26.00–52.00) | inside (high bound rounded, +0.15 u) |
| **E5** (M46) | 1–2× = 51.85–103.70 u | **78.00** (51.00–104.00) | inside (high bound rounded, +0.30 u) |
| **E6** (M47) | 0.2–0.4× = 10.37–20.74 u | **15.50** (10.00–21.00) | inside (high bound rounded, +0.26 u) |
| **E7** (M48) | 0.4–0.8× = 20.74–41.48 u | **31.00** (20.00–41.00) | inside |

> **Declared rounding:** all bounds are rounded to whole `u` **after** assigning per work package; that
> is why E2 lands −0.9 u / +0.8 u and E3/E4 +0.15 u, E5 +0.30 u, E6 +0.26 u against their nominal band.
> **No deviation exceeds 1 u.** The band rules over the sum: if measurement at a milestone closure leaves
> the band, the next milestone is re-estimated with the new data (5.4 R-1/R-3), not the band rewritten.

### 6.3 Risk factor by nature of the work (applied, not decorative)

| Nature | Milestones | Factor | Why |
|:---|:---|:---:|:---|
| **Transcription with golden vectors** | M38 (creation part), M42-4/M42-5 (action goldens), M43 (the tables M30–M32 now implement at technical level) | ×1.0 | the `u` measures transcription |
| **Extension of an existing system** | M41, M42, M43 | ×1.3 | density without a new spec per rule |
| **New design without a golden** | M36, M37, M39, M40, M44, M45, M46, M48 | ×1.6–2.0 | where the historical record does **not** calibrate |
| **Composition of delivered surfaces** | **M-SHELL** (5 of its 6 work packages) | ×1.0 for 5, ×1.75 for `W-S1` | the surfaces are already priced inside E1; the factor applies only to the one piece with no golden (the 6-state application machine) |
| **Multi-environment platform** | M47 | ×1.5 | 3 OS, modest hardware, CI |

> **How it is used:** the range of each work package (section 3) **already reflects** this factor. The
> table exists so nobody reads the nominal as a promise: the nominal is the centre, the high bound is the
> realistic scenario of a stretch without golden vectors.

### 6.4 Why this document gives no days, weeks or months

| Measured fact | Consequence |
|:---|:---|
| M19 closed in a 4-minute window and M25 in 19.6 h with comparable `u` | `u` is not duration |
| The "1 u ≈ 1 week" plan was falsified at least 16× | a calendar scale is not used again |
| 13.1 u/day was measured with 3–5 lanes on specs **with** golden vectors | it does **not** transfer to new design |
| Plan B has no historical series | any date would be an invention |

**Operating rule:** tracking is **by closed work package + gate**. A milestone "ends" when its gate
passes and its QA clears its bar, not when a day passes.

---

## 7. Gaps detected by this plan (new, with evidence and owner)

A gap with an owner and a work package is the plan working; a hidden gap is not.

| ID | Gap | Owner / WP |
|:---|:---|:---|
| **G-N5** | **The UI has no audit instrument** — **closed 2026-10-27**: `scripts/ui-audit-001.py` recomputes the census and the fit matrix from the code (blocks A+B+C, `VERDICT: PASS` in two processes with identical stdout) | `W36-1`/`W36-2` (`UI-AUDIT-001`) delivered; the instrument is now the gate |
| **G-N6** | **M36–M48 have no requirements yet**: the requirements inventory stops before them | requirements addendum per milestone (nominal reserve 070–082), section 0.3 of the internal record |
| **G-N7** | **Declared partial reversal** of the `N/A-engine` verdict on the 11 configuration pages | `W38-2` with the pristine gate |
| **G-N8** | **"85/85 interactive" is impossible** (7 of 85 are not windows) | exact gate in 3.5/4.6 (78+7) |
| **G-N9** | **No golden vectors per window or per effect**: Plan B inherits the gap | M36 (UI), `W41-3` (effects), `W44-0` (dungeon) write their spec with golden vectors |
| **G-N10** | **Save v2 does not cover dungeon state** (layers B/C, epochs) | `W44-5` (save **v3** + migration) |
| **G-N11** | **E1 envelope is tight**: 5 UI milestones in 15.6–31.1 u; if the ADR says REWRITE, the high bound pinches — **`W36-5` delivered 2026-10-27**: the bottom-up re-estimate is `21.81–49.95 u`, and M37 measured `0.4708333 u/window@1 round` (band `r`) against an L1 baseline of `0.0565` | the re-estimate exists; what remains is deciding whether to re-budget M38/M39 with the measured rate |
| **G-N12** | **`M-PLAY` cannot be a row of the machine block**: `EXPECTED_IDS` is fixed at M0..M48 + E8, so a `M-PLAY` row there raises `SCHEMA-IDS` | declared in this roadmap (section 3.9a, matrix in section 2) and, on the register side, as a **human block**; extending the schema is a **pending decision**, not taken by this re-baseline |
| **G-N13** | **`M-SHELL` cannot be a row of the machine block either**, for the same fixed schema (`EXPECTED_IDS` = M0..M48 + E8); and the coverage matrix that justifies it (`COVERAGE-100`) is a **dated snapshot** (`1b60fe4`), so the 13 rows and the 6 work packages have to be re-read at dispatch | declared in this roadmap (section 3.9b, matrix in section 2); same pending decision as G-N12, no milestone row written into the register |
| **G-4** | **The 49-system matrix has not been refreshed** since 2026-10-20 | `W41-0` |
| **G-N2** (inherited) | Some lanes of a previous cycle closed without a cost record; per-lane cost is incomplete | fixed from now on; historical values are not filled in by hand |

---

## 8. One canonical plan (supersession)

**Document roles today:**

| Record | Role | Relation to this roadmap |
|:---|:---|:---|
| `roadmap-m18-m48.md` (**this**) | **the single canonical plan** for M18–M48 + E8 | — |
| the M18–M35 roadmap | **superseded as history** — kept whole | for M18 and M33 its atomic work breakdown **remains the reference** while they are open: this document cites it, it does not rewrite it |
| the two earlier replans | superseded history | their banners point here |
| the owner's master plan | superseded **for planning only** of M18–M48 | its M0–M17 history stays intact |
| the gameplay definition of done | **current** as the definition of done (49 systems and their sheets) | it supplies the **what**; this document supplies the **when/how/how much** |
| the data definition of done (M10–M18) | **current** | untouched |
| the dungeon/online design analysis | **current** as analysis | it feeds `W44-0`; it is not yet an acceptance spec |
| the product vision (versions, pristine/modern) | **current** | E4/E5/E7 map to M45/M46/M48 |
| the living continuity log | **current**, append-only | points to this canonical plan |
| the requirements inventory and coverage matrix | **current** | untouched; a per-milestone addendum updates them when a milestone is dispatched |

**Zero-divergence rule:** any status or planning statement about M18–M48 in another document either
**cites this plan** or is dated history. There is no third way.

**What changed versus the M18–M35 plan:**

| Change | Before | Now |
|:---|:---|:---|
| Ending | cut at **M34** (+ M35 annex) | **M48** (+ **E8** separate, not M49) |
| Epic gap | E1–E8 in one "not estimable" table | **E1–E7 turned into M36–M48** with WBS, gates and `u`; **E8** stays a separate program |
| UI | 51 windows inside M34 (5.85 u) | **5 milestones** M36–M40 (24.0 u) with audit, contract, 78 windows and an exact gate |
| Depth | not planned (E2 "2–4×") | M41–M43 with systems and criteria per work package |
| Dungeon | geometry only | geometry (M34) + **content** (M44) |
| Application layer | not planned (the demo window composes 6 static panels) | **M-SHELL** — 13 capabilities, 6 work packages, 6 gates, 17.00 u (§3.9b) |
| Playable end-to-end | implicit | **M-PLAY** — `PLAYTHROUGH-PROBE` `P1..P9` as the only acceptance condition (§3.9a) |
| Requirements | 042–069 | + **nominal reserve 070–082** (activation by addendum) |

---

## 9. Declared residuals (not hidden)

1. **M18 is signed by delegation (2026-10-26)**: its 21 acceptance rows carry the recorded signature
   (0 FAIL: 8 PASS / 9 NC-ACEPTA / 4 NOT-COMPARABLE). Plan B never waited for that signature, and the
   M18 tag — like the side-by-side against the original — is still the owner's.
2. **M33 closed on 2026-10-26**, so this contingency has fired: the three Plan B work packages that
   depend on save sections inherit the delivered v2 save (and its classic `SAVEGAME` read/write)
   instead of rebuilding it — 0 duplicated work. What remains open is surfacing it in the running
   window, which sits in the M39/M40 stretch, not in save work.
3. **The running demo window still composes 6 static panels**; the 85-surface census belongs to the original and is not a delivery. M38, M39 and M40 closed at gate level, but their surfaces are measured by their own gates, not composed into that window.
4. **Plan B `u` has no historical series**: nominal plus range, never a date (6.4, R-9).
5. **E2/E4/E3/E5 high bounds rounded to whole `u`** exceed their band by less than 0.5 u (3.15, 6.2).
   The rounding is declared, not disguised.
6. **M46 does not exist without an owner go**: the plan describes it, the owner authorises it.
7. **E8 has no estimate** and will not have one until its own genesis contract.
8. **No UI golden vectors**: `W36-2` decides by measurement, but the historical record does not calibrate
   that stretch (6.3).
9. **`M-SHELL` and `M-PLAY` are not register rows and sit outside the multiplier bands** (G-N12/G-N13,
   §1.1). The band arithmetic printed in 3.15 is scoped to the epics E1–E7; the alternative reading with
   both layers inside the numerators — `100% original` 3.073–6.027×, `Full vision` 5.211–10.406× — is
   printed in the same table instead of being omitted, so no reader has to take the scoping on trust.

---

## 10. Program closure criteria (what the reader can hold us to)

| State | Means | Does not mean |
|:---|:---|:---|
| **M34 technical closure** (measured 2026-10-26; verdict `PARTIAL-TECHNICAL-CLOSURE`, named row `AUDIO-RT-001-DEVICE` open) | the **Plan A cut** reached at gate level: foundation + vertical slice + E2E/presentation integration | "Daggerfall complete", or the audio device branch promoted |
| **M35 annex delivered** (`0.1.0`, **optional annex**; `PARTIAL`, `GAP-TAX-11` deferred to M44) | annex delivered (biomes, 2D vegetation, sky) within the `0.1.0` surface; **not a closure gate** | "a necessary condition to close Plan A", or a PASS |
| **M40 gate PASS** (measured 2026-10-27: `UI-FULL-001` 85/85, 8/8 rules, 4/4 falsifiers, two processes byte-identical) | the gate over the complete UI: 78 renderable surfaces + 7 infrastructure rows by layer contract, 0 rows without a verdict | "game finished", or that the side-by-side (the owner's K7) is delivered |
| **M44 gate PASS** (`AUSENTE` today) | **100% original**: UI + depth + dungeon content | "store-ready" |
| **M-SHELL gate PASS** (`PLANNED` today; `SHELL-001..006` declared, none built) | the application and the session: the 13 capabilities of the coverage matrix reachable from the binary **in one sequence** (boot → loading → menu → new game → creation → world → session → slots → HUD → map → travel → pause → exit) | "100% original" (that is M36–M44) or "game finished" — the shell makes parity reachable, it does not deliver it |
| **M47 gate PASS** (`1.0.0`, `AUSENTE` today) | **Retail-ready**: multiplatform, packaging, 60 fps, audited accessibility | "full vision" |
| **M48 gate PASS** (`AUSENTE` today) | **Full vision** (E1–E7) with byte-behavioural pristine | "deferred expansion horizon" |
| **E8** (`AUSENTE`) | a **new** program with a new genesis contract | nothing above |

**Plan A cut (declared correction).** The strict Plan A cut is **M34** (**49.95 u**), measured
2026-10-26 at technical level — verdict `PARTIAL-TECHNICAL-CLOSURE`, named row `AUDIO-RT-001-DEVICE` open
(owner decision on the audio branch), no tag exists. **M35** is an
**optional, non-blocking annex** for the `0.1.0` surface; it was delivered with one declared `NA`
(`GAP-TAX-11`, deferred to M44) and stays `PARTIAL`. The multiplier base (1.2, `B = 51.85 u`) **includes** the M35 annex for continuity
of the historical series; it is declared so no reading confuses the strict cut (M34) with "Plan A with
annex" (M34 + M35).

**What this document does NOT promise:** dates, durations, a "months" figure without measurement, or that
Plan B fits the same cadence as Plan A. What it **does** promise: every milestone has a goal, work
packages, dependencies, a falsifiable criterion, a named probe and a `u` range — and the ending is
**M48 + E8**.

---

## Next step

**M36** (the audit and its instrument), **M37** (the seven-axis layer exercised by three pilots), **M38**
(UI-2: 16 creation + 11 configuration + menus) and **M39** (UI-3: 51 interactive playable windows) are
DONE at gate level on 2026-10-27 — acceptance PENDING, no tags — and the
seven-axis contract they consume is in place (section 4.3), with M37 measuring its cost at
**0.4708333 u/window at one QA round**, band `r`. M40 (UI-4) closed at gate level on 2026-10-27 —
`UI-FULL-001` 85/85, 8/8 rules, 4/4 falsifiers, two processes byte-identical. The next delivery gate is
**M41** (SYS-1: depth in combat, magic, items, stats and resistances), `AUSENTE` today and dispatched
only with an explicit go. The stale
forward-plan sentence below is kept verbatim as this file's single declared understatement defect
(`M38`, registered by `CR-71f2b7bf`): **M38** (UI-2: 16 creation + 11
configuration + menus), which is **not started** and requires an explicit go before any dispatch. See
[Agent entry point](agent-entrypoint.md) for the operating rules, limits and how to continue.







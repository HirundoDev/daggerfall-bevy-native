# Roadmap — M18 to M48, plus E8

> **What this is.** The complete, single plan of the program up to its declared ending:
> **Plan A** (M18–M35 — foundation and vertical slice: M18–M29 measured and technically closed;
> M30 frozen; M31–M35 specified) plus **Plan B**
> (M36–M48 — the epics E1–E7 turned into milestones with an atomic work breakdown) plus
> **E8** (a Starfield-like horizon, a **separate program** after M48, never "M49").
>
> **What this is not.** It implements nothing, signs no milestone, creates no tag, promises no
> dates and rewrites no history. Every milestone below is described in the present tense of a
> plan, not of a delivery.
>
> **Unit of work.** `u` is the program's volume unit, on the same scale as the Plan A records.
> It is **not** a duration and it is never converted into a date. `u` figures carry a range;
> the nominal is the centre, the high bound is the realistic scenario for a stretch without
> golden references.

**Status legend.** `DONE` = probe passed and QA at or above its bar (0 critical / 0 major).
`WIP` = code in flight. `PARTIAL` = pieces delivered, closure criterion unmet.
`SPEC-ONLY` = written specification, no code. `NOT STARTED` = neither specification nor code.

---

## 0. Status at a glance

| Block | Milestones | State |
|:---|:---|:---|
| Closed, technically | M19–M29 (11 milestones) | **DONE** (each with its probes and QA score) |
| Open | M18 | **PARTIAL** (technical pass incl. the B3 tanda at 7/7 CS; owner signature and tag pending) |
| Frozen | M30 | **FROZEN** (written specification, no code) |
| Specified, not implemented | M31–M35 (5 milestones) | **SPEC-ONLY** |
| Plan B | M36–M48 (13 milestones) | **NOT STARTED** |
| Separate program | E8 | **NOT STARTED, not estimable** |

Recount: **11 DONE · 1 PARTIAL · 1 FROZEN · 5 SPEC-ONLY · 13 NOT STARTED · 1 separate program.**
No milestone tag exists for M18 or later.

---

## 1. What "100%" means — four definitions that are never mixed

The original defect in the definition of done was reading "100% of the data DoD" as "100% of the
game"; the symmetric error would be selling "100% original" as "the full vision". This roadmap keeps
them apart.

### 1.1 The four definitions

| Definition | Milestones | What it includes | Size |
|:---|:---|:---|:---|
| **Plan A** (foundation + slice) — **planned scope, not delivery** | **M18–M35** | retail data, 49 systems at slice depth, presentation UI, E2E and save. State today: **M18 PARTIAL · M19–M29 DONE · M30 FROZEN · M31–M35 SPEC-ONLY** | **51.85 u** with the M35 annex · **49.95 u** without it |
| **100% original** | **M36–M44** | full UI (E1) + real system depth (E2) + dungeon content (E3) | **2.8–5.6×** Plan A |
| **Retail-ready** | **M36–M44 + M47** | 100% original + platform and distribution (E6) | **3.0–6.0×** |
| **Full vision** | **M36–M48** | E1–E7 | **4.9–9.8×** |
| **E8** (Starfield-like horizon) | **E8** | new program, new genesis contract | **not estimable** |

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
"13 modules" and expects the pace of M19–M29 is off by an order of magnitude.

### 1.4 Measured corrections of premise

Five status claims circulated before this plan; each is kept as intent and corrected where measurement
says otherwise. None of them reduces scope; all of them prevent a false gate.

| # | Premise | Measured | Consequence |
|:-:|:---|:---|:---|
| **C-1** | "M18–M32 are DONE" | **M18 PARTIAL**; **M19–M29 DONE** (11 rows, technical closure, no tags); **M30 FROZEN** (spec, no code); **M31–M34 SPEC-ONLY** (not implemented); **M35 SPEC-ONLY** optional; **M36–M48 NOT STARTED**; **E8 separate** | the matrix marks M18 `PARTIAL`; Plan B does **not** depend on its signature |
| **C-2** | "UI is 51/85 (60%) delivered" | 51/85 is a census ratio (playable windows within the 85), not a delivery. Delivered today: **6 static panels** and a shell with a registry and a stack | M36 measures the real per-window state; the M40 gate is per **interactive flow**, not per "panel present" |
| **C-3** | "34 are missing" | 34 = 16 character-creation + 11 configuration + 7 infrastructure | **M38 reverses part of that `N/A`**: the 11 configuration pages enter with **functional** parity (not pixel). The 7 infrastructure entries stay out: they are base classes, factories and constants, not windows |
| **C-4** | "Gate: 85/85 interactive windows" | 7 of the 85 are not windows | the gate is stated exactly: **78/78 interactive windows + 7/7 classified infrastructure + 0 rows without a verdict = 85/85 resolved** |
| **C-5** | "Dialogue only in trade, no exterior–interior transition" | the dialogue **UI consumer** does not exist; the dialogue **system** does (M26 DONE); the exterior→interior transition is measured | the premise describes the public `0.1.0` surface; M36 treats it as a hypothesis to measure, not a fact of HEAD |
| **C-6** | "13.1 u/day" | correct as a measured reference for work **with golden references** | used only as an order of magnitude; **zero dates** |

---

## 2. Canonical matrix — M18 to M48 plus E8

| M | Epic | Scope | State | `u` remaining |
|:-:|:---|:---|:---|--:|
| **M18** | slice | presentation parity + map closure | **PARTIAL** (technical pass, B3 tanda at 7/7 CS; owner signature pending) | 5.60 |
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
| **M30** | slice | economy and banking | **FROZEN** (written specification, no code) | — |
| **M31** | slice | travel, weather, calendar | **SPEC-ONLY** | — |
| **M32** | slice | conditions, death, rest | **SPEC-ONLY** | — |
| **M33** | slice | total persistence (save v2) | **SPEC-ONLY** | 1.65 |
| **M34** | slice | end-to-end integration + presentation | **SPEC-ONLY** | 5.85 |
| **M35** | slice (annex) | biomes, 2D vegetation, sky | **SPEC-ONLY** (non-blocking, `0.1.0`) | 1.90 |
| **M36** | **UI-0** | audit of the 85 windows + patch/rewrite decision + compatibility contract + E1 re-estimate | **NOT STARTED** | 2.00 |
| **M37** | **UI-1** | UI compatibility layer (7 axes) | **NOT STARTED** | 4.50 |
| **M38** | **UI-2** | 16 creation + 11 configuration + menus (27) | **NOT STARTED** | 6.50 |
| **M39** | **UI-3** | 51 interactive playable windows | **NOT STARTED** | 7.50 |
| **M40** | **UI-4** | UI gate 78/78 + side-by-side + input + accessibility | **NOT STARTED** | 3.50 |
| **M41** | **SYS-1** | depth: combat, magic, items, stats, resistances | **NOT STARTED** | 63.00 |
| **M42** | **SYS-2** | depth: NPCs, guilds, crime and law, quests | **NOT STARTED** | 42.00 |
| **M43** | **SYS-3** | depth: economy, travel, weather, calendar, conditions | **NOT STARTED** | 51.00 |
| **M44** | **DUN-1** | dungeon content: grid + loot/respawn/bosses/instances | **NOT STARTED** | 39.00 |
| **M45** | **E4 MODS** | declarative mod pipeline | **NOT STARTED** | 39.00 |
| **M46** | **E5 ONLINE** | optional online: lobby, instances, authority | **NOT STARTED** | 78.00 |
| **M47** | **E6 PLATFORM** | packaging, multi-OS CI, sustained perf, accessibility | **NOT STARTED** | 15.50 |
| **M48** | **E7 MODERN** | opt-in modern layer (pristine/modern) | **NOT STARTED** | 31.00 |
| **E8** | **Starfield-like** | separate program, **after M48**, new genesis contract | **NOT STARTED / NOT ESTIMABLE** | — |

**Reading note.** The 31 rows M18–M48 are the program; **E8 is not row 32** and not an "M49": it is a
program with its own genesis contract, its own definition of done and its own budget.

**Frozen and spec-only block.** M30 economy and banking is **frozen**: a written specification and no
code. M31–M35 keep one written specification each and no implementation. `—` in the `u` column means the
remaining envelope of that block was not re-measured in this edition; the Plan B figures in §3 stand.

---

## 3. Atomic work breakdown for Plan B

**Closure rule for every work package (inherited):** `probe PASS → printable demo/evidence → QA at or
above its bar (0 critical / 0 major) → commit with a named pathspec → closure record`.
A work package without a probe is not a work package.

> Plan A is **not duplicated** here. Its atomic work breakdown stays the reference for M18, M33, M34 and
> M35 until those milestones close; this document cites it and only summarises it in sections 2 and 6.

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

**Closing E1:** M40 PASS ⇒ **complete UI**; the 34 non-playable surfaces are resolved (11 with
functional parity, 7 classified, 16 with flow parity).

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

**Goal:** the **geometry is specified in M34** (SPEC-ONLY, not implemented); here enters the **content** and its state. The working
specification today is an analysis, not an acceptance spec: M44's first act is to write the spec with
golden vectors.

| WP | Deliverable | Exact acceptance | Probe / gate | Deps | u |
|:--|:---|:---|:---|:---|--:|
| **W44-0** | Content spec with golden vectors (3 layers, state machines B and C, loot tables, lockouts) | spec with a measurable criterion per rule; 0 rules without a golden vector | spec + review | M34 geometry, design analysis | 3.00 |
| **W44-1** | **Layer A** complete: 4,232 dungeons, 51.2 m grid, doors/connections/props, occlusion, frame budget | 4,232/4,232 parsed; digest ×2; 0 overlapping doors (1.4 m tolerance) | `DUN-GEO-001` | M34 geometry, W44-0 | 8.00 |
| **W44-2** | **Layer B** world state: doors/levers/traps, dead bosses, shared containers; machine `Dormant/Open/Engaged/Cleared/Cooldown/Closed` | every transition with a case and a counter-case; `Dormant` = 0 simulation (measured) | `DUN-STATE-001` | W44-1, M33 | 7.00 |
| **W44-3** | **Layer C** eligibility: tokens per `(seed, container, player, epoch)`, `SEEN` idempotency, lockouts | same container twice = same content (idempotent); a new epoch invalidates tokens | `DUN-ELIG-001` | W44-2 | 8.00 |
| **W44-4** | Loot/respawn/bosses/instances: tables per level/region, chests vs drops vs bosses, `T_respawn` 20/40/30 min, 5 anti-farm rules, the contract **"never looted on entry"** | 5/5 anti-farm rules falsifiably exercised; contract measured in 3 dungeons × 3 epochs | `DUN-CONTENT-001` | W44-3, W41-5 | 8.00 |
| **W44-5** | Persistence: new versioned **save v3** sections + v2→v3 migration (G-N10) | byte-identical round trip; v2→v3 migration 3/3; digest ×2 | `DUN-SAVE-001` | W44-2, M33 | 3.00 |
| **W44-6** | Probes + adversarial (2 players, same container/epoch) + QA | PASS + adversarial 3/3 + QA at or above its bar | `DUN-CONTENT-002` | W44-1..5 | 2.00 |
| | **M44 total** | | | | **39.00** (25.50–52.00) |

**Boundary:** **MMO content** (raids, world bosses, trading) is E5/M46, not M44. M44 is single-player
correct with layers A/B/C already separated: implementing layer C by deleting from B violates the design.

### 3.10 M45 — E4: declarative mod pipeline (39.00 u; range 26.00–52.00)

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

### 3.14 E8 — Starfield-like horizon: a **separate** program (not estimable, **not M49**)

| Field | Content |
|:---|:---|
| **What it is** | A deferred requirement: a Starfield-like expansion horizon (world scale, simulation, generated content). It is not a layer of this game: it is **another game** on the same engine |
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
| **M42** SYS-2 | 42.00 | 28.00 | 56.00 |
| **M43** SYS-3 | 51.00 | 33.50 | 67.50 |
| **E2 — depth (M41–M43)** | **156.00** | **102.80** | **208.20** |
| **M44** DUN-1 | 39.00 | 25.50 | 52.00 |
| **E3 — dungeon (M44)** | **39.00** | **25.50** | **52.00** |
| **100% original (M36–M44)** | **219.00** | **146.40** | **290.55** |
| **M45** E4 MODS | 39.00 | 26.00 | 52.00 |
| **M46** E5 ONLINE | 78.00 | 51.00 | 104.00 |
| **M47** E6 PLATFORM | 15.50 | 10.00 | 21.00 |
| **M48** E7 MODERN | 31.00 | 20.00 | 41.00 |
| **Full vision (M36–M48)** | **382.50** | **253.40** | **508.55** |
| **E8** | **not estimable** | — | — |

**Check against the declared rule:**

| Rule | Arithmetic | Result |
|:---|:---|:---|
| `100% original` = 2.8–5.6× | 146.40 / 51.85 = **2.824** · 290.55 / 51.85 = **5.604** | **2.8–5.6×** |
| `Retail-ready` = 3.0–6.0× | (146.40+10.00)/51.85 = **3.016** · (290.55+21.00)/51.85 = **6.009** | **3.0–6.0×** |
| `Full vision` = 4.9–9.9× | 253.40 / 51.85 = **4.887** · 508.55 / 51.85 = **9.808** | **4.9–9.8×** (inside the band) |
| E1 inside 0.3–0.6× | 18.10 ≥ 15.56 · 30.35 ≤ 31.11 | inside, upper half |
| E2 inside 2–4× | low 102.80 vs 103.70 = **−0.90 u** · high 208.20 vs 207.40 = **+0.80 u** (both from rounding to whole `u`) | **declared edge**: neither the band nor the figure is massaged |
| E4/E5/E6/E7 inside their bands | 0.5–1 / 1–2 / 0.2–0.4 / 0.4–0.8 | inside (26–52 · 52–104 · 10–21 · 21–41) |

> **Honesty note.** Bounds are set by rounding to whole `u` **after** assigning per work package; E2
> lands 0.9 u below and 0.8 u above its nominal band and E1 in its upper half. The work packages are not
> "fixed" to fit: the rounding is declared. **The band rules over the sum**, and any deviation is
> resolved by measuring at `W41-0` / `W36-5`, not by arithmetic.

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
Plan A (pending)                            Plan B
M18 ─┐                                      M36 ─► M37 ─┬─► M38 ─┐
M33 ─┼─► M34 ─► M35 (annex)                             └─► M39 ─┴─► M40 ─┐
     │        │                                                            │
     └────────┴─► M41 ─┐                                                   │
                  M42 ─┼─► M44 ─► M45 ─┐                                   │
                  M43 ─┘        └─► M46 ─┐                                 │
                                          ├─► M47 ─┐                       │
     M40 ─────────────────────────────────┘        ├─► M48 ─► E8
                                                   └─► (release 1.0.0)
```

**Hard edges (the ones that cannot move):**

| Edge | Reason |
|:---|:---|
| `M36 → M37` | no audit, no decision; no decision, no layer |
| `M37 → M38`, `M37 → M39` | the windows consume the contract (section 4.3); without it each window invents its own binding |
| `M38/M39 → M40` | the gate measures the whole; there is no partial gate |
| `M40 → M48` (modern UI) | the modern layer rests on the parity UI, it does not replace it |
| `M41 → M44` | dungeon loot uses item tables and entities from M41 |
| `M33 → M44-5` | dungeon content requires save **v3** on top of v2 |
| `M44 → M45`, `M44 → M46` | packs and online need the world state (layers B/C) |
| `M44 → M47` | packaging a game with complete content |
| `M42 → M44` (soft) | quest bosses depend on deep quests; otherwise M44 uses **declared** placeholders |

### 5.2 Critical path (nominal)

`M41 (63) → M44 (39) → M46 (78) → M47 (15.5) → M48 (31)` = **226.5 u** of 382.5 ⇒ the critical path
is **not** the UI: it is **depth → dungeon → online → platform → modern**. The UI (24.0 u) runs in
parallel and does **not** move the ending; it moves the **perceived value** earlier (E1 closes first and
enables an early side-by-side).

> **Recommended order (derived, not invented):** `M36` **now** (it is cheap and unlocks everything), and
> in parallel `M41-0` (re-measurement) + `M41-1`. The UI does not wait for depth and depth does not wait
> for the UI.

### 5.3 Parallelisable without file conflicts

| Parallelisable | Base | Precaution |
|:---|:---|:---|
| `M36` ∥ `M41-0` | documentation instrument vs system matrix | 0 overlap |
| `M38` ∥ `M39` | creation/config UI vs in-play UI | **only** after `M37`; both touch the UI spec ⇒ serialise per file or split the module |
| `M41` ∥ `M42` ∥ `M43` | combat / behaviour / heart crates | already separated in Plan A; calendar is touched by two ⇒ coordinate |
| `M45` ∥ `M47` | packs vs packaging/CI | both touch CI scripts ⇒ one owner |
| **Do not parallelise** | — | `M40` with anything (it measures the whole) · `M44-5` with M33 open (migration on a moving base) · `M46` without an owner go |

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
| **R-12** | Plan A mis-cited (M18 `PARTIAL` read as `DONE`) | false gates | section 1.4 C-1 and the matrix in section 2: the measured state rules |

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
| **Transcription with golden vectors** | M38 (creation part), M42-4/M42-5 (action goldens), M43 (tables specified in M30–M32, not implemented) | ×1.0 | the `u` measures transcription |
| **Extension of an existing system** | M41, M42, M43 | ×1.3 | density without a new spec per rule |
| **New design without a golden** | M36, M37, M39, M40, M44, M45, M46, M48 | ×1.6–2.0 | where the historical record does **not** calibrate |
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
| **G-N5** | **The UI has no audit instrument**: no executable census of the 85 windows and no fit matrix; the measurable state today is 6 static panels and 2 verbs | `W36-1`/`W36-2` (`UI-AUDIT-001`) |
| **G-N6** | **M36–M48 have no requirements yet**: the requirements inventory stops before them | requirements addendum per milestone (nominal reserve 070–082), section 0.3 of the internal record |
| **G-N7** | **Declared partial reversal** of the `N/A-engine` verdict on the 11 configuration pages | `W38-2` with the pristine gate |
| **G-N8** | **"85/85 interactive" is impossible** (7 of 85 are not windows) | exact gate in 3.5/4.6 (78+7) |
| **G-N9** | **No golden vectors per window or per effect**: Plan B inherits the gap | M36 (UI), `W41-3` (effects), `W44-0` (dungeon) write their spec with golden vectors |
| **G-N10** | **Save v2 does not cover dungeon state** (layers B/C, epochs) | `W44-5` (save **v3** + migration) |
| **G-N11** | **E1 envelope is tight**: 5 UI milestones in 15.6–31.1 u; if the ADR says REWRITE, the high bound pinches | `W36-5` re-estimates **before** `M37` |
| **G-4** | **The 49-system matrix has not been refreshed** since 2026-10-20 | `W41-0` |
| **G-N2** (inherited) | Some lanes of a previous cycle closed without a cost record; per-lane cost is incomplete | fixed from now on; historical values are not filled in by hand |

---

## 8. One canonical plan (supersession)

**Document roles today:**

| Record | Role | Relation to this roadmap |
|:---|:---|:---|
| `roadmap-m18-m48.md` (**this**) | **the single canonical plan** for M18–M48 + E8 | — |
| the M18–M35 roadmap | **superseded as history** — kept whole | for M18, M33, M34 and M35 its atomic work breakdown **remains the reference** while they are open: this document cites it, it does not rewrite it |
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
| Requirements | 042–069 | + **nominal reserve 070–082** (activation by addendum) |

---

## 9. Declared residuals (not hidden)

1. **M18 remains `PARTIAL`**: Plan B does not wait for its signature, but the M18 tag is the owner's.
2. **M33 remains `SPEC-ONLY`**: three Plan B work packages depend on save sections; if M33 closes first, their
   QA is consumed and that save stretch disappears (0 duplicated work).
3. **The UI today is 6 static panels**; the census of 85 belongs to the original, it is not a delivery.
4. **Plan B `u` has no historical series**: nominal plus range, never a date (6.4, R-9).
5. **E2/E4/E3/E5 high bounds rounded to whole `u`** exceed their band by less than 0.5 u (3.15, 6.2).
   The rounding is declared, not disguised.
6. **M46 does not exist without an owner go**: the plan describes it, the owner authorises it.
7. **E8 has no estimate** and will not have one until its own genesis contract.
8. **No UI golden vectors**: `W36-2` decides by measurement, but the historical record does not calibrate
   that stretch (6.3).

---

## 10. Program closure criteria (what the reader can hold us to)

| State | Means | Does not mean |
|:---|:---|:---|
| **M34 PASS** | **Plan A closed**: foundation + vertical slice + E2E/presentation integration | "Daggerfall complete" |
| **M35 PASS** (`0.1.0`, **optional annex**) | annex complete (biomes, 2D vegetation, sky) within the `0.1.0` surface; **not a closure gate** | "a necessary condition to close Plan A" |
| **M40 PASS** | UI 78/78 + 7/7 classified, complete input, accessibility | "game finished" |
| **M44 PASS** | **100% original**: UI + depth + dungeon content | "store-ready" |
| **M47 PASS** (`1.0.0`) | **Retail-ready**: multiplatform, packaging, 60 fps, audited accessibility | "full vision" |
| **M48 PASS** | **Full vision** (E1–E7) with byte-behavioural pristine | "Starfield horizon" |
| **E8** | a **new** program with a new genesis contract | nothing above |

**Plan A cut (declared correction).** Plan A closes with **M34 PASS** (**49.95 u** strict). **M35** is an
**optional, non-blocking annex** planned for the `0.1.0` surface; its PASS is **not** required to
declare Plan A closed. The multiplier base (1.2, `B = 51.85 u`) **includes** the M35 annex for continuity
of the historical series; it is declared so no reading confuses "Plan A closed" (M34) with "Plan A with
annex" (M34 + M35).

**What this document does NOT promise:** dates, durations, a "months" figure without measurement, or that
Plan B fits the same cadence as Plan A. What it **does** promise: every milestone has a goal, work
packages, dependencies, a falsifiable criterion, a named probe and a `u` range — and the ending is
**M48 + E8**.

---

## Next step

The immediate next gate is **M36**, and it is a measurement gate, not a code gate: it produces the audit
instrument, the patch/rewrite decision, the compatibility contract and the E1 re-estimate. It is **not
started** and requires an explicit go before any dispatch. See
[Agent entry point](agent-entrypoint.md) for the operating rules, limits and how to continue.







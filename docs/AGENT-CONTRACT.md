# Executor contract

> Commit shas on this page name commits of the private engineering history unless the object
> resolves in your clone (`git cat-file -t <sha>`); the published history is grafted, so a
> source-tree sha does not resolve here.

> **Purpose:** define what an **executor** (cheap producing model) may and may not do
> versus an **auditor** (the captain, who certifies). The contract governs every run without renegotiation
> and does not depend on the model's memory.
>
> **Portability:** §0-§8 are normative and generic: they name no repo and no absolute path.
> Copy this file to another project-target unchanged: the normative part is agnostic. The final
> **annex** is one concrete case, marked as non-normative.
>
> **Self-loaded stub:** the `CONTRATO-EJECUTOR` block of `AGENTS.md` summarizes the minimums and points here.
> This document rules; the block is its telegraphic form.

## §0 — Roles and authority

| Role | Decides | Produces | Certifies | Hard boundary |
|---|---|---|---|---|
| **Owner** (human) | Irreducible GO: scope, canon, spend, publication | — | — | Never self-delegated; everything else escalates here |
| **Captain** (target principal) | Decides with a cited source (`file:line`, ledger row, verdict or prior GO) | — | **YES, it is the only auditor** | Without a cited source it does not decide: it opens a pending decision |
| **Orchestrator** | Routing and decomposition: which lane, with which pack, with which budget | Lane plan | Does not certify the product | Produces no domain work and does not audit its own lane |
| **Executor** (cheap model) | No scope decisions: executes the closed assignment | Code, docs, measurements | **NEVER** | Its maximum state is `candidato` |
| **Auditor** (= captain, or a lane with an explicit audit mandate) | Accepts / re-delegates / escalates | Verdict | **YES** | Does not inherit claims: re-derives them |

**Authority rule:** the captain is the **only auditor**. The executor never audits, not even itself,
not even when it "just checks that it came out right". An executor that issues a verdict on its own work
has left the contract, even if the verdict is correct.

## §1 — The 8 non-negotiable minimums

Each clause is literal and binding. The consequence line says what it implies in practice.

- **(a)** The gate/instrument is written FIRST, by another lane, and the executor cannot edit it.
  - Consequence: if the instrument does not exist at start-up, the executor stops and reports it; it does not invent it.
- **(b)** Deliverable = commits by individual pathspec + raw command output; the word DONE/PASS is never pronounced by the executor: its maximum state is `candidato`.
  - Consequence: the report ends at `candidato` and someone else writes the verdict.
- **(c)** If a figure does not hold, it is withdrawn; the instrument is never adjusted to the figure.
  - Consequence: faced with a metric that does not add up, the figure is withdrawn and the hole is declared; the threshold is not relaxed.
- **(d)** 0 mock / 0 TODO / 0 placeholder / 0 unsafe / 0 unwrap in probes.
  - Consequence: a probe that aborts on `unwrap`/`expect` yields a red row, not an opaque exit.
- **(e)** Forbidden to write self-verification probes, or to edit the closure register, the closures index or any ledger.
  - Consequence: closure ledgers are append-only and their row is written by whoever closes, not by whoever produces.
- **(f)** 0 push / 0 tag.
  - Consequence: the work lives in local commits; publishing is a captain/owner decision.
- **(g)** Context checkpoint: at 70% it emits a handoff and closes.
  - Consequence: an honest partial closure beats a complete closure invented out of exhaustion.
- **(h)** Every claim must carry `file:line` or a re-runnable command, and the auditor re-derives it with a cold build: it does not inherit.
  - Consequence: a claim with no anchor is not evidence; it is prose and is discarded.
## §2 — Anatomy of the deliverable
## §2 — Anatomía del entregable

1. **Commits by individual pathspec.** One `git add <path>` per commit. Never `git add -A` or `git add .`:
   the shared tree carries other lanes' changes and the global commit hijacks them.
2. **Raw output.** Every command that supports a claim is pasted with its observed output
   (`command -> output`). The interpreted summary goes separately; it never replaces the output.
3. **Declared state `candidato`.** The deliverable never declares itself finished. It declares
   *what was done*, *what was verified by running it* and *what remains open*.
4. **Report format to the auditor** (minimum):
   - executed scope vs. requested scope (one line per divergence);
   - list of commits (sha + pathspec);
   - commands run with their output;
   - claims with `file:line` or a re-runnable command;
   - withdrawn figures (and why they did not hold);
   - open holes, with the exact blocker.
5. **Tree boundary.** Files of other lanes in flight are not touched; other lanes' changes are not reverted.

## §3 — Auditor verification

The auditor **re-derives**, it does not read and accept:

1. **Cold build.** Rebuilds from scratch (no cache, no previous artifacts). A warm green does not
   count as green.
2. **Re-derivation of every figure.** Every metric in the report is recomputed with its own command.
   A non-reproducible figure means it is withdrawn from the report and a finding is opened.
3. **`file:line` mandatory.** A claim with no verifiable anchor is not audited: it is returned.
4. **No inheritance.** The auditor does not inherit claims from the executor or from previous audits; it inherits
   only the instrument and its output.
5. **Separation of gates and verdicts.** A mechanical gate (lint, closures linter, validator) produces
   PASS/FAIL; the semantic verdict (does this meet the assignment?) belongs to the auditor and goes in a separate row.
6. **Closure audit (§9) mandatory.** Before accepting, the auditor runs the mechanical closure
   auditor over the CR (or the range) and attaches its matrix. A §9 FAIL is not
   compensated with prose: the closure is rejected or re-delegated.

## §4 — Context checkpoint at 70%

On reaching ~70% of the context budget, the executor:

- **emits a handoff** with: done / verified / open / for whom, plus the paths touched and the
  commands run;
- **closes** the lane in state `candidato` (or `PARTIAL` if the assignment was not completed);
- **does NOT declare** cycle closure, nor acceptance, nor PASS: that is the auditor's;
- **does NOT** try to "finish fast" by trimming verification: it trims scope and declares it.

## §5 — Good assignment / bad assignment

| Good assignment (delegable at night) | Bad assignment (NEVER to an executor) |
|---|---|
| Code against an already written contract | Writing its own verification |
| Mechanical refactor with explicit criteria | Deciding coverage |
| Implementing a closed spec | Declaring closure |
| Repairing findings with a prior report | Auditing itself |
| Filling a table whose schema already exists | Arbitrating state (who is right) |

Cut criterion: if the assignment requires **deciding** (what counts as done, what it covers, who is
right), it is not an executor assignment. If it requires **producing against already written criteria**, it is.

## §6 — Why the separation is structural

The last 3 milestones showed **honest product** (552/552 real lines, 0 `unsafe`, 0 mock)
next to **inflated measurement** (0/16 real coverage; a gate that compared its own literals;
a delta with no consumer). The code was true; the number that certified it was not. The cause was not
bad intent: it was **position** — the same actor that produced measured itself, and measuring your own work
biases what counts as finished. That is why the executor/auditor separation is **structural, not
cosmetic**: it is not fixed by asking for honesty, it is fixed by taking the certification pen
away from the producer.

## §7 — How to adopt it in another target

1. **Copy this document as is** to the target's `docs/AGENT-CONTRACT.md`: §0-§8 are agnostic and
   require no adjustment at all. If the source case is to be kept as a sample, replace the
   content of the **annex** (marked NON-normative) with that of the new repo.
2. **Paste the block** of `AGENTS.md` (markers `CONTRATO-EJECUTOR:INICIO` … `CONTRATO-EJECUTOR:FIN`)
   as the last section, respecting the block's ~1.5 KB cap.
3. **Declare what an "instrument" is in that repo**: which gate/linter/validator measures, who writes it and
   who may not edit it. Without that declaration, clause (a) has no referent.

**Durability notice:** if `AGENTS.md` is **generated by a sync** from an external source, the block is
lost on the next sync application. The markers exist for that: they allow **re-seeding** it
after every sync and **detecting the loss mechanically** (look for the start marker and count
exactly 1). Real durability requires the block to live also in the sync **source**; until
then, manual re-seeding is the mechanism and the marker check is its alarm.

## §8 — Contract death metrics

The contract is dead (it does not bite) if, in a window of **≥10 closures**:

1. the auditor **withdrew no inflated figure** — a sign that it does not re-derive, only reads; and
2. **found no claim without `file:line`** — a sign that it does not audit anchors.

Either of the two, sustained over the window, forces a review of the instrument (not of the executor):
a contract that never produces a finding is not measuring anything.

## §9 — Mechanical closure audit (MANDATORY step)

No closure is accepted for what its report says: it is accepted for what the
history says. This step is mandatory and runs **before** proposing a closure:

1. **The instrument.** An independent closure auditor that receives **one CR** or **one
   commit range** and dictates PASS/FAIL without knowing the executor. It does not read the
   lane's reports; it re-derives from git and from the closure files. In this repo:
   `python3 scripts/guard/lane-integrity.py --cr <CR>` (or `--commits a..b`).
2. **What it requires** (a single FAIL fails the closure):
   - **(a) Separation of acts:** no commit touches at once implementation
     (`crates/**`, `src/**`) and gate (`scripts/*-001.py`, `scripts/*gate*.py`).
   - **(b) Prior gate:** the lane's gate exists in a commit **prior** to the
     first implementation commit of the same CR. If the lane touches no gates, a gate written by another lane must
     pre-exist.
   - **(c) State ≤ `candidato`:** HH-1 cannot declare more than `candidato` unless
     there is an **independent** QA row (author ≠ HH-1 author) with a verdict.
   - **(d) Live evidence:** every path cited as evidence exists in the audited
     revision and, if INDEX declares `sha256`, matches the bytes of that file.
   - **(e) 0 push / 0 tag:** no commit in the range is reachable from
     `refs/remotes/*` or from `refs/tags/*`.
3. **Exit codes:** `0` = PASS · `1` = FAIL (with typed findings) · `2` = usage/error.
   The finding code (`A-MIXTO`, `GATE_POSTERIOR`, `STATUS_SIN_QA`,
   `EVIDENCIA_AUSENTE`, `EVIDENCIA_SHA`, `PUSH_DETECTADO`, `TAG_EN_RANGO`) is cited as is
   in the report: the auditor does not paraphrase the finding.
4. **A FAIL is not patched in the lane that suffers it.** It is recorded (HH-1 + INDEX) and its own lane
   is opened. Withdrawing the figure or editing the evidence to turn it green is the
   violation of §1(c).
5. **The instrument falsifies itself:** `--selftest` builds 4 negative cases
   (gate+impl in one commit, gate after the impl, evidence with a nonexistent path,
   high `status` without QA) and 1 no-false-red control, with dangling commit chains
   **over the real repo** (plumbing: 0 refs, 0 HEAD, 0 index). If a case does not bite, the
   instrument exits 1.

**Declared limit (what this step does NOT verify):** if the gate is **weak inside**
(it exists, runs and always goes green) it passes every check; its quality is a matter
of the auditor (§3.5), not of this instrument. Nor does it judge whether the QA row's verdict
is honest (§3.4: it is not inherited, it is re-derived), nor see a push whose remote ref was
deleted, nor a history rewritten by `amend`/`rebase`.

---

## Annex — Case that motivated the contract (NON-normative)

Data from the repo where this document was written; they serve as a sample, not as a rule:

- Audit `CR-276c7d94`, sha `eead706f`: 7 findings `BLOQUEA-M43`.
- Milestone M42 closed as `PARTIAL` with 8 `GAP-M42-01..08`.
- Milestone M41: 3 false claims detected in the re-audit.
- 64.5% of `crates/app/src` are probes (measurement code, not product code).
- 26 `expect()` that abort with exit 101 instead of emitting a red row.

Reading: the product existed and was real; the certification was not. That delta is exactly what this
contract separates.

# Architecture

This repository is an instance of the **Sintergic architecture** — the construction method we are
building, in public, while this engine is built with it. The method is general: it is meant for
applications and systems of any kind, not only games. This engine is its **reference instance** and
the place where the method is falsified: every claim the method makes about how to build software has
to survive contact with a real, data-driven, deterministic simulator whose ground truth is a 1996
retail product.

Nothing here is a secret and nothing here is a product pitch. The method is unfinished, and this
document says which parts are settled and which are still open.

## What this document is, and what it is not

It describes the architecture at the conceptual level: what we commit to, how those commitments turn
into module boundaries, why we chose this shape over the alternatives, and what it costs. It is not a
catalogue of the project's day-to-day working artifacts. Every project built this way produces
operational material — task orders, agent rosters, budget notes, milestone tickets — and that
material is **not** the architecture. Removing it loses no idea; the ideas are here.

The concrete commands, the review checklist and the step-by-step pattern for adding code are in
[CONTRIBUTING.md](CONTRIBUTING.md).

## The commitments

Seven commitments define the method. Each one is chosen because it makes a specific failure mode
impossible or visible; none is decoration.

### 1. Evidence before claims

A statement about the system counts only when it can be re-executed. "It works", "it is done" and
"it looks right" are not evidence; a test, a probe, a digest or a golden vector is. The consequence
is a single direction of proof: **claim → executable check → recorded result**. A reader who
distrusts a status statement can follow that chain to a command they can run.

This is the commitment everything else serves. Organs, contracts and registers exist to make the
evidence cheap to produce and impossible to fake.

### 2. Organs as module boundaries

A system is decomposed into **organs**: small crates, one responsibility each, that talk through an
event bus instead of calling each other. `nervous` owns the bus and the event vocabulary; `heart`
owns time; `memory` owns snapshots and the loaded-chunk set; `behavior` owns rules; `perception`
owns read models; `skills`, `inventory`, `crafting` and `combat` own their domains; `antibodies`
owns invariants. The application composes them.

The boundary is enforced by the dependency graph, not by convention: an organ does not depend on
another organ, and no organ depends on the renderer. That is what makes it possible to test a game
rule in a headless binary with no GPU and no window.

### 3. Contract before code

Before implementation, the change states what it is for in a form someone else can check: the
requirement, the observable outcome, what is explicitly out of scope, and what will count as
acceptance. That contract is written in the change itself (a short design note in the pull request
is enough), not reconstructed after the fact from the diff.

Writing the contract first is not ceremony; it is how the acceptance check gets chosen before the
implementation biases it.

### 4. Probes and antibodies

Two kinds of executable check, with different jobs:

- **Probes** ask whether behaviour is right: `ALL-001` runs the suite, and each probe in it —
  `VIEW-001`, `MAP-001`, `SEED-001`, `COMBAT-001`, `MAGIC-001` and the rest — prints a verdict and
  exits with a code. Probes run headless, so they can run in CI.
- **Antibodies** ask whether an invariant still holds: guardrails over data custody, content scans,
  determinism rules, structural checks. An antibody does not test a feature; it tests that a class of
  mistake cannot enter.

The split matters. A probe that has to be remembered by a human is a promise; an antibody that runs
on every push is a fact.

### 5. Determinism as a testable property

The same seed and the same data must produce the same world, the same names and the same outcomes,
byte for byte. Determinism here is not a performance claim or a marketing word: probes compare
digests, not screenshots, and a graphical run cannot silently change simulation results. Golden
vectors — 0 ULP against a compiled Perlin reference, FNV-1a64 texture vectors, exact text and quest
counts, a full map census — are how the property is pinned.

### 6. Registers that outlive the session

Anything load-bearing is a file in version control, readable at the start of a session and diffable
at the end. `ASSET-REGISTRY.json` records path, size and SHA-256 of every retail asset used;
`DATA-CLASSIFICATION.md` records what may be committed and what may not; `docs/acknowledgments/status.md`
records what each milestone closed and which gaps remain. Nothing important lives only in a chat log,
an editor's binary project state or someone's memory.

### 7. Positions and gates

Work advances through declared positions, each with an entry condition. A position opens only when
the previous one is met by recorded evidence, not when it feels finished. The gate is mechanical
wherever it can be: a script, a scan or a suite decides, and a human decides only what cannot be
mechanised — such as whether the reimplementation looks like the original to someone who played it.

## How the commitments become structure

```text
daggerfall-app                    Bevy 0.20, window/headless, ECS systems, probe dispatch
├── 10 organ crates               pure Rust, no Bevy, event bus only
│   nervous · heart · memory · behaviour · perception · skills
│   inventory · crafting · combat · antibodies
├── daggerfall-parser-bevy-bridge the only place where records become Mesh/Image/material handles
└── daggerfall_parser_rs          zero-dependency leaf: clean-room BSA and record parsers
```

Four structural rules follow from the commitments:

1. **The parser is a leaf.** It has zero engine dependencies and knows nothing about Bevy, so it is
   testable in isolation and cannot be contaminated by rendering concerns.
2. **Organs carry no Bevy types.** A render-side rewrite cannot reach a game rule, and a rule change
   cannot force a render change.
3. **One bridge.** Conversion from parsed records to engine assets happens in exactly one crate,
   which is where hallucinated second pipelines would otherwise appear.
4. **The app owns custody.** `PathChecker` is the single predicate for where game data comes from
   (the player's own installation, or a custom universe), so "which data is this run reading?" is one
   question with one answer, never a string spread across systems.

## The loop every change follows

```text
requirement → contract/spec → probe → code → test → QA → record
```

The requirement says why. The contract says what will count. The probe is written before or beside
the code, so acceptance cannot be redefined by the implementation. Tests pin the details. QA checks
that the numbers in the record come from a run. The record keeps the result where the next session
can find it. [CONTRIBUTING.md](CONTRIBUTING.md) turns this loop into concrete steps, including for a
project that does not use our scripts.

## Why this architecture, and not another

Every alternative below is a real, defensible way to build this system. Each is rejected for a reason
specific to the constraints here: a retail data source of truth, a simulator that must be
reproducible, and a codebase meant to be adopted and extended by strangers.

**A — a layered monolith (MVC, clean or hexagonal layering).** *Buys:* familiarity, tooling, and a
shape every developer can navigate on day one; controllers, services, repositories and entities map
cleanly onto CRUD applications. *Why not:* layering organises *technical* roles, while this domain is
organised by *responsibility* — time, memory, rules, perception, combat. A layered game grows a
service layer that knows everything, the "domain" ends up importing the framework anyway, and nothing
in the layering speaks about executability, so the evidence discipline has no home in it. *Cost
accepted:* a newcomer must learn the organ vocabulary before the layout reads naturally.

**B — microservices, or modules deployed as services.** *Buys:* independent deployment, fault
isolation, per-service scaling, hard process boundaries. *Why not:* the engine is a single-player
process that must be deterministic on a desktop. Services add network boundaries — latency, partial
failure, serialization, clock skew — to a simulation whose whole value is reproducibility. The organ
boundary already gives in-process modularity; if a server-authoritative mode ever appears, an organ can
be lifted behind a transport without redesigning the core. *Cost accepted:* everything ships as one
binary today.

**C — a pure ECS with no organ boundaries ("entity soup").** *Buys:* maximum data locality, minimal
abstraction, the shortest path from idea to running system; many Bevy projects are built this way and
it works. *Why not:* without declared ownership, systems multiply, every system may touch every
component, a rule becomes untestable without the full world, and a change's blast radius is discovered
at runtime. ECS stays the substrate here; the point is to stop it from being the only structure.
*Cost accepted:* small features pay a small structural tax.

**D — prototype-first, document later.** *Buys:* speed, motivation and early fun; it is how most games
start and a legitimate way to discover what the game is. *Why not:* the ground truth is an external,
byte-exact format with partly undocumented behaviour, and correctness cannot be judged by eye.
Prototype-first produces a demo that cannot be verified against the original nor distinguish a real
regression from a rendering preference. *Cost accepted:* the first milestones shipped parsers and
digests, not a fun demo.

**E — full specification up front (big design up front / waterfall).** *Buys:* a complete model of the
system before code, with no rework from misunderstanding. *Why not:* the public specification of the
retail formats is incomplete and partly wrong; the only way to learn the rest is to parse the bytes and
compare. A complete specification written before contact with the data would be fiction with good
formatting, so the method specifies in *increments* that end in a probe. *Cost accepted:* no finished
blueprint; the roadmap lists stages and their acceptance, not a final design.

## What this architecture costs

- **A discipline tax on small changes, and slower starts.** Writing a requirement and a probe for a
  two-line fix costs more than the fix, and the first weeks produce parsers, digests and invariants
  rather than features. The method is applied in proportion; a typo does not need a contract.
- **A larger documentation surface.** Every status statement owes an evidence path. That surface must
  be maintained or it becomes decoration, which is the failure mode this method exists to prevent.
- **Gates can turn into ritual.** A gate passed without being understood is worse than no gate. When
  a check stops discriminating, it is rewritten or removed — not venerated.

## When this architecture is over-engineering

Do not use it — or use it in a reduced form — when:

- the artifact is **throwaway** (a spike, a one-off script, a demo for tomorrow);
- there is **no external source of truth** and no long-lived invariant, so a probe would only restate
  the implementation;
- the project is **one person for one week**, where the cost of the registers exceeds the cost of
  remembering;
- the requirement is **exploratory**: you do not yet know what to build, and measurement would pin a
  guess. Explore first; adopt the method when a decision needs defending.

The reduced form is simple: keep the *requirement → check* direction, drop the registers and the
gates. That still buys falsifiability at almost no cost.

## Status of the architecture

**Proven in this repository.** The evidence discipline, organ boundaries, contract-before-code,
probe-and-antibody split, determinism as a property and registers that survive a session are all
exercised end to end: 30 milestones with recorded technical acceptance (M18–M29 still awaiting the
owner signature), a 37-probe headless suite, 854 workspace tests passing, clippy clean in both
configurations, and two CI guardrails that reject retail content and unauthorised marks.

**Still open.** These are real gaps, not roadmap poetry:

- **Automation is partial.** CI runs the content and trademark scans; the test, clippy and probe
  battery is not yet wired into it as required jobs. A gate that depends on the developer running it
  is a promise, not a fact.
- **The method's tooling is immature.** The position gates, registers and review checklists are
  currently maintained by hand and by repository conventions. There is no packaged toolchain a third
  party can install.
- **Cost and effort are not measured.** The method claims to make progress visible; it does not yet
  make the *price* of progress visible.
- **Acceptance is still partly human.** Technical closure is mechanical; visual and experiential
  acceptance belongs to a person who played the original, and that step is neither automated nor
  reproducible.
- **Validation outside this instance is thin.** One reference implementation is not evidence that the
  method generalises. Adopting it in a second, unrelated project is the test that has not run yet.

**How a third party can adopt or criticise it.** Adopt the loop from
[CONTRIBUTING.md](CONTRIBUTING.md); it needs no tooling from this project. Then attack the weakest
points: try to pass a gate without understanding it, write a probe that restates the implementation,
or keep a status claim alive after its evidence rotted. Those are the failure modes the method is
supposed to prevent, and a reproducible report of one is the most useful contribution this
architecture can receive.

## How to read this repository as an instance

| Commitment | Where it is visible | How to falsify it |
|---|---|---|
| Evidence before claims | `docs/acknowledgments/status.md`, the probe suite | Run `./scripts/launch.sh --probe ALL-001` and compare with the recorded total |
| Organs as boundaries | `crates/`, workspace `Cargo.toml` | Inspect the dependency graph; no organ depends on another organ or on Bevy |
| Contract before code | Pull requests and design notes | Read the contract before the diff; check the acceptance check was not written after it |
| Probes and antibodies | Probe IDs in output; `scripts/ci/` | Break an invariant and confirm a guardrail fails |
| Determinism | Probe digests, golden vectors | Re-run the same seed on another machine and diff the digests |
| Registers | `ASSET-REGISTRY.json`, `DATA-CLASSIFICATION.md` | Ask a question about the project and see whether the answer is a file |
| Positions and gates | Milestone closures and their recorded evidence | Look for a closure with no executable result behind it |

## Related documents

- [CONTRIBUTING.md](CONTRIBUTING.md) — the loop in concrete steps, the battery, the PR checklist.
- [Data, legal and trademarks](docs/acknowledgments/legal-and-trademarks.md) — licence, retail data, marks.
- [How parity is verified](docs/acknowledgments/parity-verification.md) — sources of truth, vectors, probes.
- [Crates, data custody and determinism](docs/acknowledgments/crates-and-determinism.md) — the 13 members.
- [Status: what is actually done](docs/acknowledgments/status.md) — the record, including known gaps.

# Contributing

Thank you for considering it. This guide explains how to build the engine, what must pass before a
change can be reviewed, and how to add a module following the pattern the project is built with.
The pattern is described conceptually in [ARCHITECTURE.md](ARCHITECTURE.md); here it is concrete.

Read [DATA-CLASSIFICATION.md](DATA-CLASSIFICATION.md) before your first commit. The short version:
**the repository never contains Daggerfall game data.**

## Contributions are welcome

We receive contributions with gratitude. The repository is open, the issues and pull requests are the
channel, and a well-argued "this is wrong" is as welcome as a patch. What follows is not a gate on who
may contribute; it is the shape a contribution has to take while the engine is still being proven.

### Current phase — parity first, redesigns deferred (not forbidden)

The work in progress is **100% parity with the original game**: the formulas, the data and the
observable behaviour, verified with golden vectors instead of screenshots.

Until that parity signature exists, the project's own work is parity: we do not merge gameplay
redesigns, modernisations that diverge from the 1996 formulas, new content, or engine changes into the
parity build.

That is a statement about **timing**, not about the value of the ideas. We do want redesigns: after the
**0.0.0 parity signature**, the project itself will begin a full rewrite of the game, and a later stage
can explore a more modern feel, better meshes and redesigns. We want people who want to propose them.

A redesign can be accepted as a real modification only when **all** of these hold:

- the parity signature exists for the area it touches;
- the proposal includes probes and golden vectors showing it does not break parity or existing
  behaviour;
- it is discussed and agreed in an issue or pull request before implementation;
- the result stays fully MIT.

During the parity phase, a change that improves feel but breaks parity is not merged; it can be kept
as a proposal for the post-parity stage. A nicer game that decodes the wrong formula is a regression,
and it would hide the very discrepancies this phase exists to find.

This keeps the door open: proposals are welcome now, merge conditions are what change.

### Contributing means understanding the pattern

You are contributing to a method, not only to a codebase. Before a pull request:

- Read [ARCHITECTURE.md](ARCHITECTURE.md) — what an organ is, why the boundaries are where they are.
- Follow the loop below: `requirement → contract → probe → code → test → QA → record`. The order is
  the point; steps 1–3 are cheap precisely because they happen before the code exists.
- Respect the organ boundaries: an organ crate never imports Bevy and never reaches into another
  organ; the parser is a zero-dependency leaf.
- Do not break determinism. Same seed and same inputs must produce the same result and the same
  digests, on your machine and on the reviewer's.

### Configuring your version of the pattern in a fork

You do not need our scripts to apply the method to your own project. The minimum is:

1. **Boundaries.** Define your modules or organs and one dependency rule that the review can check.
2. **Contracts.** Write the contract for a module before its code: inputs, outputs, invariants,
   failure modes, source of truth.
3. **Probes.** One executable check per behaviour, with an exit code and evidence — output that a
   stranger can run and compare, not a claim.
4. **Deterministic tests.** Golden vectors for anything numeric or byte-exact, and a test that fails
   before the fix.
5. **A record.** Decisions and their evidence in version control, so they survive the session that
   made them.

[Adopting the pattern elsewhere](#adopting-the-pattern-elsewhere) lists the same steps as a procedure.

### What we accept now

- **Bug reports** with a minimal reproduction: command, expected versus actual result, platform.
- **Probes and tests** that fail for a real reason — including ones that expose a parity gap.
- **Documentation corrections**, especially where a document claims more than the code delivers.
- **Deterministic refactors** that change no observable output and keep the digests identical.
- **Platform support**: other Linux distributions, other window systems, other toolchains.
- **Translations for local use.** They are welcome in your fork or as a patch kept local; the public
  export ships English only, as [the language policy](docs/i18n/POLICY.md) explains.

### What waits until the engine is stable

New gameplay, new content, multiplayer or MMO work, visual overhauls and engine migrations are out of
scope for pull requests today. They are not forbidden topics: open an issue, state the requirement and
the acceptance you would want, and the discussion can happen while the parity work continues. The
decision on when they enter is tied to that work, not to a date.

### The golden rule

If your change alters a formula, a datum or an observable output, **include the probe that proves it**
and the parity evidence behind it. If you cannot demonstrate the result, the pull request stays in
discussion, however plausible the change looks. This is the cheapest rule in the project and the one
that keeps the rest honest.

### An invitation

If you want to help us reach that stability — writing probes, reporting discrepancies, porting the
build, fixing documentation — you are welcome, and that is exactly the open challenge. The interesting
work right now is not adding features; it is proving the ones the original game already had.

## Ground rules

- **No retail bytes, ever.** No `*.BSA`, `*.QRC`, `*.QBN`, extracted textures, audio, palettes or
  executables. The engine reads the player's own installation at runtime; importing those files is a
  hard rejection, not a style issue.
- **No personal data.** No absolute home paths, no email addresses, no hostnames, no credentials in
  tracked files, code, test fixtures or issue text. Use `$DF_APP_ROOT` or `<install-dir>` instead.
- **Clean-room parsers.** Product parsers are written from public format documentation. The
  Daggerfall Unity source under `reference/dfu-spec/` is a *specification*, MIT-licensed and vendored
  verbatim: never copy, compile or link its `.cs` files into a crate, and never edit the corpus.
- **MIT in, MIT out.** By contributing you agree your contribution is licensed under the MIT License
  ([LICENSE](LICENSE)). Do not paste code from projects under other terms.
- **English from the first commit.** Code, comments, commit messages, docs and pull-request text
  are written in English — written that way, not translated later. The rule covers `crates/**`,
  `scripts/**`, `tools/**`, the Cargo manifests, `README.md`, `ARCHITECTURE.md`, this file, the
  public docs and the CI workflows. Exemptions are declared, never implicit: `docs/sintergic/**`
  (internal engineering record, not published), `*.es.md` (local translations, excluded from the
  export), `reference/dfu-spec/**` (vendored MIT corpus, never edited), and quoted text or
  identifiers. The policy is [`docs/i18n/POLICY.md`](docs/i18n/POLICY.md); the executable gate is
  described in [The English gate](#the-english-gate).
- **Be proportionate.** A typo fix needs a diff. A new rule needs the full loop below.

### The English gate

The rule above is enforced, not merely stated:

```bash
bash scripts/ci/language-check.sh       # exit 0 = the exportable surface is English-only
bash scripts/ci/test-language-check.sh  # exit 0 = the gate behaves (Spanish fixture fails, English passes)
```

The gate scans the surface the public export can ship — everything
`tools/public-export/exclude.txt` does **not** exclude — and reads human text only: markdown prose
(fenced code, inline code and HTML comments are skipped), Rust `//` and `/* */` comments, whole-line
`#` comments in shell, Python, TOML and YAML, and the `description` value of a manifest. Per
contiguous prose block it measures the density of unambiguous Spanish function words against English
ones, and treats Spanish-specific letters and inverted punctuation as a second signal. A borrowed
word never fails a file; identifiers, paths, probe IDs and English words that coincide with Spanish
ones (`no`, `sin`, `son`) are not signals.

The gate does not replace the publication gate:
`tools/public-export/export-public.py` re-checks the exported tree and remains the last barrier. A
line the gate reports is fixed by writing that prose in English, never by shipping a translation
next to it.

## Setup

Verified toolchain: `cargo` + `rustc` 1.97.1 (edition 2021, Bevy 0.20.0). Linux is the first-class
target; X11 or Wayland is needed for the windowed mode, and nothing is required for headless probes.

```bash
export DF_APP_ROOT="$HOME/Games/Daggerfall/app"   # your own copy, outside the repo
./scripts/launch.sh --mode complete               # release build + window
./scripts/launch.sh --mode preview --headless     # fast debug build, no window
./scripts/launch.sh --probe ALL-001               # the probe suite
./scripts/launch.sh --print-plan --mode complete  # show the command without running it
```

`--mode pristine` builds with no opt-in features and stays headless: it is the parity configuration.
Every flag and exit code is documented in [The launcher](docs/acknowledgments/launcher.md).

## The battery

Run all of it before opening a pull request. A reviewer may re-run it, and will ask for the output.

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p daggerfall-app --features window --all-targets -- -D warnings
bash scripts/ci/run-ci.sh
./scripts/launch.sh --probe ALL-001
```

What each one proves:

| Command | Proves |
|---|---|
| `cargo test --workspace` | Unit and integration behaviour, including the golden vectors |
| `cargo clippy ... -D warnings` | The build is warning-free in **both** configurations, headless and windowed |
| `scripts/ci/run-ci.sh` | Zero retail content tracked or embedded, zero unauthorised marks, zero Spanish prose on the exportable surface |
| `--probe ALL-001` | The headless behaviour suite passes and its digests match |

**CI is not the whole battery today.** The workflow in `.github/workflows/ci.yml` runs the three
guardrails only: content scan, trademark check and the [language gate](#the-english-gate). Tests,
clippy and probes are still the contributor's job; saying so is part of the honesty rule. If you
touch the workflow, do not claim a gate that is not wired.

## Adding a module: the loop

Every non-trivial change follows seven steps. Skipping one is how a project ends up with features
nobody can verify.

### 1. Requirement

Write down, in your own words, before the code:

- the problem (what is missing or wrong, with a reproducible observation);
- the observable outcome (what will be true when it is done);
- what is explicitly **out of scope**;
- what will count as acceptance (the executable check you intend to write).

Put it in the pull-request description. One page is plenty; the point is that it exists before the
implementation does.

### 2. Contract

State the shape of the change: inputs, outputs, invariants, failure modes, and the source of truth
(the retail format, the public specification or the executable reference it must agree with). If the
change touches data custody or determinism, say how at this step — those decisions are cheap now and
expensive later.

### 3. Probe

Choose the executable check **before** writing the implementation:

- Add or extend a probe (`PROBE-<DOMAIN>-NNN`) if the change is behaviour. It must run headless,
  print a verdict, and communicate failure through its exit code.
- Add a golden vector if the change is a formula, a decode or a transformation that must match a
  reference byte for byte. Compare digests, not pixels.
- Add a plain test if the change is a pure rule with no cross-system behaviour.
- Add an antibody (a guardrail) if the change introduces a class of mistake that must never recur.

A check that cannot fail does not count. Run it against the unmodified implementation first: it must
fail for the reason you expect.

### 4. Code

Place the change according to its responsibility:

| The change is… | It belongs in… |
|---|---|
| A rule, model or state machine with no rendering | an existing organ crate (`behavior`, `skills`, `inventory`, `combat`, `crafting`, …) |
| A new retail format or record decode | `daggerfall_parser_rs` — zero dependencies, no engine types |
| Conversion of parsed records into engine assets | `daggerfall-parser-bevy-bridge` — the only place that touches Bevy handles |
| Composition, ECS systems, CLI, probe dispatch | `daggerfall-app` |
| An invariant or structural guardrail | `daggerfall-antibodies`, or `scripts/ci/` if it is a scan |

Boundary rules, checked in review:

- Fix an organ before adding an organ. A two-hundred-line rule is not automatically a new crate.
- An organ must not depend on another organ or on Bevy; it speaks through the event bus.
- The parser stays a zero-dependency leaf; new dependencies there need a written reason in the PR.
- One bridge. Do not introduce a second conversion path, even "temporarily".
- No hidden globals, no ambient state, no wall-clock reads in simulation code.

### 5. Test

Pin the details the probe cannot see:

- Unit tests for edge cases (empty archives, truncated records, out-of-range indices).
- Golden vectors for anything numeric or byte-exact.
- A regression test that would have failed **before** your fix.
- Determinism: run the same seed twice and compare digests. If your change touches iteration order,
  make it stable — sort before hashing, never rely on hash-map order.

### 6. QA

Self-review before requesting review, and quote numbers only from a run you did:

- [ ] The battery above passes; the outputs are in the PR.
- [ ] The acceptance check from step 1 exists and passes; it failed before the change.
- [ ] Scope: the diff contains nothing the contract did not ask for.
- [ ] Boundaries: no organ-to-organ dependency, no Bevy in organs, parser still a leaf.
- [ ] Determinism: digests reproduce across two runs; no unordered iteration in hashes.
- [ ] Data: no retail bytes, no personal paths, no secrets in the diff.
- [ ] Docs: status claims updated where behaviour changed; all text in English.
- [ ] Honesty: nothing is described as finished that lacks an executable check.

### 7. Record

Leave the evidence where the next person will look:

- The PR description carries the requirement, the contract, the commands and their output.
- If a milestone or capability status changed, update
  [Status: what is actually done](docs/acknowledgments/status.md) and, if the stage changed,
  [Roadmap](docs/acknowledgments/roadmap.md). A status claim without a run behind it is a defect.
- If the change alters what may be committed, update [DATA-CLASSIFICATION.md](DATA-CLASSIFICATION.md).

### Worked example — a new formula

`PROBE-FORMULA-001` pins trade-price outcomes against golden cases. To add a related rule:

1. **Requirement:** "prices must react to personality as the original does; today they ignore it;
   out of scope: UI display; acceptance: golden cases for three personality values."
2. **Contract:** inputs (item value, skill, personality, reputation), output (integer price),
   source of truth (public specification plus the executable reference), failure mode (overflow).
3. **Probe:** extend `FORMULA-001` with the three cases; run it, confirm it fails.
4. **Code:** the rule goes in `daggerfall-skills`; no Bevy, no global state.
5. **Test:** boundary cases, saturation, and a digest over a price table.
6. **QA:** the checklist above.
7. **Record:** status line updated only if the coverage claim changed.

## Commits

Conventional Commits, imperative mood, one topic per commit:

```text
feat(combat): resolve melee damage from equipment and skill
fix(parser): reject truncated RMB records instead of panicking
docs(readme): state the verified test count from the last run
test(skills): add golden cases for personality-adjusted prices
refactor(memory): move chunk bookkeeping behind the save trait
```

Scopes are crate short names (`combat`, `parser`, `app`, `bridge`, `ci`, `readme`, `docs`). Subject
at most 72 characters. The body says **why** and, when the change is behavioural, includes the
command that proves it. Squash noise; keep history reviewable.

## Pull-request checklist

- [ ] Requirement, contract and acceptance are in the description.
- [ ] The battery passes and its output is quoted.
- [ ] The new check failed before the change and passes after it.
- [ ] No retail bytes, no personal data, no secrets.
- [ ] No new cross-crate dependency without a written reason.
- [ ] Public docs are English and their links resolve.
- [ ] Status claims that changed are updated in the same PR.

## What review looks for

1. **Evidence first.** The claim and the check are read together. "It works" is not reviewable.
2. **Boundaries.** The dependency graph is part of the review: a change that reaches across organs is
   rejected even if it works.
3. **Scope.** Unrelated improvements are welcome as their own PR, not bundled.
4. **Determinism.** Any change to iteration, seeding or timing is scrutinised, because it can make a
   digest pass on one machine and fail on another.
5. **Honesty.** A PR that fixes a status claim which was wrong is a good PR. One that quietly
   strengthens a claim is not.

## Issues and reports

- A bug report is most useful with: the command, the probe or test ID, the expected and actual
  result, and the platform. Do not attach retail data or extracted assets.
- Security-relevant findings (data leaks, custody violations, unsafe deserialisation) should be
  reported before any public discussion of exploit details.
- Feature proposals are welcome as an issue that states the requirement and the acceptance in the
  form above; that is also the fastest way to find out whether someone is already building it.

## Adopting the pattern elsewhere

None of the loop depends on this repository's scripts or tooling:

1. Write the requirement and the acceptance check first.
2. Make the check fail.
3. Implement inside an explicit boundary — one responsibility per module, no reach-across.
4. Make the check pass; add unit tests for the details.
5. Record the evidence where the project's next session will find it (a file in version control, not
   a chat).
6. Keep the gate mechanical wherever a script can decide it.

If you adopt it and it breaks in a way this document does not admit, open an issue with the
reproduction. That is the most valuable contribution the method can receive.

## License

[MIT](LICENSE). The license covers this source: not Daggerfall's data, and not the third-party corpus
under `reference/dfu-spec/`, which keeps its own MIT notice. See
[Data, legal and trademarks](docs/acknowledgments/legal-and-trademarks.md).

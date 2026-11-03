# Night-run guardrails (git hooks)

CR: `CR-b367c9dd-db42-4465-9b2f-d8a341aa3135` · lane `NIGHT-GUARD-KIT` · target `daggerfall-bevy-native`.
QA reservations fix (R2 token · R1/D2b gates · R8 falsifier · R3 limit):
`CR-4b576323-4c49-4607-90e3-59c92cf5c2ea`.
Code and prose coherence (D3: `_` boundary + dated census):
`CR-3862257b-fb7e-4eda-9178-e982e3a89ffc`.
Closing of the two gaps before switching on (D4: the exception is decided in `commit-msg`
on the real message + PATTERN gates):
`CR-a58f600a-6b7b-4b48-9c31-716625fce035`.

The prose failed: coverage was declared that did not exist, and a gate compared its own
literals. This kit does not ask for trust: it is a git hook that returns `exit != 0`.
What cannot be demonstrated with a command that fails is not declared here.

State at delivery (D4, 2026-10-29): **installed and OFF**. `GUARD_ACTIVE` unset means
the 3 hooks exit 0 before reading anything (the off kit disturbs no lane). It is switched on
when M43 closes (see §2).

---

## 1. What each hook protects (hook → path → effect table)

| Hook | Looks at | Paths / condition | Effect with `GUARD_ACTIVE=1` |
|---|---|---|---|
| `pre-commit` | the git **index** (`git diff --cached`) | anything matching `GUARD_PROTECTED_PATTERNS` or `GUARD_PROTECTED_DERIVED` | rejects the commit (`exit 1`) and prints path + pattern + remedy for **every** protected path in the index. It does **NOT** evaluate the exception: with `GUARD_ALLOW_CR` set it authorizes nothing, it prints its deferral line and exits 0, because the in-progress commit message does not exist yet |
| `commit-msg` | the **message** of the in-progress commit: the file git passes in **`$1`** (never `.git/COMMIT_EDITMSG`) | same index; requires `GUARD_ALLOW_CR` to appear as a **delimited token** in that message (see §3) | **THE ONLY place where the exception is decided**. Rejects if the expected CR is not in the message as a token (`exit 1`, prints `NO contiene`); grants with `GUARD-EXCEPCION` if it is |
| `pre-push` | the refs being published (stdin) | every push: branches, tags and deletes | rejects (`exit 1`) unless `GUARD_ALLOW_PUSH` |

Protected paths (single source: `scripts/guard/guard-paths.sh`):

| Pattern | Origin | What it is |
|---|---|---|
| `docs/sintergic/validation/closure-register*` | ORDER | closure register |
| `docs/sintergic/handoffs/INDEX.jsonl` | ORDER | closure ledger |
| `crates/promotion-gate/**` | ORDER | the gate itself |
| `crates/side-contract/**` | ORDER | the side contract |
| `docs/sintergic/promotion-gate/promotion-ledger.jsonl` | ORDER | promotion ledger |
| `daggerfall-side-projects/*` | ORDER | side-projects (see D2: no subject) |
| `scripts/*-00[0-9]-*.py` | ORDER | gate sources (see D2: 0 files, re-measured 2026-10-29) |
| `scripts/*-00[0-9].py` | derived D2 | the real gate sources: **23** in the §1.1 snapshot (2026-10-29 11:44 MST), not a fixed number |
| `sides/*` | derived D2 | the real side-projects tree |
| `scripts/*gate*.py` | derived D2b/D4 | **closure gates, by PATTERN** (not by loose name): today it matches 4 files, see §1.1 |
| `scripts/handoff-lint.py` | derived D2b | mechanical lint of the HH-1 closure (its name carries no `gate`) |
| `scripts/mission-log.py` | derived D2b | mission log (its name carries no `gate`) |
| `scripts/validate-lane-registry.py` | derived D2b | lane registry validator (its name carries no `gate`) |

The directory patterns (`…/**` and `…/*`) cover the directory itself **and any
depth**: `crates/promotion-gate/src/x.rs` is protected. File patterns use glob
(`*` crosses `/`, intentional). `./path` is normalized before comparing.

Why these and not others: they are the **instrument** (gates, ledgers, contracts) and the
**closure records**. An autonomous night run cannot edit what measures it.

### 1.1 Measured coverage of `scripts/*.py` (D2b · CR-4b576323 · re-measured D4 CR-a58f600a)

**This is a dated snapshot, not a permanent property of the kit.** Measurement:
**2026-10-29 12:01 MST**, over the working tree as it was (it includes files still
`untracked` from other lanes: the hook sees them all the same). The total is **41** `scripts/*.py`; the
numeric pattern `scripts/*-00[0-9].py` covers **25**; the gate pattern `scripts/*gate*.py`
covers **4**; **3** loose D2b names (they carry no `gate`) stay declared one by one; the
**9** remaining are left out by declared decision, not by oversight.

| Datum | Re-measurement command | Value in that snapshot (2026-10-29 12:01 MST) |
|---|---|---|
| total sources | `printf '%s\n' scripts/*.py \| wc -l` | **41** |
| covered by the numeric pattern | `printf '%s\n' scripts/*-00[0-9].py \| wc -l` | **25** |
| covered by the gate pattern | `ls scripts/*gate*.py` | **4** |
| outside both patterns | total − 25 − 4 | **12** |
| of those, protected by loose name (D2b) | 3 array rows | **3** |
| outside the guard | 12 − 3 | **9** |

The 4 of the gate pattern are measured, not assumed:

| Protected by `scripts/*gate*.py` | Why it is a closure instrument |
|---|---|
| `scripts/closure-gate.py` | closure gate: decides whether a closure counts |
| `scripts/plan-gate-check.py` | check of the plan the closure cites |
| `scripts/m43-gate-contract.py` | contract of the M43 gates |
| `scripts/m43_gate_lib.py` | shared harness of the M43 gates: the gap the previous snapshot declared open (untracked then and now, because the pattern does not depend on it being tracked) |

The 3 the pattern does not reach because they carry **no `gate` in the name**, declared one by one:

| Protected by loose name (D2b) | Why it is a closure instrument |
|---|---|
| `scripts/handoff-lint.py` | mechanical HH-1/INDEX lint (AB1-AB13): it signs the closure |
| `scripts/mission-log.py` | mission log: closure record |
| `scripts/validate-lane-registry.py` | lane registry validator |

If those numbers do not match the snapshot, **the snapshot is old**: run the three
commands again. The `scripts/guard/guard-paths.sh` comment keeps figures from its own dates
('18 files' from D2): they are values from then, not current measurements; this document is
the one that gets re-measured.

### 1.1.1 Why the PATTERN and not the list of names (D4 · CR-a58f600a)

**Any `*gate*.py` / `*_gate_lib.py` under `scripts/` enters by pattern, not by loose
name.** The reason is mechanical: `guard_is_protected` compares the index path against each
pattern with a glob (`guard__match`), so the pattern decides **without anyone editing it**:

| Situation | With a list of names | With `scripts/*gate*.py` |
|---|---|---|
| already-known gate (`closure-gate.py`) | protected if someone wrote its line | protected |
| new gate (`scripts/foo-gate.py`, `scripts/x_gate_lib.py`) | **outside** until someone adds its line, and while it is outside a night run edits it with no warning | **protected the day it exists** |
| `untracked` gate of another lane in flight (`m43_gate_lib.py`) | outside: exactly the gap this document declared measured and open | protected: the glob does not look at the git index, it looks at the path |

The measured case that forced this change is `scripts/m43_gate_lib.py`: the previous snapshot
(2026-10-29 11:44 MST) listed it as outside the guard and proposed a single loose line
`'scripts/m43_gate_lib.py'` as the remedy. **That line was not applied**: it was replaced by the pattern,
which includes it and also covers the next one. A list of names protects only what was already
known the day it was written; the pattern protects whatever is named the same tomorrow.

The 2 loose rows the pattern already covered (`scripts/closure-gate.py`,
`scripts/plan-gate-check.py`) were **withdrawn** from the array: identical coverage, 0 duplication
that lies to the census (a row that says name when the pattern already covers it invites the belief
that deleting it unprotects it).

Honest limit of the pattern: it depends on the **name**. A closure instrument that carries no
`gate` (`scripts/closure-check.py`, for example) still needs its line, hence the 3
loose names above.

| Outside both patterns and D2b (measured, 9) | Real role (docstring): why it is NOT in the guard |
|---|---|
| `scripts/principal-decision-audit.py` | principal decision auditor; it does not decide a lane closure |
| `scripts/ledger-append.py` | **write point** for ledgers: protecting it would prevent appending the log |
| `scripts/cost-ledger.py` | cost/tokens aggregator per lane (read) |
| `scripts/launch.py` | release launcher wrapper; editing it is normal work |
| `scripts/demo-035.py` | DEMO-035 campaign driver (evidence, not closure) |
| `scripts/measure-m18-cs.py` | CS-01..CS-21 evidence driver of the M18 campaign |
| `scripts/npc-flats-capture.py` | window capture driver (visual evidence) |
| `scripts/quest-82-census.py` | one-off census of 82 actions: a report, not a gate |
| `scripts/verify-m18-owner-pack.py` | cross-check of **one** pack (M18): one-off verification |

The list can be **withdrawn or extended line by line** from `GUARD_PROTECTED_DERIVED`
(`scripts/guard/guard-paths.sh`): deleting a line takes that path out of the guard touching
nothing else. What goes in and what does not is a **decision of the owner**; here it is only measured.

## 2. How it is switched on

```bash
export GUARD_ACTIVE=1
```

**State at delivery of this correction (D4, CR-a58f600a, 2026-10-29): the kit is still
OFF.** `GUARD_ACTIVE` is not exported (`printenv GUARD_ACTIVE` gives rc=1) and
`core.hooksPath` is not set (`git config --get core.hooksPath` gives rc=1). The hooks
are installed in `.git/hooks/`, but their first line turns them off.

Switching on happens **when M43 closes**, not before (captain's decision): while lane
`CR-39d656e1` writes its gates, turning the kit on would turn each of its writes into a
rejection that measures nothing. The command is the usual one and the kit does not take it
on its own:

```bash
export GUARD_ACTIVE=1        # 0 export persistente: cada lane lo pone en su propio comando
```

Nothing else to export: the hooks are already installed. With the variable unset or with
any value other than `1`, the three hooks exit **0** on their first line (without reading
the index, without running git). Recommended for the night run: export it in the script
that launches the executor, not in the human's shell.

## 3. How it is disabled per CR (legitimate write)

The exception is a **conjunction**, not a flag, and it is decided **in one place**:

1. `GUARD_ALLOW_CR=<cr>` set means `pre-commit` **evaluates nothing**: it prints
   its deferral line (`GUARD-DEFERIDO: pre-commit does not evaluate the exception`) and exits 0.
   Pre-commit runs **before** git writes the message; if it decided here it would have to read
   `.git/COMMIT_EDITMSG`, which at that moment still holds the **previous** commit message,
   so it would decide on an inherited message.
2. The **same** CR must appear in the in-progress commit message **as a delimited
   token**: `commit-msg` checks it, on the file git passes in **`$1`**.
   It is the **only** place where the exception is granted; `commit-msg.sh` never opens
   `.git/COMMIT_EDITMSG` on its own.

```bash
GUARD_ALLOW_CR=CR-b367c9dd-db42-4465-9b2f-d8a341aa3135 \
  git commit -m "docs(closure): registrar cierre CR-b367c9dd-db42-4465-9b2f-d8a341aa3135" -- docs/sintergic/handoffs/INDEX.jsonl
```

Falsified in both directions (`scripts/guard/verify-guard.sh`): **J** proves that the
message arrives via `git commit -F <file>` and the hook reads it the same way (`$1`, not a fixed
path); **K** proves that without the CR in that message the commit falls; **L** reproduces the
inherited-message defect: two steps, the first leaves `.git/COMMIT_EDITMSG` with the CR, the
second brings a message without it and **must** fail.

### Token, not substring (R2 · CR-4b576323 · boundary `_` D3 CR-3862257b)

The CR is a **delimited** identifier: to its left and right there must be
**any character outside `[A-Za-z0-9_-]`** (start/end of message, space,
punctuation). The underscore counts as an identifier character, just like letters:
`CR-TEST-1_suffix` is **one** identifier, not the token `CR-TEST-1` followed by a suffix.
It is checked with the boundary `(^|[^A-Za-z0-9_-])<CR>([^A-Za-z0-9_-]|$)`, built
by escaping everything non-alphanumeric in the expected CR (`commit-msg.sh`).

This cuts no legitimate case: CRs are of the form `CR-<uuid>` and **contain no
`_`**, so every real way of citing them (`CR-…`, `(CR-…)`, `CR-…, motivo`, `CR-…:`,
end of line) keeps passing.

The comparison used to be a substring one, and that turned the permission into a wildcard: any
message that mentioned a CR **longer than and starting the same** authorized the commit.

| `GUARD_ALLOW_CR` | commit message | result |
|---|---|---|
| `CR-TEST` | `… ver CR-TEST-9999 …` | **REJECTED** because `CR-TEST` sits inside another identifier |
| `CR-TEST` | `… XCR-TEST …` or `… CR-TEST-9 …` | **REJECTED** because it is glued on the left or on the right |
| `CR-TEST` | `… CR-TEST_suffix …` or `… CR-TEST_suf …` | **REJECTED** because `_` is an identifier character, not a boundary |
| `CR-TEST` | `… ver CR-TEST …` / `(CR-TEST)` / `CR-TEST, motivo` / `CR-TEST:` / end of line | **PASSES** as a delimited token |

With (1) without (2) the commit is rejected with `GUARD_ALLOW_CR=… pero el mensaje NO contiene ese CR como token delimitado`.
It is the falsifier's negative control (cases F, H and I): the pass is not
free, and the reason stays written in the git history forever.

If the instrument does not need writing: `git restore --staged -- <path>` (remedy i).

## 4. Publication exception

```bash
GUARD_ALLOW_PUSH="motivo explícito del owner" git push origin <rama>
```

`pre-push` prints `GUARD_ALLOW_PUSH: <motivo>` and lets it through. Without the variable, every push
—branch, tag or delete— is rejected. Publishing is not a side effect of a night
run: it is a signed act.

## 5. Installation and idempotency

```bash
bash scripts/guard/install-hooks.sh --dry-run   # plan, 0 escrituras
bash scripts/guard/install-hooks.sh             # instala los 3 wrappers
```

- Writes `.git/hooks/{pre-commit,commit-msg,pre-push}`: wrappers with **no logic** that
  resolve `git rev-parse --show-toplevel` and source `scripts/guard/<hook>.sh`.
- Deterministic bytes (no timestamps, no absolute paths): running it N times leaves the
  **same sha256**. Measured: 2 consecutive runs, 3/3 identical hashes.
- Foreign hook (it exists and does **not** carry the `# GUARD-KIT:CR-b367c9dd` marker) gives `exit 2`,
  with no backup and writing nothing: this lane touches no other hooks.
- The kit updates itself: the wrapper is stable and the body lives versioned in
  `scripts/guard/`.

## 6. How it is tested

```bash
bash scripts/guard/verify-guard.sh    # exit 0 solo si las 12 filas coinciden
```

Creates a temporary repo (`mktemp -d`): a bare `remote.git` + a `repo` with stubs of every
protected path, installs the hooks with the real installer and runs A..L. It prints the
matrix and one line per case with the hook's real message. It also checks: local remote
with **0 refs** (nothing was published), temporary repo **clean**, and target fingerprint (hooks +
`scripts/guard` + this doc + `core.hooksPath`) **identical** before/after.

**`rc != 0` is not enough (R8).** Every rejection case declares the **marker** the
matching hook must print, and the verdict requires it in the real output:

- `GUARD: commit RECHAZADO` goes to `pre-commit` / `commit-msg` (case A)
- `NO contiene` goes to `commit-msg`, CR missing or not delimited (cases F, H, I, K and L)
- `GUARD: push RECHAZADO` goes to `pre-push` (cases D and G)

If the command exits `rc != 0` but the marker does not appear (typo, broken repo, missing
dependency), the row comes out **`FALLA (motivo equivocado)`** and the script ends `exit 1`. The
falsifier was falsified too: with an impossible marker in case A, the matrix gives
`FALLA (motivo equivocado)` and exit 1, as measured.

**Bit `+x`.** Before the cases, the script measures the executable bit of the 3 hooks: in the
temporary repo it is **blocking** (if missing it aborts: git would ignore the hook and the guard
would vanish in silence); in the real target's `.git/hooks/` it is **informational** and, if one
is missing, it prints `AVISO R3` with the remedy. See §7.6.

| case | what it proves |
|---|---|
| A | commit to `INDEX.jsonl` with the kit on **fails** |
| B | same write with `GUARD_ALLOW_CR` + delimited CR in the message **passes** |
| C | commit to a free path **passes** (it does not block normal work) |
| D | branch push **fails** |
| E | kit off, commit to a protected path **passes** (it does not disturb) |
| F | `GUARD_ALLOW_CR` with a message lacking the CR **fails** (the pass is not free) |
| G | tag push **fails** |
| H | `GUARD_ALLOW_CR=CR-TEST-1` with a message that only carries `CR-TEST-1999` **fails** (token, not substring) |
| I | `GUARD_ALLOW_CR=CR-TEST-1` with a message carrying the identifier `CR-TEST-1_suffix` **fails** (`_` does not delimit: it is still another identifier) |
| J | message delivered with `git commit -F <file>` that does contain `CR-TEST-1` **passes**: the hook reads the file git passes in `$1`, not a fixed path |
| K | same protected path and same exception, new message **without** the CR **fails** with `NO contiene` |
| L | **inherited-message regression**, in two steps: step 1 (with CR) leaves `.git/COMMIT_EDITMSG` with that CR and step 2 (message without CR) **fails**. If step 1 does not leave the CR in `COMMIT_EDITMSG`, the case is declared **empty** and comes out FALLA, so it never passes by accident. A hook that read the inherited message would PASS step 2: measured with a `commit-msg` mutant that reads `git log -1 --format=%B` (step 2 `rc=0`, that is, the defect reproduced), while the real hook rejects it |

Total measured: **12/12** (D4, CR-a58f600a, 2026-10-29). Before J/K/L it was 9/9 (D3,
CR-3862257b: `_` counted as a boundary in the code and the prose called it never inside
another identifier: two claims that could not be true at once; case I
keeps them from drifting apart again). J/K/L close the other pair that could drift: the prose
said the exception was decided on the real message and the hook could be reading the
inherited one with nothing to reveal it.

## 7. Declared limits (what this kit does NOT do)

1. **It validates no content and no truthfulness.** Only ownership (which files) and publication (whether it
   leaves the repo). A false figure in a document is not caught by the hook: the gate catches it.
   If a closure lies about coverage, the hook lets it through, and that is written here
   so nobody mistakes it for a guarantee.
2. **`--no-verify` skips the hooks.** It is a real git limit, not hidden. There is no
   technical defence inside the repo against whoever can run git with that flag; there is a
   trace: the commit is left without the CR in the message and without the `GUARD-EXCEPCION` line (the one
   only `commit-msg` grants), and with `--no-verify` `pre-commit` does not print its
   `GUARD-DEFERIDO` either, so the absence of **both** marks is the signal.
3. **The exception is decided ONLY in `commit-msg`, on the REAL message; `pre-commit`
   authorizes nothing** (D1 + D4). `pre-commit` runs before git writes the message, so
   it cannot decide: with `GUARD_ALLOW_CR` set it prints `GUARD-DEFERIDO` and exits 0.
   The residual limit D1: **if someone deletes `.git/hooks/commit-msg`, `GUARD_ALLOW_CR`
   stays as a free pass** (the other arm of the conjunction disappears and `pre-commit` no longer
   authorized anything to lose). How it is detected, without a daemon:
   - `bash scripts/guard/install-hooks.sh --dry-run` says `crea` for `commit-msg` (with
     the kit hook in place it would say `sobreescribe wrapper del kit`);
   - `ls -l .git/hooks/commit-msg` shows it absent (and `stat -c '%A %n'` shows no `+x` if it lost the bit);
   - `bash scripts/guard/verify-guard.sh` measures the 3 executable hooks in its temporary repo and
     reports `AVISO R3` if any is missing in the target's `.git/hooks/`;
   - the CR-authorized commit in that state is left **without** the `GUARD-EXCEPCION` line, which
     is the signature that `commit-msg` granted: its absence in a commit with the CR in the message
     is the evidence that the hook did not run.
   Mitigation: `install-hooks.sh` reinstalls the 3 (idempotent), but it does not watch the
   real `.git/hooks/` on every run.
4. **Two ORDER literals have no real subject in this tree.** It was measured before
   writing the kit; the derivatives cover the intent. Withdrawing the derived part means
   cleaning one line per pattern from the `GUARD_PROTECTED_DERIVED` array (§1.1 also measures the
   closure gates, which since D4 enter **by pattern** (`scripts/*gate*.py`, 4 files
   today) plus 3 loose names that carry no `gate`):

| ORDER literal | measurement | declared derivative | why |
|---|---|---|---|
| `daggerfall-side-projects/*` | directory **absent** | `sides/*` | the real side-projects tree is `sides/` (e.g. `sides/i1-minimal-side/`) |
| `scripts/*-00[0-9]-*.py` | **0** files | `scripts/*-00[0-9].py` | the real gate sources are `scripts/<family>-00[0-9].py`, **25** in the §1.1 snapshot (2026-10-29 12:01 MST); the pattern is stable, the count is not |

   Declared consequence: while the derivatives exist, protection is slightly wider
   than the ORDER (all `sides/*` and all `scripts/*-00[0-9].py`, not just gate
   sources). Preferable to a pattern that protects nothing.

   **Limit of the pattern limit (D4):** `scripts/*gate*.py` covers by **name**, not by
   role. Every instrument whose file contains `gate` stays **inside**
   (`*gate*.py`: `closure-gate.py`, `plan-gate-check.py`, `m43-gate-contract.py`,
   `m43_gate_lib.py`) and any closure instrument that does not carry it stays **outside**
   (`handoff-lint.py`, `mission-log.py`, `validate-lane-registry.py`: declared one by one
   for that reason). A future gate whose name has no `gate` (`scripts/closure-check.py`) would still
   need its line: the pattern closes the hole of the third gate named like a gate,
   not the one of any instrument with any name.

5. **The installer writes to `.git/hooks/`, not to the versioned repo.** A new `git clone`
   does not bring the kit active: you have to run `install-hooks.sh` (one line).

6. **Silent failure mode: the guard can vanish with nothing to warn (R3).** Git
   only runs hooks that are **executable** files in the hooks directory. If the
   `+x` bit is lost, if the file is missing, or if `core.hooksPath` points elsewhere,
   git **ignores the hooks and prints nothing**: commits exit 0 and it looks like the kit had
   nothing to say. Four forms of the same hole:

   | Mode | What is seen in the commit | How the operator catches it |
   |---|---|---|
   | `+x` bit lost (chmod, copy, odd extraction) | nothing; the hook does not run | `stat -c '%A %n' .git/hooks/pre-commit .git/hooks/commit-msg .git/hooks/pre-push` shows some trio without `x` (expected `-rwxr-xr-x`) |
   | hook missing, or installation never run (new clone) | nothing | `bash scripts/guard/install-hooks.sh --dry-run` says `crea`; with the kit hook already in place it would say `sobreescribe wrapper del kit` |
   | `core.hooksPath` pointing at another directory | nothing; git uses that other one | `git config --get core.hooksPath` gives rc=0 with a value (expected **rc=1**: unset) |
   | work in another clone/worktree without the kit | nothing | the two commands above, in that clone |

   **This kit does NOT warn on its own.** There is no daemon, no start-up check, no heartbeat: the only
   net is `verify-guard.sh`, which measures the `+x` bit of the three hooks (blocking in its temporary
   repo; in the target's `.git/hooks/` it prints `AVISO R3` with the remedy), and it must be
   **run**. If nobody runs it, the guard may be absent and the tree does not say so.
   Reinstalling is one line (`bash scripts/guard/install-hooks.sh`, idempotent) and the
   `--dry-run` writes nothing. This declaration is the limit: no vigilance is claimed that
   does not exist.

## 8. Detection layer (manifest + reflog) — CR-01e3aebf

The hooks of §1–§6 are **friction**: when on, they reject the commit that touches an
instrument. This section is **detection**: it does not take part in the commit, and it serves exactly in
the cases where friction did not happen: kit off (§2), `--no-verify` (§7.2), hook missing
or without the `+x` bit or `core.hooksPath` diverted (§7.6), or the instrument edited another way
(`chmod +w`, direct edit, another clone). None of this **prevents** writing an instrument:
**it detects, it does not prevent**. What it guarantees is that the trace stays and can be named later.

### 8.1 The three scripts and their literal interface

| Script | Interface | Exit codes | What it does |
|---|---|---|---|
| `scripts/guard/manifest-protected.sh` | `--write <file>` · `--check <file>` · `--help` | `--write`: 0 written / 2 error · `--check`: 0 all `OK` / 1 there is `AUSENTE`·`CAMBIADO`·`NUEVO` / 2 usage or manifest unreadable-missing-empty | Takes and verifies the **sha256 fingerprint of every protected path** |
| `scripts/guard/reflog-audit.sh` | `[--limit N] [--since <date>] [--ref <ref>]` | 0 = 0 touches measured / 1 = there are touches / 2 = usage or git **or `SIN EVIDENCIA`** (0 auditable entries) | Lists the **reflog** commits that touch any protected path |
| `scripts/guard/verify-manifest.sh` | (no arguments) | 0 = matrix 5/5 `OK` / 1 = some box or precondition fails / 2 = impossible setup | Falsifier: exercises the 5 boxes (a)-(e) in a temporary repo |

None of the three contains path patterns, but they **do not distribute them the same way** (measured):

- `manifest-protected.sh` and `reflog-audit.sh` **source** `scripts/guard/guard-paths.sh`
  (sibling, resolved via `BASH_SOURCE`) and a path's ownership is decided by
  `guard_is_protected`, so there is **zero pattern duplication**: changing the protected list is done
  in one place, as in the hooks.
- `verify-manifest.sh` **sources NOTHING** (0 `source`): it copies the kit into a temporary repo
  (`mktemp -d`) and **seeds two protected paths as stubs** (`scripts/closure-gate.py`,
  `docs/sintergic/handoffs/INDEX.jsonl`) so the matrix has a subject. **It is not a source of
  truth for patterns and must not be cited as one**: if the protected list grows, this
  script does not change, because its stubs are still two.

`manifest-protected.sh --write <file>` writes `sha256  <rel-path>` (two spaces), one line
per path, sorted with `LC_ALL=C sort` over the **list of paths** (not over the line, so that
paths with spaces do not disturb the order). No timestamps, no absolute paths, so **two consecutive
runs leave identical bytes** (measured in this tree on 2026-10-29: 475 protected paths,
manifest sha256 identical in both runs). The path universe is `git ls-files -z`
plus `git ls-files -z --others --exclude-standard`: the second term is what makes the
definition of `NUEVO` literal (protected **on disk**), and it covers the `untracked` instrument of another
lane (the same case §1.1.1 measured with `m43_gate_lib.py`).

`manifest-protected.sh --check <file>` prints **one line per path**, undecorated, on
stdout, with exactly one of the four states of §8.2, and a final line
`RESUMEN OK=<n> AUSENTE=<n> CAMBIADO=<n> NUEVO=<n> TOTAL=<n>`. The walk is the **union**
manifest ∪ protected-on-disk, sorted `LC_ALL=C`: a manifest path that no longer matches
any pattern is still compared (the manifest rules for its cycle), and a protected path that
is on disk and not in the manifest comes out `NUEVO` even if it was never committed.

`reflog-audit.sh` walks the **reflog** entries of the ref (default `HEAD`, most recent
first) and, for each distinct commit, prints:

```
<sha-corto> | <fecha ISO> | <autor %ae> | <path1>,<path2>
```

`--limit N` (default **50**) bounds to the last N reflog entries, which is **PARTIAL**
coverage. A short window **does not** equal no touches, and the closure must
**declare the window used**: the header always prints it, as `ventana medida:
last N entries of <ref>` with `--limit` and `ventana medida: desde <date>` with `--since`.
`--since <date>` accepts whatever `git log --since` accepts (`2026-10-29`, `2 days ago`, …) and
**takes precedence over `--limit`**: every reflog entry inside the window is walked. The
second header line says what is actually audited: `entradas medidas: <n> · commits
distintos auditados: <m>` (with `--since`, n = reflog entries falling inside the
window; with `--limit`, n = entries read). A footer closes with
`commits infractores: N · paths protegidos diferentes: M`; with 0 offenders **and
`entradas medidas > 0`** it prints literally **`0 toques a instrumentos`** and exits 0. If
`entradas medidas = 0` (empty reflog, or a `--since` that matches no entry) it prints
on **stderr** `SIN EVIDENCIA: el reflog de <ref> no tiene entradas auditables (¿git reflog
expire / git gc --prune?) — NO se puede atestiguar «0 toques»` and exits **2**, never 0: the
absence of evidence is not an acquittal. It accepts no positional arguments (a bare sha or
path is a usage error, exit 2).

Paths of a commit: declared decision, merge commits included: the **first
parent** is resolved with `git rev-list --parents -n1` and compared against it with
`git diff-tree -r --no-renames <parent> <commit>` (equivalent to
`--diff-merges=first-parent` / `git show --first-parent`, applied by hand so as not to depend on the
git version or on `git show`'s default mode); a root commit is measured with
`git diff-tree -r --no-renames --root`. `--no-renames` makes a rename show up as a delete
+ create: renaming an instrument is seen too. Measured in a temporary repo with a merge whose
side did touch the ledger: the audit names **the merge**
(`b1d5a3f | … | docs/sintergic/handoffs/INDEX.jsonl`).

**It detects `--no-verify` by design.** A commit with `--no-verify` is a normal commit: the only
thing that did not happen is the hook. This layer's evidence is **history**, not the hook, so the
commit appears in the reflog like any other and `reflog-audit.sh` names it. Measured in
box (c) of `verify-manifest.sh`: with a marker `pre-commit` hook that leaves a mark and exits 0,
the commit with `--no-verify` comes out **without a mark** (the hook did not run) and the audit names it
all the same, with its sha and its path.

`verify-manifest.sh` is the falsifiable proof of the layer. It sets up a temporary repo (`mktemp -d`),
copies `guard-paths.sh`, `manifest-protected.sh` and `reflog-audit.sh`, seeds two protected paths
(`scripts/closure-gate.py`, `docs/sintergic/handoffs/INDEX.jsonl`) plus one free one (`src/foo.rs`),
installs the marker hook and runs the matrix:

| Box | Scenario | Expected |
|---|---|---|
| (a) | manifest just written ⇒ `--check` | rc 0 · all lines `OK` · `AUSENTE=0 CAMBIADO=0 NUEVO=0` |
| (b) | edit `scripts/closure-gate.py` + commit ⇒ `--check` | rc 1 · prints `CAMBIADO scripts/closure-gate.py` |
| (c) | `git commit --no-verify` touching `INDEX.jsonl` ⇒ `reflog-audit.sh` | rc 1 · names the path and the sha · **the hook left no mark** |
| (d) | declared revert (restore **bytes** and recommit) ⇒ `--check` + `reflog-audit.sh` | `--check` rc 0 · `reflog-audit` rc 1 and still sees the offending commits (including the sha of (c)) |
| (e) | offending commit then `git reflog expire --expire=now --all && git gc --prune=now` ⇒ `reflog-audit.sh` | rc **2** · prints `SIN EVIDENCIA` · **never** rc 0 with «0 toques» |

Exit 0 **only** with the 5 boxes as expected and 3 measured preconditions: `--write`
deterministic (two runs, `cmp` identical), starting repository **without touches**
(`reflog-audit` rc 0 with `0 toques a instrumentos`) and a **positive control** of the marker hook
(a normal commit does leave a mark; without that control, the absence of a mark in (c) would prove
nothing). Deliberate difference from box (b) of `verify-guard.sh`: here (b) really commits,
because what is measured is the on-disk fingerprint, not the hook's rejection. Decision for (d):
it uses **byte restore + commit** instead of `git revert` because the stub is born
`untracked`, and a `git revert` of the commit that added it would **delete** it (state `AUSENTE`, not
`OK`); with the restore the state goes back to `OK` and the offending commits stay in the
history, which is exactly what (d) must prove. Box (e) goes **last on purpose**
(it destroys the reflog that (c) and (d) need) and it is the one that makes the green
false negative falsifiable: a `reflog-audit.sh` that returned 0 with no evidence would put (e) in `FALLA` and the
falsifier at exit 1.

### 8.2 The four manifest states

| State | Meaning | What it implies |
|---|---|---|
| `OK <path>` | in the manifest **and** on disk, identical sha256 | the instrument is byte for byte as it was when the cycle opened |
| `AUSENTE <path>` | in the manifest, **no longer on disk** | someone deleted or moved it (includes paths that also left `git ls-files`) |
| `CAMBIADO <path>` | in both, **different sha** | someone edited it, the case this layer names |
| `NUEVO <path>` | protected **on disk** and absent from the manifest | an instrument that appeared after seeding: either it is legitimate and gets re-seeded, or nobody declared it |

The four states are about **bytes on disk**, not ownership: a manifest path that no longer
matches any pattern is still compared (the manifest rules for its cycle), and the ownership
list is still decided by `guard-paths.sh` for whatever enters as `NUEVO`.

### 8.3 The real limit: **it detects, it does not prevent**

No decoration: the same uid that runs the night run can `chmod +w` the instruments,
edit `guard-paths.sh` to drop a pattern, delete `.git/hooks/*`, point
`core.hooksPath` elsewhere and `git commit --no-verify` to skip the hooks. **None of
that is technically prevented** from inside the repo: the hooks (§7.2, §7.6) and this layer
share that ceiling, because no defence is possible against whoever already has write access to the
tree and to `.git`.

What is guaranteed, and its exact size:

- the **bytes** of the instruments are anchored: `manifest-protected.sh --check` names the
  specific path that changed (`CAMBIADO`), the one that is missing (`AUSENTE`) and the one that appeared
  undeclared (`NUEVO`);
- the **history** is anchored, with its exact size: `reflog-audit.sh` names the exact
  commit that touched an instrument, with its sha, its date, its author and the paths, **even if the
  hook was skipped** and even if the commit is no longer on any branch: a `reset --hard`
  keeps the abandoned sha as an entry of its own reflog, so **it is still detected**.
  What does **not** survive is reflog expiry followed by `git gc --prune=now`: that
  deletes the entries and the evidence with them.

The reflog ceiling, no decoration: **`git reflog expire --expire=now --all` +
`git gc --prune=now` destroys the evidence and the instrument can no longer attest anything**. In
that state `reflog-audit.sh` exits **2 `SIN EVIDENCIA`** and not `0 toques`: absence of evidence
≠ acquittal: the hole stops being silent, but **destroying is still possible**, and
destroying the reflog **is in itself** the act a closure must look at. It detects, it does not prevent,
here too.

That is all, and it is enough for an honest closure: there is no prevention, there is **constancy**. If
a closure says no instrument was touched, this layer turns that sentence into something that can
be checked in two commands, or refuted by naming the path and the commit.

### 8.4 How it is used in a closure

```bash
# 1. SEMBRAR (primera vez en este repo, y tras cada cambio legítimo del kit)
bash scripts/guard/manifest-protected.sh --write docs/sintergic/guard/protected.sha256

# 2. AL CERRAR: los bytes
bash scripts/guard/manifest-protected.sh --check docs/sintergic/guard/protected.sha256
#    exigido: RC=0 y `RESUMEN ... AUSENTE=0 CAMBIADO=0 NUEVO=0`

# 3. AL CERRAR: la historia del ciclo
bash scripts/guard/reflog-audit.sh --since "<inicio del ciclo>"
#    exigido: RC=0, `entradas medidas` > 0 y la línea `0 toques a instrumentos`
#    RC=2 `SIN EVIDENCIA` NO es un cierre válido: es un reflog sin evidencia
#    (¿expire/gc?) o una ventana vacía. El cierre declara la ventana que usó.
```

Usage notes, no fine print:

- **The manifest has to be seeded**: it does not travel in the repo and no hook creates it. The **path is
  chosen by whoever closes the cycle** (recommended `docs/sintergic/guard/protected.sha256`, which is
  relative to the caller's directory). If it is meant to travel, it gets committed: a decision of the
  closure, not of these scripts.
- **The manifest file itself is NOT a protected instrument**: it matches no pattern of
  `guard-paths.sh`, so `--write` may rewrite it freely (if it were, every
  re-seed would show up as `CAMBIADO` of itself). Adding it to the list is an **owner
  decision**, not an effect of this document; today it is not there.
- **Re-seed after a legitimate kit change** (`guard-paths.sh`, a gate, a ledger): run
  `--write`, and the reason stays in the message of the commit that made the change. Re-seeding
  **without** a legitimate change turns the layer into decoration: it erases the evidence being sought.
- **`--write` refuses with rc 2 if a *tracked* protected file is missing from disk.** There is no silent
  drop: the path is still in the git index, `sha256sum` cannot read it and the script
  prints `no se pudo hashear '<path>' (¿permisos?): manifiesto NO escrito, el anterior queda
  intacto`, so the previous manifest **is not touched**: a deletion is not whitewashed
  by re-seeding. What to do in that case: **commit the deletion** (if legitimate) or
  **revert it** (`git checkout -- <path>`) **before** re-seeding with `--write`.
- **Legitimate append to the protected ledger (there is no allowlist, by design).** `INDEX.jsonl` is a
  protected instrument and protocol HH-1 requires **one append per lane**, so `reflog-audit.sh`
  will give **rc=1 legitimately**. The closure must **declare** those appends (sha + lane) as
  authorized touches: the instrument does not distinguish an append from a destructive edit, and that is
  why the declaration is mandatory, not optional. Short recipe: run
  `bash scripts/guard/reflog-audit.sh --since "<start>"` and, for each line naming
  `docs/sintergic/handoffs/INDEX.jsonl`, one row in the closure's authorized touches table
  with `<short-sha> · <lane> · append HH-1`. An undeclared append is a finding, not noise.
- The three commands are **deterministic and free of hidden state**: they write nothing to the repo (the
  manifest is written where it is asked for), they do not touch `.git/hooks/`, they install nothing.

### 8.5 They do not depend on `GUARD_ACTIVE`

`manifest-protected.sh`, `reflog-audit.sh` and `verify-manifest.sh` **do not read `GUARD_ACTIVE`** in
any decision. They work with the kit **off** (state of §2: `GUARD_ACTIVE` unset),
which is precisely the use case: if detection depended on the hooks being active, it would
detect nothing in the scenario that motivates it. Measured: all three run with
`env -u GUARD_ACTIVE` and with the variable unset in the environment (`printenv GUARD_ACTIVE`
⇒ rc 1). `verify-manifest.sh` also sets up its temporary repo **without** exporting the variable at
any point.

### 8.6 Coherence with §7.2 (the `--no-verify` limit)

§7.2 declares that `--no-verify` skips the hooks and that the trace is the commit without the CR in the
message and without the `GUARD-EXCEPCION` line. That reasoning had an awkward dependency: the
evidence was inferred from the **absence** of a mark written by a hook. If the hook did not
run, the absence explains itself, but it would also be explained by other causes, and in a
commit without a CR the absence is normal.

Now there are **two independent evidences**:

1. the **absence of `GUARD-EXCEPCION`** in a commit with the CR in the message ⇒ `commit-msg` did not
   grant (or did not run): it is still the hook's signature, dependent on the hooks existing;
2. `reflog-audit.sh` **names the commit** that touched the instrument, with sha, date, author and
   paths: it does not depend on any hook having run, nor on the mark being present or missing, nor on
   the commit still being on a branch.

The second does not replace the first: the first says **who authorized** (and with which CR), the
second says **what actually happened**. A closure that requires both (`reflog-audit.sh --since
<start>` at 0 touches, and `GUARD-EXCEPCION` present only where there was a declared exception) can no longer
be a victim of the hole of §7.6 (hook missing, without `+x`, `core.hooksPath` diverted): that
turns off friction, not constancy.

### 8.7 Notice: the `AGENTS.md` reconciliation belongs to the block, not to the file

The reconciliation with the single source covers **only the `CONTRATO-EJECUTOR` block** (1228 B,
sha256 `2606ce56…4d0f`): the next real `sync --apply` will reorder the rest of the file
(`## Prohibiciones` moves behind the block) **without touching the block**; the rest of `AGENTS.md`
is not certified by this layer, only the block.

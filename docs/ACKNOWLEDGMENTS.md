# Acknowledgements, legal and project record

This is the long-form companion to [`README.md`](../README.md), split into **one file per case**. It
holds the material that is too heavy for the entry door: credits, legal and trademark notices, the
engine-choice rationale, the verification methodology, the per-milestone record, the roadmap detail
and the launcher/crate reference.

**Provenance.** The text was moved, not rewritten. It was first moved out of the README into this
record (only relative links rebased from the repository root to `docs/`), and later split into case
files under [`docs/acknowledgments/`](acknowledgments/) (only relative link targets and heading
levels rebased). Every case file states that note at the top and its body is verbatim: the split is
mechanical, this index is the only part written for the split.

## Contents

| Case | File | Holds |
|---|---|---|
| Acknowledgements | [`credits.md`](acknowledgments/credits.md) | Daggerfall Unity, DaggerfallJS/DFO Online, UESP and the format community, modders, Bethesda, Rust and Bevy |
| Data, legal and trademarks | [`legal-and-trademarks.md`](acknowledgments/legal-and-trademarks.md) | MIT code licence, why game data is not MIT, third-party source, marks and affiliation |
| Why not build on Daggerfall Unity or DaggerfallJS — the long-form comparison | [`why-not-dfu-or-daggerfalljs.md`](acknowledgments/why-not-dfu-or-daggerfalljs.md) | The argued comparison; the positive Bevy case lives in the README |
| How parity is verified | [`parity-verification.md`](acknowledgments/parity-verification.md) | Sources of truth, golden vectors, behaviour probes, CI guardrails, what is *not* claimed |
| Status: what is actually done | [`status.md`](acknowledgments/status.md) | The full M0–M29 record, evidence at each closure, the campaign re-audit and the known gaps |
| Roadmap | [`roadmap.md`](acknowledgments/roadmap.md) | Stage table — what each stage closes, what is explicitly out of scope today |
| The launcher | [`launcher.md`](acknowledgments/launcher.md) | Modes, flags, exit codes, the verified launcher behaviour |
| Crates, data custody and determinism | [`crates-and-determinism.md`](acknowledgments/crates-and-determinism.md) | The 13 workspace members, Mode A vs. Mode B, determinism rules |

## Where to look first

- **Can I use this code?** → [`legal-and-trademarks.md`](acknowledgments/legal-and-trademarks.md#code-license)
  and [`DATA-CLASSIFICATION.md`](../DATA-CLASSIFICATION.md).
- **Why not just use Daggerfall Unity?** → the README's
  [Why Bevy](../README.md#why-bevy--and-why-not-build-on-daggerfall-unity-or-daggerfalljs) and, in full,
  [`why-not-dfu-or-daggerfalljs.md`](acknowledgments/why-not-dfu-or-daggerfalljs.md).
- **What exactly is finished?** → [`status.md`](acknowledgments/status.md), including
  [known gaps](acknowledgments/status.md#known-gaps-declared-not-hidden).
- **How do you know it works?** → [`parity-verification.md`](acknowledgments/parity-verification.md).
- **I want to build a derived game.** → [`credits.md`](acknowledgments/credits.md),
  [`legal-and-trademarks.md`](acknowledgments/legal-and-trademarks.md) and the README's
  [A seed, not a one-off](../README.md#a-seed-not-a-one-off).

## Language

This record is written in **English**, the repository's canonical language. The language policy, the
naming convention for translations and the current inventory live in
[`docs/i18n/POLICY.md`](i18n/POLICY.md) and [`docs/i18n/STATUS.md`](i18n/STATUS.md).

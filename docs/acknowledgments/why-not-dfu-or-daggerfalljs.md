# Why not build on Daggerfall Unity or DaggerfallJS — the long-form comparison

*Case file split from [`docs/ACKNOWLEDGMENTS.md`](../ACKNOWLEDGMENTS.md): the body was moved verbatim; only relative link targets and heading levels were rebased.*

Both existing projects are excellent, and both are credited above. This project deliberately does
not fork either. The reasons are technical, not tribal.

## Why Bevy 0.20

The positive case — ECS and determinism, memory control and streaming, the Rust toolchain, the MIT
licence chain, the web/WASM ceiling and the dependency risk of a young ecosystem — lives in the entry
door, where a new reader asks that question first:
[Why Bevy — and why not build on Daggerfall Unity or DaggerfallJS](../../README.md#why-bevy--and-why-not-build-on-daggerfall-unity-or-daggerfalljs).
This file keeps the long-form comparison that follows.

## Why not build on Daggerfall Unity

Daggerfall Unity is a C# reimplementation on Unity, MIT-licensed, and it is the single most
valuable piece of Daggerfall engineering in existence. We use it as an **executable specification**:

- Its source explains behaviour that the file formats do not (terrain sampling constants, climate
  swaps, name-bank selection, quest semantics).
- We pin a release (`v1.1.1`, commit `0038a1e61999ad2c73fb1f33b706cae9a88ec66f`) and keep a small,
  licensed copy under `reference/dfu-spec/` so that citations are reproducible.
- Its MIT license is preserved verbatim; attribution is in [NOTICE](../../NOTICE).

We do not use it as the base because:

1. **Runtime dependency.** A Unity game depends on the Unity runtime and editor toolchain. This
   project targets a self-contained native binary with no proprietary runtime.
2. **Architecture.** DFU is organized around Unity `MonoBehaviour`s and scene objects. Our target is
   an ECS where simulation state is data, systems are pure-ish, and headless execution is normal.
3. **Language and memory model.** C# on Unity's managed runtime is a different performance and
   determinism profile from Rust. Porting the architecture would import assumptions we do not want.
4. **Different product goal.** DFU aims to recreate Daggerfall inside Unity, with its mod ecosystem.
   This project aims at a data-oriented engine with deterministic probes and a data-only content
   pipeline. Sharing the specification is useful; sharing the architecture would fight both goals.

## Why not build on DaggerfallJS / DFO Online

[DaggerfallJS](https://github.com/Lattymoy/daggerfall-js-source) (also known as DFO Online,
historically "Daggerfall Enhanced") is a JavaScript/WebGL2 reimplementation, MIT-licensed,
© 2026 Lattymoy. It is a serious, fast-moving project and a useful reference for product decisions:

- bring-your-own-data with no game bytes in the repository;
- a browser-first distribution model alongside native installers;
- a single registry of feature switches instead of scattered flags;
- treating the attached data folder as the switch that enables a feature.

We do not use its code or architecture because:

1. **Platform mismatch.** It is JavaScript/WebGL2; this engine is Rust/Bevy with a native renderer
   and a Rust parser. There is no shared runtime or asset path to build on.
2. **Different performance and determinism envelope.** A browser engine optimizes for download size
   and WebGL2 limits; this project optimizes for native simulation throughput and reproducible
   headless probes.
3. **Different content strategy.** Our roadmap is declarative content packs with a Rust-side
   manifest and load order; adopting a JS mod runtime would add a second execution model.
4. **No code reuse is needed to share the good ideas.** The patterns above are adopted as design
   inputs, with attribution, not as vendored code.

Neither project's assets are used, and this project does not present itself as either project or
use their names as its brand.

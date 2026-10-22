# Acknowledgements

*Case file split from [`docs/ACKNOWLEDGMENTS.md`](../ACKNOWLEDGMENTS.md): the body was moved verbatim; only relative link targets and heading levels were rebased.*

This project stands on work done by other people. Thank you.

## Daggerfall Unity

**Daggerfall Unity** by Interkarma and contributors — MIT, © 2009–2023 Daggerfall Workshop
([repository](https://github.com/Interkarma/daggerfall-unity),
[LICENSE](https://github.com/Interkarma/daggerfall-unity/blob/v1.1.1/LICENSE)).

It is the reference implementation that made Daggerfall legible to a generation of developers. This
project uses it as an executable specification, pinned at release `v1.1.1`
(commit `0038a1e61999ad2c73fb1f33b706cae9a88ec66f`), for terrain sampling, climate mapping, NPC
naming, quest semantics and the formulas implemented so far. The pinned corpus in `reference/dfu-spec/` retains its
license and attribution. No DFU code is compiled into this engine.

## DaggerfallJS / DFO Online

**DaggerfallJS** by Lattymoy — MIT, © 2026 Lattymoy
([repository](https://github.com/Lattymoy/daggerfall-js-source),
[LICENSE](https://github.com/Lattymoy/daggerfall-js-source/blob/main/LICENSE)).

A JavaScript/WebGL2 reimplementation that demonstrates, in public, how a modern Daggerfall engine can
be distributed with bring-your-own-data and a clean feature-switch registry. It is cited as design
inspiration for those patterns; no code or assets are used.

## UESP and the format community

- **Dave Humphrey**, *Daggerfall's BSA File Formats* (UESP, 2002) —
  [`dfbsa.txt`](https://en.uesp.net/dagger/dfbsa.txt). The public documentation that makes a
  clean-room BSA parser possible.
- **UESP** ([en.uesp.net](https://en.uesp.net/wiki/Daggerfall_Mod:BSA_file_formats)) for the BSA
  record documentation (`.RMB`, `.RDB`, `MAPS.BSA`, `ARCH3D.BSA` and friends), maintained for
  decades.
- The **Daggerfall Workshop** community for reverse-engineering notes, tools and endless patience
  with format edge cases.

## Modding community

The Daggerfall modding scene — DREAM, Dynamic Skies, and many others — showed what a data-driven
Daggerfall can look like. Mods are mentioned as inspiration only; **no mod assets or code are used
or redistributed** by this project.

## Bethesda Softworks

For Daggerfall itself, and for the 2009 freeware release that keeps it playable. The game data
remains theirs; this project only reads a copy the user already owns.

## Rust and Bevy

Thanks to the Rust project and the Bevy contributors. The engine's determinism, headless testing and
native binaries are direct consequences of their work.

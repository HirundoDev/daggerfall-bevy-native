# Data, legal and trademarks

*Case file split from [`docs/ACKNOWLEDGMENTS.md`](../ACKNOWLEDGMENTS.md): the body was moved verbatim; only relative link targets and heading levels were rebased.*

## Code license

The engine source code is released under the **MIT License**. See [LICENSE](../../LICENSE).

## Game data is not included and is not MIT

Daggerfall's data files are the property of Bethesda Softworks. Daggerfall was released as freeware
in 2009, but freeware is **not** the same as public domain, and it does not permit redistribution of
the game's assets. Therefore:

- No retail bytes are committed. `.gitignore` excludes `ARENA2/`, `*.BSA`, `*.VID`, `*.PAL` and
  related paths.
- For game data, the repository contains only **metadata and SHA-256 hashes** in
  [`ASSET-REGISTRY.json`](../../ASSET-REGISTRY.json), plus the parsers that read the data at runtime.
- `PathChecker` resolves the player's own installation (Mode A). Switching to a custom universe
  (Mode B) is an explicit, separate mode.
- `scripts/ci/content-scan.sh` enforces the zero-bytes rule in CI, including build artifacts.

See [`DATA-CLASSIFICATION.md`](../../DATA-CLASSIFICATION.md) for the full classification.

## Third-party source

[`reference/dfu-spec/`](../../reference/dfu-spec/) contains a pinned subset of Daggerfall Unity source
(120 `.cs` files plus `NameGen.txt`, 124 tracked files with the licence and the two custody records),
© 2009–2023 Daggerfall Workshop, MIT. Its original license is
preserved at [`reference/dfu-spec/LICENSE.txt`](../../reference/dfu-spec/LICENSE.txt) and its provenance
is recorded in [`reference/dfu-spec/PROVENANCE.md`](../../reference/dfu-spec/PROVENANCE.md).
It is documentation/specification input, not linked into any product crate.

## Trademarks and affiliation

Daggerfall and all related marks are trademarks of their respective owners. This project is **not
affiliated with, endorsed by, or sponsored by** Bethesda Softworks or ZeniMax Media. The name
`daggerfall-bevy-native` is descriptive: it identifies the kind of engine this is, not a product
brand. Any public release under a different brand must pass the project's naming checklist first.

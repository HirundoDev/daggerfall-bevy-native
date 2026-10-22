# PROVENANCE — Daggerfall Unity source-class pin for M13 / M15 / M16

> Translation of the original Spanish custody record; claims unchanged. Only internal process
> identifiers (record and lane ids) were dropped: upstream commits, line references, byte counts,
> hashes and clean-room rules are as recorded.

**Upstream repository:** https://github.com/Interkarma/daggerfall-unity (MIT, `Copyright (c) 2009-2023 Daggerfall Workshop`)
**Fixed ref:** release tag **`v1.1.1`** → commit **`0038a1e61999ad2c73fb1f33b706cae9a88ec66f`**
**Release:** `v1.1.1` — "DFU 1.1.1 - Security Update", published 2025-10-05T01:34:39Z (API `/releases/latest`, HTTP 200).
**Why this ref:** a release tag (not a moving `master`) → reproducible pin. The 112 pre-existing `.cs` files of the corpus stay at their original commit `2343305d1d83ccc0de57a81e3b1e61188a997e34` ([CLASSIFICATION.md](CLASSIFICATION.md) §1); this pin **does not rewrite them**.

## 1. Verification protocol (per file)

1. `GET https://raw.githubusercontent.com/Interkarma/daggerfall-unity/<commit>/<path>` → **HTTP 200** required.
2. `git hash-object <local file>` compared against the **blob SHA-1** that the GitHub Trees API declares for that path at that commit. Equality ⇒ the local file is **byte-for-byte** the blob of the commit (no truncation, no BOM, no line-ending normalisation).
3. `sha256sum` of the written file (table column).
4. Header checked: `// Project: Daggerfall Unity` + `Copyright (C) 2009-2023 Daggerfall Workshop` in all 7 `.cs` files.

## 2. Pin table

| # | Class / datum | Mirror path in `reference/dfu-spec/` | Upstream path | Bytes | sha256 | blob SHA-1 (12) | State |
|---:|---|---|---|---:|---|---|---|
| 1 | `WoodsFile` — `WOODS.WLD` layout | `Assets/Scripts/API/WoodsFile.cs` | `Assets/Scripts/API/WoodsFile.cs` | 14,779 | `85e1b713e68f11c03046489a4b062984eeec4db12d466f3d72603e7305748123` | `f3f142f2cc59` | **PINNED** |
| 2 | `ClimateSwaps` — nature set → archive | `Assets/Scripts/Utility/ClimateSwaps.cs` | `Assets/Scripts/Utility/ClimateSwaps.cs` | 19,975 | `332bc2b1ca12a779bbc516dce3d6bd1fca3531c6f44df8e3b553d965a1cecb63` | `2a4670f661d6` | **PINNED** |
| 3 | `MapsFile` — `GetClimateIndex`/`GetPoliticIndex` | `Assets/Scripts/API/MapsFile.cs` | `Assets/Scripts/API/MapsFile.cs` | 62,196 | `998f36c402e295fe5d3558f9afcba80b8a2b6bd66277f0919e392097c6de9947` | `78c0d0bd942e` | **PINNED** |
| 4 | `PakFile` — RLE of `CLIMATE.PAK`/`POLITIC.PAK` (direct dependency of anchor C) | `Assets/Scripts/API/PakFile.cs` | `Assets/Scripts/API/PakFile.cs` | 6,091 | `102edee32dde03feb3c7ac2725653863449f01509505883a5a4af47764335af2` | `81e4a72d3d5a` | **PINNED** |
| 5 | `NameHelper` — name generator (consumer) | `Assets/Scripts/Game/Utility/NameHelper.cs` | `Assets/Scripts/Game/Utility/NameHelper.cs` | 12,934 | `18d889bd16b2f3a31f8975649ea688079f91c3c51c5ae4f688ebfe922c334c76` | `cd273da09599` | **PINNED** |
| 6 | `NameGen.txt` — name bank consumed by `NameHelper` (data, not a binary) | `Assets/Resources/NameGen.txt` | `Assets/Resources/NameGen.txt` | 28,005 | `372497541fe3b7e22fe4bd155e0cd8ad34ce09b7b330edbaf91c6f997c8adb81` | `1a228e736fab` | **PINNED** (supplementary) |
| 7 | `TextFile` — `TEXT.RSC` parser | `Assets/Scripts/API/TextFile.cs` | `Assets/Scripts/API/TextFile.cs` | 17,772 | `a56a77e44e47da258cd29e8339bc2b0a35ec87922eb5e6d82facd76a262eb96d` | `43a2b40ba316` | **PINNED** |
| 8 | `TextProvider` — text interface/manager | `Assets/Scripts/Utility/TextProvider.cs` | `Assets/Scripts/Utility/TextProvider.cs` | 27,343 | `465ea4568f2db23b2df9db2a135a1c4a26a78957f5f8ed3c89e90f3cc1314088` | `724659a0982e` | **PINNED** |

Raw URL (exact pattern used): `https://raw.githubusercontent.com/Interkarma/daggerfall-unity/0038a1e61999ad2c73fb1f33b706cae9a88ec66f/<upstream path>`

**Download record:** 8/8 with HTTP 200, size > 0, blob SHA-1 == commit tree. **0 invented files, 0 stubs.**

## 3. Verified anchors (the ones M13 needs)

### (a) Terrain height in `WOODS.WLD` — **PINNED**

- `Assets/Scripts/API/WoodsFile.cs:80` — `public UInt32 HeightMapOffset;` field of `struct FileHeader`.
- `Assets/Scripts/API/WoodsFile.cs:383-393` — header read order: `OffsetSize, Width, Height, NullValue1, DataSection1Offset, Unknown1, Unknown2, HeightMapOffset` + `NullValue2[28]`. [arithmetic derivation] `HeightMapOffset` lands at byte **28** of the file (7 × u32) and the header occupies **144 B** (28 + 4 + 112).
- `Assets/Scripts/API/WoodsFile.cs:434-438` — **height read**: `reader.BaseStream.Position = header.HeightMapOffset; heightMapBuffer = reader.ReadBytes(mapBufferLengthValue);`
- `Assets/Scripts/API/WoodsFile.cs:28,31,34` — `mapWidthValue = 1000`, `mapHeightValue = 500`, `mapBufferLengthValue = 500,000`.
- `Assets/Scripts/API/WoodsFile.cs:231` — row-major sampling: `return Buffer[(mapPixelY * mapWidthValue) + mapPixelX];` (1 byte per pixel, 0–255).

### (b) Nature set → archive mapping — **PINNED**

- `Assets/Scripts/Utility/ClimateSwaps.cs:116-152` — `GetClimateTextureInfo`: if `archive > 499` the archive **is** the nature set (`ci.textureSet = (DFLocation.ClimateTextureSet)archive`) and is grouped under `Nature`; snow subset (`supportsWinter = true`): `Nature_TemperateWoodland`, `Nature_WoodlandHills`, `Nature_HauntedWoodlands`, `Nature_Mountains` (`:137-140`); without snow: `Nature_RainForest`, `Nature_SubTropical`, `Nature_Swamp`, `Nature_Desert` (`:130-133`).
- `Assets/Scripts/Utility/ClimateSwaps.cs:93-107` — `climateIndex` arithmetic: `climateIndex = archive` (or `archive + (int)DFLocation.ClimateWeather.Winter|Rain` if `supportsWinter|supportsRain`).
- `Assets/Scripts/API/DFLocation.cs:252-263` — explicit values **500…511** (`Nature_RainForest=500` … `Nature_Mountains_Snow=511`). This file is **pre-existing** in the corpus (commit `2343305d…`, not re-pinned); the cited lines were read from the local file.

### (c) `GetClimateIndex` / `GetPoliticIndex` — **PINNED**

- `Assets/Scripts/API/MapsFile.cs:891-896` — `public int GetClimateIndex(int mapPixelX, int mapPixelY)` → `return climatePak.GetValue(mapPixelX, mapPixelY);`
- `Assets/Scripts/API/MapsFile.cs:904-907` — `public int GetPoliticIndex(...)` → `return politicPak.GetValue(...);`
- `Assets/Scripts/API/MapsFile.cs:576-577` — load: `new PakFile(arena2Path + "CLIMATE.PAK")` / `"POLITIC.PAK"`.
- `Assets/Scripts/API/PakFile.cs:28,31,34` — grid `1001 × 500` (`pakBufferLengthValue = 500,500`).
- `Assets/Scripts/API/PakFile.cs:110-135` — **actual PAK layout (RLE)**: 500 `UInt32` offsets (a 2,000 B table at the start); per row, at `offset`: repetitions `(UInt16 count, Byte value)` until 1001 cells are covered. This is why `CLIMATE.PAK` (14,771 B) is far smaller than 500,500 B.

### Supporting anchors (M15 / M16)

- **M16 — `TEXT.RSC`:** `Assets/Scripts/API/TextFile.cs:26` (`Filename = "TEXT.RSC"`), `:176-185` header (`UInt16 TextRecordHeaderLength`; elements `{ UInt16 TextRecordId; UInt32 Offset; }`), `:48` count `(TextRecordHeaderLength / 6) - 1`. **PINNED.**
- **M15 — names:** `Assets/Scripts/Game/Utility/NameHelper.cs:28` (`nameGenFilename = "NameGen"`), `:372-390` (`LoadNameGenData`: deserialises `NameGen.txt` into `Dictionary<BankTypes, NameBank>`). **PINNED as consumer** — see gap H1.

## 4. Declared gaps

| ID | Gap | State | Literal reason |
|---|---|---|---|
| **H1** | Binary layout of `NAMEGEN.DAT` | **NOT PINNED** | Daggerfall Unity **does not parse** `NAMEGEN.DAT` at runtime: it converts the datum to `Assets/Resources/NameGen.txt` (JSON: `setCount`/`sets[]`/`parts[]`) and `NameHelper` deserialises it (`NameHelper.cs:372-390`). The Trees API of `v1.1.1` contains no file that reads the `.DAT` (`Assets/Licenses/Daggerfall Connect.txt` is only a licence; `DaggerfallConnect` is not in this repo). No upstream source in this repo ⇒ no invented layout is written. Mitigation: the pinned `NameGen.txt` (row 6) gives the exact semantic model (banks/sets/parts) for validating a future parser. |
| **H2** | `PakFile.SetValue` / PAK writing | out of scope | Only reading is required for M13; the whole file is pinned anyway. |
| **H3** | `ManagedFile`/`FileProxy` (base I/O used by `WoodsFile`/`PakFile`) | not pinned | Layout semantics do not depend on it (it only opens an in-memory stream); it does not block M13. It can be pinned in a later change if friction appears. |

**Impact of the most expensive gap:** H1. M15 can implement the generator against the `NameGen.txt` model, but **cannot** claim byte-for-byte parity with `NAMEGEN.DAT` without an external specification (the retail `.DAT` lives in `ARENA2/`; empirically reverse-engineering it would be a separate task).

## 5. Licence and clean-room

- MIT identical to the `LICENSE.txt` already present in the corpus (`sha256 6e5e22fa…669a`): all 8 files declare `Copyright (C) 2009-2023 Daggerfall Workshop`.
- This material is **documentary input**. **0 bytes** of DFU are copied into `crates/`: the translation to Rust is clean-room from this specification.
- **0 retail bytes**: no `.WLD`, `.PAK`, `.RSC` or `.DAT` was copied; only `.cs` source code and the textual datum `NameGen.txt` (a code asset of the MIT repository, not retail data).

## 6. Traceability

Every write of this record and of the pinned files was logged one line per file in the line-level
write registry kept with the working record (not published here).

# reference/dfu-spec — Daggerfall Unity executable-spec corpus (MIT classification)

> Translation of the original Spanish custody record; claims unchanged. Only internal process
> identifiers were dropped: requirement and invariant ids of the working record were replaced by
> their public equivalent (system code, probe id). Upstream commits, file hashes, byte counts, row
> order and clean-room rules are as recorded.

Version 1.1 · 2026-10-21 · see also [PROVENANCE.md](PROVENANCE.md) for the source-class pin.

**Governing rule:** this directory is **NOT product code**. It is documentary clean-room input: an
executable reference specification for the parity (Mode A) and design work. **0 bytes of DFU are
copied into product crates.**

## 1. Origin and chain of custody

| Field | Value |
|---|---|
| Official repo | https://github.com/Interkarma/daggerfall-unity |
| Commit | `2343305d1d83ccc0de57a81e3b1e61188a997e34` (HEAD of the clone taken 2026-10-18) |
| Command | `git clone --depth 1 --filter=blob:none --sparse` + `git sparse-checkout set --no-cone` (32 file patterns, minimal necessary paths — see §7) |
| History | 0 history under reference/ (no `.git`; the clone lives in /tmp and is NOT copied) |
| Assets | 0 graphical/audio/binary assets, 0 Unity `.meta` files: only `.cs` plus textual licences/docs (check R3, §5) |
| Licence | MIT — `Copyright (c) 2009-2023 Daggerfall Workshop` |
| LICENSE | `LICENSE.txt` sha256 `6e5e22fa760938ab398743150787896bd20fcf4936aad0122704700bad1e669a` (1081 B) = byte-for-byte the repo's root `LICENSE` (renamed `LICENSE`→`LICENSE.txt` only to satisfy check R3 on extensions; the identical sha256 proves the content is equal) |
| NOTICE | **ABSENT** in the whole repo tree (`git ls-tree -r --name-only HEAD` + `grep -iE 'notice|copying'` → 0 hits) — documented here, 0 invented (MIT does not require a NOTICE) |

## 2. Chain verification against pin S2

- `Assets/Scripts/API/DFRandom.cs` of this corpus: sha256 `7f2a7a7e54168b254c6da18db55de66e1e4df2e84c3e7058c712c5eddfd5047b`, 3043 B.
- The pinned input manifest (kept with the working record, not published) lists the same sha256
  `7f2a7a7e54168b254c6da18db55de66e1e4df2e84c3e7058c712c5eddfd5047b` and the same 3043 B.
- **Result: sha256 equal and bytes equal → chain verified** (a match is itself the verification; 0 re-pinning).

## 3. Clean-room rule (anti-copy)

- Outside `reference/`: **0 DFU content in product crates**. Evidence: `grep -ril '1103515245' <target>/crates` → **0 hits (exit 1)**; native grep `1103515245|random_range_inclusive|Dice100` over `crates/` → **0 matches**; `glob **/*.cs` over the target → all 112 `.cs` files exist ONLY under `reference/dfu-spec/`.
- The translation to Rust is clean-room from this specification, partitioned by mode (DFU parity only in Mode A; constraint envelope recommendation 9).
- The selected corpus is deliberately NOT buildable in Unity (documentary reference); 0 compilation claims.

## 4. System legend (constraint envelope §2/§3 + the requirement inventory)

| Code | System (envelope) | Classification | Public anchor |
|---|---|---|---|
| A | Deterministic seed / RNG (§2, group C; invariant 1) | modifiable under contract (effectively untouchable in Mode A) | PROBE-SEED-001 |
| B | Combat/damage/healing/magic formulas (§2, group B; invariant 3) | modifiable under contract (Mode A parity) | PROBE-FORMULA-001 |
| C | Skill/attribute progression (§2, group B; invariant 4) | modifiable under contract (use-based mechanism) | — |
| D | Procedural terrain generation/chunking/world data (§2, groups A/C; invariants 1/2) | modifiable under contract | — |
| E | Questing DSL (§2, group C, quests) | 100% modifiable (content) | — |
| F | Time/weather (§2, group B) | 100% modifiable | — |
| G | Enemy AI/perception | **gap**: 0 dedicated row in the §2 envelope and 0 dedicated requirement — mapped to the nearest anchor, no invented row | — |
| L | Provenance/licence | MIT | — |

## 5. Acceptance checks (executed 2026-10-18)

- **R3 (0 assets):** `find reference/dfu-spec -type f ! -name '*.cs' ! -name '*.txt' ! -name '*.md'` → **empty (PASS)**.
- **Anti-copy:** see §3 → PASS.
- **Count:** 112 `.cs` + 1 `LICENSE.txt` + this classification record = 114 files · **113 mapping rows** in §6.

## 6. File → system mapping (sha256 per file)

| # | path (DFU repo) | sha256 | bytes | system | public anchor |
|---:|---|---|---:|---|---|
| 1 | `Assets/Scripts/API/DFCareer.cs` | `3aaf2adcca838f7c42c25ff6abf5e8bccd28ff5cfad5989ba57dabaa0283bc6d` | 27099 | C · skill/attribute/career catalogue (§2-B) | — |
| 2 | `Assets/Scripts/API/DFLocation.cs` | `5a9da091c7541052ea2648df6ceaa08318f59c3035a1770cf85a1474af719195` | 21896 | D · region/location worldgen data (§2-C) | — |
| 3 | `Assets/Scripts/API/DFRandom.cs` | `7f2a7a7e54168b254c6da18db55de66e1e4df2e84c3e7058c712c5eddfd5047b` | 3043 | A · deterministic seed / RNG (§2-C) | PROBE-SEED-001 · pin S2 |
| 4 | `Assets/Scripts/API/DFRegion.cs` | `c12f97ef62eb06f3c81fe69d5feaf3aa3bf81322a44d505d5bd909158d685f73` | 8218 | D · region/location worldgen data (§2-C) | — |
| 5 | `Assets/Scripts/Game/EnemyAttack.cs` | `4ce3fe4ca93909f017a32aa6e600083f045e39192b8587f1a2b90dd5fae45b46` | 18851 | G · enemy AI/perception (no row in the §2 envelope — gap) | — (enemy-AI gap) |
| 6 | `Assets/Scripts/Game/EnemyMotor.cs` | `7017a98e18cee8478cfd491cfe2b153c64b4d06c2f9384646e34773c69bafb51` | 60454 | G · enemy AI/perception (no row in the §2 envelope — gap) | — (enemy-AI gap) |
| 7 | `Assets/Scripts/Game/EnemySenses.cs` | `c9166a5aa4c1527698cb15992b18e4ba8c7a9ca8df5f8b3a8809623c96238d7c` | 40332 | G · enemy AI/perception (no row in the §2 envelope — gap) | — (enemy-AI gap) |
| 8 | `Assets/Scripts/Game/Entities/DaggerfallSkills.cs` | `ac30ffb76e053214aaad28c49d9a9bb482e3585d935357257a15615f8093b4e0` | 21114 | C · skill progression by use (§2-B) | — |
| 9 | `Assets/Scripts/Game/Formulas/FormulaHelper.cs` | `b7ba3b2dfdf3e10826b6c5532d593e35125731afac5fd2524ddae6c1c3a2a09d` | 153749 | B · combat/damage/healing/magic formulas (§2-B) | PROBE-FORMULA-001 |
| 10 | `Assets/Scripts/Game/Questing/Actions/AddAsQuestor.cs` | `655a4f3f2e9fb436a167b9e2a9446bbf71627ae27a6d135b9be1d0a3bc8e747a` | 2114 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 11 | `Assets/Scripts/Game/Questing/Actions/AddDialog.cs` | `079927bfc48ba7f910241ece97ec242eb5a2abfd27832046aef638b15ac0adec` | 4274 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 12 | `Assets/Scripts/Game/Questing/Actions/AddFace.cs` | `254468de22f943868440d6454e1059831e77e70bd6c4edf334da3b6c75fe570e` | 3584 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 13 | `Assets/Scripts/Game/Questing/Actions/CastEffectDo.cs` | `5909129bb679f836447cf6e5278dd243f76bcbfb0be7157f6b8fafbc814845db` | 4120 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 14 | `Assets/Scripts/Game/Questing/Actions/CastSpellDo.cs` | `8f3d5453307d8af5136b01eafad267640b6ae3c5e704268858f2a16f30a5755d` | 6125 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 15 | `Assets/Scripts/Game/Questing/Actions/CastSpellOnFoe.cs` | `702a08f61dced71c9912e887cce2a61453ebb0ce1aa07fad32be3002b071969f` | 4356 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 16 | `Assets/Scripts/Game/Questing/Actions/ChangeFoeInfighting.cs` | `2b4392f5d281c76fef315b303c2c0ec4b42ddedb418c79f564e28806baa46473` | 3154 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 17 | `Assets/Scripts/Game/Questing/Actions/ChangeFoeTeam.cs` | `b2ffde5538c8963aec8f99670632ed2cfd33cb33cb7f624367bb8fe66dad7bae` | 4052 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 18 | `Assets/Scripts/Game/Questing/Actions/ChangeReputeWith.cs` | `b8f8e19987b6dfb0c6cfdbf9952a44344c83382757f9b798a0690923738bde7b` | 3118 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 19 | `Assets/Scripts/Game/Questing/Actions/ClearTask.cs` | `2d2bb8db0315a09e954fc3ecd5c01cad3e6003b3493dd85c3eced7ecf36b3e18` | 3071 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 20 | `Assets/Scripts/Game/Questing/Actions/ClickedFoe.cs` | `1bdc8fe90af369191256daf5037261f6bee23e23b645b9abca3f15bbc8b63b39` | 5168 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 21 | `Assets/Scripts/Game/Questing/Actions/ClickedItem.cs` | `33137073d040cddf5df44886e4e520b8b5e4612cc849ad906f5944b92374e5da` | 3442 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 22 | `Assets/Scripts/Game/Questing/Actions/ClickedNpc.cs` | `9297fbd7443ce1968946ae2e7f49938c3922c0d55bb7a962c98fe8e76356961c` | 5379 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 23 | `Assets/Scripts/Game/Questing/Actions/Climate.cs` | `af965aee04b6292c8b188834ebe1295e50003f064af86d2932214b4b3319cdf7` | 3685 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 24 | `Assets/Scripts/Game/Questing/Actions/CreateFoe.cs` | `f00415a0eeb2f47ac3f378545493fbd18f4e9644d440c29b64aeed3372f1f234` | 17277 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 25 | `Assets/Scripts/Game/Questing/Actions/CreateNpcAt.cs` | `cdf36a905582207ce64fbabb1a981ea7a76bfdbd767102f3e69b2b4da712dd20` | 3342 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 26 | `Assets/Scripts/Game/Questing/Actions/CreateNpc.cs` | `bcf19e768afb17b7967ecacbb8b924b052a1bae4bfadfcb549f9690efab746f7` | 3079 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 27 | `Assets/Scripts/Game/Questing/Actions/CurePcDisease.cs` | `6fa42b70cde1b1e4409c38784bf4e569617a243ae633836a5eb41c454b536196` | 4008 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 28 | `Assets/Scripts/Game/Questing/Actions/DailyFrom.cs` | `6a2ba2a74b3af3ce711a555d00fe3a9a992a52133d98b02cad016b961fb23b1f` | 3418 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 29 | `Assets/Scripts/Game/Questing/Actions/DestroyNpc.cs` | `581119be05bca9e8394a6b9dbd4e94ed0e138b8d40c0fd04c4d0e69ba04c40ac` | 2691 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 30 | `Assets/Scripts/Game/Questing/Actions/DialogLink.cs` | `92c6909181c6ad01b66c82dc031c00a6ae1025a5d7f06b3507a3584ac466e83f` | 6532 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 31 | `Assets/Scripts/Game/Questing/Actions/DropAsQuestor.cs` | `34cce3a1a4343895879ec0cf9faef3cb0d1298f824af99da6254859f9d2c6e2f` | 2121 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 32 | `Assets/Scripts/Game/Questing/Actions/DropFace.cs` | `edca86d82952c0fd032ad5a44f5c52458e346338accc3589c27b56aeb3a2135c` | 3074 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 33 | `Assets/Scripts/Game/Questing/Actions/DroppedItemAtPlace.cs` | `45741843558323a71dfb9b98e8f7547139f8dea7d056467cc496c2c82f54b2d9` | 4146 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 34 | `Assets/Scripts/Game/Questing/Actions/EndQuest.cs` | `1adf076a8661b1f194758c0a37a5eb6e2c6f4d20633d6b7ff2c57d878664c0f0` | 2382 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 35 | `Assets/Scripts/Game/Questing/Actions/Enemies.cs` | `0da8b975c30b3a3123bdb7da2e246c2f0525fe0ba3e7e0f13f2727e981d977c1` | 2174 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 36 | `Assets/Scripts/Game/Questing/Actions/GetItem.cs` | `82f4dfa33ac7d462e088914a6f809ac911bd20b49768bf7b1ea6fc19a166c1bc` | 3991 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 37 | `Assets/Scripts/Game/Questing/Actions/GiveItem.cs` | `d79e21e0f0c140c60aec7c6be3c8812b84a494c86f089e26d9fcdd31b91e5593` | 5732 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 38 | `Assets/Scripts/Game/Questing/Actions/GivePc.cs` | `7c6d90a3eeed9bcd9a8dce2287ffd34ac4efe14984422640e9a11e118d8a88a5` | 8956 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 39 | `Assets/Scripts/Game/Questing/Actions/HaveItem.cs` | `44da2744a520bad51d461c7a4ac6f54cefe81b67ff33d8e6352fdecfd2b94690` | 2864 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 40 | `Assets/Scripts/Game/Questing/Actions/HideNpc.cs` | `bf8dd529064f199dbce17a4451e0568fd0795ed27a956b3c48ba43df1b5fa412` | 2360 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 41 | `Assets/Scripts/Game/Questing/Actions/InjuredFoe.cs` | `9455caf4ab52d0eb4dafc0ed10c89cb3f4c37ee9c1e1226fe620599451fadacf` | 2867 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 42 | `Assets/Scripts/Game/Questing/Actions/ItemUsedDo.cs` | `6d84dd637afe29483dcde6ffeea1316aabafe9eb3bd9ed2ce712801fa0269273` | 3388 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 43 | `Assets/Scripts/Game/Questing/Actions/JournalNote.cs` | `992e337f17b6fb5fda748828b632c221e45ae2ed3f69e534c1f4e10dfebc80e2` | 2193 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 44 | `Assets/Scripts/Game/Questing/Actions/KilledFoe.cs` | `522adeab6367523d66af30c0ff78705314ed8a229aca8d542ae8ea0942ed10fc` | 3189 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 45 | `Assets/Scripts/Game/Questing/Actions/KillFoe.cs` | `1ea824ce1f3ef1672f1d83bbd3dd09168594dbac3eabd97d6771e35cd212461f` | 2358 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 46 | `Assets/Scripts/Game/Questing/Actions/LegalRepute.cs` | `02b65be6bb185c8d1a56cb698fcbb676560e309aa97247c85f6d17acf2f3b464` | 2304 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 47 | `Assets/Scripts/Game/Questing/Actions/LevelCompleted.cs` | `34bdf769e7b48efee7286f80cfd44b3169b8bc6f9d5055328fbf76fe176f1929` | 2188 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 48 | `Assets/Scripts/Game/Questing/Actions/LogMessage.cs` | `82a5978b0418812638a6a4ec536f0fcf75b65088c4fc5ab1733385e64f9858a1` | 2889 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 49 | `Assets/Scripts/Game/Questing/Actions/MakePcDiseased.cs` | `2296538444bc68767636e7cf2bcc0c9cecda5f13861ba95d35c030530840943d` | 3056 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 50 | `Assets/Scripts/Game/Questing/Actions/MakePermanent.cs` | `89de97808f2f0496368e060003ea9b6510b813ab597b0fc0e044546b7907a805` | 2395 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 51 | `Assets/Scripts/Game/Questing/Actions/MuteNpc.cs` | `b9c4c889c9e9bb3ae26fdd09ed4ed85f323a337e4bbec628f86cad9df74899b6` | 2746 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 52 | `Assets/Scripts/Game/Questing/Actions/PayMoney.cs` | `db7839dd9b0dd69b89ff1dd3afec6c71f20a7d982c16b278f65bbfdeb71a7f8f` | 4307 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 53 | `Assets/Scripts/Game/Questing/Actions/PcAt.cs` | `9326571ac9442fc125d45eb91491feee8c2629e99e5665e95ec1a2a55a223705` | 6714 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 54 | `Assets/Scripts/Game/Questing/Actions/PickOneOf.cs` | `f4fb9247df55931503847b85f139884bf6cb5a67d1972be8dd8ddddd1edc823e` | 3728 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 55 | `Assets/Scripts/Game/Questing/Actions/PlaceFoe.cs` | `5707b461c2b1aad078faa913160ec80608a3ef9ca3af8c0eee83a18d133243d3` | 3604 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 56 | `Assets/Scripts/Game/Questing/Actions/PlaceItem.cs` | `7f80527adfb1d4173d63c1994d44dc5aac754a3c69f56fe46a942a6900d4e6cd` | 4723 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 57 | `Assets/Scripts/Game/Questing/Actions/PlaceNpc.cs` | `b88136ab3a059b56eca3b5c38be372538546e1d93184ebe350927b4c5e91a1cc` | 5010 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 58 | `Assets/Scripts/Game/Questing/Actions/PlaySong.cs` | `082263b0f0aa8da093ae89a211c66482222bb2c293a1bbf329019a27b1c7b3d3` | 2812 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 59 | `Assets/Scripts/Game/Questing/Actions/PlaySound.cs` | `1ba699d7251b48a741ef7caf0f3f9fc6eb35552b57915dd5135d8cedbbe10987` | 6687 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 60 | `Assets/Scripts/Game/Questing/Actions/PlayVideo.cs` | `010c94193608d2295d44a56358aa6f78f259f6dc7bc416517063fb0873b8c6bb` | 3153 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 61 | `Assets/Scripts/Game/Questing/Actions/Prompt.cs` | `e0b4b5a55f9381d431471634fa9a3309adb8c5d507c0530ef6dc282f50381d6f` | 3869 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 62 | `Assets/Scripts/Game/Questing/Actions/PromptMulti.cs` | `71986d13ce824f854ab548d96f9c3b7d1eb519fb85e870ac20f4278b7d12b224` | 6990 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 63 | `Assets/Scripts/Game/Questing/Actions/RemoveFoe.cs` | `6170ea368fb8c79a151bcf122636c97646aa8d0db08634ef9d7b8e573b9ca7d2` | 2346 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 64 | `Assets/Scripts/Game/Questing/Actions/RemoveLogMessage.cs` | `e95adf773a860dae0b5fa60b1764fd28f927edc21173e39a3947a8c45fc0acc5` | 2371 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 65 | `Assets/Scripts/Game/Questing/Actions/ReputeExceedsDo.cs` | `b6f7f2e9c937831c82eaef056d73978514edb85e3763ed81b41d9aff4c0effb6` | 3292 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 66 | `Assets/Scripts/Game/Questing/Actions/RestoreNpc.cs` | `e49485596f3b9851380fdf8a1d03e33a5f70e1df4e58fb55abe5917ae26860b9` | 2391 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 67 | `Assets/Scripts/Game/Questing/Actions/RestrainFoe.cs` | `05a2ed1171e0cf672e6c8f3eda1300864ba641c5138ce5a055b8a67a5fbf6005` | 2377 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 68 | `Assets/Scripts/Game/Questing/Actions/RevealLocation.cs` | `5691c65f5eae5000d65197583e8999085331fef4c1ac014ab0d770f7421a3d9e` | 2797 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 69 | `Assets/Scripts/Game/Questing/Actions/RumorMill.cs` | `791da3aabb3e5361419de1621578425603355fbfd0c18d8b8c1055f9ff80c13d` | 2155 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 70 | `Assets/Scripts/Game/Questing/Actions/RunQuest.cs` | `be825b5f3307d50f98f6612304d72de5b4f88f2a3a6e82b2fdb63c619081f47f` | 4548 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 71 | `Assets/Scripts/Game/Questing/Actions/Say.cs` | `2dfe839b4a64ab217cab6234ce88d3f8e3bcbdedc93349cfeff2146a0947df8f` | 2402 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 72 | `Assets/Scripts/Game/Questing/Actions/Season.cs` | `34240cbcf63714e3f07d8c26ec5a2e97715d844ebc463a96518ecd5174876af4` | 2420 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 73 | `Assets/Scripts/Game/Questing/Actions/SetPlayerCrime.cs` | `4d8d39ad00234ed40cf3216295b4e771fcb45e326fd985d9f5d5b88967d6cbea` | 2588 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 74 | `Assets/Scripts/Game/Questing/Actions/SpawnCityGuards.cs` | `ca5051a2f477602492ced8d414d6b2d956620efb9795fe832d6c8f5676c74686` | 2304 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 75 | `Assets/Scripts/Game/Questing/Actions/StartQuest.cs` | `caef02626cb8a23525234201d4f1a6db3f5312de251cfe840a23b7bdc1cbaa0f` | 3356 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 76 | `Assets/Scripts/Game/Questing/Actions/StartStopTimer.cs` | `b4c2df18d23e4e53c19184248a715429c746e95ae7ef944c9da2fabf48918da1` | 3062 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 77 | `Assets/Scripts/Game/Questing/Actions/StartTask.cs` | `3fc57711f5cbf555541841d301a56b7ffe51234237aa8be9e90635e41814b2de` | 2213 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 78 | `Assets/Scripts/Game/Questing/Actions/TakeItem.cs` | `f6457b9f9727cf626427c78b4d138b78b01d2234702ab219ecaeb95ce56661ba` | 3065 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 79 | `Assets/Scripts/Game/Questing/Actions/TeleportPc.cs` | `8bf705d2a86fef0bac8effe2299e65237adc05b44efc4cf0cf04e1d0a5e26295` | 6007 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 80 | `Assets/Scripts/Game/Questing/Actions/TotingItemAndClickedNpc.cs` | `b4018037bfcd0278b9a568a58daabc2161602cac81462176df41b773747973d7` | 4857 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 81 | `Assets/Scripts/Game/Questing/Actions/TrainPc.cs` | `c671bea6210c4369340b734c8ba3dea73aa3c75e71d12adaa2c0a4aebf47eeab` | 3616 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 82 | `Assets/Scripts/Game/Questing/Actions/UnrestrainFoe.cs` | `9d580d8471b08aebfa0f88bc17a1c8178c5dc51ee9dbd7c3dc75121b93f05c45` | 2293 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 83 | `Assets/Scripts/Game/Questing/Actions/UnsetTask.cs` | `534e5e23dcb3eed7c7fad1ce75e8eed3f48d1b3843aeca11365ebd936a095972` | 3012 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 84 | `Assets/Scripts/Game/Questing/Actions/Weather.cs` | `1897deb33b1b85c49257138343f113ec64995a3e48051372210f7eda8c163adb` | 3996 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 85 | `Assets/Scripts/Game/Questing/Actions/WhenAttributeLevel.cs` | `7a3ce1aadc42d2e9527d7433257c87f3e14512903faf33b87d68f7f8ffaa8d8a` | 2960 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 86 | `Assets/Scripts/Game/Questing/Actions/WhenNpcIsAvailable.cs` | `b80ab573fe0700fc7173cb5e75d0464a71a167c4a3b9171c7595cb5e5cf17983` | 4779 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 87 | `Assets/Scripts/Game/Questing/Actions/WhenPcEntersExits.cs` | `578dfdff5ef89695fe058b4883aa47e04e07d400c35574de3f68fdf470b2f566` | 7632 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 88 | `Assets/Scripts/Game/Questing/Actions/WhenReputeWith.cs` | `b5dd1c8fe74340950e8790bbc0417bf5c8117a4db1b2c73cc3ace13d3fb932a1` | 3655 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 89 | `Assets/Scripts/Game/Questing/Actions/WhenSkillLevel.cs` | `a8c9e4ef0c4ffb3a7b0025e62b92c5e653c90fc4a992ebcd69055dc05f55c08f` | 2810 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 90 | `Assets/Scripts/Game/Questing/Actions/WhenTask.cs` | `f23148988a5b695d5ca22ff3392afd467f470ce9f1afa39577b185ecf5e4d3b9` | 8421 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 91 | `Assets/Scripts/Game/Questing/Actions/WorldUpdate.cs` | `dcfdc685ac8b2af20f5df812fc4c3b8bc000917901780d5d32b574c589f5f4db` | 6283 | E · quest DSL: action vocabulary, 82 verbs (§2-C) | — |
| 92 | `Assets/Scripts/Game/Questing/Clock.cs` | `605b8d905b2a7ff66a0b3138575f476f6de964ca51fbeaf8e218ca61a80d56a3` | 20943 | E · quest DSL: core/resources (§2-C) | — |
| 93 | `Assets/Scripts/Game/Questing/Foe.cs` | `879d3687c9f8b28c21ffc73c62ddd6273f0b049877b9868fa877faaaf5311e35` | 12576 | E · quest DSL: core/resources (§2-C) | — |
| 94 | `Assets/Scripts/Game/Questing/Item.cs` | `3c71c8b81ea62567fa30027dd0bebcebccb8488daa819d30ed94e81a0f5ea815` | 18001 | E · quest DSL: core/resources (§2-C) | — |
| 95 | `Assets/Scripts/Game/Questing/Message.cs` | `9eb06eb0edc0427124cbe90704051f797f10fa0513a0ec197a71a585baa1f4bc` | 10369 | E · quest DSL: core/resources (§2-C) | — |
| 96 | `Assets/Scripts/Game/Questing/Parser.cs` | `31866a4d778a39a026f0009e20cc852dfdf14acd9f8d8128552f3e78f8dcb668` | 25572 | E · quest DSL: core/resources (§2-C) | — |
| 97 | `Assets/Scripts/Game/Questing/Person.cs` | `51841567fa3bd397bb248fd825322fe6a25641bb60c88c0c525a4ee0f8f7bc83` | 46320 | E · quest DSL: core/resources (§2-C) | — |
| 98 | `Assets/Scripts/Game/Questing/Place.cs` | `eed87012e2d5c2607d7b2287451338e088a34a3e1ca83560d7cafeeb590e7622` | 76890 | E · quest DSL: core/resources (§2-C) | — |
| 99 | `Assets/Scripts/Game/Questing/QuestAction.cs` | `5e2f69f883a97e390bf85ae4e8afefd0a7dafcd1d3218cf9dbaf7f08bec8d152` | 7323 | E · quest DSL: core/resources (§2-C) | — |
| 100 | `Assets/Scripts/Game/Questing/Quest.cs` | `093110348ef708ff5d17d45467ccd837cd3774a339e9ffc6c2f6fce96a58518d` | 38115 | E · quest DSL: core/resources (§2-C) | — |
| 101 | `Assets/Scripts/Game/Questing/QuestMachine.cs` | `baa79fae0fd9e9b3f9eb0136141dd68a7d0c77977f9e779aff9fa7a5af976bc6` | 75229 | E · quest DSL: core/resources (§2-C) | — |
| 102 | `Assets/Scripts/Game/Questing/QuestResource.cs` | `3536c05e0c61a84631962fda57c08a332dc0ce630cd0d6b19f71104cce7a652f` | 13173 | E · quest DSL: core/resources (§2-C) | — |
| 103 | `Assets/Scripts/Game/Questing/Symbol.cs` | `c1ff41744ccf338b53805e22b4f0d256fd54c26983b460623f898f97e540839c` | 3520 | E · quest DSL: core/resources (§2-C) | — |
| 104 | `Assets/Scripts/Game/Questing/Task.cs` | `667965d4e41a38bc0b46bcd45e0c3115f3cfff2e6be3ea42231768f4d6df8296` | 20153 | E · quest DSL: core/resources (§2-C) | — |
| 105 | `Assets/Scripts/Game/Utility/Dice100.cs` | `bc3199145953775aec5f8139a833402aca1e180cab0ccaed2228756bf147b89f` | 612 | B · d100 rolls for hit/damage/critical (§2-B) | PROBE-FORMULA-001 |
| 106 | `Assets/Scripts/Game/WeatherManager.cs` | `ca547da7747d9cd57420631b397835e45c06a0e53543e2cbbe7affa42db3485a` | 21030 | F · weather logic (§2-B) | — |
| 107 | `Assets/Scripts/Game/Weather/Weather.cs` | `d068ee3f72392817a057f8bb4549bbb26b7143e38d49fb8faa9e6a1c71be2d23` | 9056 | F · weather logic (§2-B) | — |
| 108 | `Assets/Scripts/Terrain/DefaultTerrainSampler.cs` | `b135df6228b2e373a413a95ed4a2156ce96d98a126e99b15c2658bddd97e01d9` | 7881 | D · procedural terrain generation (§2-A) | — |
| 109 | `Assets/Scripts/Terrain/StreamingWorld.cs` | `d00734766492a97e23c3f47dd5143c1f4d777299e8155582638bd4c927347d04` | 79514 | D · world streaming/chunking (chunking purity) | — |
| 110 | `Assets/Scripts/Terrain/TerrainHelper.cs` | `e88cb86b339c94add44b74b321c656d596c7ac3e11eaa4634910c24866b0194e` | 24586 | D · procedural terrain generation (§2-A) | — |
| 111 | `Assets/Scripts/Terrain/TerrainSampler.cs` | `5cab39dff746f3513a6695490f8904f29f390be3cd5518ebb576fcd8f99845ec` | 5727 | D · procedural terrain generation (§2-A) | — |
| 112 | `Assets/Scripts/Utility/DaggerfallDateTime.cs` | `3d63bd8cfe922e1b816461048bfcfd871a0d3e16e38872ec77481cb21ba8a9c0` | 20628 | F · time logic (§2-B) | — |
| 113 | `LICENSE (repo root) → LICENSE.txt` | `6e5e22fa760938ab398743150787896bd20fcf4936aad0122704700bad1e669a` | 1081 | L · provenance/licence | MIT (root LICENSE; no NOTICE in the tree) |

## 7. Candidate pack paths discarded (no inflation)

| Pack candidate | Verdict | Why |
|---|---|---|
| `Assets/Scripts/API/DFMap.cs` | discarded (does not exist) | the tree has no `DFMap.cs`; the `MAPS.BSA` parser is `MapsFile.cs` (container I/O → parser work; the format specification is already public in `dfbsa.txt`) |
| `Skills/ (SkillRolls)` | resolved inside the corpus | there is no `Game/Skills/` directory and no `SkillRolls*` file (0 in `git ls-tree`); the rolls live in `Game/Utility/Dice100.cs` plus `Dice100.SuccessRoll` calls in `FormulaHelper.cs` (L827 hit, L984 backstab, L1216 critical) — both included |
| `Game/World/` | redirected | it does not exist; real generation/streaming lives in `Assets/Scripts/Terrain/` (`TerrainHelper`, `TerrainSampler`, `DefaultTerrainSampler`, `StreamingWorld`) |
| `Quest/` | redirected + filtered | real path: `Assets/Scripts/Game/Questing/` → 13 core files + 82 actions included; discarded `QuestListsManager.cs`, `QuestMCP.cs`, `Components/`, `Demo/` (UI/macro pack loading, not DSL semantics) |
| `EnemyAI/EnemyMotor` | partial | there is no `EnemyAI.cs`; `EnemyMotor`/`EnemySenses`/`EnemyAttack` are included; 0 A* module in the tree (DFU moves enemies on a grid from EnemyMotor) — gap documented in §4-G |
| rest of `Utility/`, `MapsFile.cs`, `DFBsaFile.cs`, `Noise`/`Simple` samplers, terrain rendering (`DaggerfallTerrain`, `TerrainNature`, `TerrainTexturing`, `TerrainMaterialProvider`, `JobHelpers`), `PlayerWeather.cs`, the repo `README.md` | discarded | outside "minimal and necessary": Bethesda binary I/O belongs to the parser work; rendering is 100% modifiable (envelope group A); no inflation of the clone |

## 8. Traceability

Every write to this directory was logged one line per file in the line-level write registry kept
with the working record (not published here): 114 rows (113 corpus files + this record).

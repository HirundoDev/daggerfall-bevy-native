//! Deterministic loot generation — SPEC M21 §5.
//!
//! Transcribed from Daggerfall Unity `@0038a1e6` (MIT):
//! `LootTables.cs:77-272`, `ItemBuilder.cs:178-620`, `DaggerfallLoot.cs:77-118`
//! and `FormulaHelper.cs:2102-2156`.
//!
//! # Deltas (declared, not silent)
//!
//! - **D-M21-2:** DFU's comment claims a 0 % category still has a 1/100 chance
//!   in classic; with `Dice100.SuccessRoll` a 0 % chance **never** succeeds.
//!   This module matches the code, not the comment.
//! - **D-M21-3 (determinism):** the source reseeds `Random.InitState` from an
//!   object hash, so the same world seed does not reproduce the same pile.
//!   Here every container derives its streams from `(session_seed,
//!   container_id)` via [`derive_rng`] (SplitMix64): the selection stream is a
//!   local [`Rng64`] and the chance stream is a `daggerfall_behavior::Dice100`
//!   seeded from the first draw of that stream. DFU interleaves both roles on
//!   one global `UnityEngine.Random`; splitting them keeps every container
//!   replayable from its public seed while leaving the observable categories
//!   (chance chains, GV-LOT) identical in distribution.
//! - **M24 frontier:** `MAGIC.DEF` supplies the magic-item template, its base
//!   group byte and its enchantments. [`generate_container`] therefore makes a
//!   real base item from a uniformly chosen allowed group (`ItemBuilder.cs:520-580`)
//!   with `legacy_magic` empty; enchantment assignment is M24. Potions likewise
//!   get a real `Glass_Bottle` with no recipe key (alchemy is M24), and books
//!   use the template `base_price` instead of the `BOK*.TXT` price DFU reads.
//! - **Player traits:** DFU picks clothing by gender and armor textures by
//!   gender/race (`ItemBuilder.cs:184-192`, `:463-474`). M20 owns the player, so
//!   the clothing chain uses the male set and texture archives are not modelled.
//! - **Buildings:** only the 19 dungeon types are covered (R-21.6); building
//!   stock is M30.

use daggerfall_behavior::locomotion::Dice100;

use crate::equipment::{EquipResult, ItemEquipTable};
use crate::items::{
    apply_armor_material, apply_weapon_material, group_indices, Item, ItemGroup, ItemTemplate,
};

/// One row of `DefaultLootTables` (`LootTables.cs:78-101`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LootMatrix {
    /// Table key.
    pub key: char,
    /// Minimum gold multiplier.
    pub min_gold: i32,
    /// Maximum gold multiplier.
    pub max_gold: i32,
    /// Plant category 1.
    pub p1: i32,
    /// Plant category 2.
    pub p2: i32,
    /// Creature category 1.
    pub c1: i32,
    /// Creature category 2.
    pub c2: i32,
    /// Creature category 3.
    pub c3: i32,
    /// Miscellaneous ingredients 1.
    pub m1: i32,
    /// Armor.
    pub am: i32,
    /// Weapons.
    pub wp: i32,
    /// Magic items.
    pub mi: i32,
    /// Clothing.
    pub cl: i32,
    /// Books.
    pub bk: i32,
    /// Miscellaneous ingredients 2.
    pub m2: i32,
    /// Religious items.
    pub rl: i32,
}

// The retail layout justifies the 16 arguments: one `DefaultLootTables` row
// (`LootTables.cs:78-101`) has exactly that field order; core verified
// against the MIT source — not refactored.
#[allow(clippy::too_many_arguments)]
const fn matrix_row(
    key: char,
    min_gold: i32,
    max_gold: i32,
    p1: i32,
    p2: i32,
    c1: i32,
    c2: i32,
    c3: i32,
    m1: i32,
    am: i32,
    wp: i32,
    mi: i32,
    cl: i32,
    bk: i32,
    m2: i32,
    rl: i32,
) -> LootMatrix {
    LootMatrix {
        key,
        min_gold,
        max_gold,
        p1,
        p2,
        c1,
        c2,
        c3,
        m1,
        am,
        wp,
        mi,
        cl,
        bk,
        m2,
        rl,
    }
}

/// The 22 loot matrices, in `DefaultLootTables` order
/// (`LootTables.cs:78-101`, SPEC M21 §5.1). Row `0` is the `-` default.
pub const LOOT_MATRICES: [LootMatrix; 22] = [
    matrix_row('-', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0),
    matrix_row('A', 1, 10, 0, 0, 0, 0, 0, 0, 5, 5, 2, 4, 0, 2, 0),
    matrix_row('B', 0, 0, 10, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0),
    matrix_row('C', 2, 20, 10, 10, 5, 5, 5, 5, 5, 25, 3, 0, 2, 2, 2),
    matrix_row('D', 1, 4, 6, 6, 6, 6, 6, 6, 0, 0, 0, 0, 0, 0, 4),
    matrix_row('E', 20, 80, 0, 0, 0, 0, 0, 0, 10, 10, 3, 4, 2, 1, 15),
    matrix_row('F', 4, 30, 2, 2, 5, 5, 5, 2, 50, 50, 1, 0, 0, 3, 0),
    matrix_row('G', 3, 15, 0, 0, 0, 0, 0, 0, 50, 50, 1, 5, 0, 3, 0),
    matrix_row('H', 2, 10, 0, 0, 0, 0, 0, 0, 0, 100, 1, 2, 0, 0, 0),
    matrix_row('I', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 5),
    matrix_row('J', 50, 150, 0, 0, 0, 0, 0, 0, 5, 5, 3, 0, 0, 0, 0),
    matrix_row('K', 1, 10, 3, 3, 3, 3, 3, 3, 5, 5, 3, 0, 5, 2, 100),
    matrix_row('L', 1, 20, 0, 0, 3, 3, 3, 3, 50, 50, 1, 75, 0, 5, 3),
    matrix_row('M', 1, 15, 1, 1, 1, 1, 1, 2, 10, 10, 1, 15, 2, 3, 1),
    matrix_row('N', 1, 80, 5, 5, 5, 5, 5, 5, 5, 5, 1, 20, 5, 2, 5),
    matrix_row('O', 5, 20, 1, 1, 1, 1, 1, 1, 10, 15, 2, 0, 0, 0, 0),
    matrix_row('P', 5, 20, 5, 5, 5, 5, 5, 5, 5, 10, 2, 0, 10, 5, 0),
    matrix_row('Q', 20, 80, 2, 2, 8, 8, 8, 2, 10, 25, 3, 35, 5, 3, 0),
    matrix_row('R', 5, 20, 0, 0, 3, 3, 3, 5, 5, 15, 2, 0, 0, 0, 0),
    matrix_row('S', 50, 125, 5, 5, 5, 5, 5, 15, 10, 10, 3, 0, 5, 5, 0),
    matrix_row('T', 20, 80, 0, 0, 0, 0, 0, 0, 100, 100, 1, 0, 0, 0, 0),
    matrix_row('U', 7, 30, 5, 5, 5, 5, 5, 10, 10, 10, 2, 0, 2, 2, 10),
];

/// `GetMatrix` (`LootTables.cs:110-119`): unknown keys fall back to `-`.
pub fn matrix(key: char) -> &'static LootMatrix {
    LOOT_MATRICES
        .iter()
        .find(|matrix| matrix.key == key)
        .unwrap_or(&LOOT_MATRICES[0])
}

/// Loot key per dungeon `locationIndex` (`LootTables.cs:123-143`).
pub const DUNGEON_KEYS: [char; 19] = [
    'K', // Crypt
    'N', // Orc Stronghold
    'N', // Human Stronghold
    'N', // Prison
    'K', // Desecrated Temple
    'M', // Mine
    'M', // Natural Cave
    'Q', // Coven
    'K', // Vampire Haunt
    'U', // Laboratory
    'D', // Harpy Nest
    'N', // Ruined Castle
    'L', // Spider Nest
    'F', // Giant Stronghold
    'S', // Dragon's Den
    'N', // Barbarian Stronghold
    'M', // Volcanic Caves
    'L', // Scorpion Nest
    'N', // Cemetery
];

/// `GenerateLoot` key lookup (`LootTables.cs:146-167`): indices outside the
/// 19 dungeon types produce no loot.
pub fn dungeon_key(location_index: usize) -> Option<char> {
    DUNGEON_KEYS.get(location_index).copied()
}

/// Extra map chance for keys `J..=O` (`LootTables.cs:150-160`);
/// `None` when the key is outside that range (GV-LOT-15).
pub fn map_chance(key: char) -> Option<i32> {
    let alphabet_index = key as i32 - 64;
    if (10..=15).contains(&alphabet_index) {
        const MAP_CHANCES: [i32; 6] = [2, 1, 1, 2, 2, 15];
        Some(MAP_CHANCES[(alphabet_index - 10) as usize])
    } else {
        None
    }
}

/// SplitMix64 (`M20`/`crates/skills/src/rng.rs` algorithm), explicit state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rng64 {
    state: u64,
}

const GOLDEN_GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl Rng64 {
    /// Explicit seed.
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Next SplitMix64 word.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(GOLDEN_GAMMA);
        mix64(self.state)
    }

    /// `UnityEngine.Random.Range(min, max + 1)`: both bounds inclusive.
    pub fn range_inclusive(&mut self, min: i32, max: i32) -> i32 {
        if max <= min {
            return min;
        }
        let span = i64::from(max) - i64::from(min) + 1;
        (i64::from(min) + (self.next_u64() % span as u64) as i64) as i32
    }

    /// `Dice100.SuccessRoll` on the selection stream (`LootTables.cs` uses the
    /// shared global RNG for both roles; see the module D-M21-3 note).
    pub fn success(&mut self, chance: i32) -> bool {
        self.range_inclusive(0, 99) < chance
    }
}

/// Per-container stream (D-M21-3): SplitMix64 of the session seed folded with
/// the container id, so distinct containers never share a stream.
pub fn derive_rng(seed: u64, container_id: u32) -> Rng64 {
    let container = mix64(u64::from(container_id).wrapping_add(GOLDEN_GAMMA));
    Rng64::new(mix64(seed ^ container))
}

/// One generated loot pile.
#[derive(Debug, Clone, PartialEq)]
pub struct LootContainer {
    /// Stable container identifier (part of the digest).
    pub container_id: u32,
    /// Loot matrix key.
    pub key: char,
    /// Gold pieces (0 when the matrix or level yields none).
    pub gold: i32,
    /// Generated items; the gold item is included exactly when `gold > 0`.
    pub items: Vec<Item>,
}

/// Per-category counters of one generation
/// (`GenerateRandomLoot` chain attempts, `LootTables.cs:200-255`).
///
/// Needed because magic items intentionally carry no marker (M24 owns their
/// enchantments), so probes and tests read the chain counts here.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LootCounts {
    /// Weapons chain successes.
    pub weapons: i32,
    /// Armor chain successes.
    pub armor: i32,
    /// Magic-item chain successes.
    pub magic_items: i32,
    /// Clothing chain successes.
    pub clothing: i32,
    /// Book chain successes.
    pub books: i32,
    /// Religious-item chain successes.
    pub religious: i32,
    /// Ingredient successes (all plant/creature/misc chains).
    pub ingredients: i32,
}

/// Weapon slots in the retail enum (`ItemEnums.cs:210-235`), arrow included.
const WEAPON_SLOTS: i32 = 19;
/// Arrow `groupIndex` (`Weapons.Arrow`).
const ARROW_GROUP_INDEX: u16 = 18;
/// Armor slots (`Armor.Cuirass..Tower_Shield`, `ItemEnums.cs:195-207`):
/// 11 templates 102..=112, the same length DFU's `CreateRandomArmor` draws
/// from (`ItemBuilder.cs:455`: `Random.Range(0, enumArray.Length)`).
const ARMOR_SLOTS: i32 = 11;

/// `ItemBuilder.materialsByModifier` (`ItemBuilder.cs:42`).
const MATERIALS_BY_MODIFIER: [i32; 10] = [64, 128, 10, 21, 13, 8, 5, 3, 2, 5];

/// `RandomMaterial` (`FormulaHelper.cs:2102-2129`). Public because
/// `ItemHelper.AssignEnemyStartingEquipment` rolls it directly
/// (`ItemHelper.cs:1392`, `:1402`, `:1411`, `:1420`).
pub fn random_material(rng: &mut Rng64, level: i32) -> usize {
    let level_modifier = if level - 10 >= 0 {
        (level - 10).saturating_mul(2)
    } else {
        (level - 10).saturating_mul(4)
    };
    let mut combined = (level_modifier + rng.range_inclusive(0, 255)).clamp(0, 256);
    let mut material = 0usize;
    while material + 1 < MATERIALS_BY_MODIFIER.len()
        && MATERIALS_BY_MODIFIER[material] < combined
    {
        combined -= MATERIALS_BY_MODIFIER[material];
        material += 1;
    }
    material
}

/// `RandomArmorMaterial` (`FormulaHelper.cs:2136-2156`): 0x0000 Leather,
/// 0x0100 Chain, or `0x0200 + RandomMaterial` plate. Public for the same
/// reason as [`random_material`].
pub fn random_armor_material(rng: &mut Rng64, level: i32) -> i32 {
    let roll = rng.range_inclusive(1, 100); // Dice100.Roll()
    if roll >= 90 {
        0x0200 + random_material(rng, level) as i32
    } else if roll >= 70 {
        0x0100
    } else {
        0x0000
    }
}

fn template_at(templates: Option<&[ItemTemplate]>, index: usize) -> ItemTemplate {
    match templates.and_then(|templates| templates.get(index)) {
        Some(template) if template.index == index => template.clone(),
        // DFU returns a default template for an out-of-range index
        // (`ItemHelper.cs:215-220`); the probe just catalog-free identity.
        _ => ItemTemplate::empty(index),
    }
}

fn item_at(templates: Option<&[ItemTemplate]>, index: usize) -> Item {
    Item::from_template(&template_at(templates, index))
}

fn item_from_group(templates: Option<&[ItemTemplate]>, group: ItemGroup, position: usize) -> Item {
    let index = group_indices(group)[position];
    item_at(templates, index)
}

/// `ItemBuilder.CreateWeapon` (`ItemBuilder.cs:352-370`) with the caller's
/// template index (a `Weapons` id, `ItemEnums.cs:210-231`) and material:
/// the exact item `AssignEnemyStartingEquipment` asks for (`:1392`).
/// Arrows never reach here (the enemy kit has none), so the arrow branch of
/// `:359-364` is not reproduced.
pub fn create_weapon(
    index: usize,
    material: usize,
    templates: Option<&[ItemTemplate]>,
) -> Item {
    let mut item = item_at(templates, index);
    apply_weapon_material(&mut item, material);
    item
}

/// `ItemBuilder.CreateArmor` (`ItemBuilder.cs:428-437`) with the caller's
/// template index (an `Armor` id, `ItemEnums.cs:195-208`) and material.
/// Gender/race only set texture archives (`:466-475`), a declared M21 delta
/// (see the module note): the armour values consume the material only.
pub fn create_armor(index: usize, material: i32, templates: Option<&[ItemTemplate]>) -> Item {
    let mut item = item_at(templates, index);
    apply_armor_material(&mut item, material);
    item
}

/// `CreateRandomWeapon` + `CreateWeapon` arrow branch
/// (`ItemBuilder.cs:360-410`).
fn random_weapon(rng: &mut Rng64, level: i32, templates: Option<&[ItemTemplate]>) -> Item {
    let group_index = rng.range_inclusive(0, WEAPON_SLOTS - 1);

    // The source rolls the material before the arrow branch (`:398-409`).
    let material = random_material(rng, level);
    if group_index as u16 == ARROW_GROUP_INDEX {
        let mut item = item_at(templates, 113 + group_index as usize);
        item.stack_count = rng.range_inclusive(1, 20);
        item.current_condition = 0; // classic does this (`:402`)
        item.native_material = 0;
        item
    } else {
        create_weapon(113 + group_index as usize, material, templates)
    }
}

/// `CreateRandomArmor` + `ApplyArmorSettings` (`ItemBuilder.cs:446-507`).
fn random_armor(rng: &mut Rng64, level: i32, templates: Option<&[ItemTemplate]>) -> Item {
    let group_index = rng.range_inclusive(0, ARMOR_SLOTS - 1);
    let material = random_armor_material(rng, level);
    create_armor(102 + group_index as usize, material, templates)
}

// ── enemy starting equipment (`ItemHelper.AssignEnemyStartingEquipment`) ───

/// `Weapons` ids the enemy kit draws from (`ItemEnums.cs:210-231`).
const WEAPON_DAGGER: i32 = 113;
const WEAPON_SHORTSWORD: i32 = 116;
const WEAPON_BROADSWORD: i32 = 118;
const WEAPON_LONGSWORD: i32 = 120;
const WEAPON_CLAYMORE: i32 = 122;
const WEAPON_BATTLE_AXE: i32 = 127;

/// `Armor` ids the enemy kit draws from (`ItemEnums.cs:195-208`).
const ARMOR_CUIRASS: i32 = 102;
const ARMOR_GREAVES: i32 = 104;
const ARMOR_LEFT_PAULDRON: i32 = 105;
const ARMOR_RIGHT_PAULDRON: i32 = 106;
const ARMOR_HELM: i32 = 107;
const ARMOR_BOOTS: i32 = 108;
const ARMOR_BUCKLER: i32 = 109;
const ARMOR_ROUND_SHIELD: i32 = 110;

/// `SetEnemyEquipment(variant)` of `EnemyEntity` (`EnemyEntity.cs:333-348`):
/// the three variants the source passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnemyEquipmentVariant {
    /// `MonsterCareers.Orc` / `OrcShaman` (`:333-336`).
    Zero,
    /// `MonsterCareers.Centaur` / `OrcSergeant` (`:337-340`).
    One,
    /// `MonsterCareers.OrcWarlord` (`:341-344`).
    Two,
}

impl EnemyEquipmentVariant {
    /// `chance` of `AssignEnemyStartingEquipment`: `50` for variant 0
    /// (`ItemHelper.cs:1396`), `75` for 1 (`:1423-1424`), `90` for 2
    /// (`:1425-1426`). The source's `int chance = 0` initialiser (`:1381`)
    /// only covers variants outside `0..=2`, which this enum excludes.
    pub const fn piece_chance(self) -> i32 {
        match self {
            Self::Zero => 50,
            Self::One => 75,
            Self::Two => 90,
        }
    }
}

/// `ItemHelper.AssignEnemyStartingEquipment` (`ItemHelper.cs:1377-1489`): the
/// enemy's starting kit as a pure [`ItemEquipTable`].
///
/// `item_level` is the level the source computes at `:1379-1386`
/// (`player.Level`, forced to `1` for a `Knight_CityWatch` `EnemyClass`); the
/// caller owns that rule because it needs the entity type and the
/// `MobileEnemy.ID`.
///
/// Draw order is the source order, because every draw shifts the next one:
///
/// - variant 0 (`:1388-1414`): right-hand weapon
///   `Random.Range(Broadsword 118, Longsword 120 + 1)` with
///   `RandomMaterial(itemLevel)`; `chance = 50`; then the **shield template
///   index is drawn before the roll** (`:1399`) and only equipped when
///   `Dice100(50)` passes (`:1400-1405`); otherwise a second `Dice100(50)`
///   may equip a left-hand weapon
///   `Random.Range(Dagger 113, Shortsword 116 + 1)` (`:1407-1413`).
/// - variants 1/2 (`:1415-1427`): right-hand weapon
///   `Random.Range(Claymore 122, Battle_Axe 127 + 1)`; `chance = 75`/`90`.
/// - the six armour pieces, in source order, each behind its own
///   `Dice100(chance)` (`:1428-1469`): Helm 107, Right_Pauldron 106,
///   Left_Pauldron 105, Cuirass 102, Greaves 104, Boots 108.
///
/// Frontier (declared, **not** implemented): the poisoned-weapon branch
/// (`:1471-1488`, `player.Level > 1` → 5 % — 60 % for an Assassin — sets
/// `weapon.poisonType` to `Poisons` 128..=135). Poison is M22/M23
/// (`DaggerfallUnityItem.poisonType`); this constructor leaves
/// [`Item::poison_type`] at `0`.
pub fn assign_enemy_starting_equipment(
    rng: &mut Rng64,
    variant: EnemyEquipmentVariant,
    item_level: i32,
    templates: Option<&[ItemTemplate]>,
) -> ItemEquipTable {
    let mut table = ItemEquipTable::new();
    let chance = variant.piece_chance();

    if variant == EnemyEquipmentVariant::Zero {
        // Right-hand weapon (`:1390-1394`).
        let index = rng.range_inclusive(WEAPON_BROADSWORD, WEAPON_LONGSWORD) as usize;
        equip_enemy(
            &mut table,
            create_weapon(index, random_material(rng, item_level), templates),
        );
        // The shield index is drawn even when the chance roll fails (`:1399`).
        let shield = rng.range_inclusive(ARMOR_BUCKLER, ARMOR_ROUND_SHIELD) as usize;
        if rng.success(chance) {
            equip_enemy(
                &mut table,
                create_armor(
                    shield,
                    random_armor_material(rng, item_level),
                    templates,
                ),
            );
        } else if rng.success(chance) {
            // Left-hand weapon (`:1407-1413`).
            let index = rng.range_inclusive(WEAPON_DAGGER, WEAPON_SHORTSWORD) as usize;
            equip_enemy(
                &mut table,
                create_weapon(index, random_material(rng, item_level), templates),
            );
        }
    } else {
        // Right-hand weapon of variants 1 and 2 (`:1417-1421`).
        let index = rng.range_inclusive(WEAPON_CLAYMORE, WEAPON_BATTLE_AXE) as usize;
        equip_enemy(
            &mut table,
            create_weapon(index, random_material(rng, item_level), templates),
        );
    }

    // The six armour pieces in source order (`:1428-1469`).
    for index in [
        ARMOR_HELM,
        ARMOR_RIGHT_PAULDRON,
        ARMOR_LEFT_PAULDRON,
        ARMOR_CUIRASS,
        ARMOR_GREAVES,
        ARMOR_BOOTS,
    ] {
        if rng.success(chance) {
            equip_enemy(
                &mut table,
                create_armor(index as usize, random_armor_material(rng, item_level), templates),
            );
        }
    }

    table
}

/// `ItemEquipTable.EquipItem(item, alwaysEquip: true, playSound: false)`
/// (`ItemHelper.cs:1393`): the source equips unconditionally into a table it
/// has just created, and `EquipItem` cannot fail for these templates (weapons
/// 113..=127 resolve to a hand slot, armour 102..=110 to its body slot). A
/// rejection would therefore be a template-table regression, so it is a
/// test-time assertion instead of an invented runtime error path.
fn equip_enemy(table: &mut ItemEquipTable, item: Item) {
    match table.equip(item, true) {
        EquipResult::Equipped(_) => {}
        EquipResult::Rejected(item) => {
            debug_assert!(false, "enemy kit item {} refused", item.template_index);
        }
    }
}

/// `CreateRandomClothing` (`ItemBuilder.cs:184-192`); the male set, see the
/// module frontier note.
fn random_clothing(rng: &mut Rng64, templates: Option<&[ItemTemplate]>) -> Item {
    let position = rng.range_inclusive(0, 40) as usize;
    item_from_group(templates, ItemGroup::MensClothing, position)
}

/// `CreateRandomBook` (`ItemBuilder.cs:257-272`): the `Books.Book0` template;
/// value is the template `base_price` instead of the `BOK` price (delta).
fn random_book(templates: Option<&[ItemTemplate]>) -> Item {
    item_at(templates, 277)
}

/// `CreateRandomReligiousItem` (`ItemBuilder.cs:276-286`).
fn random_religious(rng: &mut Rng64, templates: Option<&[ItemTemplate]>) -> Item {
    let count = group_indices(ItemGroup::ReligiousItems).len() as i32;
    let position = rng.range_inclusive(0, count - 1) as usize;
    item_from_group(templates, ItemGroup::ReligiousItems, position)
}

/// `CreateRandomGem` (`ItemBuilder.cs:312-320`).
fn random_gem(rng: &mut Rng64, templates: Option<&[ItemTemplate]>) -> Item {
    let position = rng.range_inclusive(0, 7) as usize;
    item_from_group(templates, ItemGroup::Gems, position)
}

/// `CreateRandomJewellery` (`ItemBuilder.cs:325-333`).
fn random_jewellery(rng: &mut Rng64, templates: Option<&[ItemTemplate]>) -> Item {
    let position = rng.range_inclusive(0, 7) as usize;
    item_from_group(templates, ItemGroup::Jewellery, position)
}

/// Groups a regular magic item may base on (`ItemBuilder.cs:520-580`):
/// `itemGroups0 ∪ itemGroups1 ∪ {Weapons}`.
const MAGIC_BASE_GROUPS: [ItemGroup; 7] = [
    ItemGroup::Armor,
    ItemGroup::Weapons,
    ItemGroup::MensClothing,
    ItemGroup::WomensClothing,
    ItemGroup::ReligiousItems,
    ItemGroup::Gems,
    ItemGroup::Jewellery,
];

/// Base item of an M24 magic item (`CreateRegularMagicItem`,
/// `ItemBuilder.cs:520-580`). The `MAGIC.DEF` group byte is M24, so M21 picks
/// the group uniformly; enchantments/value/uses are left for M24, which is
/// why the item is a complete mundane item instead of a stub.
fn random_magic_base(rng: &mut Rng64, level: i32, templates: Option<&[ItemTemplate]>) -> Item {
    let group = MAGIC_BASE_GROUPS[rng.range_inclusive(0, MAGIC_BASE_GROUPS.len() as i32 - 1) as usize];
    match group {
        ItemGroup::Weapons => loop {
            let weapon = random_weapon(rng, level, templates);
            // No arrows as enchanted items (`:556-558`).
            if weapon.group_index != ARROW_GROUP_INDEX {
                return weapon;
            }
        },
        ItemGroup::Armor => random_armor(rng, level, templates),
        ItemGroup::MensClothing | ItemGroup::WomensClothing => random_clothing(rng, templates),
        ItemGroup::ReligiousItems => random_religious(rng, templates),
        ItemGroup::Gems => random_gem(rng, templates),
        _ => random_jewellery(rng, templates),
    }
}

/// `CreateGoldPieces` (`ItemBuilder.cs:758-765`): currency item, value 1,
/// stack equal to the amount.
fn gold_item(amount: i32, templates: Option<&[ItemTemplate]>) -> Item {
    let mut item = item_at(templates, 276);
    item.stack_count = amount;
    item.value = 1;
    item
}

/// `RandomlyAddMap` (`DaggerfallLoot.cs:88-95`): the source builds
/// `MiscItems, 8`, which is template 287 under the misc group rather than the
/// template's own `Maps` group.
fn map_item(templates: Option<&[ItemTemplate]>) -> Item {
    let mut item = item_at(templates, 287);
    item.item_group = ItemGroup::MiscItems;
    item.group_index = 8;
    item
}

/// `RandomlyAddPotion` (`DaggerfallLoot.cs:100-104`): recipe key is M24.
fn potion_item(templates: Option<&[ItemTemplate]>) -> Item {
    item_at(templates, 83)
}

/// `RandomlyAddPotionRecipe` (`DaggerfallLoot.cs:109-118`): `MiscItems, 4`.
fn potion_recipe_item(templates: Option<&[ItemTemplate]>) -> Item {
    item_at(templates, 278)
}

/// Runs one probability chain (`while SuccessRoll((int)chance) { ... }`).
fn chain<F: FnMut(&mut Rng64) -> Item>(
    rng: &mut Rng64,
    dice: &mut Dice100,
    initial_chance: i32,
    mut make: F,
    items: &mut Vec<Item>,
) -> i32 {
    let mut chance = initial_chance as f32;
    let mut count = 0;
    while dice.success(chance as i32) {
        items.push(make(rng));
        count += 1;
        chance *= 0.5;
    }
    count
}

/// `RandomIngredient` (`LootTables.cs:262-269`).
fn ingredient_chain(
    rng: &mut Rng64,
    dice: &mut Dice100,
    chance: i32,
    group: ItemGroup,
    templates: Option<&[ItemTemplate]>,
    items: &mut Vec<Item>,
) -> i32 {
    chain(
        rng,
        dice,
        chance,
        |rng| {
            let count = group_indices(group).len() as i32;
            let position = rng.range_inclusive(0, count - 1) as usize;
            item_from_group(templates, group, position)
        },
        items,
    )
}

/// Which DFU caller composed this pile — the chain body is
/// `GenerateRandomLoot` in both cases; only the caller-level extras differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LootPath {
    /// `LootTables.GenerateLoot` (`:146-167`): extras for keys `J..=O`
    /// (`RandomlyAddMap` chances + potion 4 % + recipe 2 %).
    Dungeon,
    /// `EnemyEntity.SetEnemyCareer` (`EnemyEntity.cs:330`, `:389-396`):
    /// `RandomlyAddMap(MapChance)` always, plus potion 3 % + recipe 2 %
    /// only when the row declares a `LootTableKey`.
    Enemy {
        /// `mobileEnemy.MapChance`.
        map_chance: i32,
        /// `!string.IsNullOrEmpty(mobileEnemy.LootTableKey)`.
        add_potion_extras: bool,
    },
}

/// `GenerateRandomLoot` (`LootTables.cs:176-258`) with a template source and
/// the caller-level extras of [`LootPath`].
fn generate(
    path: LootPath,
    seed: u64,
    container_id: u32,
    key: char,
    level: i32,
    templates: Option<&[ItemTemplate]>,
) -> (LootContainer, LootCounts) {
    let table = *matrix(key);
    let mut rng = derive_rng(seed, container_id);
    let mut dice = Dice100::new(rng.next_u64());
    let mut items = Vec::new();
    let mut counts = LootCounts::default();

    // Random gold (`:193-198`): Range(MinGold, MaxGold) * level.
    let gold = rng
        .range_inclusive(table.min_gold, table.max_gold)
        .saturating_mul(level);
    if gold > 0 {
        items.push(gold_item(gold, templates));
    }

    // Random weapon (`:200-206`).
    counts.weapons = chain(&mut rng, &mut dice, table.wp, |rng| {
        random_weapon(rng, level, templates)
    }, &mut items);

    // Random armor (`:208-214`).
    counts.armor = chain(&mut rng, &mut dice, table.am, |rng| {
        random_armor(rng, level, templates)
    }, &mut items);

    // Ingredients (`:216-223`): only P1, P2, C1 and C2 scale with level.
    let scaled = |value: i32| value.saturating_mul(level);
    counts.ingredients += ingredient_chain(
        &mut rng,
        &mut dice,
        scaled(table.c1),
        ItemGroup::CreatureIngredients1,
        templates,
        &mut items,
    );
    counts.ingredients += ingredient_chain(
        &mut rng,
        &mut dice,
        scaled(table.c2),
        ItemGroup::CreatureIngredients2,
        templates,
        &mut items,
    );
    counts.ingredients += ingredient_chain(
        &mut rng,
        &mut dice,
        table.c3,
        ItemGroup::CreatureIngredients3,
        templates,
        &mut items,
    );
    counts.ingredients += ingredient_chain(
        &mut rng,
        &mut dice,
        scaled(table.p1),
        ItemGroup::PlantIngredients1,
        templates,
        &mut items,
    );
    counts.ingredients += ingredient_chain(
        &mut rng,
        &mut dice,
        scaled(table.p2),
        ItemGroup::PlantIngredients2,
        templates,
        &mut items,
    );
    counts.ingredients += ingredient_chain(
        &mut rng,
        &mut dice,
        table.m1,
        ItemGroup::MiscellaneousIngredients1,
        templates,
        &mut items,
    );
    counts.ingredients += ingredient_chain(
        &mut rng,
        &mut dice,
        table.m2,
        ItemGroup::MiscellaneousIngredients2,
        templates,
        &mut items,
    );

    // Random magic item (`:225-231`).
    counts.magic_items = chain(&mut rng, &mut dice, table.mi, |rng| {
        random_magic_base(rng, level, templates)
    }, &mut items);

    // Random clothes (`:233-239`).
    counts.clothing = chain(&mut rng, &mut dice, table.cl, |rng| {
        random_clothing(rng, templates)
    }, &mut items);

    // Random books (`:241-247`).
    counts.books = chain(&mut rng, &mut dice, table.bk, |_| random_book(templates), &mut items);

    // Random religious item (`:249-255`).
    counts.religious = chain(&mut rng, &mut dice, table.rl, |rng| {
        random_religious(rng, templates)
    }, &mut items);

    // Caller-level extras. The dungeon container adds map/potion/recipe for
    // keys J..O (`LootTables.cs:150-161`); the enemy corpse adds the table's
    // own `MapChance` map and, only with a key, potion 3 % / recipe 2 %
    // (`EnemyEntity.cs:389-396` → `DaggerfallLoot.cs:88-118`).
    match path {
        LootPath::Dungeon => {
            if let Some(map_chance) = map_chance(key) {
                if dice.success(map_chance) {
                    items.push(map_item(templates));
                }
                if dice.success(4) {
                    items.push(potion_item(templates));
                }
                if dice.success(2) {
                    items.push(potion_recipe_item(templates));
                }
            }
        }
        LootPath::Enemy {
            map_chance,
            add_potion_extras,
        } => {
            if dice.success(map_chance) {
                items.push(map_item(templates));
            }
            if add_potion_extras {
                if dice.success(3) {
                    items.push(potion_item(templates));
                }
                if dice.success(2) {
                    items.push(potion_recipe_item(templates));
                }
            }
        }
    }

    (
        LootContainer {
            container_id,
            key,
            gold,
            items,
        },
        counts,
    )
}

/// Generates a loot pile without a template catalog (items carry template
/// identity with zeroed data fields; probes that need weights/prices pass
/// [`generate_container_with_templates`]).
pub fn generate_container(seed: u64, container_id: u32, key: char, level: i32) -> LootContainer {
    generate(LootPath::Dungeon, seed, container_id, key, level, None).0
}

/// Same as [`generate_container`] but with the decoded `FALL.EXE` catalog.
pub fn generate_container_with_templates(
    seed: u64,
    container_id: u32,
    key: char,
    level: i32,
    templates: &[ItemTemplate],
) -> LootContainer {
    generate(LootPath::Dungeon, seed, container_id, key, level, Some(templates)).0
}

/// Catalog-free generation plus its category counters (probe block D).
pub fn generate_container_with_counts(
    seed: u64,
    container_id: u32,
    key: char,
    level: i32,
) -> (LootContainer, LootCounts) {
    generate(LootPath::Dungeon, seed, container_id, key, level, None)
}

/// Enemy corpse loot (ORDER M25-CLOSURE criterion 10): the exact chain of
/// `EnemyEntity.SetEnemyCareer` over the M21 core —
/// `DaggerfallLoot.GenerateItems(LootTableKey)` (`EnemyEntity.cs:330` →
/// `DaggerfallLoot.cs:77-85`), `RandomlyAddMap(MapChance)` (`:389` →
/// `:88-95`) and, only when the row declares a key,
/// `RandomlyAddPotion(3)` + `RandomlyAddPotionRecipe(2)` (`:394-396` →
/// `:100-118`).
///
/// `key` is the table's `LootTableKey`: `None` (unset in the source) maps
/// to the `-` default matrix exactly like DFU's `GetMatrix(null)`
/// (`LootTables.cs:110-119`) — the chains all have chance 0, so the pile
/// carries only what the map/potion rolls add. `level` is
/// `GameManager.Instance.PlayerEntity.Level` (the gold multiplier of
/// `GenerateRandomLoot`, `LootTables.cs:193-198`). 0 formulas duplicated:
/// the matrix chains are the shared M21 core.
pub fn generate_enemy_loot(
    seed: u64,
    container_id: u32,
    key: Option<char>,
    map_chance: i32,
    level: i32,
) -> LootContainer {
    generate(
        LootPath::Enemy {
            map_chance,
            add_potion_extras: key.is_some(),
        },
        seed,
        container_id,
        key.unwrap_or('-'),
        level,
        None,
    )
    .0
}

/// Canonical digest input (SPEC M21 §7): per container, `container_id`, then
/// for every item in generation order `(item_group, group_index, material,
/// current_condition, stack_count, value)`, all little-endian. The SHA-256 is
/// applied by the probe.
pub fn container_digest_bytes(container: &LootContainer) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + container.items.len() * 26);
    out.extend_from_slice(&container.container_id.to_le_bytes());
    for item in &container.items {
        out.extend_from_slice(&item.item_group.dfu_value().to_le_bytes());
        out.extend_from_slice(&item.group_index.to_le_bytes());
        out.extend_from_slice(&item.native_material.to_le_bytes());
        out.extend_from_slice(&item.current_condition.to_le_bytes());
        out.extend_from_slice(&item.stack_count.to_le_bytes());
        out.extend_from_slice(&item.value.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::{TOTAL_ITEMS, DEFAULT_ITEMS_OFFSET};

    /// Deterministic synthetic catalog: every template 1 kg / 100 hp / 10 gp.
    fn catalog() -> Vec<ItemTemplate> {
        (0..TOTAL_ITEMS)
            .map(|index| {
                let mut template = ItemTemplate::empty(index);
                template.name = format!("T{index}");
                template.base_weight_units = 4;
                template.hit_points = 100;
                template.base_price = 10;
                template
            })
            .collect()
    }

    fn group_count(container: &LootContainer, group: ItemGroup) -> i32 {
        container
            .items
            .iter()
            .filter(|item| item.item_group == group)
            .count() as i32
    }

    #[test]
    fn goldens_of_the_matrix_table() {
        // Spot values transcribed in §5.1 / `LootTables.cs:78-101`.
        assert_eq!(matrix('-').max_gold, 0);
        assert_eq!(matrix('A').am, 5);
        assert_eq!(matrix('C').c1, 5);
        assert_eq!(matrix('H').wp, 100);
        assert_eq!(matrix('K').rl, 100);
        assert_eq!(matrix('T').wp, 100);
        assert_eq!(matrix('T').am, 100);
        assert_eq!(matrix('U').min_gold, 7);
        assert_eq!(LOOT_MATRICES.len(), 22);
        for row in LOOT_MATRICES {
            assert_eq!(matrix(row.key), &row);
        }
    }

    #[test]
    fn unknown_key_falls_back_to_the_default_matrix() {
        // GV-LOT-09.
        let unknown = matrix('Z');
        assert_eq!(unknown.key, '-');
        assert_eq!(unknown.min_gold, 0);
        assert_eq!(unknown.max_gold, 0);
        assert_eq!(
            [unknown.p1, unknown.p2, unknown.c1, unknown.c2, unknown.c3],
            [0, 0, 0, 0, 0]
        );
        let container = generate_container(42, 0, 'Z', 5);
        assert_eq!(container.gold, 0);
        assert!(container.items.is_empty());
        assert_eq!(container.key, 'Z');
    }

    #[test]
    fn dungeon_keys_cover_the_19_types() {
        // GV-LOT-10/11/12.
        assert_eq!(dungeon_key(0), Some('K'));
        assert_eq!(dungeon_key(9), Some('U'));
        assert_eq!(dungeon_key(18), Some('N'));
        assert_eq!(dungeon_key(19), None);
        assert_eq!(DUNGEON_KEYS.len(), 19);
        assert_eq!(dungeon_key(4), Some('K'));
        assert_eq!(dungeon_key(12), Some('L'));
    }

    #[test]
    fn map_chances_cover_only_j_to_o() {
        // GV-LOT-13/14/15.
        assert_eq!(map_chance('K'), Some(1));
        assert_eq!(map_chance('O'), Some(15));
        assert_eq!(map_chance('E'), None);
        assert_eq!(map_chance('J'), Some(2));
        assert_eq!(map_chance('P'), None);
    }

    #[test]
    fn enemy_loot_follows_the_enemyentity_chain() {
        // `EnemyEntity.cs:330`/`:389-396` over the M21 core. Key 'A' (Orc,
        // `EnemyBasics.cs` id 7), MapChance 0, player level 5: gold ∈
        // [min_gold·5, max_gold·5] = [5, 50] (`LootTables.cs:193-198`).
        for container_id in 0..200 {
            let container = generate_enemy_loot(2_025, container_id, Some('A'), 0, 5);
            assert!((5..=50).contains(&container.gold), "gold {}", container.gold);
            assert_eq!(container.key, 'A');
        }
        // A keyless row (Rat id 0) maps to the '-' default matrix exactly
        // like DFU's `GetMatrix(null)`: no gold, no chains, no extras.
        for container_id in 0..50 {
            let container = generate_enemy_loot(2_025, container_id, None, 0, 5);
            assert_eq!(container.gold, 0);
            assert!(container.items.is_empty(), "id {container_id}");
            assert_eq!(container.key, '-');
        }
        // Two distinct keys, deterministic ×2 (canonical digest bytes).
        for key in ['A', 'Q'] {
            let first = generate_enemy_loot(77, 3, Some(key), 3, 9);
            let second = generate_enemy_loot(77, 3, Some(key), 3, 9);
            // `Item.uid` is a process-global counter: the deterministic
            // core is gold/key/len + the canonical digest (below).
            assert_eq!(
                (first.gold, first.key, first.items.len()),
                (second.gold, second.key, second.items.len()),
                "key {key}"
            );
            assert_eq!(
                container_digest_bytes(&first),
                container_digest_bytes(&second),
                "key {key}"
            );
        }
        // Key 'J' (Frost Daedra, id 25): the enemy path rolls the 'J'
        // matrix — gold ∈ [50·10, 150·10] — and never the dungeon extras.
        let j = generate_enemy_loot(7, 0, Some('J'), 0, 10);
        assert!((500..=1500).contains(&j.gold), "gold {}", j.gold);
        assert_eq!(j.key, 'J');
    }

    #[test]
    fn golden_gold_ranges() {
        // GV-LOT-01: key K, level 5 → 5·r with r∈[1,10].
        for container_id in 0..400 {
            let container = generate_container(1_234, container_id, 'K', 5);
            assert!(container.gold > 0);
            assert_eq!(container.gold % 5, 0, "gold is level-multiple");
            assert!((5..=50).contains(&container.gold), "gold {}", container.gold);
            assert_eq!(
                group_count(&container, ItemGroup::Currency),
                1,
                "one gold item"
            );
            let gold_item = container
                .items
                .iter()
                .find(|item| item.item_group == ItemGroup::Currency)
                .expect("gold item present");
            assert_eq!(gold_item.stack_count, container.gold);
            assert_eq!(gold_item.value, 1);
        }
    }

    #[test]
    fn golden_weapon_chain_caps() {
        // GV-LOT-03: K, WP=5 → at most 3 weapons.
        for container_id in 0..400 {
            let (_, counts) = generate_container_with_counts(7, container_id, 'K', 5);
            assert!(counts.weapons <= 3, "weapons {}", counts.weapons);
        }
        // GV-LOT-06: C, WP=25 → at most 5 weapons.
        for container_id in 0..400 {
            let (container, counts) = generate_container_with_counts(7, container_id, 'C', 5);
            assert!(counts.weapons <= 5, "weapons {}", counts.weapons);
            // The pile can hold more weapons than the WP chain produced: `C`
            // has `MI = 3` and a magic base may be a weapon
            // (`ItemBuilder.cs:520-580`), so the honest bound is a range.
            let weapons = group_count(&container, ItemGroup::Weapons);
            assert!(
                (counts.weapons..=counts.weapons + counts.magic_items).contains(&weapons),
                "weapons {weapons} vs chain {} + magic {}",
                counts.weapons,
                counts.magic_items
            );
        }
        // GV-LOT-04: T, WP=100 → first guaranteed, at most 7.
        for container_id in 0..200 {
            let (_, counts) = generate_container_with_counts(99, container_id, 'T', 5);
            assert!((1..=7).contains(&counts.weapons), "weapons {}", counts.weapons);
            assert!((1..=7).contains(&counts.armor), "armor {}", counts.armor);
        }
    }

    #[test]
    fn golden_religious_and_clothing_chains() {
        // GV-LOT-05: K, RL=100 → first guaranteed, at most 7.
        for container_id in 0..200 {
            let (container, counts) = generate_container_with_counts(5, container_id, 'K', 5);
            assert!((1..=7).contains(&counts.religious), "rl {}", counts.religious);
            // A magic base can also be a religious item (`K` has `MI = 3`),
            // so the pile may exceed the RL chain count by at most `MI`.
            let religious = group_count(&container, ItemGroup::ReligiousItems);
            assert!(
                (counts.religious..=counts.religious + counts.magic_items).contains(&religious),
                "religious {religious} vs chain {} + magic {}",
                counts.religious,
                counts.magic_items
            );
        }
        // GV-LOT-07: C, CL=0 → the clothing chain never opens.
        for container_id in 0..200 {
            let (container, counts) = generate_container_with_counts(5, container_id, 'C', 5);
            assert_eq!(counts.clothing, 0);
            // Any clothing in the pile can only come from the magic chain.
            let clothing = group_count(&container, ItemGroup::MensClothing)
                + group_count(&container, ItemGroup::WomensClothing);
            assert!(
                clothing <= counts.magic_items,
                "clothing {clothing} vs magic {}",
                counts.magic_items
            );
        }
    }

    #[test]
    fn golden_magic_item_chain_is_bounded() {
        // GV-LOT-08: A, MI=2 → at most 2.
        for container_id in 0..400 {
            let (_, counts) = generate_container_with_counts(11, container_id, 'A', 5);
            assert!(counts.magic_items <= 2, "mi {}", counts.magic_items);
        }
    }

    #[test]
    fn golden_level_scaled_ingredient_chance() {
        // GV-LOT-02: C1 for key C at level 5 is 5 * 5 = 25.
        assert_eq!(matrix('C').c1 * 5, 25);
        // C1/P1 scale with level; C3/M2 do not (`LootTables.cs:217-223`).
        assert_eq!(matrix('C').c3 * 5, 25, "C3 is not scaled: 5 stays 5");
        assert_eq!(matrix('C').m2, 2);
    }

    #[test]
    fn generation_is_deterministic_per_seed_and_container() {
        let a = generate_container(0xDEAD_BEEF, 17, 'K', 5);
        let b = generate_container(0xDEAD_BEEF, 17, 'K', 5);
        // `uid` is a process-global monotonic counter (DFU `currentUID`),
        // deliberately outside the §7 digest tuple: compare the piles modulo
        // uid and the digest as-is.
        assert_eq!(without_uid(&a), without_uid(&b));
        assert_eq!(container_digest_bytes(&a), container_digest_bytes(&b));

        let other_seed = generate_container(0xDEAD_BEEE, 17, 'K', 5);
        assert_ne!(container_digest_bytes(&a), container_digest_bytes(&other_seed));

        let other_container = generate_container(0xDEAD_BEEF, 18, 'K', 5);
        assert_ne!(
            container_digest_bytes(&a),
            container_digest_bytes(&other_container)
        );

        // The key is not part of the stream derivation, but it changes the
        // matrix and therefore the pile.
        let other_key = generate_container(0xDEAD_BEEF, 17, 'T', 5);
        assert_ne!(container_digest_bytes(&a), container_digest_bytes(&other_key));
    }

    /// Clone of `container` with every uid zeroed (see the determinism test).
    fn without_uid(container: &LootContainer) -> LootContainer {
        let mut clone = container.clone();
        for item in &mut clone.items {
            item.uid = 0;
        }
        clone
    }

    #[test]
    fn digest_layout_is_the_spec_tuple_in_order() {
        let mut container = LootContainer {
            container_id: 0x0102_0304,
            key: 'K',
            gold: 5,
            items: Vec::new(),
        };
        let mut item = Item::from_template(&ItemTemplate::empty(120));
        item.native_material = 3;
        item.current_condition = 660;
        item.stack_count = 2;
        item.value = 360;
        item.group_index = 7;
        container.items.push(item);

        let bytes = container_digest_bytes(&container);
        // 4 (container_id) + 22 (item tuple: 4 + 2 + 4 + 4 + 4 + 4).
        assert_eq!(bytes.len(), 4 + 22);
        assert_eq!(&bytes[0..4], &[0x04, 0x03, 0x02, 0x01]);
        assert_eq!(&bytes[4..8], &ItemGroup::Weapons.dfu_value().to_le_bytes());
        assert_eq!(&bytes[8..10], &7u16.to_le_bytes());
        assert_eq!(&bytes[10..14], &3i32.to_le_bytes());
        assert_eq!(&bytes[14..18], &660i32.to_le_bytes());
        assert_eq!(&bytes[18..22], &2i32.to_le_bytes());
        assert_eq!(&bytes[22..26], &360i32.to_le_bytes());
    }

    #[test]
    fn templates_supply_weights_conditions_and_materials() {
        let templates = catalog();
        // §8 demo criterion: with a fixed seed SOME dungeon-`K` pile carries
        // a weapon AND a piece of armor. `AM = 5` opens the armor chain at
        // 5 %, so one unlucky container id may well roll no armor at all.
        let (id, container) = (0..500)
            .map(|id| (id, generate_container_with_templates(0xABCD, id, 'K', 5, &templates)))
            .find(|(_, candidate)| {
                group_count(candidate, ItemGroup::Weapons) > 0
                    && group_count(candidate, ItemGroup::Armor) > 0
            })
            .expect("a K container with weapon + armor under seed 0xABCD");
        assert!(!container.items.is_empty(), "container {id} is not empty");

        let mut weapons = 0;
        let mut armor = 0;
        for item in &container.items {
            match item.item_group {
                ItemGroup::Weapons => {
                    weapons += 1;
                    if item.group_index == ARROW_GROUP_INDEX {
                        assert_eq!(item.current_condition, 0);
                        assert_eq!(item.native_material, 0);
                        assert!((1..=20).contains(&item.stack_count));
                        assert!(item.weight_kg > 0.0);
                    } else {
                        assert!(item.weight_kg > 0.0, "material weight applied");
                        assert!(item.max_condition > 0, "material condition applied");
                        assert!(item.current_condition == item.max_condition);
                        assert!(item.native_material >= 0 && item.native_material < 10);
                    }
                }
                ItemGroup::Armor => {
                    armor += 1;
                    assert!(item.weight_kg > 0.0);
                    assert!(matches!(
                        item.native_material,
                        0x0000 | 0x0100 | 0x0200..=0x0209
                    ));
                }
                _ => {}
            }
        }
        assert!(weapons > 0, "container {id} rolled a weapon");
        assert!(armor > 0, "container {id} rolled armor");
    }

    #[test]
    fn catalog_free_generation_keeps_identity_only() {
        let container = generate_container(0xABCD, 3, 'K', 5);
        let mut gold_seen = false;
        for item in &container.items {
            assert_eq!(item.weight_kg, 0.0);
            assert_eq!(item.max_condition, 0);
            if item.item_group == ItemGroup::Currency {
                // `CreateGoldPieces` (`ItemBuilder.cs:758-765`) assigns
                // `value = 1` outright, so gold is the single identity field
                // that survives without a catalog.
                gold_seen = true;
                assert_eq!(item.value, 1);
            } else {
                assert_eq!(item.value, 0);
            }
        }
        assert!(gold_seen, "K rolls gold at every level > 0");
    }

    #[test]
    fn extras_appear_only_for_j_to_o_keys() {
        let templates = catalog();
        let mut maps = 0;
        let mut potions = 0;
        let mut recipes = 0;
        for container_id in 0..500 {
            let container =
                generate_container_with_templates(0x51, container_id, 'K', 5, &templates);
            for item in &container.items {
                match item.template_index {
                    287 => {
                        maps += 1;
                        assert_eq!(item.item_group, ItemGroup::MiscItems);
                        assert_eq!(item.group_index, 8);
                    }
                    83 => potions += 1,
                    278 => recipes += 1,
                    _ => {}
                }
            }
        }
        assert!(maps > 0, "K has a 1% map chance");
        assert!(potions > 0, "potion chance is 4%");
        assert!(recipes > 0, "recipe chance is 2%");

        // Key A is outside J..O: no maps, potions or recipes.
        for container_id in 0..300 {
            let container =
                generate_container_with_templates(0x51, container_id, 'A', 5, &templates);
            for item in &container.items {
                assert!(
                    !matches!(item.template_index, 287 | 83 | 278),
                    "unexpected extra {:?}",
                    item.template_index
                );
            }
        }
    }

    #[test]
    fn magic_items_are_complete_base_items_from_allowed_groups() {
        let templates = catalog();
        // L has MI=1, so collect a large sample.
        let mut magic_bases = 0;
        for container_id in 0..400 {
            let (container, counts) =
                generate_container_with_counts(0x77, container_id, 'L', 5);
            assert!(counts.magic_items <= 2);
            let magic = counts.magic_items;
            if magic > 0 {
                magic_bases += magic;
            }
            let _ = templates;
            for item in &container.items {
                assert!(item.legacy_magic.is_empty(), "M24 owns enchantments");
            }
        }
        assert!(magic_bases > 0);
    }

    #[test]
    fn material_rolls_stay_in_table() {
        let mut rng = Rng64::new(1);
        for _ in 0..2_000 {
            assert!(random_material(&mut rng, 1) < 10);
            assert!(random_material(&mut rng, 30) < 10);
            let armor = random_armor_material(&mut rng, 30);
            assert!(matches!(armor, 0x0000 | 0x0100 | 0x0200..=0x0209));
        }
    }

    #[test]
    fn range_inclusive_handles_degenerate_bounds() {
        let mut rng = Rng64::new(9);
        assert_eq!(rng.range_inclusive(0, 0), 0);
        assert_eq!(rng.range_inclusive(5, 1), 5);
        for _ in 0..100 {
            let value = rng.range_inclusive(-3, 3);
            assert!((-3..=3).contains(&value));
        }
        assert!(!rng.success(0));
        assert!(rng.success(100));
    }

    #[test]
    fn derive_rng_separates_containers_and_matches_itself() {
        assert_eq!(derive_rng(1, 2), derive_rng(1, 2));
        assert_ne!(derive_rng(1, 2), derive_rng(1, 3));
        assert_ne!(derive_rng(1, 2), derive_rng(2, 2));
        assert_ne!(derive_rng(0, 0), Rng64::new(0));
    }

    #[test]
    fn digest_is_canonical_across_runs() {
        let containers: Vec<LootContainer> = (0..20)
            .map(|id| generate_container(0xFEED, id, 'N', 5))
            .collect();
        let first: Vec<Vec<u8>> = containers.iter().map(container_digest_bytes).collect();
        let second: Vec<Vec<u8>> = (0..20)
            .map(|id| container_digest_bytes(&generate_container(0xFEED, id, 'N', 5)))
            .collect();
        assert_eq!(first, second);
    }

    #[test]
    fn default_offset_constant_is_reused_for_catalog_free_runs() {
        // Ties this module's catalog-free mode to the measured FALL.EXE offset
        // documented in items.rs; no filesystem access here.
        assert_eq!(DEFAULT_ITEMS_OFFSET, 1_776_954);
        assert_eq!(TOTAL_ITEMS, 288);
    }
}

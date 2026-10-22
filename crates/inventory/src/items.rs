//! Item templates, instances, materials and derived stats — SPEC M21 §1-2.
//!
//! Item templates are **not** a data file: they live inside `FALL.EXE` as
//! 288 records of 48 bytes (`ItemsFile.cs:176-180`, `:425-452`, MIT
//! `@0038a1e6`). This module receives the raw bytes as `&[u8]` — it never
//! touches the filesystem (0 `std::fs`), so retail bytes never enter the repo.
//!
//! Layout of one 48-byte record (little-endian, §1.2):
//!
//! | off | size | field |
//! |----:|-----:|:------|
//! | 0 | 24 | NUL-padded ASCII name |
//! | 24 | 4 | `baseWeightUnits` (i32, 0.25 kg each) |
//! | 28 | 2 | `hitPoints` (u16) |
//! | 30 | 4 | `capacityOrTarget` (i32) |
//! | 34 | 4 | `basePrice` (i32) |
//! | 38 | 2 | `enchantmentPoints` (i16) |
//! | 40 | 1 | `rarity` |
//! | 41 | 1 | `variants` |
//! | 42 | 1 | `drawOrderOrEffect` |
//! | 43 | 1 | `propertiesBitfield` |
//! | 44 | 2 | `worldTextureBitfield` |
//! | 46 | 2 | `playerTextureBitfield` |
//!
//! Offset discovery (`ItemsFile.cs:425-433`): use `defaultItemsOffset`
//! (1776954) only when the four bytes there already read `Ruby`; otherwise the
//! first occurrence of `Ruby` in the file wins. In the measured retail
//! `FALL.EXE` the default is `0x40 '@'` and the live offset is 1796138, so the
//! fallback path is the one that matters (R-21.1).
//!
//! Frontiers: enchantment effects are M24 (only `legacy_magic` storage here);
//! combat consumes [`weapon_material_modifier`], [`material_armor_value`] and
//! [`weapon_skill`] but dice/attack resolution is M22; `ApplyRegionalPriceAdjustment`
//! is M30.

use std::fmt;

pub use daggerfall_behavior::locomotion::max_encumbrance;

/// Expected `FALL.EXE` file name (`ItemsFile.cs:176`).
pub const FALL_EXE_FILENAME: &str = "FALL.EXE";

/// Sentinel of the first item record (`ItemsFile.cs:175`).
pub const RUBY_SENTINEL: &[u8] = b"Ruby";

/// Offset reported by DFU for the retail `FALL.EXE` (`ItemsFile.cs:177`).
pub const DEFAULT_ITEMS_OFFSET: usize = 1_776_954;

/// Bytes of the NUL-padded name field (`ItemsFile.cs:178`).
pub const ITEM_NAME_LENGTH: usize = 24;

/// Templates in the classic item table (`ItemsFile.cs:180`).
pub const TOTAL_ITEMS: usize = 288;

/// Encoded size of one template record (sum of §1.2 fields).
pub const ITEM_RECORD_SIZE: usize = 48;

/// `identifiedMask` (`DaggerfallUnityItem.cs:96-97`).
pub const FLAG_IDENTIFIED: u16 = 0x20;

/// `artifactMask` (`DaggerfallUnityItem.cs:96-97`).
pub const FLAG_ARTIFACT: u16 = 0x800;

/// Weight multipliers, Iron..=Daedric (`ItemBuilder.cs:45`).
pub const WEIGHT_MUL: [i32; 10] = [4, 5, 4, 4, 3, 4, 4, 2, 4, 5];

/// Value multipliers, Iron..=Daedric (`ItemBuilder.cs:48`).
pub const VALUE_MUL: [i32; 10] = [1, 2, 4, 8, 16, 32, 64, 128, 256, 512];

/// Condition multipliers, Iron..=Daedric (`ItemBuilder.cs:51`).
pub const COND_MUL: [i32; 10] = [4, 6, 6, 8, 12, 16, 20, 24, 28, 32];

/// To-hit bonus per material (`GetWeaponMaterialModifier() * 10`,
/// `FormulaHelper.cs:1140-1146`).
pub const TO_HIT_MOD: [i32; 10] = [-10, 0, 0, 10, 20, 30, 30, 40, 50, 60];

/// Damage modifier per material (`DaggerfallUnityItem.cs:979-1005`).
pub const DAMAGE_MOD: [i32; 10] = [-1, 0, 0, 1, 2, 3, 3, 4, 5, 6];

/// A template record decoded from `FALL.EXE` (SPEC M21 §1.2).
#[derive(Debug, Clone, PartialEq)]
pub struct ItemTemplate {
    /// Position in the 288-record table.
    pub index: usize,
    /// `TrimEnd('\0')` of the ASCII name field.
    pub name: String,
    /// `baseWeightUnits` — kilograms are `units * 0.25` (`ItemsFile.cs:332`).
    pub base_weight_units: i32,
    /// `hitPoints` — base maximum condition.
    pub hit_points: u16,
    /// Container capacity / effect target.
    pub capacity_or_target: i32,
    /// `basePrice` before material.
    pub base_price: i32,
    /// `enchantmentPoints` before material (M24 consumes it).
    pub enchantment_points: i16,
    /// Minimum building quality.
    pub rarity: u8,
    /// Number of visual variants.
    pub variants: u8,
    /// Paper-doll order or ingredient effect.
    pub draw_order_or_effect: u8,
    /// Property bitfield (§1.3).
    pub properties: u8,
    /// World texture bitfield: archive `>> 7`, record `& 0x7f`.
    pub world_texture: i16,
    /// Player texture bitfield: archive `>> 7`, record `& 0x7f`.
    pub player_texture: i16,
}

impl ItemTemplate {
    /// An all-zero template with the given index.
    ///
    /// DFU returns a default `new ItemTemplate()` for an out-of-range index
    /// (`ItemHelper.cs:215-220`); used only when the caller supplies no
    /// `FALL.EXE` catalog.
    pub fn empty(index: usize) -> Self {
        Self {
            index,
            name: String::new(),
            base_weight_units: 0,
            hit_points: 0,
            capacity_or_target: 0,
            base_price: 0,
            enchantment_points: 0,
            rarity: 0,
            variants: 0,
            draw_order_or_effect: 0,
            properties: 0,
            world_texture: 0,
            player_texture: 0,
        }
    }

    /// Kilograms (`ItemsFile.cs:330`: `baseWeightUnits * 0.25f`).
    pub fn base_weight_kg(&self) -> f32 {
        self.base_weight_units as f32 * 0.25
    }

    /// Bit 0 (`ItemsFile.cs:343`).
    pub fn is_ingredient(&self) -> bool {
        self.properties & 1 == 1
    }

    /// Bit 2 (`ItemsFile.cs:342`).
    pub fn is_one_handed(&self) -> bool {
        (self.properties >> 2) & 1 == 1
    }

    /// Bit 3 (`ItemsFile.cs:341`).
    pub fn is_liquid(&self) -> bool {
        (self.properties >> 3) & 1 == 1
    }

    /// Bit 4 (`ItemsFile.cs:340`).
    pub fn is_blunt_weapon(&self) -> bool {
        (self.properties >> 4) & 1 == 1
    }

    /// World texture archive (`ItemsFile.cs:345`: `bitfield >> 7`).
    pub fn world_texture_file(&self) -> i16 {
        self.world_texture >> 7
    }

    /// World texture record (`ItemsFile.cs:346`: `bitfield & 0x7f`).
    pub fn world_texture_record(&self) -> i16 {
        self.world_texture & 0x7f
    }

    /// Player texture archive (`ItemsFile.cs:347`).
    pub fn player_texture_file(&self) -> i16 {
        self.player_texture >> 7
    }

    /// Player texture record (`ItemsFile.cs:348`).
    pub fn player_texture_record(&self) -> i16 {
        self.player_texture & 0x7f
    }

    /// `hasNoEncumbrance` (§2.5). DFU documents the flag in
    /// `DaggerfallUnityItem.cs:665-672` but never assigns it at this commit
    /// (the comment names horses, carts, arrows and maps); SPEC M21 §2.5 fixes
    /// the intended set, which is exactly templates 93..=98 (Transportation),
    /// 131 (Arrow) and 287 (Map).
    pub fn has_no_encumbrance(&self) -> bool {
        has_no_encumbrance_index(self.index)
    }
}

/// Template-level no-encumbrance rule (see [`ItemTemplate::has_no_encumbrance`]).
fn has_no_encumbrance_index(index: usize) -> bool {
    matches!(index, 93..=98 | 131 | 287)
}

/// First occurrence of `needle` in `data` (`FileProxy.FindString`).
pub fn find_string(data: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    data.windows(needle.len()).position(|window| window == needle)
}

/// Offset of the item table (`ItemsFile.cs:425-433`): the default offset when
/// it already reads `Ruby`, otherwise the first `Ruby` in the file.
pub fn find_items_offset(data: &[u8]) -> Option<usize> {
    let end = DEFAULT_ITEMS_OFFSET.checked_add(RUBY_SENTINEL.len())?;
    if data.get(DEFAULT_ITEMS_OFFSET..end) == Some(RUBY_SENTINEL) {
        return Some(DEFAULT_ITEMS_OFFSET);
    }
    find_string(data, RUBY_SENTINEL)
}

/// Failure modes of [`parse_item_templates`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemError {
    /// No `Ruby` sentinel anywhere in the blob.
    SentinelNotFound,
    /// The table does not fit in the supplied bytes.
    TruncatedTable {
        /// Discovered table offset.
        offset: usize,
        /// Required end offset (`offset + 288 * 48`).
        needed: usize,
        /// Bytes actually supplied.
        available: usize,
    },
}

impl fmt::Display for ItemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ItemError::SentinelNotFound => {
                write!(f, "item table: \"Ruby\" sentinel not found")
            }
            ItemError::TruncatedTable {
                offset,
                needed,
                available,
            } => write!(
                f,
                "item table truncated at offset {offset}: need {needed} bytes, have {available}"
            ),
        }
    }
}

impl std::error::Error for ItemError {}

/// Decodes the 288 templates of a `FALL.EXE` image (SPEC M21 §1.2).
pub fn parse_item_templates(data: &[u8]) -> Result<Vec<ItemTemplate>, ItemError> {
    let offset = find_items_offset(data).ok_or(ItemError::SentinelNotFound)?;
    let needed = offset + TOTAL_ITEMS * ITEM_RECORD_SIZE;
    if data.len() < needed {
        return Err(ItemError::TruncatedTable {
            offset,
            needed,
            available: data.len(),
        });
    }
    let mut templates = Vec::with_capacity(TOTAL_ITEMS);
    for index in 0..TOTAL_ITEMS {
        let start = offset + index * ITEM_RECORD_SIZE;
        templates.push(parse_record(&data[start..start + ITEM_RECORD_SIZE], index));
    }
    Ok(templates)
}

fn parse_record(record: &[u8], index: usize) -> ItemTemplate {
    let name_field = &record[..ITEM_NAME_LENGTH];
    let name_end = name_field
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(ITEM_NAME_LENGTH);
    ItemTemplate {
        index,
        name: String::from_utf8_lossy(&name_field[..name_end]).into_owned(),
        base_weight_units: le_i32(record, 24),
        hit_points: u16::from_le_bytes(
            record[28..30].try_into().expect("u16 field fits record"),
        ),
        capacity_or_target: le_i32(record, 30),
        base_price: le_i32(record, 34),
        enchantment_points: i16::from_le_bytes(
            record[38..40].try_into().expect("i16 field fits record"),
        ),
        rarity: record[40],
        variants: record[41],
        draw_order_or_effect: record[42],
        properties: record[43],
        world_texture: i16::from_le_bytes(
            record[44..46].try_into().expect("i16 field fits record"),
        ),
        player_texture: i16::from_le_bytes(
            record[46..48].try_into().expect("i16 field fits record"),
        ),
    }
}

fn le_i32(record: &[u8], at: usize) -> i32 {
    i32::from_le_bytes(record[at..at + 4].try_into().expect("i32 field fits record"))
}

/// Classic item group (`ItemEnums.cs:27-58`). `None = -1`; the remaining 28
/// values are the DFU enum values 0..=28, and [`GROUP_COUNT`] counts them all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum ItemGroup {
    /// `ItemGroups.None`.
    None = -1,
    /// 0 — potions/drugs.
    Drugs = 0,
    /// 1 — jars, sacks, chests, quiver, backpack.
    UselessItems1 = 1,
    /// 2 — armor templates 102..=112.
    Armor = 2,
    /// 3 — weapon templates 113..=131.
    Weapons = 3,
    /// 4 — virtual group fed by `MAGIC.DEF` (M24).
    MagicItems = 4,
    /// 5 — virtual group fed by `MAGIC.DEF` artifact templates.
    Artifacts = 5,
    /// 6 — templates 141..=181.
    MensClothing = 6,
    /// 7 — template 277 (four aliases in the enum).
    Books = 7,
    /// 8 — templates 217..=245.
    Furniture = 8,
    /// 9 — torch, lantern, bandage, oil, candle, parchment.
    UselessItems2 = 9,
    /// 10 — templates 258..=265 and 267..=271.
    ReligiousItems = 10,
    /// 11 — template 287.
    Maps = 11,
    /// 12 — templates 182..=216.
    WomensClothing = 12,
    /// 13 — template 284.
    Paintings = 13,
    /// 14 — templates 0..=7.
    Gems = 14,
    /// 15 — plant ingredients (northern set).
    PlantIngredients1 = 15,
    /// 16 — plant ingredients (southern set).
    PlantIngredients2 = 16,
    /// 17 — creature ingredients.
    CreatureIngredients1 = 17,
    /// 18 — creature ingredients.
    CreatureIngredients2 = 18,
    /// 19 — creature ingredients.
    CreatureIngredients3 = 19,
    /// 20 — miscellaneous ingredients.
    MiscellaneousIngredients1 = 20,
    /// 21 — metal ingredients.
    MetalIngredients = 21,
    /// 22 — ivory, pearl.
    MiscellaneousIngredients2 = 22,
    /// 23 — cart, horse, boats (templates 93..=98).
    Transportation = 23,
    /// 24 — deeds, templates 99..=101 (see [`group_indices`] correction note).
    Deeds = 24,
    /// 25 — templates 133..=140.
    Jewellery = 25,
    /// 26 — quest items.
    QuestItems = 26,
    /// 27 — spellbook, soul trap, letter of credit, recipe, deeds, map.
    MiscItems = 27,
    /// 28 — gold pieces, template 276.
    Currency = 28,
}

/// Number of `ItemGroups` values, including `None` (`ItemEnums.cs:27-58`).
pub const GROUP_COUNT: usize = 29;

impl ItemGroup {
    /// DFU enum value (`None` is `-1`).
    pub fn dfu_value(self) -> i32 {
        self as i32
    }
}

/// Template indices owned by `group`, transcribed from the group enums of
/// `ItemEnums.cs:172-620` (enum value = template index; list position =
/// DFU `groupIndex`).
///
/// **Measured corrections (declared in the lane yield):**
/// - `Deeds` is `[99, 100, 101]`, not the implicit `0, 1, 2` of
///   `ItemEnums.cs:561-565`: retail templates 99..=101 are named
///   "Deed to Townhouse/House/Manor" (`ItemTemplates.txt` @0038a1e6). The
///   implicit values are unreachable through `GetGroupIndex` and would alias
///   Ruby/Emerald/Sapphire.
/// - `MiscItems` keeps the `Unused` placeholder at position 3 (template 276)
///   because DFU positions are list indices and `CreateRandomRecipe` uses
///   `MiscItems, 4` (`ItemBuilder.cs:765`); template 276 is really gold.
///
/// `MagicItems` and `Artifacts` are virtual `MAGIC.DEF` groups and own no
/// retail template.
pub fn group_indices(group: ItemGroup) -> &'static [usize] {
    match group {
        ItemGroup::None | ItemGroup::MagicItems | ItemGroup::Artifacts => &[],
        ItemGroup::Drugs => &[78, 79, 80, 81],
        ItemGroup::UselessItems1 => &[82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92],
        ItemGroup::Armor => &[102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112],
        ItemGroup::Weapons => &[
            113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 123, 124, 125, 126, 127, 128, 129,
            130, 131,
        ],
        ItemGroup::MensClothing => &[
            141, 142, 143, 144, 145, 146, 147, 148, 149, 150, 151, 152, 153, 154, 155, 156, 157,
            158, 159, 160, 161, 162, 163, 164, 165, 166, 167, 168, 169, 170, 171, 172, 173, 174,
            175, 176, 177, 178, 179, 180, 181,
        ],
        ItemGroup::Books => &[277, 277, 277, 277],
        ItemGroup::Furniture => &[
            217, 218, 219, 220, 221, 222, 223, 224, 225, 226, 227, 228, 229, 230, 231, 232, 233,
            234, 235, 236, 237, 238, 239, 240, 241, 242, 243, 244, 245,
        ],
        ItemGroup::UselessItems2 => &[247, 248, 249, 252, 253, 279],
        ItemGroup::ReligiousItems => &[258, 259, 260, 261, 262, 263, 264, 265, 267, 268, 269, 270, 271],
        ItemGroup::Maps => &[287],
        ItemGroup::WomensClothing => &[
            182, 183, 184, 185, 186, 187, 188, 189, 190, 191, 192, 193, 194, 195, 196, 197, 198,
            199, 200, 201, 202, 203, 204, 205, 206, 207, 208, 209, 210, 211, 212, 213, 214, 215,
            216,
        ],
        ItemGroup::Paintings => &[284],
        ItemGroup::Gems => &[0, 1, 2, 3, 4, 5, 6, 7],
        ItemGroup::PlantIngredients1 => &[
            8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 23, 25,
        ],
        ItemGroup::PlantIngredients2 => &[
            8, 9, 10, 11, 12, 13, 15, 16, 17, 21, 22, 24, 26, 27, 28, 29, 30, 31, 32,
        ],
        ItemGroup::CreatureIngredients1 => &[
            33, 35, 38, 39, 40, 41, 42, 43, 44, 45, 50, 51, 53, 54, 61,
        ],
        ItemGroup::CreatureIngredients2 => &[46, 47, 48, 49, 52],
        ItemGroup::CreatureIngredients3 => &[34, 36, 37],
        ItemGroup::MiscellaneousIngredients1 => &[55, 56, 57, 58, 59, 60, 62, 63, 64],
        ItemGroup::MetalIngredients => &[65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75],
        ItemGroup::MiscellaneousIngredients2 => &[76, 77],
        ItemGroup::Transportation => &[93, 94, 95, 96, 97, 98],
        ItemGroup::Deeds => &[99, 100, 101],
        ItemGroup::Jewellery => &[133, 134, 135, 136, 137, 138, 139, 140],
        ItemGroup::QuestItems => &[254, 255, 256, 257, 280, 281, 282, 283],
        ItemGroup::MiscItems => &[132, 274, 275, 276, 278, 281, 285, 286, 287],
        ItemGroup::Currency => &[276],
    }
}

/// DFU group order (`ItemEnums.cs:27-58`) used to resolve a template index to
/// its primary group. `Currency` is handled before the sweep because template
/// 276 is gold.
const DFU_GROUP_ORDER: [ItemGroup; 28] = [
    ItemGroup::Drugs,
    ItemGroup::UselessItems1,
    ItemGroup::Armor,
    ItemGroup::Weapons,
    ItemGroup::MagicItems,
    ItemGroup::Artifacts,
    ItemGroup::MensClothing,
    ItemGroup::Books,
    ItemGroup::Furniture,
    ItemGroup::UselessItems2,
    ItemGroup::ReligiousItems,
    ItemGroup::Maps,
    ItemGroup::WomensClothing,
    ItemGroup::Paintings,
    ItemGroup::Gems,
    ItemGroup::PlantIngredients1,
    ItemGroup::PlantIngredients2,
    ItemGroup::CreatureIngredients1,
    ItemGroup::CreatureIngredients2,
    ItemGroup::CreatureIngredients3,
    ItemGroup::MiscellaneousIngredients1,
    ItemGroup::MetalIngredients,
    ItemGroup::MiscellaneousIngredients2,
    ItemGroup::Transportation,
    ItemGroup::Deeds,
    ItemGroup::Jewellery,
    ItemGroup::QuestItems,
    ItemGroup::MiscItems,
];

/// Primary group of a template index.
///
/// Collisions in the DFU enums, resolved by enum order:
/// - 276 → [`ItemGroup::Currency`] (before the sweep; `MiscItems.Unused` is
///   the unreachable placeholder).
/// - 281 → [`ItemGroup::QuestItems`] (`QuestItems.Dead_body` = 26 before
///   `MiscItems.Dead_Body` = 27).
/// - 287 → [`ItemGroup::Maps`] (`Maps` = 11 before `MiscItems` = 27).
/// - 277 → [`ItemGroup::Books`] (four aliases, same index).
/// - 0..=7 → [`ItemGroup::Gems`] (Deeds corrected away from 0..=2).
///
/// Templates 246, 250, 251, 266, 272 and 273 are named decor objects
/// (Candelabra, Shrubbery, Flowers, Large Statue, Altar, Font) that no DFU
/// group enum addresses; [`ItemGroup::MiscItems`] is the declared catch-all
/// `[INFERENCE]` so the mapping stays total.
pub fn template_group(template_index: usize) -> ItemGroup {
    if template_index == 276 {
        return ItemGroup::Currency;
    }
    for group in DFU_GROUP_ORDER {
        if group_indices(group).contains(&template_index) {
            return group;
        }
    }
    ItemGroup::MiscItems
}

static NEXT_UID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn next_uid() -> u64 {
    NEXT_UID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Allocates the UID of a newly created instance (equip split, loot).
/// Process-local; durable allocation is M33.
pub(crate) fn fresh_uid() -> u64 {
    next_uid()
}

/// One item instance (SPEC M21 §2.1), built from a template exactly like
/// `DaggerfallUnityItem(group, groupIndex)` (`DaggerfallUnityItem.cs:540-583`).
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// Process-local identifier; `0` means "no item" in the equip table.
    ///
    /// DFU generates a random UID per instance; durable UID allocation and
    /// persistence are M33, so this counter is deliberately not persisted.
    pub uid: u64,
    /// Owning group.
    pub item_group: ItemGroup,
    /// DFU `groupIndex` = list position inside the group enum.
    pub group_index: u16,
    /// Template index (`ItemTemplate.index`).
    pub template_index: usize,
    /// `0` for mundane items; `0x0200 + plate` for plate armor (§2.1).
    pub native_material: i32,
    /// Weight after material adjustments.
    pub weight_kg: f32,
    /// Value after material adjustments.
    pub value: i32,
    /// Current condition (hit points of the item, not a percentage).
    pub current_condition: i32,
    /// Maximum condition.
    pub max_condition: i32,
    /// Enchantment points (M24).
    pub enchantment_points: i32,
    /// Stack size, always `>= 1`.
    pub stack_count: i32,
    /// [`FLAG_IDENTIFIED`] / [`FLAG_ARTIFACT`].
    pub flags: u16,
    /// Weapon poison type (M22 applies it).
    pub poison_type: i32,
    /// Legacy `(type, param)` enchantments (M24 applies them).
    pub legacy_magic: Vec<(i32, i32)>,
    /// Whether an equip table holds this instance.
    pub equipped: bool,
}

impl Item {
    /// Builds an instance from a template (`DaggerfallUnityItem.cs:552-583`):
    /// group/index from the template, `nativeMaterialValue = 0`, weight/value/
    /// condition/enchantment points copied, `stackCount = 1`, no flags.
    ///
    /// `group_index` is the DFU list position inside the group enum. For
    /// Weapons (113..=131) and Armor (102..=112) that equals
    /// `template_index - first`; for gapped groups (e.g. PlantIngredients1)
    /// the position is the correct DFU index. Templates no group owns (246,
    /// 250, 251, 266, 272, 273) fall back to `group_index = template_index`,
    /// mirroring DFU's custom-item convention (`ItemHelper.cs:246-249`).
    pub fn from_template(template: &ItemTemplate) -> Self {
        let item_group = template_group(template.index);
        let group_index = group_indices(item_group)
            .iter()
            .position(|&index| index == template.index)
            .map(|position| position as u16)
            .unwrap_or(template.index as u16);
        Self {
            uid: next_uid(),
            item_group,
            group_index,
            template_index: template.index,
            native_material: 0,
            weight_kg: template.base_weight_kg(),
            value: template.base_price,
            current_condition: i32::from(template.hit_points),
            max_condition: i32::from(template.hit_points),
            enchantment_points: i32::from(template.enchantment_points),
            stack_count: 1,
            flags: 0,
            poison_type: 0,
            legacy_magic: Vec::new(),
            equipped: false,
        }
    }
}

/// `ConditionPercentage` (`DaggerfallUnityItem.cs:460-463`): integer
/// percentage with truncation; `max <= 0` reports 100.
pub fn condition_percent(current: i32, max: i32) -> i32 {
    if max > 0 {
        100 * current / max
    } else {
        100
    }
}

/// `IsShield` (`DaggerfallUnityItem.cs:1804-1818`): armor templates 109..=112.
pub fn is_shield(item: &Item) -> bool {
    item.item_group == ItemGroup::Armor && matches!(item.template_index, 109..=112)
}

/// DFU `IsIngredient` (`DaggerfallUnityItem.cs:250-253`) reads
/// `ItemTemplate.isIngredient`, which retail sets for templates 0..=77 (gems +
/// every ingredient group; measured from `ItemTemplates.txt` @0038a1e6). The
/// instance does not carry the property byte, so the measured range stands in.
pub fn is_ingredient(item: &Item) -> bool {
    item.template_index <= 77
}

/// `IsPotion` (`DaggerfallUnityItem.cs:352-355`): `Glass_Bottle` (template 83)
/// in `UselessItems1`.
pub fn is_potion(item: &Item) -> bool {
    item.item_group == ItemGroup::UselessItems1 && item.template_index == 83
}

/// `IsEnchanted` (`DaggerfallUnityItem.cs:266-303`): any legacy enchantment
/// whose type is not `None` (`EnchantmentTypes.None = -1`,
/// `ItemsFile.cs:111-114`).
pub fn is_enchanted(item: &Item) -> bool {
    item.legacy_magic.iter().any(|&(kind, _)| kind != -1)
}

/// `IsStackable` (`DaggerfallUnityItem.cs:681-695` + `FormulaHelper.cs:2080-2094`):
/// equipped, quest and enchanted items never stack; ingredients, potions,
/// books, gold, arrows and oil do.
///
/// Books are included because the source includes them
/// (`FormulaHelper.cs:2087`); quest items are detected through
/// [`ItemGroup::QuestItems`] because the per-instance quest flag is M29 state.
pub fn is_stackable(item: &Item) -> bool {
    if item.equipped || item.item_group == ItemGroup::QuestItems || is_enchanted(item) {
        return false;
    }
    is_ingredient(item)
        || is_potion(item)
        || item.item_group == ItemGroup::Books
        || (item.item_group == ItemGroup::Currency && item.template_index == 276)
        || (item.item_group == ItemGroup::Weapons && item.template_index == 131)
        || (item.item_group == ItemGroup::UselessItems2 && item.template_index == 252)
}

/// `hasNoEncumbrance` for an instance (§2.5, see
/// [`ItemTemplate::has_no_encumbrance`]).
pub fn has_no_encumbrance(item: &Item) -> bool {
    has_no_encumbrance_index(item.template_index)
}

/// `EffectiveUnitWeightInKg` (`DaggerfallUnityItem.cs:665-673`): zero for
/// horses, carts, boats, arrows and maps.
pub fn effective_unit_weight_kg(item: &Item, template: &ItemTemplate) -> f32 {
    if template.has_no_encumbrance() {
        0.0
    } else {
        item.weight_kg
    }
}

/// Summed carried weight of one stack (§2.5):
/// `effective_unit_weight * stack_count`.
pub fn total_weight_kg(item: &Item, template: &ItemTemplate) -> f32 {
    effective_unit_weight_kg(item, template) * item.stack_count.max(0) as f32
}

/// `Mathf.Round` is half-to-even (`ItemBuilder.cs:653-658`): 22.5 → 22,
/// 13.5 → 14, 62.5 → 62 (R-21.2). Rust's default `round` is half-away, so the
/// tie-to-even rule is explicit.
pub fn round_half_even(value: f32) -> f32 {
    value.round_ties_even()
}

/// `SetItemPropertiesByMaterial` (`ItemBuilder.cs:643-651`) without touching
/// `nativeMaterialValue` (armor sets it before calling this).
fn set_item_properties_by_material(item: &mut Item, material: usize) {
    item.value = (i64::from(item.value) * 3 * i64::from(VALUE_MUL[material]))
        .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;

    let quarter_kgs = (item.weight_kg * 4.0) as i64;
    let material_quarters = (quarter_kgs * i64::from(WEIGHT_MUL[material])) as f32 / 4.0;
    item.weight_kg = round_half_even(material_quarters) / 4.0;

    item.max_condition =
        (i64::from(item.max_condition) * i64::from(COND_MUL[material]) / 4) as i32;
    // The source refills the item to its new maximum (R-21.3).
    item.current_condition = item.max_condition;
}

/// `ApplyWeaponMaterial` + `SetItemPropertiesByMaterial`
/// (`ItemBuilder.cs:414-427`, `:643-651`): stores the material index and
/// applies the value/weight/condition tables.
pub fn apply_weapon_material(item: &mut Item, material: usize) {
    item.native_material = material as i32;
    set_item_properties_by_material(item, material);
}

/// `ApplyArmorMaterial` (`ItemBuilder.cs:488-507`):
/// `Leather (0x0000)` halves weight, `Chain (0x0100)` doubles value, `Chain2
/// (0x0103)` falls through both branches (the source `if/else if` does not
/// capture it), and plate (`>= 0x0200`) applies the weapon table with
/// `plate = native_material - 0x0200`.
pub fn apply_armor_material(item: &mut Item, native_material: i32) {
    item.native_material = native_material;
    if native_material == 0x0000 {
        item.weight_kg /= 2.0;
    } else if native_material == 0x0100 {
        item.value = item.value.saturating_mul(2);
    } else if native_material >= 0x0200 {
        let plate = (native_material - 0x0200) as usize;
        set_item_properties_by_material(item, plate);
    }
}

/// Damage modifier of a weapon material (`DaggerfallUnityItem.cs:979-1005`):
/// Iron −1, Steel/Silver 0, Elven +1, Dwarven +2, Mithril/Adamantium +3,
/// Ebony +4, Orcish +5, Daedric +6; anything else 0.
pub fn weapon_material_modifier(material: i32) -> i32 {
    usize::try_from(material)
        .ok()
        .and_then(|index| DAMAGE_MOD.get(index).copied())
        .unwrap_or(0)
}

/// `GetMaterialArmorValue` (`DaggerfallUnityItem.cs:1007-1058`): material
/// table for armor, shield table for shields (Buckler 1, Round 2, Kite 3,
/// Tower 4), artifact armor halved.
pub fn material_armor_value(item: &Item) -> i32 {
    if is_shield(item) {
        return match item.template_index {
            109 => 1,
            110 => 2,
            111 => 3,
            112 => 4,
            _ => 0,
        };
    }
    let value = match item.native_material {
        0x0000 => 3,
        0x0100 | 0x0103 => 6,
        0x0200 => 7,
        0x0201 | 0x0202 => 9,
        0x0203 => 11,
        0x0204 => 13,
        0x0205 | 0x0206 => 15,
        0x0207 => 17,
        0x0208 => 19,
        0x0209 => 21,
        _ => 0,
    };
    if item.flags & FLAG_ARTIFACT != 0 && item.item_group == ItemGroup::Armor {
        value / 2
    } else {
        value
    }
}

/// Weapon skill families (`DaggerfallUnityItem.cs:910-935`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeaponSkill {
    /// Not a weapon (`Skills.None`).
    None,
    /// Dagger, tanto, wakazashi, shortsword.
    ShortBlade,
    /// Broadsword, longsword, saber, katana, claymore, dai-katana.
    LongBlade,
    /// Battle axe, war axe.
    Axe,
    /// Flail, mace, warhammer, staff.
    BluntWeapon,
    /// Short bow, long bow.
    Archery,
}

/// Weapon skill from the DFU `groupIndex` inside [`ItemGroup::Weapons`]
/// (`DaggerfallUnityItem.cs:910-935`; indices follow the enum order of
/// `ItemEnums.cs:210-235`).
pub fn weapon_skill(group_index: usize) -> WeaponSkill {
    match group_index {
        0 | 1 | 3 | 4 => WeaponSkill::ShortBlade, // Dagger, Tanto, Shortsword, Wakazashi
        5..=10 => WeaponSkill::LongBlade,          // Broadsword..Dai_Katana
        14 | 15 => WeaponSkill::Axe,               // Battle_Axe, War_Axe
        2 | 11 | 12 | 13 => WeaponSkill::BluntWeapon, // Staff, Mace, Flail, Warhammer
        16 | 17 => WeaponSkill::Archery,           // Short_Bow, Long_Bow
        _ => WeaponSkill::None,                    // Arrow and non-weapons
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_kg(got: f32, want: f32) {
        assert!((got - want).abs() < 1e-6, "got {got} kg, want {want} kg");
    }

    fn template(index: usize, units: i32, hp: u16, price: i32) -> ItemTemplate {
        let mut template = ItemTemplate::empty(index);
        template.name = format!("template-{index}");
        template.base_weight_units = units;
        template.hit_points = hp;
        template.base_price = price;
        template
    }

    // The 48 B `FALL.EXE` record has 12 fields (spec §1.2) and the helper
    // takes them in byte order; core verified against the MIT source — not refactored.
    #[allow(clippy::too_many_arguments)]
    fn record(
        name: &str,
        weight: i32,
        hp: u16,
        capacity: i32,
        price: i32,
        enchantment: i16,
        rarity: u8,
        variants: u8,
        draw: u8,
        properties: u8,
        world: i16,
        player: i16,
    ) -> [u8; ITEM_RECORD_SIZE] {
        let mut record = [0u8; ITEM_RECORD_SIZE];
        let name = name.as_bytes();
        record[..name.len()].copy_from_slice(name);
        record[24..28].copy_from_slice(&weight.to_le_bytes());
        record[28..30].copy_from_slice(&hp.to_le_bytes());
        record[30..34].copy_from_slice(&capacity.to_le_bytes());
        record[34..38].copy_from_slice(&price.to_le_bytes());
        record[38..40].copy_from_slice(&enchantment.to_le_bytes());
        record[40] = rarity;
        record[41] = variants;
        record[42] = draw;
        record[43] = properties;
        record[44..46].copy_from_slice(&world.to_le_bytes());
        record[46..48].copy_from_slice(&player.to_le_bytes());
        record
    }

    /// Retail-shaped table: the `Ruby` sentinel **is** the name of record 0,
    /// so the table starts at the sentinel (`ItemsFile.cs:425-433`;
    /// measured in `FALL.EXE`: 1796138 + 288 · 48 = 1809962). Record 0 is
    /// therefore always `Ruby`; `names` fills the remaining 287 records.
    fn table_blob(offset: usize, names: impl Fn(usize) -> String) -> Vec<u8> {
        let mut blob = vec![0u8; offset];
        for index in 0..TOTAL_ITEMS {
            let name = if index == 0 {
                "Ruby".to_string()
            } else {
                names(index)
            };
            blob.extend_from_slice(&record(
                &name,
                index as i32,
                1,
                0,
                index as i32,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
            ));
        }
        blob
    }

    // ── offset discovery (GV-IT-01, R-21.1) ─────────────────────────────

    #[test]
    fn sentinel_at_default_offset_returns_default() {
        let mut blob = vec![0u8; DEFAULT_ITEMS_OFFSET + 4];
        blob[DEFAULT_ITEMS_OFFSET..].copy_from_slice(RUBY_SENTINEL);
        assert_eq!(find_items_offset(&blob), Some(DEFAULT_ITEMS_OFFSET));
    }

    #[test]
    fn default_offset_without_sentinel_falls_back_to_first_ruby() {
        // Retail shape: byte 1776954 is '@', first "Ruby" is much later.
        let mut blob = vec![b'@'; DEFAULT_ITEMS_OFFSET + 4];
        blob.extend_from_slice(b"xxRubyxx");
        let expected = DEFAULT_ITEMS_OFFSET + 6;
        assert_eq!(find_items_offset(&blob), Some(expected));
        assert_ne!(find_items_offset(&blob), Some(DEFAULT_ITEMS_OFFSET));
    }

    #[test]
    fn missing_sentinel_is_reported() {
        assert_eq!(find_items_offset(&[0u8; 64]), None);
        assert_eq!(find_string(b"abc", b"Ruby"), None);
    }

    // ── record decoding (GV-IT-02/10/11 shape, synthetic) ───────────────

    #[test]
    fn golden_shape_of_ruby_record() {
        // GV-IT-02 field values, re-encoded as a synthetic 48-byte record.
        let data = record("Ruby", 1, 1000, 0, 250, 1750, 10, 12, 20, 0x01, 0x7F00, 0);
        // The sentinel IS record 0's name, so the table starts after 4
        // filler bytes that simply push it off the default offset.
        let mut blob = vec![0u8; 4];
        blob.extend_from_slice(&data);
        for _ in 1..TOTAL_ITEMS {
            blob.extend_from_slice(&data);
        }
        let templates = parse_item_templates(&blob).expect("table parses");
        let ruby = &templates[0];
        assert_eq!(ruby.name, "Ruby");
        assert_kg(ruby.base_weight_kg(), 0.25);
        assert_eq!(ruby.hit_points, 1000);
        assert_eq!(ruby.base_price, 250);
        assert_eq!(ruby.enchantment_points, 1750);
        assert_eq!(ruby.rarity, 10);
        assert_eq!(ruby.variants, 12);
        assert_eq!(ruby.draw_order_or_effect, 20);
        assert_eq!(ruby.world_texture_file(), 254);
        assert_eq!(ruby.world_texture_record(), 0);
        assert!(ruby.is_ingredient());
        assert_eq!(templates.len(), TOTAL_ITEMS);
    }

    #[test]
    fn property_bits_match_source() {
        // The retail table starts at the sentinel, so record 0 is named
        // `Ruby`; the name field carries no property bits (see `table_blob`).
        let dagger = record("Ruby", 2, 50, 0, 1, 200, 1, 0, 100, 0x04, 0, 0);
        let mut blob = Vec::new();
        for _ in 0..TOTAL_ITEMS {
            blob.extend_from_slice(&dagger);
        }
        let parsed = parse_item_templates(&blob).expect("table parses");
        let dagger = &parsed[0];
        assert!(dagger.is_one_handed());
        assert!(!dagger.is_blunt_weapon());
        assert!(!dagger.is_liquid());
        assert!(!dagger.is_ingredient());

        let staff = record("Ruby", 8, 300, 0, 5, 700, 1, 0, 100, 0x10, 0, 0);
        let mut blob = Vec::new();
        for _ in 0..TOTAL_ITEMS {
            blob.extend_from_slice(&staff);
        }
        let parsed = parse_item_templates(&blob).expect("table parses");
        assert!(parsed[0].is_blunt_weapon());
        assert!(!parsed[0].is_one_handed());
    }

    #[test]
    fn parse_rejects_truncated_and_sentinel_less_blobs() {
        let blob = table_blob(4, |index| format!("Item{index}"));
        let short = &blob[..blob.len() - 1];
        assert!(matches!(
            parse_item_templates(short),
            Err(ItemError::TruncatedTable { .. })
        ));
        assert_eq!(
            parse_item_templates(&[0u8; 32]),
            Err(ItemError::SentinelNotFound)
        );
    }

    #[test]
    fn parse_keeps_names_and_indices_in_order() {
        let blob = table_blob(16, |index| format!("Item{index}"));
        let templates = parse_item_templates(&blob).expect("table parses");
        assert_eq!(templates.len(), TOTAL_ITEMS);
        assert_eq!(templates[0].name, "Ruby", "record 0 is the sentinel");
        assert_eq!(templates[1].name, "Item1");
        assert_eq!(templates[287].name, "Item287");
        assert_eq!(templates[287].index, 287);
        assert_eq!(templates[287].base_weight_units, 287);
        assert_eq!(templates[287].base_price, 287);
        assert_eq!(blob.len() - 16, TOTAL_ITEMS * ITEM_RECORD_SIZE);
    }

    // ── group mapping ──────────────────────────────────────────────────

    #[test]
    fn groups_cover_the_retail_table() {
        assert_eq!(GROUP_COUNT, 29);
        assert_eq!(DEFAULT_ITEMS_OFFSET, 1_776_954);
        assert_eq!(TOTAL_ITEMS * ITEM_RECORD_SIZE, 13_824);
        assert_eq!(group_indices(ItemGroup::Weapons)[18], 131);
        assert_eq!(group_indices(ItemGroup::Armor)[0], 102);
        assert_eq!(group_indices(ItemGroup::Deeds), &[99, 100, 101]);
        assert!(group_indices(ItemGroup::MagicItems).is_empty());
    }

    #[test]
    fn template_group_resolves_primary_owner() {
        assert_eq!(template_group(0), ItemGroup::Gems);
        assert_eq!(template_group(99), ItemGroup::Deeds);
        assert_eq!(template_group(102), ItemGroup::Armor);
        assert_eq!(template_group(131), ItemGroup::Weapons);
        assert_eq!(template_group(276), ItemGroup::Currency);
        assert_eq!(template_group(277), ItemGroup::Books);
        assert_eq!(template_group(281), ItemGroup::QuestItems);
        assert_eq!(template_group(287), ItemGroup::Maps);
        assert_eq!(template_group(246), ItemGroup::MiscItems);
    }

    #[test]
    fn from_template_maps_group_index_like_dfu() {
        let sword = Item::from_template(&template(120, 18, 800, 15));
        assert_eq!(sword.item_group, ItemGroup::Weapons);
        assert_eq!(sword.group_index, 7);
        assert_eq!(sword.template_index, 120);
        assert_eq!(sword.stack_count, 1);
        assert_eq!(sword.current_condition, 800);
        assert_eq!(sword.max_condition, 800);

        let cuirass = Item::from_template(&template(102, 50, 4096, 100));
        assert_eq!(cuirass.item_group, ItemGroup::Armor);
        assert_eq!(cuirass.group_index, 0);

        let twigs = Item::from_template(&template(8, 1, 40, 4));
        assert_eq!(twigs.item_group, ItemGroup::PlantIngredients1);
        assert_eq!(twigs.group_index, 0);
        let golden_poppy = Item::from_template(&template(25, 1, 40, 4));
        assert_eq!(golden_poppy.group_index, 14);
    }

    #[test]
    fn uid_is_non_zero_and_monotonic() {
        let a = Item::from_template(&template(120, 18, 800, 15));
        let b = Item::from_template(&template(120, 18, 800, 15));
        assert!(a.uid > 0);
        assert!(b.uid > a.uid);
    }

    // ── GV-MAT-01..10: Longsword material table ────────────────────────

    #[test]
    fn golden_weapon_materials() {
        // GV-IT-12 base: 4.5 kg, 15 gp, 800 condition.
        let cases = [
            (0, 4.5, 45, 800),
            (1, 5.5, 90, 1200),
            (2, 4.5, 180, 1200),
            (3, 4.5, 360, 1600),
            (4, 3.5, 720, 2400),
            (5, 4.5, 1440, 3200),
            (6, 4.5, 2880, 4000),
            (7, 2.25, 5760, 4800),
            (8, 4.5, 11520, 5600),
            (9, 5.5, 23040, 6400),
        ];
        for (material, kg, value, condition) in cases {
            let mut item = Item::from_template(&template(120, 18, 800, 15));
            apply_weapon_material(&mut item, material);
            assert_eq!(item.native_material, material as i32);
            assert_kg(item.weight_kg, kg);
            assert_eq!(item.value, value, "GV-MAT value m={material}");
            assert_eq!(item.max_condition, condition, "GV-MAT cond m={material}");
            assert_eq!(item.current_condition, condition, "R-21.3 refill");
        }
    }

    #[test]
    fn round_half_even_matches_unity() {
        assert_eq!(round_half_even(22.5), 22.0);
        assert_eq!(round_half_even(13.5), 14.0);
        assert_eq!(round_half_even(62.5), 62.0);
        assert_eq!(round_half_even(37.5), 38.0);
    }

    // ── GV-MAT-11..15: Cuirass armor materials ─────────────────────────

    #[test]
    fn golden_armor_materials() {
        // GV-IT-07 base: 12.5 kg, 100 gp, 4096 condition.
        let mut leather = Item::from_template(&template(102, 50, 4096, 100));
        apply_armor_material(&mut leather, 0x0000);
        assert_kg(leather.weight_kg, 6.25);
        assert_eq!(leather.value, 100);
        assert_eq!(leather.max_condition, 4096);

        let cases = [
            (0x0200, 12.5, 300, 4096),
            (0x0201, 15.5, 600, 6144),
            (0x0204, 9.5, 4800, 12288),
            (0x0209, 15.5, 153600, 32768),
        ];
        for (material, kg, value, condition) in cases {
            let mut item = Item::from_template(&template(102, 50, 4096, 100));
            apply_armor_material(&mut item, material);
            assert_eq!(item.native_material, material);
            assert_kg(item.weight_kg, kg);
            assert_eq!(item.value, value);
            assert_eq!(item.max_condition, condition);
        }
    }

    #[test]
    fn chain_and_chain2_branches_match_the_source() {
        let mut chain = Item::from_template(&template(102, 50, 4096, 100));
        apply_armor_material(&mut chain, 0x0100);
        assert_kg(chain.weight_kg, 12.5);
        assert_eq!(chain.value, 200);
        assert_eq!(chain.max_condition, 4096);

        let mut chain2 = Item::from_template(&template(102, 50, 4096, 100));
        apply_armor_material(&mut chain2, 0x0103);
        assert_kg(chain2.weight_kg, 12.5);
        assert_eq!(chain2.value, 100);
        assert_eq!(chain2.max_condition, 4096);
    }

    // ── §2.6 condition and weight ──────────────────────────────────────

    #[test]
    fn golden_condition_percentages() {
        assert_eq!(condition_percent(50, 50), 100, "GV-CND-01");
        assert_eq!(condition_percent(25, 50), 50, "GV-CND-02");
        assert_eq!(condition_percent(49, 50), 98, "GV-CND-03");
        assert_eq!(condition_percent(1, 3), 33, "GV-CND-04");
        assert_eq!(condition_percent(5, 0), 100, "GV-CND-05");
    }

    #[test]
    fn golden_encumbrance_and_arrow_weight() {
        assert_eq!(max_encumbrance(50), 75, "GV-CND-06");
        assert_eq!(max_encumbrance(100), 150, "GV-CND-06");
        assert_eq!(max_encumbrance(33), 49, "GV-CND-06");

        let arrow_template = template(131, 2, 0, 1);
        let mut arrow = Item::from_template(&arrow_template);
        assert!(has_no_encumbrance(&arrow));
        assert_kg(effective_unit_weight_kg(&arrow, &arrow_template), 0.0);
        arrow.stack_count = 20;
        assert_kg(total_weight_kg(&arrow, &arrow_template), 0.0);
        assert_kg(arrow.weight_kg, 0.5); // GV-CND-07 stored weight
    }

    #[test]
    fn no_encumbrance_covers_transportation_arrows_and_maps() {
        for (index, expected) in [(93, true), (98, true), (131, true), (287, true), (120, false)] {
            let item = Item::from_template(&template(index, 4, 10, 1));
            assert_eq!(has_no_encumbrance(&item), expected, "template {index}");
            assert_eq!(item.template_index, index);
            assert_eq!(
                ItemTemplate::empty(index).has_no_encumbrance(),
                expected,
                "template flag {index}"
            );
        }
    }

    #[test]
    fn stackability_matches_source_rules() {
        let mut gem = Item::from_template(&template(0, 1, 1000, 250));
        assert!(is_stackable(&gem), "gems carry the isIngredient bit");
        gem.equipped = true;
        assert!(!is_stackable(&gem), "equipped items never stack");
        gem.equipped = false;
        gem.legacy_magic.push((0, 0));
        assert!(!is_stackable(&gem), "enchanted items never stack");

        let dagger = Item::from_template(&template(113, 2, 50, 1));
        assert!(!is_stackable(&dagger));
        let arrow = Item::from_template(&template(131, 2, 50, 1));
        assert!(is_stackable(&arrow));
        let gold = Item::from_template(&template(276, 1, 1, 1));
        assert_eq!(gold.item_group, ItemGroup::Currency);
        assert!(is_stackable(&gold));
        let book = Item::from_template(&template(277, 1, 1, 10));
        assert!(is_stackable(&book), "books are stackable in the source");
        let oil = Item::from_template(&template(252, 4, 1, 10));
        assert!(is_stackable(&oil));
        let potion = Item::from_template(&template(83, 4, 1, 10));
        assert!(is_potion(&potion));
        assert!(is_stackable(&potion));

        let legacy_none = Item::from_template(&template(0, 1, 1000, 250));
        let mut legacy_none = legacy_none;
        legacy_none.legacy_magic.push((-1, 0));
        assert!(!is_enchanted(&legacy_none));
    }

    // ── M22 API surface ────────────────────────────────────────────────

    #[test]
    fn armor_values_follow_the_source_table() {
        let cases = [
            (0x0000, 3),
            (0x0100, 6),
            (0x0103, 6),
            (0x0200, 7),
            (0x0201, 9),
            (0x0202, 9),
            (0x0203, 11),
            (0x0204, 13),
            (0x0205, 15),
            (0x0206, 15),
            (0x0207, 17),
            (0x0208, 19),
            (0x0209, 21),
        ];
        for (material, expected) in cases {
            let mut item = Item::from_template(&template(102, 50, 4096, 100));
            item.native_material = material;
            assert_eq!(material_armor_value(&item), expected, "material {material:#06x}");
        }

        for (index, expected) in [(109, 1), (110, 2), (111, 3), (112, 4)] {
            let item = Item::from_template(&template(index, 5, 100, 10));
            assert!(is_shield(&item));
            assert_eq!(material_armor_value(&item), expected, "shield {index}");
        }

        let mut cuirass = Item::from_template(&template(102, 50, 4096, 100));
        cuirass.native_material = 0x0209;
        cuirass.flags = FLAG_ARTIFACT;
        assert_eq!(material_armor_value(&cuirass), 10, "artifact armor halves");
    }

    #[test]
    fn weapon_material_modifier_is_the_damage_table() {
        let expected = [-1, 0, 0, 1, 2, 3, 3, 4, 5, 6];
        for (material, modifier) in expected.iter().enumerate() {
            assert_eq!(weapon_material_modifier(material as i32), *modifier);
        }
        assert_eq!(weapon_material_modifier(-1), 0);
        assert_eq!(weapon_material_modifier(99), 0);
        assert_eq!(TO_HIT_MOD[9], 60);
        assert_eq!(TO_HIT_MOD[0], -10);
    }

    #[test]
    fn weapon_skills_follow_the_source_switch() {
        let expected = [
            WeaponSkill::ShortBlade,  // 0 Dagger
            WeaponSkill::ShortBlade,  // 1 Tanto
            WeaponSkill::BluntWeapon, // 2 Staff
            WeaponSkill::ShortBlade,  // 3 Shortsword
            WeaponSkill::ShortBlade,  // 4 Wakazashi
            WeaponSkill::LongBlade,   // 5 Broadsword
            WeaponSkill::LongBlade,   // 6 Saber
            WeaponSkill::LongBlade,   // 7 Longsword
            WeaponSkill::LongBlade,   // 8 Katana
            WeaponSkill::LongBlade,   // 9 Claymore
            WeaponSkill::LongBlade,   // 10 Dai_Katana
            WeaponSkill::BluntWeapon, // 11 Mace
            WeaponSkill::BluntWeapon, // 12 Flail
            WeaponSkill::BluntWeapon, // 13 Warhammer
            WeaponSkill::Axe,         // 14 Battle_Axe
            WeaponSkill::Axe,         // 15 War_Axe
            WeaponSkill::Archery,     // 16 Short_Bow
            WeaponSkill::Archery,     // 17 Long_Bow
            WeaponSkill::None,        // 18 Arrow
        ];
        for (index, skill) in expected.iter().enumerate() {
            assert_eq!(weapon_skill(index), *skill, "group index {index}");
        }
        assert_eq!(weapon_skill(19), WeaponSkill::None);
    }
}

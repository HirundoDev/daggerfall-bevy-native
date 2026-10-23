//! Equipment slots and the 27-slot equip table — SPEC M21 §3.
//!
//! Transcribed from `ItemEquipTable.cs` / `ItemEnums.cs` of Daggerfall Unity
//! `@0038a1e6` (MIT). The slot indices are the legacy DFU equip slots
//! (`ItemEnums.cs:105-135`), not a freshly invented order: jewelry occupies
//! 0..=11, body armor 12..=18, hands 19..=21, legs/clothes 23..=24 and feet
//! 26. GV-EQP-08 (Cuirass → 18) and GV-EQP-09 (Tower shield → 21) pin that
//! mapping.
//!
//! Frontiers: equip sounds and enchantment payloads (`StartEquippedItem`) are
//! M24/audio, and the inventory collection that owns stacks (`SplitStack`)
//! lives in M33 persistence; this table only models the equipped slots.

use daggerfall_behavior::durability::{apply_wear, WearOutcome};
use daggerfall_behavior::locomotion::Dice100;

use crate::items::{fresh_uid, is_shield, material_armor_value, Item, ItemGroup};

/// Number of legacy equip slots (`ItemEquipTable.cs:23`).
pub const EQUIP_TABLE_LENGTH: usize = 27;

/// `DaggerfallUnity.Settings.BowLeftHandWithSwitching` (`ItemEquipTable.cs:633`).
/// The target fixes it to `false`, so bows are two-handed (GV-EQP-10).
pub const BOW_LEFT_HAND_WITH_SWITCHING: bool = false;

/// Legacy equipment slots (`ItemEnums.cs:105-135`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum EquipSlot {
    /// Not equippable.
    None = -1,
    /// Amulets / torcs.
    Amulet0 = 0,
    /// Second amulet slot.
    Amulet1 = 1,
    /// Bracelets.
    Bracelet0 = 2,
    /// Second bracelet slot.
    Bracelet1 = 3,
    /// Rings.
    Ring0 = 4,
    /// Second ring slot.
    Ring1 = 5,
    /// Bracers.
    Bracer0 = 6,
    /// Second bracer slot.
    Bracer1 = 7,
    /// Marks.
    Mark0 = 8,
    /// Second mark slot.
    Mark1 = 9,
    /// Gems.
    Crystal0 = 10,
    /// Second gem slot.
    Crystal1 = 11,
    /// Helm.
    Head = 12,
    /// Right pauldron.
    RightArm = 13,
    /// Cloak.
    Cloak1 = 14,
    /// Left pauldron.
    LeftArm = 15,
    /// Second cloak slot.
    Cloak2 = 16,
    /// Shirt / straps / tunic / robes.
    ChestClothes = 17,
    /// Cuirass.
    ChestArmor = 18,
    /// Right weapon / two-handed weapon.
    RightHand = 19,
    /// Gauntlets.
    Gloves = 20,
    /// Left weapon / shields.
    LeftHand = 21,
    /// Unknown legacy slot (`Unknown1`).
    Unknown1 = 22,
    /// Greaves.
    LegsArmor = 23,
    /// Pants / skirts / loincloth.
    LegsClothes = 24,
    /// Unknown legacy slot (`Unknown2`).
    Unknown2 = 25,
    /// Boots / shoes / sandals.
    Feet = 26,
}

impl EquipSlot {
    /// Array index, or `None` for [`EquipSlot::None`].
    pub fn index(self) -> Option<usize> {
        usize::try_from(self as i32).ok()
    }
}

/// `ItemHands` (`ItemEquipTable.cs:598-650`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemHands {
    /// Not a hand-held item.
    None,
    /// Either hand.
    Either,
    /// Right hand only (no current template uses it).
    RightOnly,
    /// Left hand only (shields).
    LeftOnly,
    /// Both hands (two-handed weapons; bows when switching is off).
    Both,
}

/// `ItemEquipTable.GetItemHands` (`ItemEquipTable.cs:598-650`).
pub fn item_hands(item: &Item) -> ItemHands {
    if item.item_group != ItemGroup::Weapons && item.item_group != ItemGroup::Armor {
        return ItemHands::None;
    }

    if item.item_group == ItemGroup::Weapons {
        return match item.template_index {
            // Either-hand weapons (`:610-622`).
            113 | 114 | 116 | 117 | 118 | 119 | 120 | 121 | 124 | 127 => ItemHands::Either,
            // Two-handed weapons (`:624-631`).
            115 | 122 | 123 | 125 | 126 | 128 => ItemHands::Both,
            // Bows (`:633-635`).
            129 | 130 => {
                if BOW_LEFT_HAND_WITH_SWITCHING {
                    ItemHands::LeftOnly
                } else {
                    ItemHands::Both
                }
            }
            // Arrow and anything else: custom hook (`item.GetItemHands()`) is
            // None in the target, so arrows never reach a slot (GV-EQP-06).
            _ => ItemHands::None,
        };
    }

    // Shields (`:638-644`).
    if is_shield(item) {
        ItemHands::LeftOnly
    } else {
        ItemHands::None
    }
}

/// The 7 DFU `BodyParts` (`ItemEnums.cs:140-150`): `Head = 0`, `RightArm = 1`,
/// `LeftArm = 2`, `Chest = 3`, `Hands = 4`, `Legs = 5`, `Feet = 6`. These are
/// the indices of `DaggerfallEntity.ArmorValues` (`DaggerfallEntity.cs:270`),
/// not a freshly invented order.
///
/// **This module yields the delta, never the entity vector:** the summary here
/// is the wear reduction of the equipped table (`UpdateEquippedArmorValues`,
/// `DaggerfallEntity.cs:589-636`). A player entity's `ArmorValues` starts at
/// **100** on every part (`SerializablePlayer.cs:355-366`,
/// `PlayerEntity.cs:1188-1191`) and the app adds that seed in
/// `Fighter::refresh_equipment`; the 0-based reading of these indices is the
/// M21 *delta* convention, not the player's array.
pub const BODY_PART_COUNT: usize = 7;

/// `DaggerfallUnityItem.GetBodyPartForEquipSlot`
/// (`DaggerfallUnityItem.cs:1131-1153`): which of the 7 body parts an equip
/// slot protects; `None` for slots that map to `BodyParts.None` (jewelry,
/// weapons, cloaks, clothes).
pub fn body_part_for_equip_slot(slot: EquipSlot) -> Option<usize> {
    match slot {
        EquipSlot::Head => Some(0),
        EquipSlot::RightArm => Some(1),
        EquipSlot::LeftArm => Some(2),
        EquipSlot::ChestArmor => Some(3),
        EquipSlot::Gloves => Some(4),
        EquipSlot::LegsArmor => Some(5),
        EquipSlot::Feet => Some(6),
        _ => None,
    }
}

/// `DaggerfallUnityItem.GetEquipSlotForBodyPart`
/// (`DaggerfallUnityItem.cs:1103-1125`): the armour slot damaged by a strike
/// on `body_part`; [`EquipSlot::None`] outside `0..BODY_PART_COUNT`.
pub fn equip_slot_for_body_part(body_part: usize) -> EquipSlot {
    match body_part {
        0 => EquipSlot::Head,
        1 => EquipSlot::RightArm,
        2 => EquipSlot::LeftArm,
        3 => EquipSlot::ChestArmor,
        4 => EquipSlot::Gloves,
        5 => EquipSlot::LegsArmor,
        6 => EquipSlot::Feet,
        _ => EquipSlot::None,
    }
}

/// `DaggerfallUnityItem.GetShieldProtectedBodyParts`
/// (`DaggerfallUnityItem.cs:1082-1097`): Buckler covers `LeftArm` + `Hands`;
/// Round and Kite add `Legs`; Tower adds `Head`. Non-shields protect nothing.
pub fn shield_protected_body_parts(item: &Item) -> &'static [usize] {
    match item.template_index {
        109 => &[2, 4],
        110 | 111 => &[2, 4, 5],
        112 => &[0, 2, 4, 5],
        _ => &[],
    }
}

/// Report of one `ApplyConditionDamageThroughPhysicalHit`
/// (`FormulaHelper.cs:1123-1138`) — the wear of an armour piece
/// ([`ItemEquipTable::damage_equipment`]) or of the attacker's weapon
/// ([`ItemEquipTable::damage_item`]).
#[derive(Debug, Clone, PartialEq)]
pub struct ItemWear {
    /// Slot the damaged item occupied: the shield's `LeftHand`, the struck
    /// body part's armour slot, or the attacker's weapon hand.
    pub slot: EquipSlot,
    /// UID of the damaged item (stable after a break removes it).
    pub item_uid: u64,
    /// Condition points lost (`0` when the wear roll is zero and the item is
    /// still intact, `FormulaHelper.cs:1130-1132`).
    pub wear: i32,
    /// Condition left after the wear (`0` when broken).
    pub condition: i32,
    /// The item reached zero (`ItemBreaks`, `DaggerfallUnityItem.cs:1170-1182`).
    pub broke: bool,
    /// Broken piece taken out of the slot (DFU `ItemBreaks → UnequipItem`,
    /// `DaggerfallUnityItem.cs:1170-1190`, `:1212-1218`), so the owner can
    /// hand it to its inventory collection (M33). `None` when it survived.
    pub removed: Option<Item>,
}

/// Result of [`ItemEquipTable::equip`].
#[derive(Debug, Clone, PartialEq)]
pub struct EquipAttempt {
    /// Slot the item landed in.
    pub slot: EquipSlot,
    /// Items displaced by this equip, in `UnequipItem` call order.
    pub unequipped: Vec<Item>,
    /// Units left behind when a stack was split (source: `SplitStack(item, 1)`
    /// leaves `stack_count - 1` in the collection).
    pub remainder: Option<Item>,
}

/// `EquipItem` result: the attempt or the item the table refused to hold.
#[derive(Debug, Clone, PartialEq)]
pub enum EquipResult {
    /// Equipped, with everything the operation displaced.
    Equipped(EquipAttempt),
    /// Rejected (`EquipSlots.None`, arrows, or occupied slot without
    /// `always_equip`). The item is handed back instead of dropped, a declared
    /// delta from DFU returning `null` and losing the split unit.
    Rejected(Item),
}

/// The 27 equipped slots (`ItemEquipTable.cs`).
#[derive(Debug, Clone, PartialEq)]
pub struct ItemEquipTable {
    slots: [Option<Item>; EQUIP_TABLE_LENGTH],
}

impl Default for ItemEquipTable {
    fn default() -> Self {
        Self::new()
    }
}

impl ItemEquipTable {
    /// Empty table (`new DaggerfallUnityItem.()` clears all slots).
    pub fn new() -> Self {
        Self {
            slots: std::array::from_fn(|_| None),
        }
    }

    /// `GetItem` (`ItemEquipTable.cs:80-87`).
    pub fn get(&self, slot: EquipSlot) -> Option<&Item> {
        self.slots[slot.index()?].as_ref()
    }

    /// `GetEquipSlot` (`ItemEquipTable.cs:238-281`).
    pub fn get_equip_slot(&self, item: &Item) -> EquipSlot {
        match item.item_group {
            ItemGroup::Gems => self.first_slot(EquipSlot::Crystal0, EquipSlot::Crystal1),
            ItemGroup::Jewellery => match item.template_index {
                // Amulet, Torc, Cloth_amulet (`:371-374`).
                133 | 138 | 139 => self.first_slot(EquipSlot::Amulet0, EquipSlot::Amulet1),
                134 => self.first_slot(EquipSlot::Bracer0, EquipSlot::Bracer1),
                135 => self.first_slot(EquipSlot::Ring0, EquipSlot::Ring1),
                136 => self.first_slot(EquipSlot::Bracelet0, EquipSlot::Bracelet1),
                137 => self.first_slot(EquipSlot::Mark0, EquipSlot::Mark1),
                // Wand has no slot (`:395-396` default).
                _ => EquipSlot::None,
            },
            ItemGroup::Armor => match item.template_index {
                102 => EquipSlot::ChestArmor,
                103 => EquipSlot::Gloves,
                104 => EquipSlot::LegsArmor,
                105 => EquipSlot::LeftArm,
                106 => EquipSlot::RightArm,
                107 => EquipSlot::Head,
                108 => EquipSlot::Feet,
                109..=112 => EquipSlot::LeftHand,
                _ => EquipSlot::None,
            },
            ItemGroup::Weapons => self.weapon_slot(item),
            ItemGroup::MensClothing => self.mens_clothing_slot(item),
            ItemGroup::WomensClothing => self.womens_clothing_slot(item),
            _ => EquipSlot::None,
        }
    }

    /// `GetWeaponSlot` (`ItemEquipTable.cs:436-463`).
    fn weapon_slot(&self, item: &Item) -> EquipSlot {
        // A two-handed weapon in the right hand is always replaced from the
        // right hand, unless the incoming item is left-hand only.
        if let Some(right) = self.get(EquipSlot::RightHand) {
            if item_hands(right) == ItemHands::Both && item_hands(item) != ItemHands::LeftOnly {
                return EquipSlot::RightHand;
            }
        }
        match item_hands(item) {
            ItemHands::RightOnly | ItemHands::Both => EquipSlot::RightHand,
            ItemHands::LeftOnly => EquipSlot::LeftHand,
            ItemHands::Either => self.first_slot(EquipSlot::RightHand, EquipSlot::LeftHand),
            ItemHands::None => EquipSlot::None,
        }
    }

    /// `GetMensClothingSlot` (`ItemEquipTable.cs:458-524`).
    fn mens_clothing_slot(&self, item: &Item) -> EquipSlot {
        match item.template_index {
            141..=146 | 157..=161 | 163..=164 | 165..=173 | 176..=181 => EquipSlot::ChestClothes,
            147..=150 => EquipSlot::Feet,
            151 | 152 | 153 | 156 | 162 | 174 | 175 => EquipSlot::LegsClothes,
            // Casual_cloak / Formal_cloak use Cloak2 first (`:518-521`).
            154 | 155 => self.first_slot(EquipSlot::Cloak2, EquipSlot::Cloak1),
            _ => EquipSlot::None,
        }
    }

    /// `GetWomensClothingSlot` (`ItemEquipTable.cs:526-570`).
    fn womens_clothing_slot(&self, item: &Item) -> EquipSlot {
        match item.template_index {
            182..=185 | 194..=198 | 200..=210 | 214..=216 => EquipSlot::ChestClothes,
            186..=189 => EquipSlot::Feet,
            190 | 193 | 199 | 211 | 212 | 213 => EquipSlot::LegsClothes,
            // Casual_cloak / Formal_cloak use Cloak2 first (`:563-566`).
            191 | 192 => self.first_slot(EquipSlot::Cloak2, EquipSlot::Cloak1),
            _ => EquipSlot::None,
        }
    }

    /// `IsSlotOpen` (`ItemEquipTable.cs:283-294`); `None` is never open.
    fn is_slot_open(&self, slot: EquipSlot) -> bool {
        match slot.index() {
            Some(index) => self.slots[index].is_none(),
            None => false,
        }
    }

    /// `GetFirstSlot` (`ItemEquipTable.cs:298-306`): first open slot, else the
    /// first slot.
    fn first_slot(&self, slot1: EquipSlot, slot2: EquipSlot) -> EquipSlot {
        if self.is_slot_open(slot1) {
            slot1
        } else if self.is_slot_open(slot2) {
            slot2
        } else {
            slot1
        }
    }

    /// `EquipItem` (`ItemEquipTable.cs:94-149`).
    ///
    /// Order is the source order and matters:
    /// 1. resolve the slot (`None` rejects, which is how arrows are refused);
    /// 2. split one unit off a stack;
    /// 3. a `Both` weapon unequips left then right;
    /// 4. a `LeftOnly` item unequips a `Both` weapon in the right hand;
    /// 5. an occupied slot rejects unless `always_equip`, otherwise the
    ///    previous item is unequipped;
    /// 6. the item lands in the slot.
    pub fn equip(&mut self, mut item: Item, always_equip: bool) -> EquipResult {
        let slot = self.get_equip_slot(&item);
        if slot == EquipSlot::None {
            return EquipResult::Rejected(item);
        }

        // If more than one item selected, equip only one (`:104-106`).
        let mut remainder = None;
        if item.stack_count > 1 {
            let mut left_behind = item.clone();
            left_behind.uid = fresh_uid();
            left_behind.stack_count = item.stack_count - 1;
            left_behind.equipped = false;
            item.stack_count = 1;
            remainder = Some(left_behind);
        }

        let mut unequipped = Vec::new();

        // Equipping a 2H weapon always unequips both hands (`:120-125`).
        if item.item_group == ItemGroup::Weapons && item_hands(&item) == ItemHands::Both {
            self.push_unequipped(EquipSlot::LeftHand, &mut unequipped);
            self.push_unequipped(EquipSlot::RightHand, &mut unequipped);
        }

        // Equipping a shield always unequips a 2H weapon (`:127-133`).
        if item_hands(&item) == ItemHands::LeftOnly {
            let right_is_two_handed = self
                .get(EquipSlot::RightHand)
                .is_some_and(|right| item_hands(right) == ItemHands::Both);
            if right_is_two_handed {
                self.push_unequipped(EquipSlot::RightHand, &mut unequipped);
            }
        }

        // Unequip any previous item (`:135-139`).
        if !self.is_slot_open(slot) && !always_equip {
            return EquipResult::Rejected(item);
        }
        self.push_unequipped(slot, &mut unequipped);

        // Equip item to slot (`:141-143`).
        item.equipped = true;
        if let Some(index) = slot.index() {
            self.slots[index] = Some(item);
        }

        EquipResult::Equipped(EquipAttempt {
            slot,
            unequipped,
            remainder,
        })
    }

    fn push_unequipped(&mut self, slot: EquipSlot, out: &mut Vec<Item>) {
        if let Some(item) = self.unequip(slot) {
            out.push(item);
        }
    }

    /// `UnequipItem` (`ItemEquipTable.cs:180-197`): returns the item if the
    /// slot held one; `None` for an empty slot or [`EquipSlot::None`].
    pub fn unequip(&mut self, slot: EquipSlot) -> Option<Item> {
        let index = slot.index()?;
        let mut item = self.slots[index].take()?;
        item.equipped = false;
        Some(item)
    }

    /// Armour **delta** of the 7 `ArmorValues`
    /// (`PlayerEntity.UpdateEquippedArmorValues`, `DaggerfallEntity.cs:589-636`):
    /// each worn piece subtracts `GetMaterialArmorValue() * 5` from the body
    /// part it covers (`:608-615`), and a shield subtracts
    /// `GetShieldArmorValue() * 5` from every part it protects (`:616-636`).
    ///
    /// The source mutates the live array incrementally on equip/unequip;
    /// recomputing it from the occupied slots is the same function because
    /// every update is a signed sum of the same terms and this projection
    /// starts at `0`. **That `0` is the delta convention of this table, not an
    /// `ArmorValues` value**: DFU seeds every body part at `100` "no armour"
    /// — player `SerializablePlayer.cs:355-366` / `PlayerEntity.cs:1188-1191`,
    /// enemy `EnemyEntity.cs:409-413` — and the entity vector is therefore
    /// `100 + delta` (the app rebases it in
    /// `Fighter::refresh_equipment`). DFU also admits clothing groups 6-8 /
    /// 4-6 into the branch, but those items resolve to `BodyParts.None`
    /// (`DaggerfallUnityItem.cs:1131-1153`) and therefore contribute nothing;
    /// restricting to [`ItemGroup::Armor`] is behaviourally identical.
    pub fn armor_values(&self) -> [i32; BODY_PART_COUNT] {
        let mut values = [0i32; BODY_PART_COUNT];
        for item in self.slots.iter().flatten() {
            self.add_armor_contribution(item, &mut values);
        }
        values
    }

    /// Armour contribution of the items occupying the slot range
    /// `[first, last_exclusive)`.
    ///
    /// `EnemyEntity.SetEnemyEquipment` recomputes an enemy's `ArmorValues`
    /// with `for (i = (int)EquipSlots.Head; i < (int)EquipSlots.Feet; i++)`
    /// (`EnemyEntity.cs:415-422`): slots `12..=25`, so the `Feet` slot (26)
    /// is **never** enumerated and a spawned enemy's boots contribute no
    /// armour even though they are equipped. Same arithmetic as
    /// [`Self::armor_values`], which is the player's full-table projection
    /// (`DaggerfallEntity.UpdateEquippedArmorValues`, `:589-636`).
    pub fn armor_values_over_slots(
        &self,
        first: EquipSlot,
        last_exclusive: EquipSlot,
    ) -> [i32; BODY_PART_COUNT] {
        let mut values = [0i32; BODY_PART_COUNT];
        let (Some(first), Some(last)) = (first.index(), last_exclusive.index()) else {
            return values;
        };
        for index in first..last {
            if let Some(item) = self.slots.get(index).and_then(Option::as_ref) {
                self.add_armor_contribution(item, &mut values);
            }
        }
        values
    }

    /// One worn piece's contribution
    /// (`DaggerfallEntity.UpdateEquippedArmorValues`, `DaggerfallEntity.cs:589-636`):
    /// armour subtracts `GetMaterialArmorValue() * 5` from the body part its
    /// own slot covers, a shield subtracts `GetShieldArmorValue() * 5` from
    /// every part it protects. Clothing groups are excluded because they
    /// resolve to `BodyParts.None` (see [`Self::armor_values`]).
    fn add_armor_contribution(&self, item: &Item, values: &mut [i32; BODY_PART_COUNT]) {
        if item.item_group != ItemGroup::Armor {
            return;
        }
        let bonus = material_armor_value(item) * 5;
        if is_shield(item) {
            for &part in shield_protected_body_parts(item) {
                values[part] -= bonus;
            }
            return;
        }
        // DFU asks the table for the item's own slot
        // (`ItemEquipTable.GetEquipSlot(armor)`, `DaggerfallEntity.cs:601`),
        // not the array index, so the projection matches the source even if
        // an item was placed by another path.
        if let Some(part) = body_part_for_equip_slot(self.get_equip_slot(item)) {
            values[part] -= bonus;
        }
    }

    /// `FormulaHelper.DamageEquipment` (`FormulaHelper.cs:1080-1121`) through
    /// `ApplyConditionDamageThroughPhysicalHit` (`:1123-1138`): wear the armour
    /// piece that covers `struck_body_part`. A shield in the left hand takes the
    /// hit when it protects that part; otherwise the part's armour slot does
    /// (`DaggerfallUnityItem.cs:1103-1125`). No armour piece there ⇒ `None`.
    ///
    /// The caller's contract mirrors DFU's guard: `DamageEquipment` only runs
    /// for a weapon hit with `damage > 0` (`FormulaHelper.cs:1090`), so a
    /// non-positive damage returns `None` without consuming a roll. Attacker
    /// weapon wear (`ApplyConditionDamageThroughPhysicalHit(weapon, attacker, …)`,
    /// `:1093`) is applied through [`Self::damage_item`] on the attacker's own
    /// table.
    pub fn damage_equipment(
        &mut self,
        struck_body_part: usize,
        damage: i32,
        dice: &mut Dice100,
    ) -> Option<ItemWear> {
        if damage <= 0 {
            return None;
        }
        let slot = self.armor_slot_for_struck_part(struck_body_part)?;
        self.damage_item(slot, damage, dice)
    }

    /// `ApplyConditionDamageThroughPhysicalHit` (`FormulaHelper.cs:1123-1138`):
    /// wear the item in `slot` by `damage`. `damage <= 0` returns `None` without
    /// consuming a roll (`DamageEquipment` guard `:1090`). A break unequips
    /// (`ItemBreaks -> UnequipItem`, `DaggerfallUnityItem.cs:1170-1190`).
    ///
    /// The wear is `(10 * damage + 50) / 100` with the 20 % "minimum one point"
    /// re-roll at zero (`FormulaHelper.cs:1130-1132`), applied through
    /// `behavior::durability::apply_wear` — the arithmetic lives in exactly one
    /// place (M21 §4).
    pub fn damage_item(
        &mut self,
        slot: EquipSlot,
        damage: i32,
        dice: &mut Dice100,
    ) -> Option<ItemWear> {
        if damage <= 0 {
            return None;
        }
        let index = slot.index()?;
        let item = self.slots[index].as_mut()?;
        let item_uid = item.uid;
        let outcome = apply_wear(&mut item.current_condition, damage, dice);
        let (wear, broke) = match outcome {
            WearOutcome::Unchanged => (0, false),
            WearOutcome::Worn { amount, .. } => (amount, false),
            WearOutcome::Broke { amount } => (amount, true),
        };
        let condition = item.current_condition;
        let removed = if broke { self.unequip(slot) } else { None };
        Some(ItemWear {
            slot,
            item_uid,
            wear,
            condition,
            broke,
            removed,
        })
    }

    /// Slot of the melee weapon this table holds: right hand first, then left
    /// (`DaggerfallUnityItem` hands; a two-hander lands in `RightHand` per
    /// `ItemEquipTable.GetWeaponSlot`, `ItemEquipTable.cs:435-456`).
    ///
    /// Only [`ItemGroup::Weapons`] items count (an Arrow is refused by
    /// [`Self::equip`], so it is never here).
    pub fn equipped_weapon_slot(&self) -> Option<EquipSlot> {
        [EquipSlot::RightHand, EquipSlot::LeftHand]
            .into_iter()
            .find(|slot| {
                self.get(*slot)
                    .is_some_and(|item| item.item_group == ItemGroup::Weapons)
            })
    }

    /// Shield-if-it-protects → part's armour slot (`FormulaHelper.cs:1094-1110`).
    fn armor_slot_for_struck_part(&self, struck_body_part: usize) -> Option<EquipSlot> {
        if let Some(shield) = self.get(EquipSlot::LeftHand) {
            if is_shield(shield) && shield_protected_body_parts(shield).contains(&struck_body_part)
            {
                return Some(EquipSlot::LeftHand);
            }
        }
        let slot = equip_slot_for_body_part(struck_body_part);
        (slot != EquipSlot::None).then_some(slot)
    }

    /// `SerializeEquipTable` (`ItemEquipTable.cs:331-345`): UID per slot, 0
    /// for empty slots.
    pub fn serialize_uids(&self) -> [u64; EQUIP_TABLE_LENGTH] {
        std::array::from_fn(|index| self.slots[index].as_ref().map_or(0, |item| item.uid))
    }

    /// Stable one-line summary of the occupied slots (debug/probe helper).
    pub fn dump(&self) -> String {
        let mut out = String::new();
        for index in 0..EQUIP_TABLE_LENGTH {
            if let Some(item) = &self.slots[index] {
                if !out.is_empty() {
                    out.push_str(", ");
                }
                out.push_str(&format!("slot{index}=uid{}", item.uid));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::{apply_armor_material, ItemTemplate, TOTAL_ITEMS};

    fn equippable(index: usize, stack: i32) -> Item {
        let mut template = ItemTemplate::empty(index);
        template.name = format!("item-{index}");
        template.hit_points = 100;
        template.base_weight_units = 4;
        template.base_price = 10;
        let mut item = Item::from_template(&template);
        item.stack_count = stack;
        item
    }

    fn equipped(item: EquipResult) -> EquipAttempt {
        match item {
            EquipResult::Equipped(attempt) => attempt,
            EquipResult::Rejected(item) => {
                panic!("expected equip, got rejection of template {}", item.template_index)
            }
        }
    }

    #[test]
    fn slot_indices_match_legacy_layout() {
        assert_eq!(EQUIP_TABLE_LENGTH, 27);
        assert_eq!(EquipSlot::ChestArmor as i32, 18);
        assert_eq!(EquipSlot::RightHand as i32, 19);
        assert_eq!(EquipSlot::LeftHand as i32, 21);
        assert_eq!(EquipSlot::Cloak1 as i32, 14);
        assert_eq!(EquipSlot::Cloak2 as i32, 16);
        assert_eq!(EquipSlot::None.index(), None);
        assert_eq!(EquipSlot::Feet.index(), Some(26));
    }

    #[test]
    fn hands_follow_the_weapon_table() {
        for index in [113, 114, 116, 117, 118, 119, 120, 121, 124, 127] {
            assert_eq!(item_hands(&equippable(index, 1)), ItemHands::Either, "{index}");
        }
        for index in [115, 122, 123, 125, 126, 128] {
            assert_eq!(item_hands(&equippable(index, 1)), ItemHands::Both, "{index}");
        }
        assert_eq!(item_hands(&equippable(131, 1)), ItemHands::None, "arrow");
        assert_eq!(item_hands(&equippable(102, 1)), ItemHands::None, "cuirass");
        for index in [109, 110, 111, 112] {
            assert_eq!(item_hands(&equippable(index, 1)), ItemHands::LeftOnly);
        }
        assert_eq!(item_hands(&equippable(0, 1)), ItemHands::None, "gem");
    }

    #[test]
    fn golden_equipment_vectors() {
        // GV-EQP-01: Longsword on free hands → RightHand (19).
        let mut table = ItemEquipTable::new();
        let attempt = equipped(table.equip(equippable(120, 1), true));
        assert_eq!(attempt.slot, EquipSlot::RightHand);
        assert!(attempt.unequipped.is_empty());
        assert!(attempt.remainder.is_none());

        // GV-EQP-02: Dagger with Right occupied, Left free → LeftHand (21).
        let attempt = equipped(table.equip(equippable(113, 1), true));
        assert_eq!(attempt.slot, EquipSlot::LeftHand);
        assert!(table.get(EquipSlot::RightHand).is_some());
        assert!(table.get(EquipSlot::LeftHand).is_some());

        // GV-EQP-03: Claymore leaves both hands free.
        let mut table = ItemEquipTable::new();
        let attempt = equipped(table.equip(equippable(122, 1), true));
        assert_eq!(attempt.slot, EquipSlot::RightHand);
        assert!(attempt.unequipped.is_empty());

        // GV-EQP-04: Buckler displaces the Claymore, lands in LeftHand.
        let attempt = equipped(table.equip(equippable(109, 1), true));
        assert_eq!(attempt.slot, EquipSlot::LeftHand);
        assert_eq!(attempt.unequipped.len(), 1);
        assert_eq!(attempt.unequipped[0].template_index, 122);
        assert!(table.get(EquipSlot::RightHand).is_none());
        assert_eq!(
            table.get(EquipSlot::LeftHand).map(|item| item.template_index),
            Some(109)
        );

        // GV-EQP-05: Staff displaces the Buckler.
        let attempt = equipped(table.equip(equippable(115, 1), true));
        assert_eq!(attempt.slot, EquipSlot::RightHand);
        assert_eq!(attempt.unequipped.len(), 1);
        assert_eq!(attempt.unequipped[0].template_index, 109);
        assert!(table.get(EquipSlot::LeftHand).is_none());

        // GV-EQP-06: Arrows are rejected and handed back.
        let mut table = ItemEquipTable::new();
        match table.equip(equippable(131, 1), true) {
            EquipResult::Rejected(item) => assert_eq!(item.template_index, 131),
            EquipResult::Equipped(attempt) => panic!("arrow equipped in {:?}", attempt.slot),
        }
        assert!(table.serialize_uids().iter().all(|&uid| uid == 0));

        // GV-EQP-08: Cuirass → ChestArmor (18).
        let mut table = ItemEquipTable::new();
        assert_eq!(
            equipped(table.equip(equippable(102, 1), true)).slot,
            EquipSlot::ChestArmor
        );

        // GV-EQP-09: Tower shield → LeftHand (21).
        let attempt = equipped(table.equip(equippable(112, 1), true));
        assert_eq!(attempt.slot, EquipSlot::LeftHand);

        // GV-EQP-10: Bow with switching off → Both.
        let mut table = ItemEquipTable::new();
        assert_eq!(item_hands(&equippable(129, 1)), ItemHands::Both);
        let attempt = equipped(table.equip(equippable(129, 1), true));
        assert_eq!(attempt.slot, EquipSlot::RightHand);
        assert!(table.get(EquipSlot::LeftHand).is_none());
    }

    #[test]
    fn golden_stack_split_keeps_one_equipped() {
        // GV-EQP-07: a 5-unit stack equips 1 and leaves 4 behind.
        let mut table = ItemEquipTable::new();
        let original = equippable(0, 5); // gem, stackable and equippable
        let original_uid = original.uid;
        let attempt = equipped(table.equip(original, true));
        assert_eq!(attempt.slot, EquipSlot::Crystal0);
        let remainder = attempt.remainder.expect("stack leaves a remainder");
        assert_eq!(remainder.stack_count, 4);
        assert_ne!(remainder.uid, original_uid, "split gets a fresh uid");
        assert!(!remainder.equipped);
        let equipped_item = table.get(EquipSlot::Crystal0).expect("gem equipped");
        assert_eq!(equipped_item.uid, original_uid);
        assert_eq!(equipped_item.stack_count, 1);

        // A second unit goes to the other gem slot.
        let attempt = equipped(table.equip(equippable(0, 1), true));
        assert_eq!(attempt.slot, EquipSlot::Crystal1);
    }

    #[test]
    fn two_handed_weapon_unequips_both_hands_in_source_order() {
        let mut table = ItemEquipTable::new();
        equipped(table.equip(equippable(120, 1), true)); // longsword right
        equipped(table.equip(equippable(113, 1), true)); // dagger left
        let attempt = equipped(table.equip(equippable(122, 1), true));
        assert_eq!(attempt.slot, EquipSlot::RightHand);
        let templates: Vec<usize> = attempt.unequipped.iter().map(|i| i.template_index).collect();
        assert_eq!(templates, vec![113, 120], "left before right");
        assert!(table.get(EquipSlot::LeftHand).is_none());
        assert_eq!(
            table.get(EquipSlot::RightHand).map(|i| i.template_index),
            Some(122)
        );
    }

    #[test]
    fn occupied_slot_honors_always_equip() {
        let mut table = ItemEquipTable::new();
        equipped(table.equip(equippable(120, 1), true)); // Longsword → right
        equipped(table.equip(equippable(113, 1), true)); // Dagger → left
        // Both hands populated: `GetFirstSlot` falls back to the first slot
        // (`ItemEquipTable.cs:298-306`), which is occupied, so `EquipItem`
        // returns null without `alwaysEquip` (`:135-139`).
        match table.equip(equippable(121, 1), false) {
            EquipResult::Rejected(item) => assert_eq!(item.template_index, 121),
            EquipResult::Equipped(attempt) => panic!("{:?} should be rejected", attempt.slot),
        }
        let attempt = equipped(table.equip(equippable(121, 1), true));
        assert_eq!(attempt.slot, EquipSlot::RightHand);
        assert_eq!(attempt.unequipped.len(), 1);
        assert_eq!(attempt.unequipped[0].template_index, 120);
    }

    #[test]
    fn jewelry_and_clothing_slot_resolution() {
        let table = ItemEquipTable::new();
        let cases = [
            (133, EquipSlot::Amulet0),
            (138, EquipSlot::Amulet0),
            (134, EquipSlot::Bracer0),
            (135, EquipSlot::Ring0),
            (136, EquipSlot::Bracelet0),
            (137, EquipSlot::Mark0),
            (140, EquipSlot::None), // wand
            (154, EquipSlot::Cloak2),  // casual cloak
            (191, EquipSlot::Cloak2),  // women's casual cloak
            (147, EquipSlot::Feet),    // men's shoes
            (151, EquipSlot::LegsClothes),
            (182, EquipSlot::ChestClothes),
            (277, EquipSlot::None),    // book
            (287, EquipSlot::None),    // map
        ];
        for (index, slot) in cases {
            let item = equippable(index, 1);
            assert_eq!(table.get_equip_slot(&item), slot, "template {index}");
        }
    }

    #[test]
    fn second_slot_falls_back_to_first_when_full() {
        let mut table = ItemEquipTable::new();
        assert_eq!(
            equipped(table.equip(equippable(135, 1), true)).slot,
            EquipSlot::Ring0
        );
        assert_eq!(
            equipped(table.equip(equippable(135, 1), true)).slot,
            EquipSlot::Ring1
        );
        let attempt = equipped(table.equip(equippable(135, 1), true));
        assert_eq!(attempt.slot, EquipSlot::Ring0, "both full → first slot");
        assert_eq!(attempt.unequipped.len(), 1);
    }

    #[test]
    fn unequip_serialize_and_dump() {
        let mut table = ItemEquipTable::new();
        let sword = equippable(120, 1);
        let sword_uid = sword.uid;
        equipped(table.equip(sword, true));

        let uids = table.serialize_uids();
        assert_eq!(uids.iter().filter(|&&uid| uid != 0).count(), 1);
        assert_eq!(uids[EquipSlot::RightHand.index().unwrap()], sword_uid);
        assert!(table.dump().contains(&format!("slot19=uid{sword_uid}")));

        let removed = table.unequip(EquipSlot::RightHand).expect("sword removed");
        assert_eq!(removed.uid, sword_uid);
        assert!(!removed.equipped);
        assert!(table.unequip(EquipSlot::RightHand).is_none());
        assert!(table.unequip(EquipSlot::None).is_none());
        assert_eq!(table.dump(), "");
        assert!(table.serialize_uids().iter().all(|&uid| uid == 0));
    }

    /// Armour instance with a material, as the item builder produces it
    /// (`ItemBuilder.ApplyArmorMaterial`, `ItemBuilder.cs:488-507`).
    fn armor(index: usize, native_material: i32) -> Item {
        let mut item = equippable(index, 1);
        apply_armor_material(&mut item, native_material);
        item
    }

    #[test]
    fn armor_values_project_each_slot_onto_its_body_part() {
        // FormulaHelper/DaggerfallEntity: `ArmorValues[part] -= material * 5`
        // (`DaggerfallEntity.cs:608-615`), parts Head 0 … Feet 6.
        let mut table = ItemEquipTable::new();
        assert_eq!(table.armor_values(), [0; BODY_PART_COUNT], "bare");

        // Steel cuirass: GetMaterialArmorValue(0x0201) = 9 ⇒ −45 on Chest (3).
        equipped(table.equip(armor(102, 0x0201), true));
        assert_eq!(table.armor_values(), [0, 0, 0, -45, 0, 0, 0]);

        // Leather helm (0x0000 ⇒ 3) and plate boots (0x0204 ⇒ 13).
        equipped(table.equip(armor(107, 0x0000), true));
        equipped(table.equip(armor(108, 0x0204), true));
        assert_eq!(
            table.armor_values(),
            [-15, 0, 0, -45, 0, 0, -65],
            "Head=3·5, Chest=9·5, Feet=13·5"
        );

        // Tower shield (109..=112 table ⇒ 4) covers Head, LeftArm, Hands, Legs
        // (`DaggerfallUnityItem.cs:1091-1092`), never Chest/RightArm/Feet.
        equipped(table.equip(armor(112, 0x0000), true));
        assert_eq!(
            table.armor_values(),
            [-35, 0, -20, -45, -20, -20, -65],
            "shield bonus is material-independent (`DaggerfallEntity.cs:618-620`)"
        );

        // A material outside the armour switch contributes nothing
        // (`GetMaterialArmorValue` default 0, `DaggerfallUnityItem.cs:1040-1053`).
        let before = table.armor_values();
        let mut blank = equippable(103, 1);
        blank.native_material = 0x0300;
        equipped(table.equip(blank, true)); // gloves
        assert_eq!(table.armor_values(), before, "GetMaterialArmorValue = 0");
    }

    #[test]
    fn body_part_mappings_follow_the_source() {
        for (slot, part) in [
            (EquipSlot::Head, 0),
            (EquipSlot::RightArm, 1),
            (EquipSlot::LeftArm, 2),
            (EquipSlot::ChestArmor, 3),
            (EquipSlot::Gloves, 4),
            (EquipSlot::LegsArmor, 5),
            (EquipSlot::Feet, 6),
        ] {
            assert_eq!(body_part_for_equip_slot(slot), Some(part), "{slot:?}");
            assert_eq!(equip_slot_for_body_part(part), slot);
        }
        assert_eq!(body_part_for_equip_slot(EquipSlot::RightHand), None);
        assert_eq!(body_part_for_equip_slot(EquipSlot::Cloak1), None);
        assert_eq!(equip_slot_for_body_part(BODY_PART_COUNT), EquipSlot::None);
    }

    #[test]
    fn damage_equipment_wears_the_struck_piece() {
        let mut table = ItemEquipTable::new();
        let cuirass = armor(102, 0x0200); // iron plate: 7 · 5 = 35, condition 100
        let uid = cuirass.uid;
        equipped(table.equip(cuirass, true));
        let mut dice = Dice100::new(7);

        // Damage 20 ⇒ (10·20+50)/100 = 2 wear, no re-roll possible.
        let wear = table
            .damage_equipment(3, 20, &mut dice)
            .expect("chest armour takes wear");
        assert_eq!(wear.slot, EquipSlot::ChestArmor);
        assert_eq!(wear.item_uid, uid);
        assert_eq!(wear.wear, 2);
        assert!(!wear.broke);
        assert!(wear.removed.is_none());
        assert_eq!(
            table.get(EquipSlot::ChestArmor).map(|item| item.current_condition),
            Some(98)
        );
        assert_eq!(wear.condition, 98);

        // A strike on a bare arm hits no armour and consumes no roll.
        assert!(table.damage_equipment(1, 20, &mut dice).is_none());
        // DFU only calls DamageEquipment for damage > 0 (`:1089`).
        assert!(table.damage_equipment(3, 0, &mut dice).is_none());
        assert!(table.damage_equipment(7, 20, &mut dice).is_none());
    }

    #[test]
    fn shield_takes_the_hit_when_it_protects_the_part() {
        let mut table = ItemEquipTable::new();
        equipped(table.equip(armor(102, 0x0200), true)); // cuirass
        let shield = armor(110, 0x0000); // Round shield: protects 2, 4, 5
        let shield_uid = shield.uid;
        equipped(table.equip(shield, true));
        let mut dice = Dice100::new(11);

        let wear = table
            .damage_equipment(5, 30, &mut dice) // legs are protected
            .expect("shield absorbs the hit");
        assert_eq!(wear.slot, EquipSlot::LeftHand);
        assert_eq!(wear.item_uid, shield_uid);
        assert_eq!(wear.wear, 3, "(10·30+50)/100");
        assert_eq!(
            table.get(EquipSlot::ChestArmor).map(|item| item.current_condition),
            Some(100),
            "cuirass untouched (`FormulaHelper.cs:1104-1113`)"
        );

        // RightArm is not protected by a Round shield: the arm has no armour.
        assert!(table.damage_equipment(1, 30, &mut dice).is_none());
    }

    #[test]
    fn breaking_damage_zeroes_removes_and_unequips() {
        let mut table = ItemEquipTable::new();
        let mut cuirass = armor(102, 0x0200);
        cuirass.current_condition = 3;
        let uid = cuirass.uid;
        equipped(table.equip(cuirass, true));
        let mut dice = Dice100::new(3);

        let wear = table
            .damage_equipment(3, 300, &mut dice) // wear 30 ≫ 3
            .expect("chest armour takes wear");
        assert!(wear.broke);
        assert_eq!(wear.wear, 30);
        assert_eq!(wear.condition, 0);
        assert_eq!(wear.item_uid, uid);
        let removed = wear.removed.expect("broken piece leaves the slot");
        assert_eq!(removed.uid, uid);
        assert_eq!(removed.current_condition, 0);
        assert!(!removed.equipped);
        assert!(table.get(EquipSlot::ChestArmor).is_none(), "ItemBreaks → UnequipItem");
        assert_eq!(table.armor_values(), [0; BODY_PART_COUNT], "bonus removed");
    }

    #[test]
    fn zero_wear_reroll_matches_the_dfu_dice_stream() {
        // damage 3 ⇒ (10·3+50)/100 = 0, then a 20 % re-roll of 1 point.
        // Same explicit seed ⇒ the outcome must equal `wear_amount_rolled`.
        let mut table = ItemEquipTable::new();
        equipped(table.equip(armor(102, 0x0200), true));
        let mut dice = Dice100::new(42);
        let mut expected_dice = Dice100::new(42);
        let expected = daggerfall_behavior::durability::wear_amount_rolled(3, &mut expected_dice);
        let wear = table.damage_equipment(3, 3, &mut dice).expect("wear report");
        assert_eq!(wear.wear, expected);
        assert_eq!(
            table.get(EquipSlot::ChestArmor).map(|item| item.current_condition),
            Some(100 - expected)
        );
    }

    #[test]
    fn damage_item_wears_the_weapon_in_the_attacker_hand() {
        // First hit of a Longsword (template 120) with condition 600:
        // `(10·15 + 50) / 100 = 2` (`FormulaHelper.cs:1130-1132`).
        let mut table = ItemEquipTable::new();
        let mut sword = equippable(120, 1);
        sword.current_condition = 600;
        let uid = sword.uid;
        equipped(table.equip(sword, true));
        let mut dice = Dice100::new(5);

        let wear = table
            .damage_item(EquipSlot::RightHand, 15, &mut dice)
            .expect("the wielded weapon takes wear");
        assert_eq!(wear.slot, EquipSlot::RightHand);
        assert_eq!(wear.item_uid, uid);
        assert_eq!(wear.wear, 2);
        assert_eq!(wear.condition, 598);
        assert!(!wear.broke);
        assert!(wear.removed.is_none());
        assert_eq!(
            table.get(EquipSlot::RightHand).map(|item| item.current_condition),
            Some(598)
        );
    }

    #[test]
    fn damage_item_guards_non_positive_damage_and_empty_slots() {
        let mut table = ItemEquipTable::new();
        let mut sword = equippable(120, 1);
        sword.current_condition = 600;
        equipped(table.equip(sword, true));
        let mut dice = Dice100::new(9);

        assert!(table.damage_item(EquipSlot::RightHand, 0, &mut dice).is_none());
        assert!(table.damage_item(EquipSlot::RightHand, -5, &mut dice).is_none());
        assert_eq!(
            table.get(EquipSlot::RightHand).map(|item| item.current_condition),
            Some(600),
            "no wear on damage <= 0 (`DamageEquipment` guard `:1090`)"
        );
        assert!(table.damage_item(EquipSlot::LeftHand, 20, &mut dice).is_none());
        assert!(table.damage_item(EquipSlot::None, 20, &mut dice).is_none());
    }

    #[test]
    fn weapon_break_unequips_the_attacker_hand() {
        let mut table = ItemEquipTable::new();
        let mut sword = equippable(120, 1);
        sword.current_condition = 3;
        let uid = sword.uid;
        equipped(table.equip(sword, true));
        let mut dice = Dice100::new(3);

        let wear = table
            .damage_item(EquipSlot::RightHand, 300, &mut dice) // wear 30 ≫ 3
            .expect("the wielded weapon takes wear");
        assert!(wear.broke);
        assert_eq!(wear.wear, 30);
        assert_eq!(wear.condition, 0);
        assert_eq!(wear.item_uid, uid);
        let removed = wear.removed.expect("broken weapon leaves the slot");
        assert_eq!(removed.uid, uid);
        assert_eq!(removed.current_condition, 0);
        assert!(!removed.equipped);
        assert!(
            table.get(EquipSlot::RightHand).is_none(),
            "ItemBreaks → UnequipItem"
        );
    }

    #[test]
    fn equipped_weapon_slot_prefers_the_right_hand() {
        let mut table = ItemEquipTable::new();
        assert_eq!(table.equipped_weapon_slot(), None, "empty table");

        // Longsword (template 120, `Either` hands → first open slot = right).
        equipped(table.equip(equippable(120, 1), true));
        assert_eq!(table.equipped_weapon_slot(), Some(EquipSlot::RightHand));

        // A second longsword lands in the left hand; the right one reports.
        equipped(table.equip(equippable(120, 1), true));
        assert_eq!(table.equipped_weapon_slot(), Some(EquipSlot::RightHand));

        // Shield-only: `ItemGroup::Armor` never counts as a weapon.
        let mut shields = ItemEquipTable::new();
        equipped(shields.equip(equippable(109, 1), true));
        assert_eq!(shields.equipped_weapon_slot(), None);

        // Off-hand only: two daggers (template 113, `Either`) fill right then
        // left; emptying the right one leaves the weapon in the left hand.
        let mut left = ItemEquipTable::new();
        equipped(left.equip(equippable(113, 1), true));
        equipped(left.equip(equippable(113, 1), true));
        assert_eq!(
            left.unequip(EquipSlot::RightHand).map(|item| item.template_index),
            Some(113)
        );
        assert_eq!(left.equipped_weapon_slot(), Some(EquipSlot::LeftHand));
    }

    #[test]
    // Guard tripwire on the retail constant (288 templates, spec §1.1): core
    // verified against the MIT source — not refactored into dynamic logic.
    #[allow(clippy::assertions_on_constants)]
    fn table_length_is_bounded_by_templates() {
        // Guard against a wrong slot table: the highest template indexes are
        // still resolvable without panicking.
        assert!(TOTAL_ITEMS > 287);
        let table = ItemEquipTable::new();
        assert_eq!(
            table.get_equip_slot(&equippable(287, 1)),
            EquipSlot::None,
            "maps are not equippable"
        );
    }
}

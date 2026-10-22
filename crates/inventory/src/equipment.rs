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

use crate::items::{fresh_uid, is_shield, Item, ItemGroup};

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
    use crate::items::{ItemTemplate, TOTAL_ITEMS};

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

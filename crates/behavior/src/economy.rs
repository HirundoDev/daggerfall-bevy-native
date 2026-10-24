//! Economy — SPEC-M30 (REQ-L99-063): shops, inns, banking and transport.
//!
//! Every constant and formula is transcribed from Daggerfall Unity
//! `@0038a1e61999ad2c73fb1f33b706cae9a88ec66f` (MIT):
//!
//! - stock per building type: `DaggerfallTradeWindow.cs:107-127`, window
//!   modes `:133-141`, sale filter `:694-697`
//! - `CalculateCost` + `ApplyRegionalPriceAdjustment`:
//!   `FormulaHelper.cs:1868-1884`, `:2008-2026`
//! - regional price drift and flags: `FormulaHelper.cs:2031-2070`
//! - trade price `CalculateTradePrice`: `FormulaHelper.cs:1960-1988` — its
//!   exact port already lives in the 🎮 Skills organ
//!   (`daggerfall_skills::formula::calculate_trade_price`, M5); the `app`
//!   composes both stages and this organ does **not** call another organ
//! - repair cost/time and identify cost: `FormulaHelper.cs:1885-1940`,
//!   collective repair time `DaggerfallTradeWindow.cs:550-556`
//! - holidays: `FormulaHelper.cs:1803-1840` (53 rows) and the purchase
//!   discounts `DaggerfallTradeWindow.cs:444-449`
//! - inn: `CalculateRoomCost` `FormulaHelper.cs:1842-1860`, rental limits and
//!   expiry `DaggerfallTavernWindow.cs:188,192-196,199-200,233-258`
//! - bank: `DaggerfallBankManager.cs:80-87,178-180`, accounts/loans
//!   `:328-394`, `:498-545`, houses/ships `:152-161,396-474,487-495`;
//!   overdue and reminders `LoanChecker.cs:13,17-71`
//! - transport: `DaggerfallTransportWindow.cs:197-261`, availability
//!   `TransportManager.cs:82-131`, speeds `PlayerSpeedChanger.cs:30-34,147-166`
//!
//! **Arithmetic:** all integer (C# `int` truncates toward zero — Rust `/` on
//! integers does the same), arithmetic `>>`; no banker's rounding anywhere.
//! The only `f32` values are the bank's declared float constants (`0.85`,
//! `1280`, the gold weight) and the transport speeds, exactly as in the
//! source; the gold-weight comparison uses exact 1/10 000 kg units so
//! `2000 × 0.0025 kg` is not "heavier" than 5 kg by a floating accident
//! (spec §11.6).
//!
//! **Frontiers (spec §10, not invented here):** item templates/condition
//! state (M21), real rest (M32), guild ranks/discounts (M27), crime
//! reputation and legal seizure (M28 — the loan default leaves this module
//! as the typed [`LoanCrime::LoanDefault`] fact), travel (M31), UI (M34) and
//! `ReadNativeBankData` (M33). **NEGATIVE:** no variable interest rates and
//! no inflation — the source only applies the fixed `+10 %` loan repayment
//! (`FormulaHelper.cs:2001-2008`).

use std::fmt;

// ═══════════════════════════════════════════════════════════════════════════
// §1 · Stock per building type
// ═══════════════════════════════════════════════════════════════════════════

/// `DFLocation.BuildingTypes` values that have an entry in the source's
/// `storeBuysItemType` dictionary (`DaggerfallTradeWindow.cs:107-127`).
///
/// A building type **without** an entry does not exist in the source: no
/// stock is invented for it. The field default is
/// [`BuildingType::GeneralStore`] (`DaggerfallTradeWindow.cs:92`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum BuildingType {
    /// `DFLocation.BuildingTypes.Alchemist` (`0x00`).
    Alchemist = 0,
    /// `DFLocation.BuildingTypes.Armorer` (`0x02`).
    Armorer = 2,
    /// `DFLocation.BuildingTypes.Bookseller` (`0x05`).
    Bookseller = 5,
    /// `DFLocation.BuildingTypes.ClothingStore` (`0x06`).
    ClothingStore = 6,
    /// `DFLocation.BuildingTypes.FurnitureStore` (`0x07`).
    FurnitureStore = 7,
    /// `DFLocation.BuildingTypes.GemStore` (`0x08`).
    GemStore = 8,
    /// `DFLocation.BuildingTypes.GeneralStore` (`0x09`).
    GeneralStore = 9,
    /// `DFLocation.BuildingTypes.PawnShop` (`0x0C`).
    PawnShop = 12,
    /// `DFLocation.BuildingTypes.WeaponSmith` (`0x0D`).
    WeaponSmith = 13,
}

impl BuildingType {
    /// The nine shop types of `storeBuysItemType`, in source order.
    pub const ALL: [BuildingType; 9] = [
        BuildingType::Alchemist,
        BuildingType::Armorer,
        BuildingType::Bookseller,
        BuildingType::ClothingStore,
        BuildingType::FurnitureStore,
        BuildingType::GemStore,
        BuildingType::GeneralStore,
        BuildingType::PawnShop,
        BuildingType::WeaponSmith,
    ];

    /// Retail `DFLocation.BuildingTypes` index (`None` outside the 9 shops).
    pub const fn index(self) -> u8 {
        self as u8
    }

    /// Lookup by the retail `DFLocation.BuildingTypes` index.
    pub const fn from_index(index: u8) -> Option<BuildingType> {
        match index {
            0 => Some(BuildingType::Alchemist),
            2 => Some(BuildingType::Armorer),
            5 => Some(BuildingType::Bookseller),
            6 => Some(BuildingType::ClothingStore),
            7 => Some(BuildingType::FurnitureStore),
            8 => Some(BuildingType::GemStore),
            9 => Some(BuildingType::GeneralStore),
            12 => Some(BuildingType::PawnShop),
            13 => Some(BuildingType::WeaponSmith),
            _ => None,
        }
    }

    /// Source identifier (`DFLocation.BuildingTypes` name).
    pub const fn name(self) -> &'static str {
        match self {
            BuildingType::Alchemist => "Alchemist",
            BuildingType::Armorer => "Armorer",
            BuildingType::Bookseller => "Bookseller",
            BuildingType::ClothingStore => "ClothingStore",
            BuildingType::FurnitureStore => "FurnitureStore",
            BuildingType::GemStore => "GemStore",
            BuildingType::GeneralStore => "GeneralStore",
            BuildingType::PawnShop => "PawnShop",
            BuildingType::WeaponSmith => "WeaponSmith",
        }
    }

    /// `ItemGroups` accepted when the player **sells** to this building.
    pub fn accepted_groups(self) -> &'static [ItemGroup] {
        match self {
            BuildingType::Alchemist => &[
                ItemGroup::Gems,
                ItemGroup::CreatureIngredients1,
                ItemGroup::CreatureIngredients2,
                ItemGroup::CreatureIngredients3,
                ItemGroup::PlantIngredients1,
                ItemGroup::PlantIngredients2,
                ItemGroup::MiscellaneousIngredients1,
                ItemGroup::MiscellaneousIngredients2,
                ItemGroup::MetalIngredients,
            ],
            BuildingType::Armorer | BuildingType::WeaponSmith => {
                &[ItemGroup::Armor, ItemGroup::Weapons]
            }
            BuildingType::Bookseller => &[ItemGroup::Books],
            BuildingType::ClothingStore => {
                &[ItemGroup::MensClothing, ItemGroup::WomensClothing]
            }
            BuildingType::FurnitureStore => &[ItemGroup::Furniture],
            BuildingType::GemStore => &[ItemGroup::Gems, ItemGroup::Jewellery],
            BuildingType::GeneralStore => &[
                ItemGroup::Books,
                ItemGroup::MensClothing,
                ItemGroup::WomensClothing,
                ItemGroup::Transportation,
                ItemGroup::Jewellery,
                ItemGroup::Weapons,
                ItemGroup::UselessItems2,
            ],
            BuildingType::PawnShop => &[
                ItemGroup::Armor,
                ItemGroup::Books,
                ItemGroup::MensClothing,
                ItemGroup::WomensClothing,
                ItemGroup::Gems,
                ItemGroup::Jewellery,
                ItemGroup::ReligiousItems,
                ItemGroup::Weapons,
                ItemGroup::UselessItems2,
                ItemGroup::Paintings,
            ],
        }
    }

    /// Does this building buy `group` when the player sells?
    pub fn accepts(self, group: ItemGroup) -> bool {
        self.accepted_groups().contains(&group)
    }
}

/// `DFU:ItemGroups` members referenced by `storeBuysItemType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemGroup {
    /// `ItemGroups.Gems`.
    Gems,
    /// `ItemGroups.CreatureIngredients1`.
    CreatureIngredients1,
    /// `ItemGroups.CreatureIngredients2`.
    CreatureIngredients2,
    /// `ItemGroups.CreatureIngredients3`.
    CreatureIngredients3,
    /// `ItemGroups.PlantIngredients1`.
    PlantIngredients1,
    /// `ItemGroups.PlantIngredients2`.
    PlantIngredients2,
    /// `ItemGroups.MiscellaneousIngredients1`.
    MiscellaneousIngredients1,
    /// `ItemGroups.MiscellaneousIngredients2`.
    MiscellaneousIngredients2,
    /// `ItemGroups.MetalIngredients`.
    MetalIngredients,
    /// `ItemGroups.Armor`.
    Armor,
    /// `ItemGroups.Weapons`.
    Weapons,
    /// `ItemGroups.Books`.
    Books,
    /// `ItemGroups.MensClothing`.
    MensClothing,
    /// `ItemGroups.WomensClothing`.
    WomensClothing,
    /// `ItemGroups.Furniture`.
    Furniture,
    /// `ItemGroups.Jewellery`.
    Jewellery,
    /// `ItemGroups.Transportation`.
    Transportation,
    /// `ItemGroups.UselessItems2`.
    UselessItems2,
    /// `ItemGroups.ReligiousItems`.
    ReligiousItems,
    /// `ItemGroups.Paintings`.
    Paintings,
}

impl ItemGroup {
    /// Source identifier.
    pub const fn name(self) -> &'static str {
        match self {
            ItemGroup::Gems => "Gems",
            ItemGroup::CreatureIngredients1 => "CreatureIngredients1",
            ItemGroup::CreatureIngredients2 => "CreatureIngredients2",
            ItemGroup::CreatureIngredients3 => "CreatureIngredients3",
            ItemGroup::PlantIngredients1 => "PlantIngredients1",
            ItemGroup::PlantIngredients2 => "PlantIngredients2",
            ItemGroup::MiscellaneousIngredients1 => "MiscellaneousIngredients1",
            ItemGroup::MiscellaneousIngredients2 => "MiscellaneousIngredients2",
            ItemGroup::MetalIngredients => "MetalIngredients",
            ItemGroup::Armor => "Armor",
            ItemGroup::Weapons => "Weapons",
            ItemGroup::Books => "Books",
            ItemGroup::MensClothing => "MensClothing",
            ItemGroup::WomensClothing => "WomensClothing",
            ItemGroup::Furniture => "Furniture",
            ItemGroup::Jewellery => "Jewellery",
            ItemGroup::Transportation => "Transportation",
            ItemGroup::UselessItems2 => "UselessItems2",
            ItemGroup::ReligiousItems => "ReligiousItems",
            ItemGroup::Paintings => "Paintings",
        }
    }
}

/// `DaggerfallTradeWindow.WindowModes` (`:133-141`) minus `Inventory`.
///
/// The source comments `Inventory` as *"Should never get used, treat as
/// 'none'"*: it is the neutral window state, not a trade mode. The five
/// trade modes are the ones that price or filter items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeMode {
    /// Player sells to the merchant (`GetTradePrice(…, selling=true)`).
    Sell,
    /// Player buys (`GetTradePrice(…, selling=false)`).
    Buy,
    /// Repair quote (`selling=false`).
    Repair,
    /// Identify quote (no haggling: `GetTradePrice` returns `cost`).
    Identify,
    /// Player sells magic items (`selling=true`, requires `IsEnchanted`).
    SellMagic,
}

impl TradeMode {
    /// The five trade modes of `WindowModes`, in source order.
    pub const ALL: [TradeMode; 5] = [
        TradeMode::Sell,
        TradeMode::Buy,
        TradeMode::Repair,
        TradeMode::Identify,
        TradeMode::SellMagic,
    ];

    /// `GetTradePrice` direction (`DaggerfallTradeWindow.cs:492-508`):
    /// `Some(true)` = selling, `Some(false)` = buying, `None` = no haggling.
    pub const fn selling(self) -> Option<bool> {
        match self {
            TradeMode::Sell | TradeMode::SellMagic => Some(true),
            TradeMode::Buy | TradeMode::Repair => Some(false),
            TradeMode::Identify => None,
        }
    }

    /// Source identifier.
    pub const fn name(self) -> &'static str {
        match self {
            TradeMode::Sell => "Sell",
            TradeMode::Buy => "Buy",
            TradeMode::Repair => "Repair",
            TradeMode::Identify => "Identify",
            TradeMode::SellMagic => "SellMagic",
        }
    }
}

/// Sale-list filter for the player's own items (`DaggerfallTradeWindow.cs:694-697`).
///
/// Only `Sell` (group accepted by the building) and `SellMagic`
/// (`item.IsEnchanted`) filter a sale list; the other modes have no sale
/// list and return `false` (documented, not a rule).
pub fn sale_offer(building: BuildingType, mode: TradeMode, group: ItemGroup, enchanted: bool) -> bool {
    match mode {
        TradeMode::Sell => building.accepts(group),
        TradeMode::SellMagic => enchanted,
        TradeMode::Buy | TradeMode::Repair | TradeMode::Identify => false,
    }
}

/// One offered item of a shop (declared content; `value` is the DFU
/// `DaggerfallUnityItem.value` input of `CalculateCost`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShopItem {
    /// Stable id used by the trade session.
    pub id: &'static str,
    /// Visible name.
    pub name: &'static str,
    /// DFU `ItemGroup` (sale filter).
    pub group: ItemGroup,
    /// Base value (`item.value`).
    pub base_value: i64,
    /// Current condition (M21 owns the wear; this is the input value).
    pub condition: i32,
    /// Maximum condition.
    pub max_condition: i32,
    /// `item.IsEnchanted` (SellMagic filter).
    pub enchanted: bool,
    /// `item.IsIdentified` (Identify quote filter).
    pub identified: bool,
    /// Units on offer.
    pub stack: i32,
}

/// Shop catalog: building type, quality (0..20) and regional adjustment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shop {
    /// `DFLocation.BuildingTypes` of the shop.
    pub building: BuildingType,
    /// `buildingDiscoveryData.quality` (`0..=20`).
    pub quality: u8,
    /// `RegionData.PriceAdjustment` when the player has a location.
    pub price_adjustment: Option<u16>,
    /// Offered items.
    pub items: Vec<ShopItem>,
}

impl Shop {
    /// Catalog cost of one item (`CalculateCost(value, quality) × stack`,
    /// `DaggerfallTradeWindow.cs:443`).
    pub fn item_cost(&self, item: &ShopItem) -> i64 {
        calculate_cost(item.base_value, self.quality, self.price_adjustment) * i64::from(item.stack)
    }

    /// Total catalog cost of the basket (`UpdateCostAndGold`).
    pub fn basket_cost(&self, items: &[&ShopItem]) -> i64 {
        items.iter().map(|item| self.item_cost(item)).sum()
    }

    /// Items the player can sell in `mode` (`sale_offer` per item).
    pub fn sellable(&self, mode: TradeMode) -> Vec<&ShopItem> {
        self.items
            .iter()
            .filter(|item| sale_offer(self.building, mode, item.group, item.enchanted))
            .collect()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// §2 · Price: CalculateCost, regional adjustment and drift
// ═══════════════════════════════════════════════════════════════════════════

/// `PriceAdjustment` base (`FormulaHelper.cs:2025`: divisor `/ 1000`).
pub const PRICE_ADJUSTMENT_BASE: u16 = 1000;
/// Minimum `PriceAdjustment` (`FormulaHelper.cs:2058` clamp).
pub const PRICE_ADJUSTMENT_MIN: u16 = 250;
/// Maximum `PriceAdjustment` (`FormulaHelper.cs:2058` clamp).
pub const PRICE_ADJUSTMENT_MAX: u16 = 4000;
/// Initial random draw range, upper bound (`Random.Range(0, 500 + 1)`,
/// `FormulaHelper.cs:2034`).
pub const INITIAL_PRICE_DRAW_MAX: u16 = 500;
/// Initial `PriceAdjustment` floor: the draw is offset by 750, so the retail
/// range is **750..1250** (`FormulaHelper.cs:2034`).
pub const INITIAL_PRICE_OFFSET: u16 = 750;

/// `ApplyRegionalPriceAdjustment` (`FormulaHelper.cs:2008-2026`):
/// `max(cost * PriceAdjustment / 1000, 1)`; `None` = no current location and
/// the cost is returned untouched (`:2020-2023`).
pub fn apply_regional_price_adjustment(cost: i64, adjustment: Option<u16>) -> i64 {
    match adjustment {
        None => cost,
        Some(adjustment) => (cost * i64::from(adjustment) / 1000).max(1),
    }
}

/// `CalculateCost` (`FormulaHelper.cs:1868-1884`):
/// `c = max(base, 1)` → regional adjustment → `2 * (c*(q-10)/100 + c)`,
/// integer division truncated toward zero.
pub fn calculate_cost(base_value: i64, shop_quality: u8, adjustment: Option<u16>) -> i64 {
    let cost = base_value.max(1);
    let cost = apply_regional_price_adjustment(cost, adjustment);
    2 * (cost * (i64::from(shop_quality) - 10) / 100 + cost)
}

/// `RandomizeInitialRegionalPrices` value for one draw in `0..=500`
/// (`FormulaHelper.cs:2031-2035`): `draw + 750` ⇒ 750..1250.
pub fn initial_regional_price(draw: u16) -> u16 {
    INITIAL_PRICE_OFFSET + draw.min(INITIAL_PRICE_DRAW_MAX)
}

/// `chanceOfPriceRise` of `UpdateRegionalPrices` (`FormulaHelper.cs:2051-2056`):
/// `(merchants.power - region.power)/5 + 50 - (adj - 1000)/25`.
pub fn chance_of_price_rise(merchants_power: i64, region_power: i64, adjustment: u16) -> i64 {
    (merchants_power - region_power) / 5 + 50 - (i64::from(adjustment) - 1000) / 25
}

/// One `UpdateRegionalPrices` step: `Dice100.FailedRoll(chance)` ⇒
/// `49*adj/50`, otherwise `51*adj/50`; clamped to `250..=4000`
/// (`FormulaHelper.cs:2051-2058`).
pub fn update_regional_price(adjustment: u16, failed_roll: bool) -> u16 {
    let scaled = if failed_roll {
        49 * i64::from(adjustment) / 50
    } else {
        51 * i64::from(adjustment) / 50
    };
    scaled.clamp(
        i64::from(PRICE_ADJUSTMENT_MIN),
        i64::from(PRICE_ADJUSTMENT_MAX),
    ) as u16
}

/// Regional price flag (`FormulaHelper.cs:2059-2070`): `PricesHigh` when
/// `> 2000`, `PricesLow` when `< 500`, and both flags are turned **off** for
/// the `[500, 2000]` band.
pub fn price_flag(adjustment: u16) -> Option<PriceFlag> {
    if adjustment > 2000 {
        Some(PriceFlag::High)
    } else if adjustment < 500 {
        Some(PriceFlag::Low)
    } else {
        None
    }
}

/// `PlayerEntity.RegionDataFlags` price flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceFlag {
    /// `PricesHigh` (`adjustment > 2000`).
    High,
    /// `PricesLow` (`adjustment < 500`).
    Low,
}

// ═══════════════════════════════════════════════════════════════════════════
// §2.3 · Repair and identify
// ═══════════════════════════════════════════════════════════════════════════

// `calculate_trade_price`'s second stage is the Skills organ; this module
// owns the catalog stage only (see the module doc).

/// `CalculateItemRepairCost` (`FormulaHelper.cs:1885-1906`) without the
/// guild discount (M27 frontier): `0` when already repaired, otherwise
/// `CalculateCost(max(10*value/100, 1), quality)`.
pub fn repair_cost(
    base_item_value: i64,
    shop_quality: u8,
    condition: i32,
    max_condition: i32,
    adjustment: Option<u16>,
) -> i64 {
    if condition == max_condition {
        return 0;
    }
    let cost = (10 * base_item_value / 100).max(1);
    calculate_cost(cost, shop_quality, adjustment)
}

/// `CalculateItemRepairTime` (`FormulaHelper.cs:1908-1917`):
/// `max((max - condition) * 86400 / 1000, 86400)` seconds. No rounding to
/// days: 4.5 days is a valid result.
pub fn repair_time_seconds(condition: i32, max_condition: i32) -> i64 {
    let damage = i64::from(max_condition) - i64::from(condition);
    (damage * 86_400 / 1000).max(86_400)
}

/// Collective repair time (`DaggerfallTradeWindow.cs:550-556`):
/// `longest + (total - longest) / 2` for the slowest item.
pub fn collective_repair_seconds(longest: i64, total: i64) -> i64 {
    longest + (total - longest) / 2
}

/// `CalculateItemIdentifyCost` (`FormulaHelper.cs:1919-1940`):
/// `(25 * value) >> 8`, free on `Witches_Festival`; **no** regional
/// adjustment and no floor (the source does not clamp).
pub fn identify_cost(base_item_value: i64, holiday_id: i32) -> i64 {
    if holiday_id == holidays::WITCHES_FESTIVAL {
        return 0;
    }
    (25 * base_item_value) >> 8
}

// ═══════════════════════════════════════════════════════════════════════════
// §3 · Holidays
// ═══════════════════════════════════════════════════════════════════════════

/// Holiday ids and purchase-discount rules.
pub mod holidays {
    /// `DFLocation.Holidays` values used by M30.
    pub mod ids {
        /// `South_Winds_Prayer` (free cure — M27 consumes it).
        pub const SOUTH_WINDS_PRAYER: i32 = 4;
        /// `Hearts_Day` (`CalculateRoomCost` free night).
        pub const HEARTS_DAY: i32 = 10;
        /// `First_Harvest` (free cure — M27).
        pub const FIRST_HARVEST: i32 = 13;
        /// `Second_Harvest` (free cure — M27).
        pub const SECOND_HARVEST: i32 = 21;
        /// `Merchants_Festival` (half price: normal shops).
        pub const MERCHANTS_FESTIVAL: i32 = 29;
        /// `Tales_and_Tallow` (half price: Mages Guild).
        pub const TALES_AND_TALLOW: i32 = 38;
        /// `Witches_Festival` (free identification).
        pub const WITCHES_FESTIVAL: i32 = 43;
        /// `Warriors_Festival` (half price: weapons in normal shops).
        pub const WARRIORS_FESTIVAL: i32 = 49;
        /// `North_Winds_Festival` (half cure price — M27).
        pub const NORTH_WINDS_FESTIVAL: i32 = 50;
    }
    pub use ids::*;

    /// Minutes in one classic year (`360 × 1440`, `FormulaHelper.cs:1825`).
    pub const MINUTES_PER_YEAR: u64 = 518_400;
    /// Minutes in one classic day.
    pub const MINUTES_PER_DAY: u64 = 1440;
    /// Last day that can celebrate a holiday (`:1828`).
    pub const LAST_HOLIDAY_DAY: u32 = 355;

    /// Regions celebrating each holiday; `0xFF` = all regions
    /// (`FormulaHelper.cs:1807-1812`).
    pub const REGION: [u8; 53] = [
        0xFF, 0x19, 0x01, 0xFF, 0x1D, 0x05, 0x19, 0x06, 0x3C, 0xFF, 0x29, 0x1A, 0xFF, 0x02, 0x19,
        0x01, 0x0E, 0x12, 0x14, 0xFF, 0xFF, 0x1C, 0x21, 0x1F, 0x2C, 0xFF, 0x12, 0x23, 0xFF, 0x38,
        0xFF, 0x01, 0x30, 0x29, 0x0B, 0x16, 0xFF, 0xFF, 0x11, 0x17, 0x14, 0x01, 0xFF, 0x13, 0xFF,
        0x33, 0x3C, 0x2E, 0xFF, 0xFF, 0x01, 0x2D, 0x18,
    ];

    /// Day of the year of each holiday (`FormulaHelper.cs:1815-1822`).
    pub const DAY: [u16; 53] = [
        0x01, 0x02, 0x0C, 0x0F, 0x10, 0x12, 0x20, 0x23, 0x26, 0x2E, 0x39, 0x3A, 0x43, 0x45, 0x55,
        0x56, 0x5B, 0x67, 0x6E, 0x76, 0x7F, 0x81, 0x8C, 0x96, 0x97, 0xA6, 0xAD, 0xAE, 0xBE, 0xC0,
        0xC8, 0xD1, 0xD4, 0xDD, 0xE0, 0xE7, 0xED, 0xF3, 0xF6, 0xFC, 0x103, 0x113, 0x11B, 0x125,
        0x12C, 0x12F, 0x134, 0x13E, 0x140, 0x159, 0x15C, 0x162, 0x163,
    ];

    /// `GetHolidayId(gameMinutes, regionIndex)` (`FormulaHelper.cs:1803-1840`).
    ///
    /// `dayOfYear = gameMinutes % 518400 / 1440 + 1`; only `dayOfYear <= 355`
    /// is evaluated; returns `index + 1` or `0` (not a holiday). No
    /// extrapolation outside the table.
    pub fn get_holiday_id(game_minutes: u64, region_index: i32) -> i32 {
        if region_index < 0 {
            return 0;
        }
        let day_of_year = (game_minutes % MINUTES_PER_YEAR / MINUTES_PER_DAY + 1) as u32;
        if day_of_year > LAST_HOLIDAY_DAY {
            return 0;
        }
        let region_one_based = (region_index + 1) as u32;
        for index in 0..REGION.len() {
            let region = REGION[index];
            if (region == 0xFF || u32::from(region) == region_one_based)
                && day_of_year == u32::from(DAY[index])
            {
                return index as i32 + 1;
            }
        }
        0
    }

    /// Half-price purchase rule (`DaggerfallTradeWindow.cs:444-449`):
    /// Merchants Festival in a normal shop, Tales and Tallows in the Mages
    /// Guild, or Warriors Festival for weapons in a normal shop.
    pub fn halves_purchase(
        holiday_id: i32,
        in_guild: bool,
        mages_guild: bool,
        group: super::ItemGroup,
    ) -> bool {
        (holiday_id == MERCHANTS_FESTIVAL && !in_guild)
            || (holiday_id == TALES_AND_TALLOW && mages_guild)
            || (holiday_id == WARRIORS_FESTIVAL
                && !in_guild
                && group == super::ItemGroup::Weapons)
    }

    /// Basket price with the holiday discount applied **before** the trade
    /// price stage (`itemPrice /= 2` per item, `:444-449`).
    pub fn discounted_basket_price(
        catalog_cost: i64,
        holiday_id: i32,
        in_guild: bool,
        mages_guild: bool,
        group: super::ItemGroup,
    ) -> i64 {
        if halves_purchase(holiday_id, in_guild, mages_guild, group) {
            catalog_cost / 2
        } else {
            catalog_cost
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// §4 · Inn / room rental (M32 consumes the expiry)
// ═══════════════════════════════════════════════════════════════════════════

/// Gold per night (`FormulaHelper.cs:1857-1859`).
pub const ROOM_COST_PER_DAY: i64 = 7;
/// Heart's Day day of the year (`:1855`).
pub const HEARTS_DAY_OF_YEAR: i64 = 46;
/// Maximum rentable window (`DaggerfallTavernWindow.cs:188`).
pub const MAX_RENT_DAYS: i64 = 350;
/// Seconds per day (`DaggerfallDateTime.SecondsPerDay = 86400`).
pub const SECONDS_PER_DAY: u64 = 86_400;

/// `CalculateRoomCost(daysToRent)` (`FormulaHelper.cs:1842-1860`):
/// `7 × days`, minus one night when the rental covers Heart's Day (day 46).
pub fn room_cost(days_to_rent: i64, day_of_year: i64) -> i64 {
    if day_of_year <= HEARTS_DAY_OF_YEAR && day_of_year + days_to_rent > HEARTS_DAY_OF_YEAR {
        ROOM_COST_PER_DAY * (days_to_rent - 1)
    } else {
        ROOM_COST_PER_DAY * days_to_rent
    }
}

/// Rental limit (`DaggerfallTavernWindow.cs:188`):
/// `daysToRent + daysAlreadyRented > 350` ⇒ refusal.
pub fn room_rental_allowed(days_to_rent: i64, days_already_rented: i64) -> bool {
    days_to_rent + days_already_rented <= MAX_RENT_DAYS
}

/// `RoomRental_v1.expiryTime = now + 86400 × days`
/// (`DaggerfallTavernWindow.cs:253`).
pub fn room_expiry_seconds(now_seconds: u64, days: i64) -> u64 {
    now_seconds + SECONDS_PER_DAY * u64::try_from(days.max(0)).unwrap_or(0)
}

/// Hours left on a rental (M32's `GetRemainingHours` input): expiry minus
/// now, in whole hours, never negative.
pub fn room_remaining_hours(now_seconds: u64, expiry_seconds: u64) -> u64 {
    expiry_seconds.saturating_sub(now_seconds) / 3600
}

// ═══════════════════════════════════════════════════════════════════════════
// §5 · Banking
// ═══════════════════════════════════════════════════════════════════════════

/// `DaggerfallBankManager.cs:83`: `DaysPerYear × MinutesPerDay = 360 × 1440`.
pub const LOAN_REPAY_MINUTES: i64 = 518_400;
/// One classic month in minutes (`LoanChecker.cs:13`: 30 × 1440).
pub const MINUTES_PER_MONTH: i64 = 43_200;
/// `loanMaxPerLevel` (`DaggerfallBankManager.cs:178`).
pub const LOAN_MAX_PER_LEVEL: i64 = 50_000;
/// Minimum loan request (`DaggerfallBankManager.cs:534`).
pub const MIN_LOAN: i64 = 100;
/// Minimum letter of credit (`DaggerfallBankManager.cs:389`).
pub const MIN_LOC: i64 = 100;
/// `goldUnitWeightInKg` (`DaggerfallBankManager.cs:80`).
pub const GOLD_UNIT_WEIGHT_KG: f32 = 0.0025;
/// Gold weight in 1/10 000 kg units (0.0025 kg × 10 000 = 25 units/coin).
///
/// **Declared delta:** the source compares `float` weights; this engine
/// compares the exact fixed-point equivalent so E35 (`70 + 5.0 = 75`, not
/// heavy) cannot flip on a float rounding accident (spec §11.6).
pub const GOLD_WEIGHT_UNITS_PER_PIECE: i64 = 25;
/// Weight units per kilogram (fixed-point scale of the comparison).
pub const WEIGHT_UNITS_PER_KG: i64 = 10_000;
/// `deedSellMult` (`DaggerfallBankManager.cs:81`).
pub const DEED_SELL_MULT: f32 = 0.85;
/// `housePriceMult` (`DaggerfallBankManager.cs:82`).
pub const HOUSE_PRICE_MULT: f32 = 1280.0;
/// `locCommission` (`DaggerfallBankManager.cs:180`): `1.01`.
///
/// The source declares it `double`, and the difference is observable:
/// `int(100 × 1.01)` is `101` in `f64` but `100` in `f32`.
pub const LOC_COMMISSION: f64 = 1.01;
/// Ship prices `{small, large}` (`DaggerfallBankManager.cs:87`).
pub const SHIP_PRICES: [i64; 2] = [100_000, 200_000];
/// Default region count (`MapFileReader.RegionCount`, `DaggerfallBankManager.cs:213`).
pub const REGION_COUNT: usize = 62;
/// Months at which the overdue reminder fires (`LoanChecker.cs:36`).
pub const REMINDER_MONTHS: [i64; 3] = [6, 3, 1];

/// `ShipType` (`DaggerfallBankManager.cs:71-76`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShipType {
    /// Small ship (price index 0).
    Small = 0,
    /// Large ship (price index 1).
    Large = 1,
}

impl ShipType {
    /// Price (`GetShipPrice`, `:96`).
    pub fn price(self) -> i64 {
        SHIP_PRICES[self as usize]
    }

    /// Sale price (`GetShipSellPrice`, `:97`): `int(price × 0.85)`.
    pub fn sell_price(self) -> i64 {
        (self.price() as f32 * DEED_SELL_MULT) as i64
    }

    /// Source identifier.
    pub const fn name(self) -> &'static str {
        match self {
            ShipType::Small => "Small",
            ShipType::Large => "Large",
        }
    }
}

/// House price (`GetHousePrice`, `DaggerfallBankManager.cs:152-159`):
/// `int(meshRadius × 1280)`.
pub fn house_price(mesh_radius: f32) -> i64 {
    (mesh_radius * HOUSE_PRICE_MULT) as i64
}

/// House sale price (`GetHouseSellPrice`, `:161`): `int(price × 0.85)`.
pub fn house_sell_price(mesh_radius: f32) -> i64 {
    (house_price(mesh_radius) as f32 * DEED_SELL_MULT) as i64
}

/// Bank transaction error (`TransactionResult`, `DaggerfallBankManager.cs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BankError {
    /// `NOT_ENOUGH_GOLD`.
    NotEnoughGold,
    /// `NOT_ENOUGH_ACCOUNT`.
    NotEnoughAccount,
    /// `TOO_HEAVY` (withdrawal over the encumbrance limit).
    TooHeavy,
    /// `NOT_ENOUGH_ACCOUNT_LOC`.
    NotEnoughAccountLoc,
    /// `LOC_REQUEST_TOO_SMALL`.
    LocRequestTooSmall,
    /// `LOAN_REQUEST_TOO_LOW`.
    LoanRequestTooLow,
    /// `LOAN_REQUEST_TOO_HIGH`.
    LoanRequestTooHigh,
    /// `%` region outside the ledger.
    UnknownRegion,
}

impl fmt::Display for BankError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            BankError::NotEnoughGold => "NOT_ENOUGH_GOLD",
            BankError::NotEnoughAccount => "NOT_ENOUGH_ACCOUNT",
            BankError::TooHeavy => "TOO_HEAVY",
            BankError::NotEnoughAccountLoc => "NOT_ENOUGH_ACCOUNT_LOC",
            BankError::LocRequestTooSmall => "LOC_REQUEST_TOO_SMALL",
            BankError::LoanRequestTooLow => "LOAN_REQUEST_TOO_LOW",
            BankError::LoanRequestTooHigh => "LOAN_REQUEST_TOO_HIGH",
            BankError::UnknownRegion => "UNKNOWN_REGION",
        };
        f.write_str(name)
    }
}

/// Loan lifecycle events emitted by the ledger.
///
/// Exactly five (`PROBE-BANK-001` B): borrowing, an upcoming-due reminder,
/// the overdue transition, the single default, and a repayment that clears
/// the debt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoanEvent {
    /// A loan was taken (`BorrowLoan`).
    Borrow,
    /// A reminder crossed one of [`REMINDER_MONTHS`].
    Due,
    /// `loanDueDate < now`.
    Overdue,
    /// First default: `hasDefaulted` was set.
    Default,
    /// A repayment cleared the debt.
    Repay,
}

impl LoanEvent {
    /// Source-independent name used by the probes.
    pub const fn name(self) -> &'static str {
        match self {
            LoanEvent::Borrow => "Borrow",
            LoanEvent::Due => "Due",
            LoanEvent::Overdue => "Overdue",
            LoanEvent::Default => "Default",
            LoanEvent::Repay => "Repay",
        }
    }
}

/// Typed fact leaving this organ for the 🔨 Anticuerpos/law path: M28 owns
/// the reputation drop and any legal seizure (spec §10.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoanCrime {
    /// `PlayerEntity.Crimes.LoanDefault`.
    LoanDefault,
}

/// Player purse: gold pieces, wagon gold, letters of credit and the weight
/// inputs of the withdrawal check. NUMERIC-never-FLOAT for gold.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Purse {
    /// Carried gold pieces.
    pub gold_pieces: i64,
    /// Gold stored in the wagon (`WagonItems` gold stack).
    pub wagon_gold: i64,
    /// `Letter_of_credit` values, in inventory order.
    pub letters_of_credit: Vec<i64>,
    /// Carried weight in 1/10 000 kg units.
    pub carried_weight_units: i64,
    /// `MaxEncumbrance` in 1/10 000 kg units.
    pub max_encumbrance_units: i64,
}

impl Purse {
    /// Purse with `gold` pieces and no other holdings.
    pub fn new(gold_pieces: i64) -> Self {
        Self {
            gold_pieces,
            ..Self::default()
        }
    }

    /// `PlayerEntity.GetGoldAmount()` (`:1313-1316`): gold pieces + LOC.
    /// The wagon is **not** part of it.
    pub fn available_gold(&self) -> i64 {
        self.gold_pieces + self.letters_of_credit.iter().sum::<i64>()
    }

    /// `PlayerEntity.DeductGoldAmount` (`:1324-1354`): pieces first, then
    /// letters of credit, then pieces again; returns the unpaid remainder.
    pub fn deduct_gold(&mut self, amount: i64) -> i64 {
        if amount <= self.gold_pieces {
            self.gold_pieces -= amount;
            return 0;
        }
        let mut remaining = amount;
        while remaining > 0 {
            let Some(first) = self.letters_of_credit.first().copied() else {
                break;
            };
            if remaining < first {
                self.letters_of_credit[0] = first - remaining;
                remaining = 0;
            } else {
                remaining -= first;
                self.letters_of_credit.remove(0);
            }
        }
        if remaining > 0 {
            if remaining <= self.gold_pieces {
                self.gold_pieces -= remaining;
            } else {
                remaining -= self.gold_pieces;
                self.gold_pieces = 0;
                return remaining;
            }
        }
        0
    }

    /// Weight in the fixed-point units used by the withdrawal check.
    pub fn carried_units(&self) -> i64 {
        self.carried_weight_units
    }
}

/// Convert kilograms to the exact fixed-point weight units.
pub fn weight_units_from_kg(kg: f64) -> i64 {
    (kg * WEIGHT_UNITS_PER_KG as f64).round() as i64
}

/// Per-region bank record (`BankRecordData_v1`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BankAccount {
    /// Region index.
    pub region_index: usize,
    /// `accountGold`.
    pub account_gold: i64,
    /// `loanTotal` (principal + the fixed 10 %).
    pub loan_total: i64,
    /// `loanDueDate` in classic minutes (`0` = no loan).
    pub loan_due: i64,
    /// `hasDefaulted` (set once).
    pub has_defaulted: bool,
}

impl BankAccount {
    fn new(region_index: usize) -> Self {
        Self {
            region_index,
            account_gold: 0,
            loan_total: 0,
            loan_due: 0,
            has_defaulted: false,
        }
    }
}

/// Owned house in one region (`HouseData_v1`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HouseRecord {
    /// `buildingKey` assigned on purchase.
    pub building_key: i64,
    /// Mesh radius used to re-derive the deed price on sale.
    pub mesh_radius: f32,
}

/// Result of a loan repayment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepayOutcome {
    /// Amount actually applied to the debt.
    pub paid: i64,
    /// Debt reached zero (`loanDueDate` was reset).
    pub cleared: bool,
    /// The request exceeded the debt and was clipped (`OVERPAID_LOAN`).
    pub overpaid: bool,
}

/// Bank ledger: **per-region** accounts, houses and the owned ship.
///
/// Accounts are not global (`long GetAccountTotal(int regionIndex)`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BankLedger {
    accounts: Vec<BankAccount>,
    houses: Vec<Option<HouseRecord>>,
    owned_ship: Option<ShipType>,
    events: Vec<LoanEvent>,
}

impl BankLedger {
    /// Ledger with one empty account per region.
    pub fn new(region_count: usize) -> Self {
        Self {
            accounts: (0..region_count).map(BankAccount::new).collect(),
            houses: vec![None; region_count],
            owned_ship: None,
            events: Vec::new(),
        }
    }

    /// Region count.
    pub fn region_count(&self) -> usize {
        self.accounts.len()
    }

    /// Account view.
    pub fn account(&self, region: usize) -> Result<&BankAccount, BankError> {
        self.accounts.get(region).ok_or(BankError::UnknownRegion)
    }

    /// Gold in the region's account (`GetAccountTotal`).
    pub fn account_gold(&self, region: usize) -> Result<i64, BankError> {
        Ok(self.account(region)?.account_gold)
    }

    /// Outstanding loan (`GetLoanedTotal`).
    pub fn loan_total(&self, region: usize) -> Result<i64, BankError> {
        Ok(self.account(region)?.loan_total)
    }

    /// Loan due date (`GetLoanDueDate`, classic minutes).
    pub fn loan_due(&self, region: usize) -> Result<i64, BankError> {
        Ok(self.account(region)?.loan_due)
    }

    /// `HasLoan`.
    pub fn has_loan(&self, region: usize) -> Result<bool, BankError> {
        Ok(self.account(region)?.loan_total > 0)
    }

    /// `HasDefaulted`.
    pub fn has_defaulted(&self, region: usize) -> Result<bool, BankError> {
        Ok(self.account(region)?.has_defaulted)
    }

    /// Owned ship (`None` when the player owns none).
    pub fn owned_ship(&self) -> Option<ShipType> {
        self.owned_ship
    }

    /// Owned house of a region.
    pub fn owned_house(&self, region: usize) -> Option<HouseRecord> {
        self.houses.get(region).copied().flatten()
    }

    /// Emitted events, in order.
    pub fn events(&self) -> &[LoanEvent] {
        &self.events
    }

    /// Drains the event mailbox (bus-only: the caller publishes them).
    pub fn take_events(&mut self) -> Vec<LoanEvent> {
        std::mem::take(&mut self.events)
    }

    fn account_mut(&mut self, region: usize) -> Result<&mut BankAccount, BankError> {
        self.accounts.get_mut(region).ok_or(BankError::UnknownRegion)
    }

    /// `DepositGold` (`DaggerfallBankManager.cs:328-350`).
    ///
    /// `amount > GoldPieces + wagonGold` ⇒ `NOT_ENOUGH_GOLD` and **0
    /// mutation**. Otherwise the account is credited and the purse debited
    /// (pieces first, then the wagon remainder).
    pub fn deposit_gold(
        &mut self,
        region: usize,
        amount: i64,
        purse: &mut Purse,
    ) -> Result<(), BankError> {
        let account = self.account_mut(region)?;
        if amount > purse.gold_pieces + purse.wagon_gold {
            return Err(BankError::NotEnoughGold);
        }
        account.account_gold += amount;
        if amount > purse.gold_pieces {
            purse.wagon_gold -= amount - purse.gold_pieces;
            purse.gold_pieces = 0;
        } else {
            purse.gold_pieces -= amount;
        }
        Ok(())
    }

    /// `WithdrawGold` (`:351-364`): account check first, then the weight
    /// check; **0 mutation** on error.
    pub fn withdraw_gold(
        &mut self,
        region: usize,
        amount: i64,
        purse: &mut Purse,
    ) -> Result<(), BankError> {
        let account = self.account_mut(region)?;
        if amount > account.account_gold {
            return Err(BankError::NotEnoughAccount);
        }
        if purse.carried_weight_units + amount * GOLD_WEIGHT_UNITS_PER_PIECE
            > purse.max_encumbrance_units
        {
            return Err(BankError::TooHeavy);
        }
        account.account_gold -= amount;
        purse.gold_pieces += amount;
        Ok(())
    }

    /// `DepositAll_LOC` (`:366-378`): sums every letter's value into the
    /// account and removes them.
    pub fn deposit_all_loc(&mut self, region: usize, purse: &mut Purse) -> Result<(), BankError> {
        let account = self.account_mut(region)?;
        account.account_gold += purse.letters_of_credit.iter().sum::<i64>();
        purse.letters_of_credit.clear();
        Ok(())
    }

    /// `Withdraw_LOC` (`:380-394`): commission `int(amount × 1.01)`,
    /// account check before the minimum.
    pub fn withdraw_loc(
        &mut self,
        region: usize,
        amount: i64,
        purse: &mut Purse,
    ) -> Result<(), BankError> {
        let account = self.account_mut(region)?;
        let amount_plus_commission = (amount as f64 * LOC_COMMISSION) as i64;
        if amount_plus_commission > account.account_gold {
            return Err(BankError::NotEnoughAccountLoc);
        }
        if amount < MIN_LOC {
            return Err(BankError::LocRequestTooSmall);
        }
        account.account_gold -= amount_plus_commission;
        purse.letters_of_credit.push(amount);
        Ok(())
    }

    /// `BorrowLoan` (`:531-545`) + `CalculateBankLoanRepayment`
    /// (`FormulaHelper.cs:1990-2008`): the debt grows by the fixed `+10 %`,
    /// the account by the principal, and the due date is
    /// `now + 518400` minutes.
    pub fn borrow_loan(
        &mut self,
        region: usize,
        amount: i64,
        level: i64,
        now_minutes: i64,
    ) -> Result<(), BankError> {
        if amount < MIN_LOAN {
            return Err(BankError::LoanRequestTooLow);
        }
        if amount > level * LOAN_MAX_PER_LEVEL {
            return Err(BankError::LoanRequestTooHigh);
        }
        let account = self.account_mut(region)?;
        account.loan_total += amount + amount / 10;
        account.account_gold += amount;
        account.loan_due = now_minutes + LOAN_REPAY_MINUTES;
        self.events.push(LoanEvent::Borrow);
        Ok(())
    }

    /// `RepayLoan` (`:498-528`). `account_only` is the overdue path
    /// (`Repaying_loan_from_account`).
    ///
    /// `amount > loan` ⇒ [`RepayOutcome::overpaid`] with the amount clipped
    /// to the debt; the debt is closed with `loanDueDate = 0`.
    pub fn repay_loan(
        &mut self,
        region: usize,
        amount: i64,
        account_only: bool,
        purse: &mut Purse,
    ) -> Result<RepayOutcome, BankError> {
        let account = self.account_mut(region)?;
        if account.loan_total <= 0 {
            return Ok(RepayOutcome {
                paid: 0,
                cleared: true,
                overpaid: false,
            });
        }
        let available = account.account_gold + if account_only { 0 } else { purse.available_gold() };
        if amount > available {
            return Err(BankError::NotEnoughGold);
        }
        let mut applied = amount;
        let mut overpaid = false;
        if applied > account.loan_total {
            overpaid = true;
            applied = account.loan_total;
        }
        account.loan_total -= applied;
        if !account_only {
            let shortfall = purse.deduct_gold(applied);
            if shortfall > 0 {
                account.account_gold -= shortfall;
            }
        }
        let cleared = account.loan_total <= 0;
        if cleared {
            account.loan_due = 0;
            self.events.push(LoanEvent::Repay);
        }
        Ok(RepayOutcome {
            paid: applied,
            cleared,
            overpaid,
        })
    }

    /// `LoanChecker.CheckOverdueLoans` (`LoanChecker.cs:17-71`) for one
    /// region.
    ///
    /// Overdue: pays `min(loan, account)` from the account; if debt remains
    /// and the player had not defaulted, sets `hasDefaulted` **once** and
    /// returns the [`LoanCrime::LoanDefault`] fact (M28 owns the penalty).
    /// Otherwise, when `remainingMonths < lastRemainingMonths` and the
    /// boundary `{6,3,1}` was crossed, emits [`LoanEvent::Due`].
    pub fn check_overdue(
        &mut self,
        region: usize,
        last_game_minutes: i64,
        now_minutes: i64,
    ) -> Result<Option<LoanCrime>, BankError> {
        let due = self.account(region)?.loan_due;
        if due == 0 {
            return Ok(None);
        }
        if due < now_minutes {
            self.events.push(LoanEvent::Overdue);
            let account = self.account_mut(region)?;
            let transfer = account.loan_total.min(account.account_gold);
            account.loan_total -= transfer;
            account.account_gold -= transfer;
            if account.loan_total <= 0 {
                account.loan_due = 0;
                self.events.push(LoanEvent::Repay);
                return Ok(None);
            }
            if account.has_defaulted {
                return Ok(None);
            }
            account.has_defaulted = true;
            self.events.push(LoanEvent::Default);
            return Ok(Some(LoanCrime::LoanDefault));
        }
        let last_months = (due - last_game_minutes) / MINUTES_PER_MONTH;
        let remaining_months = (due - now_minutes) / MINUTES_PER_MONTH;
        if remaining_months < last_months
            && REMINDER_MONTHS
                .iter()
                .any(|month| last_months >= *month && remaining_months < *month)
        {
            self.events.push(LoanEvent::Due);
        }
        Ok(None)
    }

    /// `PurchaseHouse` (`:396-412`): price paid with `playerGold + account`;
    /// the gold is deducted first and the account covers the remainder.
    pub fn purchase_house(
        &mut self,
        region: usize,
        building_key: i64,
        mesh_radius: f32,
        purse: &mut Purse,
    ) -> Result<i64, BankError> {
        let account = self.account_mut(region)?;
        let price = house_price(mesh_radius);
        if price > purse.available_gold() + account.account_gold {
            return Err(BankError::NotEnoughGold);
        }
        let shortfall = purse.deduct_gold(price);
        account.account_gold -= shortfall;
        if let Some(slot) = self.houses.get_mut(region) {
            *slot = Some(HouseRecord {
                building_key,
                mesh_radius,
            });
        }
        Ok(price)
    }

    /// `SellHouse` (`:438-452`): credits `int(price × 0.85)` to the account
    /// and clears the deed.
    pub fn sell_house(&mut self, region: usize) -> Result<i64, BankError> {
        let house = self
            .houses
            .get_mut(region)
            .and_then(Option::take)
            .ok_or(BankError::UnknownRegion)?;
        let price = house_sell_price(house.mesh_radius);
        self.account_mut(region)?.account_gold += price;
        Ok(price)
    }

    /// `PurchaseShip` (`:455-468`): same payment rule as the house.
    pub fn purchase_ship(
        &mut self,
        region: usize,
        ship: ShipType,
        purse: &mut Purse,
    ) -> Result<(), BankError> {
        let account = self.account_mut(region)?;
        let price = ship.price();
        if price > purse.available_gold() + account.account_gold {
            return Err(BankError::NotEnoughGold);
        }
        let shortfall = purse.deduct_gold(price);
        account.account_gold -= shortfall;
        self.owned_ship = Some(ship);
        Ok(())
    }

    /// `SellShip` (`:487-495`): credits `int(price × 0.85)` and resets the
    /// ownership.
    pub fn sell_ship(&mut self, region: usize) -> Result<i64, BankError> {
        let ship = self.owned_ship.ok_or(BankError::UnknownRegion)?;
        let price = ship.sell_price();
        self.account_mut(region)?.account_gold += price;
        self.owned_ship = None;
        Ok(price)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// §6 · Transport
// ═══════════════════════════════════════════════════════════════════════════

/// `TransportManager.TransportModes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportMode {
    /// On foot.
    Foot,
    /// Riding a horse (requires the `Transportation.Horse` item).
    Horse,
    /// Driving a cart (requires the `Transportation.Small_cart` item).
    Cart,
    /// Sailing the owned ship (requires `DaggerfallBankManager.OwnsShip`).
    Ship,
}

impl TransportMode {
    /// All four modes (`DaggerfallTransportWindow.cs:200-258`).
    pub const ALL: [TransportMode; 4] = [
        TransportMode::Foot,
        TransportMode::Horse,
        TransportMode::Cart,
        TransportMode::Ship,
    ];

    /// Source identifier.
    pub const fn name(self) -> &'static str {
        match self {
            TransportMode::Foot => "Foot",
            TransportMode::Horse => "Horse",
            TransportMode::Cart => "Cart",
            TransportMode::Ship => "Ship",
        }
    }

    /// Speed in m/s (`PlayerSpeedChanger.cs:147-166`):
    /// `(LiveSpeed + base) / 39.5`; `Ship` has no walking speed (M31 owns
    /// travel and boarding).
    pub fn speed_m_per_s(self, live_speed: f32) -> Option<f32> {
        let base = match self {
            TransportMode::Foot => DF_WALK_BASE,
            TransportMode::Horse => DF_RIDE_BASE,
            TransportMode::Cart => DF_CART_BASE,
            TransportMode::Ship => return None,
        };
        Some((live_speed + base) / CLASSIC_TO_UNITY_SPEED_RATIO)
    }

    /// `CanRunUnlessRiding` (`PlayerSpeedChanger.cs:137-141`): mounted modes
    /// cannot run.
    pub const fn can_run(self) -> bool {
        matches!(self, TransportMode::Foot | TransportMode::Ship)
    }
}

/// `classicToUnitySpeedUnitRatio` (`PlayerSpeedChanger.cs:31`).
pub const CLASSIC_TO_UNITY_SPEED_RATIO: f32 = 39.5;
/// `dfWalkBase` (`:32`).
pub const DF_WALK_BASE: f32 = 150.0;
/// `dfCrouchBase` (`:33`) — declared for completeness of the constants.
pub const DF_CROUCH_BASE: f32 = 50.0;
/// `dfRideBase = dfWalkBase + 225` (`:34`).
pub const DF_RIDE_BASE: f32 = 375.0;
/// `dfCartBase = dfWalkBase + 100` (`:35`).
pub const DF_CART_BASE: f32 = 250.0;

/// Transport availability and selection (`TransportManager.cs:82-131`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportState {
    /// Currently selected mode.
    pub mode: TransportMode,
    /// `HasHorse()` (Transportation item).
    pub has_horse: bool,
    /// `HasCart()` (Transportation item).
    pub has_cart: bool,
    /// `DaggerfallBankManager.OwnsShip`.
    pub owns_ship: bool,
}

impl Default for TransportState {
    fn default() -> Self {
        Self {
            mode: TransportMode::Foot,
            has_horse: false,
            has_cart: false,
            owns_ship: false,
        }
    }
}

impl TransportState {
    /// Fresh state on foot.
    pub fn new() -> Self {
        Self::default()
    }

    /// Is `mode` available (`DaggerfallTransportWindow.cs:82-84`)?
    pub fn available(&self, mode: TransportMode) -> bool {
        match mode {
            TransportMode::Foot => true,
            TransportMode::Horse => self.has_horse,
            TransportMode::Cart => self.has_cart,
            TransportMode::Ship => self.owns_ship,
        }
    }

    /// Selects a mode if available; state is unchanged on error.
    pub fn select(&mut self, mode: TransportMode) -> Result<(), TransportError> {
        if self.available(mode) {
            self.mode = mode;
            Ok(())
        } else {
            Err(TransportError::Unavailable)
        }
    }

    /// `ToggleMount` (`TransportManager.cs:108-124`): off a mount returns to
    /// foot, otherwise mounts the horse, else the cart; foot if neither.
    pub fn toggle_mount(&mut self) {
        if matches!(self.mode, TransportMode::Horse | TransportMode::Cart) {
            self.mode = TransportMode::Foot;
        } else if self.has_horse {
            self.mode = TransportMode::Horse;
        } else if self.has_cart {
            self.mode = TransportMode::Cart;
        } else {
            self.mode = TransportMode::Foot;
        }
    }
}

/// Transport selection error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportError {
    /// The item/ship required by the mode is not owned.
    Unavailable,
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TRANSPORT_UNAVAILABLE")
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::holidays::{
        get_holiday_id, WITCHES_FESTIVAL,
    };
    use super::*;

    #[test]
    fn shop_table_has_nine_buildings_five_modes_and_36_pairs() {
        let pairs: usize = BuildingType::ALL
            .iter()
            .map(|building| building.accepted_groups().len())
            .sum();
        assert_eq!(BuildingType::ALL.len(), 9);
        assert_eq!(TradeMode::ALL.len(), 5);
        assert_eq!(pairs, 36);
        assert_eq!(
            BuildingType::from_index(9),
            Some(BuildingType::GeneralStore),
            "field default is GeneralStore (`DaggerfallTradeWindow.cs:92`)"
        );
        assert_eq!(BuildingType::from_index(15), None, "Tavern has no stock table");
        assert!(BuildingType::GeneralStore.accepts(ItemGroup::Transportation));
        assert!(!BuildingType::Bookseller.accepts(ItemGroup::Weapons));
        assert_eq!(TradeMode::Buy.selling(), Some(false));
        assert_eq!(TradeMode::Sell.selling(), Some(true));
        assert_eq!(TradeMode::Identify.selling(), None);
        assert!(sale_offer(
            BuildingType::WeaponSmith,
            TradeMode::Sell,
            ItemGroup::Weapons,
            false
        ));
        assert!(!sale_offer(
            BuildingType::Bookseller,
            TradeMode::SellMagic,
            ItemGroup::Books,
            false
        ));
        assert!(!BuildingType::PawnShop.accepts(ItemGroup::Transportation));
    }

    #[test]
    fn calculate_cost_golden_vectors_e1_e8() {
        // E1-E8 of SPEC-M30 §7.
        assert_eq!(calculate_cost(100, 10, Some(1000)), 200);
        assert_eq!(calculate_cost(100, 0, Some(1000)), 180);
        assert_eq!(calculate_cost(100, 20, Some(1000)), 220);
        assert_eq!(calculate_cost(0, 10, Some(1000)), 2);
        assert_eq!(calculate_cost(100, 10, Some(750)), 150);
        assert_eq!(calculate_cost(100, 10, Some(4000)), 800);
        assert_eq!(calculate_cost(33, 10, Some(500)), 32);
        assert_eq!(calculate_cost(1, 0, Some(250)), 2);
        assert_eq!(calculate_cost(100, 10, None), 200, "no location ⇒ identity");
    }

    #[test]
    fn repair_and_identify_golden_vectors_e16_e23() {
        assert_eq!(repair_cost(500, 10, 40, 100, Some(1000)), 100);
        assert_eq!(repair_cost(5, 10, 0, 100, Some(1000)), 2);
        assert_eq!(repair_cost(500, 10, 100, 100, Some(1000)), 0);
        assert_eq!(repair_time_seconds(0, 100), 86_400);
        assert_eq!(repair_time_seconds(500, 5000), 388_800);
        assert_eq!(repair_time_seconds(0, 5000), 432_000);
        assert_eq!(collective_repair_seconds(86_400, 172_800), 129_600);
        assert_eq!(identify_cost(100, 0), 9);
        assert_eq!(identify_cost(1000, 0), 97);
        assert_eq!(identify_cost(10, 0), 0);
        assert_eq!(identify_cost(1000, WITCHES_FESTIVAL), 0);
    }

    #[test]
    fn room_golden_vectors_e24_e27() {
        assert_eq!(room_cost(3, 10), 21);
        assert_eq!(room_cost(2, 45), 7);
        assert_eq!(room_cost(1, 46), 0);
        assert_eq!(room_cost(5, 47), 35);
        assert!(room_rental_allowed(350, 0));
        assert!(!room_rental_allowed(351, 0));
        assert_eq!(room_expiry_seconds(1000, 3), 1000 + 259_200);
        assert_eq!(room_remaining_hours(1000, 1000 + 259_200), 72);
    }

    #[test]
    fn holidays_golden_vectors_e29_e31() {
        assert_eq!(get_holiday_id(45 * 1440, 0), 10, "day 46 = Hearts Day, all regions");
        assert_eq!(get_holiday_id(57 * 1440, 25), 12, "day 58 = Aduros Nau in region 25");
        assert_eq!(get_holiday_id(0, 7), 1, "day 1 = New Life, all regions");
        assert_eq!(get_holiday_id(355 * 1440, 0), 0, "day > 355 ⇒ no holiday");
        assert_eq!(get_holiday_id(57 * 1440, 0), 0, "region restriction");
        assert_eq!(super::holidays::REGION.len(), 53);
        assert_eq!(super::holidays::DAY.len(), 53);
    }

    #[test]
    fn regional_drift_and_flags() {
        assert_eq!(initial_regional_price(0), 750);
        assert_eq!(initial_regional_price(500), 1250);
        assert_eq!(chance_of_price_rise(100, 50, 1000), 60);
        assert_eq!(update_regional_price(1000, true), 980);
        assert_eq!(update_regional_price(1000, false), 1020);
        assert_eq!(update_regional_price(250, true), 250, "clamped at 250");
        assert_eq!(update_regional_price(4000, false), 4000, "clamped at 4000");
        assert_eq!(price_flag(2001), Some(PriceFlag::High));
        assert_eq!(price_flag(2000), None);
        assert_eq!(price_flag(500), None);
        assert_eq!(price_flag(499), Some(PriceFlag::Low));
    }

    #[test]
    fn borrow_golden_vectors_e32_e33_e34() {
        let mut ledger = BankLedger::new(REGION_COUNT);
        ledger.borrow_loan(0, 1000, 3, 10_000).unwrap();
        assert_eq!(ledger.loan_total(0).unwrap(), 1100);
        assert_eq!(ledger.loan_due(0).unwrap(), 10_000 + 518_400);
        assert_eq!(ledger.account_gold(0).unwrap(), 1000);
        assert_eq!(ledger.events(), &[LoanEvent::Borrow]);

        let mut low = BankLedger::new(REGION_COUNT);
        assert_eq!(
            low.borrow_loan(0, 99, 3, 0),
            Err(BankError::LoanRequestTooLow)
        );
        assert_eq!(
            low.borrow_loan(0, 150_001, 3, 0),
            Err(BankError::LoanRequestTooHigh)
        );
        assert_eq!(low.account_gold(0).unwrap(), 0, "0 mutation on Err");

        let mut loc = BankLedger::new(REGION_COUNT);
        loc.deposit_gold(0, 1000, &mut Purse::new(1000)).unwrap();
        let mut purse = Purse::new(0);
        loc.withdraw_loc(0, 100, &mut purse).unwrap();
        assert_eq!(loc.account_gold(0).unwrap(), 899, "commission 101");
        assert_eq!(purse.letters_of_credit, vec![100]);
        assert_eq!(
            loc.withdraw_loc(0, 99, &mut purse),
            Err(BankError::LocRequestTooSmall)
        );
        assert_eq!(loc.account_gold(0).unwrap(), 899, "0 mutation on Err");
    }

    #[test]
    fn withdrawal_weight_golden_vectors_e35_e36() {
        let mut ledger = BankLedger::new(REGION_COUNT);
        ledger.deposit_gold(0, 10_000, &mut Purse::new(10_000)).unwrap();
        let mut purse = Purse {
            carried_weight_units: weight_units_from_kg(70.0),
            max_encumbrance_units: weight_units_from_kg(75.0),
            ..Purse::new(0)
        };
        assert_eq!(ledger.withdraw_gold(0, 2000, &mut purse), Ok(()), "70+5.0 = 75");
        assert_eq!(ledger.account_gold(0).unwrap(), 8000);
        assert_eq!(
            ledger.withdraw_gold(0, 3000, &mut purse),
            Err(BankError::TooHeavy)
        );
        assert_eq!(ledger.account_gold(0).unwrap(), 8000, "0 mutation on Err");
        assert_eq!(
            ledger.withdraw_gold(0, 9000, &mut purse),
            Err(BankError::NotEnoughAccount)
        );
    }

    #[test]
    fn ship_and_house_golden_vectors_e37_e39() {
        assert_eq!(ShipType::Small.sell_price(), 85_000);
        assert_eq!(ShipType::Large.sell_price(), 170_000);
        assert_eq!(house_price(12.5), 16_000);
        assert_eq!(house_sell_price(12.5), 13_600);
        assert_eq!(house_price(7.75), 9920);
        assert_eq!(house_sell_price(7.75), 8432);

        let mut ledger = BankLedger::new(REGION_COUNT);
        let mut purse = Purse::new(20_000);
        ledger.purchase_house(0, 7, 12.5, &mut purse).unwrap();
        assert_eq!(purse.gold_pieces, 4000);
        assert_eq!(ledger.owned_house(0).unwrap().building_key, 7);
        assert_eq!(ledger.sell_house(0), Ok(13_600));
        assert_eq!(ledger.account_gold(0).unwrap(), 13_600);
    }

    #[test]
    fn loan_reminder_golden_vector_e40() {
        let mut ledger = BankLedger::new(REGION_COUNT);
        ledger.borrow_loan(0, 1000, 3, 0).unwrap();
        let due = ledger.loan_due(0).unwrap();
        // 7 months left → 5 months left crosses the 6-month reminder.
        assert_eq!(
            ledger.check_overdue(0, due - 302_400, due - 216_000),
            Ok(None)
        );
        assert_eq!(ledger.events(), &[LoanEvent::Borrow, LoanEvent::Due]);
        // 5 → 4 months does not cross any boundary.
        let mut quiet = BankLedger::new(REGION_COUNT);
        quiet.borrow_loan(0, 1000, 3, 0).unwrap();
        let due = quiet.loan_due(0).unwrap();
        quiet.check_overdue(0, due - 216_000, due - 172_800).unwrap();
        assert_eq!(quiet.events(), &[LoanEvent::Borrow]);
    }

    #[test]
    fn overdue_golden_vector_e41_defaults_once() {
        // E41 state: debt 5000, account 3000. `BorrowLoan` credits the
        // principal, so the state is reached by borrowing 4546 (4546 + 454 =
        // 5000), withdrawing it, then depositing 3000.
        let mut ledger = BankLedger::new(REGION_COUNT);
        ledger.borrow_loan(0, 4546, 3, 0).unwrap();
        let mut purse = Purse {
            max_encumbrance_units: weight_units_from_kg(1000.0),
            ..Purse::new(0)
        };
        ledger.withdraw_gold(0, 4546, &mut purse).unwrap();
        ledger.deposit_gold(0, 3000, &mut purse).unwrap();
        let due = ledger.loan_due(0).unwrap();
        assert_eq!(
            ledger.check_overdue(0, 0, due + 1),
            Ok(Some(LoanCrime::LoanDefault))
        );
        assert_eq!(ledger.account_gold(0).unwrap(), 0);
        assert_eq!(ledger.loan_total(0).unwrap(), 2000);
        assert!(ledger.has_defaulted(0).unwrap());
        assert_eq!(
            ledger.events(),
            &[
                LoanEvent::Borrow,
                LoanEvent::Overdue,
                LoanEvent::Default
            ]
        );
        // Second overdue does not repeat the default (`hasDefaulted` gate).
        assert_eq!(ledger.check_overdue(0, 0, due + 2), Ok(None));
        assert_eq!(
            ledger
                .events()
                .iter()
                .filter(|event| **event == LoanEvent::Default)
                .count(),
            1
        );
    }

    #[test]
    fn accounts_are_per_region_and_errors_do_not_mutate() {
        let mut ledger = BankLedger::new(REGION_COUNT);
        ledger.deposit_gold(0, 500, &mut Purse::new(500)).unwrap();
        assert_eq!(ledger.account_gold(0).unwrap(), 500);
        assert_eq!(ledger.account_gold(1).unwrap(), 0, "region isolation");
        let mut poor = Purse::new(10);
        assert_eq!(
            ledger.deposit_gold(0, 500, &mut poor),
            Err(BankError::NotEnoughGold)
        );
        assert_eq!(ledger.account_gold(0).unwrap(), 500, "0 mutation");
        assert_eq!(poor.gold_pieces, 10, "purse untouched");
        assert_eq!(ledger.account(REGION_COUNT), Err(BankError::UnknownRegion));
    }

    #[test]
    fn transport_modes_availability_and_speeds() {
        let mut state = TransportState::new();
        assert_eq!(state.mode, TransportMode::Foot);
        assert_eq!(
            state.select(TransportMode::Horse),
            Err(TransportError::Unavailable)
        );
        state.has_horse = true;
        state.select(TransportMode::Horse).unwrap();
        assert_eq!(state.mode, TransportMode::Horse);
        state.toggle_mount();
        assert_eq!(state.mode, TransportMode::Foot);
        state.toggle_mount();
        assert_eq!(state.mode, TransportMode::Horse);
        assert!(!TransportMode::Horse.can_run());
        assert!(TransportMode::Foot.can_run());

        let live = 50.0_f32;
        assert_eq!(
            TransportMode::Foot.speed_m_per_s(live),
            Some((live + 150.0) / 39.5)
        );
        assert_eq!(
            TransportMode::Horse.speed_m_per_s(live),
            Some((live + 375.0) / 39.5)
        );
        assert_eq!(
            TransportMode::Cart.speed_m_per_s(live),
            Some((live + 250.0) / 39.5)
        );
        assert_eq!(TransportMode::Ship.speed_m_per_s(live), None);
    }

    #[test]
    fn purchase_holiday_halves_before_haggling() {
        use super::holidays::{
            discounted_basket_price, MERCHANTS_FESTIVAL, TALES_AND_TALLOW, WARRIORS_FESTIVAL,
        };
        assert_eq!(
            discounted_basket_price(100, MERCHANTS_FESTIVAL, false, false, ItemGroup::Books),
            50
        );
        assert_eq!(
            discounted_basket_price(100, MERCHANTS_FESTIVAL, true, true, ItemGroup::Books),
            100
        );
        assert_eq!(
            discounted_basket_price(100, TALES_AND_TALLOW, true, true, ItemGroup::Books),
            50
        );
        assert_eq!(
            discounted_basket_price(100, WARRIORS_FESTIVAL, false, false, ItemGroup::Weapons),
            50
        );
        assert_eq!(
            discounted_basket_price(100, WARRIORS_FESTIVAL, false, false, ItemGroup::Books),
            100
        );
    }

    #[test]
    fn shop_costs_compose_catalog_price_and_stack() {
        let shop = Shop {
            building: BuildingType::GeneralStore,
            quality: 10,
            price_adjustment: Some(1000),
            items: vec![ShopItem {
                id: "potion",
                name: "minor potion",
                group: ItemGroup::MiscellaneousIngredients1,
                base_value: 80,
                condition: 100,
                max_condition: 100,
                enchanted: false,
                identified: true,
                stack: 2,
            }],
        };
        let item = &shop.items[0];
        assert_eq!(shop.item_cost(item), 320, "2 × CalculateCost(80)");
        assert!(shop.sellable(TradeMode::Sell).is_empty(), "GeneralStore buys no ingredients");
        assert!(shop.sellable(TradeMode::SellMagic).is_empty());
    }
}

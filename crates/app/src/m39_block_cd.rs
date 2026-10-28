//! M39/UI-3 — blocks **C** (inventario · hoja de personaje, 2 windows) and
//! **D** (comercio · banca · taberna · descanso · reparación, 7 windows) of
//! plan §1/§2/§5.
//!
//! **Frozen surface (plan §4).** [`CdState`], [`CdState::from_retail`],
//! [`interact`], [`evidence`] and [`evidence_d`] are consumed verbatim by the
//! probe grammar of §4.1; the lane extends the *state*, never the vocabulary.
//!
//! **Provenance (plan §3.1: zero invention).** Every painted number comes from
//!
//! * `daggerfall_inventory::{items, equipment}` — the retail `FALL.EXE` item
//!   table (`parse_item_templates`), the item groups and the classic weights;
//! * `daggerfall_skills::{career, character, formula, rest}` — the retail
//!   `CLASS01.CFG` career, its live attributes/skills, `calculate_trade_price`
//!   and the rest-window simulation;
//! * `daggerfall_behavior::{economy, locomotion}` — room rent, repair and
//!   identify prices, the loan ledger, ship deeds and the encumbrance rule;
//! * `daggerfall_heart::{calendar, clock}` — the classic session calendar.
//!
//! The session purse and the merchant quality are the app's **already
//! declared** session parameters (`m5_probe::{STARTING_GOLD, SHOP_QUALITY}`),
//! reused here instead of minting a second set of literals. When a retail
//! datum is absent (no `FALL.EXE`) the affected intents are `Rejected` with a
//! citable reason — never an invented zero.
//!
//! **Determinism.** The state stores no `f32`, no item UID and no wall clock:
//! weights are integer grams, the calendar is [`GameClock::classic_default`]
//! and every change goes through [`M39State::commit_i64`] /
//! [`M39State::commit_text`], so two runs over the same retail pair hand back
//! equal [`Interaction`]s and equal digests.

use std::path::Path;

use daggerfall_behavior::economy::{
    self, calculate_cost, identify_cost, repair_cost, room_cost, room_expiry_seconds,
    room_remaining_hours, room_rental_allowed, BankError, BankLedger, LoanEvent, Purse, ShipType,
    TradeMode, MAX_RENT_DAYS, MIN_LOAN, REGION_COUNT,
};
use daggerfall_behavior::locomotion::{is_overencumbered, max_encumbrance};
use daggerfall_heart::calendar::day_of_year;
use daggerfall_heart::clock::GameClock;
use daggerfall_inventory::equipment::EQUIP_TABLE_LENGTH;
use daggerfall_inventory::items::{
    parse_item_templates, round_half_even, template_group, total_weight_kg, Item, ItemGroup,
    ItemTemplate, FALL_EXE_FILENAME,
};
use daggerfall_parser_bevy_bridge::ui_compat::UiCompatError;
use daggerfall_parser_bevy_bridge::ui_spec::UiAssets;
use daggerfall_perception::ui_shell::canonical_id_for;
use daggerfall_skills::career::Career;
use daggerfall_skills::character::{Attribute, Character, Skill};
use daggerfall_skills::formula::calculate_trade_price;
use daggerfall_skills::rest::{RestMode, RestVitals, RestWindow, SLEEP_EVENT_MINIMUM_HOURS};
use daggerfall_skills::rng::Rng64;

use crate::m18_demo::{M19_SEED, M20_CLASS_FILE};
use crate::m39_state::{BlockEvidence, Interaction, M39State, ModalOutcome};

/// Block C windows (plan §1/§2: inventario + hoja de personaje).
pub const BLOCK_C_WINDOWS: usize = 2;
/// Block D windows (plan §1/§2: comercio, banca, taberna, descanso, reparación).
pub const BLOCK_D_WINDOWS: usize = 7;

/// Window names of block C, in census order.
pub const BLOCK_C_NAMES: [&str; BLOCK_C_WINDOWS] = [
    "DaggerfallCharacterSheetWindow",
    "DaggerfallInventoryWindow",
];

/// Window names of block D, in census order.
pub const BLOCK_D_NAMES: [&str; BLOCK_D_WINDOWS] = [
    "DaggerfallBankPurchasePopUp",
    "DaggerfallBankingWindow",
    "DaggerfallMerchantRepairPopupWindow",
    "DaggerfallMerchantServicePopupWindow",
    "DaggerfallRestWindow",
    "DaggerfallTavernWindow",
    "DaggerfallTradeWindow",
];

/// Census ordinals of blocks C+D (plan §1: `028`…`063`), ascending.
///
/// The ordinal is the frozen census position carried by
/// [`Interaction::id`]; the *window identity* itself is always resolved by
/// name through `ui_shell::canonical_id_for`, never guessed from a number.
pub const WINDOWS: [(u16, &str); BLOCK_C_WINDOWS + BLOCK_D_WINDOWS] = [
    (24, "DaggerfallBankPurchasePopUp"),
    (25, "DaggerfallBankingWindow"),
    (28, "DaggerfallCharacterSheetWindow"),
    (40, "DaggerfallInventoryWindow"),
    (45, "DaggerfallMerchantRepairPopupWindow"),
    (46, "DaggerfallMerchantServicePopupWindow"),
    (55, "DaggerfallRestWindow"),
    (61, "DaggerfallTavernWindow"),
    (63, "DaggerfallTradeWindow"),
];

/// Census ordinal of a C/D window, after proving the name is in the census.
fn window_ordinal(name: &str) -> Result<u16, UiCompatError> {
    if canonical_id_for(name).is_none() {
        return Err(UiCompatError::Navigation(RULE_UNKNOWN_WINDOW));
    }
    WINDOWS
        .iter()
        .find(|(_, window)| *window == name)
        .map(|(ordinal, _)| *ordinal)
        .ok_or(UiCompatError::Navigation(RULE_UNKNOWN_WINDOW))
}

/// Region of the session bank account (single-region M39 session).
const REGION: usize = 0;
/// Grams per kilogram (the read model reports grams, plan §3.7).
const GRAMS_PER_KG: i64 = 1_000;
/// Classis rental clock origin (the session calendar has no accumulated
/// seconds of its own; the rental helpers take an explicit origin).
const RENTAL_ORIGIN_SECONDS: u64 = 0;

/// Rule string: the session career (`CLASS01.CFG`) cannot be read.
const RULE_CAREER_READ: &str = "m39.cd: session career CLASS01.CFG unreadable";
/// Rule string: the session career (`CLASS01.CFG`) is not a career.
const RULE_CAREER_PARSE: &str = "m39.cd: session career CLASS01.CFG rejected";
/// Rule string: the name is not a window of blocks C/D.
const RULE_UNKNOWN_WINDOW: &str = "m39.cd: name is not a C/D census window";
/// Rule string: the window already delivered its flow in this session.
const RULE_REPLAY: &str = "m39.cd: window already delivered in this session";
/// Rule string: a slot index outside the classic equip table.
const RULE_SLOT_RANGE: &str = "m39.cd: slot index outside ItemEquipTable";
/// Rule string: the retail item table is absent, so no item can be shown.
const RULE_NO_ITEM_TABLE: &str = "m39.cd: retail FALL.EXE item table absent";
/// Rule string: the retail catalogue carries no priced template.
const RULE_TRADE_EMPTY: &str = "m39.cd: retail catalogue has no priced template";
/// Rule string: the rental would exceed the classic cap.
const RULE_RENT_LIMIT: &str = "m39.cd: rental exceeds MAX_RENT_DAYS";
/// Rule string: nothing to repair on an item at maximum condition.
const RULE_REPAIRED: &str = "m39.cd: item already at maximum condition";
/// Rule string: the shop refuses the item group in sell mode.
const RULE_SELL_REFUSED: &str = "m39.cd: shop refuses this item group";
/// Rule string: the block did not deliver all of its windows.
const RULE_INCOMPLETE: &str = "m39.cd: block flow incomplete";
/// Rule string: the merchant quality does not fit the domain parameter.
const RULE_QUALITY: &str = "m39.cd: merchant quality outside 0..=255";
/// Rule string: a projected value does not fit its read-model route.
const RULE_RANGE: &str = "m39.cd: value outside the read-model route";

/// One equipped item projected from the retail table (no `f32`, no UID).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquippedRow {
    /// Item name (`ItemTemplate::name`, retail bytes).
    pub name: String,
    /// Classic item group of the template.
    pub group: ItemGroup,
    /// Carried weight in grams (classic weight units projected to g).
    pub weight_g: i64,
}

/// One priced row of the retail catalogue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogueRow {
    /// Template index in the retail table.
    pub template: usize,
    /// Item name (retail bytes).
    pub name: String,
    /// Classic item group of the template.
    pub group: ItemGroup,
    /// Retail base price (`ItemTemplate::base_price`).
    pub value: i64,
    /// Current condition (`hit_points`, items are generated pristine).
    pub condition: i64,
    /// Maximum condition (`hit_points`).
    pub max_condition: i64,
    /// Classic weight in grams.
    pub weight_g: i64,
}

/// One delivered flow, kept for [`evidence`] / [`evidence_d`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdFlow {
    /// Block letter (`C` or `D`).
    pub block: char,
    /// Census window name.
    pub name: &'static str,
    /// Intents attempted by the surface.
    pub intents: usize,
    /// Intents delivered with an outcome.
    pub delivered: usize,
    /// Read models whose value actually changed.
    pub changed: usize,
}

/// Blocks C + D state (plan §4: one struct for the two commerce-side blocks).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdState {
    /// Live level of the session-start character.
    level: u16,
    /// `max_encumbrance` of the live strength, in grams.
    max_weight_g: i64,
    /// Live `Mercantile` skill of the session-start character.
    mercantile: i32,
    /// Live `Personality` attribute of the session-start character.
    personality: i32,
    /// Encumbrance band of the equipped load (the two states of
    /// `is_overencumbered`, plan §5 `m39.sheet.encumbrance_band`).
    encumbrance_band: &'static str,
    /// Equipped items projected from the retail table.
    equipped: Vec<EquippedRow>,
    /// Priced retail catalogue, in template order.
    catalogue: Vec<CatalogueRow>,
    /// Selected equipped row (inventory window).
    selected: usize,
    /// Selected catalogue row (trade window).
    selected_row: usize,
    /// Basket of catalogue row indices (trade window).
    basket: Vec<usize>,
    /// Session purse in gold pieces (`m5_probe::STARTING_GOLD`).
    gold: i64,
    /// Session calendar (`GameClock::classic_default`).
    clock: GameClock,
    /// Delivered flows, in delivery order.
    flows: Vec<CdFlow>,
}

impl CdState {
    /// Fills both blocks from the retail bank and the `ARENA2` directory.
    ///
    /// The career is the session start (`CLASS01.CFG`, [`M19_SEED`], level 1);
    /// the equipment/catalogue come from the retail `FALL.EXE` item table when
    /// it is reachable (either next to `ARENA2` or inside it). A missing item
    /// table is **not** an error: the item intents then reject with a citable
    /// reason instead of painting an invented inventory.
    pub fn from_retail(assets: &UiAssets, arena2: &Path) -> Result<Self, UiCompatError> {
        let _ = assets;
        let bytes = std::fs::read(arena2.join(M20_CLASS_FILE))
            .map_err(|_| UiCompatError::Text(RULE_CAREER_READ))?;
        let career =
            Career::parse(&bytes).map_err(|_| UiCompatError::Text(RULE_CAREER_PARSE))?;
        let character = Character::starting(career, &mut Rng64::new(M19_SEED), 1);

        let level = u16::try_from(character.level).map_err(|_| UiCompatError::Text(RULE_RANGE))?;
        let max_weight_g = i64::from(max_encumbrance(character.live(Attribute::Strength)))
            * GRAMS_PER_KG;
        let mercantile = i32::from(character.live_skill(Skill::Mercantile));
        let personality = character.live(Attribute::Personality);

        let templates = item_templates(arena2);
        let equipped = project_equipped(&templates);
        let catalogue = project_catalogue(&templates);
        let carried_kg = grams_to_kg(equipped.iter().map(|row| row.weight_g).sum());
        let encumbrance_band = if is_overencumbered(carried_kg) {
            "overencumbered"
        } else {
            "within capacity"
        };

        Ok(Self {
            level,
            max_weight_g,
            mercantile,
            personality,
            encumbrance_band,
            equipped,
            catalogue,
            selected: 0,
            selected_row: 0,
            basket: Vec::new(),
            gold: i64::from(crate::m5_probe::STARTING_GOLD),
            clock: GameClock::classic_default(),
            flows: Vec::new(),
        })
    }
}

/// Retail item templates next to (or inside) the `ARENA2` directory.
///
/// A missing or truncated `FALL.EXE` yields an empty table: the callers above
/// then reject the item intents with [`RULE_NO_ITEM_TABLE`], never invent one.
fn item_templates(arena2: &Path) -> Vec<ItemTemplate> {
    let mut candidates = vec![arena2.join(FALL_EXE_FILENAME)];
    if let Some(parent) = arena2.parent() {
        candidates.push(parent.join(FALL_EXE_FILENAME));
    }
    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            if let Ok(templates) = parse_item_templates(&bytes) {
                return templates;
            }
        }
    }
    Vec::new()
}

/// The classic weights in integer grams (half-to-even, as the domain does).
fn grams(kg: f32) -> i64 {
    let scaled = round_half_even(kg * GRAMS_PER_KG as f32);
    if scaled.is_finite() {
        scaled.max(0.0) as i64
    } else {
        0
    }
}

/// Grams back to kg for the domain encumbrance rule.
fn grams_to_kg(grams: i64) -> f32 {
    grams as f32 / GRAMS_PER_KG as f32
}

/// First template of a group that carries a retail price.
fn first_of(templates: &[ItemTemplate], group: ItemGroup) -> Option<&ItemTemplate> {
    templates.iter().find(|template| {
        template_group(template.index) == group && template.base_price > 0 && !template.name.is_empty()
    })
}

/// Weapon + body armour projection (the two equipped rows of the inventory).
fn project_equipped(templates: &[ItemTemplate]) -> Vec<EquippedRow> {
    let mut rows = Vec::new();
    for group in [ItemGroup::Weapons, ItemGroup::Armor] {
        if let Some(template) = first_of(templates, group) {
            let item = Item::from_template(template);
            rows.push(EquippedRow {
                name: template.name.clone(),
                group: item.item_group,
                weight_g: grams(total_weight_kg(&item, template)),
            });
        }
    }
    rows
}

/// Priced catalogue in template order.
fn project_catalogue(templates: &[ItemTemplate]) -> Vec<CatalogueRow> {
    templates
        .iter()
        .filter(|template| template.base_price > 0 && !template.name.is_empty())
        .map(|template| {
            let item = Item::from_template(template);
            CatalogueRow {
                template: template.index,
                name: template.name.clone(),
                group: item.item_group,
                value: i64::from(template.base_price),
                condition: i64::from(template.hit_points),
                max_condition: i64::from(template.hit_points),
                weight_g: grams(total_weight_kg(&item, template)),
            }
        })
        .collect()
}

/// Merchant quality parameter of the session (`m5_probe::SHOP_QUALITY`).
fn shop_quality() -> Result<u8, UiCompatError> {
    u8::try_from(crate::m5_probe::SHOP_QUALITY).map_err(|_| UiCompatError::Text(RULE_QUALITY))
}

/// Bridge from the inventory item group to the economy sale-filter group.
///
/// `None` means the economy filter has no counterpart (quest items, deeds,
/// currency, virtual magic groups): such a group is never offered for sale by
/// these buildings, which is exactly what the trade window's refusal probe
/// reports.
fn economy_group(group: ItemGroup) -> Option<economy::ItemGroup> {
    let mapped = match group {
        ItemGroup::Armor => economy::ItemGroup::Armor,
        ItemGroup::Weapons => economy::ItemGroup::Weapons,
        ItemGroup::Books => economy::ItemGroup::Books,
        ItemGroup::MensClothing => economy::ItemGroup::MensClothing,
        ItemGroup::WomensClothing => economy::ItemGroup::WomensClothing,
        ItemGroup::Furniture => economy::ItemGroup::Furniture,
        ItemGroup::UselessItems2 => economy::ItemGroup::UselessItems2,
        ItemGroup::ReligiousItems => economy::ItemGroup::ReligiousItems,
        ItemGroup::Paintings => economy::ItemGroup::Paintings,
        ItemGroup::Gems => economy::ItemGroup::Gems,
        ItemGroup::PlantIngredients1 => economy::ItemGroup::PlantIngredients1,
        ItemGroup::PlantIngredients2 => economy::ItemGroup::PlantIngredients2,
        ItemGroup::CreatureIngredients1 => economy::ItemGroup::CreatureIngredients1,
        ItemGroup::CreatureIngredients2 => economy::ItemGroup::CreatureIngredients2,
        ItemGroup::CreatureIngredients3 => economy::ItemGroup::CreatureIngredients3,
        ItemGroup::MiscellaneousIngredients1 => economy::ItemGroup::MiscellaneousIngredients1,
        ItemGroup::MiscellaneousIngredients2 => economy::ItemGroup::MiscellaneousIngredients2,
        ItemGroup::MetalIngredients => economy::ItemGroup::MetalIngredients,
        ItemGroup::Transportation => economy::ItemGroup::Transportation,
        ItemGroup::Jewellery => economy::ItemGroup::Jewellery,
        ItemGroup::None
        | ItemGroup::Drugs
        | ItemGroup::UselessItems1
        | ItemGroup::MagicItems
        | ItemGroup::Artifacts
        | ItemGroup::Maps
        | ItemGroup::Deeds
        | ItemGroup::QuestItems
        | ItemGroup::MiscItems
        | ItemGroup::Currency => return None,
    };
    Some(mapped)
}

/// Citable reason of a refused bank rule.
fn bank_reason(error: BankError) -> &'static str {
    match error {
        BankError::NotEnoughGold => "m39.cd: bank refused: NOT_ENOUGH_GOLD",
        BankError::NotEnoughAccount => "m39.cd: bank refused: NOT_ENOUGH_ACCOUNT",
        BankError::UnknownRegion => "m39.cd: bank refused: UNKNOWN_REGION",
        BankError::LoanRequestTooLow => "m39.cd: bank refused: LOAN_REQUEST_TOO_LOW",
        BankError::LoanRequestTooHigh => "m39.cd: bank refused: LOAN_REQUEST_TOO_HIGH",
        _ => "m39.cd: bank refused: domain rule",
    }
}

/// Commits one `i64`-shaped route and records its signed delta.
fn put_i64(
    state: &mut M39State,
    path: &'static str,
    value: i64,
    outcomes: &mut Vec<ModalOutcome>,
    delta: &mut Vec<(&'static str, i64)>,
) -> Result<(), UiCompatError> {
    let outcome = state.commit_i64(path, value)?;
    if let ModalOutcome::Committed { after, .. } = &outcome {
        if let Ok(after) = after.parse::<i64>() {
            delta.push((path, after));
        }
    }
    outcomes.push(outcome);
    Ok(())
}

/// Commits one text-shaped route (a text route carries no numeric delta).
fn put_text(
    state: &mut M39State,
    path: &'static str,
    value: &str,
    outcomes: &mut Vec<ModalOutcome>,
) -> Result<(), UiCompatError> {
    let outcome = state.commit_text(path, value)?;
    outcomes.push(outcome);
    Ok(())
}

/// Seals the delivery, validates every outcome and records the flow.
fn deliver(
    state: &mut M39State,
    block: char,
    id: u16,
    name: &'static str,
    outcomes: Vec<ModalOutcome>,
    mut delta: Vec<(&'static str, i64)>,
) -> Result<Vec<Interaction>, UiCompatError> {
    for outcome in &outcomes {
        M39State::validate(outcome)?;
    }
    delta.sort_by(|left, right| left.0.cmp(right.0));
    let delivered = outcomes.len();
    let changed = delta.len();
    let interaction = Interaction {
        id,
        name,
        block,
        intents: delivered,
        delivered,
        outcomes,
        delta,
        read_only: None,
        digest: String::new(),
    }
    .sealed();
    state.cd.flows.push(CdFlow {
        block,
        name,
        intents: delivered,
        delivered,
        changed,
    });
    Ok(vec![interaction])
}

/// Refuses a second delivery of the same window in one session.
fn guard_replay(state: &M39State, name: &'static str) -> Result<(), UiCompatError> {
    if state.cd.flows.iter().any(|flow| flow.name == name) {
        return Err(UiCompatError::Input(RULE_REPLAY));
    }
    Ok(())
}

/// Delivers the interactions of block C (inventory · character sheet).
pub fn interact(state: &mut M39State, name: &'static str) -> Result<Vec<Interaction>, UiCompatError> {
    let id = window_ordinal(name)?;
    match name {
        "DaggerfallCharacterSheetWindow" | "DaggerfallInventoryWindow" => {
            interact_c(state, name, id)
        }
        _ => interact_d(state, name, id),
    }
}

/// Block C scripts (inventory · sheet routes).
fn interact_c(
    state: &mut M39State,
    name: &'static str,
    id: u16,
) -> Result<Vec<Interaction>, UiCompatError> {
    guard_replay(state, name)?;
    let mut outcomes = Vec::new();
    let mut delta = Vec::new();

    match name {
        "DaggerfallCharacterSheetWindow" => {
            let level = state.hud.level;
            let level = u16::try_from(level).map_err(|_| UiCompatError::Text(RULE_RANGE))?;
            let slots_filled = state.cd.equipped.len();
            let slots_filled =
                u16::try_from(slots_filled).map_err(|_| UiCompatError::Text(RULE_RANGE))?;
            let band = state.cd.encumbrance_band;
            put_i64(state, "m39.sheet.level", i64::from(level), &mut outcomes, &mut delta)?;
            put_i64(
                state,
                "m39.sheet.slots_filled",
                i64::from(slots_filled),
                &mut outcomes,
                &mut delta,
            )?;
            put_text(state, "m39.sheet.encumbrance_band", band, &mut outcomes)?;
            outcomes.push(ModalOutcome::Closed);
        }
        "DaggerfallInventoryWindow" => {
            let equipped = state.cd.equipped.clone();
            let weight_g: i64 = equipped.iter().map(|row| row.weight_g).sum();
            let max_weight_g = state.cd.max_weight_g;
            let gold = state.cd.gold;
            let selected = state.cd.selected;

            put_i64(
                state,
                "m39.inv.slots",
                i64::try_from(EQUIP_TABLE_LENGTH).map_err(|_| UiCompatError::Text(RULE_RANGE))?,
                &mut outcomes,
                &mut delta,
            )?;
            put_i64(
                state,
                "m39.inv.used_slots",
                i64::try_from(equipped.len()).map_err(|_| UiCompatError::Text(RULE_RANGE))?,
                &mut outcomes,
                &mut delta,
            )?;
            put_i64(state, "m39.inv.weight_g", weight_g, &mut outcomes, &mut delta)?;
            put_i64(
                state,
                "m39.inv.max_weight_g",
                max_weight_g,
                &mut outcomes,
                &mut delta,
            )?;
            put_i64(state, "m39.inv.gold", gold, &mut outcomes, &mut delta)?;

            match (equipped.first(), equipped.get(1)) {
                (Some(weapon), Some(armor)) => {
                    put_text(state, "m39.inv.equipped.weapon", &weapon.name, &mut outcomes)?;
                    put_text(state, "m39.inv.equipped.armor", &armor.name, &mut outcomes)?;
                    let index = selected % equipped.len();
                    let row = &equipped[index];
                    put_text(state, "m39.inv.selected.name", &row.name, &mut outcomes)?;
                    put_text(state, "m39.inv.action", &format!("{:?}", row.group), &mut outcomes)?;
                    state.cd.selected = (index + 1) % equipped.len();
                }
                _ => outcomes.push(M39State::rejected(RULE_NO_ITEM_TABLE)),
            }
            // Impossible intent: a paper-doll slot outside the equip table.
            put_text(state, "m39.inv.reject_reason", RULE_SLOT_RANGE, &mut outcomes)?;
            outcomes.push(M39State::rejected(RULE_SLOT_RANGE));
        }
        _ => return Err(UiCompatError::Navigation(RULE_UNKNOWN_WINDOW)),
    }

    deliver(state, 'C', id, name, outcomes, delta)
}

/// Block D scripts (16 `m39.trade.*` / `m39.bank.*` / `m39.inn.*` /
/// `m39.rest.*` / `m39.repair.*` / `m39.merchant.*` routes).
fn interact_d(
    state: &mut M39State,
    name: &'static str,
    id: u16,
) -> Result<Vec<Interaction>, UiCompatError> {
    guard_replay(state, name)?;
    let mut outcomes = Vec::new();
    let mut delta = Vec::new();
    let quality = shop_quality()?;
    let clock = state.cd.clock;
    let day = i64::try_from(day_of_year(&clock)).unwrap_or(i64::MAX);
    let level = i64::from(state.cd.level);

    match name {
        "DaggerfallBankPurchasePopUp" => {
            let mut ledger = BankLedger::new(REGION_COUNT);
            let mut purse = Purse::new(state.cd.gold);
            put_i64(
                state,
                "m39.bank.purchase_price",
                ShipType::Small.price(),
                &mut outcomes,
                &mut delta,
            )?;
            put_text(state, "m39.bank.action", ShipType::Small.name(), &mut outcomes)?;
            match ledger.purchase_ship(REGION, ShipType::Small, &mut purse) {
                Ok(()) => outcomes.push(ModalOutcome::Closed),
                Err(error) => outcomes.push(M39State::rejected(bank_reason(error))),
            }
            put_i64(
                state,
                "m39.bank.purchase_price",
                ShipType::Large.price(),
                &mut outcomes,
                &mut delta,
            )?;
            put_text(state, "m39.bank.action", ShipType::Large.name(), &mut outcomes)?;
        }
        "DaggerfallBankingWindow" => {
            let mut ledger = BankLedger::new(REGION_COUNT);
            let mut purse = Purse::new(state.cd.gold);
            let now_minutes = i64::try_from(clock.total_minutes()).unwrap_or(i64::MAX);
            if ledger
                .borrow_loan(REGION, MIN_LOAN, level, now_minutes)
                .is_ok()
            {
                let balance = ledger.account_gold(REGION).unwrap_or(0);
                let owed = ledger.loan_total(REGION).unwrap_or(0);
                put_i64(state, "m39.bank.balance", balance, &mut outcomes, &mut delta)?;
                put_i64(state, "m39.bank.loan_owed", owed, &mut outcomes, &mut delta)?;
                put_text(state, "m39.bank.action", LoanEvent::Borrow.name(), &mut outcomes)?;
                // Impossible intent: withdraw more than the account holds.
                let over = balance.saturating_add(MIN_LOAN);
                match ledger.withdraw_gold(REGION, over, &mut purse) {
                    Ok(()) => outcomes.push(ModalOutcome::Cancelled),
                    Err(error) => outcomes.push(M39State::rejected(bank_reason(error))),
                }
                // Repay from the account only (no purse/weight path).
                match ledger.repay_loan(REGION, owed, true, &mut purse) {
                    Ok(_) => {
                        let balance = ledger.account_gold(REGION).unwrap_or(0);
                        let owed = ledger.loan_total(REGION).unwrap_or(0);
                        put_i64(state, "m39.bank.balance", balance, &mut outcomes, &mut delta)?;
                        put_i64(state, "m39.bank.loan_owed", owed, &mut outcomes, &mut delta)?;
                        put_text(state, "m39.bank.action", LoanEvent::Repay.name(), &mut outcomes)?;
                    }
                    Err(error) => outcomes.push(M39State::rejected(bank_reason(error))),
                }
            } else {
                outcomes.push(M39State::rejected(bank_reason(
                    ledger
                        .borrow_loan(REGION, MIN_LOAN, level, now_minutes)
                        .err()
                        .unwrap_or(BankError::UnknownRegion),
                )));
            }
        }
        "DaggerfallMerchantRepairPopupWindow" => {
            let row = state.cd.catalogue.first().cloned();
            match row {
                Some(row) => {
                    let condition =
                        u16::try_from(row.condition).map_err(|_| UiCompatError::Text(RULE_RANGE))?;
                    let cost = repair_cost(
                        row.value,
                        quality,
                        i32::try_from(row.condition).unwrap_or(i32::MAX),
                        i32::try_from(row.max_condition).unwrap_or(i32::MAX),
                        None,
                    );
                    put_i64(
                        state,
                        "m39.repair.condition",
                        i64::from(condition),
                        &mut outcomes,
                        &mut delta,
                    )?;
                    put_i64(state, "m39.repair.cost", cost, &mut outcomes, &mut delta)?;
                    outcomes.push(M39State::rejected(RULE_REPAIRED));
                }
                None => outcomes.push(M39State::rejected(RULE_NO_ITEM_TABLE)),
            }
        }
        "DaggerfallMerchantServicePopupWindow" => {
            let row = state.cd.catalogue.first().cloned();
            match row {
                Some(row) => {
                    let holiday = economy::holidays::get_holiday_id(
                        clock.total_minutes(),
                        i32::try_from(REGION).unwrap_or(0),
                    );
                    let identify = identify_cost(row.value, holiday);
                    put_text(
                        state,
                        "m39.merchant.service",
                        TradeMode::Identify.name(),
                        &mut outcomes,
                    )?;
                    put_i64(
                        state,
                        "m39.merchant.charged",
                        identify,
                        &mut outcomes,
                        &mut delta,
                    )?;
                    let repair = repair_cost(
                        row.value,
                        quality,
                        i32::try_from(row.condition).unwrap_or(i32::MAX),
                        i32::try_from(row.max_condition).unwrap_or(i32::MAX),
                        None,
                    );
                    put_text(
                        state,
                        "m39.merchant.service",
                        TradeMode::Repair.name(),
                        &mut outcomes,
                    )?;
                    put_i64(state, "m39.merchant.charged", repair, &mut outcomes, &mut delta)?;
                }
                None => outcomes.push(M39State::rejected(RULE_NO_ITEM_TABLE)),
            }
        }
        "DaggerfallRestWindow" => {
            let mut rest = RestWindow::new(
                RestMode::TimedRest,
                RestVitals::new(
                    state.hud.health,
                    state.hud.max_health,
                    state.hud.stamina,
                    state.hud.max_stamina,
                    0,
                    0,
                ),
                SLEEP_EVENT_MINIMUM_HOURS,
            );
            let _ = rest.run(|_| false);
            let hours = u16::try_from(rest.total_hours).map_err(|_| UiCompatError::Text(RULE_RANGE))?;
            put_i64(state, "m39.rest.hours", i64::from(hours), &mut outcomes, &mut delta)?;
            put_i64(state, "m39.rest.cost", room_cost(0, day), &mut outcomes, &mut delta)?;
            outcomes.push(ModalOutcome::Closed);
        }
        "DaggerfallTavernWindow" => {
            put_i64(
                state,
                "m39.inn.room_price",
                room_cost(1, day),
                &mut outcomes,
                &mut delta,
            )?;
            let expiry = room_expiry_seconds(RENTAL_ORIGIN_SECONDS, 1);
            let hours = room_remaining_hours(RENTAL_ORIGIN_SECONDS, expiry);
            let hours = u16::try_from(hours).map_err(|_| UiCompatError::Text(RULE_RANGE))?;
            put_i64(state, "m39.rest.hours", i64::from(hours), &mut outcomes, &mut delta)?;
            put_i64(
                state,
                "m39.inn.room_price",
                room_cost(MAX_RENT_DAYS, day),
                &mut outcomes,
                &mut delta,
            )?;
            if !room_rental_allowed(1, MAX_RENT_DAYS) {
                outcomes.push(M39State::rejected(RULE_RENT_LIMIT));
            }
        }
        "DaggerfallTradeWindow" => {
            let catalogue = state.cd.catalogue.clone();
            let mercantile = state.cd.mercantile;
            let personality = state.cd.personality;
            let selected = state.cd.selected_row;
            if catalogue.is_empty() {
                outcomes.push(M39State::rejected(RULE_TRADE_EMPTY));
            } else {
                let quality_i32 = i32::from(quality);
                let quote = |row: &CatalogueRow| -> (i64, i64) {
                    let display = calculate_cost(row.value, quality, None);
                    let cost = i32::try_from(display).unwrap_or(i32::MAX);
                    let charged = i64::from(calculate_trade_price(
                        cost,
                        quality_i32,
                        false,
                        mercantile,
                        personality,
                    ));
                    (display, charged)
                };
                let rows = u16::try_from(catalogue.len())
                    .map_err(|_| UiCompatError::Text(RULE_RANGE))?;
                put_i64(state, "m39.trade.rows", i64::from(rows), &mut outcomes, &mut delta)?;
                let (display, charged) = quote(&catalogue[selected % catalogue.len()]);
                put_i64(state, "m39.trade.display_price", display, &mut outcomes, &mut delta)?;
                put_i64(
                    state,
                    "m39.trade.charged_price",
                    charged,
                    &mut outcomes,
                    &mut delta,
                )?;
                // Next row with a different charge: the window must move.
                let next = (1..catalogue.len())
                    .map(|step| (selected + step) % catalogue.len())
                    .find(|&index| quote(&catalogue[index]).1 != charged);
                if let Some(index) = next {
                    let (display, charged) = quote(&catalogue[index]);
                    state.cd.selected_row = index;
                    put_i64(
                        state,
                        "m39.trade.display_price",
                        display,
                        &mut outcomes,
                        &mut delta,
                    )?;
                    put_i64(
                        state,
                        "m39.trade.charged_price",
                        charged,
                        &mut outcomes,
                        &mut delta,
                    )?;
                    state.cd.basket = vec![selected % catalogue.len(), index];
                    let mut total_display = 0i64;
                    let mut total_charged = 0i64;
                    for &row_index in &state.cd.basket {
                        let (display, charged) = quote(&catalogue[row_index % catalogue.len()]);
                        total_display = total_display.saturating_add(display);
                        total_charged = total_charged.saturating_add(charged);
                    }
                    put_i64(
                        state,
                        "m39.trade.total_display",
                        total_display,
                        &mut outcomes,
                        &mut delta,
                    )?;
                    put_i64(
                        state,
                        "m39.trade.total_charged",
                        total_charged,
                        &mut outcomes,
                        &mut delta,
                    )?;
                }
                // Impossible intent: the shop refuses this item group in sell mode.
                let refused = catalogue.iter().find(|row| {
                    match economy_group(row.group) {
                        Some(group) => !economy::sale_offer(
                            economy::BuildingType::GeneralStore,
                            TradeMode::Sell,
                            group,
                            false,
                        ),
                        None => true,
                    }
                });
                match refused {
                    Some(_) => outcomes.push(M39State::rejected(RULE_SELL_REFUSED)),
                    None => outcomes.push(ModalOutcome::Closed),
                }
            }
        }
        _ => return Err(UiCompatError::Navigation(RULE_UNKNOWN_WINDOW)),
    }

    deliver(state, 'D', id, name, outcomes, delta)
}

/// Evidence of block C.
pub fn evidence(state: &M39State) -> BlockEvidence {
    evidence_of(state, 'C', BLOCK_C_WINDOWS)
}

/// Evidence of block D.
pub fn evidence_d(state: &M39State) -> BlockEvidence {
    evidence_of(state, 'D', BLOCK_D_WINDOWS)
}

/// Counts the delivered flows of one block; `OK` only when every window of the
/// block delivered a validated flow with at least one real delta.
fn evidence_of(state: &M39State, block: char, expected: usize) -> BlockEvidence {
    let flows: Vec<&CdFlow> = state
        .cd
        .flows
        .iter()
        .filter(|flow| flow.block == block)
        .collect();
    if flows.is_empty() {
        return BlockEvidence::pending(block, expected);
    }
    let intents: usize = flows.iter().map(|flow| flow.intents).sum();
    let delivered: usize = flows.iter().map(|flow| flow.delivered).sum();
    let state_delta: usize = flows.iter().map(|flow| flow.changed).sum();
    let ok = flows.len() == expected
        && delivered == intents
        && flows
            .iter()
            .all(|flow| flow.intents > 0 && flow.delivered == flow.intents && flow.changed > 0);
    BlockEvidence {
        block,
        windows: flows.len(),
        flows: flows.len(),
        intents,
        delivered,
        state_delta,
        read_only: 0,
        ok,
        reason: if ok { None } else { Some(RULE_INCOMPLETE) },
    }
}

#[cfg(test)]
mod m39_block_cd_tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    use daggerfall_parser_rs::texture::Palette;
    use daggerfall_perception::read_models::{M39CommerceModels, M39InventoryModels};
    use daggerfall_skills::career::CFG_BYTES;

    use super::*;

    /// Scratch `ARENA2` directory carrying one synthetic career and one
    /// synthetic `FALL.EXE` item table, removed on drop.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    static NEXT_SCRATCH: AtomicU32 = AtomicU32::new(0);

    /// 74-byte synthetic career (`CLASS01.CFG` shape), attributes
    /// `60/50/55/50/50/40/45/50` (Strength 60, Personality 50).
    fn synthetic_career_bytes() -> Vec<u8> {
        let mut bytes = vec![0u8; CFG_BYTES];
        bytes[4..6].copy_from_slice(&0x0C00u16.to_le_bytes());
        bytes[16..19].copy_from_slice(&[31, 29, 32]);
        bytes[19..22].copy_from_slice(&[22, 24, 25]);
        bytes[22..28].copy_from_slice(&[23, 26, 27, 28, 30, 33]);
        bytes[28..38].copy_from_slice(b"Spellsword");
        bytes[52..54].copy_from_slice(&12u16.to_le_bytes());
        bytes[54..58].copy_from_slice(&0x0001_0000u32.to_le_bytes());
        let attributes: [u16; 8] = [60, 50, 55, 50, 50, 40, 45, 50];
        for (index, value) in attributes.iter().enumerate() {
            bytes[58 + index * 2..60 + index * 2].copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    /// Writes one 48-byte item record at `index` (plan §1.2 layout).
    fn write_item(blob: &mut [u8], index: usize, name: &str, weight_units: i32, price: i32) {
        let start = index * daggerfall_inventory::items::ITEM_RECORD_SIZE;
        let record = &mut blob[start..start + daggerfall_inventory::items::ITEM_RECORD_SIZE];
        let bytes = name.as_bytes();
        record[..bytes.len()].copy_from_slice(bytes);
        record[24..28].copy_from_slice(&weight_units.to_le_bytes());
        record[28..30].copy_from_slice(&200u16.to_le_bytes());
        record[30..34].copy_from_slice(&0i32.to_le_bytes());
        record[34..38].copy_from_slice(&price.to_le_bytes());
    }

    /// `FALL.EXE` shaped blob whose item table starts at the `Ruby` sentinel
    /// (offset 0), with one weapon, one body armour and one shield.
    fn synthetic_fall_exe() -> Vec<u8> {
        let mut blob = vec![
            0u8;
            daggerfall_inventory::items::TOTAL_ITEMS
                * daggerfall_inventory::items::ITEM_RECORD_SIZE
        ];
        blob[..daggerfall_inventory::items::RUBY_SENTINEL.len()]
            .copy_from_slice(daggerfall_inventory::items::RUBY_SENTINEL);
        write_item(&mut blob, 102, "Cuirass", 80, 1_000);
        write_item(&mut blob, 110, "Buckler", 40, 400);
        write_item(&mut blob, 113, "Longsword", 60, 2_000);
        blob
    }

    fn retail_dir() -> Scratch {
        let serial = NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "daggerfall-m39-cd-{}-{serial}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("scratch arena2");
        std::fs::write(dir.join(M20_CLASS_FILE), synthetic_career_bytes()).expect("career write");
        std::fs::write(
            dir.join(FALL_EXE_FILENAME),
            synthetic_fall_exe(),
        )
        .expect("item table write");
        Scratch(dir)
    }

    fn zero_palette() -> Palette {
        Palette {
            colors: [[0u8; 3]; 256],
        }
    }

    fn empty_assets() -> UiAssets {
        UiAssets::new(zero_palette(), zero_palette(), zero_palette())
    }

    fn state(dir: &Scratch) -> M39State {
        M39State::new(&empty_assets(), &dir.0).expect("state construction")
    }

    fn delivered(dir: &Scratch) -> (M39State, Vec<Interaction>) {
        let mut state = state(dir);
        let mut interactions = Vec::new();
        for name in BLOCK_C_NAMES.iter().chain(BLOCK_D_NAMES.iter()) {
            let mut rows = interact(&mut state, name).expect("delivery");
            assert_eq!(rows.len(), 1, "{name}");
            interactions.push(rows.remove(0));
        }
        (state, interactions)
    }

    #[test]
    fn every_window_delivers_validated_deltas() {
        let dir = retail_dir();
        let (_state, interactions) = delivered(&dir);
        assert_eq!(interactions.len(), BLOCK_C_WINDOWS + BLOCK_D_WINDOWS);

        let c_paths = M39InventoryModels::PATHS;
        let d_paths = M39CommerceModels::PATHS;
        for interaction in &interactions {
            assert!(interaction.intents > 0, "{}", interaction.name);
            assert_eq!(interaction.delivered, interaction.outcomes.len());
            assert_eq!(interaction.intents, interaction.delivered);
            assert!(!interaction.delta.is_empty(), "{}", interaction.name);
            assert!(interaction.read_only.is_none(), "{}", interaction.name);
            assert_eq!(interaction.digest.len(), 16);
            assert_eq!(
                interaction.id,
                WINDOWS
                    .iter()
                    .find(|(_, window)| *window == interaction.name)
                    .map(|(ordinal, _)| *ordinal)
                    .expect("census ordinal"),
                "{}",
                interaction.name
            );
            let owned = match interaction.block {
                'C' => c_paths,
                'D' => d_paths,
                other => panic!("unexpected block {other}"),
            };
            for outcome in &interaction.outcomes {
                M39State::validate(outcome).expect("valid outcome");
                if let ModalOutcome::Committed { read_model, .. } = outcome {
                    assert!(
                        owned.contains(read_model),
                        "{} wrote {read_model} outside its block",
                        interaction.name
                    );
                }
            }
            for (path, _) in &interaction.delta {
                assert!(owned.contains(path), "{} delta on {path}", interaction.name);
            }
        }
    }

    #[test]
    fn two_runs_yield_the_same_digests_and_state() {
        let dir = retail_dir();
        let (first_state, first) = delivered(&dir);
        let (second_state, second) = delivered(&dir);
        assert_eq!(first_state, second_state);
        assert_eq!(first, second);
        for (left, right) in first.iter().zip(second.iter()) {
            assert_eq!(left.digest, right.digest, "{}", left.name);
            assert_eq!(left.signature(), right.signature(), "{}", left.name);
        }
    }

    #[test]
    fn impossible_intents_are_rejected_with_a_reason() {
        let dir = retail_dir();
        let (_state, interactions) = delivered(&dir);
        let rejected: Vec<&ModalOutcome> = interactions
            .iter()
            .flat_map(|interaction| interaction.outcomes.iter())
            .filter(|outcome| matches!(outcome, ModalOutcome::Rejected { .. }))
            .collect();
        assert!(!rejected.is_empty());
        for outcome in rejected {
            match outcome {
                ModalOutcome::Rejected { reason } => assert!(!reason.is_empty()),
                other => panic!("expected Rejected, got {other:?}"),
            }
        }
    }

    #[test]
    fn evidence_counts_match_delivery_and_start_pending() {
        let dir = retail_dir();
        let mut state = state(&dir);
        assert_eq!(evidence(&state), BlockEvidence::pending('C', BLOCK_C_WINDOWS));
        assert_eq!(
            evidence_d(&state),
            BlockEvidence::pending('D', BLOCK_D_WINDOWS)
        );

        for name in BLOCK_C_NAMES {
            interact(&mut state, name).expect("C delivery");
        }
        let c = evidence(&state);
        assert!(c.ok, "{c:?}");
        assert_eq!(c.block, 'C');
        assert_eq!(c.windows, BLOCK_C_WINDOWS);
        assert_eq!(c.flows, BLOCK_C_WINDOWS);
        assert_eq!(c.delivered, c.intents);
        assert!(c.state_delta > 0);
        assert_eq!(c.read_only, 0);

        for name in BLOCK_D_NAMES {
            interact(&mut state, name).expect("D delivery");
        }
        let d = evidence_d(&state);
        assert!(d.ok, "{d:?}");
        assert_eq!(d.block, 'D');
        assert_eq!(d.windows, BLOCK_D_WINDOWS);
        assert_eq!(d.flows, BLOCK_D_WINDOWS);
        assert_eq!(d.delivered, d.intents);
        assert!(d.state_delta > 0);
    }

    #[test]
    fn unknown_window_is_an_error_and_replay_is_refused() {
        let dir = retail_dir();
        let mut state = state(&dir);
        assert!(interact(&mut state, "DaggerfallStartWindow").is_err());
        interact(&mut state, "DaggerfallTradeWindow").expect("first delivery");
        assert!(interact(&mut state, "DaggerfallTradeWindow").is_err());
    }

    #[test]
    fn inventory_and_sheet_read_the_retail_career_and_table() {
        let dir = retail_dir();
        let mut state = state(&dir);
        interact(&mut state, "DaggerfallCharacterSheetWindow").expect("sheet");
        interact(&mut state, "DaggerfallInventoryWindow").expect("inventory");
        assert_eq!(state.value("m39.sheet.level").unwrap(), "1");
        assert_eq!(state.value("m39.sheet.slots_filled").unwrap(), "2");
        assert_eq!(state.value("m39.inv.slots").unwrap(), "27");
        assert_eq!(state.value("m39.inv.used_slots").unwrap(), "2");
        assert_eq!(state.value("m39.inv.equipped.weapon").unwrap(), "Longsword");
        assert_eq!(state.value("m39.inv.equipped.armor").unwrap(), "Cuirass");
        assert_ne!(state.value("m39.inv.max_weight_g").unwrap(), "ABSENT");
        assert_ne!(state.value("m39.sheet.encumbrance_band").unwrap(), "ABSENT");
        // The bank pop-up has not been delivered yet: its route is still ABSENT.
        assert_eq!(state.value("m39.bank.purchase_price").unwrap(), "ABSENT");
    }

    /// End-to-end smoke over the real retail bank when the developer points
    /// `DAGGERFALL_ARENA2` at it. Absent variable or directory skips the probe
    /// (returns), so the synthetic suite stays hermetic.
    #[test]
    fn real_retail_arena2_delivers_both_blocks_when_configured() {
        let Ok(arena2) = std::env::var("DAGGERFALL_ARENA2") else {
            return;
        };
        let arena2 = PathBuf::from(arena2);
        if !arena2.is_dir() {
            return;
        }
        let mut state = M39State::new(&empty_assets(), &arena2).expect("state over real retail");
        for name in BLOCK_C_NAMES.iter().chain(BLOCK_D_NAMES.iter()) {
            interact(&mut state, name).unwrap_or_else(|error| panic!("{name}: {error}"));
        }
        let c = evidence(&state);
        let d = evidence_d(&state);
        assert!(c.ok, "block C not OK: {c:?}");
        assert!(d.ok, "block D not OK: {d:?}");
        assert_eq!(state.value("m39.inv.slots").unwrap(), "27");
        let rows: usize = state
            .value("m39.trade.rows")
            .unwrap()
            .parse()
            .unwrap_or(0);
        assert!(rows > 0, "retail catalogue empty");
    }
}

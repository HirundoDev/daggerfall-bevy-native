//! M39/UI-3 — blocks **E** (magia y efectos, 7 windows) and **F** (diario,
//! libros e historia, 3 windows) of plan §1/§2/§5.
//!
//! **Frozen surface (plan §4).** [`EfState`], [`EfState::from_retail`],
//! [`interact`], [`evidence`] and [`evidence_f`] are consumed verbatim by the
//! probe grammar of §4.1; the lane extends the *state*, never the vocabulary.
//!
//! **Provenance (plan §3.1: zero invention).** Every painted number comes from
//!
//! * `daggerfall_behavior::magic` — the classic effect registry
//!   ([`magic::templates`]), `SpellMaker`, `SpellCost` and
//!   `MAX_EFFECTS_PER_SPELL`;
//! * `daggerfall_crafting::magic_def` — the retail `MAGIC.DEF` record table
//!   (`parse_magic_def`: name, `value` in gold and `uses` charges);
//! * `daggerfall_crafting::alchemy` — the classic recipe table
//!   (`CLASSIC_RECIPES`) and `brew`, which resolves the potion price;
//! * `daggerfall_parser_rs::spells_std` — the retail `SPELLS.STD` standard
//!   spell list (name, gold cost, icon);
//! * `daggerfall_parser_rs::{quest, text_rsc}` — the retail `*.QRC` journal
//!   records and the retail `books/BOK*.TXT` page rasters;
//! * `daggerfall_skills::{bio, career, character}` — the retail `BIO.DAT`
//!   biography and the retail `CLASS01.CFG` live magic skills.
//!
//! When a retail datum is absent (no `MAGIC.DEF`, no `SPELLS.STD`, no book,
//! no `BIO.DAT`, no `*.QRC`) the affected intent is `Rejected` with a citable
//! reason — never an invented zero.
//!
//! **Determinism.** The state stores no `f32`, no clock and no hashed pointer:
//! registry tables keep their declaration order and every retail directory is
//! read in file-name order, so two runs over the same retail pair hand back
//! equal [`Interaction`]s, equal digests and equal [`M39State`]s.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use daggerfall_behavior::magic::{
    self, EffectEntry, EffectProperties, EffectSettings, SpellMaker, MAX_EFFECTS_PER_SPELL,
};
use daggerfall_crafting::alchemy::{self, CLASSIC_RECIPES};
use daggerfall_crafting::magic_def::parse_magic_def;
use daggerfall_parser_bevy_bridge::ui_compat::UiCompatError;
use daggerfall_parser_bevy_bridge::ui_spec::UiAssets;
use daggerfall_parser_rs::quest::Qrc;
use daggerfall_parser_rs::spells_std::parse_spells_std;
use daggerfall_parser_rs::text_rsc::Book;
use daggerfall_skills::bio::Biography;
use daggerfall_skills::career::Career;
use daggerfall_skills::character::{Character, Skill};
use daggerfall_skills::rng::Rng64;

use crate::m18_demo::{M19_SEED, M20_CLASS_FILE};
use crate::m39_state::{BlockEvidence, Interaction, M39State, ModalOutcome};
use crate::m39_surfaces::surfaces;

/// Block E windows (plan §1/§2: magia y efectos).
pub const BLOCK_E_WINDOWS: usize = 7;
/// Block F windows (plan §1/§2: diario, libros e historia).
pub const BLOCK_F_WINDOWS: usize = 3;
/// Numeric/text routes owned by block E (plan §4/§5).
pub const E_ROUTES: usize = 12;
/// Numeric/text routes owned by block F (plan §4/§5).
pub const F_ROUTES: usize = 10;

/// Census ordinals of block E, in plan §1 order.
pub const BLOCK_E_IDS: [u16; BLOCK_E_WINDOWS] = [32, 41, 51, 56, 57, 70, 82];
/// Census ordinals of block F, in plan §1 order.
pub const BLOCK_F_IDS: [u16; BLOCK_F_WINDOWS] = [27, 49, 52];

const RULE_UNKNOWN_WINDOW: &str =
    "m39.ef: window is not one of the block E/F Playable surfaces of plan §1";
const RULE_INCOMPLETE: &str =
    "m39.ef: block evidence is incomplete (windows, routes, intents or deliveries missing)";
const RULE_RANGE: &str = "m39.ef: retail value does not fit the read-model domain";
const RULE_TYPE: &str = "m39.ef: read-model rejected the value type";
const RULE_CAREER_READ: &str = "m39.ef: retail CLASS01.CFG is unreadable";
const RULE_CAREER_PARSE: &str = "m39.ef: retail CLASS01.CFG is not a career record";
const RULE_REGISTRY: &str = "m39.ef: classic effect registry lost a key it declared";
const RULE_TEMPLATES_EMPTY: &str = "m39.ef: the classic effect registry is empty";
const RULE_TEMPLATES_EXHAUSTED: &str =
    "m39.ef: every registry effect is already in the composition";
const RULE_EFFECT_LIMIT: &str = "m39.ef: spell composition already holds MAX_EFFECTS_PER_SPELL";
const RULE_MAGIC_DEF_ABSENT: &str = "m39.ef: retail MAGIC.DEF carries no regular item template";
const RULE_ITEMS_EXHAUSTED: &str = "m39.ef: retail MAGIC.DEF has no further item template";
const RULE_CHARGES_SPENT: &str = "m39.ef: the selected magic item has no charges left";
const RULE_ALCHEMY_EXHAUSTED: &str =
    "m39.ef: the classic alchemy table has no further distinct-price recipe";
const RULE_SPELLS_STD_ABSENT: &str = "m39.ef: retail SPELLS.STD carries no standard spell";
const RULE_SPELLS_EXHAUSTED: &str = "m39.ef: retail SPELLS.STD has no further standard spell";
const RULE_ICONS_ABSENT: &str = "m39.ef: retail SPELLS.STD carries no spell icon";
const RULE_ICONS_EXHAUSTED: &str = "m39.ef: the retail spell icon list is exhausted";
const RULE_BOOKS_ABSENT: &str = "m39.ef: retail books/BOK*.TXT is absent";
const RULE_BOOK_END: &str = "m39.ef: the retail book has no further page";
const RULE_BIO_ABSENT: &str = "m39.ef: retail BIO.DAT carries no history entry";
const RULE_BIO_END: &str = "m39.ef: the retail biography has no further entry";
const RULE_JOURNAL_ABSENT: &str = "m39.ef: retail *.QRC carries no journal record";
const RULE_JOURNAL_END: &str = "m39.ef: the retail journal has no further record";

const SPELLS_STD_FILE: &str = "SPELLS.STD";
const MAGIC_DEF_FILE: &str = "MAGIC.DEF";
const BIO_FILE: &str = "BIO.DAT";
const BOOKS_DIR: &str = "books";
const QRC_EXTENSION: &str = "qrc";

const PATH_ELEMENTS: &str = "m39.spellmaker.elements";
const PATH_COMPOSED_COST: &str = "m39.spellmaker.composed_cost";
const PATH_SPELLMAKER_PERSISTED: &str = "m39.spellmaker.persisted";
const PATH_ITEM_COST: &str = "m39.itemmaker.cost";
const PATH_ITEM_PERSISTED: &str = "m39.itemmaker.persisted";
const PATH_POTION_COST: &str = "m39.potionmaker.cost";
const PATH_POTION_PERSISTED: &str = "m39.potionmaker.persisted";
const PATH_SPELL_COUNT: &str = "m39.spellbook.count";
const PATH_SPELL_NAME: &str = "m39.spellbook.selected.name";
const PATH_SPELL_COST: &str = "m39.spellbook.selected.cost";
const PATH_CHARGES: &str = "m39.magicitem.charges";
const PATH_ICON: &str = "m39.spellicon.selected";

const PATH_BOOK_PAGES: &str = "m39.book.pages";
const PATH_BOOK_PAGE: &str = "m39.book.page_index";
const PATH_HISTORY: &str = "m39.history.entries";
const PATH_JOURNAL_ENTRIES: &str = "m39.journal.entries";
const PATH_JOURNAL_TEXT: &str = "m39.journal.selected.text";
const PATH_JOURNAL_CHARS: &str = "m39.journal.line_chars";
const PATH_JOURNAL_LIMIT: &str = "m39.journal.line_limit";
const PATH_JOURNAL_PREFIXES: &str = "m39.journal.prefixes";
const PATH_NOTES_COUNT: &str = "m39.notes.count";
const PATH_NOTES_PERSISTED: &str = "m39.notes.persisted";

/// One regular `MAGIC.DEF` template reduced to the fields blocks E paint.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ItemRow {
    name: String,
    value: i32,
    uses: i16,
}

/// One `SPELLS.STD` standard spell reduced to the fields block E paints.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SpellRow {
    name: String,
    cost: i64,
    icon: u16,
}

/// One composed spell the spellmaker persisted into the session book.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionSpell {
    name: String,
    effects: usize,
    icon: u16,
    cost: u16,
}

/// One journal record read from a retail `*.QRC`, with the classic `%macro:`
/// prefix flag observed on its first line.
#[derive(Debug, Clone, PartialEq, Eq)]
struct JournalRow {
    text: String,
    prefixed: bool,
}

/// Session state of blocks E and F (plan §4: the lane extends the state).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EfState {
    /// Registry keys of the classic effect table, in declaration order.
    template_keys: Vec<&'static str>,
    /// Effects of the in-flight spellmaker composition, in add order.
    effect_keys: Vec<&'static str>,
    /// Live magic-skill values per career-skill index (`CLASS01.CFG`).
    skills: Vec<(i32, i16)>,
    /// Icon chosen for the composed spell (first retail spell icon).
    maker_icon: u16,
    /// Spells the spellmaker persisted this session.
    session_spells: Vec<SessionSpell>,
    /// Regular retail `MAGIC.DEF` templates, in file order.
    magic_items: Vec<ItemRow>,
    /// Cursor over [`EfState::magic_items`] for the itemmaker window.
    item_cursor: usize,
    /// Index into [`EfState::magic_items`] of the item being used.
    charge_slot: Option<usize>,
    /// Charges left on the selected magic item.
    charges: i32,
    /// Retail `SPELLS.STD` standard spells, in file order.
    standard_spells: Vec<SpellRow>,
    /// Cursor over [`EfState::standard_spells`] for the spellbook window.
    spell_cursor: usize,
    /// Distinct retail spell icons, ascending.
    icons: Vec<u16>,
    /// Cursor over [`EfState::icons`].
    icon_cursor: usize,
    /// Cursor over [`CLASSIC_RECIPES`].
    recipe_cursor: usize,
    /// Price of the last brewed potion (drives "next distinct price").
    last_recipe_price: Option<i32>,
    /// Recipe keys brewed this session.
    brewed: Vec<i32>,
    /// Retail book, when `books/BOK*.TXT` is present.
    book: Option<Book>,
    /// Page cursor over [`EfState::book`].
    page: usize,
    /// Retail biography lines (`BIO.DAT`), in file order.
    bio_lines: Vec<String>,
    /// Biography lines revealed so far.
    revealed: usize,
    /// Retail journal records, in file-name then record order.
    journal: Vec<JournalRow>,
    /// Cursor over [`EfState::journal`].
    journal_cursor: usize,
    /// Journal records the player copied into the notes.
    notes: Vec<String>,
    /// Block E/F windows that delivered at least one interaction.
    windows: BTreeSet<u16>,
    /// Every route a block E/F interaction committed.
    changed: BTreeSet<&'static str>,
    /// Every **numeric** route committed (the `state_delta` count).
    numeric: BTreeSet<&'static str>,
    /// Intents block E served.
    intents_e: usize,
    /// Intents block E delivered (one per interaction with ≥1 outcome).
    delivered_e: usize,
    /// Intents block F served.
    intents_f: usize,
    /// Intents block F delivered (one per interaction with ≥1 outcome).
    delivered_f: usize,
}

impl EfState {
    /// Builds the block E/F session from the retail pair.
    ///
    /// The career record is mandatory (the app already requires
    /// `CLASS01.CFG`); every other retail file is optional and simply leaves
    /// its window with a citable `Rejected` reason when absent.
    pub fn from_retail(_assets: &UiAssets, arena2: &Path) -> Result<Self, UiCompatError> {
        let career = read_career(arena2)?;
        let mut state = Self::default();
        state.template_keys = magic::templates().map(|props| props.key).collect();
        state.skills = live_magic_skills(&career);
        state.magic_items = read_magic_items(arena2);
        state.standard_spells = read_standard_spells(arena2);
        state.icons = state
            .standard_spells
            .iter()
            .map(|spell| spell.icon)
            .collect::<BTreeSet<u16>>()
            .into_iter()
            .collect();
        state.maker_icon = state.icons.first().copied().unwrap_or(0);
        state.book = read_book(arena2);
        state.bio_lines = read_biography(arena2);
        state.journal = read_journal(arena2);
        Ok(state)
    }

    /// Rebuilds the in-flight spellmaker composition from its registry keys.
    fn maker(&self) -> Result<SpellMaker, UiCompatError> {
        let icon = u8::try_from(self.maker_icon).unwrap_or(0);
        let mut maker = SpellMaker::new(icon, magic::TargetType::CasterOnly);
        for key in &self.effect_keys {
            let props =
                magic::effect_by_key(key).ok_or(UiCompatError::Text(RULE_REGISTRY))?;
            maker
                .add_effect(EffectEntry {
                    props,
                    settings: EffectSettings::classic_default(),
                })
                .map_err(|_| UiCompatError::Input(RULE_EFFECT_LIMIT))?;
        }
        Ok(maker)
    }

    /// Live magic skill for one effect property, `0` when the career has no
    /// such skill (the classic cost formula then uses the floor).
    fn live(&self, props: &EffectProperties) -> i32 {
        props
            .magic_skill
            .career_skill_index()
            .and_then(|index| self.skills.iter().find(|(i, _)| *i == index))
            .map(|(_, value)| i32::from(*value))
            .unwrap_or(0)
    }
}

/// Dispatches one block E/F intent by the census ordinal of `name`.
pub fn interact(
    state: &mut M39State,
    name: &'static str,
) -> Result<Vec<Interaction>, UiCompatError> {
    let surface = surfaces()
        .iter()
        .find(|surface| surface.pilot.key == name)
        .ok_or(UiCompatError::Navigation(RULE_UNKNOWN_WINDOW))?;
    let mut delivery = Delivery::new(state, surface.id, name, surface.block);
    if let Some(reason) = surface.read_only {
        delivery.outcomes.push(ModalOutcome::ReadOnly { reason });
    } else {
        match surface.block {
            'E' => interact_e(surface.id, &mut delivery)?,
            'F' => interact_f(surface.id, &mut delivery)?,
            _ => return Err(UiCompatError::Navigation(RULE_UNKNOWN_WINDOW)),
        }
    }
    delivery.finish()
}

/// Block E evidence (plan §4: `windows = 7`, `flows = 12`).
pub fn evidence(state: &M39State) -> BlockEvidence {
    block_evidence(state, 'E', BLOCK_E_WINDOWS, E_ROUTES)
}

/// Block F evidence (plan §4: `windows = 3`, `flows = 10`).
pub fn evidence_f(state: &M39State) -> BlockEvidence {
    block_evidence(state, 'F', BLOCK_F_WINDOWS, F_ROUTES)
}

fn block_evidence(
    state: &M39State,
    block: char,
    windows: usize,
    routes: usize,
) -> BlockEvidence {
    let delivered_windows = state
        .ef
        .windows
        .iter()
        .filter(|id| block_of(**id) == Some(block))
        .count();
    let flows = state
        .ef
        .changed
        .iter()
        .filter(|path| route_block(path) == Some(block))
        .count();
    let state_delta = state
        .ef
        .numeric
        .iter()
        .filter(|path| route_block(path) == Some(block))
        .count();
    let (intents, delivered) = match block {
        'E' => (state.ef.intents_e, state.ef.delivered_e),
        _ => (state.ef.intents_f, state.ef.delivered_f),
    };
    if delivered_windows == 0 && intents == 0 {
        return BlockEvidence::pending(block, windows);
    }
    let ok = delivered_windows == windows && flows == routes && intents > 0 && delivered > 0;
    BlockEvidence {
        block,
        windows,
        flows,
        intents,
        delivered,
        state_delta,
        read_only: 0,
        ok,
        reason: if ok { None } else { Some(RULE_INCOMPLETE) },
    }
}

fn block_of(id: u16) -> Option<char> {
    crate::m39_surfaces::block_of(id)
}

/// Block letter that owns one read-model route (plan §5).
fn route_block(path: &str) -> Option<char> {
    if path.starts_with("m39.spellmaker.")
        || path.starts_with("m39.itemmaker.")
        || path.starts_with("m39.potionmaker.")
        || path == PATH_SPELL_COUNT
        || path.starts_with("m39.spellbook.")
        || path.starts_with("m39.magicitem.")
        || path.starts_with("m39.spellicon.")
    {
        Some('E')
    } else if path.starts_with("m39.book.")
        || path.starts_with("m39.history.")
        || path.starts_with("m39.journal.")
        || path.starts_with("m39.notes.")
    {
        Some('F')
    } else {
        None
    }
}

fn interact_e(id: u16, delivery: &mut Delivery<'_>) -> Result<(), UiCompatError> {
    match id {
        32 => effect_editor(delivery),
        41 => item_maker(delivery),
        51 => potion_maker(delivery),
        56 => spell_book(delivery),
        57 => spell_maker(delivery),
        70 => use_magic_item(delivery),
        82 => icon_picker(delivery),
        _ => Err(UiCompatError::Navigation(RULE_UNKNOWN_WINDOW)),
    }
}

fn interact_f(id: u16, delivery: &mut Delivery<'_>) -> Result<(), UiCompatError> {
    match id {
        27 => book_reader(delivery),
        49 => player_history(delivery),
        52 => quest_journal(delivery),
        _ => Err(UiCompatError::Navigation(RULE_UNKNOWN_WINDOW)),
    }
}

/// `032 DaggerfallEffectSettingsEditorWindow` — adds one registry effect to
/// the spellmaker composition (plan §1/§4: `m39.spellmaker.elements`).
fn effect_editor(delivery: &mut Delivery<'_>) -> Result<(), UiCompatError> {
    let state = &mut *delivery.state;
    if state.ef.effect_keys.len() >= MAX_EFFECTS_PER_SPELL {
        delivery.reject(RULE_EFFECT_LIMIT);
        return Ok(());
    }
    let next = state
        .ef
        .template_keys
        .iter()
        .find(|key| !state.ef.effect_keys.contains(key))
        .copied();
    let Some(key) = next else {
        let empty = state.ef.template_keys.is_empty();
        delivery.reject(if empty {
            RULE_TEMPLATES_EMPTY
        } else {
            RULE_TEMPLATES_EXHAUSTED
        });
        return Ok(());
    };
    state.ef.effect_keys.push(key);
    let elements = i64::try_from(state.ef.effect_keys.len())
        .map_err(|_| UiCompatError::Text(RULE_RANGE))?;
    delivery.numeric(PATH_ELEMENTS, elements)?;
    delivery.text_outcome(format!("{key} · {elements} effect(s) in the composition"));
    Ok(())
}

/// `057 DaggerfallSpellMakerWindow` — prices the composition and persists it
/// into the session book (`m39.spellmaker.composed_cost`,
/// `m39.spellmaker.persisted`).
fn spell_maker(delivery: &mut Delivery<'_>) -> Result<(), UiCompatError> {
    let state = &mut *delivery.state;
    if state.ef.effect_keys.is_empty() {
        let Some(key) = state.ef.template_keys.first().copied() else {
            delivery.reject(RULE_TEMPLATES_EMPTY);
            return Ok(());
        };
        state.ef.effect_keys.push(key);
    }
    let maker = state.ef.maker()?;
    let cost = maker.displayed_cost(&|props: &EffectProperties| state.ef.live(props));
    let points = u16::try_from(cost.spell_points).map_err(|_| UiCompatError::Text(RULE_RANGE))?;
    let name = state.ef.effect_keys.join(" + ");
    let effects = state.ef.effect_keys.len();
    let icon = state.ef.maker_icon;
    if !state.ef.session_spells.iter().any(|spell| spell.name == name) {
        state.ef.session_spells.push(SessionSpell {
            name: name.clone(),
            effects,
            icon,
            cost: points,
        });
    }
    delivery.numeric(PATH_COMPOSED_COST, i64::from(points))?;
    delivery.flag(PATH_SPELLMAKER_PERSISTED, true)?;
    delivery.text_outcome(format!(
        "{name} · {points} spell points · {effects} effect(s)"
    ));
    Ok(())
}

/// `041 DaggerfallItemMakerWindow` — sells the next regular `MAGIC.DEF`
/// template at its retail gold value (`m39.itemmaker.cost`,
/// `m39.itemmaker.persisted`).
fn item_maker(delivery: &mut Delivery<'_>) -> Result<(), UiCompatError> {
    let state = &mut *delivery.state;
    if state.ef.magic_items.is_empty() {
        delivery.reject(RULE_MAGIC_DEF_ABSENT);
        return Ok(());
    }
    let cursor = state.ef.item_cursor;
    let Some(item) = state.ef.magic_items.get(cursor).cloned() else {
        delivery.reject(RULE_ITEMS_EXHAUSTED);
        return Ok(());
    };
    state.ef.item_cursor += 1;
    delivery.numeric(PATH_ITEM_COST, i64::from(item.value))?;
    delivery.flag(PATH_ITEM_PERSISTED, true)?;
    delivery.text_outcome(format!(
        "{} · {} gold · {} charge(s)",
        item.name, item.value, item.uses
    ));
    Ok(())
}

/// `051 DaggerfallPotionMakerWindow` — brews the next distinct-price classic
/// recipe (`m39.potionmaker.cost`, `m39.potionmaker.persisted`).
fn potion_maker(delivery: &mut Delivery<'_>) -> Result<(), UiCompatError> {
    let state = &mut *delivery.state;
    let total = CLASSIC_RECIPES.len();
    let start = state.ef.recipe_cursor % total;
    let mut chosen: Option<(usize, i32, i32, String)> = None;
    for offset in 0..total {
        let index = (start + offset) % total;
        let recipe = &CLASSIC_RECIPES[index];
        let Some(potion) = alchemy::brew(recipe.ingredients) else {
            continue;
        };
        let price = potion.price;
        if state.ef.last_recipe_price == Some(price) {
            continue;
        }
        chosen = Some((index, potion.recipe.key(), price, recipe_key_label(index)));
        break;
    }
    let Some((index, key, price, label)) = chosen else {
        delivery.reject(RULE_ALCHEMY_EXHAUSTED);
        return Ok(());
    };
    state.ef.recipe_cursor = (index + 1) % total;
    state.ef.last_recipe_price = Some(price);
    state.ef.brewed.push(key);
    delivery.numeric(PATH_POTION_COST, i64::from(price))?;
    delivery.flag(PATH_POTION_PERSISTED, true)?;
    delivery.text_outcome(format!("{label} · {price} gold"));
    Ok(())
}

/// Retail-neutral label of a classic recipe slot: the table index and its
/// display key (never an invented potion name).
fn recipe_key_label(index: usize) -> String {
    let recipe = &CLASSIC_RECIPES[index];
    format!("recipe {} · {}", index + 1, recipe.display_name_key)
}

/// `056 DaggerfallSpellBookWindow` — pages through the retail `SPELLS.STD`
/// standard spells (`m39.spellbook.count`, `…selected.name`, `…selected.cost`).
fn spell_book(delivery: &mut Delivery<'_>) -> Result<(), UiCompatError> {
    let state = &mut *delivery.state;
    let total = state.ef.standard_spells.len();
    if total == 0 {
        delivery.reject(RULE_SPELLS_STD_ABSENT);
        return Ok(());
    }
    let cursor = state.ef.spell_cursor;
    let Some(spell) = state.ef.standard_spells.get(cursor).cloned() else {
        delivery.reject(RULE_SPELLS_EXHAUSTED);
        return Ok(());
    };
    state.ef.spell_cursor += 1;
    delivery.numeric(
        PATH_SPELL_COUNT,
        i64::try_from(total).map_err(|_| UiCompatError::Text(RULE_RANGE))?,
    )?;
    delivery.text(PATH_SPELL_NAME, &spell.name)?;
    delivery.numeric(PATH_SPELL_COST, spell.cost)?;
    delivery.text_outcome(format!("{} · {} gold", spell.name, spell.cost));
    Ok(())
}

/// `070 DaggerfallUseMagicItemWindow` — consumes one charge of the selected
/// retail magic item (`m39.magicitem.charges`).
fn use_magic_item(delivery: &mut Delivery<'_>) -> Result<(), UiCompatError> {
    let state = &mut *delivery.state;
    if state.ef.charge_slot.is_none() {
        let Some((index, item)) = state
            .ef
            .magic_items
            .iter()
            .enumerate()
            .find(|(_, item)| item.uses > 0)
        else {
            let empty = state.ef.magic_items.is_empty();
            delivery.reject(if empty {
                RULE_MAGIC_DEF_ABSENT
            } else {
                RULE_CHARGES_SPENT
            });
            return Ok(());
        };
        state.ef.charge_slot = Some(index);
        state.ef.charges = i32::from(item.uses);
    }
    if state.ef.charges <= 0 {
        delivery.reject(RULE_CHARGES_SPENT);
        return Ok(());
    }
    state.ef.charges -= 1;
    let left = state.ef.charges;
    delivery.numeric(PATH_CHARGES, i64::from(left))?;
    delivery.text_outcome(format!("{left} charge(s) left"));
    Ok(())
}

/// `082 SpellIconPickerWindow` — walks the distinct retail spell icons
/// (`m39.spellicon.selected`).
fn icon_picker(delivery: &mut Delivery<'_>) -> Result<(), UiCompatError> {
    let state = &mut *delivery.state;
    if state.ef.icons.is_empty() {
        delivery.reject(RULE_ICONS_ABSENT);
        return Ok(());
    }
    let cursor = state.ef.icon_cursor;
    let Some(icon) = state.ef.icons.get(cursor).copied() else {
        delivery.reject(RULE_ICONS_EXHAUSTED);
        return Ok(());
    };
    state.ef.icon_cursor += 1;
    delivery.numeric(PATH_ICON, i64::from(icon))?;
    delivery.text_outcome(format!("icon {icon}"));
    Ok(())
}

/// `027 DaggerfallBookReaderWindow` — turns the pages of the first retail
/// `books/BOK*.TXT` (`m39.book.pages`, `m39.book.page_index`).
fn book_reader(delivery: &mut Delivery<'_>) -> Result<(), UiCompatError> {
    let state = &mut *delivery.state;
    let Some(book) = state.ef.book.as_ref() else {
        delivery.reject(RULE_BOOKS_ABSENT);
        return Ok(());
    };
    let pages = book.pages.len();
    if pages == 0 {
        delivery.reject(RULE_BOOKS_ABSENT);
        return Ok(());
    }
    let page = state.ef.page;
    if page >= pages {
        delivery.reject(RULE_BOOK_END);
        return Ok(());
    }
    let text = book
        .page_text(page)
        .filter(|text| !text.trim().is_empty())
        .unwrap_or_else(|| format!("page {}", page + 1));
    state.ef.page += 1;
    delivery.numeric(
        PATH_BOOK_PAGES,
        i64::try_from(pages).map_err(|_| UiCompatError::Text(RULE_RANGE))?,
    )?;
    delivery.numeric(
        PATH_BOOK_PAGE,
        i64::try_from(page).map_err(|_| UiCompatError::Text(RULE_RANGE))?,
    )?;
    delivery.text_outcome(text);
    Ok(())
}

/// `049 DaggerfallPlayerHistoryWindow` — reveals the next retail `BIO.DAT`
/// entry (`m39.history.entries`).
fn player_history(delivery: &mut Delivery<'_>) -> Result<(), UiCompatError> {
    let state = &mut *delivery.state;
    if state.ef.bio_lines.is_empty() {
        delivery.reject(RULE_BIO_ABSENT);
        return Ok(());
    }
    if state.ef.revealed >= state.ef.bio_lines.len() {
        delivery.reject(RULE_BIO_END);
        return Ok(());
    }
    let line = state.ef.bio_lines[state.ef.revealed].clone();
    state.ef.revealed += 1;
    let revealed = state.ef.revealed;
    delivery.numeric(
        PATH_HISTORY,
        i64::try_from(revealed).map_err(|_| UiCompatError::Text(RULE_RANGE))?,
    )?;
    delivery.text_outcome(line);
    Ok(())
}

/// `052 DaggerfallQuestJournalWindow` — reads the next retail `*.QRC` record,
/// copies it to the notes and paints the journal geometry
/// (`m39.journal.{entries,selected.text,line_chars,line_limit,prefixes}`,
/// `m39.notes.{count,persisted}`).
fn quest_journal(delivery: &mut Delivery<'_>) -> Result<(), UiCompatError> {
    let state = &mut *delivery.state;
    let total = state.ef.journal.len();
    if total == 0 {
        delivery.reject(RULE_JOURNAL_ABSENT);
        return Ok(());
    }
    let cursor = state.ef.journal_cursor;
    let Some(row) = state.ef.journal.get(cursor).cloned() else {
        delivery.reject(RULE_JOURNAL_END);
        return Ok(());
    };
    state.ef.journal_cursor += 1;
    state.ef.notes.push(row.text.clone());
    let line_chars = row.text.chars().count();
    let line_limit = state
        .ef
        .journal
        .iter()
        .flat_map(|row| row.text.lines())
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);
    let prefixes = state.ef.journal.iter().filter(|row| row.prefixed).count();
    let notes = state.ef.notes.len();
    delivery.numeric(
        PATH_JOURNAL_ENTRIES,
        i64::try_from(total).map_err(|_| UiCompatError::Text(RULE_RANGE))?,
    )?;
    delivery.text(PATH_JOURNAL_TEXT, &row.text)?;
    delivery.numeric(
        PATH_JOURNAL_CHARS,
        i64::try_from(line_chars).map_err(|_| UiCompatError::Text(RULE_RANGE))?,
    )?;
    delivery.numeric(
        PATH_JOURNAL_LIMIT,
        i64::try_from(line_limit).map_err(|_| UiCompatError::Text(RULE_RANGE))?,
    )?;
    delivery.numeric(
        PATH_JOURNAL_PREFIXES,
        i64::try_from(prefixes).map_err(|_| UiCompatError::Text(RULE_RANGE))?,
    )?;
    delivery.numeric(
        PATH_NOTES_COUNT,
        i64::try_from(notes).map_err(|_| UiCompatError::Text(RULE_RANGE))?,
    )?;
    delivery.flag(PATH_NOTES_PERSISTED, true)?;
    delivery.text_outcome(row.text);
    Ok(())
}

/// Collects the outcomes, the numeric delta and the changed routes of one
/// intent, then seals it into the frozen [`Interaction`].
struct Delivery<'a> {
    state: &'a mut M39State,
    id: u16,
    name: &'static str,
    block: char,
    outcomes: Vec<ModalOutcome>,
    delta: Vec<(&'static str, i64)>,
    changed: Vec<&'static str>,
    numeric: Vec<&'static str>,
}

impl<'a> Delivery<'a> {
    fn new(state: &'a mut M39State, id: u16, name: &'static str, block: char) -> Self {
        Self {
            state,
            id,
            name,
            block,
            outcomes: Vec::new(),
            delta: Vec::new(),
            changed: Vec::new(),
            numeric: Vec::new(),
        }
    }

    /// Commits one numeric route; an unchanged value is not an outcome.
    fn numeric(&mut self, path: &'static str, value: i64) -> Result<(), UiCompatError> {
        if self
            .state
            .value(path)
            .map(|current| current == value.to_string())
            .unwrap_or(false)
        {
            return Ok(());
        }
        let outcome = self.state.commit_i64(path, value)?;
        if let ModalOutcome::Committed { before, after, .. } = &outcome {
            let after_value = after.trim().parse::<i64>().unwrap_or(value);
            let before_value = before.trim().parse::<i64>().unwrap_or(0);
            let change = after_value - before_value;
            if change != 0 {
                self.delta.push((path, change));
            }
        }
        self.outcomes.push(outcome);
        self.changed.push(path);
        self.numeric.push(path);
        Ok(())
    }

    /// Commits one text route; text routes carry no numeric delta (plan §4).
    fn text(&mut self, path: &'static str, value: &str) -> Result<(), UiCompatError> {
        if value.is_empty() {
            return Ok(());
        }
        if self
            .state
            .value(path)
            .map(|current| current == value)
            .unwrap_or(false)
        {
            return Ok(());
        }
        let outcome = self.state.commit_text(path, value)?;
        self.outcomes.push(outcome);
        self.changed.push(path);
        Ok(())
    }

    /// Commits one boolean route through the read-model setter (the frozen
    /// `commit_i64` rejects `bool`, `M39Value::from_i64`).
    fn flag(&mut self, path: &'static str, value: bool) -> Result<(), UiCompatError> {
        let current = self
            .state
            .value(path)
            .map(|current| current == "1" || current == "true")
            .unwrap_or(false);
        if current == value && self.state.value(path).is_ok() {
            return Ok(());
        }
        if !self.state.models.set_bool(path, value) {
            return Err(UiCompatError::Input(RULE_TYPE));
        }
        self.outcomes.push(ModalOutcome::Committed {
            read_model: path,
            before: u8::from(current).to_string(),
            after: u8::from(value).to_string(),
        });
        self.changed.push(path);
        Ok(())
    }

    fn reject(&mut self, reason: &'static str) {
        self.outcomes.push(ModalOutcome::Rejected { reason });
    }

    fn text_outcome(&mut self, value: String) {
        self.outcomes.push(ModalOutcome::Text { value });
    }

    fn finish(mut self) -> Result<Vec<Interaction>, UiCompatError> {
        // One interaction is one intent: it counts as delivered when it commits
        // at least one outcome (`Interaction::delivered`), never once per
        // committed route — `intents` stays 1 and `delivered <= intents` holds.
        let delivered = usize::from(!self.outcomes.is_empty());
        for outcome in &self.outcomes {
            M39State::validate(outcome)?;
        }
        self.delta.sort_by(|left, right| left.0.cmp(right.0));
        let interaction = Interaction {
            id: self.id,
            name: self.name,
            block: self.block,
            intents: 1,
            delivered,
            outcomes: self.outcomes,
            delta: self.delta,
            read_only: None,
            digest: String::new(),
        }
        .sealed();
        self.state.ef.windows.insert(self.id);
        self.state.ef.changed.extend(self.changed.iter().copied());
        self.state.ef.numeric.extend(self.numeric.iter().copied());
        if self.block == 'E' {
            self.state.ef.intents_e += 1;
            self.state.ef.delivered_e += delivered;
        } else {
            self.state.ef.intents_f += 1;
            self.state.ef.delivered_f += delivered;
        }
        Ok(vec![interaction])
    }
}

// ── retail readers (every one is optional except the career) ────────────────

fn read_bytes(arena2: &Path, name: &str) -> Option<Vec<u8>> {
    std::fs::read(arena2.join(name)).ok()
}

fn read_career(arena2: &Path) -> Result<Career, UiCompatError> {
    let bytes = read_bytes(arena2, M20_CLASS_FILE)
        .ok_or(UiCompatError::Text(RULE_CAREER_READ))?;
    Career::parse(&bytes).map_err(|_| UiCompatError::Text(RULE_CAREER_PARSE))
}

fn live_magic_skills(career: &Career) -> Vec<(i32, i16)> {
    let character = Character::starting(career.clone(), &mut Rng64::new(M19_SEED), 1);
    let mut indices: Vec<i32> = magic::templates()
        .filter_map(|props| props.magic_skill.career_skill_index())
        .collect();
    indices.sort_unstable();
    indices.dedup();
    indices
        .into_iter()
        .filter_map(|index| {
            Skill::from_index(index).map(|skill| (index, character.live_skill(skill)))
        })
        .collect()
}

fn read_magic_items(arena2: &Path) -> Vec<ItemRow> {
    let Some(bytes) = read_bytes(arena2, MAGIC_DEF_FILE) else {
        return Vec::new();
    };
    let Ok(templates) = parse_magic_def(&bytes) else {
        return Vec::new();
    };
    templates
        .into_iter()
        .filter(|template| template.kind == 0)
        .map(|template| ItemRow {
            name: template.name,
            value: template.value,
            uses: template.uses,
        })
        .collect()
}

fn read_standard_spells(arena2: &Path) -> Vec<SpellRow> {
    let Some(bytes) = read_bytes(arena2, SPELLS_STD_FILE) else {
        return Vec::new();
    };
    parse_spells_std(&bytes)
        .into_iter()
        .filter_map(|record| {
            let name = record.spell_name.trim().to_string();
            if name.is_empty() {
                return None;
            }
            let cost = i64::from(record.cost);
            let icon = u16::from(record.icon);
            Some(SpellRow { name, cost, icon })
        })
        .collect()
}

fn read_book(arena2: &Path) -> Option<Book> {
    let dir = arena2.join(BOOKS_DIR);
    let dir = if dir.is_dir() { dir } else { arena2.to_path_buf() };
    sorted_files(&dir, "txt")
        .into_iter()
        .find_map(|path| std::fs::read(path).ok().and_then(|bytes| Book::parse(&bytes).ok()))
}

fn read_biography(arena2: &Path) -> Vec<String> {
    let Some(bytes) = read_bytes(arena2, BIO_FILE) else {
        return Vec::new();
    };
    let biography = Biography::parse(&bytes);
    biography.lines().to_vec()
}

fn read_journal(arena2: &Path) -> Vec<JournalRow> {
    let mut rows = Vec::new();
    for path in sorted_files(arena2, QRC_EXTENSION) {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Ok(qrc) = Qrc::parse(&bytes) else {
            continue;
        };
        // `.QRC` ids are 1000-based and 17 retail files repeat/disorder them, so
        // the table index — never the id — is the iteration cursor (a `0..count`
        // loop passed as an id misses every record: `index_of` says
        // `UnknownRecord`).
        for index in 0..qrc.text.records.len() {
            let Ok(text) = qrc.text.record_text_by_index(&bytes, index) else {
                continue;
            };
            let text = text.trim().to_string();
            if text.is_empty() {
                continue;
            }
            let prefixed = text
                .lines()
                .next()
                .map(|line| line.starts_with('%') && line.contains(':'))
                .unwrap_or(false);
            rows.push(JournalRow { text, prefixed });
        }
    }
    rows
}

/// Files of `dir` whose extension matches `extension`, in file-name order.
fn sorted_files(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .and_then(|value| value.to_str())
                .map(|value| value.eq_ignore_ascii_case(extension))
                .unwrap_or(false)
        })
        .collect();
    paths.sort();
    paths
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    use daggerfall_inventory::items::{ITEM_RECORD_SIZE, RUBY_SENTINEL, TOTAL_ITEMS};
    use daggerfall_parser_rs::texture::Palette;
    use daggerfall_skills::career::CFG_BYTES;

    use super::*;

    /// Scratch `ARENA2` with the mandatory career plus a synthetic
    /// `MAGIC.DEF`, `BIO.DAT` and `FALL.EXE` item table; removed on drop.
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

    /// One 62-byte `MAGIC.DEF` record (`magic_def` layout: name 0..32,
    /// `kind` 32, `uses` 55..57, `value` 57..61).
    fn magic_record(name: &str, kind: u8, uses: i16, value: i32) -> [u8; 62] {
        let mut record = [0u8; 62];
        let raw = name.as_bytes();
        let length = raw.len().min(32);
        record[..length].copy_from_slice(&raw[..length]);
        record[32] = kind;
        record[55..57].copy_from_slice(&uses.to_le_bytes());
        record[57..61].copy_from_slice(&value.to_le_bytes());
        record
    }

    /// Three regular templates plus one artifact-class record.
    fn synthetic_magic_def() -> Vec<u8> {
        let records = [
            magic_record("Ebony Dagger", 0, 10, 500),
            magic_record("Mithril Blade", 0, 0, 1_500),
            magic_record("Artifact Relic", 1, 3, 9_999),
            magic_record("Dragon Cuirass", 0, 4, 2_500),
        ];
        let mut blob = (records.len() as u32).to_le_bytes().to_vec();
        for record in records {
            blob.extend_from_slice(&record);
        }
        blob
    }

    /// Writes one 48-byte item record (`FALL.EXE` item table).
    fn write_item(blob: &mut [u8], index: usize, name: &str, weight_units: i32, price: i32) {
        let start = index * ITEM_RECORD_SIZE;
        let record = &mut blob[start..start + ITEM_RECORD_SIZE];
        let bytes = name.as_bytes();
        record[..bytes.len()].copy_from_slice(bytes);
        record[24..28].copy_from_slice(&weight_units.to_le_bytes());
        record[28..30].copy_from_slice(&200u16.to_le_bytes());
        record[30..34].copy_from_slice(&0i32.to_le_bytes());
        record[34..38].copy_from_slice(&price.to_le_bytes());
    }

    /// `FALL.EXE` shaped blob whose item table starts at the `Ruby` sentinel.
    fn synthetic_fall_exe() -> Vec<u8> {
        let mut blob = vec![0u8; TOTAL_ITEMS * ITEM_RECORD_SIZE];
        blob[..RUBY_SENTINEL.len()].copy_from_slice(RUBY_SENTINEL);
        write_item(&mut blob, 102, "Cuirass", 80, 1_000);
        write_item(&mut blob, 110, "Buckler", 40, 400);
        write_item(&mut blob, 113, "Longsword", 60, 2_000);
        blob
    }

    fn arena2() -> Scratch {
        let serial = NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "daggerfall-m39-ef-{}-{serial}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("scratch arena2");
        std::fs::write(dir.join(M20_CLASS_FILE), synthetic_career_bytes()).expect("career write");
        std::fs::write(dir.join("FALL.EXE"), synthetic_fall_exe()).expect("item table write");
        std::fs::write(dir.join(MAGIC_DEF_FILE), synthetic_magic_def()).expect("MAGIC.DEF write");
        std::fs::write(
            dir.join(BIO_FILE),
            b"Born in Sentinel.\0Served the Blades.\0Retired to Daggerfall.\0",
        )
        .expect("BIO.DAT write");
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

    fn name_of(id: u16) -> &'static str {
        surfaces()
            .iter()
            .find(|surface| surface.id == id)
            .map(|surface| surface.pilot.key)
            .expect("census ordinal")
    }

    fn names_of(block: char) -> Vec<&'static str> {
        surfaces()
            .iter()
            .filter(|surface| surface.block == block)
            .map(|surface| surface.pilot.key)
            .collect()
    }

    fn delivered(dir: &Scratch) -> (M39State, Vec<Interaction>) {
        let mut state = state(dir);
        let mut interactions = Vec::new();
        for name in names_of('E').into_iter().chain(names_of('F')) {
            let mut rows = interact(&mut state, name).expect("delivery");
            assert_eq!(rows.len(), 1, "{name}");
            interactions.push(rows.remove(0));
        }
        (state, interactions)
    }

    #[test]
    fn every_window_answers_with_a_validated_outcome() {
        let dir = arena2();
        let (state, interactions) = delivered(&dir);
        assert_eq!(interactions.len(), BLOCK_E_WINDOWS + BLOCK_F_WINDOWS);

        for interaction in &interactions {
            assert!(interaction.intents > 0, "{}", interaction.name);
            assert!(interaction.delivered > 0, "{}", interaction.name);
            assert_eq!(
                interaction.delivered,
                usize::from(!interaction.outcomes.is_empty()),
                "one intent counts once, whatever the outcomes it commits ({})",
                interaction.name
            );
            assert!(interaction.delivered <= interaction.intents);
            assert_eq!(interaction.id, {
                surfaces()
                    .iter()
                    .find(|surface| surface.pilot.key == interaction.name)
                    .map(|surface| surface.id)
                    .expect("census ordinal")
            });
            assert!(interaction.read_only.is_none(), "{}", interaction.name);
            assert_eq!(interaction.digest.len(), 16, "{}", interaction.name);
            let owned = match interaction.block {
                'E' => E_ROUTES,
                'F' => F_ROUTES,
                other => panic!("unexpected block {other}"),
            };
            assert_eq!(owned, if interaction.block == 'E' { 12 } else { 10 });
            for outcome in &interaction.outcomes {
                M39State::validate(outcome).expect("valid outcome");
                if let ModalOutcome::Committed { read_model, .. } = outcome {
                    assert_eq!(
                        route_block(read_model),
                        Some(interaction.block),
                        "{} wrote {read_model} outside its block",
                        interaction.name
                    );
                }
            }
            for (path, change) in &interaction.delta {
                assert_eq!(route_block(path), Some(interaction.block), "{}", path);
                assert_ne!(*change, 0, "{path} carries a numeric change");
            }
        }
        // The synthetic retail pair cannot serve SPELLS.STD, books or QRC:
        // those windows answer with a citable `Rejected` outcome instead.
        for id in [56u16, 82, 27, 52] {
            let name = name_of(id);
            let interaction = interactions
                .iter()
                .find(|interaction| interaction.name == name)
                .expect("window delivered");
            assert!(
                interaction
                    .outcomes
                    .iter()
                    .any(|outcome| matches!(outcome, ModalOutcome::Rejected { reason } if !reason.is_empty())),
                "{name} must reject when its retail file is absent"
            );
        }
        let evidence_e = evidence(&state);
        assert_eq!(evidence_e.block, 'E');
        assert_eq!(evidence_e.windows, BLOCK_E_WINDOWS);
        assert!(evidence_e.intents > 0);
        assert!(evidence_e.delivered > 0);
        assert!(evidence_e.flows > 0);
        assert_eq!(evidence_e.read_only, 0);
        let evidence_f = evidence_f(&state);
        assert_eq!(evidence_f.block, 'F');
        assert_eq!(evidence_f.windows, BLOCK_F_WINDOWS);
        assert!(evidence_f.intents > 0);
        assert!(evidence_f.delivered > 0);
        assert!(evidence_f.flows > 0);
        assert_eq!(evidence_f.read_only, 0);
    }

    #[test]
    fn numeric_windows_deliver_deltas() {
        let dir = arena2();
        let mut state = state(&dir);
        for (id, path) in [
            (32u16, PATH_ELEMENTS),
            (41, PATH_ITEM_COST),
            (51, PATH_POTION_COST),
            (70, PATH_CHARGES),
            (57, PATH_COMPOSED_COST),
        ] {
            let name = name_of(id);
            let mut rows = interact(&mut state, name).expect("delivery");
            let interaction = rows.remove(0);
            assert!(
                interaction.delta.iter().any(|(route, _)| *route == path),
                "{name} must change {path}"
            );
        }
        // `041` persists the template and `051` the potion as session items.
        for flag in [
            PATH_ITEM_PERSISTED,
            PATH_POTION_PERSISTED,
            PATH_SPELLMAKER_PERSISTED,
        ] {
            let current = state.value(flag).expect("bool route");
            assert!(current == "1" || current == "true", "{flag} = {current}");
        }

        let name = name_of(49);
        let mut rows = interact(&mut state, name).expect("delivery");
        let interaction = rows.remove(0);
        assert!(
            interaction.delta.iter().any(|(route, _)| *route == PATH_HISTORY),
            "history window must change {PATH_HISTORY}"
        );
        assert_eq!(state.value(PATH_HISTORY).expect("history"), "1");
    }

    #[test]
    fn effect_composition_stops_at_the_classic_limit() {
        let dir = arena2();
        let mut state = state(&dir);
        let name = name_of(32);
        let mut committed = 0;
        let mut rejected = 0;
        for _ in 0..(MAX_EFFECTS_PER_SPELL + 1) {
            let rows = interact(&mut state, name).expect("delivery");
            for outcome in &rows[0].outcomes {
                match outcome {
                    ModalOutcome::Committed { .. } => committed += 1,
                    ModalOutcome::Rejected { reason } => {
                        assert!(!reason.is_empty());
                        rejected += 1;
                    }
                    _ => {}
                }
            }
        }
        assert_eq!(committed, MAX_EFFECTS_PER_SPELL);
        assert_eq!(rejected, 1);
        assert_eq!(
            state.value(PATH_ELEMENTS).expect("elements"),
            MAX_EFFECTS_PER_SPELL.to_string()
        );
    }

    #[test]
    fn charges_are_consumed_until_the_classic_uses_run_out() {
        let dir = arena2();
        let mut state = state(&dir);
        let name = name_of(70);
        for expected in (0..10).rev() {
            let rows = interact(&mut state, name).expect("delivery");
            assert!(rows[0].outcomes.iter().any(|outcome| matches!(
                outcome,
                ModalOutcome::Committed { read_model, .. } if *read_model == PATH_CHARGES
            )));
            assert_eq!(
                state.value(PATH_CHARGES).expect("charges"),
                expected.to_string()
            );
        }
        let rows = interact(&mut state, name).expect("delivery");
        assert!(matches!(
            rows[0].outcomes.as_slice(),
            [ModalOutcome::Rejected { reason }] if *reason == RULE_CHARGES_SPENT
        ));
    }

    #[test]
    fn missing_retail_files_reject_with_a_citable_reason() {
        let dir = arena2();
        let mut state = state(&dir);
        for (id, reason) in [
            (56u16, RULE_SPELLS_STD_ABSENT),
            (82, RULE_ICONS_ABSENT),
            (27, RULE_BOOKS_ABSENT),
            (52, RULE_JOURNAL_ABSENT),
        ] {
            let name = name_of(id);
            let rows = interact(&mut state, name).expect("delivery");
            assert!(
                matches!(
                    rows[0].outcomes.as_slice(),
                    [ModalOutcome::Rejected { reason: got }] if *got == reason
                ),
                "{name} must reject with {reason}"
            );
        }
    }

    #[test]
    fn two_runs_yield_the_same_state_and_digests() {
        let dir = arena2();
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
    fn unknown_window_is_navigation_error() {
        let dir = arena2();
        let mut state = state(&dir);
        assert!(interact(&mut state, "DaggerfallNowhereWindow").is_err());
    }

    #[test]
    fn evidence_starts_pending_for_both_blocks() {
        let dir = arena2();
        let state = state(&dir);
        let evidence_e = evidence(&state);
        assert_eq!(evidence_e.block, 'E');
        assert_eq!(evidence_e.windows, BLOCK_E_WINDOWS);
        assert_eq!(evidence_e.intents, 0);
        assert_eq!(evidence_e.delivered, 0);
        assert_eq!(evidence_e.state_delta, 0);
        assert!(!evidence_e.ok);
        let evidence_f = evidence_f(&state);
        assert_eq!(evidence_f.block, 'F');
        assert_eq!(evidence_f.windows, BLOCK_F_WINDOWS);
        assert_eq!(evidence_f.intents, 0);
        assert!(!evidence_f.ok);
    }
}

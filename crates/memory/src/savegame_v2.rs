//! `savegame_v2` — M33 total persistence: the canonical `DFL-SESSION 2`
//! text document and the **twelve section codecs** of the v2 envelope.
//!
//! Spec: `docs/sintergic/game/programming/m33-total-persistence-savegame-spec-CR-5243af63.md`
//! (§3 grammar and section split, §4 golden sample S0, §5 v1→v2 migration,
//! §6 probe contract). The envelope itself (`DFSV`) lives in
//! [`crate::save_v2`]; this module owns the **payload** of the twelve
//! canonical sections and nothing else: it persists state, it never
//! interprets it (§7.1: HP > maxHP is written as given).
//!
//! ## Wire format (§3.1, line by line, ASCII/UTF-8, `\n`)
//!
//! ```text
//! DFL-SESSION 2                                 ← magic + version
//! <key>=<value>                                 ← 48 fixed keys, always present, §3.3-§3.10 order
//! <family>_count=<n>                            ← opens a list; exactly n entries follow
//! <family>=<fields> | <family>.<idx>=<fields>   ← entries (idx 0..n-1)
//! checksum=<16 lowercase hex>                   ← LAST line, FNV-1a 64 of every byte before it
//! ```
//!
//! Binding rules: decimal integers only (0 float, NFR NUMERIC), fixed-arity
//! CSV (`attributes` 8, `skills` 35, `equipped` 27, `weather` 6), text fields
//! escaped like v1 (`\`→`\\`, `|`→`\p`, `\n`→`\n`, `\r`→`\r`), closed keys
//! (unknown, duplicated or missing ⇒ [`SaveError::Corrupt`]), negative or
//! short counts ⇒ `Corrupt`, and a fixed section/entry order so that the same
//! state always yields the same bytes.
//!
//! ## Sections and owners (§3.3)
//!
//! | key | owner | source of the state |
//! |:---|:---|:---|
//! | `world` | M19/M20/M18 | position (mm), orientation (mrad), region/location/environment/map |
//! | `clock` | ❤️ heart | the six `GameClock` fields |
//! | `character` | M20 (+ heart vitals) | level, vitals, attributes, 35 skills |
//! | `inventory` | M21/M30 | gold, goods, item instances, 27 equipment slots, containers |
//! | `effects` | M23/M24/M32 | active effects, diseases, vampirism, lycanthropy |
//! | `guilds` | M27 | guild membership (ascending by faction) |
//! | `crime` | M28 | crime flag, legal reputation (ascending by region), prison |
//! | `quests` | M29 | quest log (acceptance order) and quest state (ascending by key) |
//! | `economy` | M30 | bank (ascending by region), loans, house/ship property |
//! | `calendar` | M31/M18 | absolute game minutes, 6 weather values, `is_day` |
//! | `map` | M18/M34 | loaded chunks (ascending) and per-dungeon automap |
//! | `rng` | all | persisted RNG states (ascending by name) |
//!
//! Each section is a [`SaveSection`]: it writes its own lines and parses them
//! back (order-strict, closed keys), which is exactly what a
//! [`crate::save_v2::SectionCodec`] needs. `memory` composes the twelve into
//! [`SaveGameV2`] and the envelope, and never looks inside a section.
//!
//! ## Migration (§5)
//!
//! [`migrate_text`] accepts `DFL-SESSION 1` (through the v1 reader) and
//! `DFL-SESSION 2` (re-parse + re-emit, so it is idempotent). New keys take
//! the declared defaults (`0`, `-1` for equipment slots, empty lists) and the
//! empty name stays empty: a partial migration is declared, never invented.

use std::fmt;
use std::fs;
use std::path::Path;

use crate::save_v2::{
    RegistryError, SaveError as EnvelopeError, SaveStateV2, SectionCodec, SectionError, SectionId,
    SectionRegistry,
};
use crate::session_store::{escape, fnv1a64, unescape, SaveError, SessionSnapshot};

/// Magic of the text document (same family as v1, new version).
pub const MAGIC: &str = "DFL-SESSION";
/// Version this build writes and accepts.
pub const VERSION: i64 = 2;

/// Fixed keys of §3.3-§3.10 (GV-10).
pub const FIXED_KEYS: usize = 48;
/// `*_count` lines that open a list family (GV-10).
pub const COUNT_KEYS: usize = 14;
/// Fixed lines of the document: 48 keys + 14 counts (`GV-10`).
pub const FIXED_LINES: usize = FIXED_KEYS + COUNT_KEYS;
/// Equipment slots of the classic character record (§2.2, `equippedCount=27`).
pub const EQUIPPED_SLOTS: usize = 27;
/// Skills persisted by M20.
pub const SKILL_COUNT: usize = 35;
/// Attributes persisted by M20.
pub const ATTRIBUTE_COUNT: usize = 8;
/// Climate weather values of M31.
pub const WEATHER_COUNT: usize = 6;
/// Accumulated damage stats tracked by M32 (`HEA…LUC`).
pub const DISEASE_ACCUMULATORS: usize = 11;
/// CSV fields of `item.<i>` (§4).
pub const ITEM_FIELDS: usize = 14;

// ═══════════════════════════════════════════════════════════════════════════
// Line cursor: order-strict, closed-key parsing of one document or section
// ═══════════════════════════════════════════════════════════════════════════

/// Sequential reader over `key=value` lines.
///
/// Parsing never searches: every section consumes its lines in the exact
/// order it writes them, so a duplicated key, a missing key or an unknown key
/// is a [`SaveError::Corrupt`] instead of a silently accepted document.
///
/// `pub` because every `SaveSection::parse` signature takes `&mut Cursor` and
/// that trait is public: a private parameter type in a public interface is
/// `private_interfaces` (13 clippy errors with `-D warnings`, measured
/// 2026-10-24 in the M18 close gate). Only the read side is exposed; the
/// fields stay private.
pub struct Cursor<'a> {
    lines: Vec<(&'a str, &'a str)>,
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(body: &'a str) -> Result<Self, SaveError> {
        let mut lines = Vec::new();
        for line in body.lines() {
            let (key, value) = line.split_once('=').ok_or_else(|| {
                SaveError::Corrupt(format!("line without '=': {line:?}"))
            })?;
            lines.push((key, value));
        }
        Ok(Self { lines, pos: 0 })
    }

    fn next(&mut self) -> Result<(&'a str, &'a str), SaveError> {
        if self.pos >= self.lines.len() {
            return Err(SaveError::Corrupt("unexpected end of the save".into()));
        }
        let pair = self.lines[self.pos];
        self.pos += 1;
        Ok(pair)
    }

    fn expect(&mut self, key: &str) -> Result<&'a str, SaveError> {
        let (found, value) = self.next()?;
        if found != key {
            return Err(SaveError::Corrupt(format!(
                "expected key {key:?}, found {found:?}"
            )));
        }
        Ok(value)
    }

    fn expect_i64(&mut self, key: &str) -> Result<i64, SaveError> {
        parse_i64(self.expect(key)?)
    }

    fn expect_u64(&mut self, key: &str) -> Result<u64, SaveError> {
        parse_u64(self.expect(key)?)
    }

    fn expect_count(&mut self, key: &str) -> Result<usize, SaveError> {
        let count = self.expect_i64(key)?;
        usize::try_from(count)
            .map_err(|_| SaveError::Corrupt(format!("{key}: negative count {count}")))
    }

    fn expect_text(&mut self, key: &str) -> Result<String, SaveError> {
        unescape(self.expect(key)?)
    }

    fn remaining(&self) -> usize {
        self.lines.len() - self.pos
    }
}

fn parse_i64(value: &str) -> Result<i64, SaveError> {
    value
        .parse()
        .map_err(|_| SaveError::Corrupt(format!("invalid integer: {value:?}")))
}

fn parse_u64(value: &str) -> Result<u64, SaveError> {
    value
        .parse()
        .map_err(|_| SaveError::Corrupt(format!("invalid u64: {value:?}")))
}

fn parse_csv_fixed<const N: usize>(value: &str, what: &str) -> Result<[i64; N], SaveError> {
    let mut out = [0i64; N];
    let mut count = 0usize;
    for part in value.split(',') {
        if count == N {
            return Err(SaveError::Corrupt(format!(
                "{what}: more than {N} values"
            )));
        }
        out[count] = parse_i64(part)?;
        count += 1;
    }
    if count != N {
        return Err(SaveError::Corrupt(format!(
            "{what}: {count} values, want {N}"
        )));
    }
    Ok(out)
}

fn parse_csv_vec(value: &str) -> Result<Vec<i64>, SaveError> {
    if value.is_empty() {
        return Ok(Vec::new());
    }
    value.split(',').map(parse_i64).collect()
}

fn push_pair(out: &mut String, key: &str, value: impl fmt::Display) {
    out.push_str(key);
    out.push('=');
    out.push_str(&value.to_string());
    out.push('\n');
}

fn push_text(out: &mut String, key: &str, value: &str) {
    out.push_str(key);
    out.push('=');
    out.push_str(&escape(value));
    out.push('\n');
}

fn push_csv(out: &mut String, key: &str, values: &[i64]) {
    out.push_str(key);
    out.push('=');
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&value.to_string());
    }
    out.push('\n');
}

// ═══════════════════════════════════════════════════════════════════════════
// Section contract
// ═══════════════════════════════════════════════════════════════════════════

/// One of the twelve canonical sections of §3.3.
///
/// Implementors write **only** their own lines and parse them back in the
/// same order. The default methods give the bytes the envelope carries
/// (`encode/decode` of a [`SectionCodec`]) without any module knowing another
/// module's types.
pub trait SaveSection: Default + Clone + PartialEq + Eq + Sized {
    /// Section key of the envelope ([`crate::save_v2::M33_SECTIONS`]).
    const KEY: &'static str;
    /// Owner string declared in the same table.
    const OWNER: &'static str;

    /// Appends this section's canonical lines to `out`.
    fn write_lines(&self, out: &mut String);

    /// Reads this section's lines, in canonical order, from `cur`.
    fn parse(cur: &mut Cursor<'_>) -> Result<Self, SaveError>;

    /// Envelope id of the section.
    fn id() -> SectionId {
        SectionId::new(Self::KEY)
    }

    /// Canonical bytes of the section (its own lines, UTF-8).
    fn encode_bytes(&self) -> Vec<u8> {
        let mut out = String::new();
        self.write_lines(&mut out);
        out.into_bytes()
    }

    /// Parses canonical section bytes. Trailing lines are refused: a section
    /// carries exactly the lines of its own contract.
    fn decode_bytes(bytes: &[u8]) -> Result<Self, SaveError> {
        let text = std::str::from_utf8(bytes).map_err(|_| {
            SaveError::Corrupt(format!("section {} is not UTF-8", Self::KEY))
        })?;
        let mut cur = Cursor::new(text)?;
        let section = Self::parse(&mut cur)?;
        if cur.remaining() != 0 {
            return Err(SaveError::Corrupt(format!(
                "section {}: {} trailing lines",
                Self::KEY,
                cur.remaining()
            )));
        }
        Ok(section)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 1 · world (M19/M20/M18)
// ═══════════════════════════════════════════════════════════════════════════

/// Position, orientation and place (§3.3 row 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WorldSection {
    /// X position in integer millimeters.
    pub pos_x_mm: i64,
    /// Y (vertical) position in integer millimeters.
    pub pos_y_mm: i64,
    /// Z position in integer millimeters.
    pub pos_z_mm: i64,
    /// Yaw in integer milliradians.
    pub yaw_mrad: i64,
    /// Pitch in integer milliradians.
    pub pitch_mrad: i64,
    /// Region index (M18 `DFRegion`).
    pub region_index: i64,
    /// Location index inside the region.
    pub location_index: i64,
    /// 1 exterior, 2 building, 3 dungeon (source `SaveTree.Environments`).
    pub environment: i64,
    /// Map id (`65535` = the open world).
    pub map_id: i64,
}

impl SaveSection for WorldSection {
    const KEY: &'static str = "world";
    const OWNER: &'static str = "M19/M20/M18";

    fn write_lines(&self, out: &mut String) {
        push_pair(out, "pos_x_mm", self.pos_x_mm);
        push_pair(out, "pos_y_mm", self.pos_y_mm);
        push_pair(out, "pos_z_mm", self.pos_z_mm);
        push_pair(out, "yaw_mrad", self.yaw_mrad);
        push_pair(out, "pitch_mrad", self.pitch_mrad);
        push_pair(out, "region_index", self.region_index);
        push_pair(out, "location_index", self.location_index);
        push_pair(out, "environment", self.environment);
        push_pair(out, "map_id", self.map_id);
    }

    fn parse(cur: &mut Cursor<'_>) -> Result<Self, SaveError> {
        Ok(Self {
            pos_x_mm: cur.expect_i64("pos_x_mm")?,
            pos_y_mm: cur.expect_i64("pos_y_mm")?,
            pos_z_mm: cur.expect_i64("pos_z_mm")?,
            yaw_mrad: cur.expect_i64("yaw_mrad")?,
            pitch_mrad: cur.expect_i64("pitch_mrad")?,
            region_index: cur.expect_i64("region_index")?,
            location_index: cur.expect_i64("location_index")?,
            environment: cur.expect_i64("environment")?,
            map_id: cur.expect_i64("map_id")?,
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 2 · clock (heart)
// ═══════════════════════════════════════════════════════════════════════════

/// The six `GameClock` fields (❤️ heart owns the time loop).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClockSection {
    /// Year (classic calendar 3E).
    pub year: i64,
    /// Month `0..11`.
    pub month: i64,
    /// Day of month.
    pub day: i64,
    /// Hour `0..23`.
    pub hour: i64,
    /// Minute `0..59`.
    pub minute: i64,
    /// Second `0..59`.
    pub second: i64,
}

impl SaveSection for ClockSection {
    const KEY: &'static str = "clock";
    const OWNER: &'static str = "heart";

    fn write_lines(&self, out: &mut String) {
        push_pair(out, "clock_year", self.year);
        push_pair(out, "clock_month", self.month);
        push_pair(out, "clock_day", self.day);
        push_pair(out, "clock_hour", self.hour);
        push_pair(out, "clock_minute", self.minute);
        push_pair(out, "clock_second", self.second);
    }

    fn parse(cur: &mut Cursor<'_>) -> Result<Self, SaveError> {
        Ok(Self {
            year: cur.expect_i64("clock_year")?,
            month: cur.expect_i64("clock_month")?,
            day: cur.expect_i64("clock_day")?,
            hour: cur.expect_i64("clock_hour")?,
            minute: cur.expect_i64("clock_minute")?,
            second: cur.expect_i64("clock_second")?,
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 3 · character (M20 + heart vitals)
// ═══════════════════════════════════════════════════════════════════════════

/// Level, vitals, attributes and the 35 skills (§3.3 row 3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterSection {
    /// Character level.
    pub level: i64,
    /// Experience within the level.
    pub experience: i64,
    /// Current health.
    pub health: i64,
    /// Maximum health.
    pub max_health: i64,
    /// Current stamina.
    pub stamina: i64,
    /// Maximum stamina.
    pub max_stamina: i64,
    /// Current spell points.
    pub spell_points: i64,
    /// Maximum spell points.
    pub max_spell_points: i64,
    /// Fatigue (M32 tracks it).
    pub fatigue: i64,
    /// Character name (free text, escaped; empty = unnamed).
    pub name: String,
    /// Gender index.
    pub gender: i64,
    /// Race index.
    pub race: i64,
    /// Career id (class).
    pub career_id: i64,
    /// Face index.
    pub face_index: i64,
    /// Reflexes.
    pub reflexes: i64,
    /// Current attributes (8, classic order).
    pub attributes: [i64; ATTRIBUTE_COUNT],
    /// Base attributes (8).
    pub attributes_base: [i64; ATTRIBUTE_COUNT],
    /// Skill values (35).
    pub skills: [i64; SKILL_COUNT],
    /// Skill uses (35, drives the level-up rolls).
    pub skill_uses: [i64; SKILL_COUNT],
    /// Primary skills raised this level.
    pub skills_raised_1: i64,
    /// Secondary skills raised this level.
    pub skills_raised_2: i64,
    /// Accumulated skill sum that triggers a level-up.
    pub levelup_skill_sum: i64,
}

impl Default for CharacterSection {
    fn default() -> Self {
        Self {
            level: 0,
            experience: 0,
            health: 0,
            max_health: 0,
            stamina: 0,
            max_stamina: 0,
            spell_points: 0,
            max_spell_points: 0,
            fatigue: 0,
            name: String::new(),
            gender: 0,
            race: 0,
            career_id: 0,
            face_index: 0,
            reflexes: 0,
            attributes: [0; ATTRIBUTE_COUNT],
            attributes_base: [0; ATTRIBUTE_COUNT],
            skills: [0; SKILL_COUNT],
            skill_uses: [0; SKILL_COUNT],
            skills_raised_1: 0,
            skills_raised_2: 0,
            levelup_skill_sum: 0,
        }
    }
}

impl SaveSection for CharacterSection {
    const KEY: &'static str = "character";
    const OWNER: &'static str = "M20";

    fn write_lines(&self, out: &mut String) {
        push_pair(out, "level", self.level);
        push_pair(out, "experience", self.experience);
        push_pair(out, "health", self.health);
        push_pair(out, "max_health", self.max_health);
        push_pair(out, "stamina", self.stamina);
        push_pair(out, "max_stamina", self.max_stamina);
        push_pair(out, "spell_points", self.spell_points);
        push_pair(out, "max_spell_points", self.max_spell_points);
        push_pair(out, "fatigue", self.fatigue);
        push_text(out, "name", &self.name);
        push_pair(out, "gender", self.gender);
        push_pair(out, "race", self.race);
        push_pair(out, "career_id", self.career_id);
        push_pair(out, "face_index", self.face_index);
        push_pair(out, "reflexes", self.reflexes);
        push_csv(out, "attributes", &self.attributes);
        push_csv(out, "attributes_base", &self.attributes_base);
        push_csv(out, "skills", &self.skills);
        push_csv(out, "skill_uses", &self.skill_uses);
        push_pair(out, "skills_raised_1", self.skills_raised_1);
        push_pair(out, "skills_raised_2", self.skills_raised_2);
        push_pair(out, "levelup_skill_sum", self.levelup_skill_sum);
    }

    fn parse(cur: &mut Cursor<'_>) -> Result<Self, SaveError> {
        Ok(Self {
            level: cur.expect_i64("level")?,
            experience: cur.expect_i64("experience")?,
            health: cur.expect_i64("health")?,
            max_health: cur.expect_i64("max_health")?,
            stamina: cur.expect_i64("stamina")?,
            max_stamina: cur.expect_i64("max_stamina")?,
            spell_points: cur.expect_i64("spell_points")?,
            max_spell_points: cur.expect_i64("max_spell_points")?,
            fatigue: cur.expect_i64("fatigue")?,
            name: cur.expect_text("name")?,
            gender: cur.expect_i64("gender")?,
            race: cur.expect_i64("race")?,
            career_id: cur.expect_i64("career_id")?,
            face_index: cur.expect_i64("face_index")?,
            reflexes: cur.expect_i64("reflexes")?,
            attributes: parse_csv_fixed(cur.expect("attributes")?, "attributes")?,
            attributes_base: parse_csv_fixed(cur.expect("attributes_base")?, "attributes_base")?,
            skills: parse_csv_fixed(cur.expect("skills")?, "skills")?,
            skill_uses: parse_csv_fixed(cur.expect("skill_uses")?, "skill_uses")?,
            skills_raised_1: cur.expect_i64("skills_raised_1")?,
            skills_raised_2: cur.expect_i64("skills_raised_2")?,
            levelup_skill_sum: cur.expect_i64("levelup_skill_sum")?,
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 4 · inventory (M21/M30)
// ═══════════════════════════════════════════════════════════════════════════

/// Named good with a quantity (`goods=` entry).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoodV2 {
    /// Good id (escaped text).
    pub id: String,
    /// Quantity in whole units.
    pub quantity: i64,
}

/// Item instance (`item.<i>`, 14 CSV fields, §4).
///
/// Mapped from the classic `ItemRecord` oracle (source `ItemRecord.cs:133-151`);
/// the 32-byte name of the original is **not** persisted: M21 derives it from
/// `group/index/material`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ItemV2 {
    /// Item group (weapons, armor, …).
    pub group: i64,
    /// Index inside the group.
    pub index: i64,
    /// Value in gold.
    pub value: i64,
    /// Item flags bitfield.
    pub flags: i64,
    /// Current condition.
    pub current_condition: i64,
    /// Maximum condition.
    pub max_condition: i64,
    /// Type-dependent value (armor rating, damage, …).
    pub type_dependent: i64,
    /// Material index.
    pub material: i64,
    /// Dye color index.
    pub color: i64,
    /// Weight in classic weight units (`1 kg = 4 cu`, M19 §5.1).
    pub weight_cu: i64,
    /// Enchantment points.
    pub enchantment_points: i64,
    /// Variants bitfield.
    pub variants: i64,
    /// Draw order (world sprite).
    pub draw_order: i64,
    /// Number of enchantment slots (`item.<i>.ench` carries exactly this many pairs).
    pub enchantment_slots: i64,
    /// `(type, param)` enchantment pairs.
    pub enchants: Vec<(i64, i64)>,
}

/// Container (`container.<i>=<type>,<item indices>`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContainerV2 {
    /// Container type (0 weapons/armor … 4 cart, 5 house, 6 ship, 7 inn room, 8 repairer).
    pub kind: i64,
    /// Indices of the items it holds.
    pub items: Vec<i64>,
}

/// Gold, goods, item instances, equipment and containers (§3.3 row 4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventorySection {
    /// Gold in whole units.
    pub gold: i64,
    /// Named goods in insertion order.
    pub goods: Vec<GoodV2>,
    /// Item instances in insertion order.
    pub items: Vec<ItemV2>,
    /// The 27 equipment slots: item index or `-1` (empty).
    pub equipped: [i64; EQUIPPED_SLOTS],
    /// Containers in insertion order.
    pub containers: Vec<ContainerV2>,
}

impl Default for InventorySection {
    fn default() -> Self {
        Self {
            gold: 0,
            goods: Vec::new(),
            items: Vec::new(),
            equipped: [-1; EQUIPPED_SLOTS],
            containers: Vec::new(),
        }
    }
}

impl InventorySection {
    fn item_line(item: &ItemV2) -> String {
        let fields = [
            item.group,
            item.index,
            item.value,
            item.flags,
            item.current_condition,
            item.max_condition,
            item.type_dependent,
            item.material,
            item.color,
            item.weight_cu,
            item.enchantment_points,
            item.variants,
            item.draw_order,
            item.enchantment_slots,
        ];
        let mut line = String::new();
        for (index, value) in fields.iter().enumerate() {
            if index > 0 {
                line.push(',');
            }
            line.push_str(&value.to_string());
        }
        line
    }
}

impl SaveSection for InventorySection {
    const KEY: &'static str = "inventory";
    const OWNER: &'static str = "M21/M30";

    fn write_lines(&self, out: &mut String) {
        push_pair(out, "gold", self.gold);
        push_pair(out, "goods_count", self.goods.len());
        for good in &self.goods {
            out.push_str("goods=");
            out.push_str(&escape(&good.id));
            out.push('|');
            out.push_str(&good.quantity.to_string());
            out.push('\n');
        }
        push_pair(out, "items_count", self.items.len());
        for (index, item) in self.items.iter().enumerate() {
            push_pair(out, &format!("item.{index}"), Self::item_line(item));
            if !item.enchants.is_empty() {
                let key = format!("item.{index}.ench");
                let mut line = String::new();
                for (position, (kind, param)) in item.enchants.iter().enumerate() {
                    if position > 0 {
                        line.push(',');
                    }
                    line.push_str(&format!("{kind}:{param}"));
                }
                push_pair(out, &key, line);
            }
        }
        push_csv(out, "equipped", &self.equipped);
        push_pair(out, "containers_count", self.containers.len());
        for (index, container) in self.containers.iter().enumerate() {
            let key = format!("container.{index}");
            out.push_str(&key);
            out.push('=');
            out.push_str(&container.kind.to_string());
            for item in &container.items {
                out.push(',');
                out.push_str(&item.to_string());
            }
            out.push('\n');
        }
    }

    fn parse(cur: &mut Cursor<'_>) -> Result<Self, SaveError> {
        let gold = cur.expect_i64("gold")?;
        let goods_count = cur.expect_count("goods_count")?;
        let mut goods = Vec::with_capacity(goods_count);
        for _ in 0..goods_count {
            let value = cur.expect("goods")?;
            let (id, quantity) = value.split_once('|').ok_or_else(|| {
                SaveError::Corrupt(format!("goods without '|': {value:?}"))
            })?;
            goods.push(GoodV2 {
                id: unescape(id)?,
                quantity: parse_i64(quantity)?,
            });
        }

        let items_count = cur.expect_count("items_count")?;
        let mut items = Vec::with_capacity(items_count);
        for index in 0..items_count {
            let value = cur.expect(&format!("item.{index}"))?;
            let fields = parse_csv_fixed::<ITEM_FIELDS>(value, "item")?;
            let mut item = ItemV2 {
                group: fields[0],
                index: fields[1],
                value: fields[2],
                flags: fields[3],
                current_condition: fields[4],
                max_condition: fields[5],
                type_dependent: fields[6],
                material: fields[7],
                color: fields[8],
                weight_cu: fields[9],
                enchantment_points: fields[10],
                variants: fields[11],
                draw_order: fields[12],
                enchantment_slots: fields[13],
                enchants: Vec::new(),
            };
            if item.enchantment_slots > 0 {
                let key = format!("item.{index}.ench");
                let value = cur.expect(&key)?;
                let want = usize::try_from(item.enchantment_slots).map_err(|_| {
                    SaveError::Corrupt(format!("{key}: negative slots"))
                })?;
                let mut enchants = Vec::with_capacity(want);
                for pair in value.split(',') {
                    let (kind, param) = pair.split_once(':').ok_or_else(|| {
                        SaveError::Corrupt(format!("{key}: pair without ':': {pair:?}"))
                    })?;
                    enchants.push((parse_i64(kind)?, parse_i64(param)?));
                }
                if enchants.len() != want {
                    return Err(SaveError::Corrupt(format!(
                        "{key}: {} pairs, want {want}",
                        enchants.len()
                    )));
                }
                item.enchants = enchants;
            }
            items.push(item);
        }

        let equipped = parse_csv_fixed(cur.expect("equipped")?, "equipped")?;

        let containers_count = cur.expect_count("containers_count")?;
        let mut containers = Vec::with_capacity(containers_count);
        for index in 0..containers_count {
            let value = cur.expect(&format!("container.{index}"))?;
            let (kind, rest) = value.split_once(',').ok_or_else(|| {
                SaveError::Corrupt(format!("container {index}: no type separator"))
            })?;
            containers.push(ContainerV2 {
                kind: parse_i64(kind)?,
                items: parse_csv_vec(rest)?,
            });
        }

        Ok(Self {
            gold,
            goods,
            items,
            equipped,
            containers,
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 5 · effects (M23/M24/M32)
// ═══════════════════════════════════════════════════════════════════════════

/// One active effect (`effect.<i>=<key>,<school>,<variants>,<rounds>,<magnitude>`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ActiveEffectV2 {
    /// Effect key (escaped text; no comma, the line is CSV).
    pub key: String,
    /// Magic school index.
    pub school: i64,
    /// Effect variants bitfield.
    pub variants: i64,
    /// Remaining rounds.
    pub rounds: i64,
    /// Magnitude per round.
    pub magnitude: i64,
}

/// One disease/poison state (`disease.<i>`, 14 CSV fields, §2.2/M32).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DiseaseV2 {
    /// Disease id.
    pub id: i64,
    /// Days of symptoms left.
    pub days: i64,
    /// Incubation days before symptoms.
    pub incubation: i64,
    /// The 11 accumulated damage stats tracked by M32.
    pub accumulated: [i64; DISEASE_ACCUMULATORS],
}

/// Active effects, diseases and the vampirism/lycanthropy flags (row 5).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EffectsSection {
    /// Active effects in insertion order.
    pub effects: Vec<ActiveEffectV2>,
    /// Diseases in insertion order.
    pub diseases: Vec<DiseaseV2>,
    /// Vampire clan (`0` = not a vampire).
    pub vampire_clan: i64,
    /// Werewolf/wereboar flag (`0` = none).
    pub lycanthropy: i64,
}

impl SaveSection for EffectsSection {
    const KEY: &'static str = "effects";
    const OWNER: &'static str = "M23/M24/M32";

    fn write_lines(&self, out: &mut String) {
        push_pair(out, "effects_count", self.effects.len());
        for (index, effect) in self.effects.iter().enumerate() {
            let key = format!("effect.{index}");
            out.push_str(&key);
            out.push('=');
            out.push_str(&escape(&effect.key));
            out.push_str(&format!(
                ",{},{},{},{}\n",
                effect.school, effect.variants, effect.rounds, effect.magnitude
            ));
        }
        push_pair(out, "diseases_count", self.diseases.len());
        for (index, disease) in self.diseases.iter().enumerate() {
            let key = format!("disease.{index}");
            out.push_str(&key);
            out.push('=');
            out.push_str(&format!("{},{},{}", disease.id, disease.days, disease.incubation));
            for value in disease.accumulated {
                out.push(',');
                out.push_str(&value.to_string());
            }
            out.push('\n');
        }
        push_pair(out, "vampire_clan", self.vampire_clan);
        push_pair(out, "lycanthropy", self.lycanthropy);
    }

    fn parse(cur: &mut Cursor<'_>) -> Result<Self, SaveError> {
        let effects_count = cur.expect_count("effects_count")?;
        let mut effects = Vec::with_capacity(effects_count);
        for index in 0..effects_count {
            let value = cur.expect(&format!("effect.{index}"))?;
            // The key is free text; the four numeric fields are the tail.
            let mut tail = value.rsplitn(5, ',');
            let magnitude = parse_i64(tail.next().unwrap_or_default())?;
            let rounds = parse_i64(tail.next().unwrap_or_default())?;
            let variants = parse_i64(tail.next().unwrap_or_default())?;
            let school = parse_i64(tail.next().unwrap_or_default())?;
            let key = unescape(tail.next().ok_or_else(|| {
                SaveError::Corrupt(format!("effect {index}: missing key"))
            })?)?;
            effects.push(ActiveEffectV2 {
                key,
                school,
                variants,
                rounds,
                magnitude,
            });
        }

        let diseases_count = cur.expect_count("diseases_count")?;
        let mut diseases = Vec::with_capacity(diseases_count);
        for index in 0..diseases_count {
            let value = cur.expect(&format!("disease.{index}"))?;
            let fields = parse_csv_fixed::<{ 3 + DISEASE_ACCUMULATORS }>(value, "disease")?;
            let mut accumulated = [0i64; DISEASE_ACCUMULATORS];
            accumulated.copy_from_slice(&fields[3..]);
            diseases.push(DiseaseV2 {
                id: fields[0],
                days: fields[1],
                incubation: fields[2],
                accumulated,
            });
        }

        Ok(Self {
            effects,
            diseases,
            vampire_clan: cur.expect_i64("vampire_clan")?,
            lycanthropy: cur.expect_i64("lycanthropy")?,
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 6 · guilds (M27)
// ═══════════════════════════════════════════════════════════════════════════

/// One guild membership (`guild.<i>`, ascending by faction).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GuildMembershipV2 {
    /// Faction id of the guild.
    pub faction: i64,
    /// Rank inside the guild.
    pub rank: i64,
    /// Noted-by-guild flag.
    pub noted: i64,
    /// Minutes since the last rank change.
    pub min_last_change: i64,
    /// Blessing magnitude.
    pub blessing: i64,
}

/// Guild memberships in ascending faction order (row 6).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GuildsSection {
    /// Memberships, ascending by faction.
    pub guilds: Vec<GuildMembershipV2>,
}

impl SaveSection for GuildsSection {
    const KEY: &'static str = "guilds";
    const OWNER: &'static str = "M27";

    fn write_lines(&self, out: &mut String) {
        push_pair(out, "guilds_count", self.guilds.len());
        for (index, guild) in self.guilds.iter().enumerate() {
            let key = format!("guild.{index}");
            push_pair(
                out,
                &key,
                format!(
                    "{},{},{},{},{}",
                    guild.faction, guild.rank, guild.noted, guild.min_last_change, guild.blessing
                ),
            );
        }
    }

    fn parse(cur: &mut Cursor<'_>) -> Result<Self, SaveError> {
        let count = cur.expect_count("guilds_count")?;
        let mut guilds = Vec::with_capacity(count);
        for index in 0..count {
            let value = cur.expect(&format!("guild.{index}"))?;
            let fields = parse_csv_fixed::<5>(value, "guild")?;
            guilds.push(GuildMembershipV2 {
                faction: fields[0],
                rank: fields[1],
                noted: fields[2],
                min_last_change: fields[3],
                blessing: fields[4],
            });
        }
        Ok(Self { guilds })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 7 · crime (M28)
// ═══════════════════════════════════════════════════════════════════════════

/// Crime flag, per-region legal reputation (ascending) and prison days (row 7).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CrimeSection {
    /// Has the player committed a crime.
    pub crime_committed: i64,
    /// `(region, reputation)` ascending by region.
    pub legal_rep: Vec<(i64, i64)>,
    /// Days left in prison.
    pub prison_days: i64,
}

impl SaveSection for CrimeSection {
    const KEY: &'static str = "crime";
    const OWNER: &'static str = "M28";

    fn write_lines(&self, out: &mut String) {
        push_pair(out, "crime_committed", self.crime_committed);
        push_pair(out, "legal_rep_count", self.legal_rep.len());
        for (region, value) in &self.legal_rep {
            push_pair(out, &format!("legal_rep.{region}"), value);
        }
        push_pair(out, "prison_days", self.prison_days);
    }

    fn parse(cur: &mut Cursor<'_>) -> Result<Self, SaveError> {
        let crime_committed = cur.expect_i64("crime_committed")?;
        let count = cur.expect_count("legal_rep_count")?;
        let mut legal_rep = Vec::with_capacity(count);
        for _ in 0..count {
            let (key, value) = cur.next()?;
            let region = key.strip_prefix("legal_rep.").ok_or_else(|| {
                SaveError::Corrupt(format!("expected a legal_rep entry, found {key:?}"))
            })?;
            legal_rep.push((parse_i64(region)?, parse_i64(value)?));
        }
        Ok(Self {
            crime_committed,
            legal_rep,
            prison_days: cur.expect_i64("prison_days")?,
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 8 · quests (M29)
// ═══════════════════════════════════════════════════════════════════════════

/// Quest log and quest state (§3.3 row 8).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QuestsSection {
    /// `(id, title)` in acceptance order.
    pub quests: Vec<(String, String)>,
    /// `(key, value)` ascending by key.
    pub states: Vec<(String, String)>,
}

impl SaveSection for QuestsSection {
    const KEY: &'static str = "quests";
    const OWNER: &'static str = "M29";

    fn write_lines(&self, out: &mut String) {
        push_pair(out, "quests_count", self.quests.len());
        for (id, title) in &self.quests {
            out.push_str("quest=");
            out.push_str(&escape(id));
            out.push('|');
            out.push_str(&escape(title));
            out.push('\n');
        }
        push_pair(out, "quest_state_count", self.states.len());
        for (index, (key, value)) in self.states.iter().enumerate() {
            let line = format!("quest_state.{index}");
            out.push_str(&line);
            out.push('=');
            out.push_str(&escape(key));
            out.push('|');
            out.push_str(&escape(value));
            out.push('\n');
        }
    }

    fn parse(cur: &mut Cursor<'_>) -> Result<Self, SaveError> {
        let count = cur.expect_count("quests_count")?;
        let mut quests = Vec::with_capacity(count);
        for _ in 0..count {
            let value = cur.expect("quest")?;
            let (id, title) = value.split_once('|').ok_or_else(|| {
                SaveError::Corrupt(format!("quest without '|': {value:?}"))
            })?;
            quests.push((unescape(id)?, unescape(title)?));
        }
        let state_count = cur.expect_count("quest_state_count")?;
        let mut states = Vec::with_capacity(state_count);
        for index in 0..state_count {
            let value = cur.expect(&format!("quest_state.{index}"))?;
            let (key, value) = value.split_once('|').ok_or_else(|| {
                SaveError::Corrupt(format!("quest_state {index}: no '|' separator"))
            })?;
            states.push((unescape(key)?, unescape(value)?));
        }
        Ok(Self { quests, states })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 9 · economy (M30)
// ═══════════════════════════════════════════════════════════════════════════

/// One loan (`loan.<i>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LoanV2 {
    /// Region of the bank.
    pub region: i64,
    /// Principal owed.
    pub principal: i64,
    /// Due date in game minutes.
    pub due_min: i64,
    /// Unpaid flag.
    pub unpaid: i64,
}

/// Bank accounts, loans and property (row 9).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EconomySection {
    /// `(region, amount)` ascending by region.
    pub bank: Vec<(i64, i64)>,
    /// Loans in insertion order.
    pub loans: Vec<LoanV2>,
    /// House property id (`0` = none).
    pub property_house: i64,
    /// Ship property id (`0` = none).
    pub property_ship: i64,
}

impl SaveSection for EconomySection {
    const KEY: &'static str = "economy";
    const OWNER: &'static str = "M30";

    fn write_lines(&self, out: &mut String) {
        push_pair(out, "bank_count", self.bank.len());
        for (region, amount) in &self.bank {
            push_pair(out, &format!("bank.{region}"), amount);
        }
        push_pair(out, "loans_count", self.loans.len());
        for (index, loan) in self.loans.iter().enumerate() {
            let key = format!("loan.{index}");
            push_pair(
                out,
                &key,
                format!(
                    "{},{},{},{}",
                    loan.region, loan.principal, loan.due_min, loan.unpaid
                ),
            );
        }
        push_pair(out, "property_house", self.property_house);
        push_pair(out, "property_ship", self.property_ship);
    }

    fn parse(cur: &mut Cursor<'_>) -> Result<Self, SaveError> {
        let bank_count = cur.expect_count("bank_count")?;
        let mut bank = Vec::with_capacity(bank_count);
        for _ in 0..bank_count {
            let (key, value) = cur.next()?;
            let region = key
                .strip_prefix("bank.")
                .ok_or_else(|| SaveError::Corrupt(format!("expected a bank entry, found {key:?}")))?;
            bank.push((parse_i64(region)?, parse_i64(value)?));
        }
        let loans_count = cur.expect_count("loans_count")?;
        let mut loans = Vec::with_capacity(loans_count);
        for index in 0..loans_count {
            let value = cur.expect(&format!("loan.{index}"))?;
            let fields = parse_csv_fixed::<4>(value, "loan")?;
            loans.push(LoanV2 {
                region: fields[0],
                principal: fields[1],
                due_min: fields[2],
                unpaid: fields[3],
            });
        }
        Ok(Self {
            bank,
            loans,
            property_house: cur.expect_i64("property_house")?,
            property_ship: cur.expect_i64("property_ship")?,
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 10 · calendar (M31/M18)
// ═══════════════════════════════════════════════════════════════════════════

/// Absolute game minutes, weather and the day/night flag (row 10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalendarSection {
    /// Minutes since the start of the game (absolute, M31).
    pub game_minutes: u64,
    /// The six climate weather values.
    pub weather: [i64; WEATHER_COUNT],
    /// `1` = day, `0` = night.
    pub is_day: i64,
}

impl Default for CalendarSection {
    fn default() -> Self {
        Self {
            game_minutes: 0,
            weather: [0; WEATHER_COUNT],
            is_day: 0,
        }
    }
}

impl SaveSection for CalendarSection {
    const KEY: &'static str = "calendar";
    const OWNER: &'static str = "M31/M18";

    fn write_lines(&self, out: &mut String) {
        push_pair(out, "game_minutes", self.game_minutes);
        push_csv(out, "weather", &self.weather);
        push_pair(out, "is_day", self.is_day);
    }

    fn parse(cur: &mut Cursor<'_>) -> Result<Self, SaveError> {
        Ok(Self {
            game_minutes: cur.expect_u64("game_minutes")?,
            weather: parse_csv_fixed(cur.expect("weather")?, "weather")?,
            is_day: cur.expect_i64("is_day")?,
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 11 · map (M18/M34)
// ═══════════════════════════════════════════════════════════════════════════

/// Per-dungeon automap (`automap.<i>`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AutomapV2 {
    /// Location index of the dungeon.
    pub location: i64,
    /// Explored blocks.
    pub blocks: Vec<i64>,
}

/// Loaded chunks and automaps (row 11).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MapSection {
    /// `(x, z)` chunks, ascending.
    pub chunks: Vec<(i64, i64)>,
    /// Automaps in insertion order.
    pub automaps: Vec<AutomapV2>,
}

impl SaveSection for MapSection {
    const KEY: &'static str = "map";
    const OWNER: &'static str = "M18/M34";

    fn write_lines(&self, out: &mut String) {
        push_pair(out, "chunks_count", self.chunks.len());
        for (x, z) in &self.chunks {
            push_pair(out, "chunk", format!("{x}|{z}"));
        }
        push_pair(out, "automap_count", self.automaps.len());
        for (index, automap) in self.automaps.iter().enumerate() {
            let key = format!("automap.{index}");
            out.push_str(&key);
            out.push('=');
            out.push_str(&automap.location.to_string());
            for block in &automap.blocks {
                out.push(',');
                out.push_str(&block.to_string());
            }
            out.push('\n');
        }
    }

    fn parse(cur: &mut Cursor<'_>) -> Result<Self, SaveError> {
        let chunks_count = cur.expect_count("chunks_count")?;
        let mut chunks = Vec::with_capacity(chunks_count);
        for _ in 0..chunks_count {
            let value = cur.expect("chunk")?;
            let (x, z) = value.split_once('|').ok_or_else(|| {
                SaveError::Corrupt(format!("chunk without '|': {value:?}"))
            })?;
            chunks.push((parse_i64(x)?, parse_i64(z)?));
        }
        let automap_count = cur.expect_count("automap_count")?;
        let mut automaps = Vec::with_capacity(automap_count);
        for index in 0..automap_count {
            let value = cur.expect(&format!("automap.{index}"))?;
            let (location, rest) = value.split_once(',').ok_or_else(|| {
                SaveError::Corrupt(format!("automap {index}: no location separator"))
            })?;
            automaps.push(AutomapV2 {
                location: parse_i64(location)?,
                blocks: parse_csv_vec(rest)?,
            });
        }
        Ok(Self { chunks, automaps })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 12 · rng (all)
// ═══════════════════════════════════════════════════════════════════════════

/// Persisted RNG states, ascending by name (row 12).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RngSection {
    /// `(system name, state)` ascending by name.
    pub states: Vec<(String, u64)>,
}

impl SaveSection for RngSection {
    const KEY: &'static str = "rng";
    const OWNER: &'static str = "all";

    fn write_lines(&self, out: &mut String) {
        push_pair(out, "rng_count", self.states.len());
        for (index, (name, state)) in self.states.iter().enumerate() {
            let key = format!("rng.{index}");
            out.push_str(&key);
            out.push('=');
            out.push_str(&escape(name));
            out.push('=');
            out.push_str(&state.to_string());
            out.push('\n');
        }
    }

    fn parse(cur: &mut Cursor<'_>) -> Result<Self, SaveError> {
        let count = cur.expect_count("rng_count")?;
        let mut states = Vec::with_capacity(count);
        for index in 0..count {
            let value = cur.expect(&format!("rng.{index}"))?;
            let (name, state) = value.rsplit_once('=').ok_or_else(|| {
                SaveError::Corrupt(format!("rng {index}: no '=' separator"))
            })?;
            states.push((unescape(name)?, parse_u64(state)?));
        }
        Ok(Self { states })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// The document: the twelve sections in canonical order
// ═══════════════════════════════════════════════════════════════════════════

/// The complete v2 save state: twelve sections, canonical order (§3.3).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SaveGameV2 {
    /// Section 1 — world.
    pub world: WorldSection,
    /// Section 2 — clock.
    pub clock: ClockSection,
    /// Section 3 — character.
    pub character: CharacterSection,
    /// Section 4 — inventory.
    pub inventory: InventorySection,
    /// Section 5 — effects.
    pub effects: EffectsSection,
    /// Section 6 — guilds.
    pub guilds: GuildsSection,
    /// Section 7 — crime.
    pub crime: CrimeSection,
    /// Section 8 — quests.
    pub quests: QuestsSection,
    /// Section 9 — economy.
    pub economy: EconomySection,
    /// Section 10 — calendar.
    pub calendar: CalendarSection,
    /// Section 11 — map.
    pub map: MapSection,
    /// Section 12 — rng.
    pub rng: RngSection,
}

impl SaveGameV2 {
    /// An empty document at the current version (every list empty, every
    /// equipment slot `-1`).
    pub fn new() -> Self {
        Self::default()
    }

    /// The canonical golden sample of spec §4 (S0).
    ///
    /// Breton, level 2, one equipped item, one cart container, one levitation
    /// effect, one disease (id 3, 5 days), no vampirism/lycanthropy, one guild
    /// (faction 150, rank 2), two legal reputations, two quests, one quest
    /// state, one bank account, one loan, five chunks, one dungeon automap and
    /// one RNG state: [`S0_TEXT`] byte for byte.
    pub fn sample() -> Self {
        Self {
            world: WorldSection {
                pos_x_mm: 1500,
                pos_y_mm: 0,
                pos_z_mm: -2000,
                yaw_mrad: 314,
                pitch_mrad: 0,
                region_index: 19,
                location_index: 0,
                environment: 1,
                map_id: 65535,
            },
            clock: ClockSection {
                year: 405,
                month: 5,
                day: 0,
                hour: 13,
                minute: 1,
                second: 1,
            },
            character: CharacterSection {
                level: 2,
                experience: 50,
                health: 70,
                max_health: 100,
                stamina: 100,
                max_stamina: 100,
                spell_points: 30,
                max_spell_points: 30,
                fatigue: 100,
                name: "Bretón de la Costa".to_string(),
                gender: 0,
                race: 1,
                career_id: 7,
                face_index: 12,
                reflexes: 1,
                attributes: [50, 51, 52, 53, 54, 55, 56, 57],
                attributes_base: [50, 51, 52, 53, 54, 55, 56, 57],
                skills: [
                    5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
                    26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39,
                ],
                skill_uses: [0; SKILL_COUNT],
                skills_raised_1: 3,
                skills_raised_2: 0,
                levelup_skill_sum: 120,
            },
            inventory: InventorySection {
                gold: 55,
                goods: vec![
                    GoodV2 {
                        id: "pieles".to_string(),
                        quantity: 1,
                    },
                    GoodV2 {
                        id: "pocion".to_string(),
                        quantity: 1,
                    },
                ],
                items: vec![ItemV2 {
                    group: 0,
                    index: 12,
                    value: 150,
                    flags: 1,
                    current_condition: 45,
                    max_condition: 50,
                    type_dependent: 1,
                    material: 2,
                    color: 3,
                    weight_cu: 40,
                    enchantment_points: 0,
                    variants: 0,
                    draw_order: 1,
                    enchantment_slots: 0,
                    enchants: Vec::new(),
                }],
                equipped: {
                    let mut slots = [-1i64; EQUIPPED_SLOTS];
                    slots[0] = 0;
                    slots
                },
                containers: vec![ContainerV2 {
                    kind: 4,
                    items: vec![0],
                }],
            },
            effects: EffectsSection {
                effects: vec![ActiveEffectV2 {
                    key: "LEVITATE".to_string(),
                    school: 0,
                    variants: 0,
                    rounds: 12,
                    magnitude: 4,
                }],
                diseases: vec![DiseaseV2 {
                    id: 3,
                    days: 5,
                    incubation: 1,
                    accumulated: [0; DISEASE_ACCUMULATORS],
                }],
                vampire_clan: 0,
                lycanthropy: 0,
            },
            guilds: GuildsSection {
                guilds: vec![GuildMembershipV2 {
                    faction: 150,
                    rank: 2,
                    noted: 0,
                    min_last_change: 1200,
                    blessing: 0,
                }],
            },
            crime: CrimeSection {
                crime_committed: 0,
                legal_rep: vec![(19, 100), (20, 90)],
                prison_days: 0,
            },
            quests: QuestsSection {
                quests: vec![
                    (
                        "q-tienda-vendel".to_string(),
                        "Localiza la tienda de Vendel".to_string(),
                    ),
                    (
                        "q-caverna".to_string(),
                        "Explora la caverna al este".to_string(),
                    ),
                ],
                states: vec![("q-tienda-vendel".to_string(), "stage=2".to_string())],
            },
            economy: EconomySection {
                bank: vec![(19, 120)],
                loans: vec![LoanV2 {
                    region: 19,
                    principal: 500,
                    due_min: 9000,
                    unpaid: 0,
                }],
                property_house: 0,
                property_ship: 0,
            },
            calendar: CalendarSection {
                game_minutes: 142_561,
                weather: [0, 1, 2, 3, 4, 5],
                is_day: 1,
            },
            map: MapSection {
                chunks: vec![(0, 0), (0, 1), (1, 0), (1, 1), (2, 0)],
                automaps: vec![AutomapV2 {
                    location: 19,
                    blocks: vec![0, 0, 0, 0],
                }],
            },
            rng: RngSection {
                states: vec![("world".to_string(), 12_345_678_901_234_567)],
            },
        }
    }

    /// Puts the ascending families in canonical order (§3.3 rows 6, 7, 8, 11, 12).
    ///
    /// A document read back from [`Self::from_text`] is always canonical; this
    /// is the constructor door for state assembled by modules.
    pub fn canonicalize(&mut self) {
        self.guilds.guilds.sort_by_key(|guild| guild.faction);
        self.crime.legal_rep.sort_by_key(|(region, _)| *region);
        self.quests.states.sort_by(|a, b| a.0.cmp(&b.0));
        self.economy.bank.sort_by_key(|(region, _)| *region);
        self.map.chunks.sort();
        self.rng.states.sort_by(|a, b| a.0.cmp(&b.0));
    }

    /// Checks the invariants a canonical document must satisfy: ascending
    /// families, `ench` arity and the two characters that would break a line.
    pub fn validate(&self) -> Result<(), SaveError> {
        fn ascending<T: Ord>(items: impl Iterator<Item = T>, what: &str) -> Result<(), SaveError> {
            let mut previous: Option<T> = None;
            for item in items {
                if let Some(previous) = &previous {
                    if item <= *previous {
                        return Err(SaveError::Corrupt(format!(
                            "{what} must be strictly ascending"
                        )));
                    }
                }
                previous = Some(item);
            }
            Ok(())
        }

        ascending(
            self.guilds.guilds.iter().map(|guild| guild.faction),
            "guilds",
        )?;
        ascending(
            self.crime.legal_rep.iter().map(|(region, _)| *region),
            "legal_rep",
        )?;
        ascending(self.quests.states.iter().map(|(key, _)| key.as_str()), "quest_state")?;
        ascending(self.economy.bank.iter().map(|(region, _)| *region), "bank")?;
        ascending(self.map.chunks.iter().copied(), "chunks")?;
        ascending(self.rng.states.iter().map(|(name, _)| name.as_str()), "rng")?;

        for (index, item) in self.inventory.items.iter().enumerate() {
            let want = usize::try_from(item.enchantment_slots)
                .map_err(|_| SaveError::Corrupt(format!("item.{index}: negative slots")))?;
            if item.enchants.len() != want {
                return Err(SaveError::Corrupt(format!(
                    "item.{index}: {} enchantments, want {want}",
                    item.enchants.len()
                )));
            }
        }
        for (index, effect) in self.effects.effects.iter().enumerate() {
            if effect.key.contains(',') {
                return Err(SaveError::Corrupt(format!(
                    "effect.{index}: the key contains ',' (the line is CSV)"
                )));
            }
        }
        for (index, (name, _)) in self.rng.states.iter().enumerate() {
            if name.contains('=') {
                return Err(SaveError::Corrupt(format!(
                    "rng.{index}: the name contains '=' (the state is the tail)"
                )));
            }
        }
        Ok(())
    }

    /// Appends the body (header + fixed keys + families) to `out`; the
    /// checksum line is the caller's job so it can hash exactly these bytes.
    fn write_body(&self, out: &mut String) {
        out.push_str(MAGIC);
        out.push(' ');
        out.push_str(&VERSION.to_string());
        out.push('\n');
        self.world.write_lines(out);
        self.clock.write_lines(out);
        self.character.write_lines(out);
        self.inventory.write_lines(out);
        self.effects.write_lines(out);
        self.guilds.write_lines(out);
        self.crime.write_lines(out);
        self.quests.write_lines(out);
        self.economy.write_lines(out);
        self.calendar.write_lines(out);
        self.map.write_lines(out);
        self.rng.write_lines(out);
    }

    /// Canonical text of the document, checksum line included.
    pub fn to_text(&self) -> String {
        let mut body = String::new();
        self.write_body(&mut body);
        let checksum = fnv1a64(body.as_bytes());
        body.push_str(&format!("checksum={checksum:016x}\n"));
        body
    }

    /// Payload bytes the checksum covers (the body before `checksum=`).
    pub fn payload(&self) -> String {
        let mut body = String::new();
        self.write_body(&mut body);
        body
    }

    /// FNV-1a 64 of [`Self::payload`] — the digest the probes print.
    pub fn digest(&self) -> u64 {
        fnv1a64(self.payload().as_bytes())
    }

    /// Number of lines [`Self::to_text`] produces.
    pub fn line_count(&self) -> usize {
        FIXED_LINES + 2 + self.entry_count()
    }

    /// Entries across the fourteen list families (GV-08).
    pub fn entry_count(&self) -> usize {
        self.inventory.goods.len()
            + self.inventory.items.len()
            + self.inventory.containers.len()
            + self.effects.effects.len()
            + self.effects.diseases.len()
            + self.guilds.guilds.len()
            + self.crime.legal_rep.len()
            + self.quests.quests.len()
            + self.quests.states.len()
            + self.economy.bank.len()
            + self.economy.loans.len()
            + self.map.chunks.len()
            + self.map.automaps.len()
            + self.rng.states.len()
    }

    /// Parses a canonical document (magic + version + checksum + closed keys).
    pub fn from_text(text: &str) -> Result<Self, SaveError> {
        let header = text
            .lines()
            .next()
            .ok_or_else(|| SaveError::Corrupt("empty save".into()))?;
        let (magic, version) = header
            .split_once(' ')
            .ok_or_else(|| SaveError::Corrupt("header without version".into()))?;
        if magic != MAGIC {
            return Err(SaveError::BadMagic);
        }
        let version: i64 = version
            .parse()
            .map_err(|_| SaveError::Corrupt("non-numeric version".into()))?;
        if version != VERSION {
            return Err(SaveError::BadVersion(version));
        }

        // The checksum covers every byte before its own line.
        let last = text
            .lines()
            .next_back()
            .ok_or_else(|| SaveError::Corrupt("no checksum line".into()))?;
        let expected_hex = last
            .strip_prefix("checksum=")
            .ok_or_else(|| SaveError::Corrupt("last line is not a checksum".into()))?;
        let expected = u64::from_str_radix(expected_hex, 16)
            .map_err(|_| SaveError::Corrupt("non-hexadecimal checksum".into()))?;
        let payload = payload_of(text);
        let actual = fnv1a64(payload.as_bytes());
        if actual != expected {
            return Err(SaveError::Corrupt(format!(
                "checksum mismatch (expected {expected:016x}, computed {actual:016x})"
            )));
        }

        let body = payload_of_body(&payload);
        let mut cur = Cursor::new(&body)?;
        let doc = Self {
            world: WorldSection::parse(&mut cur)?,
            clock: ClockSection::parse(&mut cur)?,
            character: CharacterSection::parse(&mut cur)?,
            inventory: InventorySection::parse(&mut cur)?,
            effects: EffectsSection::parse(&mut cur)?,
            guilds: GuildsSection::parse(&mut cur)?,
            crime: CrimeSection::parse(&mut cur)?,
            quests: QuestsSection::parse(&mut cur)?,
            economy: EconomySection::parse(&mut cur)?,
            calendar: CalendarSection::parse(&mut cur)?,
            map: MapSection::parse(&mut cur)?,
            rng: RngSection::parse(&mut cur)?,
        };
        if cur.remaining() != 0 {
            let (key, _) = cur.next()?;
            return Err(SaveError::Corrupt(format!(
                "unknown key after the last section: {key:?}"
            )));
        }
        doc.validate()?;
        // A canonical document is byte-identical to its own re-emission: this
        // is the determinism check, not a stylistic one.
        if doc.to_text() != text {
            return Err(SaveError::Corrupt(
                "the document is not in canonical form (bytes differ from re-emission)".into(),
            ));
        }
        Ok(doc)
    }

    /// Writes the document to `path` (creates missing parents).
    pub fn save_to_path(&self, path: &Path) -> Result<(), SaveError> {
        self.validate()?;
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(SaveError::Io)?;
            }
        }
        fs::write(path, self.to_text().as_bytes()).map_err(SaveError::Io)
    }

    /// Reads a document from `path`.
    pub fn load_from_path(path: &Path) -> Result<Self, SaveError> {
        let bytes = fs::read(path).map_err(SaveError::Io)?;
        let text = String::from_utf8(bytes)
            .map_err(|_| SaveError::Corrupt("the save is not valid UTF-8".into()))?;
        Self::from_text(&text)
    }

    /// Envelope of the twelve sections, in canonical id order.
    ///
    /// This is the "`memory` only composes" path: each section contributes its
    /// own [`SaveSection::encode_bytes`], nothing else.
    pub fn to_state(&self) -> Result<SaveStateV2, SaveError> {
        self.validate()?;
        let mut state = SaveStateV2::new();
        let sections: [(SectionId, Vec<u8>); 12] = [
            (WorldSection::id(), self.world.encode_bytes()),
            (ClockSection::id(), self.clock.encode_bytes()),
            (CharacterSection::id(), self.character.encode_bytes()),
            (InventorySection::id(), self.inventory.encode_bytes()),
            (EffectsSection::id(), self.effects.encode_bytes()),
            (GuildsSection::id(), self.guilds.encode_bytes()),
            (CrimeSection::id(), self.crime.encode_bytes()),
            (QuestsSection::id(), self.quests.encode_bytes()),
            (EconomySection::id(), self.economy.encode_bytes()),
            (CalendarSection::id(), self.calendar.encode_bytes()),
            (MapSection::id(), self.map.encode_bytes()),
            (RngSection::id(), self.rng.encode_bytes()),
        ];
        for (id, bytes) in sections {
            state.push(id, bytes).map_err(|error| {
                SaveError::Corrupt(format!("composing section {}: {error}", id.key()))
            })?;
        }
        Ok(state)
    }

    /// Rebuilds the document from an envelope (every section must be present).
    pub fn from_state(state: &SaveStateV2) -> Result<Self, SaveError> {
        Ok(Self {
            world: decode_section::<WorldSection>(state)?,
            clock: decode_section::<ClockSection>(state)?,
            character: decode_section::<CharacterSection>(state)?,
            inventory: decode_section::<InventorySection>(state)?,
            effects: decode_section::<EffectsSection>(state)?,
            guilds: decode_section::<GuildsSection>(state)?,
            crime: decode_section::<CrimeSection>(state)?,
            quests: decode_section::<QuestsSection>(state)?,
            economy: decode_section::<EconomySection>(state)?,
            calendar: decode_section::<CalendarSection>(state)?,
            map: decode_section::<MapSection>(state)?,
            rng: decode_section::<RngSection>(state)?,
        })
    }

    /// Migrates the sixteen v1 scalars into a v2 document (spec §5).
    ///
    /// New keys take their declared defaults; `goods`, `quests` and `chunks`
    /// keep their insertion order. The v1 name does not exist, so `name` stays
    /// empty (a partial migration is declared, never invented), and
    /// `game_minutes` stays `0`: the caller must supply the absolute date if it
    /// has one.
    pub fn from_v1(snapshot: &SessionSnapshot) -> Self {
        let mut doc = Self::default();
        doc.world.pos_x_mm = snapshot.pos_x_mm;
        doc.world.pos_z_mm = snapshot.pos_z_mm;
        doc.world.yaw_mrad = snapshot.yaw_mrad;
        doc.clock = ClockSection {
            year: snapshot.clock_year,
            month: snapshot.clock_month,
            day: snapshot.clock_day,
            hour: snapshot.clock_hour,
            minute: snapshot.clock_minute,
            second: snapshot.clock_second,
        };
        doc.character.level = snapshot.level;
        doc.character.experience = snapshot.experience;
        doc.character.health = snapshot.health;
        doc.character.max_health = snapshot.max_health;
        doc.character.stamina = snapshot.stamina;
        doc.character.max_stamina = snapshot.max_stamina;
        doc.inventory.gold = snapshot.gold;
        doc.inventory.goods = snapshot
            .goods
            .iter()
            .map(|(id, quantity)| GoodV2 {
                id: id.clone(),
                quantity: *quantity,
            })
            .collect();
        doc.quests.quests = snapshot.quests.clone();
        doc.map.chunks = snapshot.chunks.clone();
        doc
    }
}

fn decode_section<S: SaveSection>(state: &SaveStateV2) -> Result<S, SaveError> {
    let section = state.section(S::id()).ok_or_else(|| {
        SaveError::Corrupt(format!("section {} is missing from the envelope", S::KEY))
    })?;
    S::decode_bytes(&section.bytes)
}

/// Every byte before the `checksum=` line, as text.
fn payload_of(text: &str) -> String {
    let total = text.lines().count();
    let mut payload = String::with_capacity(text.len());
    for line in text.lines().take(total.saturating_sub(1)) {
        payload.push_str(line);
        payload.push('\n');
    }
    payload
}

/// The body without its header line (sections start after `DFL-SESSION 2`).
fn payload_of_body(payload: &str) -> String {
    let mut body = String::with_capacity(payload.len());
    for line in payload.lines().skip(1) {
        body.push_str(line);
        body.push('\n');
    }
    body
}

// ═══════════════════════════════════════════════════════════════════════════
// Migration v1 → v2 (§5)
// ═══════════════════════════════════════════════════════════════════════════

/// Migrates `DFL-SESSION 1` text to canonical `DFL-SESSION 2` text.
///
/// A v2 document is re-parsed and re-emitted, which makes the migration
/// idempotent by construction: `migrate(migrate(x)) == migrate(x)`. Any other
/// version is [`SaveError::BadVersion`] and a foreign magic is
/// [`SaveError::BadMagic`] (spec §5a).
pub fn migrate_text(text: &str) -> Result<String, SaveError> {
    let header = text
        .lines()
        .next()
        .ok_or_else(|| SaveError::Corrupt("empty save".into()))?;
    let (magic, version) = header
        .split_once(' ')
        .ok_or_else(|| SaveError::Corrupt("header without version".into()))?;
    if magic != MAGIC {
        return Err(SaveError::BadMagic);
    }
    let version: i64 = version
        .parse()
        .map_err(|_| SaveError::Corrupt("non-numeric version".into()))?;
    match version {
        crate::session_store::FORMAT_VERSION => {
            let snapshot = SessionSnapshot::from_text(text)?;
            Ok(SaveGameV2::from_v1(&snapshot).to_text())
        }
        VERSION => {
            let doc = SaveGameV2::from_text(text)?;
            Ok(doc.to_text())
        }
        other => Err(SaveError::BadVersion(other)),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Envelope codecs (the owner rule of contract §3)
// ═══════════════════════════════════════════════════════════════════════════

/// [`SectionCodec`] of one M33 section: `memory` sees bytes, the section keeps
/// its own types.
pub struct M33Codec<S: SaveSection> {
    state: S,
}

impl<S: SaveSection> Default for M33Codec<S> {
    fn default() -> Self {
        Self { state: S::default() }
    }
}

impl<S: SaveSection> M33Codec<S> {
    /// Codec holding `state` (the owner's current section).
    pub fn new(state: S) -> Self {
        Self { state }
    }

    /// The section the codec currently holds.
    pub fn state(&self) -> &S {
        &self.state
    }
}

impl<S: SaveSection> SectionCodec for M33Codec<S> {
    fn id(&self) -> SectionId {
        S::id()
    }

    fn encode(&self) -> Vec<u8> {
        self.state.encode_bytes()
    }

    fn decode(&mut self, bytes: &[u8]) -> Result<(), SectionError> {
        match S::decode_bytes(bytes) {
            Ok(state) => {
                self.state = state;
                Ok(())
            }
            Err(_) => Err(SectionError::new("M33: the section payload was rejected")),
        }
    }
}

fn attach<S: SaveSection + 'static>(registry: &mut SectionRegistry) -> Result<(), RegistryError> {
    registry.attach_codec(S::OWNER, Box::new(M33Codec::<S>::default()))
}

/// Attaches the codec of all twelve canonical sections, each under its owner.
///
/// After this call a registry that declared [`crate::save_v2::M33_SECTIONS`]
/// reports `12/12` coverage: `memory` can now compose, encode and decode the
/// whole game state.
pub fn attach_m33_codecs(registry: &mut SectionRegistry) -> Result<(), RegistryError> {
    attach::<WorldSection>(registry)?;
    attach::<ClockSection>(registry)?;
    attach::<CharacterSection>(registry)?;
    attach::<InventorySection>(registry)?;
    attach::<EffectsSection>(registry)?;
    attach::<GuildsSection>(registry)?;
    attach::<CrimeSection>(registry)?;
    attach::<QuestsSection>(registry)?;
    attach::<EconomySection>(registry)?;
    attach::<CalendarSection>(registry)?;
    attach::<MapSection>(registry)?;
    attach::<RngSection>(registry)?;
    Ok(())
}

/// Maps an envelope refusal into the save error the caller already handles.
impl From<EnvelopeError> for SaveError {
    fn from(error: EnvelopeError) -> Self {
        SaveError::Corrupt(format!("save envelope: {error}"))
    }
}

/// The canonical golden sample S0 of spec §4, byte for byte (GV-07).
///
/// 85 lines · 1705 bytes · `checksum=f793b743974d7f39`.
pub const S0_TEXT: &str = "\
DFL-SESSION 2
pos_x_mm=1500
pos_y_mm=0
pos_z_mm=-2000
yaw_mrad=314
pitch_mrad=0
region_index=19
location_index=0
environment=1
map_id=65535
clock_year=405
clock_month=5
clock_day=0
clock_hour=13
clock_minute=1
clock_second=1
level=2
experience=50
health=70
max_health=100
stamina=100
max_stamina=100
spell_points=30
max_spell_points=30
fatigue=100
name=Bretón de la Costa
gender=0
race=1
career_id=7
face_index=12
reflexes=1
attributes=50,51,52,53,54,55,56,57
attributes_base=50,51,52,53,54,55,56,57
skills=5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,38,39
skill_uses=0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0
skills_raised_1=3
skills_raised_2=0
levelup_skill_sum=120
gold=55
goods_count=2
goods=pieles|1
goods=pocion|1
items_count=1
item.0=0,12,150,1,45,50,1,2,3,40,0,0,1,0
equipped=0,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1
containers_count=1
container.0=4,0
effects_count=1
effect.0=LEVITATE,0,0,12,4
diseases_count=1
disease.0=3,5,1,0,0,0,0,0,0,0,0,0,0,0
vampire_clan=0
lycanthropy=0
guilds_count=1
guild.0=150,2,0,1200,0
crime_committed=0
legal_rep_count=2
legal_rep.19=100
legal_rep.20=90
prison_days=0
quests_count=2
quest=q-tienda-vendel|Localiza la tienda de Vendel
quest=q-caverna|Explora la caverna al este
quest_state_count=1
quest_state.0=q-tienda-vendel|stage=2
bank_count=1
bank.19=120
loans_count=1
loan.0=19,500,9000,0
property_house=0
property_ship=0
game_minutes=142561
weather=0,1,2,3,4,5
is_day=1
chunks_count=5
chunk=0|0
chunk=0|1
chunk=1|0
chunk=1|1
chunk=2|0
automap_count=1
automap.0=19,0,0,0,0
rng_count=1
rng.0=world=12345678901234567
checksum=f793b743974d7f39
";

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> usize {
        text.lines().count()
    }

    #[test]
    fn golden_sample_matches_spec_section_4() {
        let doc = SaveGameV2::sample();
        let text = doc.to_text();
        assert_eq!(text, S0_TEXT, "S0 must be transcribed literally");
        assert_eq!(lines(&text), 85);
        assert_eq!(text.len(), 1705);
        assert_eq!(doc.digest(), 0xf793_b743_974d_7f39);
        assert_eq!(doc.entry_count(), 21, "GV-08");
        assert_eq!(doc.line_count(), 85);
    }

    #[test]
    fn golden_sample_roundtrips_byte_identically() {
        let doc = SaveGameV2::from_text(S0_TEXT).expect("S0 parses");
        assert_eq!(doc, SaveGameV2::sample());
        assert_eq!(doc.to_text(), S0_TEXT);
        assert_eq!(doc.digest(), SaveGameV2::sample().digest());
    }

    #[test]
    fn section_bytes_roundtrip() {
        let doc = SaveGameV2::sample();
        assert_eq!(
            WorldSection::decode_bytes(&doc.world.encode_bytes()).expect("world"),
            doc.world
        );
        assert_eq!(
            InventorySection::decode_bytes(&doc.inventory.encode_bytes()).expect("inventory"),
            doc.inventory
        );
        assert_eq!(
            EffectsSection::decode_bytes(&doc.effects.encode_bytes()).expect("effects"),
            doc.effects
        );
        assert_eq!(
            QuestsSection::decode_bytes(&doc.quests.encode_bytes()).expect("quests"),
            doc.quests
        );
        assert_eq!(
            RngSection::decode_bytes(&doc.rng.encode_bytes()).expect("rng"),
            doc.rng
        );
    }

    #[test]
    fn envelope_composition_roundtrips() {
        let doc = SaveGameV2::sample();
        let state = doc.to_state().expect("compose");
        assert_eq!(state.sections.len(), 12);
        assert_eq!(SaveGameV2::from_state(&state).expect("decode"), doc);
    }

    #[test]
    fn all_twelve_codecs_attach_to_the_canonical_table() {
        let mut registry = SectionRegistry::new();
        registry
            .declare_all(&crate::save_v2::M33_SECTIONS)
            .expect("declare");
        attach_m33_codecs(&mut registry).expect("attach");
        let coverage = registry.coverage(&crate::save_v2::M33_SECTIONS);
        assert!(coverage.complete(), "missing: {}", coverage.missing_list());
        assert_eq!(coverage.ratio(), "12/12");
        let composed = registry.compose();
        let restored = SaveGameV2::from_state(&composed).expect("compose/decode");
        assert_eq!(restored, SaveGameV2::default());
    }

    #[test]
    fn checksum_detects_one_changed_byte() {
        let text = S0_TEXT.replace("gold=55", "gold=65");
        assert!(matches!(
            SaveGameV2::from_text(&text),
            Err(SaveError::Corrupt(_))
        ));
    }

    #[test]
    fn unknown_duplicate_and_short_count_are_corrupt() {
        // Unknown key, with a valid checksum: the closed-key rule is the barrier.
        let mut lines: Vec<String> = S0_TEXT.lines().map(str::to_string).collect();
        lines.pop();
        lines.push("gold_extra=1".to_string());
        let mut body = lines.join("\n");
        body.push('\n');
        let checksum = fnv1a64(body.as_bytes());
        body.push_str(&format!("checksum={checksum:016x}\n"));
        assert!(matches!(
            SaveGameV2::from_text(&body),
            Err(SaveError::Corrupt(_))
        ));

        // Duplicated key, valid checksum.
        let mut lines: Vec<String> = S0_TEXT.lines().map(str::to_string).collect();
        lines.pop();
        lines.insert(42, "gold=55".to_string());
        let mut body = lines.join("\n");
        body.push('\n');
        let checksum = fnv1a64(body.as_bytes());
        body.push_str(&format!("checksum={checksum:016x}\n"));
        assert!(matches!(
            SaveGameV2::from_text(&body),
            Err(SaveError::Corrupt(_))
        ));

        // Inflated count, valid checksum.
        let text = S0_TEXT.replace("goods_count=2", "goods_count=3");
        let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
        lines.pop();
        let mut body = lines.join("\n");
        body.push('\n');
        let checksum = fnv1a64(body.as_bytes());
        body.push_str(&format!("checksum={checksum:016x}\n"));
        assert!(matches!(
            SaveGameV2::from_text(&body),
            Err(SaveError::Corrupt(_))
        ));
    }

    #[test]
    fn bad_magic_and_version_are_rejected() {
        let bad_magic = S0_TEXT.replacen(MAGIC, "OTHER-FORMAT", 1);
        assert!(matches!(
            SaveGameV2::from_text(&bad_magic),
            Err(SaveError::BadMagic)
        ));
        let bad_version = S0_TEXT.replacen("DFL-SESSION 2", "DFL-SESSION 999", 1);
        assert!(matches!(
            SaveGameV2::from_text(&bad_version),
            Err(SaveError::BadVersion(999))
        ));
    }

    #[test]
    fn escaping_roundtrips_every_special_character() {
        let mut doc = SaveGameV2::sample();
        doc.character.name = "Título con \\ barra y | pipe\ny salto\rfin".to_string();
        doc.quests.quests.push((
            "q-|odd".to_string(),
            "Title with \\ backslash and | pipe\nand newline".to_string(),
        ));
        let text = doc.to_text();
        let parsed = SaveGameV2::from_text(&text).expect("escaped document parses");
        assert_eq!(parsed, doc);
        assert_eq!(parsed.to_text(), text);
        assert!(!text.contains("pipe\ny salto"), "the newline must be escaped");
    }

    #[test]
    fn migration_from_v1_keeps_the_sixteen_scalars_and_is_idempotent() {
        let mut snapshot = SessionSnapshot {
            pos_x_mm: 1500,
            pos_z_mm: -2000,
            yaw_mrad: 314,
            clock_year: 405,
            clock_month: 5,
            clock_day: 0,
            clock_hour: 13,
            clock_minute: 1,
            clock_second: 1,
            level: 2,
            experience: 50,
            health: 70,
            max_health: 100,
            stamina: 100,
            max_stamina: 100,
            gold: 55,
            goods: vec![("hides".into(), 1), ("potion".into(), 1)],
            quests: vec![
                ("q-vendel-shop".into(), "Find Vendel's shop".into()),
                ("q-cavern".into(), "Explore the cavern to the east".into()),
            ],
            chunks: vec![(0, 0), (0, 1), (1, 0), (1, 1), (2, 0)],
        };
        let v1 = snapshot.to_text();
        let migrated = migrate_text(&v1).expect("v1 migrates");
        let doc = SaveGameV2::from_text(&migrated).expect("migrated v2 parses");
        assert_eq!(doc.world.pos_x_mm, snapshot.pos_x_mm);
        assert_eq!(doc.world.pos_z_mm, snapshot.pos_z_mm);
        assert_eq!(doc.clock.hour, snapshot.clock_hour);
        assert_eq!(doc.character.health, snapshot.health);
        assert_eq!(doc.inventory.gold, snapshot.gold);
        assert_eq!(doc.quests.quests, snapshot.quests);
        assert_eq!(doc.map.chunks, snapshot.chunks);
        assert_eq!(doc.character.name, "");
        assert_eq!(doc.calendar.game_minutes, 0);
        assert_eq!(
            doc.inventory.equipped,
            [-1; EQUIPPED_SLOTS],
            "new slots default to -1"
        );
        assert_eq!(migrate_text(&migrated).expect("idempotent"), migrated);
        assert_eq!(migrate_text(&v1).expect("deterministic"), migrated);

        snapshot.goods.clear();
        let empty = migrate_text(&snapshot.to_text()).expect("empty goods migrate");
        assert_eq!(
            SaveGameV2::from_text(&empty).expect("parse").inventory.goods.len(),
            0
        );
    }

    #[test]
    fn migration_rejects_a_foreign_version() {
        let text = S0_TEXT.replacen("DFL-SESSION 2", "DFL-SESSION 3", 1);
        assert!(matches!(
            migrate_text(&text),
            Err(SaveError::BadVersion(3))
        ));
        assert!(matches!(
            migrate_text("OTHER 1\n"),
            Err(SaveError::BadMagic)
        ));
    }

    #[test]
    fn file_roundtrip_preserves_the_document() {
        let path = std::env::temp_dir().join(format!(
            "daggerfall-m33-{}-{}.dfl",
            std::process::id(),
            line!()
        ));
        let doc = SaveGameV2::sample();
        doc.save_to_path(&path).expect("save");
        let loaded = SaveGameV2::load_from_path(&path).expect("load");
        assert_eq!(loaded, doc);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn unordered_families_are_refused_by_validate() {
        let mut doc = SaveGameV2::sample();
        doc.guilds.guilds = vec![
            GuildMembershipV2 {
                faction: 200,
                ..GuildMembershipV2::default()
            },
            GuildMembershipV2 {
                faction: 100,
                ..GuildMembershipV2::default()
            },
        ];
        assert!(doc.validate().is_err());
        doc.canonicalize();
        assert!(doc.validate().is_ok());
        assert_eq!(doc.guilds.guilds[0].faction, 100);
    }

    #[test]
    fn empty_document_has_the_fixed_census() {
        let doc = SaveGameV2::new();
        let text = doc.to_text();
        assert_eq!(lines(&text), FIXED_LINES + 2);
        assert_eq!(doc.entry_count(), 0);
        let parsed = SaveGameV2::from_text(&text).expect("empty parses");
        assert_eq!(parsed, doc);
        assert_eq!(
            SaveGameV2::from_v1(&SessionSnapshot::default()).to_text(),
            text,
            "a default v1 snapshot migrates to the empty document"
        );
    }
}

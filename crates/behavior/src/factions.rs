//! M27 · Factions — `FACTION.TXT` graph, reputation and propagation
//! (SPEC-M27-FACTIONS §1 and §4; REQ-L99-060).
//!
//! 1:1 port of `DaggerfallConnect.Arena2.FactionFile.ParseFactions`
//! (`API/FactionFile.cs:930-1026`), `ParseFactionData` (`:1027-1210`) and of
//! `PersistentFactionData.ChangeReputation` / `PropagateReputationChange`
//! (`Game/Player/PersistentFactionData.cs:390-458`) over the MIT source
//! pinned at `0038a1e61999ad2c73fb1f33b706cae9a88ec66f`.
//!
//! Fidelity decisions the port preserves (bug-compatible):
//! - **Name**: *case-insensitive* opening by the caller; the parser does not
//!   rename the file (`FactionFile.cs:741`, R-G2).
//! - **Comments**: only lines starting with `;` or empty lines; a line with
//!   tabs and spaces is NOT discarded (`:945`).
//! - **Blocks**: **any** line containing `#` opens a block (`:945-957`).
//! - **Kinship**: leading tabs + parent stack (`:965-989`); `parent` defaults
//!   to `0` (C# struct), and `propagate_tree` uses it as the "root"
//!   (`PersistentFactionData.cs:452`).
//! - **Duplicates**: the repeated id is reassigned with `resolverId` from
//!   `980` and `name → id` keeps the **first** occurrence (`:999-1016`).
//! - **`region`**: 1-based in the file, 0-based in memory except `-1` (`:1081`).
//! - **`flat`**: a single value is copied to male and female; `flat: -1`
//!   parses to `0` and is ignored (`:1186-1205`).
//! - **`rulerNameSeed` / `rulerPowerBonus`**: they do not come from the file;
//!   they are drawn with `DFRandom` in file order (`:1019-1021`).
//! - **Order**: insertion order is preserved (the .NET `Dictionary`
//!   enumerates by insertion), because `FindFactionByTypeAndRegion` returns
//!   *the last* fallback match and `RelinkChildren` orders by it.
//!
//! Pure: 0 Bevy, 0 I/O, 0 global RNG (the seed enters as a parameter).

use std::collections::BTreeMap;

use crate::determinism::{fnv1a64, DfRandom};

/// Canonical file name declared by the source (`FactionFile.cs:684-687`).
pub const FACTION_FILENAME: &str = "FACTION.TXT";

/// First reassignment id for duplicate ids (`FactionFile.cs:921`).
pub const RESOLVER_ID_START: i32 = 980;

/// Reputation clamp (`PersistentFactionData.cs:32-33`).
pub const MIN_REPUTATION: i32 = -100;
/// Reputation clamp (`PersistentFactionData.cs:32-33`).
pub const MAX_REPUTATION: i32 = 100;

/// Power clamp (`PersistentFactionData.cs:34-35`, `:509-514`).
pub const MIN_POWER: i32 = 1;
/// Power clamp (`PersistentFactionData.cs:34-35`, `:509-514`).
pub const MAX_POWER: i32 = 100;

/// Shops are marked as a guild in the data: they are forced to `None`
/// (`GuildManager.cs:266-268`).
pub const SHOP_FACTION_ID: i32 = 510;
/// `Generic_Temple` (`FactionFile.cs:321`).
pub const GENERIC_TEMPLE_ID: i32 = 450;
/// `Generic_Knightly_Order` (`FactionFile.cs:515`).
pub const GENERIC_KNIGHTLY_ORDER_ID: i32 = 844;
/// `The_Dark_Brotherhood` is treated as a root (`:422-424`).
pub const DARK_BROTHERHOOD_ID: i32 = 108;
/// `GuildGroups.KnightlyOrder` (`FactionFile.cs:579`).
pub const KNIGHTLY_ORDER_GGROUP: i32 = 9;
/// `FactionTypes.God` (`FactionFile.cs:535`).
pub const GOD_TYPE: i32 = 1;
/// `FactionTypes.Province` (`FactionFile.cs:541`).
pub const PROVINCE_TYPE: i32 = 7;

/// Quest `GuildNpcServices` (`PersistentFactionData.cs:37-38`): MG/FG/TG/
/// DB/T/KO `_Quests` (`Game/Guilds/Services.cs:62,72,76,81,93,137`).
pub const QUESTOR_IDS: [i32; 6] = [63, 851, 804, 807, 240, 846];

/// `Flags.RulerImmune` (`FactionFile.cs:623`).
pub const FLAG_RULER_IMMUNE: i32 = 0x10;
/// `Flags.Summoned` (`FactionFile.cs:624`).
pub const FLAG_SUMMONED: i32 = 0x40;

/// `rulerPowerBonus = random_range_inclusive(0,50) + 20` (`:1021`).
pub const RULER_POWER_BONUS_MIN: i32 = 20;
/// Maximum `rulerPowerBonus` (`:1021`).
pub const RULER_POWER_BONUS_MAX: i32 = 70;

/// `FactionTypes` (`FactionFile.cs:531-550`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactionType {
    /// 0.
    Daedra,
    /// 1.
    God,
    /// 2.
    Group,
    /// 3.
    Subgroup,
    /// 4.
    Individual,
    /// 5.
    Official,
    /// 6.
    VampireClan,
    /// 7.
    Province,
    /// 8.
    WitchesCoven,
    /// 9.
    Temple,
    /// 10.
    KnightlyGuard,
    /// 11.
    MagicUser,
    /// 12.
    Generic,
    /// 13.
    Thieves,
    /// 14.
    Courts,
    /// 15.
    People,
}

impl FactionType {
    /// Maps the file integer to the enum (`None` = `-1` or out of range).
    pub fn from_id(id: i32) -> Option<Self> {
        Some(match id {
            0 => FactionType::Daedra,
            1 => FactionType::God,
            2 => FactionType::Group,
            3 => FactionType::Subgroup,
            4 => FactionType::Individual,
            5 => FactionType::Official,
            6 => FactionType::VampireClan,
            7 => FactionType::Province,
            8 => FactionType::WitchesCoven,
            9 => FactionType::Temple,
            10 => FactionType::KnightlyGuard,
            11 => FactionType::MagicUser,
            12 => FactionType::Generic,
            13 => FactionType::Thieves,
            14 => FactionType::Courts,
            15 => FactionType::People,
            _ => return None,
        })
    }
}

/// One `factionDict` entry (`FactionFile.cs:634-669`). The 3 inherited
/// pointers are not exposed: the engine uses `children` (relink).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactionData {
    /// `#id` of the block (or the reassigned id if duplicated).
    pub id: i32,
    /// Parent id from leading tabs; `0` = root (struct default).
    pub parent: i32,
    /// Raw `type` integer (`FactionType::from_id` for the enum).
    pub faction_type: i32,
    /// Literal `name` from the file (`GetLocalizedFactionName` is M16).
    pub name: String,
    /// Initial `rep`.
    pub rep: i32,
    /// `summon`.
    pub summon: i32,
    /// `region` already converted to 0-based.
    pub region: i32,
    /// `power`.
    pub power: i32,
    /// Accumulated `flags` (`|=`).
    pub flags: i32,
    /// Ruling faction.
    pub ruler: i32,
    /// `ally1..3`.
    pub ally: [i32; 3],
    /// `enemy1..3`.
    pub enemy: [i32; 3],
    /// `face`.
    pub face: i32,
    /// `race`.
    pub race: i32,
    /// `flat1` (male).
    pub flat1: i32,
    /// `flat2` (female).
    pub flat2: i32,
    /// `sgroup`.
    pub sgroup: i32,
    /// `ggroup`.
    pub ggroup: i32,
    /// `minf`.
    pub minf: i32,
    /// `maxf`.
    pub maxf: i32,
    /// `vam`.
    pub vam: i32,
    /// `rank`.
    pub rank: i32,
    /// Ruler name seed (`DFRandom`, does not come from the file).
    pub ruler_name_seed: u32,
    /// Ruler power bonus, `[20, 70]`.
    pub ruler_power_bonus: i32,
    /// Children relinked in insertion order (`RelinkChildren`).
    pub children: Vec<i32>,
}

impl Default for FactionData {
    /// C# struct default: everything `0` except `name` (`null` → empty).
    fn default() -> Self {
        Self {
            id: 0,
            parent: 0,
            faction_type: 0,
            name: String::new(),
            rep: 0,
            summon: 0,
            region: 0,
            power: 0,
            flags: 0,
            ruler: 0,
            ally: [0; 3],
            enemy: [0; 3],
            face: 0,
            race: 0,
            flat1: 0,
            flat2: 0,
            sgroup: 0,
            ggroup: 0,
            minf: 0,
            maxf: 0,
            vam: 0,
            rank: 0,
            ruler_name_seed: 0,
            ruler_power_bonus: 0,
            children: Vec::new(),
        }
    }
}

impl FactionData {
    /// `IsAlly(this, other)` (`FactionFile.cs:895-899`).
    pub fn is_ally(&self, other_id: i32) -> bool {
        self.ally.contains(&other_id)
    }

    /// `IsEnemy(this, other)` (`FactionFile.cs:907-911`).
    pub fn is_enemy(&self, other_id: i32) -> bool {
        self.enemy.contains(&other_id)
    }

    /// Does it have the flag? (`PersistentFactionData.cs:488-496`).
    pub fn has_flag(&self, flag: i32) -> bool {
        self.flags & flag > 0
    }
}

/// Parse error. DFU throws an exception (R-27-4): the target fails the whole
/// file and never silently ignores tags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FactionError {
    /// `tag:value` without exactly 2 parts and not splittable by space either.
    InvalidTag {
        /// Raw line of the file.
        line: String,
    },
    /// Unrecognized tag (`FactionFile.cs:1166-1167`).
    UnknownTag {
        /// Normalized tag.
        tag: String,
        /// Raw value.
        value: String,
        /// Faction being parsed.
        faction: i32,
    },
    /// Not an integer.
    InvalidNumber {
        /// Raw value.
        value: String,
    },
    /// `flat` with more than 2 components (`:1195-1205`).
    InvalidFlat {
        /// Raw value.
        value: String,
    },
    /// Fourth `ally`/`enemy` (`:1120-1143`).
    TooManyLinks {
        /// Tag repeated too many times.
        tag: &'static str,
        /// Faction being parsed.
        faction: i32,
    },
    /// Third `flat` (`:1155-1163`).
    TooManyFlats {
        /// Faction being parsed.
        faction: i32,
    },
}

/// Region faction error (`PersistentFactionData.cs:272-287`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionFactionError {
    /// `0` `Province` factions for the region.
    Missing,
    /// More than one (the DFU 1:1 breaks).
    Duplicate {
        /// How many were found.
        found: usize,
    },
}

/// Complete graph: id → faction dictionary + insertion order + names.
#[derive(Debug, Clone, Default)]
pub struct FactionGraph {
    factions: BTreeMap<i32, FactionData>,
    name_to_id: BTreeMap<String, i32>,
    order: Vec<i32>,
    duplicate_ids: usize,
}

impl FactionGraph {
    /// Parses `FACTION.TXT` (full text) with the `DFRandom` seed.
    ///
    /// `seed` is exactly `DFRandom.srand(seed)` from `FactionFile.cs:1019`;
    /// the two `rand()` calls per faction set `rulerNameSeed` and
    /// `rulerPowerBonus` in file order.
    pub fn parse(text: &str, seed: u32) -> Result<Self, FactionError> {
        let mut rng = DfRandom::srand(seed);
        let mut blocks: Vec<Vec<&str>> = Vec::new();
        let mut current: Vec<&str> = Vec::new();
        for line in text.lines() {
            if line.starts_with(';') || line.is_empty() {
                continue;
            }
            if line.contains('#') && !current.is_empty() {
                blocks.push(std::mem::take(&mut current));
            }
            current.push(line);
        }
        if !current.is_empty() {
            blocks.push(current);
        }

        let mut graph = FactionGraph {
            factions: BTreeMap::new(),
            name_to_id: BTreeMap::new(),
            order: Vec::with_capacity(blocks.len()),
            duplicate_ids: 0,
        };
        let mut resolver_id = RESOLVER_ID_START;
        let mut last_tabs = 0usize;
        let mut parent_stack: Vec<i32> = Vec::new();
        let mut previous_id = 0i32;

        for block in &blocks {
            let tabs = count_preceding_tabs(block[0]);
            if tabs > last_tabs {
                parent_stack.push(previous_id);
            } else if tabs < last_tabs {
                while parent_stack.len() > tabs {
                    parent_stack.pop();
                }
            }
            last_tabs = tabs;

            let mut faction = FactionData::default();
            if let Some(parent) = parent_stack.last() {
                faction.parent = *parent;
            }
            parse_faction_data(block, &mut faction)?;

            if graph.factions.contains_key(&faction.id) {
                faction.id = resolver_id;
                resolver_id += 1;
                graph.duplicate_ids += 1;
            }
            graph
                .name_to_id
                .entry(faction.name.clone())
                .or_insert(faction.id);

            // Same order as DFU: the high part first, then the low part of the `or`.
            let high = rng.rand() << 16;
            faction.ruler_name_seed = rng.rand() | high;
            faction.ruler_power_bonus = ruler_power_bonus_from_draw(rng.rand());

            previous_id = faction.id;
            graph.order.push(faction.id);
            graph.factions.insert(faction.id, faction);
        }

        graph.relink_children();
        Ok(graph)
    }

    /// `RelinkChildren` (`FactionFile.cs:873-887`): adds each faction to its
    /// parent's `children` list, in insertion order.
    pub fn relink_children(&mut self) {
        for node in self.factions.values_mut() {
            node.children.clear();
        }
        let pairs: Vec<(i32, i32)> = self
            .order
            .iter()
            .filter_map(|id| {
                let faction = self.factions.get(id)?;
                if self.factions.contains_key(&faction.parent) {
                    Some((faction.parent, *id))
                } else {
                    None
                }
            })
            .collect();
        for (parent, child) in pairs {
            if let Some(node) = self.factions.get_mut(&parent) {
                node.children.push(child);
            }
        }
    }

    /// Number of factions (includes reassigned ids).
    pub fn len(&self) -> usize {
        self.factions.len()
    }

    /// Is the graph empty?
    pub fn is_empty(&self) -> bool {
        self.factions.is_empty()
    }

    /// How many duplicate ids were reassigned.
    pub fn duplicate_ids(&self) -> usize {
        self.duplicate_ids
    }

    /// Ids in insertion order (file order).
    pub fn ids(&self) -> &[i32] {
        &self.order
    }

    /// Faction by id.
    pub fn get(&self, id: i32) -> Option<&FactionData> {
        self.factions.get(&id)
    }

    /// Mutable faction by id.
    pub fn get_mut(&mut self, id: i32) -> Option<&mut FactionData> {
        self.factions.get_mut(&id)
    }

    /// Id by name (first occurrence; `-1` if absent).
    pub fn id_by_name(&self, name: &str) -> i32 {
        self.name_to_id.get(name).copied().unwrap_or(-1)
    }

    /// `GetReputation`: `0` if the id does not exist (`PersistentFactionData.cs:361`).
    pub fn reputation(&self, id: i32) -> i32 {
        self.factions.get(&id).map_or(0, |f| f.rep)
    }

    /// `SetReputation` with clamp (`:370-381`).
    pub fn set_reputation(&mut self, id: i32, value: i32) -> bool {
        match self.factions.get_mut(&id) {
            Some(faction) => {
                faction.rep = value.clamp(MIN_REPUTATION, MAX_REPUTATION);
                true
            }
            None => false,
        }
    }

    /// `ChangePower` with clamp `[1,100]` (`:509-514`).
    pub fn change_power(&mut self, id: i32, amount: i32) -> bool {
        match self.factions.get_mut(&id) {
            Some(faction) => {
                faction.power = (faction.power + amount).clamp(MIN_POWER, MAX_POWER);
                true
            }
            None => false,
        }
    }

    /// `GetFlag` (`:488-496`).
    pub fn has_flag(&self, id: i32, flag: i32) -> bool {
        self.factions.get(&id).is_some_and(|f| f.has_flag(flag))
    }

    /// `FindFactions` with optional filters (`:236-264`) in file order.
    pub fn find_factions(
        &self,
        faction_type: Option<i32>,
        sgroup: Option<i32>,
        ggroup: Option<i32>,
        region: Option<i32>,
    ) -> Vec<i32> {
        self.order
            .iter()
            .copied()
            .filter(|id| {
                let Some(f) = self.factions.get(id) else {
                    return false;
                };
                faction_type.is_none_or(|v| v == f.faction_type)
                    && sgroup.is_none_or(|v| v == f.sgroup)
                    && ggroup.is_none_or(|v| v == f.ggroup)
                    && region.is_none_or(|v| v == f.region)
            })
            .collect()
    }

    /// `FindFactionByTypeAndRegion` (`:285-320`): exact match or the
    /// **last** one with `region == -1`.
    pub fn find_faction_by_type_and_region(&self, faction_type: i32, region: i32) -> Option<i32> {
        let mut partial = None;
        for id in &self.order {
            let Some(f) = self.factions.get(id) else {
                continue;
            };
            if f.faction_type == faction_type && f.region == region {
                return Some(*id);
            }
            if f.faction_type == faction_type && f.region == -1 {
                partial = Some(*id);
            }
        }
        partial
    }

    /// `GetRegionFaction` (`:272-287`): exactly one `Province`.
    pub fn region_faction(&self, region: i32) -> Result<i32, RegionFactionError> {
        let matches = self.find_factions(Some(PROVINCE_TYPE), None, None, Some(region));
        match matches.len() {
            0 => Err(RegionFactionError::Missing),
            1 => Ok(matches[0]),
            found => Err(RegionFactionError::Duplicate { found }),
        }
    }

    /// `GetGuildGroup` (`GuildManager.cs:266-293`): `-1` = `None`.
    pub fn guild_group(&self, faction_id: i32) -> i32 {
        if faction_id == SHOP_FACTION_ID {
            return -1;
        }
        let Some(faction) = self.factions.get(&faction_id) else {
            return -1;
        };
        let mut group = faction.ggroup;
        if faction.faction_type == GOD_TYPE && group == -1 {
            if let Some(child) = faction.children.first() {
                if let Some(first_child) = self.factions.get(child) {
                    group = first_child.ggroup;
                }
            }
        }
        group
    }

    /// `ChangeReputation` (`PersistentFactionData.cs:390-458`).
    ///
    /// With `propagate = false` it only moves the faction (clamp). With
    /// `propagate = true`: first allies (`amount/2`) and enemies
    /// (`-amount/2`); then the generic knightly order or the root's tree
    /// (half except initiator/root/questor); and finally, if the root is a
    /// deity, the generic temple.
    pub fn change_reputation(&mut self, faction_id: i32, amount: i32, propagate: bool) -> bool {
        let Some(faction) = self.factions.get(&faction_id).cloned() else {
            return false;
        };
        if !propagate {
            if let Some(node) = self.factions.get_mut(&faction_id) {
                node.rep = (node.rep + amount).clamp(MIN_REPUTATION, MAX_REPUTATION);
            }
            return true;
        }

        for i in 0..3 {
            self.change_reputation(faction.ally[i], amount / 2, false);
            self.change_reputation(faction.enemy[i], -amount / 2, false);
        }

        if faction.ggroup == KNIGHTLY_ORDER_GGROUP {
            self.change_reputation(faction_id, amount, false);
            self.change_reputation(GENERIC_KNIGHTLY_ORDER_ID, amount, true);
            return true;
        }

        let mut node = faction;
        while self.factions.contains_key(&node.parent) && node.id != DARK_BROTHERHOOD_ID {
            match self.factions.get(&node.parent) {
                Some(parent) => node = parent.clone(),
                None => break,
            }
        }

        if node.children.is_empty() {
            self.change_reputation(faction_id, amount, false);
        } else {
            self.propagate_reputation_change(&node, faction_id, amount);
        }

        if node.faction_type == GOD_TYPE {
            self.change_reputation(GENERIC_TEMPLE_ID, amount, true);
        }
        true
    }

    /// `PropagateReputationChange` (`:448-458`).
    pub fn propagate_reputation_change(
        &mut self,
        node: &FactionData,
        initiator_id: i32,
        amount: i32,
    ) {
        let full =
            node.id == initiator_id || node.parent == 0 || QUESTOR_IDS.contains(&node.id);
        self.change_reputation(node.id, if full { amount } else { amount / 2 }, false);
        for child in &node.children {
            if let Some(child_node) = self.factions.get(child).cloned() {
                self.propagate_reputation_change(&child_node, initiator_id, amount);
            }
        }
    }

    /// Quest verb `change repute with <person> by ±N`
    /// (`ChangeReputeWith.cs:27,73`): always propagates.
    pub fn change_repute_with(&mut self, faction_id: i32, amount: i32) -> bool {
        self.change_reputation(faction_id, amount, true)
    }

    /// FNV-1a 64 digest of the graph in file order (includes `children`).
    pub fn digest(&self) -> u64 {
        let mut bytes: Vec<u8> = Vec::with_capacity(self.order.len() * 96);
        for id in &self.order {
            let Some(f) = self.factions.get(id) else {
                continue;
            };
            for value in [
                f.id,
                f.parent,
                f.faction_type,
                f.rep,
                f.summon,
                f.region,
                f.power,
                f.flags,
                f.ruler,
                f.face,
                f.race,
                f.flat1,
                f.flat2,
                f.sgroup,
                f.ggroup,
                f.minf,
                f.maxf,
                f.vam,
                f.rank,
                f.ruler_power_bonus,
            ] {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            bytes.extend_from_slice(&f.ruler_name_seed.to_le_bytes());
            for value in f.ally.iter().chain(f.enemy.iter()) {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            bytes.extend_from_slice(&(f.children.len() as i32).to_le_bytes());
            for child in &f.children {
                bytes.extend_from_slice(&child.to_le_bytes());
            }
            bytes.extend_from_slice(&(f.name.len() as i32).to_le_bytes());
            bytes.extend_from_slice(f.name.as_bytes());
        }
        fnv1a64(&bytes)
    }
}

/// `random_range_inclusive(0, 50) + 20` over an already-made roll
/// (`FactionFile.cs:1021`, `DFRandom.cs:75-78`): `draw % 51 + 20`.
pub fn ruler_power_bonus_from_draw(draw: u32) -> i32 {
    (draw % 51) as i32 + RULER_POWER_BONUS_MIN
}

/// Counts leading tabs (`FactionFile.cs:1172-1184`).
fn count_preceding_tabs(line: &str) -> usize {
    line.chars().take_while(|c| *c == '\t').count()
}

fn parse_int(value: &str) -> Result<i32, FactionError> {
    value.parse::<i32>().map_err(|_| FactionError::InvalidNumber {
        value: value.to_string(),
    })
}

/// `ParseFlat` (`FactionFile.cs:1186-1205`): a single component → `0`.
fn parse_flat(value: &str) -> Result<i32, FactionError> {
    let parts: Vec<&str> = value.split_whitespace().collect();
    match parts.len() {
        1 => Ok(0),
        2 => Ok((parse_int(parts[0])? << 7) + parse_int(parts[1])?),
        _ => Err(FactionError::InvalidFlat {
            value: value.to_string(),
        }),
    }
}

/// `ParseFactionData` (`FactionFile.cs:1027-1210`).
fn parse_faction_data(block: &[&str], faction: &mut FactionData) -> Result<(), FactionError> {
    let mut allies = 0usize;
    let mut enemies = 0usize;
    let mut flats = 0usize;

    for line in block {
        if line.contains('#') {
            let mut parts = line.split('#');
            parts.next();
            let id = parts.next().ok_or_else(|| FactionError::InvalidTag {
                line: (*line).to_string(),
            })?;
            faction.id = parse_int(id.trim())?;
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split(':').collect();
        let (tag, value) = if parts.len() == 2 {
            (parts[0], parts[1])
        } else {
            let spaced: Vec<&str> = line.split(' ').collect();
            if spaced.len() == 2 {
                (spaced[0], spaced[1])
            } else {
                return Err(FactionError::InvalidTag {
                    line: (*line).to_string(),
                });
            }
        };
        let tag = tag.trim().to_lowercase();
        let value = value.trim();

        match tag.as_str() {
            "name" => faction.name = value.to_string(),
            "rep" => faction.rep = parse_int(value)?,
            "summon" => faction.summon = parse_int(value)?,
            "region" => {
                faction.region = parse_int(value)?;
                if faction.region != -1 {
                    faction.region -= 1;
                }
            }
            "type" => faction.faction_type = parse_int(value)?,
            "power" => faction.power = parse_int(value)?,
            "flags" => faction.flags |= parse_int(value)?,
            "ruler" => faction.ruler = parse_int(value)?,
            "face" => faction.face = parse_int(value)?,
            "race" => faction.race = parse_int(value)?,
            "sgroup" => faction.sgroup = parse_int(value)?,
            "ggroup" => faction.ggroup = parse_int(value)?,
            "minf" => faction.minf = parse_int(value)?,
            "maxf" => faction.maxf = parse_int(value)?,
            "vam" => faction.vam = parse_int(value)?,
            "rank" => faction.rank = parse_int(value)?,
            "ally" => {
                if allies == 3 {
                    return Err(FactionError::TooManyLinks {
                        tag: "ally",
                        faction: faction.id,
                    });
                }
                faction.ally[allies] = parse_int(value)?;
                allies += 1;
            }
            "enemy" => {
                if enemies == 3 {
                    return Err(FactionError::TooManyLinks {
                        tag: "enemy",
                        faction: faction.id,
                    });
                }
                faction.enemy[enemies] = parse_int(value)?;
                enemies += 1;
            }
            "flat" => {
                let flat = parse_flat(value)?;
                if flat > 0 {
                    match flats {
                        0 => {
                            faction.flat1 = flat;
                            faction.flat2 = flat;
                        }
                        1 => faction.flat2 = flat,
                        _ => {
                            return Err(FactionError::TooManyFlats { faction: faction.id });
                        }
                    }
                    flats += 1;
                }
            }
            _ => {
                return Err(FactionError::UnknownTag {
                    tag,
                    value: value.to_string(),
                    faction: faction.id,
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// §4.3 tree: root `10 (parent=0)`, children `A=11` (initiator) and
    /// `B=12`; `A.ally1=21`, `A.enemy1=20`.
    const PROP_TREE: &str = "#10\ntype: 2\nname: Root\n\t#11\ntype: 2\nname: A\nally: 21\nenemy: 20\n\t#12\ntype: 2\nname: B\n#21\ntype: 2\nname: L\n#20\ntype: 2\nname: E\n";

    fn parse(text: &str) -> FactionGraph {
        FactionGraph::parse(text, 0x1234).expect("valid fixture")
    }

    #[test]
    fn duplicate_id_gets_resolver_id_and_name_keeps_first() {
        let graph = parse(
            "#77\ntype: 2\nname: Original\n\t#77\ntype: 2\nname: Second\n#78\ntype: 2\nname: Other\n",
        );
        assert_eq!(graph.len(), 3);
        assert_eq!(graph.duplicate_ids(), 1);
        assert_eq!(graph.get(77).map(|f| f.name.as_str()), Some("Original"));
        assert_eq!(graph.id_by_name("Original"), 77);
        assert_eq!(graph.id_by_name("Second"), 980);
        assert_eq!(graph.get(980).map(|f| f.name.as_str()), Some("Second"));
    }

    #[test]
    fn region_is_converted_to_zero_based_except_minus_one() {
        let graph = parse("#1\ntype: 7\nregion: 18\n#2\ntype: 7\nregion: -1\n");
        assert_eq!(graph.get(1).unwrap().region, 17);
        assert_eq!(graph.get(2).unwrap().region, -1);
    }

    #[test]
    fn single_flat_copies_to_male_and_female() {
        let graph = parse("#1\nflat: 175 2\nflat: 182 23\n");
        let faction = graph.get(1).unwrap();
        assert_eq!((faction.flat1 >> 7, faction.flat1 & 0x7f), (175, 2));
        assert_eq!((faction.flat2 >> 7, faction.flat2 & 0x7f), (182, 23));
        let single = parse("#2\nflat: 175 2\n");
        assert_eq!(single.get(2).unwrap().flat1, single.get(2).unwrap().flat2);
    }

    #[test]
    fn flat_minus_one_is_parsed_as_zero_and_ignored() {
        let graph = parse("#1\nflat: -1\n");
        assert_eq!(graph.get(1).unwrap().flat1, 0);
    }

    #[test]
    fn flags_accumulate_and_expose_bits() {
        let graph = parse("#1\nflags: 1\nflags: 16\nflags: 64\n");
        let faction = graph.get(1).unwrap();
        assert_eq!(faction.flags, 0x51);
        assert!(faction.has_flag(FLAG_RULER_IMMUNE));
        assert!(faction.has_flag(FLAG_SUMMONED));
        assert!(graph.has_flag(1, FLAG_SUMMONED));
        assert!(!graph.has_flag(1, 0x8000));
    }

    #[test]
    fn unknown_tag_fails_the_whole_file() {
        let error = FactionGraph::parse("#1\nname: X\nbogus: 1\n", 0).unwrap_err();
        assert!(matches!(error, FactionError::UnknownTag { tag, .. } if tag == "bogus"));
    }

    #[test]
    fn malformed_tag_falls_back_to_space_and_uppercase_tag_normalizes() {
        let graph = parse("#1\nname Value\nType: 2\n");
        let faction = graph.get(1).unwrap();
        assert_eq!(faction.name, "Value");
        assert_eq!(faction.faction_type, 2);
    }

    #[test]
    fn invalid_tag_with_three_parts_is_rejected() {
        let error = FactionGraph::parse("#1\nflat 175 2\n", 0).unwrap_err();
        assert!(matches!(error, FactionError::InvalidTag { .. }));
    }

    #[test]
    fn parent_comes_from_leading_tabs() {
        let graph = parse("#1\nname: root\n\t#2\nname: child\n\t\t#3\nname: grandchild\n#4\nname: sibling_of_root\n");
        assert_eq!(graph.get(2).unwrap().parent, 1);
        assert_eq!(graph.get(3).unwrap().parent, 2);
        assert_eq!(graph.get(4).unwrap().parent, 0);
        assert_eq!(graph.get(1).unwrap().children, vec![2]);
        assert_eq!(graph.get(3).unwrap().children, Vec::<i32>::new());
    }

    #[test]
    fn ruler_seed_and_bonus_are_deterministic_per_seed_and_in_range() {
        let text = "#1\nname: A\n#2\nname: B\n";
        let a = FactionGraph::parse(text, 0x1234).unwrap();
        let b = FactionGraph::parse(text, 0x1234).unwrap();
        let c = FactionGraph::parse(text, 7).unwrap();
        assert_eq!(a.digest(), b.digest());
        assert_ne!(a.digest(), c.digest());
        for section in [&a, &c] {
            for id in section.ids() {
                let bonus = section.get(*id).unwrap().ruler_power_bonus;
                assert!((RULER_POWER_BONUS_MIN..=RULER_POWER_BONUS_MAX).contains(&bonus));
            }
        }
    }

    #[test]
    fn ruler_bonus_goldens_gv_27_07_and_gv_27_08() {
        // `rand()=0` ⇒ `0 % 51 + 20 = 20`; `rand()=50` ⇒ `50 % 51 + 20 = 70`.
        assert_eq!(ruler_power_bonus_from_draw(0), 20);
        assert_eq!(ruler_power_bonus_from_draw(50), 70);
        assert_eq!(ruler_power_bonus_from_draw(51), 20);
    }

    #[test]
    fn propagation_golden_gv_27_36() {
        let mut graph = parse(PROP_TREE);
        assert!(graph.change_reputation(11, 10, true));
        assert_eq!(graph.reputation(10), 10);
        assert_eq!(graph.reputation(11), 10);
        assert_eq!(graph.reputation(12), 5);
        assert_eq!(graph.reputation(21), 5);
        assert_eq!(graph.reputation(20), -5);
    }

    #[test]
    fn propagation_golden_gv_27_37_truncates_toward_zero() {
        let mut graph = parse(PROP_TREE);
        graph.change_reputation(11, 5, true);
        assert_eq!(graph.reputation(10), 5);
        assert_eq!(graph.reputation(11), 5);
        assert_eq!(graph.reputation(12), 2);
        assert_eq!(graph.reputation(21), 2);
        assert_eq!(graph.reputation(20), -2);
    }

    #[test]
    fn propagation_golden_gv_27_38_is_sign_symmetric() {
        let mut graph = parse(PROP_TREE);
        graph.change_reputation(11, -10, true);
        assert_eq!(graph.reputation(10), -10);
        assert_eq!(graph.reputation(11), -10);
        assert_eq!(graph.reputation(12), -5);
        assert_eq!(graph.reputation(21), -5);
        assert_eq!(graph.reputation(20), 5);
    }

    #[test]
    fn no_propagation_moves_only_the_faction() {
        let mut graph = parse(PROP_TREE);
        graph.change_reputation(11, 10, false);
        assert_eq!(graph.reputation(11), 10);
        assert_eq!(graph.reputation(10), 0);
        assert_eq!(graph.reputation(12), 0);
        assert_eq!(graph.reputation(21), 0);
        assert_eq!(graph.reputation(20), 0);
    }

    #[test]
    fn reputation_clamps_at_both_ends() {
        let mut graph = parse(PROP_TREE);
        graph.set_reputation(11, 95);
        graph.change_reputation(11, 10, false);
        assert_eq!(graph.reputation(11), MAX_REPUTATION);
        graph.set_reputation(11, -95);
        graph.change_reputation(11, -10, false);
        assert_eq!(graph.reputation(11), MIN_REPUTATION);
    }

    #[test]
    fn unknown_faction_returns_false_and_changes_nothing() {
        let mut graph = parse(PROP_TREE);
        let before = graph.digest();
        assert!(!graph.change_reputation(999, 10, true));
        assert!(!graph.change_reputation(999, 10, false));
        assert_eq!(graph.digest(), before);
        assert_eq!(graph.reputation(999), 0);
    }

    #[test]
    fn quest_verb_propagates_with_negative_amount() {
        let mut graph = parse(PROP_TREE);
        graph.set_reputation(20, 4);
        assert!(graph.change_repute_with(11, -5));
        assert_eq!(graph.reputation(11), -5);
        assert_eq!(graph.reputation(10), -5);
        assert_eq!(graph.reputation(12), -2);
        assert_eq!(graph.reputation(21), -2);
        assert_eq!(graph.reputation(20), 6);
    }

    #[test]
    fn shops_force_guild_group_none() {
        let graph = parse("#510\ntype: 2\nggroup: 11\n");
        assert_eq!(graph.guild_group(SHOP_FACTION_ID), -1);
        assert_eq!(graph.guild_group(510), -1);
    }

    #[test]
    fn god_without_group_takes_first_childs_group() {
        let graph = parse("#1\ntype: 1\nname: Deity\nggroup: -1\n\t#2\ntype: 9\nname: Temple\nggroup: 17\n");
        assert_eq!(graph.guild_group(1), 17);
        let empty = parse("#3\ntype: 1\nname: Lonely\nggroup: -1\n");
        assert_eq!(empty.guild_group(3), -1);
    }

    #[test]
    fn region_faction_requires_exactly_one_province() {
        let single = parse("#1\ntype: 7\nregion: 18\n");
        assert_eq!(single.region_faction(17), Ok(1));
        assert_eq!(single.region_faction(18), Err(RegionFactionError::Missing));
        let double = parse("#1\ntype: 7\nregion: 1\n#2\ntype: 7\nregion: 1\n");
        assert_eq!(
            double.region_faction(0),
            Err(RegionFactionError::Duplicate { found: 2 })
        );
    }

    #[test]
    fn type_and_region_falls_back_to_last_regionless_match() {
        let graph = parse("#1\ntype: 4\nregion: -1\nname: First\n#2\ntype: 7\nregion: 18\n#3\ntype: 4\nregion: -1\nname: Later\n");
        assert_eq!(graph.find_faction_by_type_and_region(7, 17), Some(2));
        assert_eq!(graph.find_faction_by_type_and_region(4, 5), Some(3));
        assert_eq!(graph.find_faction_by_type_and_region(5, 5), None);
    }

    #[test]
    fn digest_tracks_reputation_and_children() {
        let mut graph = parse(PROP_TREE);
        let base = graph.digest();
        assert_eq!(graph.digest(), base);
        graph.change_reputation(11, 1, false);
        assert_ne!(graph.digest(), base);
        let mut relinked = parse(PROP_TREE);
        relinked.get_mut(10).unwrap().children.clear();
        assert_ne!(relinked.digest(), base);
    }

    #[test]
    fn ally_and_enemy_helpers_match_the_three_slots() {
        let graph = parse("#1\nally: 2\nally: 3\nally: 4\nenemy: 5\n");
        let faction = graph.get(1).unwrap();
        assert!(faction.is_ally(2) && faction.is_ally(4) && !faction.is_ally(5));
        assert!(faction.is_enemy(5) && !faction.is_enemy(2));
    }

    #[test]
    fn power_clamps_in_range() {
        let mut graph = parse("#1\npower: 100\n");
        graph.change_power(1, 50);
        assert_eq!(graph.get(1).unwrap().power, MAX_POWER);
        graph.change_power(1, -1000);
        assert_eq!(graph.get(1).unwrap().power, MIN_POWER);
        assert!(!graph.change_power(42, 1));
    }
}

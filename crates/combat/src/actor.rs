//! Combat actors (spec M22 §1/§2/§5) — what the engine consumes **already
//! composed** by the app: live stats/skills from M20, materials from
//! M21 and templates from M25 enter as plain fields (ORDER M22: 0 crate
//! cycle; `Combatant.strength_modifier` is composed by the app from
//! `daggerfall_skills::character::damage_modifier`).

/// DFU `EntityTypes` — decides the monster `+40` (`FormulaHelper.cs:1242-1245`)
/// and the D-M22-2 bio gate (biography only if the target is the player).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityKind {
    /// Player.
    Player,
    /// Class enemy (`EnemyClass`) — no `+40`.
    EnemyClass,
    /// `EnemyMonster` — `+40` to hit when it is the target.
    EnemyMonster,
}

impl EntityKind {
    /// Is this the player? (biography only subtracts if the target is, D-M22-2).
    pub fn is_player(self) -> bool {
        matches!(self, EntityKind::Player)
    }

    /// Is it `EnemyMonster`? (`FormulaHelper.cs:1242-1245`).
    pub fn is_monster(self) -> bool {
        matches!(self, EntityKind::EnemyMonster)
    }
}

/// Combatant consumed by [`crate::engine::resolve_attack`]. The fields are
/// exactly those the `behavior::combat` formulas read literally; the composer
/// (app/probe) fills `strength_modifier`, `armor_values`, `min_metal_to_hit`
/// and `skeletal_warrior` from M20/M21/M25 (spec boundaries §5.1-§5.3).
#[derive(Debug, Clone)]
pub struct Combatant {
    /// Actor id — feeds [`crate::rng::actor_seed`] `(session_seed, id)`.
    pub id: u32,
    /// Entity role (§1.2 adjustments).
    pub kind: EntityKind,
    /// Live luck (M20) — `(luckA − luckT)/10` (`FormulaHelper.cs:1185-1200`).
    pub luck: i32,
    /// Live agility (M20) — `(agiA − agiT)/10` (`:1185-1200`).
    pub agility: i32,
    /// `CriticalStrike` skill (M20) — `SuccessRoll(crit)` and `crit/10`
    /// (`:1216-1218`).
    pub critical_strike: i32,
    /// `Dodging` skill **of this actor when it is the target** (`/4`, `:1213`).
    pub dodging: i32,
    /// `HandToHand` skill — fist min/max (`CalculateHandToHandMin/MaxDamage`,
    /// `:434-453`).
    pub hand_to_hand_skill: i32,
    /// Enchantment `ChanceToHitModifier` (composed by M24; `:814`).
    pub chance_to_hit_modifier: i32,
    /// `BiographyAvoidHitMod` (M20) — only subtracted if THIS is the player
    /// target (delta D-M22-2, `:1224-1251`).
    pub biography_avoid_hit_mod: i32,
    /// Target `ArmorValues[7]`, negative (M21 armor, `:1149-1161`).
    pub armor_values: [i32; 7],
    /// `IncreasedArmorValueModifier` (`:1149-1161`).
    pub inc_armor_mod: i32,
    /// `DecreasedArmorValueModifier` (`:1149-1161`).
    pub dec_armor_mod: i32,
    /// `Career.AdrenalineRush` (M20) — attacker `+5/+8`, target `−5/−8`
    /// (`:1163-1183`).
    pub adrenaline_rush: bool,
    /// `ImprovedAdrenalineRush` — the improved `+8/−8` (`:1163-1183`).
    pub improved_adrenaline_rush: bool,
    /// Current health (M20) — adrenaline condition and `set_health`.
    pub health: i32,
    /// Maximum health (M20) — `max/8` and `set_health` clamp.
    pub max_health: i32,
    /// `DamageModifier(LiveStrength)` **composed by the app since M20**
    /// (`floor((str − 50)/5)`, `daggerfall_skills::character::damage_modifier`)
    /// — ORDER M22: the engine does not depend on `skills` (0 crate cycle).
    pub strength_modifier: i32,
    /// `MinMetalToHit` if THIS is the target (M25 templates; scales
    /// `WeaponMaterialTypes`, spec §2.1 material gate).
    pub min_metal_to_hit: i32,
    /// Is it `SkeletalWarrior`? (M25) — enables bone halving + silver ×2
    /// (`FormulaHelper.cs:743`, `:748-749`).
    pub skeletal_warrior: bool,
}

impl Combatant {
    /// Combatant with the spec §1.4 canonical actor defaults (luck/agi
    /// 50, crit 20, health 100/100, armor 0, no mods). The remaining fields
    /// are adjusted via struct-literal or direct assignment.
    pub fn new(id: u32, kind: EntityKind) -> Self {
        Self {
            id,
            kind,
            luck: 50,
            agility: 50,
            critical_strike: 20,
            dodging: 0,
            hand_to_hand_skill: 30,
            chance_to_hit_modifier: 0,
            biography_avoid_hit_mod: 0,
            armor_values: [0; 7],
            inc_armor_mod: 0,
            dec_armor_mod: 0,
            adrenaline_rush: false,
            improved_adrenaline_rush: false,
            health: 100,
            max_health: 100,
            strength_modifier: 0,
            min_metal_to_hit: 0,
            skeletal_warrior: false,
        }
    }

    /// Adrenaline bonus of THIS actor (`behavior::combat::adrenaline_bonus`).
    pub fn adrenaline_bonus(&self) -> i32 {
        daggerfall_behavior::combat::adrenaline_bonus(
            self.adrenaline_rush,
            self.improved_adrenaline_rush,
            self.health,
            self.max_health,
        )
    }
}

/// Weapon as seen by the engine — composed by the app from M21 (`Item` +
/// material tables §2.2; R-22.7: base damage ranges live in `FormulaHelper`
/// and the composer supplies them).
#[derive(Debug, Clone)]
pub struct Weapon {
    /// `NativeMaterialValue` — `WeaponMaterialTypes` index (M21
    /// `Item.native_material`: 0 Iron, 1 Steel, 2 Silver … 9 Daedric).
    pub native_material_value: i32,
    /// Weapon minimum damage (`FormulaHelper` table, composed by the app).
    pub min_damage: i32,
    /// Weapon maximum damage (`FormulaHelper` table, composed by the app).
    pub max_damage: i32,
    /// Weapon flags: `0x10` = sharpened for the literal bone halving
    /// (`FormulaHelper.cs:743`).
    pub flags: u16,
    /// `GetWeaponMaterialModifier() * 10` to hit (M21 `TO_HIT_MOD`,
    /// `FormulaHelper.cs:1140-1146`) — the engine adds it at `:679`.
    pub to_hit_mod: i32,
    /// Material modifier to damage (M21 `weapon_material_modifier`,
    /// `DaggerfallUnityItem.cs:979-1005`).
    pub material_damage_mod: i32,
}

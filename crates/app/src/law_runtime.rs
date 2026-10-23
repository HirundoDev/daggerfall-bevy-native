//! M28 **production composition layer** — the live half of
//! `PlayerEntity.SpawnCityGuards` (`:621-745`).
//!
//! `daggerfall_behavior::law` decides *whether* guards are requested and *what*
//! the request is (`guard_spawn_request`, `law.rs:1076`); this module is the
//! caller production was missing (G6 of the post-23 re-verification): it owns
//! the player's law state, translates each of the four [`GuardSpawnRequest`]
//! variants into a [`GuardOrder`] the scene can execute, honours the countdown
//! of the `Countdown` variant, and composes the city-watch combatant that the
//! M22 runtime spawns.
//!
//! What this module deliberately does **not** do (declared frontiers, not
//! silent gaps):
//!
//! - **Guard entity/IA = M26.** No runtime NPC instantiation exists in this
//!   tree (the demo places its population once, at compose time), so the guard
//!   is spawned as an M22 [`Fighter`] (the entity contract M28 consumes:
//!   "M28 sólo pide el spawn y consume su ataque", spec §2.1) and its billboard
//!   stays M26's.
//! - **Kit = M25.** `AssignEnemyEquipment` (the class kit roll,
//!   `ItemHelper.cs:1379-1394`) is composed today inside the M25 probe
//!   (`m25_probe::encounter_fighter`); a city watch therefore spawns with its
//!   career health/level and the class armour policy, and an **empty** equip
//!   table. Its strike is the H2H branch, which is what an unarmed guard does.
//! ## Frontiers (declared, with proof — criterion 2 of ORDER R2)
//!
//! - **`Interior`**: M18 renders the exterior city only; no system in the demo
//!   puts the player inside a building, so `inside_building` is never `true` in
//!   the live scene. The translation exists and is unit-tested
//!   (`each_request_variant_translates_to_an_executable_order`); the producer
//!   arrives with the interior milestone (M11/M26).
//! - **`Countdown`**: DFU arms it from `SpawnCityGuards(false)`, the
//!   *non-immediate* path (`PlayerEntity.cs:734-742`), reached when a townsman
//!   — not a guard — saw the crime. The live scene's only crime source is the
//!   forced door, and `PlayerActivate.cs:571-579` calls
//!   `SpawnCityGuards(true)` (immediate), so the countdown branch has no
//!   producer yet. `LawRuntime::tick` is unit-tested
//!   (`the_countdown_expires_into_the_foe_spawner_branch`); the window system
//!   `m28_guard_arrival` that consumes it is read-verified, not executed by a
//!   test. The producer arrives with the crime witnesses of M26.
//! - **`npc_spec::spawn_city_guards` is not the live spawner.** Its positions
//!   come from the *synthetic* `CityLayout` (`gap_center`), not from the retail
//!   city the window renders, and its `GUARD_CRIME_THRESHOLD` binary rejects
//!   `AttemptedBreakingAndEntering` (value 1) — the very crime a forced door
//!   raises. The live scene places guards from its own retail doors/NPCs; the
//!   M15 spawner keeps its `spawncityguards` quest-verb contract.

use daggerfall_behavior::law::{
    guard_spawn_request, CrimeTrigger, DfRandom, GuardSpawnContext, GuardSpawnRequest, PlayerLaw,
    PrisonState,
};
use daggerfall_skills::career::EnemyClassTemplate;
use daggerfall_skills::character::Skill;
use daggerfall_skills::rng::Rng64;
use daggerfall_skills::humanoid::{city_watch_level, HumanoidProfile, CITY_WATCH_CLASS_ID};

use crate::combat_runtime::{EnemyArmorPolicy, Fighter};
use daggerfall_combat::{Combatant, DeathSequence, EntityKind};

/// ARENA2 career file of [`CITY_WATCH_CLASS_ID`]:
/// `CLASS{careerIndex:00}.CFG` (`DaggerfallEntity.GetClassCareerTemplate`,
/// `DaggerfallEntity.cs:907-915`) with `careerIndex = 18` ⇒ `CLASS18.CFG`, the
/// retail file that is **not** a playable class (it stores `major3 = 35`), so it
/// is read with [`parse_enemy_class_template`], never [`Career::parse`].
pub const CITY_WATCH_CLASS_FILE: &str = "CLASS18.CFG";

/// How the scene places the guards a [`GuardOrder`] asks for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GuardPlacement {
    /// Inside a shop/tavern/residence: at the lowest outer door, offset along
    /// its normal (`:630-641`). DFU's offset is `PlayerController.radius + 0.1f`
    /// (`:633-634`); the law carries only the `0.1` term
    /// ([`INTERIOR_DOOR_OFFSET_METERS`]), so the scene adds its own controller
    /// radius to place the guard.
    Door {
        /// The `+ 0.1f` term of `radius + 0.1f` (`:633-634`).
        offset_meters: f32,
    },
    /// Immediate spawn from the nearby NPC pool (`:644-688`): the guards take
    /// the positions of the NPCs they replace inside the attention radius.
    FromPool {
        /// `77.5` m (`:665`).
        radius_meters: f32,
    },
    /// `CreateFoeSpawner` fallback: the guards appear on a ring around the
    /// player, `Random.Range(2, 6)` and `12.8…51.2` m (`:685-688`).
    Ring {
        /// `12.8` m (`:687`).
        min_radius_meters: f32,
        /// `51.2` m (`:687`).
        max_radius_meters: f32,
    },
}

/// One executable guard order: the law's request, already translated.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GuardOrder {
    /// The request the law produced (kept verbatim: the scene reports it).
    pub request: GuardSpawnRequest,
    /// Guards to place (`Random.Range(2, 6)` / the replaced pool size).
    pub count: usize,
    /// Where they appear.
    pub placement: GuardPlacement,
    /// `guardsArriveCountdown` (`:737-742`): `None` = spawn now.
    pub after_seconds: Option<u32>,
}

/// Live law state of the scene: `PlayerLaw` + prison + the guard bookkeeping
/// the countdown and the cap need.
#[derive(Debug, Clone, PartialEq)]
pub struct LawRuntime {
    /// `PlayerEntity` law state (crime, regional legal records).
    pub law: PlayerLaw,
    /// `UpdatePrisonScreen`/`ReleaseFromPrison` state (`:464-490`).
    pub prison: PrisonState,
    /// `HowManyEnemiesOfType(Knight_CityWatch)` the scene currently holds.
    pub active_guards: usize,
    /// Seconds left of `guardsArriveCountdown`, when armed.
    pub countdown: Option<u32>,
    /// Orders emitted (census, never a substitute for the law state).
    pub dispatches: u32,
    /// Guards actually spawned by the scene.
    pub spawned: u32,
}

impl LawRuntime {
    /// A player with `regions` neutral legal records and no crime.
    pub fn new(regions: usize) -> Self {
        Self {
            law: PlayerLaw::new(regions),
            prison: PrisonState::begin(0),
            active_guards: 0,
            countdown: None,
            dispatches: 0,
            spawned: 0,
        }
    }

    /// A crime source fired (`spec §1.3`): stores the crime
    /// (`SetCrimeCommitted`, `:2346-2355`) and asks
    /// `PlayerEntity.SpawnCityGuards` (`:621-745`) what the scene must spawn.
    ///
    /// `None` = the law refused (dungeon, or more than
    /// `MAX_ACTIVE_GUARD_SPAWNS` active guards) — nobody saw the player and the
    /// spawn was not immediate is also `None`, exactly as the source.
    pub fn commit(
        &mut self,
        trigger: CrimeTrigger,
        mut context: GuardSpawnContext,
        rng: &mut DfRandom,
    ) -> Option<GuardOrder> {
        self.law.set_crime(trigger.crime(), false);
        // `HowManyEnemiesOfType(Knight_CityWatch)` is the runtime's own census:
        // a caller cannot raise or lower the cap by passing a stale count.
        context.active_guards = self.active_guards;
        let request = guard_spawn_request(context, rng)?;
        self.translate(request, context)
    }

    /// Turns the typed request into an executable order. Every number comes
    /// from the request or from the context, never from a fresh literal.
    fn translate(
        &mut self,
        request: GuardSpawnRequest,
        context: GuardSpawnContext,
    ) -> Option<GuardOrder> {
        let order = match request {
            GuardSpawnRequest::Interior {
                guards,
                door_offset_meters,
            } => GuardOrder {
                request,
                count: guards,
                placement: GuardPlacement::Door {
                    offset_meters: door_offset_meters,
                },
                after_seconds: None,
            },
            // `:668-680`: one guard per NPC the pool yields inside the radius.
            GuardSpawnRequest::ImmediateFromPool { radius_meters } => GuardOrder {
                request,
                count: context.npcs_in_radius,
                placement: GuardPlacement::FromPool { radius_meters },
                after_seconds: None,
            },
            GuardSpawnRequest::FoeSpawner {
                guards,
                min_radius_meters,
                max_radius_meters,
            } => GuardOrder {
                request,
                count: guards,
                placement: GuardPlacement::Ring {
                    min_radius_meters,
                    max_radius_meters,
                },
                after_seconds: None,
            },
            GuardSpawnRequest::Countdown { seconds } => {
                self.countdown = Some(seconds);
                GuardOrder {
                    request,
                    count: 0,
                    placement: GuardPlacement::Ring {
                        min_radius_meters: 0.0,
                        max_radius_meters: 0.0,
                    },
                    after_seconds: Some(seconds),
                }
            }
        };
        if order.after_seconds.is_none() && order.count == 0 {
            // `ImmediateFromPool` with an empty pool never reaches here (the
            // law answers `FoeSpawner`), but a 0-guard order is not a spawn.
            return None;
        }
        self.dispatches += 1;
        Some(order)
    }

    /// Advances `guardsArriveCountdown` (`:737-742`). On expiry the guards
    /// arrive through the source's own path: `CreateFoeSpawner(true,
    /// Knight_CityWatch, Random.Range(2, 6), 12.8, 51.2)` — modelled by asking
    /// [`guard_spawn_request`] for an immediate spawn with no NPC pool, which
    /// is exactly that branch.
    pub fn tick(&mut self, seconds: u32, rng: &mut DfRandom) -> Option<GuardOrder> {
        let left = self.countdown?;
        if seconds < left {
            self.countdown = Some(left - seconds);
            return None;
        }
        self.countdown = None;
        let context = GuardSpawnContext {
            active_guards: self.active_guards,
            immediate: true,
            npcs_in_radius: 0,
            ..GuardSpawnContext::default()
        };
        let request = guard_spawn_request(context, rng)?;
        self.translate(request, context)
    }

    /// Registers the guards the scene actually placed (count and positions are
    /// the caller's evidence). Returns the running total.
    pub fn record_spawn(&mut self, count: usize) -> usize {
        self.active_guards += count;
        self.spawned = self.spawned.saturating_add(count as u32);
        self.active_guards
    }

    /// Re-projects the census from the live scene (`HowManyEnemiesOfType`
    /// counts what is alive, `PlayerEntity.cs:623`): guards die and the cap
    /// must free the slots they held. Returns the new census.
    pub fn sync_census(&mut self, live_guards: usize) -> usize {
        self.active_guards = live_guards;
        self.active_guards
    }
}

/// Composes the `Knight_CityWatch` combatant: career profile (`Level` = the
/// player's, `+3..6` for the watch, `ItemHelper`/`city_watch_level`), the row's
/// `MinMetalToHit`, and the class armour policy. The kit is M25's (see the
/// module docs): the guard fights unarmed until `AssignEnemyEquipment` moves
/// out of the probe.
pub fn city_watch_fighter(
    id: u32,
    player_level: i32,
    template: &EnemyClassTemplate,
    rng: &mut Rng64,
) -> Result<Fighter, daggerfall_skills::humanoid::HumanoidProfileError> {
    let level = city_watch_level(player_level, rng);
    let profile = HumanoidProfile::from_parts(
        CITY_WATCH_CLASS_ID,
        template.name.clone(),
        template.attributes,
        template.hit_points_per_level,
        level,
        rng,
    )?;
    let row = daggerfall_behavior::mobile_enemies::mobile_enemy(146).ok();
    let mut combatant = Combatant::new(id, EntityKind::EnemyClass);
    combatant.max_health = profile.max_health;
    combatant.health = profile.max_health;
    combatant.min_metal_to_hit = row.map_or(0, |row| row.min_metal_to_hit.raw());
    // `CalculateSuccessfulHit` reads the attacker's live weapon skill; an
    // unarmed guard rolls H2H (`FormulaHelper.cs:589-592`), the same skill the
    // profile carries for the class.
    combatant.hand_to_hand_skill = i32::from(profile.skills[Skill::HandToHand as usize]);
    let mut fighter = Fighter::new(combatant, DeathSequence::for_enemy())
        .with_enemy_armor(EnemyArmorPolicy::Class);
    fighter.level = level;
    fighter.chance_mod = i32::from(profile.skills[Skill::HandToHand as usize]);
    Ok(fighter)
}

#[cfg(test)]
mod tests {
    use super::*;
    use daggerfall_behavior::law::{Crimes, INTERIOR_DOOR_OFFSET_METERS, MAX_ACTIVE_GUARD_SPAWNS};

    fn context() -> GuardSpawnContext {
        GuardSpawnContext::default()
    }

    #[test]
    fn each_request_variant_translates_to_an_executable_order() {
        let mut law = LawRuntime::new(1);
        let mut rng = DfRandom::srand(7);

        // Interior: the door offset and the drawn 2..=5 survive verbatim.
        let order = law
            .commit(
                CrimeTrigger::LockedDoorForce,
                GuardSpawnContext {
                    inside_building: true,
                    ..context()
                },
                &mut rng,
            )
            .expect("an interior crime asks for guards");
        assert_eq!(law.law.crime, Crimes::AttemptedBreakingAndEntering);
        assert_eq!(order.placement, GuardPlacement::Door { offset_meters: INTERIOR_DOOR_OFFSET_METERS });
        assert!((2..=5).contains(&order.count));
        assert_eq!(order.after_seconds, None);

        // Immediate exterior with a pool: one guard per nearby NPC.
        let order = law
            .commit(
                CrimeTrigger::Pickpocket,
                GuardSpawnContext {
                    immediate: true,
                    npcs_in_radius: 3,
                    ..context()
                },
                &mut rng,
            )
            .expect("immediate spawn with a pool");
        assert_eq!(order.count, 3);
        assert!(matches!(
            order.placement,
            GuardPlacement::FromPool { radius_meters } if radius_meters == 77.5
        ));

        // Immediate exterior with an empty pool: the foe-spawner fallback.
        let order = law
            .commit(
                CrimeTrigger::WeaponKill,
                GuardSpawnContext {
                    immediate: true,
                    npcs_in_radius: 0,
                    ..context()
                },
                &mut rng,
            )
            .expect("immediate spawn without a pool");
        assert!((2..=5).contains(&order.count));
        assert!(matches!(
            order.placement,
            GuardPlacement::Ring {
                min_radius_meters: 12.8,
                max_radius_meters: 51.2
            }
        ));

        // Seen by a guard, not immediate: immediate-from-pool, no countdown.
        let order = law
            .commit(
                CrimeTrigger::CriminalConspiracy,
                GuardSpawnContext {
                    seen_by_guard: true,
                    npcs_in_radius: 1,
                    ..context()
                },
                &mut rng,
            )
            .expect("a guard that saw the player summons");
        assert_eq!(order.count, 1);
        assert_eq!(order.after_seconds, None);

        // Seen by a townsman only: countdown, armed on the runtime.
        let order = law
            .commit(
                CrimeTrigger::OpenDoorForce,
                GuardSpawnContext {
                    seen_by_npc: true,
                    ..context()
                },
                &mut rng,
            )
            .expect("a witness arms the countdown");
        assert_eq!(order.count, 0);
        assert!((5..=10).contains(&order.after_seconds.expect("countdown")));
        assert_eq!(law.countdown, order.after_seconds);
    }

    #[test]
    fn the_law_refuses_in_a_dungeon_or_with_too_many_active_guards() {
        let mut law = LawRuntime::new(1);
        let mut rng = DfRandom::srand(11);
        assert!(law
            .commit(
                CrimeTrigger::WeaponKill,
                GuardSpawnContext {
                    inside_dungeon: true,
                    immediate: true,
                    ..context()
                },
                &mut rng
            )
            .is_none());
        // The census is the runtime's: 6 live guards refuse, and a stale
        // caller count cannot talk the law into spawning.
        law.record_spawn(MAX_ACTIVE_GUARD_SPAWNS + 1);
        assert!(law
            .commit(
                CrimeTrigger::WeaponKill,
                GuardSpawnContext {
                    active_guards: 0,
                    immediate: true,
                    ..context()
                },
                &mut rng
            )
            .is_none());
        // The crime is stored even when no guard is requested (`SetCrimeCommitted`
        // runs before `SpawnCityGuards`).
        assert_eq!(law.law.crime, Crimes::Murder);
    }

    #[test]
    fn the_countdown_expires_into_the_foe_spawner_branch() {
        let mut law = LawRuntime::new(1);
        let mut rng = DfRandom::srand(23);
        let order = law
            .commit(
                CrimeTrigger::LockedDoorForce,
                GuardSpawnContext {
                    seen_by_npc: true,
                    ..context()
                },
                &mut rng,
            )
            .expect("countdown armed");
        let seconds = order.after_seconds.expect("countdown");
        assert!(law.tick(seconds - 1, &mut rng).is_none(), "still counting");
        assert_eq!(law.countdown, Some(1));
        let arrival = law.tick(1, &mut rng).expect("guards arrive");
        assert_eq!(law.countdown, None);
        assert_eq!(arrival.after_seconds, None);
        assert!((2..=5).contains(&arrival.count), "Random.Range(2, 6)");
        assert!(matches!(arrival.placement, GuardPlacement::Ring { .. }));
        // The law state the countdown belonged to is untouched.
        assert_eq!(law.law.crime, Crimes::AttemptedBreakingAndEntering);
    }

    #[test]
    fn a_guard_is_composed_from_the_class_template_never_from_defaults() {
        // Synthetic template (0 retail bytes): the guard path only needs the
        // three `EnemyClassTemplate` fields (`EnemyEntity.cs:294,303`).
        let template = EnemyClassTemplate {
            name: "Knight_CityWatch".to_string(),
            attributes: [50, 50, 50, 50, 50, 50, 50, 50],
            hit_points_per_level: 12,
        };
        let mut rng = Rng64::new(9);
        let player_level = 4;
        let guard = city_watch_fighter(700, player_level, &template, &mut rng).expect("composed");
        // `Random.Range(3, 7)` above the player (`EnemyEntity.cs:300-301`).
        assert!((player_level + 3..=player_level + 6).contains(&guard.level));
        assert!(guard.combatant.max_health > 0, "RollEnemyClassMaxHealth");
        assert_eq!(guard.combatant.health, guard.combatant.max_health);
        assert_eq!(guard.combatant.kind, EntityKind::EnemyClass);
        // Skills are `min(5·level + 30, 100)` for every slot (`:318-326`).
        let expected = (guard.level * 5 + 30).min(100);
        assert_eq!(guard.combatant.hand_to_hand_skill, expected);
        assert_eq!(guard.chance_mod, expected);
        // Class armour policy, no kit yet: `100 >> 1` clamped to 60 on every
        // part (`EnemyArmorPolicy::Class`, M25's `AssignEnemyEquipment` gap).
        assert_eq!(guard.combatant.armor_values, [60; 7]);
        assert_eq!(guard.weapon_slot, None, "kit-less guard, declared gap");
    }

    #[test]
    fn spawns_are_counted_and_removals_free_the_cap() {
        let mut law = LawRuntime::new(1);
        assert_eq!(law.record_spawn(3), 3);
        assert_eq!(law.spawned, 3);
        assert_eq!(law.record_spawn(2), MAX_ACTIVE_GUARD_SPAWNS);
        // The scene is the authority on who is still alive.
        assert_eq!(law.sync_census(0), 0);
        assert_eq!(law.spawned, 5, "the census never goes backwards");
    }
}

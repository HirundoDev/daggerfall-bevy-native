//! Spine event vocabulary (contract
//! `docs/sintergic/game/programming/spine-contracts-CR-f716455c.md` §6).
//!
//! The bus owns the **payload types** as well as the payloads: §6 types all
//! nine events, and §1 forbids the edge `nervous → app`, so the scene
//! vocabulary the events carry ([`LocationRef`], [`SceneId`], [`SceneTarget`])
//! lives here, in the crate every organ is allowed to depend on. `app`
//! re-exports them from `interior_transit` so the §4 signatures keep their
//! exact paths (`app::interior_transit::SceneTarget`).
//!
//! **What this module is not:** it holds no game rules. It is the shape of the
//! messages — the values are decided by the emitting organ (`combat` decides a
//! hit's damage, `heart` decides the hour band, `perception` decides which
//! window is requested).

/// Reference to a location in the world index (contract §2:
/// `location: LocationRef // (region, location, block) — índice M18`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct LocationRef {
    /// Region index (M13/M18 map index).
    pub region: u32,
    /// Location index inside the region.
    pub location: u32,
    /// Block index of the location (`LocationExterior` grid cell).
    pub block: u32,
}

impl LocationRef {
    /// `(region, location, block)`, as the contract documents it.
    pub const fn new(region: u32, location: u32, block: u32) -> Self {
        Self {
            region,
            location,
            block,
        }
    }
}

/// Which scene the session is showing (contract §2 `scene: SceneId`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SceneId {
    /// Title/menu scene.
    Menu,
    /// Exterior world (city/terrain).
    Exterior,
    /// Loaded interior (`.RDB` block).
    Interior,
}

impl SceneId {
    /// Short label for evidence lines.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Menu => "Menu",
            Self::Exterior => "Exterior",
            Self::Interior => "Interior",
        }
    }
}

/// Scene the transition moves to (contract §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SceneTarget {
    /// Menu scene.
    Menu,
    /// Exterior world at a map pixel of a location.
    Exterior {
        /// Map pixel of the location (`(x, y)` of the location grid).
        map_pixel: (u16, u16),
        /// Owning location.
        location: LocationRef,
    },
    /// Interior: one `.RDB` block of a building of a location.
    Interior {
        /// Owning location.
        location: LocationRef,
        /// `.RDB` block index (BLOCKS.BSA record).
        block: u32,
        /// Building index inside the location.
        building: u32,
    },
}

impl SceneTarget {
    /// Scene this target shows.
    pub const fn scene(&self) -> SceneId {
        match self {
            Self::Menu => SceneId::Menu,
            Self::Exterior { .. } => SceneId::Exterior,
            Self::Interior { .. } => SceneId::Interior,
        }
    }

    /// Owning location.
    pub const fn location(&self) -> LocationRef {
        match self {
            Self::Menu => LocationRef::new(0, 0, 0),
            Self::Exterior { location, .. } | Self::Interior { location, .. } => *location,
        }
    }

    /// Stable label for evidence lines (`Exterior(1,2,3)@(4,5)` /
    /// `Interior(1,2,3) block 7 building 2`).
    pub fn label(&self) -> String {
        match self {
            Self::Menu => "Menu".to_string(),
            Self::Exterior {
                map_pixel,
                location,
            } => format!(
                "Exterior({},{},{})@({},{})",
                location.region, location.location, location.block, map_pixel.0, map_pixel.1
            ),
            Self::Interior {
                location,
                block,
                building,
            } => format!(
                "Interior({},{},{}) block {block} building {building}",
                location.region, location.location, location.block
            ),
        }
    }
}

/// The seven weather states of the source (`m31…:129` — the enum order is
/// `Sunny, Cloudy, Overcast, Fog, Rain, Thunder, Snow`).
///
/// Owned by M31; the spine only carries the value in
/// [`GameEvent::WeatherChanged`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WeatherType {
    /// Clear sky.
    Sunny,
    /// Scattered clouds.
    Cloudy,
    /// Full cloud cover, no precipitation.
    Overcast,
    /// Fog/visibility penalty.
    Fog,
    /// Rain.
    Rain,
    /// Rain plus lightning.
    Thunder,
    /// Snow.
    Snow,
}

impl WeatherType {
    /// Every state, in the source enum order.
    pub const ALL: [Self; 7] = [
        Self::Sunny,
        Self::Cloudy,
        Self::Overcast,
        Self::Fog,
        Self::Rain,
        Self::Thunder,
        Self::Snow,
    ];

    /// Short label for evidence lines.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Sunny => "Sunny",
            Self::Cloudy => "Cloudy",
            Self::Overcast => "Overcast",
            Self::Fog => "Fog",
            Self::Rain => "Rain",
            Self::Thunder => "Thunder",
            Self::Snow => "Snow",
        }
    }
}

/// Day/night band of the game clock (contract §6 `DayNightChanged`).
///
/// The **thresholds** (`DawnHour = 6`, `DuskHour = 18`) are ❤️ heart's
/// (`daggerfall_heart::clock::DAWN_HOUR`/`DUSK_HOUR`): this enum carries the
/// band, it does not decide it — 0 duplicated rules in the bus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HourBand {
    /// Night (the complement of DFU `IsDay`).
    Night,
    /// Day (`DawnHour ≤ hour < DuskHour`).
    Day,
}

impl HourBand {
    /// Short label for evidence lines.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Night => "Night",
            Self::Day => "Day",
        }
    }
}

/// Identifier of a UI window of the shell (contract §5 `WindowId`).
///
/// The **census** of the 85 windows (16 character-creation + 11 config +
/// 7 infra + 51 playable, `m34…:230`) is M34's business; the spine owns the
/// identifier and the four panels that already exist as read models
/// (`m34…:154`: `hud`, `inventory`, `quests`, `main-menu`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WindowId(u16);

impl WindowId {
    /// HUD layer (always alive, `m34…:69-77`).
    pub const HUD: Self = Self(0);
    /// Inventory panel.
    pub const INVENTORY: Self = Self(1);
    /// Quests/journal panel.
    pub const QUESTS: Self = Self(2);
    /// Main menu panel.
    pub const MAIN_MENU: Self = Self(3);

    /// The four panels that exist today, in panel order.
    pub const PANELS: [Self; 4] = [Self::HUD, Self::INVENTORY, Self::QUESTS, Self::MAIN_MENU];

    /// Wraps a raw window index.
    pub const fn new(index: u16) -> Self {
        Self(index)
    }

    /// Raw window index.
    pub const fn index(self) -> u16 {
        self.0
    }
}

/// The nine spine events (contract §6), typed.
///
/// Payloads are the §6 minimums and stay `Copy`/small: the whole enum is
/// bounded by [`MAX_EVENT_BYTES`], asserted at compile time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameEvent {
    /// `app` activated a door (M34 S06). Consumed by `SceneTransition` and
    /// quests. Payload: `(location, block, door_id)`.
    DoorActivated {
        /// Location the door belongs to.
        location: LocationRef,
        /// `.RDB` block of the door.
        block: u32,
        /// Door index inside the registry.
        door_id: u32,
    },
    /// `SceneTransition` finished a load. Consumed by `perception` and `heart`.
    SceneChanged(SceneTarget),
    /// `combat` resolved a hit. Consumed by `app` (audio/HUD) and
    /// `antibodies`. Payload: `(attacker, target, damage, crit)`.
    CombatHit {
        /// Attacker entity id.
        attacker: u32,
        /// Target entity id.
        target: u32,
        /// Damage dealt.
        damage: u16,
        /// Critical hit?
        crit: bool,
    },
    /// `inventory`/M30 completed a purchase. Consumed by UI, quests and save.
    /// Payload: `(item, price, seller)`.
    PurchaseCompleted {
        /// Item id.
        item: u32,
        /// Price paid (integer gold).
        price: u32,
        /// Seller entity/faction id.
        seller: u32,
    },
    /// `behavior::quests` advanced a quest. Consumed by UI/journal and save.
    /// Payload: `(quest_id, stage)`.
    QuestStage {
        /// Quest id.
        quest_id: u32,
        /// Stage index.
        stage: u16,
    },
    /// `behavior::law` recorded a crime. Consumed by guards and court.
    /// Payload: `(crime, region)`.
    CrimeCommitted {
        /// Crime type id.
        crime: u16,
        /// Region index.
        region: u16,
    },
    /// `heart`/M31 changed the weather. Consumed by `app` (sky/fog).
    WeatherChanged(WeatherType),
    /// `heart::clock` crossed a day/night band. Consumed by `app` (sky) and
    /// NPC routines.
    DayNightChanged(HourBand),
    /// Any system asked the shell to show a window. Consumed by `UiShell`.
    UiRequested(WindowId),
}

/// Upper bound of a spine event payload (contract §6: "≤64 B").
pub const MAX_EVENT_BYTES: usize = 64;

// The §6 size contract is a compile-time fact, not a probe assertion.
const _: () = assert!(core::mem::size_of::<GameEvent>() <= MAX_EVENT_BYTES);

impl GameEvent {
    /// Stable label for evidence lines (`DoorActivated`, `SceneChanged`, …).
    pub const fn label(&self) -> &'static str {
        match self {
            Self::DoorActivated { .. } => "DoorActivated",
            Self::SceneChanged(_) => "SceneChanged",
            Self::CombatHit { .. } => "CombatHit",
            Self::PurchaseCompleted { .. } => "PurchaseCompleted",
            Self::QuestStage { .. } => "QuestStage",
            Self::CrimeCommitted { .. } => "CrimeCommitted",
            Self::WeatherChanged(_) => "WeatherChanged",
            Self::DayNightChanged(_) => "DayNightChanged",
            Self::UiRequested(_) => "UiRequested",
        }
    }

    /// Size of the payload the bus actually queues (bytes, `size_of` of the
    /// enum — the largest variant plus its discriminant).
    pub const fn payload_len(&self) -> usize {
        core::mem::size_of::<Self>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payloads_fit_the_contract_cap() {
        for event in [
            GameEvent::DoorActivated {
                location: LocationRef::new(19, 0, 3),
                block: 7,
                door_id: 2,
            },
            GameEvent::SceneChanged(SceneTarget::Interior {
                location: LocationRef::new(19, 0, 3),
                block: 7,
                building: 1,
            }),
            GameEvent::WeatherChanged(WeatherType::Thunder),
            GameEvent::DayNightChanged(HourBand::Night),
            GameEvent::UiRequested(WindowId::HUD),
        ] {
            assert!(event.payload_len() <= MAX_EVENT_BYTES);
        }
    }

    #[test]
    fn scene_target_scene_and_location() {
        let target = SceneTarget::Interior {
            location: LocationRef::new(1, 2, 3),
            block: 9,
            building: 4,
        };
        assert_eq!(target.scene(), SceneId::Interior);
        assert_eq!(target.location(), LocationRef::new(1, 2, 3));
        assert_eq!(SceneTarget::Menu.location(), LocationRef::new(0, 0, 0));
    }

    #[test]
    fn weather_census_is_seven() {
        assert_eq!(WeatherType::ALL.len(), 7);
        assert_eq!(WeatherType::ALL[0], WeatherType::Sunny);
        assert_eq!(WeatherType::ALL[6], WeatherType::Snow);
    }
}

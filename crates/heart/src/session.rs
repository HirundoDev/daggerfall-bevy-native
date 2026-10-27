//! `GameSession` — the spine's session (contract
//! `docs/sintergic/game/programming/spine-contracts-CR-f716455c.md` §2).
//!
//! ❤️ Heart owns the session: the seed, the mode, the game clock, where the
//! player is and which scene is on screen. It does **not** render, does not
//! touch disk and holds no combat/economy rule (§1 "Prohibido").
//!
//! ## Invariants (§2), each one checkable
//!
//! | # | Invariant | Where |
//! |:--|:---|:---|
//! | i | `tick` is pure wrt `(estado, dt, rng)` | [`GameSession::tick`] advances only the clock; 0 ambient state |
//! | ii | the system order is fixed and documented | [`GameSession::SYSTEMS`] (empty until M34 registers systems) |
//! | iii | 0 disk/render access | 0 `std::fs`, 0 Bevy in this file |
//! | iv | `mode=Menu` does not advance the game clock | [`GameSession::tick`] returns `minutes=0` |
//! | v | `Dead` does not resurrect | [`GameSession::start`] ⇒ [`SessionError::Dead`] |
//!
//! ## Additive surface — RATIFIED by M34-SPINE (W34-0, `CR-2d5c8f41`)
//!
//! §2 exposes no way to *reach* two states that §7 and invariant (v) require:
//! the scene after a transition (§7 step 4 asserts `scene=Interior`) and the
//! `Dead` mode. [`GameSession::enter_scene`] and [`GameSession::mark_dead`] are
//! **additions** — 0 signature changed, 0 new game rule — on the same footing
//! as the R-8 addendum `SceneTransition::{begin, complete, fail}` (contract §4),
//! which `app` already needed to report the outcome of the load it owns (§4
//! "la carga es responsabilidad de `app`").
//!
//! The addendum's contract, ratified as-is (the type lives in
//! `daggerfall_game_core::scene` and is re-exported unchanged by
//! `daggerfall_app::interior_transit`; W34-0 only documents it):
//!
//! | Method | Contract | Observable in `SPINE-001` |
//! |:---|:---|:---|
//! | `begin` | returns the `Loading` target **once**; `None` before a request and after `Done`/`Failed` | steps 04/05 (`loads` = 2, one per door) |
//! | `complete` | accepts only the target in flight and makes `Done` deliverable **exactly once** | steps 04/05 (`done_once=true done_again=false`) |
//! | `fail` | latches `Failed` with the loader's error; `poll` keeps the failure readable (`Some(Failed)`) and never delivers `Done`; `begin` stays `None`; the scene does not change | control C4 |
//!
//! Ratification of this surface is **documentation of measured plumbing**, not
//! a new rule: no domain formula, system or state is added (M34 §3.3 / W34-0
//! acceptance "0 reglas de dominio nuevas").

use std::fmt;

use daggerfall_memory::save_v2::{
    SaveError, SaveSection, SaveStateV2, SectionCodec, SectionError, SectionId, SAVE_VERSION,
};
use daggerfall_nervous::events::{HourBand, LocationRef, SceneId};

use crate::clock::GameClock;

/// Section owned by ❤️ heart inside the v2 envelope (§3 owner rule).
pub const SECTION_SESSION: SectionId = SectionId::new("session");

/// Bytes of [`GameSession::to_bytes`] (fixed width — the encoding is a struct
/// dump, 0 text, 0 float).
pub const SESSION_BYTES: usize = 8 + 4 + 1 + 1 + 24 + 24 + 12 + 8;

/// Session mode (§2 `SessionMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SessionMode {
    /// Title screen: the game clock does not advance.
    Menu,
    /// Playing: `tick` advances the clock.
    Playing,
    /// Paused: `tick` does not advance the clock.
    Paused,
    /// Dead: latched, [`GameSession::start`] refuses (invariant v, M32).
    Dead,
}

impl SessionMode {
    /// Short label for evidence lines.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Menu => "Menu",
            Self::Playing => "Playing",
            Self::Paused => "Paused",
            Self::Dead => "Dead",
        }
    }

    /// Wire code of the mode (stable, never the enum discriminant).
    const fn code(self) -> u8 {
        match self {
            Self::Menu => 0,
            Self::Playing => 1,
            Self::Paused => 2,
            Self::Dead => 3,
        }
    }

    /// Inverse of [`Self::code`].
    const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Menu),
            1 => Some(Self::Playing),
            2 => Some(Self::Paused),
            3 => Some(Self::Dead),
            _ => None,
        }
    }
}

/// Career the session was created with (§2 `new(seed, career)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CareerId(pub u32);

/// Session RNG (§2 `rng: SessionRng // serializable (M33)`).
///
/// **Algorithm = the contract:** the DFU LCG `DFU:API/DFRandom.cs`
/// (`next = next·1103515245 + 12345`, return `(next >> 16) & 0x7FFF`), the same
/// sequence as `daggerfall_parser_rs::dfrandom::DfRandom` (M15) and
/// `behavior::determinism::DfRandom`. Kept local for the same reason as those
/// copies: ❤️ heart does not depend on another organ and the contract is the
/// algorithm, not the module path. The state is an explicit `u64` so the
/// stream is local, serializable and testable without a global.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionRng {
    next: u64,
}

impl SessionRng {
    /// Seeds the stream (`DFRandom.srand`: `next = seed`).
    pub const fn from_seed(seed: u64) -> Self {
        Self { next: seed }
    }

    /// Current state (the value the save carries).
    pub const fn state(self) -> u64 {
        self.next
    }

    /// One draw, `0…32767` (`DFRandom.rand`, no bias correction).
    pub fn next_u32(&mut self) -> u32 {
        self.next = self.next.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        ((self.next >> 16) & 0x7FFF) as u32
    }
}

/// Result of one [`GameSession::tick`] — the numbers the probe prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickReport {
    /// Game minutes actually advanced (`0` outside `Playing`).
    pub minutes: u32,
    /// Clock before the tick.
    pub before: GameClock,
    /// Clock after the tick.
    pub after: GameClock,
    /// Day/night band before the tick.
    pub band_before: HourBand,
    /// New band when the tick crossed it — the value `DayNightChanged` carries.
    pub band_changed: Option<HourBand>,
    /// Session clock after the tick (`0` at `new`, in game minutes).
    pub elapsed_minutes: u64,
}

/// Why a session operation was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionError {
    /// Invariant (v): a dead session does not restart (M32 `NEGATIVE`).
    Dead,
    /// The envelope carries no `session` section.
    MissingSection,
    /// The envelope version is not the one this build writes.
    UnsupportedVersion {
        /// Version found.
        version: u16,
    },
    /// The section bytes are not a session this build can restore.
    Corrupt {
        /// What is wrong.
        why: &'static str,
    },
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dead => f.write_str("session is Dead: a dead session does not restart"),
            Self::MissingSection => {
                write!(f, "save carries no {:?} section", SECTION_SESSION.key())
            }
            Self::UnsupportedVersion { version } => write!(
                f,
                "envelope version {version} is not supported (want {SAVE_VERSION})"
            ),
            Self::Corrupt { why } => write!(f, "corrupt session section: {why}"),
        }
    }
}

impl std::error::Error for SessionError {}

impl From<SaveError> for SessionError {
    fn from(_: SaveError) -> Self {
        Self::Corrupt {
            why: "envelope could not be read",
        }
    }
}

/// The spine's single session (contract §2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameSession {
    /// World seed (§2).
    pub seed: u64,
    /// Mode (§2). `Menu` at `new`, `Dead` is terminal.
    pub mode: SessionMode,
    /// Game clock (§2 — ❤️ [`GameClock`], M4/M31).
    pub clock: GameClock,
    /// Where the player is (§2 — M18 index).
    pub location: LocationRef,
    /// Scene on screen (§2).
    pub scene: SceneId,
    /// Session RNG (§2 — serializable).
    pub rng: SessionRng,
    career: CareerId,
    /// Clock at `new` — the origin of [`Self::elapsed_minutes`].
    epoch: GameClock,
}

impl GameSession {
    /// Fixed system order of [`Self::tick`] (invariant ii).
    ///
    /// Empty on purpose: the spine is plumbing and registers **no** system.
    /// M34 (and every gameplay WP) appends here, in this order, and the probe
    /// then measures the documented order instead of an accidental one.
    pub const SYSTEMS: [&'static str; 0] = [];

    /// Fresh session: `mode=Menu`, session clock `0`, `scene=Menu` (§2/§7.1).
    pub fn new(seed: u64, career: CareerId) -> Self {
        let clock = GameClock::classic_default();
        Self {
            seed,
            mode: SessionMode::Menu,
            clock,
            location: LocationRef::new(0, 0, 0),
            scene: SceneId::Menu,
            rng: SessionRng::from_seed(seed),
            career,
            epoch: clock,
        }
    }

    /// Career the session was created with.
    pub const fn career(&self) -> CareerId {
        self.career
    }

    /// Session clock in game minutes since [`Self::new`] (`0` at creation,
    /// `60` after six `tick(10)`). Derived from the clock, not stored twice.
    pub fn elapsed_minutes(&self) -> u64 {
        self.clock
            .total_minutes()
            .saturating_sub(self.epoch.total_minutes())
    }

    /// Starts play at `location`: `mode=Playing`, `scene=Exterior` (§7.2).
    ///
    /// A dead session refuses (invariant v).
    pub fn start(&mut self, location: LocationRef) -> Result<(), SessionError> {
        if self.mode == SessionMode::Dead {
            return Err(SessionError::Dead);
        }
        self.mode = SessionMode::Playing;
        self.location = location;
        self.scene = SceneId::Exterior;
        Ok(())
    }

    /// Advances the clock by `dt_minutes` **only while `Playing`** (§2 iv).
    ///
    /// Pure: the only input is the state and `dt` (invariant i); no system is
    /// registered yet ([`Self::SYSTEMS`]), so the rng is not consumed here —
    /// the systems M34 registers are what consume it, in the documented order.
    pub fn tick(&mut self, dt_minutes: u32) -> TickReport {
        let before = self.clock;
        let band_before = band(before);
        let minutes = if self.mode == SessionMode::Playing {
            dt_minutes
        } else {
            0
        };
        if minutes > 0 {
            self.clock
                .raise_seconds(u64::from(minutes) * crate::clock::SECONDS_PER_MINUTE);
        }
        let after = self.clock;
        let band_after = band(after);
        TickReport {
            minutes,
            before,
            after,
            band_before,
            // One tick may span more than a day; only a net band flip is
            // reported (the spine's ticks are minutes, M31's are hours).
            band_changed: (band_after != band_before).then_some(band_after),
            elapsed_minutes: self.elapsed_minutes(),
        }
    }

    /// Puts `scene` on screen — the `app` side of a finished transition (§4:
    /// the gameplay only reacts to `Done`). Additive, R-8 class, **ratified by
    /// M34-SPINE (W34-0, `CR-2d5c8f41`)** — see module docs.
    pub fn enter_scene(&mut self, scene: SceneId) {
        self.scene = scene;
    }

    /// Latches `Dead` (M32 death). Additive, R-8 class, **ratified by
    /// M34-SPINE (W34-0, `CR-2d5c8f41`)**: without it invariant (v) would be
    /// unstateable.
    pub fn mark_dead(&mut self) {
        self.mode = SessionMode::Dead;
    }

    /// The session as a v2 envelope holding **only** the section heart owns
    /// (§2 `snapshot`): one section, no module's payload is invented here.
    pub fn snapshot(&self) -> SaveStateV2 {
        let mut state = SaveStateV2::new();
        state.sections.push(SaveSection {
            id: SECTION_SESSION,
            bytes: self.to_bytes(),
        });
        state
    }

    /// Restores a session from an envelope (§2 `restore`). The envelope must
    /// be version 2 and carry heart's section.
    pub fn restore(state: &SaveStateV2) -> Result<Self, SessionError> {
        if state.version != SAVE_VERSION {
            return Err(SessionError::UnsupportedVersion {
                version: state.version,
            });
        }
        let section = state
            .section(SECTION_SESSION)
            .ok_or(SessionError::MissingSection)?;
        Self::from_bytes(&section.bytes)
    }

    /// Fixed-width encoding of the session section (little endian, 0 text).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(SESSION_BYTES);
        out.extend_from_slice(&self.seed.to_le_bytes());
        out.extend_from_slice(&self.career.0.to_le_bytes());
        out.push(self.mode.code());
        out.push(scene_code(self.scene));
        push_clock(&mut out, self.clock);
        push_clock(&mut out, self.epoch);
        out.extend_from_slice(&self.location.region.to_le_bytes());
        out.extend_from_slice(&self.location.location.to_le_bytes());
        out.extend_from_slice(&self.location.block.to_le_bytes());
        out.extend_from_slice(&self.rng.state().to_le_bytes());
        debug_assert_eq!(out.len(), SESSION_BYTES);
        out
    }

    /// Inverse of [`Self::to_bytes`]; 0 panics on hostile input.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SessionError> {
        if bytes.len() != SESSION_BYTES {
            return Err(SessionError::Corrupt {
                why: "session payload has the wrong length",
            });
        }
        let mut cursor = 0usize;
        let seed = read_u64(bytes, &mut cursor)?;
        let career = CareerId(read_u32(bytes, &mut cursor)?);
        let mode = SessionMode::from_code(read_u8(bytes, &mut cursor)?).ok_or(
            SessionError::Corrupt {
                why: "unknown session mode",
            },
        )?;
        let scene = scene_from_code(read_u8(bytes, &mut cursor)?).ok_or(SessionError::Corrupt {
            why: "unknown scene id",
        })?;
        let clock = read_clock(bytes, &mut cursor)?;
        let epoch = read_clock(bytes, &mut cursor)?;
        let location = LocationRef::new(
            read_u32(bytes, &mut cursor)?,
            read_u32(bytes, &mut cursor)?,
            read_u32(bytes, &mut cursor)?,
        );
        let rng = SessionRng::from_seed(read_u64(bytes, &mut cursor)?);
        Ok(Self {
            seed,
            mode,
            clock,
            location,
            scene,
            rng,
            career,
            epoch,
        })
    }
}

impl SectionCodec for GameSession {
    fn id(&self) -> SectionId {
        SECTION_SESSION
    }

    fn encode(&self) -> Vec<u8> {
        self.to_bytes()
    }

    fn decode(&mut self, bytes: &[u8]) -> Result<(), SectionError> {
        *self = Self::from_bytes(bytes).map_err(|error| SectionError::new(match error {
            SessionError::MissingSection => "session section missing",
            SessionError::UnsupportedVersion { .. } => "session envelope version",
            SessionError::Dead => "session is dead",
            SessionError::Corrupt { why } => why,
        }))?;
        Ok(())
    }
}

/// Day/night band of `clock` — the thresholds are the clock's own
/// (`DAWN_HOUR`/`DUSK_HOUR`), 0 duplicated rules in the bus.
fn band(clock: GameClock) -> HourBand {
    if clock.is_day() {
        HourBand::Day
    } else {
        HourBand::Night
    }
}

const fn scene_code(scene: SceneId) -> u8 {
    match scene {
        SceneId::Menu => 0,
        SceneId::Exterior => 1,
        SceneId::Interior => 2,
    }
}

const fn scene_from_code(code: u8) -> Option<SceneId> {
    match code {
        0 => Some(SceneId::Menu),
        1 => Some(SceneId::Exterior),
        2 => Some(SceneId::Interior),
        _ => None,
    }
}

fn push_clock(out: &mut Vec<u8>, clock: GameClock) {
    for field in [
        clock.year,
        clock.month,
        clock.day,
        clock.hour,
        clock.minute,
        clock.second,
    ] {
        out.extend_from_slice(&field.to_le_bytes());
    }
}

fn read_clock(bytes: &[u8], cursor: &mut usize) -> Result<GameClock, SessionError> {
    Ok(GameClock {
        year: read_u32(bytes, cursor)?,
        month: read_u32(bytes, cursor)?,
        day: read_u32(bytes, cursor)?,
        hour: read_u32(bytes, cursor)?,
        minute: read_u32(bytes, cursor)?,
        second: read_u32(bytes, cursor)?,
    })
}

fn read_u8(bytes: &[u8], cursor: &mut usize) -> Result<u8, SessionError> {
    let value = *bytes.get(*cursor).ok_or(SessionError::Corrupt {
        why: "session payload ended early",
    })?;
    *cursor += 1;
    Ok(value)
}

fn read_u32(bytes: &[u8], cursor: &mut usize) -> Result<u32, SessionError> {
    let end = *cursor + 4;
    let slice = bytes.get(*cursor..end).ok_or(SessionError::Corrupt {
        why: "session payload ended early",
    })?;
    let mut raw = [0u8; 4];
    raw.copy_from_slice(slice);
    *cursor = end;
    Ok(u32::from_le_bytes(raw))
}

fn read_u64(bytes: &[u8], cursor: &mut usize) -> Result<u64, SessionError> {
    let end = *cursor + 8;
    let slice = bytes.get(*cursor..end).ok_or(SessionError::Corrupt {
        why: "session payload ended early",
    })?;
    let mut raw = [0u8; 8];
    raw.copy_from_slice(slice);
    *cursor = end;
    Ok(u64::from_le_bytes(raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn started() -> GameSession {
        let mut session = GameSession::new(0x5EED, CareerId(7));
        session
            .start(LocationRef::new(19, 0, 3))
            .expect("start on a fresh session");
        session
    }

    #[test]
    fn new_is_menu_with_clock_zero() {
        let session = GameSession::new(1, CareerId(0));
        assert_eq!(session.mode, SessionMode::Menu);
        assert_eq!(session.elapsed_minutes(), 0);
        assert_eq!(session.scene, SceneId::Menu);
    }

    #[test]
    fn menu_does_not_advance_the_clock() {
        let mut session = GameSession::new(1, CareerId(0));
        let report = session.tick(600);
        assert_eq!(report.minutes, 0);
        assert_eq!(session.elapsed_minutes(), 0);
    }

    #[test]
    fn six_ticks_of_ten_minutes_are_one_hour() {
        let mut session = started();
        for _ in 0..6 {
            session.tick(10);
        }
        assert_eq!(session.elapsed_minutes(), 60);
        assert_eq!(session.clock.hour, 13);
    }

    #[test]
    fn crossing_dusk_reports_the_night_band_once() {
        let mut session = started();
        let mut changes = Vec::new();
        for _ in 0..36 {
            // 12:00 → 18:00
            if let Some(band) = session.tick(10).band_changed {
                changes.push(band);
            }
        }
        assert_eq!(changes, vec![HourBand::Night]);
        assert_eq!(session.clock.hour, 18);
    }

    #[test]
    fn dead_does_not_restart() {
        let mut session = started();
        session.mark_dead();
        assert_eq!(
            session.start(LocationRef::new(1, 2, 3)),
            Err(SessionError::Dead)
        );
    }

    #[test]
    fn snapshot_restore_is_byte_identical() {
        let session = started();
        let state = session.snapshot();
        let restored = GameSession::restore(&state).expect("restore");
        assert_eq!(restored, session);
        assert_eq!(restored.snapshot().encode(), state.encode());
    }

    #[test]
    fn restore_refuses_a_foreign_or_old_envelope() {
        let empty = SaveStateV2::new();
        assert_eq!(
            GameSession::restore(&empty),
            Err(SessionError::MissingSection)
        );

        let mut wrong = empty;
        wrong.version = 1;
        assert_eq!(
            GameSession::restore(&wrong),
            Err(SessionError::UnsupportedVersion { version: 1 })
        );
    }

    #[test]
    fn corrupt_payload_is_an_error_not_a_panic() {
        for len in 0..SESSION_BYTES {
            assert!(GameSession::from_bytes(&vec![0u8; len]).is_err());
        }
        let mut bytes = started().to_bytes();
        bytes[12] = 0xFF; // unknown mode code (seed u64 + career u32 precede it)
        assert!(GameSession::from_bytes(&bytes).is_err());
    }

    #[test]
    fn rng_is_seed_deterministic() {
        let mut a = GameSession::new(42, CareerId(0));
        let mut b = GameSession::new(42, CareerId(0));
        let mut c = GameSession::new(43, CareerId(0));
        let stream_a: Vec<u32> = (0..8).map(|_| a.rng.next_u32()).collect();
        let stream_b: Vec<u32> = (0..8).map(|_| b.rng.next_u32()).collect();
        let stream_c: Vec<u32> = (0..8).map(|_| c.rng.next_u32()).collect();
        assert_eq!(stream_a, stream_b);
        assert_ne!(stream_a, stream_c);
        assert!(stream_a.iter().all(|value| *value <= 0x7FFF));
    }
}

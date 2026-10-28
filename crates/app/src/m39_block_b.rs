//! M39/UI-3 — block **B** (HUD: `DaggerfallHUD`), 1 window of plan §1/§2.
//!
//! **The HUD is a live vitals panel: one intent paints, the next advances an
//! hour.** One [`interact`] delivers the window's two intents:
//!
//! 1. **tick** — paint the frame of the state's HUD projection
//!    ([`M39State::hud`]) and print every declared HUD read model of it;
//! 2. **hour** — advance one hour of world time through the domain's own rest
//!    window ([`RestWindow`] in `Loiter` mode: the clock advances and the
//!    window recovers nothing, `rest.rs` `Loiter` branch), paying the
//!    domain's activity fatigue price for the hour
//!    ([`Character::spend_fatigue`] with the same amount
//!    `Character::train_skill` charges for a skill use,
//!    `character.rs:1115`). The HUD is then re-projected from that character,
//!    so the frame the block paints is the living session's, never a copy.
//!
//! Every route whose value moved between the painted frame and the new one is
//! reported with its exact signed delta. Nothing is invented: a printed value
//! is the read model's own resolution ([`SessionReadModels::resolve`], the
//! same single source `ui_compat::resolve_text` prints through), a vital is
//! the [`Character`]'s own field, and the hour's price is a `daggerfall_skills`
//! constant — the block never formats a number the read model does not carry
//! and never names a game number of its own.
//!
//! **Routes (plan §3.1 (a): a *declared* read model).** The HUD prints the
//! seven session HUD routes of `ui_compat::text::READ_MODEL_PATHS`
//! (`hud.health` … `hud.stamina_pct`) — the family the HUD window already
//! paints on M37 (`m37_pilots::character_sheet`). `M39Models` declares the 96
//! `m39.*` routes of plan §5 with **no** `hud` family (A21 C14 D16 E12 F10
//! G11 H12), so naming an `m39.hud.*` route here would be an undeclared route
//! — an error by plan §3.3, never a default. [`HUD_PATHS`] is the single place
//! the block names its routes: if the registry ever declares a `m39.hud.*`
//! family, only this table (and the projection below) changes.
//!
//! **Provenance of the baseline (plan §3.1).** [`BState::from_retail`] derives
//! the session-start HUD exactly as `M39State::hud_from_retail` does — retail
//! `CLASS01.CFG` → [`Career::parse`] → [`Character::starting`] with the
//! `M19_SEED` seed and level 1 → [`HudReadModel::project`] — so the block's
//! previous frame is the composed frame and a fresh session reports a steady
//! HUD (no delta) instead of a fabricated one. `from_retail` never reads a
//! value out of thin air and no literal game number appears in this module.
//!
//! **Determinism (plan §3.2).** `interact` is a pure function of the state and
//! the intent: the digest is `fnv1a64` of the interaction's own signature
//! ([`Interaction::sealed`]), so the same state plus the same intents hand back
//! the same `Interaction` and the same digest.

use std::collections::BTreeSet;
use std::path::Path;

use daggerfall_nervous::events::WindowId;
use daggerfall_parser_bevy_bridge::ui_compat::{is_read_model_path, UiCompatError};
use daggerfall_parser_bevy_bridge::ui_spec::UiAssets;
use daggerfall_perception::read_models::{HudReadModel, SessionReadModels, M39_ABSENT};
use daggerfall_perception::ui_census;
use daggerfall_perception::ui_shell::canonical_id_for;
use daggerfall_skills::career::Career;
use daggerfall_skills::character::{Character, DEFAULT_FATIGUE_LOSS};
use daggerfall_skills::rest::{RestMode, RestVitals, RestWindow};
use daggerfall_skills::rng::Rng64;

use crate::m18_demo::{M19_SEED, M20_CLASS_FILE};
use crate::m39_state::{BlockEvidence, Interaction, M39State, ModalOutcome};

/// Windows of the block (plan §2 partition: 51 = 16+1+2+7+7+3+6+9).
pub const BLOCK_B_WINDOWS: usize = 1;

/// Census name of the block's only window (plan §2, block B).
pub const HUD_WINDOW: &str = "DaggerfallHUD";

/// Session-start level of the composed character (`M39State::hud_from_retail`
/// composes the retail career at level 1; the HUD shows that character).
const SESSION_LEVEL: i32 = 1;

/// Hours the window's second intent advances in one delivery.
///
/// The intent is *one* hour of world time; the rest window reports how many
/// hours it closed (`RestOutcome::hours`) and the block prices exactly those.
const HOURS_PER_ADVANCE: u32 = 1;

/// Raw fatigue the domain charges for one hour of activity.
///
/// It is the fatigue price the domain already puts on a skill use
/// ([`Character::train_skill`], `character.rs:1115`:
/// `DEFAULT_FATIGUE_LOSS * 180`) — [`DEFAULT_FATIGUE_LOSS`] is the crate's own
/// base fatigue loss (`PlayerEntity.cs:109`). The block multiplies a
/// `daggerfall_skills` constant by a `daggerfall_skills` constant and never
/// writes a game number itself.
const ACTIVITY_FATIGUE_LOSS: i32 = DEFAULT_FATIGUE_LOSS * 180;

/// The HUD routes the block prints, in paint order — the seven declared
/// session HUD read models of `ui_compat::text::READ_MODEL_PATHS`.
///
/// Every entry is asserted to be a declared read-model path (a path outside
/// that list is an `unknown_path` error, not a string in a caller).
pub const HUD_PATHS: [&str; 7] = [
    "hud.health",
    "hud.max_health",
    "hud.stamina",
    "hud.max_stamina",
    "hud.level",
    "hud.health_pct",
    "hud.stamina_pct",
];

/// Rule string: the block delivers only its own window.
const RULE_WINDOW: &str = "m39: block B delivers only DaggerfallHUD";
/// Rule string: the window is not in the frozen census (plan §3.3).
const RULE_CENSUS: &str = "m39: DaggerfallHUD is not in the frozen census";
/// Rule string: the session-start career (`CLASS01.CFG`) cannot be read.
const RULE_CAREER_READ: &str = "m39: session career CLASS01.CFG unreadable";
/// Rule string: the session-start career (`CLASS01.CFG`) is not a career.
const RULE_CAREER_PARSE: &str = "m39: session career CLASS01.CFG rejected";
/// Rule string: a HUD route is not declared by the read-model registry.
const RULE_ROUTE: &str = "m39: HUD read-model route undeclared";
/// Rule string: a HUD route did not resolve to an integer (the seven HUD
/// routes are numeric by contract).
const RULE_NUMERIC: &str = "m39: HUD read model is not numeric";
/// Rule string: the block reports a frame it never painted.
const RULE_FRAME: &str = "m39: HUD frame was never painted";
/// Rule string: a reported delta is not a HUD route, or is zero.
const RULE_DELTA: &str = "m39: HUD delta is not a declared route or is zero";
/// Rule string: the block was asked for evidence before its first tick.
const RULE_UNTICKED: &str = "m39: HUD tick never delivered";
/// Rule string: the rest window advanced no hour, so the tick cannot price it.
const RULE_ADVANCE: &str = "m39: HUD hour did not advance";
/// Rule string: the window delivered fewer than its two intents (paint + hour).
const RULE_INTENTS: &str = "m39: HUD delivered fewer than two intents";

/// Block B's own state.
///
/// Holds what one HUD needs across ticks: the resolved window and its census
/// ordinal, the tick counter, the counters of the block evidence, the
/// **living session character** the HUD projects (the only source of the
/// vitals the frame prints), the frame painted last and the exact delta
/// reported by the last tick (plus every route that has moved since the block
/// started).
#[derive(Debug, Clone)]
pub struct BState {
    /// Typed window handle, resolved by name from the census (plan §3.3).
    window: WindowId,
    /// Census ordinal of the window (`index + 1`, plan §1: 038).
    ordinal: u16,
    /// HUD ticks delivered.
    tick: u64,
    /// Intents attempted (two per tick: paint + hour).
    intents: usize,
    /// Intents delivered.
    delivered: usize,
    /// Living session character: the HUD's source of truth. Its vitals are
    /// the ones the frame prints, so the hour intent mutates *it* and
    /// re-projects the read model instead of writing a display value.
    character: Character,
    /// Frame painted last — the values the HUD printed (never a copy of the
    /// source: it is the projection of the tick that printed it).
    painted: Option<HudReadModel>,
    /// Delta reported by the last tick, in paint order.
    last_delta: Vec<(&'static str, i64)>,
    /// Routes that moved at least once since the block started (sorted).
    changed: BTreeSet<&'static str>,
}

/// Field-wise equality, including the living character.
///
/// [`M39State`] is frozen with `#[derive(Eq)]`, while [`Character`] carries
/// the domain's own floating-point fields and only derives `PartialEq`
/// (`skills/src/character.rs:691`). The comparison below is the domain's own
/// `PartialEq` for every field, and the `Eq` marker is sound here because a
/// character in this block is always `Character::starting` over the retail
/// `CLASS01.CFG` — an integer file — so no field can hold a `NaN`.
impl PartialEq for BState {
    fn eq(&self, other: &Self) -> bool {
        self.window == other.window
            && self.ordinal == other.ordinal
            && self.tick == other.tick
            && self.intents == other.intents
            && self.delivered == other.delivered
            && self.character == other.character
            && self.painted == other.painted
            && self.last_delta == other.last_delta
            && self.changed == other.changed
    }
}

impl Eq for BState {}

impl BState {
    /// Fills the block state from the retail bank and the `ARENA2` directory.
    ///
    /// The baseline frame is the session-start HUD: retail
    /// [`M20_CLASS_FILE`], the [`M19_SEED`] seed, [`SESSION_LEVEL`] and the
    /// domain's own [`Character::starting`] — the same composition
    /// `M39State::hud_from_retail` performs, so the block's previous frame and
    /// the state's projection agree by construction (a divergence is caught by
    /// `m39_block_b_baseline_is_the_composed_projection`).
    ///
    /// `assets` is not read: the HUD **surface** (asset, nodes, glyphs) is
    /// declared by `m39_surfaces` / `m37_pilots`; this block owns the *tick*,
    /// whose only source is the retail career of the session.
    pub fn from_retail(assets: &UiAssets, arena2: &Path) -> Result<Self, UiCompatError> {
        let _ = assets;
        let bytes = std::fs::read(arena2.join(M20_CLASS_FILE))
            .map_err(|_| UiCompatError::Text(RULE_CAREER_READ))?;
        let career = Career::parse(&bytes).map_err(|_| UiCompatError::Text(RULE_CAREER_PARSE))?;
        let character = Character::starting(career, &mut Rng64::new(M19_SEED), SESSION_LEVEL);
        Ok(Self {
            window: window_for(HUD_WINDOW)?,
            ordinal: ordinal_for(HUD_WINDOW)?,
            tick: 0,
            intents: 0,
            delivered: 0,
            painted: Some(project(&character)),
            character,
            last_delta: Vec::new(),
            changed: BTreeSet::new(),
        })
    }

    /// The typed handle the block delivers.
    pub fn window(&self) -> WindowId {
        self.window
    }

    /// Census ordinal the block reports as its `id` (plan §4.1).
    pub fn ordinal(&self) -> u16 {
        self.ordinal
    }

    /// Ticks the block has delivered.
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Frame painted last (`None` = nothing painted yet).
    pub fn painted(&self) -> Option<&HudReadModel> {
        self.painted.as_ref()
    }

    /// Delta reported by the last tick (exact signed magnitude, paint order).
    pub fn last_delta(&self) -> &[(&'static str, i64)] {
        &self.last_delta
    }

    /// Routes that moved at least once since the block started.
    pub fn changed(&self) -> &BTreeSet<&'static str> {
        &self.changed
    }
}

/// Resolves a census window by **name** (plan §3.3: never a numeric id).
fn window_for(name: &str) -> Result<WindowId, UiCompatError> {
    canonical_id_for(name).ok_or(UiCompatError::Navigation(RULE_CENSUS))
}

/// Census ordinal of a window (`index + 1`, plan §1: `ordinal = índice + 1`).
fn ordinal_for(name: &str) -> Result<u16, UiCompatError> {
    let index = ui_census::census()
        .iter()
        .position(|row| row.name == name)
        .ok_or(UiCompatError::Navigation(RULE_CENSUS))?;
    u16::try_from(index + 1).map_err(|_| UiCompatError::Navigation(RULE_CENSUS))
}

/// The HUD frame the session character projects.
///
/// The single composition point of the block: `RpgStats` → `HudSource` →
/// [`HudReadModel::project`], i.e. exactly what `M39State::hud_from_retail`
/// composes at session start. Every frame the block paints comes from here.
fn project(character: &Character) -> HudReadModel {
    HudReadModel::project(&crate::m7_readmodels::hud_source_of(&character.rpg_stats()))
}

/// The vitals of the living character as the rest window reads them.
fn vitals_of(character: &Character) -> RestVitals {
    RestVitals::new(
        character.health,
        character.max_health,
        character.fatigue,
        character.max_fatigue(),
        character.magicka,
        character.max_magicka(),
    )
}

/// Texts one projected HUD frame prints, in [`HUD_PATHS`] order.
///
/// Each text is the read model's own resolution through
/// [`SessionReadModels::resolve`] — the family the probe prints through
/// `ui_compat::resolve_text` — so the HUD cannot print a value the read model
/// does not carry. An undeclared route is an error, never a default.
fn frame(hud: &HudReadModel) -> Result<Vec<(&'static str, String)>, UiCompatError> {
    let models = SessionReadModels {
        hud: hud.clone(),
        ..SessionReadModels::default()
    };
    let mut texts = Vec::with_capacity(HUD_PATHS.len());
    for path in HUD_PATHS {
        if !is_read_model_path(path) {
            return Err(UiCompatError::Text(RULE_ROUTE));
        }
        let text = models.resolve(path).ok_or(UiCompatError::Text(RULE_ROUTE))?;
        texts.push((path, text));
    }
    Ok(texts)
}

/// Exact signed magnitude of one HUD route between two frames.
///
/// Both texts come from the same numeric read model, so a non-integer text is
/// a contract violation, not a value to coerce.
fn magnitude(before: &str, after: &str) -> Result<i64, UiCompatError> {
    let before: i64 = before.parse().map_err(|_| UiCompatError::Text(RULE_NUMERIC))?;
    let after: i64 = after.parse().map_err(|_| UiCompatError::Text(RULE_NUMERIC))?;
    Ok(after - before)
}

/// Delivers the interactions of block B for one window name.
///
/// One delivery executes the HUD's two intents (plan §2, block B), in order:
/// the **paint** of the current projection, then the **hour** advanced through
/// [`RestWindow`] and priced with the domain's activity fatigue. The frame is
/// then printed, every route that moved since the previous frame is reported
/// as a `Committed` outcome with its exact `before`/`after`, and the HUD line
/// itself is delivered as a `Text` outcome. The tick always paints what it
/// printed, so the same state plus the same intents hand back the same
/// interaction — no delta is invented for a route that did not move.
pub fn interact(
    state: &mut M39State,
    name: &'static str,
) -> Result<Vec<Interaction>, UiCompatError> {
    if name != HUD_WINDOW {
        return Err(UiCompatError::Navigation(RULE_WINDOW));
    }
    // Plan §3.3: the name resolves through the census and must be the handle
    // the block was built for.
    if window_for(name)? != state.b.window {
        return Err(UiCompatError::Navigation(RULE_WINDOW));
    }

    // Intent 1 — paint the frame of the projection as it stands.
    let mut intents = 1usize;
    let mut delivered = 1usize;

    // Intent 2 — advance one hour of world time through the domain's own rest
    // window in `Loiter` mode (`rest.rs`: the clock advances and the window
    // recovers nothing), paying the domain's activity fatigue price for every
    // hour it closed. The HUD's source is the living character, so the frame
    // is re-projected from it — never written into the read model.
    let mut window = RestWindow::new(
        RestMode::Loiter,
        vitals_of(&state.b.character),
        HOURS_PER_ADVANCE,
    );
    let rested = window.run(|_| false);
    let hours = i32::try_from(rested.hours).map_err(|_| UiCompatError::Text(RULE_ADVANCE))?;
    if hours < 1 {
        return Err(UiCompatError::Text(RULE_ADVANCE));
    }
    intents += 1;
    state
        .b
        .character
        .spend_fatigue(ACTIVITY_FATIGUE_LOSS.saturating_mul(hours));
    state.hud = project(&state.b.character);
    delivered += 1;

    let after = frame(&state.hud)?;
    let before = match &state.b.painted {
        Some(hud) => frame(hud)?,
        None => Vec::new(),
    };

    let mut outcomes = Vec::new();
    let mut delta = Vec::new();
    for (index, (path, after_text)) in after.iter().enumerate() {
        let before_text = before
            .get(index)
            .map(|(_, text)| text.as_str())
            .unwrap_or(M39_ABSENT);
        if before_text == after_text.as_str() {
            continue;
        }
        let outcome = ModalOutcome::Committed {
            read_model: path,
            before: before_text.to_string(),
            after: after_text.clone(),
        };
        M39State::validate(&outcome)?;
        // A route that had no datum yet (an unpainted frame) has no numeric
        // magnitude; every painted HUD route is numeric and reports one.
        if before_text != M39_ABSENT {
            let moved = magnitude(before_text, after_text)?;
            delta.push((*path, moved));
        }
        outcomes.push(outcome);
    }
    // The HUD always paints its line (title bar of the window).
    let line = ModalOutcome::Text {
        value: state.hud.summary(),
    };
    M39State::validate(&line)?;
    outcomes.push(line);

    let delivered_now = delivered;
    state.b.tick += 1;
    state.b.intents += intents;
    state.b.delivered += delivered_now;
    for (path, _) in &delta {
        state.b.changed.insert(*path);
    }
    state.b.last_delta = delta.clone();
    state.b.painted = Some(state.hud.clone());

    Ok(vec![Interaction {
        id: state.b.ordinal,
        name,
        block: 'B',
        intents,
        delivered: delivered_now,
        outcomes,
        delta,
        read_only: None,
        digest: String::new(),
    }
    .sealed()])
}

/// Evidence of block B.
///
/// Before the first tick the block is honestly **pending** (plan §7.5: never a
/// false `OK`). After it, `OK` requires the seven routes to be declared, the
/// painted frame to be readable and every reported delta to name a HUD route
/// with a non-zero magnitude — otherwise the evidence carries the citable
/// rule that failed.
pub fn evidence(state: &M39State) -> BlockEvidence {
    let block = &state.b;
    if block.intents == 0 {
        return BlockEvidence::pending('B', BLOCK_B_WINDOWS);
    }

    let routes_declared = HUD_PATHS.iter().all(|path| is_read_model_path(path));
    let frame_readable = block
        .painted
        .as_ref()
        .map(|hud| frame(hud).is_ok())
        .unwrap_or(false);
    let deltas_citable = block
        .last_delta
        .iter()
        .all(|(path, moved)| HUD_PATHS.contains(path) && *moved != 0);
    let deltas_unique = block
        .last_delta
        .iter()
        .enumerate()
        .all(|(index, (path, _))| !block.last_delta[..index].iter().any(|(seen, _)| seen == path));

    let reason = if !routes_declared {
        Some(RULE_ROUTE)
    } else if !frame_readable {
        Some(RULE_FRAME)
    } else if !deltas_citable || !deltas_unique {
        Some(RULE_DELTA)
    } else if block.intents < 2 {
        Some(RULE_INTENTS)
    } else if block.delivered == 0 {
        Some(RULE_UNTICKED)
    } else {
        None
    };

    BlockEvidence {
        block: 'B',
        windows: BLOCK_B_WINDOWS,
        flows: HUD_PATHS.len(),
        intents: block.intents,
        delivered: block.delivered,
        state_delta: block.changed.len(),
        read_only: 0,
        ok: reason.is_none(),
        reason,
    }
}

#[cfg(test)]
mod m39_block_b_tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    use daggerfall_parser_rs::texture::Palette;
    use daggerfall_skills::career::CFG_BYTES;

    use super::*;
    use crate::m39_state::fnv1a64_hex;

    /// Scratch `ARENA2` directory carrying one synthetic `CLASS01.CFG`,
    /// removed on drop.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    static NEXT_SCRATCH: AtomicU32 = AtomicU32::new(0);

    /// 74-byte synthetic career (`CLASS01.CFG` shape) — attributes
    /// `60/50/55/50/50/40/45/50`, `HitPointsPerLevel` 12, advancement ×1.00.
    fn synthetic_career_bytes() -> Vec<u8> {
        let mut bytes = vec![0u8; CFG_BYTES];
        bytes[4..6].copy_from_slice(&0x0C00u16.to_le_bytes()); // SP× 1.5
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

    fn retail_dir() -> Scratch {
        let serial = NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "daggerfall-m39-block-b-{}-{serial}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("scratch arena2");
        std::fs::write(dir.join(M20_CLASS_FILE), synthetic_career_bytes()).expect("career write");
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

    /// The HUD line one interaction delivered.
    fn hud_line(interaction: &Interaction) -> &str {
        interaction
            .outcomes
            .iter()
            .find_map(|outcome| match outcome {
                ModalOutcome::Text { value } => Some(value.as_str()),
                _ => None,
            })
            .expect("the HUD prints its line")
    }

    #[test]
    fn m39_block_b_baseline_is_the_composed_projection() {
        let dir = retail_dir();
        let state = state(&dir);

        // The block derives the session-start HUD the same way the state
        // composes it, so both frames agree before the first tick.
        assert_eq!(state.b.painted(), Some(&state.hud));
        assert_eq!(state.b.window(), window_for(HUD_WINDOW).expect("census"));
        assert_eq!(state.b.window(), WindowId::HUD);
        assert_eq!(state.b.tick(), 0);
        assert!(state.b.last_delta().is_empty());
        assert!(state.b.changed().is_empty());
        // The HUD is not a game value: the projection carries the retail
        // career's own health, so a zeroed HUD would be a fabrication.
        assert_ne!(state.hud, HudReadModel::default());
        // Before the first tick the block is honestly pending (plan §7.5).
        let pending = evidence(&state);
        assert_eq!(pending, BlockEvidence::pending('B', BLOCK_B_WINDOWS));
    }

    #[test]
    fn m39_block_b_prints_the_projected_hud_of_its_tick() {
        let dir = retail_dir();
        let mut state = state(&dir);
        let baseline = state.hud.clone();

        let delivered = interact(&mut state, HUD_WINDOW).expect("tick");
        assert_eq!(delivered.len(), 1);
        let interaction = &delivered[0];
        let projection = state.hud.clone();

        // Grammar of plan §4.1: the id is the census ordinal (plan §1: 038).
        assert_eq!(interaction.id, 38);
        assert_eq!(interaction.name, HUD_WINDOW);
        assert_eq!(interaction.block, 'B');
        // Two intents, both delivered: the paint and the hour.
        assert_eq!(interaction.intents, 2);
        assert_eq!(interaction.delivered, 2);
        assert_eq!(interaction.read_only, None);
        assert_eq!(interaction.digest.len(), 16);
        assert_eq!(interaction.digest, fnv1a64_hex(&interaction.signature()));

        // The painted frame is the projection of the living character — never
        // a display value written by the block.
        assert_eq!(projection, project(&state.b.character));
        assert_eq!(
            projection.stamina,
            state.b.character.fatigue / daggerfall_skills::character::FATIGUE_MULTIPLIER
        );

        // Every value the frame prints is the read model projected in this
        // tick — resolved through the single source the probe prints through.
        let models = SessionReadModels {
            hud: projection.clone(),
            ..SessionReadModels::default()
        };
        assert_eq!(HUD_PATHS.len(), 7);
        let frame_texts = frame(&projection).expect("frame");
        assert_eq!(frame_texts.len(), HUD_PATHS.len());
        for (path, printed) in &frame_texts {
            assert!(is_read_model_path(path), "{path} is not declared");
            assert_eq!(Some(printed.clone()), models.resolve(path), "{path}");
            assert!(!printed.trim().is_empty(), "{path} printed nothing");
            assert!(printed.parse::<i64>().is_ok(), "{path} is not numeric");
            assert_ne!(printed, M39_ABSENT, "{path} printed a marker");
        }
        assert_eq!(
            models.resolve("hud.health").as_deref(),
            Some(projection.health.to_string().as_str())
        );
        assert_eq!(
            models.resolve("hud.stamina_pct").as_deref(),
            Some(projection.stamina_pct.to_string().as_str())
        );

        // The HUD line is the projection's own summary, not a re-formatting.
        assert_eq!(hud_line(interaction), projection.summary());
        for outcome in &interaction.outcomes {
            assert!(M39State::validate(outcome).is_ok(), "{outcome:?}");
        }

        // The hour prices fatigue and nothing else: the stamina routes move
        // down and every other route is untouched, so the delta names exactly
        // the routes that moved, in paint order.
        assert!(projection.stamina < baseline.stamina, "{baseline:?} {projection:?}");
        assert_eq!(projection.health, baseline.health);
        assert_eq!(projection.max_health, baseline.max_health);
        assert_eq!(projection.level, baseline.level);
        let moved: Vec<(&'static str, String)> = frame(&baseline)
            .expect("baseline frame")
            .into_iter()
            .zip(frame(&projection).expect("frame"))
            .filter(|(before, after)| before.1 != after.1)
            .map(|(before, after)| {
                assert_eq!(after.0, before.0);
                (before.0, after.1)
            })
            .collect();
        assert_eq!(
            interaction.delta.iter().map(|(path, _)| *path).collect::<Vec<_>>(),
            moved.iter().map(|(path, _)| *path).collect::<Vec<_>>()
        );
        for (path, after) in &moved {
            let before = frame(&baseline)
                .expect("baseline frame")
                .into_iter()
                .find(|(candidate, _)| candidate == path)
                .expect("route in the baseline frame")
                .1;
            let magnitude = after.parse::<i64>().expect("numeric") - before.parse::<i64>().expect("numeric");
            assert!(
                interaction.delta.contains(&(*path, magnitude)),
                "{path}: {magnitude}"
            );
        }
        // The tick painted exactly what it printed.
        assert_eq!(state.b.painted(), Some(&state.hud));
        assert_eq!(state.b.tick(), 1);
        assert_eq!(state.b.intents, 2);
        assert_eq!(state.b.delivered, 2);
    }

    #[test]
    fn m39_block_b_hour_charges_the_domain_fatigue_price() {
        let dir = retail_dir();
        let mut state = state(&dir);

        // The hour prices exactly the domain's activity fatigue: one hour of
        // `RestWindow` in `Loiter` mode, then `spend_fatigue` of the amount
        // `Character::train_skill` charges for a skill use.
        let before_raw = state.b.character.fatigue;
        let before = state.hud.clone();
        let delivered = interact(&mut state, HUD_WINDOW).expect("hour");
        let interaction = &delivered[0];
        assert_eq!(before_raw - state.b.character.fatigue, ACTIVITY_FATIGUE_LOSS);

        // The reported delta is the exact before/after of the routes that
        // moved — nothing else is reported, and each `Committed` carries the
        // read model's own two texts.
        let projection = state.hud.clone();
        let committed: Vec<(&str, String, String)> = interaction
            .outcomes
            .iter()
            .filter_map(|outcome| match outcome {
                ModalOutcome::Committed {
                    read_model,
                    before,
                    after,
                } => Some((*read_model, before.clone(), after.clone())),
                _ => None,
            })
            .collect();
        assert!(!committed.is_empty(), "the hour moved no route");
        assert_eq!(committed.len(), interaction.delta.len());
        let baseline = SessionReadModels {
            hud: before.clone(),
            ..SessionReadModels::default()
        };
        let current = SessionReadModels {
            hud: projection.clone(),
            ..SessionReadModels::default()
        };
        for (index, (path, before_text, after_text)) in committed.iter().enumerate() {
            assert_eq!(Some(before_text.clone()), baseline.resolve(path), "{path}");
            assert_eq!(Some(after_text.clone()), current.resolve(path), "{path}");
            assert_ne!(before_text, after_text, "{path}");
            assert_eq!(interaction.delta[index].0, *path);
        }
        for outcome in &interaction.outcomes {
            assert!(M39State::validate(outcome).is_ok(), "{outcome:?}");
        }
        assert_eq!(hud_line(interaction), projection.summary());

        // A second hour charges the same domain price again (the fixture is
        // far from empty) and the evidence keeps counting real routes.
        let second_raw = state.b.character.fatigue;
        interact(&mut state, HUD_WINDOW).expect("second hour");
        assert_eq!(second_raw - state.b.character.fatigue, ACTIVITY_FATIGUE_LOSS);
        assert_eq!(state.hud, project(&state.b.character));

        // Evidence counts the routes that really moved.
        let rows = evidence(&state);
        assert!(rows.ok, "{}", rows.reason.unwrap_or("-"));
        assert_eq!(rows.block, 'B');
        assert_eq!(rows.windows, BLOCK_B_WINDOWS);
        assert_eq!(rows.flows, HUD_PATHS.len());
        assert_eq!(rows.intents, 4);
        assert_eq!(rows.delivered, 4);
        assert_eq!(rows.state_delta, interaction.delta.len());
        assert!(rows.state_delta > 0);
        assert_eq!(rows.read_only, 0);
        assert_eq!(rows.reason, None);
    }

    #[test]
    fn m39_block_b_same_state_same_intents_same_interaction() {
        let dir = retail_dir();
        let mut first_state = state(&dir);
        let mut second_state = state(&dir);

        let first = interact(&mut first_state, HUD_WINDOW).expect("first");
        let second = interact(&mut second_state, HUD_WINDOW).expect("second");
        assert_eq!(first, second);
        assert_eq!(first[0].digest, second[0].digest);

        // The same state plus the same intents hands back the same digest.
        let first_after = interact(&mut first_state, HUD_WINDOW).expect("first hour");
        let second_after = interact(&mut second_state, HUD_WINDOW).expect("second hour");
        assert_eq!(first_after, second_after);
        assert_eq!(first_after[0].digest, second_after[0].digest);
        // The second hour charges the same domain price, so the same routes
        // move (the integer percentages drift by a point, which is the
        // read model's own rounding, not a different delta) — but the
        // interaction as a whole (counters, before/after texts) moved, so the
        // digest cannot be the same.
        assert_eq!(
            first_after[0].delta.iter().map(|(path, _)| *path).collect::<Vec<_>>(),
            first[0].delta.iter().map(|(path, _)| *path).collect::<Vec<_>>()
        );
        assert!(first_after[0].delta.iter().all(|(_, moved)| *moved < 0));
        assert_ne!(first_after[0].digest, first[0].digest);
        assert_eq!(first_after[0].intents, 2);
        assert_eq!(first_after[0].delivered, 2);

        // Two states that took a different number of hours differ: the digest
        // is a function of the reported fields, never a constant.
        let mut third_state = state(&dir);
        let third = interact(&mut third_state, HUD_WINDOW).expect("third");
        assert_ne!(third[0].digest, first_after[0].digest);
        assert_ne!(third[0].outcomes, first_after[0].outcomes);
    }

    #[test]
    fn m39_block_b_delivers_only_the_hud() {
        let dir = retail_dir();
        let mut state = state(&dir);

        // Another census window (a sibling block's) is refused, not defaulted.
        let other = interact(&mut state, "DaggerfallInventoryWindow");
        assert!(matches!(other, Err(UiCompatError::Navigation(RULE_WINDOW))));
        // A name the census does not register is refused too.
        let unknown = interact(&mut state, "DaggerfallNotAWindow");
        assert!(matches!(unknown, Err(UiCompatError::Navigation(RULE_WINDOW))));
        // Neither refusal ticked the block.
        assert_eq!(state.b.tick(), 0);
        assert_eq!(state.b.intents, 0);
        assert_eq!(evidence(&state), BlockEvidence::pending('B', BLOCK_B_WINDOWS));
    }

    #[test]
    fn m39_block_b_evidence_is_honest_about_its_routes() {
        let dir = retail_dir();
        let mut state = state(&dir);
        interact(&mut state, HUD_WINDOW).expect("tick");

        // Every route the block prints is declared by the registry, and the
        // census resolves the window and its ordinal.
        for path in HUD_PATHS {
            assert!(is_read_model_path(path), "{path}");
        }
        assert_eq!(ordinal_for(HUD_WINDOW), Ok(38));
        assert_eq!(ordinal_for("DaggerfallNotAWindow"), Err(UiCompatError::Navigation(RULE_CENSUS)));

        // A delta of magnitude zero can never be reported (plan §7.3).
        let mut zeroed = state.clone();
        zeroed.b.last_delta = vec![("hud.health", 0)];
        assert_eq!(evidence(&zeroed).reason, Some(RULE_DELTA));
        // A non-HUD route in the delta is not citable either.
        let mut foreign = state.clone();
        foreign.b.last_delta = vec![("m39.inv.gold", 5)];
        assert_eq!(evidence(&foreign).reason, Some(RULE_DELTA));
        // A frame that never painted is a hard failure.
        let mut unpainted = state.clone();
        unpainted.b.painted = None;
        assert_eq!(evidence(&unpainted).reason, Some(RULE_FRAME));
        // The delivered block is OK with real counts: two intents per tick and
        // a state delta above zero (the hour priced its fatigue).
        let rows = evidence(&state);
        assert!(rows.ok);
        assert_eq!(rows.reason, None);
        assert_eq!(rows.windows, 1);
        assert_eq!(rows.flows, 7);
        assert_eq!(rows.intents, 2);
        assert_eq!(rows.delivered, 2);
        assert!(rows.state_delta > 0);
        assert_eq!(rows.state_delta, state.b.changed().len());
    }
}

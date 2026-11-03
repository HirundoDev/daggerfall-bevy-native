//! **M-QST · main-quest trigger route** (`CR-9b3e5d70`): the
//! seam between the pure state ([`daggerfall_behavior::mainquest`]), the
//! shell (evidence queue, clock, session), the v3 save and the keys.
//!
//! ## What it is and what it is NOT
//!
//! There are **no** new rules here: the legal table lives in the organ
//! (`mainquest::apply`, 7 edges) and this module only
//!
//! 1. fires the offer **once**, on entry to the world
//!    ([`offer`], called by [`crate::shell_route::enter_playing`] after
//!    `state = Playing` and **before** the missing-scene early return: with
//!    `scene = None` the headless pass sees it too);
//! 2. translates keys into actions ([`QuestKey`], [`press`]) and the closing of
//!    the objective ([`on_defeat`], called from `m14_melee_sfx` when the engine
//!    reports `target_died`);
//! 3. persists and rehydrates the state as **one text row** in
//!    `QuestsSection::states` (`MQ-01.state=<token>`), the shape the v3 save
//!    already models: **0 version bump, 0 migration, 0 new section**.
//!
//! ## Key mapping (declared, 4 keys for 4 actions)
//!
//! | Key | Action | Where it is legal |
//! |:---:|:---|:---|
//! | `Y` | `Accept` | only `Offered` |
//! | `N` | `Decline` | only `Offered` |
//! | `P` | `Complete` (deliver the proof) | only `Act1Return` |
//! | `X` | `Abandon` | `Act1Hunt` · `Act1Return` |
//!
//! The inherited HH-1 fixes the **set** of keys (`Y`/`N`/`P`/`X`); the
//! split of `P` (deliver) and `X` (abandon) is this lane's reading:
//! delivery needs a key, and `X` is the abandonment of the two edges the
//! table declares. An illegal key in the current state is **not swallowed**:
//! it publishes `refused` with the requested pair and does not move the state.
//!
//! ## Text
//!
//! The name and objectives of `MQ-01` are **invented** in this lane
//! (0 retail bytes): the quest contract does not exist in Daggerfall's
//! data and this tree copies nobody's prose.
//!
//! ## Visible prompt
//!
//! `prompt_lines()` is the only text: the window composes it with the journal's
//! `.FNT` ([`prompt_overlay`]) and paints it without the player pressing `I`.
//! The prompt is a function of the state (offer = title + `Y`/`N`; act = objective +
//! keys; terminal = one line) and `unstarted` paints nothing. The
//! `PROBE-MAINQUEST-002` probe measures that surface in pixels, not in promises.
//!
//! ## Evidence
//!
//! Own channel `MQ VE:` (it does not collide with `SHELL VE:`/`M18 VE:` nor with the
//! shapes the gates already capture, so it does not alter their stable spine):
//! one line per applied transition and one per refusal, in queue order.
//! After accepting, act 1 is **1 death + delivery**: `compose_demo_fight`
//! composes one enemy per fight, and two deaths would leave it uncompletable
//! outside the probe.

use std::path::Path;

use daggerfall_behavior::mainquest::{self, Action, Refusal, State};
use daggerfall_memory::savegame_v2::SaveGameV2;
use daggerfall_parser_rs::image::FntFile;

use crate::m18_demo::DemoTexture;
use crate::m18_phase::AppPhase;
use crate::shell::{self, HudFacts, Shell, ShellState};

/// Evidence prefix of the quest (own channel).
pub const PREFIX: &str = "MQ VE:";

/// Invented title of `MQ-01` (0 retail bytes — see the module docs).
pub const TITLE: &str = "The court's summons";

/// Keys with meaning for the quest, already translated by the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestKey {
    /// `Y`: aceptar la oferta.
    Accept,
    /// `N`: rechazar la oferta.
    Decline,
    /// `P`: deliver the act 1 proof.
    Deliver,
    /// `X`: abandonar el acto.
    Abandon,
}

impl QuestKey {
    /// Table action this key requests (it does not decide whether it is legal:
    /// `mainquest::apply` decides that).
    pub const fn action(self) -> Action {
        match self {
            Self::Accept => Action::Accept,
            Self::Decline => Action::Decline,
            Self::Deliver => Action::Complete,
            Self::Abandon => Action::Abandon,
        }
    }
}

/// Quest state in the session + how many offers were emitted in it.
///
/// The state has **no** setter: [`Self::apply`] is the single door and delegates
/// to the organ's table. The offer counter is the instrument of the
/// "the offer does not repeat" invariant: only [`Self::apply`] with
/// [`mainquest::Action::Offer`] can increment it, and that edge only exists
/// from `Unstarted`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MainQuest {
    state: State,
    offers: u32,
}

impl MainQuest {
    /// Sesión nueva: nadie ha ofrecido la quest.
    pub fn new() -> Self {
        Self {
            state: State::Unstarted,
            offers: 0,
        }
    }

    /// Session rehydrated from a save (`state` comes in already validated).
    ///
    /// The counter starts at 0: the previous session's offers cannot be
    /// re-emitted because the persisted state is no longer `Unstarted`, and a
    /// counter that were inherited would claim this session offered something it
    /// did not offer.
    pub fn restored(state: State) -> Self {
        Self { state, offers: 0 }
    }

    /// Estado actual (lectura).
    pub const fn state(&self) -> State {
        self.state
    }

    /// Offers emitted in this session (instrument of the invariant).
    pub const fn offers(&self) -> u32 {
        self.offers
    }

    /// **The single door**: applies the action if the table declares it and returns
    /// the new state; otherwise [`Refusal`] and the state is left untouched.
    pub fn apply(&mut self, action: Action) -> Result<State, Refusal> {
        let next = mainquest::apply(self.state, action)?;
        self.state = next;
        if action == Action::Offer {
            self.offers += 1;
        }
        Ok(next)
    }
}

impl Default for MainQuest {
    fn default() -> Self {
        Self::new()
    }
}

// ── líneas de evidencia ─────────────────────────────────────────────────────

/// Readable note of each applied edge (the `from=/action=/state=` block is
/// what the probe and the human read).
fn note(action: Action) -> &'static str {
    match action {
        Action::Offer => "Y accept / N decline",
        Action::Accept => "act 1: hunt the target · X abandon",
        Action::Decline => "declined, the offer will not repeat",
        Action::Slay => "target down · P deliver / X abandon",
        Action::Complete => "act 1 delivered",
        Action::Abandon => "abandoned",
    }
}

/// `MQ VE: quest MQ-01 from=<s> action=<a> state=<s> · <nota>`
pub fn transition_line(from: State, action: Action, to: State) -> String {
    if action == Action::Offer {
        return format!(
            "{PREFIX} quest {} offered title=\"{TITLE}\" from={} action={} state={} · {}",
            mainquest::QUEST_ID,
            from.token(),
            action.token(),
            to.token(),
            note(action)
        );
    }
    format!(
        "{PREFIX} quest {} from={} action={} state={} · {}",
        mainquest::QUEST_ID,
        from.token(),
        action.token(),
        to.token(),
        note(action)
    )
}

/// `MQ VE: quest MQ-01 refused action=<a> from=<s> (legal: ...)`
pub fn refusal_line(refusal: &Refusal) -> String {
    let legal: Vec<&str> = mainquest::legal_actions(refusal.from)
        .map(Action::token)
        .collect();
    let legal = if legal.is_empty() {
        "none".to_string()
    } else {
        legal.join(", ")
    };
    format!(
        "{PREFIX} quest {} refused action={} from={} (legal from here: {legal})",
        mainquest::QUEST_ID,
        refusal.action.token(),
        refusal.from.token()
    )
}

// ── disparador y teclas ─────────────────────────────────────────────────────

/// Fires the offer **once** (only reachable from `Unstarted`).
///
/// Called by [`crate::shell_route::enter_playing`] after setting `state = Playing`
/// and before the missing-scene early return. Returns `true` if the offer
/// was emitted: a second call in the same session cannot (the state left
/// `Unstarted`), and neither can a rehydrated session.
pub fn offer(shell: &mut Shell) -> bool {
    let from = shell.mainquest.state();
    match shell.mainquest.apply(Action::Offer) {
        Ok(to) => {
            shell.say(0.0, transition_line(from, Action::Offer, to));
            true
        }
        // The table has no other edge from `Unstarted`: an `Err` here is
        // "already offered" (or terminal), silence by design, not a failure.
        Err(_) => false,
    }
}

/// Applies a key's action and publishes a transition **or** a refusal.
/// Returns `true` if the table accepted it.
pub fn press(shell: &mut Shell, key: QuestKey) -> bool {
    apply_action(shell, key.action())
}

/// The act 1 objective died (`target_died` from the engine): `Act1Hunt →
/// Act1Return`. A second death in `Act1Return` is published as a refusal and
/// does not advance the act.
pub fn on_defeat(shell: &mut Shell) -> bool {
    apply_action(shell, Action::Slay)
}

/// Single route for the consumer's actions: door + evidence.
fn apply_action(shell: &mut Shell, action: Action) -> bool {
    let from = shell.mainquest.state();
    match shell.mainquest.apply(action) {
        Ok(to) => {
            shell.say(0.0, transition_line(from, action, to));
            true
        }
        Err(refusal) => {
            shell.say(0.0, refusal_line(&refusal));
            false
        }
    }
}

// ── persistencia (0 bump, 0 migración) ──────────────────────────────────────

/// Writes `MQ-01.state=<token>` into `QuestsSection::states`.
///
/// Idempotent: it removes the previous row (if any) and reinserts it in
/// key order, the order the canonical document demands. A session that never
/// fired the trigger (`Unstarted`) does **not** write a row: there is no state
/// to restore, and a document without a quest stays byte for byte as it was
/// before this lane.
pub fn persist(doc: &mut SaveGameV2, state: State) {
    doc.quests
        .states
        .retain(|(key, _)| key != mainquest::STATE_KEY);
    if state == State::Unstarted {
        return;
    }
    doc.quests.states.push((
        mainquest::STATE_KEY.to_string(),
        state.token().to_string(),
    ));
    doc.quests
        .states
        .sort_by(|left, right| left.0.cmp(&right.0));
}

/// Rehydrates the state from the document.
///
/// Missing row ⇒ `Unstarted` (a save from before this lane is a session that
/// never received the offer). Unknown token ⇒ `Unstarted`: it is declared,
/// not guessed, and the load is not aborted for a future-version row.
pub fn restore(doc: &SaveGameV2) -> State {
    doc.quests
        .states
        .iter()
        .find(|(key, _)| key == mainquest::STATE_KEY)
        .and_then(|(_, value)| State::from_token(value))
        .unwrap_or_default()
}

/// Rehydrates the session from the document **before** entering the world, so
/// that [`crate::shell_route::enter_playing`]'s trigger sees the real state
/// and does not offer again.
pub fn restore_into(shell: &mut Shell, doc: &SaveGameV2) {
    shell.mainquest = MainQuest::restored(restore(doc));
}

// ── prompt on screen (the VISIBLE quest) ────────────────────────────────────
//
// M-QST measured the state machine, the probe and the save; nothing reached
// the player because the offer only existed as a channel `MQ VE:` line.
// This section paints the prompt through the **path that already exists** (the
// journal's retail `.FNT` and `m18_b3b::text_overlay`), with no `bevy_ui`, no new
// font and no new UI system: one more texture, composed from the state.

/// Prompt scale over the 2D camera: the same 2× the retail overlays
/// declare. It lives here so the window and the probe measure the SAME region.
pub const PROMPT_SCALE: f32 = 2.0;

/// Prompt margin to the window edge (the one of the retail overlays).
pub const PROMPT_MARGIN_PX: f32 = 8.0;

/// Reference screen of the probe: the size the product window declares
/// (1280×720). Only the probe uses it; the window measures its own.
pub const PROBE_SCREEN: (u32, u32) = (1280, 720);

/// Prompt lines for the current state. Empty ⇒ there is nothing to paint
/// (`unstarted`: a world that never received the offer shows no prompt).
///
/// The `MQ-01` text is **ours** (see the module docs); the keys each line
/// announces are the ones the legal table accepts from that state, so
/// the prompt cannot promise an action `apply()` would refuse.
pub fn prompt_lines(state: State) -> Vec<String> {
    let id = mainquest::QUEST_ID;
    match state {
        State::Unstarted => Vec::new(),
        State::Offered => vec![format!("{id} · {TITLE}"), "Y accept / N decline".to_string()],
        State::Act1Hunt => vec![format!("{id} · act 1: hunt the target"), "X abandon".to_string()],
        State::Act1Return => vec![
            format!("{id} · act 1: deliver the proof"),
            "P deliver · X abandon".to_string(),
        ],
        State::Act1Done => vec![format!("{id} complete · act 1 delivered")],
        State::Declined => vec![format!("{id} declined · the offer will not repeat")],
        State::Abandoned => vec![format!("{id} abandoned · the act is closed")],
    }
}

/// Prompt texture. `None` when the state paints nothing (`unstarted`).
///
/// The composition is the journal's (`text_overlay`): same font, same
/// ink, same stacking — 0 antialiasing and 0 pixel outside the ink.
pub fn prompt_overlay(font: &FntFile, ink: [u8; 3], state: State) -> Option<DemoTexture> {
    let lines = prompt_lines(state);
    if lines.is_empty() {
        return None;
    }
    Some(crate::m18_b3b::text_overlay(font, ink, &lines))
}

/// Prompt centre in the 2D camera (origin at the centre, `+y` up):
/// top-right corner, the gap neither the dialogue (top-left) nor
/// the journal (bottom-centre) occupies.
pub fn prompt_center(screen_w: f32, screen_h: f32, size: (u16, u16), scale: f32) -> (f32, f32) {
    let width = f32::from(size.0) * scale;
    let height = f32::from(size.1) * scale;
    (
        screen_w * 0.5 - PROMPT_MARGIN_PX - width * 0.5,
        screen_h * 0.5 - PROMPT_MARGIN_PX - height * 0.5,
    )
}

/// The same position seen from the frame: prompt rectangle in screen
/// pixels, origin top-left (`x0, y0, w, h`) and already scaled.
pub fn prompt_rect(screen: (u32, u32), size: (u16, u16), scale: f32) -> (u32, u32, u32, u32) {
    let (center_x, center_y) = prompt_center(screen.0 as f32, screen.1 as f32, size, scale);
    let width = (f32::from(size.0) * scale).round() as u32;
    let height = (f32::from(size.1) * scale).round() as u32;
    let pixel_x = (screen.0 as f32 * 0.5 + center_x).round() as u32;
    let pixel_y = (screen.1 as f32 * 0.5 - center_y).round() as u32;
    (
        pixel_x.saturating_sub(width / 2),
        pixel_y.saturating_sub(height / 2),
        width,
        height,
    )
}

/// Ink pixels of the texture: opaque (the painter only writes ink with
/// alpha 255).
pub fn ink_pixels(texture: &DemoTexture) -> usize {
    texture.rgba.chunks_exact(4).filter(|px| px[3] == 255).count()
}

/// `(outside_ink, partial_alpha)`: both must be 0 — the prompt paints
/// ink and nothing else.
pub fn off_ink_pixels(texture: &DemoTexture, ink: [u8; 3]) -> (usize, usize) {
    let mut off_ink = 0usize;
    let mut partial = 0usize;
    for pixel in texture.rgba.chunks_exact(4) {
        match pixel[3] {
            0 => {}
            255 if pixel[..3] == ink[..] => {}
            255 => off_ink += 1,
            _ => partial += 1,
        }
    }
    (off_ink, partial)
}

/// FNV-1a 64 digest of the texture (dimensions + RGBA): two different
/// paintings give different digests and the same painting twice gives the same.
pub fn texture_digest(texture: &DemoTexture) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in texture
        .width
        .to_le_bytes()
        .iter()
        .chain(texture.height.to_le_bytes().iter())
        .chain(texture.rgba.iter())
    {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

// ── PROBE-MAINQUEST-001 (headless, --features probes) ───────────────────────

/// Verdict rows of the probe (one per assertion, with its measured facts).
#[derive(Default)]
struct Rows {
    fails: usize,
}

impl Rows {
    fn check(&mut self, id: &str, ok: bool, facts: &str) {
        if ok {
            println!("[OK] {id} — {facts}");
        } else {
            self.fails += 1;
            println!("[FAIL] {id} — {facts}");
        }
    }
}

/// Probe directory: `temp/dfl-mainquest-<pid>` (deleted at the end).
fn probe_dir() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("dfl-mainquest-{}", std::process::id()))
}

/// Headless shell: no scene, no measured HUD, no retail location (the
/// fields that are not measured stay declared absent, not filled in).
fn fresh_shell() -> Shell {
    Shell::new(
        Vec::new(),
        HudFacts {
            panel: crate::m18_phase::HUD_PANEL_ID.to_string(),
            opaque: 0,
            digest: 0,
        },
        None,
        [0, 0],
        Vec::new(),
        None,
    )
}

/// New game through the **production route** (`create_begin` →
/// `create_next_step` → `confirm_create`), which is what enters `Playing` and
/// therefore what fires the offer. Returns the character token.
fn new_game(shell: &mut Shell, phase: &mut AppPhase, dir: &Path) -> Result<String, String> {
    shell.state = ShellState::Menu {
        selection: shell::MENU_NEW_GAME,
    };
    crate::shell_route::create_begin(shell);
    crate::shell_route::create_next_step(shell);
    let slots_present = crate::shell_slots::scan_in(dir).count();
    crate::shell_route::confirm_create(shell, phase, None, slots_present)
        .ok_or_else(|| "create: the wizard was not open".to_string())
}

/// Publishes the queue already prepared (one line per transition) and returns how many.
fn publish(shell: &mut Shell) -> usize {
    shell.drain()
}

/// `PROBE-MAINQUEST-001` — trigger + act 1 + persistence, headless.
///
/// Declared walkthrough: new game → offer → accept → real engine death →
/// delivery → save → load → state preserved and **no re-offer**.
/// Then the three falsifiers with their measurement, not with their promise.
pub fn run_mainquest_001() -> i32 {
    println!(
        "PROBE-MAINQUEST-001 — M-QST main quest trigger: new game → offer → accept → slay → \
         deliver → save → load"
    );
    println!(
        "PROBE-MAINQUEST-001 map Y=accept N=decline P=deliver X=abandon · window: none · \
         DISPLAY: not required"
    );
    let dir = probe_dir();
    if dir.exists() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    if let Err(err) = std::fs::create_dir_all(&dir) {
        println!("PROBE-MAINQUEST-001 SETUP-FAIL — {}: {err}", dir.display());
        return 1;
    }
    let mut rows = Rows::default();
    let result = probe_body(&dir, &mut rows);
    let _ = std::fs::remove_dir_all(&dir);
    if let Err(err) = result {
        rows.check("body", false, &err);
    }
    if rows.fails == 0 {
        println!("VERDICT: PROBE-MAINQUEST-001 PASS");
        0
    } else {
        println!("VERDICT: PROBE-MAINQUEST-001 FAIL ({} checks failed)", rows.fails);
        1
    }
}

fn probe_body(dir: &Path, rows: &mut Rows) -> Result<(), String> {
    // ── A: nueva partida → oferta (una sola vez) ────────────────────────────
    let mut shell = fresh_shell();
    let mut phase = AppPhase::Menu;
    let character = new_game(&mut shell, &mut phase, dir)?;
    publish(&mut shell);
    rows.check(
        "A1 enter Playing",
        phase == AppPhase::Playing && shell.state == ShellState::Playing,
        &format!(
            "character={character} phase={} state={:?}",
            phase.label(),
            shell.state
        ),
    );
    rows.check(
        "A2 offer fired once from Unstarted",
        shell.mainquest.state() == State::Offered && shell.mainquest.offers() == 1,
        &format!(
            "state={} offers={}",
            shell.mainquest.state().token(),
            shell.mainquest.offers()
        ),
    );
    // The second call of the same hook cannot offer: the state is no longer
    // `Unstarted`.
    let offered_again = offer(&mut shell);
    publish(&mut shell);
    rows.check(
        "A3 the trigger does not repeat itself",
        !offered_again && shell.mainquest.offers() == 1,
        &format!(
            "second_call_emitted={offered_again} offers={}",
            shell.mainquest.offers()
        ),
    );

    // ── B: aceptar por tecla ────────────────────────────────────────────────
    let accepted = press(&mut shell, QuestKey::Accept);
    publish(&mut shell);
    rows.check(
        "B1 Y accepts: Offered → Act1Hunt",
        accepted && shell.mainquest.state() == State::Act1Hunt,
        &format!(
            "accepted={accepted} state={}",
            shell.mainquest.state().token()
        ),
    );
    // `N` stops making sense once accepted: it is not swallowed, it is published.
    let declined_after_accept = press(&mut shell, QuestKey::Decline);
    publish(&mut shell);
    rows.check(
        "B2 N after accepting is refused, state untouched",
        !declined_after_accept && shell.mainquest.state() == State::Act1Hunt,
        &format!(
            "emitted={declined_after_accept} state={}",
            shell.mainquest.state().token()
        ),
    );

    // ── C: REAL engine death (not a fake `apply`) ────────────────────────────
    let (strikes, died) = strike_until_dead();
    rows.check(
        "C1 the engine killed the target",
        died,
        &format!("engine=CombatSession::player_strike strikes={strikes} died={died}"),
    );
    let defeated = on_defeat(&mut shell);
    publish(&mut shell);
    rows.check(
        "C2 target_died → Act1Hunt → Act1Return",
        defeated && shell.mainquest.state() == State::Act1Return,
        &format!(
            "emitted={defeated} state={}",
            shell.mainquest.state().token()
        ),
    );
    // A second death does not advance the act (the edge does not exist).
    let second = on_defeat(&mut shell);
    publish(&mut shell);
    rows.check(
        "C3 a second death is refused (no Act1Done shortcut)",
        !second && shell.mainquest.state() == State::Act1Return,
        &format!(
            "emitted={second} state={}",
            shell.mainquest.state().token()
        ),
    );

    // ── D: entrega ──────────────────────────────────────────────────────────
    let delivered = press(&mut shell, QuestKey::Deliver);
    publish(&mut shell);
    rows.check(
        "D1 P delivers: Act1Return → Act1Done",
        delivered && shell.mainquest.state() == State::Act1Done,
        &format!(
            "emitted={delivered} state={}",
            shell.mainquest.state().token()
        ),
    );

    // ── E: guardar → cargar (formato intacto) ───────────────────────────────
    let (slot, bytes, digest) =
        crate::shell_route::write_slot(&mut shell, dir).map_err(|err| format!("save: {err}"))?;
    publish(&mut shell);
    let doc = crate::shell_slots::read_in(dir, slot).map_err(|err| format!("load: {err}"))?;
    let restored = restore(&doc);
    let row_state = doc
        .quests
        .states
        .iter()
        .find(|(key, _)| key == mainquest::STATE_KEY)
        .map(|(_, value)| value.as_str())
        .unwrap_or("-");
    let header = doc
        .to_text()
        .lines()
        .next()
        .unwrap_or_default()
        .to_string();
    rows.check(
        "E1 save keeps the v3 header and one MQ-01.state row",
        header == "DFL-SESSION 3"
            && doc.quests.states.len() == 1
            && row_state == State::Act1Done.token(),
        &format!(
            "slot={slot} bytes={bytes} digest={digest:016x} header={header:?} states={} row={}",
            doc.quests.states.len(),
            row_state
        ),
    );
    rows.check(
        "E2 load restores Act1Done",
        restored == State::Act1Done,
        &format!(
            "persisted={row_state} restored={}",
            restored.token()
        ),
    );
    // The real load: rehydrate and enter the world through the same hook.
    let mut loaded = fresh_shell();
    restore_into(&mut loaded, &doc);
    let mut loaded_phase = AppPhase::Menu;
    crate::shell_route::enter_playing(&mut loaded, &mut loaded_phase, None, false);
    publish(&mut loaded);
    let reoffered = offer(&mut loaded);
    publish(&mut loaded);
    let offers_total = shell.mainquest.offers() + loaded.mainquest.offers();
    rows.check(
        "E3 the loaded session keeps the state and never re-offers",
        loaded.mainquest.state() == State::Act1Done
            && !reoffered
            && loaded.mainquest.offers() == 0
            && offers_total == 1,
        &format!(
            "state={} reoffered={reoffered} session_offers={} offers_total={offers_total}",
            loaded.mainquest.state().token(),
            loaded.mainquest.offers()
        ),
    );

    // ── F: falsificador (b) — guardar en Act1Hunt y cargar ──────────────────
    let mut mid = fresh_shell();
    let mut mid_phase = AppPhase::Menu;
    new_game(&mut mid, &mut mid_phase, dir)?;
    press(&mut mid, QuestKey::Accept);
    publish(&mut mid);
    let mid_slot = crate::shell_route::write_slot(&mut mid, dir)
        .map_err(|err| format!("save mid-act: {err}"))?
        .0;
    let mid_doc =
        crate::shell_slots::read_in(dir, mid_slot).map_err(|err| format!("load mid-act: {err}"))?;
    let mut mid_loaded = fresh_shell();
    restore_into(&mut mid_loaded, &mid_doc);
    let mut mid_loaded_phase = AppPhase::Menu;
    crate::shell_route::enter_playing(&mut mid_loaded, &mut mid_loaded_phase, None, false);
    publish(&mut mid_loaded);
    let mid_reoffered = offer(&mut mid_loaded);
    publish(&mut mid_loaded);
    let mid_total = mid.mainquest.offers() + mid_loaded.mainquest.offers();
    rows.check(
        "F1 falsifier(b) saved in Act1Hunt, loaded in Act1Hunt, 1 offer total",
        mid.mainquest.state() == State::Act1Hunt
            && mid_loaded.mainquest.state() == State::Act1Hunt
            && !mid_reoffered
            && mid_total == 1,
        &format!(
            "before_save={} after_load={} reoffered={mid_reoffered} offers_total={mid_total}",
            mid.mainquest.state().token(),
            mid_loaded.mainquest.state().token()
        ),
    );

    // ── G: falsifiers (a) and (c) over the pure table ──────────────────────
    let offered_complete = mainquest::apply(State::Offered, Action::Complete);
    let offered_reaches_done = Action::ALL
        .into_iter()
        .any(|action| mainquest::apply(State::Offered, action) == Ok(State::Act1Done));
    rows.check(
        "G1 falsifier(a) Offered --Complete--> is refused, never Act1Done",
        matches!(
            offered_complete,
            Err(Refusal {
                from: State::Offered,
                action: Action::Complete
            })
        ) && !offered_reaches_done,
        &format!(
            "apply(offered, complete)={offered_complete:?} any_action_reaches_done={offered_reaches_done}"
        ),
    );
    let declined_slay = mainquest::apply(State::Declined, Action::Slay);
    let declined_accept = mainquest::apply(State::Declined, Action::Accept);
    rows.check(
        "G2 falsifier(c) Declined is not accepted: Slay/Accept refused",
        declined_slay.is_err() && declined_accept.is_err(),
        &format!(
            "apply(declined, slay)={declined_slay:?} apply(declined, accept)={declined_accept:?}"
        ),
    );
    // The same lesson with the live session: refuse and then try to close.
    let mut decliner = fresh_shell();
    let mut decliner_phase = AppPhase::Menu;
    new_game(&mut decliner, &mut decliner_phase, dir)?;
    press(&mut decliner, QuestKey::Decline);
    let slay_after_decline = on_defeat(&mut decliner);
    let deliver_after_decline = press(&mut decliner, QuestKey::Deliver);
    publish(&mut decliner);
    rows.check(
        "G3 a declined offer cannot complete the act",
        !slay_after_decline
            && !deliver_after_decline
            && decliner.mainquest.state() == State::Declined,
        &format!(
            "slay_emitted={slay_after_decline} deliver_emitted={deliver_after_decline} state={}",
            decliner.mainquest.state().token()
        ),
    );

    // ── H: la tabla completa (7 legales, 35 rechazos) ───────────────────────
    let mut accepted = 0usize;
    let mut refused = 0usize;
    for from in State::ALL {
        for action in Action::ALL {
            if mainquest::apply(from, action).is_ok() {
                accepted += 1;
            } else {
                refused += 1;
            }
        }
    }
    rows.check(
        "H1 the legal table is exactly 7 of 42 pairs",
        accepted == 7 && refused == 35,
        &format!("pairs=42 legal={accepted} refused={refused}"),
    );

    // ── I: abandonment, the other exit edge ───────────────────────────────
    let mut abandoner = fresh_shell();
    let mut abandoner_phase = AppPhase::Menu;
    new_game(&mut abandoner, &mut abandoner_phase, dir)?;
    press(&mut abandoner, QuestKey::Accept);
    let abandoned = press(&mut abandoner, QuestKey::Abandon);
    publish(&mut abandoner);
    rows.check(
        "I1 X abandons from Act1Hunt (terminal)",
        abandoned
            && abandoner.mainquest.state() == State::Abandoned
            && State::Abandoned.is_terminal(),
        &format!(
            "emitted={abandoned} state={} terminal={}",
            abandoner.mainquest.state().token(),
            State::Abandoned.is_terminal()
        ),
    );
    // Abandonment is persisted too (a terminal does not reopen on load).
    let mut doc = SaveGameV2::new();
    persist(&mut doc, abandoner.mainquest.state());
    let restored_abandon = restore(&doc);
    rows.check(
        "I2 Abandoned survives the save/load round trip",
        restored_abandon == State::Abandoned,
        &format!(
            "persisted={} restored={}",
            abandoner.mainquest.state().token(),
            restored_abandon.token()
        ),
    );

    Ok(())
}

/// Real death through the production engine: a `CombatSession` with the player
/// unarmed and a class enemy, struck until it falls. No retail tables and
/// no window: the session is the same one `m14_melee_sfx` uses.
fn strike_until_dead() -> (usize, bool) {
    use daggerfall_combat::{Combatant, DeathSequence, EntityKind};

    use daggerfall_app::combat_runtime::{CombatSession, Fighter};

    let mut player = Fighter::new(
        Combatant::new(1, EntityKind::Player),
        DeathSequence::for_player_height(1.7),
    );
    // The manual strike is the production one; the modifiers only keep the
    // lottery of 200 rolls from turning the probe into a luck test.
    player.chance_mod = 100;
    player.damage_mods = 300;
    let enemy = Fighter::new(
        Combatant::new(9_001, EntityKind::EnemyClass),
        DeathSequence::for_enemy(),
    );
    let mut session = CombatSession::new(player, vec![enemy], 0x4D51_0001);
    let mut strikes = 0usize;
    while session
        .enemies
        .first()
        .is_some_and(|enemy| enemy.is_alive())
        && strikes < 200
    {
        session.player_strike(0);
        strikes += 1;
    }
    let died = session.enemies.first().is_some_and(|enemy| !enemy.is_alive());
    (strikes, died)
}

// ── PROBE-MAINQUEST-002 (headless, --features probes) ───────────────────────

/// The seven states of the legal table, in order: the probe walks them all
/// (one of them, `unstarted`, must **not** paint anything).
fn paint_states() -> [State; 7] {
    [
        State::Unstarted,
        State::Offered,
        State::Act1Hunt,
        State::Act1Return,
        State::Act1Done,
        State::Declined,
        State::Abandoned,
    ]
}

/// Composes the probe frame: screen [`PROBE_SCREEN`] (RGBA, origin
/// top-left) with the state's prompt pasted into its rectangle and
/// scaled with nearest sampling, which is what the window's 2× sprite
/// does. Returns the frame and the occupied rectangle (`None` = it painted nothing).
fn probe_frame(
    font: &FntFile,
    ink: [u8; 3],
    state: State,
) -> (Vec<u8>, Option<(u32, u32, u32, u32)>) {
    let (screen_w, screen_h) = PROBE_SCREEN;
    let mut frame = vec![0u8; screen_w as usize * screen_h as usize * 4];
    let Some(texture) = prompt_overlay(font, ink, state) else {
        return (frame, None);
    };
    let rect = prompt_rect(PROBE_SCREEN, (texture.width, texture.height), PROMPT_SCALE);
    let (x0, y0, width, height) = rect;
    let scale = PROMPT_SCALE as u32;
    let stride = usize::from(texture.width);
    for row in 0..height {
        let source_y = row / scale;
        let target_y = y0 + row;
        if target_y >= screen_h {
            continue;
        }
        for col in 0..width {
            let target_x = x0 + col;
            if target_x >= screen_w {
                continue;
            }
            let source = (source_y as usize * stride + (col / scale) as usize) * 4;
            if texture.rgba[source + 3] != 255 {
                continue;
            }
            let target = (target_y as usize * screen_w as usize + target_x as usize) * 4;
            frame[target..target + 4].copy_from_slice(&texture.rgba[source..source + 4]);
        }
    }
    (frame, Some(rect))
}

/// Ink pixels of the frame: `(inside the rectangle, outside the rectangle)`.
/// Ink = opaque and of the ink colour (the only thing the painter writes).
fn count_ink(
    frame: &[u8],
    ink: [u8; 3],
    rect: Option<(u32, u32, u32, u32)>,
) -> (usize, usize) {
    let width = PROBE_SCREEN.0 as usize;
    let height = PROBE_SCREEN.1 as usize;
    let mut inside = 0usize;
    let mut outside = 0usize;
    for y in 0..height {
        for x in 0..width {
            let at = (y * width + x) * 4;
            if frame[at + 3] != 255 || frame[at..at + 3] != ink[..] {
                continue;
            }
            let in_rect = rect.is_some_and(|(x0, y0, w, h)| {
                let (px, py) = (x as u32, y as u32);
                px >= x0 && px < x0 + w && py >= y0 && py < y0 + h
            });
            if in_rect {
                inside += 1;
            } else {
                outside += 1;
            }
        }
    }
    (inside, outside)
}

/// `PROBE-MAINQUEST-002` — the quest prompt, measured in pixels.
///
/// Composes the frame of **each** state with the retail font the journal
/// already loads and counts the ink inside the region the window declares
/// (top-right corner). `unstarted` paints nothing: 0 pixels. The
/// falsifier forces the state without repainting: if two states gave the same
/// texture, the dependency row would go red — and that red is shown here.
pub fn run_mainquest_002() -> i32 {
    println!("PROBE-MAINQUEST-002 — M-QST quest prompt painted per state (ink pixels, not promises)");
    println!(
        "PROBE-MAINQUEST-002 frame {}×{} · prompt at the top-right corner (margin {} px, scale {}×) · window: none · DISPLAY: not required",
        PROBE_SCREEN.0, PROBE_SCREEN.1, PROMPT_MARGIN_PX, PROMPT_SCALE
    );
    let dir = match crate::mode_a_arena2_dir() {
        Ok(dir) => dir,
        Err(reason) => {
            println!("ARENA2 not accessible: {reason}");
            println!("VERDICT: PROBE-MAINQUEST-002 NO DATA - ARENA2 not accessible");
            return 2;
        }
    };
    let font = match crate::ui_assets::load_ui_assets(&dir)
        .map_err(|err| err.to_string())
        .and_then(|assets| {
            assets
                .font(daggerfall_parser_bevy_bridge::ui_spec::DEFAULT_FONT)
                .cloned()
                .ok_or_else(|| "FONT0000.FNT missing".to_string())
        }) {
        Ok(font) => font,
        Err(reason) => {
            println!("PROBE-MAINQUEST-002 SETUP-FAIL — {reason}");
            println!("VERDICT: PROBE-MAINQUEST-002 NO DATA - font not loaded");
            return 2;
        }
    };
    let ink = [255u8, 255, 255];
    let mut rows = Rows::default();

    // ── A · one frame per state, ink INSIDE the overlay region ──────────────
    let mut digests: Vec<(State, u64)> = Vec::new();
    let mut off_ink_total = 0usize;
    let mut partial_total = 0usize;
    for (index, state) in paint_states().iter().copied().enumerate() {
        let id = index + 1;
        let (frame, rect) = probe_frame(&font, ink, state);
        let (inside, outside) = count_ink(&frame, ink, rect);
        match rect {
            None => {
                rows.check(
                    &format!("A{id} {} → nothing painted", state.token()),
                    inside == 0 && outside == 0,
                    &format!(
                        "frame {}×{} · {inside} ink px in the overlay region · {outside} outside · region: none (no texture composed)",
                        PROBE_SCREEN.0, PROBE_SCREEN.1
                    ),
                );
            }
            Some((x0, y0, width, height)) => {
                let texture = prompt_overlay(&font, ink, state)
                    .ok_or_else(|| state.token().to_string());
                let (texture, texture_ink, off_ink, partial, digest) = match texture {
                    Ok(texture) => {
                        let (off_ink, partial) = off_ink_pixels(&texture, ink);
                        (
                            (texture.width, texture.height),
                            ink_pixels(&texture),
                            off_ink,
                            partial,
                            texture_digest(&texture),
                        )
                    }
                    Err(token) => {
                        rows.check(&format!("A{id} {token} → texture"), false, "no texture");
                        continue;
                    }
                };
                off_ink_total += off_ink;
                partial_total += partial;
                digests.push((state, digest));
                rows.check(
                    &format!("A{id} {} → painted", state.token()),
                    inside > 0 && outside == 0,
                    &format!(
                        "region ({x0},{y0}) {width}×{height} px · {inside} ink px in the region · {outside} outside · texture {}×{} px {texture_ink} ink · off-ink {off_ink} · partial {partial} · digest {digest:016x}",
                        texture.0, texture.1
                    ),
                );
            }
        }
    }

    // ── B · the falsifier: the painter READS the state (or the probe falls) ──
    let digest_of = |state: State| -> Option<u64> {
        digests
            .iter()
            .find(|(painted, _)| *painted == state)
            .map(|(_, digest)| *digest)
    };
    let offered = digest_of(State::Offered);
    let hunt = digest_of(State::Act1Hunt);
    let unique = {
        let mut seen: Vec<u64> = digests.iter().map(|(_, digest)| *digest).collect();
        seen.sort_unstable();
        seen.dedup();
        seen.len()
    };
    rows.check(
        "B1 state-dependence — the six painted states have six distinct digests",
        unique == 6 && offered != hunt,
        &format!(
            "distinct={unique} · offered={:016x} · act1-hunt={:016x} · equal={}",
            offered.unwrap_or(0),
            hunt.unwrap_or(0),
            offered == hunt
        ),
    );
    // Falsifier: a frozen painter returns the `offered` texture even though
    // the state is already `act1-hunt` (that is the real defect: caching the texture
    // and not looking at the state). The state is forced WITHOUT repainting and the
    // **same predicate of B1** is evaluated over that painter: if it does not go red,
    // the falsifier does not bite.
    let stale = prompt_overlay(&font, ink, State::Offered).map(|texture| texture_digest(&texture));
    let real = prompt_overlay(&font, ink, State::Act1Hunt).map(|texture| texture_digest(&texture));
    let frozen_paint = |_state: State| prompt_overlay(&font, ink, State::Offered);
    let frozen_ok = match (
        frozen_paint(State::Offered).as_ref(),
        frozen_paint(State::Act1Hunt).as_ref(),
    ) {
        (Some(offered), Some(hunt)) => texture_digest(offered) != texture_digest(hunt),
        _ => false,
    };
    println!(
        "[RED] B2-would-be FAIL — B1 state-dependence under a painter that ignores the state: forced act1-hunt without repaint keeps the `offered` texture (stale={:016x} vs real act1-hunt={:016x}) → the SAME predicate reads distinct={frozen_ok}, so that painter turns B1 red (this line is the demonstration, not a verdict)",
        stale.unwrap_or(0),
        real.unwrap_or(0)
    );
    rows.check(
        "B2 the falsifier bites — a painter that ignores the state is caught",
        !frozen_ok && stale.is_some() && real.is_some(),
        &format!(
            "frozen_painter_distinct={frozen_ok} (must be false) · stale={:016x} · real={:016x} · real painter distinct={}",
            stale.unwrap_or(0),
            real.unwrap_or(0),
            stale != real
        ),
    );

    // ── C · determinismo ×2 y tinta pura ───────────────────────────────────
    let mut drift = 0usize;
    for (state, digest) in &digests {
        let again = prompt_overlay(&font, ink, *state).map(|texture| texture_digest(&texture));
        if again != Some(*digest) {
            drift += 1;
        }
    }
    rows.check(
        "C1 determinism ×2 — the same state paints the same bytes",
        drift == 0,
        &format!("states checked={} · drifting={drift}", digests.len()),
    );
    rows.check(
        "C2 ink only — the prompt paints ink and nothing else",
        off_ink_total == 0 && partial_total == 0,
        &format!(
            "off-ink px={off_ink_total} · partial-alpha px={partial_total} across {} painted states",
            digests.len()
        ),
    );

    if rows.fails == 0 {
        println!("VERDICT: PROBE-MAINQUEST-002 PASS");
        0
    } else {
        println!(
            "VERDICT: PROBE-MAINQUEST-002 FAIL ({} checks failed)",
            rows.fails
        );
        1
    }
}

#[cfg(test)]
mod prompt_tests {
    use super::*;

    #[test]
    fn prompt_is_silent_before_the_offer_and_speaks_every_other_state() {
        assert!(
            prompt_lines(State::Unstarted).is_empty(),
            "unstarted must paint nothing"
        );
        let mut seen: Vec<Vec<String>> = Vec::new();
        for state in paint_states() {
            let lines = prompt_lines(state);
            if state == State::Unstarted {
                continue;
            }
            assert!(!lines.is_empty(), "{} paints nothing", state.token());
            if !seen.contains(&lines) {
                seen.push(lines);
            }
        }
        assert_eq!(seen.len(), 6, "six painted states, six distinct prompts");
    }

    #[test]
    fn offer_announces_both_keys_and_the_act_announces_abandon() {
        let offered = prompt_lines(State::Offered);
        assert!(
            offered
                .iter()
                .any(|line| line.contains("accept") && line.contains("decline")),
            "{offered:?}"
        );
        assert!(offered[0].contains(TITLE), "{offered:?}");
        let hunt = prompt_lines(State::Act1Hunt);
        assert!(hunt.iter().any(|line| line.contains("hunt")), "{hunt:?}");
        assert!(
            hunt.iter()
                .any(|line| line.contains('X') && line.contains("abandon")),
            "{hunt:?}"
        );
        let deliver = prompt_lines(State::Act1Return);
        assert!(
            deliver.iter().any(|line| line.contains("deliver")),
            "{deliver:?}"
        );
        assert!(
            deliver
                .iter()
                .any(|line| line.contains('X') && line.contains("abandon")),
            "{deliver:?}"
        );
        assert_ne!(offered, hunt, "the painter must not ignore the state");
    }

    #[test]
    fn prompt_region_sits_in_the_top_right_corner_inside_the_frame() {
        let rect = prompt_rect(PROBE_SCREEN, (200, 40), PROMPT_SCALE);
        assert_eq!(PROBE_SCREEN.0 - (rect.0 + rect.2), PROMPT_MARGIN_PX as u32);
        assert_eq!(rect.1, PROMPT_MARGIN_PX as u32);
        assert_eq!(rect.2, 400);
        assert_eq!(rect.3, 80);
        assert!(rect.0 + rect.2 <= PROBE_SCREEN.0);
        assert!(rect.1 + rect.3 <= PROBE_SCREEN.1);
    }
}

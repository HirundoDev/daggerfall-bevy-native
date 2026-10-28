//! M39/UI-3 — shared state of the 51 `Playable` windows (plan §4, frozen).
//!
//! `M39State` **composes** the per-block state: each block owns its own struct
//! (`AState`, `BState`, `CdState`, `EfState`, `GhState`) and this module owns
//! only what is common — the session read models ([`M39Models`]), the HUD
//! projection, and the outcome / delta vocabulary of an interaction.
//!
//! **Frozen interface (plan §4).** [`ModalOutcome`], [`Interaction`] and
//! [`BlockEvidence`] are consumed verbatim by the probe grammar of §4.1 and by
//! the five block modules: renaming a field or a variant breaks the
//! `UI-JUGABLE-001` gate, so the olas 2–3 must extend the *states*, never
//! this vocabulary.
//!
//! **Provenance (plan §3.1: zero invention).** The only state built here is
//!
//! * `hud` — the HUD projection of the session-start character: the retail
//!   career [`M20_CLASS_FILE`] parsed by `daggerfall_skills`, the [`M19_SEED`]
//!   seed and level 1 (the same `Character::starting` call the integrated demo
//!   composes). Every painted number is therefore a function of the retail
//!   bytes and the domain formula — no literal game value.
//! * `models` — [`M39Models::default`] with the session flag set: every route
//!   starts **absent** ([`daggerfall_perception::read_models::M39_ABSENT`]),
//!   and each block fills only its own block's routes.
//!
//! **Determinism.** An interaction can change a read model only through
//! [`M39State::commit_i64`] / [`M39State::commit_text`]; both report the exact
//! `before`/`after` text and refuse a commit that leaves the model unchanged,
//! so the same state plus the same intents always hand back the same
//! [`Interaction`] and the same digest.

use std::path::Path;

use daggerfall_nervous::events::WindowId;
use daggerfall_parser_bevy_bridge::ui_compat::UiCompatError;
use daggerfall_parser_bevy_bridge::ui_spec::UiAssets;
use daggerfall_perception::read_models::{HudReadModel, M39Models};
use daggerfall_skills::career::Career;
use daggerfall_skills::character::Character;
use daggerfall_skills::rng::Rng64;
use sha2::{Digest, Sha256};

use crate::m18_demo::{M19_SEED, M20_CLASS_FILE};
use crate::m39_block_a::AState;
use crate::m39_block_b::BState;
use crate::m39_block_cd::CdState;
use crate::m39_block_ef::EfState;
use crate::m39_block_gh::GhState;

/// Rule string: the route is not declared by [`M39Models`].
const RULE_UNDECLARED: &str = "m39: undeclared read-model route";
/// Rule string: the commit was refused because the route's type does not take
/// the value (or the value does not fit it).
const RULE_TYPE: &str = "m39: route refuses this value (type or range)";
/// Rule string: the commit would leave the read model unchanged.
const RULE_UNCHANGED: &str = "m39: commit leaves the read model unchanged";
/// Rule string: a reported commit whose `before` equals its `after`.
const RULE_EQUAL_COMMIT: &str = "m39: committed outcome with before == after";
/// Rule string: an `Interact`-level outcome without a citable reason.
const RULE_NO_REASON: &str = "m39: read-only/rejected requires a citable reason";
/// Rule string: the session-start career (`CLASS01.CFG`) cannot be read.
const RULE_CAREER_READ: &str = "m39: session career CLASS01.CFG unreadable";
/// Rule string: the session-start career (`CLASS01.CFG`) is not a career.
const RULE_CAREER_PARSE: &str = "m39: session career CLASS01.CFG rejected";

/// One delivered intent outcome (plan §4, frozen).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModalOutcome {
    /// The intent changed one read-model route; `before` and `after` are the
    /// resolved texts and are never equal (plan §7.3).
    Committed {
        /// Route that changed (`M39Models::paths()` member).
        read_model: &'static str,
        /// Resolved text before the intent.
        before: String,
        /// Resolved text after the intent.
        after: String,
    },
    /// The surface declares no delta with a citable reason (plan §3.4).
    ReadOnly {
        /// Why the surface cannot change state (`assets=-` note of the TSV).
        reason: &'static str,
    },
    /// The intent was abandoned by the caller; nothing changed.
    Cancelled,
    /// The intent was refused by a domain rule; nothing changed.
    Rejected {
        /// Citable domain reason.
        reason: &'static str,
    },
    /// The intent produced text (input box, message, answer).
    Text {
        /// The produced text.
        value: String,
    },
    /// The intent opened another window (resolved by name, plan §3.3).
    Opened {
        /// Window that the intent opened.
        window: WindowId,
    },
    /// The intent closed the surface.
    Closed,
}

/// One interactive surface delivery, as printed by the `INTERACT` line of
/// plan §4.1 (frozen by §4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interaction {
    /// Census ordinal (`022`…`082` as a number, plan §4.1).
    pub id: u16,
    /// Census window name (`Daggerfall*Window`).
    pub name: &'static str,
    /// Block letter of plan §2 (`A`…`H`).
    pub block: char,
    /// Intents attempted by the surface.
    pub intents: usize,
    /// Intents delivered with an outcome.
    pub delivered: usize,
    /// Outcomes, in delivery order.
    pub outcomes: Vec<ModalOutcome>,
    /// Read models that changed, sorted, with their signed delta.
    pub delta: Vec<(&'static str, i64)>,
    /// `Some(reason)` when the surface is read-only (plan §3.4).
    pub read_only: Option<&'static str>,
    /// `fnv1a64` of [`Interaction::signature`], 16 hex (plan §4.1).
    pub digest: String,
}

impl Interaction {
    /// Seals the digest of this interaction (plan §4.1: `digest=<16hex>`).
    ///
    /// The digest is `fnv1a64` of [`Interaction::signature`] — the ordered
    /// field signature, never a previously sealed digest — so the same state
    /// plus the same intents always yields the same `Interaction` and the same
    /// digest.
    pub fn sealed(mut self) -> Self {
        self.digest = fnv1a64_hex(&self.signature());
        self
    }

    /// Canonical signature of every reported field, in grammar order.
    pub fn signature(&self) -> String {
        let mut out = format!(
            "{}|{}|{}|{}|{}|",
            self.id, self.name, self.block, self.intents, self.delivered
        );
        for outcome in &self.outcomes {
            out.push_str(&outcome_signature(outcome));
            out.push(';');
        }
        out.push('|');
        for (path, delta) in &self.delta {
            out.push_str(&format!("{path}:{delta:+};"));
        }
        out.push('|');
        match self.read_only {
            Some(reason) => out.push_str(reason),
            None => out.push('-'),
        }
        out
    }
}

/// Syntax of one outcome inside [`Interaction::signature`].
fn outcome_signature(outcome: &ModalOutcome) -> String {
    match outcome {
        ModalOutcome::Committed {
            read_model,
            before,
            after,
        } => format!("C:{read_model}:{before}>{after}"),
        ModalOutcome::ReadOnly { reason } => format!("R:{reason}"),
        ModalOutcome::Cancelled => "X".to_string(),
        ModalOutcome::Rejected { reason } => format!("J:{reason}"),
        ModalOutcome::Text { value } => format!("T:{value}"),
        ModalOutcome::Opened { window } => format!("O:{window:?}"),
        ModalOutcome::Closed => "E".to_string(),
    }
}

/// Evidence of one UI block, as printed by the `BLOCK <L> OK|FAIL` line of
/// plan §4.1 (frozen by §4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockEvidence {
    /// Block letter (`A`…`H`).
    pub block: char,
    /// Windows of the block that the probe exercised.
    pub windows: usize,
    /// Flows declared by the block.
    pub flows: usize,
    /// Intents attempted.
    pub intents: usize,
    /// Intents delivered.
    pub delivered: usize,
    /// Read models whose value changed.
    pub state_delta: usize,
    /// Read-only surfaces declared with a citable reason.
    pub read_only: usize,
    /// `true` only when the block met every gate of §4.1.
    pub ok: bool,
    /// `Some(rule)` when `ok = false`; `None` on `OK`.
    pub reason: Option<&'static str>,
}

impl BlockEvidence {
    /// Evidence of a block whose lane has not landed yet.
    ///
    /// `ok = false` with a citable `reason`, so the probe prints
    /// `BLOCK <L> FAIL reason=pending` instead of a false `OK` (plan §7.5).
    pub fn pending(block: char, windows: usize) -> Self {
        Self {
            block,
            windows,
            flows: 0,
            intents: 0,
            delivered: 0,
            state_delta: 0,
            read_only: 0,
            ok: false,
            reason: Some("pending"),
        }
    }
}

/// Shared state of the 51 `Playable` windows (plan §4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct M39State {
    /// Whether a live UI session backs this state; mirrors `models.session`
    /// (the single source of truth for route resolution).
    pub session: bool,
    /// HUD projection of the session-start character.
    pub hud: HudReadModel,
    /// The 96 `m39.*` read-model routes.
    pub models: M39Models,
    /// Block A state (owner: ola 2/3).
    pub a: AState,
    /// Block B state (owner: ola 2/3).
    pub b: BState,
    /// Blocks C + D state (owner: ola 2/3).
    pub cd: CdState,
    /// Blocks E + F state (owner: ola 2/3).
    pub ef: EfState,
    /// Blocks G + H state (owner: ola 3).
    pub gh: GhState,
}

impl M39State {
    /// Builds the shared state of a live M39 UI session.
    ///
    /// `assets` is the retail UI bank and `arena2` the `ARENA2` directory, both
    /// already resolved by Mode A; every block receives the pair and fills its
    /// own `XState` from them ([`AState::from_retail`] and siblings). The
    /// constructor is deterministic: two calls over the same bank and
    /// directory hand back equal state.
    pub fn new(assets: &UiAssets, arena2: &Path) -> Result<Self, UiCompatError> {
        let hud = Self::hud_from_retail(arena2)?;
        let models = M39Models {
            session: true,
            ..M39Models::default()
        };
        Ok(Self {
            session: models.session,
            hud,
            models,
            a: AState::from_retail(assets, arena2)?,
            b: BState::from_retail(assets, arena2)?,
            cd: CdState::from_retail(assets, arena2)?,
            ef: EfState::from_retail(assets, arena2)?,
            gh: GhState::from_retail(assets, arena2)?,
        })
    }

    /// HUD projection of the session-start character.
    ///
    /// The character is the app's canonical session start — retail career
    /// [`M20_CLASS_FILE`], the [`M19_SEED`] seed and level 1, through the
    /// domain's own `Character::starting` — projected through
    /// [`HudReadModel::project`]. The HUD therefore carries the character's
    /// health/stamina/level and nothing invented.
    fn hud_from_retail(arena2: &Path) -> Result<HudReadModel, UiCompatError> {
        let bytes = std::fs::read(arena2.join(M20_CLASS_FILE))
            .map_err(|_| UiCompatError::Text(RULE_CAREER_READ))?;
        let career =
            Career::parse(&bytes).map_err(|_| UiCompatError::Text(RULE_CAREER_PARSE))?;
        let character = Character::starting(career, &mut Rng64::new(M19_SEED), 1);
        Ok(HudReadModel::project(&crate::m7_readmodels::hud_source_of(
            &character.rpg_stats(),
        )))
    }

    /// Resolves one declared read-model route to its text.
    ///
    /// An undeclared route is an error — never a default (plan §3.3); a
    /// declared route without a datum resolves to
    /// [`daggerfall_perception::read_models::M39_ABSENT`], never to an empty
    /// string or an invented zero.
    pub fn value(&self, path: &str) -> Result<String, UiCompatError> {
        self.models
            .resolve(path)
            .ok_or(UiCompatError::Text(RULE_UNDECLARED))
    }

    /// Commits `new` into one `i64`-shaped route and reports the exact delta.
    ///
    /// Returns [`ModalOutcome::Committed`] only when the resolved text actually
    /// changed: a commit that leaves the model untouched is an error (plan
    /// §7.3 falsifier), never a silent no-op. A value the route's own type
    /// cannot hold is refused without clamping (plan §3.6).
    pub fn commit_i64(
        &mut self,
        path: &'static str,
        new: i64,
    ) -> Result<ModalOutcome, UiCompatError> {
        let before = self.value(path)?;
        if !self.models.set_i64(path, new) {
            return Err(UiCompatError::Input(RULE_TYPE));
        }
        let after = self.value(path)?;
        if before == after {
            return Err(UiCompatError::Input(RULE_UNCHANGED));
        }
        Ok(ModalOutcome::Committed {
            read_model: path,
            before,
            after,
        })
    }

    /// Commits `new` into one text-shaped route and reports the exact delta.
    ///
    /// Same contract as [`M39State::commit_i64`]: an unchanged text is an
    /// error, an empty text is refused by the route itself.
    pub fn commit_text(
        &mut self,
        path: &'static str,
        new: &str,
    ) -> Result<ModalOutcome, UiCompatError> {
        let before = self.value(path)?;
        if !self.models.set_text(path, new) {
            return Err(UiCompatError::Input(RULE_TYPE));
        }
        let after = self.value(path)?;
        if before == after {
            return Err(UiCompatError::Input(RULE_UNCHANGED));
        }
        Ok(ModalOutcome::Committed {
            read_model: path,
            before,
            after,
        })
    }

    /// A rejected intent with its citable reason.
    ///
    /// [`M39State::validate`] rejects an empty reason: the probe cannot print
    /// `read-only=`/`reason=` with nothing behind it.
    pub fn rejected(reason: &'static str) -> ModalOutcome {
        ModalOutcome::Rejected { reason }
    }

    /// A read-only surface with its citable reason (plan §3.4).
    ///
    /// [`M39State::validate`] rejects an empty reason.
    pub fn read_only(reason: &'static str) -> ModalOutcome {
        ModalOutcome::ReadOnly { reason }
    }

    /// Rejects an outcome that cannot be reported (plan §7.3).
    ///
    /// * `Committed` with `before == after` — no delta would be visible.
    /// * `ReadOnly` / `Rejected` with an empty reason — not citable.
    pub fn validate(outcome: &ModalOutcome) -> Result<(), UiCompatError> {
        match outcome {
            ModalOutcome::Committed { before, after, .. } if before == after => {
                Err(UiCompatError::Input(RULE_EQUAL_COMMIT))
            }
            ModalOutcome::ReadOnly { reason } | ModalOutcome::Rejected { reason }
                if reason.is_empty() =>
            {
                Err(UiCompatError::Input(RULE_NO_REASON))
            }
            _ => Ok(()),
        }
    }

    /// SHA-256 (hex) of an ordered block of lines — one `\n` per line.
    ///
    /// This is the `DIGEST` of plan §4.1: no path, no timestamp and no
    /// constant takes part, so two runs over the same state agree.
    pub fn digest_of(&self, lines: &[String]) -> String {
        let mut hasher = Sha256::new();
        for line in lines {
            hasher.update(line.as_bytes());
            hasher.update(b"\n");
        }
        format!("{:x}", hasher.finalize())
    }

    /// The eight block evidences `A`…`H` of plan §4.1, in partition order.
    ///
    /// Plan §4 freezes `evidence(state) -> BlockEvidence` per module (five
    /// modules, one letter each) while §4.1 prints one `BLOCK <L>` line for
    /// each of the eight blocks. The five modules therefore report their two
    /// blocks as `evidence` + `evidence_<second>`, and this composer is the
    /// single place that orders the eight lines.
    pub fn evidences(&self) -> [BlockEvidence; 8] {
        [
            crate::m39_block_a::evidence(self),
            crate::m39_block_b::evidence(self),
            crate::m39_block_cd::evidence(self),
            crate::m39_block_cd::evidence_d(self),
            crate::m39_block_ef::evidence(self),
            crate::m39_block_ef::evidence_f(self),
            crate::m39_block_gh::evidence(self),
            crate::m39_block_gh::evidence_g(self),
        ]
    }
}

/// `fnv1a64` of `text`, 16 hex digits (the per-surface digest of plan §4.1).
pub fn fnv1a64_hex(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod m39_state_tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    use daggerfall_parser_rs::texture::Palette;
    use daggerfall_skills::career::CFG_BYTES;

    use super::*;

    /// Scratch `ARENA2` directory carrying one synthetic `CLASS01.CFG`,
    /// removed on drop.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    static NEXT_SCRATCH: AtomicU32 = AtomicU32::new(0);

    /// 74-byte synthetic career (`CLASS01.CFG` shape) — the fixture the
    /// `daggerfall-skills` tests parse: attributes `60/50/55/50/50/40/45/50`,
    /// `HitPointsPerLevel` 12, advancement ×1.00.
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
            "daggerfall-m39-state-{}-{serial}",
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

    #[test]
    fn m39_state_new_is_deterministic() {
        let dir = retail_dir();
        let assets = empty_assets();
        let first = M39State::new(&assets, &dir.0).expect("first construction");
        let second = M39State::new(&assets, &dir.0).expect("second construction");

        assert_eq!(first, second);
        assert_eq!(first.hud, second.hud);
        assert_eq!(first.models, second.models);
        assert_eq!(first.session, second.session);
        // The HUD is a projection of the retail career, not the empty default.
        assert_ne!(first.hud, HudReadModel::default());
        for path in M39Models::paths() {
            assert_eq!(first.value(path), second.value(path), "{path}");
        }

        // A fresh state may not claim any block OK, and every failed row must
        // carry a citable reason (post-landing invariant of olas 2–3).
        let evidences = first.evidences();
        let letters: Vec<char> = evidences.iter().map(|row| row.block).collect();
        assert_eq!(letters, vec!['A', 'B', 'C', 'D', 'E', 'F', 'G', 'H']);
        let windows: usize = evidences.iter().map(|row| row.windows).sum();
        assert_eq!(windows, 51);
        assert!(
            evidences.iter().all(|row| !row.ok),
            "a fresh state must not claim any block OK: {evidences:?}"
        );
        assert!(
            evidences.iter().all(|row| row.reason.is_some()),
            "a failed block row must carry a citable reason"
        );
    }

    #[test]
    fn m39_commit_reports_the_delta_and_refuses_an_unchanged_model() {
        let dir = retail_dir();
        let mut state = state(&dir);
        state.models.a.menu_depth = Some(2);

        match state.commit_i64("m39.menu.depth", 3).expect("commit") {
            ModalOutcome::Committed {
                read_model,
                before,
                after,
            } => {
                assert_eq!(read_model, "m39.menu.depth");
                assert_eq!(before, "2");
                assert_eq!(after, "3");
            }
            other => panic!("expected Committed, got {other:?}"),
        }
        // Same value again: the model would not move, so the commit is an
        // error (never a silent no-op).
        assert!(state.commit_i64("m39.menu.depth", 3).is_err());
        // Out of the route's range, and the wrong shape: refused, not clamped.
        assert!(state.commit_i64("m39.menu.depth", 256).is_err());
        assert!(state.commit_i64("m39.menu.selected.name", 1).is_err());
        // Undeclared route.
        assert!(state.commit_i64("m39.nope", 1).is_err());

        // Text commit on a text route.
        match state
            .commit_text("m39.menu.selected.name", "DaggerfallStartWindow")
            .expect("text commit")
        {
            ModalOutcome::Committed { before, after, .. } => {
                assert_eq!(before, "ABSENT");
                assert_eq!(after, "DaggerfallStartWindow");
            }
            other => panic!("expected Committed, got {other:?}"),
        }
        assert!(state
            .commit_text("m39.menu.selected.name", "DaggerfallStartWindow")
            .is_err());
        assert!(state.commit_text("m39.menu.selected.name", "").is_err());
    }

    #[test]
    fn m39_outcome_falsifiers_reject_equal_commits_and_empty_reasons() {
        let equal = ModalOutcome::Committed {
            read_model: "m39.menu.depth",
            before: "2".to_string(),
            after: "2".to_string(),
        };
        assert!(M39State::validate(&equal).is_err());
        let moved = ModalOutcome::Committed {
            read_model: "m39.menu.depth",
            before: "2".to_string(),
            after: "3".to_string(),
        };
        assert!(M39State::validate(&moved).is_ok());

        assert!(M39State::validate(&M39State::read_only("")).is_err());
        assert!(M39State::validate(&M39State::rejected("")).is_err());
        assert!(M39State::validate(&M39State::read_only("assets=- (no retail raster)")).is_ok());
        assert!(M39State::validate(&M39State::rejected("not enough gold")).is_ok());
        assert!(M39State::validate(&ModalOutcome::Cancelled).is_ok());
        assert!(M39State::validate(&ModalOutcome::Closed).is_ok());
    }

    #[test]
    fn m39_same_state_and_intents_hand_back_the_same_interaction_and_digest() {
        let dir = retail_dir();
        let mut first_state = state(&dir);
        let mut second_state = state(&dir);
        first_state.models.c.inv_gold = Some(120);
        second_state.models.c.inv_gold = Some(120);

        let deliver = |state: &mut M39State| -> Interaction {
            let outcome = state.commit_i64("m39.inv.gold", 90).expect("commit");
            Interaction {
                id: 40,
                name: "DaggerfallInventoryWindow",
                block: 'C',
                intents: 1,
                delivered: 1,
                outcomes: vec![outcome],
                delta: vec![("m39.inv.gold", -30)],
                read_only: None,
                digest: String::new(),
            }
            .sealed()
        };

        let first = deliver(&mut first_state);
        let second = deliver(&mut second_state);
        assert_eq!(first, second);
        assert_eq!(first.digest.len(), 16);
        assert_eq!(first.digest, fnv1a64_hex(&first.signature()));
        // The digest is a function of the reported fields, not of a constant.
        let mut other = first.clone();
        other.delivered = 2;
        assert_ne!(other.sealed().digest, first.digest);
    }

    #[test]
    fn m39_digest_of_lines_is_stable_and_order_sensitive() {
        let dir = retail_dir();
        let state = state(&dir);
        let lines = vec![
            "SURFACE 040 id=DaggerfallInventoryWindow block=C".to_string(),
            "INTERACT 040 id=DaggerfallInventoryWindow block=C".to_string(),
        ];
        let digest = state.digest_of(&lines);
        assert_eq!(digest.len(), 64);
        assert_eq!(digest, state.digest_of(&lines));
        let mut reversed = lines.clone();
        reversed.reverse();
        assert_ne!(digest, state.digest_of(&reversed));
        assert_ne!(digest, state.digest_of(&lines[..1]));
    }
}

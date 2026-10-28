//! A4 — text axis: every string comes from a declared source (plan §3.4).
//!
//! **The rule:** a rendered string is one of three things and nothing else —
//! a `TEXT.RSC` record ([`TextSource::Rsc`]), a **declared** read-model path
//! ([`TextSource::ReadModel`], the declared paths of [`READ_MODEL_PATHS`]) or a
//! literal compiled into the layer ([`TextSource::Literal`]). There is no
//! fallback: an unknown path is an error, not the path echoed back; a missing
//! record is an error, not an empty string; and an empty result is an error,
//! not an invisible run.
//!
//! **Why `rsc_file`:** `TextRsc` holds the record *table*; the bytes of a
//! record live in the file behind it (`record_text(file, id)`). [`TextEnv`]
//! therefore carries both, and a table without its bytes is refused rather
//! than silently treated as empty.

use daggerfall_parser_rs::text_rsc::TextRsc;
use daggerfall_perception::read_models::SessionReadModels;

use super::error::{UiCompatError, RULE_NO_ENV, RULE_NO_RSC_BYTES};

/// The read-model paths the layer is allowed to print.
///
/// Frozen data: the HUD and the character panel of `perception::read_models`
/// (the ten M37 paths, byte for byte) plus the two M38/UI-2 families —
/// `createchar.*` (36) and `config.*` (11) — and the seven M39/UI-3 families
/// `m39.*` (A21 C14 D16 E12 F10 G11 H12) — for **153** declared paths.
/// The 57 pre-M39 paths keep their order and content byte for byte.
/// A path outside this list is [`UiCompatError::unknown_path`] — adding one is
/// a contract change, not a string in a caller.
pub const READ_MODEL_PATHS: [&str; 153] = [
    // M37 — session HUD and panel (10).
    "hud.health",
    "hud.max_health",
    "hud.stamina",
    "hud.max_stamina",
    "hud.level",
    "hud.health_pct",
    "hud.stamina_pct",
    "panel.gold",
    "panel.item_count",
    "panel.quest_count",
    // M38/UI-2 — character creator (36).
    "createchar.name.value",
    "createchar.name.max_len",
    "createchar.gender.name",
    "createchar.race.name",
    "createchar.race.count",
    "createchar.class.name",
    "createchar.class.count",
    "createchar.face.index",
    "createchar.face.count",
    "createchar.level",
    "createchar.bonus_remaining",
    "createchar.stats_remaining",
    "createchar.stat.strength",
    "createchar.stat.agility",
    "createchar.skill_slot1",
    "createchar.skill_slot2",
    "createchar.skill_slot3",
    "createchar.biography.body",
    "createchar.biography.line_count",
    "createchar.biography_count",
    "createchar.biography.selected_title",
    "createchar.question.text",
    "createchar.question.answer1",
    "createchar.question.answer2",
    "createchar.question.index",
    "createchar.classgen.method",
    "createchar.customclass.name",
    "createchar.customclass.skill_count",
    "createchar.customclass.stat_sum",
    "createchar.reflex.remaining",
    "createchar.reflex.stat_top",
    "createchar.reputation.value",
    "createchar.reputation.band",
    "createchar.advantage.count",
    "createchar.disadvantage.count",
    "createchar.stat_sum",
    // M38/UI-2 — typed configuration (11).
    "config.ao.mode",
    "config.antialiasing.mode",
    "config.bloom.amount",
    "config.colorboost.saturation",
    "config.dof.distance",
    "config.dither.mode",
    "config.effects.count",
    "config.effects_applied.count",
    "config.motionblur.amount",
    "config.retromode.mode",
    "config.vignette.amount",
    // M39/UI-3 — A: menú, opciones, guardar/cargar, mensajes, entrada,
    // controles, vídeo (21).
    "m39.menu.items",
    "m39.menu.selected",
    "m39.menu.selected.name",
    "m39.menu.depth",
    "m39.settings.scale",
    "m39.settings.language",
    "m39.settings.applied",
    "m39.saves.count",
    "m39.saves.selected.slot",
    "m39.saves.selected.name",
    "m39.saves.loaded.name",
    "m39.messages.count",
    "m39.messages.selected.prefix",
    "m39.messages.selected.text",
    "m39.messages.decision",
    "m39.input.prompt",
    "m39.input.value",
    "m39.input.max_len",
    "m39.controls.binding",
    "m39.controls.conflicts",
    "m39.video.frames",
    // M39/UI-3 — C: inventario y hoja (14).
    "m39.inv.slots",
    "m39.inv.used_slots",
    "m39.inv.weight_g",
    "m39.inv.max_weight_g",
    "m39.inv.equipped.weapon",
    "m39.inv.equipped.armor",
    "m39.inv.equipped.offhand",
    "m39.inv.selected.name",
    "m39.inv.action",
    "m39.inv.reject_reason",
    "m39.inv.gold",
    "m39.sheet.slots_filled",
    "m39.sheet.level",
    "m39.sheet.encumbrance_band",
    // M39/UI-3 — D: comercio, banca, taberna, descanso, reparación (16).
    "m39.trade.display_price",
    "m39.trade.charged_price",
    "m39.trade.total_display",
    "m39.trade.total_charged",
    "m39.trade.rows",
    "m39.bank.balance",
    "m39.bank.loan_owed",
    "m39.bank.purchase_price",
    "m39.bank.action",
    "m39.inn.room_price",
    "m39.rest.hours",
    "m39.rest.cost",
    "m39.repair.condition",
    "m39.repair.cost",
    "m39.merchant.service",
    "m39.merchant.charged",
    // M39/UI-3 — E: magia (12).
    "m39.spellbook.count",
    "m39.spellbook.selected.name",
    "m39.spellbook.selected.cost",
    "m39.spellmaker.elements",
    "m39.spellmaker.composed_cost",
    "m39.spellmaker.persisted",
    "m39.itemmaker.cost",
    "m39.itemmaker.persisted",
    "m39.potionmaker.cost",
    "m39.potionmaker.persisted",
    "m39.magicitem.charges",
    "m39.spellicon.selected",
    // M39/UI-3 — F: diario, notas, libro, historial (10).
    "m39.journal.entries",
    "m39.journal.selected.text",
    "m39.journal.line_chars",
    "m39.journal.line_limit",
    "m39.journal.prefixes",
    "m39.notes.count",
    "m39.notes.persisted",
    "m39.book.pages",
    "m39.book.page_index",
    "m39.history.entries",
    // M39/UI-3 — G: automap, viaje, transporte, tiempo (11).
    "m39.automap.chunks",
    "m39.automap.markers",
    "m39.automap.dungeon_blocks",
    "m39.travel.regions",
    "m39.travel.selected_region",
    "m39.travel.cost_days",
    "m39.travel.cost_gold",
    "m39.transport.fare",
    "m39.transport.time_hours",
    "m39.teleport.destination",
    "m39.time.day",
    // M39/UI-3 — H: conversación, servicios, encargos, corte, cultos (12).
    "m39.talk.topics",
    "m39.talk.selected.topic",
    "m39.talk.answer",
    "m39.talk.faction",
    "m39.service.kind",
    "m39.service.charged",
    "m39.service.executed",
    "m39.quest.offers",
    "m39.quest.decision",
    "m39.court.ruling",
    "m39.daedra.summoned",
    "m39.coven.service",
];

/// Where a string comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextSource {
    /// A `TEXT.RSC` record id.
    Rsc(u16),
    /// A literal compiled into the layer.
    Literal(&'static str),
    /// A declared read-model path (see [`READ_MODEL_PATHS`]).
    ReadModel(&'static str),
}

/// Everything a text source may need, borrowed.
#[derive(Debug, Clone, Copy, Default)]
pub struct TextEnv<'a> {
    /// Parsed record table (`TEXT.RSC`).
    pub rsc: Option<&'a TextRsc>,
    /// Bytes of `TEXT.RSC` (the table alone cannot yield a record body).
    pub rsc_file: Option<&'a [u8]>,
    /// Live read models.
    pub models: Option<&'a SessionReadModels>,
}

impl<'a> TextEnv<'a> {
    /// No source available.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Read models only.
    pub fn with_models(models: &'a SessionReadModels) -> Self {
        Self {
            rsc: None,
            rsc_file: None,
            models: Some(models),
        }
    }

    /// Record table plus its bytes.
    pub fn with_rsc(rsc: &'a TextRsc, file: &'a [u8]) -> Self {
        Self {
            rsc: Some(rsc),
            rsc_file: Some(file),
            models: None,
        }
    }
}

/// `true` when `path` is one of the declared read-model paths.
pub fn is_read_model_path(path: &str) -> bool {
    READ_MODEL_PATHS.contains(&path)
}

fn read_model(path: &str, models: &SessionReadModels) -> Option<String> {
    models.resolve(path)
}

fn non_empty(text: String) -> Result<String, UiCompatError> {
    if text.trim().is_empty() {
        Err(UiCompatError::empty_text())
    } else {
        Ok(text)
    }
}

/// Resolves one text source (plan §3.4 rules 1-3).
pub fn resolve_text(source: TextSource, env: &TextEnv<'_>) -> Result<String, UiCompatError> {
    match source {
        TextSource::Rsc(id) => {
            let table = env.rsc.ok_or(UiCompatError::Text(RULE_NO_ENV))?;
            let file = env
                .rsc_file
                .ok_or(UiCompatError::Text(RULE_NO_RSC_BYTES))?;
            let text = table
                .record_text(file, id)
                .map_err(|_| UiCompatError::missing_record())?;
            non_empty(text)
        }
        TextSource::Literal(literal) => non_empty(literal.to_string()),
        TextSource::ReadModel(path) => {
            let models = env.models.ok_or(UiCompatError::Text(RULE_NO_ENV))?;
            let text = read_model(path, models).ok_or(UiCompatError::unknown_path())?;
            non_empty(text)
        }
    }
}

/// Resolves a list of sources in declaration order (first error wins).
pub fn resolve_texts(
    sources: &[TextSource],
    env: &TextEnv<'_>,
) -> Result<Vec<String>, UiCompatError> {
    let mut texts = Vec::with_capacity(sources.len());
    for source in sources {
        texts.push(resolve_text(*source, env)?);
    }
    Ok(texts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_compat::test_support::text_rsc;

    /// A4 rule 2 — a path outside the declared list is an error, never the
    /// path itself.
    ///
    /// Violation caught: echoing the path (a placeholder on screen), defaulting
    /// to an empty string, and a read-model path that is declared but cannot be
    /// resolved (a typo in the match arm).
    #[test]
    fn falsifier_a4_undeclared_path_is_error() {
        let models = SessionReadModels::default();
        assert!(!is_read_model_path("hud.secret"));
        assert_eq!(
            resolve_text(
                TextSource::ReadModel("hud.secret"),
                &TextEnv::with_models(&models)
            ),
            Err(UiCompatError::unknown_path())
        );
        for path in READ_MODEL_PATHS {
            assert!(is_read_model_path(path), "{path} must be declared");
            let text = resolve_text(TextSource::ReadModel(path), &TextEnv::with_models(&models))
                .unwrap_or_else(|error| panic!("{path}: {error}"));
            assert!(!text.trim().is_empty(), "{path} resolved empty");
        }
        assert_eq!(
            resolve_text(TextSource::ReadModel("hud.health"), &TextEnv::empty()),
            Err(UiCompatError::Text(RULE_NO_ENV))
        );
    }

    /// A4 rule 1 — a record that does not exist is an error, and a table
    /// without its bytes is an error too.
    ///
    /// Violation caught: returning an empty string for an unknown record id
    /// (which would reach the screen as a blank run) and treating `TextRsc`
    /// alone as enough to read a record.
    #[test]
    fn falsifier_a4_missing_record_is_error() {
        let (bytes, table) = text_rsc();
        let env = TextEnv::with_rsc(&table, &bytes);
        match resolve_text(TextSource::Rsc(0), &env) {
            Ok(text) => assert!(text.starts_with("HELLO"), "record 0: {text}"),
            Err(error) => panic!("record 0 must resolve: {error}"),
        }
        assert_eq!(
            resolve_text(TextSource::Rsc(7), &env),
            Err(UiCompatError::missing_record())
        );
        let table_only = TextEnv {
            rsc: Some(&table),
            rsc_file: None,
            models: None,
        };
        assert_eq!(
            resolve_text(TextSource::Rsc(0), &table_only),
            Err(UiCompatError::Text(RULE_NO_RSC_BYTES))
        );
        assert_eq!(
            resolve_text(TextSource::Rsc(0), &TextEnv::empty()),
            Err(UiCompatError::Text(RULE_NO_ENV))
        );
    }

    /// A4 rule 3 — an empty text is an error, from any of the three sources.
    ///
    /// Violation caught: an empty literal (a placeholder that paints nothing)
    /// and a `TEXT.RSC` record whose body is only the terminator.
    #[test]
    fn falsifier_a4_empty_text_is_error() {
        assert_eq!(
            resolve_text(TextSource::Literal(""), &TextEnv::empty()),
            Err(UiCompatError::empty_text())
        );
        assert_eq!(
            resolve_text(TextSource::Literal("   "), &TextEnv::empty()),
            Err(UiCompatError::empty_text())
        );
        let (bytes, table) = text_rsc();
        assert_eq!(
            resolve_text(TextSource::Rsc(2), &TextEnv::with_rsc(&table, &bytes)),
            Err(UiCompatError::empty_text())
        );
        assert_eq!(
            resolve_text(TextSource::Literal("Health"), &TextEnv::empty()),
            Ok("Health".to_string())
        );
    }

    /// A4 order — a list resolves in declaration order and fails on the first
    /// bad source.
    #[test]
    fn resolve_texts_keeps_declaration_order() {
        let models = SessionReadModels::default();
        let env = TextEnv::with_models(&models);
        let texts = resolve_texts(
            &[
                TextSource::Literal("A"),
                TextSource::ReadModel("hud.level"),
                TextSource::Literal("B"),
            ],
            &env,
        )
        .expect("texts");
        assert_eq!(texts, vec!["A".to_string(), "0".to_string(), "B".to_string()]);
        assert_eq!(
            resolve_texts(&[TextSource::Literal("A"), TextSource::ReadModel("nope")], &env),
            Err(UiCompatError::unknown_path())
        );
        assert_eq!(
            resolve_texts(&[TextSource::Literal("A")], &env),
            Ok(vec!["A".to_string()])
        );
        assert_eq!(
            resolve_texts(&[], &env),
            Ok(Vec::new()),
            "no sources, no text — never a placeholder"
        );
    }

    /// D-1 — the declared list is **153** paths: the ten M37 paths, the 36
    /// `createchar.*` and 11 `config.*` of M38/UI-2, and the 96 `m39.*` of
    /// M39/UI-3 (A21 C14 D16 E12 F10 G11 H12).
    ///
    /// Violation caught: a family added to the read models but not to the layer
    /// (the path would be `unknown_path` at runtime), a path declared twice
    /// (the count would still be 153 with a family short by one), an M39 route
    /// out of block order, and a family silently dropped back to the M37 ten.
    #[test]
    fn falsifier_d1_path_list_is_153_with_the_36_11_96_split() {
        use daggerfall_perception::read_models::{ConfigModels, CreateCharModels, M39Models};

        assert_eq!(READ_MODEL_PATHS.len(), 153);
        let createchar = READ_MODEL_PATHS
            .iter()
            .filter(|path| path.starts_with("createchar."))
            .count();
        let config = READ_MODEL_PATHS
            .iter()
            .filter(|path| path.starts_with("config."))
            .count();
        let m39 = READ_MODEL_PATHS
            .iter()
            .filter(|path| path.starts_with("m39."))
            .count();
        let session = READ_MODEL_PATHS.len() - createchar - config - m39;
        assert_eq!(
            (session, createchar, config, m39),
            (10, 36, 11, 96),
            "declared split must be 10 session + 36 createchar + 11 config + 96 m39"
        );
        // The M39 suffix is the family declaration, in family order.
        let suffix = &READ_MODEL_PATHS[READ_MODEL_PATHS.len() - m39..];
        assert_eq!(suffix, M39Models::paths().as_slice());
        // The unique set is exactly the families' own declaration (0 drift
        // between the layer and `perception::read_models`).
        let declared: std::collections::BTreeSet<&str> = READ_MODEL_PATHS.into_iter().collect();
        assert_eq!(declared.len(), READ_MODEL_PATHS.len(), "no duplicated path");
        for path in CreateCharModels::PATHS {
            assert!(declared.contains(path), "{path} missing from the layer");
        }
        for path in ConfigModels::PATHS {
            assert!(declared.contains(path), "{path} missing from the layer");
        }
        for path in M39Models::paths() {
            assert!(declared.contains(path), "{path} missing from the layer");
        }
        for path in [
            "hud.health",
            "hud.max_health",
            "hud.stamina",
            "hud.max_stamina",
            "hud.level",
            "hud.health_pct",
            "hud.stamina_pct",
            "panel.gold",
            "panel.item_count",
            "panel.quest_count",
        ] {
            assert!(declared.contains(path), "M37 path {path} was dropped");
        }
    }
}

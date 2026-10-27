//! M38/UI-2 — the **27 surfaces** of the character creator (16) and the typed
//! configuration (11), declared on the closed `ui_compat` layer.
//!
//! **Contract:** `docs/sintergic/l5-mission/m38-ui2-plan-CR-71f2b7bf-cb0d-43b3-bfaa-eb66de77988f.md`
//! §2.1/§2.2 (ids and census names verbatim), §2.3 (≥ 2 labels + ≥ 1 value per
//! surface), §3 D-2 (retail string map) and §3 D-3 (the `SPOP.RCI` 22×22 side
//! forces 44×44 as the minimum nine-slice target, so the two 40-high rectangles
//! of the plan are declared 44 high with x/y/width intact).
//!
//! Nothing here paints by hand: surfaces leave through
//! [`WindowComposition::paint`] (the M17 compositor) and glyphs through
//! `ui_compat::typography`. Every window is resolved by **name** through
//! `ui_shell::canonical_id_for` — an unknown name is an error, never a default,
//! and no numeric `WindowId` is hard-coded.
//!
//! **Values are never invented.** Each `ReadModel` declaration names one of the
//! 47 `createchar.*`/`config.*` paths of `perception::read_models` (D-1); the
//! probe fills the models from `BIO.DAT`, `BIOG00T0.TXT`, `CLASS00.CFG`,
//! `FACES.CIF` and the typed settings — a path that does not resolve is an
//! `UiCompatError`, never a `0`.

use daggerfall_nervous::events::WindowId;
use daggerfall_parser_bevy_bridge::ui_compat::{
    resolve_text, GlyphRun, Navigation, RetailFont, SurfaceOwnership, SurfaceSource, SurfaceSpec,
    TextEnv, TextSource, UiCompatError, WindowComposition,
};
use daggerfall_parser_bevy_bridge::ui_spec::{UiRect, DEFAULT_INK};
use daggerfall_perception::ui_shell::canonical_id_for;

use crate::m37_pilots::TextDecl;

// ── declared census hosts (§2.1 / §2.2) ─────────────────────────────────────

/// Host of the sixteen `CreateChar*` windows ("orquestador sin panel propio").
pub const CREATE_HOST: &str = "DaggerfallStartNewGameWizard";
/// Host of the eleven `*ConfigPage` / effects windows (retail parents them).
pub const CONFIG_HOST: &str = "DaggerfallAdvancedSettingsWindow";

/// Size of the two families, as the probes print them.
pub const CREATE_SURFACE_COUNT: usize = 16;
/// Size of the configuration family.
pub const CONFIG_SURFACE_COUNT: usize = 11;

// ── retail assets (measured, never guessed) ────────────────────────────────

/// 64000 B raw 320×200 — the main creation panel.
pub const INFO_ASSET: &str = "INFO00I0.IMG";
/// 25612 B (200×128) — the list picker.
pub const PICK_ASSET: &str = "PICK00I0.IMG";
/// 4356 B — the 9 × 22×22 nine-slice container.
pub const SLICE_ASSET: &str = "SPOP.RCI";
/// 64000 B raw 320×200 — the biography background.
pub const LGBK_ASSET: &str = "LGBK00I0.IMG";
/// 64768 B (320×200 + 768 B palette) — the class questionnaire.
pub const CHGN_ASSET: &str = "CHGN00I0.IMG";
/// 64000 B raw 320×200 — the settings frame.
pub const CNFG_ASSET: &str = "CNFG00I0.IMG";
/// 249 856 B = 61 records of 64×64 (pure RCI) — the character faces.
pub const FACE_ASSET: &str = "FACES.CIF";

/// Side of one `FACES.CIF` record (the atlas cell), in pixels.
pub const FACE_SIDE: u16 = 64;
/// Canvas position of the selected face.
pub const FACE_AT: (i16, i16) = (140, 60);

/// Side of one `SPOP.RCI` cell (`4356 B = 9 × 22 × 22`).
pub const SLICE_SIDE: u16 = 22;
/// Minimum nine-slice target: two cells per axis (**D-3**).
pub const SLICE_MINIMUM: u16 = 2 * SLICE_SIDE;

/// Characters one line may carry before the *view* clips it (the read model
/// itself stays whole: the surface shows a window, it does not rewrite).
pub const TITLE_CHARS: usize = 21;
/// Characters of an action label.
pub const ACTION_CHARS: usize = 40;
/// Characters of one value line.
pub const VALUE_CHARS: usize = 56;
/// Lines of a multi-line value (the biography body).
pub const BODY_LINES: usize = 3;

// ── face selection (K2) ────────────────────────────────────────────────────

/// Which `FACES.CIF` record C09/C16 show.
///
/// The selection is **modular over the number of records the file was measured
/// to carry** (`createchar.face.count`, filled from `FACES.CIF` itself), never
/// over a literal: a build with 61 records and a build with any other measured
/// count both wrap inside their own atlas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FaceSelection {
    index: u16,
    count: u16,
}

impl FaceSelection {
    /// Selection of `index` out of `count` records; `None` when the pair does
    /// not name a record (`count == 0` or `index >= count`).
    pub fn new(index: u16, count: u16) -> Option<Self> {
        if count == 0 || index >= count {
            return None;
        }
        Some(Self { index, count })
    }

    /// Selected record.
    pub fn index(&self) -> u16 {
        self.index
    }

    /// Records measured in the container.
    pub fn count(&self) -> u16 {
        self.count
    }

    /// Next record, wrapping at the measured count.
    pub fn next(self) -> Self {
        Self {
            index: (self.index + 1) % self.count,
            count: self.count,
        }
    }

    /// Previous record, wrapping at the measured count.
    pub fn prev(self) -> Self {
        Self {
            index: (self.index + self.count - 1) % self.count,
            count: self.count,
        }
    }
}

// ── chrome ─────────────────────────────────────────────────────────────────

/// Top row of record `record` in the `FACES.CIF` atlas, saturated to `i16`.
///
/// Saturation is deliberate: a record past the atlas yields a cell that starts
/// outside the decoded image, so `place()` fails with `RULE_SURFACE_SOURCE` —
/// it never wraps onto record 0 (a silent wrong face).
fn face_cell_top(record: u16) -> i16 {
    i16::try_from(i32::from(record) * i32::from(FACE_SIDE)).unwrap_or(i16::MAX)
}

/// One chrome layer of a surface, before it is bound to a host.
///
/// The owners of `Nested` layers are the census **hosts** (§2.2: "the page is
/// drawn inside the host frame, which must be open"), so the
/// ownership rule of `ui_compat` (rule 4) is exercised by every surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// `INFO00I0.IMG` whole, owned by the surface window.
    Info,
    /// `LGBK00I0.IMG` whole, owned.
    Lgbk,
    /// `CHGN00I0.IMG` whole, owned.
    Chgn,
    /// `PICK00I0.IMG` whole at (60,36), nested in the host.
    Pick,
    /// Record `n` of `FACES.CIF` at [`FACE_AT`], owned by the surface (the
    /// atlas is a decode of the retail file, so nothing else owns it).
    Face(u16),
    /// `SPOP.RCI` nine-slice `(x, y, width, height)` nested in the host.
    Slice(i16, i16, u16, u16),
    /// `CNFG00I0.IMG` clip `(4,20,312,160)` nested in the host.
    Cnfg,
}

impl Layer {
    /// Asset this layer paints.
    pub fn asset(self) -> &'static str {
        match self {
            Self::Info => INFO_ASSET,
            Self::Lgbk => LGBK_ASSET,
            Self::Chgn => CHGN_ASSET,
            Self::Pick => PICK_ASSET,
            Self::Face(_) => FACE_ASSET,
            Self::Slice(..) => SLICE_ASSET,
            Self::Cnfg => CNFG_ASSET,
        }
    }

    /// Binds the layer to its host (typed `WindowId`).
    pub fn spec(self, host: WindowId) -> SurfaceSpec {
        match self {
            Self::Info => SurfaceSpec::owned(
                INFO_ASSET,
                UiRect::new(0, 0, 320, 200),
                SurfaceSource::Whole,
            ),
            Self::Lgbk => SurfaceSpec::owned(
                LGBK_ASSET,
                UiRect::new(0, 0, 320, 200),
                SurfaceSource::Whole,
            ),
            Self::Chgn => SurfaceSpec::owned(
                CHGN_ASSET,
                UiRect::new(0, 0, 320, 200),
                SurfaceSource::Whole,
            ),
            Self::Pick => SurfaceSpec::nested(
                PICK_ASSET,
                UiRect::new(60, 36, 200, 128),
                SurfaceSource::Whole,
                host,
            ),
            Self::Face(record) => SurfaceSpec::owned(
                FACE_ASSET,
                UiRect::new(FACE_AT.0, FACE_AT.1, FACE_SIDE, FACE_SIDE),
                SurfaceSource::Rect(UiRect::new(0, face_cell_top(record), FACE_SIDE, FACE_SIDE)),
            ),
            Self::Slice(x, y, width, height) => SurfaceSpec::nested(
                SLICE_ASSET,
                UiRect::new(x, y, width, height),
                SurfaceSource::Whole,
                host,
            ),
            Self::Cnfg => SurfaceSpec::nested(
                CNFG_ASSET,
                UiRect::new(4, 20, 312, 160),
                SurfaceSource::Rect(UiRect::new(4, 20, 312, 160)),
                host,
            ),
        }
    }
}

// ── declarations ───────────────────────────────────────────────────────────

/// One declared text of a surface: the `ui_compat` declaration plus the width
/// of the *view* window applied to each rendered line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceText {
    /// The `ui_compat` declaration (source, face, ink, position).
    pub decl: TextDecl,
    /// Maximum characters per rendered line (`0` = no clipping).
    pub max_chars: usize,
}

/// One of the 27 declared surfaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceWindow {
    /// Gate id (`C01`…`C16`, `CFG01`…`CFG11`).
    pub id: &'static str,
    /// Census class name (resolved through `canonical_id_for`).
    pub name: &'static str,
    /// Census name of the host that must be open.
    pub host_name: &'static str,
    /// Typed host window.
    pub host: WindowId,
    /// Typed window of this surface.
    pub window: WindowId,
    /// Assets painted, declaration order, distinct.
    pub assets: Vec<&'static str>,
    /// Chrome, in painting order.
    pub composition: WindowComposition,
    /// Text declarations, in painting order.
    pub texts: Vec<SurfaceText>,
    /// Focusable rows.
    pub focus: Vec<crate::m37_pilots::FocusTarget>,
}

fn window_of(name: &str) -> Result<WindowId, UiCompatError> {
    canonical_id_for(name).ok_or_else(UiCompatError::unowned_surface)
}

/// Builds one surface: chrome bound to the host, the two labels (§2.3) and the
/// declared read-model values.
fn surface(
    id: &'static str,
    name: &'static str,
    host_name: &'static str,
    layers: &[Layer],
    labels: [&'static str; 2],
    values: &[&'static str],
    focus_rows: usize,
) -> Result<SurfaceWindow, UiCompatError> {
    let host = window_of(host_name)?;
    let window = window_of(name)?;
    let mut specs = Vec::with_capacity(layers.len());
    let mut assets: Vec<&'static str> = Vec::with_capacity(layers.len());
    for layer in layers {
        specs.push(layer.spec(host));
        let asset = layer.asset();
        if !assets.contains(&asset) {
            assets.push(asset);
        }
    }

    let mut texts = Vec::with_capacity(2 + values.len());
    texts.push(SurfaceText {
        decl: TextDecl::line(
            TextSource::Literal(labels[0]),
            RetailFont::Title,
            DEFAULT_INK,
            (8, 4),
        ),
        max_chars: TITLE_CHARS,
    });
    texts.push(SurfaceText {
        decl: TextDecl::line(
            TextSource::Literal(labels[1]),
            RetailFont::Small,
            DEFAULT_INK,
            (12, 22),
        ),
        max_chars: ACTION_CHARS,
    });
    let mut y = 38i16;
    for path in values {
        let body = *path == "createchar.biography.body";
        texts.push(SurfaceText {
            decl: TextDecl {
                source: TextSource::ReadModel(path),
                font: RetailFont::Default,
                ink: DEFAULT_INK,
                at: (16, y),
                max_lines: if body { BODY_LINES } else { 1 },
            },
            max_chars: VALUE_CHARS,
        });
        y += if body { 8 * BODY_LINES as i16 + 6 } else { 12 };
    }

    let mut focus = Vec::with_capacity(focus_rows);
    for row in 0..focus_rows {
        focus.push(crate::m37_pilots::FocusTarget {
            window,
            row,
            rect: UiRect::new(12, 148, 120, 44),
        });
    }

    Ok(SurfaceWindow {
        id,
        name,
        host_name,
        host,
        window,
        assets,
        composition: WindowComposition::new(window, specs),
        texts,
        focus,
    })
}

/// The sixteen creation surfaces of §2.1, in id order.
pub fn create_surfaces() -> Result<Vec<SurfaceWindow>, UiCompatError> {
    let mut out = Vec::with_capacity(CREATE_SURFACE_COUNT);
    out.push(surface(
        "C01",
        "CreateCharAddBonusSkills",
        CREATE_HOST,
        &[Layer::Info, Layer::Pick, Layer::Slice(12, 148, 120, 44)],
        ["BONUS SKILLS", "SELECT SKILL"],
        &[
            "createchar.bonus_remaining",
            "createchar.skill_slot1",
            "createchar.skill_slot2",
            "createchar.skill_slot3",
        ],
        4,
    )?);
    out.push(surface(
        "C02",
        "CreateCharAddBonusStats",
        CREATE_HOST,
        &[Layer::Info, Layer::Pick, Layer::Slice(12, 148, 120, 44)],
        ["BONUS STATS", "SELECT STAT"],
        &[
            "createchar.stats_remaining",
            "createchar.stat.strength",
            "createchar.stat.agility",
        ],
        4,
    )?);
    out.push(surface(
        "C03",
        "CreateCharBiography",
        CREATE_HOST,
        &[Layer::Lgbk],
        ["BIOGRAPHY", "PRESS CONTINUE"],
        &["createchar.biography.body", "createchar.biography.line_count"],
        2,
    )?);
    out.push(surface(
        "C04",
        "CreateCharChooseBio",
        CREATE_HOST,
        &[Layer::Info, Layer::Pick],
        ["CHOOSE BIOGRAPHY", "SELECT ONE"],
        &[
            "createchar.biography_count",
            "createchar.biography.selected_title",
        ],
        3,
    )?);
    out.push(surface(
        "C05",
        "CreateCharChooseClassGen",
        CREATE_HOST,
        &[Layer::Info, Layer::Slice(60, 50, 200, 100)],
        ["CLASS GENERATION", "SELECT METHOD"],
        &["createchar.classgen.method"],
        3,
    )?);
    out.push(surface(
        "C06",
        "CreateCharClassQuestions",
        CREATE_HOST,
        &[Layer::Chgn],
        ["CLASS QUESTIONS", "CHOOSE ANSWER"],
        &[
            "createchar.question.text",
            "createchar.question.answer1",
            "createchar.question.answer2",
            "createchar.question.index",
        ],
        3,
    )?);
    out.push(surface(
        "C07",
        "CreateCharClassSelect",
        CREATE_HOST,
        &[Layer::Info, Layer::Pick],
        ["SELECT CLASS", "CHOOSE ONE"],
        &["createchar.class.name", "createchar.class.count"],
        4,
    )?);
    out.push(surface(
        "C08",
        "CreateCharCustomClass",
        CREATE_HOST,
        &[Layer::Info, Layer::Pick, Layer::Slice(12, 148, 120, 44)],
        ["CUSTOM CLASS", "SET SKILLS"],
        &[
            "createchar.customclass.name",
            "createchar.customclass.skill_count",
            "createchar.customclass.stat_sum",
        ],
        5,
    )?);
    out.push(surface(
        "C09",
        "CreateCharFaceSelect",
        CREATE_HOST,
        // The face is painted **last** on purpose: `PICK00I0.IMG` (200×128 at
        // (60,36)) is fully opaque — measured 25 600/25 600 index ≠ 0, 4096/4096
        // inside `FACE_AT` — so a face painted before it would be erased and
        // the surface would not show the selection at all (K2).
        &[Layer::Info, Layer::Pick, Layer::Face(0)],
        ["SELECT FACE", "CHOOSE ONE"],
        &["createchar.face.index", "createchar.face.count"],
        4,
    )?);
    out.push(surface(
        "C10",
        "CreateCharGenderSelect",
        CREATE_HOST,
        &[Layer::Info, Layer::Slice(90, 70, 140, 60)],
        ["SELECT GENDER", "CHOOSE ONE"],
        &["createchar.gender.name"],
        2,
    )?);
    out.push(surface(
        "C11",
        "CreateCharNameSelect",
        CREATE_HOST,
        &[Layer::Info, Layer::Slice(40, 80, 240, 44)],
        ["ENTER NAME", "TYPE NAME"],
        &["createchar.name.value", "createchar.name.max_len"],
        2,
    )?);
    out.push(surface(
        "C12",
        "CreateCharRaceSelect",
        CREATE_HOST,
        &[Layer::Info, Layer::Pick],
        ["SELECT RACE", "CHOOSE ONE"],
        &["createchar.race.name", "createchar.race.count"],
        4,
    )?);
    out.push(surface(
        "C13",
        "CreateCharReflexSelect",
        CREATE_HOST,
        &[Layer::Info, Layer::Pick],
        ["REFLEXES", "SELECT STAT"],
        &["createchar.reflex.remaining", "createchar.reflex.stat_top"],
        4,
    )?);
    out.push(surface(
        "C14",
        "CreateCharReputationWindow",
        CREATE_HOST,
        &[Layer::Info, Layer::Slice(60, 60, 200, 80)],
        ["REPUTATION", "ACCEPT"],
        &["createchar.reputation.value", "createchar.reputation.band"],
        2,
    )?);
    out.push(surface(
        "C15",
        "CreateCharSpecialAdvantageWindow",
        CREATE_HOST,
        &[Layer::Info, Layer::Pick, Layer::Slice(12, 148, 120, 44)],
        ["ADVANTAGES", "SELECT ONE"],
        &[
            "createchar.advantage.count",
            "createchar.disadvantage.count",
        ],
        4,
    )?);
    out.push(surface(
        "C16",
        "CreateCharSummary",
        CREATE_HOST,
        &[Layer::Info, Layer::Face(0)],
        ["SUMMARY", "BEGIN GAME"],
        &[
            "createchar.name.value",
            "createchar.gender.name",
            "createchar.race.name",
            "createchar.class.name",
            "createchar.level",
            "createchar.stat_sum",
        ],
        2,
    )?);
    Ok(out)
}

/// The eleven configuration surfaces of §2.2, in id order.
pub fn config_surfaces() -> Result<Vec<SurfaceWindow>, UiCompatError> {
    let mut out = Vec::with_capacity(CONFIG_SURFACE_COUNT);
    out.push(surface(
        "CFG01",
        "AmbientOcclusionConfigPage",
        CONFIG_HOST,
        &[Layer::Cnfg],
        ["AMBIENT OCCLUSION", "APPLY"],
        &["config.ao.mode"],
        3,
    )?);
    out.push(surface(
        "CFG02",
        "AntialiasingConfigPage",
        CONFIG_HOST,
        &[Layer::Cnfg],
        ["ANTIALIASING", "APPLY"],
        &["config.antialiasing.mode"],
        3,
    )?);
    out.push(surface(
        "CFG03",
        "BloomConfigPage",
        CONFIG_HOST,
        &[Layer::Cnfg],
        ["BLOOM", "APPLY"],
        &["config.bloom.amount"],
        3,
    )?);
    out.push(surface(
        "CFG04",
        "ColorBoostConfigPage",
        CONFIG_HOST,
        &[Layer::Cnfg],
        ["COLOR BOOST", "APPLY"],
        &["config.colorboost.saturation"],
        3,
    )?);
    out.push(surface(
        "CFG05",
        "DepthOfFieldConfigPage",
        CONFIG_HOST,
        &[Layer::Cnfg],
        ["DEPTH OF FIELD", "APPLY"],
        &["config.dof.distance"],
        3,
    )?);
    out.push(surface(
        "CFG06",
        "DitherConfigPage",
        CONFIG_HOST,
        &[Layer::Cnfg],
        ["DITHER", "APPLY"],
        &["config.dither.mode"],
        3,
    )?);
    out.push(surface(
        "CFG07",
        "GameEffectConfigPage",
        CONFIG_HOST,
        &[Layer::Cnfg],
        ["GAME EFFECTS", "APPLY"],
        &["config.effects.count"],
        4,
    )?);
    out.push(surface(
        "CFG08",
        "GameEffectsConfigWindow",
        CONFIG_HOST,
        &[Layer::Cnfg, Layer::Slice(100, 30, 120, 120)],
        ["EFFECTS LIST", "SCROLL LIST"],
        &["config.effects_applied.count"],
        4,
    )?);
    out.push(surface(
        "CFG09",
        "MotionBlurConfigPage",
        CONFIG_HOST,
        &[Layer::Cnfg],
        ["MOTION BLUR", "APPLY"],
        &["config.motionblur.amount"],
        3,
    )?);
    out.push(surface(
        "CFG10",
        "RetroModeConfigPage",
        CONFIG_HOST,
        &[Layer::Cnfg],
        ["RETRO MODE", "APPLY"],
        &["config.retromode.mode"],
        3,
    )?);
    out.push(surface(
        "CFG11",
        "VignetteConfigPage",
        CONFIG_HOST,
        &[Layer::Cnfg],
        ["VIGNETTE", "APPLY"],
        &["config.vignette.amount"],
        3,
    )?);
    Ok(out)
}

// ── text: resolution, view clipping, glyph runs ────────────────────────────

impl SurfaceWindow {
    /// Resolves the declared sources, clips each line to its view window and
    /// lays the runs out (surfaces first, text on top).
    ///
    /// `max_lines` of the declaration bounds the runs of a multi-line value
    /// (the biography body), and `max_chars` bounds the width of one line: the
    /// **view** clips, the read model stays whole (A4 measures the whole value).
    pub fn glyphs(
        &self,
        assets: &daggerfall_parser_bevy_bridge::ui_spec::UiAssets,
        env: &TextEnv<'_>,
    ) -> Result<Vec<GlyphRun>, UiCompatError> {
        use daggerfall_parser_bevy_bridge::ui_compat::glyphs;

        let mut runs = Vec::new();
        for text in &self.texts {
            let value = resolve_text(text.decl.source, env)?;
            let clipped: Vec<&str> = value
                .lines()
                .map(str::trim_end)
                .filter(|line| !line.is_empty())
                .map(|line| clip(line, text.max_chars))
                .filter(|line| !line.is_empty())
                .collect();
            let lines = if clipped.is_empty() {
                vec![clip(value.trim(), text.max_chars)]
            } else {
                clipped
            };
            let mut y = text.decl.at.1;
            for (index, line) in lines.iter().enumerate() {
                if text.decl.max_lines > 0 && index >= text.decl.max_lines {
                    break;
                }
                let run = glyphs(text.decl.font, text.decl.ink, line, assets)?.at(text.decl.at.0, y);
                y += run.height as i16 + 1;
                runs.push(run);
            }
        }
        Ok(runs)
    }

    /// Paints chrome then glyphs onto `framebuffer`.
    pub fn paint(
        &self,
        framebuffer: &mut daggerfall_parser_bevy_bridge::ui_spec::Framebuffer,
        assets: &daggerfall_parser_bevy_bridge::ui_spec::UiAssets,
        env: &TextEnv<'_>,
    ) -> Result<(), UiCompatError> {
        use daggerfall_parser_bevy_bridge::ui_compat::paint_runs;

        self.composition.paint(framebuffer, assets)?;
        let runs = self.glyphs(assets, env)?;
        paint_runs(framebuffer, &runs, assets)
    }

    /// Focus rows the router must declare.
    pub fn focus_rows(&self) -> usize {
        self.focus.len()
    }

    /// Read-model paths this surface shows (`A4`).
    pub fn read_model_paths(&self) -> Vec<&'static str> {
        self.texts
            .iter()
            .filter_map(|text| match text.decl.source {
                TextSource::ReadModel(path) => Some(path),
                _ => None,
            })
            .collect()
    }

    /// Declared literals of this surface (`D-2` input).
    pub fn literals(&self) -> Vec<&'static str> {
        self.texts
            .iter()
            .filter_map(|text| match text.decl.source {
                TextSource::Literal(literal) => Some(literal),
                _ => None,
            })
            .collect()
    }

    /// Record of `FACES.CIF` this composition paints, `None` when it declares
    /// no face.
    ///
    /// The record is read back from the declaration (`source.y / FACE_SIDE`),
    /// not kept in a parallel field: one number, one owner.
    pub fn face_record(&self) -> Option<u16> {
        self.composition
            .surfaces
            .iter()
            .find(|spec| spec.asset == FACE_ASSET)
            .and_then(|spec| match spec.source {
                SurfaceSource::Rect(cell) => u16::try_from(cell.y).ok().map(|y| y / FACE_SIDE),
                _ => None,
            })
    }

    /// Same composition with the face bound to `selection`.
    ///
    /// `None` when the surface declares no face (nothing to rebind) or when
    /// the declaration of that face is not a record cell — never a silent
    /// fallback to record 0. The result paints through
    /// [`SurfaceWindow::paint`] like any other surface.
    pub fn with_face(&self, selection: FaceSelection) -> Option<Self> {
        let mut window = self.clone();
        let spec = window
            .composition
            .surfaces
            .iter_mut()
            .find(|spec| spec.asset == FACE_ASSET)?;
        if !matches!(spec.source, SurfaceSource::Rect(_)) {
            return None;
        }
        spec.source = SurfaceSource::Rect(UiRect::new(
            0,
            face_cell_top(selection.index()),
            FACE_SIDE,
            FACE_SIDE,
        ));
        Some(window)
    }
}

/// Cuts `text` to `max_chars` characters (`0` = untouched), never mid-char.
pub fn clip(text: &str, max_chars: usize) -> &str {
    if max_chars == 0 {
        return text;
    }
    match text.char_indices().nth(max_chars) {
        Some((at, _)) => &text[..at],
        None => text,
    }
}

// ── D-2 retail string map ──────────────────────────────────────────────────

/// Outcome of mapping one declared literal against `TEXT.RSC`.
///
/// The map **never guesses**: a literal becomes a record identity only when it
/// matches the whole text of exactly one record. An absent or ambiguous match
/// stays a `Literal` (and is counted as unmapped), so no surface can inherit a
/// record the retail file does not vouch for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiteralMatch {
    /// Exactly one record carries this text.
    Unique(u16),
    /// No record carries this text.
    Absent,
    /// Several records carry this text (the count).
    Ambiguous(usize),
}

impl LiteralMatch {
    /// `true` when the literal became a verified record identity.
    pub fn is_unique(self) -> bool {
        matches!(self, Self::Unique(_))
    }

    /// `true` when the match may not name a record.
    pub fn is_unmapped(self) -> bool {
        !self.is_unique()
    }

    /// Short evidence form (`rsc=14`, `absent`, `ambiguous=2`).
    pub fn label(self) -> &'static str {
        match self {
            Self::Unique(_) => "rsc",
            Self::Absent => "absent",
            Self::Ambiguous(_) => "ambiguous",
        }
    }
}

/// Classifies one literal against the record texts of `TEXT.RSC`.
///
/// `records` is `(id, text)` in file order; the comparison is **exact**
/// (byte equality after the table's own decoding), so `"Apply"` never matches
/// `"APPLY"` and a whitespace difference keeps the literal unmapped.
pub fn classify_literal(literal: &str, records: &[(u16, String)]) -> LiteralMatch {
    let mut found: Option<u16> = None;
    let mut count = 0usize;
    for (id, text) in records {
        if text == literal {
            count += 1;
            if found.is_none() {
                found = Some(*id);
            }
        }
    }
    match (found, count) {
        (Some(id), 1) => LiteralMatch::Unique(id),
        (Some(_), many) => LiteralMatch::Ambiguous(many),
        (None, _) => LiteralMatch::Absent,
    }
}

/// One entry of the string map: a surface, a literal and its outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StringMapEntry {
    /// Surface id (`C01`…).
    pub surface: &'static str,
    /// Declared literal.
    pub literal: &'static str,
    /// Verified outcome.
    pub matched: LiteralMatch,
}

/// Published counts of the map (`AXIS A4` / `D-5`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StringMapCounts {
    /// Literals that became a verified record identity.
    pub rsc: usize,
    /// Literals that stayed literals (unique + absent + ambiguous).
    pub literal: usize,
    /// Literals with no unique record (absent or ambiguous).
    pub unmapped: usize,
    /// Ambiguous literals (a subset of `unmapped`).
    pub ambiguous: usize,
}

/// Builds the map of every declared literal of `surfaces`.
pub fn retail_string_map(
    surfaces: &[SurfaceWindow],
    records: &[(u16, String)],
) -> Vec<StringMapEntry> {
    let mut out = Vec::new();
    for surface in surfaces {
        for literal in surface.literals() {
            out.push(StringMapEntry {
                surface: surface.id,
                literal,
                matched: classify_literal(literal, records),
            });
        }
    }
    out
}

/// Counts the entries of a built map.
pub fn string_map_counts(entries: &[StringMapEntry]) -> StringMapCounts {
    let mut counts = StringMapCounts::default();
    for entry in entries {
        if entry.matched.is_unique() {
            counts.rsc += 1;
            counts.literal += 1;
        } else {
            counts.literal += 1;
            counts.unmapped += 1;
            if matches!(entry.matched, LiteralMatch::Ambiguous(_)) {
                counts.ambiguous += 1;
            }
        }
    }
    counts
}

// ── measurement ────────────────────────────────────────────────────────────

/// FNV-1a64 of a canonical byte form.
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn ownership_label(ownership: SurfaceOwnership) -> String {
    match ownership {
        SurfaceOwnership::Owned => "owned".to_string(),
        SurfaceOwnership::Nested(owner) => format!("nested={}", owner.index()),
        SurfaceOwnership::External => "external".to_string(),
    }
}

/// Canonical form of one measured surface: id, counts, every node rectangle and
/// source, every glyph digest, in declaration order.
pub fn canonical_measure(
    id: &str,
    nodes: &[daggerfall_parser_bevy_bridge::ui_compat::SurfaceNode],
    runs: &[GlyphRun],
    opaque: usize,
) -> String {
    let mut canonical = format!(
        "{id}|n={}|g={}|o={opaque}",
        nodes.len(),
        runs.len()
    );
    for node in nodes {
        canonical.push_str(&format!(
            "|{}@{},{} {}x{} src={} own={} kind={}",
            node.asset,
            node.rect.x,
            node.rect.y,
            node.rect.width,
            node.rect.height,
            match node.source {
                None => "-".to_string(),
                Some(cell) => format!("{},{} {}x{}", cell.x, cell.y, cell.width, cell.height),
            },
            ownership_label(node.ownership),
            node.kind.as_str(),
        ));
    }
    for run in runs {
        canonical.push_str(&format!(
            "|{}:{}:{}x{}@{}:{}",
            run.font.file(),
            run.ink,
            run.rect.width,
            run.rect.height,
            run.rect.y,
            run.digest
        ));
    }
    canonical
}

/// 64-bit digest of [`canonical_measure`] (the `SURFACE … digest=` field).
pub fn measure_digest(
    id: &str,
    nodes: &[daggerfall_parser_bevy_bridge::ui_compat::SurfaceNode],
    runs: &[GlyphRun],
    opaque: usize,
) -> u64 {
    fnv1a64(canonical_measure(id, nodes, runs, opaque).as_bytes())
}

/// Assets a surface declares that the bank does not carry (A1: an orphan
/// declaration is an error, never a blank rectangle).
pub fn missing_assets<'a>(
    declared: &[&'a str],
    present: &dyn Fn(&str) -> bool,
) -> Vec<&'a str> {
    declared
        .iter()
        .copied()
        .filter(|asset| !present(asset))
        .collect()
}

/// Hosts that the declarations require to be open (rule 4), distinct and in
/// declaration order.
pub fn required_hosts(surfaces: &[SurfaceWindow]) -> Vec<WindowId> {
    let mut hosts: Vec<WindowId> = Vec::new();
    for surface in surfaces {
        if !hosts.contains(&surface.host) {
            hosts.push(surface.host);
        }
    }
    hosts
}

/// Fails when a declared host is not open (the `Nested` rule of the layer,
/// checked at declaration level so the probe can report it before painting).
pub fn verify_nested_hosts(
    surfaces: &[SurfaceWindow],
    nav: &Navigation,
) -> Result<(), UiCompatError> {
    for host in required_hosts(surfaces) {
        if !nav.is_open(host) {
            return Err(UiCompatError::unowned_surface());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use daggerfall_parser_bevy_bridge::ui_compat::{
        PaletteIdentity, SurfaceKind, SurfaceNode,
    };
    use daggerfall_parser_rs::image::PaletteFamily;
    use daggerfall_parser_bevy_bridge::ui_spec::UiRect;
    use std::collections::BTreeSet;

    fn palette(name: &str) -> PaletteIdentity {
        PaletteIdentity {
            asset: name.to_string(),
            family: PaletteFamily::Hud,
            hash: 7,
            range: (0, 255),
        }
    }

    fn node(rect: UiRect, ownership: SurfaceOwnership) -> SurfaceNode {
        SurfaceNode {
            asset: INFO_ASSET.to_string(),
            rect,
            source: None,
            ownership,
            palette: palette(INFO_ASSET),
            kind: SurfaceKind::Sprite,
        }
    }

    /// The census resolves every one of the 27 names, and the ids/names are
    /// the ones of §2.1/§2.2 — a renamed window is an error, not a default.
    #[test]
    fn falsifier_m38_the_27_surfaces_are_the_census_names() {
        let create = create_surfaces().expect("creation surfaces");
        let config = config_surfaces().expect("configuration surfaces");
        assert_eq!(create.len(), CREATE_SURFACE_COUNT);
        assert_eq!(config.len(), CONFIG_SURFACE_COUNT);

        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        for surface in create.iter().chain(config.iter()) {
            assert!(ids.insert(surface.id), "duplicate id {}", surface.id);
            assert!(names.insert(surface.name), "duplicate {}", surface.name);
            assert_eq!(
                canonical_id_for(surface.name),
                Some(surface.window),
                "{} must resolve through the census",
                surface.name
            );
            assert_eq!(canonical_id_for(surface.host_name), Some(surface.host));
        }
        assert_eq!(ids.len(), 27);
        assert_eq!(create[0].host_name, CREATE_HOST);
        assert_eq!(config[0].host_name, CONFIG_HOST);
        // §2.3: at least two labels and one value per surface.
        for surface in create.iter().chain(config.iter()) {
            assert!(surface.literals().len() >= 2, "{} labels", surface.id);
            assert!(!surface.read_model_paths().is_empty(), "{} value", surface.id);
            assert!(surface.focus_rows() >= 2, "{} focus", surface.id);
        }
    }

    /// **D-3 falsifier** — every `SPOP.RCI` target is at least 44×44 (two
    /// 22-pixel cells): a 40-high rectangle would refuse to resolve.
    #[test]
    fn falsifier_d3_nine_slice_targets_are_at_least_two_cells() {
        let mut slices = 0;
        for surface in create_surfaces()
            .expect("create")
            .into_iter()
            .chain(config_surfaces().expect("config"))
        {
            for spec in &surface.composition.surfaces {
                if spec.asset == SLICE_ASSET {
                    slices += 1;
                    assert!(
                        spec.canvas_rect.width >= SLICE_MINIMUM
                            && spec.canvas_rect.height >= SLICE_MINIMUM,
                        "{} declares a {}x{} nine-slice target (minimum {}x{})",
                        surface.id,
                        spec.canvas_rect.width,
                        spec.canvas_rect.height,
                        SLICE_MINIMUM,
                        SLICE_MINIMUM
                    );
                }
                assert!(
                    spec.canvas_rect.fits_canvas(),
                    "{} escapes the 320x200 canvas",
                    surface.id
                );
            }
        }
        // Measured chrome: C01, C02, C05, C08, C10, C11, C14, C15 and CFG08.
        assert_eq!(slices, 9, "the nine-slice chrome must be declared");
    }

    /// **K2 falsifier 1** — only C09 and C16 declare the face, they declare it
    /// once, and the declared record sits at `FACE_AT` at `FACE_SIDE` square.
    #[test]
    fn c09_and_c16_are_the_only_face_surfaces() {
        let create = create_surfaces().expect("create");
        let config = config_surfaces().expect("config");
        let mut face_surfaces: Vec<&'static str> = Vec::new();
        for surface in create.iter().chain(config.iter()) {
            match surface.face_record() {
                Some(record) => {
                    assert_eq!(record, 0, "{} must start on record 0", surface.id);
                    face_surfaces.push(surface.id);
                }
                None => assert!(
                    !surface.assets.contains(&FACE_ASSET),
                    "{} names the face without a record",
                    surface.id
                ),
            }
        }
        assert_eq!(face_surfaces, vec!["C09", "C16"]);

        for id in ["C09", "C16"] {
            let surface = create.iter().find(|s| s.id == id).expect("surface");
            assert_eq!(
                surface
                    .assets
                    .iter()
                    .filter(|asset| **asset == FACE_ASSET)
                    .count(),
                1,
                "{id} declares the face exactly once"
            );
            let spec = surface
                .composition
                .surfaces
                .iter()
                .find(|spec| spec.asset == FACE_ASSET)
                .expect("face spec");
            assert_eq!(spec.canvas_rect, UiRect::new(FACE_AT.0, FACE_AT.1, 64, 64));
            assert_eq!(spec.source, SurfaceSource::Rect(UiRect::new(0, 0, 64, 64)));
        }
    }

    /// **K2 falsifier 2** — binding a selection moves the source cell and
    /// nothing else; a surface without a face refuses the binding.
    #[test]
    fn with_face_binds_the_record() {
        let create = create_surfaces().expect("create");
        let selection = FaceSelection::new(7, 61).expect("7 of 61");
        for id in ["C09", "C16"] {
            let surface = create.iter().find(|s| s.id == id).expect("surface");
            let bound = surface.with_face(selection).expect("bind");
            assert_eq!(bound.face_record(), Some(7), "{id}");
            let spec = bound
                .composition
                .surfaces
                .iter()
                .find(|spec| spec.asset == FACE_ASSET)
                .expect("face spec");
            assert_eq!(
                spec.source,
                SurfaceSource::Rect(UiRect::new(0, 448, 64, 64))
            );
            // Everything else is untouched: the chrome is the same declaration.
            assert_eq!(
                bound.composition.surfaces.len(),
                surface.composition.surfaces.len()
            );
        }
        let without = create.iter().find(|s| s.id == "C01").expect("C01");
        assert_eq!(without.face_record(), None);
        assert!(without.with_face(selection).is_none());
    }

    /// **K2 falsifier 3** — the selection wraps over the measured count, in
    /// both directions, and refuses a pair that names no record.
    #[test]
    fn face_selection_wraps_deterministically() {
        assert_eq!(FaceSelection::new(60, 61).expect("60/61").next().index(), 0);
        assert_eq!(FaceSelection::new(0, 61).expect("0/61").prev().index(), 60);
        let mut walk = FaceSelection::new(0, 61).expect("0/61");
        let mut seen = BTreeSet::new();
        for _ in 0..61 {
            walk = walk.next();
            seen.insert(walk.index());
        }
        assert_eq!(seen.len(), 61, "61 steps visit every record");
        assert_eq!(walk.index(), 0, "61 steps return to the start");
        assert_eq!(FaceSelection::new(61, 61), None);
        assert_eq!(FaceSelection::new(0, 0), None);
        assert_eq!(FaceSelection::new(5, 3), None);
        // The count travels with the selection: it is the measured one.
        assert_eq!(FaceSelection::new(0, 61).expect("0/61").next().count(), 61);
    }

    /// **K2 falsifier 4** — a record outside the atlas never wraps onto record
    /// 0: the source cell leaves the image, so `place()` refuses it.
    #[test]
    fn out_of_range_record_never_wraps() {
        let host = canonical_id_for(CREATE_HOST).expect("host");
        let spec = Layer::Face(61).spec(host);
        assert_eq!(spec.asset, FACE_ASSET);
        assert_eq!(
            spec.source,
            SurfaceSource::Rect(UiRect::new(0, 3904, 64, 64))
        );
        assert_ne!(spec.source, Layer::Face(0).spec(host).source);
    }

    /// **Falsifier 4 (D-2 plan)** — a `Nested` surface whose host is closed is
    /// `unowned_surface`, at declaration level and at node level.
    #[test]
    fn falsifier_4_nested_with_closed_host_is_unowned() {
        let mut nav = Navigation::canonical().expect("canonical shell");
        let surfaces = create_surfaces().expect("create");
        let surface = &surfaces[0];
        assert!(!nav.is_open(surface.host));
        assert_eq!(
            verify_nested_hosts(&surfaces, &nav),
            Err(UiCompatError::unowned_surface())
        );
        let nodes = vec![node(
            UiRect::new(60, 36, 200, 128),
            SurfaceOwnership::Nested(surface.host),
        )];
        assert_eq!(
            surface_ownership_check(&surface.composition, &nodes, &nav),
            Err(UiCompatError::unowned_surface())
        );

        nav.open(surface.host).expect("open host");
        assert_eq!(verify_nested_hosts(&surfaces[..1], &nav), Ok(()));
        assert_eq!(
            surface_ownership_check(&surface.composition, &nodes, &nav),
            Ok(())
        );
    }

    fn surface_ownership_check(
        composition: &WindowComposition,
        nodes: &[SurfaceNode],
        nav: &Navigation,
    ) -> Result<(), UiCompatError> {
        composition.check_ownership(nodes, nav)
    }

    /// **Falsifier 5** — an asset that is not in the bank is an error (an
    /// orphan declaration never paints a blank rectangle).
    #[test]
    fn falsifier_5_missing_asset_is_an_error() {
        let declared = [INFO_ASSET, "NOPE00I0.IMG", SLICE_ASSET];
        let present = |name: &str| name == INFO_ASSET || name == SLICE_ASSET;
        assert_eq!(missing_assets(&declared, &present), vec!["NOPE00I0.IMG"]);
        // Every asset the 27 surfaces declare is carried by the bank.
        let bank = |name: &str| {
            [
                INFO_ASSET, PICK_ASSET, SLICE_ASSET, LGBK_ASSET, CHGN_ASSET, CNFG_ASSET, FACE_ASSET,
            ]
            .contains(&name)
        };
        for surface in create_surfaces()
            .expect("create")
            .into_iter()
            .chain(config_surfaces().expect("config"))
        {
            assert!(
                missing_assets(&surface.assets, &bank).is_empty(),
                "{} declares an orphan asset",
                surface.id
            );
        }
    }

    /// **Falsifier 6** — two compositions that differ in one node are two
    /// different digests (a digest that survives an edit measures nothing).
    #[test]
    fn falsifier_6_digest_detects_a_different_composition() {
        let left = vec![
            node(UiRect::new(0, 0, 320, 200), SurfaceOwnership::Owned),
            node(UiRect::new(12, 148, 120, 44), SurfaceOwnership::Owned),
        ];
        let mut right = left.clone();
        right[1].rect = UiRect::new(12, 148, 120, 43);
        assert_ne!(
            measure_digest("C01", &left, &[], 64000),
            measure_digest("C01", &right, &[], 64000)
        );
        // Same shape, same digest (re-measuring cannot change the number).
        assert_eq!(
            measure_digest("C01", &left, &[], 64000),
            measure_digest("C01", &left.clone(), &[], 64000)
        );
        // The id is part of the canonical form.
        assert_ne!(
            measure_digest("C01", &left, &[], 64000),
            measure_digest("C02", &left, &[], 64000)
        );
    }

    /// **Falsifier 7** — an ambiguous string-map match is rejected: it never
    /// becomes a record identity and it is counted as unmapped.
    #[test]
    fn falsifier_7_ambiguous_string_map_match_is_rejected() {
        let records = vec![
            (14u16, "APPLY".to_string()),
            (300u16, "APPLY".to_string()),
            (301u16, "BIOGRAPHY".to_string()),
        ];
        assert_eq!(classify_literal("APPLY", &records), LiteralMatch::Ambiguous(2));
        assert_eq!(
            classify_literal("BIOGRAPHY", &records),
            LiteralMatch::Unique(301)
        );
        assert_eq!(classify_literal("apply", &records), LiteralMatch::Absent);
        assert_eq!(classify_literal("APPLY ", &records), LiteralMatch::Absent);

        let entries = vec![
            StringMapEntry {
                surface: "C01",
                literal: "APPLY",
                matched: classify_literal("APPLY", &records),
            },
            StringMapEntry {
                surface: "C03",
                literal: "BIOGRAPHY",
                matched: classify_literal("BIOGRAPHY", &records),
            },
        ];
        let counts = string_map_counts(&entries);
        assert_eq!(counts.rsc, 1);
        assert_eq!(counts.literal, 2);
        assert_eq!(counts.unmapped, 1);
        assert_eq!(counts.ambiguous, 1);
    }

    /// The `clip` view window never splits a character and never empties a
    /// non-empty line before the caller sees it.
    #[test]
    fn clip_respects_char_boundaries() {
        assert_eq!(clip("AMBIENT OCCLUSION", 0), "AMBIENT OCCLUSION");
        assert_eq!(clip("AMBIENT OCCLUSION", 7), "AMBIENT");
        assert_eq!(clip("ÃRBOL", 1), "Ã");
        assert_eq!(clip("SHORT", 56), "SHORT");
    }
}

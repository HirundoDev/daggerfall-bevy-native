//! Census of the 85 DFU `UserInterfaceWindows` — spec M34 §1.1, GV-01/GV-02/GV-19.
//!
//! The census is **data, closed**: 85 names (basenames without `.cs`) in
//! ascending byte order — §3.1's transcript rule, the convention that
//! reproduces GV-19 exactly. Measured against
//! `DFU@0038a1e6:Assets/Scripts/Game/UserInterfaceWindows` (85 `.cs` objects):
//!
//! | Slice | N | Bytes | FNV-1a 64 |
//! |:---|---:|---:|:---|
//! | all 85, `\n`, no trailing newline | 85 | 2126 | `1da6a516914e33db` (GV-02 literal) |
//! | playable 51, same rule | 51 | 1399 | `c85e0bb5ae867373` (GV-19 literal) |
//!
//! Both literals are **reproduced**, not adjusted. The orchestrator's working
//! copy `docs/sintergic/l5-mission/m34-census85-CR-2d5c8f41.txt` records
//! `9fad460278a3d1cf` for the same multiset: that value is the
//! **locale-collated** order (`es_MX.UTF-8` `sort`, which transposes
//! `DaggerfallBankingWindow` ↔ `DaggerfallBankPurchasePopUp`). Byte order —
//! the one GV-02/GV-19 themselves are the oracle of — gives the literals.
//! Same 2126 B, same multiset, one transposition; recorded in the W34-1
//! handoff, never forced into a digest to make an assert pass.
//!
//! Categories are the binding partition of §1.1 (GV-01): `CreateChar*` = 16,
//! `*ConfigPage` + `GameEffectsConfigWindow` = 11, the 7 infrastructure
//! windows, and the 51 playable ones. `Config`/`Infra` are `N/A-engine`
//! (§9.5): they **must** appear in the census and are never dressed as 1996
//! UI; omitting them would be a census FAIL, not a silent exclusion.
//!
//! The HUD composition is here too because it is the same instrument (§1.3,
//! GV-03): 15 component instances, 14 registered in the panel, 11 `Show*`
//! visibility flags and 14 distinct types (`TextLabel` twice).

/// Category of one census window (spec M34 §1.1, GV-01).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WindowCategory {
    /// Character creation (`CreateChar*.cs`).
    CreateChar,
    /// Engine configuration pages (`*ConfigPage` + `GameEffectsConfigWindow`).
    Config,
    /// Window infrastructure (base classes, factory, message tables).
    Infra,
    /// Playable 1996 UI with retail assets.
    Playable,
}

impl WindowCategory {
    /// Every category, in [`CENSUS_COUNTS`] order.
    pub const ALL: [Self; 4] = [Self::CreateChar, Self::Config, Self::Infra, Self::Playable];

    /// Stable lower-case name (the literal used by the probe and the docs).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CreateChar => "createchar",
            Self::Config => "config",
            Self::Infra => "infra",
            Self::Playable => "playable",
        }
    }

    /// `true` for the windows declared `N/A-engine` (§9.5): they are engine
    /// configuration/infrastructure, not 1996 UI.
    pub fn is_na_engine(self) -> bool {
        matches!(self, Self::Config | Self::Infra)
    }
}

/// One census row: the DFU class name and its category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CensusEntry {
    /// Class name (file basename without `.cs`).
    pub name: &'static str,
    /// Binding category (§1.1).
    pub category: WindowCategory,
}

/// Const constructor for [`CENSUS_WINDOWS`].
pub const fn entry(name: &'static str, category: WindowCategory) -> CensusEntry {
    CensusEntry { name, category }
}

/// Number of windows in the census (GV-01).
pub const CENSUS_SIZE: usize = 85;

/// The binding partition, in [`WindowCategory::ALL`] order (GV-01).
pub const CENSUS_COUNTS: [usize; 4] = [16, 11, 7, 51];

/// Byte length of the census text (§3.1 rule: `\n` joined, no trailing `\n`).
pub const CENSUS_BYTES: usize = 2126;

/// FNV-1a 64 of the census text = GV-02 literal (reproduced).
pub const CENSUS_DIGEST: u64 = 0x1da6_a516_914e_33db;

/// Byte length of the 51 playable names under the same rule.
pub const PLAYABLE_BYTES: usize = 1399;

/// FNV-1a 64 of the 51 playable names = GV-19 literal (reproduced).
pub const PLAYABLE_DIGEST: u64 = 0xc85e_0bb5_ae86_7373;

/// The 85 windows, ascending byte order (the order of the digest rule).
///
/// Read it as frozen data: the digest of the names in this order **is**
/// [`CENSUS_DIGEST`], and a reordering is a census change (GV-02).
pub const CENSUS_WINDOWS: [CensusEntry; CENSUS_SIZE] = [
    entry("AmbientOcclusionConfigPage", WindowCategory::Config),
    entry("AntialiasingConfigPage", WindowCategory::Config),
    entry("BloomConfigPage", WindowCategory::Config),
    entry("ColorBoostConfigPage", WindowCategory::Config),
    entry("ColorPicker", WindowCategory::Infra),
    entry("CreateCharAddBonusSkills", WindowCategory::CreateChar),
    entry("CreateCharAddBonusStats", WindowCategory::CreateChar),
    entry("CreateCharBiography", WindowCategory::CreateChar),
    entry("CreateCharChooseBio", WindowCategory::CreateChar),
    entry("CreateCharChooseClassGen", WindowCategory::CreateChar),
    entry("CreateCharClassQuestions", WindowCategory::CreateChar),
    entry("CreateCharClassSelect", WindowCategory::CreateChar),
    entry("CreateCharCustomClass", WindowCategory::CreateChar),
    entry("CreateCharFaceSelect", WindowCategory::CreateChar),
    entry("CreateCharGenderSelect", WindowCategory::CreateChar),
    entry("CreateCharNameSelect", WindowCategory::CreateChar),
    entry("CreateCharRaceSelect", WindowCategory::CreateChar),
    entry("CreateCharReflexSelect", WindowCategory::CreateChar),
    entry("CreateCharReputationWindow", WindowCategory::CreateChar),
    entry("CreateCharSpecialAdvantageWindow", WindowCategory::CreateChar),
    entry("CreateCharSummary", WindowCategory::CreateChar),
    entry("DaggerfallAdvancedSettingsWindow", WindowCategory::Playable),
    entry("DaggerfallAutomapWindow", WindowCategory::Playable),
    entry("DaggerfallBankPurchasePopUp", WindowCategory::Playable),
    entry("DaggerfallBankingWindow", WindowCategory::Playable),
    entry("DaggerfallBaseWindow", WindowCategory::Infra),
    entry("DaggerfallBookReaderWindow", WindowCategory::Playable),
    entry("DaggerfallCharacterSheetWindow", WindowCategory::Playable),
    entry("DaggerfallControlsWindow", WindowCategory::Playable),
    entry("DaggerfallCourtWindow", WindowCategory::Playable),
    entry("DaggerfallDaedraSummonedWindow", WindowCategory::Playable),
    entry("DaggerfallEffectSettingsEditorWindow", WindowCategory::Playable),
    entry("DaggerfallExteriorAutomapWindow", WindowCategory::Playable),
    entry("DaggerfallGuildServiceCureDisease", WindowCategory::Playable),
    entry("DaggerfallGuildServiceDonation", WindowCategory::Playable),
    entry("DaggerfallGuildServicePopupWindow", WindowCategory::Playable),
    entry("DaggerfallGuildServiceTraining", WindowCategory::Playable),
    entry("DaggerfallHUD", WindowCategory::Playable),
    entry("DaggerfallInputMessageBox", WindowCategory::Playable),
    entry("DaggerfallInventoryWindow", WindowCategory::Playable),
    entry("DaggerfallItemMakerWindow", WindowCategory::Playable),
    entry("DaggerfallJoystickControlsWindow", WindowCategory::Playable),
    entry("DaggerfallListPickerWindow", WindowCategory::Playable),
    entry("DaggerfallLoadClassicGameWindow", WindowCategory::Playable),
    entry("DaggerfallMerchantRepairPopupWindow", WindowCategory::Playable),
    entry("DaggerfallMerchantServicePopupWindow", WindowCategory::Playable),
    entry("DaggerfallMessageBox", WindowCategory::Playable),
    entry("DaggerfallPauseOptionsWindow", WindowCategory::Playable),
    entry("DaggerfallPlayerHistoryWindow", WindowCategory::Playable),
    entry("DaggerfallPopupWindow", WindowCategory::Infra),
    entry("DaggerfallPotionMakerWindow", WindowCategory::Playable),
    entry("DaggerfallQuestJournalWindow", WindowCategory::Playable),
    entry("DaggerfallQuestOfferWindow", WindowCategory::Playable),
    entry("DaggerfallQuestPopupWindow", WindowCategory::Infra),
    entry("DaggerfallRestWindow", WindowCategory::Playable),
    entry("DaggerfallSpellBookWindow", WindowCategory::Playable),
    entry("DaggerfallSpellMakerWindow", WindowCategory::Playable),
    entry("DaggerfallStartNewGameWizard", WindowCategory::Playable),
    entry("DaggerfallStartWindow", WindowCategory::Playable),
    entry("DaggerfallTalkWindow", WindowCategory::Playable),
    entry("DaggerfallTavernWindow", WindowCategory::Playable),
    entry("DaggerfallTeleportPopUp", WindowCategory::Playable),
    entry("DaggerfallTradeWindow", WindowCategory::Playable),
    entry("DaggerfallTransportWindow", WindowCategory::Playable),
    entry("DaggerfallTravelMapWindow", WindowCategory::Playable),
    entry("DaggerfallTravelPopUp", WindowCategory::Playable),
    entry("DaggerfallUnityMouseControlsWindow", WindowCategory::Playable),
    entry("DaggerfallUnitySaveGameWindow", WindowCategory::Playable),
    entry("DaggerfallUnitySetupGameWizard", WindowCategory::Playable),
    entry("DaggerfallUseMagicItemWindow", WindowCategory::Playable),
    entry("DaggerfallVidPlayerWindow", WindowCategory::Playable),
    entry("DaggerfallWitchesCovenPopupWindow", WindowCategory::Playable),
    entry("DemoClassQuestionsWindow", WindowCategory::Playable),
    entry("DemoFLCWindow", WindowCategory::Playable),
    entry("DepthOfFieldConfigPage", WindowCategory::Config),
    entry("DitherConfigPage", WindowCategory::Config),
    entry("GameEffectConfigPage", WindowCategory::Config),
    entry("GameEffectsConfigWindow", WindowCategory::Config),
    entry("HardStrings", WindowCategory::Infra),
    entry("MotionBlurConfigPage", WindowCategory::Config),
    entry("RetroModeConfigPage", WindowCategory::Config),
    entry("SpellIconPickerWindow", WindowCategory::Playable),
    entry("UIWindowFactory", WindowCategory::Infra),
    entry("VignetteConfigPage", WindowCategory::Config),
    entry("WindowMessages", WindowCategory::Infra),
];

/// The frozen census (85 rows, ascending byte order).
pub fn census() -> &'static [CensusEntry] {
    &CENSUS_WINDOWS
}

/// How many windows the census assigns to `category`.
pub fn count(category: WindowCategory) -> usize {
    CENSUS_WINDOWS
        .iter()
        .filter(|row| row.category == category)
        .count()
}

/// Names of `category`, in census order.
pub fn names(category: WindowCategory) -> impl Iterator<Item = &'static str> {
    CENSUS_WINDOWS
        .iter()
        .filter(move |row| row.category == category)
        .map(|row| row.name)
}

/// Every name of the census, in census order.
pub fn all_names() -> impl Iterator<Item = &'static str> {
    CENSUS_WINDOWS.iter().map(|row| row.name)
}

/// Is the table in the ascending byte order the digest rule requires?
pub fn is_ascending() -> bool {
    CENSUS_WINDOWS
        .windows(2)
        .all(|pair| pair[0].name.as_bytes() < pair[1].name.as_bytes())
}

/// Census text: names joined with `\n`, no trailing newline (§3.1).
pub fn census_text() -> String {
    all_names().collect::<Vec<_>>().join("\n")
}

/// Digest of [`census_text`] recomputed from the table (GV-02).
pub fn census_digest() -> u64 {
    fnv1a64(census_text().as_bytes())
}

/// Names of the 51 playable windows, in census order.
pub fn playable_names() -> impl Iterator<Item = &'static str> {
    names(WindowCategory::Playable)
}

/// Playable text: 51 names joined with `\n`, no trailing newline (§3.1).
pub fn playable_text() -> String {
    playable_names().collect::<Vec<_>>().join("\n")
}

/// Digest of [`playable_text`] recomputed from the table (GV-19).
pub fn playable_digest() -> u64 {
    fnv1a64(playable_text().as_bytes())
}

/// FNV-1a 64 (same family as the M33 digest, spec §0).
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

// ── HUD census (§1.3, GV-03) ────────────────────────────────────────────────

/// Component instances declared by `DaggerfallHUD` (GV-03).
pub const HUD_INSTANCES: usize = 15;

/// Instances added to a panel (7 in the constructor + 7 in `Setup`).
pub const HUD_REGISTERED: usize = 14;

/// `Show*` visibility flags (GV-03).
pub const HUD_FLAGS: usize = 11;

/// Distinct component types (two fields are `TextLabel`).
pub const HUD_TYPES: usize = 14;

/// FNV-1a 64 of the 15 field names, ascending byte order (GV-03 literal).
pub const HUD_DIGEST: u64 = 0x0f4f_d7ef_9652_7f30;

/// One HUD component instance (`DFU@0038a1e6:Assets/Scripts/Game/UserInterfaceWindows/DaggerfallHUD.cs:32-46`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HudField {
    /// Field name in `DaggerfallHUD`.
    pub name: &'static str,
    /// Declared type.
    pub ty: &'static str,
    /// Added to a panel (ctor `:157-163` or `Setup` `:170-193`).
    pub registered: bool,
}

/// The 15 HUD instances, ascending byte order (the order of the GV-03 digest).
///
/// `debugPanel` is declared `null` (`:46`) and only added by its setter
/// (`:125-137`), which is why it is the one unregistered instance.
pub const HUD_FIELDS: [HudField; HUD_INSTANCES] = [
    HudField {
        name: "activeSpells",
        ty: "HUDActiveSpells",
        registered: true,
    },
    HudField {
        name: "arrowCountTextLabel",
        ty: "TextLabel",
        registered: true,
    },
    HudField {
        name: "breathBar",
        ty: "HUDBreathBar",
        registered: true,
    },
    HudField {
        name: "compass",
        ty: "HUDCompass",
        registered: true,
    },
    HudField {
        name: "crosshair",
        ty: "HUDCrosshair",
        registered: true,
    },
    HudField {
        name: "debugPanel",
        ty: "Panel",
        registered: false,
    },
    HudField {
        name: "escortingFaces",
        ty: "EscortingNPCFacePanel",
        registered: true,
    },
    HudField {
        name: "flickerController",
        ty: "HUDFlickerController",
        registered: true,
    },
    HudField {
        name: "interactionModeIcon",
        ty: "HUDInteractionModeIcon",
        registered: true,
    },
    HudField {
        name: "largeHUD",
        ty: "HUDLarge",
        registered: true,
    },
    HudField {
        name: "midScreenTextLabel",
        ty: "TextLabel",
        registered: true,
    },
    HudField {
        name: "placeMarker",
        ty: "HUDPlaceMarker",
        registered: true,
    },
    HudField {
        name: "popupText",
        ty: "PopupText",
        registered: true,
    },
    HudField {
        name: "questDebugger",
        ty: "HUDQuestDebugger",
        registered: true,
    },
    HudField {
        name: "vitals",
        ty: "HUDVitals",
        registered: true,
    },
];

/// The 11 `Show*` visibility flags, in declaration order
/// (`DFU@0038a1e6:…/DaggerfallHUD.cs:53-63`).
pub const HUD_VISIBILITY_FLAGS: [&str; HUD_FLAGS] = [
    "ShowPopupText",
    "ShowMidScreenText",
    "ShowCrosshair",
    "ShowVitals",
    "ShowBreathBar",
    "ShowCompass",
    "ShowInteractionModeIcon",
    "ShowLocalQuestPlaces",
    "ShowEscortingFaces",
    "ShowActiveSpells",
    "ShowArrowCount",
];

/// Instances the table marks as registered.
pub fn hud_registered() -> usize {
    HUD_FIELDS.iter().filter(|field| field.registered).count()
}

/// Distinct types in the HUD table.
pub fn hud_types() -> usize {
    let mut types: Vec<&'static str> = HUD_FIELDS.iter().map(|field| field.ty).collect();
    types.sort_unstable();
    types.dedup();
    types.len()
}

/// HUD text: field names joined with `\n`, no trailing newline (§3.1).
pub fn hud_text() -> String {
    HUD_FIELDS
        .iter()
        .map(|field| field.name)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Digest of [`hud_text`] recomputed from the table (GV-03).
pub fn hud_digest() -> u64 {
    fnv1a64(hud_text().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn census_is_the_closed_85_with_the_binding_partition() {
        assert_eq!(census().len(), CENSUS_SIZE);
        assert_eq!(CENSUS_COUNTS, [16, 11, 7, 51]);
        for (category, expected) in WindowCategory::ALL.into_iter().zip(CENSUS_COUNTS) {
            assert_eq!(count(category), expected, "{}", category.as_str());
        }
    }

    #[test]
    fn census_digest_is_gv_02_and_playable_digest_is_gv_19() {
        assert!(is_ascending(), "the table must be in ascending byte order");
        assert_eq!(census_text().len(), CENSUS_BYTES);
        assert_eq!(census_digest(), CENSUS_DIGEST);
        assert_eq!(playable_text().len(), PLAYABLE_BYTES);
        assert_eq!(playable_digest(), PLAYABLE_DIGEST);
    }

    #[test]
    fn config_and_infra_are_na_engine_and_present() {
        for category in [WindowCategory::Config, WindowCategory::Infra] {
            assert!(category.is_na_engine(), "{}", category.as_str());
            assert!(names(category).next().is_some(), "{}", category.as_str());
        }
        assert!(!WindowCategory::Playable.is_na_engine());
        assert!(!WindowCategory::CreateChar.is_na_engine());
    }

    #[test]
    fn hud_census_matches_gv_03() {
        assert_eq!(HUD_FIELDS.len(), HUD_INSTANCES);
        assert_eq!(hud_registered(), HUD_REGISTERED);
        assert_eq!(HUD_VISIBILITY_FLAGS.len(), HUD_FLAGS);
        assert_eq!(hud_types(), HUD_TYPES);
        assert_eq!(hud_text().len(), 195);
        assert_eq!(hud_digest(), HUD_DIGEST);
        assert!(!HUD_FIELDS
            .windows(2)
            .any(|pair| pair[0].name.as_bytes() >= pair[1].name.as_bytes()));
    }
}

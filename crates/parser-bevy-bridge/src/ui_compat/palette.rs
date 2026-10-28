//! A2 — palette axis: the palette identity of an asset and the ink written on
//! it (plan §3.2).
//!
//! **The rule:** the palette family of a surface is a property of the *asset*
//! (`family_for_asset`) and its usable index window is the *asset-derived*
//! range (`asset_index_range_resolved`) — never a global default. An ink that
//! falls outside that window would either read as the background or paint a
//! foreign colour, so it is a [`UiCompatError::Palette`] (`PaletteViolation`),
//! not a warning.
//!
//! **The default guard:** a `(1, 255)` window is what a substituted default
//! looks like, so [`palette_identity`] rejects it **unless** the asset carries
//! a recorded measurement that produces exactly that window. `PICK03I0.IMG`
//! measures `1..=255` on its own bytes (49 px at index 255), so the guard
//! protects against defaults and never against a measured asset.
//!
//! **Measured ink table** (retail `PAL.PAL`, 776 B = 8 B header + 768 B colour
//! table, read-only from `ARENA2` on 2026-10-27):
//!
//! | Role | Index | `PAL.PAL` RGB | Luminance |
//! |:---|---:|:---|---:|
//! | `Primary` | 127 | `(44, 44, 44)` | 44.0 |
//! | `Secondary` | 119 | `(119, 119, 119)` | 119.0 |
//! | `Disabled` | 112 | `(220, 220, 220)` | 220.0 |
//! | `Alert` | 246 | `(147, 12, 4)` | 40.1 |
//!
//! Index 127 is the same index [`crate::ui_spec::DEFAULT_INK`] uses, which is
//! why the primary role reuses it instead of introducing a second constant.

use daggerfall_parser_rs::image::{
    asset_index_range_derived, asset_index_range_resolved, PaletteFamily,
};

use super::error::{
    UiCompatError, RULE_PALETTE_ASSET, RULE_PALETTE_DERIVED, RULE_PALETTE_FLAT, RULE_PALETTE_INK,
};
use super::surface::SurfaceNode;
use crate::ui_spec::UiAssets;

/// Ink of primary text (index 127).
pub const PRIMARY_INK: u8 = 127;
/// Ink of secondary text (index 119).
pub const SECONDARY_INK: u8 = 119;
/// Ink of disabled text (index 112).
pub const DISABLED_INK: u8 = 112;
/// Ink of alert text (index 246 — the only red of the four).
pub const ALERT_INK: u8 = 246;

/// Semantic ink role of a text run on a surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InkRole {
    /// Readable text.
    Primary,
    /// Supporting text.
    Secondary,
    /// Text that is not actionable.
    Disabled,
    /// Text that demands attention.
    Alert,
}

impl InkRole {
    /// Every role, in declaration order.
    pub fn all() -> [InkRole; 4] {
        [
            Self::Primary,
            Self::Secondary,
            Self::Disabled,
            Self::Alert,
        ]
    }

    /// The palette index this role writes.
    pub fn index(self) -> u8 {
        match self {
            Self::Primary => PRIMARY_INK,
            Self::Secondary => SECONDARY_INK,
            Self::Disabled => DISABLED_INK,
            Self::Alert => ALERT_INK,
        }
    }

    /// Stable name (for reports and registry rows).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Secondary => "secondary",
            Self::Disabled => "disabled",
            Self::Alert => "alert",
        }
    }
}

/// Palette identity of an asset: family, hash and usable index window.
///
/// `hash` is the FNV-1a64 of the resolved palette
/// ([`crate::ui_spec::palette_hash`]), so two assets of the same family that
/// resolve to the same colours share it and two that resolve differently do
/// not — the identity is checkable, not declarative.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteIdentity {
    /// Asset name the identity was derived from.
    pub asset: String,
    /// Palette family of the asset.
    pub family: PaletteFamily,
    /// FNV-1a64 of the resolved palette.
    pub hash: u64,
    /// Asset-derived index window `(low, high)`, both inclusive.
    pub range: (u8, u8),
}

impl PaletteIdentity {
    /// `true` when `ink` lies inside the asset window.
    pub fn contains(&self, ink: u8) -> bool {
        self.range.0 <= ink && ink <= self.range.1
    }

    /// Lowest index of the window — the colour the surface's background uses,
    /// which is what [`ink_contrast`] measures distance from.
    pub fn background(&self) -> u8 {
        self.range.0
    }

    /// Width of the window in indices (`high - low + 1`).
    pub fn span(&self) -> u16 {
        self.range.1 as u16 - self.range.0 as u16 + 1
    }

    /// `true` when the identity has no palette to write text with.
    pub fn is_flat(&self) -> bool {
        self.span() < 2
    }
}

/// Derives the palette identity of `asset` from the bank.
///
/// The reported `range` is [`asset_index_range_resolved`] — the range the
/// parser derives from the asset itself (family rule plus the measured
/// per-asset derivations). A `(1, 255)` window **without** a recorded
/// measurement is rejected outright ([`RULE_PALETTE_DERIVED`]): no family rule
/// produces it, so seeing it means somebody substituted a default for the
/// derivation (§3.2 rule 1). The same window **with** a measurement is the
/// measured reality of that asset — `PICK03I0.IMG` paints index 255 (49 px on
/// its own bytes, `PalFile[255] = (16, 30, 25)`, a real colour) and its
/// recorded derivation is exactly `1..=255` — so it is legitimate and the
/// surface is accepted. The guard protects against defaults, not against
/// measurements.
pub fn palette_identity(
    asset: &str,
    assets: &UiAssets,
) -> Result<PaletteIdentity, UiCompatError> {
    let entry = assets
        .image(asset)
        .ok_or(UiCompatError::Palette(RULE_PALETTE_ASSET))?;
    let derived = asset_index_range_resolved(asset);
    let range = (*derived.start(), *derived.end());
    reject_substituted_default(asset, range)?;
    Ok(PaletteIdentity {
        asset: entry.name.clone(),
        family: entry.family,
        hash: entry.palette_hash,
        range,
    })
}

/// Guards the window against a **substituted default** (§3.2 rule 1).
///
/// `(1, 255)` is what a blanket default looks like when the derivation is
/// skipped, so it is rejected — **unless** the asset carries a recorded
/// measurement that produces exactly that window
/// ([`asset_index_range_derived`], the [`daggerfall_parser_rs::image::ASSET_RANGE_DERIVATIONS`]
/// table). The invariant is therefore `(1, 255)` without a derivation ⇒
/// default, not `(1, 255)` ⇒ default: `PICK03I0.IMG` measures that window.
fn reject_substituted_default(asset: &str, range: (u8, u8)) -> Result<(), UiCompatError> {
    if range == (1, 255) && asset_index_range_derived(asset).is_none() {
        return Err(UiCompatError::Palette(RULE_PALETTE_DERIVED));
    }
    Ok(())
}

/// Resolves the palette index a role writes **on this surface**.
///
/// Fails with [`UiCompatError::Palette`] when the surface is flat or when the
/// role's index falls outside the surface window.
pub fn ink_for(surface: &SurfaceNode, role: InkRole) -> Result<u8, UiCompatError> {
    if surface.palette.is_flat() {
        return Err(UiCompatError::Palette(RULE_PALETTE_FLAT));
    }
    let ink = role.index();
    if !surface.palette.contains(ink) {
        return Err(UiCompatError::Palette(RULE_PALETTE_INK));
    }
    Ok(ink)
}

/// Index distance between `ink` and the surface background.
///
/// This is the measurable form of "readable": 0 means the ink **is** the
/// background (invisible text), anything else means the two indices differ, so
/// the compositor writes a colour the background does not already have. It is
/// index-space distance, not perceptual contrast — the perceptual question
/// needs the resolved RGB and belongs to the presentation axis, not here.
pub fn ink_contrast(surface: &SurfaceNode, ink: u8) -> u32 {
    ink.abs_diff(surface.palette.background()) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_compat::surface::{place, SurfaceOwnership, SurfaceSource, SurfaceSpec};
    use crate::ui_compat::test_support::{bank, image};
    use crate::ui_spec::UiRect;
    use daggerfall_parser_rs::image::PaletteFamily;

    fn surface_in(assets: &UiAssets, asset: &str) -> SurfaceNode {
        let image = assets.image(asset).expect("bank asset");
        let spec = SurfaceSpec {
            asset: asset.to_string(),
            canvas_rect: UiRect::new(0, 0, image.image.width, image.image.height),
            source: SurfaceSource::Whole,
            ownership: SurfaceOwnership::Owned,
        };
        place(&spec, assets).expect("surface")
    }

    fn surface(asset: &str) -> SurfaceNode {
        surface_in(&bank(), asset)
    }

    /// A2 rule 1 — the reported window is the asset-derived one, not a family
    /// default and not a global `1..=255`.
    ///
    /// Violation caught: reporting `family_index_range(entry.family)` (or a
    /// constant) instead of `asset_index_range_resolved(asset)`. `MASK99I0.IMG`
    /// belongs to the HUD family (0..=254) but its own window is 144..=158, and
    /// `MAP100I0.IMG` carries a measured derivation (16..=254) narrower than a
    /// blanket default.
    #[test]
    fn falsifier_a2_range_is_asset_derived() {
        let assets = bank();
        let mask = palette_identity("MASK99I0.IMG", &assets).expect("mask identity");
        assert_eq!(mask.family, PaletteFamily::Hud);
        assert_eq!(mask.range, (144, 158));
        assert_ne!(mask.range, (0, 254));

        let map = palette_identity("MAP100I0.IMG", &assets).expect("map identity");
        assert_eq!(map.family, PaletteFamily::Map);
        assert_eq!(map.range, (16, 254));
        assert_ne!(map.range, (1, 255));
        assert_ne!(map.range, (63, 254));

        let main = palette_identity("MAIN00I0.IMG", &assets).expect("hud identity");
        assert_eq!(main.range, (0, 254));
        assert_eq!(
            main.hash,
            assets.image("MAIN00I0.IMG").expect("asset").palette_hash
        );
        // Same family, same resolved palette: the hash follows the colours.
        assert_eq!(main.hash, mask.hash);
        // Different family: a different palette hash.
        assert_ne!(main.hash, map.hash);
    }

    /// A2 rule 1 — a `(1, 255)` window is a **substituted default** unless the
    /// asset carries a recorded measurement that produces it.
    ///
    /// Violation caught in both directions: rejecting the *measured*
    /// `PICK03I0.IMG` window (`1..=255` — its own bytes paint index 255, 49 px
    /// of a real `PAL.PAL` colour) because it looks like the blanket default,
    /// and accepting a `(1, 255)` window for an asset with **no** measurement
    /// at all, which is exactly the default the rule forbids.
    #[test]
    fn falsifier_a2_default_window_needs_a_measured_derivation() {
        let assets = bank()
            .with_image("PICK03I0.IMG", image(320, 200, 1))
            .expect("pick screen");

        // Measured: the derivation is the asset's own span, ceiling included.
        let pick = palette_identity("PICK03I0.IMG", &assets).expect("measured pick identity");
        assert_eq!(pick.family, PaletteFamily::Hud);
        assert_eq!(pick.range, (1, 255));
        assert_ne!(pick.range, (0, 254), "the Hud ceiling is not this asset's window");
        let pick_surface = surface_in(&assets, "PICK03I0.IMG");
        assert_eq!(ink_for(&pick_surface, InkRole::Primary), Ok(PRIMARY_INK));
        assert_eq!(PRIMARY_INK, 127);
        assert_eq!(ink_contrast(&pick_surface, PRIMARY_INK), 126);

        // Unmeasured: the same window is a substituted default and is rejected.
        assert_eq!(
            reject_substituted_default("MAIN00I0.IMG", (1, 255)),
            Err(UiCompatError::Palette(RULE_PALETTE_DERIVED))
        );
        // The asset with a measurement may not be rejected by the same guard …
        assert_eq!(reject_substituted_default("PICK03I0.IMG", (1, 255)), Ok(()));
        // … and no other window is touched by it.
        assert_eq!(reject_substituted_default("MAIN00I0.IMG", (0, 254)), Ok(()));
        assert_eq!(reject_substituted_default("MASK99I0.IMG", (144, 158)), Ok(()));
    }

    /// A2 rule 2 — an ink outside the asset window is a `PaletteViolation`.
    ///
    /// Violation caught: accepting the role's index because it is inside the
    /// *family* window (127 is a legal HUD index) while it is outside the
    /// *asset* window of a mask (144..=158). Every role fails on the mask, and
    /// the same roles succeed on a HUD sprite.
    #[test]
    fn falsifier_a2_ink_outside_asset_window_is_violation() {
        let mask = surface("MASK99I0.IMG");
        for role in InkRole::all() {
            assert_eq!(
                ink_for(&mask, role),
                Err(UiCompatError::Palette(RULE_PALETTE_INK)),
                "role {} must not be accepted on a mask surface",
                role.as_str()
            );
        }
        assert_eq!(
            ink_for(&mask, InkRole::Primary),
            Err(UiCompatError::palette_violation())
        );

        let hud = surface("MAIN00I0.IMG");
        for role in InkRole::all() {
            assert_eq!(ink_for(&hud, role), Ok(role.index()));
        }
    }

    /// A2 rule 2/3 — a window with a single index has no readable ink, and the
    /// contrast measure is what makes "readable" checkable.
    ///
    /// Violation caught: a flat window (`span() == 1`) being treated as a
    /// valid text surface, and `ink_contrast` returning a constant or an
    /// absolute value that cannot distinguish the background from real ink.
    #[test]
    fn falsifier_a2_flat_window_and_contrast() {
        let mut flat = surface("MAIN00I0.IMG");
        flat.palette.range = (127, 127);
        assert!(flat.palette.is_flat());
        assert_eq!(
            ink_for(&flat, InkRole::Primary),
            Err(UiCompatError::Palette(RULE_PALETTE_FLAT))
        );

        let hud = surface("MAIN00I0.IMG");
        assert_eq!(ink_contrast(&hud, hud.palette.background()), 0);
        assert_eq!(ink_contrast(&hud, PRIMARY_INK), PRIMARY_INK as u32);
        assert_eq!(ink_contrast(&hud, ALERT_INK), ALERT_INK as u32);
        assert!(InkRole::all().iter().all(|role| ink_contrast(&hud, role.index()) > 0));
        assert!(image(2, 2, 1).width == 2);
    }
}

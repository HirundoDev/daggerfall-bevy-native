//! A1 — surface axis: where every painted thing comes from and who owns it
//! (plan §3.1, technical addendum §3.10.1).
//!
//! A [`SurfaceSpec`] is a **declaration**: asset + canvas rectangle + source
//! selector + ownership. [`place`] turns it into a [`SurfaceNode`] — the same
//! data the M17 compositor already understands — and enforces four rules the
//! compositor cannot, because it only receives nodes that were already
//! resolved:
//!
//! 1. **No orphan asset:** the declared asset exists in the bank.
//! 2. **No silent clipping:** the destination fits the 320×200 canvas, the
//!    source cell lies inside the decoded image, and a whole-asset source
//!    covers the whole asset (clipping must be declared as a rectangle).
//! 3. **No partial nine-slice:** the nine pieces of a 3×3 source cover the
//!    target exactly once (area sum equals target area, every pixel once).
//! 4. **No unowned surface:** a nested surface names a window that is open.
//!
//! **Kind is derived, never declared:** a `MASK*` asset (window 144..=158) is
//! [`SurfaceKind::Mask`], a nine-slice source is [`SurfaceKind::Sliced`] and
//! everything else is [`SurfaceKind::Sprite`]. A caller that could declare
//! `Mask` over a HUD sprite would be declaring a lie.
//!
//! **Stretch:** the M17 blitter is 1:1. A nine-slice target larger than
//! `2 × side` needs centre/edge replication, so [`WindowComposition::paint`]
//! replicates nearest-neighbour **only** for the pieces whose destination is
//! not their source cell, and registers the palette slot exactly as
//! `Framebuffer::slot_for` does so `canvas_rgba`/`palette_compliance` stay
//! valid. Every other piece still leaves through
//! [`crate::ui_spec::Framebuffer::paint_nodes`] — there is no second blitter.

use std::array;

use daggerfall_parser_rs::image::{asset_index_range_resolved, DecodedImage};
use daggerfall_nervous::events::WindowId;

use super::error::{
    UiCompatError, RULE_SURFACE_CANVAS, RULE_SURFACE_COVERAGE, RULE_SURFACE_MINIMUM,
    RULE_SURFACE_ORPHAN, RULE_SURFACE_RECORD, RULE_SURFACE_SOURCE, RULE_SURFACE_WHOLE,
};
use super::navigation::Navigation;
use super::palette::{palette_identity, PaletteIdentity};
use crate::ui_spec::{
    Framebuffer, FramebufferPalette, UiAnchor, UiAsset, UiAssets, UiNode, UiNodeKind, UiRect,
    DEFAULT_INK,
};

/// Where the pixels of a surface come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceSource {
    /// The whole decoded asset, placed whole.
    Whole,
    /// A cell of the asset (clip or sub-image), placed 1:1.
    Rect(UiRect),
    /// One record of an RCI record container (`SPOP.RCI`, `BUTN*.IMG`…).
    RciRecord {
        /// Record index within the container.
        record: u16,
    },
}

/// Who owns the surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceOwnership {
    /// The surface belongs to its own window.
    Owned,
    /// The surface is drawn inside another window's frame; that window must
    /// be open (§3.1 rule 4).
    Nested(WindowId),
    /// The surface belongs to a window that lives outside this composition
    /// (the HUD frame is always on screen).
    External,
}

/// Primitive kind of a placed surface — always derived from the asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceKind {
    /// Nine-slice piece from a 3×3 container.
    Sliced,
    /// Plain palette-index sprite.
    Sprite,
    /// `MASK*` surface (window 144..=158), painted by the mask rules.
    Mask,
}

impl SurfaceKind {
    /// Stable name (for reports).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sliced => "sliced",
            Self::Sprite => "sprite",
            Self::Mask => "mask",
        }
    }
}

/// A declared surface, before it is resolved against the asset bank.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceSpec {
    /// Asset name in the bank.
    pub asset: String,
    /// Destination rectangle in the 320×200 canvas.
    pub canvas_rect: UiRect,
    /// Source selector inside the asset.
    pub source: SurfaceSource,
    /// Owner of the surface.
    pub ownership: SurfaceOwnership,
}

impl SurfaceSpec {
    /// A surface owned by its own window.
    pub fn owned(asset: &str, canvas_rect: UiRect, source: SurfaceSource) -> Self {
        Self {
            asset: asset.to_string(),
            canvas_rect,
            source,
            ownership: SurfaceOwnership::Owned,
        }
    }

    /// A surface drawn inside `owner`'s frame.
    pub fn nested(
        asset: &str,
        canvas_rect: UiRect,
        source: SurfaceSource,
        owner: WindowId,
    ) -> Self {
        Self {
            asset: asset.to_string(),
            canvas_rect,
            source,
            ownership: SurfaceOwnership::Nested(owner),
        }
    }

    /// A surface owned by a window outside this composition.
    pub fn external(asset: &str, canvas_rect: UiRect, source: SurfaceSource) -> Self {
        Self {
            asset: asset.to_string(),
            canvas_rect,
            source,
            ownership: SurfaceOwnership::External,
        }
    }
}

/// A resolved surface: what the compositor paints and what the gates assert.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceNode {
    /// Asset name.
    pub asset: String,
    /// Destination rectangle.
    pub rect: UiRect,
    /// Resolved source cell (`None` = whole asset).
    pub source: Option<UiRect>,
    /// Owner.
    pub ownership: SurfaceOwnership,
    /// Palette identity of the asset.
    pub palette: PaletteIdentity,
    /// Derived kind.
    pub kind: SurfaceKind,
}

impl SurfaceNode {
    /// `true` when this node is a nine-slice piece.
    pub fn is_slice(&self) -> bool {
        self.kind == SurfaceKind::Sliced
    }

    /// The M17 node equivalent, ready for
    /// [`crate::ui_spec::Framebuffer::paint_nodes`].
    ///
    /// A sprite node writes index 0 as transparent and never uses the ink; the
    /// glyph path builds its own nodes ([`super::typography::paint_runs`]).
    pub fn ui_node(&self) -> UiNode {
        UiNode {
            id: format!("{}@{},{}", self.asset, self.rect.x, self.rect.y),
            kind: UiNodeKind::Sprite,
            rect: self.rect,
            anchor: UiAnchor::Declared("ui_compat::surface::place"),
            asset: self.asset.clone(),
            family: self.palette.family,
            palette_hash: self.palette.hash,
            source: self.source,
            text: None,
            ink: DEFAULT_INK,
        }
    }
}

/// Side of one cell when the decoded image is a raw 3×3 grid, `None` otherwise.
///
/// The container is measured on the **decoded** image (`SPOP.RCI` decodes to a
/// 66×66 grid of nine 22×22 records), never assumed: a 40×40 image is not a
/// nine-slice, it is a sprite.
pub fn nine_slice_side(image: &DecodedImage) -> Option<u16> {
    let side = image.width / 3;
    (image.width == image.height && image.width.is_multiple_of(3) && side >= 2).then_some(side)
}

/// Kind derived from the asset name's index window.
fn sprite_kind(asset: &str) -> SurfaceKind {
    let range = asset_index_range_resolved(asset);
    if *range.start() == 144 && *range.end() == 158 {
        SurfaceKind::Mask
    } else {
        SurfaceKind::Sprite
    }
}

fn cell_fits(cell: UiRect, image: &DecodedImage) -> bool {
    cell.width > 0
        && cell.height > 0
        && cell.x >= 0
        && cell.y >= 0
        && cell.right() as i32 <= image.width as i32
        && cell.bottom() as i32 <= image.height as i32
}

fn record_cell(image: &DecodedImage, record: u16) -> Result<UiRect, UiCompatError> {
    if record >= 9 {
        return Err(UiCompatError::Surface(RULE_SURFACE_RECORD));
    }
    if let Some(side) = nine_slice_side(image) {
        let column = record % 3;
        let row = record / 3;
        return Ok(UiRect::new(
            (column * side) as i16,
            (row * side) as i16,
            side,
            side,
        ));
    }
    let strip = image.width.checked_mul(9);
    if image.width > 0 && strip == Some(image.height) {
        return Ok(UiRect::new(
            0,
            record as i16 * image.width as i16,
            image.width,
            image.width,
        ));
    }
    if record == 0 {
        return Ok(UiRect::new(0, 0, image.width, image.height));
    }
    Err(UiCompatError::Surface(RULE_SURFACE_RECORD))
}

/// Resolves a declared surface against the bank (rules 1 and 2).
pub fn place(spec: &SurfaceSpec, assets: &UiAssets) -> Result<SurfaceNode, UiCompatError> {
    let asset = assets
        .image(&spec.asset)
        .ok_or(UiCompatError::Surface(RULE_SURFACE_ORPHAN))?;
    if !spec.canvas_rect.fits_canvas() {
        return Err(UiCompatError::Surface(RULE_SURFACE_CANVAS));
    }
    let image = &asset.image;
    let (source, kind) = match spec.source {
        SurfaceSource::Whole => {
            if spec.canvas_rect.width != image.width || spec.canvas_rect.height != image.height {
                return Err(UiCompatError::Surface(RULE_SURFACE_WHOLE));
            }
            (None, sprite_kind(&spec.asset))
        }
        SurfaceSource::Rect(cell) => {
            if !cell_fits(cell, image)
                || cell.width < spec.canvas_rect.width
                || cell.height < spec.canvas_rect.height
            {
                return Err(UiCompatError::Surface(RULE_SURFACE_SOURCE));
            }
            (Some(cell), sprite_kind(&spec.asset))
        }
        SurfaceSource::RciRecord { record } => {
            let cell = record_cell(image, record)?;
            if cell.width < spec.canvas_rect.width || cell.height < spec.canvas_rect.height {
                return Err(UiCompatError::Surface(RULE_SURFACE_SOURCE));
            }
            (Some(cell), sprite_kind(&spec.asset))
        }
    };
    Ok(SurfaceNode {
        asset: asset.name.clone(),
        rect: spec.canvas_rect,
        source,
        ownership: spec.ownership,
        palette: palette_identity(&spec.asset, assets)?,
        kind,
    })
}

/// Resolves a nine-slice declaration into its nine pieces (rule 3).
///
/// The source must be [`SurfaceSource::Whole`] and the asset must decode to a
/// 3×3 grid; the target must be at least `2 × side` on both axes and the nine
/// destination rectangles must cover it exactly once — checked by area **and**
/// by geometry, so an off-by-one in the middle column cannot pass.
pub fn place_nine_slice(
    spec: &SurfaceSpec,
    assets: &UiAssets,
) -> Result<[SurfaceNode; 9], UiCompatError> {
    let asset = assets
        .image(&spec.asset)
        .ok_or(UiCompatError::Surface(RULE_SURFACE_ORPHAN))?;
    if !spec.canvas_rect.fits_canvas() {
        return Err(UiCompatError::Surface(RULE_SURFACE_CANVAS));
    }
    if !matches!(spec.source, SurfaceSource::Whole) {
        return Err(UiCompatError::Surface(RULE_SURFACE_WHOLE));
    }
    let side = nine_slice_side(&asset.image).ok_or(UiCompatError::Surface(RULE_SURFACE_RECORD))?;
    let target = spec.canvas_rect;
    if target.width < side * 2 || target.height < side * 2 {
        return Err(UiCompatError::Surface(RULE_SURFACE_MINIMUM));
    }
    let palette = palette_identity(&spec.asset, assets)?;
    let offsets_x = [0i16, side as i16, target.width as i16 - side as i16];
    let offsets_y = [0i16, side as i16, target.height as i16 - side as i16];
    let widths = [side, target.width - 2 * side, side];
    let heights = [side, target.height - 2 * side, side];
    let nodes: [SurfaceNode; 9] = array::from_fn(|index| {
        let column = index % 3;
        let row = index / 3;
        SurfaceNode {
            asset: spec.asset.clone(),
            rect: UiRect::new(
                target.x + offsets_x[column],
                target.y + offsets_y[row],
                widths[column],
                heights[row],
            ),
            source: Some(UiRect::new(
                column as i16 * side as i16,
                row as i16 * side as i16,
                side,
                side,
            )),
            ownership: spec.ownership,
            palette: palette.clone(),
            kind: SurfaceKind::Sliced,
        }
    });
    let mut covered = 0u32;
    for node in &nodes {
        if !node.rect.fits_canvas() {
            return Err(UiCompatError::Surface(RULE_SURFACE_CANVAS));
        }
        covered += node.rect.width as u32 * node.rect.height as u32;
    }
    if covered != target.width as u32 * target.height as u32 {
        return Err(UiCompatError::Surface(RULE_SURFACE_COVERAGE));
    }
    // Geometry, not just area: the three bands must abut (left + middle = right)
    // on both axes, so two pieces can never overlap while the sum still adds up.
    for bands in [
        (offsets_x, widths),
        (offsets_y, heights),
    ] {
        let (offsets, sizes) = bands;
        if offsets[0] + sizes[0] as i16 != offsets[1]
            || offsets[1] + sizes[1] as i16 != offsets[2]
        {
            return Err(UiCompatError::Surface(RULE_SURFACE_COVERAGE));
        }
    }
    Ok(nodes)
}

/// Rule 4: every nested surface names a window that is open.
///
/// `place` stays pure (it takes no navigation), so ownership is verified at
/// composition entry with the host stack in hand.
pub fn verify_ownership(nodes: &[SurfaceNode], nav: &Navigation) -> Result<(), UiCompatError> {
    for node in nodes {
        if let SurfaceOwnership::Nested(owner) = node.ownership {
            if !nav.is_open(owner) {
                return Err(UiCompatError::unowned_surface());
            }
        }
    }
    Ok(())
}

/// A window's surface composition: ordered declarations, painted in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowComposition {
    /// Window the composition belongs to.
    pub window: WindowId,
    /// Declarations in painting order (back to front).
    pub surfaces: Vec<SurfaceSpec>,
}

impl WindowComposition {
    /// Ordered composition for one window.
    pub fn new(window: WindowId, surfaces: Vec<SurfaceSpec>) -> Self {
        Self { window, surfaces }
    }

    /// Resolves every declaration into paint-ready nodes.
    ///
    /// A nine-slice declaration expands to its nine pieces (in row-major
    /// order), so `nodes().len()` is `≥ surfaces.len()`.
    pub fn nodes(&self, assets: &UiAssets) -> Result<Vec<SurfaceNode>, UiCompatError> {
        let mut nodes = Vec::new();
        for spec in &self.surfaces {
            if is_nine_slice_spec(spec, assets) {
                nodes.extend(place_nine_slice(spec, assets)?);
            } else {
                nodes.push(place(spec, assets)?);
            }
        }
        Ok(nodes)
    }

    /// Ownership check for the resolved nodes (rule 4).
    pub fn check_ownership(
        &self,
        nodes: &[SurfaceNode],
        nav: &Navigation,
    ) -> Result<(), UiCompatError> {
        verify_ownership(nodes, nav)
    }

    /// Paints the composition onto `framebuffer` in declaration order.
    ///
    /// Every piece whose destination equals its source cell goes through
    /// [`crate::ui_spec::Framebuffer::paint_nodes`]; only a stretched nine-slice
    /// piece (destination larger than its cell) is replicated here, because the
    /// compositor is 1:1 by contract.
    pub fn paint(
        &self,
        framebuffer: &mut Framebuffer,
        assets: &UiAssets,
    ) -> Result<(), UiCompatError> {
        for node in self.nodes(assets)? {
            match node.source {
                Some(source)
                    if source.width != node.rect.width || source.height != node.rect.height =>
                {
                    paint_stretched(framebuffer, &node, source, assets)?;
                }
                _ => framebuffer
                    .paint_nodes(std::slice::from_ref(&node.ui_node()), assets)
                    .map_err(UiCompatError::from_spec)?,
            }
        }
        Ok(())
    }
}

fn is_nine_slice_spec(spec: &SurfaceSpec, assets: &UiAssets) -> bool {
    if !matches!(spec.source, SurfaceSource::Whole) {
        return false;
    }
    assets.image(&spec.asset).is_some_and(|asset| {
        nine_slice_side(&asset.image).is_some()
            && (asset.image.width != spec.canvas_rect.width
                || asset.image.height != spec.canvas_rect.height)
    })
}

/// Registers (or reuses) the palette slot of a node, exactly as
/// `Framebuffer::slot_for` does, so the stretched pixels stay attributable.
fn palette_slot(framebuffer: &mut Framebuffer, node: &SurfaceNode, asset: &UiAsset) -> u8 {
    if let Some(index) = framebuffer
        .palettes
        .iter()
        .position(|entry| entry.asset == node.asset && entry.hash == node.palette.hash)
    {
        return index as u8;
    }
    framebuffer.palettes.push(FramebufferPalette {
        asset: node.asset.clone(),
        family: node.palette.family,
        hash: node.palette.hash,
        colors: asset.palette.colors,
    });
    (framebuffer.palettes.len() - 1) as u8
}

/// Nearest-neighbour replication of one piece whose target is larger than its
/// source cell.
fn paint_stretched(
    framebuffer: &mut Framebuffer,
    node: &SurfaceNode,
    source: UiRect,
    assets: &UiAssets,
) -> Result<(), UiCompatError> {
    let asset = assets
        .image(&node.asset)
        .ok_or(UiCompatError::Surface(RULE_SURFACE_ORPHAN))?;
    if !node.rect.fits_canvas() {
        return Err(UiCompatError::Surface(RULE_SURFACE_CANVAS));
    }
    if node.rect.width == 0 || node.rect.height == 0 {
        // A degenerate piece covers nothing: `place_nine_slice` already
        // accounted for it in the area sum, so there is nothing to paint.
        return Ok(());
    }
    let slot = palette_slot(framebuffer, node, asset);
    let range = asset_index_range_resolved(&node.asset);
    for dy in 0..node.rect.height {
        let sy = source.y as u32 + (dy as u32 * source.height as u32) / node.rect.height as u32;
        for dx in 0..node.rect.width {
            let sx = source.x as u32 + (dx as u32 * source.width as u32) / node.rect.width as u32;
            let Some(index) = asset.image.pixel(sx as u16, sy as u16) else {
                return Err(UiCompatError::Surface(RULE_SURFACE_SOURCE));
            };
            if index == 0 {
                continue;
            }
            if !range.contains(&index) {
                return Err(UiCompatError::palette_violation());
            }
            let x = node.rect.x as u16 + dx;
            let y = node.rect.y as u16 + dy;
            let at = y as usize * framebuffer.width as usize + x as usize;
            framebuffer.pixels[at] = index;
            framebuffer.slots[at] = slot;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use daggerfall_parser_rs::image::PaletteFamily;

    fn bank() -> UiAssets {
        crate::ui_compat::test_support::bank()
    }

    /// A1 rule 1 — a surface naming an asset outside the bank is an error.
    ///
    /// Violation caught: `place` returning a node with an unresolved asset (or
    /// the compositor's `MissingAsset` surfacing at paint time, after a partial
    /// frame).
    #[test]
    fn falsifier_a1_orphan_asset_is_error() {
        let assets = bank();
        let spec = SurfaceSpec::owned("GHOST00I0.IMG", UiRect::new(0, 0, 32, 32), SurfaceSource::Whole);
        assert_eq!(
            place(&spec, &assets),
            Err(UiCompatError::Surface(RULE_SURFACE_ORPHAN))
        );
        let slice = SurfaceSpec::owned("GHOST00I0.IMG", UiRect::new(0, 0, 64, 64), SurfaceSource::Whole);
        assert_eq!(
            place_nine_slice(&slice, &assets),
            Err(UiCompatError::Surface(RULE_SURFACE_ORPHAN))
        );
    }

    /// A1 rule 2 — a destination outside the canvas, or a source cell outside
    /// the image, is refused instead of clipped.
    ///
    /// Violation caught: a rectangle whose right/bottom edge crosses 320×200
    /// (the blitter would index out of bounds or drop pixels silently) and a
    /// whole-asset placement into a smaller rectangle (an undeclared clip).
    #[test]
    fn falsifier_a1_rect_must_fit_canvas() {
        let assets = bank();
        let edge = SurfaceSpec::owned(
            "MAIN00I0.IMG",
            UiRect::new(300, 190, 40, 20),
            SurfaceSource::Rect(UiRect::new(0, 0, 40, 20)),
        );
        assert_eq!(
            place(&edge, &assets),
            Err(UiCompatError::Surface(RULE_SURFACE_CANVAS))
        );

        let clipped = SurfaceSpec::owned("MAIN00I0.IMG", UiRect::new(0, 0, 40, 20), SurfaceSource::Whole);
        assert_eq!(
            place(&clipped, &assets),
            Err(UiCompatError::Surface(RULE_SURFACE_WHOLE))
        );

        let outside = SurfaceSpec::owned(
            "MAIN00I0.IMG",
            UiRect::new(0, 0, 40, 20),
            SurfaceSource::Rect(UiRect::new(300, 40, 40, 20)),
        );
        assert_eq!(
            place(&outside, &assets),
            Err(UiCompatError::Surface(RULE_SURFACE_SOURCE))
        );

        let slice = SurfaceSpec::owned(
            "SPOP.RCI",
            UiRect::new(200, 0, 200, 100),
            SurfaceSource::Whole,
        );
        assert_eq!(
            place_nine_slice(&slice, &assets),
            Err(UiCompatError::Surface(RULE_SURFACE_CANVAS))
        );
    }

    /// A1 rule 3 — the nine pieces cover the target exactly once.
    ///
    /// Violation caught: an off-by-one in the middle column/row (area sum still
    /// close), a piece placed at the wrong offset (a target pixel painted twice
    /// or never), and a target smaller than its own corners.
    #[test]
    fn falsifier_a1_nine_pieces_cover_target_once() {
        let assets = bank();
        let target = UiRect::new(10, 20, 100, 80);
        let spec = SurfaceSpec::owned("SPOP.RCI", target, SurfaceSource::Whole);
        let pieces = place_nine_slice(&spec, &assets).expect("nine pieces");
        assert_eq!(pieces.len(), 9);
        assert!(pieces.iter().all(|piece| piece.is_slice()));
        assert!(pieces
            .iter()
            .all(|piece| piece.source.map(|cell| (cell.width, cell.height)) == Some((22, 22))));
        assert_eq!(pieces[0].source, Some(UiRect::new(0, 0, 22, 22)));
        assert_eq!(pieces[8].source, Some(UiRect::new(44, 44, 22, 22)));

        let mut counts = vec![0u8; target.width as usize * target.height as usize];
        let mut area = 0u32;
        for piece in &pieces {
            area += piece.rect.width as u32 * piece.rect.height as u32;
            for y in piece.rect.y..piece.rect.bottom() {
                for x in piece.rect.x..piece.rect.right() {
                    let at = (y as usize - target.y as usize) * target.width as usize
                        + (x as usize - target.x as usize);
                    counts[at] += 1;
                }
            }
        }
        assert_eq!(area, target.width as u32 * target.height as u32);
        assert!(counts.iter().all(|count| *count == 1));
        assert_eq!(
            pieces[4].rect,
            UiRect::new(10 + 22, 20 + 22, 100 - 44, 80 - 44)
        );

        let tiny = SurfaceSpec::owned("SPOP.RCI", UiRect::new(0, 0, 40, 40), SurfaceSource::Whole);
        assert_eq!(
            place_nine_slice(&tiny, &assets),
            Err(UiCompatError::Surface(RULE_SURFACE_MINIMUM))
        );
    }

    /// A1 rule 4 — a nested surface needs an open owner.
    ///
    /// Violation caught: a surface drawn inside a window that is not open (the
    /// detector needs `&Navigation`, which `place` deliberately does not take).
    #[test]
    fn falsifier_a1_nested_owner_must_be_open() {
        let assets = bank();
        let mut nav = Navigation::canonical().expect("canonical shell");
        let spec = SurfaceSpec::nested(
            "MAIN00I0.IMG",
            UiRect::new(0, 154, 320, 46),
            SurfaceSource::Whole,
            WindowId::INVENTORY,
        );
        let node = place(&spec, &assets).expect("node");
        assert_eq!(
            verify_ownership(std::slice::from_ref(&node), &nav),
            Err(UiCompatError::unowned_surface())
        );
        nav.open(WindowId::INVENTORY).expect("open inventory");
        assert_eq!(verify_ownership(&[node], &nav), Ok(()));

        let external = place(
            &SurfaceSpec::external(
                "MAIN00I0.IMG",
                UiRect::new(0, 154, 320, 46),
                SurfaceSource::Whole,
            ),
            &assets,
        )
        .expect("external node");
        let empty = Navigation::canonical().expect("canonical shell");
        assert_eq!(verify_ownership(&[external], &empty), Ok(()));
    }

    /// A1 derived kind — the mask window (144..=158) is never a plain sprite.
    ///
    /// Violation caught: a declared kind. `MASK99I0.IMG` is HUD-family, so only
    /// the asset window distinguishes it.
    #[test]
    fn derived_kind_comes_from_the_asset() {
        let assets = bank();
        let mask = place(
            &SurfaceSpec::owned("MASK99I0.IMG", UiRect::new(0, 0, 64, 64), SurfaceSource::Whole),
            &assets,
        )
        .expect("mask");
        assert_eq!(mask.kind, SurfaceKind::Mask);
        assert_eq!(mask.palette.family, PaletteFamily::Hud);

        let sprite = place(
            &SurfaceSpec::owned("MAIN00I0.IMG", UiRect::new(0, 154, 320, 46), SurfaceSource::Whole),
            &assets,
        )
        .expect("sprite");
        assert_eq!(sprite.kind, SurfaceKind::Sprite);
    }

    /// A1 stretch path — the painted frame stays palette-clean and covers the
    /// target, and the same declaration paints the same pixels twice.
    #[test]
    fn paint_is_deterministic_and_clean() {
        let assets = bank();
        let composition = WindowComposition::new(
            WindowId::INVENTORY,
            vec![
                SurfaceSpec::owned("SPOP.RCI", UiRect::new(4, 4, 120, 90), SurfaceSource::Whole),
                SurfaceSpec::owned(
                    "MAIN00I0.IMG",
                    UiRect::new(0, 154, 320, 46),
                    SurfaceSource::Whole,
                ),
            ],
        );
        let mut first = Framebuffer::new();
        composition.paint(&mut first, &assets).expect("first paint");
        let mut second = Framebuffer::new();
        composition.paint(&mut second, &assets).expect("second paint");
        assert_eq!(first, second);
        assert!(crate::ui_spec::palette_compliance(&first).is_empty());
        assert!(!first.palettes.is_empty());
        // The stretched target is fully painted, not partially: every pixel of
        // the 120×90 nine-slice is an asset index (the fixture fills with 1).
        for y in 4..94u16 {
            for x in 4..124u16 {
                assert_eq!(first.pixel(x, y), 1, "nine-slice pixel ({x},{y})");
            }
        }
    }
}

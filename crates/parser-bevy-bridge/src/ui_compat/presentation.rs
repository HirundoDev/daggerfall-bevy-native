//! A7 — presentation axis: one integer factor, one frame size, outside the
//! composition (plan §3.7).
//!
//! **The rule:** the integer presentation factor is a property of the *host*,
//! not of a window. `SurfaceSpec` and `WindowComposition` have no factor field,
//! and this module is the only place the 640×480 output is decided. The
//! admissible factors are exactly those whose canvas fits 640×480 — `1` and
//! `2` for a 320×200 canvas — and the letterbox is derived, never chosen.
//!
//! `present` is total and frame-exact: the output is always 640×480 RGBA, the
//! canvas is nearest-neighbour, and no pixel is interpolated:
//!
//! - factor 2 → [`present_nearest`] (the M17 frame, letterbox 40 + 40);
//! - factor 1 → the canvas centred at `(160, 140)` with side and top/bottom
//!   bars, from [`canvas_rgba`].
//!
//! The M17 validators `present_delta` and `present_interpolated_pixels` are
//! defined on the factor-2 frame (they compare against `PRESENT_SCALE` and
//! `LETTERBOX`), so they are used here for factor 2 only — stated once, in the
//! test that calls them.
//!
//! `paint`/`present` never allocate per pixel beyond the frame itself, and the
//! frame of two compositions with the same nodes is the same frame regardless
//! of the order the nodes were declared in (rule 4).

use super::error::{UiCompatError, RULE_FACTOR_FIT};
use crate::ui_spec::{
    canvas_rgba, present_nearest, Framebuffer, PresentFrame, PresentMode, CANVAS_HEIGHT,
    CANVAS_WIDTH, LETTERBOX, PRESENT_HEIGHT, PRESENT_SCALE, PRESENT_WIDTH,
};

/// Integer presentation of the canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Presentation {
    /// Integer factor applied to the 320×200 canvas.
    pub scale: u8,
    /// Rows of the top bar.
    pub letterbox_top: u16,
    /// Rows of the bottom bar.
    pub letterbox_bottom: u16,
}

impl Presentation {
    /// The canonical presentation: factor 2, letterbox 40 + 40.
    pub fn canonical() -> Self {
        Self {
            scale: PRESENT_SCALE,
            letterbox_top: LETTERBOX,
            letterbox_bottom: LETTERBOX,
        }
    }

    /// A presentation with an integer factor that fits 640×480.
    ///
    /// `0` (nothing visible) and any factor whose canvas is wider than 640 or
    /// taller than 480 are refused: a fractional factor does not exist by
    /// construction, because the canvas is only ever replicated.
    pub fn new(scale: u8) -> Result<Self, UiCompatError> {
        let width = CANVAS_WIDTH as u32 * scale as u32;
        let height = CANVAS_HEIGHT as u32 * scale as u32;
        if scale == 0 || width > PRESENT_WIDTH as u32 || height > PRESENT_HEIGHT as u32 {
            return Err(UiCompatError::Presentation(RULE_FACTOR_FIT));
        }
        let letterbox = ((PRESENT_HEIGHT as u32 - height) / 2) as u16;
        Ok(Self {
            scale,
            letterbox_top: letterbox,
            letterbox_bottom: letterbox,
        })
    }

    /// Output frame size (`640×480` for every admissible factor).
    pub fn frame_size(&self) -> (u16, u16) {
        (PRESENT_WIDTH, PRESENT_HEIGHT)
    }

    /// Top-left corner of the canvas inside the frame.
    pub fn canvas_origin(&self) -> (u16, u16) {
        (
            (PRESENT_WIDTH - CANVAS_WIDTH * self.scale as u16) / 2,
            self.letterbox_top,
        )
    }

    /// Presents `framebuffer` as a 640×480 RGBA frame.
    ///
    /// Factor 2 delegates to [`present_nearest`]; factor 1 copies the canvas
    /// rows into a centred window and leaves the rest transparent black.
    pub fn present(&self, framebuffer: &Framebuffer) -> PresentFrame {
        if self.scale == PRESENT_SCALE {
            let mut frame = present_nearest(framebuffer);
            frame.letterbox_top = self.letterbox_top;
            frame.letterbox_bottom = self.letterbox_bottom;
            return frame;
        }
        let canvas = canvas_rgba(framebuffer);
        let (origin_x, origin_y) = self.canvas_origin();
        let mut rgba = vec![0u8; PRESENT_WIDTH as usize * PRESENT_HEIGHT as usize * 4];
        // Bars are opaque black, like the factor-2 frame's letterbox: a
        // transparent bar would let the host's clear colour through and make
        // the frame depend on the host.
        for pixel in rgba.chunks_exact_mut(4) {
            pixel[3] = 255;
        }
        for y in 0..CANVAS_HEIGHT as usize {
            for x in 0..CANVAS_WIDTH as usize {
                let source = (y * CANVAS_WIDTH as usize + x) * 4;
                let at = (origin_y as usize + y) * PRESENT_WIDTH as usize + origin_x as usize + x;
                rgba[at * 4..at * 4 + 4].copy_from_slice(&canvas[source..source + 4]);
            }
        }
        PresentFrame {
            width: PRESENT_WIDTH,
            height: PRESENT_HEIGHT,
            scale: self.scale,
            letterbox_top: self.letterbox_top,
            letterbox_bottom: self.letterbox_bottom,
            mode: PresentMode::Letterbox,
            rgba,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_compat::surface::{SurfaceOwnership, SurfaceSource, SurfaceSpec, WindowComposition};
    use crate::ui_compat::test_support::bank;
    use crate::ui_spec::{present_delta, present_interpolated_pixels, UiRect};
    use crate::ui_spec::canvas_rgba;
    use daggerfall_nervous::events::WindowId;

    /// A7 rule 1 — the presentation factor is not part of the composition.
    ///
    /// Violation caught at **build time**: these enumerations break the moment a
    /// `scale`/`factor` field is added to [`SurfaceSpec`] or
    /// [`WindowComposition`] (a struct literal must name every field). The
    /// source scan catches a factor smuggled in as a method or a default on the
    /// axis itself.
    #[test]
    fn falsifier_a7_factor_is_not_in_the_composition() {
        let spec = SurfaceSpec {
            asset: "MAIN00I0.IMG".to_string(),
            canvas_rect: UiRect::new(0, 154, 320, 46),
            source: SurfaceSource::Whole,
            ownership: SurfaceOwnership::Owned,
        };
        let composition = WindowComposition {
            window: WindowId::HUD,
            surfaces: vec![spec.clone()],
        };
        assert_eq!(composition.surfaces.len(), 1);
        assert_eq!(composition.window, WindowId::HUD);
        assert!(
            !include_str!("surface.rs").contains("scale"),
            "the composition module must not carry a presentation factor"
        );
        assert!(!include_str!("surface.rs").contains("factor"));
    }

    /// A7 rule 2 — only an integer factor that fits 640×480 is admissible.
    ///
    /// Violation caught: factor 0 (invisible), factor 3 (960 px wide, wider
    /// than the frame), and a fractional factor — which cannot be expressed,
    /// because the field is an integer and the constructor refuses the two
    /// factors that do not fit.
    #[test]
    fn falsifier_a7_only_fitting_factors() {
        assert_eq!(
            Presentation::new(0),
            Err(UiCompatError::Presentation(RULE_FACTOR_FIT))
        );
        assert_eq!(
            Presentation::new(3),
            Err(UiCompatError::Presentation(RULE_FACTOR_FIT))
        );
        assert_eq!(
            Presentation::new(255),
            Err(UiCompatError::Presentation(RULE_FACTOR_FIT))
        );

        let one = Presentation::new(1).expect("factor 1");
        assert_eq!(one.letterbox_top, 140);
        assert_eq!(one.letterbox_bottom, 140);
        assert_eq!(one.canvas_origin(), (160, 140));

        let two = Presentation::new(2).expect("factor 2");
        assert_eq!(two, Presentation::canonical());
        assert_eq!(two.letterbox_top, 40);
        assert_eq!(two.canvas_origin(), (0, 40));
        assert_eq!(two.frame_size(), (640, 480));
    }

    /// A7 rule 3 — the frame is exactly 640×480 with no interpolated pixel.
    ///
    /// Violation caught: a frame sized from the canvas (320×200), a bilinear
    /// blit (any blended pixel), and a factor-2 frame that drifts from the M17
    /// presentation.
    #[test]
    fn falsifier_a7_present_is_640x480_exact() {
        let assets = bank();
        let composition = WindowComposition::new(
            WindowId::HUD,
            vec![SurfaceSpec::owned(
                "MAIN00I0.IMG",
                UiRect::new(0, 154, 320, 46),
                SurfaceSource::Whole,
            )],
        );
        let mut framebuffer = Framebuffer::new();
        composition.paint(&mut framebuffer, &assets).expect("paint");

        let canonical = Presentation::canonical().present(&framebuffer);
        assert_eq!((canonical.width, canonical.height), (640, 480));
        assert_eq!(canonical.rgba.len(), 640 * 480 * 4);
        assert_eq!(present_delta(&framebuffer, &canonical), 0);
        assert_eq!(present_interpolated_pixels(&canonical), 0);

        let single = Presentation::new(1).expect("factor 1").present(&framebuffer);
        assert_eq!((single.width, single.height), (640, 480));
        assert_eq!(single.rgba.len(), 640 * 480 * 4);
        assert_eq!(single.scale, 1);
        // The frame edge is an opaque bar; the canvas is centred at (160, 140)
        // and holds exactly the canvas bytes, one pixel for one pixel.
        let frame_stride = 640 * 4;
        assert!(single.rgba[..frame_stride]
            .chunks_exact(4)
            .all(|pixel| pixel == [0, 0, 0, 255]));
        let canvas = canvas_rgba(&framebuffer);
        let inside = (140 * 640 + 160) * 4;
        assert_eq!(&single.rgba[inside..inside + 4], &canvas[..4]);
        assert_eq!(&single.rgba[inside + 319 * 4..inside + 320 * 4], &canvas[318 * 4..319 * 4]);
    }

    /// A7 rule 4 — the same nodes in a different declaration order present the
    /// same frame.
    ///
    /// Violation caught: a presentation that depends on the order surfaces were
    /// declared (e.g. hashing them into a set, or painting in a map order).
    #[test]
    fn falsifier_a7_present_is_order_independent() {
        let assets = bank();
        let left = SurfaceSpec::owned(
            "MAIN00I0.IMG",
            UiRect::new(0, 154, 320, 46),
            SurfaceSource::Whole,
        );
        let right = SurfaceSpec::owned(
            "MASK00I0.IMG",
            UiRect::new(0, 0, 64, 64),
            SurfaceSource::Whole,
        );
        let forward = WindowComposition::new(WindowId::HUD, vec![left.clone(), right.clone()]);
        let backward = WindowComposition::new(WindowId::HUD, vec![right, left]);

        let mut first = Framebuffer::new();
        forward.paint(&mut first, &assets).expect("forward");
        let mut second = Framebuffer::new();
        backward.paint(&mut second, &assets).expect("backward");
        // Pixels are order free. The palette-slot registry is a sorted set
        // built at present time (`canvas_rgba`), so it is not compared here.
        assert_eq!(first.pixels, second.pixels);

        let presentation = Presentation::canonical();
        assert_eq!(
            presentation.present(&first).rgba,
            presentation.present(&second).rgba
        );
    }
}

//! Painting the overlay thumbs (design/11 §11.3.12) into a surface's scene, after the document,
//! in document logical px scaled to the buffer. The one impure part of the indicator.

use anyrender::PaintScene;
use blitz_traits::shell::ColorScheme;
use peniko::kurbo::{Affine, RoundedRect};
use peniko::{Color, Fill};

use crate::scroll::indicator::ThumbPaint;
use blitz_kit::scroll::geom::Area;

/// The thumb: ink at 0.5 alpha under a 0.8 layer opacity (design/11 R19), so 0.4.
const THUMB_ALPHA: f32 = 0.4;
/// The track shown while hovered.
const TRACK_ALPHA: f32 = 0.08;

/// The thumb and track colours for `scheme`: black on light, white on dark. (quire's
/// `--scroll-thumb` token is not read: the host cannot see the design system's tokens.)
fn ink(scheme: ColorScheme) -> [f32; 3] {
    match scheme {
        ColorScheme::Dark => [1.0, 1.0, 1.0],
        ColorScheme::Light => [0.0, 0.0, 0.0],
    }
}

fn pill(area: Area, radius: f64) -> RoundedRect {
    RoundedRect::new(
        area.x,
        area.y,
        area.x + area.width,
        area.y + area.height,
        radius,
    )
}

/// Paint `thumbs` into `scene`: document px scaled by `scale`, offset by the content origin
/// `(x, y)` in buffer px.
pub(crate) fn paint(
    scene: &mut impl PaintScene,
    thumbs: &[ThumbPaint],
    scheme: ColorScheme,
    scale: f64,
    origin: (f64, f64),
) {
    let transform = Affine::translate(origin) * Affine::scale(scale);
    let [r, g, b] = ink(scheme);
    for thumb in thumbs {
        if thumb.track_opacity > 0.0 {
            let color = Color::new([r, g, b, TRACK_ALPHA * thumb.track_opacity as f32]);
            scene.fill(
                Fill::NonZero,
                transform,
                color,
                None,
                &pill(thumb.track, 0.0),
            );
        }
        let radius = thumb.thumb.width.min(thumb.thumb.height) / 2.0;
        let color = Color::new([r, g, b, THUMB_ALPHA * thumb.opacity as f32]);
        scene.fill(
            Fill::NonZero,
            transform,
            color,
            None,
            &pill(thumb.thumb, radius),
        );
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::task::Context;

    use blitz_traits::shell::Viewport;
    use dioxus::prelude::*;

    use super::*;
    use crate::clipboard::SharedSelection;
    use crate::dom::document::{DocumentEnv, SurfaceDocument};
    use crate::dom::shell_provider::HostShellProvider;
    use crate::gpu::presenter::{FrameTarget, Overlay};
    use crate::gpu::shm::paint_cpu;
    use crate::prepaint::Prepaint;
    use crate::root::HostRoot;
    use crate::scale::{Scale, Size};
    use blitz_kit::fonts::SharedFonts;
    use blitz_kit::net::LocalNet;

    fn white() -> Element {
        rsx! { div { style: "width:100vw;height:100vh;background:white" } }
    }

    /// The RGBA at `(x, y)` of a 100 x 100 white document painted at `scale` with `thumbs`.
    fn pixel(
        thumbs: Vec<ThumbPaint>,
        scheme: ColorScheme,
        scale: Scale,
        at: (u32, u32),
    ) -> [u8; 4] {
        let (ping, _source) = calloop::ping::make_ping().expect("eventfd ping");
        let side = (100 * scale.0 / 120) as i32;
        let env = DocumentEnv {
            shell: Arc::new(HostShellProvider::new(ping, SharedSelection::default())),
            fonts: SharedFonts::system().for_document(),
            net: Arc::new(LocalNet),
            viewport: Viewport::new(side as u32, side as u32, scale.as_f64() as f32, scheme),
            prepaint: Prepaint::None,
        };
        let mut doc = SurfaceDocument::build(HostRoot::new(white), env);
        doc.poll(&mut Context::from_waker(std::task::Waker::noop()));
        doc.resolve(0.0);
        let buffer = Size {
            width: side,
            height: side,
        };
        let target = FrameTarget::whole(buffer, scale, 0.0);
        let overlay = Overlay { thumbs, scheme };
        let rgba = paint_cpu(&mut doc.dioxus().inner.borrow_mut(), target, &overlay);
        let i = ((at.1 * side as u32 + at.0) * 4) as usize;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    }

    fn thumb(opacity: f64) -> ThumbPaint {
        ThumbPaint {
            thumb: Area {
                x: 90.0,
                y: 2.0,
                width: 8.0,
                height: 40.0,
            },
            track: Area::default(),
            opacity,
            track_opacity: 0.0,
        }
    }

    #[test]
    fn the_thumb_is_painted_over_the_document_where_the_indicator_put_it() {
        let light = ColorScheme::Light;
        // 0.4 black over white: 153.
        let on = pixel(vec![thumb(1.0)], light, Scale::ONE, (94, 20));
        assert!((150..=156).contains(&on[0]), "{on:?}");
        let beside = pixel(vec![thumb(1.0)], light, Scale::ONE, (80, 20));
        assert_eq!(beside, [255, 255, 255, 255]);
        let faded = pixel(vec![thumb(0.0)], light, Scale::ONE, (94, 20));
        assert_eq!(faded, [255, 255, 255, 255], "opacity 0 paints nothing");
        // At 2x the same logical point is twice as far into the buffer.
        let scaled = pixel(vec![thumb(1.0)], light, Scale(240), (188, 40));
        assert!((150..=156).contains(&scaled[0]), "{scaled:?}");
    }
}

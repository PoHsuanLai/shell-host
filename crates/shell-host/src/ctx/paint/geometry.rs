//! Following a new size or scale, and the document viewport a geometry implies.

use blitz_traits::shell::{ColorScheme, Viewport};

use crate::csd::{Inset, content_px};
use crate::frame::FrameInput;
use crate::id::SurfaceId;
use crate::scale::{Scale, Size, buffer_size};
use crate::state::HostState;
use crate::surface::Geometry;

/// Follow a new preferred scale or configured size: the viewport destination (or the integer
/// buffer scale without fractional-scale), the document's viewport, the presenter's size; then
/// repaint with every region re-sent.
pub(super) fn sync_geometry(state: &mut HostState, id: SurfaceId) {
    crate::ctx::rescale::follow_preferred(state, id);
    let Some(surface) = state.surfaces.get_mut(&id) else {
        return;
    };
    let wl = surface.role.wl_surface().clone();
    let now = Geometry {
        logical: surface.logical,
        inset: surface.inset,
        scale: surface.scale,
    };
    if surface.geometry == Some(now) {
        return;
    }
    match &state.fractional {
        Some(fractional) => fractional.set_destination(&wl, now.logical.width, now.logical.height),
        None => wl.set_buffer_scale(integer_scale(now.scale)),
    }
    surface.document.set_viewport(viewport(
        now.logical,
        now.inset,
        now.scale,
        state.color_scheme,
    ));
    let buffer = surface.buffer();
    if let Some(presenter) = surface.presenter.as_mut() {
        presenter.resize(state.gpu.as_ref(), buffer);
    }
    surface.geometry = Some(now);
    state.frame_input(id, FrameInput::Invalidate);
}

/// The document viewport for a surface of `logical` size with resize band `inset` at `scale`:
/// the physical pixels the document paints in (`csd::content_px`, the whole buffer without a
/// band), the scale factor, the colour scheme. At least 1x1, so an unconfigured surface still
/// lays out.
pub(in crate::ctx) fn viewport(
    logical: Size,
    inset: Inset,
    scale: Scale,
    scheme: ColorScheme,
) -> Viewport {
    let content = content_px(buffer_size(logical, scale), inset, scale);
    let px = |v: i32| u32::try_from(v).unwrap_or(0).max(1);
    // Blitz takes the scale as f32; 120ths are exact in it.
    let factor = scale.as_f64() as f32;
    Viewport::new(px(content.width), px(content.height), factor, scheme)
}

/// `wl_surface.set_buffer_scale` for a compositor without fractional-scale: whole numbers only.
fn integer_scale(scale: Scale) -> i32 {
    i32::try_from((scale.0 / 120).max(1)).unwrap_or(1)
}

//! The input and blur regions sent to the compositor: the only place they are sent from.

use smithay_client_toolkit::compositor::{CompositorState, Region};
use wayland_client::QueueHandle;

use crate::blur::BlurSpec;
use crate::csd::{Inset, band, to_surface};
use crate::handlers::background_effect::{BackgroundEffect, Support};
use crate::region::Rect;
use crate::resolve::{Change, Resolved, resolve};
use crate::state::HostState;
use crate::surface::HostSurface;

/// Resolve the input and blur regions against the laid-out document and send whichever
/// changed since they were last sent. Both are the document's, moved to surface coordinates;
/// the input region adds the resize band (`csd::band`), the blur region never covers it.
pub(super) fn send_regions(
    compositor: &CompositorState,
    effect: Option<&mut BackgroundEffect>,
    qh: &QueueHandle<HostState>,
    surface: &mut HostSurface,
    blur: Support,
) {
    let size = surface.document_size();
    let inset = surface.inset;
    let document = &surface.document;
    let input = in_surface(
        resolve(&surface.input, size, |id| document.element_rect(id)),
        inset,
        band(surface.logical, inset),
    );
    let blur = match (&surface.blur, blur) {
        (BlurSpec::Region(spec), Support::Supported) => in_surface(
            resolve(spec, size, |id| document.element_rect(id)),
            inset,
            Vec::new(),
        ),
        (BlurSpec::None, _) | (BlurSpec::Region(_), Support::Unsupported) => {
            Resolved::Strips(Vec::new())
        }
    };
    let wl = surface.role.wl_surface();
    if surface.regions.input.update(&input) == Change::Changed
        && let Resolved::Strips(rects) = &input
    {
        let region = region_of(compositor, rects);
        wl.set_input_region(region.as_ref().map(Region::wl_region));
    }
    if surface.regions.blur.update(&blur) == Change::Changed
        && let Resolved::Strips(rects) = &blur
        && let Some(effect) = effect
    {
        let region = (!rects.is_empty())
            .then(|| region_of(compositor, rects))
            .flatten();
        effect.set_blur_region(qh, wl, region.as_ref().map(Region::wl_region));
    }
}

/// A document-coordinate region in surface coordinates, plus `extra` surface strips. A
/// pending region stays pending: the surface keeps the one it has.
fn in_surface(resolved: Resolved, inset: Inset, extra: Vec<Rect>) -> Resolved {
    match resolved {
        Resolved::Strips(rects) => {
            let mut strips = to_surface(rects, inset);
            strips.extend(extra);
            Resolved::Strips(strips)
        }
        Resolved::Pending => Resolved::Pending,
    }
}

fn region_of(compositor: &CompositorState, rects: &[Rect]) -> Option<Region> {
    let region = Region::new(compositor).ok()?;
    for rect in rects {
        region.add(rect.x, rect.y, rect.width, rect.height);
    }
    Some(region)
}

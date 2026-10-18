//! Fractional scale: [`Scale120`] (blitz-kit's 120ths type, the wire value of
//! `wp_fractional_scale_v1.preferred_scale`) and the logical/physical conversions built on it.

pub use blitz_kit::units::Scale120;

use crate::live::Live;
use crate::region::Px;

/// The largest width or height, in pixels, the host accepts from the compositor for a surface,
/// logical or buffer: the 2D texture limit of the device the host asks for
/// (`wgpu::Limits::default()`), so a buffer of this size can always be configured. A larger
/// number is a lie or a bug, and a swapchain that size would be a validation error.
pub(crate) const MAX_SURFACE_EXTENT: Px = 8192;

/// One surface's current scale, and the waiters for its next change: what
/// [`crate::handle::use_surface_scale`] reads. The host learns a surface's scale from the
/// compositor (`wp_fractional_scale_v1`, else `preferred_buffer_scale` or the output's scale)
/// and publishes it here; the surface's `SurfaceHandle` holds a clone, so a component can draw
/// for the device grid it lands on. The mechanism is [`crate::live`]'s (FINDINGS F43).
pub(crate) type LiveScale = Live<Scale120>;

/// A size in pixels. Used for both logical (surface-local) and physical (buffer) sizes; which
/// is which is a property of the call site, not the type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub width: Px,
    pub height: Px,
}

impl Size {
    /// Whether both sides are positive: only then can a viewport destination or a buffer be set.
    pub(crate) fn has_area(self) -> bool {
        self.width > 0 && self.height > 0
    }
}

/// A width or height the compositor sent on the wire, kept within [`MAX_SURFACE_EXTENT`].
pub(crate) fn wire_extent(value: u32) -> Px {
    Px::try_from(value).map_or(MAX_SURFACE_EXTENT, |px| px.min(MAX_SURFACE_EXTENT))
}

/// The buffer size to allocate for a surface of `logical` size at `scale`, rounded half away
/// from zero (`fractional-scale-v1`: "For toplevel surfaces, the size is rounded halfway away
/// from zero"), and at most [`MAX_SURFACE_EXTENT`] on a side.
pub fn buffer_size(logical: Size, scale: Scale) -> Size {
    Size {
        width: logical_to_physical(logical.width, scale),
        height: logical_to_physical(logical.height, scale),
    }
}

/// One logical dimension converted to its physical (buffer) equivalent at `scale`, rounded half
/// away from zero, at most [`MAX_SURFACE_EXTENT`].
pub(crate) fn logical_to_physical(value: Px, scale: Scale) -> Px {
    round_half_away_from_zero(f64::from(value) * scale.factor()).min(MAX_SURFACE_EXTENT)
}

/// How a surface's buffer is tied to its logical size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BufferMapping {
    /// `wp_viewport.set_destination`: the buffer, whatever its pixel size, fills this logical size.
    Viewport(Size),
    /// `wl_surface.set_buffer_scale`: whole numbers only.
    Integer(i32),
}

/// The mapping for a surface of `logical` size at `scale`, given the fractional scale the
/// compositor last preferred for it: the viewport once it has said one, else the rounded
/// integer scale (a compositor without fractional-scale, or one that has not sent it yet).
pub(crate) fn buffer_mapping(
    preferred: Option<Scale>,
    logical: Size,
    scale: Scale,
) -> BufferMapping {
    match preferred {
        Some(_) => BufferMapping::Viewport(logical),
        None => BufferMapping::Integer(scale.factor().round().max(1.0) as i32),
    }
}

fn round_half_away_from_zero(value: f64) -> Px {
    // `f64::round` already rounds halves away from zero, matching the protocol's rule.
    value.round() as Px
}

#[cfg(test)]
mod tests;

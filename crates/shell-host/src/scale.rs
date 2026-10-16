//! Fractional scale: the wire value from `wp_fractional_scale_v1.preferred_scale` (120ths of a
//! logical pixel) and the logical/physical conversions built on it.

use crate::region::Px;

/// The largest width or height, in pixels, the host accepts from the compositor for a surface,
/// logical or buffer: the 2D texture limit of the device the host asks for
/// (`wgpu::Limits::default()`), so a buffer of this size can always be configured. A larger
/// number is a lie or a bug, and a swapchain that size would be a validation error.
pub(crate) const MAX_SURFACE_EXTENT: Px = 8192;

/// The smallest preferred scale accepted, in 120ths: a quarter of a logical pixel per pixel.
const MIN_SCALE_120THS: u32 = 30;

/// The largest preferred scale accepted, in 120ths: ten pixels per logical pixel.
const MAX_SCALE_120THS: u32 = 1200;

/// The largest whole `wl_surface.preferred_buffer_scale` accepted.
const MAX_INTEGER_FACTOR: i32 = 10;

/// A scale factor as `wp_fractional_scale_v1` sends it: the numerator of a fraction over 120.
/// `Scale(120)` is 1.0x, `Scale(180)` is 1.5x.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Scale(pub u32);

impl Scale {
    /// 1.0x, the scale a surface has until told otherwise.
    pub const ONE: Scale = Scale(120);

    /// Build a `Scale` from the raw wire value of a `preferred_scale` event, kept between a
    /// quarter and ten times: a scale outside that is a compositor bug, and would size a buffer
    /// beyond any device.
    pub fn from_wire(value: u32) -> Scale {
        Scale(value.clamp(MIN_SCALE_120THS, MAX_SCALE_120THS))
    }

    /// The scale of a whole buffer-scale factor (`wl_output.scale`, `wl_surface`'s
    /// `scale_factor_changed`), at least 1 and at most 10.
    pub(crate) fn from_integer_factor(factor: i32) -> Scale {
        let whole = u32::try_from(factor.clamp(1, MAX_INTEGER_FACTOR)).unwrap_or(1);
        Scale(whole * Scale::ONE.0)
    }

    /// The scale as a floating-point multiplier, for the one place arithmetic needs it: sizing
    /// a buffer. Never stored — see `CONVENTIONS.md` §12 on floats in data.
    pub fn as_f64(self) -> f64 {
        f64::from(self.0) / 120.0
    }
}

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
    round_half_away_from_zero(f64::from(value) * scale.as_f64()).min(MAX_SURFACE_EXTENT)
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
        None => BufferMapping::Integer(scale.as_f64().round().max(1.0) as i32),
    }
}

fn round_half_away_from_zero(value: f64) -> Px {
    // `f64::round` already rounds halves away from zero, matching the protocol's rule.
    value.round() as Px
}

#[cfg(test)]
mod tests;

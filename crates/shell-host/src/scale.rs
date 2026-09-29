//! Fractional scale: the wire value from `wp_fractional_scale_v1.preferred_scale` (120ths of a
//! logical pixel) and the logical/physical conversions built on it.

use crate::region::Px;

/// A scale factor as `wp_fractional_scale_v1` sends it: the numerator of a fraction over 120.
/// `Scale(120)` is 1.0x, `Scale(180)` is 1.5x.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Scale(pub u32);

impl Scale {
    /// 1.0x, the scale a surface has until told otherwise.
    pub const ONE: Scale = Scale(120);

    /// Build a `Scale` from the raw wire value of a `preferred_scale` event.
    pub fn from_wire(value: u32) -> Scale {
        Scale(value)
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

/// The buffer size to allocate for a surface of `logical` size at `scale`, rounded half away
/// from zero (`fractional-scale-v1`: "For toplevel surfaces, the size is rounded halfway away
/// from zero").
pub fn buffer_size(logical: Size, scale: Scale) -> Size {
    Size {
        width: logical_to_physical(logical.width, scale),
        height: logical_to_physical(logical.height, scale),
    }
}

/// One logical dimension converted to its physical (buffer) equivalent at `scale`, rounded half
/// away from zero.
pub(crate) fn logical_to_physical(value: Px, scale: Scale) -> Px {
    round_half_away_from_zero(f64::from(value) * scale.as_f64())
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

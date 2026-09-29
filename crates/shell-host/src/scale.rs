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
pub fn logical_to_physical(value: Px, scale: Scale) -> Px {
    round_half_away_from_zero(f64::from(value) * scale.as_f64())
}

fn round_half_away_from_zero(value: f64) -> Px {
    // `f64::round` already rounds halves away from zero, matching the protocol's rule.
    value.round() as Px
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCALE_CASES: &[(u32, f64)] = &[(120, 1.0), (150, 1.25), (180, 1.5), (240, 2.0)];

    #[test]
    fn as_f64_matches_the_documented_fraction() {
        for &(wire, expected) in SCALE_CASES {
            let scale = Scale::from_wire(wire);
            assert!((scale.as_f64() - expected).abs() < 1e-9, "wire={wire}");
        }
    }

    #[test]
    fn one_is_the_identity_scale() {
        assert_eq!(Scale::ONE, Scale::from_wire(120));
        assert_eq!(logical_to_physical(37, Scale::ONE), 37);
    }

    const BUFFER_CASES: &[(Px, u32, Px)] = &[
        (100, 120, 100), // 1.0x: unchanged
        (100, 150, 125), // 1.25x: exact
        (100, 180, 150), // 1.5x: exact
        (100, 240, 200), // 2.0x: exact
        (101, 180, 152), // 1.5x: 151.5 rounds away from zero to 152
        (1, 180, 2),     // 1.5x: 1.5 rounds away from zero to 2
        (3, 180, 5),     // 1.5x: 4.5 rounds away from zero to 5
        (0, 180, 0),
    ];

    #[test]
    fn buffer_size_matches_the_table() {
        for &(logical, wire, expected) in BUFFER_CASES {
            let got = logical_to_physical(logical, Scale::from_wire(wire));
            assert_eq!(got, expected, "logical={logical} wire={wire}");
        }
    }

    #[test]
    fn buffer_size_applies_to_both_dimensions() {
        let size = buffer_size(
            Size {
                width: 200,
                height: 32,
            },
            Scale::from_wire(180),
        );
        assert_eq!(
            size,
            Size {
                width: 300,
                height: 48
            }
        );
    }

    #[test]
    fn two_logical_pixels_at_one_point_five_is_three_physical() {
        // The acceptance check from render acceptance item (f): a 2-logical-px checkerboard
        // must land on exactly 3 physical pixels at scale 1.5.
        assert_eq!(logical_to_physical(2, Scale::from_wire(180)), 3);
    }
}

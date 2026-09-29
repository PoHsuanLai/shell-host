mod buffer;

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

#[test]
fn the_buffer_is_mapped_by_viewport_once_a_fractional_scale_is_preferred() {
    let logical = Size {
        width: 200,
        height: 32,
    };
    let cases: &[(&str, Option<Scale>, Scale, BufferMapping)] = &[
        (
            "a preferred fractional scale uses the viewport",
            Some(Scale(180)),
            Scale(180),
            BufferMapping::Viewport(logical),
        ),
        (
            "no fractional scale yet rounds the scale up to whole",
            None,
            Scale(180),
            BufferMapping::Integer(2),
        ),
        (
            "no fractional scale at 1x is 1",
            None,
            Scale::ONE,
            BufferMapping::Integer(1),
        ),
        (
            "a scale under 1x never maps to 0",
            None,
            Scale(30),
            BufferMapping::Integer(1),
        ),
    ];
    for (name, preferred, scale, want) in cases {
        assert_eq!(buffer_mapping(*preferred, logical, *scale), *want, "{name}");
    }
}

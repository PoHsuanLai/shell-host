mod buffer;

use super::*;

const BUFFER_CASES: &[(Px, u32, Px)] = &[
    (100, 120, 100), // 1.0x: unchanged
    (100, 150, 125), // 1.25x: exact
    (100, 180, 150), // 1.5x: exact
    (100, 240, 200), // 2.0x: exact
    (101, 180, 152), // 1.5x: 151.5 rounds away from zero to 152
    (1, 180, 2),     // 1.5x: 1.5 rounds away from zero to 2
    (3, 180, 5),     // 1.5x: 4.5 rounds away from zero to 5
    (2, 180, 3),     // 1.5x: a 2-logical-px checkerboard lands on exactly 3 physical pixels
    (0, 180, 0),
];

#[test]
fn buffer_size_matches_the_table() {
    for &(logical, wire, expected) in BUFFER_CASES {
        let got = logical_to_physical(logical, Scale120::from_wire(wire));
        assert_eq!(got, expected, "logical={logical} wire={wire}");
    }
}

#[test]
fn the_buffer_is_mapped_by_viewport_once_a_fractional_scale_is_preferred() {
    let logical = Size {
        width: 200,
        height: 32,
    };
    let cases: &[(&str, Option<Scale120>, Scale120, BufferMapping)] = &[
        (
            "a preferred fractional scale uses the viewport",
            Some(Scale120(180)),
            Scale120(180),
            BufferMapping::Viewport(logical),
        ),
        (
            "no fractional scale yet rounds the scale up to whole",
            None,
            Scale120(180),
            BufferMapping::Integer(2),
        ),
        (
            "no fractional scale at 1x is 1",
            None,
            Scale120::ONE,
            BufferMapping::Integer(1),
        ),
        (
            "a scale under 1x never maps to 0",
            None,
            Scale120(30),
            BufferMapping::Integer(1),
        ),
    ];
    for (name, preferred, scale, want) in cases {
        assert_eq!(buffer_mapping(*preferred, logical, *scale), *want, "{name}");
    }
}

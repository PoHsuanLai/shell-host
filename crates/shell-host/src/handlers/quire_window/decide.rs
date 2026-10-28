//! The decision behind a placement request: a menu choice and the latest input serial become
//! the protocol request, or none when the seat has made no input yet.

use casement_protocols::vocab::PlaceTarget;

use crate::window_placement::WindowPlacement;

/// What `quire_window_v1.request_placement` is sent with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PlacementRequest {
    pub(super) target: PlaceTarget,
    pub(super) serial: u32,
}

/// The wire target of a menu choice.
pub(super) fn target_of(placement: WindowPlacement) -> PlaceTarget {
    match placement {
        WindowPlacement::Left => PlaceTarget::Left,
        WindowPlacement::Right => PlaceTarget::Right,
        WindowPlacement::Top => PlaceTarget::Top,
        WindowPlacement::Bottom => PlaceTarget::Bottom,
        WindowPlacement::TopLeft => PlaceTarget::TopLeft,
        WindowPlacement::TopRight => PlaceTarget::TopRight,
        WindowPlacement::BottomLeft => PlaceTarget::BottomLeft,
        WindowPlacement::BottomRight => PlaceTarget::BottomRight,
        WindowPlacement::Fill => PlaceTarget::Fill,
        WindowPlacement::Centre => PlaceTarget::Centre,
        WindowPlacement::Restore => PlaceTarget::Restore,
    }
}

/// The request for `placement` after the input event with `serial` (0 while the seat has made
/// none: `None`, since a compositor ignores a request without a serial, so windows never move
/// on their own). Pure.
pub(super) fn plan(placement: WindowPlacement, serial: u32) -> Option<PlacementRequest> {
    (serial != 0).then(|| PlacementRequest {
        target: target_of(placement),
        serial,
    })
}

#[cfg(test)]
mod tests {
    use casement_protocols::vocab::WireCode;

    use super::*;

    const ALL: [WindowPlacement; 11] = [
        WindowPlacement::Left,
        WindowPlacement::Right,
        WindowPlacement::Top,
        WindowPlacement::Bottom,
        WindowPlacement::TopLeft,
        WindowPlacement::TopRight,
        WindowPlacement::BottomLeft,
        WindowPlacement::BottomRight,
        WindowPlacement::Fill,
        WindowPlacement::Centre,
        WindowPlacement::Restore,
    ];

    #[test]
    fn every_choice_maps_to_its_wire_target() {
        let want = [
            PlaceTarget::Left,
            PlaceTarget::Right,
            PlaceTarget::Top,
            PlaceTarget::Bottom,
            PlaceTarget::TopLeft,
            PlaceTarget::TopRight,
            PlaceTarget::BottomLeft,
            PlaceTarget::BottomRight,
            PlaceTarget::Fill,
            PlaceTarget::Centre,
            PlaceTarget::Restore,
        ];
        for (choice, target) in ALL.into_iter().zip(want) {
            assert_eq!(target_of(choice), target, "{choice:?}");
        }
    }

    #[test]
    fn the_targets_are_distinct_wire_codes_covering_the_protocol() {
        let mut codes: Vec<u32> = ALL.into_iter().map(|p| target_of(p).code()).collect();
        codes.sort_unstable();
        let all: Vec<u32> = PlaceTarget::ALL.iter().map(|t| t.code()).collect();
        assert_eq!(codes, all);
    }

    #[test]
    fn a_request_needs_an_input_serial() {
        let cases = [
            ("no input yet", WindowPlacement::Left, 0, None),
            (
                "left half after a click",
                WindowPlacement::Left,
                41,
                Some(PlacementRequest {
                    target: PlaceTarget::Left,
                    serial: 41,
                }),
            ),
            (
                "centre after a key",
                WindowPlacement::Centre,
                7,
                Some(PlacementRequest {
                    target: PlaceTarget::Centre,
                    serial: 7,
                }),
            ),
        ];
        for (name, placement, serial, want) in cases {
            assert_eq!(plan(placement, serial), want, "{name}");
        }
    }
}

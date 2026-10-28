//! The decisions behind the pointer handler: where a press, release or scroll goes, and whether
//! a press is the click that activates its window.

use std::time::Duration;

use blitz_traits::events::MouseEventButton;

use crate::csd::Zone;
use crate::input::pointer::first_mouse;
use crate::role::RoleKind;
use crate::toplevel::ResizeEdge;

/// Where a press goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PressRoute {
    /// The band is the host's: a primary press starts this resize.
    Resize(ResizeEdge),
    /// Any other press in the band: the document never hears of it.
    Absorbed,
    /// The click that activated the window: it counts as pressed, but the document never
    /// hears of it.
    Swallowed,
    /// To the document, preceded by its move.
    Deliver,
}

/// The route of `button` pressed in `zone`, where `swallow` says the press is the click that
/// activated the window (`first_mouse_verdict`) and the element pressed did not ask to receive
/// that click anyway.
pub(super) fn press_route(zone: Zone, button: MouseEventButton, swallow: bool) -> PressRoute {
    match (zone, button, swallow) {
        (Zone::Edge(edge), MouseEventButton::Main, _) => PressRoute::Resize(edge),
        (Zone::Edge(_), _, _) => PressRoute::Absorbed,
        (Zone::Document, _, true) => PressRoute::Swallowed,
        (Zone::Document, _, false) => PressRoute::Deliver,
    }
}

/// Whether a release or a scroll in a zone reaches the document. Only the document's zone hears
/// releases and scrolls: a drag that began in the band ends there.
pub(super) fn reaches_document(zone: Zone) -> bool {
    matches!(zone, Zone::Document)
}

/// Whether a press on a surface of `role` (`None` when the surface is gone) is the click that
/// activates it (design/13 §13.3.8): only for a toplevel, only when it is the one surface that
/// has keyboard focus (`focused`), and only within `window` of that focus arriving
/// (`since_activation`, `None` when it was already focused). `true` swallows it.
pub(super) fn first_mouse_verdict(
    role: Option<RoleKind>,
    focused: bool,
    since_activation: Option<Duration>,
    window: Duration,
) -> bool {
    matches!(role, Some(RoleKind::Toplevel)) && focused && first_mouse(since_activation, window)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EDGE: Zone = Zone::Edge(ResizeEdge::BottomRight);

    #[test]
    fn a_press_goes_where_its_zone_and_the_first_mouse_rule_send_it() {
        use MouseEventButton::{Main, Secondary};
        type Row = (&'static str, Zone, MouseEventButton, bool, PressRoute);
        let resize = PressRoute::Resize(ResizeEdge::BottomRight);
        let cases: &[Row] = &[
            (
                "primary in the band resizes, active window or not",
                EDGE,
                Main,
                true,
                resize,
            ),
            (
                "secondary in the band is absorbed",
                EDGE,
                Secondary,
                false,
                PressRoute::Absorbed,
            ),
            (
                "the activating click on a plain element is swallowed",
                Zone::Document,
                Main,
                true,
                PressRoute::Swallowed,
            ),
            (
                "the activating secondary click on a plain element is swallowed",
                Zone::Document,
                Secondary,
                true,
                PressRoute::Swallowed,
            ),
            (
                "the activating click on an opted-in element, or a press on an active window, is delivered",
                Zone::Document,
                Main,
                false,
                PressRoute::Deliver,
            ),
            (
                "a secondary press on an active window is delivered",
                Zone::Document,
                Secondary,
                false,
                PressRoute::Deliver,
            ),
        ];
        for (name, zone, button, swallow, want) in cases {
            assert_eq!(press_route(*zone, *button, *swallow), *want, "{name}");
        }
    }

    #[test]
    fn releases_and_scrolls_reach_only_the_document() {
        let cases: &[(&str, Zone, bool)] =
            &[("document", Zone::Document, true), ("band", EDGE, false)];
        for (name, zone, want) in cases {
            assert_eq!(reaches_document(*zone), *want, "{name}");
        }
    }

    type Case = (&'static str, Option<RoleKind>, bool, Option<Duration>, bool);

    #[test]
    fn only_a_focused_toplevel_can_swallow_its_activating_click() {
        let window = Duration::from_millis(100);
        let recent = Some(Duration::from_millis(3));
        let toplevel = Some(RoleKind::Toplevel);
        let cases: &[Case] = &[
            (
                "focused toplevel just activated",
                toplevel,
                true,
                recent,
                true,
            ),
            (
                "focused toplevel long after activation",
                toplevel,
                true,
                Some(Duration::from_millis(101)),
                false,
            ),
            (
                "focused toplevel that was already active",
                toplevel,
                true,
                None,
                false,
            ),
            (
                "toplevel without keyboard focus",
                toplevel,
                false,
                recent,
                false,
            ),
            (
                "a layer surface never swallows",
                Some(RoleKind::Layer),
                true,
                recent,
                false,
            ),
            (
                "a popup never swallows",
                Some(RoleKind::Popup),
                true,
                recent,
                false,
            ),
            ("a gone surface delivers", None, true, recent, false),
        ];
        for (name, role, focused, since, want) in cases {
            assert_eq!(
                first_mouse_verdict(*role, *focused, *since, window),
                *want,
                "{name}"
            );
        }
    }
}

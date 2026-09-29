//! The decisions behind the pointer handler: where a press, release or scroll goes, and whether
//! a press is the click that activates its window.

use std::time::Duration;

use blitz_traits::events::MouseEventButton;

use crate::csd::Zone;
use crate::input::pointer::{FirstMouse, first_mouse};
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

/// The route of `button` pressed in `zone`, where `first_mouse` is the verdict for the window's
/// activation.
pub(super) fn press_route(zone: Zone, button: MouseEventButton, first: FirstMouse) -> PressRoute {
    match (zone, button, first) {
        (Zone::Edge(edge), MouseEventButton::Main, _) => PressRoute::Resize(edge),
        (Zone::Edge(_), _, _) => PressRoute::Absorbed,
        (Zone::Document, _, FirstMouse::Swallow) => PressRoute::Swallowed,
        (Zone::Document, _, FirstMouse::Deliver) => PressRoute::Deliver,
    }
}

/// Whether a release or a scroll in a zone reaches the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Delivery {
    ToDocument,
    Withheld,
}

/// Only the document's zone hears releases and scrolls: a drag that began in the band ends there.
pub(super) fn delivery(zone: Zone) -> Delivery {
    match zone {
        Zone::Document => Delivery::ToDocument,
        Zone::Edge(_) => Delivery::Withheld,
    }
}

/// Whether the surface pressed has the keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Focused {
    Yes,
    No,
}

/// Whether a press on a surface of `role` (`None` when the surface is gone) is the click that
/// activates it (design/13 §13.3.8): only for a toplevel, only when it is the one surface that
/// has keyboard focus, and only within `window` of that focus arriving (`since_activation`,
/// `None` when it was already focused).
pub(super) fn first_mouse_verdict(
    role: Option<RoleKind>,
    focus: Focused,
    since_activation: Option<Duration>,
    window: Duration,
) -> FirstMouse {
    match (role, focus) {
        (Some(RoleKind::Toplevel), Focused::Yes) => first_mouse(since_activation, window),
        _ => FirstMouse::Deliver,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EDGE: Zone = Zone::Edge(ResizeEdge::BottomRight);

    #[test]
    fn a_press_goes_where_its_zone_and_the_first_mouse_rule_send_it() {
        use MouseEventButton::{Main, Secondary};
        let cases: &[(&str, Zone, MouseEventButton, FirstMouse, PressRoute)] = &[
            (
                "primary in the band resizes, active window or not",
                EDGE,
                Main,
                FirstMouse::Swallow,
                PressRoute::Resize(ResizeEdge::BottomRight),
            ),
            (
                "secondary in the band is absorbed",
                EDGE,
                Secondary,
                FirstMouse::Deliver,
                PressRoute::Absorbed,
            ),
            (
                "the activating click on the document is swallowed",
                Zone::Document,
                Main,
                FirstMouse::Swallow,
                PressRoute::Swallowed,
            ),
            (
                "a press on the document is delivered",
                Zone::Document,
                Main,
                FirstMouse::Deliver,
                PressRoute::Deliver,
            ),
            (
                "a secondary press on the document is delivered",
                Zone::Document,
                Secondary,
                FirstMouse::Deliver,
                PressRoute::Deliver,
            ),
        ];
        for (name, zone, button, first, want) in cases {
            assert_eq!(press_route(*zone, *button, *first), *want, "{name}");
        }
    }

    #[test]
    fn releases_and_scrolls_reach_only_the_document() {
        let cases: &[(&str, Zone, Delivery)] = &[
            ("document", Zone::Document, Delivery::ToDocument),
            ("band", EDGE, Delivery::Withheld),
        ];
        for (name, zone, want) in cases {
            assert_eq!(delivery(*zone), *want, "{name}");
        }
    }

    type Case = (
        &'static str,
        Option<RoleKind>,
        Focused,
        Option<Duration>,
        FirstMouse,
    );

    #[test]
    fn only_a_focused_toplevel_can_swallow_its_activating_click() {
        let window = Duration::from_millis(100);
        let recent = Some(Duration::from_millis(3));
        let cases: &[Case] = &[
            (
                "focused toplevel just activated",
                Some(RoleKind::Toplevel),
                Focused::Yes,
                recent,
                FirstMouse::Swallow,
            ),
            (
                "focused toplevel long after activation",
                Some(RoleKind::Toplevel),
                Focused::Yes,
                Some(Duration::from_millis(101)),
                FirstMouse::Deliver,
            ),
            (
                "focused toplevel that was already active",
                Some(RoleKind::Toplevel),
                Focused::Yes,
                None,
                FirstMouse::Deliver,
            ),
            (
                "toplevel without keyboard focus",
                Some(RoleKind::Toplevel),
                Focused::No,
                recent,
                FirstMouse::Deliver,
            ),
            (
                "a layer surface never swallows",
                Some(RoleKind::Layer),
                Focused::Yes,
                recent,
                FirstMouse::Deliver,
            ),
            (
                "a popup never swallows",
                Some(RoleKind::Popup),
                Focused::Yes,
                recent,
                FirstMouse::Deliver,
            ),
            (
                "a gone surface delivers",
                None,
                Focused::Yes,
                recent,
                FirstMouse::Deliver,
            ),
        ];
        for (name, role, focus, since, want) in cases {
            assert_eq!(
                first_mouse_verdict(*role, *focus, *since, window),
                *want,
                "{name}"
            );
        }
    }
}

//! What each palmrest message means for the host (design/12 §12.3.11): the fingers' state for
//! the double-scroll window, and the scroll event for the engine. Pure.

use std::time::Duration;

use crate::scroll::surface::PalmScroll;
use blitz_kit::scroll::geom::ScrollAxis;
use blitz_kit::scroll::velocity::SampleTime;
use palmrest_protocol::{ActiveTouch, Axis as WireAxis, Phase, ToHost};

/// A message's effects.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Route {
    /// Whether the fingers are down, as it reports, if it does.
    pub fingers: Option<bool>,
    /// The engine event it is, if any (for the surface under the pointer).
    pub scroll: Option<PalmScroll>,
}

/// `message`'s effects: `Touches{Down}` is touch to stop, a scroll phase moves the engine, and
/// every scroll phase also says where the fingers are. Tap (SmartZoom) and Swipe (the
/// host-decided page swipe, 12 §12.3.6) have no consumer in the host yet.
pub(crate) fn route(message: &ToHost) -> Route {
    match *message {
        ToHost::Touches { active } => Route {
            fingers: Some(active == ActiveTouch::Down),
            scroll: (active == ActiveTouch::Down).then_some(PalmScroll::MayBegin),
        },
        ToHost::Scroll {
            phase,
            dx_px,
            dy_px,
            axis,
            t_ns,
            ..
        } => {
            let at = SampleTime(Duration::from_nanos(t_ns));
            let (fingers, event) = scroll(phase, axis, f64::from(dx_px), f64::from(dy_px), at);
            Route {
                fingers: Some(fingers),
                scroll: Some(event),
            }
        }
        ToHost::Tap { .. }
        | ToHost::Swipe { .. }
        | ToHost::Hello { .. }
        | ToHost::DeviceGone
        | ToHost::Run { .. } => Route {
            fingers: None,
            scroll: None,
        },
    }
}

fn scroll(
    phase: Phase,
    axis: Option<WireAxis>,
    dx: f64,
    dy: f64,
    at: SampleTime,
) -> (bool, PalmScroll) {
    match phase {
        Phase::MayBegin => (true, PalmScroll::MayBegin),
        Phase::Began => {
            let axis = match axis {
                Some(WireAxis::X) => ScrollAxis::X,
                Some(WireAxis::Y) | None => ScrollAxis::Y,
            };
            (true, PalmScroll::Began { axis })
        }
        Phase::Changed => (true, PalmScroll::Changed { dx, dy, at }),
        Phase::Ended => (false, PalmScroll::Ended { at }),
        Phase::Cancelled => (false, PalmScroll::Cancelled),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use palmrest_protocol::{GestureId, TapKind};

    fn scroll_msg(phase: Phase, dy: f32) -> ToHost {
        ToHost::Scroll {
            gesture: GestureId(1),
            phase,
            dx_px: 0.0,
            dy_px: dy,
            fingers: 1,
            axis: Some(WireAxis::Y),
            t_ns: 2_000_000,
        }
    }

    #[test]
    fn the_table_of_12_3_11() {
        let at = SampleTime(Duration::from_millis(2));
        let cases = [
            (
                "touch down",
                ToHost::Touches {
                    active: ActiveTouch::Down,
                },
                Some(true),
                Some(PalmScroll::MayBegin),
            ),
            (
                "touch up",
                ToHost::Touches {
                    active: ActiveTouch::Up,
                },
                Some(false),
                None,
            ),
            (
                "began",
                scroll_msg(Phase::Began, 0.0),
                Some(true),
                Some(PalmScroll::Began {
                    axis: ScrollAxis::Y,
                }),
            ),
            (
                "changed",
                scroll_msg(Phase::Changed, 3.0),
                Some(true),
                Some(PalmScroll::Changed {
                    dx: 0.0,
                    dy: 3.0,
                    at,
                }),
            ),
            (
                "ended",
                scroll_msg(Phase::Ended, 0.0),
                Some(false),
                Some(PalmScroll::Ended { at }),
            ),
            (
                "cancelled",
                scroll_msg(Phase::Cancelled, 0.0),
                Some(false),
                Some(PalmScroll::Cancelled),
            ),
            (
                "tap",
                ToHost::Tap {
                    kind: TapKind::One,
                    t_ns: 0,
                },
                None,
                None,
            ),
        ];
        for (name, message, fingers, scroll) in cases {
            assert_eq!(route(&message), Route { fingers, scroll }, "{name}");
        }
    }
}

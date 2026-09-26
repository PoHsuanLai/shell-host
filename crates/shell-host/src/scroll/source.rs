//! Input sources to engine inputs (design/11 §11.3.2, §11.3.9, §11.3.13): a `wl_pointer`
//! axis frame is a wheel's detents or a touchpad's pan; a touchpad pan becomes a phased gesture
//! once its axis locks; palmrest's finger-direction deltas get `scroll.natural`. Pure.

use crate::scroll::config::NaturalScroll;
use crate::scroll::engine::Kinetic;
use crate::scroll::geom::{Axis, Dir};
use crate::scroll::latch::AxisLock;

/// libinput's degrees per wheel click, for a wheel event carrying neither `value120` nor
/// `discrete` (KWin and libinput report a click as 15).
const DEGREES_PER_DETENT: f64 = 15.0;

/// `wl_pointer.axis_source`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisSource {
    Wheel,
    Finger,
    Continuous,
    WheelTilt,
    /// No `axis_source` (a `wl_pointer` older than v5).
    Unknown,
}

/// Whether an axis carried `axis_stop` in this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    Stopped,
    Moving,
}

/// `wl_pointer.axis_relative_direction`: whether the compositor inverted the value relative to
/// the physical motion (its natural-scrolling setting). Informational: compositor values are
/// used as delivered, so natural scrolling is never applied twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relative {
    Identical,
    Inverted,
}

/// Whether Shift is held (a vertical wheel then scrolls horizontally).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shift {
    Held,
    Up,
}

/// One axis of a `wl_pointer.frame`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AxisValue {
    /// `wl_pointer.axis`, px; positive moves the offset toward its maximum.
    pub px: f64,
    pub v120: i32,
    pub discrete: i32,
    pub stop: Stop,
    pub relative: Relative,
}

impl AxisValue {
    /// No motion on this axis.
    pub const NONE: AxisValue = AxisValue {
        px: 0.0,
        v120: 0,
        discrete: 0,
        stop: Stop::Moving,
        relative: Relative::Identical,
    };
}

/// One `wl_pointer.frame`'s axis content.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AxisFrame {
    pub source: AxisSource,
    pub horizontal: AxisValue,
    pub vertical: AxisValue,
    pub shift: Shift,
}

/// What an axis frame means for scrolling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AxisInput {
    /// Wheel detents on `axis` (signed, 1.0 per click; positive toward the maximum).
    Detents {
        axis: Axis,
        steps: f64,
    },
    /// A continuous pan in offset px.
    Pan {
        dx: f64,
        dy: f64,
        kinetic: Kinetic,
    },
    /// `axis_stop` on the scrolling axes: the fingers lifted.
    Stop,
    Nothing,
}

/// The detents a wheel axis carries: `value120`, else `discrete`, else its degrees.
fn steps(axis: &AxisValue) -> f64 {
    if axis.v120 != 0 {
        f64::from(axis.v120) / 120.0
    } else if axis.discrete != 0 {
        f64::from(axis.discrete)
    } else {
        axis.px / DEGREES_PER_DETENT
    }
}

/// Classify one axis frame.
pub fn classify(frame: &AxisFrame) -> AxisInput {
    let (h, v) = (&frame.horizontal, &frame.vertical);
    let stopped = h.stop == Stop::Stopped || v.stop == Stop::Stopped;
    let moved = h.px != 0.0 || v.px != 0.0 || h.v120 != 0 || v.v120 != 0;
    // A frame without `axis_source` is a wheel's: a compositor that sends no source sends no
    // `axis_stop` either, so a pan from it could never end (and KWin's fake input, the live
    // acceptance's wheel, sends exactly this: `axis 15` and nothing else, FINDINGS F63).
    let wheel_like = match frame.source {
        AxisSource::Wheel | AxisSource::WheelTilt | AxisSource::Unknown => true,
        AxisSource::Finger | AxisSource::Continuous => false,
    };
    if !moved {
        return if stopped {
            AxisInput::Stop
        } else {
            AxisInput::Nothing
        };
    }
    if wheel_like {
        let (sx, sy) = (steps(h), steps(v));
        let horizontal = frame.source == AxisSource::WheelTilt || frame.shift == Shift::Held;
        return match (horizontal, sy != 0.0) {
            (true, true) => AxisInput::Detents {
                axis: Axis::X,
                steps: sy,
            },
            (_, _) if sx != 0.0 => AxisInput::Detents {
                axis: Axis::X,
                steps: sx,
            },
            (_, true) => AxisInput::Detents {
                axis: Axis::Y,
                steps: sy,
            },
            _ => AxisInput::Nothing,
        };
    }
    let kinetic = match frame.source {
        AxisSource::Finger => Kinetic::Momentum,
        _ => Kinetic::Plain,
    };
    AxisInput::Pan {
        dx: h.px,
        dy: v.px,
        kinetic,
    }
}

/// A touchpad gesture's phase, from its pans and stop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PadEvent {
    /// The axis locked: latch toward `dir`, then apply `delta`.
    Began {
        axis: Axis,
        dir: Dir,
        delta: f64,
        kinetic: Kinetic,
    },
    Changed {
        delta: f64,
    },
    Ended,
}

/// A touchpad gesture on one of our surfaces (design/11 §11.3.2: the first axis event is
/// Began, `axis_stop` is Ended; §11.3.9: locked to the larger axis at 10 px).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pad {
    Idle,
    Moving { lock: AxisLock, kinetic: Kinetic },
}

impl Pad {
    /// Feed one classified frame.
    pub fn feed(self, input: AxisInput) -> (Pad, Option<PadEvent>) {
        match (self, input) {
            (Pad::Idle, AxisInput::Pan { dx, dy, kinetic }) => Pad::Moving {
                lock: AxisLock::START,
                kinetic,
            }
            .feed(AxisInput::Pan { dx, dy, kinetic }),
            (Pad::Moving { lock, kinetic }, AxisInput::Pan { dx, dy, .. }) => match lock {
                AxisLock::Locked(axis) => {
                    let delta = match axis {
                        Axis::X => dx,
                        Axis::Y => dy,
                    };
                    (self, (delta != 0.0).then_some(PadEvent::Changed { delta }))
                }
                AxisLock::Free { .. } => {
                    let (lock, pending) = lock.feed(dx, dy);
                    let next = Pad::Moving { lock, kinetic };
                    let event = match (lock, pending) {
                        (AxisLock::Locked(axis), Some(delta)) => {
                            Dir::of(delta).map(|dir| PadEvent::Began {
                                axis,
                                dir,
                                delta,
                                kinetic,
                            })
                        }
                        _ => None,
                    };
                    (next, event)
                }
            },
            (Pad::Moving { lock, .. }, AxisInput::Stop) => {
                let event = matches!(lock, AxisLock::Locked(_)).then_some(PadEvent::Ended);
                (Pad::Idle, event)
            }
            (state, _) => (state, None),
        }
    }
}

/// A palmrest delta (finger direction) as an offset delta: with `Natural` the content follows
/// the finger, so a finger moving toward the maximum's side moves the offset the other way.
pub fn natural(finger: f64, setting: NaturalScroll) -> f64 {
    match setting {
        NaturalScroll::Natural => -finger,
        NaturalScroll::Traditional => finger,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(px: f64, v120: i32) -> AxisValue {
        AxisValue {
            px,
            v120,
            ..AxisValue::NONE
        }
    }

    fn frame(source: AxisSource, h: AxisValue, v: AxisValue, shift: Shift) -> AxisFrame {
        AxisFrame {
            source,
            horizontal: h,
            vertical: v,
            shift,
        }
    }

    #[test]
    fn frames_classify_by_source() {
        use AxisSource::*;
        let none = AxisValue::NONE;
        let stop = AxisValue {
            stop: Stop::Stopped,
            ..AxisValue::NONE
        };
        let cases = [
            (
                "one detent",
                frame(Wheel, none, value(15.0, 120), Shift::Up),
                AxisInput::Detents {
                    axis: Axis::Y,
                    steps: 1.0,
                },
            ),
            (
                "hi-res half",
                frame(Wheel, none, value(7.5, 60), Shift::Up),
                AxisInput::Detents {
                    axis: Axis::Y,
                    steps: 0.5,
                },
            ),
            (
                "shift turns it",
                frame(Wheel, none, value(-15.0, -120), Shift::Held),
                AxisInput::Detents {
                    axis: Axis::X,
                    steps: -1.0,
                },
            ),
            (
                "tilt",
                frame(WheelTilt, none, value(15.0, 120), Shift::Up),
                AxisInput::Detents {
                    axis: Axis::X,
                    steps: 1.0,
                },
            ),
            (
                "no value120: degrees",
                frame(Wheel, none, value(30.0, 0), Shift::Up),
                AxisInput::Detents {
                    axis: Axis::Y,
                    steps: 2.0,
                },
            ),
            (
                "no source: a wheel in degrees",
                frame(Unknown, none, value(15.0, 0), Shift::Up),
                AxisInput::Detents {
                    axis: Axis::Y,
                    steps: 1.0,
                },
            ),
            (
                "touchpad",
                frame(Finger, value(1.0, 0), value(-4.0, 0), Shift::Up),
                AxisInput::Pan {
                    dx: 1.0,
                    dy: -4.0,
                    kinetic: Kinetic::Momentum,
                },
            ),
            (
                "trackpoint",
                frame(Continuous, none, value(3.0, 0), Shift::Up),
                AxisInput::Pan {
                    dx: 0.0,
                    dy: 3.0,
                    kinetic: Kinetic::Plain,
                },
            ),
            (
                "stop",
                frame(Finger, none, stop, Shift::Up),
                AxisInput::Stop,
            ),
            (
                "nothing",
                frame(Finger, none, none, Shift::Up),
                AxisInput::Nothing,
            ),
        ];
        for (name, f, want) in cases {
            assert_eq!(classify(&f), want, "{name}");
        }
    }

    #[test]
    fn a_touchpad_stroke_is_began_changed_ended() {
        let pan = |dx, dy| AxisInput::Pan {
            dx,
            dy,
            kinetic: Kinetic::Momentum,
        };
        let (pad, e) = Pad::Idle.feed(pan(0.0, 4.0));
        assert_eq!(e, None);
        let (pad, e) = pad.feed(pan(1.0, 7.0));
        assert_eq!(
            e,
            Some(PadEvent::Began {
                axis: Axis::Y,
                dir: Dir::Pos,
                delta: 11.0,
                kinetic: Kinetic::Momentum
            })
        );
        let (pad, e) = pad.feed(pan(30.0, 5.0));
        assert_eq!(
            e,
            Some(PadEvent::Changed { delta: 5.0 }),
            "locked: x ignored"
        );
        let (pad, e) = pad.feed(AxisInput::Stop);
        assert_eq!((pad, e), (Pad::Idle, Some(PadEvent::Ended)));
        let (pad, e) = Pad::Idle.feed(pan(0.0, 2.0)).0.feed(AxisInput::Stop);
        assert_eq!(
            (pad, e),
            (Pad::Idle, None),
            "a stroke that never locked ends silently"
        );
    }

    #[test]
    fn natural_scrolling_of_11_8_item_15() {
        // A finger moving toward the user (+y) decreases the offset with Natural.
        assert_eq!(natural(10.0, NaturalScroll::Natural), -10.0);
        assert_eq!(natural(10.0, NaturalScroll::Traditional), 10.0);
    }
}

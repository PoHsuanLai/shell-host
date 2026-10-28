//! Input sources to engine inputs (design/11 §11.3.2, §11.3.9, §11.3.13): a `wl_pointer`
//! axis frame is a wheel's detents or a touchpad's pan; a touchpad pan becomes a phased gesture
//! once its axis locks (`blitz_kit::scroll::pad`). Pure.

use blitz_kit::scroll::engine::Kinetic;
use blitz_kit::scroll::geom::ScrollAxis;
use blitz_kit::scroll::pad::PointerScroll;

/// libinput's degrees per wheel click, for a wheel event carrying neither `value120` nor
/// `discrete` (KWin and libinput report a click as 15).
const DEGREES_PER_DETENT: f64 = 15.0;

/// `wl_pointer.axis_source`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AxisSource {
    Wheel,
    Finger,
    Continuous,
    WheelTilt,
    /// No `axis_source` (a `wl_pointer` older than v5).
    Unknown,
}

/// One axis of a `wl_pointer.frame`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AxisValue {
    /// `wl_pointer.axis`, px; positive moves the offset toward its maximum.
    pub px: f64,
    pub v120: i32,
    pub discrete: i32,
    /// The axis carried `axis_stop` in this frame. (`wl_pointer.axis_relative_direction` is not
    /// kept: compositor values are used as delivered, so natural scrolling is never applied
    /// twice.)
    pub stop: bool,
}

#[cfg(test)]
impl AxisValue {
    /// No motion on this axis.
    pub(crate) const NONE: AxisValue = AxisValue {
        px: 0.0,
        v120: 0,
        discrete: 0,
        stop: false,
    };
}

/// One `wl_pointer.frame`'s axis content.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AxisFrame {
    pub source: AxisSource,
    pub horizontal: AxisValue,
    pub vertical: AxisValue,
    /// Shift is held (a vertical wheel then scrolls horizontally).
    pub shift: bool,
}

/// The detents a wheel axis carries: `value120`, else `discrete`, else its degrees.
pub(crate) fn steps(axis: &AxisValue) -> f64 {
    if axis.v120 != 0 {
        f64::from(axis.v120) / 120.0
    } else if axis.discrete != 0 {
        f64::from(axis.discrete)
    } else {
        axis.px / DEGREES_PER_DETENT
    }
}

/// Classify one axis frame.
pub(crate) fn classify(frame: &AxisFrame) -> PointerScroll {
    let (h, v) = (&frame.horizontal, &frame.vertical);
    let stopped = h.stop || v.stop;
    let moved = h.px != 0.0 || v.px != 0.0 || h.v120 != 0 || v.v120 != 0;
    // A frame without `axis_source` is a wheel's: a compositor that sends no source sends no
    // `axis_stop` either, so a pan from it could never end (and KWin's fake input, the live
    // acceptance's wheel, sends exactly this: `axis 15` and nothing else).
    let wheel_like = match frame.source {
        AxisSource::Wheel | AxisSource::WheelTilt | AxisSource::Unknown => true,
        AxisSource::Finger | AxisSource::Continuous => false,
    };
    if !moved {
        return if stopped {
            PointerScroll::Stop
        } else {
            PointerScroll::Nothing
        };
    }
    if wheel_like {
        let (sx, sy) = (steps(h), steps(v));
        let horizontal = frame.source == AxisSource::WheelTilt || frame.shift;
        return match (horizontal, sy != 0.0) {
            (true, true) => PointerScroll::Detents {
                axis: ScrollAxis::X,
                steps: sy,
            },
            (_, _) if sx != 0.0 => PointerScroll::Detents {
                axis: ScrollAxis::X,
                steps: sx,
            },
            (_, true) => PointerScroll::Detents {
                axis: ScrollAxis::Y,
                steps: sy,
            },
            _ => PointerScroll::Nothing,
        };
    }
    let kinetic = match frame.source {
        AxisSource::Finger => Kinetic::Momentum,
        _ => Kinetic::Plain,
    };
    PointerScroll::Pan {
        dx: h.px,
        dy: v.px,
        kinetic,
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

    fn frame(source: AxisSource, h: AxisValue, v: AxisValue, shift: bool) -> AxisFrame {
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
            stop: true,
            ..AxisValue::NONE
        };
        let cases = [
            (
                "one detent",
                frame(Wheel, none, value(15.0, 120), false),
                PointerScroll::Detents {
                    axis: ScrollAxis::Y,
                    steps: 1.0,
                },
            ),
            (
                "hi-res half",
                frame(Wheel, none, value(7.5, 60), false),
                PointerScroll::Detents {
                    axis: ScrollAxis::Y,
                    steps: 0.5,
                },
            ),
            (
                "shift turns it",
                frame(Wheel, none, value(-15.0, -120), true),
                PointerScroll::Detents {
                    axis: ScrollAxis::X,
                    steps: -1.0,
                },
            ),
            (
                "tilt",
                frame(WheelTilt, none, value(15.0, 120), false),
                PointerScroll::Detents {
                    axis: ScrollAxis::X,
                    steps: 1.0,
                },
            ),
            (
                "no value120: degrees",
                frame(Wheel, none, value(30.0, 0), false),
                PointerScroll::Detents {
                    axis: ScrollAxis::Y,
                    steps: 2.0,
                },
            ),
            (
                "no source: a wheel in degrees",
                frame(Unknown, none, value(15.0, 0), false),
                PointerScroll::Detents {
                    axis: ScrollAxis::Y,
                    steps: 1.0,
                },
            ),
            (
                "touchpad",
                frame(Finger, value(1.0, 0), value(-4.0, 0), false),
                PointerScroll::Pan {
                    dx: 1.0,
                    dy: -4.0,
                    kinetic: Kinetic::Momentum,
                },
            ),
            (
                "trackpoint",
                frame(Continuous, none, value(3.0, 0), false),
                PointerScroll::Pan {
                    dx: 0.0,
                    dy: 3.0,
                    kinetic: Kinetic::Plain,
                },
            ),
            (
                "stop",
                frame(Finger, none, stop, false),
                PointerScroll::Stop,
            ),
            (
                "nothing",
                frame(Finger, none, none, false),
                PointerScroll::Nothing,
            ),
        ];
        for (name, f, want) in cases {
            assert_eq!(classify(&f), want, "{name}");
        }
    }
}

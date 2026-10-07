//! A wheel or touchpad as a component hears it (`use_wheel`): the `wl_pointer` axis frame
//! reduced to a source, a phase and a distance. The scroll engine takes the same frame for the
//! host's own scrolling (`scroll::source`); this is for a component that moves its own content
//! (a canvas, a viewer) and needs what the engine hides: whether the fingers are down, and
//! when they lift. Pure.

use crate::geometry::Point;
use crate::modifiers::Modifiers;
use crate::scroll::source::{AxisFrame, AxisSource, Stop, steps};

/// What turned: the compositor's `wl_pointer.axis_source`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WheelSource {
    /// A wheel's detents (a tilt wheel's too; a compositor that names no source is taken as
    /// a wheel).
    Wheel,
    /// Fingers on a touchpad: they have a beginning and a lift.
    Finger,
    /// A continuous motion with no fingers, such as a pointing stick or button scrolling.
    Continuous,
}

/// Where in its run a wheel event is. Only a source with a beginning and an end (`Finger`,
/// `Continuous`) goes through `Began` and `Ended`; a wheel reports `Changed` only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WheelPhase {
    /// The first motion of a run.
    Began,
    /// Motion within a run, and every wheel detent.
    Changed,
    /// The fingers lifted (`wl_pointer.axis_stop`); this event may carry a last motion.
    Ended,
}

/// How far the content moves, in logical px. Fractional because a touchpad's motion is; the
/// only float in the public vocabulary of input.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WheelMotion {
    /// Positive to the right: the direction the content travels, not the offset's.
    pub x: f64,
    /// Positive down.
    pub y: f64,
}

/// One `wl_pointer.frame`'s scroll: all its axes, source and stop grouped as the compositor
/// grouped them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WheelEvent {
    pub source: WheelSource,
    pub phase: WheelPhase,
    /// A wheel detent is the host's wheel step (60 px by default), a touchpad's px are its own.
    pub by: WheelMotion,
    /// The pointer, in the surface's logical px.
    pub at: Point,
    /// The modifiers held: a viewer zooms with the wheel under Control and scrolls otherwise.
    pub held: Modifiers,
}

/// Whether a run of a source with fingers is under way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum WheelRun {
    #[default]
    Idle,
    Under,
}

/// What the host knows of a frame besides the frame: the pointer, the modifiers and the step a
/// wheel detent is.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WheelContext {
    pub at: Point,
    pub held: Modifiers,
    pub detent_px: f64,
}

/// The source `source` names.
fn source_of(source: AxisSource) -> WheelSource {
    match source {
        AxisSource::Finger => WheelSource::Finger,
        AxisSource::Continuous => WheelSource::Continuous,
        AxisSource::Wheel | AxisSource::WheelTilt | AxisSource::Unknown => WheelSource::Wheel,
    }
}

/// The distance `frame` moves the content, in the sign a gesture reports: `wl_pointer`'s axis
/// is the offset's direction, so it is negated.
fn motion_of(frame: &AxisFrame, source: WheelSource, detent_px: f64) -> WheelMotion {
    let px = |axis: &crate::scroll::source::AxisValue| match source {
        WheelSource::Wheel => steps(axis) * detent_px,
        WheelSource::Finger | WheelSource::Continuous => axis.px,
    };
    WheelMotion {
        x: -px(&frame.horizontal),
        y: -px(&frame.vertical),
    }
}

/// The phase a frame with `moved` motion and `stop` gives a run that is `run`, and the run after
/// it. `None` for a frame that says nothing: no motion, and nothing to end.
fn phase_after(run: WheelRun, moved: bool, stop: Stop) -> (WheelRun, Option<WheelPhase>) {
    match (run, moved, stop) {
        (WheelRun::Under, _, Stop::Stopped) | (WheelRun::Idle, true, Stop::Stopped) => {
            (WheelRun::Idle, Some(WheelPhase::Ended))
        }
        (WheelRun::Idle, true, Stop::Moving) => (WheelRun::Under, Some(WheelPhase::Began)),
        (WheelRun::Under, true, Stop::Moving) => (WheelRun::Under, Some(WheelPhase::Changed)),
        (run, false, Stop::Moving) => (run, None),
        (WheelRun::Idle, false, Stop::Stopped) => (WheelRun::Idle, None),
    }
}

/// The event `frame` is, if it is one, and the run after it. A wheel never has a run.
pub(crate) fn wheel_event(
    run: WheelRun,
    frame: &AxisFrame,
    context: WheelContext,
) -> (WheelRun, Option<WheelEvent>) {
    let source = source_of(frame.source);
    let by = motion_of(frame, source, context.detent_px);
    let moved = by != WheelMotion::default();
    let stop = match (frame.horizontal.stop, frame.vertical.stop) {
        (Stop::Moving, Stop::Moving) => Stop::Moving,
        _ => Stop::Stopped,
    };
    let (next, phase) = match source {
        WheelSource::Wheel => (WheelRun::Idle, moved.then_some(WheelPhase::Changed)),
        WheelSource::Finger | WheelSource::Continuous => phase_after(run, moved, stop),
    };
    let event = phase.map(|phase| WheelEvent {
        source,
        phase,
        by,
        at: context.at,
        held: context.held,
    });
    (next, event)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modifiers::Modifier;
    use crate::scroll::source::{AxisValue, Relative, Shift};

    fn context() -> WheelContext {
        WheelContext {
            at: Point { x: 10, y: 20 },
            held: Modifiers {
                ctrl: Modifier::Held,
                ..Modifiers::default()
            },
            detent_px: 60.0,
        }
    }

    fn value(px: f64, v120: i32, stop: Stop) -> AxisValue {
        AxisValue {
            px,
            v120,
            discrete: 0,
            stop,
            relative: Relative::Identical,
        }
    }

    fn frame(source: AxisSource, h: AxisValue, v: AxisValue) -> AxisFrame {
        AxisFrame {
            source,
            horizontal: h,
            vertical: v,
            shift: Shift::Up,
        }
    }

    const MOVING: Stop = Stop::Moving;

    #[test]
    fn a_frame_becomes_a_source_a_phase_and_a_distance() {
        use AxisSource::*;
        let none = value(0.0, 0, MOVING);
        let lifted = value(0.0, 0, Stop::Stopped);
        // name, run before, frame, then the run after and the event's source, phase and by
        #[rustfmt::skip]
        let cases = [
            ("a wheel click is one detent and always Changed",
                WheelRun::Idle, frame(Wheel, none, value(15.0, 120, MOVING)),
                WheelRun::Idle, Some((WheelSource::Wheel, WheelPhase::Changed, (0.0, -60.0)))),
            ("a wheel scrolled up is content moving down",
                WheelRun::Idle, frame(Wheel, none, value(-15.0, -120, MOVING)),
                WheelRun::Idle, Some((WheelSource::Wheel, WheelPhase::Changed, (0.0, 60.0)))),
            ("a compositor that names no source is a wheel",
                WheelRun::Idle, frame(Unknown, none, value(15.0, 0, MOVING)),
                WheelRun::Idle, Some((WheelSource::Wheel, WheelPhase::Changed, (0.0, -60.0)))),
            ("fingers' first motion begins a run",
                WheelRun::Idle, frame(Finger, value(1.0, 0, MOVING), value(-4.0, 0, MOVING)),
                WheelRun::Under, Some((WheelSource::Finger, WheelPhase::Began, (-1.0, 4.0)))),
            ("more motion changes it",
                WheelRun::Under, frame(Finger, none, value(-4.0, 0, MOVING)),
                WheelRun::Under, Some((WheelSource::Finger, WheelPhase::Changed, (0.0, 4.0)))),
            ("axis_stop ends it",
                WheelRun::Under, frame(Finger, none, lifted),
                WheelRun::Idle, Some((WheelSource::Finger, WheelPhase::Ended, (0.0, 0.0)))),
            ("a last motion rides on the end",
                WheelRun::Under, frame(Finger, none, value(-2.0, 0, Stop::Stopped)),
                WheelRun::Idle, Some((WheelSource::Finger, WheelPhase::Ended, (0.0, 2.0)))),
            ("a stop with nothing running says nothing",
                WheelRun::Idle, frame(Finger, none, lifted),
                WheelRun::Idle, None),
            ("a frame with no motion says nothing",
                WheelRun::Under, frame(Finger, none, none),
                WheelRun::Under, None),
            ("a pointing stick is its own source",
                WheelRun::Idle, frame(Continuous, none, value(3.0, 0, MOVING)),
                WheelRun::Under, Some((WheelSource::Continuous, WheelPhase::Began, (0.0, -3.0)))),
        ];
        for (name, run, frame, want_run, want) in cases {
            let (got_run, got) = wheel_event(run, &frame, context());
            let got = got.map(|e| (e.source, e.phase, (e.by.x, e.by.y)));
            assert_eq!((got_run, got), (want_run, want), "{name}");
        }
    }

    #[test]
    fn the_event_carries_the_pointer_and_the_modifiers() {
        let none = value(0.0, 0, MOVING);
        let frame = frame(AxisSource::Wheel, none, value(15.0, 120, MOVING));
        let (_, event) = wheel_event(WheelRun::Idle, &frame, context());
        let event = event.expect("a click is an event");
        assert_eq!((event.at, event.held), (context().at, context().held));
    }
}

//! The frame pacing machine: when a surface renders, and when it waits.
//!
//! Pure and table-tested. The host feeds it what happened ([`FrameInput`]) and performs what it
//! returns ([`FrameAction`]); it never touches a surface, a timer or a clock itself.
//!
//! The rules, from the plan's "Frame machine" and SPIKE.md S1:
//!
//! - At most one frame is in flight. Rendering happens only at the host's flush point, and only
//!   from `Dirty`; a change while a frame is in flight is remembered as `InFlightDirty`.
//! - The host's render order is `wl_surface.frame()`, then `queue.submit`, then
//!   `SurfaceTexture::present()` (the WSI's commit carries the frame request, S1 finding 3), and
//!   it clears the document's redraw flag after presenting (S1 finding 4).
//! - On the frame callback the surface renders again iff it was dirtied meanwhile or the
//!   document is still animating; otherwise it goes idle, and idle means no timer, no frame and
//!   no commit.
//! - A watchdog runs only while a frame is in flight *and* more work is wanted (dirty or
//!   animating). It covers compositors that stop sending callbacks to a surface (cosmic-comp
//!   #2777; KWin keeps them flowing even when occluded, S1 item c). It starts at
//!   [`WATCHDOG_MIN`], doubles on every expiry up to [`WATCHDOG_MAX`], and resets on a real
//!   callback.
//! - A present that could not happen (no texture, render error) is treated as a frame in flight
//!   whose callback will never come, so the watchdog retries it with the same backoff instead of
//!   spinning.

use std::time::Duration;

/// The shortest watchdog wait: about three refresh periods at 60 Hz.
pub const WATCHDOG_MIN: Duration = Duration::from_millis(50);

/// The longest watchdog wait: the backoff stops doubling here.
pub const WATCHDOG_MAX: Duration = Duration::from_millis(1000);

/// Where a surface is in its render cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pace {
    /// Nothing to draw and nothing in flight.
    Idle,
    /// Something changed; the next flush renders.
    Dirty,
    /// A frame was presented and its callback has not arrived.
    InFlight,
    /// A frame is in flight and something changed since it was rendered.
    InFlightDirty,
}

/// Whether the document still has a CSS transition or animation running
/// (`BaseDocument::is_animating`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Animation {
    Running,
    Stopped,
}

/// Whether the watchdog timer is currently armed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Timer {
    Disarmed,
    Armed,
}

/// Everything the machine remembers about one surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameState {
    pub pace: Pace,
    pub timer: Timer,
    /// The wait the watchdog uses the next time it is armed.
    pub backoff: Duration,
}

impl FrameState {
    /// A freshly created surface: idle, no timer, shortest backoff. Its first configure
    /// arrives as an [`FrameInput::Invalidate`].
    pub const NEW: FrameState = FrameState {
        pace: Pace::Idle,
        timer: Timer::Disarmed,
        backoff: WATCHDOG_MIN,
    };
}

/// Something that happened to a surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameInput {
    /// The document asked for a redraw, was polled with changes, got an input event that
    /// changed it, or the surface was configured or rescaled.
    Invalidate,
    /// The host reached its flush point with this surface configured and visible.
    Flush,
    /// The host rendered and presented the frame `Flush` asked for.
    Presented(Animation),
    /// The host could not present the frame `Flush` asked for.
    PresentFailed,
    /// The compositor's frame callback for the frame in flight arrived.
    Callback(Animation),
    /// The watchdog timer expired.
    WatchdogFired,
}

/// Whether the host should render and present now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderCmd {
    Skip,
    Render,
}

/// What the host should do with the surface's watchdog timer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerCmd {
    /// Leave it as it is (armed, disarmed, or already fired and gone).
    Keep,
    /// Start a one-shot timer that fires `FrameInput::WatchdogFired` after this long.
    Arm(Duration),
    /// Remove the pending timer.
    Cancel,
}

/// What the host must do after a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameAction {
    pub render: RenderCmd,
    pub timer: TimerCmd,
}

impl FrameAction {
    /// Do nothing.
    pub const NONE: FrameAction = FrameAction {
        render: RenderCmd::Skip,
        timer: TimerCmd::Keep,
    };

    const RENDER: FrameAction = FrameAction {
        render: RenderCmd::Render,
        timer: TimerCmd::Keep,
    };
}

/// Advance the machine by one input.
pub fn step(state: FrameState, input: FrameInput) -> (FrameState, FrameAction) {
    match input {
        FrameInput::Invalidate => invalidate(state),
        FrameInput::Flush => flush(state),
        FrameInput::Presented(animation) => presented(state, animation),
        FrameInput::PresentFailed => present_failed(state),
        FrameInput::Callback(animation) => callback(state, animation),
        FrameInput::WatchdogFired => watchdog_fired(state),
    }
}

fn invalidate(state: FrameState) -> (FrameState, FrameAction) {
    match state.pace {
        Pace::Idle => (with_pace(state, Pace::Dirty), FrameAction::NONE),
        Pace::Dirty | Pace::InFlightDirty => (state, FrameAction::NONE),
        Pace::InFlight => arm(with_pace(state, Pace::InFlightDirty)),
    }
}

fn flush(state: FrameState) -> (FrameState, FrameAction) {
    match state.pace {
        Pace::Dirty => (with_pace(state, Pace::InFlight), FrameAction::RENDER),
        Pace::Idle | Pace::InFlight | Pace::InFlightDirty => (state, FrameAction::NONE),
    }
}

fn presented(state: FrameState, animation: Animation) -> (FrameState, FrameAction) {
    match (state.pace, animation) {
        (Pace::InFlight, Animation::Running) => arm(state),
        _ => (state, FrameAction::NONE),
    }
}

fn present_failed(state: FrameState) -> (FrameState, FrameAction) {
    match state.pace {
        Pace::InFlight | Pace::InFlightDirty => arm(with_pace(state, Pace::InFlightDirty)),
        Pace::Idle | Pace::Dirty => (state, FrameAction::NONE),
    }
}

fn callback(state: FrameState, animation: Animation) -> (FrameState, FrameAction) {
    let next = match (state.pace, animation) {
        (Pace::InFlight, Animation::Stopped) => Pace::Idle,
        (Pace::InFlight, Animation::Running) | (Pace::InFlightDirty, _) => Pace::Dirty,
        // A callback for a frame we no longer track: nothing to settle.
        (Pace::Idle | Pace::Dirty, _) => return (state, FrameAction::NONE),
    };
    let timer = match state.timer {
        Timer::Armed => TimerCmd::Cancel,
        Timer::Disarmed => TimerCmd::Keep,
    };
    let settled = FrameState {
        pace: next,
        timer: Timer::Disarmed,
        backoff: WATCHDOG_MIN,
    };
    (
        settled,
        FrameAction {
            render: RenderCmd::Skip,
            timer,
        },
    )
}

fn watchdog_fired(state: FrameState) -> (FrameState, FrameAction) {
    let pace = match state.pace {
        Pace::InFlight | Pace::InFlightDirty => Pace::Dirty,
        Pace::Idle | Pace::Dirty => state.pace,
    };
    let fired = FrameState {
        pace,
        timer: Timer::Disarmed,
        backoff: doubled(state.backoff),
    };
    (fired, FrameAction::NONE)
}

/// Arm the watchdog unless it already is.
fn arm(state: FrameState) -> (FrameState, FrameAction) {
    match state.timer {
        Timer::Armed => (state, FrameAction::NONE),
        Timer::Disarmed => (
            FrameState {
                timer: Timer::Armed,
                ..state
            },
            FrameAction {
                render: RenderCmd::Skip,
                timer: TimerCmd::Arm(state.backoff),
            },
        ),
    }
}

fn with_pace(state: FrameState, pace: Pace) -> FrameState {
    FrameState { pace, ..state }
}

fn doubled(backoff: Duration) -> Duration {
    (backoff * 2).clamp(WATCHDOG_MIN, WATCHDOG_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn st(pace: Pace, timer: Timer, backoff_ms: u64) -> FrameState {
        FrameState {
            pace,
            timer,
            backoff: Duration::from_millis(backoff_ms),
        }
    }

    const fn act(render: RenderCmd, timer: TimerCmd) -> FrameAction {
        FrameAction { render, timer }
    }

    const ARM_50: TimerCmd = TimerCmd::Arm(Duration::from_millis(50));
    const ARM_400: TimerCmd = TimerCmd::Arm(Duration::from_millis(400));

    use Animation::{Running, Stopped};
    use FrameInput as I;
    use Pace::{Dirty, Idle, InFlight, InFlightDirty};
    use RenderCmd::{Render, Skip};
    use Timer::{Armed, Disarmed};
    use TimerCmd::{Cancel, Keep};

    #[allow(clippy::type_complexity)]
    #[rustfmt::skip]
    const CASES: &[(&str, FrameState, FrameInput, FrameState, FrameAction)] = &[
        // Invalidate
        ("idle dirtied waits for flush", st(Idle, Disarmed, 50), I::Invalidate, st(Dirty, Disarmed, 50), act(Skip, Keep)),
        ("dirty stays dirty", st(Dirty, Disarmed, 50), I::Invalidate, st(Dirty, Disarmed, 50), act(Skip, Keep)),
        ("in flight dirtied arms the watchdog", st(InFlight, Disarmed, 50), I::Invalidate, st(InFlightDirty, Armed, 50), act(Skip, ARM_50)),
        ("in flight, animating, dirtied keeps its timer", st(InFlight, Armed, 50), I::Invalidate, st(InFlightDirty, Armed, 50), act(Skip, Keep)),
        ("in flight dirty stays", st(InFlightDirty, Armed, 50), I::Invalidate, st(InFlightDirty, Armed, 50), act(Skip, Keep)),
        // Flush
        ("flush renders a dirty surface", st(Dirty, Disarmed, 50), I::Flush, st(InFlight, Disarmed, 50), act(Render, Keep)),
        ("flush leaves idle alone", st(Idle, Disarmed, 50), I::Flush, st(Idle, Disarmed, 50), act(Skip, Keep)),
        ("flush never renders over a frame in flight", st(InFlight, Disarmed, 50), I::Flush, st(InFlight, Disarmed, 50), act(Skip, Keep)),
        ("flush waits out in-flight-dirty", st(InFlightDirty, Armed, 50), I::Flush, st(InFlightDirty, Armed, 50), act(Skip, Keep)),
        // Presented
        ("static frame presented: no timer", st(InFlight, Disarmed, 50), I::Presented(Stopped), st(InFlight, Disarmed, 50), act(Skip, Keep)),
        ("animating frame presented arms", st(InFlight, Disarmed, 50), I::Presented(Running), st(InFlight, Armed, 50), act(Skip, ARM_50)),
        ("animating uses the current backoff", st(InFlight, Disarmed, 400), I::Presented(Running), st(InFlight, Armed, 400), act(Skip, ARM_400)),
        // PresentFailed
        ("failed present retries via the watchdog", st(InFlight, Disarmed, 50), I::PresentFailed, st(InFlightDirty, Armed, 50), act(Skip, ARM_50)),
        ("failed present on idle is ignored", st(Idle, Disarmed, 50), I::PresentFailed, st(Idle, Disarmed, 50), act(Skip, Keep)),
        // Callback
        ("callback, still: idle and quiet", st(InFlight, Disarmed, 50), I::Callback(Stopped), st(Idle, Disarmed, 50), act(Skip, Keep)),
        ("callback, animating: render again", st(InFlight, Armed, 50), I::Callback(Running), st(Dirty, Disarmed, 50), act(Skip, Cancel)),
        ("callback, dirtied: render again", st(InFlightDirty, Armed, 50), I::Callback(Stopped), st(Dirty, Disarmed, 50), act(Skip, Cancel)),
        ("callback resets the backoff", st(InFlightDirty, Armed, 800), I::Callback(Stopped), st(Dirty, Disarmed, 50), act(Skip, Cancel)),
        ("stale callback on idle", st(Idle, Disarmed, 50), I::Callback(Running), st(Idle, Disarmed, 50), act(Skip, Keep)),
        ("stale callback on dirty", st(Dirty, Disarmed, 200), I::Callback(Stopped), st(Dirty, Disarmed, 200), act(Skip, Keep)),
        // WatchdogFired
        ("watchdog unsticks and doubles", st(InFlightDirty, Armed, 50), I::WatchdogFired, st(Dirty, Disarmed, 100), act(Skip, Keep)),
        ("watchdog on animating in-flight", st(InFlight, Armed, 200), I::WatchdogFired, st(Dirty, Disarmed, 400), act(Skip, Keep)),
        ("backoff caps at 1000 ms", st(InFlightDirty, Armed, 800), I::WatchdogFired, st(Dirty, Disarmed, 1000), act(Skip, Keep)),
        ("backoff stays at the cap", st(InFlightDirty, Armed, 1000), I::WatchdogFired, st(Dirty, Disarmed, 1000), act(Skip, Keep)),
        ("late watchdog on idle", st(Idle, Armed, 50), I::WatchdogFired, st(Idle, Disarmed, 100), act(Skip, Keep)),
    ];

    #[test]
    fn every_transition_matches_the_table() {
        for &(name, before, input, after, action) in CASES {
            assert_eq!(step(before, input), (after, action), "{name}");
        }
    }

    /// Drives a sequence and returns every action, so a whole cycle reads as one case.
    fn run(inputs: &[FrameInput]) -> (FrameState, Vec<FrameAction>) {
        inputs
            .iter()
            .fold((FrameState::NEW, Vec::new()), |(s, mut acts), &i| {
                let (next, a) = step(s, i);
                acts.push(a);
                (next, acts)
            })
    }

    #[test]
    #[rustfmt::skip]
    fn a_static_change_renders_once_then_idles_with_no_timer() {
        let (end, acts) = run(&[I::Invalidate, I::Flush, I::Presented(Stopped), I::Callback(Stopped), I::Flush]);
        assert_eq!(end, FrameState::NEW);
        let renders = acts.iter().filter(|a| a.render == Render).count();
        assert_eq!(renders, 1);
        assert!(acts.iter().all(|a| a.timer == Keep), "{acts:?}");
    }

    #[test]
    #[rustfmt::skip]
    fn a_burst_of_changes_in_flight_coalesces_into_one_more_frame() {
        let (end, acts) = run(&[
            I::Invalidate, I::Flush, I::Presented(Stopped),
            I::Invalidate, I::Invalidate, I::Invalidate, I::Flush,
            I::Callback(Stopped), I::Flush, I::Presented(Stopped), I::Callback(Stopped),
        ]);
        assert_eq!(end.pace, Idle);
        assert_eq!(acts.iter().filter(|a| a.render == Render).count(), 2);
    }

    #[test]
    fn a_compositor_that_stops_calling_back_is_retried_with_growing_backoff() {
        // Animating, and no callback ever arrives: every retry is paced by the watchdog.
        let mut s = run(&[I::Invalidate, I::Flush]).0;
        let mut waits = Vec::new();
        for _ in 0..7 {
            let (armed, action) = step(s, I::Presented(Running));
            if let TimerCmd::Arm(wait) = action.timer {
                waits.push(wait.as_millis());
            }
            let (dirty, _) = step(armed, I::WatchdogFired);
            let (rendering, action) = step(dirty, I::Flush);
            assert_eq!(action.render, Render);
            s = rendering;
        }
        assert_eq!(waits, vec![50, 100, 200, 400, 800, 1000, 1000]);
    }
}

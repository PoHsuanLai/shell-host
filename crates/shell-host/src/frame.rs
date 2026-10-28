//! The frame pacing machine: when a surface renders, and when it waits.
//!
//! Pure and table-tested. The host feeds it what happened ([`FrameInput`]) and performs what it
//! returns ([`FrameAction`]); it never touches a surface, a timer or a clock itself.
//!
//! The rules, from the plan's "Frame machine":
//!
//! - At most one frame is in flight. Rendering happens only at the host's flush point, and only
//!   from `Dirty`; a change while a frame is in flight is remembered as `InFlightDirty`.
//! - The host's render order is `wl_surface.frame()`, then `queue.submit`, then
//!   `SurfaceTexture::present()` (the WSI's commit carries the frame request), and
//!   it clears the document's redraw flag after presenting.
//! - On the frame callback the surface renders again iff it was dirtied meanwhile or the
//!   document is still animating; otherwise it goes idle, and idle means no timer, no frame and
//!   no commit.
//! - A watchdog runs only while a frame is in flight *and* more work is wanted (dirty or
//!   animating). It covers compositors that stop sending callbacks to a surface (some stop
//!   them for an occluded surface; KWin keeps them flowing). It starts at
//!   [`WATCHDOG_MIN`], doubles on every expiry up to [`WATCHDOG_MAX`], and resets on a real
//!   callback.
//! - A present that could not happen (no texture, render error) is treated as a frame in flight
//!   whose callback will never come, so the watchdog retries it with the same backoff instead of
//!   spinning.
//! - A redraw a document asks for while its frame renders (a resource load completing on
//!   another thread) is not the render's own echo: the host feeds it as an `Invalidate` after
//!   `Presented`, which marks the frame in flight dirty and arms the watchdog, so it is painted
//!   on the callback or, on a compositor that never sends one, when the watchdog fires.
//!
//! See design/10 §10.7; FINDINGS F75.

use std::time::Duration;

/// The shortest watchdog wait: about three refresh periods at 60 Hz.
pub(crate) const WATCHDOG_MIN: Duration = Duration::from_millis(50);

/// The longest watchdog wait: the backoff stops doubling here.
pub(crate) const WATCHDOG_MAX: Duration = Duration::from_millis(1000);

/// Where a surface is in its render cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pace {
    /// Nothing to draw and nothing in flight.
    Idle,
    /// Something changed; the next flush renders.
    Dirty,
    /// A frame was presented and its callback has not arrived.
    InFlight,
    /// A frame is in flight and something changed since it was rendered.
    InFlightDirty,
}

/// Everything the machine remembers about one surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FrameState {
    pub pace: Pace,
    /// The watchdog timer is currently armed.
    pub timer_armed: bool,
    /// The wait the watchdog uses the next time it is armed.
    pub backoff: Duration,
}

impl FrameState {
    /// A freshly created surface: idle, no timer, shortest backoff. Its first configure
    /// arrives as an [`FrameInput::Invalidate`].
    pub(crate) const NEW: FrameState = FrameState {
        pace: Pace::Idle,
        timer_armed: false,
        backoff: WATCHDOG_MIN,
    };
}

/// Something that happened to a surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameInput {
    /// The document asked for a redraw, was polled with changes, got an input event that
    /// changed it, or the surface was configured or rescaled.
    Invalidate,
    /// The host reached its flush point with this surface configured and visible.
    Flush,
    /// The host rendered and presented the frame `Flush` asked for. `animating`: the surface
    /// still has a CSS transition or animation running, or is driven (`tick::pacing`).
    Presented { animating: bool },
    /// The host could not present the frame `Flush` asked for.
    PresentFailed,
    /// The compositor's frame callback for the frame in flight arrived; `animating` as for
    /// `Presented`.
    Callback { animating: bool },
    /// The watchdog timer expired.
    WatchdogFired,
}

/// What the host should do with the surface's watchdog timer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TimerCmd {
    /// Leave it as it is (armed, disarmed, or already fired and gone).
    Keep,
    /// Start a one-shot timer that fires `FrameInput::WatchdogFired` after this long.
    Arm(Duration),
    /// Remove the pending timer.
    Cancel,
}

/// What the host must do after a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FrameAction {
    /// The host should render and present now.
    pub render: bool,
    pub timer: TimerCmd,
}

impl FrameAction {
    /// Do nothing.
    pub(crate) const NONE: FrameAction = FrameAction {
        render: false,
        timer: TimerCmd::Keep,
    };

    const RENDER: FrameAction = FrameAction {
        render: true,
        timer: TimerCmd::Keep,
    };
}

/// Advance the machine by one input.
pub(crate) fn step(state: FrameState, input: FrameInput) -> (FrameState, FrameAction) {
    match input {
        FrameInput::Invalidate => invalidate(state),
        FrameInput::Flush => flush(state),
        FrameInput::Presented { animating } => presented(state, animating),
        FrameInput::PresentFailed => present_failed(state),
        FrameInput::Callback { animating } => callback(state, animating),
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

fn presented(state: FrameState, animating: bool) -> (FrameState, FrameAction) {
    match (state.pace, animating) {
        (Pace::InFlight, true) => arm(state),
        _ => (state, FrameAction::NONE),
    }
}

fn present_failed(state: FrameState) -> (FrameState, FrameAction) {
    match state.pace {
        Pace::InFlight | Pace::InFlightDirty => arm(with_pace(state, Pace::InFlightDirty)),
        Pace::Idle | Pace::Dirty => (state, FrameAction::NONE),
    }
}

fn callback(state: FrameState, animating: bool) -> (FrameState, FrameAction) {
    let next = match (state.pace, animating) {
        (Pace::InFlight, false) => Pace::Idle,
        (Pace::InFlight, true) | (Pace::InFlightDirty, _) => Pace::Dirty,
        // A callback for a frame we no longer track: nothing to settle.
        (Pace::Idle | Pace::Dirty, _) => return (state, FrameAction::NONE),
    };
    let timer = if state.timer_armed {
        TimerCmd::Cancel
    } else {
        TimerCmd::Keep
    };
    let settled = FrameState {
        pace: next,
        timer_armed: false,
        backoff: WATCHDOG_MIN,
    };
    (
        settled,
        FrameAction {
            render: false,
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
        timer_armed: false,
        backoff: doubled(state.backoff),
    };
    (fired, FrameAction::NONE)
}

/// Arm the watchdog unless it already is.
fn arm(state: FrameState) -> (FrameState, FrameAction) {
    if state.timer_armed {
        return (state, FrameAction::NONE);
    }
    (
        FrameState {
            timer_armed: true,
            ..state
        },
        FrameAction {
            render: false,
            timer: TimerCmd::Arm(state.backoff),
        },
    )
}

fn with_pace(state: FrameState, pace: Pace) -> FrameState {
    FrameState { pace, ..state }
}

fn doubled(backoff: Duration) -> Duration {
    (backoff * 2).clamp(WATCHDOG_MIN, WATCHDOG_MAX)
}

#[cfg(test)]
mod tests;

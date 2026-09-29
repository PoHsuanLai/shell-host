//! Per-document frame ticks: the compositor's frame rate, observable from a component.
//!
//! A surface's [`FrameTicks`] is shared between the host (which publishes a [`FrameTick`] on
//! every frame callback while the surface is animating or driven) and the surface's
//! `SurfaceHandle` (which hands out [`FrameDrive`]s and waits for ticks). Everything here runs
//! on the main thread, so the sharing is `Rc`.
//!
//! The rules:
//!
//! - A tick fires once per presented frame, at its frame callback (or at a watchdog expiry
//!   standing in for a callback that never came, cosmic-comp #2777), while the document has a
//!   CSS transition or animation running or at least one [`FrameDrive`] is held
//!   ([`pacing`]). A frame presented for any other reason (a one-off change) does not tick.
//! - Holding a drive makes the frame machine treat the surface as animating, so it renders a
//!   frame per callback even when nothing changed. That is the one way to get non-zero idle
//!   frames; a component holds a drive only while its own animation is converging and drops it
//!   when the motion settles or the hover that started it ends.
//! - `dt` is the time since the previous tick of the same run, and zero for the first tick
//!   after a frame that did not tick ([`tick_after`]), so a component never integrates over an
//!   idle gap.
//!
//! See design/10 §10.4; FINDINGS F31.

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use crate::frame::Animation;

/// A point on the host clock: the time since the host connected, the same timeline every
/// document's CSS is resolved on (`clock::HostClock`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct HostInstant(pub Duration);

impl HostInstant {
    /// The host's start.
    pub const ZERO: HostInstant = HostInstant(Duration::ZERO);
}

/// One frame of a surface that is animating or driven: when it was called back, and how long
/// since the previous tick of the same run (zero for the first).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FrameTick {
    pub at: HostInstant,
    pub dt: Duration,
}

/// Whether any [`FrameDrive`] of a surface is alive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Drive {
    Held,
    Released,
}

/// What the frame machine is told about a surface: animating when its document is, or when
/// a drive is held. Pure.
pub(crate) fn pacing(document: Animation, drive: Drive) -> Animation {
    match (document, drive) {
        (Animation::Running, _) | (_, Drive::Held) => Animation::Running,
        (Animation::Stopped, Drive::Released) => Animation::Stopped,
    }
}

/// The tick at `at`, after the previous tick of this run at `previous` (none: a new run).
/// Pure.
pub(crate) fn tick_after(previous: Option<HostInstant>, at: HostInstant) -> FrameTick {
    let dt = previous.map_or(Duration::ZERO, |before| at.0.saturating_sub(before.0));
    FrameTick { at, dt }
}

/// Whether a frame due now ticked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ticked {
    Ticked,
    Quiet,
}

#[derive(Debug, Default)]
struct TickState {
    drives: u32,
    /// The previous tick of the current run; `None` between runs.
    run: Option<HostInstant>,
    /// Ticks published so far, and the latest.
    count: u64,
    latest: FrameTick,
    waiting: Vec<Waker>,
}

/// One surface's frame ticks and drives. Cloned into the surface's `SurfaceHandle`.
#[derive(Debug, Clone, Default)]
pub(crate) struct FrameTicks(Rc<RefCell<TickState>>);

impl FrameTicks {
    /// Whether a drive is held.
    pub(crate) fn drive(&self) -> Drive {
        match self.0.borrow().drives {
            0 => Drive::Released,
            _ => Drive::Held,
        }
    }

    /// Hold a drive until the returned value is dropped. The host must also be told to render
    /// (an idle surface has no callback to tick on); `SurfaceHandle::drive_frames` does both.
    pub(crate) fn hold(&self) -> FrameDrive {
        self.0.borrow_mut().drives += 1;
        FrameDrive {
            ticks: self.clone(),
        }
    }

    /// A frame is due at `at` (its callback, or the watchdog standing in for it) on a surface
    /// whose document is in `document`: tick when [`pacing`] says running, else end the run.
    /// Returns what the frame machine should be told, and whether a tick fired.
    pub(crate) fn frame_due(&self, document: Animation, at: HostInstant) -> (Animation, Ticked) {
        let animation = pacing(document, self.drive());
        let mut state = self.0.borrow_mut();
        match animation {
            Animation::Running => {
                let tick = tick_after(state.run, at);
                state.run = Some(at);
                state.count += 1;
                state.latest = tick;
                state.waiting.drain(..).for_each(Waker::wake);
                (animation, Ticked::Ticked)
            }
            Animation::Stopped => {
                state.run = None;
                (animation, Ticked::Quiet)
            }
        }
    }

    /// Ticks published so far.
    pub(crate) fn count(&self) -> u64 {
        self.0.borrow().count
    }

    /// The first tick after the `seen`th: resolves with its number and the latest tick.
    pub(crate) fn after(&self, seen: u64) -> NextTick {
        NextTick {
            ticks: self.clone(),
            seen,
        }
    }
}

/// Keeps a surface rendering one frame per compositor frame callback while alive. Returned by
/// `SurfaceHandle::drive_frames`; dropping it stops the frame requests (the surface idles after
/// its next callback unless its document is still animating).
///
/// # Hold until settled
///
/// The pattern a component's own motion follows (`examples/demo_dock.rs`, `--mode=tick`):
///
/// 1. take a drive when the motion starts (the first pointer move, a leave that sends the
///    tiles home), unless one is held already;
/// 2. on every [`FrameTick`], step the motion by `dt` towards its target;
/// 3. drop the drive on the tick where the motion reaches its target.
///
/// Step 3 must be reached in bounded time **at any tick rate**. The rate is the compositor's:
/// 60 Hz on KWin, but nested cosmic-comp (winit, mailbox) calls back every 1-2 ms, a tenth or
/// less of the `dt` a 60 Hz test sees. A step that moves `gap * dt / tau` rounded to whole
/// units moves nothing once the gap is under `tau / (2 dt)` units (30 for `tau` = 60 ms at
/// 1 ms); with a settle tolerance below that, the motion stalls short of its target and the
/// drive, and a frame per callback, lives on until a long tick happens along.
/// Make each step move at least one unit, or integrate in floating point and settle within a
/// tolerance, and test the settle at a 1 ms `dt` as well as at 16.7 ms.
#[must_use = "dropping the drive stops the frame requests at once"]
#[derive(Debug)]
pub struct FrameDrive {
    ticks: FrameTicks,
}

impl Drop for FrameDrive {
    fn drop(&mut self) {
        let mut state = self.ticks.0.borrow_mut();
        state.drives = state.drives.saturating_sub(1);
    }
}

/// The future [`FrameTicks::after`] returns.
#[derive(Debug)]
pub(crate) struct NextTick {
    ticks: FrameTicks,
    seen: u64,
}

impl Future for NextTick {
    type Output = (u64, FrameTick);

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.ticks.0.borrow_mut();
        if state.count > self.seen {
            return Poll::Ready((state.count, state.latest));
        }
        state.waiting.push(cx.waker().clone());
        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> HostInstant {
        HostInstant(Duration::from_millis(n))
    }

    #[test]
    fn pacing_runs_while_animating_or_driven() {
        use Animation::{Running, Stopped};
        let cases = [
            (Stopped, Drive::Released, Stopped),
            (Stopped, Drive::Held, Running),
            (Running, Drive::Released, Running),
            (Running, Drive::Held, Running),
        ];
        for (document, drive, want) in cases {
            assert_eq!(pacing(document, drive), want, "{document:?} {drive:?}");
        }
    }

    #[test]
    fn dt_is_zero_for_the_first_tick_of_a_run() {
        let cases = [
            (None, ms(500), Duration::ZERO),
            (Some(ms(500)), ms(516), Duration::from_millis(16)),
            (Some(ms(516)), ms(516), Duration::ZERO),
            (Some(ms(600)), ms(516), Duration::ZERO),
        ];
        for (previous, at, dt) in cases {
            assert_eq!(
                tick_after(previous, at),
                FrameTick { at, dt },
                "{previous:?}"
            );
        }
    }

    #[test]
    fn drives_count_and_a_quiet_frame_ends_the_run() {
        let ticks = FrameTicks::default();
        assert_eq!(ticks.drive(), Drive::Released);
        let a = ticks.hold();
        let b = ticks.hold();
        drop(a);
        assert_eq!(ticks.drive(), Drive::Held);
        let due = |at| ticks.frame_due(Animation::Stopped, at);
        assert_eq!(due(ms(0)), (Animation::Running, Ticked::Ticked));
        assert_eq!(due(ms(16)), (Animation::Running, Ticked::Ticked));
        assert_eq!(ticks.0.borrow().latest.dt, Duration::from_millis(16));
        drop(b);
        assert_eq!(due(ms(33)), (Animation::Stopped, Ticked::Quiet));
        assert_eq!(ticks.count(), 2);
        // A new run starts with dt zero, even with the document animating instead.
        let run = ticks.frame_due(Animation::Running, ms(900));
        assert_eq!(run, (Animation::Running, Ticked::Ticked));
        assert_eq!(
            ticks.0.borrow().latest,
            FrameTick {
                at: ms(900),
                dt: Duration::ZERO
            }
        );
    }

    #[test]
    fn a_waiting_future_resolves_on_the_next_tick() {
        let ticks = FrameTicks::default();
        let _drive = ticks.hold();
        let mut next = std::pin::pin!(ticks.after(ticks.count()));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(next.as_mut().poll(&mut cx).is_pending());
        ticks.frame_due(Animation::Stopped, ms(7));
        let tick = FrameTick {
            at: ms(7),
            dt: Duration::ZERO,
        };
        assert_eq!(next.as_mut().poll(&mut cx), Poll::Ready((1, tick)));
    }
}

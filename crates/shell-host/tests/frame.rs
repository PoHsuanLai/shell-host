//! The frame machine against a simulated compositor: whole sequences, not single steps (the
//! step table is in `src/frame.rs`). They are the unit-level form of the spike's item (c)
//! acceptance.
//!
//! `Sim` plays host and compositor at once: time advances in milliseconds, the host flushes
//! after every event exactly as `HostCtx::flush` does (step `Flush`, render, feed `Presented`),
//! and the watchdog is a single deadline armed and cancelled by the machine's `TimerCmd`s.

use std::time::Duration;

use shell_host::frame::{
    Animation, FrameAction, FrameInput, FrameState, Pace, RenderCmd, TimerCmd, step,
};

/// A surface, the machine driving it, and what happened to it.
struct Sim {
    state: FrameState,
    now_ms: u64,
    /// When the armed watchdog fires, if armed.
    watchdog_at: Option<u64>,
    /// The frame in flight, if any, by the time it was presented.
    in_flight: Option<u64>,
    animation: Animation,
    frames: u32,
    callbacks: u32,
    arms: u32,
    fired: u32,
    /// Every wait the watchdog was armed with, in order.
    waits: Vec<u128>,
}

impl Sim {
    fn new(animation: Animation) -> Sim {
        Sim {
            state: FrameState::NEW,
            now_ms: 0,
            watchdog_at: None,
            in_flight: None,
            animation,
            frames: 0,
            callbacks: 0,
            arms: 0,
            fired: 0,
            waits: Vec::new(),
        }
    }

    fn feed(&mut self, input: FrameInput) -> FrameAction {
        let (next, action) = step(self.state, input);
        self.state = next;
        match action.timer {
            TimerCmd::Arm(wait) => {
                self.arms += 1;
                self.waits.push(wait.as_millis());
                self.watchdog_at = Some(self.now_ms + millis(wait));
            }
            TimerCmd::Cancel => self.watchdog_at = None,
            TimerCmd::Keep => {}
        }
        action
    }

    /// The host's flush point: render iff the machine says so, never over a frame in flight.
    fn flush(&mut self) {
        if self.feed(FrameInput::Flush).render == RenderCmd::Render {
            assert!(self.in_flight.is_none(), "rendered over a frame in flight at {}", self.now_ms);
            self.frames += 1;
            self.in_flight = Some(self.now_ms);
            self.feed(FrameInput::Presented(self.animation));
        }
    }

    fn invalidate(&mut self) {
        self.feed(FrameInput::Invalidate);
        self.flush();
    }

    /// The compositor's frame callback for the frame in flight.
    fn callback(&mut self) {
        if self.in_flight.take().is_some() {
            self.callbacks += 1;
            self.feed(FrameInput::Callback(self.animation));
            self.flush();
        }
    }

    /// Advance to `to_ms`, firing the watchdog on the way if it is due.
    fn advance(&mut self, to_ms: u64) {
        if let Some(at) = self.watchdog_at
            && at <= to_ms
        {
            self.now_ms = at;
            self.watchdog_at = None;
            self.fired += 1;
            // A watchdog retry abandons the frame it was waiting for.
            self.in_flight = None;
            self.feed(FrameInput::WatchdogFired);
            self.flush();
        }
        self.now_ms = to_ms;
    }
}

fn millis(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

/// Refresh periods at 60 Hz, in ms, rounded down: 16, 33, 50, ...
fn vsync(n: u64) -> u64 {
    n * 1000 / 60
}

/// A 1 s hover sweep at 60 Hz with the document animating throughout: frames presented are at
/// least 0.9 x callbacks received, and no frame is ever rendered while one is in flight.
#[test]
fn a_hover_sweep_renders_once_per_callback() {
    let mut sim = Sim::new(Animation::Running);
    // Pointer motion at 120 Hz (the acceptance script's rate), callbacks at 60 Hz.
    for tick in 0..120u64 {
        sim.advance(tick * 1000 / 120);
        sim.invalidate();
        if tick % 2 == 1 {
            sim.advance(vsync(tick / 2 + 1));
            sim.callback();
        }
    }
    assert!(sim.callbacks >= 59, "{} callbacks", sim.callbacks);
    assert!(
        f64::from(sim.frames) >= 0.9 * f64::from(sim.callbacks),
        "{} frames for {} callbacks",
        sim.frames,
        sim.callbacks
    );
    // KWin calls back every refresh, so the watchdog is armed but never fires.
    assert_eq!(sim.fired, 0);
}

/// After the last change and one settling callback, 5 s of nothing produces no render, no
/// timer arm and no commit (idle means zero wakeups).
#[test]
fn idle_after_a_change_is_silent() {
    let mut sim = Sim::new(Animation::Stopped);
    sim.invalidate();
    sim.advance(vsync(1));
    sim.callback();
    assert_eq!(sim.state.pace, Pace::Idle);
    let (frames, arms) = (sim.frames, sim.arms);
    // Five seconds of flushes with nothing changed (other sources waking the loop).
    for ms in (0..5000).step_by(100) {
        sim.advance(vsync(1) + ms);
        let action = sim.feed(FrameInput::Flush);
        assert_eq!(action, FrameAction::NONE, "at {ms} ms");
    }
    assert_eq!(sim.frames, frames, "no frame while idle");
    assert_eq!(sim.arms, arms, "no timer while idle");
    assert_eq!(sim.watchdog_at, None);
    assert_eq!(sim.state, FrameState::NEW);
}

/// A compositor that stops calling back while the document animates (cosmic-comp #2777): the
/// watchdog keeps the animation moving, backing off 50, 100, ... 1000 ms, and one real
/// callback resets the backoff to 50 ms.
#[test]
fn a_stalled_compositor_is_paced_by_the_watchdog_and_recovers() {
    let mut sim = Sim::new(Animation::Running);
    sim.invalidate();
    // No callbacks for 5 s: only the watchdog moves the animation.
    for ms in 1..=5000 {
        sim.advance(ms);
    }
    assert!(sim.fired >= 6, "{} watchdog expiries", sim.fired);
    assert_eq!(
        sim.waits[..7],
        [50, 100, 200, 400, 800, 1000, 1000],
        "backoff doubles to the cap"
    );
    assert_eq!(sim.frames, sim.fired + 1, "one retry per expiry");
    // The compositor comes back: the frame in flight is called back, the backoff resets.
    sim.callback();
    assert_eq!(sim.state.backoff, Duration::from_millis(50));
    let arms = sim.waits.len();
    sim.callback();
    assert_eq!(sim.waits[arms..], [50], "the next arm uses the reset backoff");
}

/// An occluded surface whose compositor keeps sending callbacks (KWin, SPIKE.md S1 item c):
/// the watchdog never fires.
#[test]
fn callbacks_while_occluded_never_trip_the_watchdog() {
    let mut sim = Sim::new(Animation::Running);
    sim.invalidate();
    for n in 1..=300 {
        // A callback 16-17 ms after every present, well inside the 50 ms watchdog.
        sim.advance(vsync(n));
        sim.callback();
    }
    assert_eq!(sim.fired, 0);
    assert_eq!(sim.callbacks, 300);
    assert_eq!(sim.frames, 301, "one frame per callback, plus the first");
    assert_eq!(sim.state.backoff, Duration::from_millis(50));
}

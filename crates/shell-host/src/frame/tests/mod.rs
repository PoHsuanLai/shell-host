mod sweeps;

use super::*;

const fn st(pace: Pace, timer_armed: bool, backoff_ms: u64) -> FrameState {
    FrameState {
        pace,
        timer_armed,
        backoff: Duration::from_millis(backoff_ms),
    }
}

const fn act(render: bool, timer: TimerCmd) -> FrameAction {
    FrameAction { render, timer }
}

const ARM_50: TimerCmd = TimerCmd::Arm(Duration::from_millis(50));
const ARM_400: TimerCmd = TimerCmd::Arm(Duration::from_millis(400));

use FrameInput as I;
use Pace::{Dirty, Idle, InFlight, InFlightDirty};
use TimerCmd::{Cancel, Keep};

const RENDER: bool = true;
const SKIP: bool = false;
const ARMED: bool = true;
const DISARMED: bool = false;

#[allow(clippy::type_complexity)]
#[rustfmt::skip]
const CASES: &[(&str, FrameState, FrameInput, FrameState, FrameAction)] = &[
    // Invalidate
    ("idle dirtied waits for flush", st(Idle, DISARMED, 50), I::Invalidate, st(Dirty, DISARMED, 50), act(SKIP, Keep)),
    ("dirty stays dirty", st(Dirty, DISARMED, 50), I::Invalidate, st(Dirty, DISARMED, 50), act(SKIP, Keep)),
    ("in flight dirtied arms the watchdog", st(InFlight, DISARMED, 50), I::Invalidate, st(InFlightDirty, ARMED, 50), act(SKIP, ARM_50)),
    ("in flight, animating, dirtied keeps its timer", st(InFlight, ARMED, 50), I::Invalidate, st(InFlightDirty, ARMED, 50), act(SKIP, Keep)),
    ("in flight dirty stays", st(InFlightDirty, ARMED, 50), I::Invalidate, st(InFlightDirty, ARMED, 50), act(SKIP, Keep)),
    // Flush
    ("flush renders a dirty surface", st(Dirty, DISARMED, 50), I::Flush, st(InFlight, DISARMED, 50), act(RENDER, Keep)),
    ("flush leaves idle alone", st(Idle, DISARMED, 50), I::Flush, st(Idle, DISARMED, 50), act(SKIP, Keep)),
    ("flush never renders over a frame in flight", st(InFlight, DISARMED, 50), I::Flush, st(InFlight, DISARMED, 50), act(SKIP, Keep)),
    ("flush waits out in-flight-dirty", st(InFlightDirty, ARMED, 50), I::Flush, st(InFlightDirty, ARMED, 50), act(SKIP, Keep)),
    // Presented
    ("static frame presented: no timer", st(InFlight, DISARMED, 50), I::Presented { animating: false }, st(InFlight, DISARMED, 50), act(SKIP, Keep)),
    ("animating frame presented arms", st(InFlight, DISARMED, 50), I::Presented { animating: true }, st(InFlight, ARMED, 50), act(SKIP, ARM_50)),
    ("animating uses the current backoff", st(InFlight, DISARMED, 400), I::Presented { animating: true }, st(InFlight, ARMED, 400), act(SKIP, ARM_400)),
    // PresentFailed
    ("failed present retries via the watchdog", st(InFlight, DISARMED, 50), I::PresentFailed, st(InFlightDirty, ARMED, 50), act(SKIP, ARM_50)),
    ("failed present on idle is ignored", st(Idle, DISARMED, 50), I::PresentFailed, st(Idle, DISARMED, 50), act(SKIP, Keep)),
    // Callback
    ("callback, still: idle and quiet", st(InFlight, DISARMED, 50), I::Callback { animating: false }, st(Idle, DISARMED, 50), act(SKIP, Keep)),
    ("callback, animating: render again", st(InFlight, ARMED, 50), I::Callback { animating: true }, st(Dirty, DISARMED, 50), act(SKIP, Cancel)),
    ("callback, dirtied: render again", st(InFlightDirty, ARMED, 50), I::Callback { animating: false }, st(Dirty, DISARMED, 50), act(SKIP, Cancel)),
    ("callback resets the backoff", st(InFlightDirty, ARMED, 800), I::Callback { animating: false }, st(Dirty, DISARMED, 50), act(SKIP, Cancel)),
    ("stale callback on idle", st(Idle, DISARMED, 50), I::Callback { animating: true }, st(Idle, DISARMED, 50), act(SKIP, Keep)),
    ("stale callback on dirty", st(Dirty, DISARMED, 200), I::Callback { animating: false }, st(Dirty, DISARMED, 200), act(SKIP, Keep)),
    // WatchdogFired
    ("watchdog unsticks and doubles", st(InFlightDirty, ARMED, 50), I::WatchdogFired, st(Dirty, DISARMED, 100), act(SKIP, Keep)),
    ("watchdog on animating in-flight", st(InFlight, ARMED, 200), I::WatchdogFired, st(Dirty, DISARMED, 400), act(SKIP, Keep)),
    ("backoff caps at 1000 ms", st(InFlightDirty, ARMED, 800), I::WatchdogFired, st(Dirty, DISARMED, 1000), act(SKIP, Keep)),
    ("backoff stays at the cap", st(InFlightDirty, ARMED, 1000), I::WatchdogFired, st(Dirty, DISARMED, 1000), act(SKIP, Keep)),
    ("late watchdog on idle", st(Idle, ARMED, 50), I::WatchdogFired, st(Idle, DISARMED, 100), act(SKIP, Keep)),
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
fn a_burst_of_changes_in_flight_coalesces_into_one_more_frame() {
    let (end, acts) = run(&[
        I::Invalidate, I::Flush, I::Presented { animating: false },
        I::Invalidate, I::Invalidate, I::Invalidate, I::Flush,
        I::Callback { animating: false }, I::Flush, I::Presented { animating: false }, I::Callback { animating: false },
    ]);
    assert_eq!(end.pace, Idle);
    assert_eq!(acts.iter().filter(|a| a.render).count(), 2);
}

/// A load that completes while its frame renders reaches the machine as an
/// `Invalidate` after `Presented` (the host drops only the render's own request). With a
/// compositor that never calls back, the watchdog paints it once, and then nothing more
/// happens: no timer is left and no frame follows.
#[test]
#[rustfmt::skip]
fn a_load_landing_mid_render_is_painted_without_any_callback_then_stays_quiet() {
    let (end, acts) = run(&[
        I::Invalidate, I::Flush, I::Presented { animating: false },
        I::Invalidate, I::Flush,
        I::WatchdogFired, I::Flush, I::Presented { animating: false },
        I::Flush, I::Flush,
    ]);
    assert_eq!(acts.iter().filter(|a| a.render).count(), 2);
    let timers: Vec<TimerCmd> = acts.iter().map(|a| a.timer).filter(|t| *t != Keep).collect();
    assert_eq!(timers, vec![ARM_50]);
    assert_eq!(end, st(InFlight, DISARMED, 100));
}

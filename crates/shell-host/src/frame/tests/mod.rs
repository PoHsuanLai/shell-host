mod sweeps;

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
fn a_burst_of_changes_in_flight_coalesces_into_one_more_frame() {
    let (end, acts) = run(&[
        I::Invalidate, I::Flush, I::Presented(Stopped),
        I::Invalidate, I::Invalidate, I::Invalidate, I::Flush,
        I::Callback(Stopped), I::Flush, I::Presented(Stopped), I::Callback(Stopped),
    ]);
    assert_eq!(end.pace, Idle);
    assert_eq!(acts.iter().filter(|a| a.render == Render).count(), 2);
}

/// A load that completes while its frame renders reaches the machine as an
/// `Invalidate` after `Presented` (the host drops only the render's own request). With a
/// compositor that never calls back, the watchdog paints it once, and then nothing more
/// happens: no timer is left and no frame follows.
#[test]
#[rustfmt::skip]
fn a_load_landing_mid_render_is_painted_without_any_callback_then_stays_quiet() {
    let (end, acts) = run(&[
        I::Invalidate, I::Flush, I::Presented(Stopped),
        I::Invalidate, I::Flush,
        I::WatchdogFired, I::Flush, I::Presented(Stopped),
        I::Flush, I::Flush,
    ]);
    assert_eq!(acts.iter().filter(|a| a.render == Render).count(), 2);
    let timers: Vec<TimerCmd> = acts.iter().map(|a| a.timer).filter(|t| *t != Keep).collect();
    assert_eq!(timers, vec![ARM_50]);
    assert_eq!(end, st(InFlight, Disarmed, 100));
}

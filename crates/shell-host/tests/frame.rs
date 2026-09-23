//! The frame machine against a simulated compositor: whole sequences, not single steps (the
//! step table is in `src/frame.rs`). H2 core fills these; they are the unit-level form of the
//! spike's item (c) acceptance.

/// A 1 s hover sweep at 60 Hz with the document animating throughout: frames presented are at
/// least 0.9 x callbacks received, and no frame is ever rendered while one is in flight.
#[test]
#[ignore = "H2 fill"]
fn a_hover_sweep_renders_once_per_callback() {
    todo!("drive frame::step with Invalidate/Flush/Presented(Running)/Callback at 16.7 ms")
}

/// After the last change and one settling callback, 5 s of nothing produces no render, no
/// timer arm and no commit (idle means zero wakeups).
#[test]
#[ignore = "H2 fill"]
fn idle_after_a_change_is_silent() {
    todo!("assert every action after settling is FrameAction::NONE")
}

/// A compositor that stops calling back while the document animates (cosmic-comp #2777): the
/// watchdog keeps the animation moving, backing off 50, 100, ... 1000 ms, and one real
/// callback resets the backoff to 50 ms.
#[test]
#[ignore = "H2 fill"]
fn a_stalled_compositor_is_paced_by_the_watchdog_and_recovers() {
    todo!("simulate a callback-less stretch, then one callback")
}

/// An occluded surface whose compositor keeps sending callbacks (KWin, SPIKE.md S1 item c):
/// the watchdog never fires.
#[test]
#[ignore = "H2 fill"]
fn callbacks_while_occluded_never_trip_the_watchdog() {
    todo!("interleave callbacks within 50 ms of every present")
}

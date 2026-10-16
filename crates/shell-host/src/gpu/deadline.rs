//! Running work that a hung driver can leave unfinished: on its own thread, with a deadline.

use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

/// How a piece of work ended.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Finished<T> {
    /// It returned this.
    In(T),
    /// It was still running at the deadline. Its thread is left to end by itself or never.
    TimedOut,
    /// It panicked, or its thread could not be started.
    Failed,
}

/// Run `work` on a new thread and wait at most `deadline` for its result.
pub(crate) fn run_within<T: Send + 'static>(
    deadline: Duration,
    work: impl FnOnce() -> T + Send + 'static,
) -> Finished<T> {
    let (tx, rx) = mpsc::channel();
    let started = thread::Builder::new()
        .name("gpu-deadline".into())
        .spawn(move || {
            // The receiver is gone once the deadline passed; nobody is left to tell.
            let _ = tx.send(work());
        });
    if started.is_err() {
        return Finished::Failed;
    }
    match rx.recv_timeout(deadline) {
        Ok(value) => Finished::In(value),
        Err(RecvTimeoutError::Timeout) => Finished::TimedOut,
        Err(RecvTimeoutError::Disconnected) => Finished::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_that_finishes_in_time_returns_its_value() {
        assert_eq!(run_within(Duration::from_secs(5), || 7), Finished::In(7));
    }

    #[test]
    fn work_that_hangs_is_given_up_on_at_the_deadline() {
        let started = std::time::Instant::now();
        let finished = run_within(Duration::from_millis(50), || {
            thread::sleep(Duration::from_millis(600));
        });
        assert_eq!(finished, Finished::TimedOut);
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "the caller did not wait for the work"
        );
    }

    #[test]
    fn work_that_panics_is_reported_not_propagated() {
        let finished = run_within(Duration::from_secs(5), || -> u8 { panic!("a driver bug") });
        assert_eq!(finished, Finished::Failed);
    }
}

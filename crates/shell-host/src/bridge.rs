//! `use_watch`: shared state across surfaces. Each surface is its own `VirtualDom`, so a Signal
//! cannot cross surfaces; shared data travels on a `tokio::sync::watch` channel and each root
//! reads it as a signal of its own.

use dioxus::core::{spawn, use_hook};
use dioxus::hooks::use_signal;
use dioxus::signals::{ReadSignal, ReadableExt, WritableExt};
use tokio::sync::watch;

/// A signal holding the latest value of `rx`, updated (and the component re-rendered) whenever
/// the sender changes it to a different value. The host holds `rt.enter()` on the main thread,
/// so the task that awaits `rx.changed()` runs on the surface's own Dioxus scheduler.
pub fn use_watch<T: Clone + PartialEq + 'static>(rx: watch::Receiver<T>) -> ReadSignal<T> {
    let mut initial = rx.clone();
    let mut signal = use_signal(move || initial.borrow_and_update().clone());
    use_hook(move || {
        let mut rx = rx;
        spawn(async move {
            while rx.changed().await.is_ok() {
                let value = rx.borrow_and_update().clone();
                if *signal.peek() != value {
                    signal.set(value);
                }
            }
        });
    });
    ReadSignal::from(signal)
}

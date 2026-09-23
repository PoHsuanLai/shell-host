//! `use_watch`: shared state across surfaces. Each surface is its own `VirtualDom`, so a Signal
//! cannot cross surfaces; shared data travels on a `tokio::sync::watch` channel and each root
//! reads it as a signal of its own.
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use dioxus::signals::ReadSignal;
use tokio::sync::watch;

/// A signal holding the latest value of `rx`, updated (and the component re-rendered) whenever
/// the sender changes it to a different value. The host holds `rt.enter()` on the main thread,
/// so the task that awaits `rx.changed()` runs on the surface's own Dioxus scheduler.
pub fn use_watch<T: Clone + PartialEq + 'static>(rx: watch::Receiver<T>) -> ReadSignal<T> {
    todo!()
}

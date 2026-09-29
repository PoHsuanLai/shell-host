//! `LockHandle`: what a lock surface's document holds (`use_lock`). It says which lock the
//! document belongs to and where that lock is (`LockPhase`, live), and it asks for the unlock
//! once the shell has authenticated the user. The host tells every lock document `Unlocking`
//! and polls them before it sends `unlock_and_destroy`.
//!
//! See FINDINGS F65.

use dioxus::core::{spawn, use_hook};
use dioxus::hooks::use_signal;
use dioxus::signals::{ReadSignal, WritableExt};

use crate::handle::RequestQueue;
use crate::live::Live;
use crate::lock::LockId;
use crate::lock::machine::LockPhase;
use crate::msg::Request;

/// One session lock, as seen from inside a lock surface's document.
#[derive(Debug, Clone)]
pub struct LockHandle {
    id: LockId,
    requests: RequestQueue,
    phase: Live<LockPhase>,
}

impl LockHandle {
    pub(crate) fn new(id: LockId, requests: RequestQueue, phase: Live<LockPhase>) -> LockHandle {
        LockHandle {
            id,
            requests,
            phase,
        }
    }

    /// The lock this document belongs to.
    pub fn id(&self) -> LockId {
        self.id
    }

    /// Where the lock is right now, without subscribing: for an event handler or a task.
    pub fn phase(&self) -> LockPhase {
        self.phase.get()
    }

    /// Unlock the session: call it after the shell has authenticated the user. The host tells
    /// every lock document `Unlocking`, polls them, then sends `unlock_and_destroy` and removes
    /// the lock surfaces; `HostEvent::Lock { state: Unlocked }` follows. Before `locked` it
    /// cancels the request instead. Repeated calls do nothing more.
    pub fn unlock(&self) {
        self.requests.push(Request::Unlock(self.id));
    }
}

/// The lock handle of the lock surface this component renders into. Panics outside a lock
/// surface's document, like `use_context`.
pub fn use_lock() -> LockHandle {
    dioxus::prelude::use_context::<LockHandle>()
}

/// The lock's phase as a signal: re-renders the component when it changes (`Locked` once the
/// compositor has locked, `Unlocking` just before the surfaces go).
pub fn use_lock_phase() -> ReadSignal<LockPhase> {
    let live = use_lock().phase;
    let mut latest = use_signal(|| live.get());
    use_hook(move || {
        // Read before the task first runs, as `use_surface_scale` does, so a change landing
        // between this render and the task's first poll is not missed.
        let mut seen = live.changes();
        spawn(async move {
            loop {
                let (count, phase) = live.changed_after(seen).await;
                seen = count;
                latest.set(phase);
            }
        });
    });
    ReadSignal::from(latest)
}

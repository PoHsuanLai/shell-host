//! Timers the Wayland handlers need but cannot arm themselves. The handlers run on the
//! non-generic `HostState`; the calloop loop and its timers belong to the generic `HostCtx<M>`.
//! So a handler records what it wants here, and the context arms or cancels it after the
//! dispatch that produced it (effects as data, CONVENTIONS §0).

use std::time::Duration;

use crate::frame::TimerCmd;
use crate::id::SurfaceId;

/// A timer change a handler asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerRequest {
    /// The frame machine's watchdog for one surface. Firing feeds
    /// `FrameInput::WatchdogFired` back to that surface.
    Watchdog { id: SurfaceId, cmd: TimerCmd },
    /// Start repeating the held key: first after `delay`, then every `interval`
    /// (`wl_keyboard.repeat_info`). Replaces any repeat in progress.
    RepeatStart { delay: Duration, interval: Duration },
    /// Stop repeating (key released, focus lost, or a new key pressed).
    RepeatStop,
}

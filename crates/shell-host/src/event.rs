//! What the host tells the shell.

use crate::id::{OutputId, SurfaceId};
use crate::lock::LockId;
use crate::lock::machine::LockState;
use crate::output::OutputInfo;

/// What palmrest asks of the shell, and its workspace steps.
pub use palmrest_protocol::{HostAction as PalmrestAction, Workspace as PalmrestWorkspace};

/// An event delivered to the closure given to `Host::run`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostEvent<M> {
    /// The loop is running and the initial outputs have been reported; spawn the first surfaces
    /// here. Delivered exactly once, first.
    Started,
    OutputAdded(OutputInfo),
    OutputChanged(OutputInfo),
    OutputRemoved(OutputId),
    /// A surface is gone; its id no longer matches anything.
    SurfaceClosed {
        id: SurfaceId,
        reason: CloseReason,
    },
    /// The surface's first frame is on screen (also after a hidden surface is shown again, with
    /// a new `wl_surface`). From here `SurfaceHandle::wl_surface` and `HostCtx::wl_surface`
    /// name the object the compositor shows, for a `quire_*` object the shell creates on it.
    SurfaceMapped {
        id: SurfaceId,
    },
    /// Keyboard focus entered or left a surface.
    KeyboardFocus {
        id: SurfaceId,
        focus: KeyboardFocus,
    },
    /// A session lock from `HostCtx::lock_session` changed state: `Locked` once the compositor
    /// locked, `Unlocked` after the unlock, `Finished` when the compositor refused the lock
    /// (another locker holds it) or ended it itself.
    Lock {
        id: LockId,
        state: LockState,
    },
    /// The person asked the whole app to quit (the Dock's Quit, Command-Q from the shell,
    /// logging out): the app's should-terminate path. Save or ask ("Save changes?"), then
    /// `HostCtx::exit`, or stay open by doing nothing. There is no reply to send; the compositor
    /// takes the app for running again if it is still there after a short grace. Raised only on a
    /// compositor that offers it (casement, from `quire_window_manager_v1` version 2); elsewhere
    /// the windows are closed one by one and `SurfaceClosed` is all the app sees.
    QuitAsked,
    /// palmrest asks for an action that needs the person's session, and the host carries none of it
    /// out: the shell does, as it does its own commands (the workspace steps, a line of its own
    /// command language, a program of the person's). The daemon is a system service with no session
    /// of its own. The words are within [`PalmrestAction::fits`]'s bounds.
    Palmrest(PalmrestAction),
    /// A message sent through a `HostHandle`.
    App(M),
}

/// Why a surface closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CloseReason {
    /// `HostCtx::close`, `SurfaceHandle::close` or a `HostHandle` close.
    Requested,
    /// The compositor dismissed a popup (`xdg_popup.popup_done`, an outside click on a grab).
    PopupDismissed,
    /// The popup's parent closed, and popups close with their parent.
    ParentClosed,
    /// The compositor closed a layer surface (`zwlr_layer_surface_v1.closed`), for example
    /// because its output went away.
    Compositor,
    /// The user asked a toplevel to close (`xdg_toplevel.close`). The host closes it; a shell
    /// that wants to confirm first reopens.
    UserClose,
    /// The surface failed (its GPU surface could not be created or presented) and was removed.
    Failed,
}

/// Keyboard focus, gained or lost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyboardFocus {
    Gained,
    Lost,
}

/// An event raised inside the Wayland handlers, which never carry an app message. Widened to
/// the caller's `HostEvent<M>` with [`widen`] when it is delivered.
pub(crate) type ShellEvent = HostEvent<std::convert::Infallible>;

/// `event` as a `HostEvent<M>`. Total: a `ShellEvent` cannot be `App`.
pub(crate) fn widen<M>(event: ShellEvent) -> HostEvent<M> {
    match event {
        HostEvent::Started => HostEvent::Started,
        HostEvent::OutputAdded(info) => HostEvent::OutputAdded(info),
        HostEvent::OutputChanged(info) => HostEvent::OutputChanged(info),
        HostEvent::OutputRemoved(id) => HostEvent::OutputRemoved(id),
        HostEvent::SurfaceClosed { id, reason } => HostEvent::SurfaceClosed { id, reason },
        HostEvent::SurfaceMapped { id } => HostEvent::SurfaceMapped { id },
        HostEvent::KeyboardFocus { id, focus } => HostEvent::KeyboardFocus { id, focus },
        HostEvent::Lock { id, state } => HostEvent::Lock { id, state },
        HostEvent::QuitAsked => HostEvent::QuitAsked,
        HostEvent::Palmrest(ask) => HostEvent::Palmrest(ask),
        HostEvent::App(never) => match never {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widening_keeps_every_shell_event() {
        let cases: Vec<(ShellEvent, HostEvent<String>)> = vec![
            (HostEvent::Started, HostEvent::Started),
            (HostEvent::QuitAsked, HostEvent::QuitAsked),
            (
                HostEvent::Palmrest(PalmrestAction::Workspace(PalmrestWorkspace::Next)),
                HostEvent::Palmrest(PalmrestAction::Workspace(PalmrestWorkspace::Next)),
            ),
            (
                HostEvent::OutputRemoved(OutputId(3)),
                HostEvent::OutputRemoved(OutputId(3)),
            ),
            (
                HostEvent::SurfaceClosed {
                    id: SurfaceId(2),
                    reason: CloseReason::PopupDismissed,
                },
                HostEvent::SurfaceClosed {
                    id: SurfaceId(2),
                    reason: CloseReason::PopupDismissed,
                },
            ),
            (
                HostEvent::SurfaceMapped { id: SurfaceId(5) },
                HostEvent::SurfaceMapped { id: SurfaceId(5) },
            ),
            (
                HostEvent::KeyboardFocus {
                    id: SurfaceId(1),
                    focus: KeyboardFocus::Gained,
                },
                HostEvent::KeyboardFocus {
                    id: SurfaceId(1),
                    focus: KeyboardFocus::Gained,
                },
            ),
            (
                HostEvent::Lock {
                    id: LockId(4),
                    state: LockState::Finished,
                },
                HostEvent::Lock {
                    id: LockId(4),
                    state: LockState::Finished,
                },
            ),
        ];
        for (shell, want) in cases {
            assert_eq!(widen::<String>(shell.clone()), want, "{shell:?}");
        }
    }
}

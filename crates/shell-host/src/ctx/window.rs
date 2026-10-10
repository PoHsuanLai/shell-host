//! The window-management ops of a toplevel: interactive move and resize under the serial rule
//! (`grab`), maximize, minimize and fullscreen.
//!
//! See FINDINGS F46, F49.

use smithay_client_toolkit::shell::xdg::window::Window;

use crate::grab::GrabRefused;
use crate::id::SurfaceId;
use crate::msg::SurfaceOp;
use crate::state::HostState;
use crate::toplevel::{Fullscreen, Maximize, ResizeEdge, to_wire_edge};

/// The xdg_toplevel request for `op` on toplevel `id`'s `window`; a layer or popup op is
/// ignored. A move or resize the compositor would ignore is refused here with a log line (a
/// `SurfaceHandle` caller already got the refusal as a value).
pub(super) fn send(state: &HostState, id: SurfaceId, window: &Window, op: &SurfaceOp) {
    let started = match op {
        SurfaceOp::BeginMove => begin_move(state, id, window),
        SurfaceOp::BeginResize(edge) => begin_resize(state, id, window, *edge),
        SurfaceOp::SetMaximized(Maximize::Set) => {
            window.set_maximized();
            Ok(())
        }
        SurfaceOp::SetMaximized(Maximize::Unset) => {
            window.unset_maximized();
            Ok(())
        }
        SurfaceOp::Minimize => {
            window.set_minimized();
            Ok(())
        }
        SurfaceOp::SetFullscreen(Fullscreen::Set) => {
            window.set_fullscreen(None);
            Ok(())
        }
        SurfaceOp::SetFullscreen(Fullscreen::Unset) => {
            window.unset_fullscreen();
            Ok(())
        }
        SurfaceOp::SetBlur(_)
        | SurfaceOp::SetInputRegion(_)
        | SurfaceOp::SetKeyboard(_)
        | SurfaceOp::SetExclusiveZone(_)
        | SurfaceOp::SetSize(_)
        | SurfaceOp::SetMargin(_)
        | SurfaceOp::SetAnchor(_)
        | SurfaceOp::SetLayer(_)
        | SurfaceOp::SetVisibility(_)
        | SurfaceOp::Reposition(_)
        // Needs `&mut HostState` for its protocol object: `ops::apply` takes it first.
        | SurfaceOp::RequestPlacement(_) => Ok(()),
    };
    if let Err(refused) = started {
        eprintln!("shell-host: {op:?} on {id:?} refused: {refused}");
    }
}

/// `xdg_toplevel.move` with the held press's serial.
pub(crate) fn begin_move(
    state: &HostState,
    id: SurfaceId,
    window: &Window,
) -> Result<(), GrabRefused> {
    let serial = state.input.grab.serial_for(id)?;
    let seat = state.input.seat.as_ref().ok_or(GrabRefused::NoSeat)?;
    window.move_(seat, serial);
    // The compositor owns the button from here; its release never reaches us.
    state.input.grab.update(|facts| facts.press = None);
    Ok(())
}

/// `xdg_toplevel.resize` from `edge` with the held press's serial.
pub(crate) fn begin_resize(
    state: &HostState,
    id: SurfaceId,
    window: &Window,
    edge: ResizeEdge,
) -> Result<(), GrabRefused> {
    let serial = state.input.grab.serial_for(id)?;
    let seat = state.input.seat.as_ref().ok_or(GrabRefused::NoSeat)?;
    window.resize(seat, serial, to_wire_edge(edge));
    state.input.grab.update(|facts| facts.press = None);
    Ok(())
}

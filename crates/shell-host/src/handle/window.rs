//! Toplevel-only requests: move, resize, maximize, minimize, fullscreen.

use super::SurfaceHandle;
use crate::grab::GrabRefused;
use crate::msg::SurfaceOp;
use crate::role::RoleKind;
use crate::toplevel::{Fullscreen, Maximize, ResizeEdge, ToplevelState};

impl SurfaceHandle {
    /// Start moving this toplevel with the pointer (`xdg_toplevel.move`): call it from the
    /// press handler of a titlebar's drag region, while the button is still down. The
    /// compositor moves the window until the button is released; the document sees no motion
    /// meanwhile. Refused, and nothing sent, when the compositor would ignore it (`grab`'s
    /// serial rule): no button held, the press was on another surface, the pointer is not on
    /// this one, or this surface is not a toplevel.
    pub fn begin_move(&self) -> Result<(), GrabRefused> {
        self.grab_check()?;
        self.apply(SurfaceOp::BeginMove);
        Ok(())
    }

    /// Start resizing this toplevel from `edge` (`xdg_toplevel.resize`), under the same rule as
    /// [`SurfaceHandle::begin_move`]. A `Decorations::Client` window's resize band already does
    /// this in the host; this is for a frame that draws its own resize affordance.
    pub fn begin_resize(&self, edge: ResizeEdge) -> Result<(), GrabRefused> {
        self.grab_check()?;
        self.apply(SurfaceOp::BeginResize(edge));
        Ok(())
    }

    /// Ask the compositor to maximize (zoom) this toplevel or restore it. Nothing changes until
    /// the configure that answers it, which [`use_toplevel_state`] publishes; a compositor may
    /// refuse (a fixed-size window) and send nothing.
    pub fn set_maximized(&self, maximize: Maximize) {
        self.apply(SurfaceOp::SetMaximized(maximize));
    }

    /// Ask the compositor to minimize this toplevel. Restoring is the compositor's (the dock,
    /// the task switcher); a client has no request for it.
    pub fn set_minimized(&self) {
        self.apply(SurfaceOp::Minimize);
    }

    /// Ask for fullscreen on the compositor's choice of output, or leave it.
    pub fn set_fullscreen(&self, fullscreen: Fullscreen) {
        self.apply(SurfaceOp::SetFullscreen(fullscreen));
    }

    /// This toplevel's state as of its last configure, without subscribing: for an event
    /// handler (the zoom button toggles on `maximized`). A component that draws the state reads
    /// [`use_toplevel_state`]. The default for any other surface.
    pub fn current_toplevel_state(&self) -> ToplevelState {
        self.toplevel.get()
    }

    fn grab_check(&self) -> Result<(), GrabRefused> {
        if self.kind != RoleKind::Toplevel {
            return Err(GrabRefused::NotAToplevel);
        }
        self.grab.serial_for(self.id).map(drop)
    }
}

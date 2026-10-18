//! `SurfaceHandle`: what a component uses to change its own surface. Provided as a root
//! context of every surface's `VirtualDom`; read it with [`use_surface`].
//!
//! See FINDINGS F31, F43, F46, F60.

mod hooks;
mod queue;
mod surface;
mod window;
mod wl;

pub use hooks::{
    use_frame_tick, use_input_modality, use_modifiers, use_surface, use_surface_scale,
    use_toplevel_state, use_wheel,
};
pub(crate) use queue::RequestQueue;
pub(crate) use wl::WlCell;

use crate::grab::GrabRecord;
use crate::id::{SurfaceId, SurfaceIds};
use crate::live::Live;
use crate::modality::InputModality;
use crate::modifiers::Modifiers;
use crate::msg::{Request, SurfaceOp};
use crate::role::RoleKind;
use crate::scale::{LiveScale, Scale120};
use crate::scroll::watch::ScrollBoard;
use crate::scroll::wheel_feed::WheelFeed;
use crate::tick::FrameTicks;
use crate::toplevel::ToplevelState;
use crate::window_placement::PlacementSupport;
use wayland_client::protocol::wl_surface::WlSurface;

/// One surface, as seen from inside its document.
#[derive(Debug, Clone)]
pub struct SurfaceHandle {
    id: SurfaceId,
    kind: RoleKind,
    requests: RequestQueue,
    ids: SurfaceIds,
    ticks: FrameTicks,
    scale: LiveScale,
    toplevel: Live<ToplevelState>,
    grab: GrabRecord,
    scrolls: ScrollBoard,
    wheels: WheelFeed,
    modifiers: Live<Modifiers>,
    modality: Live<InputModality>,
    wl: WlCell,
    placement: PlacementSupport,
}

/// The host's shared state a `SurfaceHandle` reads, besides its queue: built by the host when
/// it creates the surface.
#[derive(Debug, Clone)]
pub(crate) struct HandleParts {
    pub(crate) requests: RequestQueue,
    pub(crate) ids: SurfaceIds,
    pub(crate) ticks: FrameTicks,
    pub(crate) scale: LiveScale,
    pub(crate) toplevel: Live<ToplevelState>,
    pub(crate) grab: GrabRecord,
    pub(crate) scrolls: ScrollBoard,
    pub(crate) wheels: WheelFeed,
    pub(crate) modifiers: Live<Modifiers>,
    /// The seat's, shared by every surface (not a per-surface copy like `modifiers`).
    pub(crate) modality: Live<InputModality>,
    /// This surface's `wl_surface`, refreshed when the host rebuilds the role.
    pub(crate) wl: WlCell,
    /// Whether the compositor takes placement requests (the same for every surface).
    pub(crate) placement: PlacementSupport,
}

impl SurfaceHandle {
    /// The handle for surface `id` of role `kind`.
    pub(crate) fn new(id: SurfaceId, kind: RoleKind, parts: HandleParts) -> SurfaceHandle {
        let HandleParts {
            requests,
            ids,
            ticks,
            scale,
            toplevel,
            grab,
            scrolls,
            wheels,
            modifiers,
            modality,
            wl,
            placement,
        } = parts;
        SurfaceHandle {
            id,
            kind,
            requests,
            ids,
            ticks,
            scale,
            toplevel,
            grab,
            scrolls,
            wheels,
            modifiers,
            modality,
            wl,
            placement,
        }
    }

    /// This surface's id.
    pub fn id(&self) -> SurfaceId {
        self.id
    }

    /// This surface's scale right now, without subscribing to changes: for an event handler
    /// or a task. A component that draws for the scale reads [`use_surface_scale`] instead.
    pub fn current_scale(&self) -> Scale120 {
        self.scale.get()
    }

    /// This surface's `wl_surface`, for a protocol the host does not wrap (a `quire_*` object
    /// the app's own code creates on the shell's connection). It is replaced when the surface
    /// is hidden and shown again, so read it again after `HostEvent::SurfaceMapped`. `None` for
    /// a handle that has no surface behind it (one built without a connection).
    pub fn wl_surface(&self) -> Option<WlSurface> {
        self.wl.get()
    }

    /// Any op, as `HostCtx::apply` would apply it.
    pub fn apply(&self, op: impl Into<SurfaceOp>) {
        self.requests.push(Request::Apply {
            id: self.id,
            op: op.into(),
        });
    }

    /// The seat's input modality right now, without subscribing: for an event handler. A
    /// component that draws from it reads [`use_input_modality`].
    pub fn current_input_modality(&self) -> InputModality {
        self.modality.get()
    }

    /// The scrollers this surface's components watch (`use_scroll`).
    pub(crate) fn scroll_board(&self) -> ScrollBoard {
        self.scrolls.clone()
    }
}

#[cfg(test)]
mod tests;

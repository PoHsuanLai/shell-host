//! `SurfaceHandle`: what a component uses to change its own surface. Provided as a root
//! context of every surface's `VirtualDom`; read it with [`use_surface`].
//!
//! See FINDINGS F31, F43, F46, F60.

mod hooks;
mod queue;
mod surface;
mod window;

pub use hooks::{
    use_frame_tick, use_input_modality, use_modifiers, use_surface, use_surface_scale,
    use_toplevel_state,
};
pub(crate) use queue::RequestQueue;

use crate::grab::GrabRecord;
use crate::id::{SurfaceId, SurfaceIds};
use crate::live::Live;
use crate::live_scale::LiveScale;
use crate::modality::InputModality;
use crate::modifiers::Modifiers;
use crate::msg::{Request, SurfaceOp};
use crate::role::RoleKind;
use crate::scale::Scale;
use crate::scroll::watch::ScrollBoard;
use crate::tick::FrameTicks;
use crate::toplevel::ToplevelState;

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
    modifiers: Live<Modifiers>,
    modality: Live<InputModality>,
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
    pub(crate) modifiers: Live<Modifiers>,
    /// The seat's, shared by every surface (not a per-surface copy like `modifiers`).
    pub(crate) modality: Live<InputModality>,
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
            modifiers,
            modality,
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
            modifiers,
            modality,
        }
    }

    /// This surface's id.
    pub fn id(&self) -> SurfaceId {
        self.id
    }

    /// This surface's scale right now, without subscribing to changes: for an event handler
    /// or a task. A component that draws for the scale reads [`use_surface_scale`] instead.
    pub fn current_scale(&self) -> Scale {
        self.scale.get()
    }

    /// Any other op, as `HostCtx::apply` would apply it.
    pub fn apply(&self, op: SurfaceOp) {
        self.requests.push(Request::Apply { id: self.id, op });
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

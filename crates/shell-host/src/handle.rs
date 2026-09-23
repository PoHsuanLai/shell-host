//! `SurfaceHandle`: what a component uses to change its own surface. Provided as a root
//! context of every surface's `VirtualDom`; read it with [`use_surface`].
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use calloop::ping::Ping;

use crate::blur::BlurSpec;
use crate::id::{SurfaceId, SurfaceIds};
use crate::layer::{ExclusiveZone, KeyboardMode, LayerSize};
use crate::msg::{Request, SurfaceOp};
use crate::placement::PopupPlacement;
use crate::popup::PopupConfig;
use crate::region::RegionSpec;
use crate::root::Root;

/// The main-thread queue a `SurfaceHandle` pushes into, and the ping that wakes the loop to
/// drain it. Ops queue while a document is borrowed and apply after `poll()`, before paint.
#[derive(Debug, Clone)]
pub struct RequestQueue {
    queue: Rc<RefCell<VecDeque<Request>>>,
    wake: Ping,
}

impl RequestQueue {
    /// A queue that pings `wake` on every push.
    pub fn new(wake: Ping) -> RequestQueue {
        todo!()
    }

    /// Queue `request` and wake the loop.
    pub fn push(&self, request: Request) {
        todo!()
    }

    /// Everything queued so far, oldest first.
    pub fn drain(&self) -> Vec<Request> {
        todo!()
    }
}

/// One surface, as seen from inside its document.
#[derive(Debug, Clone)]
pub struct SurfaceHandle {
    id: SurfaceId,
    requests: RequestQueue,
    ids: SurfaceIds,
}

impl SurfaceHandle {
    /// The handle for surface `id`. Built by the host when it creates the surface.
    pub(crate) fn new(id: SurfaceId, requests: RequestQueue, ids: SurfaceIds) -> SurfaceHandle {
        todo!()
    }

    /// This surface's id.
    pub fn id(&self) -> SurfaceId {
        todo!()
    }

    pub fn set_blur(&self, blur: BlurSpec) {
        todo!()
    }

    pub fn set_input_region(&self, region: RegionSpec) {
        todo!()
    }

    /// Layer surfaces only.
    pub fn set_keyboard(&self, mode: KeyboardMode) {
        todo!()
    }

    /// Layer surfaces only.
    pub fn set_exclusive_zone(&self, zone: ExclusiveZone) {
        todo!()
    }

    /// Layer surfaces only.
    pub fn set_size(&self, size: LayerSize) {
        todo!()
    }

    /// Any other op, as `HostCtx::apply` would apply it.
    pub fn apply(&self, op: SurfaceOp) {
        todo!()
    }

    /// Open a popup whose parent is this surface, rendering `root` as its own `VirtualDom`.
    /// Returns the popup's id at once; the popup appears after the next flush. Opening a
    /// grabbing popup from a `KeyboardMode::None` layer switches the layer to `OnDemand` until
    /// the popup closes.
    pub fn open_popup(
        &self,
        config: PopupConfig,
        placement: PopupPlacement,
        root: Root,
    ) -> SurfaceId {
        todo!()
    }

    /// Close this surface (and, for a parent, its popups).
    pub fn close(&self) {
        todo!()
    }

    /// Put `text` on the clipboard, owned by this surface's client.
    pub fn copy_text(&self, text: String) {
        todo!()
    }

    /// An xdg-activation token for launching `app_id` with focus handed over; `None` when the
    /// compositor has no xdg-activation or refused.
    pub async fn activation_token(&self, app_id: Option<String>) -> Option<String> {
        todo!()
    }
}

/// The handle of the surface this component renders in. Panics outside a host surface (a
/// caller contract, like any missing Dioxus context).
pub fn use_surface() -> SurfaceHandle {
    todo!()
}

//! `SurfaceHandle`: what a component uses to change its own surface. Provided as a root
//! context of every surface's `VirtualDom`; read it with [`use_surface`].

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use calloop::ping::Ping;
use tokio::sync::oneshot;

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
        RequestQueue {
            queue: Rc::default(),
            wake,
        }
    }

    /// Queue `request` and wake the loop.
    pub fn push(&self, request: Request) {
        self.queue.borrow_mut().push_back(request);
        self.wake.ping();
    }

    /// Everything queued so far, oldest first.
    pub fn drain(&self) -> Vec<Request> {
        self.queue.borrow_mut().drain(..).collect()
    }

    /// The ping this queue wakes the loop with, shared by every waker and shell provider.
    pub(crate) fn ping(&self) -> Ping {
        self.wake.clone()
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
        SurfaceHandle { id, requests, ids }
    }

    /// This surface's id.
    pub fn id(&self) -> SurfaceId {
        self.id
    }

    /// Blur behind this surface (`SurfaceOp::SetBlur`).
    pub fn set_blur(&self, blur: BlurSpec) {
        self.apply(SurfaceOp::SetBlur(blur));
    }

    /// Where this surface takes pointer input (`SurfaceOp::SetInputRegion`).
    pub fn set_input_region(&self, region: RegionSpec) {
        self.apply(SurfaceOp::SetInputRegion(region));
    }

    /// Layer surfaces only.
    pub fn set_keyboard(&self, mode: KeyboardMode) {
        self.apply(SurfaceOp::SetKeyboard(mode));
    }

    /// Layer surfaces only.
    pub fn set_exclusive_zone(&self, zone: ExclusiveZone) {
        self.apply(SurfaceOp::SetExclusiveZone(zone));
    }

    /// Layer surfaces only.
    pub fn set_size(&self, size: LayerSize) {
        self.apply(SurfaceOp::SetSize(size));
    }

    /// Any other op, as `HostCtx::apply` would apply it.
    pub fn apply(&self, op: SurfaceOp) {
        self.requests.push(Request::Apply { id: self.id, op });
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
        let id = self.ids.next();
        self.requests.push(Request::OpenPopup {
            parent: self.id,
            id,
            config,
            placement,
            root,
        });
        id
    }

    /// Close this surface (and, for a parent, its popups).
    pub fn close(&self) {
        self.requests.push(Request::Close(self.id));
    }

    /// Put `text` on the clipboard, owned by this surface's client.
    pub fn copy_text(&self, text: String) {
        self.requests.push(Request::CopyText {
            from: self.id,
            text,
        });
    }

    /// An xdg-activation token for launching `app_id` with focus handed over; `None` when the
    /// compositor has no xdg-activation or refused.
    pub async fn activation_token(&self, app_id: Option<String>) -> Option<String> {
        let (reply, answer) = oneshot::channel();
        self.requests.push(Request::ActivationToken {
            from: self.id,
            app_id,
            reply,
        });
        // A dropped sender (the surface closed first) is the same answer as a refusal.
        answer.await.ok().flatten()
    }
}

/// The handle of the surface this component renders in. Panics outside a host surface (a
/// caller contract, like any missing Dioxus context).
pub fn use_surface() -> SurfaceHandle {
    dioxus::prelude::use_context::<SurfaceHandle>()
}

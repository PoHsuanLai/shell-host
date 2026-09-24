//! `SurfaceHandle`: what a component uses to change its own surface. Provided as a root
//! context of every surface's `VirtualDom`; read it with [`use_surface`].

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use calloop::ping::Ping;
use dioxus::core::{spawn, use_hook};
use dioxus::hooks::use_signal;
use dioxus::signals::{ReadSignal, WritableExt};
use tokio::sync::oneshot;

use crate::blur::BlurSpec;
use crate::id::{SurfaceId, SurfaceIds};
use crate::layer::{ExclusiveZone, KeyboardMode, LayerSize};
use crate::live_scale::LiveScale;
use crate::msg::{Request, SurfaceOp};
use crate::placement::PopupPlacement;
use crate::popup::PopupConfig;
use crate::region::RegionSpec;
use crate::root::Root;
use crate::scale::Scale;
use crate::tick::{FrameDrive, FrameTick, FrameTicks};

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
    ticks: FrameTicks,
    scale: LiveScale,
}

impl SurfaceHandle {
    /// The handle for surface `id`. Built by the host when it creates the surface.
    pub(crate) fn new(
        id: SurfaceId,
        requests: RequestQueue,
        ids: SurfaceIds,
        ticks: FrameTicks,
        scale: LiveScale,
    ) -> SurfaceHandle {
        SurfaceHandle {
            id,
            requests,
            ids,
            ticks,
            scale,
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

    /// Render this surface once per compositor frame callback until the returned drive is
    /// dropped, ticking [`use_frame_tick`] and [`SurfaceHandle::next_frame`] every frame: how
    /// a component runs its own animation (the dock's magnification, design/10 §10.4) at the
    /// compositor's frame rate without a timer. Holding a drive with nothing animating is the
    /// one way to get non-zero idle frames, so take it when the motion starts and drop it when
    /// the motion settles or the hover that started it ends; never keep one for the life of a
    /// component.
    pub fn drive_frames(&self) -> FrameDrive {
        let drive = self.ticks.hold();
        self.requests.push(Request::Redraw(self.id));
        drive
    }

    /// The next frame tick of this surface. Ticks fire once per presented frame while the
    /// document has a CSS transition or animation running or a [`FrameDrive`] is held; at any
    /// other time this waits until one of those starts.
    pub async fn next_frame(&self) -> FrameTick {
        self.ticks.after(self.ticks.count()).await.1
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

/// The latest frame tick of the surface this component renders in, updated (and the component
/// re-rendered) once per presented frame while the document animates or a [`FrameDrive`] is
/// held (`SurfaceHandle::drive_frames`); `FrameTick::default()` before the first. Ticks are
/// never skipped by this hook's own task: each one is written, though a component rendering
/// slower than the frame rate reads only the latest.
pub fn use_frame_tick() -> ReadSignal<FrameTick> {
    let ticks = use_surface().ticks;
    let mut latest = use_signal(FrameTick::default);
    use_hook(move || {
        spawn(async move {
            let mut seen = ticks.count();
            loop {
                let (count, tick) = ticks.after(seen).await;
                seen = count;
                latest.set(tick);
            }
        });
    });
    ReadSignal::from(latest)
}

/// The scale of the surface this component renders in, in 120ths (`Scale(180)` is 1.5x), as
/// live state: the component re-renders when the compositor changes the surface's preferred
/// scale (`wp_fractional_scale_v1`, else `preferred_buffer_scale` or the output's integer
/// scale). Its first value is the scale the host knew when the component first rendered: the
/// host publishes a new scale before it polls documents, so the frame that first shows a scale
/// was rendered by a tree that had already read it. A surface starts at `Scale::ONE` until the
/// compositor says otherwise.
///
/// For a design system's device-pixel snapping (quire's `HostScale`, or `Ds { scale }`); the
/// layout half of snapping is `HostConfig::prepaint`.
pub fn use_surface_scale() -> ReadSignal<Scale> {
    let live = use_surface().scale;
    let mut latest = use_signal(|| live.get());
    use_hook(move || {
        // Read before the task first runs, in the same render as the initial value, so a change
        // landing between this render and the task's first poll is not missed.
        let mut seen = live.changes();
        spawn(async move {
            loop {
                let (count, scale) = live.changed_after(seen).await;
                seen = count;
                latest.set(scale);
            }
        });
    });
    ReadSignal::from(latest)
}

/// The handle of the surface this component renders in. Panics outside a host surface (a
/// caller contract, like any missing Dioxus context).
pub fn use_surface() -> SurfaceHandle {
    dioxus::prelude::use_context::<SurfaceHandle>()
}

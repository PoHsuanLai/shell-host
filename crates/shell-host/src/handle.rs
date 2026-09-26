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
use crate::drag::outcome::DragOutcome;
use crate::drag::spec::DragSpec;
use crate::grab::{GrabRecord, GrabRefused};
use crate::id::{SurfaceId, SurfaceIds};
use crate::layer::{ExclusiveZone, KeyboardMode, LayerSize};
use crate::live::Live;
use crate::live_scale::LiveScale;
use crate::msg::{Request, SurfaceOp};
use crate::placement::PopupPlacement;
use crate::popup::PopupConfig;
use crate::region::RegionSpec;
use crate::role::RoleKind;
use crate::root::Root;
use crate::scale::Scale;
use crate::scroll::cmd::ScrollCmd;
use crate::scroll::watch::ScrollBoard;
use crate::tick::{FrameDrive, FrameTick, FrameTicks};
use crate::toplevel::{Fullscreen, Maximize, ResizeEdge, ToplevelState};

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
    kind: RoleKind,
    requests: RequestQueue,
    ids: SurfaceIds,
    ticks: FrameTicks,
    scale: LiveScale,
    toplevel: Live<ToplevelState>,
    grab: GrabRecord,
    scrolls: ScrollBoard,
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

    /// Scroll a scroll container of this surface through the host's engine (the smooth rule,
    /// or at once): the way to scroll from code, since Blitz's own `scroll_to` would fight the
    /// engine (design/11 §11.3.11).
    pub fn scroll(&self, cmd: ScrollCmd) {
        self.requests.push(Request::Scroll { id: self.id, cmd });
    }

    /// The scrollers this surface's components watch (`use_scroll`).
    pub(crate) fn scroll_board(&self) -> ScrollBoard {
        self.scrolls.clone()
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

    /// Drag `spec.payload` out of this surface: call it while the button that started the
    /// gesture is still down (from a pointer-move handler past a threshold, say). The
    /// compositor then owns the pointer until release, and this surface sees no motion
    /// meanwhile. Resolves when the drag ends: `Dropped` when a target took the payload and
    /// finished (`Move` means delete the original), `Cancelled` when released over nothing
    /// that accepted it, or at once `Unsupported` when it cannot start (no button held on
    /// this surface, no data device, another drag still running).
    pub async fn start_drag(&self, spec: DragSpec) -> DragOutcome {
        let (reply, answer) = oneshot::channel();
        self.requests.push(Request::StartDrag {
            from: self.id,
            spec,
            reply,
        });
        // A dropped sender means the host went away with the drag: nothing was dropped.
        answer.await.unwrap_or(DragOutcome::Cancelled)
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

/// The state of the toplevel this component renders in, from each `xdg_toplevel.configure`:
/// maximized, fullscreen, activated and the tiled edges, as live state (the component
/// re-renders when a configure changes it; a configure that changes only the size wakes
/// nobody). For a client-decorated frame: draw the zoom button's state, square the corners
/// and drop the shadow when maximized or tiled, dim the traffic lights when inactive. The
/// host already removes the resize band in those states. The default (floating, inactive)
/// before the first configure, and forever on a layer surface or popup.
pub fn use_toplevel_state() -> ReadSignal<ToplevelState> {
    let live = use_surface().toplevel;
    let mut latest = use_signal(|| live.get());
    use_hook(move || {
        let mut seen = live.changes();
        spawn(async move {
            loop {
                let (count, state) = live.changed_after(seen).await;
                seen = count;
                latest.set(state);
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

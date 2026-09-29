//! What a component asks of its own surface: blur, regions, popups, frames, scrolling, the
//! clipboard, drags and closing.

use tokio::sync::oneshot;

use super::SurfaceHandle;
use crate::blur::BlurSpec;
use crate::drag::outcome::DragOutcome;
use crate::drag::spec::DragSpec;
use crate::id::SurfaceId;
use crate::layer::{ExclusiveZone, KeyboardMode, LayerSize};
use crate::msg::{Request, SurfaceOp};
use crate::placement::PopupPlacement;
use crate::popup::PopupConfig;
use crate::region::RegionSpec;
use crate::root::HostRoot;
use crate::scroll::cmd::ScrollCmd;
use crate::tick::{FrameDrive, FrameTick};

impl SurfaceHandle {
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

    /// Open a popup whose parent is this surface, rendering `root` as its own `VirtualDom`.
    /// Returns the popup's id at once; the popup appears after the next flush. Opening a
    /// grabbing popup from a `KeyboardMode::None` layer switches the layer to `OnDemand` until
    /// the popup closes, except on KWin, which focuses the popup itself and gives the keyboard
    /// back to the app on close (`popup_keyboard`). A click on the parent layer
    /// (another item of the same bar) is the shell's to dismiss by: KWin sends no `popup_done`
    /// for a press on the popup's own client.
    pub fn open_popup(
        &self,
        config: PopupConfig,
        placement: PopupPlacement,
        root: HostRoot,
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

    /// Scroll a scroll container of this surface through the host's engine (the smooth rule,
    /// or at once): the way to scroll from code, since Blitz's own `scroll_to` would fight the
    /// engine (design/11 §11.3.11).
    pub fn scroll(&self, cmd: ScrollCmd) {
        self.requests.push(Request::Scroll { id: self.id, cmd });
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

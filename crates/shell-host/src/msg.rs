//! What flows into the event loop: changes to a surface, and the messages behind a
//! `HostHandle` (any thread) and a `SurfaceHandle` (inside a document, main thread).

use tokio::sync::oneshot;

use crate::blur::BlurSpec;
use crate::drag::outcome::DragOutcome;
use crate::drag::spec::DragSpec;
use crate::geometry::Margin;
use crate::id::SurfaceId;
use crate::layer::{Anchor, ExclusiveZone, KeyboardMode, LayerLevel, LayerSize};
use crate::placement::PopupPlacement;
use crate::popup::PopupConfig;
use crate::region::RegionSpec;
use crate::root::HostRoot;
use crate::toplevel::{Fullscreen, Maximize, ResizeEdge};
use crate::window_placement::WindowPlacement;
use blitz_kit::scroll::cmd::ScrollCmd;

/// A change to one surface, grouped by the roles that can take it. Applied after the documents
/// are polled and before painting, so a region change lands in the same commit as the frame
/// that caused it. An op that does not fit the surface's role (a layer op on a popup) is
/// ignored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SurfaceOp {
    /// Any surface.
    Common(CommonOp),
    /// Layer surfaces only.
    Layer(LayerOp),
    /// Toplevels only.
    Window(WindowOp),
    /// Popups only: move an open popup (`xdg_popup.reposition`, xdg_wm_base v3), which is how a
    /// menu bar switches menus on hover without a new grab (design/13 §13.7).
    Reposition(PopupPlacement),
}

/// A change any surface takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommonOp {
    SetBlur(BlurSpec),
    SetInputRegion(RegionSpec),
    SetVisibility(Visibility),
}

/// A change only a layer surface takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayerOp {
    SetKeyboard(KeyboardMode),
    SetExclusiveZone(ExclusiveZone),
    SetSize(LayerSize),
    SetMargin(Margin),
    SetAnchor(Anchor),
    /// `zwlr_layer_surface_v1.set_layer`, v2.
    SetLayer(LayerLevel),
}

/// A change only a toplevel takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowOp {
    /// Start an interactive move (`xdg_toplevel.move`) with the seat's held press on this
    /// surface (`grab`'s rule); ignored, with a log line, without one.
    BeginMove,
    /// Start an interactive resize from `edge` (`xdg_toplevel.resize`), under the same rule as
    /// `BeginMove`.
    BeginResize(ResizeEdge),
    /// `xdg_toplevel.set_maximized` / `unset_maximized`. The compositor answers with a
    /// configure; nothing changes until it does.
    SetMaximized(Maximize),
    /// `xdg_toplevel.set_minimized`. There is no un-minimize request: the compositor (a dock, a
    /// task switcher) restores the window.
    Minimize,
    /// `xdg_toplevel.set_fullscreen` on the compositor's choice of output, or
    /// `unset_fullscreen`.
    SetFullscreen(Fullscreen),
    /// `quire_window_v1.request_placement` with the seat's latest input serial. Dropped (logged
    /// when there is no input yet) on a compositor without the protocol.
    RequestPlacement(WindowPlacement),
}

impl From<CommonOp> for SurfaceOp {
    fn from(op: CommonOp) -> SurfaceOp {
        SurfaceOp::Common(op)
    }
}

impl From<LayerOp> for SurfaceOp {
    fn from(op: LayerOp) -> SurfaceOp {
        SurfaceOp::Layer(op)
    }
}

impl From<WindowOp> for SurfaceOp {
    fn from(op: WindowOp) -> SurfaceOp {
        SurfaceOp::Window(op)
    }
}

/// Whether applying an op must repaint the surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repaint {
    /// The op changes what the next commit carries (a region, a layer's size or anchor).
    Needed,
    /// A window-management request: the compositor answers with a configure if anything
    /// changes, and that configure repaints. Repainting now would cost an idle frame.
    NotNeeded,
}

impl SurfaceOp {
    /// Whether applying this op repaints the surface.
    pub fn repaint(&self) -> Repaint {
        match self {
            SurfaceOp::Window(_) => Repaint::NotNeeded,
            SurfaceOp::Common(_) | SurfaceOp::Layer(_) | SurfaceOp::Reposition(_) => {
                Repaint::Needed
            }
        }
    }
}

/// Whether a surface is on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Visibility {
    Shown,
    /// Unmapped, with its document kept alive (a dock hidden over a fullscreen window keeps its
    /// state and its timers).
    Hidden,
}

/// A message sent through a `HostHandle`. `Send` whenever `M` is.
#[derive(Debug)]
pub(crate) enum HostMsg<M> {
    /// Delivered to the event closure as `HostEvent::App`.
    App(M),
    Apply {
        id: SurfaceId,
        op: SurfaceOp,
    },
    Close(SurfaceId),
    /// Close every surface and return from `Host::run`.
    Exit,
}

/// A request made from inside a document through its `SurfaceHandle`. Queued on the main
/// thread (a `HostRoot` is not `Send`), drained after the documents are polled.
#[derive(Debug)]
pub(crate) enum Request {
    Apply {
        id: SurfaceId,
        op: SurfaceOp,
    },
    OpenPopup {
        parent: SurfaceId,
        /// Allocated by the handle, so `open_popup` can return it before the popup exists.
        id: SurfaceId,
        config: PopupConfig,
        placement: PopupPlacement,
        root: HostRoot,
    },
    Close(SurfaceId),
    /// Render surface `id` on the next flush (a frame drive was taken on an idle surface, which
    /// has no callback coming to tick on).
    Redraw(SurfaceId),
    CopyText {
        text: String,
    },
    ActivationToken {
        from: SurfaceId,
        app_id: Option<String>,
        reply: oneshot::Sender<Option<String>>,
    },
    /// Start dragging out of surface `from` (`SurfaceHandle::start_drag`); `reply` gets the
    /// outcome when the drag ends, or at once when it cannot start.
    StartDrag {
        from: SurfaceId,
        spec: DragSpec,
        reply: oneshot::Sender<DragOutcome>,
    },
    /// A programmatic scroll of surface `id` (`SurfaceHandle::scroll`).
    Scroll {
        id: SurfaceId,
        cmd: ScrollCmd,
    },
    /// Unlock session lock `LockId` (`LockHandle::unlock`, `HostCtx::unlock_session`).
    Unlock(crate::lock::LockId),
}

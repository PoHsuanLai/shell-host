//! What flows into the event loop: changes to a surface, and the messages behind a
//! `HostHandle` (any thread) and a `SurfaceHandle` (inside a document, main thread).

use tokio::sync::oneshot;

use crate::blur::BlurSpec;
use crate::geometry::Margin;
use crate::id::SurfaceId;
use crate::layer::{Anchor, ExclusiveZone, KeyboardMode, Layer, LayerSize};
use crate::placement::PopupPlacement;
use crate::popup::PopupConfig;
use crate::region::RegionSpec;
use crate::root::Root;

/// A change to one surface. Applied after the documents are polled and before painting, so a
/// region change lands in the same commit as the frame that caused it. An op that does not fit
/// the surface's role (a layer op on a popup) is ignored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SurfaceOp {
    SetBlur(BlurSpec),
    SetInputRegion(RegionSpec),
    /// Layer surfaces only.
    SetKeyboard(KeyboardMode),
    /// Layer surfaces only.
    SetExclusiveZone(ExclusiveZone),
    /// Layer surfaces only.
    SetSize(LayerSize),
    /// Layer surfaces only.
    SetMargin(Margin),
    /// Layer surfaces only.
    SetAnchor(Anchor),
    /// Layer surfaces only (`zwlr_layer_surface_v1.set_layer`, v2).
    SetLayer(Layer),
    SetVisibility(Visibility),
    /// Popups only: move an open popup (`xdg_popup.reposition`, xdg_wm_base v3), which is how a
    /// menu bar switches menus on hover without a new grab (design/13 §13.7).
    Reposition(PopupPlacement),
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
pub enum HostMsg<M> {
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
/// thread (a `Root` is not `Send`), drained after the documents are polled.
#[derive(Debug)]
pub enum Request {
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
        root: Root,
    },
    Close(SurfaceId),
    CopyText {
        from: SurfaceId,
        text: String,
    },
    ActivationToken {
        from: SurfaceId,
        app_id: Option<String>,
        reply: oneshot::Sender<Option<String>>,
    },
}

//! One surface's document: building it from a `Root`, polling it, feeding it events, and
//! answering layout questions (element rectangles for regions and popup anchors).
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use std::sync::Arc;
use std::task::Context;

use blitz_dom::FontContext;
use blitz_traits::events::UiEvent;
use blitz_traits::shell::Viewport;
use dioxus_native_dom::DioxusDocument;

use crate::dom::head::HeadQueue;
use crate::dom::net::DataUriNet;
use crate::dom::shell_provider::HostShellProvider;
use crate::frame::Animation;
use crate::region::{ElementId, Rect};
use crate::root::Root;

/// The host's user-agent stylesheet, added to every document. A sized root is what lets Blitz
/// hit-test into the content at all (SPIKE.md S1 finding 2: with `html,body` at zero height,
/// hover never reached the dock icons), and a transparent body is what lets blur and the
/// desktop show through. Margins and the rest of the reset come from quire's `css/reset.css`.
pub const UA_STYLESHEET: &str = "html,body{height:100%;background:transparent}";

/// Everything a document is built with besides its root.
#[derive(Clone)]
pub struct DocumentEnv {
    pub shell: Arc<HostShellProvider>,
    /// A clone of the host's one `FontContext` (shares the source cache).
    pub fonts: FontContext,
    pub net: Arc<DataUriNet>,
    pub viewport: Viewport,
}

impl std::fmt::Debug for DocumentEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DocumentEnv")
            .field("shell", &self.shell)
            .field("viewport", &self.viewport)
            .finish_non_exhaustive()
    }
}

/// Whether polling changed the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Polled {
    Changed,
    Unchanged,
}

/// A surface's live document.
pub struct SurfaceDocument {
    doc: DioxusDocument,
    head: HeadQueue,
}

impl SurfaceDocument {
    /// Build and initially render `root`: `StyleThreading::Sequential` (every document resolves
    /// on the main thread, blitz #430), the UA stylesheet, the head shim as the Dioxus
    /// `Document` context, the `SurfaceHandle` as a root context.
    pub fn build(root: Root, env: DocumentEnv) -> SurfaceDocument {
        todo!()
    }

    /// Run ready Dioxus work with `cx`'s waker, then apply queued head elements.
    pub fn poll(&mut self, cx: &mut Context<'_>) -> Polled {
        todo!()
    }

    /// Set the physical size, scale and colour scheme.
    pub fn set_viewport(&mut self, viewport: Viewport) {
        todo!()
    }

    /// Feed one input event.
    pub fn handle_event(&mut self, event: UiEvent) {
        todo!()
    }

    /// `clear_hover()`; `Polled::Changed` if something was hovered.
    pub fn clear_hover(&mut self) -> Polled {
        todo!()
    }

    /// Resolve style and layout at document time `t` (seconds on the host clock).
    pub fn resolve(&mut self, t: f64) {
        todo!()
    }

    /// Whether a transition or animation is still running.
    pub fn animation(&self) -> Animation {
        todo!()
    }

    /// The laid-out rectangle of the element `id` names, in surface-local logical pixels
    /// (`geometry::element_name`, `get_element_by_id`, `get_client_bounding_rect`,
    /// `geometry::enclosing_rect`); `None` before layout or when absent.
    pub fn element_rect(&self, id: ElementId) -> Option<Rect> {
        todo!()
    }

    /// Whether the element at a surface-local point, or an ancestor, carries
    /// `data-first-mouse` (design/13 §13.3.8).
    pub fn wants_first_mouse(&self, x: f64, y: f64) -> FirstMouseOptIn {
        todo!()
    }

    /// The underlying Dioxus document, for painting.
    pub fn dioxus(&mut self) -> &mut DioxusDocument {
        todo!()
    }
}

/// Whether an element opted into receiving the click that activates its window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstMouseOptIn {
    OptedIn,
    Default,
}

impl std::fmt::Debug for SurfaceDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SurfaceDocument").finish_non_exhaustive()
    }
}

//! One surface's document: building it from a `Root`, polling it, feeding it events, and
//! answering layout questions (element rectangles for regions and popup anchors).

use std::rc::Rc;
use std::sync::Arc;
use std::task::Context;

use blitz_dom::{Document as _, DocumentConfig, FontContext, StyleThreading};
use blitz_traits::events::UiEvent;
use blitz_traits::net::NetProvider;
use blitz_traits::shell::{ShellProvider, Viewport};
use dioxus_native_dom::DioxusDocument;

use crate::dom::head::{HeadDocument, HeadElement, HeadQueue};
use crate::dom::net::DataUriNet;
use crate::dom::shell_provider::HostShellProvider;
use crate::frame::Animation;
use crate::geometry::{element_name, enclosing_rect};
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
        let head = HeadQueue::default();
        let head_document: Rc<dyn dioxus_document::Document> =
            Rc::new(HeadDocument::new(head.clone()));
        let vdom = root.context(head_document).into_vdom();

        let config = DocumentConfig {
            viewport: Some(env.viewport),
            net_provider: Some(env.net as Arc<dyn NetProvider>),
            shell_provider: Some(env.shell as Arc<dyn ShellProvider>),
            font_ctx: Some(env.fonts),
            style_threading: StyleThreading::Sequential,
            ..Default::default()
        };

        let mut doc = DioxusDocument::new(vdom, config);
        doc.inner_mut().add_user_agent_stylesheet(UA_STYLESHEET);
        doc.initial_build();
        apply_head(&mut doc, &head);

        SurfaceDocument { doc, head }
    }

    /// Run ready Dioxus work with `cx`'s waker, then apply queued head elements.
    pub fn poll(&mut self, cx: &mut Context<'_>) -> Polled {
        let waker = cx.waker().clone();
        let task_cx = Context::from_waker(&waker);
        let doc_changed = self.doc.poll(Some(task_cx));
        let head_changed = apply_head(&mut self.doc, &self.head);
        if doc_changed || head_changed {
            Polled::Changed
        } else {
            Polled::Unchanged
        }
    }

    /// Set the physical size, scale and colour scheme.
    pub fn set_viewport(&mut self, viewport: Viewport) {
        self.doc.inner_mut().set_viewport(viewport);
    }

    /// Feed one input event.
    pub fn handle_event(&mut self, event: UiEvent) {
        self.doc.handle_ui_event(event);
    }

    /// `clear_hover()`; `Polled::Changed` if something was hovered.
    pub fn clear_hover(&mut self) -> Polled {
        if self.doc.inner_mut().clear_hover() {
            Polled::Changed
        } else {
            Polled::Unchanged
        }
    }

    /// Resolve style and layout at document time `t` (seconds on the host clock).
    pub fn resolve(&mut self, t: f64) {
        self.doc.inner_mut().resolve(t);
    }

    /// Whether a transition or animation is still running.
    pub fn animation(&self) -> Animation {
        if self.doc.inner().is_animating() {
            Animation::Running
        } else {
            Animation::Stopped
        }
    }

    /// The laid-out rectangle of the element `id` names, in surface-local logical pixels
    /// (`geometry::element_name`, `get_element_by_id`, `get_client_bounding_rect`,
    /// `geometry::enclosing_rect`); `None` before layout or when absent.
    pub fn element_rect(&self, id: ElementId) -> Option<Rect> {
        let name = element_name(id)?;
        let inner = self.doc.inner();
        let node_id = inner.get_element_by_id(name)?;
        let bounds = inner.get_client_bounding_rect(node_id)?;
        Some(enclosing_rect(
            bounds.x,
            bounds.y,
            bounds.width,
            bounds.height,
        ))
    }

    /// Whether the element at a surface-local point, or an ancestor, carries
    /// `data-first-mouse` (design/13 §13.3.8).
    pub fn wants_first_mouse(&self, x: f64, y: f64) -> FirstMouseOptIn {
        let inner = self.doc.inner();
        let Some(mut node_id) = inner.element_from_point(x as f32, y as f32) else {
            return FirstMouseOptIn::Default;
        };
        loop {
            let Some(node) = inner.get_node(node_id) else {
                return FirstMouseOptIn::Default;
            };
            let opted_in = node.element_data().is_some_and(|data| {
                data.attrs
                    .iter()
                    .any(|attr| *attr.name.local == *"data-first-mouse")
            });
            if opted_in {
                return FirstMouseOptIn::OptedIn;
            }
            match node.parent {
                Some(parent) => node_id = parent,
                None => return FirstMouseOptIn::Default,
            }
        }
    }

    /// The underlying Dioxus document, for painting.
    pub fn dioxus(&mut self) -> &mut DioxusDocument {
        &mut self.doc
    }
}

/// Drain `head`'s queue into `doc`'s `<head>`; `true` if anything was applied.
fn apply_head(doc: &mut DioxusDocument, head: &HeadQueue) -> bool {
    let elements = head.drain();
    let changed = !elements.is_empty();
    for HeadElement {
        name,
        attributes,
        contents,
    } in elements
    {
        doc.create_head_element(&name, &attributes, &contents);
    }
    changed
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

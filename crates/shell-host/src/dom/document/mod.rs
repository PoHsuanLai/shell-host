//! One surface's document: building it from a `Root`, polling it, feeding it events, and
//! answering layout questions (element rectangles for regions and popup anchors).

mod first_mouse;
mod hover;
mod rects;
mod resolve;

pub use first_mouse::FirstMouseOptIn;
pub use hover::HoverShift;
pub use resolve::Resolved;

use std::rc::Rc;
use std::sync::Arc;
use std::task::Context;

use blitz_dom::{Document as _, DocumentConfig, FontContext, StyleThreading};
use blitz_traits::events::UiEvent;
use blitz_traits::net::NetProvider;
use blitz_traits::shell::{ShellProvider, Viewport};
use dioxus_native_dom::DioxusDocument;

use crate::dom::head::{HeadDocument, HeadElement, HeadQueue};
use crate::dom::hover_sync::{self, LastMove};
use crate::dom::net::LocalNet;
use crate::dom::shell_provider::HostShellProvider;
use crate::prepaint::Prepaint;
use crate::root::Root;

/// The host's user-agent stylesheet, added to every document. A sized root is what lets Blitz
/// hit-test into the content at all (with `html,body` at zero height, hover never reached
/// the dock icons), and a transparent body is what lets blur and the
/// desktop show through. Margins and the rest of the reset come from quire's `css/reset.css`.
pub const UA_STYLESHEET: &str = "html,body{height:100%;background:transparent}";

/// Zero `<body>`'s margin, added to every document the host builds (the headless path too):
/// Blitz's default stylesheet gives it `margin: 8px` (`blitz-dom/assets/default.css`), which
/// kept a bare document, one without quire's reset, from ever reaching its surface's (0, 0)
/// corner. A user-agent sheet, so any rule of the document's own wins.
pub const BODY_RESET: &str = "body{margin:0}";

/// Everything a document is built with besides its root.
#[derive(Clone)]
pub struct DocumentEnv {
    pub shell: Arc<HostShellProvider>,
    /// A clone of the host's one `FontContext` (shares the source cache).
    pub fonts: FontContext,
    pub net: Arc<LocalNet>,
    pub viewport: Viewport,
    /// Run after every `resolve` (`HostConfig::prepaint`).
    pub prepaint: Prepaint,
}

impl std::fmt::Debug for DocumentEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DocumentEnv")
            .field("shell", &self.shell)
            .field("viewport", &self.viewport)
            .field("prepaint", &self.prepaint)
            .finish_non_exhaustive()
    }
}

/// Renders one `poll` may run before it yields to the rest of the flush.
const MAX_POLLS: u32 = 8;

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
    prepaint: Prepaint,
    /// The last real pointer position the surface fed, for `resolve`'s hover repair.
    last_move: LastMove,
    /// The document's shell provider, for `resolve`'s IME repair.
    shell: Arc<HostShellProvider>,
}

impl SurfaceDocument {
    /// Build and initially render `root`: `StyleThreading::Sequential` (every document resolves
    /// on the main thread, blitz #430), the UA stylesheet and the body reset, the head shim as the Dioxus
    /// `Document` context, the `SurfaceHandle` as a root context.
    pub fn build(root: Root, env: DocumentEnv) -> SurfaceDocument {
        let head = HeadQueue::default();
        let head_document: Rc<dyn dioxus_document::Document> =
            Rc::new(HeadDocument::new(head.clone()));
        let vdom = root.context(head_document).into_vdom();

        let config = DocumentConfig {
            viewport: Some(env.viewport),
            net_provider: Some(env.net as Arc<dyn NetProvider>),
            shell_provider: Some(Arc::clone(&env.shell) as Arc<dyn ShellProvider>),
            font_ctx: Some(env.fonts),
            style_threading: StyleThreading::Sequential,
            ..Default::default()
        };

        let mut doc = DioxusDocument::new(vdom, config);
        doc.inner_mut().add_user_agent_stylesheet(UA_STYLESHEET);
        doc.inner_mut().add_user_agent_stylesheet(BODY_RESET);
        doc.initial_build();
        apply_head(&mut doc, &head);

        SurfaceDocument {
            doc,
            head,
            prepaint: env.prepaint,
            last_move: LastMove::Unknown,
            shell: env.shell,
        }
    }

    /// Run ready Dioxus work with `cx`'s waker until none is left, then apply queued head
    /// elements.
    ///
    /// `DioxusDocument::poll` renders and returns as soon as the `VirtualDom` has dirty work,
    /// without the `VirtualDom` keeping `cx`'s waker: only a poll that finds nothing to do
    /// leaves it registered. So this polls until one finds nothing; stopping
    /// after a render would lose the next wake of any task (a frame tick, a `use_watch`
    /// value) and leave the document unpolled until something else woke it. A document still
    /// busy after `MAX_POLLS` wakes itself and is polled again on the next flush.
    pub fn poll(&mut self, cx: &mut Context<'_>) -> Polled {
        let waker = cx.waker().clone();
        let mut doc_changed = false;
        let mut settled = false;
        for _ in 0..MAX_POLLS {
            if self.doc.poll(Some(Context::from_waker(&waker))) {
                doc_changed = true;
            } else {
                settled = true;
                break;
            }
        }
        if !settled {
            waker.wake_by_ref();
        }
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

    /// Feed one input event, remembering a pointer position for `resolve`'s hover repair. A
    /// pointer or wheel event's page coordinates are its surface-local ones plus the viewport's
    /// scroll (the engine scrolls the viewport; Blitz hit-tests page coordinates).
    pub fn handle_event(&mut self, event: UiEvent) {
        let event = crate::input::pointer::scrolled(event, self.viewport_scroll());
        self.last_move = hover_sync::remember(std::mem::take(&mut self.last_move), &event);
        self.doc.handle_ui_event(event);
    }

    /// The viewport's scroll offset.
    fn viewport_scroll(&self) -> (f64, f64) {
        let scroll = self.doc.inner().viewport_scroll();
        (scroll.x, scroll.y)
    }

    /// The Blitz document, read (the scroll engine's geometry, `dom::scroll`).
    pub fn base(&self) -> blitz_dom::DocGuard<'_> {
        self.doc.inner()
    }

    /// The Blitz document, written (the scroll engine's raw offsets, `dom::scroll`).
    pub fn base_mut(&mut self) -> blitz_dom::DocGuardMut<'_> {
        self.doc.inner_mut()
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

impl std::fmt::Debug for SurfaceDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SurfaceDocument").finish_non_exhaustive()
    }
}

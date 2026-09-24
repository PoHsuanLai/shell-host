//! One surface's document: building it from a `Root`, polling it, feeding it events, and
//! answering layout questions (element rectangles for regions and popup anchors).

use std::rc::Rc;
use std::sync::Arc;
use std::task::Context;

use blitz_dom::{Document as _, DocumentConfig, ElementData, FontContext, StyleThreading};
use blitz_traits::events::UiEvent;
use blitz_traits::net::NetProvider;
use blitz_traits::shell::{ShellProvider, Viewport};
use dioxus_native_dom::DioxusDocument;

use crate::dom::head::{HeadDocument, HeadElement, HeadQueue};
use crate::dom::net::LocalNet;
use crate::dom::shell_provider::HostShellProvider;
use crate::frame::Animation;
use crate::geometry::enclosing_rect;
use crate::region::{ElementId, Rect};
use crate::root::Root;
use crate::scale::Size;

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
    pub net: Arc<LocalNet>,
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

    /// Run ready Dioxus work with `cx`'s waker until none is left, then apply queued head
    /// elements.
    ///
    /// `DioxusDocument::poll` renders and returns as soon as the `VirtualDom` has dirty work,
    /// without the `VirtualDom` keeping `cx`'s waker: only a poll that finds nothing to do
    /// leaves it registered. So this polls until one finds nothing (FINDINGS F33); stopping
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

    /// Feed one input event.
    pub fn handle_event(&mut self, event: UiEvent) {
        self.doc.handle_ui_event(event);
    }

    /// The pointer left the surface: the leave move (`input::pointer::leave_event`) first, so
    /// Blitz's own hover change dispatches `mouseout`/`mouseleave` to the hovered chain and a
    /// component can see the hover end; then `clear_hover()` as a backstop for anything the
    /// move left hovered. `Polled::Changed` if something was hovered before.
    pub fn pointer_left(&mut self, mods: keyboard_types::Modifiers) -> Polled {
        let hovered = self.doc.inner().get_hover_node_id().is_some();
        self.doc
            .handle_ui_event(crate::input::pointer::leave_event(mods));
        self.clear_hover();
        if hovered {
            Polled::Changed
        } else {
            Polled::Unchanged
        }
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
    /// (`get_element_by_id`, `get_client_bounding_rect`, `geometry::enclosing_rect`); `None`
    /// before layout or when absent.
    pub fn element_rect(&self, id: ElementId) -> Option<Rect> {
        let inner = self.doc.inner();
        let node_id = inner.get_element_by_id(id.as_str())?;
        let bounds = inner.get_client_bounding_rect(node_id)?;
        Some(enclosing_rect(
            bounds.x,
            bounds.y,
            bounds.width,
            bounds.height,
        ))
    }

    /// A popup document's content size, in logical px, for `PopupSize::FitContent`
    /// (`popup::refit`). Neither `<body>` nor `dioxus-native-dom`'s own `<main id="main">` (the
    /// fixed mount point every `DioxusDocument` renders its `VirtualDom` into,
    /// `dioxus_native_dom::DioxusDocument::new`) is it: a block's default `width:auto` fills
    /// its container, so both stretch to the surface's width regardless of content, which is
    /// exactly the size `FitContent` needs to *not* read (`<body>`'s `height:100%`, from this
    /// crate's own `UA_STYLESHEET`, does the same on the other axis). The measured element is
    /// `#main`'s first content child: `<style>`, `<script>`, `<title>`, `<link>`, `<meta>`,
    /// `<base>` and `<template>` are skipped (Blitz's own default stylesheet already hides them,
    /// `display:none`, so their box carries no useful size, and a document's own `<style>`
    /// tends to render as the first child of its root). `None` before the first layout, when
    /// `#main` is missing (a caller contract: `dioxus-native-dom` always creates it), or when it
    /// has no such child.
    pub fn content_size(&self) -> Option<Size> {
        let inner = self.doc.inner();
        let main = inner.get_element_by_id("main")?;
        let children = &inner.get_node(main)?.children;
        let content = children.iter().copied().find(|&child| {
            inner
                .get_node(child)
                .and_then(|node| node.element_data())
                .is_some_and(is_content_element)
        })?;
        let bounds = inner.get_client_bounding_rect(content)?;
        let rect = enclosing_rect(0.0, 0.0, bounds.width, bounds.height);
        Some(Size {
            width: rect.width,
            height: rect.height,
        })
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

/// Whether `data` names a tag that `SurfaceDocument::content_size` should skip: never a
/// document's own content, always `display:none` by Blitz's default stylesheet.
fn is_content_element(data: &ElementData) -> bool {
    !matches!(
        &*data.name.local,
        "style" | "script" | "title" | "link" | "meta" | "base" | "template"
    )
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

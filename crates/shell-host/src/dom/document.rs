//! One surface's document: building it from a `Root`, polling it, feeding it events, and
//! answering layout questions (element rectangles for regions and popup anchors).

use std::rc::Rc;
use std::sync::Arc;
use std::task::Context;

use blitz_dom::{
    BaseDocument, Document as _, DocumentConfig, ElementData, FontContext, NodeId, StyleThreading,
};
use blitz_traits::events::UiEvent;
use blitz_traits::net::NetProvider;
use blitz_traits::shell::{ShellProvider, Viewport};
use dioxus_native_dom::DioxusDocument;

use crate::dom::head::{HeadDocument, HeadElement, HeadQueue};
use crate::dom::hover_sync::{self, Bounds, HoverSync, LastMove, Shift};
use crate::dom::net::LocalNet;
use crate::dom::shell_provider::HostShellProvider;
use crate::dom::stacking;
use crate::frame::Animation;
use crate::geometry::enclosing_rect;
use crate::input::pointer::PointerPos;
use crate::prepaint::{Prepaint, Prepainted};
use crate::region::{ElementId, Rect};
use crate::root::Root;
use crate::scale::Size;

/// The host's user-agent stylesheet, added to every document. A sized root is what lets Blitz
/// hit-test into the content at all (SPIKE.md S1 finding 2: with `html,body` at zero height,
/// hover never reached the dock icons), and a transparent body is what lets blur and the
/// desktop show through. Margins and the rest of the reset come from quire's `css/reset.css`.
pub const UA_STYLESHEET: &str = "html,body{height:100%;background:transparent}";

/// Zero `<body>`'s margin, added to every document the host builds (the headless path too):
/// Blitz's default stylesheet gives it `margin: 8px` (`blitz-dom/assets/default.css`), which
/// kept a bare document, one without quire's reset, from ever reaching its surface's (0, 0)
/// corner (FINDINGS F62). A user-agent sheet, so any rule of the document's own wins.
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
}

/// Whether a resolve moved the hover: the restyle it needs lands on a later frame, and the
/// resolve's own redraw request is dropped as the render's echo (`ctx::paint`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoverShift {
    Moved,
    Kept,
}

/// What one `SurfaceDocument::resolve` did besides laying out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolved {
    pub prepainted: Prepainted,
    pub hover: HoverShift,
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
            shell_provider: Some(env.shell as Arc<dyn ShellProvider>),
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
        }
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

    /// `clear_hover()`, and forget the pointer; `Polled::Changed` if something was hovered.
    pub fn clear_hover(&mut self) -> Polled {
        self.last_move = LastMove::Unknown;
        if self.doc.inner_mut().clear_hover() {
            Polled::Changed
        } else {
            Polled::Unchanged
        }
    }

    /// Resolve style and layout at document time `t` (seconds on the host clock), hoist the
    /// z-indexed boxes again from the fresh layout (Blitz gathers them from the previous one,
    /// FINDINGS F69), repair the hover the resolve moved, then run the pre-paint hook on the
    /// result (and hoist once more after it). Resolve and hook are
    /// one call because Blitz re-rounds the layout on every resolve: a hook that snapped it must
    /// follow each one, or the paint shows the unsnapped layout. So a frame resolves exactly
    /// once, here, and paints what this left. The hover repair (FINDINGS F59) touches hover
    /// state and runs handlers, never layout, so the hook and the paint see the same layout;
    /// the restyle it causes is the next frame's (`Resolved::hover`).
    pub fn resolve(&mut self, t: f64) -> Resolved {
        let before = self.hovered();
        self.doc.inner_mut().resolve(t);
        stacking::rehoist(&mut self.doc.inner_mut());
        let after = self.hovered();
        self.sync_hover(Shift { before, after });
        let prepainted = self.prepaint.run(&mut self.doc.inner_mut());
        if prepainted == Prepainted::Ran {
            // The hook may have moved boxes (snapping); hoist from where it left them.
            stacking::rehoist(&mut self.doc.inner_mut());
        }
        let hover = if before == after {
            HoverShift::Kept
        } else {
            HoverShift::Moved
        };
        Resolved { prepainted, hover }
    }

    /// The hovered element (a hovered text node counts as its element).
    fn hovered(&self) -> Option<NodeId> {
        let inner = self.doc.inner();
        element_of(&inner, inner.get_hover_node_id()?)
    }

    /// Blitz's `refresh_hover` moved the hover without dispatching (its TODO): put the old
    /// hover back without dispatch, then feed the last move again, so the driver dispatches the
    /// leave/enter diff itself. At most once per resolve.
    fn sync_hover(&mut self, shift: Shift<NodeId>) {
        let sync = {
            let inner = self.doc.inner();
            hover_sync::decide(
                shift,
                &self.last_move,
                |id| bounds_of(&inner, id),
                |at| element_at(&inner, at),
            )
        };
        match sync {
            HoverSync::Unchanged => return,
            HoverSync::Restore(at) => {
                self.doc.inner_mut().set_hover_to(at.x as f32, at.y as f32);
            }
            HoverSync::Clear => {
                self.doc.inner_mut().clear_hover();
            }
        }
        if let LastMove::At(event) = &self.last_move {
            let again = UiEvent::PointerMove(event.clone());
            let again = crate::input::pointer::scrolled(again, self.viewport_scroll());
            self.doc.handle_ui_event(again);
        }
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

/// `id` if it is an element, else its nearest element ancestor: the node whose listeners a
/// hover change reaches.
fn element_of(doc: &BaseDocument, id: NodeId) -> Option<NodeId> {
    let node = doc.get_node(id)?;
    if node.is_element() {
        Some(id)
    } else {
        element_of(doc, node.parent?)
    }
}

/// The element Blitz would hover at `at`: the hit, lifted out of anonymous boxes as
/// `set_hover_to` lifts it, then to its element.
fn element_at(doc: &BaseDocument, at: PointerPos) -> Option<NodeId> {
    let hit = doc.hit(at.x as f32, at.y as f32)?;
    element_of(doc, doc.nearest_non_anonymous_ancestor(hit.node_id)?)
}

/// `id`'s current border box, in the page coordinates Blitz hit-tests (client plus the
/// viewport's scroll).
fn bounds_of(doc: &BaseDocument, id: NodeId) -> Option<Bounds> {
    let rect = doc.get_client_bounding_rect(id)?;
    let scroll = doc.viewport_scroll();
    Some(Bounds {
        x: rect.x + scroll.x,
        y: rect.y + scroll.y,
        width: rect.width,
        height: rect.height,
    })
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

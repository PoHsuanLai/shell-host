//! Pointer events reaching a real document: the move-before-down, leave and wheel rules from
//! `input::pointer`, applied to a live Blitz document.
//!
//! `dom::document::SurfaceDocument` (the host's own wrapper) is a different wave's stub
//! (`todo!()` in this worktree; H2 dom owns `dom/**`), so these tests build the document the
//! same way `spike/doc.rs` does: a `dioxus_native_dom::DioxusDocument` fed directly with
//! `blitz_dom`'s UA stylesheet and the events `input::pointer` produces. That exercises exactly
//! the same event sequences `handlers/pointer.rs` feeds to a `SurfaceDocument`, since
//! `SurfaceDocument::handle_event` is specified to forward to the same `Document::handle_ui_event`
//! these tests call. The `data-first-mouse` opt-in check below (design/13 §13.3.8) mirrors what
//! `SurfaceDocument::wants_first_mouse` will do once it exists, rather than calling it.

use std::sync::Arc;

use blitz_dom::{Document, DocumentConfig, NodeId};
use blitz_traits::events::{
    BlitzWheelDelta, BlitzWheelEvent, MouseEventButton, MouseEventButtons, Point, PointerCoords,
    UiEvent,
};
use blitz_traits::shell::{ColorScheme, Viewport};
use dioxus::prelude::*;
use dioxus_native_dom::DioxusDocument;
use keyboard_types::Modifiers;
use shell_host::input::pointer::{self, PointerPos};

/// SPIKE.md S1 finding 2: a sized root is what lets hit-testing reach in-flow content at all.
const UA_STYLESHEET: &str = "html,body{height:100%;background:transparent;margin:0}";

const CSS: &str = "
#dock{display:flex;gap:8px;padding:8px}
.icon{width:40px;height:40px;background:#456}
#scroller{width:120px;height:80px;overflow:auto}
#tall{height:600px}
#drag{width:60px;height:20px}
";

fn app() -> Element {
    rsx! {
        style { "{UA_STYLESHEET}{CSS}" }
        div { id: "dock",
            div { id: "icon0", class: "icon" }
            div { id: "icon1", class: "icon" }
            div { id: "icon2", class: "icon" }
        }
        div { id: "scroller",
            div { id: "tall" }
        }
        div { id: "drag", "data-first-mouse": "true" }
    }
}

fn build() -> DioxusDocument {
    let config = DocumentConfig {
        viewport: Some(Viewport::new(400, 300, 1.0, ColorScheme::Light)),
        shell_provider: Some(Arc::new(NoopShell) as _),
        style_threading: blitz_dom::StyleThreading::Sequential,
        ..Default::default()
    };
    let mut doc = DioxusDocument::new(VirtualDom::new(app), config);
    doc.initial_build();
    doc.inner.borrow_mut().resolve(0.0);
    doc
}

/// A `ShellProvider` that does nothing; these tests only care about the DOM's own state.
struct NoopShell;
impl blitz_traits::shell::ShellProvider for NoopShell {}

fn node_id(doc: &DioxusDocument, selector: &str) -> NodeId {
    doc.inner
        .borrow()
        .query_selector(selector)
        .unwrap()
        .unwrap_or_else(|| panic!("no element matches {selector}"))
}

fn center(doc: &DioxusDocument, id: NodeId) -> PointerPos {
    let rect = doc
        .inner
        .borrow()
        .get_client_bounding_rect(id)
        .expect("node is laid out");
    PointerPos {
        x: rect.x + rect.width / 2.0,
        y: rect.y + rect.height / 2.0,
    }
}

fn feed(doc: &mut DioxusDocument, events: impl IntoIterator<Item = UiEvent>) {
    for event in events {
        doc.handle_ui_event(event);
    }
}

/// With the UA stylesheet's sized root, a hover over an in-flow dock icon hovers the icon, not
/// some ancestor (SPIKE.md S1 finding 2).
#[test]
fn hover_reaches_in_flow_content() {
    let mut doc = build();
    let icon2 = node_id(&doc, "#icon2");
    let pos = center(&doc, icon2);

    feed(
        &mut doc,
        [pointer::move_event(
            pos,
            MouseEventButtons::None,
            Modifiers::empty(),
        )],
    );

    assert_eq!(doc.inner.borrow().get_hover_node_id(), Some(icon2));
}

/// A press with no motion before it still clicks its target: `press_events`' synthesized move
/// sets the hover target before the down is dispatched.
#[test]
fn a_press_without_prior_motion_clicks_its_target() {
    let mut doc = build();
    let icon1 = node_id(&doc, "#icon1");
    let pos = center(&doc, icon1);

    // No PointerMove fed first: `press_events` supplies its own.
    let events = pointer::press_events(
        pos,
        MouseEventButton::Main,
        MouseEventButtons::None,
        Modifiers::empty(),
    );
    feed(&mut doc, events);

    assert_eq!(doc.inner.borrow().get_hover_node_id(), Some(icon1));
    assert_eq!(doc.inner.borrow().get_mousedown_node_id(), Some(icon1));
}

/// Leaving clears `:hover`: the hovered element's hover state is gone after `clear_hover` and
/// the parking move to (-1, -1).
#[test]
fn leaving_clears_hover() {
    let mut doc = build();
    let icon0 = node_id(&doc, "#icon0");
    let pos = center(&doc, icon0);
    feed(
        &mut doc,
        [pointer::move_event(
            pos,
            MouseEventButtons::None,
            Modifiers::empty(),
        )],
    );
    assert_eq!(
        doc.inner.borrow().get_hover_node_id(),
        Some(icon0),
        "precondition: icon0 is hovered before leaving"
    );

    doc.inner.borrow_mut().clear_hover();
    feed(&mut doc, [pointer::leave_event(Modifiers::empty())]);

    assert_eq!(doc.inner.borrow().get_hover_node_id(), None);
}

/// One wheel notch (`Lines`) and a touchpad delta (`Pixels`) both scroll the hovered
/// container, by Blitz's line height (20 px, `blitz-dom`'s own `Lines * 20`) and by the raw
/// pixel amount respectively.
#[test]
fn wheel_notches_scroll_by_lines_and_touchpads_by_pixels() {
    let mut doc = build();
    let scroller = node_id(&doc, "#scroller");
    let pos = center(&doc, scroller);
    feed(
        &mut doc,
        [pointer::move_event(
            pos,
            MouseEventButtons::None,
            Modifiers::empty(),
        )],
    );

    let before = scroll_y(&doc, scroller);

    let one_notch_down = pointer::wheel_delta(default_axis(), axis_value120(120))
        .expect("one notch down produces a delta");
    doc.handle_ui_event(UiEvent::Wheel(wheel_event(one_notch_down)));
    let after_notch = scroll_y(&doc, scroller);
    assert_eq!(
        after_notch - before,
        20.0,
        "one notch (value120 120, negated to -1 line) scrolls by exactly one line height"
    );

    let touchpad = pointer::wheel_delta(default_axis(), axis_absolute(15.0))
        .expect("a continuous delta produces pixels");
    doc.handle_ui_event(UiEvent::Wheel(wheel_event(touchpad)));
    let after_pixels = scroll_y(&doc, scroller);
    assert_eq!(
        after_pixels - after_notch,
        15.0,
        "a continuous (touchpad) delta scrolls by exactly its pixel amount"
    );
}

/// `#scroller`'s own scroll offset (not the document/root scroll: `#scroller` is a nested
/// `overflow:auto` box, so the wheel scrolls it, not the page).
fn scroll_y(doc: &DioxusDocument, scroller: NodeId) -> f64 {
    doc.inner
        .borrow()
        .get_node(scroller)
        .expect("scroller node exists")
        .scroll_offset()
        .y
}

fn wheel_event(delta: BlitzWheelDelta) -> BlitzWheelEvent {
    BlitzWheelEvent {
        delta,
        coords: PointerCoords {
            page_x: 0.0,
            page_y: 0.0,
            screen_x: 0.0,
            screen_y: 0.0,
            client_x: 0.0,
            client_y: 0.0,
        },
        buttons: MouseEventButtons::None,
        mods: Modifiers::empty(),
        element: Point::default(),
    }
}

fn default_axis() -> smithay_client_toolkit::seat::pointer::AxisScroll {
    smithay_client_toolkit::seat::pointer::AxisScroll::default()
}

fn axis_value120(value120: i32) -> smithay_client_toolkit::seat::pointer::AxisScroll {
    smithay_client_toolkit::seat::pointer::AxisScroll {
        value120,
        ..Default::default()
    }
}

fn axis_absolute(absolute: f64) -> smithay_client_toolkit::seat::pointer::AxisScroll {
    smithay_client_toolkit::seat::pointer::AxisScroll {
        absolute,
        ..Default::default()
    }
}

/// A press on an inactive toplevel is swallowed unless the element it lands on (or an
/// ancestor) carries `data-first-mouse` (design/13 §13.3.8). `input::pointer::first_mouse`
/// (already table-tested in `src/input/pointer.rs`) decides *whether* a press is in the
/// activating window; this checks the DOM-side half of the rule the handler applies the
/// decision with — the opt-in attribute walk `SurfaceDocument::wants_first_mouse` will do.
#[test]
fn the_activating_click_is_swallowed_unless_opted_in() {
    let doc = build();
    let drag = node_id(&doc, "#drag");
    let icon0 = node_id(&doc, "#icon0");

    assert!(
        has_first_mouse_opt_in(&doc, drag),
        "the data-first-mouse element opts in"
    );
    assert!(
        !has_first_mouse_opt_in(&doc, icon0),
        "an ordinary element does not opt in"
    );
}

/// Walks `id` and its ancestors looking for `data-first-mouse`, the same walk
/// `SurfaceDocument::wants_first_mouse` is specified to do.
///
/// `local_name!` (used elsewhere in `blitz_dom` for well-known HTML/ARIA names) only matches
/// names baked into its generated atom table at build time, which a `data-*` attribute is not
/// in, so this compares through `Attribute::name.local`'s `Deref<Target = str>` instead.
fn has_first_mouse_opt_in(doc: &DioxusDocument, id: NodeId) -> bool {
    let inner = doc.inner.borrow();
    let mut current = Some(id);
    while let Some(node_id) = current {
        let Some(node) = inner.get_node(node_id) else {
            break;
        };
        let opted_in = node
            .data
            .attrs()
            .is_some_and(|attrs| attrs.iter().any(|a| &*a.name.local == "data-first-mouse"));
        if opted_in {
            return true;
        }
        current = node.parent;
    }
    false
}

//! Pointer events reaching a real document: the move-before-down, leave and wheel rules from
//! `input::pointer`, applied to a live Blitz document.
//!
//! The documents are the host's own `SurfaceDocument`s (UA stylesheet included), fed the event
//! sequences `input::pointer` produces, as `handlers/pointer.rs` feeds them; the
//! `data-first-mouse` opt-in (design/13 §13.3.8) is asked of `SurfaceDocument::wants_first_mouse`.

use std::sync::Arc;

use blitz_dom::NodeId;
use blitz_traits::events::{
    BlitzWheelDelta, BlitzWheelEvent, MouseEventButton, MouseEventButtons, Point, PointerCoords,
    UiEvent,
};
use blitz_traits::shell::{ColorScheme, Viewport};
use dioxus::prelude::*;
use keyboard_types::Modifiers;
use shell_host::clipboard::SharedSelection;
use shell_host::dom::document::{DocumentEnv, FirstMouseOptIn, SurfaceDocument};
use shell_host::dom::fonts::SharedFonts;
use shell_host::dom::net::LocalNet;
use shell_host::dom::shell_provider::HostShellProvider;
use shell_host::id::SurfaceId;
use shell_host::input::pointer::{self, PointerPos};
use shell_host::root::Root;

/// The margin reset quire's `css/reset.css` brings; the host's UA sheet sizes the root
/// (SPIKE.md S1 finding 2) but leaves margins to the design system.
const RESET: &str = "html,body{margin:0}";

const CSS: &str = "
#dock{display:flex;gap:8px;padding:8px}
.icon{width:40px;height:40px;background:#456}
#scroller{width:120px;height:80px;overflow:auto}
#tall{height:600px}
#drag{width:60px;height:20px}
";

fn app() -> Element {
    rsx! {
        style { "{RESET}{CSS}" }
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

fn build() -> SurfaceDocument {
    let (ping, _source) = calloop::ping::make_ping().expect("eventfd ping");
    let env = DocumentEnv {
        shell: Arc::new(HostShellProvider::new(
            SurfaceId(1),
            ping,
            SharedSelection::default(),
        )),
        fonts: SharedFonts::system().for_document(),
        net: Arc::new(LocalNet),
        viewport: Viewport::new(400, 300, 1.0, ColorScheme::Light),
    };
    let mut doc = SurfaceDocument::build(Root::new(app), env);
    doc.resolve(0.0);
    doc
}

fn node_id(doc: &mut SurfaceDocument, selector: &str) -> NodeId {
    doc.dioxus()
        .inner
        .borrow()
        .query_selector(selector)
        .unwrap()
        .unwrap_or_else(|| panic!("no element matches {selector}"))
}

fn center(doc: &mut SurfaceDocument, id: NodeId) -> PointerPos {
    let rect = doc
        .dioxus()
        .inner
        .borrow()
        .get_client_bounding_rect(id)
        .expect("node is laid out");
    PointerPos {
        x: rect.x + rect.width / 2.0,
        y: rect.y + rect.height / 2.0,
    }
}

fn feed(doc: &mut SurfaceDocument, events: impl IntoIterator<Item = UiEvent>) {
    for event in events {
        doc.handle_event(event);
    }
}

/// With the UA stylesheet's sized root, a hover over an in-flow dock icon hovers the icon, not
/// some ancestor (SPIKE.md S1 finding 2).
#[test]
fn hover_reaches_in_flow_content() {
    let mut doc = build();
    let icon2 = node_id(&mut doc, "#icon2");
    let pos = center(&mut doc, icon2);

    feed(
        &mut doc,
        [pointer::move_event(
            pos,
            MouseEventButtons::None,
            Modifiers::empty(),
        )],
    );

    assert_eq!(doc.dioxus().inner.borrow().get_hover_node_id(), Some(icon2));
}

/// A press with no motion before it still clicks its target: `press_events`' synthesized move
/// sets the hover target before the down is dispatched.
#[test]
fn a_press_without_prior_motion_clicks_its_target() {
    let mut doc = build();
    let icon1 = node_id(&mut doc, "#icon1");
    let pos = center(&mut doc, icon1);

    // No PointerMove fed first: `press_events` supplies its own.
    let events = pointer::press_events(
        pos,
        MouseEventButton::Main,
        MouseEventButtons::None,
        Modifiers::empty(),
    );
    feed(&mut doc, events);

    assert_eq!(doc.dioxus().inner.borrow().get_hover_node_id(), Some(icon1));
    assert_eq!(
        doc.dioxus().inner.borrow().get_mousedown_node_id(),
        Some(icon1)
    );
}

/// Leaving clears `:hover`: the hovered element's hover state is gone after `clear_hover` and
/// the parking move to (-1, -1).
#[test]
fn leaving_clears_hover() {
    let mut doc = build();
    let icon0 = node_id(&mut doc, "#icon0");
    let pos = center(&mut doc, icon0);
    feed(
        &mut doc,
        [pointer::move_event(
            pos,
            MouseEventButtons::None,
            Modifiers::empty(),
        )],
    );
    assert_eq!(
        doc.dioxus().inner.borrow().get_hover_node_id(),
        Some(icon0),
        "precondition: icon0 is hovered before leaving"
    );

    doc.clear_hover();
    feed(&mut doc, [pointer::leave_event(Modifiers::empty())]);

    assert_eq!(doc.dioxus().inner.borrow().get_hover_node_id(), None);
}

/// One wheel notch (`Lines`) and a touchpad delta (`Pixels`) both scroll the hovered
/// container, by Blitz's line height (20 px, `blitz-dom`'s own `Lines * 20`) and by the raw
/// pixel amount respectively.
#[test]
fn wheel_notches_scroll_by_lines_and_touchpads_by_pixels() {
    let mut doc = build();
    let scroller = node_id(&mut doc, "#scroller");
    let pos = center(&mut doc, scroller);
    feed(
        &mut doc,
        [pointer::move_event(
            pos,
            MouseEventButtons::None,
            Modifiers::empty(),
        )],
    );

    let before = scroll_y(&mut doc, scroller);

    let one_notch_down = pointer::wheel_delta(default_axis(), axis_value120(120))
        .expect("one notch down produces a delta");
    doc.handle_event(UiEvent::Wheel(wheel_event(one_notch_down)));
    let after_notch = scroll_y(&mut doc, scroller);
    assert_eq!(
        after_notch - before,
        20.0,
        "one notch (value120 120, negated to -1 line) scrolls by exactly one line height"
    );

    let touchpad = pointer::wheel_delta(default_axis(), axis_absolute(15.0))
        .expect("a continuous delta produces pixels");
    doc.handle_event(UiEvent::Wheel(wheel_event(touchpad)));
    let after_pixels = scroll_y(&mut doc, scroller);
    assert_eq!(
        after_pixels - after_notch,
        15.0,
        "a continuous (touchpad) delta scrolls by exactly its pixel amount"
    );
}

/// `#scroller`'s own scroll offset (not the document/root scroll: `#scroller` is a nested
/// `overflow:auto` box, so the wheel scrolls it, not the page).
fn scroll_y(doc: &mut SurfaceDocument, scroller: NodeId) -> f64 {
    doc.dioxus()
        .inner
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
/// (table-tested in `src/input/pointer.rs`) decides *whether* a press is in the activating
/// window; this checks the DOM-side half the pointer handler asks the document.
#[test]
fn the_activating_click_is_swallowed_unless_opted_in() {
    let mut doc = build();
    let drag = node_id(&mut doc, "#drag");
    let icon0 = node_id(&mut doc, "#icon0");
    let (drag, icon0) = (center(&mut doc, drag), center(&mut doc, icon0));

    assert_eq!(
        doc.wants_first_mouse(drag.x, drag.y),
        FirstMouseOptIn::OptedIn,
        "the data-first-mouse element opts in"
    );
    assert_eq!(
        doc.wants_first_mouse(icon0.x, icon0.y),
        FirstMouseOptIn::Default,
        "an ordinary element does not opt in"
    );
}

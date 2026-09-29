//! Pointer events reaching a real document: the move-before-down, leave and wheel rules from
//! `input::pointer`, applied to a live Blitz document.
//!
//! The documents are the host's own `SurfaceDocument`s (UA stylesheet included), fed the event
//! sequences `input::pointer` produces, as `handlers/pointer.rs` feeds them; the
//! `data-first-mouse` opt-in (design/13 §13.3.8) is asked of `SurfaceDocument::wants_first_mouse`.

use crate::dom::document::{FirstMouseOptIn, Polled, SurfaceDocument};
use crate::dom::tests::env::test_env;
use crate::input::pointer::{self, PointerPos};
use crate::root::HostRoot;
use blitz_dom::NodeId;
use blitz_traits::events::{
    BlitzWheelDelta, BlitzWheelEvent, MouseEventButton, MouseEventButtons, Point, PointerCoords,
    UiEvent,
};
use dioxus::prelude::*;
use keyboard_types::Modifiers;

/// The margin reset quire's `css/reset.css` brings; the host's UA sheet sizes the root
/// but leaves margins to the design system.
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
    let env = test_env(400, 300);
    let mut doc = SurfaceDocument::build(HostRoot::new(app), env);
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
/// some ancestor.
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

    doc.pointer_left(Modifiers::empty());

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

fn leave_counter() -> Element {
    let mut left = use_signal(|| 0u32);
    rsx! {
        style { "{RESET}{CSS}" }
        div { id: "dock", onmouseleave: move |_| left += 1,
            div { id: "icon0", class: "icon" }
        }
        p { id: "left", "left {left}" }
    }
}

fn text_of(doc: &mut SurfaceDocument, selector: &str) -> String {
    let id = node_id(doc, selector);
    doc.dioxus()
        .inner
        .borrow()
        .get_node(id)
        .expect("node")
        .text_content()
}

/// Leaving the surface runs the hovered chain's `mouseleave` handlers, so a component can see
/// the hover end (the dock drops its `FrameDrive` there): the leave move goes first and
/// Blitz's own hover change dispatches them; `clear_hover()` after it finds nothing left.
#[test]
fn leaving_the_surface_runs_mouseleave_handlers() {
    let env = test_env(400, 300);
    let mut doc = SurfaceDocument::build(HostRoot::new(leave_counter), env);
    doc.resolve(0.0);
    let icon = node_id(&mut doc, "#icon0");
    let pos = center(&mut doc, icon);
    feed(
        &mut doc,
        [pointer::move_event(
            pos,
            MouseEventButtons::None,
            Modifiers::empty(),
        )],
    );
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    doc.poll(&mut cx);
    assert_eq!(text_of(&mut doc, "#left"), "left 0");

    let left = doc.pointer_left(Modifiers::empty());
    doc.poll(&mut cx);

    assert_eq!(left, Polled::Changed);
    assert_eq!(text_of(&mut doc, "#left"), "left 1");
    assert_eq!(doc.dioxus().inner.borrow().get_hover_node_id(), None);
}

fn context_menu_probe() -> Element {
    let mut clicks = use_signal(|| 0u32);
    let mut menus = use_signal(|| 0u32);
    rsx! {
        style { "{RESET}{CSS}" }
        div {
            id: "icon0",
            class: "icon",
            onclick: move |_| clicks += 1,
            oncontextmenu: move |_| menus += 1,
        }
        p { id: "clicks", "clicks {clicks}" }
        p { id: "menus", "menus {menus}" }
    }
}

/// `Inject::Click { button: PointerButton::Secondary }` (`inject::pressed`/`released`, built
/// from the same `press_events`/`release_event` this test drives directly) reaches the document
/// as Blitz's own `contextmenu` event, not a `click` (`blitz-dom` `handle_pointerup`: a release
/// of `MouseEventButton::Secondary` dispatches `DomEventData::ContextMenu` instead of `Click`),
/// which Dioxus delivers to `oncontextmenu`. A primary release is the reverse: `onclick` only.
#[test]
fn a_secondary_click_reaches_oncontextmenu_and_a_primary_click_reaches_onclick() {
    let env = test_env(400, 300);
    let mut doc = SurfaceDocument::build(HostRoot::new(context_menu_probe), env);
    doc.resolve(0.0);
    let icon = node_id(&mut doc, "#icon0");
    let pos = center(&mut doc, icon);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());

    let secondary = pointer::press_events(
        pos,
        MouseEventButton::Secondary,
        MouseEventButtons::None,
        Modifiers::empty(),
    );
    feed(&mut doc, secondary);
    feed(
        &mut doc,
        [pointer::release_event(
            pos,
            MouseEventButton::Secondary,
            MouseEventButtons::Secondary,
            Modifiers::empty(),
        )],
    );
    doc.poll(&mut cx);
    assert_eq!(text_of(&mut doc, "#clicks"), "clicks 0");
    assert_eq!(text_of(&mut doc, "#menus"), "menus 1");

    let primary = pointer::press_events(
        pos,
        MouseEventButton::Main,
        MouseEventButtons::None,
        Modifiers::empty(),
    );
    feed(&mut doc, primary);
    feed(
        &mut doc,
        [pointer::release_event(
            pos,
            MouseEventButton::Main,
            MouseEventButtons::Primary,
            Modifiers::empty(),
        )],
    );
    doc.poll(&mut cx);
    assert_eq!(text_of(&mut doc, "#clicks"), "clicks 1");
    assert_eq!(
        text_of(&mut doc, "#menus"),
        "menus 1",
        "unchanged by the primary click"
    );
}

/// A press, a held `PointerMove` and a release: the sequence a scripted drag needs
/// (`Inject::PointerDown`, one or more `Inject::Pointer` while the button stays down, then
/// `Inject::PointerUp`; a whole `Inject::Click` has no move in
/// between to give it) built from the exact same `input::pointer` functions `handlers/pointer.rs`
/// calls for a real press, motion and release. A move that stays within Blitz's own 2px drag
/// threshold (`blitz-dom`'s `handle_pointermove`, checked against `mousedown_position`) still
/// fires `onclick` on release; a move past it does not — `blitz-dom`'s `handle_pointerup` only
/// dispatches `Click` when `drag_mode == DragMode::None`, and crossing the threshold with a
/// button held sets `DragMode::Selecting` (the icon and its dock ancestor both compute
/// `user-select: auto`, the default; `blitz-dom`'s stylesheet has no rule for `div`). This is
/// the drag/no-drag split a scripted drag must reach, and it depends on the held `PointerMove`
/// carrying `buttons: Primary` (a plain hover, `MouseEventButtons::None`, could never trip it).
#[test]
fn a_move_past_the_threshold_between_press_and_release_suppresses_the_click() {
    let press_move_release = |dx: f64, dy: f64| -> String {
        let env = test_env(400, 300);
        let mut doc = SurfaceDocument::build(HostRoot::new(context_menu_probe), env);
        doc.resolve(0.0);
        let icon = node_id(&mut doc, "#icon0");
        let start = center(&mut doc, icon);
        let moved_to = PointerPos {
            x: start.x + dx,
            y: start.y + dy,
        };

        // The press: a move-before-down (no buttons yet) then the down (the pointer's own
        // `Inject::PointerDown`/`inject::pressed`).
        feed(
            &mut doc,
            pointer::press_events(
                start,
                MouseEventButton::Main,
                MouseEventButtons::None,
                Modifiers::empty(),
            ),
        );
        // The held move: `Inject::Pointer`/`inject::moved` between the press and the release,
        // carrying the button a real `Motion` would still see held.
        feed(
            &mut doc,
            [pointer::move_event(
                moved_to,
                MouseEventButtons::Primary,
                Modifiers::empty(),
            )],
        );
        // The release: `Inject::PointerUp`/`inject::released`.
        feed(
            &mut doc,
            [pointer::release_event(
                moved_to,
                MouseEventButton::Main,
                MouseEventButtons::Primary,
                Modifiers::empty(),
            )],
        );
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        doc.poll(&mut cx);
        text_of(&mut doc, "#clicks")
    };

    assert_eq!(
        press_move_release(1.0, 0.0),
        "clicks 1",
        "a move within the 2px threshold still clicks on release"
    );
    assert_eq!(
        press_move_release(10.0, 0.0),
        "clicks 0",
        "a move past the threshold starts a drag, so the release no longer clicks"
    );
}

//! design/11 §11.8 acceptance on a real document: `SurfaceScroll` driving a laid-out
//! `SurfaceDocument` through `dom::scroll`, with no compositor. Latching (item 9), no rubber
//! band without the opt-in (item 8), the thumb (item 10), keys in a text input (item 11),
//! natural direction (item 15), `data-wheel="capture"`, and the raw offsets surviving
//! Blitz's resolve. (`use_scroll` is tested beside the surface builder, `ctx::spawn`.)

use std::task::Context;
use std::time::Duration;

use dioxus::prelude::*;

use crate::dom::document::SurfaceDocument;
use crate::dom::scroll::{self as dom, KeyFocus, WheelRoute};
use crate::dom::tests::env::test_env;
use crate::input::key::Repeat;
use crate::input::pointer::PointerPos;
use crate::region::ElementId;
use crate::root::HostRoot;
use crate::scroll::engine::{Engine, Motion};
use crate::scroll::geom::{Dir, Elastic, ScrollAxis, Scroller};
use crate::scroll::keys::ScrollKey;
use crate::scroll::source::{AxisFrame, AxisSource, AxisValue, Shift, Stop};
use crate::scroll::surface::{KeyUse, PalmScroll, SurfaceScroll};
use crate::scroll::tuning::Tuning;
use crate::scroll::velocity::SampleTime;
use crate::scroll::watch::ScrollBoard;
use crate::tick::HostInstant;

const WIDTH: u32 = 400;
const HEIGHT: u32 = 600;
const FRAME: Duration = Duration::from_micros(8333);

/// A 2000 px page with a banded 300 px list (1000 px of rows), a rigid 200 px menu list, a
/// capturing slider and a text input.
fn page() -> Element {
    rsx! {
        style { "#main{{height:auto}} body{{margin:0}}" }
        div { id: "page", style: "height:2000px;position:relative",
            div {
                id: "list",
                "data-overscroll": "band",
                style: "position:absolute;left:0;top:100px;width:300px;height:300px;overflow:auto",
                div { style: "height:1000px", "rows" }
            }
            div {
                id: "menu",
                style: "position:absolute;left:0;top:450px;width:300px;height:100px;overflow:auto",
                div { style: "height:400px", "items" }
            }
            div {
                id: "slider",
                "data-wheel": "capture",
                style: "position:absolute;left:320px;top:100px;width:60px;height:60px",
            }
            input { id: "field", style: "position:absolute;left:320px;top:200px;width:60px" }
        }
    }
}

fn document(app: fn() -> Element) -> SurfaceDocument {
    let env = test_env(WIDTH, HEIGHT);
    let mut doc = SurfaceDocument::build(HostRoot::new(app), env);
    doc.poll(&mut Context::from_waker(std::task::Waker::noop()));
    doc.resolve(0.0);
    doc
}

/// A document, its scrolling and a clock.
struct Bench {
    doc: SurfaceDocument,
    scroll: SurfaceScroll,
    now: HostInstant,
}

impl Bench {
    fn new() -> Bench {
        Bench::with(Tuning::default())
    }

    fn with(tuning: Tuning) -> Bench {
        Bench {
            doc: document(page),
            scroll: SurfaceScroll::new(ScrollBoard::default(), tuning),
            now: HostInstant(Duration::from_secs(10)),
        }
    }

    fn scroller(&self, id: &str) -> Scroller {
        dom::scroller_by_id(&self.doc.base(), &ElementId::new(id)).expect("laid out")
    }

    fn offset(&self, scroller: Scroller) -> f64 {
        dom::geom(&self.doc.base(), scroller, ScrollAxis::Y)
            .expect("geom")
            .offset
            .0
    }

    fn max(&self, scroller: Scroller) -> f64 {
        dom::geom(&self.doc.base(), scroller, ScrollAxis::Y)
            .expect("geom")
            .max
            .0
    }

    fn set(&mut self, scroller: Scroller, offset: f64) {
        dom::write(&mut self.doc.base_mut(), scroller, ScrollAxis::Y, offset);
    }

    /// One touchpad frame at `at`: `dy` px (offset space), or `axis_stop`.
    fn pad(&mut self, dy: f64, stop: Stop, at: PointerPos) {
        let frame = AxisFrame {
            source: AxisSource::Finger,
            horizontal: AxisValue::NONE,
            vertical: AxisValue {
                px: dy,
                stop,
                ..AxisValue::NONE
            },
            shift: Shift::Up,
        };
        self.now = HostInstant(self.now.0 + FRAME);
        let mut base = self.doc.base_mut();
        self.scroll.axis(&mut base, &frame, at, self.now);
    }

    /// A 120 Hz touchpad stroke of `speed` px/s for `ms`, then the lift.
    fn stroke(&mut self, speed: f64, ms: u64, at: PointerPos) {
        for _ in 0..(ms * 120 / 1000) {
            self.pad(speed * FRAME.as_secs_f64(), Stop::Moving, at);
        }
        self.pad(0.0, Stop::Stopped, at);
    }

    /// Frames (each with a resolve) until nothing animates; every scroller's offsets after
    /// each, for `watch`.
    fn settle(&mut self, watch: &[Scroller]) -> Vec<Vec<f64>> {
        let mut trace = Vec::new();
        for _ in 0..600 {
            if self.scroll.motion(self.now) == Motion::Still {
                break;
            }
            self.now = HostInstant(self.now.0 + FRAME);
            {
                let mut base = self.doc.base_mut();
                self.scroll.frame(&mut base, self.now);
            }
            self.doc.resolve(self.now.0.as_secs_f64());
            trace.push(watch.iter().map(|s| self.offset(*s)).collect());
        }
        trace
    }
}

const OVER_LIST: PointerPos = PointerPos { x: 150.0, y: 250.0 };
const OVER_MENU: PointerPos = PointerPos { x: 150.0, y: 500.0 };
const OVER_PAGE: PointerPos = PointerPos { x: 350.0, y: 500.0 };

#[test]
fn raw_offsets_survive_blitz_resolve_even_out_of_range() {
    let mut bench = Bench::new();
    let list = bench.scroller("list");
    assert_eq!(bench.max(list), 700.0);
    assert_eq!(bench.max(Scroller::Viewport), 1400.0);
    for (scroller, raw) in [(list, -40.0), (list, 740.0), (Scroller::Viewport, 1450.0)] {
        bench.set(scroller, raw);
        bench.doc.resolve(1.0);
        bench.doc.resolve(2.0);
        assert_eq!(bench.offset(scroller), raw, "{scroller:?}");
    }
}

#[test]
fn only_an_opted_in_container_is_elastic() {
    let bench = Bench::new();
    let geom =
        |id: &str| dom::geom(&bench.doc.base(), bench.scroller(id), ScrollAxis::Y).expect("geom");
    assert_eq!(geom("list").elastic, Elastic::Elastic);
    assert_eq!(geom("menu").elastic, Elastic::Rigid);
    let viewport = dom::geom(&bench.doc.base(), Scroller::Viewport, ScrollAxis::Y).expect("geom");
    assert_eq!(
        viewport.elastic,
        Elastic::Rigid,
        "the viewport is not opted in"
    );
    let chain = dom::chain_at(&bench.doc.base(), OVER_LIST.x, OVER_LIST.y, ScrollAxis::Y);
    let order: Vec<Scroller> = chain.iter().map(|c| c.scroller).collect();
    assert_eq!(order, vec![bench.scroller("list"), Scroller::Viewport]);
}

#[test]
fn item_8_the_menu_list_never_stretches() {
    let mut bench = Bench::new();
    let menu = bench.scroller("menu");
    // 500 px of pull past the top, in 10 px steps.
    for _ in 0..50 {
        bench.pad(-10.0, Stop::Moving, OVER_MENU);
        assert_eq!(bench.offset(menu), 0.0);
    }
    bench.pad(0.0, Stop::Stopped, OVER_MENU);
    for offsets in bench.settle(&[menu]) {
        assert_eq!(offsets[0], 0.0);
    }
}

#[test]
fn item_9a_an_inner_list_at_its_bottom_lets_the_page_scroll() {
    let mut bench = Bench::new();
    let list = bench.scroller("list");
    bench.set(list, 700.0);
    bench.stroke(1500.0, 100, OVER_LIST);
    let trace = bench.settle(&[list, Scroller::Viewport]);
    assert!(bench.offset(Scroller::Viewport) > 100.0, "the page moved");
    assert!(trace.iter().all(|o| o[0] == 700.0), "the list never moved");
}

#[test]
fn item_9b_an_inner_list_midway_takes_the_whole_gesture_and_stretches() {
    let mut bench = Bench::new();
    let list = bench.scroller("list");
    bench.set(list, 400.0);
    // 450 px down in 150 ms: past the list's bottom (300 px away) while the fingers are down.
    for _ in 0..18 {
        bench.pad(25.0, Stop::Moving, OVER_LIST);
        assert_eq!(
            bench.offset(Scroller::Viewport),
            0.0,
            "the page holds still"
        );
    }
    assert!(
        bench.offset(list) > 700.0,
        "stretched: {}",
        bench.offset(list)
    );
    bench.pad(0.0, Stop::Stopped, OVER_LIST);
    let trace = bench.settle(&[list, Scroller::Viewport]);
    assert!(trace.iter().all(|o| o[1] == 0.0), "the page never moved");
    assert_eq!(bench.offset(list), 700.0, "rebounded to its edge");
}

#[test]
fn item_10_the_thumb_shows_on_the_change_and_fades_after_740_ms() {
    let mut bench = Bench::new();
    bench.pad(15.0, Stop::Moving, OVER_LIST);
    bench.pad(15.0, Stop::Moving, OVER_LIST);
    let changed = bench.now;
    let thumbs = bench.scroll.overlay(&bench.doc.base(), changed);
    assert_eq!(thumbs.len(), 1);
    assert_eq!(thumbs[0].opacity, 1.0);
    // The list's thumb, on its right edge: x = 300 - 2 - 8.
    assert_eq!(thumbs[0].thumb.x, 290.0);
    let later = |ms| HostInstant(changed.0 + Duration::from_millis(ms));
    assert_eq!(
        bench.scroll.overlay(&bench.doc.base(), later(499))[0].opacity,
        1.0
    );
    let fading = bench.scroll.overlay(&bench.doc.base(), later(620))[0].opacity;
    assert!(fading > 0.0 && fading < 1.0, "{fading}");
    assert!(
        bench
            .scroll
            .overlay(&bench.doc.base(), later(741))
            .is_empty()
    );
}

#[test]
fn item_11_keys_in_a_text_input_do_not_scroll() {
    let mut bench = Bench::new();
    let field = bench.doc.base().get_element_by_id("field").expect("input");
    bench.doc.base_mut().set_focus_to(field);
    assert_eq!(
        dom::key_focus(&bench.doc.base(), ScrollAxis::Y),
        KeyFocus::Document
    );
    let key = ScrollKey::Page(Dir::Pos);
    let used = {
        let mut base = bench.doc.base_mut();
        bench
            .scroll
            .key(&mut base, key, Repeat::First, Some(OVER_PAGE), bench.now)
    };
    assert_eq!(used, KeyUse::Passed);
    assert_eq!(bench.offset(Scroller::Viewport), 0.0);
}

#[test]
fn item_11_page_down_over_the_page_scrolls_it() {
    let mut bench = Bench::new();
    let used = {
        let mut base = bench.doc.base_mut();
        bench.scroll.key(
            &mut base,
            ScrollKey::Page(Dir::Pos),
            Repeat::First,
            Some(OVER_PAGE),
            bench.now,
        )
    };
    assert!(matches!(used, KeyUse::Scrolled(_)));
    bench.settle(&[Scroller::Viewport]);
    // A 600 px view pages by max(480, 560).
    assert_eq!(bench.offset(Scroller::Viewport), 560.0);
}

#[test]
fn item_15_natural_palmrest_content_follows_the_finger() {
    for (natural, want_down) in [
        (crate::NaturalScroll::Natural, false),
        (crate::NaturalScroll::Traditional, true),
    ] {
        let mut config = crate::ScrollConfig::default();
        config.scroll.natural = natural;
        let mut bench = Bench::with(Tuning::new(config));
        bench.set(Scroller::Viewport, 500.0);
        let t = |ms: u64| SampleTime(Duration::from_millis(ms));
        let feed = |bench: &mut Bench, event| {
            let mut base = bench.doc.base_mut();
            bench
                .scroll
                .palmrest(&mut base, event, OVER_PAGE, bench.now);
        };
        feed(&mut bench, PalmScroll::MayBegin);
        feed(
            &mut bench,
            PalmScroll::Began {
                axis: ScrollAxis::Y,
            },
        );
        for i in 0..5 {
            // The finger moves toward the user: +y.
            feed(
                &mut bench,
                PalmScroll::Changed {
                    dx: 0.0,
                    dy: 10.0,
                    at: t(i * 8),
                },
            );
        }
        let moved = bench.offset(Scroller::Viewport) - 500.0;
        assert_eq!(moved > 0.0, want_down, "{natural:?}: {moved}");
        assert_eq!(moved.abs(), 50.0, "{natural:?}");
    }
}

#[test]
fn a_capturing_element_gets_the_raw_wheel() {
    let bench = Bench::new();
    assert_eq!(
        dom::wheel_route(&bench.doc.base(), 350.0, 130.0),
        WheelRoute::Capture
    );
    assert_eq!(
        dom::wheel_route(&bench.doc.base(), 150.0, 250.0),
        WheelRoute::Engine
    );
}

#[test]
fn a_wheel_detent_over_the_list_moves_it_60_px() {
    let mut bench = Bench::new();
    let list = bench.scroller("list");
    let frame = AxisFrame {
        source: AxisSource::Wheel,
        horizontal: AxisValue::NONE,
        vertical: AxisValue {
            px: 15.0,
            v120: 120,
            ..AxisValue::NONE
        },
        shift: Shift::Up,
    };
    {
        let mut base = bench.doc.base_mut();
        bench.scroll.axis(&mut base, &frame, OVER_LIST, bench.now);
    }
    assert!(matches!(bench.scroll.engine(), Engine::Smooth(_)));
    bench.settle(&[list]);
    assert_eq!(bench.offset(list), 60.0);
    assert_eq!(bench.offset(Scroller::Viewport), 0.0);
}

/// Shapes of focus that must keep their keys, and ones that must scroll: a ds-style menu
/// (`role="menu"` with `onkeydown`) inside a scroller; sill's notification column (a focused
/// scroller with its own `onkeydown`); sill's power menu (a focused root with `onkeydown` over
/// a banded sheet); a focusable scroller with no listener; and a wheel-reading element that is
/// not a scroll container.
fn keyed() -> Element {
    rsx! {
        style { "#main{{height:auto}} body{{margin:0}} .box{{position:absolute;width:200px;height:100px;overflow:auto}}" }
        div { id: "menuwrap", class: "box", style: "left:0;top:0",
            div { id: "menu", role: "menu", tabindex: "0", onkeydown: |_| {}, style: "height:400px", "items" }
        }
        div { id: "column", class: "box", style: "left:0;top:120px", tabindex: "0", onkeydown: |_| {},
            div { style: "height:400px", "groups" }
        }
        div { id: "power", style: "position:absolute;left:0;top:240px;width:200px;height:120px", tabindex: "0", onkeydown: |_| {},
            div { id: "sheet", class: "box", "data-overscroll": "band", style: "left:0;top:0",
                div { style: "width:600px;height:400px", "sheet" }
            }
        }
        div { id: "plain", class: "box", style: "left:220px;top:0", tabindex: "0",
            div { style: "height:400px", "rows" }
        }
        div {
            id: "tray",
            "data-wheel": "capture",
            style: "position:absolute;left:220px;top:120px;width:40px;height:40px",
            onwheel: |event: WheelEvent| {
                let y = event.delta().strip_units().y;
                WHEEL_Y.store(y.to_bits(), std::sync::atomic::Ordering::SeqCst);
            },
        }
    }
}

static WHEEL_Y: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn keyed_bench() -> Bench {
    Bench {
        doc: document(keyed),
        scroll: SurfaceScroll::new(ScrollBoard::default(), Tuning::default()),
        now: HostInstant(Duration::from_secs(10)),
    }
}

impl Bench {
    fn focus(&mut self, id: &str) {
        let node = self.doc.base().get_element_by_id(id).expect("element");
        self.doc.base_mut().set_focus_to(node);
    }

    fn press(&mut self, key: ScrollKey, at: Option<PointerPos>) -> KeyUse {
        let mut base = self.doc.base_mut();
        self.scroll.key(&mut base, key, Repeat::First, at, self.now)
    }
}

#[test]
fn a_menu_that_handles_keys_keeps_arrow_down() {
    let mut bench = keyed_bench();
    bench.focus("menu");
    let wrap = bench.scroller("menuwrap");
    assert_eq!(
        bench.press(ScrollKey::Line(ScrollAxis::Y, Dir::Pos), None),
        KeyUse::Passed
    );
    bench.settle(&[wrap]);
    assert_eq!(bench.offset(wrap), 0.0);
}

#[test]
fn a_focused_scroller_with_its_own_keydown_keeps_down() {
    let mut bench = keyed_bench();
    bench.focus("column");
    assert_eq!(
        bench.press(ScrollKey::Line(ScrollAxis::Y, Dir::Pos), None),
        KeyUse::Passed
    );
    assert_eq!(bench.offset(bench.scroller("column")), 0.0);
}

#[test]
fn a_focused_root_with_keydown_keeps_left_and_right_over_a_banded_sheet() {
    let mut bench = keyed_bench();
    bench.focus("power");
    for dir in [Dir::Pos, Dir::Neg] {
        assert_eq!(
            bench.press(ScrollKey::Line(ScrollAxis::X, dir), None),
            KeyUse::Passed
        );
    }
    let sheet = bench.scroller("sheet");
    assert_eq!(
        dom::geom(&bench.doc.base(), sheet, ScrollAxis::X)
            .expect("geom")
            .offset
            .0,
        0.0
    );
}

#[test]
fn a_plain_scroller_scrolls_with_focus_on_it_or_on_nothing() {
    // Nothing focused (the body): the scroller under the pointer.
    let mut bench = keyed_bench();
    let plain = bench.scroller("plain");
    let over = Some(PointerPos { x: 300.0, y: 50.0 });
    assert!(matches!(
        bench.press(ScrollKey::Line(ScrollAxis::Y, Dir::Pos), over),
        KeyUse::Scrolled(_)
    ));
    bench.settle(&[plain]);
    assert_eq!(bench.offset(plain), 40.0);
    // Focused, with no listener on its path: the focused scroller, wherever the pointer is.
    bench.focus("plain");
    assert!(matches!(
        bench.press(ScrollKey::Line(ScrollAxis::Y, Dir::Pos), None),
        KeyUse::Scrolled(_)
    ));
    bench.settle(&[plain]);
    assert_eq!(bench.offset(plain), 80.0);
    // Nothing focused and nothing scrollable under the pointer: the document's key. (Between
    // the boxes: Blitz's hit test does not clip a scroller's overflow, so a point under a
    // scroller's hidden content hits it.)
    let nowhere = Some(PointerPos { x: 210.0, y: 110.0 });
    let mut fresh = keyed_bench();
    assert_eq!(
        fresh.press(ScrollKey::Line(ScrollAxis::Y, Dir::Pos), nowhere),
        KeyUse::Passed
    );
}

#[test]
fn a_capturing_element_that_is_not_a_scroller_gets_the_wheel_as_before() {
    use smithay_client_toolkit::seat::pointer::AxisScroll;
    let mut bench = keyed_bench();
    let at = PointerPos { x: 240.0, y: 140.0 };
    assert_eq!(
        dom::wheel_route(&bench.doc.base(), at.x, at.y),
        WheelRoute::Capture
    );
    // One notch down, as a compositor sends it: the delta Blitz was fed before the engine.
    let notch = AxisScroll {
        absolute: 15.0,
        value120: 120,
        discrete: 1,
        ..AxisScroll::default()
    };
    let delta = crate::input::pointer::wheel_delta(AxisScroll::default(), notch).expect("a delta");
    let event = crate::input::pointer::wheel_event(
        delta,
        at,
        blitz_traits::events::MouseEventButtons::None,
        keyboard_types::Modifiers::empty(),
    );
    // The host always has the pointer there first (an enter or a motion): Blitz targets a
    // wheel at the hovered element.
    bench.doc.handle_event(crate::input::pointer::move_event(
        at,
        blitz_traits::events::MouseEventButtons::None,
        keyboard_types::Modifiers::empty(),
    ));
    bench.doc.handle_event(event);
    let y = f64::from_bits(WHEEL_Y.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(y, -1.0, "one line, the sign Blitz always had");
}

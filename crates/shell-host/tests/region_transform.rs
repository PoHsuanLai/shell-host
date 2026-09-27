//! sill F1010 / G410 (FINDINGS F73): an element region follows where the element paints, not
//! its layout box. A box moved by a CSS `transform` (quire's swipe `--swipe-dx`, its
//! `banner-out` keyframes) resolved to its untransformed rect, so a blur region on it stayed
//! behind as a grey rectangle while the card was dragged or slid out.

use std::sync::Arc;
use std::task::Context;

use blitz_traits::shell::{ColorScheme, Viewport};
use dioxus::prelude::*;

use shell_host::clipboard::SharedSelection;
use shell_host::dom::document::{DocumentEnv, SurfaceDocument};
use shell_host::dom::fonts::SharedFonts;
use shell_host::dom::net::LocalNet;
use shell_host::dom::shell_provider::HostShellProvider;
use shell_host::id::SurfaceId;
use shell_host::region::{ElementId, Rect};
use shell_host::root::Root;

fn document(root: Root) -> SurfaceDocument {
    let (ping, _source) = calloop::ping::make_ping().expect("eventfd ping");
    let env = DocumentEnv {
        shell: Arc::new(HostShellProvider::new(
            SurfaceId(1),
            ping,
            SharedSelection::default(),
        )),
        fonts: SharedFonts::system().for_document(),
        net: Arc::new(LocalNet),
        viewport: Viewport::new(300, 100, 1.0, ColorScheme::Light),
        prepaint: shell_host::Prepaint::None,
    };
    let mut doc = SurfaceDocument::build(root, env);
    doc.poll(&mut Context::from_waker(std::task::Waker::noop()));
    doc
}

const fn rect(x: i32, y: i32, width: i32, height: i32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn moved() -> Element {
    rsx! {
        style { "html,body{{margin:0}}" }
        div { style: "transform:translateX(60px)",
            div { id: "card", style: "width:100px;height:40px" }
        }
    }
}

#[test]
fn a_transformed_elements_region_is_where_it_paints() {
    let mut doc = document(Root::new(moved));
    doc.resolve(0.0);
    assert_eq!(
        doc.element_rect(ElementId::new("card")),
        Some(rect(60, 0, 100, 40))
    );
}

fn scaled() -> Element {
    rsx! {
        style { "html,body{{margin:0}}" }
        div { style: "margin-left:20px;width:200px;transform:scale(0.5);transform-origin:0 0",
            div { id: "card", style: "margin-left:40px;width:100px;height:40px;transform:translateY(10px)" }
        }
    }
}

/// A parent's scale moves and shrinks a child that is itself translated: the child's own
/// translation is scaled by the parent's, about the parent's origin.
#[test]
fn a_parents_scale_and_a_childs_translation_compose() {
    let mut doc = document(Root::new(scaled));
    doc.resolve(0.0);
    // Child laid out at (60, 0) 100x40; its translateY(10) -> (60, 10); the parent's
    // scale(0.5) about (20, 0) -> (40, 5) 50x20.
    assert_eq!(
        doc.element_rect(ElementId::new("card")),
        Some(rect(40, 5, 50, 20))
    );
}

fn sliding() -> Element {
    rsx! {
        style {
            "html,body{{margin:0}}
            @keyframes out {{ from {{ transform: translateX(0px) }} to {{ transform: translateX(200px) }} }}
            #card {{ width:100px; height:40px; animation: out 1s linear forwards }}"
        }
        div { id: "card" }
    }
}

/// A region follows a running animation frame by frame: every rendered frame resolves the
/// document at that frame's time and then resolves regions from `element_rect`
/// (`ctx/paint.rs::render`), so the rect read after each resolve is the one the frame commits.
#[test]
fn the_rect_follows_an_animated_transform_every_frame() {
    let mut doc = document(Root::new(sliding));
    let mut seen = Vec::new();
    for time in [0.0, 0.25, 0.5, 1.0, 2.0] {
        doc.resolve(time);
        let card = doc
            .element_rect(ElementId::new("card"))
            .expect("the card is laid out"); // test-only
        seen.push(card.x);
        assert_eq!((card.width, card.height), (100, 40), "at {time}s");
    }
    assert_eq!(seen[0], 0, "starts where it was laid out: {seen:?}");
    assert!(
        seen.windows(2).all(|pair| pair[0] <= pair[1]) && seen[2] > seen[0],
        "moves with the animation: {seen:?}"
    );
    assert_eq!(seen[4], 200, "ends where the animation leaves it: {seen:?}");
}

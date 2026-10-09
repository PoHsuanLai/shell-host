//! The host's body reset: Blitz's default stylesheet gives `<body>` an 8 px
//! margin, and the host zeroes it with a user-agent sheet, so a bare document (no quire root,
//! no stylesheet of its own) reaches its surface's (0, 0) corner, while a document's own body
//! rule still wins.
//!
//! The reset fixes the corner, not the height: an `inset:0` box is placed against its parent,
//! dioxus-native-dom's `height:auto` `#main`, which an all-positioned document leaves 0 px tall
//! (quire's `RootExtent::Viewport`). A box sized in viewport units covers it all.

use std::task::Context;

use dioxus::prelude::*;

use crate::dom::document::SurfaceDocument;
use crate::dom::tests::env::test_env;
use crate::region::{ElementId, Rect};
use crate::root::HostRoot;

const WIDTH: u32 = 120;
const HEIGHT: u32 = 80;

/// A bare document: one `inset:0` box and no stylesheet at all.
fn bare_inset() -> Element {
    rsx! {
        div { id: "fill", style: "position:absolute;inset:0" }
    }
}

/// A bare document whose box claims the viewport itself.
fn bare_viewport() -> Element {
    rsx! {
        div { id: "fill", style: "width:100vw;height:100vh" }
    }
}

/// A document that sets its own body margin; the box sits where that margin puts it.
fn own_margin() -> Element {
    rsx! {
        style { "body{{margin:4px}}" }
        div { id: "fill", style: "height:10px" }
    }
}

fn resolved(app: fn() -> Element) -> SurfaceDocument {
    let env = test_env(WIDTH, HEIGHT);
    let mut doc = SurfaceDocument::build(HostRoot::new(app), env);
    doc.poll(&mut Context::from_waker(std::task::Waker::noop()));
    doc.resolve(0.0);
    doc
}

fn fill_rect(doc: &SurfaceDocument) -> Rect {
    doc.element_rect(ElementId::new("fill"))
        .expect("#fill is laid out")
}

/// name, document, (x, y), width, height (None: not pinned)
type Case = (&'static str, fn() -> Element, (i32, i32), i32, Option<i32>);

#[test]
fn the_body_reset_puts_a_box_at_the_corner_unless_the_document_sets_its_own_margin() {
    let cases: &[Case] = &[
        (
            // `#main` is as tall as its flow content, none here: unchanged by the reset.
            "a bare document's inset-zero box starts at the corner and spans the width",
            bare_inset,
            (0, 0),
            WIDTH as i32,
            Some(0),
        ),
        (
            "a bare document's viewport box covers the whole surface",
            bare_viewport,
            (0, 0),
            WIDTH as i32,
            Some(HEIGHT as i32),
        ),
        (
            "a document's own body margin still wins",
            own_margin,
            (4, 4),
            WIDTH as i32 - 8,
            None,
        ),
    ];
    for &(name, app, origin, width, height) in cases {
        let rect = fill_rect(&resolved(app));
        assert_eq!((rect.x, rect.y), origin, "{name}: origin");
        assert_eq!(rect.width, width, "{name}: width");
        if let Some(height) = height {
            assert_eq!(rect.height, height, "{name}: height");
        }
    }
}

/// The headless path gets the same reset: a bare viewport-sized red box paints both corners.
#[cfg(feature = "headless")]
#[test]
fn a_headless_bare_document_paints_its_corner() {
    use blitz_traits::shell::ColorScheme;

    use crate::Scale;
    use crate::headless::{Fonts, HeadlessConfig, PixelSize, bundled_faces, render_png};

    fn red() -> Element {
        rsx! {
            div { style: "width:100vw;height:100vh;background:#ff0000" }
        }
    }
    let images = render_png(
        red,
        HeadlessConfig {
            size: PixelSize::new(WIDTH, HEIGHT),
            scale: Scale::ONE,
            color_scheme: ColorScheme::Light,
            at_seconds: vec![0.0],
            fonts: Fonts::Bundled(bundled_faces()),
        },
    );
    let image = &images[0];
    assert_eq!(image.get_pixel(0, 0).0, [255, 0, 0, 255]);
    assert_eq!(image.get_pixel(WIDTH - 1, HEIGHT - 1).0, [255, 0, 0, 255]);
}

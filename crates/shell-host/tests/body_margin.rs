//! The host's body reset: Blitz's default stylesheet gives `<body>` an 8 px
//! margin, and the host zeroes it with a user-agent sheet, so a bare document (no quire root,
//! no stylesheet of its own) reaches its surface's (0, 0) corner, while a document's own body
//! rule still wins.
//!
//! The reset fixes the corner, not the height: an `inset:0` box is placed against its parent,
//! dioxus-native-dom's `height:auto` `#main`, which an all-positioned document leaves 0 px tall
//! (quire's `RootExtent::Viewport`). A box sized in viewport units covers it all.

use std::sync::Arc;
use std::task::Context;

use blitz_traits::shell::{ColorScheme, Viewport};
use dioxus::prelude::*;

use blitz_kit::fonts::SharedFonts;
use blitz_kit::net::LocalNet;
use shell_host::clipboard::SharedSelection;
use shell_host::dom::document::{DocumentEnv, SurfaceDocument};
use shell_host::dom::shell_provider::HostShellProvider;
use shell_host::id::SurfaceId;
use shell_host::region::{ElementId, Rect};
use shell_host::root::HostRoot;

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
    let (ping, _source) = calloop::ping::make_ping().expect("eventfd ping");
    let env = DocumentEnv {
        shell: Arc::new(HostShellProvider::new(
            SurfaceId(1),
            ping,
            SharedSelection::default(),
        )),
        fonts: SharedFonts::system().for_document(),
        net: Arc::new(LocalNet),
        viewport: Viewport::new(WIDTH, HEIGHT, 1.0, ColorScheme::Light),
        prepaint: shell_host::Prepaint::None,
    };
    let mut doc = SurfaceDocument::build(HostRoot::new(app), env);
    doc.poll(&mut Context::from_waker(std::task::Waker::noop()));
    doc.resolve(0.0);
    doc
}

fn fill_rect(doc: &SurfaceDocument) -> Rect {
    doc.element_rect(ElementId::new("fill"))
        .expect("#fill is laid out")
}

#[test]
fn a_bare_documents_inset_zero_box_starts_at_the_corner_and_spans_the_width() {
    let doc = resolved(bare_inset);
    let rect = fill_rect(&doc);
    assert_eq!((rect.x, rect.y), (0, 0));
    assert_eq!(rect.width, WIDTH as i32);
    // `#main` is as tall as its flow content, none here: unchanged by the reset.
    assert_eq!(rect.height, 0);
}

#[test]
fn a_bare_documents_viewport_box_covers_the_whole_surface() {
    let doc = resolved(bare_viewport);
    let rect = fill_rect(&doc);
    assert_eq!((rect.x, rect.y), (0, 0));
    assert_eq!((rect.width, rect.height), (WIDTH as i32, HEIGHT as i32));
}

#[test]
fn a_documents_own_body_margin_still_wins() {
    let doc = resolved(own_margin);
    let rect = fill_rect(&doc);
    assert_eq!((rect.x, rect.y), (4, 4));
    assert_eq!(rect.width, WIDTH as i32 - 8);
}

/// The headless path gets the same reset: a bare viewport-sized red box paints both corners.
#[cfg(feature = "headless")]
#[test]
fn a_headless_bare_document_paints_its_corner() {
    use shell_host::headless::{
        Fonts, HeadlessConfig, PixelSize, Scale, bundled_faces, render_png,
    };

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

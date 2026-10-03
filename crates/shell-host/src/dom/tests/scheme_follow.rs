//! A root that switches its scheme while the document is up repaints its glyphs and text in the
//! new ink, with the same nodes: no remount. Read off the painted pixels, because a computed
//! colour cannot see a glyph painted in a stale one (`dom::scheme`).

use std::task::Context;

use anyrender::render_to_buffer;
use anyrender_vello_cpu::VelloCpuImageRenderer;
use blitz_dom::Document as _;
use blitz_paint::paint_scene;
use blitz_traits::shell::ColorScheme;
use dioxus::prelude::*;
use image::RgbaImage;
use tokio::sync::watch;

use crate::dom::document::SurfaceDocument;
use crate::dom::tests::env::test_env;
use crate::id::SurfaceId;
use crate::root::HostRoot;
use crate::use_watch;
use crate::wake::{WokenSet, surface_waker};

const WIDTH: u32 = 200;
const HEIGHT: u32 = 100;

/// The glyph's top-left quarter of the picture: a 24 px square filled with `currentColor`.
const GLYPH: Band = Band { top: 0, bottom: 24 };
/// Below the glyph: a line of large capitals, whose stems are wider than a pixel.
const WORD: Band = Band {
    top: 24,
    bottom: 90,
};

#[derive(Debug, Clone, Copy)]
struct Band {
    top: u32,
    bottom: u32,
}

/// A root whose appearance is the watched scheme's slug, as the design system writes it: `data-theme`
/// on a `.ds`. The ink is a `color` rule, so a switch is a colour-only restyle.
fn themed_root(rx: watch::Receiver<&'static str>) -> Element {
    let theme = use_watch(rx);
    rsx! {
        style { ".ds{{color:#000}}.ds.dark{{color:#fff}}" }
        div { class: "ds {theme}", "data-theme": "{theme}",
            svg {
                id: "glyph",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                path { d: "M0 0h24v24H0z", fill: "currentColor" }
            }
            div { id: "word", style: "font-size:48px;font-weight:700", "HHHH" }
        }
    }
}

/// A root with no `.ds`: nothing to follow.
fn bare_root() -> Element {
    rsx! {
        div { id: "word", "HHHH" }
    }
}

fn paint(doc: &mut SurfaceDocument) -> RgbaImage {
    let mut inner = doc.dioxus().inner_mut();
    let buffer = render_to_buffer::<VelloCpuImageRenderer, _>(
        |painter| paint_scene(painter, &mut inner, 1.0, WIDTH, HEIGHT, 0, 0),
        WIDTH,
        HEIGHT,
    );
    RgbaImage::from_raw(WIDTH, HEIGHT, buffer).expect("width * height * 4 bytes")
}

/// The red channel of every fully opaque pixel in `band`: ink only, with no anti-aliased edge and
/// no backdrop mixed in (the canvas is transparent and the ink is grey).
fn opaque_ink(image: &RgbaImage, band: Band) -> Vec<u8> {
    (band.top..band.bottom)
        .flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
        .map(|(x, y)| image.get_pixel(x, y).0)
        .filter(|pixel| pixel[3] == 255)
        .map(|pixel| pixel[0])
        .collect()
}

struct Switchable {
    doc: SurfaceDocument,
    theme: watch::Sender<&'static str>,
    cx: Context<'static>,
    t: f64,
}

impl Switchable {
    fn up(start: &'static str) -> Self {
        let (theme, rx) = watch::channel(start);
        let doc = SurfaceDocument::build(
            HostRoot::with_props(themed_root, rx),
            test_env(WIDTH, HEIGHT),
        );
        let (ping, _source) = calloop::ping::make_ping().expect("eventfd ping");
        let waker = Box::leak(Box::new(surface_waker(
            SurfaceId(1),
            WokenSet::default(),
            ping,
        )));
        let mut up = Switchable {
            doc,
            theme,
            cx: Context::from_waker(waker),
            t: 0.0,
        };
        up.frame();
        up
    }

    /// One frame: poll the root's work, then resolve.
    fn frame(&mut self) {
        self.doc.poll(&mut self.cx);
        self.t += 0.016;
        self.doc.resolve(self.t);
    }

    fn switch_to(&mut self, scheme: &'static str) {
        self.theme.send(scheme).expect("the root is listening");
        self.frame();
    }
}

#[tokio::test]
async fn a_live_switch_to_dark_repaints_the_glyph_and_the_text_in_light_ink() {
    let mut up = Switchable::up("light");
    let before = paint(&mut up.doc);
    let glyph_before = opaque_ink(&before, GLYPH);
    let word_before = opaque_ink(&before, WORD);
    assert!(
        !glyph_before.is_empty() && !word_before.is_empty(),
        "the Light picture draws a glyph and text"
    );
    assert!(
        glyph_before.iter().chain(&word_before).all(|&red| red == 0),
        "Light ink is black"
    );
    let glyph_node = up.doc.dioxus().inner().get_element_by_id("glyph");

    up.switch_to("dark");

    let after = paint(&mut up.doc);
    assert!(
        opaque_ink(&after, GLYPH).iter().all(|&red| red == 255),
        "the glyph's currentColor follows the switch"
    );
    let word_after = opaque_ink(&after, WORD);
    assert!(!word_after.is_empty(), "the Dark picture draws text");
    assert!(
        word_after.iter().all(|&red| red == 255),
        "the text's colour follows the switch"
    );
    assert_eq!(
        up.doc.dioxus().inner().get_element_by_id("glyph"),
        glyph_node,
        "the same node, not a remount"
    );
}

#[tokio::test]
async fn a_live_switch_back_to_light_repaints_in_dark_ink() {
    let mut up = Switchable::up("dark");
    up.switch_to("light");

    let image = paint(&mut up.doc);
    for band in [GLYPH, WORD] {
        let ink = opaque_ink(&image, band);
        assert!(!ink.is_empty(), "{band:?} is drawn");
        assert!(ink.iter().all(|&red| red == 0), "{band:?} is Light ink");
    }
}

#[tokio::test]
async fn the_viewport_follows_the_root_and_incremental_layout_comes_back_the_frame_after() {
    let mut up = Switchable::up("light");
    assert_eq!(up.doc.scheme(), ColorScheme::Light);
    assert!(up.doc.dioxus().inner().incremental_layout());

    up.switch_to("dark");
    assert_eq!(up.doc.scheme(), ColorScheme::Dark);
    assert!(
        !up.doc.dioxus().inner().incremental_layout(),
        "the switching frame rebuilds every box"
    );

    up.frame();
    assert!(
        up.doc.dioxus().inner().incremental_layout(),
        "nothing is animating, so the next frame is incremental again"
    );
}

#[test]
fn a_document_with_no_root_scheme_keeps_the_hosts() {
    let mut doc = SurfaceDocument::build(HostRoot::new(bare_root), test_env(WIDTH, HEIGHT));
    doc.set_scheme(ColorScheme::Dark);
    doc.resolve(0.0);
    assert_eq!(doc.scheme(), ColorScheme::Dark);
}

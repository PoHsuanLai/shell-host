//! A z-indexed box whose ancestors moved in the same resolve paints where layout put it.
//!
//! The shape is sill's widget centre: a column of tiles whose body is `position: relative;
//! z-index` (quire's `.ds-widget-body`). Tiles added after the first layout used to paint their
//! bodies at the column's top on screen — over the first tile — while `debug capture`, which
//! resolves once more, drew them in place. The live frame resolves once per change and the
//! capture twice, so the test does exactly that and compares.

use std::task::Context;

use anyrender::render_to_buffer;
use anyrender_vello_cpu::VelloCpuImageRenderer;
use blitz_paint::paint_scene;
use dioxus::prelude::*;
use tokio::sync::watch;

use crate::dom::document::SurfaceDocument;
use crate::dom::tests::env::test_env;
use crate::root::HostRoot;
use crate::use_watch;

const WIDTH: u32 = 100;
const HEIGHT: u32 = 200;
/// Each tile is 60 px tall with its body 20 px down: tile `i`'s body spans y 60i+20 .. 60i+40.
const TILE: u32 = 60;
const BODY_TOP: u32 = 20;
const BODY_HEIGHT: u32 = 20;

const STYLE: &str = "\
    #col{display:flex;flex-direction:column}\
    .tile{height:60px;background:rgb(0,0,255)}\
    .body{position:relative;z-index:1;margin-top:20px;height:20px;background:rgb(255,0,0)}";

fn tiles(rx: watch::Receiver<u32>) -> Element {
    let count = use_watch(rx);
    rsx! {
        dioxus_document::Style { "{STYLE}" }
        div { id: "col",
            for i in 0..count() {
                div { key: "{i}", class: "tile",
                    div { id: "body{i}", class: "body" }
                }
            }
        }
    }
}

/// The document with one tile laid out, then grown to three and polled, not yet resolved.
fn grown() -> SurfaceDocument {
    let (tx, rx) = watch::channel(1);
    let mut doc = SurfaceDocument::build(HostRoot::with_props(tiles, rx), test_env(WIDTH, HEIGHT));
    let waker = std::task::Waker::noop().clone();
    let mut cx = Context::from_waker(&waker);
    doc.poll(&mut cx);
    doc.resolve(0.0);
    tx.send(3).expect("receiver alive");
    assert!(doc.poll(&mut cx));
    doc
}

fn paint(doc: &mut SurfaceDocument) -> Vec<u8> {
    let mut base = doc.dioxus().inner.borrow_mut();
    render_to_buffer::<VelloCpuImageRenderer, _>(
        |painter| paint_scene(painter, &mut base, 1.0, WIDTH, HEIGHT, 0, 0),
        WIDTH,
        HEIGHT,
    )
}

fn pixel(image: &[u8], x: u32, y: u32) -> [u8; 4] {
    let at = ((y * WIDTH + x) * 4) as usize;
    [image[at], image[at + 1], image[at + 2], image[at + 3]]
}

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

/// Tiles added after the first layout: the frame the host presents (one resolve) paints every
/// body inside its own tile, the capture (another resolve first) paints the same pixels, and
/// hit testing reads the same hoisted positions.
#[test]
fn tiles_added_after_the_first_layout_paint_and_hit_where_layout_put_them() {
    let mut doc = grown();
    doc.resolve(0.0);
    let frame = paint(&mut doc);
    for i in 0..3 {
        let body = TILE * i + BODY_TOP + BODY_HEIGHT / 2;
        let above = TILE * i + BODY_TOP / 2;
        assert_eq!(pixel(&frame, 50, body), RED, "frame: tile {i}'s body");
        assert_eq!(
            pixel(&frame, 50, above),
            BLUE,
            "frame: tile {i} above its body"
        );
    }

    doc.resolve(0.0);
    let capture = paint(&mut doc);
    let differing = frame
        .chunks(4)
        .zip(capture.chunks(4))
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(differing, 0, "pixels that differ between frame and capture");

    let base = doc.base();
    let y = (TILE * 2 + BODY_TOP + BODY_HEIGHT / 2) as f32;
    let hit = base.element_from_point(50.0, y);
    let body = base.get_element_by_id("body2");
    assert_eq!(
        hit, body,
        "the pointer over a new tile's body hits that body"
    );
}

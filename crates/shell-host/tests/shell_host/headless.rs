//! Pixel probes for `shell_host::headless::render_png`: the CI gate every future surface
//! (terminal, video, PDF viewer, quire component) will eventually get one of.
//!
//! Why `@keyframes` and not a `transition`: `render_png` builds the document once, then calls
//! `BaseDocument::resolve(t)` once per requested timestamp with no DOM mutation in between (see
//! `src/headless.rs`). A CSS `transition` only starts when a *later* resolve observes a changed
//! computed value relative to an *earlier* one, and a document's first style pass has no earlier
//! one. A `@keyframes` animation's computed style is a pure function of `resolve`'s timestamp
//! from the first call, which is what a render test wants.

use blitz_traits::shell::ColorScheme;
use dioxus::core::Element;
use dioxus::prelude::*;
use image::RgbaImage;
use shell_host::Scale;
use shell_host::headless::{Fonts, HeadlessConfig, PixelSize, render_png};

const SIZE: (u32, u32) = (240, 160);

/// The page colour behind the drifting box.
const PAGE: [u8; 4] = [248, 249, 246, 255];

/// A row through the middle of the box (it spans y 64..104).
const PROBE_ROW: u32 = 84;

fn scene() -> Element {
    rsx! {
        style { "{STYLES}" }
        div { id: "card",
            div { id: "drift" }
        }
    }
}

const STYLES: &str = "
#card {
    box-sizing: border-box;
    width: 240px;
    height: 160px;
    background: #f8f9f6;
    position: relative;
}
#drift {
    position: absolute;
    left: 16px;
    top: 64px;
    width: 80px;
    height: 40px;
    border-radius: 8px;
    background: rgba(35, 80, 143, 0.45);
    animation: drift 1s linear;
}
@keyframes drift {
    from { transform: translateX(0px); }
    to { transform: translateX(120px); }
}
";

/// The first column of `row` whose pixel is not the page colour, i.e. the box's left edge.
fn first_box_column(frame: &RgbaImage, row: u32) -> Option<u32> {
    (0..frame.width()).find(|&x| {
        let pixel = frame.get_pixel(x, row).0;
        pixel
            .iter()
            .zip(PAGE)
            .any(|(got, page)| got.abs_diff(page) > 16)
    })
}

/// The box starts at x = 16 and the animation carries it 120 px in 1 s: 12 px by t = 0.1 s.
#[test]
fn the_keyframe_box_moves_between_timestamps() {
    let images = render_png(
        scene,
        HeadlessConfig {
            size: PixelSize::new(SIZE.0, SIZE.1),
            scale: Scale::ONE,
            color_scheme: ColorScheme::Light,
            at_seconds: vec![0.0, 0.1],
            fonts: Fonts::System,
        },
    );
    let [t0, t0_1] =
        <[RgbaImage; 2]>::try_from(images).expect("two timestamps requested, two frames returned");
    for (frame, want, when) in [(&t0, 16, "t = 0"), (&t0_1, 28, "t = 0.1 s")] {
        let left = first_box_column(frame, PROBE_ROW).expect("the box is drawn");
        assert!(
            left.abs_diff(want) <= 1,
            "the box's left edge at {when}: {left}, want {want}"
        );
    }
}

fn raised_tiles() -> Element {
    rsx! {
        style { "{RAISED_STYLES}" }
        div { id: "col",
            div { class: "tile", div { class: "body" } }
            div { class: "tile", div { class: "body" } }
        }
    }
}

/// quire's `.ds-widget-body` shape: a `position: relative; z-index` box inside each tile.
const RAISED_STYLES: &str = "
#col { display: flex; flex-direction: column; }
.tile { height: 60px; background: rgb(0, 0, 255); }
.body { position: relative; z-index: 1; margin-top: 20px; height: 20px; background: rgb(255, 0, 0); }
";

/// A document's very first resolve paints its z-indexed boxes in place: Blitz gathers them
/// before laying out, when every location is still zero, so without the
/// re-hoist the second tile's body painted over the first tile's.
#[test]
fn the_first_frame_paints_raised_boxes_where_layout_put_them() {
    let frames = render_png(
        raised_tiles,
        HeadlessConfig {
            size: PixelSize::new(100, 120),
            scale: Scale::ONE,
            color_scheme: ColorScheme::Light,
            at_seconds: vec![0.0],
            fonts: Fonts::System,
        },
    );
    let frame = &frames[0];
    const RED: [u8; 4] = [255, 0, 0, 255];
    const BLUE: [u8; 4] = [0, 0, 255, 255];
    for (y, want, what) in [
        (10, BLUE, "tile 0 above its body"),
        (30, RED, "tile 0's body"),
        (70, BLUE, "tile 1 above its body"),
        (90, RED, "tile 1's body"),
    ] {
        assert_eq!(frame.get_pixel(50, y).0, want, "{what}");
    }
}

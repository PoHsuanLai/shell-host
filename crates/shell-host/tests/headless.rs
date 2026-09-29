//! Snapshot test for `shell_host::headless::render_png`: the CI gate every future surface
//! (terminal, video, PDF viewer, quire component) will eventually get one of.
//!
//! Renders a small Dioxus tree — a div with text and a nested translucent box animated by a
//! CSS `@keyframes` rule — at two timestamps 0.1s apart, and checks each frame against a
//! committed PNG under `tests/snapshots/` with `assert_snapshot`.
//!
//! Why `@keyframes` and not a `transition`: `render_png` builds the document once, then calls
//! `BaseDocument::resolve(t)` once per requested timestamp with no DOM mutation in between (see
//! `src/headless.rs`). A CSS `transition` only starts when a *later* resolve observes a changed
//! computed value relative to an *earlier* one for the same element — there is no such earlier
//! resolve to diverge from on a document's first-ever style pass, so a single `render_png` call
//! can never show a `transition` "mid-flight". A `@keyframes` animation has no such before/after
//! requirement: its computed style is a pure function of `resolve`'s timestamp from the very
//! first call, which is exactly what a snapshot test wants. This is a real difference from the
//! plan's "Findings: Blitz / Dioxus Native" section, which lists CSS `transition` and
//! `@keyframes` together as "what works" without distinguishing them on this point.
//!
//! `assert_snapshot` was proved to actually reject a mismatched render during development (a
//! deliberately different image compared against the committed golden failed the assertion, as
//! it must for this to be a meaningful CI gate) — that throwaway case is not kept here; only
//! the passing scenario is.

use std::path::{Path, PathBuf};

use blitz_traits::shell::ColorScheme;
use dioxus::core::Element;
use dioxus::prelude::*;
use image::RgbaImage;
use shell_host::headless::{Fonts, HeadlessConfig, PixelSize, Scale, bundled_faces, render_png};

const SIZE: (u32, u32) = (240, 160);

fn scene() -> Element {
    rsx! {
        style { "{STYLES}" }
        div { id: "card",
            p { "Hello, shell-host" }
            div { id: "drift" }
        }
    }
}

const STYLES: &str = "
#card {
    box-sizing: border-box;
    width: 240px;
    height: 160px;
    padding: 12px;
    background: #f8f9f6;
    position: relative;
    font-family: sans-serif;
}
#card p {
    margin: 0;
    color: #1a1e1a;
    font-size: 16px;
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

#[test]
fn keyframe_animation_mid_flight_matches_snapshots() {
    let images = render_png(
        scene,
        HeadlessConfig {
            size: PixelSize::new(SIZE.0, SIZE.1),
            scale: Scale::ONE,
            color_scheme: ColorScheme::Light,
            at_seconds: vec![0.0, 0.1],
            fonts: Fonts::Bundled(bundled_faces()),
        },
    );

    let [t0, t0_1] =
        <[RgbaImage; 2]>::try_from(images).expect("two timestamps requested, two frames returned");
    assert_snapshot("drift_t0", &t0);
    assert_snapshot("drift_t0_1", &t0_1);

    // The animation actually moved: the two frames must not be identical, or this test would
    // pass just as well with the animation deleted (see CONVENTIONS.md, "an assertion that
    // was already true proves nothing").
    assert_ne!(
        t0.as_raw(),
        t0_1.as_raw(),
        "keyframe animation produced identical frames at t=0 and t=0.1s"
    );
}

// -------------------------------------------------------------------------------------------
// Snapshot comparison: per-channel tolerance <= 2, <= 0.1% of pixels allowed to differ beyond
// that, `UPDATE_SNAPSHOTS=1` to (re)write the golden file instead of comparing against it.
// -------------------------------------------------------------------------------------------

const PER_CHANNEL_TOLERANCE: i16 = 2;
const MAX_DIFFERING_FRACTION: f64 = 0.001;

fn assert_snapshot(name: &str, img: &RgbaImage) {
    let path = snapshot_path(name);

    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        img.save(&path)
            .unwrap_or_else(|err| panic!("writing snapshot {}: {err}", path.display()));
        return;
    }

    let golden = image::open(&path)
        .unwrap_or_else(|err| {
            panic!(
                "snapshot {} is missing or unreadable ({err}); run with UPDATE_SNAPSHOTS=1 to create it",
                path.display()
            )
        })
        .to_rgba8();

    assert_eq!(
        (img.width(), img.height()),
        (golden.width(), golden.height()),
        "snapshot {} size mismatch",
        path.display()
    );

    let total_pixels = (img.width() as u64) * (img.height() as u64);
    let mut differing = 0u64;
    let mut first_diff = None;
    for (x, y, actual) in img.enumerate_pixels() {
        let expected = golden.get_pixel(x, y);
        let channel_diff = actual
            .0
            .iter()
            .zip(expected.0.iter())
            .map(|(a, e)| (*a as i16 - *e as i16).abs())
            .max()
            .unwrap_or(0);
        if channel_diff > PER_CHANNEL_TOLERANCE {
            differing += 1;
            first_diff.get_or_insert((x, y, *actual, *expected));
        }
    }

    let differing_fraction = differing as f64 / total_pixels as f64;
    if differing_fraction > MAX_DIFFERING_FRACTION {
        let actual_path = path.with_extension("actual.png");
        let _ = img.save(&actual_path);
        let (x, y, actual, expected) =
            first_diff.expect("differing_fraction > 0 implies at least one diff");
        let snapshot_display = path.display();
        let actual_display = actual_path.display();
        let differing_pct = differing_fraction * 100.0;
        let tolerance_pct = MAX_DIFFERING_FRACTION * 100.0;
        panic!(
            "snapshot {snapshot_display} differs in {differing}/{total_pixels} pixels \
             ({differing_pct:.3}%, tolerance {tolerance_pct:.1}%); first mismatch at ({x}, {y}): \
             actual {actual:?} vs expected {expected:?}; actual render written to \
             {actual_display} for inspection; re-run with UPDATE_SNAPSHOTS=1 if this change is \
             intended"
        );
    }
}

fn snapshot_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/snapshots")
        .join(format!("{name}.png"))
}

/// `Inject::Capture`'s writer (`inject::write_capture`, what `HostCtx::inject` calls after its
/// offscreen render) on a headless render: it writes a PNG that decodes to the same pixels and
/// returns the path and the pixel size instead of printing anything.
#[cfg(feature = "debug-inject")]
#[test]
fn inject_capture_writes_a_png_and_returns_its_size() {
    use shell_host::scale::Size;
    use shell_host::{Injected, inject::write_capture};

    let frame = render_png(
        scene,
        HeadlessConfig {
            size: PixelSize::new(SIZE.0, SIZE.1),
            scale: Scale::ONE,
            color_scheme: ColorScheme::Light,
            at_seconds: vec![0.0],
            fonts: Fonts::Bundled(bundled_faces()),
        },
    )
    .remove(0);
    let dir = std::env::temp_dir().join(format!("shell-host-capture-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("capture.png");
    let _ = std::fs::remove_file(&path);

    let written = write_capture(&frame, path.clone()).expect("capture written");

    let size = Size {
        width: 240,
        height: 160,
    };
    assert_eq!(
        written,
        Injected::Captured {
            path: path.clone(),
            size
        }
    );
    let read = image::open(&path).expect("a PNG").to_rgba8();
    assert_eq!(read, frame, "the file holds the rendered pixels");
    std::fs::remove_dir_all(&dir).ok();
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

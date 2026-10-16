//! A wallpaper-shaped surface whose content changes after its first frame (a resource
//! load that lands mid-render must still be painted): `Layer::Background`, anchored to every edge,
//! `ExclusiveZone::Ignore`, an empty input region, no keyboard, no blur. The root paints a
//! fallback colour, then `SWAP_DELAY` later swaps in content of a known solid colour, so a
//! screenshot's centre pixel says whether the second paint reached the screen.
//!
//! `--source=` picks what the swap shows: `file` (an `img` of a generated PNG in a temp file,
//! the default), `data` (the same PNG as a `data:` URL), `css` (a `background-image` of the
//! file), or `none` (a plain element in the image colour, no resource at all); `--png-side=N`
//! sizes the PNG (default 64). Driven by
//! `wallpaper stats`, `wallpaper capture PATH`, `stats`, `quit` on stdin.

use std::path::PathBuf;
use std::time::Duration;

use dioxus::prelude::*;
use shell_host::{
    Anchor, BlurSpec, ExclusiveZone, Host, HostConfig, HostEvent, HostRoot, KeyboardMode,
    LayerConfig, LayerExtent, LayerLevel, LayerSize, Margin, OutputTarget, RegionSpec,
};

/// How long after the first frame the content is swapped in.
const SWAP_DELAY: Duration = Duration::from_millis(300);

/// The fallback colour the root paints before the swap.
const FALLBACK: [u8; 3] = [200, 40, 40];

/// The colour the swapped-in content paints.
const IMAGE: [u8; 3] = [20, 120, 220];

/// The side of the generated PNG, in pixels, unless `--png-side=N` says otherwise; the page
/// stretches it over the surface. A larger image decodes for longer, which moves the moment
/// its load completes relative to the render that started it.
const PNG_SIDE: u32 = 64;

/// What the swap shows.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Content {
    /// `<img src="file:///...">`.
    FileImg(String),
    /// `<img src="data:image/png,...">`.
    DataImg(String),
    /// A `div` with `background-image: url(file:///...)`.
    CssBackground(String),
    /// A `div` whose own background is the image colour.
    Plain,
}

/// The page. `#main` (dioxus-native-dom's mount point) is sized too: without it the root's
/// `height:100%` resolves against an auto height and the whole page is 0 px tall, so a capture
/// is transparent and only an `img`'s intrinsic size would ever show.
fn css() -> String {
    let [fr, fg, fb] = FALLBACK;
    let [ir, ig, ib] = IMAGE;
    format!(
        "html,body{{margin:0;padding:0}}#main{{height:100%}}\
         .root{{width:100%;height:100%;background:rgb({fr},{fg},{fb})}}\
         .fill{{display:block;width:100%;height:100%}}\
         .plain{{background:rgb({ir},{ig},{ib})}}\
         .css{{background-size:cover}}"
    )
}

/// The wallpaper document: the fallback, then `content` after `SWAP_DELAY`.
fn wallpaper(content: Content) -> Element {
    let mut swapped = use_signal(|| false);
    use_hook(|| {
        spawn(async move {
            tokio::time::sleep(SWAP_DELAY).await;
            swapped.set(true);
        });
    });
    let style = css();
    let inner = match (swapped(), &content) {
        (false, _) => rsx! {},
        (true, Content::FileImg(url) | Content::DataImg(url)) => {
            rsx! { img { class: "fill", src: "{url}" } }
        }
        (true, Content::CssBackground(url)) => rsx! {
            div { class: "fill css", style: "background-image:url({url})" }
        },
        (true, Content::Plain) => rsx! { div { class: "fill plain" } },
    };
    rsx! {
        style { "{style}" }
        div { class: "root", {inner} }
    }
}

/// The wallpaper's layer surface, shaped like sill's.
fn wallpaper_config() -> LayerConfig {
    LayerConfig {
        namespace: "wallpaper".to_owned(),
        layer: LayerLevel::Background,
        anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
        size: LayerSize {
            width: LayerExtent::Fill,
            height: LayerExtent::Fill,
        },
        exclusive: ExclusiveZone::Ignore,
        margin: Margin::ZERO,
        keyboard: KeyboardMode::None,
        output: OutputTarget::CompositorPicks,
        blur: BlurSpec::None,
        input: RegionSpec::Empty,
    }
}

/// A solid `IMAGE`-coloured PNG, encoded.
fn png_bytes(side: u32) -> Result<Vec<u8>, String> {
    let [r, g, b] = IMAGE;
    let image = image::RgbaImage::from_pixel(side, side, image::Rgba([r, g, b, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|e| format!("encode png: {e}"))?;
    Ok(bytes.into_inner())
}

/// Write the PNG next to the process's other temp files and return its `file://` URL.
fn png_file(bytes: &[u8]) -> Result<String, String> {
    let path: PathBuf =
        std::env::temp_dir().join(format!("demo_wallpaper-{}.png", std::process::id()));
    std::fs::write(&path, bytes).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(format!("file://{}", path.display()))
}

/// A percent-encoded `data:` URL (RFC 2397 without base64; `LocalNet` decodes both).
fn data_url(bytes: &[u8]) -> String {
    let payload: String = bytes.iter().map(|b| format!("%{b:02X}")).collect();
    format!("data:image/png,{payload}")
}

fn content(args: &[String]) -> Result<Content, String> {
    let source = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--source="))
        .unwrap_or("file");
    let side = match args.iter().find_map(|arg| arg.strip_prefix("--png-side=")) {
        Some(side) => side.parse().map_err(|e| format!("--png-side: {e}"))?,
        None => PNG_SIDE,
    };
    let bytes = png_bytes(side)?;
    match source {
        "file" => png_file(&bytes).map(Content::FileImg),
        "data" => Ok(Content::DataImg(data_url(&bytes))),
        "css" => png_file(&bytes).map(Content::CssBackground),
        "none" => Ok(Content::Plain),
        other => Err(format!(
            "--source: unknown {other:?} (file, data, css, none)"
        )),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(error) = run(&args) {
        eprintln!("demo_wallpaper: {error}");
        std::process::exit(1);
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let content = content(args)?;
    let host = Host::<()>::connect(HostConfig::default()).map_err(|e| e.to_string())?;
    host.run(move |ctx, event| match event {
        HostEvent::Started => {
            if let Err(error) = shell_host::inject::stdin::install(ctx) {
                eprintln!("debug-inject: {error}");
                ctx.exit();
            }
            let root = HostRoot::with_props(wallpaper, content.clone());
            if let Err(error) = ctx.spawn_layer(wallpaper_config(), root) {
                eprintln!("wallpaper: {error}");
                ctx.exit();
            }
        }
        // The compositor closed the wallpaper (its output went away): nothing is left to drive.
        HostEvent::SurfaceClosed { .. } | HostEvent::QuitAsked => ctx.exit(),
        _ => {}
    })
    .map_err(|e| e.to_string())
}

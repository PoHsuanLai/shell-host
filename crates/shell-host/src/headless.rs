//! Render a Dioxus root component to `image::RgbaImage` buffers with no Wayland connection,
//! no window and no GPU surface. This is the CI gate for every future surface: every
//! `examples/demo_*.rs` and every quire component eventually gets a `render_png` snapshot
//! test, so this module is real code (not a spike artifact) even though it was written as
//! part of the Milestone 0 spike.
//!
//! Pipeline, per the plan's "Custom host" findings: build a [`DioxusDocument`] from a plain
//! Dioxus component, drain its initial async work, then for each requested timestamp call
//! `resolve(t)` (which drives Stylo's `DocumentAnimationSet`, so `transition`/`@keyframes`
//! progress is a pure function of the timestamp we pass in, not of wall-clock time) followed
//! by `paint_scene` and a CPU (`anyrender_vello_cpu`) readback. There is no window, so there is
//! nothing to present and nothing to poll for input.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyrender::render_to_buffer;
use anyrender_vello_cpu::VelloCpuImageRenderer;
use blitz_dom::{BULLET_FONT, Document, DocumentConfig, FontContext};
use blitz_paint::paint_scene;
use blitz_traits::shell::{ColorScheme, Viewport};
use dioxus::core::Element;
use dioxus::core::VirtualDom;
use dioxus_native_dom::DioxusDocument;
use image::RgbaImage;
use parley::fontique::{Blob, Collection, CollectionOptions, SourceCache};

/// `DioxusDocument::poll` is driven until it reports no further pending work, capped so a
/// component whose effects never settle (a bug, not a feature) cannot hang a render.
const MAX_POLL_ITERATIONS: u32 = 32;

/// A framebuffer size in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelSize {
    pub width: u32,
    pub height: u32,
}

impl PixelSize {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

/// A rendering scale factor, stored as an integer in 120ths so geometry stays `Eq` — the same
/// convention `shell-host`'s Wayland fractional-scale handling uses for buffer sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scale(pub u32);

impl Scale {
    pub const ONE: Scale = Scale(120);

    pub fn as_f32(self) -> f32 {
        self.0 as f32 / 120.0
    }

    pub fn as_f64(self) -> f64 {
        self.0 as f64 / 120.0
    }
}

/// Which fonts a headless render has available. `Bundled` is the reproducible choice for a
/// snapshot test: it registers exactly one known face and turns system font discovery off, so
/// a render does not vary with whatever happens to be installed on the machine that runs it
/// (other than the one fallback path documented on [`bundled_font`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fonts {
    /// Register a single bundled TTF and disable system font discovery.
    Bundled,
    /// Discover system fonts normally (matches what a real on-screen surface sees).
    System,
}

/// Configuration for [`render_png`].
#[derive(Debug, Clone)]
pub struct HeadlessConfig {
    pub size: PixelSize,
    pub scale: Scale,
    pub color_scheme: ColorScheme,
    /// One output image per timestamp, fed to `BaseDocument::resolve` in order.
    pub at_seconds: Vec<f64>,
    pub fonts: Fonts,
}

/// Render `root` headlessly, once per timestamp in `config.at_seconds`, returning one image
/// per timestamp in the same order.
///
/// Builds a [`DioxusDocument`] from `root`, polls it to idle (bounded, see
/// [`MAX_POLL_ITERATIONS`]), then for each timestamp resolves style/layout at that time,
/// paints the resolved document into a scene, and rasterizes that scene on the CPU
/// (`anyrender_vello_cpu`) into an RGBA8 buffer.
///
/// A custom [`blitz_dom::Widget`] on this path receives a CPU
/// [`anyrender::RenderContext`], whose `renderer_specific_context` is `None` (no wgpu
/// device) — a widget written against the wgpu texture pattern (see `examples/spike_widget.rs`)
/// will render as fully transparent here. That is expected: this function exists to snapshot
/// ordinary document content and CSS motion, not to exercise a widget's GPU path.
pub fn render_png(root: fn() -> Element, config: HeadlessConfig) -> Vec<RgbaImage> {
    let HeadlessConfig {
        size,
        scale,
        color_scheme,
        at_seconds,
        fonts,
    } = config;

    let viewport = Viewport::new(size.width, size.height, scale.as_f32(), color_scheme);
    let mut doc = DioxusDocument::new(
        VirtualDom::new(root),
        DocumentConfig {
            viewport: Some(viewport),
            font_ctx: Some(build_font_context(fonts)),
            ..Default::default()
        },
    );

    doc.initial_build();
    for _ in 0..MAX_POLL_ITERATIONS {
        if !doc.poll(None) {
            break;
        }
    }

    at_seconds
        .into_iter()
        .map(|t| render_frame(&mut doc, t, size, scale))
        .collect()
}

fn render_frame(doc: &mut DioxusDocument, t: f64, size: PixelSize, scale: Scale) -> RgbaImage {
    let mut inner = doc.inner.borrow_mut();
    inner.resolve(t);

    let buffer = render_to_buffer::<VelloCpuImageRenderer, _>(
        |painter| {
            paint_scene(
                painter,
                &mut inner,
                scale.as_f64(),
                size.width,
                size.height,
                0,
                0,
            )
        },
        size.width,
        size.height,
    );

    RgbaImage::from_raw(size.width, size.height, buffer)
        .expect("anyrender::render_to_buffer returns exactly width * height * 4 bytes")
}

/// One [`FontContext`] per call: `system_fonts` is a construction-time choice of the
/// `fontique::Collection`, not a runtime flag, so `Fonts::Bundled` must build its own
/// collection (`blitz_dom::BaseDocument`'s own default construction hard-codes
/// `system_fonts` to blitz-dom's own `system-fonts` *cargo* feature, which `shell-host`'s
/// pinned dependency block always enables — supplying `font_ctx` ourselves is the only way
/// to turn it off for a reproducible render).
fn build_font_context(fonts: Fonts) -> FontContext {
    let system_fonts = matches!(fonts, Fonts::System);
    let mut font_ctx = FontContext {
        source_cache: SourceCache::new_shared(),
        collection: Collection::new(CollectionOptions {
            shared: false,
            system_fonts,
        }),
    };

    // Registered unconditionally: this is the list-bullet glyph blitz-dom's own default
    // construction always registers (see `BaseDocument::new`); we bypass that default to
    // control `system_fonts`, so we register it ourselves to match.
    font_ctx
        .collection
        .register_fonts(Blob::new(Arc::new(BULLET_FONT) as _), None);

    if let Fonts::Bundled = fonts {
        let (path, bytes) = bundled_font();
        eprintln!(
            "shell-host::headless: Fonts::Bundled registered {}",
            path.display()
        );
        font_ctx
            .collection
            .register_fonts(Blob::new(Arc::new(bytes) as _), None);
    }

    font_ctx
}

/// The one bundled TTF a `Fonts::Bundled` render registers.
///
/// Prefers `~/quire/crates/ds/assets/fonts` (the design system's own bundled faces, once
/// quire ships one); as of this spike quire has no `assets/` directory at all, so this falls
/// back to the first TTF under `/usr/share/fonts` in sorted path order, which is
/// deterministic on a given machine but not portable across machines with a different font
/// package set. Panics if neither location has a usable font — a render with no glyphs at
/// all is a broken environment, not a value this function can return.
fn bundled_font() -> (PathBuf, Vec<u8>) {
    let quire_fonts = home_dir().join("quire/crates/ds/assets/fonts");
    if let Some(path) = first_ttf_sorted(&quire_fonts) {
        let bytes = std::fs::read(&path).expect("bundled font file is readable");
        return (path, bytes);
    }

    let path = first_ttf_sorted(Path::new("/usr/share/fonts"))
        .expect("at least one system TTF font is installed at /usr/share/fonts");
    let bytes = std::fs::read(&path).expect("system fallback font file is readable");
    (path, bytes)
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn first_ttf_sorted(dir: &Path) -> Option<PathBuf> {
    let mut ttfs: Vec<PathBuf> = walk_ttfs(dir);
    ttfs.sort();
    ttfs.into_iter().next()
}

fn walk_ttfs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(walk_ttfs(&path));
        } else if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("ttf"))
        {
            found.push(path);
        }
    }
    found
}

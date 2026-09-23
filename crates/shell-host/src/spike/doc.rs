//! The spike's documents (bar, dock) and the glue that feeds them pointer events.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use blitz_dom::DocumentConfig;
use blitz_traits::events::{
    BlitzPointerEvent, BlitzPointerId, MouseEventButton, MouseEventButtons, PointerCoords,
    PointerDetails, UiEvent,
};
use blitz_traits::shell::{ColorScheme, ShellProvider, Viewport};
use dioxus::prelude::*;
use dioxus_native_dom::DioxusDocument;

/// What a surface shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Content {
    Bar,
    /// Magnification via `transform: scale()` transitions.
    DockTransform,
    /// Magnification via `width`/`height` transitions (relayout every frame).
    DockWidth,
}

/// Sets a flag when the document asks for a redraw; the frame machine reads it.
pub struct RedrawFlag(pub Arc<AtomicBool>);

impl ShellProvider for RedrawFlag {
    fn request_redraw(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

// `height:100%`: Blitz hit-testing only descends into boxes containing the point, so absolutely
// positioned content under a zero-height body is unreachable by the pointer.
const BASE_CSS: &str = "html,body{background:transparent;margin:0;padding:0;height:100%}";

const BAR_CSS: &str = ".bar{height:32px;box-sizing:border-box;padding:0 12px;display:flex;\
align-items:center;justify-content:space-between;background:rgba(255,0,128,0.55);\
color:white;font-size:14px;font-family:sans-serif}\
.checker{position:absolute;left:200px;top:12px;display:flex;height:8px}.checker div{width:2px;height:8px}\
.checker div:nth-child(odd){background:black}.checker div:nth-child(even){background:white}";

// In-flow layout on purpose: with `.dock{position:absolute}` Blitz hit-testing returned `main`
// for every point over the icons (hover never reached them). Observed at blitz e99fbdbd.
const DOCK_CSS: &str = ".dock{height:96px;display:flex;align-items:flex-end;justify-content:center;\
gap:8px;padding-bottom:8px;box-sizing:border-box;\
background:linear-gradient(to bottom,transparent 32px,rgba(30,30,40,0.35) 32px)}\
.icon{width:48px;height:48px;border-radius:12px;flex:none}";

const DOCK_TRANSFORM_CSS: &str = ".icon{transition:transform 170ms;transform-origin:50% 100%}\
.icon:hover{transform:scale(1.5)}";

const DOCK_WIDTH_CSS: &str =
    ".icon{transition:width 170ms,height 170ms}.icon:hover{width:72px;height:72px}";

const COLOURS: [&str; 12] = [
    "#e6194b", "#3cb44b", "#ffe119", "#4363d8", "#f58231", "#911eb4", "#46f0f0", "#f032e6",
    "#bcf60c", "#fabebe", "#008080", "#e6beff",
];

fn bar() -> Element {
    rsx! {
        style { "{BASE_CSS}{BAR_CSS}" }
        div { class: "bar",
            span { "shell-host spike bar" }
            div { class: "checker",
                for i in 0..40 { div { key: "{i}" } }
            }
        }
    }
}

fn dock(css: &'static str) -> Element {
    rsx! {
        style { "{BASE_CSS}{DOCK_CSS}{css}" }
        div { class: "dock",
            for (i, c) in COLOURS.iter().enumerate() {
                div { key: "{i}", class: "icon", style: "background:{c}" }
            }
        }
    }
}

fn dock_transform() -> Element {
    dock(DOCK_TRANSFORM_CSS)
}

fn dock_width() -> Element {
    dock(DOCK_WIDTH_CSS)
}

/// Logical size a surface of this content asks the compositor for (0 = stretch).
pub fn logical_size(content: Content) -> (u32, u32) {
    match content {
        Content::Bar => (0, 32),
        // 12 icons x 48 + 11 gaps x 8 + room for the 1.5x magnified ends.
        Content::DockTransform | Content::DockWidth => (12 * 48 + 11 * 8 + 96, 96),
    }
}

/// Builds and initially renders the Dioxus document for `content`.
pub fn build(content: Content, redraw: Arc<AtomicBool>) -> DioxusDocument {
    let app = match content {
        Content::Bar => bar,
        Content::DockTransform => dock_transform,
        Content::DockWidth => dock_width,
    };
    let config = DocumentConfig {
        shell_provider: Some(Arc::new(RedrawFlag(redraw))),
        style_threading: blitz_dom::StyleThreading::Sequential,
        ..Default::default()
    };
    let mut doc = DioxusDocument::new(VirtualDom::new(app), config);
    doc.initial_build();
    doc
}

/// Sets the viewport in physical px at `scale`.
pub fn viewport(buffer: (u32, u32), scale: f32) -> Viewport {
    Viewport::new(buffer.0, buffer.1, scale, ColorScheme::Light)
}

/// A mouse pointer event at logical (x, y) with `buttons` held.
pub fn pointer(
    x: f32,
    y: f32,
    button: MouseEventButton,
    buttons: MouseEventButtons,
) -> BlitzPointerEvent {
    BlitzPointerEvent {
        id: BlitzPointerId::Mouse,
        is_primary: true,
        coords: PointerCoords {
            page_x: x,
            page_y: y,
            screen_x: x,
            screen_y: y,
            client_x: x,
            client_y: y,
        },
        button,
        buttons,
        mods: Default::default(),
        details: PointerDetails::default(),
        element: Default::default(),
        active_pointers: Default::default(),
    }
}

/// The events a `hover` produces.
pub fn hover_events(x: f32, y: f32) -> Vec<UiEvent> {
    vec![UiEvent::PointerMove(pointer(
        x,
        y,
        MouseEventButton::Main,
        MouseEventButtons::None,
    ))]
}

/// The events a `click` produces: move, down, up (the plan's "synthesize move before down").
pub fn click_events(x: f32, y: f32) -> Vec<UiEvent> {
    let none = MouseEventButtons::None;
    let primary = MouseEventButtons::Primary;
    vec![
        UiEvent::PointerMove(pointer(x, y, MouseEventButton::Main, none)),
        UiEvent::PointerDown(pointer(x, y, MouseEventButton::Main, primary)),
        UiEvent::PointerUp(pointer(x, y, MouseEventButton::Main, none)),
    ]
}

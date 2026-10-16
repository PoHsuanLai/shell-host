//! Acceptance for `PopupSize::FitContent`: after a popup's first layout, the
//! host measures its document's content and repositions the popup to that size instead of
//! whatever guess the caller opened it with, and re-fits again if the content grows.
//!
//! A one-button bar; clicking "Open" (`bar click X Y`, or a real click) opens a popup anchored
//! under the button, with `PopupSize::FitContent { max: 400 }` and an initial placeholder size
//! of one row. The popup's own list starts at 2 rows and grows to 6 `GROWTH_DELAY` after it
//! opens, with no further command needed, so a script only has to wait and re-read `bar/popup
//! stats`' `logical` size: 160x48 once the first fit lands, 160x144 once the content has grown
//! and the popup has re-fit to it.

use std::time::Duration;

use dioxus::prelude::*;
use shell_host::{
    Anchor, BlurSpec, ExclusiveZone, Host, HostConfig, HostEvent, HostRoot, KeyboardMode,
    LayerConfig, LayerExtent, LayerLevel, LayerSize, Margin, OutputTarget, PopupPlacement, Rect,
    RegionSpec, Size, use_surface,
};
use shell_host::{
    PopupAdjust, PopupAnchor, PopupConfig, PopupEdge, PopupGrab, PopupOffset, PopupSize,
};

/// The margin and padding reset quire's `css/reset.css` would bring.
const RESET_CSS: &str = "html,body{margin:0;padding:0}";

const BAR_CSS: &str = ".bar{height:32px;box-sizing:border-box;display:flex;align-items:center;\
background:rgba(40,40,60,0.92);color:white;font-family:sans-serif;font-size:14px}\
.open{margin-left:12px;padding:4px 10px;background:#456;border-radius:4px}";

/// Each `.row` is exactly 24px tall and the `.list` exactly 160px wide, so the popup's content
/// size is `(160, rows * 24)` with nothing else to account for (no padding, no border on the
/// list itself).
const LIST_CSS: &str = ".list{display:flex;flex-direction:column;width:160px}\
.row{height:24px;box-sizing:border-box;padding:0 8px;display:flex;align-items:center;\
background:#223;color:#ddd;font-family:sans-serif;font-size:12px;border-bottom:1px solid #334}";

/// The "Open" button's rect (matches `.open`'s box at the bar's default font/padding): what
/// `PopupAnchor::Rect` positions the popup from.
const BUTTON: Rect = Rect {
    x: 12,
    y: 4,
    width: 70,
    height: 24,
};

const START_ROWS: usize = 2;
const GROWN_ROWS: usize = 6;

/// How long after the popup opens its content grows from `START_ROWS` to `GROWN_ROWS`.
const GROWTH_DELAY: Duration = Duration::from_millis(300);

fn bar_config() -> LayerConfig {
    LayerConfig {
        namespace: "bar".to_owned(),
        layer: LayerLevel::Top,
        anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
        size: LayerSize {
            width: LayerExtent::Fill,
            height: LayerExtent::Px(32),
        },
        exclusive: ExclusiveZone::Reserve(32),
        margin: Margin::ZERO,
        keyboard: KeyboardMode::None,
        output: OutputTarget::CompositorPicks,
        blur: BlurSpec::None,
        input: RegionSpec::Whole,
    }
}

fn popup_config() -> PopupConfig {
    PopupConfig {
        anchor: PopupAnchor::Rect,
        size: PopupSize::FitContent { max: 400 },
        edge: PopupEdge::Bottom,
        gravity: PopupEdge::Bottom,
        adjust: PopupAdjust::SLIDE_AND_FLIP,
        grab: PopupGrab::Ungrabbed,
        offset: PopupOffset::ZERO,
    }
}

fn bar() -> Element {
    let surface = use_surface();
    rsx! {
        style { "{RESET_CSS}{BAR_CSS}" }
        div { class: "bar",
            // Nothing before the button: its position must match `BUTTON` exactly, for both
            // the anchor rect and the click coordinates the acceptance script drives it with.
            div {
                class: "open",
                onclick: move |_| {
                    // The documented placeholder (module doc comment): one row, shown until
                    // the popup's own layout is measured and it is repositioned to fit.
                    let placement = PopupPlacement {
                        anchor_rect: BUTTON,
                        size: Size {
                            width: 160,
                            height: 24,
                        },
                    };
                    surface.open_popup(popup_config(), placement, HostRoot::new(list));
                },
                "Open",
            }
        }
    }
}

/// The popup's content: a list that starts at `START_ROWS` and grows to `GROWN_ROWS`
/// `GROWTH_DELAY` after the popup opens, on its own, so the demo needs no extra command beyond
/// opening it.
fn list() -> Element {
    let mut rows = use_signal(|| START_ROWS);
    use_hook(|| {
        spawn(async move {
            tokio::time::sleep(GROWTH_DELAY).await;
            rows.set(GROWN_ROWS);
        });
    });
    rsx! {
        style { "{RESET_CSS}{LIST_CSS}" }
        div { class: "list",
            for i in 0..rows() {
                div { key: "{i}", class: "row", "item {i}" }
            }
        }
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("demo_popup: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let host = Host::<()>::connect(HostConfig::default()).map_err(|e| e.to_string())?;
    host.run(move |ctx, event| match event {
        HostEvent::Started => {
            if let Err(error) = shell_host::inject::stdin::install(ctx) {
                eprintln!("debug-inject: {error}");
                ctx.exit();
            }
            if let Err(error) = ctx.spawn_layer(bar_config(), HostRoot::new(bar)) {
                eprintln!("bar: {error}");
                ctx.exit();
            }
        }
        // The compositor closed the bar (its output went away): nothing is left to drive.
        HostEvent::SurfaceClosed { .. } | HostEvent::QuitAsked => ctx.exit(),
        _ => {}
    })
    .map_err(|e| e.to_string())
}

//! A bar menu's keyboard, for `scripts/dev/accept-popup-keyboard.sh`: a 32 px
//! `KeyboardMode::None` bar, like sill's, with one "Menu" button that opens a grabbing popup on
//! its press (as sill's bar items do, so the grab carries the press's serial). The popup's panel
//! focuses itself when it mounts and prints every key that reaches it; Escape closes it.
//!
//! The panel also prints the seat's input modality whenever it changes (`use_input_modality`):
//! `pointer` when it opens from the click, `keyboard` from the first key.
//!
//! Lines on stdout, one JSON object each: `{"event":"opened"}`, `{"event":"key","key":..}`,
//! `{"event":"modality","value":..}` and `{"event":"closed"}`. stdin: `quit`.

#[path = "shared/harness.rs"]
mod harness;

use std::io::{BufRead, Write};

use dioxus::prelude::*;
use serde_json::json;
use shell_host::{
    Anchor, BlurSpec, ExclusiveZone, Host, HostEvent, HostRoot, KeyboardMode, LayerConfig,
    LayerExtent, LayerLevel, LayerSize, Margin, OutputTarget, PopupPlacement, Rect, RegionSpec,
    Size, use_input_modality, use_surface,
};
use shell_host::{
    PopupAdjust, PopupAnchor, PopupConfig, PopupEdge, PopupGrab, PopupOffset, PopupSize,
};

const CSS: &str = "html,body{margin:0;padding:0}\
.bar{height:32px;display:flex;align-items:center;background:rgb(40,40,60);color:white;\
font:14px sans-serif}\
.menu{margin-left:12px;width:70px;height:24px;background:#456;border-radius:4px}\
.panel{width:200px;height:120px;background:rgb(230,230,240);font:14px sans-serif}";

/// The "Menu" button's box (`.menu`), which the acceptance clicks and the popup hangs from.
const BUTTON: Rect = Rect {
    x: 12,
    y: 4,
    width: 70,
    height: 24,
};

fn print(value: serde_json::Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{value}");
    let _ = out.flush();
}

fn bar_config() -> LayerConfig {
    LayerConfig {
        namespace: "popup-keys-bar".to_owned(),
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
        size: PopupSize::Fixed,
        edge: PopupEdge::BottomLeft,
        gravity: PopupEdge::BottomRight,
        adjust: PopupAdjust::SLIDE_AND_FLIP,
        grab: PopupGrab::Grabbed,
        offset: PopupOffset::ZERO,
    }
}

fn bar() -> Element {
    let surface = use_surface();
    rsx! {
        style { "{CSS}" }
        div { class: "bar",
            div {
                class: "menu",
                onmousedown: move |_| {
                    let placement = PopupPlacement {
                        anchor_rect: BUTTON,
                        size: Size {
                            width: 200,
                            height: 120,
                        },
                    };
                    surface.open_popup(popup_config(), placement, HostRoot::new(panel));
                    print(json!({"event": "opened"}));
                },
            }
        }
    }
}

fn panel() -> Element {
    let surface = use_surface();
    let modality = use_input_modality();
    use_effect(move || print(json!({"event": "modality", "value": modality().slug()})));
    rsx! {
        style { "{CSS}" }
        div {
            class: "panel",
            tabindex: "0",
            onmounted: move |event: MountedEvent| async move {
                let _ = event.set_focus(true).await;
            },
            onkeydown: move |event: KeyboardEvent| {
                print(json!({"event": "key", "key": event.key().to_string()}));
                if event.key() == Key::Escape {
                    surface.close();
                    print(json!({"event": "closed"}));
                }
            },
        }
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("demo_popup_keys: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let host = Host::<()>::connect(harness::host_config()).map_err(|e| e.to_string())?;
    let handle = host.handle();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            if line.trim() == "quit" {
                break;
            }
        }
        let _ = handle.send(());
    });
    host.run(move |ctx, event| match event {
        HostEvent::Started => {
            if let Err(error) = ctx.spawn_layer(bar_config(), HostRoot::new(bar)) {
                eprintln!("demo_popup_keys: {error}");
                ctx.exit();
            }
        }
        HostEvent::App(()) => ctx.exit(),
        _ => {}
    })
    .map_err(|e| e.to_string())
}

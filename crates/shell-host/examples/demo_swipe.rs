//! A notification banner swiped away, for `scripts/dev/accept-swipe.sh`: a 400 x 120 Overlay layer
//! anchored top-right whose blur and input regions both follow the card `#card`
//! (`RegionSpec::Elements`, radius 0, so each region is one `wl_region.add`). A press and drag
//! moves the card's wrapper by `translateX(dx)` (quire's `--swipe-dx`); a release past 40 px starts
//! the card's own exit animation (`translateX(0)` to `(420px)` over 400 ms, like quire's
//! `banner-out`), a release short of it puts the card back. The regions are what the acceptance
//! reads, from the client's `WAYLAND_DEBUG` trace.
//!
//! Lines on stdout, one JSON object each: `{"event":"drag","dx":..}` and
//! `{"event":"released","dx":..,"leaving":..}`. stdin: `quit`.

#[path = "shared/harness.rs"]
mod harness;

use std::io::{BufRead, Write};

use dioxus::prelude::*;
use serde_json::json;
use shell_host::{
    Anchor, BlurSpec, ElementId, ElementRegion, ExclusiveZone, Host, HostEvent, HostRoot,
    KeyboardMode, LayerConfig, LayerExtent, LayerLevel, LayerSize, Margin, OutputTarget,
    RegionSpec,
};

const CSS: &str = "html,body{margin:0;padding:0}\
.swipe{position:absolute;left:20px;top:20px}\
#card{width:300px;height:80px;background:rgba(240,240,250,0.8);border-radius:12px}\
#card.leaving{animation:out 400ms linear forwards}\
@keyframes out{from{transform:translateX(0px)}to{transform:translateX(420px)}}";

/// How far a release must have dragged the card for it to leave.
const DISMISS_PX: f64 = 40.0;

fn print(value: serde_json::Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{value}");
    let _ = out.flush();
}

fn card_region() -> RegionSpec {
    RegionSpec::Elements(vec![ElementRegion {
        id: ElementId::new("card"),
        radius: 0,
        inflate: 0,
    }])
}

fn layer_config() -> LayerConfig {
    LayerConfig {
        namespace: "swipe-banner".to_owned(),
        layer: LayerLevel::Overlay,
        anchor: Anchor::TOP | Anchor::RIGHT,
        size: LayerSize {
            width: LayerExtent::Px(400),
            height: LayerExtent::Px(120),
        },
        exclusive: ExclusiveZone::Ignore,
        margin: Margin::ZERO,
        keyboard: KeyboardMode::None,
        output: OutputTarget::CompositorPicks,
        blur: BlurSpec::Region(card_region()),
        input: card_region(),
    }
}

fn banner() -> Element {
    // Where a drag started (the press's x), while the button is held.
    let mut drag = use_signal(|| None::<f64>);
    let mut dx = use_signal(|| 0.0_f64);
    // The card is on its way out.
    let mut exit = use_signal(|| false);
    let class = if exit() { "leaving" } else { "" };
    rsx! {
        style { "{CSS}" }
        div { class: "swipe", style: "transform:translateX({dx}px)",
            div {
                id: "card",
                class,
                onmousedown: move |event: MouseEvent| {
                    drag.set(Some(event.client_coordinates().x - dx()));
                },
                onmousemove: move |event: MouseEvent| {
                    if let Some(start) = drag() {
                        let next = (event.client_coordinates().x - start).max(0.0);
                        dx.set(next);
                        print(json!({"event": "drag", "dx": next}));
                    }
                },
                onmouseup: move |_| {
                    drag.set(None);
                    let leaving = dx() > DISMISS_PX;
                    if leaving {
                        exit.set(true);
                    } else {
                        dx.set(0.0);
                    }
                    print(json!({"event": "released", "dx": dx(), "leaving": leaving}));
                },
            }
        }
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("demo_swipe: {error}");
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
            if let Err(error) = ctx.spawn_layer(layer_config(), HostRoot::new(banner)) {
                eprintln!("demo_swipe: {error}");
                ctx.exit();
            }
        }
        HostEvent::App(()) | HostEvent::QuitAsked => ctx.exit(),
        _ => {}
    })
    .map_err(|e| e.to_string())
}

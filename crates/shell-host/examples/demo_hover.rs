//! Two stacked cards for real-input hover acceptance (FINDINGS F57, F58): two 320 x 120 layer
//! surfaces centred on the output, `bottom` on the Top layer and `top` on the Overlay layer
//! right over it, so the pointer is on `top` until it is hidden and then on `bottom` without
//! moving. Each card prints `hover: <card> enter` / `hover: <card> leave` from its own
//! `onmouseenter` / `onmouseleave`, which is what a `kwin-virtual.sh` + `fake-input.sh` run reads.
//!
//! stdin, one command per line: `hide top|bottom` and `show top|bottom` (a
//! `SurfaceOp::SetVisibility`, printed as `hover: hide top` when applied), `quit`.

use std::io::{BufRead, Write};

use dioxus::prelude::*;
use shell_host::{
    Anchor, BlurSpec, ExclusiveZone, Extent, Host, HostConfig, HostEvent, KeyboardMode, Layer,
    LayerConfig, LayerSize, Margin, OutputTarget, RegionSpec, Root, SurfaceId, Visibility,
};

const CSS: &str = "html,body{margin:0;padding:0}\
.card{width:320px;height:120px;box-sizing:border-box;padding:16px;font:16px sans-serif;\
color:white}.card:hover{outline:4px solid white;outline-offset:-4px}\
.top{background:rgb(200,60,40)}.bottom{background:rgb(40,90,200)}";

/// Which of the two cards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Card {
    Top,
    Bottom,
}

impl Card {
    fn name(self) -> &'static str {
        match self {
            Card::Top => "top",
            Card::Bottom => "bottom",
        }
    }

    /// `top` sits one layer above `bottom`, so it is the one under the pointer while shown.
    fn layer(self) -> Layer {
        match self {
            Card::Top => Layer::Overlay,
            Card::Bottom => Layer::Top,
        }
    }
}

/// Flushed at once: the acceptance reads these lines while the demo runs.
fn say(line: &str) {
    let mut err = std::io::stderr().lock();
    let _ = writeln!(err, "hover: {line}");
    let _ = err.flush();
}

fn card(which: Card) -> Element {
    let name = which.name();
    rsx! {
        style { "{CSS}" }
        div {
            class: "card {name}",
            onmouseenter: move |_| say(&format!("{name} enter")),
            onmouseleave: move |_| say(&format!("{name} leave")),
            "{name}"
        }
    }
}

fn config(which: Card) -> LayerConfig {
    LayerConfig {
        namespace: format!("hover-{}", which.name()),
        layer: which.layer(),
        anchor: Anchor::NONE,
        size: LayerSize {
            width: Extent::Px(320),
            height: Extent::Px(120),
        },
        exclusive: ExclusiveZone::Auto,
        margin: Margin::ZERO,
        keyboard: KeyboardMode::None,
        output: OutputTarget::CompositorPicks,
        blur: BlurSpec::None,
        input: RegionSpec::Whole,
    }
}

/// A stdin line, parsed on the reader thread.
#[derive(Debug, Clone, Copy)]
enum Command {
    Set(Card, Visibility),
    Quit,
}

fn parse(line: &str) -> Option<Command> {
    let words: Vec<&str> = line.split_whitespace().collect();
    let card = |name: &str| match name {
        "top" => Some(Card::Top),
        "bottom" => Some(Card::Bottom),
        _ => None,
    };
    match words.as_slice() {
        ["hide", name] => card(name).map(|c| Command::Set(c, Visibility::Hidden)),
        ["show", name] => card(name).map(|c| Command::Set(c, Visibility::Shown)),
        ["quit"] => Some(Command::Quit),
        _ => None,
    }
}

/// The two surfaces once spawned.
#[derive(Debug, Default)]
struct Cards {
    top: Option<SurfaceId>,
    bottom: Option<SurfaceId>,
}

impl Cards {
    fn get(&self, which: Card) -> Option<SurfaceId> {
        match which {
            Card::Top => self.top,
            Card::Bottom => self.bottom,
        }
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("demo_hover: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let host = Host::<Command>::connect(HostConfig::default()).map_err(|e| e.to_string())?;
    let handle = host.handle();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            match parse(&line) {
                Some(command) => {
                    let _ = handle.send(command);
                }
                None => eprintln!("demo_hover: unknown command {line:?}"),
            }
        }
        let _ = handle.send(Command::Quit);
    });
    let mut cards = Cards::default();
    host.run(move |ctx, event| match event {
        HostEvent::Started => {
            // Bottom first, so its surface exists before the one that covers it.
            for which in [Card::Bottom, Card::Top] {
                match ctx.spawn_layer(config(which), Root::with_props(card, which)) {
                    Ok(id) if which == Card::Top => cards.top = Some(id),
                    Ok(id) => cards.bottom = Some(id),
                    Err(error) => {
                        eprintln!("demo_hover: {}: {error}", which.name());
                        ctx.exit();
                    }
                }
            }
        }
        HostEvent::App(Command::Set(which, visibility)) => {
            if let Some(id) = cards.get(which) {
                let verb = match visibility {
                    Visibility::Hidden => "hide",
                    Visibility::Shown => "show",
                };
                say(&format!("{verb} {}", which.name()));
                ctx.set_visibility(id, visibility);
            }
        }
        HostEvent::App(Command::Quit) | HostEvent::SurfaceClosed { .. } => ctx.exit(),
        _ => {}
    })
    .map_err(|e| e.to_string())
}

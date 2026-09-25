//! Two stacked cards for real-input hover acceptance (FINDINGS F57, F58): two 320 x 120 layer
//! surfaces centred on the output, `bottom` on the Top layer and `top` on the Overlay layer
//! right over it, so the pointer is on `top` until it is hidden and then on `bottom` without
//! moving. Each card prints `hover: <card> enter` / `hover: <card> leave` from its own
//! `onmouseenter` / `onmouseleave`, which is what a `kwin-virtual.sh` + `fake-input.sh` run reads.
//!
//! A third surface, `slide` (320 x 120 at the output's bottom-left corner), is F59's: a `pad`
//! filling it, and a `slider` that, 600 ms after the first pointer move on the pad, is placed
//! under wherever the pointer then rests. Blitz notices the slider only in its own
//! `refresh_hover` at the end of a resolve, which dispatches nothing; the host's repair is what
//! gives the slider its enter and the pad its leave. Both elements count their
//! `onpointerenter` / `onpointerleave` and print `hover: slide <element> enter|leave`.
//!
//! stdin, one command per line: `hide top|bottom` and `show top|bottom` (a
//! `SurfaceOp::SetVisibility`, printed as `hover: hide top` when applied), `stats` (prints
//! `hover: stats {"pad":{"enter":N,"leave":N},"slider":{...}}`, the slide surface's counts),
//! `quit`.

use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

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

/// How long after the first move on the pad the slider is placed under the pointer.
const SLIDE_DELAY: Duration = Duration::from_millis(600);

const SLIDE_CSS: &str = "html,body{margin:0;padding:0}\
.pad{width:320px;height:120px;background:rgb(40,140,80)}\
.slider{position:absolute;width:80px;height:40px;background:rgb(230,200,40)}\
.slider:hover{outline:3px solid white;outline-offset:-3px}";

/// One element's pointer enter and leave counts, shared with the `stats` command.
struct Tally {
    enter: AtomicU32,
    leave: AtomicU32,
}

impl Tally {
    const fn new() -> Tally {
        Tally {
            enter: AtomicU32::new(0),
            leave: AtomicU32::new(0),
        }
    }

    fn count(&self, name: &str, crossing: Crossing) {
        let (counter, verb) = match crossing {
            Crossing::Enter => (&self.enter, "enter"),
            Crossing::Leave => (&self.leave, "leave"),
        };
        counter.fetch_add(1, Ordering::SeqCst);
        say(&format!("slide {name} {verb}"));
    }

    fn json(&self) -> String {
        format!(
            "{{\"enter\":{},\"leave\":{}}}",
            self.enter.load(Ordering::SeqCst),
            self.leave.load(Ordering::SeqCst)
        )
    }
}

/// Which way the pointer crossed an element's edge.
#[derive(Debug, Clone, Copy)]
enum Crossing {
    Enter,
    Leave,
}

static PAD: Tally = Tally::new();
static SLIDER: Tally = Tally::new();

/// Whether the slider's timer was started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Timer {
    Idle,
    Started,
}

fn slide() -> Element {
    let mut pointer = use_signal(|| (0.0, 0.0));
    let mut placed = use_signal(|| None::<(f64, f64)>);
    let mut timer = use_signal(|| Timer::Idle);
    let onmove = move |event: PointerEvent| {
        let at = event.client_coordinates();
        pointer.set((at.x, at.y));
        if timer() == Timer::Idle {
            timer.set(Timer::Started);
            spawn(async move {
                tokio::time::sleep(SLIDE_DELAY).await;
                let (x, y) = *pointer.peek();
                say(&format!("slide placed {x} {y}"));
                placed.set(Some((x, y)));
            });
        }
    };
    rsx! {
        style { "{SLIDE_CSS}" }
        div {
            class: "pad",
            onpointermove: onmove,
            onpointerenter: |_| PAD.count("pad", Crossing::Enter),
            onpointerleave: |_| PAD.count("pad", Crossing::Leave),
        }
        if let Some((x, y)) = placed() {
            div {
                class: "slider",
                style: "left:{x - 40.0}px;top:{y - 20.0}px",
                onpointerenter: |_| SLIDER.count("slider", Crossing::Enter),
                onpointerleave: |_| SLIDER.count("slider", Crossing::Leave),
            }
        }
    }
}

fn slide_config() -> LayerConfig {
    LayerConfig {
        namespace: "hover-slide".to_string(),
        layer: Layer::Overlay,
        anchor: Anchor::BOTTOM.union(Anchor::LEFT),
        ..config(Card::Top)
    }
}

/// A stdin line, parsed on the reader thread.
#[derive(Debug, Clone, Copy)]
enum Command {
    Set(Card, Visibility),
    Stats,
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
        ["stats"] => Some(Command::Stats),
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
            if let Err(error) = ctx.spawn_layer(slide_config(), Root::new(slide)) {
                eprintln!("demo_hover: slide: {error}");
                ctx.exit();
            }
        }
        HostEvent::App(Command::Stats) => {
            say(&format!(
                "stats {{\"pad\":{},\"slider\":{}}}",
                PAD.json(),
                SLIDER.json()
            ));
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

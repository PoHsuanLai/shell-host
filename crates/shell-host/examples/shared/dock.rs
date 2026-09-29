//! The dock shared by `demo_dock` and `demo_bar --with-dock`: twelve 48 px tiles magnified on
//! hover, its layer config, regions, stdin options and commands.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use dioxus::prelude::*;
use shell_host::{
    Anchor, BlurSpec, ElementId, ElementRegion, ExclusiveZone, FrameDrive, FrameTick, HostCtx,
    HostRoot, KeyboardMode, LayerConfig, LayerExtent, LayerLevel, LayerSize, Margin, OutputTarget,
    RegionSpec, use_frame_tick, use_surface,
};

/// Tiles, gaps and the room the magnified end tiles need.
pub const DOCK_WIDTH: u32 = 12 * 48 + 11 * 8 + 96;
pub const DOCK_HEIGHT: u32 = 96;

/// The second element's (`--cards=two`) size, and the gap it keeps from the pill.
const CARD_WIDTH: u32 = 220;
const CARD_HEIGHT: u32 = 40;
const CARDS_GAP: u32 = 16;

/// How the tiles magnify.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Magnify {
    /// `transform: scale()` transitions (no relayout).
    Transform,
    /// `width`/`height` transitions (relayout every frame).
    Width,
    /// Scales computed by the component on every frame tick (`use_frame_tick`), no CSS motion.
    Tick,
}

/// How many independently region-tracked elements the dock renders: the pill alone
/// (`RegionSpec::Element`), or the pill plus a small card above it, each following its own
/// strips (`RegionSpec::Elements`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Cards {
    #[default]
    One,
    Two,
}

/// Props for the dock's root component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DockProps {
    pub magnify: Magnify,
    pub cards: Cards,
}

/// The margin and padding reset quire's `css/reset.css` would bring; the host's UA sheet sizes
/// the root but leaves the reset to the design system.
const RESET_CSS: &str = "html,body{margin:0;padding:0}";

/// `.dock`, `.pill`, `.icon` and `.card`, sized for a dock `height` px tall (`DOCK_HEIGHT` for one
/// element, `dock_height(Cards::Two)` for two). In-flow layout on purpose, `.card` included: with
/// the row `position:absolute` Blitz hit-testing never reached the icons, so
/// `flex-direction:column` stacks the card above the pill instead, both centred and the pill flush
/// with the bottom exactly as the single-element layout was.
fn dock_css(height: u32) -> String {
    format!(
        ".dock{{height:{height}px;display:flex;flex-direction:column;align-items:center;\
justify-content:flex-end;gap:{CARDS_GAP}px;padding-bottom:8px;box-sizing:border-box}}\
.pill{{display:flex;align-items:flex-end;gap:8px;padding:8px;border-radius:12px;\
background:rgba(30,30,40,0.35)}}\
.icon{{width:48px;height:48px;border-radius:12px;flex:none}}\
.card{{width:{CARD_WIDTH}px;height:{CARD_HEIGHT}px;border-radius:8px;\
background:rgba(255,255,255,0.45)}}"
    )
}

/// The dock's total height for `cards`: just the pill's row, or the card and its gap added.
fn dock_height(cards: Cards) -> u32 {
    match cards {
        Cards::One => DOCK_HEIGHT,
        Cards::Two => DOCK_HEIGHT + CARD_HEIGHT + CARDS_GAP,
    }
}

const TRANSFORM_CSS: &str = ".icon{transition:transform 170ms;transform-origin:50% 100%}\
.icon:hover{transform:scale(1.5)}";

const WIDTH_CSS: &str =
    ".icon{transition:width 170ms,height 170ms}.icon:hover{width:72px;height:72px}";

const COLOURS: [&str; 12] = [
    "#e6194b", "#3cb44b", "#ffe119", "#4363d8", "#f58231", "#911eb4", "#46f0f0", "#f032e6",
    "#bcf60c", "#fabebe", "#008080", "#e6beff",
];

/// The pill alone (`Cards::One`), or the pill and the card above it (`Cards::Two`): input and
/// blur both follow the same elements.
fn dock_region(cards: Cards) -> RegionSpec {
    let pill = ElementRegion {
        id: ElementId::new("dock-pill"),
        radius: 12,
        inflate: 0,
    };
    match cards {
        Cards::One => RegionSpec::Element {
            id: pill.id,
            radius: pill.radius,
            inflate: pill.inflate,
        },
        Cards::Two => RegionSpec::Elements(vec![
            pill,
            ElementRegion {
                id: ElementId::new("dock-card"),
                radius: 8,
                inflate: 0,
            },
        ]),
    }
}

/// The dock's layer surface: bottom-centred, never focused, input and blur on `cards`'
/// elements, tall enough to hold them.
pub fn dock_config(layer: LayerLevel, cards: Cards) -> LayerConfig {
    let region = dock_region(cards);
    LayerConfig {
        namespace: "dock".to_owned(),
        layer,
        anchor: Anchor::BOTTOM,
        size: LayerSize {
            width: LayerExtent::Px(DOCK_WIDTH),
            height: LayerExtent::Px(dock_height(cards)),
        },
        exclusive: ExclusiveZone::Auto,
        margin: Margin::ZERO,
        keyboard: KeyboardMode::None,
        output: OutputTarget::CompositorPicks,
        blur: BlurSpec::Region(region.clone()),
        input: region,
    }
}

/// Presses the card (`Cards::Two`) has received, printed as `dock: card press N` so a real-input
/// run can check they land only when the card's own region is hit (matching `demo_bar`'s
/// `bar: press N`).
static CARD_PRESSES: AtomicU32 = AtomicU32::new(0);

/// The optional second region-tracked element. Never `position:absolute` (see `dock_css`): an
/// absolutely-positioned sibling here lost pointer hit-testing.
fn card(cards: Cards) -> Element {
    match cards {
        Cards::One => rsx! {},
        Cards::Two => rsx! {
            div {
                id: "dock-card",
                class: "card",
                onmousedown: move |_| {
                    let count = CARD_PRESSES.fetch_add(1, Ordering::Relaxed) + 1;
                    eprintln!("dock: card press {count}");
                },
            }
        },
    }
}

/// The dock document for `props`.
pub fn dock(props: DockProps) -> Element {
    let magnify = match props.magnify {
        Magnify::Transform => TRANSFORM_CSS,
        Magnify::Width => WIDTH_CSS,
        Magnify::Tick => return rsx! { TickDock { cards: props.cards } },
    };
    let css = dock_css(dock_height(props.cards));
    rsx! {
        style { "{RESET_CSS}{css}{magnify}" }
        div { class: "dock",
            {card(props.cards)}
            div { id: "dock-pill", class: "pill",
                for (i, colour) in COLOURS.iter().enumerate() {
                    div { key: "{i}", class: "icon", style: "background:{colour}" }
                }
            }
        }
    }
}

/// Tile scales in thousandths.
type Scales = [u32; 12];

const REST: Scales = [1000; 12];

/// The magnified scale at the tile centre under the pointer, and the reach of the effect.
const PEAK: f64 = 1500.0;
const REACH_PX: f64 = 112.0;

/// How fast a tile closes on its target: the gap decays as `exp(-t / TIME_CONSTANT)`.
const TIME_CONSTANT: Duration = Duration::from_millis(60);

/// Close enough to the target to stop moving (thousandths).
const SETTLED: u32 = 4;

/// The centre of tile `i`, in surface-local logical px: the pill is centred in the dock with
/// 8 px padding, 48 px tiles and 8 px gaps.
fn tile_centre(i: usize) -> f64 {
    let pill = 12.0 * 48.0 + 11.0 * 8.0 + 16.0;
    let left = (f64::from(DOCK_WIDTH) - pill) / 2.0;
    left + 8.0 + i as f64 * 56.0 + 24.0
}

/// The scales the tiles move towards with the pointer at `x` (none: at rest).
fn targets(x: Option<f64>) -> Scales {
    let Some(x) = x else { return REST };
    std::array::from_fn(|i| {
        let d = ((x - tile_centre(i)) / REACH_PX).abs();
        let lift = (1.0 - d * d).max(0.0);
        (1000.0 + (PEAK - 1000.0) * lift).round() as u32
    })
}

/// One tick of motion: each tile closes `1 - exp(-dt / TIME_CONSTANT)` of its gap to
/// `target`, at least one thousandth, and snaps once within `SETTLED`. The one-unit floor is
/// what makes it settle at any tick rate: rounded to thousandths, a 1 ms tick (nested
/// cosmic-comp) would otherwise move nothing once the gap is under 30.
fn step(now: Scales, target: Scales, dt: Duration) -> Scales {
    let k = 1.0 - (-dt.as_secs_f64() / TIME_CONSTANT.as_secs_f64()).exp();
    std::array::from_fn(|i| {
        let gap = now[i].abs_diff(target[i]);
        let moved = ((f64::from(gap) * k).round() as u32).max(1).min(gap);
        let next = if now[i] < target[i] {
            now[i] + moved
        } else {
            now[i] - moved
        };
        if next.abs_diff(target[i]) <= SETTLED {
            target[i]
        } else {
            next
        }
    })
}

/// The dock with magnification run by the component at the compositor's frame rate: pointer
/// motion takes a `FrameDrive`, every `FrameTick` steps the scales, and the drive is dropped
/// once the tiles reach their targets (at rest after the pointer leaves, or still under a
/// resting pointer), so idle has zero frames.
#[component]
fn TickDock(cards: Cards) -> Element {
    let surface = use_surface();
    let tick = use_frame_tick();
    let mut pointer = use_signal(|| None::<f64>);
    let mut scales = use_signal(|| REST);
    let mut drive = use_signal(|| None::<FrameDrive>);
    let leaving = surface.clone();
    use_effect(move || {
        let FrameTick { dt, .. } = tick();
        let target = targets(*pointer.peek());
        let next = step(*scales.peek(), target, dt);
        if next != *scales.peek() {
            scales.set(next);
        }
        if next == target && drive.peek().is_some() {
            drive.set(None);
        }
    });
    let css = dock_css(dock_height(cards));
    rsx! {
        style { "{RESET_CSS}{css}.icon{{transform-origin:50% 100%}}" }
        div {
            class: "dock",
            onmousemove: move |event| {
                pointer.set(Some(event.client_coordinates().x));
                hold(drive, &surface);
            },
            onmouseleave: move |_| {
                pointer.set(None);
                hold(drive, &leaving);
            },
            {card(cards)}
            div { id: "dock-pill", class: "pill",
                for (i, colour) in COLOURS.iter().enumerate() {
                    div {
                        key: "{i}",
                        class: "icon",
                        style: "background:{colour};transform:scale({f64::from(scales()[i]) / 1000.0})",
                    }
                }
            }
        }
    }
}

/// Take a drive for `surface` unless one is held already.
fn hold(mut drive: Signal<Option<FrameDrive>>, surface: &shell_host::SurfaceHandle) {
    if drive.peek().is_none() {
        drive.set(Some(surface.drive_frames()));
    }
}

/// `--mode=transform|width|tick`, `--cards=one|two` and `--layer=top|overlay|bottom` from
/// `args`.
pub fn dock_options(args: &[String]) -> Result<(DockProps, LayerLevel), String> {
    let magnify = match value(args, "--mode").as_deref() {
        None | Some("transform") => Magnify::Transform,
        Some("width") => Magnify::Width,
        Some("tick") => Magnify::Tick,
        Some(other) => return Err(format!("unknown --mode={other}")),
    };
    let cards = match value(args, "--cards").as_deref() {
        None | Some("one") => Cards::One,
        Some("two") => Cards::Two,
        Some(other) => return Err(format!("unknown --cards={other}")),
    };
    Ok((DockProps { magnify, cards }, layer_option(args)?))
}

/// `--layer=top|overlay|bottom`, top by default.
pub fn layer_option(args: &[String]) -> Result<LayerLevel, String> {
    match value(args, "--layer").as_deref() {
        None | Some("top") => Ok(LayerLevel::Top),
        Some("overlay") => Ok(LayerLevel::Overlay),
        Some("bottom") => Ok(LayerLevel::Bottom),
        Some(other) => Err(format!("unknown --layer={other}")),
    }
}

/// The value of `--name=value` in `args`.
pub fn value(args: &[String], name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    args.iter()
        .find_map(|arg| arg.strip_prefix(&prefix).map(str::to_owned))
}

/// Spawn the dock; a failure ends the demo.
pub fn spawn_dock<M: 'static>(ctx: &mut HostCtx<M>, props: DockProps, layer: LayerLevel) {
    let config = dock_config(layer, props.cards);
    if let Err(error) = ctx.spawn_layer(config, HostRoot::with_props(dock, props)) {
        eprintln!("dock: {error}");
        ctx.exit();
    }
}

/// `debug-inject` on stdin; a failure ends the demo, since nothing could drive it.
pub fn install_commands<M: 'static>(ctx: &mut HostCtx<M>) {
    if let Err(error) = shell_host::inject::stdin::install(ctx) {
        eprintln!("debug-inject: {error}");
        ctx.exit();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Simulated time until `step` reaches `target` from `from` at a fixed tick interval, or
    /// `None` if it has not within 2 s.
    fn settle_time(from: Scales, target: Scales, dt: Duration) -> Option<Duration> {
        let mut now = from;
        let mut elapsed = Duration::ZERO;
        while elapsed < Duration::from_secs(2) {
            now = step(now, target, dt);
            elapsed += dt;
            if now == target {
                return Some(elapsed);
            }
        }
        None
    }

    /// Nested cosmic-comp calls back every 1-2 ms (mailbox, no vblank), KWin at 60 Hz: the
    /// tiles settle, and the drive is dropped, at every rate, within a few time constants.
    #[test]
    fn the_tiles_settle_at_any_tick_rate() {
        let hovered = targets(Some(tile_centre(5)));
        let rates_us = [500, 1_000, 1_500, 2_000, 6_944, 16_667, 33_333];
        for us in rates_us {
            let dt = Duration::from_micros(us);
            for (from, to) in [(hovered, REST), (REST, hovered)] {
                let took = settle_time(from, to, dt);
                assert!(
                    took.is_some_and(|t| t <= Duration::from_millis(500)),
                    "dt {us} us: settled after {took:?}"
                );
            }
        }
    }

    /// A step never passes its target, so the scales never oscillate around it.
    #[test]
    fn a_step_never_overshoots() {
        let hovered = targets(Some(tile_centre(5)));
        for us in [1_000, 16_667, 200_000] {
            let next = step(hovered, REST, Duration::from_micros(us));
            assert!(next.iter().all(|&s| (1000..=1500).contains(&s)), "{next:?}");
        }
    }
}

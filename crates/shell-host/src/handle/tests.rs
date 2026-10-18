use blitz_dom::Document as _;
use blitz_traits::shell::{ColorScheme, Viewport};
use dioxus::prelude::*;

use std::sync::Arc;

use crate::blur::BlurSpec;
use crate::root::HostRoot;

use super::*;
use crate::clipboard::SharedSelection;
use crate::dom::document::Polled;
use crate::dom::document::{DocumentEnv, SurfaceDocument};
use crate::dom::shell_provider::HostShellProvider;
use crate::grab::{GrabRecord, GrabRefused, PointerFacts, Press};
use crate::handle::{
    HandleParts, RequestQueue, use_frame_tick, use_surface, use_surface_scale, use_toplevel_state,
};
use crate::id::SurfaceIds;
use crate::live::Live;
use crate::msg::{Request, SurfaceOp};
use crate::prepaint::{Prepaint, Prepainted};
use crate::role::RoleKind;
use crate::tick::{FrameTick, HostInstant};
use crate::toplevel::{Maximize, ResizeEdge, ToplevelState};
use blitz_kit::fonts::SharedFonts;
use blitz_kit::net::LocalNet;

fn parts(requests: RequestQueue, ticks: FrameTicks, scale: LiveScale) -> HandleParts {
    HandleParts {
        requests,
        ids: SurfaceIds::new(),
        ticks,
        scale,
        toplevel: Live::new(ToplevelState::default()),
        grab: GrabRecord::default(),
        scrolls: crate::scroll::watch::ScrollBoard::default(),
        wheels: crate::scroll::wheel_feed::WheelFeed::default(),
        modifiers: Live::new(crate::modifiers::Modifiers::default()),
        modality: Live::new(crate::modality::InputModality::default()),
        wl: crate::handle::WlCell::detached(),
        placement: crate::window_placement::PlacementSupport::Absent,
    }
}

fn app() -> Element {
    let surface = use_surface();
    use_hook(|| surface.set_blur(BlurSpec::None));
    rsx! { div { "surface {surface.id().0}" } }
}

fn ticking() -> Element {
    let tick = use_frame_tick();
    let FrameTick { at, dt } = tick();
    rsx! { div { "at {at.0.as_millis()} dt {dt.as_millis()}" } }
}

fn text(document: &mut SurfaceDocument) -> String {
    let mut text = String::new();
    document.dioxus().inner().visit(|_, node| {
        if node.is_text_node() {
            text.push_str(&node.text_content());
        }
    });
    text
}

fn test_env(ping: calloop::ping::Ping, prepaint: Prepaint) -> DocumentEnv {
    DocumentEnv {
        shell: Arc::new(HostShellProvider::new(ping, SharedSelection::default())),
        fonts: SharedFonts::system().for_document(),
        net: Arc::new(LocalNet::default()),
        viewport: Viewport::new(200, 40, 1.0, ColorScheme::Light),
        prepaint,
    }
}

/// Counts its wakes, so a test polls a document only when the host would.
#[derive(Default)]
struct Wakes(std::sync::atomic::AtomicU32);

impl std::task::Wake for Wakes {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Wakes {
    fn take(&self) -> u32 {
        self.0.swap(0, std::sync::atomic::Ordering::SeqCst)
    }
}

/// `use_frame_tick` re-renders its component with every tick the host publishes, through
/// the document's own task and waker, and not before. The document is polled only when
/// its waker fired, as the flush does, so a tick whose wake is lost fails here.
#[test]
fn use_frame_tick_renders_each_published_tick() {
    let (ping, _source) = calloop::ping::make_ping().expect("eventfd ping");
    let requests = RequestQueue::new(ping.clone());
    let id = SurfaceId(7);
    let ticks = FrameTicks::default();
    let handle = SurfaceHandle::new(
        id,
        RoleKind::Toplevel,
        parts(
            requests.clone(),
            ticks.clone(),
            LiveScale::new(Scale120::ONE),
        ),
    );
    let env = test_env(ping, Prepaint::None);
    let mut document = SurfaceDocument::build(HostRoot::new(ticking).context(handle.clone()), env);
    let wakes = Arc::new(Wakes::default());
    let waker = std::task::Waker::from(Arc::clone(&wakes));
    let mut cx = std::task::Context::from_waker(&waker);
    document.poll(&mut cx);
    assert_eq!(text(&mut document), "at 0 dt 0");

    let drive = handle.drive_frames();
    assert!(
        matches!(requests.drain().as_slice(), [Request::Redraw(SurfaceId(7))]),
        "taking a drive asks for a frame"
    );
    let ms = |n| HostInstant(std::time::Duration::from_millis(n));
    let frames = [
        (100, "at 100 dt 0"),
        (116, "at 116 dt 16"),
        (133, "at 133 dt 17"),
        (150, "at 150 dt 17"),
    ];
    for (at, want) in frames {
        wakes.take();
        ticks.frame_due(crate::frame::Animation::Stopped, ms(at));
        assert!(wakes.take() > 0, "the tick at {at} ms woke the document");
        assert_eq!(document.poll(&mut cx), Polled::Changed, "{want}");
        assert_eq!(text(&mut document), want);
    }
    drop(drive);
    ticks.frame_due(crate::frame::Animation::Stopped, ms(166));
    assert_eq!(wakes.take(), 0, "no tick without a drive");
    assert_eq!(text(&mut document), "at 150 dt 17");
}

#[test]
fn a_spawned_root_reads_its_surface_handle() {
    let (ping, _source) = calloop::ping::make_ping().expect("eventfd ping");
    let requests = RequestQueue::new(ping.clone());
    let id = SurfaceId(42);
    let handle = SurfaceHandle::new(
        id,
        RoleKind::Toplevel,
        parts(
            requests.clone(),
            FrameTicks::default(),
            LiveScale::new(Scale120::ONE),
        ),
    );
    let env = test_env(ping, Prepaint::None);

    let mut document = SurfaceDocument::build(HostRoot::new(app).context(handle), env);

    let mut text = String::new();
    document.dioxus().inner().visit(|_, node| {
        if node.is_text_node() {
            text.push_str(&node.text_content());
        }
    });
    assert!(text.contains("surface 42"), "rendered {text:?}");
    let queued = requests.drain();
    assert!(
        matches!(
            queued.as_slice(),
            [Request::Apply {
                id: SurfaceId(42),
                op: SurfaceOp::SetBlur(BlurSpec::None)
            }]
        ),
        "the handle queues into the host: {queued:?}"
    );
}

fn scaled() -> Element {
    let scale = use_surface_scale();
    rsx! { div { "scale {scale().0}" } }
}

/// The scale is a root context before the first render, and each change the host
/// publishes re-renders the reader through its own task and waker; a repeated scale wakes
/// nothing, so it costs no frame.
#[test]
fn use_surface_scale_starts_at_the_published_scale_and_follows_each_change() {
    let (ping, _source) = calloop::ping::make_ping().expect("eventfd ping");
    let id = SurfaceId(9);
    let live = LiveScale::new(Scale120(150));
    let handle = SurfaceHandle::new(
        id,
        RoleKind::Toplevel,
        parts(
            RequestQueue::new(ping.clone()),
            FrameTicks::default(),
            live.clone(),
        ),
    );
    let env = test_env(ping, Prepaint::None);
    let mut document = SurfaceDocument::build(HostRoot::new(scaled).context(handle.clone()), env);
    assert_eq!(
        text(&mut document),
        "scale 150",
        "the first render has the scale"
    );
    assert_eq!(handle.current_scale(), Scale120(150));

    let wakes = Arc::new(Wakes::default());
    let waker = std::task::Waker::from(Arc::clone(&wakes));
    let mut cx = std::task::Context::from_waker(&waker);
    document.poll(&mut cx);
    wakes.take();
    live.publish(Scale120(150));
    assert_eq!(wakes.take(), 0, "an unchanged scale wakes nothing");
    for wire in [180, 210, 120] {
        live.publish(Scale120(wire));
        assert!(wakes.take() > 0, "publishing {wire} woke the document");
        assert_eq!(document.poll(&mut cx), Polled::Changed);
        assert_eq!(text(&mut document), format!("scale {wire}"));
    }
    assert_eq!(
        document.poll(&mut cx),
        Polled::Unchanged,
        "then it is quiet"
    );
}

/// The hook runs exactly once per `resolve`, after layout, at the viewport's scale (what
/// a device-grid snap reads), and never on a build or a poll.
#[test]
fn resolve_runs_the_prepaint_hook_once_after_layout() {
    let (ping, _source) = calloop::ping::make_ping().expect("eventfd ping");
    let id = SurfaceId(11);
    let seen = Arc::new(std::sync::Mutex::new(Vec::<(f64, f32)>::new()));
    let record = Arc::clone(&seen);
    let hook = Prepaint::hook(move |doc| {
        let height = doc.root_element().final_layout().size.height;
        let entry = (doc.viewport().scale_f64(), height);
        record.lock().expect("unpoisoned").push(entry);
    });
    let env = DocumentEnv {
        viewport: Viewport::new(300, 60, 1.5, ColorScheme::Light),
        ..test_env(ping.clone(), hook)
    };
    let handle = SurfaceHandle::new(
        id,
        RoleKind::Toplevel,
        parts(
            RequestQueue::new(ping),
            FrameTicks::default(),
            LiveScale::new(Scale120(180)),
        ),
    );
    let mut document = SurfaceDocument::build(HostRoot::new(app).context(handle), env);
    let waker = std::task::Waker::noop();
    document.poll(&mut std::task::Context::from_waker(waker));
    let calls = || seen.lock().expect("unpoisoned").clone();
    assert!(calls().is_empty(), "building and polling never run it");

    assert_eq!(document.resolve(0.0).prepainted, Prepainted::Ran);
    let first = calls();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].0, 1.5, "the hook sees the viewport's scale");
    assert!(first[0].1 > 0.0, "and a laid-out document: {first:?}");
    assert_eq!(document.resolve(0.1).prepainted, Prepainted::Ran);
    assert_eq!(calls().len(), 2, "once per resolve");
}

#[test]
fn without_a_hook_resolve_reports_none_ran() {
    let (ping, _source) = calloop::ping::make_ping().expect("eventfd ping");
    let id = SurfaceId(12);
    let handle = SurfaceHandle::new(
        id,
        RoleKind::Toplevel,
        parts(
            RequestQueue::new(ping.clone()),
            FrameTicks::default(),
            LiveScale::new(Scale120::ONE),
        ),
    );
    let env = test_env(ping, Prepaint::None);
    let mut document = SurfaceDocument::build(HostRoot::new(app).context(handle), env);
    assert_eq!(document.resolve(0.0).prepainted, Prepainted::NoHook);
}

fn zoom_state() -> Element {
    let state = use_toplevel_state();
    rsx! { div { "{state().maximized:?}" } }
}

/// The configure state reaches the document as live state: the first render has what the
/// host knew, each change re-renders, and a configure that changes nothing wakes nothing.
#[test]
fn use_toplevel_state_follows_each_configure() {
    let (ping, _source) = calloop::ping::make_ping().expect("eventfd ping");
    let id = SurfaceId(13);
    let live = Live::new(ToplevelState::default());
    let handle = SurfaceHandle::new(
        id,
        RoleKind::Toplevel,
        HandleParts {
            toplevel: live.clone(),
            ..parts(
                RequestQueue::new(ping.clone()),
                FrameTicks::default(),
                LiveScale::new(Scale120::ONE),
            )
        },
    );
    let env = test_env(ping, Prepaint::None);
    let mut document =
        SurfaceDocument::build(HostRoot::new(zoom_state).context(handle.clone()), env);
    assert_eq!(text(&mut document), "Unset");
    let wakes = Arc::new(Wakes::default());
    let waker = std::task::Waker::from(Arc::clone(&wakes));
    let mut cx = std::task::Context::from_waker(&waker);
    document.poll(&mut cx);
    wakes.take();
    live.publish(ToplevelState::default());
    assert_eq!(wakes.take(), 0, "an unchanged state wakes nothing");
    let zoomed = ToplevelState {
        maximized: Maximize::Set,
        ..ToplevelState::default()
    };
    live.publish(zoomed);
    assert!(wakes.take() > 0);
    assert_eq!(document.poll(&mut cx), Polled::Changed);
    assert_eq!(text(&mut document), "Set");
    assert_eq!(handle.current_toplevel_state(), zoomed);
}

fn wheel_phases() -> Element {
    let mut seen = use_signal(Vec::new);
    crate::handle::use_wheel(move |event| seen.write().push((event.phase, event.by.y)));
    rsx! { div { "{seen():?}" } }
}

/// A wheel event the host publishes reaches the component through its own task and waker, in
/// order, and a run's phases arrive as they were published.
#[test]
fn use_wheel_hears_every_event_in_order() {
    use crate::geometry::Point;
    use crate::scroll::wheel::{WheelEvent, WheelMotion, WheelPhase, WheelSource};
    let (ping, _source) = calloop::ping::make_ping().expect("eventfd ping");
    let feed = crate::scroll::wheel_feed::WheelFeed::default();
    let handle = SurfaceHandle::new(
        SurfaceId(21),
        RoleKind::Toplevel,
        HandleParts {
            wheels: feed.clone(),
            ..parts(
                RequestQueue::new(ping.clone()),
                FrameTicks::default(),
                LiveScale::new(Scale120::ONE),
            )
        },
    );
    let env = test_env(ping, Prepaint::None);
    let mut document = SurfaceDocument::build(HostRoot::new(wheel_phases).context(handle), env);
    let wakes = Arc::new(Wakes::default());
    let waker = std::task::Waker::from(Arc::clone(&wakes));
    let mut cx = std::task::Context::from_waker(&waker);
    document.poll(&mut cx);
    assert_eq!(text(&mut document), "[]");
    wakes.take();
    let event = |phase, y| WheelEvent {
        source: WheelSource::Finger,
        phase,
        by: WheelMotion { x: 0.0, y },
        at: Point::default(),
        held: crate::modifiers::Modifiers::default(),
    };
    feed.publish(event(WheelPhase::Began, 3.0));
    feed.publish(event(WheelPhase::Ended, 0.0));
    assert!(wakes.take() > 0, "an event wakes the document");
    assert_eq!(document.poll(&mut cx), Polled::Changed);
    assert_eq!(
        text(&mut document),
        "[(Began, 3.0), (Ended, 0.0)]",
        "both events, in order, from one poll"
    );
}

fn modality_text() -> Element {
    let modality = crate::handle::use_input_modality();
    rsx! { div { "{modality().slug()}" } }
}

/// One seat modality, two surfaces: a key press the host notes re-renders both documents,
/// a repeated value wakes neither, and a pointer press turns both back.
#[test]
fn use_input_modality_is_the_seats_across_surfaces() {
    use crate::modality::{self, InputModality, KeyKind, Stroke};
    let (ping, _source) = calloop::ping::make_ping().expect("eventfd ping");
    let seat = Live::new(InputModality::default());
    let mut documents: Vec<SurfaceDocument> = [SurfaceId(21), SurfaceId(22)]
        .into_iter()
        .map(|id| {
            let handle = SurfaceHandle::new(
                id,
                RoleKind::Layer,
                HandleParts {
                    modality: seat.clone(),
                    ..parts(
                        RequestQueue::new(ping.clone()),
                        FrameTicks::default(),
                        LiveScale::new(Scale120::ONE),
                    )
                },
            );
            let env = test_env(ping.clone(), Prepaint::None);
            SurfaceDocument::build(HostRoot::new(modality_text).context(handle), env)
        })
        .collect();
    let wakes = Arc::new(Wakes::default());
    let waker = std::task::Waker::from(Arc::clone(&wakes));
    let mut cx = std::task::Context::from_waker(&waker);
    for document in &mut documents {
        document.poll(&mut cx);
        assert_eq!(text(document), "pointer");
    }
    wakes.take();
    let note = |stroke| {
        let _ = seat.publish(modality::after(seat.get(), stroke));
    };
    note(Stroke::PointerPress);
    assert_eq!(wakes.take(), 0, "a press while on pointer changes nothing");
    note(Stroke::KeyPress(KeyKind::Other));
    assert!(wakes.take() > 0, "the surfaces wake");
    for document in &mut documents {
        assert_eq!(document.poll(&mut cx), Polled::Changed);
        assert_eq!(text(document), "keyboard");
    }
    note(Stroke::KeyPress(KeyKind::Other));
    assert_eq!(wakes.take(), 0, "a second key wakes nobody");
    note(Stroke::PointerPress);
    for document in &mut documents {
        assert_eq!(document.poll(&mut cx), Polled::Changed);
        assert_eq!(text(document), "pointer");
    }
}

/// `begin_move` and `begin_resize` queue their op only under the serial rule, and say why
/// not otherwise; maximize is queued as asked.
#[test]
fn a_move_is_queued_only_with_a_held_press_on_the_surface() {
    let (ping, _source) = calloop::ping::make_ping().expect("eventfd ping");
    let requests = RequestQueue::new(ping);
    let grab = GrabRecord::default();
    let id = SurfaceId(14);
    let handle_of = |kind| {
        SurfaceHandle::new(
            id,
            kind,
            HandleParts {
                grab: grab.clone(),
                ..parts(
                    requests.clone(),
                    FrameTicks::default(),
                    LiveScale::new(Scale120::ONE),
                )
            },
        )
    };
    let window = handle_of(RoleKind::Toplevel);
    assert_eq!(window.begin_move(), Err(GrabRefused::PointerElsewhere));
    grab.update(|facts| facts.focus = Some(id));
    assert_eq!(window.begin_move(), Err(GrabRefused::NoPress));
    assert!(requests.drain().is_empty(), "a refusal sends nothing");

    grab.update(|facts| {
        *facts = PointerFacts {
            focus: Some(id),
            press: Some(Press {
                surface: id,
                serial: 9,
            }),
        }
    });
    assert_eq!(window.begin_move(), Ok(()));
    assert_eq!(window.begin_resize(ResizeEdge::BottomRight), Ok(()));
    window.set_maximized(Maximize::Set);
    let ops: Vec<SurfaceOp> = requests
        .drain()
        .into_iter()
        .filter_map(|request| match request {
            Request::Apply { op, .. } => Some(op),
            _ => None,
        })
        .collect();
    assert_eq!(
        ops,
        [
            SurfaceOp::BeginMove,
            SurfaceOp::BeginResize(ResizeEdge::BottomRight),
            SurfaceOp::SetMaximized(Maximize::Set),
        ]
    );
    assert_eq!(
        handle_of(RoleKind::Layer).begin_move(),
        Err(GrabRefused::NotAToplevel)
    );
}

//! design/11 §11.8 item 13, double scroll, with a fake palmrest daemon: the daemon's stream
//! over a real `SOCK_SEQPACKET` socket, read and routed as the host does
//! (`input::palmrest::{read_all, route}`), plus the same stroke arriving again as compositor
//! touchpad frames, filtered by the suppression window. The offsets must equal the
//! palmrest-only run within 1 px; without the window they would not.

use std::os::fd::OwnedFd;
use std::path::PathBuf;
use std::task::Context;
use std::time::Duration;

use dioxus::prelude::*;
use rustix::net::{
    AddressFamily, SendFlags, SocketAddrUnix, SocketFlags, SocketType, accept, bind, listen, send,
    socket_with,
};

use crate::dom::document::SurfaceDocument;
use crate::dom::tests::env::test_env;
use crate::input::palmrest::route::route;
use crate::input::palmrest::{Palmrest, read_all};
use crate::input::pointer::PointerPos;
use crate::root::HostRoot;
use crate::scroll::source::{AxisFrame, AxisSource, AxisValue, Shift, Stop};
use crate::scroll::suppress::{AxisKind, Suppress, Verdict};
use crate::scroll::surface::SurfaceScroll;
use crate::scroll::tuning::Tuning;
use crate::scroll::watch::ScrollBoard;
use crate::tick::HostInstant;
use blitz_kit::scroll::doc as dom;
use blitz_kit::scroll::engine::Motion;
use blitz_kit::scroll::geom::{ScrollAxis, Scroller};
use palmrest_protocol::{ActiveTouch, Axis as WireAxis, GestureId, Phase, ToHost};

const FRAME: Duration = Duration::from_micros(8333);
const AT: PointerPos = PointerPos { x: 100.0, y: 300.0 };
const GRACE: Duration = Duration::from_millis(150);

fn page() -> Element {
    rsx! {
        style { "#main{{height:auto}}" }
        div { style: "height:5000px" }
    }
}

fn document() -> SurfaceDocument {
    let env = test_env(400, 600);
    let mut doc = SurfaceDocument::build(HostRoot::new(page), env);
    doc.poll(&mut Context::from_waker(std::task::Waker::noop()));
    doc.resolve(0.0);
    doc
}

/// A fake palmrest: a listening socket in a private temp directory.
struct Daemon {
    dir: PathBuf,
    listener: OwnedFd,
}

impl Daemon {
    fn start(name: &str) -> Daemon {
        let dir = std::env::temp_dir().join(format!("shell-host-{name}-{}", std::process::id()));
        let path = dir.join("events.sock");
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        let _ = std::fs::remove_file(&path);
        let listener = socket_with(
            AddressFamily::UNIX,
            SocketType::SEQPACKET,
            SocketFlags::CLOEXEC,
            None,
        )
        .expect("socket");
        bind(&listener, &SocketAddrUnix::new(&path).expect("addr")).expect("bind");
        listen(&listener, 4).expect("listen");
        Daemon { dir, listener }
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn scroll_msg(phase: Phase, dy: f32, t: Duration) -> ToHost {
    ToHost::Scroll {
        gesture: GestureId(1),
        phase,
        dx_px: 0.0,
        dy_px: dy,
        fingers: 1,
        axis: Some(WireAxis::Y),
        t_ns: u64::try_from(t.as_nanos()).expect("fits"),
    }
}

/// Which extra input a run gets besides palmrest's stream.
#[derive(Clone, Copy, PartialEq)]
enum Echo {
    /// palmrest only.
    None,
    /// The same stroke through the compositor, filtered by the window.
    Suppressed,
    /// The same stroke through the compositor, unfiltered (the bug the window prevents).
    Unfiltered,
}

/// The host's side of one run: the socket, the document and its scrolling, the window.
struct Host {
    peer: OwnedFd,
    reader: OwnedFd,
    doc: SurfaceDocument,
    scroll: SurfaceScroll,
    suppress: Suppress,
    now: HostInstant,
    echo: Echo,
}

impl Host {
    /// The daemon sends `message` (if any), the host reads and routes everything waiting, then
    /// the compositor's copy of the stroke (`echo`: finger px and stop) arrives, if any.
    fn step(&mut self, message: Option<ToHost>, echo: Option<(f64, Stop)>) {
        if let Some(message) = message {
            let bytes = palmrest_protocol::encode(&message);
            send(&self.peer, &bytes, SendFlags::empty()).expect("send");
        }
        let (messages, _) = read_all(&self.reader);
        for message in messages {
            let routed = route(&message);
            if let Some(fingers) = routed.fingers {
                self.suppress = self.suppress.fingers(fingers, self.now);
            }
            if let Some(event) = routed.scroll {
                self.scroll
                    .palmrest(&mut self.doc.base_mut(), event, AT, self.now);
            }
        }
        let Some((px, stop)) = echo else {
            return;
        };
        let verdict = match self.echo {
            Echo::None => return,
            Echo::Suppressed => self.suppress.verdict(
                AxisKind::Suppressible,
                self.scroll.driven(),
                GRACE,
                self.now,
            ),
            Echo::Unfiltered => Verdict::Deliver,
        };
        if stop == Stop::Stopped {
            self.suppress = self.suppress.axis_stop();
        }
        if verdict == Verdict::Deliver {
            let frame = AxisFrame {
                source: AxisSource::Finger,
                horizontal: AxisValue::NONE,
                // The virtual touchpad's compositor already applied natural scrolling.
                vertical: AxisValue {
                    px: -px,
                    stop,
                    ..AxisValue::NONE
                },
                shift: Shift::Up,
            };
            self.scroll
                .axis(&mut self.doc.base_mut(), &frame, AT, self.now);
        }
    }

    fn offset(&self) -> f64 {
        dom::geom(&self.doc.base(), Scroller::Viewport, ScrollAxis::Y)
            .expect("geom")
            .offset
            .0
    }

    fn advance(&mut self, by: Duration) {
        self.now = HostInstant(self.now.0 + by);
    }
}

/// A flick of 12 frames of 20 px (finger toward the user: content moves down, natural),
/// then the lift; the page's offset after every frame until it settles.
fn run(echo: Echo, name: &str) -> Vec<f64> {
    let daemon = Daemon::start(name);
    let mut client = Palmrest::trusting(daemon.dir.join("events.sock"), None);
    let reader = client.connect().expect("the fake daemon is there");
    let peer = accept(&daemon.listener).expect("accept");
    let mut doc = document();
    dom::write(
        &mut doc.base_mut(),
        Scroller::Viewport,
        ScrollAxis::Y,
        3000.0,
    );
    let mut host = Host {
        peer,
        reader,
        doc,
        scroll: SurfaceScroll::new(ScrollBoard::default(), Tuning::default()),
        suppress: Suppress::default(),
        now: HostInstant(Duration::from_secs(5)),
        echo,
    };
    let mut offsets = Vec::new();
    host.step(
        Some(ToHost::Touches {
            active: ActiveTouch::Down,
        }),
        None,
    );
    host.step(Some(scroll_msg(Phase::MayBegin, 0.0, host.now.0)), None);
    host.step(Some(scroll_msg(Phase::Began, 0.0, host.now.0)), None);
    for _ in 0..12 {
        host.advance(FRAME);
        let changed = scroll_msg(Phase::Changed, 20.0, host.now.0);
        host.step(Some(changed), Some((20.0, Stop::Moving)));
        offsets.push(host.offset());
    }
    host.step(Some(scroll_msg(Phase::Ended, 0.0, host.now.0)), None);
    host.step(
        Some(ToHost::Touches {
            active: ActiveTouch::Up,
        }),
        None,
    );
    // The virtual touchpad's lift reaches the compositor a moment later.
    host.advance(Duration::from_millis(5));
    host.step(None, Some((0.0, Stop::Stopped)));
    for _ in 0..600 {
        if host.scroll.motion(host.now) == Motion::Still {
            break;
        }
        host.advance(FRAME);
        host.scroll.frame(&mut host.doc.base_mut(), host.now);
        host.doc.resolve(host.now.0.as_secs_f64());
        offsets.push(host.offset());
    }
    offsets
}

#[test]
fn item_13_the_echo_through_the_compositor_is_dropped() {
    let alone = run(Echo::None, "alone");
    let both = run(Echo::Suppressed, "both");
    assert!(
        alone.first().is_some_and(|x| *x < 3000.0),
        "natural: content follows the finger"
    );
    assert_eq!(alone.len(), both.len(), "same frames");
    for (i, (a, b)) in alone.iter().zip(&both).enumerate() {
        assert!((a - b).abs() <= 1.0, "frame {i}: {a} vs {b}");
    }
    let unfiltered = run(Echo::Unfiltered, "unfiltered");
    let last = |v: &[f64]| *v.last().expect("frames");
    assert!(
        (last(&unfiltered) - last(&alone)).abs() > 100.0,
        "without the window the stroke counts twice: {} vs {}",
        last(&unfiltered),
        last(&alone)
    );
}

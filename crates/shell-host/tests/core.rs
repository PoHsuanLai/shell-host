//! The host core's pure parts, without a compositor: the flush order (as a recorded sequence
//! against a fake), op queueing (ops land after the poll that queued them and before the
//! paint), visibility transitions, the stats maths, and the debug command language.

use std::time::{Duration, Instant};

use calloop::ping::make_ping;
use shell_host::ctx::flush::{self, Flush, ORDER, Phase};
use shell_host::debug::{
    CommandError, DebugAction, DebugCommand, FrameCounters, Percentiles, StatsWindow, SurfaceName,
    parse_command, percentiles,
};
use shell_host::handle::RequestQueue;
use shell_host::msg::{Request, SurfaceOp, Visibility};
use shell_host::surface::{VisibilityChange, visibility_change};
use shell_host::{BlurSpec, ExclusiveZone, HostError, Point, RegionSpec, SurfaceId};

/// What the fake saw, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Seen {
    Poll,
    Applied(SurfaceOp),
    /// A paint, with the ops applied so far (what the frame's commit carries).
    Paint(usize),
    Timers,
    Flushed,
}

/// A host whose documents queue `ops_from_poll` while they are polled, the way a component
/// calling `SurfaceHandle::set_input_region` inside a render does.
struct Fake {
    queue: RequestQueue,
    ops_from_poll: Vec<SurfaceOp>,
    applied: usize,
    seen: Vec<Seen>,
    protocol_error: Option<String>,
}

impl Fake {
    fn new(ops_from_poll: Vec<SurfaceOp>) -> Fake {
        // A real ping: the queue wakes the loop on every push, and a ping needs no compositor.
        let (ping, _source) = make_ping().expect("an eventfd for the ping");
        Fake {
            queue: RequestQueue::new(ping),
            ops_from_poll,
            applied: 0,
            seen: Vec::new(),
            protocol_error: None,
        }
    }
}

impl Flush for Fake {
    fn poll_woken(&mut self) {
        self.seen.push(Seen::Poll);
        for op in self.ops_from_poll.drain(..) {
            self.queue.push(Request::Apply {
                id: SurfaceId(1),
                op,
            });
        }
    }

    fn apply_requests(&mut self) {
        for request in self.queue.drain() {
            if let Request::Apply { op, .. } = request {
                self.applied += 1;
                self.seen.push(Seen::Applied(op));
            }
        }
    }

    fn step_frames(&mut self) {
        self.seen.push(Seen::Paint(self.applied));
    }

    fn arm_timers(&mut self) {
        self.seen.push(Seen::Timers);
    }

    fn flush_connection(&mut self) -> shell_host::Result<()> {
        self.seen.push(Seen::Flushed);
        match self.protocol_error.take() {
            Some(error) => Err(HostError::Protocol(error)),
            None => Ok(()),
        }
    }
}

#[test]
fn the_flush_runs_its_phases_in_the_documented_order() {
    assert_eq!(
        ORDER,
        [
            Phase::PollWoken,
            Phase::ApplyRequests,
            Phase::StepFrames,
            Phase::ArmTimers,
            Phase::FlushConnection,
        ]
    );
    let mut fake = Fake::new(Vec::new());
    flush::run(&mut fake).expect("no protocol error");
    assert_eq!(
        fake.seen,
        [Seen::Poll, Seen::Paint(0), Seen::Timers, Seen::Flushed]
    );
}

#[test]
fn ops_queued_during_a_poll_land_before_the_paint_of_the_same_flush() {
    let region = SurfaceOp::SetInputRegion(RegionSpec::Empty);
    let blur = SurfaceOp::SetBlur(BlurSpec::Region(RegionSpec::Whole));
    let zone = SurfaceOp::SetExclusiveZone(ExclusiveZone::Reserve(40));
    let cases: Vec<(&str, Vec<SurfaceOp>, Vec<Seen>)> = vec![
        ("no ops", vec![], vec![Seen::Poll, Seen::Paint(0)]),
        (
            "one op",
            vec![region.clone()],
            vec![Seen::Poll, Seen::Applied(region.clone()), Seen::Paint(1)],
        ),
        (
            "ops keep their order",
            vec![blur.clone(), zone.clone(), region.clone()],
            vec![
                Seen::Poll,
                Seen::Applied(blur),
                Seen::Applied(zone),
                Seen::Applied(region),
                Seen::Paint(3),
            ],
        ),
    ];
    for (name, ops, mut want) in cases {
        let mut fake = Fake::new(ops);
        flush::run(&mut fake).expect("no protocol error");
        want.extend([Seen::Timers, Seen::Flushed]);
        assert_eq!(fake.seen, want, "{name}");
        assert!(
            fake.queue.drain().is_empty(),
            "{name}: the queue is drained"
        );
    }
}

#[test]
fn a_protocol_error_ends_the_flush_with_a_fatal_error() {
    let mut fake = Fake::new(Vec::new());
    fake.protocol_error = Some("zwp_linux_dmabuf_v1@12: error 7".into());
    let error = flush::run(&mut fake).expect_err("the protocol error surfaces");
    assert!(matches!(error, HostError::Protocol(_)), "{error}");
    assert_eq!(error.recovery(), shell_host::Recovery::Fatal);
    // A second flush on a healthy connection runs normally again.
    fake.seen.clear();
    flush::run(&mut fake).expect("healthy");
    assert_eq!(fake.seen.last(), Some(&Seen::Flushed));
}

#[test]
fn the_request_queue_is_first_in_first_out() {
    let (ping, _source) = make_ping().expect("an eventfd for the ping");
    let queue = RequestQueue::new(ping);
    let ids = [3, 1, 2].map(SurfaceId);
    for id in ids {
        queue.push(Request::Close(id));
    }
    let drained: Vec<SurfaceId> = queue
        .drain()
        .into_iter()
        .filter_map(|request| match request {
            Request::Close(id) => Some(id),
            _ => None,
        })
        .collect();
    assert_eq!(drained, ids);
    assert!(queue.drain().is_empty());
}

#[test]
fn visibility_changes_unmap_and_map_only_on_a_real_change() {
    use Visibility::{Hidden, Shown};
    let cases = [
        (Shown, Shown, VisibilityChange::Unchanged),
        (Shown, Hidden, VisibilityChange::Unmap),
        (Hidden, Hidden, VisibilityChange::Unchanged),
        (Hidden, Shown, VisibilityChange::Map),
    ];
    for (from, to, want) in cases {
        assert_eq!(visibility_change(from, to), want, "{from:?} -> {to:?}");
    }
}

#[test]
fn percentiles_are_nearest_rank_over_the_window() {
    let p = |p50_us, p95_us, max_us| Percentiles {
        p50_us,
        p95_us,
        max_us,
    };
    let hundred: Vec<u32> = (1..=100).collect();
    let cases: Vec<(&str, Vec<u32>, Percentiles)> = vec![
        ("empty", vec![], p(0, 0, 0)),
        ("one", vec![7], p(7, 7, 7)),
        ("two", vec![9, 1], p(1, 9, 9)),
        ("unsorted", vec![5, 3, 9, 1, 7], p(5, 9, 9)),
        ("1..=100", hundred, p(50, 95, 100)),
        (
            "an outlier moves only max",
            vec![1; 99].into_iter().chain([900]).collect(),
            p(1, 1, 900),
        ),
    ];
    for (name, samples, want) in cases {
        assert_eq!(percentiles(&samples), want, "{name}");
    }
}

#[test]
fn a_stats_window_keeps_work_and_interval_apart_and_drains_only_samples() {
    let t0 = Instant::now();
    let mut window = StatsWindow::default();
    // Three frames, 1 ms of work each, presented a refresh apart.
    for n in 0..3u64 {
        window.presented(
            Duration::from_millis(1),
            t0 + Duration::from_micros(16_667 * n),
        );
        window.callback();
    }
    window.watchdog_fired();
    let (work, interval, samples) = window.peek();
    assert_eq!(samples, 3);
    assert_eq!(work.p95_us, 1000, "work is trigger to present");
    assert_eq!(interval.p50_us, 16_667, "interval is present to present");
    assert_eq!(
        window.drain(),
        (work, interval, samples),
        "drain reports what peek did"
    );
    assert_eq!(
        window.peek(),
        (Percentiles::default(), Percentiles::default(), 0),
        "then starts over"
    );
    assert_eq!(
        window.counters(),
        FrameCounters {
            frames: 3,
            callbacks: 3,
            commits: 3,
            skipped: 0,
            watchdog_fired: 1,
            ticks: 0,
        },
        "counters are cumulative"
    );
}

#[test]
fn debug_commands_address_surfaces_by_name() {
    let on = |surface: &str, action| DebugCommand::On {
        surface: SurfaceName(surface.to_owned()),
        action,
    };
    let cases: Vec<(&str, Result<DebugCommand, CommandError>)> = vec![
        ("stats", Ok(DebugCommand::StatsAll)),
        ("quit\n", Ok(DebugCommand::Quit)),
        (
            "bar hover 40 16",
            Ok(on("bar", DebugAction::Hover(Point { x: 40, y: 16 }))),
        ),
        (
            "dock click 60 64",
            Ok(on("dock", DebugAction::Click(Point { x: 60, y: 64 }))),
        ),
        ("dock leave", Ok(on("dock", DebugAction::Leave))),
        ("bar/popup stats", Ok(on("bar/popup", DebugAction::Stats))),
        (
            "bar capture /tmp/f.png",
            Ok(on("bar", DebugAction::Capture("/tmp/f.png".into()))),
        ),
        ("   ", Err(CommandError::Empty)),
        ("leave", Err(CommandError::Unknown("leave".into()))),
        (
            "dock hover 40",
            Err(CommandError::Missing {
                verb: "hover",
                needs: "X and Y",
            }),
        ),
        (
            "dock hover x 1",
            Err(CommandError::NotACoordinate("x".into())),
        ),
        ("dock wiggle", Err(CommandError::Unknown("wiggle".into()))),
    ];
    for (line, want) in cases {
        assert_eq!(parse_command(line), want, "{line:?}");
    }
}

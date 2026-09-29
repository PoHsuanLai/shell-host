//! The flush order as a recorded sequence against a fake, and op queueing: ops land after the
//! poll that queued them and before the paint.

use calloop::ping::make_ping;

use super::{Flush, ORDER, Phase, run};
use crate::handle::RequestQueue;
use crate::msg::{Request, SurfaceOp};
use crate::{BlurSpec, ExclusiveZone, HostError, RegionSpec, SurfaceId};

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

    fn flush_connection(&mut self) -> crate::Result<()> {
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
    run(&mut fake).expect("no protocol error");
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
        run(&mut fake).expect("no protocol error");
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
    let error = run(&mut fake).expect_err("the protocol error surfaces");
    assert!(matches!(error, HostError::Protocol(_)), "{error}");
    assert_eq!(error.recovery(), crate::Recovery::Fatal);
    // A second flush on a healthy connection runs normally again.
    fake.seen.clear();
    run(&mut fake).expect("healthy");
    assert_eq!(fake.seen.last(), Some(&Seen::Flushed));
}

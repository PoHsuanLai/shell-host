//! design/11 §11.8 acceptance, the pure part: synthetic streams at 120 Hz through
//! `scroll::engine::step`, with `now` advanced by hand and the scroller's geometry updated from
//! the engine's own `SetOffset`s, as the host does.

use std::time::Duration;

use crate::scroll::config::{ScrollConfig, ScrollRubberBand, ScrollSettings};
use crate::scroll::engine::{
    self, Engine, Flight, Kinetic, Motion, NotifyPhase, Physics, ScrollAnimate, ScrollIn, ScrollOut,
};
use crate::scroll::geom::{Dir, Elastic, Geom, Latch, Px, ScrollAxis, Scroller};
use crate::scroll::keys::{self, LINE_PX};
use crate::scroll::latch::{Candidate, Latched, latch};
use crate::scroll::rubber::AtEdge;
use crate::scroll::velocity::SampleTime;
use crate::tick::HostInstant;

const FRAME: f64 = 1.0 / 120.0;

const PAGE: Latch = Latch {
    scroller: Scroller::Viewport,
    axis: ScrollAxis::Y,
};

/// One scroller, the engine, and a clock.
struct Sim {
    engine: Engine,
    geom: Geom,
    /// Seconds since the start.
    t: f64,
    physics: Physics,
    notes: Vec<(f64, NotifyPhase)>,
}

impl Sim {
    fn new(offset: f64, max: f64, elastic: Elastic) -> Sim {
        Sim::with(offset, max, elastic, ScrollSettings::default())
    }

    fn with(offset: f64, max: f64, elastic: Elastic, scroll: ScrollSettings) -> Sim {
        Sim {
            engine: Engine::Idle,
            geom: Geom {
                offset: Px(offset),
                max: Px(max),
                viewport: Px(1000.0),
                elastic,
            },
            t: 0.0,
            physics: Physics::from_config(&ScrollConfig {
                scroll,
                ..ScrollConfig::default()
            }),
            notes: Vec::new(),
        }
    }

    fn now(&self) -> HostInstant {
        HostInstant(Duration::from_secs_f64(self.t))
    }

    fn at(&self) -> SampleTime {
        SampleTime(self.now().0)
    }

    fn offset(&self) -> f64 {
        self.geom.offset.0
    }

    fn feed(&mut self, input: ScrollIn) {
        let engine = std::mem::take(&mut self.engine);
        let (next, outs) = engine::step(engine, input, self.now(), Some(self.geom), &self.physics);
        self.engine = next;
        for out in outs {
            match out {
                ScrollOut::SetOffset { raw, .. } => self.geom.offset = raw,
                ScrollOut::Notify { phase, .. } => self.notes.push((self.t, phase)),
            }
        }
    }

    fn began(&mut self, dir: Dir, began: AtEdge) {
        self.feed(ScrollIn::Began {
            latched: Latched { latch: PAGE, began },
            dir,
            kinetic: Kinetic::Momentum,
        });
    }

    /// A steady stroke of `speed` px/s (offset space) for `ms`, one Changed per frame.
    fn stroke(&mut self, speed: f64, ms: u32) {
        let frames = (f64::from(ms) / 1000.0 / FRAME).round() as u32;
        for _ in 0..frames {
            self.t += FRAME;
            self.feed(ScrollIn::Changed {
                delta: Px(speed * FRAME),
                at: self.at(),
            });
        }
    }

    fn pull(&mut self, px: f64) {
        self.t += FRAME;
        self.feed(ScrollIn::Changed {
            delta: Px(px),
            at: self.at(),
        });
    }

    fn ended(&mut self) {
        self.feed(ScrollIn::Ended { at: self.at() });
    }

    /// Frames until the engine stops or `max_s` passes; the offset after each.
    fn run(&mut self, max_s: f64) -> Vec<(f64, f64)> {
        let start = self.t;
        let mut trace = Vec::new();
        while engine::motion(&self.engine) == Motion::Animating && self.t - start < max_s {
            self.t += FRAME;
            self.feed(ScrollIn::Frame);
            trace.push((self.t - start, self.offset()));
        }
        trace
    }

    fn when(&self, phase: NotifyPhase) -> Option<f64> {
        self.notes
            .iter()
            .rev()
            .find(|(_, p)| *p == phase)
            .map(|(t, _)| *t)
    }

    fn flight(&self) -> Flight {
        match self.engine {
            Engine::Momentum(f) => f,
            ref other => panic!("not gliding: {other:?}"),
        }
    }
}

#[test]
fn item_1_a_2000_px_s_flick_glides_501_px_and_ends_at_976_ms() {
    let mut sim = Sim::new(1000.0, 100_000.0, Elastic::Rigid);
    sim.began(Dir::Pos, AtEdge::Inside);
    sim.stroke(2000.0, 100);
    sim.ended();
    let (released, at) = (sim.offset(), sim.t);
    let v0 = sim.flight().v0;
    assert!((v0 - 2000.0).abs() < 1.0, "release velocity {v0}");
    sim.run(3.0);
    let travel = sim.offset() - released;
    assert!((476.0..=527.0).contains(&travel), "travel {travel}");
    let ended = sim.when(NotifyPhase::MomentumEnded).expect("MomentumEnded") - at;
    assert!((ended - 0.976).abs() <= FRAME, "MomentumEnded at {ended}");
}

#[test]
fn item_2_a_pause_before_the_lift_glides_nowhere() {
    let mut sim = Sim::new(1000.0, 100_000.0, Elastic::Rigid);
    sim.began(Dir::Pos, AtEdge::Inside);
    sim.stroke(2000.0, 100);
    sim.t += 0.060;
    sim.ended();
    assert_eq!(sim.engine, Engine::Idle);
    assert_eq!(sim.when(NotifyPhase::MomentumBegan), None);
}

/// A 2000 px/s glide, then a second 2000 px/s flick begun when it is at 1000 px/s.
fn boosted(dir: Dir) -> f64 {
    let mut sim = Sim::new(1000.0, 100_000.0, Elastic::Rigid);
    sim.began(Dir::Pos, AtEdge::Inside);
    sim.stroke(2000.0, 100);
    sim.ended();
    let t_1000 = (2000f64.powf(0.3) - 1000f64.powf(0.3)) / 9.0;
    sim.t += t_1000;
    sim.feed(ScrollIn::Frame);
    sim.feed(ScrollIn::MayBegin);
    sim.began(dir, AtEdge::Inside);
    sim.stroke(dir.sign() * 2000.0, 100);
    sim.ended();
    sim.flight().v0
}

#[test]
fn item_3_a_second_flick_boosts_only_in_the_same_direction() {
    let same = boosted(Dir::Pos);
    assert!((same - 3000.0).abs() <= 60.0, "same direction: {same}");
    let opposite = boosted(Dir::Neg);
    assert!((opposite + 2000.0).abs() <= 40.0, "opposite: {opposite}");
}

#[test]
fn item_3_a_touchpad_began_during_the_glide_boosts_too() {
    let mut sim = Sim::new(1000.0, 100_000.0, Elastic::Rigid);
    sim.began(Dir::Pos, AtEdge::Inside);
    sim.stroke(2000.0, 100);
    sim.ended();
    sim.t += (2000f64.powf(0.3) - 1000f64.powf(0.3)) / 9.0;
    sim.began(Dir::Pos, AtEdge::Inside);
    sim.stroke(2000.0, 100);
    sim.ended();
    let v0 = sim.flight().v0;
    assert!((v0 - 3000.0).abs() <= 60.0, "{v0}");
}

#[test]
fn item_4_touch_stops_the_glide_where_it_is() {
    let mut sim = Sim::new(1000.0, 100_000.0, Elastic::Rigid);
    sim.began(Dir::Pos, AtEdge::Inside);
    sim.stroke(2000.0, 100);
    sim.ended();
    sim.run(0.2);
    let before = sim.offset();
    sim.feed(ScrollIn::MayBegin);
    for _ in 0..2 {
        sim.t += FRAME;
        sim.feed(ScrollIn::Frame);
        assert_eq!(sim.offset(), before);
    }
    assert_eq!(engine::motion(&sim.engine), Motion::Still);
}

/// At the top, a gesture begun inside at 50 px pulls `over` px past the edge.
fn stretch(over: f64, model: ScrollRubberBand) -> f64 {
    let scroll = ScrollSettings {
        rubber_band: model,
        ..ScrollSettings::default()
    };
    let mut sim = Sim::with(50.0, 5000.0, Elastic::Elastic, scroll);
    sim.began(Dir::Neg, AtEdge::Inside);
    sim.pull(-50.0 - over);
    -sim.offset()
}

#[test]
fn item_5_the_stretch_under_the_fingers() {
    let cases = [
        ("bounded 100", 100.0, ScrollRubberBand::Bounded, 52.1),
        ("bounded 1000", 1000.0, ScrollRubberBand::Bounded, 354.8),
        ("linear 100", 100.0, ScrollRubberBand::Linear, 5.0),
        ("linear 1000", 1000.0, ScrollRubberBand::Linear, 50.0),
    ];
    for (name, over, model, want) in cases {
        let s = stretch(over, model);
        assert!((s - want).abs() <= 1.0, "{name}: {s}");
    }
    // Began at the edge: the first 10 px do not stretch.
    let mut sim = Sim::new(0.0, 5000.0, Elastic::Elastic);
    let chain = [Candidate {
        scroller: Scroller::Viewport,
        geom: sim.geom,
    }];
    let latched = latch(&chain, ScrollAxis::Y, Dir::Neg).expect("the elastic page");
    assert_eq!(latched.began, AtEdge::AtEdge);
    sim.feed(ScrollIn::Began {
        latched,
        dir: Dir::Neg,
        kinetic: Kinetic::Momentum,
    });
    sim.pull(-10.0);
    assert_eq!(sim.offset(), 0.0);
    sim.pull(-100.0);
    assert!((sim.offset() + 52.1).abs() <= 1.0, "{}", sim.offset());
}

#[test]
fn item_6_snap_back_from_100_px() {
    let mut sim = Sim::new(50.0, 5000.0, Elastic::Elastic);
    sim.began(Dir::Neg, AtEdge::Inside);
    // Pull until the stretch is exactly 100 px (O = 100 d / (0.55 (d - 100)) = 202.02).
    let over = 100.0 * 1000.0 / (0.55 * 900.0);
    sim.pull(-50.0 - over);
    assert!((sim.offset() + 100.0).abs() < 1e-6, "{}", sim.offset());
    sim.t += 0.060; // held still: v0 = 0
    sim.ended();
    let trace = sim.run(1.0);
    let settled = trace
        .iter()
        .find(|(_, x)| *x == 0.0)
        .expect("back at the edge")
        .0;
    assert!(settled <= 0.400, "at the edge at {settled}");
    let small = trace
        .iter()
        .find(|(_, x)| x.abs() < 1.0)
        .expect("under 1 px")
        .0;
    assert!((small - 0.368).abs() <= FRAME, "under 1 px at {small}");
    assert!(trace.windows(2).all(|w| w[1].1 >= w[0].1), "monotonic");
    assert_eq!(sim.offset(), 0.0, "exactly the edge");
}

#[test]
fn item_7_a_glide_into_the_edge_bounces_18_px() {
    let mut sim = Sim::new(5000.0, 5000.0, Elastic::Elastic);
    sim.engine = Engine::Momentum(Flight {
        latch: PAGE,
        v0: 2000.0,
        x0: 5000.0,
        t0: sim.now(),
    });
    let trace = sim.run(2.0);
    let (peak_t, peak) = trace
        .iter()
        .map(|(t, x)| (*t, x - 5000.0))
        .fold((0.0, 0.0), |best, p| if p.1 > best.1 { p } else { best });
    assert!((peak - 18.3).abs() <= 1.0, "peak {peak}");
    assert!((peak_t - 0.080).abs() <= FRAME, "peak at {peak_t}");
    let settled = sim.when(NotifyPhase::MomentumEnded).expect("settles");
    assert!(settled <= 0.460, "settled at {settled}");
    assert_eq!(sim.offset(), 5000.0);
}

#[test]
fn item_7_a_real_flick_meets_the_edge_at_its_glide_speed() {
    // Released at 3000 px/s with just enough room to slow to 2000 px/s by the edge.
    let glide = Physics::from_config(&ScrollConfig::default()).glide;
    let t_hit = (3000f64.powf(0.3) - 2000f64.powf(0.3)) / 9.0;
    let room = glide.travel(3000.0, t_hit);
    let mut sim = Sim::new(1000.0, 1000.0 + 300.0 + room, Elastic::Elastic);
    sim.began(Dir::Pos, AtEdge::Inside);
    sim.stroke(3000.0, 100);
    sim.ended();
    let v0 = sim.flight().v0;
    assert!((v0 - 3000.0).abs() < 1.0, "{v0}");
    let max = sim.geom.max.0;
    let trace = sim.run(3.0);
    let peak = trace.iter().map(|(_, x)| x - max).fold(0.0, f64::max);
    assert!((peak - 18.3).abs() <= 1.0, "peak {peak}");
    assert_eq!(sim.offset(), max, "settles at the edge");
}

#[test]
fn item_8_a_rigid_scroller_never_stretches() {
    let mut sim = Sim::new(0.0, 800.0, Elastic::Rigid);
    sim.began(Dir::Neg, AtEdge::AtEdge);
    for _ in 0..50 {
        sim.pull(-10.0);
        assert_eq!(sim.offset(), 0.0);
    }
    sim.ended();
    sim.run(1.0);
    assert_eq!(sim.offset(), 0.0);
}

#[test]
fn item_11_keyboard_line_page_end() {
    // Down: +40 in 40 ms.
    let mut sim = Sim::new(100.0, 5000.0, Elastic::Rigid);
    sim.feed(ScrollIn::Step {
        latch: PAGE,
        by: Px(LINE_PX),
    });
    let trace = sim.run(1.0);
    let done = trace
        .iter()
        .find(|(_, x)| *x == 140.0)
        .expect("reaches +40")
        .0;
    assert!((done - 0.040).abs() <= FRAME, "line done at {done}");
    // PgDn on a 900 px viewport: +860 in 200 ms.
    let mut sim = Sim::new(0.0, 5000.0, Elastic::Rigid);
    sim.geom.viewport = Px(900.0);
    sim.feed(ScrollIn::Step {
        latch: PAGE,
        by: Px(keys::page(900.0)),
    });
    let trace = sim.run(1.0);
    let done = trace
        .iter()
        .find(|(_, x)| *x == 860.0)
        .expect("reaches +860")
        .0;
    assert!((done - 0.200).abs() <= FRAME, "page done at {done}");
    // End: at max in <= 200 ms, from far away.
    let mut sim = Sim::new(0.0, 50_000.0, Elastic::Rigid);
    sim.feed(ScrollIn::Jump {
        latch: PAGE,
        to: Px(50_000.0),
        animate: ScrollAnimate::Smooth,
    });
    let trace = sim.run(1.0);
    let done = trace
        .iter()
        .find(|(_, x)| *x == 50_000.0)
        .expect("reaches max")
        .0;
    assert!(done <= 0.200 + FRAME, "end at {done}");
}

#[test]
fn item_11_a_held_arrow_ramps_then_springs() {
    let mut sim = Sim::new(0.0, 100_000.0, Elastic::Rigid);
    sim.feed(ScrollIn::Hold {
        latch: PAGE,
        step: Px(LINE_PX),
    });
    let trace = sim.run(1.0);
    let speed = |i: usize| (trace[i].1 - trace[i - 1].1) / FRAME;
    let at_ramp = trace
        .iter()
        .position(|(t, _)| *t >= 0.2 - 1e-9)
        .expect("0.2 s");
    let v = speed(at_ramp + 1);
    assert!((v - 1000.0).abs() < 1.0, "1000 px/s after 0.2 s: {v}");
    assert!(speed(at_ramp - 5) < 1000.0 - 100.0, "still ramping before");
    // Release after 1 s: target = x + 1000 x 0.1.
    let x = sim.offset();
    sim.feed(ScrollIn::Release);
    let target = x + 100.0;
    let trace = sim.run(2.0);
    for (t, got) in &trace {
        let want = target - 100.0 * (-10.0 * t).exp() * (8.660_254 * t).cos();
        if (got - target).abs() > 0.5 {
            assert!((got - want).abs() <= 1.0, "at {t}: {got} vs {want}");
        }
    }
    let (settled, _) = *trace.last().expect("frames");
    assert!(settled <= 0.600, "settled (engine idle) at {settled}");
    assert_eq!(sim.engine, Engine::Idle);
    assert_eq!(sim.offset(), target);
    assert!(
        trace.iter().any(|(_, x)| *x > target),
        "it overshoots, as a spring does"
    );
}

#[test]
fn item_12_wheel_detents() {
    let detent = |sim: &mut Sim| {
        sim.feed(ScrollIn::Step {
            latch: PAGE,
            by: Px(60.0),
        })
    };
    // One detent: +60 in 60 ms.
    let mut sim = Sim::new(0.0, 5000.0, Elastic::Elastic);
    detent(&mut sim);
    let trace = sim.run(1.0);
    let done = trace.iter().find(|(_, x)| *x == 60.0).expect("+60").0;
    assert!((done - 0.060).abs() <= FRAME, "detent at {done}");
    // Five detents within 50 ms (frames rendering in between): +300, reached <= 250 ms after
    // the first.
    let mut sim = Sim::new(0.0, 5000.0, Elastic::Elastic);
    for _ in 0..5 {
        detent(&mut sim);
        sim.t += 0.010;
        sim.feed(ScrollIn::Frame);
    }
    let trace = sim.run(1.0);
    let arrived = 0.050 + trace.iter().find(|(_, x)| *x == 300.0).expect("+300").0;
    assert!(arrived <= 0.250 + FRAME, "five detents done at {arrived}");
    // No stretch at an edge.
    let mut edge = Sim::new(4990.0, 5000.0, Elastic::Elastic);
    detent(&mut edge);
    let trace = edge.run(1.0);
    assert!(trace.iter().all(|(_, x)| *x <= 5000.0), "{trace:?}");
    assert_eq!(edge.offset(), 5000.0);
}

#[test]
fn item_14_the_engine_is_still_once_settled() {
    let mut sim = Sim::new(1000.0, 100_000.0, Elastic::Rigid);
    sim.began(Dir::Pos, AtEdge::Inside);
    sim.stroke(2000.0, 100);
    sim.ended();
    sim.run(3.0);
    assert_eq!(engine::motion(&sim.engine), Motion::Still);
    let settled = sim.offset();
    for _ in 0..600 {
        sim.t += FRAME;
        sim.feed(ScrollIn::Frame);
    }
    assert_eq!(sim.offset(), settled, "5 s of frames move nothing");
}

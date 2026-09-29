//! Choosing the scroller (design/11 §11.3.3, §11.3.9, §11.3.11): the latch at a gesture's
//! start, the touchpad's axis lock, and the wheel's per-burst target.

use std::time::Duration;

use crate::scroll::geom::{Dir, Elastic, Geom, Latch, ScrollAxis, Scroller};
use crate::scroll::rubber::AtEdge;
use crate::tick::HostInstant;

/// A touchpad locks its axis once it has moved this far, px (design/11 §11.3.9).
const LOCK_PX: f64 = 10.0;

/// One scroller on the chain from the element under the pointer outward, with its geometry on
/// the axis being latched. Innermost first.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Candidate {
    pub scroller: Scroller,
    pub geom: Geom,
}

/// What a gesture latched, and whether it began pushing against its scroller's edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Latched {
    pub latch: Latch,
    pub began: AtEdge,
}

/// Latch a gesture on `axis` moving toward `dir` (design/11 §11.3.3): the innermost scroller
/// that can move that way; else the innermost elastic one, which rubber-bands; else nothing.
/// `chain` holds only scrollers that scroll on `axis`.
pub fn latch(chain: &[Candidate], axis: ScrollAxis, dir: Dir) -> Option<Latched> {
    let movable = chain
        .iter()
        .find(|c| c.geom.can_move(dir))
        .map(|c| Latched {
            latch: Latch {
                scroller: c.scroller,
                axis,
            },
            began: AtEdge::Inside,
        });
    movable.or_else(|| {
        chain
            .iter()
            .find(|c| c.geom.elastic == Elastic::Elastic)
            .map(|c| Latched {
                latch: Latch {
                    scroller: c.scroller,
                    axis,
                },
                began: AtEdge::AtEdge,
            })
    })
}

/// A wheel's target: the innermost scroller that can move (no rubber band for wheels).
pub fn latch_rigid(chain: &[Candidate], axis: ScrollAxis, dir: Dir) -> Option<Latch> {
    chain.iter().find(|c| c.geom.can_move(dir)).map(|c| Latch {
        scroller: c.scroller,
        axis,
    })
}

/// A touchpad gesture's axis: locked to the larger axis once either has moved 10 px.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AxisLock {
    /// Still accumulating; nothing scrolls yet.
    Free {
        dx: f64,
        dy: f64,
    },
    Locked(ScrollAxis),
}

impl AxisLock {
    /// A fresh gesture.
    pub const START: AxisLock = AxisLock::Free { dx: 0.0, dy: 0.0 };

    /// Feed a delta. Returns the new lock and, when this delta locked it, the pending motion
    /// on the locked axis (applied, so a slow start does not lose its first 10 px).
    pub fn feed(self, dx: f64, dy: f64) -> (AxisLock, Option<f64>) {
        match self {
            AxisLock::Locked(_) => (self, None),
            AxisLock::Free { dx: ax, dy: ay } => {
                let (ax, ay) = (ax + dx, ay + dy);
                if ax.abs().max(ay.abs()) < LOCK_PX {
                    return (AxisLock::Free { dx: ax, dy: ay }, None);
                }
                if ax.abs() > ay.abs() {
                    (AxisLock::Locked(ScrollAxis::X), Some(ax))
                } else {
                    (AxisLock::Locked(ScrollAxis::Y), Some(ay))
                }
            }
        }
    }
}

/// The wheel's last target, kept for detents less than the burst window apart.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WheelBurst {
    last: Option<(Latch, HostInstant)>,
}

impl WheelBurst {
    /// The previous target, if a detent on `axis` at `now` continues its burst.
    pub fn current(&self, axis: ScrollAxis, now: HostInstant, window: Duration) -> Option<Latch> {
        self.last
            .filter(|(latch, at)| latch.axis == axis && now.0.saturating_sub(at.0) < window)
            .map(|(latch, _)| latch)
    }

    /// Remember `latch` as the burst's target at `now`.
    pub fn record(self, latch: Latch, now: HostInstant) -> WheelBurst {
        WheelBurst {
            last: Some((latch, now)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scroll::geom::Px;

    fn candidate(node: u64, offset: f64, max: f64, elastic: Elastic) -> Candidate {
        Candidate {
            scroller: Scroller::Node(node),
            geom: Geom {
                offset: Px(offset),
                max: Px(max),
                viewport: Px(300.0),
                elastic,
            },
        }
    }

    fn latched(node: u64, began: AtEdge) -> Option<Latched> {
        Some(Latched {
            latch: Latch {
                scroller: Scroller::Node(node),
                axis: ScrollAxis::Y,
            },
            began,
        })
    }

    #[test]
    fn the_latching_rules_of_11_3_3() {
        use Elastic::{Elastic as E, Rigid as R};
        let cases = [
            (
                "inner can move: inner",
                vec![candidate(1, 100.0, 500.0, E), candidate(2, 0.0, 900.0, E)],
                Dir::Pos,
                latched(1, AtEdge::Inside),
            ),
            (
                "inner at its bottom: the page",
                vec![candidate(1, 500.0, 500.0, E), candidate(2, 0.0, 900.0, E)],
                Dir::Pos,
                latched(2, AtEdge::Inside),
            ),
            (
                "none can move: the innermost elastic stretches",
                vec![candidate(1, 500.0, 500.0, R), candidate(2, 900.0, 900.0, E)],
                Dir::Pos,
                latched(2, AtEdge::AtEdge),
            ),
            (
                "none can move, none elastic: nothing",
                vec![candidate(1, 0.0, 500.0, R), candidate(2, 0.0, 0.0, R)],
                Dir::Neg,
                None,
            ),
            ("empty chain", vec![], Dir::Pos, None),
        ];
        for (name, chain, dir, want) in cases {
            assert_eq!(latch(&chain, ScrollAxis::Y, dir), want, "{name}");
        }
    }

    #[test]
    fn a_wheel_never_latches_an_edge() {
        let chain = [candidate(1, 500.0, 500.0, Elastic::Elastic)];
        assert_eq!(latch_rigid(&chain, ScrollAxis::Y, Dir::Pos), None);
    }

    #[test]
    fn the_touchpad_locks_the_larger_axis_at_10_px() {
        let (lock, pending) = AxisLock::START.feed(3.0, -4.0);
        assert_eq!(lock, AxisLock::Free { dx: 3.0, dy: -4.0 });
        assert_eq!(pending, None);
        let (lock, pending) = lock.feed(1.0, -7.0);
        assert_eq!(lock, AxisLock::Locked(ScrollAxis::Y));
        assert_eq!(pending, Some(-11.0));
        assert_eq!(
            lock.feed(50.0, 0.0),
            (AxisLock::Locked(ScrollAxis::Y), None)
        );
        let (lock, _) = AxisLock::START.feed(12.0, 12.0);
        assert_eq!(lock, AxisLock::Locked(ScrollAxis::Y), "ties go vertical");
    }

    #[test]
    fn a_wheel_burst_keeps_its_target_for_300_ms() {
        let window = Duration::from_millis(300);
        let latch = Latch {
            scroller: Scroller::Viewport,
            axis: ScrollAxis::Y,
        };
        let at = |ms| HostInstant(Duration::from_millis(ms));
        let burst = WheelBurst::default().record(latch, at(1000));
        assert_eq!(burst.current(ScrollAxis::Y, at(1299), window), Some(latch));
        assert_eq!(burst.current(ScrollAxis::Y, at(1300), window), None);
        assert_eq!(burst.current(ScrollAxis::X, at(1100), window), None);
        assert_eq!(
            WheelBurst::default().current(ScrollAxis::Y, at(0), window),
            None
        );
    }
}

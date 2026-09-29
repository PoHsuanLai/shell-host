//! Double-scroll suppression, the safety net (design/11 §11.3.14 rule 2, 12 §12.3.11): the
//! same stroke can reach our surface from palmrest directly and again through the compositor
//! from palmrest's virtual touchpad. While palmrest has fingers down, while a glide or rebound
//! it drove is running, and for a grace period after its last Ended (or until an `axis_stop`),
//! compositor axis events of source finger and wheel are dropped. Pure.

use std::time::Duration;

use crate::tick::HostInstant;

/// Where palmrest's fingers are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fingers {
    Down,
    Up,
}

/// Whether a host animation palmrest started is still running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Driven {
    Running,
    Settled,
}

/// A compositor axis event's source, as far as suppression cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AxisKind {
    /// `finger` or `wheel`: what palmrest's outputs arrive as.
    Suppressible,
    /// Anything else (continuous, wheel tilt).
    Other,
}

/// Whether a compositor axis event reaches the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    Deliver,
    Drop,
}

/// The suppression window.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct Suppress {
    fingers: Option<Fingers>,
    /// When palmrest's fingers last lifted (its Ended, or `Touches{Up}`); `None` once an
    /// `axis_stop` closed the grace.
    lifted: Option<HostInstant>,
}

impl Suppress {
    /// palmrest reported its fingers `Down` or `Up` at `now` (`Touches`, or a scroll phase:
    /// MayBegin / Began / Changed are down, Ended / Cancelled are up).
    pub(crate) fn fingers(self, fingers: Fingers, now: HostInstant) -> Suppress {
        Suppress {
            fingers: Some(fingers),
            lifted: match fingers {
                Fingers::Up => Some(now),
                Fingers::Down => None,
            },
        }
    }

    /// A compositor `axis_stop` arrived: the grace after a lift ends.
    pub(crate) fn axis_stop(self) -> Suppress {
        Suppress {
            lifted: None,
            ..self
        }
    }

    /// Whether a compositor axis event of `kind` at `now` is delivered, while a palmrest-driven
    /// animation is `driven`.
    pub(crate) fn verdict(
        &self,
        kind: AxisKind,
        driven: Driven,
        grace: Duration,
        now: HostInstant,
    ) -> Verdict {
        if kind == AxisKind::Other {
            return Verdict::Deliver;
        }
        let in_grace = self
            .lifted
            .is_some_and(|at| now.0.saturating_sub(at.0) < grace);
        match (self.fingers, driven, in_grace) {
            (Some(Fingers::Down), _, _) | (_, Driven::Running, _) | (_, _, true) => Verdict::Drop,
            _ => Verdict::Deliver,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRACE: Duration = Duration::from_millis(150);

    fn at(ms: u64) -> HostInstant {
        HostInstant(Duration::from_millis(ms))
    }

    #[test]
    fn the_window_of_11_3_14() {
        let s = AxisKind::Suppressible;
        let quiet = Suppress::default();
        let down = quiet.fingers(Fingers::Down, at(0));
        let up = down.fingers(Fingers::Up, at(1000));
        let cases = [
            (
                "never touched",
                quiet,
                s,
                Driven::Settled,
                at(0),
                Verdict::Deliver,
            ),
            (
                "fingers down",
                down,
                s,
                Driven::Settled,
                at(500),
                Verdict::Drop,
            ),
            (
                "down, a tilt wheel",
                down,
                AxisKind::Other,
                Driven::Settled,
                at(500),
                Verdict::Deliver,
            ),
            (
                "grace after the lift",
                up,
                s,
                Driven::Settled,
                at(1149),
                Verdict::Drop,
            ),
            (
                "grace over",
                up,
                s,
                Driven::Settled,
                at(1150),
                Verdict::Deliver,
            ),
            (
                "glide still running",
                up,
                s,
                Driven::Running,
                at(3000),
                Verdict::Drop,
            ),
            (
                "axis_stop ends the grace",
                up.axis_stop(),
                s,
                Driven::Settled,
                at(1010),
                Verdict::Deliver,
            ),
        ];
        for (name, state, kind, driven, now, want) in cases {
            assert_eq!(state.verdict(kind, driven, GRACE, now), want, "{name}");
        }
    }
}

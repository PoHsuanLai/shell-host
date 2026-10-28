//! Double-scroll suppression, the safety net (design/11 §11.3.14 rule 2, 12 §12.3.11): the
//! same stroke can reach our surface from palmrest directly and again through the compositor
//! from palmrest's virtual touchpad. While palmrest has fingers down, while a glide or rebound
//! it drove is running, and for a grace period after its last Ended (or until an `axis_stop`),
//! compositor axis events of source finger and wheel are dropped. Pure.

use std::time::Duration;

use blitz_kit::scroll::time::Elapsed;

/// The suppression window.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct Suppress {
    /// Whether palmrest's fingers are down, once it has said.
    fingers_down: Option<bool>,
    /// When palmrest's fingers last lifted (its Ended, or `Touches{Up}`); `None` once an
    /// `axis_stop` closed the grace.
    lifted: Option<Elapsed>,
}

impl Suppress {
    /// palmrest reported its fingers `down` or up at `now` (`Touches`, or a scroll phase:
    /// MayBegin / Began / Changed are down, Ended / Cancelled are up).
    pub(crate) fn fingers(self, down: bool, now: Elapsed) -> Suppress {
        Suppress {
            fingers_down: Some(down),
            lifted: (!down).then_some(now),
        }
    }

    /// A compositor `axis_stop` arrived: the grace after a lift ends.
    pub(crate) fn axis_stop(self) -> Suppress {
        Suppress {
            lifted: None,
            ..self
        }
    }

    /// Whether a compositor axis event at `now` is delivered. `suppressible`: its source is
    /// `finger` or `wheel`, what palmrest's outputs arrive as (anything else, continuous or
    /// wheel tilt, is always delivered). `driven`: a palmrest-driven animation is running.
    pub(crate) fn delivers(
        &self,
        suppressible: bool,
        driven: bool,
        grace: Duration,
        now: Elapsed,
    ) -> bool {
        if !suppressible {
            return true;
        }
        let in_grace = self
            .lifted
            .is_some_and(|at| now.0.saturating_sub(at.0) < grace);
        !(self.fingers_down == Some(true) || driven || in_grace)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRACE: Duration = Duration::from_millis(150);

    fn at(ms: u64) -> Elapsed {
        Elapsed(Duration::from_millis(ms))
    }

    #[test]
    fn the_window_of_11_3_14() {
        let quiet = Suppress::default();
        let down = quiet.fingers(true, at(0));
        let up = down.fingers(false, at(1000));
        // name, state, suppressible source, driven, now, delivered
        let cases = [
            ("never touched", quiet, true, false, at(0), true),
            ("fingers down", down, true, false, at(500), false),
            ("down, a tilt wheel", down, false, false, at(500), true),
            ("grace after the lift", up, true, false, at(1149), false),
            ("grace over", up, true, false, at(1150), true),
            ("glide still running", up, true, true, at(3000), false),
            (
                "axis_stop ends the grace",
                up.axis_stop(),
                true,
                false,
                at(1010),
                true,
            ),
        ];
        for (name, state, suppressible, driven, now, want) in cases {
            assert_eq!(
                state.delivers(suppressible, driven, GRACE, now),
                want,
                "{name}"
            );
        }
    }
}

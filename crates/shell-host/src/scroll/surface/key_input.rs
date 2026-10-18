//! Keyboard and programmatic scrolling: scroll keys on the focused or remembered scroller
//! (design/11 §11.3.10) and `ScrollCmd` (§11.3.11).

use blitz_dom::BaseDocument;

use super::pointer_input::view_point;
use super::{Moved, SurfaceScroll};
use crate::input::key::Repeat;
use crate::input::pointer::PointerPos;
use crate::tick::HostInstant;
use blitz_kit::scroll::cmd::ScrollCmd;
use blitz_kit::scroll::driver::{Claim, KeyRepeat};
use blitz_kit::scroll::keys::ScrollKey;

/// The engine's name for a key event's repeat.
fn repeat_of(repeat: Repeat) -> KeyRepeat {
    match repeat {
        Repeat::First => KeyRepeat::First,
        Repeat::Repeated => KeyRepeat::Repeated,
    }
}

impl SurfaceScroll {
    /// A scroll key went down (`Repeat::First`) or repeated. `Claim::Left` (the document's)
    /// when something on the focus path handles keys or nothing around the focus scrolls; the
    /// `Moved` is what the engine's writes did.
    pub(crate) fn key(
        &mut self,
        doc: &mut BaseDocument,
        key: ScrollKey,
        repeat: Repeat,
        pos: Option<PointerPos>,
        now: HostInstant,
    ) -> (Claim, Moved) {
        let tuning = self.tuning;
        let env = tuning.at(now);
        let pointer = pos.map(view_point);
        let moves = self
            .scroll
            .key(doc, key, repeat_of(repeat), pointer, env.kit);
        (moves.claim(), self.applied(moves, now))
    }

    /// A held scroll key was released.
    pub(crate) fn key_up(&mut self, doc: &mut BaseDocument, now: HostInstant) -> Moved {
        let tuning = self.tuning;
        let env = tuning.at(now);
        let moves = self.scroll.key_up(doc, env.kit);
        self.applied(moves, now)
    }

    /// A programmatic scroll.
    pub(crate) fn command(
        &mut self,
        doc: &mut BaseDocument,
        command: &ScrollCmd,
        now: HostInstant,
    ) -> Moved {
        let tuning = self.tuning;
        let env = tuning.at(now);
        let moves = self.scroll.command(doc, command, env.kit);
        self.applied(moves, now)
    }
}

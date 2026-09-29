//! One resolve: layout, hoisting, the IME and hover repairs, and the pre-paint hook.

use blitz_dom::Document as _;
use blitz_kit::hover::Shift;
use blitz_traits::shell::ShellProvider as _;

use super::SurfaceDocument;
use super::hover::HoverShift;
use crate::dom::ime_focus::{self, FieldText, Repair};
use crate::dom::stacking;
use crate::frame::Animation;
use crate::prepaint::Prepainted;

/// What one `SurfaceDocument::resolve` did besides laying out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolved {
    pub prepainted: Prepainted,
    pub hover: HoverShift,
}

impl SurfaceDocument {
    /// Resolve style and layout at document time `t` (seconds on the host clock), hoist the
    /// z-indexed boxes again from the fresh layout (Blitz gathers them from the previous one),
    /// enable the IME for a text field focused before it had a layout, repair the hover the resolve
    /// moved, then run the pre-paint hook on the result (and hoist once more after it). Resolve and
    /// hook are one call because Blitz re-rounds the layout on every resolve: a hook that snapped
    /// it must follow each one, or the paint shows the unsnapped layout. So a frame resolves
    /// exactly once, here, and paints what this left. The hover repair touches hover state and runs
    /// handlers, never layout, so the hook and the paint see the same layout; the restyle it causes
    /// is the next frame's (`Resolved::hover`).
    pub fn resolve(&mut self, t: f64) -> Resolved {
        let before = self.hovered();
        self.doc.inner_mut().resolve(t);
        stacking::rehoist(&mut self.doc.inner_mut());
        self.repair_ime();
        let after = self.hovered();
        self.sync_hover(Shift { before, after });
        let prepainted = self.prepaint.run(&mut self.doc.inner_mut());
        if prepainted == Prepainted::Ran {
            // The hook may have moved boxes (snapping); hoist from where it left them.
            stacking::rehoist(&mut self.doc.inner_mut());
        }
        let hover = if before == after {
            HoverShift::Kept
        } else {
            HoverShift::Moved
        };
        Resolved { prepainted, hover }
    }

    /// Ask for the IME a text field focused before its first layout could not ask for itself
    /// (`dom::ime_focus`).
    fn repair_ime(&self) {
        let focused = ime_focus::focused_field(&self.doc.inner());
        if let Repair::Enable { cursor_area } = ime_focus::repair(self.shell.ime(), focused) {
            self.shell.set_ime_cursor_area(
                cursor_area.x as f32,
                cursor_area.y as f32,
                cursor_area.width as f32,
                cursor_area.height as f32,
            );
        }
    }

    /// The focused text field's text (`debug-inject`'s `focused`).
    pub fn focused_text(&self) -> Option<FieldText> {
        ime_focus::focused_text(&self.doc.inner())
    }

    /// Whether a transition or animation is still running.
    pub fn animation(&self) -> Animation {
        if self.doc.inner().is_animating() {
            Animation::Running
        } else {
            Animation::Stopped
        }
    }
}

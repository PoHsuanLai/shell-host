//! One resolve: layout, hoisting, the IME and hover repairs, and the pre-paint hook.

use blitz_dom::Document as _;
use blitz_dom::node::SurroundingText;
use blitz_kit::after_resolve;
use blitz_kit::hover::Shift;
use blitz_traits::shell::ShellProvider as _;

use super::SurfaceDocument;
use crate::dom::ime_focus;
#[cfg(feature = "debug-inject")]
use crate::dom::ime_focus::FieldText;
use crate::dom::{scheme, stacking};

/// What one `SurfaceDocument::resolve` did besides laying out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Resolved {
    /// A pre-paint hook ran.
    pub prepainted: bool,
    /// The resolve moved the hover: the restyle it needs lands on a later frame, and the
    /// resolve's own redraw request is dropped as the render's echo (`ctx::paint`).
    pub hover_moved: bool,
    /// Readings that changed and were published to a live component.
    pub published: usize,
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
    /// is the next frame's (`Resolved::hover`). Last, the components' after-resolve readings run on
    /// the result, and a reading that changed sets its signal, so the component renders with it on
    /// the next poll (`Resolved::published`). Before any of that the viewport follows the root's
    /// colour scheme (`dom::scheme`), so a switch is laid out with every box rebuilt.
    pub(crate) fn resolve(&mut self, t: f64) -> Resolved {
        let before = self.hovered();
        scheme::follow_root(&mut self.doc.inner_mut());
        self.doc.inner_mut().resolve(t);
        stacking::rehoist(&mut self.doc.inner_mut());
        self.repair_ime();
        let after = self.hovered();
        self.sync_hover(Shift { before, after });
        let prepainted = self.prepaint.run(&mut self.doc.inner_mut());
        if prepainted {
            // The hook may have moved boxes (snapping); hoist from where it left them.
            stacking::rehoist(&mut self.doc.inner_mut());
        }
        let publishes = self.after.read(&self.doc.inner());
        let published = self
            .doc
            .vdom
            .in_runtime(|| after_resolve::publish(publishes));
        Resolved {
            prepainted,
            hover_moved: before != after,
            published,
        }
    }

    /// Ask for the IME a text field focused before its first layout could not ask for itself
    /// (`dom::ime_focus`).
    fn repair_ime(&self) {
        let focused = ime_focus::focused_field(&self.doc.inner());
        if let Some(cursor_area) = ime_focus::repair(self.shell.ime(), focused) {
            self.shell.set_ime_cursor_area(
                cursor_area.x as f32,
                cursor_area.y as f32,
                cursor_area.width as f32,
                cursor_area.height as f32,
            );
        }
    }

    /// The focused text field's surrounding text for the input method, if a text field has the
    /// focus (`handlers::surrounding`). A password field's is empty, so its text never leaves the
    /// app and the text of the field before it is cleared.
    pub(crate) fn surrounding_text(&self) -> Option<SurroundingText> {
        let doc = self.doc.inner();
        let text = doc.focused_surrounding_text()?;
        Some(if ime_focus::focused_secret(&doc) {
            SurroundingText {
                text: String::new(),
                cursor: 0,
                anchor: 0,
            }
        } else {
            text
        })
    }

    /// Whether the focused field is a password field (`handlers::text_input`).
    pub(crate) fn field_secret(&self) -> bool {
        ime_focus::focused_secret(&self.doc.inner())
    }

    #[cfg(feature = "debug-inject")]
    /// The focused text field's text (`debug-inject`'s `focused`).
    pub(crate) fn focused_text(&self) -> Option<FieldText> {
        ime_focus::focused_text(&self.doc.inner())
    }

    /// Whether a transition or animation is still running.
    pub(crate) fn animation(&self) -> bool {
        self.doc.inner().is_animating()
    }
}

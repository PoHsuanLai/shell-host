//! The pre-paint hook: a consumer's pass over every laid-out document, between `resolve` and
//! the paint.
//!
//! It exists for pixel snapping. Blitz lays out in logical pixels and rounds every box to whole
//! *logical* pixels at the end of each `resolve`; at 1.25, 1.5 or 1.75 that is not the device
//! grid, so a 1 px line paints as one full row and one half row. A design system that knows the
//! device grid (quire's `ds_native::snap_to_device`) redoes the rounding on it, and it has to run
//! after *every* resolve, because every resolve re-runs Blitz's rounding. shell-host cannot call
//! it itself (it stays free of any design-system dependency), so it takes the pass as a hook.
//!
//! See FINDINGS F43.

use std::fmt;
use std::sync::Arc;

use blitz_dom::BaseDocument;

/// The pass itself. `Arc` + `Send + Sync` so a `HostConfig` holding one can still be built on
/// any thread; it is only ever called on the host thread.
pub type PrepaintFn = dyn Fn(&mut BaseDocument) + Send + Sync;

/// What the host does to each document after `resolve` and before painting it.
///
/// A hook runs once per rendered frame of every surface and popup (and once per debug
/// `capture`), on the host thread, after the document's viewport carries the surface's current
/// scale; never on an idle frame, since an idle surface renders nothing. It must be cheap (it
/// is on every frame's critical path) and must not request a redraw: a hook that dirties the
/// document it is handed would make every frame schedule the next, and the surface would never
/// idle.
#[derive(Clone, Default)]
pub enum Prepaint {
    /// Paint what Blitz laid out.
    #[default]
    None,
    Hook(Arc<PrepaintFn>),
}

impl Prepaint {
    /// A hook running `pass`.
    pub fn hook(pass: impl Fn(&mut BaseDocument) + Send + Sync + 'static) -> Prepaint {
        Prepaint::Hook(Arc::new(pass))
    }

    /// Run the hook, if any, on a just-resolved `doc`; whether one ran (the `prepaints` counter).
    pub(crate) fn run(&self, doc: &mut BaseDocument) -> bool {
        match self {
            Prepaint::None => false,
            Prepaint::Hook(pass) => {
                pass(doc);
                true
            }
        }
    }
}

impl fmt::Debug for Prepaint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Prepaint::None => f.write_str("None"),
            Prepaint::Hook(_) => f.write_str("Hook(..)"),
        }
    }
}

/// Two hooks are equal when they are the same hook (the same `Arc`), since functions have no
/// other equality; `HostConfig` compares by this.
impl PartialEq for Prepaint {
    fn eq(&self, other: &Prepaint) -> bool {
        match (self, other) {
            (Prepaint::None, Prepaint::None) => true,
            (Prepaint::Hook(a), Prepaint::Hook(b)) => Arc::ptr_eq(a, b),
            (Prepaint::None, Prepaint::Hook(_)) | (Prepaint::Hook(_), Prepaint::None) => false,
        }
    }
}

impl Eq for Prepaint {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hook_equals_only_itself() {
        let hook = Prepaint::hook(|_| {});
        assert_eq!(hook, hook.clone());
        assert_ne!(hook, Prepaint::hook(|_| {}));
        assert_ne!(hook, Prepaint::None);
    }
}

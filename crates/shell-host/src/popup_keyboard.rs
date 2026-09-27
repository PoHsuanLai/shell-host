//! Whether opening a popup lifts its parent layer's keyboard (FINDINGS F72).
//!
//! A bar is a `KeyboardMode::None` layer, and cosmic-comp gives a grabbing popup of a layer the
//! keyboard only when the layer itself takes keyboard input, so the host switches the parent to
//! `OnDemand` while a grabbing popup is open (the plan's input rule). KWin needs no lift: its
//! popup filter focuses the topmost grabbing popup itself and, when the popup closes, hands the
//! keyboard back to the active window. Lifting there makes KWin *activate* the bar
//! (`LayerShellV1Window::handleAcceptsFocusChanged`), and nothing deactivates it when the host
//! drops it back to `None`: the bar keeps the keyboard after every menu closes, and the app the
//! user was typing in never gets it back. Pure.

use crate::blur_health::CompositorKind;
use crate::layer::KeyboardMode;
use crate::popup::Grab;

/// What opening a popup does to its parent layer's keyboard mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParentKeyboard {
    /// Switch the parent to `OnDemand` until its last grabbing popup closes.
    Lift,
    /// Leave the parent as it is.
    Keep,
}

/// The rule: a grabbing popup of a `None` layer lifts it, except on KWin. `parent` is the
/// layer's current mode (already `OnDemand` while another grabbing popup holds it lifted).
pub fn parent_keyboard(kind: CompositorKind, grab: Grab, parent: KeyboardMode) -> ParentKeyboard {
    match (kind, grab, parent) {
        (CompositorKind::Kwin, _, _) => ParentKeyboard::Keep,
        (_, Grab::Grabbed, KeyboardMode::None) => ParentKeyboard::Lift,
        _ => ParentKeyboard::Keep,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_grabbing_popup_of_a_keyboardless_layer_lifts_it_and_never_on_kwin() {
        use CompositorKind as K;
        use Grab::{Grabbed, Ungrabbed};
        use KeyboardMode as M;
        use ParentKeyboard::{Keep, Lift};
        let cases = [
            (K::Cosmic, Grabbed, M::None, Lift),
            (K::Other, Grabbed, M::None, Lift),
            (K::Kwin, Grabbed, M::None, Keep),
            (K::Cosmic, Ungrabbed, M::None, Keep),
            (K::Kwin, Ungrabbed, M::None, Keep),
            (K::Cosmic, Grabbed, M::OnDemand, Keep),
            (K::Cosmic, Grabbed, M::Exclusive, Keep),
            (K::Other, Grabbed, M::OnDemand, Keep),
            (K::Kwin, Grabbed, M::Exclusive, Keep),
        ];
        for (kind, grab, parent, want) in cases {
            assert_eq!(
                parent_keyboard(kind, grab, parent),
                want,
                "{kind:?} {grab:?} {parent:?}"
            );
        }
    }
}

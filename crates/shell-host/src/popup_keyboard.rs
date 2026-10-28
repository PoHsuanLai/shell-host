//! Whether opening a popup lifts its parent layer's keyboard.
//!
//! A bar is a `KeyboardMode::None` layer, and a compositor may give a grabbing popup of a layer the
//! keyboard only when the layer itself takes keyboard input, so the host switches the parent to
//! `OnDemand` while a grabbing popup is open (the plan's input rule). KWin needs no lift: its
//! popup filter focuses the topmost grabbing popup itself and, when the popup closes, hands the
//! keyboard back to the active window. Lifting there makes KWin *activate* the bar
//! (`LayerShellV1Window::handleAcceptsFocusChanged`), and nothing deactivates it when the host
//! drops it back to `None`: the bar keeps the keyboard after every menu closes, and the app the
//! user was typing in never gets it back. casement behaves as KWin does there (an xdg_popup grab
//! takes the keyboard from the layer under it and gives it back on the dismiss), so it takes the
//! same arm. Pure.
//!
//! See FINDINGS F72.

use crate::layer::KeyboardMode;
use crate::popup::PopupGrab;

/// Whether the host lifts a parent layer's keyboard for a grabbing popup on this compositor:
/// everywhere except KWin and casement. KWin is recognised by `org_kde_plasma_shell` (wlroots
/// compositors offer some `org_kde_kwin_*` globals, never this one); casement by
/// `quire_window_manager_v1` (advertised from its first phase, C50). `interfaces` are the
/// globals the compositor advertises.
pub(crate) fn lifts_parent_keyboard<'a>(interfaces: impl IntoIterator<Item = &'a str>) -> bool {
    !interfaces
        .into_iter()
        .any(|name| name == "org_kde_plasma_shell" || name == "quire_window_manager_v1")
}

/// The rule: a grabbing popup of a `None` layer lifts the parent (to `OnDemand` until its last
/// grabbing popup closes), except where `lifts` is false (KWin, casement). `parent` is the
/// layer's current mode (already `OnDemand` while another grabbing popup holds it lifted).
pub(crate) fn parent_keyboard(lifts: bool, grab: PopupGrab, parent: KeyboardMode) -> bool {
    lifts && grab == PopupGrab::Grabbed && parent == KeyboardMode::None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_grabbing_popup_of_a_keyboardless_layer_lifts_it_and_never_on_kwin_or_casement() {
        use KeyboardMode as M;
        use PopupGrab::{Grabbed, Ungrabbed};
        const OTHER: bool = true;
        const KWIN_OR_CASEMENT: bool = false;
        let cases = [
            (OTHER, Grabbed, M::None, true),
            (KWIN_OR_CASEMENT, Grabbed, M::None, false),
            (KWIN_OR_CASEMENT, Ungrabbed, M::None, false),
            (OTHER, Grabbed, M::Exclusive, false),
            (OTHER, Grabbed, M::OnDemand, false),
            (OTHER, Ungrabbed, M::None, false),
            (KWIN_OR_CASEMENT, Grabbed, M::Exclusive, false),
            (KWIN_OR_CASEMENT, Grabbed, M::OnDemand, false),
        ];
        for (lifts, grab, parent, want) in cases {
            assert_eq!(
                parent_keyboard(lifts, grab, parent),
                want,
                "{lifts} {grab:?} {parent:?}"
            );
        }
    }

    #[test]
    fn kwin_and_casement_are_recognised_by_their_private_globals() {
        let cases: &[(&[&str], bool)] = &[
            (&["wl_compositor", "org_kde_kwin_blur_manager"], true),
            (
                &["wl_compositor", "org_kde_kwin_server_decoration_manager"],
                true,
            ),
            (
                &[
                    "wl_compositor",
                    "org_kde_plasma_shell",
                    "org_kde_kwin_blur_manager",
                ],
                false,
            ),
            (
                &["wl_compositor", "quire_window_manager_v1", "xdg_wm_base"],
                false,
            ),
            (&["quire_window_v1"], true),
            (&[], true),
        ];
        for (interfaces, want) in cases {
            assert_eq!(
                lifts_parent_keyboard(interfaces.iter().copied()),
                *want,
                "{interfaces:?}"
            );
        }
    }
}

//! Which compositor this is, where its behaviour differs: a grabbing popup's keyboard (KWin,
//! casement; `popup_keyboard`).
//!
//! Pure: recognised from the globals the compositor advertises.

/// The compositors the host treats specially.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompositorKind {
    Kwin,
    /// casement, our own compositor.
    Casement,
    Other,
}

/// KWin is recognised by `org_kde_plasma_shell` (wlroots compositors offer some `org_kde_kwin_*`
/// globals, never this one); casement by `quire_window_manager_v1` (advertised from its first
/// phase, C50).
pub(crate) fn compositor_kind<'a>(interfaces: impl IntoIterator<Item = &'a str>) -> CompositorKind {
    let kwin = |name: &str| name == "org_kde_plasma_shell";
    let casement = |name: &str| name == "quire_window_manager_v1";
    interfaces
        .into_iter()
        .find_map(|name| {
            if kwin(name) {
                Some(CompositorKind::Kwin)
            } else if casement(name) {
                Some(CompositorKind::Casement)
            } else {
                None
            }
        })
        .unwrap_or(CompositorKind::Other)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kwin_and_casement_are_recognised_by_their_private_globals() {
        let cases: &[(&[&str], CompositorKind)] = &[
            (
                &["wl_compositor", "org_kde_kwin_blur_manager"],
                CompositorKind::Other,
            ),
            (
                &["wl_compositor", "org_kde_kwin_server_decoration_manager"],
                CompositorKind::Other,
            ),
            (
                &[
                    "wl_compositor",
                    "org_kde_plasma_shell",
                    "org_kde_kwin_blur_manager",
                ],
                CompositorKind::Kwin,
            ),
            (
                &["wl_compositor", "quire_window_manager_v1", "xdg_wm_base"],
                CompositorKind::Casement,
            ),
            (&["quire_window_v1"], CompositorKind::Other),
            (&[], CompositorKind::Other),
        ];
        for (interfaces, want) in cases {
            assert_eq!(
                compositor_kind(interfaces.iter().copied()),
                *want,
                "{interfaces:?}"
            );
        }
    }
}

//! Whether a compositor that advertises blur actually renders it.
//!
//! `ext_background_effect_manager_v1.capabilities` only says the compositor accepts a blur
//! region. cosmic-comp 1.8.0's nested winit backend with NVIDIA's EGL advertises blur, accepts
//! the region, and then loses every pixel of a layer surface that has one: the surface stays
//! mapped and receives frame callbacks, but nothing of it reaches the output. The condition is
//! not observable from the protocol, so it is recognised from what the compositor is instead:
//! cosmic-comp (its private globals), rendering through the winit backend (its outputs are named
//! `WINIT-n`), on an NVIDIA device (the `zwp_linux_dmabuf_v1` main device). The same compositor
//! on Mesa, and KWin, render blur correctly and are left alone.
//!
//! Pure: the facts are gathered by `handlers::render_device` and `HostState`.

use blitz_kit::adapter::PciVendor;

/// Which compositor this is, where its behaviour differs: blur (cosmic-comp) and a
/// grabbing popup's keyboard (KWin, `popup_keyboard`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompositorKind {
    Cosmic,
    Kwin,
    Other,
}

/// How the compositor puts pixels on its outputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputBackend {
    /// Nested inside another compositor through winit (cosmic-comp's `COSMIC_BACKEND=winit`).
    Winit,
    /// Anything else: KMS, a virtual output, or not known yet.
    Other,
}

/// The vendor of the compositor's main render device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RenderVendor {
    Nvidia,
    Other,
    /// No `zwp_linux_dmabuf_v1` v4 feedback, or the device could not be looked up.
    Unknown,
}

/// What the host knows about the compositor, for [`blur_health`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CompositorFacts {
    pub kind: CompositorKind,
    pub backend: OutputBackend,
    pub vendor: RenderVendor,
}

/// Whether blur, once advertised, renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BlurHealth {
    Renders,
    /// A known compositor bug makes a blurred surface disappear; the host sends no blur region.
    Breaks,
}

/// The verdict for `facts`: only the exact condition that loses blurred surfaces (cosmic-comp on
/// the winit backend with an NVIDIA render device) is `Breaks`.
pub(crate) fn blur_health(facts: CompositorFacts) -> BlurHealth {
    match facts {
        CompositorFacts {
            kind: CompositorKind::Cosmic,
            backend: OutputBackend::Winit,
            vendor: RenderVendor::Nvidia,
        } => BlurHealth::Breaks,
        _ => BlurHealth::Renders,
    }
}

/// cosmic-comp, recognised by any of its private globals (`zcosmic_*`, `cosmic_*`); KWin by
/// `org_kde_plasma_shell` (wlroots compositors offer some `org_kde_kwin_*` globals, never this
/// one).
pub(crate) fn compositor_kind<'a>(interfaces: impl IntoIterator<Item = &'a str>) -> CompositorKind {
    let cosmic = |name: &str| name.starts_with("zcosmic_") || name.starts_with("cosmic_");
    let kwin = |name: &str| name == "org_kde_plasma_shell";
    interfaces
        .into_iter()
        .find_map(|name| {
            if cosmic(name) {
                Some(CompositorKind::Cosmic)
            } else if kwin(name) {
                Some(CompositorKind::Kwin)
            } else {
                None
            }
        })
        .unwrap_or(CompositorKind::Other)
}

/// The winit backend, recognised by an output named `WINIT-n` (`wl_output.name`, v4).
pub(crate) fn output_backend<'a>(names: impl IntoIterator<Item = &'a str>) -> OutputBackend {
    if names.into_iter().any(|name| name.starts_with("WINIT-")) {
        OutputBackend::Winit
    } else {
        OutputBackend::Other
    }
}

/// The vendor from a PCI vendor id (`0x10de` is NVIDIA).
pub(crate) fn render_vendor(vendor: PciVendor) -> RenderVendor {
    match vendor {
        NVIDIA => RenderVendor::Nvidia,
        _ => RenderVendor::Other,
    }
}

const NVIDIA: PciVendor = PciVendor(0x10de);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_nested_cosmic_on_nvidia_breaks() {
        use CompositorKind as K;
        use OutputBackend as B;
        use RenderVendor as V;
        let cases = [
            (K::Cosmic, B::Winit, V::Nvidia, BlurHealth::Breaks),
            (K::Kwin, B::Winit, V::Nvidia, BlurHealth::Renders),
            (K::Cosmic, B::Winit, V::Other, BlurHealth::Renders),
            (K::Cosmic, B::Winit, V::Unknown, BlurHealth::Renders),
            (K::Cosmic, B::Other, V::Nvidia, BlurHealth::Renders),
            (K::Other, B::Winit, V::Nvidia, BlurHealth::Renders),
            (K::Other, B::Other, V::Nvidia, BlurHealth::Renders),
        ];
        for (kind, backend, vendor, want) in cases {
            let facts = CompositorFacts {
                kind,
                backend,
                vendor,
            };
            assert_eq!(blur_health(facts), want, "{facts:?}");
        }
    }

    #[test]
    fn cosmic_and_kwin_are_recognised_by_their_private_globals() {
        let cases: &[(&[&str], CompositorKind)] = &[
            (
                &["wl_compositor", "zcosmic_output_manager_v1"],
                CompositorKind::Cosmic,
            ),
            (&["cosmic_corner_radius_manager_v1"], CompositorKind::Cosmic),
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

    #[test]
    fn the_winit_backend_is_recognised_by_its_output_name() {
        let cases: &[(&[&str], OutputBackend)] = &[
            (&["WINIT-0"], OutputBackend::Winit),
            (&["HDMI-A-2", "WINIT-1"], OutputBackend::Winit),
            (&["Virtual-1"], OutputBackend::Other),
            (&["DP-1"], OutputBackend::Other),
            (&[], OutputBackend::Other),
        ];
        for (names, want) in cases {
            assert_eq!(output_backend(names.iter().copied()), *want, "{names:?}");
        }
    }

    #[test]
    fn vendor_ids_are_recognised() {
        assert_eq!(render_vendor(PciVendor(0x10de)), RenderVendor::Nvidia);
        assert_eq!(render_vendor(PciVendor(0x1002)), RenderVendor::Other);
    }
}

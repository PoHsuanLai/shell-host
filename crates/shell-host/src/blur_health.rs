//! Whether a compositor that advertises blur actually renders it (FINDINGS F29).
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

/// Which compositor this is, as far as blur is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositorKind {
    Cosmic,
    Other,
}

/// How the compositor puts pixels on its outputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputBackend {
    /// Nested inside another compositor through winit (cosmic-comp's `COSMIC_BACKEND=winit`).
    Winit,
    /// Anything else: KMS, a virtual output, or not known yet.
    Other,
}

/// The vendor of the compositor's main render device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderVendor {
    Nvidia,
    Other,
    /// No `zwp_linux_dmabuf_v1` v4 feedback, or the device could not be looked up.
    Unknown,
}

/// What the host knows about the compositor, for [`blur_health`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompositorFacts {
    pub kind: CompositorKind,
    pub backend: OutputBackend,
    pub vendor: RenderVendor,
}

/// Whether blur, once advertised, renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlurHealth {
    Renders,
    /// A known compositor bug makes a blurred surface disappear; the host sends no blur region.
    Breaks,
}

/// The verdict for `facts`: only the exact condition F29 reproduced is `Breaks`.
pub fn blur_health(facts: CompositorFacts) -> BlurHealth {
    match facts {
        CompositorFacts {
            kind: CompositorKind::Cosmic,
            backend: OutputBackend::Winit,
            vendor: RenderVendor::Nvidia,
        } => BlurHealth::Breaks,
        _ => BlurHealth::Renders,
    }
}

/// cosmic-comp, recognised by any of its private globals (`zcosmic_*`, `cosmic_*`).
pub fn compositor_kind<'a>(interfaces: impl IntoIterator<Item = &'a str>) -> CompositorKind {
    let cosmic = |name: &str| name.starts_with("zcosmic_") || name.starts_with("cosmic_");
    if interfaces.into_iter().any(cosmic) {
        CompositorKind::Cosmic
    } else {
        CompositorKind::Other
    }
}

/// The winit backend, recognised by an output named `WINIT-n` (`wl_output.name`, v4).
pub fn output_backend<'a>(names: impl IntoIterator<Item = &'a str>) -> OutputBackend {
    if names.into_iter().any(|name| name.starts_with("WINIT-")) {
        OutputBackend::Winit
    } else {
        OutputBackend::Other
    }
}

/// The vendor from a PCI vendor id as sysfs prints it (`0x10de\n`).
pub fn render_vendor(sysfs_vendor: &str) -> RenderVendor {
    match sysfs_vendor.trim() {
        "0x10de" => RenderVendor::Nvidia,
        "" => RenderVendor::Unknown,
        _ => RenderVendor::Other,
    }
}

/// A Linux device number, as `zwp_linux_dmabuf_feedback_v1.main_device` carries it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceNumber {
    pub major: u32,
    pub minor: u32,
}

impl DeviceNumber {
    /// Decode the wire's `dev_t` (native-endian bytes) with glibc's `major`/`minor` split.
    /// `None` unless it is exactly the 8 bytes of a Linux `dev_t`.
    pub fn from_wire(bytes: &[u8]) -> Option<DeviceNumber> {
        let dev = u64::from_ne_bytes(bytes.try_into().ok()?);
        let major = ((dev >> 32) & 0xffff_f000) | ((dev >> 8) & 0x0000_0fff);
        let minor = ((dev >> 12) & 0xffff_ff00) | (dev & 0x0000_00ff);
        Some(DeviceNumber {
            major: u32::try_from(major).ok()?,
            minor: u32::try_from(minor).ok()?,
        })
    }

    /// Where sysfs keeps the PCI vendor of this character device.
    pub fn sysfs_vendor_path(self) -> std::path::PathBuf {
        format!("/sys/dev/char/{}:{}/device/vendor", self.major, self.minor).into()
    }
}

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
    fn cosmic_is_recognised_by_its_private_globals() {
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
    fn vendor_ids_as_sysfs_prints_them() {
        assert_eq!(render_vendor("0x10de\n"), RenderVendor::Nvidia);
        assert_eq!(render_vendor("0x1002\n"), RenderVendor::Other);
        assert_eq!(render_vendor(""), RenderVendor::Unknown);
    }

    #[test]
    fn device_numbers_decode_like_glibc() {
        // 0xE280 is 226:128, /dev/dri/renderD128 (wayland-info's `main device: 0xE280`).
        let cases: &[(u64, Option<(u32, u32)>)] = &[
            (0xE280, Some((226, 128))),
            (0xE281, Some((226, 129))),
            // glibc's makedev(511, 300): a minor above 255 uses bits 20 and up.
            (0x0011_ff2c, Some((511, 300))),
        ];
        for (dev, want) in cases {
            let got = DeviceNumber::from_wire(&dev.to_ne_bytes()).map(|d| (d.major, d.minor));
            assert_eq!(got, *want, "{dev:#x}");
        }
        assert_eq!(DeviceNumber::from_wire(&[0; 4]), None);
    }
}

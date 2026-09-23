//! Hand-written Dispatch for the protocols sctk 0.20 has no helper for: fractional-scale-v1,
//! viewporter and ext-background-effect-v1. Any state implementing [`ScaleSink`] and
//! [`BlurSink`] gets them with [`delegate_spike_protocols!`](crate::delegate_spike_protocols).

use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};
use wayland_protocols::ext::background_effect::v1::client::{
    ext_background_effect_manager_v1::{self, ExtBackgroundEffectManagerV1},
    ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1,
};
use wayland_protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
    wp_fractional_scale_v1::{self, WpFractionalScaleV1},
};
use wayland_protocols::wp::viewporter::client::{
    wp_viewport::WpViewport, wp_viewporter::WpViewporter,
};

/// Receives `wp_fractional_scale_v1.preferred_scale` (in 120ths) for a surface.
pub trait ScaleSink {
    fn preferred_scale(&mut self, surface: &WlSurface, scale120: u32);
}

/// Receives `ext_background_effect_manager_v1.capabilities`.
pub trait BlurSink {
    fn blur_capabilities(&mut self, bits: u32);
}

/// The dispatch target the protocol objects delegate to.
#[derive(Debug)]
pub struct Protocols;

macro_rules! no_events {
    ($iface:ty, $udata:ty) => {
        impl<D> Dispatch<$iface, $udata, D> for Protocols
        where
            D: Dispatch<$iface, $udata>,
        {
            fn event(
                _: &mut D,
                _: &$iface,
                _: <$iface as wayland_client::Proxy>::Event,
                _: &$udata,
                _: &Connection,
                _: &QueueHandle<D>,
            ) {
            }
        }
    };
}

no_events!(WpFractionalScaleManagerV1, ());
no_events!(WpViewporter, ());
no_events!(WpViewport, ());
no_events!(ExtBackgroundEffectSurfaceV1, ());

impl<D> Dispatch<WpFractionalScaleV1, WlSurface, D> for Protocols
where
    D: Dispatch<WpFractionalScaleV1, WlSurface> + ScaleSink,
{
    fn event(
        state: &mut D,
        _: &WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        surface: &WlSurface,
        _: &Connection,
        _: &QueueHandle<D>,
    ) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event {
            state.preferred_scale(surface, scale);
        }
    }
}

impl<D> Dispatch<ExtBackgroundEffectManagerV1, (), D> for Protocols
where
    D: Dispatch<ExtBackgroundEffectManagerV1, ()> + BlurSink,
{
    fn event(
        state: &mut D,
        _: &ExtBackgroundEffectManagerV1,
        event: ext_background_effect_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<D>,
    ) {
        if let ext_background_effect_manager_v1::Event::Capabilities { flags } = event {
            let bits = match flags {
                WEnum::Value(c) => c.bits(),
                WEnum::Unknown(u) => u,
            };
            state.blur_capabilities(bits);
        }
    }
}

/// Wires the spike protocols into `$state`'s Dispatch.
#[macro_export]
macro_rules! delegate_spike_protocols {
    ($state:ty) => {
        use $crate::spike::protocols::Protocols as __P;
        use wayland_protocols::ext::background_effect::v1::client as __be;
        use wayland_protocols::wp::fractional_scale::v1::client as __fs;
        use wayland_protocols::wp::viewporter::client as __vp;
        wayland_client::delegate_dispatch!($state: [__fs::wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1: ()] => __P);
        wayland_client::delegate_dispatch!($state: [__fs::wp_fractional_scale_v1::WpFractionalScaleV1: wayland_client::protocol::wl_surface::WlSurface] => __P);
        wayland_client::delegate_dispatch!($state: [__vp::wp_viewporter::WpViewporter: ()] => __P);
        wayland_client::delegate_dispatch!($state: [__vp::wp_viewport::WpViewport: ()] => __P);
        wayland_client::delegate_dispatch!($state: [__be::ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1: ()] => __P);
        wayland_client::delegate_dispatch!($state: [__be::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1: ()] => __P);
    };
}

/// Physical buffer size for a logical size at `scale120` (the plan's round(logical * s)).
pub fn buffer_size(logical: (u32, u32), scale120: u32) -> (u32, u32) {
    let f = |v: u32| ((v as u64 * scale120 as u64 + 60) / 120) as u32;
    (f(logical.0).max(1), f(logical.1).max(1))
}

#[cfg(test)]
mod tests {
    use super::buffer_size;

    #[test]
    fn buffer_rounds_half_up() {
        type Case = ((u32, u32), u32, (u32, u32));
        const CASES: &[Case] = &[
            ((2259, 32), 120, (2259, 32)),
            ((2259, 32), 180, (3389, 48)),
            ((2259, 32), 204, (3840, 54)),
            ((1, 1), 180, (2, 2)),
        ];
        for (i, (l, s, want)) in CASES.iter().enumerate() {
            assert_eq!(buffer_size(*l, *s), *want, "case {i}");
        }
    }
}

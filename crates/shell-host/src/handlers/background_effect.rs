//! `ext_background_effect_v1`: blur behind a surface.
//!
//! smithay-client-toolkit 0.20 has no helper for this staging protocol (0.21 does), so this is
//! a hand-written `Dispatch`, shaped like sctk's own indirect-dispatch types (for example
//! `shell::xdg::popup::PopupData`): the manager and per-surface state live in `BackgroundEffect`,
//! and `Dispatch<I, U, D>` is implemented on `BackgroundEffect` itself, generic over the
//! caller's app state `D`. A caller wires it in with `wayland_client::delegate_dispatch!` and
//! `impl AsMut<BackgroundEffect> for D`.

use std::collections::HashMap;

use wayland_client::backend::ObjectId;
use wayland_client::globals::{BindError, GlobalList};
use wayland_client::protocol::{wl_region::WlRegion, wl_surface::WlSurface};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum};
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_manager_v1::{
    self, ExtBackgroundEffectManagerV1,
};
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::{
    self, ExtBackgroundEffectSurfaceV1,
};

/// Whether the compositor reported supporting an effect, from the manager's `capabilities`
/// event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Support {
    #[default]
    Unsupported,
    Supported,
}

/// The effects the compositor supports, as of the last `capabilities` event. There is only one
/// effect in `ext-background-effect-v1` today (`blur`); the struct, not a bare `Support`, is so
/// a second effect added to the protocol later is one field, not a signature change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Capabilities {
    pub blur: Support,
}

impl Capabilities {
    fn from_wire(flags: ext_background_effect_manager_v1::Capability) -> Capabilities {
        let blur = if flags.contains(ext_background_effect_manager_v1::Capability::Blur) {
            Support::Supported
        } else {
            Support::Unsupported
        };
        Capabilities { blur }
    }
}

/// State for `ext_background_effect_manager_v1`: the bound manager, the capabilities it last
/// reported, and the per-surface `ext_background_effect_surface_v1` objects created against it.
#[derive(Debug)]
pub(crate) struct BackgroundEffect {
    manager: ExtBackgroundEffectManagerV1,
    capabilities: Capabilities,
    surfaces: HashMap<ObjectId, ExtBackgroundEffectSurfaceV1>,
}

impl BackgroundEffect {
    /// Bind the `ext_background_effect_manager_v1` global.
    pub(crate) fn bind<D>(
        globals: &GlobalList,
        qh: &QueueHandle<D>,
    ) -> Result<BackgroundEffect, BindError>
    where
        D: Dispatch<ExtBackgroundEffectManagerV1, ()> + 'static,
    {
        let manager = globals.bind(qh, 1..=1, ())?;
        Ok(BackgroundEffect {
            manager,
            capabilities: Capabilities::default(),
            surfaces: HashMap::new(),
        })
    }

    /// The effects the compositor supports, as of the last `capabilities` event.
    pub(crate) fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    /// Set (or, with `None`, clear) the blur region for `surface`, creating its
    /// `ext_background_effect_surface_v1` object on first use. Takes effect on the surface's
    /// next `wl_surface.commit`.
    pub(crate) fn set_blur_region<D>(
        &mut self,
        qh: &QueueHandle<D>,
        surface: &WlSurface,
        region: Option<&WlRegion>,
    ) where
        D: Dispatch<ExtBackgroundEffectSurfaceV1, ()> + 'static,
    {
        let manager = &self.manager;
        let effect = self
            .surfaces
            .entry(surface.id())
            .or_insert_with(|| manager.get_background_effect(surface, qh, ()));
        effect.set_blur_region(region);
    }
}

impl<D> Dispatch<ExtBackgroundEffectManagerV1, (), D> for BackgroundEffect
where
    D: Dispatch<ExtBackgroundEffectManagerV1, ()> + AsMut<BackgroundEffect> + 'static,
{
    fn event(
        state: &mut D,
        _manager: &ExtBackgroundEffectManagerV1,
        event: ext_background_effect_manager_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<D>,
    ) {
        let ext_background_effect_manager_v1::Event::Capabilities { flags } = event else {
            return;
        };
        let flags = match flags {
            WEnum::Value(flags) => flags,
            // An unknown bit is not blur; treat it as "nothing we recognise is supported".
            WEnum::Unknown(_) => ext_background_effect_manager_v1::Capability::empty(),
        };
        state.as_mut().capabilities = Capabilities::from_wire(flags);
    }
}

impl<D> Dispatch<ExtBackgroundEffectSurfaceV1, (), D> for BackgroundEffect
where
    D: Dispatch<ExtBackgroundEffectSurfaceV1, ()> + 'static,
{
    fn event(
        _state: &mut D,
        _surface: &ExtBackgroundEffectSurfaceV1,
        _event: ext_background_effect_surface_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<D>,
    ) {
        unreachable!("ext_background_effect_surface_v1 has no events in v1")
    }
}

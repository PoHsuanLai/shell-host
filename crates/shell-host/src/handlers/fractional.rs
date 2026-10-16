//! `wp_fractional_scale_v1` and `wp_viewporter`: the preferred-scale event and the viewport
//! request that go with it. sctk 0.20 has no helper for either (`image_viewporter.rs` binds
//! `WpViewporter` by hand with `SimpleGlobal`, and does not touch fractional-scale at all), so
//! both are hand-written here, `Dispatch` shaped like `background_effect.rs`.

use std::collections::HashMap;

use wayland_client::backend::ObjectId;
use wayland_client::globals::{BindError, GlobalList};
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_manager_v1::{
    self, WpFractionalScaleManagerV1,
};
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_v1::{
    self, WpFractionalScaleV1,
};
use wayland_protocols::wp::viewporter::client::wp_viewport::{self, WpViewport};
use wayland_protocols::wp::viewporter::client::wp_viewporter::{self, WpViewporter};

use crate::scale::{Scale, Size};

/// State for `wp_fractional_scale_manager_v1` and `wp_viewporter`: the two bound managers, and
/// the per-surface objects and last-reported scale created against them.
#[derive(Debug)]
pub(crate) struct Fractional {
    scale_manager: WpFractionalScaleManagerV1,
    viewporter: WpViewporter,
    preferred: HashMap<ObjectId, Scale>,
    scale_of: HashMap<ObjectId, WpFractionalScaleV1>,
    viewport_of: HashMap<ObjectId, WpViewport>,
}

impl Fractional {
    /// Bind `wp_fractional_scale_manager_v1` and `wp_viewporter`.
    pub(crate) fn bind<D>(
        globals: &GlobalList,
        qh: &QueueHandle<D>,
    ) -> Result<Fractional, BindError>
    where
        D: Dispatch<WpFractionalScaleManagerV1, ()> + Dispatch<WpViewporter, ()> + 'static,
    {
        let scale_manager = globals.bind(qh, 1..=1, ())?;
        let viewporter = globals.bind(qh, 1..=1, ())?;
        Ok(Fractional {
            scale_manager,
            viewporter,
            preferred: HashMap::new(),
            scale_of: HashMap::new(),
            viewport_of: HashMap::new(),
        })
    }

    /// Start (if not already started) reporting the preferred scale for `surface`, and ensure
    /// it has a `wp_viewport`. Call once per surface, before the first `set_destination`.
    pub(crate) fn watch<D>(&mut self, qh: &QueueHandle<D>, surface: &WlSurface)
    where
        D: Dispatch<WpFractionalScaleV1, ()> + Dispatch<WpViewport, ()> + 'static,
    {
        let id = surface.id();
        let scale_manager = &self.scale_manager;
        self.scale_of
            .entry(id.clone())
            .or_insert_with(|| scale_manager.get_fractional_scale(surface, qh, ()));
        let viewporter = &self.viewporter;
        self.viewport_of
            .entry(id)
            .or_insert_with(|| viewporter.get_viewport(surface, qh, ()));
    }

    /// Destroy the fractional-scale and viewport objects made for `surface` and forget its
    /// preferred scale. Call before the surface is destroyed; a later surface that is given the
    /// same object id is watched afresh.
    pub(crate) fn forget(&mut self, surface: &WlSurface) {
        let id = surface.id();
        self.preferred.remove(&id);
        if let Some(scale) = self.scale_of.remove(&id) {
            scale.destroy();
        }
        if let Some(viewport) = self.viewport_of.remove(&id) {
            viewport.destroy();
        }
    }

    /// The last `preferred_scale` reported for `surface`, or `None` before the first event.
    pub(crate) fn preferred_scale(&self, surface: &WlSurface) -> Option<Scale> {
        self.preferred.get(&surface.id()).copied()
    }

    /// Set the viewport's destination size (the surface-local logical size the buffer, whatever
    /// its own pixel size, should be scaled to fill). Nothing is sent for a size without an area
    /// (the protocol ends the connection on it) or for a surface `watch` never saw.
    pub(crate) fn set_destination(&self, surface: &WlSurface, size: Size) {
        if !size.has_area() {
            return;
        }
        if let Some(viewport) = self.viewport_of.get(&surface.id()) {
            viewport.set_destination(size.width, size.height);
        }
    }
}

impl<D> Dispatch<WpFractionalScaleManagerV1, (), D> for Fractional
where
    D: Dispatch<WpFractionalScaleManagerV1, ()> + 'static,
{
    fn event(
        _state: &mut D,
        _manager: &WpFractionalScaleManagerV1,
        _event: wp_fractional_scale_manager_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<D>,
    ) {
        unreachable!("wp_fractional_scale_manager_v1 has no events")
    }
}

impl<D> Dispatch<WpFractionalScaleV1, (), D> for Fractional
where
    D: Dispatch<WpFractionalScaleV1, ()> + AsMut<Fractional> + 'static,
{
    fn event(
        state: &mut D,
        proxy: &WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<D>,
    ) {
        let wp_fractional_scale_v1::Event::PreferredScale { scale } = event else {
            return;
        };
        let fractional = state.as_mut();
        // The surface this fractional-scale object belongs to is whichever surface's entry in
        // `scale_of` holds this exact proxy; a `HashMap<ObjectId, _>` keyed on the surface, not
        // on the fractional-scale object, so the lookup is a short linear scan over the (few)
        // watched surfaces rather than a second map.
        let surface_id = fractional
            .scale_of
            .iter()
            .find(|(_, v)| v.id() == proxy.id())
            .map(|(k, _)| k.clone());
        if let Some(surface_id) = surface_id {
            fractional
                .preferred
                .insert(surface_id, Scale::from_wire(scale));
        }
    }
}

impl<D> Dispatch<WpViewporter, (), D> for Fractional
where
    D: Dispatch<WpViewporter, ()> + 'static,
{
    fn event(
        _state: &mut D,
        _viewporter: &WpViewporter,
        _event: wp_viewporter::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<D>,
    ) {
        unreachable!("wp_viewporter has no events")
    }
}

impl<D> Dispatch<WpViewport, (), D> for Fractional
where
    D: Dispatch<WpViewport, ()> + 'static,
{
    fn event(
        _state: &mut D,
        _viewport: &WpViewport,
        _event: wp_viewport::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<D>,
    ) {
        unreachable!("wp_viewport has no events in v1")
    }
}

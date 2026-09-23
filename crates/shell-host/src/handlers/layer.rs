//! Layer-surface configure and close.

use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{
    LayerShellHandler, LayerSurface, LayerSurfaceConfigure,
};
use wayland_client::{Connection, QueueHandle};

use crate::event::CloseReason;
use crate::frame::FrameInput;
use crate::handlers::{close_surface_and_popups, surface_id_by_wl};
use crate::id::SurfaceId;
use crate::layer::{Extent, LayerSize};
use crate::region::Px;
use crate::role::Role;
use crate::scale::{self, Scale, Size};
use crate::state::HostState;
use crate::surface::Configured;
use crate::timers::TimerRequest;

impl LayerShellHandler for HostState {
    /// `HostEvent::SurfaceClosed{reason: Compositor}`, closing its popups first.
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, layer: &LayerSurface) {
        let Some(id) = surface_id_by_wl(self, layer.wl_surface()) else {
            return;
        };
        close_surface_and_popups(self, id, CloseReason::Compositor);
    }

    /// Record the size (0 on an axis keeps the requested size, F13), mark configured, resize
    /// the presenter, invalidate (the render that follows sends the regions).
    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        let Some(id) = surface_id_by_wl(self, layer.wl_surface()) else {
            return;
        };

        let Some((logical, surface_scale)) = record_configure(self, id, &configure) else {
            return;
        };
        let buffer = scale::buffer_size(logical, surface_scale);

        match &self.fractional {
            Some(fractional) if fractional.preferred_scale(layer.wl_surface()).is_some() => {
                fractional.set_destination(layer.wl_surface(), logical.width, logical.height);
            }
            _ => {
                let integer_scale = surface_scale.as_f64().round().max(1.0) as i32;
                layer.wl_surface().set_buffer_scale(integer_scale);
            }
        }

        if let Some(host_surface) = self.surfaces.get_mut(&id)
            && let Some(presenter) = &mut host_surface.presenter
        {
            presenter.resize(self.gpu.as_ref(), buffer);
        }

        // Regions are not sent here: the invalidate below renders, and the render resolves
        // them after layout and sends whichever changed (`ctx::paint::send_regions`).
        if let Some(host_surface) = self.surfaces.get_mut(&id) {
            let action = host_surface.step(FrameInput::Invalidate);
            self.timers.push(TimerRequest::Watchdog {
                id,
                cmd: action.timer,
            });
        }
    }
}

/// Records the configured logical size on `id`'s surface and marks it configured; returns its
/// logical size and scale (for the caller's buffer/viewport arithmetic), or `None` if `id` no
/// longer names a live layer surface.
fn record_configure(
    state: &mut HostState,
    id: SurfaceId,
    configure: &LayerSurfaceConfigure,
) -> Option<(Size, Scale)> {
    let host_surface = state.surfaces.get_mut(&id)?;
    let Role::Layer { config, .. } = &host_surface.role else {
        return None;
    };
    let logical = configured_size(config.size, configure.new_size);
    host_surface.configure(logical);
    host_surface.configured = Configured::Done;
    Some((logical, host_surface.scale))
}

/// The logical size to record from a `zwlr_layer_surface_v1.configure`: on each axis, the
/// compositor's value when it sent one, or (F13) what `requested` asked for on that axis when it
/// sent 0 — the wire's "you decide" for a fixed-size axis (`Extent::Px`). A stretching axis
/// (`Extent::Fill`) has no independent size to fall back to; a 0 there means the compositor has
/// not resolved the stretch yet (a compositor should not normally send this for an anchored,
/// opposite-edges-stretching axis, but nothing about the wire forbids it).
pub fn configured_size(requested: LayerSize, wire: (u32, u32)) -> Size {
    let axis = |extent: Extent, got: u32| -> Px {
        if got != 0 {
            return got as Px;
        }
        match extent {
            Extent::Px(px) => px as Px,
            Extent::Fill => 0,
        }
    };
    Size {
        width: axis(requested.width, wire.0),
        height: axis(requested.height, wire.1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_size_matches_the_table() {
        let bar = LayerSize {
            width: Extent::Fill,
            height: Extent::Px(32),
        };
        let dock = LayerSize {
            width: Extent::Px(740),
            height: Extent::Px(96),
        };
        let cases: &[(&str, LayerSize, (u32, u32), Size)] = &[
            (
                "compositor echoes the requested size",
                dock,
                (740, 96),
                Size {
                    width: 740,
                    height: 96,
                },
            ),
            (
                "fixed axis falls back to what was requested when the compositor sends 0",
                dock,
                (0, 0),
                Size {
                    width: 740,
                    height: 96,
                },
            ),
            (
                "compositor resolves the stretching axis",
                bar,
                (1280, 0),
                Size {
                    width: 1280,
                    height: 32,
                },
            ),
            (
                "a fixed axis is still honoured next to a stretching one",
                bar,
                (1280, 40),
                Size {
                    width: 1280,
                    height: 40,
                },
            ),
            (
                "an unresolved stretch reports 0, not the other axis's fixed size",
                bar,
                (0, 0),
                Size {
                    width: 0,
                    height: 32,
                },
            ),
        ];
        for (name, requested, wire, want) in cases {
            assert_eq!(configured_size(*requested, *wire), *want, "{name}");
        }
    }
}

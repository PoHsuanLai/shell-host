//! Toplevel and popup configure, close and dismissal.

use std::num::NonZeroU32;

use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::xdg::XdgSurface;
use smithay_client_toolkit::shell::xdg::popup::{Popup, PopupConfigure, PopupHandler};
use smithay_client_toolkit::shell::xdg::window::{Window, WindowConfigure, WindowHandler};
use wayland_client::{Connection, QueueHandle};

use crate::csd::{inset_for, surface_of, window_geometry};
use crate::event::CloseReason;
use crate::frame::FrameInput;
use crate::handlers::{close_surface_and_popups, surface_id_by_wl};
use crate::id::SurfaceId;
use crate::layer::{KeyboardMode, to_wire_keyboard};
use crate::popup::Grab;
use crate::region::Px;
use crate::role::Role;
use crate::scale::{self, Size};
use crate::state::HostState;
use crate::surface::Configured;
use crate::timers::TimerRequest;
use crate::toplevel::{Decorations, state_of};

impl WindowHandler for HostState {
    /// `HostEvent::SurfaceClosed{reason: UserClose}`.
    fn request_close(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, window: &Window) {
        let Some(id) = surface_id_by_wl(self, window.wl_surface()) else {
            return;
        };
        close_surface_and_popups(self, id, CloseReason::UserClose);
    }

    /// Size, state (published; `activated` drives the first-mouse filter), resize band and
    /// window geometry, invalidate.
    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        window: &Window,
        configure: WindowConfigure,
        _serial: u32,
    ) {
        let Some(id) = surface_id_by_wl(self, window.wl_surface()) else {
            return;
        };
        let Some((logical, surface_scale)) = record_toplevel_configure(self, id, &configure) else {
            return;
        };
        let buffer = scale::buffer_size(logical, surface_scale);

        match &self.fractional {
            Some(fractional) if fractional.preferred_scale(window.wl_surface()).is_some() => {
                fractional.set_destination(window.wl_surface(), logical.width, logical.height);
            }
            _ => {
                let integer_scale = surface_scale.as_f64().round().max(1.0) as i32;
                window.wl_surface().set_buffer_scale(integer_scale);
            }
        }

        if let Some(host_surface) = self.surfaces.get_mut(&id) {
            if let Some(presenter) = &mut host_surface.presenter {
                presenter.resize(self.gpu.as_ref(), buffer);
            }
            let action = host_surface.step(FrameInput::Invalidate);
            self.timers.push(TimerRequest::Watchdog {
                id,
                cmd: action.timer,
            });
        }
    }
}

impl PopupHandler for HostState {
    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        popup: &Popup,
        config: PopupConfigure,
    ) {
        let Some(id) = surface_id_by_wl(self, popup.wl_surface()) else {
            return;
        };
        let Some((logical, surface_scale)) = record_popup_configure(self, id, &config) else {
            return;
        };
        let buffer = scale::buffer_size(logical, surface_scale);

        match &self.fractional {
            Some(fractional) if fractional.preferred_scale(popup.wl_surface()).is_some() => {
                fractional.set_destination(popup.wl_surface(), logical.width, logical.height);
            }
            _ => {
                let integer_scale = surface_scale.as_f64().round().max(1.0) as i32;
                popup.wl_surface().set_buffer_scale(integer_scale);
            }
        }

        if let Some(host_surface) = self.surfaces.get_mut(&id) {
            if let Some(presenter) = &mut host_surface.presenter {
                presenter.resize(self.gpu.as_ref(), buffer);
            }
            let action = host_surface.step(FrameInput::Invalidate);
            self.timers.push(TimerRequest::Watchdog {
                id,
                cmd: action.timer,
            });
        }
    }

    /// `xdg_popup.popup_done`: `SurfaceClosed{reason: PopupDismissed}`, and restore the parent
    /// layer's keyboard mode. A client cannot trigger this itself, so the scripted
    /// demos dismiss by hit-testing and closing instead.
    fn done(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, popup: &Popup) {
        let Some(id) = surface_id_by_wl(self, popup.wl_surface()) else {
            return;
        };
        let parent = self.surfaces.get(&id).and_then(|s| s.role.parent());
        close_surface_and_popups(self, id, CloseReason::PopupDismissed);
        if let Some(parent_id) = parent {
            restore_keyboard_mode_if_idle(self, parent_id);
        }
    }
}

/// Records a toplevel configure on `id`: its state (published to the document), the resize
/// band that state allows, the frame size (the configure's, or the current frame on an axis it
/// leaves to us), the surface around it, and the window geometry that tells the compositor
/// where the frame is inside the surface (`csd`). Returns the surface's logical size and scale
/// for the caller's buffer/viewport arithmetic, or `None` if `id` no longer names a live
/// toplevel.
fn record_toplevel_configure(
    state: &mut HostState,
    id: SurfaceId,
    configure: &WindowConfigure,
) -> Option<(Size, scale::Scale)> {
    let host_surface = state.surfaces.get_mut(&id)?;
    let frame_now = host_surface.document_size();
    let Role::Toplevel {
        window,
        config,
        state: current,
    } = &mut host_surface.role
    else {
        return None;
    };
    let next = state_of(configure);
    let inset = inset_for(config.decorations, next);
    let frame = toplevel_configured_size(frame_now, configure.new_size);
    let surface = surface_of(frame, inset);
    if let Decorations::Client(_) = config.decorations {
        let geometry = window_geometry(surface, inset);
        let px = |v: Px| u32::try_from(v).unwrap_or(0);
        window.set_window_geometry(
            px(geometry.x),
            px(geometry.y),
            px(geometry.width),
            px(geometry.height),
        );
    }
    *current = next;
    host_surface.inset = inset;
    host_surface.configure(surface);
    host_surface.configured = Configured::Done;
    host_surface.live_toplevel.publish(next);
    Some((surface, host_surface.scale))
}

/// The frame size to record from an `xdg_toplevel.configure`: the compositor's value on an
/// axis it constrains, or `current` (the last-known frame, initially `ToplevelConfig::size`)
/// on an axis it leaves to the client (`new_size`'s `None`). Configure sizes are window
/// geometry, so they never include the resize band.
fn toplevel_configured_size(current: Size, wire: (Option<NonZeroU32>, Option<NonZeroU32>)) -> Size {
    let axis =
        |got: Option<NonZeroU32>, fallback: Px| -> Px { got.map_or(fallback, |v| v.get() as Px) };
    Size {
        width: axis(wire.0, current.width),
        height: axis(wire.1, current.height),
    }
}

/// Records the configured logical size on `id`'s popup and marks it configured. Returns the
/// logical size and scale for the caller's buffer/viewport arithmetic, or `None` if `id` no
/// longer names a live popup. `xdg_popup.configure` always carries a definite size (unlike a
/// layer surface or a toplevel), so there is no "compositor leaves it to us" case to fall back
/// from; a non-positive width or height (which the protocol does not forbid outright) is
/// clamped to at least one pixel so a degenerate configure never leaves the surface unrenderable.
fn record_popup_configure(
    state: &mut HostState,
    id: SurfaceId,
    config: &PopupConfigure,
) -> Option<(Size, scale::Scale)> {
    let host_surface = state.surfaces.get_mut(&id)?;
    if !matches!(host_surface.role, Role::Popup { .. }) {
        return None;
    }
    let logical = Size {
        width: config.width.max(1),
        height: config.height.max(1),
    };
    host_surface.configure(logical);
    host_surface.configured = Configured::Done;
    Some((logical, host_surface.scale))
}

/// After a popup closes, drop its parent layer back to `KeyboardMode::None` if that is what the
/// layer was spawned with and no other grabbing popup of that parent is still open (plan: opening
/// a grabbing popup from a `KeyboardMode::None` layer switches it to `OnDemand` until close).
fn restore_keyboard_mode_if_idle(state: &mut HostState, layer_id: SurfaceId) {
    let still_grabbing = state.surfaces.values().any(|s| {
        matches!(
            &s.role,
            Role::Popup { parent, config, .. }
                if *parent == layer_id && config.grab == Grab::Grabbed
        )
    });
    if still_grabbing {
        return;
    }
    let Some(host_surface) = state.surfaces.get_mut(&layer_id) else {
        return;
    };
    let Role::Layer {
        surface,
        config,
        keyboard,
    } = &mut host_surface.role
    else {
        return;
    };
    if config.keyboard == KeyboardMode::None && *keyboard != KeyboardMode::None {
        *keyboard = KeyboardMode::None;
        surface.set_keyboard_interactivity(to_wire_keyboard(KeyboardMode::None));
        // Layer state is double-buffered: without a commit the bar stayed `OnDemand` until its
        // next repaint.
        surface.commit();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toplevel_configured_size_falls_back_per_axis() {
        let current = Size {
            width: 640,
            height: 480,
        };
        type Wire = (Option<NonZeroU32>, Option<NonZeroU32>);
        let nz = |v: u32| NonZeroU32::new(v);
        let cases: &[(&str, Wire, Size)] = &[
            (
                "both axes left to the client keep the current size",
                (None, None),
                current,
            ),
            (
                "the compositor constrains both axes",
                (nz(800), nz(600)),
                Size {
                    width: 800,
                    height: 600,
                },
            ),
            (
                "only one axis is constrained",
                (nz(800), None),
                Size {
                    width: 800,
                    height: 480,
                },
            ),
        ];
        for (name, wire, want) in cases {
            assert_eq!(toplevel_configured_size(current, *wire), *want, "{name}");
        }
    }
}

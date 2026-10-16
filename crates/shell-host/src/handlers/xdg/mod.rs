//! Toplevel and popup configure, close and dismissal.
//!
//! See FINDINGS F77.

mod decide;

use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::xdg::XdgSurface;
use smithay_client_toolkit::shell::xdg::popup::{Popup, PopupConfigure, PopupHandler};
use smithay_client_toolkit::shell::xdg::window::{Window, WindowConfigure, WindowHandler};
use wayland_client::{Connection, QueueHandle};

use crate::event::CloseReason;
use crate::frame::FrameInput;
use crate::handlers::{close_surface_and_popups, follow_buffer_scale, surface_id_by_wl};
use crate::id::SurfaceId;
use crate::layer::{KeyboardMode, to_wire_keyboard};
use crate::popup::PopupGrab;
use crate::region::Px;
use crate::role::Role;
use crate::scale::{self, Size};
use crate::state::HostState;
use crate::timers::TimerRequest;
use crate::toplevel::state_of;
use decide::{
    FrameGeometry, GrabbingPopups, KeyboardRestore, keyboard_restore, popup_size, toplevel_layout,
};

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
        follow_buffer_scale(self, window.wl_surface(), logical, surface_scale);

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
        follow_buffer_scale(self, popup.wl_surface(), logical, surface_scale);

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
    let layout = toplevel_layout(frame_now, config.decorations, next, configure.new_size);
    if let FrameGeometry::Set(geometry) = layout.geometry {
        let px = |v: Px| u32::try_from(v).unwrap_or(0);
        window.set_window_geometry(
            px(geometry.x),
            px(geometry.y),
            px(geometry.width),
            px(geometry.height),
        );
    }
    *current = next;
    host_surface.inset = layout.inset;
    host_surface.configure(layout.surface);
    host_surface.live_toplevel.publish(next);
    Some((host_surface.logical, host_surface.scale))
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
    host_surface.configure(popup_size(config.width, config.height));
    Some((host_surface.logical, host_surface.scale))
}

/// After a popup closes, drop its parent layer back to `KeyboardMode::None` if that is what the
/// layer was spawned with and no other grabbing popup of that parent is still open (plan: opening
/// a grabbing popup from a `KeyboardMode::None` layer switches it to `OnDemand` until close).
fn restore_keyboard_mode_if_idle(state: &mut HostState, layer_id: SurfaceId) {
    let grabs = state
        .surfaces
        .values()
        .find(|s| {
            matches!(
                &s.role,
                Role::Popup { parent, config, .. }
                    if *parent == layer_id && config.grab == PopupGrab::Grabbed
            )
        })
        .map_or(GrabbingPopups::None, |_| GrabbingPopups::Open);
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
    if keyboard_restore(config.keyboard, *keyboard, grabs) == KeyboardRestore::Restore {
        *keyboard = KeyboardMode::None;
        surface.set_keyboard_interactivity(to_wire_keyboard(KeyboardMode::None));
        // Layer state is double-buffered: without a commit the bar stayed `OnDemand` until its
        // next repaint.
        surface.commit();
    }
}

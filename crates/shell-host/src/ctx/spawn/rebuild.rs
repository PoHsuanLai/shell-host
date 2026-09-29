//! Rebuilding a hidden surface's role, and mapping a built one.

use smithay_client_toolkit::compositor::Surface;

use crate::error::{HostError, Result};
use crate::popup::Grab;
use crate::role::Role;
use crate::state::HostState;

use super::layer::layer_role;
use super::new_wl_surface;
use super::popup::popup_role;
use super::toplevel::toplevel_role;

/// A fresh, unmapped role with the same configuration as `old`, which is destroyed first.
pub(crate) fn rebuild(state: &mut HostState, old: Role) -> Result<Role> {
    match old {
        Role::Layer {
            surface, config, ..
        } => {
            drop(surface);
            layer_role(state, config)
        }
        Role::Toplevel { window, config, .. } => {
            drop(window);
            Ok(toplevel_role(state, config))
        }
        Role::Popup {
            popup,
            parent,
            config,
            placement,
        } => {
            drop(popup);
            popup_role(state, parent, config, placement)
        }
        // A new surface has no role: the icon stays blank until its drag ends and it goes.
        Role::DragIcon { surface, origin } => {
            drop(surface);
            Ok(Role::DragIcon {
                surface: Surface::from(new_wl_surface(state)),
                origin,
            })
        }
        // Never hidden (`ops::set_visibility`), so never rebuilt.
        Role::Lock { .. } => Err(HostError::Unsupported {
            global: "ext_session_lock_v1",
            feature: "hiding a lock surface",
        }),
    }
}

/// Map `role`: a popup's grab (which must precede its first commit), then the initial commit
/// the compositor answers with a configure.
pub(crate) fn map(state: &HostState, role: &Role) {
    if let Role::Popup { popup, config, .. } = role
        && config.grab == Grab::Grabbed
        && let Some(seat) = &state.input.seat
    {
        popup.xdg_popup().grab(seat, state.input.last_serial);
    }
    role.wl_surface().commit();
}

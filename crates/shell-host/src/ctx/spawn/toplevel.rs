//! Toplevel windows: the request sequence that builds one and its first state.

use crate::blur::BlurSpec;
use crate::csd::{inset_for, surface_of};
use crate::id::SurfaceId;
use crate::id::SurfaceName;
use crate::region::RegionSpec;
use crate::role::Role;
use crate::root::HostRoot;
use crate::scale::Scale;
use crate::state::HostState;
use crate::toplevel::{ToplevelConfig, ToplevelState, to_wire_decorations};

use super::rebuild::map;
use super::size::wire_size;
use super::{Initial, insert, new_wl_surface};

/// Create and map toplevel `id`. A client-decorated window starts floating, so its surface
/// starts with the resize band around `config.size`.
pub(crate) fn toplevel(
    state: &mut HostState,
    id: SurfaceId,
    config: ToplevelConfig,
    root: HostRoot,
) {
    let name = SurfaceName(config.app_id.clone());
    let inset = inset_for(config.decorations, ToplevelState::default());
    let at = Initial {
        logical: surface_of(config.size, inset),
        inset,
        scale: Scale::ONE,
    };
    let role = toplevel_role(state, config);
    map(state, &role);
    let regions = (BlurSpec::None, RegionSpec::Whole);
    insert(state, id, role, name, at, regions, root);
}

pub(super) fn toplevel_role(state: &mut HostState, config: ToplevelConfig) -> Role {
    let wl = new_wl_surface(state);
    let window =
        state
            .xdg_shell
            .create_window(wl, to_wire_decorations(config.decorations), &state.qh);
    window.set_title(config.title.clone());
    window.set_app_id(config.app_id.clone());
    window.set_min_size(config.min_size.map(wire_size));
    window.set_max_size(config.max_size.map(wire_size));
    Role::Toplevel {
        window,
        config,
        state: ToplevelState::default(),
    }
}

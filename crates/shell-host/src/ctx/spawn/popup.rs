//! Popups: the positioner, the role built on a parent surface, and the keyboard lift a
//! grabbing popup asks of a layer parent.

use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::xdg::popup::Popup;
use smithay_client_toolkit::shell::xdg::{XdgPositioner, XdgSurface};

use crate::blur::BlurSpec;
use crate::csd::Inset;
use crate::error::{HostError, Result};
use crate::id::SurfaceId;
use crate::id::SurfaceName;
use crate::layer::{KeyboardMode, to_wire_keyboard};
use crate::placement::PopupPlacement;
use crate::popup::{
    PopupAnchor, PopupConfig, PopupGrab, PositionerParams, positioner_params,
    to_wire_anchor as edge, to_wire_constraint_adjustment, to_wire_gravity,
};
use crate::popup_keyboard::{ParentKeyboard, parent_keyboard};
use crate::region::RegionSpec;
use crate::role::Role;
use crate::root::HostRoot;
use crate::scale::Scale;
use crate::state::HostState;
use crate::surface::HostSurface;

use super::rebuild::map;
use super::{Initial, insert};

/// Create and map popup `id` on `parent`. A grabbing popup lifts a `KeyboardMode::None` parent
/// layer to `OnDemand` until it closes, except on KWin (`popup_keyboard`).
pub(crate) fn popup(
    state: &mut HostState,
    parent: SurfaceId,
    id: SurfaceId,
    config: PopupConfig,
    placement: PopupPlacement,
    root: HostRoot,
) -> Result<()> {
    let parent_name = state
        .surfaces
        .get(&parent)
        .map(|surface| surface.name.0.clone())
        .ok_or(HostError::NoSurface(parent))?;
    let grab = config.grab;
    // A popup opens on its parent's output nearly always, so the parent's scale is the best
    // first guess: the popup's first render already reads it (`use_surface_scale`), and the
    // compositor's own preferred scale corrects it if it differs.
    let initial = state
        .surfaces
        .get(&parent)
        .map_or(Scale::ONE, |surface| surface.scale);
    let role = popup_role(state, parent, config, placement)?;
    lift_keyboard(state, parent, grab);
    map(state, &role);
    let name = SurfaceName(format!("{parent_name}/popup"));
    let regions = (BlurSpec::None, RegionSpec::Whole);
    let at = Initial {
        logical: placement.size,
        inset: Inset::NONE,
        scale: initial,
    };
    insert(state, id, role, name, at, regions, root);
    Ok(())
}

pub(super) fn popup_role(
    state: &mut HostState,
    parent: SurfaceId,
    config: PopupConfig,
    placement: PopupPlacement,
) -> Result<Role> {
    let parent_surface = state
        .surfaces
        .get(&parent)
        .ok_or(HostError::NoSurface(parent))?;
    let anchor_rect = match &config.anchor {
        PopupAnchor::Rect => placement.anchor_rect,
        PopupAnchor::Element(element) => parent_surface
            .document
            .element_rect(ToOwned::to_owned(element))
            .unwrap_or(placement.anchor_rect),
    };
    let positioner = positioner(
        state,
        &positioner_params(&config, anchor_rect, placement.size),
    )?;
    let wl = state.compositor.create_surface(&state.qh);
    if let Some(fractional) = state.fractional.as_mut() {
        fractional.watch(&state.qh, &wl);
    }
    let parent_xdg = match &parent_surface.role {
        Role::Layer { .. } => None,
        Role::Popup { popup, .. } => Some(popup.xdg_surface()),
        Role::Toplevel { window, .. } => Some(window.xdg_surface()),
        Role::DragIcon { .. } => {
            return Err(HostError::Unsupported {
                global: "xdg_popup",
                feature: "a popup on a drag icon",
            });
        }
        Role::Lock { .. } => {
            return Err(HostError::Unsupported {
                global: "xdg_popup",
                feature: "a popup on a lock surface",
            });
        }
    };
    let popup = Popup::from_surface(parent_xdg, &positioner, &state.qh, wl, &state.xdg_shell)
        .map_err(|_| HostError::MissingGlobal {
            global: "xdg_wm_base",
        })?;
    if let Role::Layer { surface, .. } = &parent_surface.role {
        surface.get_popup(popup.xdg_popup());
    }
    Ok(Role::Popup {
        popup,
        parent,
        config,
        placement,
    })
}

/// An `xdg_positioner` set up with `params`. Sizes are clamped to 1 px, since a zero size is a
/// protocol error.
pub(crate) fn positioner(state: &HostState, params: &PositionerParams) -> Result<XdgPositioner> {
    let positioner =
        XdgPositioner::new(&state.xdg_shell).map_err(|_| HostError::MissingGlobal {
            global: "xdg_wm_base",
        })?;
    let rect = params.anchor_rect;
    positioner.set_size(params.size.width.max(1), params.size.height.max(1));
    positioner.set_anchor_rect(rect.x, rect.y, rect.width.max(1), rect.height.max(1));
    positioner.set_anchor(edge(params.anchor));
    positioner.set_gravity(to_wire_gravity(params.gravity));
    positioner.set_constraint_adjustment(to_wire_constraint_adjustment(params.adjust));
    Ok(positioner)
}

/// Switch a `KeyboardMode::None` layer to `OnDemand`, so the grabbing popup opened from it can
/// take keyboard focus (plan "Opening a grabbing popup ..."), where `parent_keyboard` says so.
fn lift_keyboard(state: &mut HostState, parent: SurfaceId, grab: PopupGrab) {
    let kind = state.compositor_kind;
    if let Some(HostSurface {
        role: Role::Layer {
            surface, keyboard, ..
        },
        ..
    }) = state.surfaces.get_mut(&parent)
        && parent_keyboard(kind, grab, *keyboard) == ParentKeyboard::Lift
    {
        *keyboard = KeyboardMode::OnDemand;
        surface.set_keyboard_interactivity(to_wire_keyboard(KeyboardMode::OnDemand));
        surface.commit();
    }
}

//! Creating surfaces: the role (layer surface, xdg toplevel, xdg popup) with its initial state,
//! the document with its `SurfaceHandle`, and the `HostSurface` that ties them together.
//!
//! A role is built unmapped (every request made, no commit) and mapped separately, because a
//! hidden surface keeps an unmapped role and maps it again when shown.

use std::sync::Arc;

use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::xdg::popup::Popup;
use smithay_client_toolkit::shell::xdg::{XdgPositioner, XdgSurface};
use wayland_client::protocol::wl_surface::WlSurface;

use crate::blur::BlurSpec;
use crate::debug::{StatsWindow, SurfaceName};
use crate::dom::document::{DocumentEnv, SurfaceDocument};
use crate::dom::net::DataUriNet;
use crate::dom::shell_provider::HostShellProvider;
use crate::error::{HostError, Result};
use crate::frame::FrameState;
use crate::handle::SurfaceHandle;
use crate::id::{OutputId, SurfaceId};
use crate::layer::{
    Extent, KeyboardMode, LayerConfig, OutputTarget, to_wire_anchor, to_wire_exclusive,
    to_wire_keyboard, to_wire_layer, to_wire_size,
};
use crate::msg::Visibility;
use crate::placement::PopupPlacement;
use crate::popup::{
    Grab, PopupAnchor, PopupConfig, PositionerParams, positioner_params, to_wire_anchor as edge,
    to_wire_constraint_adjustment, to_wire_gravity,
};
use crate::region::{Px, RegionSpec};
use crate::role::Role;
use crate::root::Root;
use crate::scale::{Scale, Size};
use crate::state::HostState;
use crate::surface::{AppliedRegions, Configured, HostSurface};
use crate::toplevel::{Activation, ToplevelConfig, to_wire_decorations};

/// Create and map layer surface `id`.
pub(super) fn layer(
    state: &mut HostState,
    id: SurfaceId,
    config: LayerConfig,
    root: Root,
) -> Result<()> {
    let name = SurfaceName(config.namespace.clone());
    let requested = requested_size(&config);
    let (blur, input) = (config.blur.clone(), config.input.clone());
    let role = layer_role(state, config)?;
    map(state, &role);
    insert(state, id, role, name, requested, (blur, input), root);
    Ok(())
}

/// Create and map toplevel `id`.
pub(super) fn toplevel(state: &mut HostState, id: SurfaceId, config: ToplevelConfig, root: Root) {
    let name = SurfaceName(config.app_id.clone());
    let size = config.size;
    let role = toplevel_role(state, config);
    map(state, &role);
    let regions = (BlurSpec::None, RegionSpec::Whole);
    insert(state, id, role, name, size, regions, root);
}

/// Create and map popup `id` on `parent`. A grabbing popup lifts a `KeyboardMode::None` parent
/// layer to `OnDemand` until it closes.
pub(super) fn popup(
    state: &mut HostState,
    parent: SurfaceId,
    id: SurfaceId,
    config: PopupConfig,
    placement: PopupPlacement,
    root: Root,
) -> Result<()> {
    let parent_name = state
        .surfaces
        .get(&parent)
        .map(|surface| surface.name.0.clone())
        .ok_or(HostError::NoSurface(parent))?;
    let grab = config.grab;
    let role = popup_role(state, parent, config, placement)?;
    if grab == Grab::Grabbed {
        lift_keyboard(state, parent);
    }
    map(state, &role);
    let name = SurfaceName(format!("{parent_name}/popup"));
    let regions = (BlurSpec::None, RegionSpec::Whole);
    insert(state, id, role, name, placement.size, regions, root);
    Ok(())
}

/// A fresh, unmapped role with the same configuration as `old`, which is destroyed first.
pub(super) fn rebuild(state: &mut HostState, old: Role) -> Result<Role> {
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
    }
}

/// Map `role`: a popup's grab (which must precede its first commit), then the initial commit
/// the compositor answers with a configure.
pub(super) fn map(state: &HostState, role: &Role) {
    if let Role::Popup { popup, config, .. } = role
        && config.grab == Grab::Grabbed
        && let Some(seat) = &state.input.seat
    {
        popup.xdg_popup().grab(seat, state.input.last_serial);
    }
    role.wl_surface().commit();
}

fn layer_role(state: &mut HostState, config: LayerConfig) -> Result<Role> {
    let layer_shell = state.layer_shell.as_ref().ok_or(HostError::Unsupported {
        global: "zwlr_layer_shell_v1",
        feature: "layer surfaces",
    })?;
    let wl = state.compositor.create_surface(&state.qh);
    if let Some(fractional) = state.fractional.as_mut() {
        fractional.watch(&state.qh, &wl);
    }
    let output = match config.output {
        OutputTarget::CompositorPicks => None,
        OutputTarget::Output(OutputId(name)) => state.outputs.outputs().find(|output| {
            state
                .outputs
                .info(output)
                .is_some_and(|info| info.id == name)
        }),
    };
    let surface = layer_shell.create_layer_surface(
        &state.qh,
        wl,
        to_wire_layer(config.layer),
        Some(config.namespace.clone()),
        output.as_ref(),
    );
    let (width, height) = to_wire_size(config.size);
    let margin = config.margin;
    surface.set_anchor(to_wire_anchor(config.anchor));
    surface.set_size(width, height);
    surface.set_exclusive_zone(to_wire_exclusive(config.exclusive));
    surface.set_margin(margin.top, margin.right, margin.bottom, margin.left);
    surface.set_keyboard_interactivity(to_wire_keyboard(config.keyboard));
    Ok(Role::Layer {
        surface,
        keyboard: config.keyboard,
        config,
    })
}

fn toplevel_role(state: &mut HostState, config: ToplevelConfig) -> Role {
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
        activation: Activation::Inactive,
    }
}

fn popup_role(
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
pub(super) fn positioner(state: &HostState, params: &PositionerParams) -> Result<XdgPositioner> {
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
/// take keyboard focus (plan "Opening a grabbing popup ...").
fn lift_keyboard(state: &mut HostState, parent: SurfaceId) {
    if let Some(HostSurface {
        role: Role::Layer {
            surface, keyboard, ..
        },
        ..
    }) = state.surfaces.get_mut(&parent)
        && *keyboard == KeyboardMode::None
    {
        *keyboard = KeyboardMode::OnDemand;
        surface.set_keyboard_interactivity(to_wire_keyboard(KeyboardMode::OnDemand));
        surface.commit();
    }
}

fn new_wl_surface(state: &mut HostState) -> WlSurface {
    let wl = state.compositor.create_surface(&state.qh);
    if let Some(fractional) = state.fractional.as_mut() {
        fractional.watch(&state.qh, &wl);
    }
    wl
}

/// Build the document (with this surface's `SurfaceHandle` as a root context) and record the
/// surface. It is marked woken, so the first flush polls it.
fn insert(
    state: &mut HostState,
    id: SurfaceId,
    role: Role,
    name: SurfaceName,
    logical: Size,
    (blur, input): (BlurSpec, RegionSpec),
    root: Root,
) {
    let shell = Arc::new(HostShellProvider::new(id, state.requests.ping()));
    let handle = SurfaceHandle::new(id, state.requests.clone(), state.ids.clone());
    let env = DocumentEnv {
        shell: Arc::clone(&shell),
        fonts: state.fonts.for_document(),
        net: Arc::new(DataUriNet),
        viewport: super::paint::viewport(logical, Scale::ONE, state.color_scheme),
    };
    let document = SurfaceDocument::build(root.context(handle), env);
    state.surfaces.insert(
        id,
        HostSurface {
            presenter: None,
            role,
            document,
            shell,
            id,
            name,
            frame: FrameState::NEW,
            watchdog: None,
            configured: Configured::Pending,
            visibility: Visibility::Shown,
            logical,
            scale: Scale::ONE,
            blur,
            input,
            stats: StatsWindow::default(),
            geometry: None,
            regions: AppliedRegions::default(),
        },
    );
    state.woken.mark(id);
}

/// The size a layer asks for; a `Fill` axis is 0 until the compositor configures it.
fn requested_size(config: &LayerConfig) -> Size {
    let px = |extent| match extent {
        Extent::Fill => 0,
        Extent::Px(px) => Px::try_from(px).unwrap_or(Px::MAX),
    };
    Size {
        width: px(config.size.width),
        height: px(config.size.height),
    }
}

fn wire_size(size: Size) -> (u32, u32) {
    let px = |v: Px| u32::try_from(v).unwrap_or(0);
    (px(size.width), px(size.height))
}

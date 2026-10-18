//! Layer surfaces: the request sequence that builds one and the size it asks for.

use crate::csd::Inset;
use crate::error::{HostError, Result};
use crate::id::SurfaceName;
use crate::id::{OutputId, SurfaceId};
use crate::layer::{
    LayerConfig, OutputTarget, to_wire_anchor, to_wire_exclusive, to_wire_keyboard, to_wire_layer,
    to_wire_size,
};
use crate::role::Role;
use crate::root::HostRoot;
use crate::scale::Scale120;
use crate::state::HostState;

use super::rebuild::map;
use super::size::requested_size;
use super::{Initial, insert};

/// Create and map layer surface `id`.
pub(crate) fn layer(
    state: &mut HostState,
    id: SurfaceId,
    config: LayerConfig,
    root: HostRoot,
) -> Result<()> {
    let name = SurfaceName(config.namespace.clone());
    let requested = requested_size(&config);
    let (blur, input) = (config.blur.clone(), config.input.clone());
    let role = layer_role(state, config)?;
    map(state, &role);
    let at = Initial {
        logical: requested,
        inset: Inset::NONE,
        scale: Scale120::ONE,
    };
    insert(state, id, role, name, at, (blur, input), root);
    Ok(())
}

pub(super) fn layer_role(state: &mut HostState, config: LayerConfig) -> Result<Role> {
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

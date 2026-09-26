//! Applying what documents and other threads asked for: ops, popups, closes, clipboard and
//! activation requests, drags, and visibility changes.

use smithay_client_toolkit::shell::WaylandSurface;

use crate::ctx::{HostCtx, request_token};
use crate::event::{CloseReason, HostEvent};
use crate::frame::{FrameInput, FrameState};
use crate::handlers::data_device::claim_selection;
use crate::id::SurfaceId;
use crate::layer::{
    to_wire_anchor, to_wire_exclusive, to_wire_keyboard, to_wire_layer, to_wire_size,
};
use crate::msg::{Repaint, Request, SurfaceOp, Visibility};
use crate::placement::PopupPlacement;
use crate::popup::{Grab, positioner_params};
use crate::role::Role;
use crate::state::HostState;
use crate::surface::{AppliedRegions, Configured, VisibilityChange, visibility_change};

/// The `xdg_popup.reposition` token. The host never matches `repositioned` events to requests,
/// so one value serves.
const REPOSITION_TOKEN: u32 = 1;

/// Apply one queued request.
pub(super) fn request<M: 'static>(ctx: &mut HostCtx<M>, request: Request) {
    match request {
        Request::Apply { id, op } => apply(ctx, id, op),
        Request::OpenPopup {
            parent,
            id,
            config,
            placement,
            root,
        } => {
            let opened = super::spawn::popup(&mut ctx.state, parent, id, config, placement, root)
                .and_then(|()| super::paint::ensure_presenter(ctx, id));
            if let Err(error) = opened {
                // A half-made popup goes silently; the event below is its one close.
                ctx.state.surfaces.remove(&id);
                eprintln!("shell-host: popup {id:?} on {parent:?}: {error}");
                let reason = CloseReason::ParentClosed;
                ctx.state
                    .events
                    .push_back(HostEvent::SurfaceClosed { id, reason });
            }
        }
        Request::Close(id) => close(ctx, id, CloseReason::Requested),
        Request::Redraw(id) => ctx.state.frame_input(id, FrameInput::Invalidate),
        Request::CopyText { text, .. } => {
            let qh = ctx.state.qh.clone();
            claim_selection(&mut ctx.state, &qh, text);
        }
        Request::ActivationToken {
            from,
            app_id,
            reply,
        } => request_token(&mut ctx.state, from, app_id, reply),
        Request::StartDrag { from, spec, reply } => super::drag::start(ctx, from, spec, reply),
        Request::Scroll { id, cmd } => ctx.state.scroll_command(id, &cmd),
        Request::Unlock(lock) => super::lock::ask_unlock(&mut ctx.state, lock),
    }
}

/// Claim the selection with text a document copied through its shell provider (Blitz's own
/// copy in a text input), the same way as `SurfaceHandle::copy_text`.
pub(super) fn claim_copied<M: 'static>(ctx: &mut HostCtx<M>) {
    let copied: Vec<String> = ctx
        .state
        .surfaces
        .values()
        .filter_map(|surface| surface.shell.take_copied())
        .collect();
    let qh = ctx.state.qh.clone();
    for text in copied {
        claim_selection(&mut ctx.state, &qh, text);
    }
}

/// Apply `op` to surface `id`: its Wayland requests now, its recorded state for the next paint,
/// and an invalidation so that paint (and its commit) happens.
pub(super) fn apply<M: 'static>(ctx: &mut HostCtx<M>, id: SurfaceId, op: SurfaceOp) {
    if let SurfaceOp::SetVisibility(visibility) = op {
        set_visibility(ctx, id, visibility);
        return;
    }
    let state = &mut ctx.state;
    let Some(surface) = state.surfaces.get_mut(&id) else {
        return;
    };
    surface.record(&op);
    if surface.visibility == Visibility::Shown {
        send(state, id, &op);
    }
    if op.repaint() == Repaint::Needed {
        state.frame_input(id, FrameInput::Invalidate);
    }
}

/// Move popup `id` to `placement`, measured from its own content by `popup::refit`. Applied
/// inline from `ctx::paint::render`, right after the layout that produced the measurement,
/// rather than queued as a `Request`: the same wire request as `SurfaceOp::Reposition` (record,
/// then send), just not routed through the request queue, so a `FitContent` popup lands in the
/// same flush that measured it instead of waiting a flush for its own `Request::Apply` to drain.
pub(super) fn refit_popup(state: &mut HostState, id: SurfaceId, placement: PopupPlacement) {
    let Some(surface) = state.surfaces.get_mut(&id) else {
        return;
    };
    let op = SurfaceOp::Reposition(placement);
    surface.record(&op);
    send(state, id, &op);
}

/// The Wayland requests for `op` on surface `id`. Region ops have none here: regions are
/// resolved and sent right before the paint that shows them.
fn send(state: &HostState, id: SurfaceId, op: &SurfaceOp) {
    let Some(surface) = state.surfaces.get(&id) else {
        return;
    };
    match (&surface.role, op) {
        (Role::Layer { surface, .. }, op) => send_layer(surface, op),
        (
            Role::Popup {
                popup,
                config,
                placement,
                ..
            },
            SurfaceOp::Reposition(_),
        ) => {
            let params = positioner_params(config, placement.anchor_rect, placement.size);
            if let Ok(positioner) = super::spawn::positioner(state, &params) {
                popup.reposition(&positioner, REPOSITION_TOKEN);
            }
        }
        (Role::Toplevel { window, .. }, op) => super::window::send(state, id, window, op),
        (Role::Popup { .. } | Role::DragIcon { .. } | Role::Lock { .. }, _) => {}
    }
}

fn send_layer(layer: &smithay_client_toolkit::shell::wlr_layer::LayerSurface, op: &SurfaceOp) {
    match op {
        SurfaceOp::SetKeyboard(mode) => layer.set_keyboard_interactivity(to_wire_keyboard(*mode)),
        SurfaceOp::SetExclusiveZone(zone) => layer.set_exclusive_zone(to_wire_exclusive(*zone)),
        SurfaceOp::SetSize(size) => {
            let (width, height) = to_wire_size(*size);
            layer.set_size(width, height);
        }
        SurfaceOp::SetMargin(m) => layer.set_margin(m.top, m.right, m.bottom, m.left),
        SurfaceOp::SetAnchor(anchor) => layer.set_anchor(to_wire_anchor(*anchor)),
        SurfaceOp::SetLayer(level) => layer.set_layer(to_wire_layer(*level)),
        SurfaceOp::SetBlur(_)
        | SurfaceOp::SetInputRegion(_)
        | SurfaceOp::SetVisibility(_)
        | SurfaceOp::Reposition(_)
        | SurfaceOp::BeginMove
        | SurfaceOp::BeginResize(_)
        | SurfaceOp::SetMaximized(_)
        | SurfaceOp::Minimize
        | SurfaceOp::SetFullscreen(_) => {}
    }
}

/// Hide or show surface `id` (`surface::visibility_change`). Hiding closes its popups.
/// A lock surface is never hidden: its output would fall back to the compositor's solid colour,
/// and a re-made lock surface may not be committed before its configure.
pub(super) fn set_visibility<M: 'static>(ctx: &mut HostCtx<M>, id: SurfaceId, to: Visibility) {
    let Some(from) = ctx
        .state
        .surfaces
        .get(&id)
        .filter(|surface| !matches!(surface.role, Role::Lock { .. }))
        .map(|surface| surface.visibility)
    else {
        return;
    };
    match visibility_change(from, to) {
        VisibilityChange::Unchanged => {}
        VisibilityChange::Unmap => unmap(ctx, id),
        VisibilityChange::Map => {
            let state = &mut ctx.state;
            if let Some(surface) = state.surfaces.get_mut(&id) {
                surface.visibility = Visibility::Shown;
            }
            if let Some(surface) = state.surfaces.get(&id) {
                super::spawn::map(state, &surface.role);
            }
            if let Err(error) = super::paint::ensure_presenter(ctx, id) {
                eprintln!("shell-host: showing {id:?}: {error}");
                close(ctx, id, CloseReason::Failed);
            }
        }
    }
}

fn unmap<M: 'static>(ctx: &mut HostCtx<M>, id: SurfaceId) {
    close_children(ctx, id);
    // Before the rebuild, while the old wl_surface is still this id's (FINDINGS F58, G210).
    ctx.state.withdraw_pointer(id);
    ctx.state.withdraw_keyboard(id);
    super::timer::cancel_watchdog(ctx, id);
    let state = &mut ctx.state;
    let Some(mut surface) = state.surfaces.remove(&id) else {
        return;
    };
    // The wgpu surface goes before the wl_surface it presents into (S1 finding 6).
    surface.presenter = None;
    let old = surface.role;
    match super::spawn::rebuild(state, old) {
        Ok(role) => {
            surface.role = role;
            surface.visibility = Visibility::Hidden;
            surface.configured = Configured::Pending;
            surface.frame = FrameState::NEW;
            surface.geometry = None;
            surface.regions = AppliedRegions::default();
            state.surfaces.insert(id, surface);
        }
        Err(error) => {
            eprintln!("shell-host: hiding {id:?}: {error}");
            let reason = CloseReason::Failed;
            state
                .events
                .push_back(HostEvent::SurfaceClosed { id, reason });
        }
    }
}

/// Close surface `id` after its popups (which close with `ParentClosed`), and report it with
/// `reason`. Unknown ids are ignored.
pub(super) fn close<M: 'static>(ctx: &mut HostCtx<M>, id: SurfaceId, reason: CloseReason) {
    close_children(ctx, id);
    // As for an unmap: the compositor's leave will name a destroyed wl_surface (FINDINGS F58, G210).
    ctx.state.withdraw_pointer(id);
    ctx.state.withdraw_keyboard(id);
    super::timer::cancel_watchdog(ctx, id);
    let state = &mut ctx.state;
    let Some(surface) = state.surfaces.remove(&id) else {
        return;
    };
    let lifted = match &surface.role {
        Role::Popup { parent, config, .. } if config.grab == Grab::Grabbed => Some(*parent),
        Role::Popup { .. }
        | Role::Layer { .. }
        | Role::Toplevel { .. }
        | Role::DragIcon { .. }
        | Role::Lock { .. } => None,
    };
    drop(surface);
    if let Some(parent) = lifted {
        restore_keyboard(state, parent);
    }
    state
        .events
        .push_back(HostEvent::SurfaceClosed { id, reason });
}

/// Close every surface, children first.
pub(super) fn close_all<M: 'static>(ctx: &mut HostCtx<M>) {
    let roots: Vec<SurfaceId> = ctx
        .state
        .surfaces
        .values()
        .filter(|surface| surface.role.parent().is_none())
        .map(|surface| surface.id)
        .collect();
    for id in roots {
        close(ctx, id, CloseReason::Requested);
    }
}

fn close_children<M: 'static>(ctx: &mut HostCtx<M>, parent: SurfaceId) {
    let children: Vec<SurfaceId> = ctx
        .state
        .surfaces
        .values()
        .filter(|surface| surface.role.parent() == Some(parent))
        .map(|surface| surface.id)
        .collect();
    for child in children {
        close(ctx, child, CloseReason::ParentClosed);
    }
}

/// Put a layer lifted to `OnDemand` by a grabbing popup back to its configured keyboard mode,
/// once no grabbing popup of it remains.
fn restore_keyboard(state: &mut HostState, parent: SurfaceId) {
    let still_grabbed = state.surfaces.values().any(|surface| {
        matches!(&surface.role, Role::Popup { parent: p, config, .. }
            if *p == parent && config.grab == Grab::Grabbed)
    });
    if still_grabbed {
        return;
    }
    let Some(surface) = state.surfaces.get_mut(&parent) else {
        return;
    };
    if let Role::Layer {
        surface: layer,
        config,
        keyboard,
    } = &mut surface.role
        && *keyboard != config.keyboard
    {
        *keyboard = config.keyboard;
        layer.set_keyboard_interactivity(to_wire_keyboard(config.keyboard));
        layer.commit();
    }
}

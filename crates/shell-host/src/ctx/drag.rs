//! Starting a drag out of a surface. The pipes its targets ask for are filled from the loop
//! (`ctx::pipe`).
//!
//! A start is checked up front (`drag::serial`, the data device, one drag at a time) and
//! answered `Unsupported` at once when it cannot run.
//!
//! See FINDINGS F60.

use tokio::sync::oneshot;
use wayland_client::Proxy;

use crate::ctx::HostCtx;
use crate::drag::offer::offers;
use crate::drag::outcome::{DragOutcome, DragRefused};
use crate::drag::serial::drag_serial;
use crate::drag::spec::DragSpec;
use crate::handlers::drag_source::ActiveDrag;
use crate::id::SurfaceId;

/// `wl_data_source.dnd_drop_performed`, `dnd_finished` and `action` exist from v3.
const DND_EVENTS_SINCE: u32 = 3;

/// Start dragging `spec` out of surface `from`; `reply` gets the outcome.
pub(super) fn start<M: 'static>(
    ctx: &mut HostCtx<M>,
    from: SurfaceId,
    spec: DragSpec,
    reply: oneshot::Sender<DragOutcome>,
) {
    if let Err(why) = check(ctx, from) {
        // The caller may have stopped waiting; nobody is left to tell then.
        let _ = reply.send(DragOutcome::Unsupported { why });
        return;
    }
    let icon = super::drag_icon::open(ctx, from, spec.icon);
    let state = &mut ctx.state;
    let (Some(manager), Some(device), Some(origin), Ok(serial)) = (
        &state.data_device,
        &state.input.data_device,
        state.surfaces.get(&from),
        drag_serial(state.input.grab.get(), from),
    ) else {
        // `check` saw all four a moment ago, on the same thread.
        return;
    };
    let offered = offers(&spec.payload);
    let source = manager.create_drag_and_drop_source(
        &state.qh,
        offered.iter().map(|offer| offer.mime.as_str()),
        spec.actions.to_wire(),
    );
    let icon_surface = icon.and_then(|id| state.surfaces.get(&id));
    source.start_drag(
        device,
        origin.role.wl_surface(),
        icon_surface.map(|icon| icon.role.wl_surface()),
        serial,
    );
    state.drag.active = Some(ActiveDrag {
        source,
        offered,
        reply,
        chosen: None,
        icon,
    });
}

/// Why a drag from `from` cannot start now, if it cannot.
fn check<M: 'static>(ctx: &HostCtx<M>, from: SurfaceId) -> Result<(), DragRefused> {
    let state = &ctx.state;
    if state.drag.active.is_some() {
        return Err(DragRefused::InProgress);
    }
    if !state.surfaces.contains_key(&from) {
        return Err(DragRefused::NoSurface);
    }
    let manager = state
        .data_device
        .as_ref()
        .ok_or(DragRefused::NoDataDevice)?;
    if manager.data_device_manager().version() < DND_EVENTS_SINCE {
        return Err(DragRefused::OldProtocol);
    }
    state
        .input
        .data_device
        .as_ref()
        .ok_or(DragRefused::NoDataDevice)?;
    drag_serial(state.input.grab.get(), from).map(drop)
}

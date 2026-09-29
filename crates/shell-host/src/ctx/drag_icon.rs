//! A drag's icon: the `DragIcon::Root` document on its own `wl_surface`, rendered through the
//! same presenter path as every other surface (one device, one renderer), from before the drag
//! starts until it ends (`handlers::drag_source::HostState::drag_end` removes it).

use smithay_client_toolkit::compositor::Surface;

use crate::blur::BlurSpec;
use crate::csd::Inset;
use crate::ctx::HostCtx;
use crate::ctx::spawn::{Initial, insert, new_wl_surface};
use crate::drag::spec::DragIcon;
use crate::error::{HostError, Result};
use crate::id::SurfaceId;
use crate::id::SurfaceName;
use crate::region::RegionSpec;
use crate::role::Role;
use crate::root::HostRoot;
use crate::scale::Size;
use crate::state::HostState;
use crate::surface::Configured;

/// The icon surface for a drag from `origin`, or `None` for `DragIcon::None` or when it could
/// not be made (the drag then goes ahead without one).
pub(super) fn open<M: 'static>(
    ctx: &mut HostCtx<M>,
    origin: SurfaceId,
    icon: DragIcon,
) -> Option<SurfaceId> {
    let DragIcon::Root { root, size } = icon else {
        return None;
    };
    let id = ctx.state.ids.next();
    let made = spawn(&mut ctx.state, origin, id, size, root)
        .and_then(|()| super::paint::ensure_presenter(ctx, id));
    match made {
        Ok(()) => Some(id),
        Err(error) => {
            eprintln!("shell-host: drag icon from {origin:?}: {error}");
            ctx.state.surfaces.remove(&id);
            None
        }
    }
}

/// Create icon surface `id` for a drag from `origin`, `size` logical px, rendering `root`. Not
/// mapped: the surface takes its role from `start_drag` and its first buffer from the first
/// render, and there is no configure to wait for.
fn spawn(
    state: &mut HostState,
    origin: SurfaceId,
    id: SurfaceId,
    size: Size,
    root: HostRoot,
) -> Result<()> {
    let (origin_name, scale) = state
        .surfaces
        .get(&origin)
        .map(|surface| (surface.name.0.clone(), surface.scale))
        .ok_or(HostError::NoSurface(origin))?;
    let role = Role::DragIcon {
        surface: Surface::from(new_wl_surface(state)),
        origin,
    };
    let name = SurfaceName(format!("{origin_name}/drag-icon"));
    let at = Initial {
        logical: size,
        inset: Inset::NONE,
        scale,
    };
    let regions = (BlurSpec::None, RegionSpec::Whole);
    insert(state, id, role, name, at, regions, root);
    if let Some(surface) = state.surfaces.get_mut(&id) {
        surface.configured = Configured::Done;
    }
    Ok(())
}

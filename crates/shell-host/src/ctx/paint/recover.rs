//! A lost GPU device: the driver reset or the device was destroyed, so the shared device and
//! every swapchain made on it are gone. Every surface would otherwise stay on its last frame and
//! fail again each time its watchdog fires, with the lock screen among them.
//!
//! The rebuild drops every presenter (each wgpu surface before the device it was configured
//! on), then the device, and makes them again from the one wgpu instance, which stays: it is
//! not what was lost, and dropping an instance that presented can leave the next one's surface
//! queries unusable (`gpu::probe`). Lock surfaces get their presenters first, so a locked
//! screen is back before anything else is. A device lost again and again ends the host
//! (`gpu::rebuild`), and the unit that started it starts it again.

use std::time::Instant;

use blitz_kit::adapter::Health;

use crate::ctx::HostCtx;
use crate::error::HostError;
use crate::event::CloseReason;
use crate::gpu::rebuild::Verdict;
use crate::id::SurfaceId;
use crate::role::RoleKind;

use super::ensure_presenter;

/// Rebuild the GPU state if the device was lost.
pub(super) fn recover_device<M: 'static>(ctx: &mut HostCtx<M>) {
    let lost = ctx
        .state
        .gpu
        .as_ref()
        .is_some_and(|gpu| gpu.health.state() == Health::Lost);
    if !lost || ctx.failed.is_some() {
        return;
    }
    if ctx.rebuilds.record(Instant::now()) == Verdict::GiveUp {
        ctx.failed = Some(HostError::GaveUp(
            "the graphics device was lost again and again".into(),
        ));
        return;
    }
    eprintln!("shell-host: the graphics device was lost; making every surface's presenter again");
    let drawn = lock_surfaces_first(presenting(ctx));
    for surface in ctx.state.surfaces.values_mut() {
        surface.presenter = None;
        surface.geometry = None;
    }
    ctx.state.gpu = None;
    for id in drawn {
        if let Err(error) = ensure_presenter(ctx, id) {
            eprintln!("shell-host: surface {id:?} after the device was lost: {error}");
            crate::ctx::ops::close(ctx, id, CloseReason::Failed);
        }
    }
}

/// The surfaces that have a presenter now, with their roles. A hidden or not yet configured
/// surface has none, and gets one when it is shown or configured, as before.
fn presenting<M>(ctx: &HostCtx<M>) -> Vec<(SurfaceId, RoleKind)> {
    ctx.state
        .surfaces
        .iter()
        .filter(|(_, surface)| surface.presenter.is_some())
        .map(|(&id, surface)| (id, surface.role.kind()))
        .collect()
}

/// `surfaces`' ids, lock surfaces first and the rest in the order given.
fn lock_surfaces_first(mut surfaces: Vec<(SurfaceId, RoleKind)>) -> Vec<SurfaceId> {
    surfaces.sort_by_key(|&(_, kind)| kind != RoleKind::Lock);
    surfaces.into_iter().map(|(id, _)| id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_surfaces_are_rebuilt_before_everything_else() {
        let surfaces = vec![
            (SurfaceId(1), RoleKind::Layer),
            (SurfaceId(2), RoleKind::Lock),
            (SurfaceId(3), RoleKind::Popup),
            (SurfaceId(4), RoleKind::Lock),
            (SurfaceId(5), RoleKind::Toplevel),
        ];
        let order: Vec<u64> = lock_surfaces_first(surfaces)
            .into_iter()
            .map(|id| id.0)
            .collect();
        assert_eq!(order, vec![2, 4, 1, 3, 5]);
    }
}

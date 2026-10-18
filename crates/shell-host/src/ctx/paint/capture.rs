//! The offscreen render behind the debug `capture`.

use std::time::Instant;

use crate::error::{HostError, Result};
use crate::gpu::presenter::FrameTarget;
use crate::id::SurfaceId;
use crate::state::HostState;

/// Render surface `id` offscreen at its buffer size (debug `capture`).
pub(crate) fn capture(state: &mut HostState, id: SurfaceId) -> Result<image::RgbaImage> {
    let time = state.clock.seconds_at(Instant::now());
    let surface = state
        .surfaces
        .get_mut(&id)
        .ok_or(HostError::NoSurface(id))?;
    let target = FrameTarget {
        buffer: surface.buffer(),
        content: surface.content(),
        scale: surface.scale,
        time,
    };
    let presenter = surface
        .presenter
        .as_mut()
        .ok_or_else(|| HostError::Render("the surface has not been presented yet".into()))?;
    presenter.capture(state.gpu.as_mut(), &mut surface.document, target)
}

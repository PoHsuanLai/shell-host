//! Creating surfaces: the role (layer surface, xdg toplevel, xdg popup) with its initial state,
//! the document with its `SurfaceHandle`, and the `HostSurface` that ties them together.
//!
//! A role is built unmapped (every request made, no commit) and mapped separately, because a
//! hidden surface keeps an unmapped role and maps it again when shown.

mod document;
mod layer;
mod popup;
mod rebuild;
mod size;
mod toplevel;

use std::sync::Arc;

use wayland_client::protocol::wl_surface::WlSurface;

use crate::blur::BlurSpec;
use crate::csd::Inset;
use crate::debug::{StatsWindow, SurfaceName};
use crate::dom::document::DocumentEnv;
use crate::dom::net::LocalNet;
use crate::dom::shell_provider::HostShellProvider;
use crate::frame::FrameState;
use crate::handle::{HandleParts, SurfaceHandle};
use crate::id::SurfaceId;
use crate::live::Live;
use crate::live_scale::LiveScale;
use crate::modifiers::Modifiers;
use crate::msg::Visibility;
use crate::region::RegionSpec;
use crate::role::Role;
use crate::root::Root;
use crate::scale::{Scale, Size};
use crate::scroll::surface::SurfaceScroll;
use crate::scroll::watch::ScrollBoard;
use crate::state::HostState;
use crate::surface::{AppliedRegions, Configured, HostSurface};
use crate::tick::FrameTicks;
use crate::toplevel::ToplevelState;

pub(super) use layer::layer;
pub(super) use popup::{popup, positioner};
pub(super) use rebuild::{map, rebuild};
pub(super) use toplevel::toplevel;

pub(super) fn new_wl_surface(state: &mut HostState) -> WlSurface {
    let wl = state.compositor.create_surface(&state.qh);
    if let Some(fractional) = state.fractional.as_mut() {
        fractional.watch(&state.qh, &wl);
    }
    wl
}

/// A new surface's size, resize band and first-guess scale.
#[derive(Debug, Clone, Copy)]
pub(super) struct Initial {
    pub(super) logical: Size,
    pub(super) inset: Inset,
    pub(super) scale: Scale,
}

/// Build the document (with this surface's `SurfaceHandle` as a root context) and record the
/// surface at its `Initial` size and scale. It is marked woken, so the first flush polls it.
pub(super) fn insert(
    state: &mut HostState,
    id: SurfaceId,
    role: Role,
    name: SurfaceName,
    Initial {
        logical,
        inset,
        scale,
    }: Initial,
    (blur, input): (BlurSpec, RegionSpec),
    root: Root,
) {
    let shell = Arc::new(HostShellProvider::new(
        id,
        state.requests.ping(),
        state.clipboard.selection.clone(),
    ));
    let ticks = FrameTicks::default();
    let live_scale = LiveScale::new(scale);
    let live_toplevel = Live::new(ToplevelState::default());
    let live_modifiers = Live::new(Modifiers::default());
    let scrolls = ScrollBoard::default();
    let parts = HandleParts {
        requests: state.requests.clone(),
        ids: state.ids.clone(),
        ticks: ticks.clone(),
        scale: live_scale.clone(),
        toplevel: live_toplevel.clone(),
        grab: state.input.grab.clone(),
        scrolls: scrolls.clone(),
        modifiers: live_modifiers.clone(),
        modality: state.input.modality.clone(),
    };
    let handle = SurfaceHandle::new(id, role.kind(), parts);
    let env = DocumentEnv {
        shell: Arc::clone(&shell),
        fonts: state.fonts.for_document(),
        net: Arc::new(LocalNet),
        viewport: super::paint::viewport(logical, inset, scale, state.color_scheme),
        prepaint: state.prepaint.clone(),
    };
    let document = document::surface_document(root, handle, env);
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
            inset,
            scale,
            blur,
            input,
            stats: StatsWindow::default(),
            geometry: None,
            regions: AppliedRegions::default(),
            ticks,
            live_scale,
            live_toplevel,
            live_modifiers,
            doc_cursor: Some(cursor_icon::CursorIcon::Default),
            scroll: SurfaceScroll::new(scrolls, state.scroll),
        },
    );
    state.woken.mark(id);
}

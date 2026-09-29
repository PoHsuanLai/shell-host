//! The document a surface shows.

use crate::dom::document::{DocumentEnv, SurfaceDocument};
use crate::handle::SurfaceHandle;
use crate::root::Root;

/// Surface `root`'s document, with `handle` provided as a root context before the first build,
/// so `use_surface()` works in every component of the surface from its first render.
pub(super) fn surface_document(
    root: Root,
    handle: SurfaceHandle,
    env: DocumentEnv,
) -> SurfaceDocument {
    SurfaceDocument::build(root.context(handle), env)
}

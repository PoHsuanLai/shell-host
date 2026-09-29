//! Custom-widget textures that outlive a frame. A `blitz_dom::Widget` registers its wgpu
//! texture through the painter's `RenderContext` (`try_register_custom_resource`), and gets the
//! device from `renderer_specific_context()` (a `wgpu_context::DeviceHandle`).
//! `VelloHybridScenePainter` borrows the texture map for one paint; this holds it per device
//! between paints and turns it into the `TextureBindings` the render needs.
//!
//! See FINDINGS F77.

use anyrender::ResourceId;
use rustc_hash::FxHashMap;
use vello_common::TextureId;
use vello_hybrid::TextureBindings;

/// The registered widget textures of one device.
#[derive(Debug, Default)]
pub(crate) struct ContextProxy {
    pub(crate) textures: FxHashMap<ResourceId, wgpu::TextureView>,
}

impl ContextProxy {
    /// The bindings for this frame's render.
    pub(crate) fn bindings(&self) -> TextureBindings {
        let mut bindings = TextureBindings::new();
        for (id, view) in &self.textures {
            // The painter draws a resource under the same id (`TextureId(id.into_ffi())`).
            bindings.insert(TextureId(id.into_ffi()), view.clone());
        }
        bindings
    }
}

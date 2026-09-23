//! Custom-widget textures that outlive a frame. A `blitz_dom::Widget` registers its wgpu
//! texture through the painter's `RenderContext` (`try_register_custom_resource`), and gets the
//! device from `renderer_specific_context()` (a `wgpu_context::DeviceHandle`; SPIKE.md S3).
//! `VelloHybridScenePainter` borrows the texture map for one paint; this holds it per device
//! between paints and turns it into the `TextureBindings` the render needs.
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use anyrender::ResourceId;
use rustc_hash::FxHashMap;
use vello_hybrid::TextureBindings;

/// The registered widget textures of one device.
#[derive(Debug, Default)]
pub struct ContextProxy {
    pub(crate) textures: FxHashMap<ResourceId, wgpu::TextureView>,
}

impl ContextProxy {
    /// The bindings for this frame's render.
    pub fn bindings(&self) -> TextureBindings {
        todo!()
    }
}

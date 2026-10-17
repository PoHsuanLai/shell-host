//! Custom-widget textures that outlive a frame. A `blitz_dom::Widget` registers its wgpu
//! texture through the painter's `RenderContext` (`try_register_custom_resource`), and gets the
//! device from `renderer_specific_context()` (a `wgpu_context::DeviceHandle`).
//! `VelloHybridScenePainter` borrows the texture map for one paint; this holds it per device
//! between paints and turns it into the `TextureBindings` the render needs.
//!
//! See FINDINGS F77.

use anyrender::ResourceId;
use blitz_dom::BaseDocument;
use rustc_hash::FxHashMap;
use vello_common::TextureId;
use vello_hybrid::TextureBindings;

/// The registered widget textures of one device.
#[derive(Debug, Default)]
pub(crate) struct ContextProxy {
    pub(crate) textures: FxHashMap<ResourceId, wgpu::TextureView>,
}

impl ContextProxy {
    /// Forget the textures of the widgets `doc` has dropped since the last call (a node that left
    /// the page takes its widget with it). Called before a paint, so the map holds what the pages
    /// still show and not what they once showed.
    pub(crate) fn forget_dropped(&mut self, doc: &mut BaseDocument) {
        for id in doc.take_pending_resource_deallocations() {
            self.textures.remove(&id);
        }
    }

    /// Forget every texture `doc`'s widgets hold, for a document that is going away for good.
    pub(crate) fn forget_document(&mut self, doc: &mut BaseDocument) {
        for node in doc.custom_widget_node_ids() {
            doc.remove_custom_widget(node);
        }
        self.forget_dropped(doc);
    }

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

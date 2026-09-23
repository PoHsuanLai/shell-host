//! Custom-widget textures that outlive a frame. A `blitz_dom::Widget` registers its wgpu
//! texture through the painter's `RenderContext` (`try_register_custom_resource`), and gets the
//! device from `renderer_specific_context()` (a `wgpu_context::DeviceHandle`; SPIKE.md S3).
//! `VelloHybridScenePainter` borrows the texture map for one paint; this holds it per device
//! between paints and turns it into the `TextureBindings` the render needs.

use std::any::Any;

use anyrender::{RegisterResourceError, RegisterResourceErrorKind, RenderContext, ResourceId};
use rustc_hash::FxHashMap;
use vello_common::TextureId;
use vello_hybrid::TextureBindings;
use wgpu_context::DeviceHandle;

use crate::gpu::context::GpuContext;

/// The registered widget textures of one device.
#[derive(Debug, Default)]
pub struct ContextProxy {
    pub(crate) textures: FxHashMap<ResourceId, wgpu::TextureView>,
}

impl ContextProxy {
    /// The bindings for this frame's render.
    pub fn bindings(&self) -> TextureBindings {
        let mut bindings = TextureBindings::new();
        for (id, view) in &self.textures {
            // The painter draws a resource under the same id (`TextureId(id.into_ffi())`).
            bindings.insert(TextureId(id.into_ffi()), view.clone());
        }
        bindings
    }

    /// Register a texture, or a view of one, under a fresh id; any other resource is refused.
    /// The painter's own `RenderContext` does the same into the same map during a paint.
    fn register(&mut self, resource: Box<dyn Any>) -> Result<ResourceId, RegisterResourceError> {
        let view = match resource.downcast::<wgpu::Texture>() {
            Ok(texture) => texture.create_view(&wgpu::TextureViewDescriptor::default()),
            Err(other) => match other.downcast::<wgpu::TextureView>() {
                Ok(view) => *view,
                Err(_) => return Err(RegisterResourceErrorKind::UnsupportedResourceKind.into()),
            },
        };
        let id = ResourceId::new();
        self.textures.insert(id, view);
        Ok(id)
    }
}

/// The shared device as a widget sees it outside a paint: registering a texture here makes it
/// drawable by `Paint::Resource` in every later frame of every surface on the device.
pub struct WidgetContext<'a> {
    textures: &'a mut ContextProxy,
    device: &'a DeviceHandle,
}

impl WidgetContext<'_> {
    /// Whether `id` names a registered texture.
    pub fn is_registered(&self, id: ResourceId) -> bool {
        self.textures.textures.contains_key(&id)
    }
}

impl RenderContext for WidgetContext<'_> {
    fn try_register_custom_resource(
        &mut self,
        resource: Box<dyn Any>,
    ) -> Result<ResourceId, RegisterResourceError> {
        self.textures.register(resource)
    }

    fn unregister_resource(&mut self, resource_id: ResourceId) {
        self.textures.textures.remove(&resource_id);
    }

    fn renderer_specific_context(&self) -> Option<Box<dyn Any>> {
        Some(Box::new(self.device.clone()))
    }
}

impl std::fmt::Debug for WidgetContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WidgetContext")
            .field("textures", &self.textures.textures.len())
            .finish_non_exhaustive()
    }
}

impl GpuContext {
    /// The device's widget textures as a `RenderContext`.
    pub fn widget_context(&mut self) -> WidgetContext<'_> {
        WidgetContext {
            textures: &mut self.widgets,
            device: &self.device,
        }
    }
}

//! The uploaded images the shared renderer keeps between frames (`GpuContext::images`, by
//! content), and when to let them go. Album art, thumbnails and rotating wallpapers bring new
//! images for as long as the shell runs; kept for good they would fill the renderer's atlas,
//! after which no new image could be drawn until the shell restarted.

use anyrender_vello_hybrid::ImageUploads;
use vello_hybrid::{Renderer, Resources};

/// How many distinct images may stay uploaded. Beyond it the next frame starts from an empty
/// cache: the images still on screen upload again within that frame, and the rest are gone.
pub(crate) const MAX_CACHED_IMAGES: usize = 1024;

/// What to do with a cache of `len` images.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Eviction {
    Keep,
    All,
}

/// The decision for a cache of `len` images.
pub(crate) fn eviction(len: usize) -> Eviction {
    if len > MAX_CACHED_IMAGES {
        Eviction::All
    } else {
        Eviction::Keep
    }
}

/// Drop every cached image, freeing its place in the atlas through `encoder`, when the cache has
/// outgrown its limit. Called before a frame is painted, so the painting re-uploads what it
/// draws.
pub(crate) fn trim(
    images: &mut ImageUploads,
    renderer: &mut Renderer,
    resources: &mut Resources,
    encoder: &mut wgpu::CommandEncoder,
) {
    if eviction(images.len()) == Eviction::Keep {
        return;
    }
    eprintln!(
        "shell-host: {} uploaded images; dropping them so the atlas does not fill",
        images.len()
    );
    for (_, uploaded) in images.drain() {
        if let Some(uploaded) = uploaded {
            renderer.destroy_image(resources, encoder, uploaded.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cache_is_dropped_only_once_it_outgrows_its_limit() {
        let cases: &[(&str, usize, Eviction)] = &[
            ("empty", 0, Eviction::Keep),
            ("at the limit", MAX_CACHED_IMAGES, Eviction::Keep),
            ("one past it", MAX_CACHED_IMAGES + 1, Eviction::All),
        ];
        for (name, len, want) in cases {
            assert_eq!(eviction(*len), *want, "{name}");
        }
    }
}

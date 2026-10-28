//! The uploaded images the shared renderer keeps between frames (`GpuContext::images`, by
//! content), and when to let them go. Album art, thumbnails and rotating wallpapers bring new
//! images for as long as the shell runs; kept for good they would fill the renderer's atlas,
//! after which no new image could be drawn until the shell restarted.
//!
//! The atlas can fill before the count limit: a few hundred full-size app icons are enough. An
//! upload it refuses is recorded as `None` and never retried, so a dock icon painted at that
//! moment would stay blank. A frame that had an upload refused therefore empties the cache
//! before the next frame and paints again.
//!
//! The emptying is its own submission. Freeing a slot clears it with a render pass, while a new
//! image is written with `Queue::write_texture`, which runs before the passes of the submission
//! it joins: recorded in the frame's encoder, the clear would wipe the icons that frame uploads
//! again into the freed slots, and they would stay blank.

use anyrender_vello_hybrid::ImageUploads;
use vello_hybrid::{Renderer, Resources};

/// How many distinct images may stay uploaded. Beyond it the next frame starts from an empty
/// cache: the images still on screen upload again within that frame, and the rest are gone.
pub(crate) const MAX_CACHED_IMAGES: usize = 1024;

/// Whether to drop the whole cache before a frame, for a cache of `len` images after a frame
/// that asked for it (`pending`).
pub(crate) fn eviction(len: usize, pending: bool) -> bool {
    len > MAX_CACHED_IMAGES || pending
}

/// Whether a painting left an image out, from whether the cache was emptied before it
/// (`evicted`) and the refused uploads the cache held before (`refused_before`) and after it:
/// the atlas refused an image, so the next frame starts from an empty cache and this surface
/// paints again. A refusal right after the cache was emptied is the image's own (too large or
/// empty), not a full atlas: it stands, or the surface would repaint for ever.
pub(crate) fn painted_missing_images(
    evicted: bool,
    refused_before: usize,
    refused_after: usize,
) -> bool {
    !evicted && refused_after > refused_before
}

/// The uploads the atlas refused, kept as `None`.
pub(crate) fn refused(images: &ImageUploads) -> usize {
    images
        .values()
        .filter(|uploaded| uploaded.is_none())
        .count()
}

/// Drop every cached image, clearing its place in the atlas in a submission of its own, when the
/// cache has outgrown its limit or the last painting asked for it (`pending`, reset here). Called
/// before a frame is painted, so the painting re-uploads what it draws. Returns what it did.
pub(crate) fn trim(
    images: &mut ImageUploads,
    pending: &mut bool,
    renderer: &mut Renderer,
    resources: &mut Resources,
    (device, queue): (&wgpu::Device, &wgpu::Queue),
) -> bool {
    let decision = eviction(images.len(), std::mem::take(pending));
    if !decision {
        return decision;
    }
    eprintln!(
        "shell-host: {} uploaded images, {} refused; dropping them so the atlas has room",
        images.len(),
        refused(images)
    );
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("shell-host image eviction"),
    });
    for (_, uploaded) in images.drain() {
        if let Some(uploaded) = uploaded {
            renderer.destroy_image(resources, &mut encoder, uploaded.id);
        }
    }
    queue.submit([encoder.finish()]);
    decision
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cache_is_dropped_once_it_outgrows_its_limit_or_a_painting_asks() {
        let cases: &[(&str, usize, bool, bool)] = &[
            ("empty", 0, false, false),
            ("at the limit", MAX_CACHED_IMAGES, false, false),
            ("one past it", MAX_CACHED_IMAGES + 1, false, true),
            ("asked by a refusal", 3, true, true),
        ];
        for (name, len, pending, want) in cases {
            assert_eq!(eviction(*len, *pending), *want, "{name}");
        }
    }

    #[test]
    fn a_refused_upload_repaints_unless_the_cache_was_just_emptied() {
        let cases: &[(&str, bool, usize, usize, bool)] = &[
            ("nothing refused", false, 0, 0, false),
            ("an old refusal only", false, 2, 2, false),
            ("a new refusal", false, 2, 3, true),
            ("refused from an empty atlas", true, 0, 1, false),
        ];
        for (name, evicted, before, after, want) in cases {
            assert_eq!(
                painted_missing_images(*evicted, *before, *after),
                *want,
                "{name}"
            );
        }
    }
}

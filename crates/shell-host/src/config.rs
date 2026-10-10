//! What a host is asked to use: which rendering backend and which GPU adapter, and the pass
//! it runs over every document before painting it.
//!
//! See FINDINGS F16.

use blitz_kit::adapter::AdapterPref;
use blitz_kit::fonts::{FontFaces, SharedFonts};
use serde::{Deserialize, Serialize};

use crate::prepaint::Prepaint;
use crate::route::WaylandRoute;

/// How a host is set up. `Host::connect` takes one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostConfig {
    pub backend: Backend,
    pub adapter: AdapterPref,
    /// How long after a toplevel is activated a press is swallowed as the activating click
    /// (design/13 §13.3.8). The settings key `menus.first_mouse_window_ms`, which design/22
    /// marks proposed, so a shell feeds it from settings.
    #[serde(default = "Ms::first_mouse_window")]
    pub first_mouse_window_ms: Ms,
    /// Run over every surface's and popup's document after each `resolve`, before painting
    /// (`Prepaint`: pixel snapping at a fractional scale). Per host rather than per document
    /// because the pass is a property of the design system the whole shell is drawn in, and
    /// a popup opened from a component must get it without the component knowing. Code, not
    /// settings, so it is never serialized.
    #[serde(skip)]
    pub prepaint: Prepaint,
    /// The faces a shell's design system bundles, registered into the shared `FontContext` once
    /// at startup (`Host::connect`) so every surface, popup and toplevel document resolves them —
    /// otherwise a `font-family` naming one falls back past every real face to `system-ui`
    /// (FINDINGS, "HostConfig::fonts"). A shell passes its own faces, e.g. quire's
    /// `ds::FACES` bytes; shell-host bundles none of its own, so the default is empty. Code, not
    /// settings — a face is compiled into the shell binary, not read from a settings file — so
    /// it is never serialized, exactly like `prepaint`.
    #[serde(skip)]
    pub fonts: FontFaces,
    /// Which connection `Host::connect` uses: the session's (default) or a socket the
    /// compositor handed over. A descriptor is not settings, so it is never serialized.
    #[serde(skip)]
    pub route: WaylandRoute,
}

impl Default for HostConfig {
    fn default() -> HostConfig {
        HostConfig {
            backend: Backend::default(),
            adapter: AdapterPref::default(),
            first_mouse_window_ms: Ms::first_mouse_window(),
            prepaint: Prepaint::None,
            fonts: FontFaces::default(),
            route: WaylandRoute::Environment,
        }
    }
}

/// The family names `config.fonts` actually resolves to once registered — built with no Wayland
/// connection, so a shell can assert its own faces survive registration (e.g. that `"Inter"` is
/// still present after a dependency bump) without connecting a host. Mirrors exactly what
/// `Host::connect` does with `HostConfig::fonts` at startup, on a throwaway `SharedFonts::system`.
pub fn registered_families(config: &HostConfig) -> Vec<String> {
    let mut fonts = SharedFonts::system();
    config
        .fonts
        .0
        .iter()
        .flat_map(|bytes| fonts.register(bytes))
        .collect()
}

/// A configured duration in whole milliseconds, as settings keys spell them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Ms(pub u32);

impl Ms {
    /// `menus.first_mouse_window_ms`'s default (design/22-SETTINGS.md).
    pub const fn first_mouse_window() -> Ms {
        Ms(100)
    }

    /// As a `Duration`.
    pub fn duration(self) -> std::time::Duration {
        std::time::Duration::from_millis(u64::from(self.0))
    }
}

/// How pixels reach the compositor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    /// wgpu (Vulkan, then GL) with one shared `vello_hybrid` renderer; falls back to `Cpu` when
    /// no adapter survives validation.
    #[default]
    Gpu,
    /// `anyrender_vello_cpu` into `wl_shm` buffers. No GPU, no custom-widget textures.
    Cpu,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_faces_resolves_no_families() {
        let config = HostConfig::default();
        assert_eq!(registered_families(&config), Vec::<String>::new());
    }

    #[test]
    fn the_default_is_gpu_with_wgpus_own_order() {
        assert_eq!(
            HostConfig::default(),
            HostConfig {
                backend: Backend::Gpu,
                adapter: AdapterPref::Auto,
                first_mouse_window_ms: Ms(100),
                prepaint: Prepaint::None,
                fonts: FontFaces::default(),
                route: WaylandRoute::Environment,
            }
        );
    }
}

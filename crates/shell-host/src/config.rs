//! What a host is asked to use: which rendering backend and which GPU adapter, and the pass
//! it runs over every document before painting it.

use serde::{Deserialize, Serialize};

use crate::prepaint::Prepaint;

/// The environment variable that names an adapter by substring, as wgpu's own
/// `initialize_adapter_from_env` reads it. It overrides [`HostConfig::adapter`].
pub const ADAPTER_ENV: &str = "WGPU_ADAPTER_NAME";

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
}

impl Default for HostConfig {
    fn default() -> HostConfig {
        HostConfig {
            backend: Backend::default(),
            adapter: AdapterPref::default(),
            first_mouse_window_ms: Ms::first_mouse_window(),
            prepaint: Prepaint::None,
        }
    }
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

/// Which adapter the GPU backend prefers. A preference, not a guarantee: every candidate is
/// validated by a first present on a probe surface over a throwaway connection, because a
/// cross-GPU adapter can pass `is_surface_supported` and still have its first buffer rejected
/// with a fatal protocol error (SPIKE.md S1: RADV on this machine, `zwp_linux_dmabuf` error 7).
/// A candidate that fails is dropped and the next one is tried.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum AdapterPref {
    /// wgpu's default order for the surface (high performance first).
    #[default]
    Auto,
    /// The first adapter whose name contains this text, case-insensitively.
    Named(String),
    /// Discrete GPUs first.
    Discrete,
    /// Integrated GPUs first.
    Integrated,
}

impl AdapterPref {
    /// The preference in force once the environment is read: a non-empty `WGPU_ADAPTER_NAME`
    /// wins over whatever was configured.
    pub fn with_env(self, env_value: Option<&str>) -> AdapterPref {
        match env_value.map(str::trim) {
            Some(name) if !name.is_empty() => AdapterPref::Named(name.to_owned()),
            _ => self,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_environment_overrides_only_when_it_names_something() {
        let cases: &[(&str, AdapterPref, Option<&str>, AdapterPref)] = &[
            ("unset", AdapterPref::Discrete, None, AdapterPref::Discrete),
            (
                "empty",
                AdapterPref::Integrated,
                Some(""),
                AdapterPref::Integrated,
            ),
            ("blank", AdapterPref::Auto, Some("  "), AdapterPref::Auto),
            (
                "named",
                AdapterPref::Discrete,
                Some("NVIDIA"),
                AdapterPref::Named("NVIDIA".into()),
            ),
            (
                "trimmed",
                AdapterPref::Auto,
                Some(" AMD "),
                AdapterPref::Named("AMD".into()),
            ),
        ];
        for (name, configured, env, want) in cases {
            assert_eq!(configured.clone().with_env(*env), *want, "{name}");
        }
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
            }
        );
    }
}

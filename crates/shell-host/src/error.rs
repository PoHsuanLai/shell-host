//! The host's one error type, and the decision each error carries.

/// What a caller should do about an error. This workspace's substitute for `Retryable`
/// (plan "Conventions substitutions"): the host itself branches on it when choosing an adapter,
/// and a shell branches on it when a surface fails to spawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Recovery {
    /// Transient; the same call may succeed later.
    Retry,
    /// This path cannot work here, but a lesser one can (next adapter, CPU backend, no blur).
    Fallback,
    /// Carry on without the feature the call was for.
    Degrade,
    /// The connection or the process is unusable; exit.
    Fatal,
}

/// Everything that can go wrong in the host.
#[derive(Debug, thiserror::Error)]
pub enum HostError {
    /// No Wayland display to connect to (`WAYLAND_DISPLAY` unset or the socket refused).
    #[error("cannot connect to the Wayland display: {0}")]
    Connect(String),
    /// A global the host cannot work without is missing (`wl_compositor`, `xdg_wm_base`).
    #[error("the compositor does not provide {global}")]
    MissingGlobal { global: &'static str },
    /// An optional global is missing (`zwlr_layer_shell_v1`, blur, text input); only the
    /// feature that needs it is lost.
    #[error("the compositor does not provide {global}; {feature} is unavailable")]
    Unsupported {
        global: &'static str,
        feature: &'static str,
    },
    /// The compositor killed the connection. libwayland fails every later call, so the host
    /// stops instead of looping (SPIKE.md S1 finding 7).
    #[error("fatal Wayland protocol error: {0}")]
    Protocol(String),
    /// No adapter could be created for the surface.
    #[error("no usable GPU adapter: {0}")]
    NoAdapter(String),
    /// An adapter was found but its first present failed validation.
    #[error("adapter {adapter} failed its first present: {reason}")]
    AdapterRejected { adapter: String, reason: String },
    /// Creating or configuring a wgpu surface failed.
    #[error("GPU surface: {0}")]
    Surface(String),
    /// Rendering a frame failed.
    #[error("render: {0}")]
    Render(String),
    /// The id does not name a live surface.
    #[error("no surface {0:?}")]
    NoSurface(crate::id::SurfaceId),
    /// The event loop or the tokio runtime could not be created or dispatched.
    #[error("event loop: {0}")]
    EventLoop(String),
    /// The host has exited; a `HostHandle` outlived it.
    #[error("the host has exited")]
    HostGone,
    /// Reading or writing a pipe or file (clipboard, debug capture).
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
}

impl HostError {
    /// The decision this error carries.
    pub fn recovery(&self) -> Recovery {
        match self {
            HostError::Connect(_) | HostError::MissingGlobal { .. } | HostError::Protocol(_) => {
                Recovery::Fatal
            }
            HostError::EventLoop(_) => Recovery::Fatal,
            HostError::NoAdapter(_) | HostError::AdapterRejected { .. } => Recovery::Fallback,
            HostError::Surface(_) => Recovery::Fallback,
            HostError::Unsupported { .. } | HostError::NoSurface(_) | HostError::HostGone => {
                Recovery::Degrade
            }
            HostError::Render(_) | HostError::Io(_) => Recovery::Retry,
        }
    }
}

/// The host's `Result`.
pub type Result<T, E = HostError> = std::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::SurfaceId;

    #[test]
    fn every_error_carries_its_decision() {
        let cases: Vec<(HostError, Recovery)> = vec![
            (HostError::Connect("no socket".into()), Recovery::Fatal),
            (
                HostError::MissingGlobal {
                    global: "wl_compositor",
                },
                Recovery::Fatal,
            ),
            (
                HostError::Protocol("zwp_linux_dmabuf 7".into()),
                Recovery::Fatal,
            ),
            (HostError::EventLoop("epoll".into()), Recovery::Fatal),
            (HostError::NoAdapter("none".into()), Recovery::Fallback),
            (
                HostError::AdapterRejected {
                    adapter: "RADV".into(),
                    reason: "dmabuf import".into(),
                },
                Recovery::Fallback,
            ),
            (HostError::Surface("gl".into()), Recovery::Fallback),
            (
                HostError::Unsupported {
                    global: "ext_background_effect_manager_v1",
                    feature: "blur",
                },
                Recovery::Degrade,
            ),
            (HostError::NoSurface(SurfaceId(9)), Recovery::Degrade),
            (HostError::HostGone, Recovery::Degrade),
            (HostError::Render("timeout".into()), Recovery::Retry),
            (
                HostError::Io(std::io::ErrorKind::TimedOut.into()),
                Recovery::Retry,
            ),
        ];
        for (error, want) in cases {
            assert_eq!(error.recovery(), want, "{error}");
        }
    }
}

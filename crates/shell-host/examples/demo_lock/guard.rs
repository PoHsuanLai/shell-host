//! The demo's refusal to lock anything but the throwaway compositor `scripts/dev/accept-lock.sh`
//! starts. Locking the user's own session would lock them out while nobody is there to type the
//! password, so the demo locks only when every rule below holds, checked once before it
//! connects (the connection and the check read the same environment):
//!
//! - `SHELL_HOST_ALLOW_LOCK` is exactly `virtual`;
//! - `WAYLAND_SOCKET` is unset (it would win over `WAYLAND_DISPLAY` at connect);
//! - `WAYLAND_DISPLAY` is an absolute path to a socket;
//! - `SHELL_HOST_LOCK_RUNTIME` names the virtual compositor's runtime directory (the one the
//!   script prints), which holds the script's marker file [`MARKER`];
//! - that directory is neither this process's `XDG_RUNTIME_DIR` nor the login session's
//!   `/run/user/<uid>` (run the demo with the session's own `XDG_RUNTIME_DIR`);
//! - the socket's directory is that directory, after resolving links;
//! - the socket is not called `wayland-0`.

use std::path::{Path, PathBuf};

/// The file `accept-lock.sh` writes into the runtime directory it made for the virtual
/// compositor.
pub const MARKER: &str = ".shell-host-virtual-compositor";

/// The environment the rules read, with the paths resolved (`None`: unset or unresolvable).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockEnv {
    pub allow: Option<String>,
    pub socket_fd: Option<String>,
    /// `WAYLAND_DISPLAY` as given.
    pub display: Option<String>,
    /// `WAYLAND_DISPLAY` resolved, when it names an existing file.
    pub display_resolved: Option<PathBuf>,
    /// The resolved display path is a socket.
    pub display_is_socket: bool,
    /// `SHELL_HOST_LOCK_RUNTIME` resolved.
    pub runtime: Option<PathBuf>,
    /// The runtime dir holds the virtual compositor's marker file.
    pub marker: bool,
    /// `XDG_RUNTIME_DIR` resolved.
    pub session_runtime: Option<PathBuf>,
    /// `/run/user/<uid>` resolved.
    pub login_runtime: Option<PathBuf>,
}

/// May the demo lock this compositor: the rules, in order; the first that fails is the reason.
/// Pure.
pub fn permit(env: &LockEnv) -> Result<(), String> {
    if env.allow.as_deref() != Some("virtual") {
        return Err("SHELL_HOST_ALLOW_LOCK is not \"virtual\"".to_owned());
    }
    if env.socket_fd.is_some() {
        return Err("WAYLAND_SOCKET is set".to_owned());
    }
    let display = env.display.as_deref().ok_or("WAYLAND_DISPLAY is unset")?;
    if !Path::new(display).is_absolute() {
        return Err(format!(
            "WAYLAND_DISPLAY {display:?} is not an absolute path"
        ));
    }
    let socket = env
        .display_resolved
        .as_deref()
        .filter(|_| env.display_is_socket)
        .ok_or_else(|| format!("WAYLAND_DISPLAY {display:?} is not a socket"))?;
    let runtime = env
        .runtime
        .as_deref()
        .ok_or("SHELL_HOST_LOCK_RUNTIME is unset or missing")?;
    if !env.marker {
        return Err(format!("{} has no {MARKER}", runtime.display()));
    }
    if env.session_runtime.as_deref() == Some(runtime) {
        return Err("SHELL_HOST_LOCK_RUNTIME is the session's XDG_RUNTIME_DIR".to_owned());
    }
    if env.login_runtime.as_deref() == Some(runtime) {
        return Err("SHELL_HOST_LOCK_RUNTIME is the login session's /run/user/<uid>".to_owned());
    }
    if socket.parent() != Some(runtime) {
        return Err(format!(
            "{} is not in {}",
            socket.display(),
            runtime.display()
        ));
    }
    if socket.file_name().and_then(|name| name.to_str()) == Some("wayland-0") {
        return Err("the socket is wayland-0".to_owned());
    }
    Ok(())
}

/// The process environment, resolved.
pub fn from_process() -> LockEnv {
    let var = |name| std::env::var(name).ok();
    let resolve = |path: Option<String>| path.and_then(|p| std::fs::canonicalize(p).ok());
    let display = var("WAYLAND_DISPLAY");
    let display_resolved = resolve(display.clone());
    let display_is_socket = display_resolved.as_deref().is_some_and(is_socket);
    let runtime = resolve(var("SHELL_HOST_LOCK_RUNTIME"));
    let marker = runtime
        .as_deref()
        .is_some_and(|dir| dir.join(MARKER).is_file());
    LockEnv {
        allow: var("SHELL_HOST_ALLOW_LOCK"),
        socket_fd: var("WAYLAND_SOCKET"),
        display,
        display_resolved,
        display_is_socket,
        runtime,
        marker,
        session_runtime: resolve(var("XDG_RUNTIME_DIR")),
        login_runtime: login_runtime(),
    }
}

/// `/run/user/<uid>`, the uid read from this process's own `/proc` entry.
fn login_runtime() -> Option<PathBuf> {
    use std::os::unix::fs::MetadataExt;
    let uid = std::fs::metadata("/proc/self").ok()?.uid();
    std::fs::canonicalize(format!("/run/user/{uid}")).ok()
}

fn is_socket(path: &Path) -> bool {
    use std::os::unix::fs::FileTypeExt;
    std::fs::metadata(path).is_ok_and(|meta| meta.file_type().is_socket())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good() -> LockEnv {
        LockEnv {
            allow: Some("virtual".to_owned()),
            socket_fd: None,
            display: Some("/run/user/1000/shell-host-lock-7/wayland-1".to_owned()),
            display_resolved: Some(PathBuf::from("/run/user/1000/shell-host-lock-7/wayland-1")),
            display_is_socket: true,
            runtime: Some(PathBuf::from("/run/user/1000/shell-host-lock-7")),
            marker: true,
            session_runtime: Some(PathBuf::from("/home/u/.cache/runtime")),
            login_runtime: Some(PathBuf::from("/run/user/1000")),
        }
    }

    #[test]
    fn only_the_virtual_compositors_socket_may_be_locked() {
        type Change = fn(&mut LockEnv);
        let cases: &[(&str, Change, &str)] = &[
            ("all rules hold", |_| {}, ""),
            (
                "no permission",
                |env| env.allow = None,
                "SHELL_HOST_ALLOW_LOCK",
            ),
            (
                "another permission word",
                |env| env.allow = Some("1".to_owned()),
                "SHELL_HOST_ALLOW_LOCK",
            ),
            (
                "WAYLAND_SOCKET set",
                |env| env.socket_fd = Some("3".to_owned()),
                "WAYLAND_SOCKET",
            ),
            (
                "the session's display by name",
                |env| env.display = Some("wayland-0".to_owned()),
                "absolute",
            ),
            ("no display", |env| env.display = None, "unset"),
            (
                "not a socket",
                |env| env.display_is_socket = false,
                "not a socket",
            ),
            (
                "no runtime dir",
                |env| env.runtime = None,
                "SHELL_HOST_LOCK_RUNTIME",
            ),
            ("no marker", |env| env.marker = false, "has no"),
            (
                "the runtime dir is the login session's",
                |env| {
                    env.runtime = Some(PathBuf::from("/run/user/1000"));
                    env.display_resolved = Some(PathBuf::from("/run/user/1000/wayland-1"));
                },
                "/run/user/<uid>",
            ),
            (
                "the runtime dir is this process's XDG_RUNTIME_DIR",
                |env| env.session_runtime = env.runtime.clone(),
                "session's XDG_RUNTIME_DIR",
            ),
            (
                "a socket outside the runtime dir (a link resolved elsewhere)",
                |env| env.display_resolved = Some(PathBuf::from("/run/user/1000/wayland-0")),
                "is not in",
            ),
            (
                "wayland-0 even inside the runtime dir",
                |env| {
                    env.display_resolved =
                        Some(PathBuf::from("/run/user/1000/shell-host-lock-7/wayland-0"));
                },
                "wayland-0",
            ),
        ];
        for (name, change, reason) in cases {
            let mut env = good();
            change(&mut env);
            match (permit(&env), reason.is_empty()) {
                (Ok(()), true) => {}
                (Err(got), false) => {
                    assert!(got.contains(reason), "{name}: {got:?} lacks {reason:?}");
                }
                (got, _) => panic!("{name}: {got:?}"),
            }
        }
    }
}

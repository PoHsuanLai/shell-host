//! Which Wayland connection a host talks over.
//!
//! The default is the session's own (`WAYLAND_DISPLAY`). A compositor that hands a client a
//! private connection (casement's `Compositor1.Connect("shell")`) gives the shell an fd
//! instead, and the host uses that one. The route is read in exactly one place,
//! `Host::connect`.
//!
//! See FINDINGS p2-z-sh.

use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::net::UnixStream;

use wayland_client::Connection;

use crate::error::{HostError, Result};

/// How `Host::connect` reaches the compositor.
#[derive(Debug, Default)]
pub enum WaylandRoute {
    /// `WAYLAND_DISPLAY` / `XDG_RUNTIME_DIR`, as every client does.
    #[default]
    Environment,
    /// An already-connected socket the compositor handed over; the host owns it from here.
    Handed(OwnedFd),
}

/// A duplicate of the socket. A route is configuration and `HostConfig` is `Clone`; the
/// duplicate is another descriptor of the same connection. When the process is out of
/// descriptors the copy falls back to `Environment` rather than panicking.
impl Clone for WaylandRoute {
    fn clone(&self) -> WaylandRoute {
        match self {
            WaylandRoute::Environment => WaylandRoute::Environment,
            WaylandRoute::Handed(fd) => fd
                .try_clone()
                .map_or(WaylandRoute::Environment, WaylandRoute::Handed),
        }
    }
}

/// Equal when both are the environment, or both hold the same descriptor number.
impl PartialEq for WaylandRoute {
    fn eq(&self, other: &WaylandRoute) -> bool {
        match (self, other) {
            (WaylandRoute::Environment, WaylandRoute::Environment) => true,
            (WaylandRoute::Handed(a), WaylandRoute::Handed(b)) => a.as_raw_fd() == b.as_raw_fd(),
            (WaylandRoute::Environment, WaylandRoute::Handed(_))
            | (WaylandRoute::Handed(_), WaylandRoute::Environment) => false,
        }
    }
}

impl Eq for WaylandRoute {}

impl WaylandRoute {
    /// The connection this route names.
    pub(crate) fn connect(self) -> Result<Connection> {
        match self {
            WaylandRoute::Environment => Connection::connect_to_env(),
            WaylandRoute::Handed(fd) => Connection::from_socket(UnixStream::from(fd)),
        }
        .map_err(|e| HostError::Connect(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_route_is_the_environment() {
        assert_eq!(WaylandRoute::default(), WaylandRoute::Environment);
    }

    #[test]
    fn a_handed_socket_connects_without_the_environment() {
        let (ours, _compositor) = UnixStream::pair().expect("socketpair");
        let route = WaylandRoute::Handed(OwnedFd::from(ours));
        assert!(route.connect().is_ok());
    }

    #[test]
    fn a_clone_of_a_handed_route_is_another_descriptor_of_the_same_socket() {
        let (ours, _compositor) = UnixStream::pair().expect("socketpair");
        let route = WaylandRoute::Handed(OwnedFd::from(ours));
        let copy = route.clone();
        assert!(matches!(copy, WaylandRoute::Handed(_)));
        assert_ne!(copy, route, "a distinct descriptor number");
    }

    #[test]
    fn routes_of_different_kinds_differ() {
        let (ours, _compositor) = UnixStream::pair().expect("socketpair");
        assert_ne!(
            WaylandRoute::Handed(OwnedFd::from(ours)),
            WaylandRoute::Environment
        );
    }
}

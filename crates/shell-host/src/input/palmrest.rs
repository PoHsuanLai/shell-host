//! The palmrest client (design/12 §12.3.10-11, design/11 §11.7): connects to
//! `$XDG_RUNTIME_DIR/palmrest/events.sock` (`SOCK_SEQPACKET`) when the daemon runs, tells it when
//! the pointer is over one of our surfaces (`PointerOver`) and whether the session is locked
//! (`SessionLock`), and reads its scroll, touch, tap and
//! swipe events for the engine (`input::scroll`). No daemon, no socket: nothing happens, and
//! the host looks again after a growing wait (`backoff`) and at the next pointer enter.
//!
//! The socket I/O is here; registering the reading side with the event loop is the context's
//! (`ctx::palmrest`), since the loop belongs to `HostCtx`.
//!
//! See FINDINGS F64.

mod backoff;
pub(crate) mod route;

use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::time::Duration;

use rustix::net::{
    AddressFamily, RecvFlags, SendFlags, SocketAddrUnix, SocketFlags, SocketType, connect, recv,
    send, socket_with, sockopt,
};
use rustix::process::getuid;

use palmrest_protocol::{
    Compat, LockState, MAX_PACKET, Over, ToDaemon, ToHost, compat, decode, encode,
};

use crate::lock::machine::LockState as HostLock;
use backoff::Misses;

/// The most datagrams one read takes: a daemon (or a process in its place) that sends faster
/// than the host reads cannot hold the loop in one read. The source is level-triggered, so the
/// loop comes back for the rest after its other sources have had their turn.
const MAX_DATAGRAMS_PER_READ: usize = 256;

/// Whether the host trusts the process at the other end of the socket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Trust {
    Trusted,
    Refused,
}

/// The daemon runs as the person (a user service) or as root; any other user's process at the
/// socket's path is not palmrest, and would be feeding scroll and touch input.
pub(crate) fn trust(peer_uid: u32, own_uid: u32) -> Trust {
    if peer_uid == own_uid || peer_uid == 0 {
        Trust::Trusted
    } else {
        Trust::Refused
    }
}

/// Whether the host should try to connect at the next flush.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Retry {
    Due,
    Waiting,
}

/// Whether the host goes on reading after a read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stream {
    Open,
    /// The daemon's end closed.
    Closed,
    /// The daemon said `Hello` with another protocol version: the host disconnects.
    Incompatible {
        version: u16,
    },
}

#[derive(Debug)]
enum Link {
    Down,
    /// Connected; this descriptor sends (the event loop owns a duplicate for reading).
    Up(OwnedFd),
}

/// The host's connection to palmrest.
#[derive(Debug)]
pub(crate) struct Palmrest {
    path: Option<PathBuf>,
    link: Link,
    /// Where the pointer is, sent on every change and again after each connect.
    over: Over,
    /// Whether the session is locked, sent on every change and again after each connect: palmrest
    /// stops gesture actions while any connection says `Locked`.
    lock: LockState,
    retry: Retry,
    /// The attempts in a row that found no usable daemon; the next wait grows with it.
    misses: Misses,
}

/// What palmrest is told for the host's lock report: only `Locked` locks. `Unlocked`, and
/// `Finished` (the compositor ended the lock), leave the session unlocked.
pub(crate) fn lock_for(state: HostLock) -> LockState {
    match state {
        HostLock::Locked => LockState::Locked,
        HostLock::Unlocked | HostLock::Finished => LockState::Unlocked,
    }
}

/// `$XDG_RUNTIME_DIR/palmrest/events.sock` for `runtime_dir`.
pub(crate) fn socket_path(runtime_dir: Option<&Path>) -> Option<PathBuf> {
    runtime_dir.map(|dir| dir.join("palmrest").join("events.sock"))
}

impl Palmrest {
    /// The client for the socket under `$XDG_RUNTIME_DIR`; unconnected, first attempt due.
    pub(crate) fn from_env() -> Palmrest {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
        Palmrest::at(socket_path(runtime.as_deref()))
    }

    /// The client for the socket at `path` (`None`: never connects).
    pub(crate) fn at(path: Option<PathBuf>) -> Palmrest {
        Palmrest {
            path,
            link: Link::Down,
            over: Over::NotOurs,
            lock: LockState::Unlocked,
            retry: Retry::Due,
            misses: Misses::default(),
        }
    }

    /// Whether a connection attempt is due: at start, after a pointer enter while down, when the
    /// session locks while down, and when the wait of [`Palmrest::retry_in`] has run out.
    pub(crate) fn retry(&self) -> Retry {
        match self.link {
            Link::Up(_) => Retry::Waiting,
            Link::Down => self.retry,
        }
    }

    /// How long to wait before the next attempt, after one that found no daemon (or after the
    /// daemon went away): the host makes the attempt due then with [`Palmrest::retry_now`], so a
    /// daemon that starts or restarts is found without a pointer enter. `None` when there is no
    /// socket path to look at, or the daemon is connected.
    pub(crate) fn retry_in(&self) -> Option<Duration> {
        match (&self.link, &self.path) {
            (Link::Down, Some(_)) => Some(self.misses.wait(self.lock)),
            (Link::Up(_), _) | (Link::Down, None) => None,
        }
    }

    /// The wait of [`Palmrest::retry_in`] ran out: an attempt is due, if still unconnected.
    pub(crate) fn retry_now(&mut self) {
        if matches!(self.link, Link::Down) {
            self.retry = Retry::Due;
        }
    }

    /// The daemon spoke: it is there and means to stay, so the next loss is waited for from the
    /// shortest wait again.
    pub(crate) fn settled(&mut self) {
        self.misses = Misses::default();
    }

    /// Connect if the socket exists: returns the descriptor the event loop reads from. Sends
    /// the session's `SessionLock` first and then the pointer's current `PointerOver` at once, so
    /// the daemon stops gesture actions before anything else it hears. `None` (and no attempt
    /// before [`Palmrest::retry_in`] has passed, or the next pointer enter) when there is no
    /// usable daemon.
    pub(crate) fn connect(&mut self) -> Option<OwnedFd> {
        self.retry = Retry::Waiting;
        let reader = self.attempt();
        if reader.is_none() {
            self.misses = self.misses.more();
        }
        reader
    }

    fn attempt(&mut self) -> Option<OwnedFd> {
        let path = self.path.as_ref()?;
        if !path.exists() {
            return None;
        }
        let fd = open(path).ok()?;
        if peer_trust(&fd) == Trust::Refused {
            eprintln!("shell-host: the palmrest socket is not served by this user; ignoring it");
            return None;
        }
        let reader = fd.try_clone().ok()?;
        self.link = Link::Up(fd);
        self.send(&ToDaemon::SessionLock { state: self.lock });
        self.send(&ToDaemon::PointerOver { over: self.over });
        Some(reader)
    }

    /// The daemon went away (the read side saw the end, or it spoke another protocol): a miss,
    /// and the next attempt waits for [`Palmrest::retry_in`].
    pub(crate) fn disconnected(&mut self) {
        self.link = Link::Down;
        self.misses = self.misses.more();
    }

    /// The pointer entered or left our surfaces. While down, an enter makes a connection
    /// attempt due.
    pub(crate) fn pointer_over(&mut self, over: Over) {
        let changed = self.over != over;
        self.over = over;
        match (&self.link, over) {
            (Link::Down, Over::Ours) => self.retry = Retry::Due,
            (Link::Down, Over::NotOurs) => {}
            (Link::Up(_), _) if changed => self.send(&ToDaemon::PointerOver { over }),
            (Link::Up(_), _) => {}
        }
    }

    /// The session was locked or unlocked. Sent on a change only; while down, the state is kept
    /// and `connect` tells it, and a lock makes an attempt due at once: the pointer stays inside
    /// the lock surface, so no enter will.
    pub(crate) fn session_lock(&mut self, lock: LockState) {
        if self.lock == lock {
            return;
        }
        self.lock = lock;
        match (&self.link, lock) {
            (Link::Down, LockState::Locked) => self.retry = Retry::Due,
            (Link::Down, LockState::Unlocked) => {}
            (Link::Up(_), _) => self.send(&ToDaemon::SessionLock { state: lock }),
        }
    }

    fn send(&mut self, message: &ToDaemon) {
        let Link::Up(fd) = &self.link else {
            return;
        };
        match send(
            fd,
            &encode(message),
            SendFlags::NOSIGNAL | SendFlags::DONTWAIT,
        ) {
            Ok(_) => {}
            // A full socket drops one message; the next change resends the state.
            Err(e) if e == rustix::io::Errno::AGAIN => {}
            Err(_) => self.link = Link::Down,
        }
    }
}

/// Whether the process that listens on `fd` is one to trust. Fails closed: a peer whose
/// credentials cannot be read is refused.
fn peer_trust(fd: &OwnedFd) -> Trust {
    match sockopt::socket_peercred(fd) {
        Ok(peer) => trust(peer.uid.as_raw(), getuid().as_raw()),
        Err(_) => Trust::Refused,
    }
}

/// A non-blocking, close-on-exec `SOCK_SEQPACKET` connection to `path`.
pub(crate) fn open(path: &Path) -> io::Result<OwnedFd> {
    let fd = socket_with(
        AddressFamily::UNIX,
        SocketType::SEQPACKET,
        SocketFlags::CLOEXEC | SocketFlags::NONBLOCK,
        None,
    )?;
    connect(&fd, &SocketAddrUnix::new(path)?)?;
    Ok(fd)
}

/// The messages waiting on `fd` (at most [`MAX_DATAGRAMS_PER_READ`] datagrams' worth), and
/// whether to go on reading. A `Hello` of another protocol version ends the read there (nothing
/// after it is understood); a packet over `MAX_PACKET` is truncated by the kernel and skipped.
pub(crate) fn read_all(fd: impl AsFd) -> (Vec<ToHost>, Stream) {
    read_up_to(fd, MAX_DATAGRAMS_PER_READ)
}

fn read_up_to(fd: impl AsFd, datagrams: usize) -> (Vec<ToHost>, Stream) {
    let mut messages = Vec::new();
    let mut buf = vec![0u8; MAX_PACKET];
    for _ in 0..datagrams {
        match recv(fd.as_fd(), &mut buf[..], RecvFlags::DONTWAIT) {
            Ok((_, 0)) => return (messages, Stream::Closed),
            Ok((len, full)) if full <= len => {
                let Ok(message) = decode::<ToHost>(&buf[..len]) else {
                    continue;
                };
                if let Compat::Incompatible { version } = compat(&message) {
                    return (messages, Stream::Incompatible { version });
                }
                messages.push(message);
            }
            // Truncated: not a message of the protocol.
            Ok(_) => {}
            Err(e) if e == rustix::io::Errno::AGAIN => return (messages, Stream::Open),
            Err(e) if e == rustix::io::Errno::INTR => {}
            Err(_) => return (messages, Stream::Closed),
        }
    }
    (messages, Stream::Open)
}

#[cfg(test)]
mod tests;

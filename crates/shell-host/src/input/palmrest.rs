//! The palmrest client (design/12 §12.3.10-11, design/11 §11.7): connects to
//! `/run/palmrest/events.sock` (`SOCK_SEQPACKET`) when the daemon runs, tells it when the pointer
//! is over one of our surfaces (`PointerOver`), whether the session is locked (`SessionLock`) and
//! what the person's gesture settings say (`UserConfig`, `user_config`), and reads its scroll,
//! touch, tap and swipe events for the engine (`input::scroll`) and the actions it asks of the
//! session (`Run`). The daemon is a system service, so the peer is believed by its control group
//! (`peer_trust`). No daemon, no socket: nothing happens, and the host looks again after a
//! growing wait (`backoff`) and at the next pointer enter.
//!
//! The socket I/O is here; registering the reading side with the event loop is the context's
//! (`ctx::palmrest`), since the loop belongs to `HostCtx`.
//!
//! See FINDINGS F64.

mod backoff;
pub(crate) mod route;
mod user_config;

use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::time::Duration;

use nix::errno::Errno;
use nix::sys::socket::{getsockopt, sockopt::PeerPidfd};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::net::{
    AddressFamily, RecvFlags, SendFlags, SocketAddrUnix, SocketFlags, SocketType, connect, recv,
    send, socket_with, sockopt,
};
use rustix::process::{PidfdFlags, pidfd_open};

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

/// The daemon's socket. The daemon is a system service, so this is a fixed path and not one under
/// the person's runtime folder.
pub(crate) const SOCKET: &str = "/run/palmrest/events.sock";

/// The control group of palmrest's unit: only the system manager can put a process there, so a
/// process in it is palmrest, whoever it runs as. Any other process at the socket's path would be
/// feeding scroll and touch input, and asking the shell to run programs.
const DAEMON_CGROUP: &str = "/system.slice/palmrest.service";

/// Whether a process whose `/proc/<pid>/cgroup` reads `text` is the daemon: the unified
/// hierarchy's line `0::<path>` names the unit's group, exactly.
pub(crate) fn trust_of_cgroup(text: &str) -> Trust {
    if text
        .lines()
        .any(|line| line.strip_prefix("0::") == Some(DAEMON_CGROUP))
    {
        Trust::Trusted
    } else {
        Trust::Refused
    }
}

/// How a peer is judged. Only the control group in production; a test's daemon is a thread of the
/// test, in no unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Check {
    Unit,
    #[cfg(test)]
    Anyone,
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
    check: Check,
    /// The person's settings file, when there is a place for one, and how it looked when its text
    /// was last sent.
    settings: Option<Settings>,
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

/// The person's settings file and how it looked when its text was last sent.
#[derive(Debug)]
struct Settings {
    path: PathBuf,
    told: Option<user_config::Stamp>,
}

impl Palmrest {
    /// The client for the daemon's socket and the person's settings file; unconnected, first
    /// attempt due.
    pub(crate) fn system() -> Palmrest {
        let settings = user_config::path(|key| std::env::var_os(key))
            .map(|path| Settings { path, told: None });
        Palmrest {
            settings,
            ..Palmrest::at(Some(PathBuf::from(SOCKET)))
        }
    }

    /// The client for the socket at `path` (`None`: never connects); sends no settings.
    pub(crate) fn at(path: Option<PathBuf>) -> Palmrest {
        Palmrest {
            path,
            check: Check::Unit,
            settings: None,
            link: Link::Down,
            over: Over::NotOurs,
            lock: LockState::Unlocked,
            retry: Retry::Due,
            misses: Misses::default(),
        }
    }

    /// A client for tests: believes whatever listens at `path`, which is a thread of the test
    /// and in no unit; reads the person's settings from `settings` when given.
    #[cfg(test)]
    pub(crate) fn trusting(path: PathBuf, settings: Option<PathBuf>) -> Palmrest {
        Palmrest {
            check: Check::Anyone,
            settings: settings.map(|path| Settings { path, told: None }),
            ..Palmrest::at(Some(path))
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
        if self.judge(&fd) == Trust::Refused {
            eprintln!(
                "shell-host: the palmrest socket is not served by palmrest's unit; ignoring it"
            );
            return None;
        }
        let reader = fd.try_clone().ok()?;
        self.link = Link::Up(fd);
        self.send(&ToDaemon::SessionLock { state: self.lock });
        self.send(&ToDaemon::PointerOver { over: self.over });
        self.tell_settings();
        Some(reader)
    }

    fn judge(&self, fd: &OwnedFd) -> Trust {
        match self.check {
            Check::Unit => peer_trust(fd),
            #[cfg(test)]
            Check::Anyone => Trust::Trusted,
        }
    }

    /// Whether a place for the person's settings is known, so that [`Palmrest::settings_changed`]
    /// is worth asking every [`user_config::LOOK_EVERY`].
    pub(crate) fn watches_settings(&self) -> bool {
        self.settings.is_some()
    }

    /// How often the settings file is looked at.
    pub(crate) fn look_every() -> Duration {
        user_config::LOOK_EVERY
    }

    /// Looks at the settings file and tells the daemon its text when it changed since the last
    /// time. Nothing while the daemon is not connected: the connect tells it.
    pub(crate) fn settings_changed(&mut self) {
        let Some(Settings { path, told }) = &self.settings else {
            return;
        };
        if matches!(self.link, Link::Up(_)) && told.as_ref() != Some(&user_config::stamp(path)) {
            self.tell_settings();
        }
    }

    /// Sends the settings file's text (empty: none), and remembers how the file looked.
    fn tell_settings(&mut self) {
        let Some(Settings { path, .. }) = &self.settings else {
            return;
        };
        // The stamp is taken before the read, so a change in between is seen at the next look.
        let stamp = user_config::stamp(path);
        let toml = user_config::read(path);
        let delivery = self.send(&ToDaemon::UserConfig { toml });
        if let (Delivery::Sent, Some(settings)) = (delivery, &mut self.settings) {
            settings.told = Some(stamp);
        }
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
            (Link::Up(_), _) if changed => {
                self.send(&ToDaemon::PointerOver { over });
            }
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
            (Link::Up(_), _) => {
                self.send(&ToDaemon::SessionLock { state: lock });
            }
        }
    }

    fn send(&mut self, message: &ToDaemon) -> Delivery {
        let Link::Up(fd) = &self.link else {
            return Delivery::Dropped;
        };
        match send(
            fd,
            &encode(message),
            SendFlags::NOSIGNAL | SendFlags::DONTWAIT,
        ) {
            Ok(_) => Delivery::Sent,
            // A full socket drops one message; the next change resends the state.
            Err(e) if e == rustix::io::Errno::AGAIN => Delivery::Dropped,
            Err(_) => {
                self.link = Link::Down;
                Delivery::Dropped
            }
        }
    }
}

/// Whether a message went into the socket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Delivery {
    Sent,
    Dropped,
}

/// Whether the process that listens on `fd` is palmrest's unit. The process is held by a pidfd,
/// its control group is read from `/proc`, and it must still be running afterwards, so the pid
/// cannot have been another process's when the group was read. Fails closed: anything that cannot
/// be read is refused.
fn peer_trust(fd: &OwnedFd) -> Trust {
    let Some(held) = held_peer(fd) else {
        return Trust::Refused;
    };
    let Ok(text) = std::fs::read_to_string(format!("/proc/{}/cgroup", held.pid)) else {
        return Trust::Refused;
    };
    match still_running(&held.fd) {
        Running::Yes => trust_of_cgroup(&text),
        Running::No => Trust::Refused,
    }
}

/// A process held by a pidfd, with the pid the socket names for it.
struct Held {
    pid: i32,
    fd: OwnedFd,
}

/// Whether a held process has not exited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Running {
    Yes,
    No,
}

/// A pidfd polls readable once its process has exited.
fn still_running(held: &OwnedFd) -> Running {
    let mut watch = [PollFd::new(held, PollFlags::IN)];
    match poll(&mut watch, Some(&Timespec::default())) {
        Ok(0) => Running::Yes,
        _ => Running::No,
    }
}

/// The process on the other end of `socket`, held. The kernel's own pidfd of the peer, made when
/// it connected (`SO_PEERPIDFD`, Linux 6.5), cannot name a process that took the pid later: a peer
/// that has exited is refused. A kernel without the option gets a pidfd opened by the pid from
/// `SO_PEERCRED`, kept only if the credentials read again are the same.
fn held_peer(socket: &impl AsFd) -> Option<Held> {
    let first = sockopt::socket_peercred(socket).ok()?;
    let pid = first.pid.as_raw_pid();
    let fd = match getsockopt(socket, PeerPidfd) {
        Ok(fd) => fd,
        Err(Errno::ESRCH) => return None,
        Err(_) => {
            let fd = pidfd_open(first.pid, PidfdFlags::empty()).ok()?;
            (sockopt::socket_peercred(socket).ok()? == first).then_some(fd)?
        }
    };
    Some(Held { pid, fd })
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

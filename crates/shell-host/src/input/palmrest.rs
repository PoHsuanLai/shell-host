//! The palmrest client (design/12 §12.3.10-11, design/11 §11.7): connects to
//! `$XDG_RUNTIME_DIR/palmrest/events.sock` (`SOCK_SEQPACKET`) when the daemon runs, tells it when
//! the pointer is over one of our surfaces (`PointerOver`) and whether the session is locked
//! (`SessionLock`), and reads its scroll, touch, tap and
//! swipe events for the engine (`input::scroll`). No daemon, no socket: nothing happens, and
//! the host tries again on the next pointer enter.
//!
//! The socket I/O is here; registering the reading side with the event loop is the context's
//! (`ctx::palmrest`), since the loop belongs to `HostCtx`.
//!
//! See FINDINGS F64.

pub(crate) mod route;

use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::path::{Path, PathBuf};

use rustix::net::{
    AddressFamily, RecvFlags, SendFlags, SocketAddrUnix, SocketFlags, SocketType, connect, recv,
    send, socket_with, sockopt,
};
use rustix::process::getuid;

use palmrest_protocol::{
    Compat, LockState, MAX_PACKET, Over, ToDaemon, ToHost, compat, decode, encode,
};

use crate::lock::machine::LockState as HostLock;

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
        }
    }

    /// Whether a connection attempt is due (at start, and after a pointer enter while down).
    pub(crate) fn retry(&self) -> Retry {
        match self.link {
            Link::Up(_) => Retry::Waiting,
            Link::Down => self.retry,
        }
    }

    /// Connect if the socket exists: returns the descriptor the event loop reads from. Sends
    /// the pointer's current `PointerOver` and the session's `SessionLock` at once. `None` (and no retry until the next enter)
    /// when there is no daemon.
    pub(crate) fn connect(&mut self) -> Option<OwnedFd> {
        self.retry = Retry::Waiting;
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
        self.send(&ToDaemon::PointerOver { over: self.over });
        self.send(&ToDaemon::SessionLock { state: self.lock });
        Some(reader)
    }

    /// The daemon went away (the read side saw the end).
    pub(crate) fn disconnected(&mut self) {
        self.link = Link::Down;
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
    /// and `connect` tells it.
    pub(crate) fn session_lock(&mut self, lock: LockState) {
        if self.lock != lock {
            self.lock = lock;
            self.send(&ToDaemon::SessionLock { state: lock });
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
mod tests {
    use super::*;
    use palmrest_protocol::{ActiveTouch, Phase};
    use rustix::net::{accept, bind, listen};

    /// A fake daemon: a listening `SOCK_SEQPACKET` socket at a temp path.
    fn daemon() -> (tempdir::Dir, OwnedFd) {
        let dir = tempdir::Dir::new();
        let path = socket_path(Some(dir.path())).expect("a path");
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        let fd = socket_with(
            AddressFamily::UNIX,
            SocketType::SEQPACKET,
            SocketFlags::CLOEXEC,
            None,
        )
        .expect("socket");
        bind(&fd, &SocketAddrUnix::new(&path).expect("addr")).expect("bind");
        listen(&fd, 4).expect("listen");
        (dir, fd)
    }

    /// A private temp directory, removed on drop (no dependency for it).
    mod tempdir {
        use std::path::{Path, PathBuf};

        pub(crate) struct Dir(PathBuf);

        impl Dir {
            pub(crate) fn new() -> Dir {
                static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let path = std::env::temp_dir()
                    .join(format!("shell-host-palmrest-{}-{n}", std::process::id()));
                std::fs::create_dir_all(&path).expect("temp dir");
                Dir(path)
            }

            pub(crate) fn path(&self) -> &Path {
                &self.0
            }
        }

        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    fn received(fd: &OwnedFd) -> String {
        let mut buf = [0u8; 256];
        let (len, _) = recv(fd, &mut buf[..], RecvFlags::empty()).expect("recv");
        String::from_utf8_lossy(&buf[..len]).into_owned()
    }

    #[test]
    fn no_daemon_is_a_no_op() {
        let dir = tempdir::Dir::new();
        let mut client = Palmrest::at(socket_path(Some(dir.path())));
        assert_eq!(client.retry(), Retry::Due);
        assert!(client.connect().is_none());
        assert_eq!(client.retry(), Retry::Waiting, "no retry storm");
        client.pointer_over(Over::Ours);
        assert_eq!(client.retry(), Retry::Due, "an enter tries again");
        assert!(Palmrest::at(None).connect().is_none());
    }

    #[test]
    fn a_fake_daemon_hears_pointer_over_and_its_events_are_read() {
        let (dir, listener) = daemon();
        let mut client = Palmrest::at(socket_path(Some(dir.path())));
        client.pointer_over(Over::Ours);
        let reader = client.connect().expect("connects");
        let peer = accept(&listener).expect("accept");
        assert_eq!(
            received(&peer),
            r#"{"kind":"pointer_over","v":{"over":"ours"}}"#,
            "state on connect"
        );
        assert_eq!(
            received(&peer),
            r#"{"kind":"session_lock","v":{"state":"unlocked"}}"#,
            "the lock state on connect"
        );
        client.pointer_over(Over::Ours);
        client.pointer_over(Over::NotOurs);
        assert_eq!(
            received(&peer),
            r#"{"kind":"pointer_over","v":{"over":"not_ours"}}"#,
            "only changes"
        );

        let hello = r#"{"kind":"hello","v":{"version":1,"device":{"name":"m","path":"/dev/hidraw0","vendor":76,"product":617}}}"#;
        let scroll = r#"{"kind":"scroll","v":{"gesture":1,"phase":"began","dx_px":0.0,"dy_px":0.0,"fingers":1,"axis":"y","t_ns":5}}"#;
        let touches = r#"{"kind":"touches","v":{"active":"down"}}"#;
        for datagram in [hello, touches, "not json", scroll] {
            send(&peer, datagram.as_bytes(), SendFlags::empty()).expect("send");
        }
        let (messages, stream) = read_all(&reader);
        assert_eq!(stream, Stream::Open);
        assert_eq!(
            messages.len(),
            3,
            "the junk datagram is skipped: {messages:?}"
        );
        assert!(matches!(messages[0], ToHost::Hello { version: 1, .. }));
        assert_eq!(
            messages[1],
            ToHost::Touches {
                active: ActiveTouch::Down
            }
        );
        assert!(matches!(
            messages[2],
            ToHost::Scroll {
                phase: Phase::Began,
                ..
            }
        ));

        drop(peer);
        let (messages, stream) = read_all(&reader);
        assert_eq!((messages.len(), stream), (0, Stream::Closed));
    }

    #[test]
    fn the_lock_is_sent_on_change_and_again_after_a_reconnect() {
        let (dir, listener) = daemon();
        let mut client = Palmrest::at(socket_path(Some(dir.path())));
        client.session_lock(LockState::Locked);
        let _reader = client.connect().expect("connects");
        let peer = accept(&listener).expect("accept");
        assert_eq!(
            received(&peer),
            r#"{"kind":"pointer_over","v":{"over":"not_ours"}}"#
        );
        let locked = r#"{"kind":"session_lock","v":{"state":"locked"}}"#;
        assert_eq!(
            received(&peer),
            locked,
            "a lock held before the connect is told at it"
        );

        client.session_lock(LockState::Locked);
        client.session_lock(LockState::Unlocked);
        assert_eq!(
            received(&peer),
            r#"{"kind":"session_lock","v":{"state":"unlocked"}}"#,
            "only changes are sent"
        );

        client.session_lock(LockState::Locked);
        assert_eq!(received(&peer), locked);
        client.disconnected();
        let _again = client.connect().expect("reconnects");
        let second = accept(&listener).expect("accept");
        received(&second);
        assert_eq!(received(&second), locked, "a reconnect tells it again");
    }

    #[test]
    fn only_locked_reports_lock_the_session() {
        let cases: &[(&str, HostLock, LockState)] = &[
            ("the compositor locked", HostLock::Locked, LockState::Locked),
            ("we unlocked", HostLock::Unlocked, LockState::Unlocked),
            (
                "the compositor ended it",
                HostLock::Finished,
                LockState::Unlocked,
            ),
        ];
        for (name, report, want) in cases {
            assert_eq!(lock_for(*report), *want, "{name}");
        }
    }

    #[test]
    fn a_flood_is_read_a_bounded_batch_at_a_time() {
        let (dir, listener) = daemon();
        let mut client = Palmrest::at(socket_path(Some(dir.path())));
        let reader = client.connect().expect("connects");
        let peer = accept(&listener).expect("accept");
        let touches = r#"{"kind":"touches","v":{"active":"down"}}"#;
        for _ in 0..10 {
            send(&peer, touches.as_bytes(), SendFlags::empty()).expect("send");
        }
        let (first, stream) = read_up_to(&reader, 4);
        assert_eq!((first.len(), stream), (4, Stream::Open), "one batch only");
        let (second, _) = read_up_to(&reader, 4);
        assert_eq!(second.len(), 4, "the next batch carries on");
        let (rest, stream) = read_up_to(&reader, 4);
        assert_eq!((rest.len(), stream), (2, Stream::Open), "then what is left");
    }

    #[test]
    fn only_this_user_or_root_may_be_the_daemon() {
        let cases: &[(&str, u32, u32, Trust)] = &[
            ("the same user", 1000, 1000, Trust::Trusted),
            ("root", 0, 1000, Trust::Trusted),
            ("another user", 1001, 1000, Trust::Refused),
            ("a system account", 65534, 1000, Trust::Refused),
        ];
        for (name, peer, own, want) in cases {
            assert_eq!(trust(*peer, *own), *want, "{name}");
        }
    }

    #[test]
    fn a_hello_of_another_version_stops_the_read() {
        let (dir, listener) = daemon();
        let mut client = Palmrest::at(socket_path(Some(dir.path())));
        let reader = client.connect().expect("connects");
        let peer = accept(&listener).expect("accept");
        let hello = r#"{"kind":"hello","v":{"version":2,"device":{"name":"m","path":"p","vendor":0,"product":0}}}"#;
        let touches = r#"{"kind":"touches","v":{"active":"down"}}"#;
        for datagram in [hello, touches] {
            send(&peer, datagram.as_bytes(), SendFlags::empty()).expect("send");
        }
        let (messages, stream) = read_all(&reader);
        assert_eq!(stream, Stream::Incompatible { version: 2 });
        assert!(
            messages.is_empty(),
            "nothing after it is read: {messages:?}"
        );
    }
}

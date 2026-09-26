//! The palmrest client (design/12 §12.3.10-11, design/11 §11.7): connects to
//! `$XDG_RUNTIME_DIR/palmrest/events.sock` (`SOCK_SEQPACKET`) when the daemon runs, tells it when
//! the pointer is over one of our surfaces (`PointerOver`), and reads its scroll, touch, tap and
//! swipe events for the engine (`input::scroll`). No daemon, no socket: nothing happens, and
//! the host tries again on the next pointer enter.
//!
//! The socket I/O is here; registering the reading side with the event loop is the context's
//! (`ctx::palmrest`), since the loop belongs to `HostCtx`.

pub mod route;
pub mod wire;

use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::path::{Path, PathBuf};

use rustix::net::{
    AddressFamily, RecvFlags, SendFlags, SocketAddrUnix, SocketFlags, SocketType, connect, recv,
    send, socket_with,
};

use crate::input::palmrest::wire::{Over, ToDaemon, ToHost};

/// The largest message the host reads; a longer datagram is truncated and skipped.
const MAX_MESSAGE: usize = 4096;

/// Whether the host should try to connect at the next flush.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retry {
    Due,
    Waiting,
}

/// Whether the daemon's end is still open after a read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Open,
    Closed,
}

#[derive(Debug)]
enum Link {
    Down,
    /// Connected; this descriptor sends (the event loop owns a duplicate for reading).
    Up(OwnedFd),
}

/// The host's connection to palmrest.
#[derive(Debug)]
pub struct Palmrest {
    path: Option<PathBuf>,
    link: Link,
    /// Where the pointer is, sent on every change and again after each connect.
    over: Over,
    retry: Retry,
}

/// `$XDG_RUNTIME_DIR/palmrest/events.sock` for `runtime_dir`.
pub fn socket_path(runtime_dir: Option<&Path>) -> Option<PathBuf> {
    runtime_dir.map(|dir| dir.join("palmrest").join("events.sock"))
}

impl Palmrest {
    /// The client for the socket under `$XDG_RUNTIME_DIR`; unconnected, first attempt due.
    pub fn from_env() -> Palmrest {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
        Palmrest::at(socket_path(runtime.as_deref()))
    }

    /// The client for the socket at `path` (`None`: never connects).
    pub fn at(path: Option<PathBuf>) -> Palmrest {
        Palmrest {
            path,
            link: Link::Down,
            over: Over::NotOurs,
            retry: Retry::Due,
        }
    }

    /// Whether a connection attempt is due (at start, and after a pointer enter while down).
    pub fn retry(&self) -> Retry {
        match self.link {
            Link::Up(_) => Retry::Waiting,
            Link::Down => self.retry,
        }
    }

    /// Connect if the socket exists: returns the descriptor the event loop reads from. Sends
    /// the pointer's current `PointerOver` at once. `None` (and no retry until the next enter)
    /// when there is no daemon.
    pub fn connect(&mut self) -> Option<OwnedFd> {
        self.retry = Retry::Waiting;
        let path = self.path.as_ref()?;
        if !path.exists() {
            return None;
        }
        let fd = open(path).ok()?;
        let reader = fd.try_clone().ok()?;
        self.link = Link::Up(fd);
        self.send(&ToDaemon::PointerOver { over: self.over });
        Some(reader)
    }

    /// The daemon went away (the read side saw the end).
    pub fn disconnected(&mut self) {
        self.link = Link::Down;
    }

    /// The pointer entered or left our surfaces. While down, an enter makes a connection
    /// attempt due.
    pub fn pointer_over(&mut self, over: Over) {
        let changed = self.over != over;
        self.over = over;
        match (&self.link, over) {
            (Link::Down, Over::Ours) => self.retry = Retry::Due,
            (Link::Down, Over::NotOurs) => {}
            (Link::Up(_), _) if changed => self.send(&ToDaemon::PointerOver { over }),
            (Link::Up(_), _) => {}
        }
    }

    /// Where the pointer was last reported.
    pub fn over(&self) -> Over {
        self.over
    }

    fn send(&mut self, message: &ToDaemon) {
        let Link::Up(fd) = &self.link else {
            return;
        };
        match send(
            fd,
            &wire::encode(message),
            SendFlags::NOSIGNAL | SendFlags::DONTWAIT,
        ) {
            Ok(_) => {}
            // A full socket drops one PointerOver; the next change resends the state.
            Err(e) if e == rustix::io::Errno::AGAIN => {}
            Err(_) => self.link = Link::Down,
        }
    }
}

/// A non-blocking, close-on-exec `SOCK_SEQPACKET` connection to `path`.
pub fn open(path: &Path) -> io::Result<OwnedFd> {
    let fd = socket_with(
        AddressFamily::UNIX,
        SocketType::SEQPACKET,
        SocketFlags::CLOEXEC | SocketFlags::NONBLOCK,
        None,
    )?;
    connect(&fd, &SocketAddrUnix::new(path)?)?;
    Ok(fd)
}

/// Every message waiting on `fd`, and whether the daemon's end is still open.
pub fn read_all(fd: impl AsFd) -> (Vec<ToHost>, Stream) {
    let mut messages = Vec::new();
    let mut buf = vec![0u8; MAX_MESSAGE];
    loop {
        match recv(fd.as_fd(), &mut buf[..], RecvFlags::DONTWAIT) {
            Ok((_, 0)) => return (messages, Stream::Closed),
            Ok((len, full)) if full <= len => messages.extend(wire::decode(&buf[..len])),
            // Truncated: not a message we can read.
            Ok(_) => {}
            Err(e) if e == rustix::io::Errno::AGAIN => return (messages, Stream::Open),
            Err(e) if e == rustix::io::Errno::INTR => {}
            Err(_) => return (messages, Stream::Closed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::palmrest::wire::{ActiveTouch, Phase};
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

        pub struct Dir(PathBuf);

        impl Dir {
            pub fn new() -> Dir {
                static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let path = std::env::temp_dir()
                    .join(format!("shell-host-palmrest-{}-{n}", std::process::id()));
                std::fs::create_dir_all(&path).expect("temp dir");
                Dir(path)
            }

            pub fn path(&self) -> &Path {
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
            r#"{"PointerOver":{"over":"Ours"}}"#,
            "state on connect"
        );
        client.pointer_over(Over::Ours);
        client.pointer_over(Over::NotOurs);
        assert_eq!(
            received(&peer),
            r#"{"PointerOver":{"over":"NotOurs"}}"#,
            "only changes"
        );

        let scroll = r#"{"Scroll":{"gesture":1,"phase":"Began","dx_px":0.0,"dy_px":0.0,"fingers":1,"axis":"Y","t_ns":5}}"#;
        for datagram in [r#"{"Touches":{"active":"Down"}}"#, "not json", scroll] {
            send(&peer, datagram.as_bytes(), SendFlags::empty()).expect("send");
        }
        let (messages, stream) = read_all(&reader);
        assert_eq!(stream, Stream::Open);
        assert_eq!(
            messages.len(),
            2,
            "the junk datagram is skipped: {messages:?}"
        );
        assert_eq!(
            messages[0],
            ToHost::Touches {
                active: ActiveTouch::Down
            }
        );
        assert!(matches!(
            messages[1],
            ToHost::Scroll {
                phase: Phase::Began,
                ..
            }
        ));

        drop(peer);
        let (messages, stream) = read_all(&reader);
        assert_eq!((messages.len(), stream), (0, Stream::Closed));
    }
}

//! The palmrest client against a fake daemon: a listening socket at a scratch path.

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
        r#"{"kind":"session_lock","v":{"state":"unlocked"}}"#,
        "the lock state is told first on connect"
    );
    assert_eq!(
        received(&peer),
        r#"{"kind":"pointer_over","v":{"over":"ours"}}"#,
        "then the pointer"
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
    let locked = r#"{"kind":"session_lock","v":{"state":"locked"}}"#;
    assert_eq!(
        received(&peer),
        locked,
        "a lock held before the connect is told at it, before anything else"
    );
    assert_eq!(
        received(&peer),
        r#"{"kind":"pointer_over","v":{"over":"not_ours"}}"#
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
    assert_eq!(received(&second), locked, "a reconnect tells it first again");
}

#[test]
fn a_daemon_that_starts_late_is_found_on_the_timer_without_a_pointer_enter() {
    let dir = tempdir::Dir::new();
    let mut client = Palmrest::at(socket_path(Some(dir.path())));
    client.session_lock(LockState::Locked);
    assert!(client.connect().is_none(), "no daemon yet");
    assert_eq!(client.retry(), Retry::Waiting);
    assert_eq!(client.retry_in(), Some(Duration::from_millis(250)));
    client.retry_now();
    assert!(client.connect().is_none(), "still none");
    assert_eq!(
        client.retry_in(),
        Some(Duration::from_millis(500)),
        "the wait doubles after a miss"
    );

    // The daemon comes up; the wait runs out; the pointer never entered anything.
    let path = socket_path(Some(dir.path())).expect("a path");
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    let listener = socket_with(
        AddressFamily::UNIX,
        SocketType::SEQPACKET,
        SocketFlags::CLOEXEC,
        None,
    )
    .expect("socket");
    bind(&listener, &SocketAddrUnix::new(&path).expect("addr")).expect("bind");
    listen(&listener, 4).expect("listen");
    assert_eq!(client.retry(), Retry::Waiting, "not before the wait is over");
    client.retry_now();
    assert_eq!(client.retry(), Retry::Due);
    let _reader = client.connect().expect("found");
    let peer = accept(&listener).expect("accept");
    assert_eq!(
        received(&peer),
        r#"{"kind":"session_lock","v":{"state":"locked"}}"#,
        "the lock is the first thing the restarted daemon hears"
    );
    assert_eq!(client.retry_in(), None, "connected: nothing to wait for");
}

#[test]
fn a_lock_while_the_daemon_is_away_makes_an_attempt_due_and_an_unlock_does_not() {
    let dir = tempdir::Dir::new();
    let mut client = Palmrest::at(socket_path(Some(dir.path())));
    assert!(client.connect().is_none());
    assert_eq!(client.retry(), Retry::Waiting);
    client.session_lock(LockState::Locked);
    assert_eq!(client.retry(), Retry::Due, "the pointer will not enter anything");
    assert!(client.connect().is_none());
    client.session_lock(LockState::Unlocked);
    assert_eq!(client.retry(), Retry::Waiting);
    assert_eq!(
        client.retry_in(),
        Some(Duration::from_millis(500)),
        "two misses so far"
    );
}

#[test]
fn a_daemon_that_went_away_is_looked_for_again_and_never_without_a_path() {
    let (dir, listener) = daemon();
    let mut client = Palmrest::at(socket_path(Some(dir.path())));
    let _reader = client.connect().expect("connects");
    let _peer = accept(&listener).expect("accept");
    client.disconnected();
    assert_eq!(client.retry_in(), Some(Duration::from_millis(250)));
    client.retry_now();
    assert_eq!(client.retry(), Retry::Due);
    assert_eq!(Palmrest::at(None).retry_in(), None);
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

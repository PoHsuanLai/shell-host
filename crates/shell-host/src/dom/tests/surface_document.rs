//! Integration tests for the document modules (`dom/**`, `wake.rs`, `bridge.rs`, `root.rs`):
//! building a `SurfaceDocument` from a `HostRoot`, polling it, and its supporting pieces. Headless —
//! no Wayland connection, no GPU surface — using a dummy `HostShellProvider` and the
//! local (`data:` and `file:`) `NetProvider`.

use std::task::Context;
use std::time::Duration;

use blitz_dom::Document as _;
use blitz_traits::net::{Bytes, NetHandler, NetProvider, Request};
use calloop::EventLoop;
use dioxus::prelude::*;
use tokio::sync::watch;

use crate::clipboard::SharedSelection;
use crate::dom::document::{Polled, SurfaceDocument};
use crate::dom::shell_provider::HostShellProvider;
use crate::dom::tests::env::test_env;
use crate::id::SurfaceId;
use crate::root::HostRoot;
use crate::use_watch;
use crate::wake::{WokenSet, surface_waker};
use blitz_kit::net::LocalNet;

/// Every text node's content, concatenated, for a coarse "did the DOM render X" assertion.
fn all_text(doc: &mut SurfaceDocument) -> String {
    let mut out = String::new();
    doc.dioxus().inner().visit(|_, node| {
        if node.is_text_node() {
            out.push_str(&node.text_content());
        }
    });
    out
}

fn styled_app() -> Element {
    rsx! {
        dioxus_document::Style { "body{{color:red}}" }
        div { "hello" }
    }
}

#[test]
fn the_head_shim_delivers_a_style_element_into_the_document() {
    let mut doc = SurfaceDocument::build(HostRoot::new(styled_app), test_env(200, 100));

    let mut found_style = false;
    doc.dioxus().inner().visit(|_, node| {
        if let Some(element) = node.element_data()
            && &*element.name.local == "style"
            && node.text_content().contains("color:red")
        {
            found_style = true;
        }
    });
    assert!(
        found_style,
        "expected a <style> element queued by the head shim"
    );
}

fn sized_content_app() -> Element {
    rsx! {
        style { "html,body{{margin:0}}#card{{width:160px;height:40px}}" }
        div { id: "card", "hi" }
    }
}

fn style_only_app() -> Element {
    rsx! {
        style { "html,body{{margin:0}}" }
    }
}

/// `content_size` skips `<body>`'s literal `<style>` child (`display:none` by Blitz's own
/// default stylesheet, and always body's first child in this codebase's rsx convention) and
/// measures the actual content element after it.
#[test]
fn content_size_measures_the_first_content_element_under_body() {
    let mut doc = SurfaceDocument::build(HostRoot::new(sized_content_app), test_env(200, 100));
    doc.resolve(0.0);
    assert_eq!(
        doc.content_size(),
        Some(crate::scale::Size {
            width: 160,
            height: 40
        }),
        "the #card div, not the #main mount point (which fills the 200px-wide viewport)"
    );
}

/// A document whose body holds nothing but a `<style>` has no content to measure.
#[test]
fn content_size_is_none_with_no_content_element() {
    let mut doc = SurfaceDocument::build(HostRoot::new(style_only_app), test_env(200, 100));
    doc.resolve(0.0);
    assert_eq!(doc.content_size(), None);
}

fn overlay_content_app() -> Element {
    rsx! {
        style { "html,body{{margin:0}}#wrap{{width:10px;height:10px}}#menu{{position:absolute;left:8px;top:6px;width:140px;height:90px}}" }
        div { id: "wrap",
            div { id: "menu", "menu" }
        }
    }
}

/// A menu drawn as an absolutely placed overlay (quire's `Menu`/`Popover`) adds nothing to its
/// parent's size; `content_size` still measures where it paints: its far edges from the origin.
#[test]
fn content_size_includes_absolutely_positioned_descendants() {
    let mut doc = SurfaceDocument::build(HostRoot::new(overlay_content_app), test_env(200, 100));
    doc.resolve(0.0);
    assert_eq!(
        doc.content_size(),
        Some(crate::scale::Size {
            width: 148,
            height: 96
        }),
        "the menu's right and bottom edges (8 + 140, 6 + 90), not the 10x10 in-flow wrapper"
    );
}

fn watched_app(rx: watch::Receiver<i32>) -> Element {
    let value = use_watch(rx);
    rsx! {
        div { "{value}" }
    }
}

#[tokio::test]
async fn use_watch_updates_the_rendered_value_when_the_channel_changes() {
    let (tx, rx) = watch::channel(1);
    let mut doc = SurfaceDocument::build(HostRoot::with_props(watched_app, rx), test_env(200, 100));
    assert!(all_text(&mut doc).contains('1'), "{:?}", all_text(&mut doc));

    let waker = std::task::Waker::noop().clone();
    let mut cx = Context::from_waker(&waker);
    // The watch task registers its interest in the channel on this poll.
    doc.poll(&mut cx);

    tx.send(2).expect("receiver still alive");
    // Sending on a watch channel with a registered waiter wakes it synchronously, so a single
    // poll after the send is enough to observe the new value with no channel-changed()-await
    // latency.
    let polled = doc.poll(&mut cx);
    assert_eq!(polled, Polled::Changed);
    let text = all_text(&mut doc);
    assert!(text.contains('2'), "expected the updated value in {text:?}");
}

#[tokio::test]
async fn poll_reports_a_change_and_the_waker_pings_when_a_watched_value_changes() {
    let (tx, rx) = watch::channel(0i32);
    let mut doc = SurfaceDocument::build(HostRoot::with_props(watched_app, rx), test_env(200, 100));

    let (ping, ping_source) = calloop::ping::make_ping().expect("eventfd ping");
    let woken = WokenSet::default();
    let waker = surface_waker(SurfaceId(9), woken.clone(), ping);
    let mut cx = Context::from_waker(&waker);

    // Baseline poll: the use_watch task registers its interest in the channel; nothing has
    // happened yet, so nothing should be marked woken.
    doc.poll(&mut cx);
    assert_eq!(woken.take(), Vec::new());

    let mut ping_loop: EventLoop<bool> = EventLoop::try_new().expect("event loop");
    ping_loop
        .handle()
        .insert_source(ping_source, |_, _, fired: &mut bool| *fired = true)
        .expect("insert ping source");

    tx.send(7).expect("receiver still alive");
    // `watch::Sender::send` wakes a registered waiter synchronously: our surface waker should
    // already have marked the surface and pinged the loop before `poll` runs again.
    assert_eq!(woken.take(), vec![SurfaceId(9)]);
    let mut fired = false;
    ping_loop
        .dispatch(Duration::from_millis(200), &mut fired)
        .expect("dispatch");
    assert!(fired, "sending on the watched channel should ping the loop");

    let polled = doc.poll(&mut cx);
    assert_eq!(polled, Polled::Changed);
    assert!(all_text(&mut doc).contains('7'));
}

/// Hands the fetched bytes back to the test.
struct CollectHandler(tokio::sync::oneshot::Sender<(String, Bytes)>);

impl NetHandler for CollectHandler {
    fn bytes(self: Box<Self>, resolved_url: String, bytes: Bytes) {
        let _ = self.0.send((resolved_url, bytes));
    }
}

#[tokio::test]
async fn the_net_provider_serves_a_data_png() {
    // A 1x1 transparent PNG, base64-encoded (the smallest valid PNG payload).
    const PNG_BASE64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";
    let uri = format!("data:image/png;base64,{PNG_BASE64}");

    let provider = LocalNet;
    let (tx, rx) = tokio::sync::oneshot::channel();

    provider.fetch(
        0,
        Request::get(uri.parse().expect("valid data: url")),
        Box::new(CollectHandler(tx)),
    );

    let (resolved_url, bytes) = rx.await.expect("the data: fetch completes");
    assert_eq!(resolved_url, uri);
    // The PNG signature, unambiguous proof the payload was base64-decoded correctly.
    let signature: &[u8] = bytes.as_ref();
    assert_eq!(
        &signature[..8],
        &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]
    );
}

#[tokio::test]
async fn the_net_provider_serves_a_file_url() {
    let path = std::env::temp_dir().join(format!("shell-host-localnet-{}.bin", std::process::id()));
    let payload: &[u8] = b"\x89PNG wallpaper bytes";
    std::fs::write(&path, payload).expect("write the fixture");
    let url = blitz_traits::net::Url::from_file_path(&path).expect("absolute path");

    let (tx, rx) = tokio::sync::oneshot::channel();
    LocalNet.fetch(0, Request::get(url.clone()), Box::new(CollectHandler(tx)));
    let fetched = tokio::time::timeout(Duration::from_secs(5), rx).await;
    std::fs::remove_file(&path).ok();

    let (resolved_url, bytes) = fetched
        .expect("in time")
        .expect("the file: fetch completes");
    assert_eq!(resolved_url, url.as_str());
    assert_eq!(bytes.as_ref(), payload);
}

#[test]
fn the_shell_provider_pastes_what_the_seat_published_and_queues_what_it_copies() {
    let (ping, _source) = calloop::ping::make_ping().expect("eventfd ping");
    let seat = SharedSelection::default();
    let provider = HostShellProvider::new(ping, seat.clone());
    use blitz_traits::shell::ShellProvider as _;

    assert!(provider.get_clipboard_text().is_err(), "empty clipboard");
    let generation = seat.begin();
    seat.publish(generation, Some("from another client".into()));
    assert_eq!(
        provider.get_clipboard_text().ok().as_deref(),
        Some("from another client")
    );

    assert!(provider.set_clipboard_text("typed here".into()).is_ok());
    assert_eq!(provider.take_copied().as_deref(), Some("typed here"));
    assert_eq!(provider.take_copied(), None, "taken once");
}

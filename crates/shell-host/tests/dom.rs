//! Integration tests for the document modules (`dom/**`, `wake.rs`, `bridge.rs`, `root.rs`):
//! building a `SurfaceDocument` from a `Root`, polling it, and its supporting pieces. Headless —
//! no Wayland connection, no GPU surface — using a dummy `HostShellProvider` and the
//! `data:`-only `NetProvider`.

use std::sync::Arc;
use std::task::Context;
use std::time::Duration;

use blitz_dom::Document as _;
use blitz_traits::net::{Bytes, NetHandler, NetProvider, Request};
use blitz_traits::shell::{ColorScheme, Viewport};
use calloop::EventLoop;
use dioxus::prelude::*;
use tokio::sync::watch;

use shell_host::dom::document::{DocumentEnv, Polled, SurfaceDocument};
use shell_host::dom::fonts::SharedFonts;
use shell_host::dom::net::DataUriNet;
use shell_host::dom::shell_provider::HostShellProvider;
use shell_host::id::SurfaceId;
use shell_host::root::Root;
use shell_host::use_watch;
use shell_host::wake::{WokenSet, surface_waker};

/// A `DocumentEnv` with no Wayland connection behind it: the shell provider only records what
/// it is asked, the font context discovers whatever fonts this machine has, and the net
/// provider serves `data:` only.
fn test_env() -> DocumentEnv {
    let (ping, _source) = calloop::ping::make_ping().expect("eventfd ping");
    DocumentEnv {
        shell: Arc::new(HostShellProvider::new(SurfaceId(1), ping)),
        fonts: SharedFonts::system().for_document(),
        net: Arc::new(DataUriNet),
        viewport: Viewport::new(200, 100, 1.0, ColorScheme::Light),
    }
}

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
    let mut doc = SurfaceDocument::build(Root::new(styled_app), test_env());

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

fn watched_app(rx: watch::Receiver<i32>) -> Element {
    let value = use_watch(rx);
    rsx! {
        div { "{value}" }
    }
}

#[tokio::test]
async fn use_watch_updates_the_rendered_value_when_the_channel_changes() {
    let (tx, rx) = watch::channel(1);
    let mut doc = SurfaceDocument::build(Root::with_props(watched_app, rx), test_env());
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
    let mut doc = SurfaceDocument::build(Root::with_props(watched_app, rx), test_env());

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

#[tokio::test]
async fn the_net_provider_serves_a_data_png() {
    // A 1x1 transparent PNG, base64-encoded (the smallest valid PNG payload).
    const PNG_BASE64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";
    let uri = format!("data:image/png;base64,{PNG_BASE64}");

    let provider = DataUriNet;
    let (tx, rx) = tokio::sync::oneshot::channel();
    struct CollectHandler(tokio::sync::oneshot::Sender<(String, Bytes)>);
    impl NetHandler for CollectHandler {
        fn bytes(self: Box<Self>, resolved_url: String, bytes: Bytes) {
            let _ = self.0.send((resolved_url, bytes));
        }
    }

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

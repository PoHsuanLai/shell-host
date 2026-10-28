//! Integration tests for the document modules (`dom/**`, `wake.rs`, `bridge.rs`, `root.rs`):
//! building a `SurfaceDocument` from a `HostRoot`, polling it, and its supporting pieces. Headless —
//! no Wayland connection, no GPU surface — using a dummy `HostShellProvider`.

use std::task::Context;
use std::time::Duration;

use blitz_dom::Document as _;
use calloop::EventLoop;
use dioxus::prelude::*;
use tokio::sync::watch;

use crate::clipboard::SharedSelection;
use crate::dom::document::SurfaceDocument;
use crate::dom::shell_provider::HostShellProvider;
use crate::dom::tests::env::test_env;
use crate::id::SurfaceId;
use crate::root::HostRoot;
use crate::use_watch;
use crate::wake::{WokenSet, surface_waker};

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

fn overlay_content_app() -> Element {
    rsx! {
        style { "html,body{{margin:0}}#wrap{{width:10px;height:10px}}#menu{{position:absolute;left:8px;top:6px;width:140px;height:90px}}" }
        div { id: "wrap",
            div { id: "menu", "menu" }
        }
    }
}

/// `content_size` skips `<body>`'s literal `<style>` child (`display:none` by Blitz's own
/// default stylesheet, and always body's first child in this codebase's rsx convention) and
/// measures the actual content element after it, including absolutely placed descendants (a
/// menu adds nothing to its parent's size but paints where it paints).
/// name, document, the measured size
type Case = (&'static str, fn() -> Element, Option<crate::scale::Size>);

#[test]
fn content_size_measures_the_content_where_it_paints() {
    use crate::scale::Size;
    let cases: &[Case] = &[
        (
            "the #card div, not the #main mount point (which fills the 200px-wide viewport)",
            sized_content_app,
            Some(Size {
                width: 160,
                height: 40,
            }),
        ),
        ("a body holding nothing but a <style>", style_only_app, None),
        (
            "the menu's right and bottom edges (8 + 140, 6 + 90), not the 10x10 in-flow wrapper",
            overlay_content_app,
            Some(Size {
                width: 148,
                height: 96,
            }),
        ),
    ];
    for &(name, app, want) in cases {
        let mut doc = SurfaceDocument::build(HostRoot::new(app), test_env(200, 100));
        doc.resolve(0.0);
        assert_eq!(doc.content_size(), want, "{name}");
    }
}

fn watched_app(rx: watch::Receiver<i32>) -> Element {
    let value = use_watch(rx);
    rsx! {
        div { "{value}" }
    }
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
    assert!(polled);
    assert!(all_text(&mut doc).contains('7'));
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

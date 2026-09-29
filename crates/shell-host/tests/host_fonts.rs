//! `HostConfig::fonts` actually reaches a document (without it a document falls back
//! to fontconfig): a face registered through it is present in
//! `registered_families`, and a document naming that family in `font-family` measures
//! differently from one naming only `system-ui` — the regression `text_stack_fonts.rs` (quire,
//! `gate`) pins from the other side, under `SharedFonts::system()` alone.
//!
//! Uses a real font's bytes rather than a synthetic one: Inter, read from quire's own bundled
//! assets (`~/quire/crates/ds/assets/fonts`) if this checkout has quire next to it, skipped
//! (with a message, not a failure) otherwise — shell-host must not depend on quire (see
//! `scripts/check-boundary.sh`), so this is a file read, never a crate dependency.

use blitz_dom::{DocumentConfig, FontContext};
use dioxus::prelude::*;
use dioxus_native_dom::DioxusDocument;
use shell_host::dom::fonts::SharedFonts;
use shell_host::{FontFaces, HostConfig, registered_families};

const INTER: &str = "inter-normal-400-700-latin.ttf";

/// Inter's bytes, leaked to `'static` (`FontFaces`' bound — a real shell's are `include_bytes!`
/// in its design-system crate, already `'static`), or `None` if quire is not checked out beside
/// this repo.
fn inter_bytes() -> Option<&'static [u8]> {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from)?;
    let path = home.join("quire/crates/ds/assets/fonts").join(INTER);
    let bytes = std::fs::read(&path).ok()?;
    Some(Box::leak(bytes.into_boxed_slice()))
}

#[component]
fn Line(stack: String, text: String) -> Element {
    rsx! {
        span { id: "line", style: "display:inline-block; white-space:pre; font-size:16px; font-family:{stack}", "{text}" }
    }
}

/// The width `text` lays out at in `stack` under `fonts`.
fn width(fonts: FontContext, stack: &str, text: &str) -> f64 {
    let vdom = VirtualDom::new_with_props(
        Line,
        LineProps {
            stack: stack.to_string(),
            text: text.to_string(),
        },
    );
    let mut doc = DioxusDocument::new(
        vdom,
        DocumentConfig {
            font_ctx: Some(fonts),
            ..Default::default()
        },
    );
    doc.initial_build();
    let mut inner = doc.inner.borrow_mut();
    inner.resolve(0.0);
    let node = inner
        .query_selector("#line")
        .ok()
        .flatten()
        .expect("the line is in the document");
    inner
        .get_client_bounding_rect(node)
        .expect("the line is laid out")
        .width
}

#[test]
fn a_face_registered_through_host_config_is_a_family() {
    let Some(bytes) = inter_bytes() else {
        eprintln!("skipped: quire is not checked out at ~/quire/crates/ds/assets/fonts/{INTER}");
        return;
    };
    let config = HostConfig {
        fonts: FontFaces(vec![bytes]),
        ..HostConfig::default()
    };
    let families = registered_families(&config);
    assert!(
        families.contains(&"Inter".to_string()),
        "Inter is not in {families:?}"
    );
}

#[test]
fn a_registered_faces_stack_measures_differently_from_system_ui() {
    let Some(bytes) = inter_bytes() else {
        eprintln!("skipped: quire is not checked out at ~/quire/crates/ds/assets/fonts/{INTER}");
        return;
    };
    // The bug this pins: before `HostConfig::fonts` had a caller, every document got only
    // `SharedFonts::system()` — a font-family naming a bundled face that is not also a system
    // font fell straight through to `system-ui`, indistinguishable from not naming it at all.
    let unregistered = width(
        SharedFonts::system().for_document(),
        "\"Inter\"",
        "shell-host",
    );
    let system = width(
        SharedFonts::system().for_document(),
        "system-ui,sans-serif",
        "shell-host",
    );
    if (unregistered - system).abs() >= 0.01 {
        eprintln!("skipped: Inter is a system font here, so it already resolved unregistered");
        return;
    }

    let config = HostConfig {
        fonts: FontFaces(vec![bytes]),
        ..HostConfig::default()
    };
    let registered = width(
        SharedFonts::system_with(&config.fonts).for_document(),
        "\"Inter\"",
        "shell-host",
    );
    assert!(
        (registered - system).abs() >= 1.0,
        "registering Inter through HostConfig::fonts did not change how \"Inter\" lays out \
         ({registered} vs system-ui's {system})"
    );
}

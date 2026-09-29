//! An element that arrives under a resting pointer: Blitz's `refresh_hover`
//! moves the hover at the end of `resolve` without dispatching, and `SurfaceDocument::resolve`
//! repairs it, so the arriving element gets its enter and the covered one its leave, once.
//!
//! A real `SurfaceDocument` fed the moves `input::pointer` makes; the card arrives through a
//! `use_watch` channel, as a notification banner arrives in sill.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::task::Context;

use blitz_traits::events::MouseEventButtons;
use blitz_traits::shell::{ColorScheme, Viewport};
use dioxus::prelude::*;
use keyboard_types::Modifiers;
use tokio::sync::watch;

use shell_host::clipboard::SharedSelection;
use shell_host::dom::document::{DocumentEnv, HoverShift, SurfaceDocument};
use shell_host::dom::fonts::SharedFonts;
use shell_host::dom::net::LocalNet;
use shell_host::dom::shell_provider::HostShellProvider;
use shell_host::id::SurfaceId;
use shell_host::input::pointer::{self, PointerPos};
use shell_host::root::Root;
use shell_host::use_watch;

const CSS: &str = "html,body{margin:0}\
#pad{width:300px;height:200px}\
#card{position:absolute;left:40px;top:40px;width:120px;height:60px}";

/// What the handlers saw, in order.
#[derive(Clone, Default)]
struct Log(Rc<RefCell<Vec<String>>>);

impl Log {
    fn push(&self, line: &str) {
        self.0.borrow_mut().push(line.to_string());
    }

    fn take(&self) -> Vec<String> {
        std::mem::take(&mut self.0.borrow_mut())
    }
}

/// Whether the card is in the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Card {
    Absent,
    Present,
}

#[derive(Clone)]
struct Props {
    card: watch::Receiver<Card>,
    log: Log,
}

fn app(props: Props) -> Element {
    let card = use_watch(props.card);
    let (pad, arrived) = (props.log.clone(), props.log);
    let (pad_leave, card_leave) = (pad.clone(), arrived.clone());
    rsx! {
        style { "{CSS}" }
        div {
            id: "pad",
            onpointerenter: move |_| pad.push("pad enter"),
            onpointerleave: move |_| pad_leave.push("pad leave"),
        }
        if card() == Card::Present {
            div {
                id: "card",
                onpointerenter: move |_| arrived.push("card enter"),
                onpointerleave: move |_| card_leave.push("card leave"),
            }
        }
    }
}

struct Fixture {
    doc: SurfaceDocument,
    card: watch::Sender<Card>,
    log: Log,
}

fn build() -> Fixture {
    let (ping, _source) = calloop::ping::make_ping().expect("eventfd ping");
    let env = DocumentEnv {
        shell: Arc::new(HostShellProvider::new(
            SurfaceId(1),
            ping,
            SharedSelection::default(),
        )),
        fonts: SharedFonts::system().for_document(),
        net: Arc::new(LocalNet),
        viewport: Viewport::new(300, 200, 1.0, ColorScheme::Light),
        prepaint: shell_host::Prepaint::None,
    };
    let (card, rx) = watch::channel(Card::Absent);
    let log = Log::default();
    let props = Props {
        card: rx,
        log: log.clone(),
    };
    let mut doc = SurfaceDocument::build(Root::with_props(app, props), env);
    poll(&mut doc);
    doc.resolve(0.0);
    Fixture { doc, card, log }
}

fn poll(doc: &mut SurfaceDocument) {
    doc.poll(&mut Context::from_waker(std::task::Waker::noop()));
}

fn move_to(doc: &mut SurfaceDocument, x: f64, y: f64) {
    let at = PointerPos { x, y };
    doc.handle_event(pointer::move_event(
        at,
        MouseEventButtons::None,
        Modifiers::empty(),
    ));
}

/// The card's rect is 40..160 x 40..100; the pointer rests at (100, 70), inside it.
#[tokio::test]
async fn a_card_arriving_under_a_resting_pointer_is_entered_once_and_the_pad_left() {
    let Fixture { mut doc, card, log } = build();
    move_to(&mut doc, 100.0, 70.0);
    poll(&mut doc);
    assert_eq!(log.take(), ["pad enter"]);

    card.send(Card::Present).expect("receiver alive");
    poll(&mut doc);
    let resolved = doc.resolve(0.1);
    poll(&mut doc);
    assert_eq!(resolved.hover, HoverShift::Moved);
    assert_eq!(log.take(), ["pad leave", "card enter"], "at the resolve");

    // The nudge that, before the repair, was the first the driver heard of the card: now
    // there is nothing left to dispatch.
    move_to(&mut doc, 101.0, 70.0);
    poll(&mut doc);
    assert_eq!(doc.resolve(0.2).hover, HoverShift::Kept);
    assert_eq!(log.take(), Vec::<String>::new(), "after a nudge");

    // And it leaves normally.
    move_to(&mut doc, 250.0, 150.0);
    poll(&mut doc);
    assert_eq!(log.take(), ["card leave", "pad enter"]);
}

/// The card going away under the pointer: the pad is entered again (a removed element gets no
/// leave, as in a browser).
#[tokio::test]
async fn the_card_removed_under_a_resting_pointer_gives_the_pad_back() {
    let Fixture { mut doc, card, log } = build();
    move_to(&mut doc, 100.0, 70.0);
    card.send(Card::Present).expect("receiver alive");
    poll(&mut doc);
    doc.resolve(0.1);
    poll(&mut doc);
    log.take();

    card.send(Card::Absent).expect("receiver alive");
    poll(&mut doc);
    doc.resolve(0.2);
    poll(&mut doc);
    assert_eq!(log.take(), ["pad enter"]);
}

/// With no pointer on the surface there is nothing to repair: the card arrives unannounced.
#[tokio::test]
async fn without_a_pointer_the_resolve_dispatches_nothing() {
    let Fixture { mut doc, card, log } = build();
    move_to(&mut doc, 100.0, 70.0);
    doc.pointer_left(Modifiers::empty());
    poll(&mut doc);
    log.take();

    card.send(Card::Present).expect("receiver alive");
    poll(&mut doc);
    assert_eq!(doc.resolve(0.1).hover, HoverShift::Kept);
    poll(&mut doc);
    assert_eq!(log.take(), Vec::<String>::new());
}

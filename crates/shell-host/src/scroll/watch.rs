//! Scroll notification to components (design/11 §11.3.15): each frame the host publishes the
//! offset, extent and phase of every scroller a component watches with [`use_scroll`]. Blitz's
//! own `onscroll` never fires for the engine's raw writes, so this is the one way to follow a
//! scroll. (design/11 §11.7 named it `ds-native::use_scroll`; it lives here, next to the engine
//! that feeds it: FINDINGS F63.)

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use dioxus::core::{spawn, use_hook};
use dioxus::hooks::use_signal;
use dioxus::signals::{ReadSignal, WritableExt};

use crate::handle::use_surface;
use crate::live::Live;
use crate::region::ElementId;
use crate::scroll::engine::{Engine, latch_of};
use crate::scroll::geom::Scroller;

/// A scroll position in logical px; raw, so out of range while rubber-banding.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ScrollPoint {
    pub x: f64,
    pub y: f64,
}

/// A scroller's content and viewport sizes, logical px.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ScrollExtent {
    pub content_width: f64,
    pub content_height: f64,
    pub viewport_width: f64,
    pub viewport_height: f64,
}

/// What is moving a scroller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollPhase {
    #[default]
    Idle,
    /// Fingers are down on it.
    Tracking,
    /// It glides after a flick.
    Momentum,
    /// It springs back from past an edge.
    Rebound,
    /// A key, wheel or command animates it.
    Animating,
}

/// One scroller as a component sees it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ScrollState {
    pub offset: ScrollPoint,
    pub extent: ScrollExtent,
    pub phase: ScrollPhase,
}

/// The phase `engine` gives `scroller`.
pub fn phase_of(engine: &Engine, scroller: Scroller) -> ScrollPhase {
    if latch_of(engine).map(|latch| latch.scroller) != Some(scroller) {
        return ScrollPhase::Idle;
    }
    match engine {
        Engine::Idle | Engine::Pending(_) => ScrollPhase::Idle,
        Engine::Tracking(_) => ScrollPhase::Tracking,
        Engine::Momentum(_) => ScrollPhase::Momentum,
        Engine::Rebound(_) => ScrollPhase::Rebound,
        Engine::Smooth(_) | Engine::Held(_) | Engine::Spring(_) => ScrollPhase::Animating,
    }
}

/// The scrollers a surface's components watch, by element id. Shared between the host (which
/// publishes) and the surface's `SurfaceHandle`, on the main thread. An entry stays for the
/// surface's life once watched.
#[derive(Debug, Clone, Default)]
pub struct ScrollBoard(Rc<RefCell<HashMap<ElementId, Live<ScrollState>>>>);

impl ScrollBoard {
    /// The live state of `id`, created on first watch.
    pub fn watch(&self, id: &ElementId) -> Live<ScrollState> {
        self.0
            .borrow_mut()
            .entry(id.clone())
            .or_insert_with(|| Live::new(ScrollState::default()))
            .clone()
    }

    /// Every watched id and its live state.
    pub fn watched(&self) -> Vec<(ElementId, Live<ScrollState>)> {
        self.0
            .borrow()
            .iter()
            .map(|(id, live)| (id.clone(), live.clone()))
            .collect()
    }
}

/// The scroll state of the scroll container whose `id` attribute is `id` in this component's
/// surface, as live state: re-rendered when its offset, extent or phase changes (at most once
/// per frame). `ScrollState::default()` until the element has laid out.
pub fn use_scroll(id: ElementId) -> ReadSignal<ScrollState> {
    let board = use_surface().scroll_board();
    let live = use_hook(|| board.watch(&id));
    let mut latest = use_signal(|| live.get());
    use_hook(move || {
        let mut seen = live.changes();
        spawn(async move {
            loop {
                let (count, state) = live.changed_after(seen).await;
                seen = count;
                latest.set(state);
            }
        });
    });
    ReadSignal::from(latest)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::task::Context;

    use blitz_traits::shell::{ColorScheme, Viewport};
    use dioxus::prelude::*;

    use super::*;
    use crate::clipboard::SharedSelection;
    use crate::dom::document::{DocumentEnv, SurfaceDocument};
    use crate::dom::fonts::SharedFonts;
    use crate::dom::net::LocalNet;
    use crate::dom::scroll as dom;
    use crate::dom::shell_provider::HostShellProvider;
    use crate::grab::GrabRecord;
    use crate::handle::{HandleParts, RequestQueue, SurfaceHandle};
    use crate::id::{SurfaceId, SurfaceIds};
    use crate::live_scale::LiveScale;
    use crate::prepaint::Prepaint;
    use crate::role::RoleKind;
    use crate::root::Root;
    use crate::scale::Scale;
    use crate::scroll::geom::Axis;
    use crate::scroll::surface::SurfaceScroll;
    use crate::scroll::tuning::Tuning;
    use crate::tick::FrameTicks;
    use crate::toplevel::ToplevelState;

    /// Shows a scroll box's offset as text, through `use_scroll`.
    fn watcher() -> Element {
        let state = use_scroll(ElementId::new("box"));
        let y = state.read().offset.y;
        rsx! {
            div { id: "box", style: "height:100px;overflow:auto",
                div { style: "height:500px" }
            }
            p { id: "out", "{y}" }
        }
    }

    fn out(doc: &SurfaceDocument) -> String {
        let base = doc.base();
        let id = base.get_element_by_id("out").expect("out");
        base.get_node(id).expect("node").text_content()
    }

    #[test]
    fn use_scroll_follows_what_the_host_publishes() {
        let (ping, _source) = calloop::ping::make_ping().expect("eventfd ping");
        let board = ScrollBoard::default();
        let parts = HandleParts {
            requests: RequestQueue::new(ping.clone()),
            ids: SurfaceIds::new(),
            ticks: FrameTicks::default(),
            scale: LiveScale::new(Scale::ONE),
            toplevel: Live::new(ToplevelState::default()),
            grab: GrabRecord::default(),
            scrolls: board.clone(),
            modifiers: Live::new(crate::modifiers::Modifiers::default()),
            modality: Live::new(crate::modality::InputModality::default()),
        };
        let handle = SurfaceHandle::new(SurfaceId(3), RoleKind::Toplevel, parts);
        let env = DocumentEnv {
            shell: Arc::new(HostShellProvider::new(
                SurfaceId(3),
                ping,
                SharedSelection::default(),
            )),
            fonts: SharedFonts::system().for_document(),
            net: Arc::new(LocalNet),
            viewport: Viewport::new(200, 300, 1.0, ColorScheme::Light),
            prepaint: Prepaint::None,
        };
        let mut doc = SurfaceDocument::build(Root::new(watcher).context(handle), env);
        let mut cx = Context::from_waker(std::task::Waker::noop());
        doc.poll(&mut cx);
        doc.resolve(0.0);
        assert_eq!(out(&doc), "0");
        let scroll = SurfaceScroll::new(board, Tuning::default());
        let target = dom::scroller_by_id(&doc.base(), &ElementId::new("box")).expect("box");
        dom::write(&mut doc.base_mut(), target, Axis::Y, 123.0);
        scroll.publish(&doc.base());
        doc.poll(&mut cx);
        doc.resolve(0.1);
        assert_eq!(out(&doc), "123");
        let published = scroll.board().watch(&ElementId::new("box")).get();
        assert_eq!(published.extent.content_height, 500.0);
        assert_eq!(published.extent.viewport_height, 100.0);
        assert_eq!(published.phase, ScrollPhase::Idle);
    }
}

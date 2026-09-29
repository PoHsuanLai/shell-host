//! Keyboard and programmatic scrolling: scroll keys on the focused or remembered scroller
//! (design/11 §11.3.10) and `ScrollCmd` (§11.3.11).

use blitz_dom::BaseDocument;

use super::{Driver, KeyUse, Moved, SurfaceScroll};
use crate::dom::scroll::{self as dom, KeyFocus};
use crate::input::key::Repeat;
use crate::input::pointer::PointerPos;
use crate::scroll::cmd::{self, ScrollCmd};
use crate::scroll::engine::{ScrollAnimate, ScrollIn};
use crate::scroll::geom::{Latch, Px, ScrollAxis, Scroller};
use crate::scroll::keys::{self, ScrollKey};
use crate::tick::HostInstant;

impl SurfaceScroll {
    /// A scroll key went down (`Repeat::First`) or repeated. `Passed` (the document's) when
    /// something on the focus path handles keys or nothing around the focus scrolls.
    pub(crate) fn key(
        &mut self,
        doc: &mut BaseDocument,
        key: ScrollKey,
        repeat: Repeat,
        pos: Option<PointerPos>,
        now: HostInstant,
    ) -> KeyUse {
        let tuning = self.tuning;
        let env = tuning.at(now);
        let axis = key.axis();
        let focus = dom::key_focus(doc, axis);
        let scroller = match focus {
            KeyFocus::Document => return KeyUse::Passed,
            KeyFocus::Scroller(scroller) => Some(scroller),
            KeyFocus::Free => self.key_target(doc, axis, pos),
        };
        let Some(scroller) = scroller else {
            return KeyUse::Passed;
        };
        let latch = Latch { scroller, axis };
        let Some(geom) = dom::geom(doc, scroller, axis) else {
            return KeyUse::Scrolled(Moved::Still);
        };
        self.driver = Driver::Other;
        let input = match (key, repeat) {
            (ScrollKey::Line(_, dir), Repeat::First) => ScrollIn::Step {
                latch,
                by: Px(dir.sign() * keys::LINE_PX),
            },
            (ScrollKey::Line(_, dir), Repeat::Repeated) => ScrollIn::Hold {
                latch,
                step: Px(dir.sign() * keys::LINE_PX),
            },
            (ScrollKey::Page(dir), _) => ScrollIn::Step {
                latch,
                by: Px(dir.sign() * keys::page(geom.viewport.0)),
            },
            (ScrollKey::Edge(dir), _) => ScrollIn::Jump {
                latch,
                to: Px(geom.edge(dir)),
                animate: ScrollAnimate::Smooth,
            },
        };
        KeyUse::Scrolled(self.run(doc, input, env))
    }

    /// The keyboard's scroller when nothing has focus: the last gesture's scroller if it is
    /// still under the pointer, else the innermost one under the pointer with content to scroll
    /// on `axis`, else the viewport if it has; `None` (the key is the document's) otherwise.
    fn key_target(
        &self,
        doc: &BaseDocument,
        axis: ScrollAxis,
        pos: Option<PointerPos>,
    ) -> Option<Scroller> {
        let chain = pos
            .map(|p| dom::chain_at(doc, p.x, p.y, axis))
            .unwrap_or_default();
        let remembered = self
            .last_gesture
            .filter(|latch| latch.axis == axis)
            .map(|latch| latch.scroller)
            .filter(|scroller| chain.iter().any(|c| c.scroller == *scroller));
        remembered
            .or_else(|| {
                chain
                    .iter()
                    .find(|c| c.geom.max.0 > 0.0)
                    .map(|c| c.scroller)
            })
            .or_else(|| {
                dom::geom(doc, Scroller::Viewport, axis)
                    .filter(|g| g.max.0 > 0.0)
                    .map(|_| Scroller::Viewport)
            })
    }

    /// A held scroll key was released.
    pub(crate) fn key_up(&mut self, doc: &mut BaseDocument, now: HostInstant) -> Moved {
        let tuning = self.tuning;
        let env = tuning.at(now);
        self.run(doc, ScrollIn::Release, env)
    }

    /// A programmatic scroll.
    pub(crate) fn command(
        &mut self,
        doc: &mut BaseDocument,
        command: &ScrollCmd,
        now: HostInstant,
    ) -> Moved {
        let tuning = self.tuning;
        let env = tuning.at(now);
        let input = match command {
            ScrollCmd::To {
                element,
                axis,
                offset,
                animate,
            } => dom::scroller_by_id(doc, element).map(|scroller| ScrollIn::Jump {
                latch: Latch {
                    scroller,
                    axis: *axis,
                },
                to: Px(*offset),
                animate: *animate,
            }),
            ScrollCmd::By {
                element,
                axis,
                delta,
                animate,
            } => dom::scroller_by_id(doc, element).and_then(|scroller| {
                let latch = Latch {
                    scroller,
                    axis: *axis,
                };
                let geom = dom::geom(doc, scroller, *axis)?;
                Some(match animate {
                    ScrollAnimate::Smooth => ScrollIn::Step {
                        latch,
                        by: Px(*delta),
                    },
                    ScrollAnimate::Instant => ScrollIn::Jump {
                        latch,
                        to: Px(geom.offset.0 + delta),
                        animate: ScrollAnimate::Instant,
                    },
                })
            }),
            ScrollCmd::IntoView { element, animate } => into_view(doc, element, *animate),
        };
        self.driver = Driver::Other;
        match input {
            Some(input) => self.run(doc, input, env),
            None => Moved::Still,
        }
    }
}

/// The jump that brings the element with `id` into its vertical scroller's view.
fn into_view(
    doc: &BaseDocument,
    id: &crate::region::ElementId,
    animate: ScrollAnimate,
) -> Option<ScrollIn> {
    let (scroller, child) = dom::container_of(doc, id, ScrollAxis::Y)?;
    let view = dom::area(doc, scroller)?;
    let geom = dom::geom(doc, scroller, ScrollAxis::Y)?;
    let to = cmd::into_view(
        (view.y, view.y + view.height),
        (child.y, child.y + child.height),
        geom.offset.0,
        geom.max.0,
        cmd::INTO_VIEW_MARGIN,
    )?;
    Some(ScrollIn::Jump {
        latch: Latch {
            scroller,
            axis: ScrollAxis::Y,
        },
        to: Px(to),
        animate,
    })
}

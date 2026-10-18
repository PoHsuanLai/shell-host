//! The overlay thumb of a surface and what its components watch: hover, paint geometry and the
//! per-frame `use_scroll` publication.

use blitz_dom::BaseDocument;

use super::{Moved, SurfaceScroll};
use crate::input::pointer::PointerPos;
use crate::scroll::indicator::{self, Metrics, ThumbPaint};
use crate::scroll::watch::{ScrollExtent, ScrollPoint, ScrollState, phase_of};
use blitz_kit::scroll::doc as dom;
use blitz_kit::scroll::geom::ScrollAxis;
use blitz_kit::scroll::time::Elapsed;

impl SurfaceScroll {
    /// The pointer moved to `pos`: expand or collapse the thumb. `Moved` when that starts an
    /// animation (the caller renders).
    pub(crate) fn pointer(&mut self, doc: &BaseDocument, pos: PointerPos, now: Elapsed) -> Moved {
        let tuning = self.tuning;
        let env = tuning.at(now);
        let Some(metrics) = self.metrics(doc) else {
            return Moved::Still;
        };
        let hover = indicator::in_zone(&metrics, pos.x, pos.y, env.scrollbar);
        let next = self.indicator.hover(hover, env.scrollbar, env.now);
        let moved = if next == self.indicator {
            Moved::Still
        } else {
            Moved::Moved
        };
        self.indicator = next;
        moved
    }

    fn metrics(&self, doc: &BaseDocument) -> Option<Metrics> {
        let latch = self.indicator.latch?;
        let geom = dom::geom(doc, latch.scroller, latch.axis)?;
        let rect = dom::area(doc, latch.scroller)?;
        Some(Metrics {
            rect,
            axis: latch.axis,
            viewport: geom.viewport.0,
            max: geom.max.0,
            offset: geom.offset.0,
        })
    }

    /// The thumbs to paint over the document at `env.now` (none for a scroller that fits).
    pub(crate) fn overlay(&self, doc: &BaseDocument, now: Elapsed) -> Vec<ThumbPaint> {
        let tuning = self.tuning;
        let env = tuning.at(now);
        let s = env.scrollbar;
        let vis = env.kit.settings.scrollbars;
        self.metrics(doc)
            .filter(|m| m.max > 0.0 || m.offset != 0.0)
            .and_then(|m| self.indicator.paint(&m, s, vis, env.now))
            .into_iter()
            .collect()
    }

    /// Publish every watched scroller's state (after the frame's resolve).
    pub(crate) fn publish(&self, doc: &BaseDocument) {
        for (id, live) in self.board.watched() {
            let Some(scroller) = dom::scroller_by_id(doc, &id) else {
                continue;
            };
            let (Some(x), Some(y)) = (
                dom::geom(doc, scroller, ScrollAxis::X),
                dom::geom(doc, scroller, ScrollAxis::Y),
            ) else {
                continue;
            };
            live.publish(ScrollState {
                offset: ScrollPoint {
                    x: x.offset.0,
                    y: y.offset.0,
                },
                extent: ScrollExtent {
                    content_width: x.max.0 + x.viewport.0,
                    content_height: y.max.0 + y.viewport.0,
                    viewport_width: x.viewport.0,
                    viewport_height: y.viewport.0,
                },
                phase: phase_of(self.scroll.engine(), scroller),
            });
        }
    }
}

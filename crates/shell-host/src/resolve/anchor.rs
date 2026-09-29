//! Resolving a popup's anchor rectangle.

use crate::popup::PopupAnchor;
use crate::region::{ElementId, Rect};

/// Resolve a popup's anchor rectangle: `PopupAnchor::Rect` uses `placement_rect` (the value
/// carried alongside the config) unchanged; `PopupAnchor::Element` looks the
/// element up the same way [`resolve`] does, falling back to `placement_rect` while the element
/// is not yet laid out (a popup has to anchor somewhere even on its very first configure).
pub fn resolve_popup_anchor(
    anchor: &PopupAnchor,
    placement_rect: Rect,
    lookup: impl Fn(ElementId) -> Option<Rect>,
) -> Rect {
    match anchor {
        PopupAnchor::Rect => placement_rect,
        PopupAnchor::Element(id) => lookup(id.clone()).unwrap_or(placement_rect),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PILL_RECT: Rect = Rect {
        x: 20,
        y: 8,
        width: 100,
        height: 24,
    };

    fn pill() -> ElementId {
        ElementId::new("dock-pill")
    }

    fn lookup(id: ElementId) -> Option<Rect> {
        (id == pill()).then_some(PILL_RECT)
    }

    #[test]
    fn popup_anchor_resolves_an_element_or_falls_back_to_the_placement_rect() {
        let placement = Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        };
        let cases: &[(&str, PopupAnchor, Rect)] = &[
            (
                "explicit rect uses the placement rect",
                PopupAnchor::Rect,
                placement,
            ),
            (
                "element uses its own rect",
                PopupAnchor::Element(pill()),
                PILL_RECT,
            ),
            (
                "unresolved element falls back to the placement rect",
                PopupAnchor::Element(ElementId::new("no-such-element")),
                placement,
            ),
        ];
        for (name, anchor, want) in cases {
            assert_eq!(
                resolve_popup_anchor(anchor, placement, lookup),
                *want,
                "{name}"
            );
        }
    }
}

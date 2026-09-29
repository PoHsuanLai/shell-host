//! Whether an element opts into receiving the click that activates its window.

use blitz_dom::Document as _;

use super::SurfaceDocument;

/// Whether an element opted into receiving the click that activates its window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstMouseOptIn {
    OptedIn,
    Default,
}

impl SurfaceDocument {
    /// Whether the element at a surface-local point, or an ancestor, carries
    /// `data-first-mouse` (design/13 §13.3.8).
    pub fn wants_first_mouse(&self, x: f64, y: f64) -> FirstMouseOptIn {
        let inner = self.doc.inner();
        let Some(mut node_id) = inner.element_from_point(x as f32, y as f32) else {
            return FirstMouseOptIn::Default;
        };
        loop {
            let Some(node) = inner.get_node(node_id) else {
                return FirstMouseOptIn::Default;
            };
            let opted_in = node.element_data().is_some_and(|data| {
                data.attrs
                    .iter()
                    .any(|attr| *attr.name.local == *"data-first-mouse")
            });
            if opted_in {
                return FirstMouseOptIn::OptedIn;
            }
            match node.parent {
                Some(parent) => node_id = parent,
                None => return FirstMouseOptIn::Default,
            }
        }
    }
}

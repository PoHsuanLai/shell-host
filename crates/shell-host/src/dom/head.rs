//! The `dioxus_document::Document` shim. `DioxusDocument` does not process `document::Style`,
//! `Link` or `Title`; this shim queues each `create_head_element` and the host applies the
//! queue to the Blitz `<head>` after every poll.

use std::cell::RefCell;
use std::rc::Rc;

use dioxus_document::{Document, Eval, NoOpDocument};

/// One element Dioxus asked for in the `<head>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadElement {
    pub name: String,
    pub attributes: Vec<(String, String)>,
    pub contents: Option<String>,
}

/// The queue the shim fills and the host drains.
#[derive(Debug, Clone, Default)]
pub struct HeadQueue(Rc<RefCell<Vec<HeadElement>>>);

impl HeadQueue {
    /// Record one queued element.
    fn push(&self, element: HeadElement) {
        self.0.borrow_mut().push(element);
    }

    /// Everything queued since the last drain, in order.
    pub fn drain(&self) -> Vec<HeadElement> {
        std::mem::take(&mut *self.0.borrow_mut())
    }
}

/// The `Document` a surface's `VirtualDom` gets as its document context.
#[derive(Debug, Clone)]
pub struct HeadDocument {
    queue: HeadQueue,
}

impl HeadDocument {
    /// A shim filling `queue`.
    pub fn new(queue: HeadQueue) -> HeadDocument {
        HeadDocument { queue }
    }
}

impl Document for HeadDocument {
    /// There is no JavaScript: every eval fails, exactly as the crate's own no-op document.
    fn eval(&self, js: String) -> Eval {
        NoOpDocument.eval(js)
    }

    fn set_title(&self, title: String) {
        self.queue.push(HeadElement {
            name: "title".to_owned(),
            attributes: Vec::new(),
            contents: Some(title),
        });
    }

    fn create_head_element(
        &self,
        name: &str,
        attributes: &[(&str, String)],
        contents: Option<String>,
    ) {
        self.queue.push(HeadElement {
            name: name.to_owned(),
            attributes: attributes
                .iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone()))
                .collect(),
            contents,
        });
    }
}

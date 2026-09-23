//! The `dioxus_document::Document` shim. `DioxusDocument` does not process `document::Style`,
//! `Link` or `Title`; this shim queues each `create_head_element` and the host applies the
//! queue to the Blitz `<head>` after every poll.
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use std::cell::RefCell;
use std::rc::Rc;

use dioxus_document::{Document, Eval};

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
    /// Everything queued since the last drain, in order.
    pub fn drain(&self) -> Vec<HeadElement> {
        todo!()
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
        todo!()
    }
}

impl Document for HeadDocument {
    /// There is no JavaScript: every eval fails.
    fn eval(&self, js: String) -> Eval {
        todo!()
    }

    fn set_title(&self, title: String) {
        todo!()
    }

    fn create_head_element(
        &self,
        name: &str,
        attributes: &[(&str, String)],
        contents: Option<String>,
    ) {
        todo!()
    }
}

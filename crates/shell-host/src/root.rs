//! The component a surface renders, with its props and root contexts. Each surface and popup is
//! its own `VirtualDom`, so a `Root` is consumed when its surface is created.
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use std::fmt;

use dioxus::core::{Element, VirtualDom};

/// Builds the `VirtualDom` for a root component and its props.
type MakeDom = Box<dyn FnOnce() -> VirtualDom>;

/// Provides one root context to a built `VirtualDom`.
type ProvideContext = Box<dyn FnOnce(&VirtualDom)>;

/// A root component waiting for its surface.
pub struct Root {
    make: MakeDom,
    contexts: Vec<ProvideContext>,
}

impl Root {
    /// A root with no props.
    pub fn new(component: fn() -> Element) -> Root {
        todo!()
    }

    /// A root with props.
    pub fn with_props<P: Clone + 'static>(component: fn(P) -> Element, props: P) -> Root {
        todo!()
    }

    /// Provide `value` as a root context, readable with `use_context::<T>()` anywhere in the
    /// surface. Shared data between surfaces travels as a `watch::Receiver` context read with
    /// `use_watch`, since a Signal belongs to one `VirtualDom`.
    pub fn context<T: Clone + 'static>(self, value: T) -> Root {
        todo!()
    }

    /// Build the `VirtualDom`, with every context provided, ready for `DioxusDocument::new`.
    pub(crate) fn into_vdom(self) -> VirtualDom {
        todo!()
    }
}

impl fmt::Debug for Root {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Root")
            .field("contexts", &self.contexts.len())
            .finish_non_exhaustive()
    }
}

//! Blitz documents as the host runs them: one `DioxusDocument` per surface, with the host's UA
//! stylesheet, head-element shim and shell provider, with `blitz-kit`'s shared fonts and local
//! (`data:` and `file:`) loader.

pub mod document;
pub mod head;
pub mod ime_focus;
pub mod scroll;
pub mod shell_provider;
pub(crate) mod stacking;

//! Blitz documents as the host runs them: one `DioxusDocument` per surface, with the host's UA
//! stylesheet, head-element shim, shell provider, shared fonts and local (`data:` and `file:`) loader.

pub mod document;
pub mod fonts;
pub mod head;
pub mod hover_sync;
pub mod ime_focus;
pub mod net;
pub mod painted;
pub mod scroll;
pub mod shell_provider;
pub(crate) mod stacking;

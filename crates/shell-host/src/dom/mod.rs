//! Blitz documents as the host runs them: one `DioxusDocument` per surface, with the host's UA
//! stylesheet, head-element shim, shell provider, shared fonts and data: URI loader.

pub mod document;
pub mod fonts;
pub mod head;
pub mod net;
pub mod shell_provider;

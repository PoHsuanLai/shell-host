//! Blitz documents as the host runs them: one `DioxusDocument` per surface, with the host's UA
//! stylesheet, head-element shim and shell provider, with `blitz-kit`'s shared fonts and local
//! (`data:` and `file:`) loader.

pub(crate) mod document;
pub(crate) mod head;
pub(crate) mod ime_focus;
pub(crate) mod scroll;
pub(crate) mod shell_provider;
pub(crate) mod stacking;

#[cfg(test)]
pub(crate) mod tests;

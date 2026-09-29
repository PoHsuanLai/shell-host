//! Input translation: Wayland seat events to Blitz `UiEvent`s. Pure, except that `pointer` and
//! `key` build values a handler then feeds to a document.

pub(crate) mod capability;
pub(crate) mod focus;
pub(crate) mod key;
pub(crate) mod keycode;
pub(crate) mod keysym;
pub(crate) mod modifiers;
pub(crate) mod palmrest;
pub(crate) mod pointer;
pub(crate) mod scroll;

//! Input translation: Wayland seat events to Blitz `UiEvent`s. Pure, except that `pointer` and
//! `key` build values a handler then feeds to a document.

pub mod capability;
pub mod key;
pub mod keycode;
pub mod keysym;
pub mod modifiers;
pub mod pointer;

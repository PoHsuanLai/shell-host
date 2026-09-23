//! Wayland event handlers: sctk handler traits implemented on `HostState`, and hand-written
//! `Dispatch` implementations for the protocols sctk 0.20 does not wrap (background effect,
//! fractional scale + viewporter, text input). The delegate macros that route to them are all
//! in `state.rs`.

pub mod activation;
pub mod background_effect;
pub mod compositor;
pub mod data_device;
pub mod fractional;
pub mod keyboard;
pub mod layer;
pub mod output;
pub mod pointer;
pub mod seat;
pub mod text_input;
pub mod xdg;

//! The `debug-inject` feature: synthetic input and offscreen captures on one surface, as a
//! typed API that returns its results ([`HostCtx::inject`]), and the stdin command source the
//! demos are driven by ([`stdin`], which maps each line onto that API and prints the result).
//! Frame statistics are not an injection: [`HostCtx::stats`] and [`HostCtx::take_stats`] are
//! the one source of them, with or without this feature.
//!
//! See FINDINGS F78.

mod command;
mod deliver;
mod key;
mod key_spec;
mod request;
pub mod stdin;

pub use command::{CommandError, DebugAction, DebugCommand, parse_command};
pub use key_spec::parse_key;
pub use request::{Inject, Injected, KeyAction, KeyStroke, PointerButton};

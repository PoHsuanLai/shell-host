//! The `debug-inject` feature: debug commands on stdin (`debug::parse_command`), one JSON line
//! of `FrameStats` per surface for `stats`, and `capture` through the presenter's offscreen
//! render. The real-API successor of the spike's command source; examples/demo_*.rs use it.
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use crate::ctx::HostCtx;
use crate::debug::DebugCommand;
use crate::error::Result;

/// Add a stdin source to the loop that parses each line and applies it. EOF on stdin exits.
pub fn install<M: 'static>(ctx: &mut HostCtx<M>) -> Result<()> {
    todo!()
}

/// Apply one command: synthesize the pointer events on the named surface (`input::pointer`),
/// print stats, write a capture, or exit.
pub fn apply<M: 'static>(ctx: &mut HostCtx<M>, command: DebugCommand) -> Result<()> {
    todo!()
}

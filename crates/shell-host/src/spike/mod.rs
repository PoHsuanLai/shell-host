//! Milestone 0 spike (throwaway): Blitz on a layer-shell surface, one GPU device for all
//! surfaces, frame-callback pacing, compositor blur and fractional scale. Decides go/no-go;
//! the real host is written from the findings in `SPIKE.md`, not from this code.
#![allow(dead_code, missing_debug_implementations, clippy::todo)]

pub mod app;
pub mod block_on;
pub mod capture;
pub mod cmd;
pub mod doc;
pub mod gpu;
pub mod probe;
pub mod protocols;
pub mod raw_handle;
pub mod render;
pub mod stats;
pub mod surface;

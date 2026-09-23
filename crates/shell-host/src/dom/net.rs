//! A `NetProvider` that resolves `data:` URIs and nothing else. Blitz loads `@font-face`,
//! `mask-image` and background images only through a net provider (quire FINDINGS S7, S8); a
//! shell surface never fetches from the network.
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use blitz_traits::net::{NetHandler, NetProvider, Request};

/// The data: URI loader.
#[derive(Debug, Clone, Copy, Default)]
pub struct DataUriNet;

impl NetProvider for DataUriNet {
    fn fetch(&self, doc_id: usize, request: Request, handler: Box<dyn NetHandler>) {
        todo!()
    }
}

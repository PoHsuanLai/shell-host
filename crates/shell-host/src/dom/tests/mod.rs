//! Documents built from components and driven without a compositor: the environment every test
//! shares, the document's own behaviour, and the pointer and geometry rules on a real Blitz tree.

pub(crate) mod env;

mod body_margin;
mod hoisted;
mod pointer;
mod region_transform;
mod scheme_follow;
mod surface_document;

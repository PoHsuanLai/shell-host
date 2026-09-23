//! Compositor blur behind a surface (`ext_background_effect_v1`). In-surface blur is impossible
//! on every Vello backend (`backdrop-filter` is dropped), so this is the only blur there is.
//!
//! The region geometry is shell-host's: quire exposes only the intent (`Material::blur()`), and
//! the shell turns it into a `BlurSpec`. Resolution to rectangles is `resolve::resolve`; binding
//! the protocol is `handlers::background_effect`.

use crate::region::RegionSpec;

/// Whether and where a surface is blurred.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlurSpec {
    /// No blur (also what a compositor without the protocol, or without the blur capability,
    /// gets regardless of the spec).
    None,
    /// Blur behind this region, resolved after layout like an input region.
    Region(RegionSpec),
}

//! Sizes as a role asks for them and as the wire wants them.

use crate::layer::{LayerConfig, LayerExtent};
use crate::region::Px;
use crate::scale::Size;

/// The size a layer asks for; a `Fill` axis is 0 until the compositor configures it.
pub(super) fn requested_size(config: &LayerConfig) -> Size {
    let px = |extent| match extent {
        LayerExtent::Fill => 0,
        LayerExtent::Px(px) => Px::try_from(px).unwrap_or(Px::MAX),
    };
    Size {
        width: px(config.size.width),
        height: px(config.size.height),
    }
}

pub(super) fn wire_size(size: Size) -> (u32, u32) {
    let px = |v: Px| u32::try_from(v).unwrap_or(0);
    (px(size.width), px(size.height))
}

//! What the host reports about an output.

use crate::id::OutputId;
use crate::region::Rect;
use crate::scale::Scale120;

/// One connected output, as of its last `wl_output.done`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputInfo {
    pub id: OutputId,
    /// The connector name (`HDMI-A-2`), when the compositor sends one (wl_output v4).
    pub name: Option<String>,
    /// The human-readable description, when the compositor sends one.
    pub description: Option<String>,
    /// Position and size in the compositor's logical space.
    pub logical: Rect,
    /// The output's scale. For an integer-only compositor this is `buffer_scale * 120`; surfaces
    /// on fractional-scale compositors get their own `preferred_scale`, which wins.
    pub scale: Scale120,
    /// Refresh rate in millihertz (`60000` for 60 Hz); 0 when unknown.
    pub refresh_mhz: u32,
}

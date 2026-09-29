//! Outputs appearing, changing and going: `HostEvent::Output*`.

use smithay_client_toolkit::output::{OutputHandler, OutputState};
use wayland_client::protocol::wl_output;
use wayland_client::{Connection, QueueHandle};

use crate::event::HostEvent;
use crate::id::OutputId;
use crate::lock::machine::LockInput;
use crate::output::OutputInfo;
use crate::region::Rect;
use crate::scale::Scale;
use crate::state::HostState;

impl OutputHandler for HostState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        let Some(sctk_info) = self.outputs.info(&output) else {
            return;
        };
        self.lock_input(LockInput::OutputAdded(OutputId(sctk_info.id)));
        self.events.push_back(HostEvent::OutputAdded(output_info(
            sctk_info.id,
            &sctk_info,
        )));
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        let Some(sctk_info) = self.outputs.info(&output) else {
            return;
        };
        self.events.push_back(HostEvent::OutputChanged(output_info(
            sctk_info.id,
            &sctk_info,
        )));
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        // "The info passed to this function was the state of the output before destruction"
        // (`OutputHandler::output_destroyed`'s own doc): the registry name is still in it.
        let Some(sctk_info) = self.outputs.info(&output) else {
            return;
        };
        self.lock_input(LockInput::OutputRemoved(OutputId(sctk_info.id)));
        self.events
            .push_back(HostEvent::OutputRemoved(OutputId(sctk_info.id)));
    }
}

/// The host's view of an sctk output, by its registry name. Pure (delegates the actual
/// fallback rules to [`resolve_output_info`]).
pub(crate) fn output_info(
    name: u32,
    info: &smithay_client_toolkit::output::OutputInfo,
) -> OutputInfo {
    let current_mode = info.modes.iter().find(|m| m.current);
    resolve_output_info(
        OutputId(name),
        OutputFacts {
            name: info.name.clone(),
            description: info.description.clone(),
            location: info.location,
            logical_position: info.logical_position,
            logical_size: info.logical_size,
            current_mode_dimensions: current_mode.map(|m| m.dimensions),
            current_mode_refresh_mhz: current_mode.map(|m| m.refresh_rate),
            scale_factor: info.scale_factor,
        },
    )
}

/// Everything [`output_info`] needs from sctk's own `OutputInfo`, as plain values. sctk's type
/// is `#[non_exhaustive]`, so it cannot be built outside sctk's own crate — not even with every
/// field named — which would make the fallback rules below untestable from here without this
/// extraction.
struct OutputFacts {
    name: Option<String>,
    description: Option<String>,
    location: (i32, i32),
    logical_position: Option<(i32, i32)>,
    logical_size: Option<(i32, i32)>,
    current_mode_dimensions: Option<(i32, i32)>,
    current_mode_refresh_mhz: Option<i32>,
    scale_factor: i32,
}

/// The pure half of [`output_info`]: xdg-output's logical position/size win over the core
/// protocol's `location` and the current mode's pixel dimensions; the current mode's refresh
/// rate is used as-is, defaulting to 0 (unknown) when there is no current mode; the scale is
/// never allowed to resolve to 0 (a `Scale` of zero would collapse every buffer to nothing).
fn resolve_output_info(id: OutputId, facts: OutputFacts) -> OutputInfo {
    let (x, y) = facts.logical_position.unwrap_or(facts.location);
    let (width, height) = facts
        .logical_size
        .or(facts.current_mode_dimensions)
        .unwrap_or((0, 0));
    let refresh_mhz = facts
        .current_mode_refresh_mhz
        .map(|r| r.max(0) as u32)
        .unwrap_or(0);
    OutputInfo {
        id,
        name: facts.name,
        description: facts.description,
        logical: Rect {
            x,
            y,
            width,
            height,
        },
        scale: Scale(facts.scale_factor.max(1) as u32 * 120),
        refresh_mhz,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> OutputFacts {
        OutputFacts {
            name: None,
            description: None,
            location: (0, 0),
            logical_position: None,
            logical_size: None,
            current_mode_dimensions: None,
            current_mode_refresh_mhz: None,
            scale_factor: 1,
        }
    }

    #[test]
    fn prefers_xdg_output_logical_geometry_over_the_core_protocol_fields() {
        let got = resolve_output_info(
            OutputId(3),
            OutputFacts {
                name: Some("HDMI-A-2".to_owned()),
                description: Some("A monitor".to_owned()),
                location: (10, 20),
                logical_position: Some((30, 40)),
                logical_size: Some((1920, 1080)),
                current_mode_dimensions: Some((3840, 2160)),
                current_mode_refresh_mhz: Some(60_000),
                scale_factor: 2,
            },
        );
        assert_eq!(got.id, OutputId(3));
        assert_eq!(got.name.as_deref(), Some("HDMI-A-2"));
        assert_eq!(got.description.as_deref(), Some("A monitor"));
        assert_eq!(
            got.logical,
            Rect {
                x: 30,
                y: 40,
                width: 1920,
                height: 1080
            }
        );
        assert_eq!(got.scale, Scale(240));
        assert_eq!(got.refresh_mhz, 60_000);
    }

    #[test]
    fn falls_back_to_location_and_the_current_mode_without_xdg_output() {
        let got = resolve_output_info(
            OutputId(7),
            OutputFacts {
                location: (5, 6),
                current_mode_dimensions: Some((1280, 720)),
                current_mode_refresh_mhz: Some(59_940),
                ..facts()
            },
        );
        assert_eq!(
            got.logical,
            Rect {
                x: 5,
                y: 6,
                width: 1280,
                height: 720
            }
        );
        assert_eq!(got.refresh_mhz, 59_940);
    }

    #[test]
    fn no_current_mode_is_an_unknown_refresh_rate_and_a_zero_size() {
        let got = resolve_output_info(OutputId(1), facts());
        assert_eq!(got.refresh_mhz, 0);
        assert_eq!(
            got.logical,
            Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0
            }
        );
    }

    #[test]
    fn a_scale_factor_of_zero_or_less_never_produces_a_zero_scale() {
        let got = resolve_output_info(
            OutputId(1),
            OutputFacts {
                scale_factor: 0,
                ..facts()
            },
        );
        assert_eq!(got.scale, Scale(120));
    }
}

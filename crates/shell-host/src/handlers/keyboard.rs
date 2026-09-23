//! Keyboard focus, keys and modifiers -> document events, through `input::key`.

use std::time::{Duration, Instant};

use blitz_traits::events::{BlitzKeyEvent, KeyState, UiEvent};
use smithay_client_toolkit::seat::keyboard::{
    KeyEvent, KeyboardHandler, Keysym, Modifiers as SctkModifiers, RawModifiers, RepeatInfo,
};
use wayland_client::protocol::{wl_keyboard, wl_surface};
use wayland_client::{Connection, QueueHandle};

use crate::event::{Focus, HostEvent};
use crate::id::SurfaceId;
use crate::ime::ImeFocus;
use crate::input::key::{self, Composing, Repeat};
use crate::input::modifiers::modifiers_from;
use crate::state::HostState;
use crate::timers::TimerRequest;

impl KeyboardHandler for HostState {
    /// `HostEvent::KeyboardFocus{Gained}`, remember the time for first-mouse.
    fn enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _serial: u32,
        _raw: &[u32],
        _keysyms: &[Keysym],
    ) {
        let Some(id) = self.surface_id_for(surface) else {
            return;
        };
        self.input.keyboard_focus = Some(id);
        self.input.focused_at = Some(Instant::now());
        self.events.push_back(HostEvent::KeyboardFocus {
            id,
            focus: Focus::Gained,
        });
    }

    /// `HostEvent::KeyboardFocus{Lost}`, stop repeating.
    fn leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _serial: u32,
    ) {
        let Some(id) = self.surface_id_for(surface) else {
            return;
        };
        if self.input.keyboard_focus == Some(id) {
            self.input.keyboard_focus = None;
            self.input.focused_at = None;
        }
        self.input.repeating = None;
        self.timers.push(TimerRequest::RepeatStop);
        self.events.push_back(HostEvent::KeyboardFocus {
            id,
            focus: Focus::Lost,
        });
    }

    fn press_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        serial: u32,
        event: KeyEvent,
    ) {
        self.input.last_serial = serial;
        let Some(id) = self.input.keyboard_focus else {
            return;
        };
        let blitz_event = key::key_event(
            &event,
            self.input.modifiers,
            KeyState::Pressed,
            Repeat::First,
            self.composing(),
        );
        self.dispatch_key(id, blitz_event);

        // Start (or restart, replacing any key already repeating) the repeat timer; the
        // context re-synthesizes `Repeat::Repeated` presses from `self.input.repeating` on
        // every firing (F9: sctk's own repeat source is not used, since it must name the
        // calloop data type and `delegate_keyboard!` names the non-generic `HostState`).
        self.input.repeating = Some(event);
        self.timers.push(TimerRequest::RepeatStart {
            delay: self.input.repeat_delay,
            interval: self.input.repeat_interval,
        });
    }

    /// Unused: the host does not use sctk's repeat source (F9).
    fn repeat_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        _event: KeyEvent,
    ) {
    }

    fn release_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: KeyEvent,
    ) {
        let is_repeating = self
            .input
            .repeating
            .as_ref()
            .is_some_and(|held| held.raw_code == event.raw_code);
        if is_repeating {
            self.input.repeating = None;
            self.timers.push(TimerRequest::RepeatStop);
        }
        let Some(id) = self.input.keyboard_focus else {
            return;
        };
        let blitz_event = key::key_event(
            &event,
            self.input.modifiers,
            KeyState::Released,
            Repeat::First,
            self.composing(),
        );
        self.dispatch_key(id, blitz_event);
    }

    fn update_modifiers(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        serial: u32,
        modifiers: SctkModifiers,
        _raw_modifiers: RawModifiers,
        _layout: u32,
    ) {
        self.input.last_serial = serial;
        self.input.modifiers = modifiers_from(modifiers);
    }

    fn update_repeat_info(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        info: RepeatInfo,
    ) {
        match info {
            RepeatInfo::Repeat { rate, delay } => {
                self.input.repeat_delay = Duration::from_millis(u64::from(delay));
                self.input.repeat_interval = Duration::from_secs_f64(1.0 / f64::from(rate.get()));
            }
            RepeatInfo::Disable => {
                self.input.repeating = None;
                self.timers.push(TimerRequest::RepeatStop);
            }
        }
    }
}

impl HostState {
    /// Whether the focused surface's IME is enabled right now (`input::key::Composing`'s best
    /// available signal: `Ime` tracks enablement, not whether a preedit is currently showing).
    fn composing(&self) -> Composing {
        match self.ime.focus {
            ImeFocus::Enabled { .. } => Composing::Active,
            ImeFocus::Disabled => Composing::Idle,
        }
    }

    /// Feed `event` to surface `id`'s document and invalidate it.
    fn dispatch_key(&mut self, id: SurfaceId, event: BlitzKeyEvent) {
        let ui_event = match event.state {
            KeyState::Pressed => UiEvent::KeyDown(event),
            KeyState::Released => UiEvent::KeyUp(event),
        };
        if let Some(surface) = self.surfaces.get_mut(&id) {
            surface.document.handle_event(ui_event);
        }
        self.invalidate(id);
    }
}

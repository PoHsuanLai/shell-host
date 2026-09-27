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
use crate::input::focus::{self, Departure};
use crate::input::key::{self, Composing, Repeat};
use crate::input::modifiers::modifiers_from;
use crate::modality::{Stroke, key_kind};
use crate::modifiers::Modifiers;
use crate::scroll::surface::KeyUse;
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
        // The seat's already-current modifiers (F66): a compositor sends a `modifiers` event
        // right after this one, but a surface that reads `use_modifiers` on its very first
        // render (an Exclusive overlay mapped while Super is held, a lock screen's initial Caps
        // Lock state) must not see empty in the meantime.
        if let Some(surface) = self.surfaces.get_mut(&id) {
            surface.live_modifiers.publish(self.input.held_modifiers);
        }
        self.events.push_back(HostEvent::KeyboardFocus {
            id,
            focus: Focus::Gained,
        });
    }

    /// `HostEvent::KeyboardFocus{Lost}`, stop repeating. A leave that names a surface the host
    /// no longer knows — its unmap or close already withdrew the keyboard from it, before the
    /// rebuild or removal took the `wl_surface` this leave names (FINDINGS F58, G210) — is
    /// ignored.
    fn leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _serial: u32,
    ) {
        let Some(id) = self.surface_id_for(surface) else {
            #[cfg(debug_assertions)]
            eprintln!("shell-host: wl_keyboard.leave for a surface no longer known (ignored)");
            return;
        };
        if self.depart_keyboard(id) == Departure::Lost {
            self.reset_keyboard_focus(id);
        }
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
        self.note_stroke(Stroke::KeyPress(key_kind(event.keysym)));
        let blitz_event = key::key_event(
            &event,
            self.input.modifiers,
            KeyState::Pressed,
            Repeat::First,
            self.composing(),
        );
        self.key_down(id, blitz_event, event.raw_code);

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
        if self.scroll_key_up(id, event.raw_code) != KeyUse::Passed {
            // Its press went to the scroll engine, so its release does too.
            return;
        }
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
        // Blitz's own key events (unchanged path): `key::key_event` reads `self.input.modifiers`.
        self.input.modifiers = modifiers_from(modifiers);
        // `use_modifiers` (F66): the seat-wide cache, and the focused surface's live copy of it.
        // The compositor may send this with no surface of ours focused (F61), so only the
        // second write is conditional.
        self.input.held_modifiers = Modifiers::from_sctk(modifiers);
        if let Some(id) = self.input.keyboard_focus
            && let Some(surface) = self.surfaces.get_mut(&id)
        {
            surface.live_modifiers.publish(self.input.held_modifiers);
        }
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
    pub(crate) fn composing(&self) -> Composing {
        match self.ime.focus {
            ImeFocus::Enabled { .. } => Composing::Active,
            ImeFocus::Disabled => Composing::Idle,
        }
    }

    /// A press or repeat for surface `id`: a scroll key the engine takes (design/11 §11.3.10:
    /// focus not in an editable, a form control or a `data-keys="capture"` element) never
    /// reaches the document; anything else is dispatched.
    pub(crate) fn key_down(&mut self, id: SurfaceId, event: BlitzKeyEvent, raw: u32) {
        if self.scroll_key_down(id, &event, raw) == KeyUse::Passed {
            self.dispatch_key(id, event);
        }
    }

    /// Feed `event` to surface `id`'s document and invalidate it.
    pub(crate) fn dispatch_key(&mut self, id: SurfaceId, event: BlitzKeyEvent) {
        let ui_event = match event.state {
            KeyState::Pressed => UiEvent::KeyDown(event),
            KeyState::Released => UiEvent::KeyUp(event),
        };
        if let Some(surface) = self.surfaces.get_mut(&id) {
            surface.document.handle_event(ui_event);
        }
        self.invalidate(id);
    }

    /// Surface `id` is being unmapped or closed by the host, possibly holding the keyboard
    /// focus. If it does, end the stay now, the same way `withdraw_pointer` ends the pointer's
    /// (FINDINGS F58): an unmap rebuilds the `wl_surface` and a close destroys it, so the
    /// compositor's later `wl_keyboard.leave` cannot be matched to anything (G210); the host
    /// resets its own record here instead of waiting for it. A later `enter` on a remapped
    /// surface proceeds normally and publishes the seat's current modifiers (F66).
    pub(crate) fn withdraw_keyboard(&mut self, id: SurfaceId) {
        if self.depart_keyboard(id) == Departure::Lost {
            self.reset_keyboard_focus(id);
        }
    }

    /// Update the keyboard-focus record for surface `id` departing (a `leave`, an unmap or a
    /// close): shared by `leave` and `withdraw_keyboard` so both take the record through the
    /// same pure transition.
    fn depart_keyboard(&mut self, id: SurfaceId) -> Departure {
        let (after, departure) = focus::depart_keyboard(self.input.keyboard_focus, id);
        self.input.keyboard_focus = after;
        departure
    }

    /// The effects of surface `id` losing the keyboard, once it is confirmed lost: forget when
    /// it was focused, stop any repeat, reset its published modifiers to the default (F66) and
    /// tell the shell once.
    fn reset_keyboard_focus(&mut self, id: SurfaceId) {
        self.input.focused_at = None;
        self.input.repeating = None;
        self.timers.push(TimerRequest::RepeatStop);
        if let Some(surface) = self.surfaces.get_mut(&id) {
            surface.live_modifiers.publish(Modifiers::default());
        }
        self.events.push_back(HostEvent::KeyboardFocus {
            id,
            focus: Focus::Lost,
        });
    }
}

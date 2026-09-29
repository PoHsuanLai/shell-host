//! The seat's device objects (`wl_pointer` with its cursor-shape device, `wl_keyboard`): made
//! once per capability and released when it goes, as `input::capability` decides.
//! An object that is dropped without `release` stays live on the compositor's side and
//! keeps delivering every press, so every path that forgets one releases it here.

use blitz_traits::events::MouseEventButtons;
use keyboard_types::Modifiers;
use wayland_client::protocol::{wl_keyboard, wl_pointer, wl_seat};
use wayland_client::{Proxy, QueueHandle};

use crate::csd::Zone;
use crate::event::{HostEvent, KeyboardFocus};
use crate::input::capability::CapabilityChange;
use crate::input::focus::PointerFocus;
use crate::input::pointer::PointerPos;
use crate::state::HostState;
use crate::timers::TimerRequest;

/// `wl_pointer.release` and `wl_keyboard.release` exist from `wl_seat` v3; before that the only
/// way to let go of the object is to drop it.
const RELEASE_SINCE: u32 = 3;

impl HostState {
    /// Act on `change` for the pointer capability of `seat`.
    pub(crate) fn change_pointer(
        &mut self,
        change: CapabilityChange,
        qh: &QueueHandle<Self>,
        seat: &wl_seat::WlSeat,
    ) {
        match change {
            CapabilityChange::New => self.make_pointer(qh, seat),
            CapabilityChange::Removed => self.release_pointer(),
            CapabilityChange::Repeat | CapabilityChange::Absent => {}
        }
    }

    /// Act on `change` for the keyboard capability of `seat`.
    pub(crate) fn change_keyboard(
        &mut self,
        change: CapabilityChange,
        qh: &QueueHandle<Self>,
        seat: &wl_seat::WlSeat,
    ) {
        match change {
            CapabilityChange::New => self.make_keyboard(qh, seat),
            CapabilityChange::Removed => self.release_keyboard(),
            CapabilityChange::Repeat | CapabilityChange::Absent => {}
        }
    }

    /// `get_pointer` plus the cursor-shape device.
    fn make_pointer(&mut self, qh: &QueueHandle<Self>, seat: &wl_seat::WlSeat) {
        let Ok(pointer) = self.seat.get_pointer(qh, seat) else {
            return;
        };
        if let Some(manager) = &self.cursor.manager {
            self.cursor.device = Some(manager.get_shape_device(&pointer, qh));
        }
        self.cursor.current = None;
        self.input.pointer = Some(pointer);
    }

    /// `get_keyboard` without sctk's repeat (the repeat timer is the context's, via
    /// `TimerRequest::Repeat*`), plus the text input and the data device. Those two belong to
    /// the seat, not to the keyboard, so a keyboard that comes back reuses them.
    fn make_keyboard(&mut self, qh: &QueueHandle<Self>, seat: &wl_seat::WlSeat) {
        if let Ok(keyboard) = self.seat.get_keyboard::<Self, Self>(qh, seat, None) {
            self.input.keyboard = Some(keyboard);
        }
        if let Some(text_input) = &mut self.text_input {
            text_input.attach(seat, qh);
        }
        if let (None, Some(manager)) = (&self.input.data_device, &self.data_device) {
            self.input.data_device = Some(manager.get_data_device(qh, seat));
        }
    }

    /// Destroy the cursor-shape device, release the pointer, and end what it was doing: the
    /// surface under it hears a leave, held buttons and the grab record are cleared.
    pub(crate) fn release_pointer(&mut self) {
        if let Some(device) = self.cursor.device.take() {
            device.destroy();
        }
        self.cursor.current = None;
        if let Some(pointer) = self.input.pointer.take() {
            release_pointer_object(pointer);
        }
        let focus = std::mem::replace(&mut self.input.pointer_focus, PointerFocus::Nowhere);
        if let Some(id) = focus.surface() {
            if let Some(surface) = self.surfaces.get_mut(&id) {
                surface.document.pointer_left(self.input.modifiers);
            }
            self.invalidate(id);
        }
        self.input.position = PointerPos::OUTSIDE;
        self.input.buttons = MouseEventButtons::None;
        self.input.zone = Zone::Document;
        self.input.grab.update(|facts| *facts = Default::default());
    }

    /// Release the keyboard and end what it was doing: the focused surface loses focus, the
    /// repeating key stops, the modifiers are forgotten (the next keyboard sends its own).
    pub(crate) fn release_keyboard(&mut self) {
        if let Some(keyboard) = self.input.keyboard.take() {
            release_keyboard_object(keyboard);
        }
        if let Some(id) = self.input.keyboard_focus.take() {
            self.events.push_back(HostEvent::KeyboardFocus {
                id,
                focus: KeyboardFocus::Lost,
            });
        }
        self.input.focused_at = None;
        self.input.modifiers = Modifiers::empty();
        self.input.repeating = None;
        self.timers.push(TimerRequest::RepeatStop);
    }
}

fn release_pointer_object(pointer: wl_pointer::WlPointer) {
    if pointer.version() >= RELEASE_SINCE {
        pointer.release();
    }
}

fn release_keyboard_object(keyboard: wl_keyboard::WlKeyboard) {
    if keyboard.version() >= RELEASE_SINCE {
        keyboard.release();
    }
}

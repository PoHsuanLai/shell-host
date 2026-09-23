//! Keyboard focus, keys and modifiers -> document events, through `input::key`.
#![allow(unused_variables, dead_code)] // Freeze stubs: remove with the last todo!().

use smithay_client_toolkit::seat::keyboard::{
    KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers, RepeatInfo,
};
use wayland_client::protocol::{wl_keyboard, wl_surface};
use wayland_client::{Connection, QueueHandle};

use crate::state::HostState;

impl KeyboardHandler for HostState {
    /// `HostEvent::KeyboardFocus{Gained}`, remember the time for first-mouse.
    fn enter(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        serial: u32,
        raw: &[u32],
        keysyms: &[Keysym],
    ) {
        todo!()
    }

    /// `HostEvent::KeyboardFocus{Lost}`, stop repeating.
    fn leave(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        serial: u32,
    ) {
        todo!()
    }

    fn press_key(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        serial: u32,
        event: KeyEvent,
    ) {
        todo!()
    }

    /// Unused: the host does not use sctk's repeat source.
    fn repeat_key(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        serial: u32,
        event: KeyEvent,
    ) {
        todo!()
    }

    fn release_key(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        serial: u32,
        event: KeyEvent,
    ) {
        todo!()
    }

    fn update_modifiers(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        serial: u32,
        modifiers: Modifiers,
        raw_modifiers: RawModifiers,
        layout: u32,
    ) {
        todo!()
    }

    fn update_repeat_info(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        info: RepeatInfo,
    ) {
        todo!()
    }
}

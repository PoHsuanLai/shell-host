//! Synthetic pointer and keyboard input for a nested KWin through `org_kde_kwin_fake_input`, read
//! as a tiny command language from stdin, one command per line:
//!
//! ```text
//! move X Y              absolute pointer position, global logical px
//! press left|right|middle
//! release left|right|middle
//! key KEYSYM down|up    an xkb keysym name (Return, a, Shift_L), in KWin's own keymap
//! type "text"           each character pressed and released, with Shift when the keymap needs it
//! axis vertical|horizontal VALUE   one `axis` request: VALUE in wl_pointer axis units (KWin
//!                       turns it into a wheel event; 15 is one detent on libinput's scale)
//! sleep MS
//! ```
//!
//! Prints `ok <line>` after each command has reached the compositor (a roundtrip), or `err <line>:
//! <why>` and carries on. Authorisation: KWin 6 advertises the global only to a
//! client it trusts (`X-KDE-Wayland-Interfaces` in the client's .desktop file) or to every client
//! when the compositor runs with `KWIN_WAYLAND_NO_PERMISSION_CHECKS=1`, which is what
//! `scripts/dev/kwin-virtual.sh` sets; `authenticate` is then still sent first, as the protocol
//! asks, but KWin grants it without a prompt.

use std::io::BufRead;
use std::os::fd::OwnedFd;
use std::os::unix::fs::FileExt;

use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_keyboard, wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};
use wayland_protocols_plasma::fake_input::client::org_kde_kwin_fake_input::OrgKdeKwinFakeInput;
use xkbcommon::xkb;

/// evdev `KEY_LEFTSHIFT`, pressed around a character on the keymap's shift level.
const KEY_LEFTSHIFT: u32 = 42;

#[derive(Default)]
struct State {
    /// The compositor's keymap, from `wl_keyboard.keymap`.
    keymap: Option<xkb::Keymap>,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<OrgKdeKwinFakeInput, ()> for State {
    fn event(
        _: &mut Self,
        _: &OrgKdeKwinFakeInput,
        _: <OrgKdeKwinFakeInput as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_seat::WlSeat,
        _: wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_keyboard::Event::Keymap {
            format: WEnum::Value(wl_keyboard::KeymapFormat::XkbV1),
            fd,
            size,
        } = event
        {
            state.keymap = keymap_from(fd, size);
        }
    }
}

/// Compile the keymap the compositor sent. Read from offset 0 rather than mapped (no unsafe),
/// and positionally: KWin hands every client a descriptor of the same file, so the shared
/// offset may already be at its end.
fn keymap_from(fd: OwnedFd, size: u32) -> Option<xkb::Keymap> {
    let mut bytes = vec![0; size as usize];
    std::fs::File::from(fd).read_exact_at(&mut bytes, 0).ok()?;
    let text = String::from_utf8_lossy(&bytes)
        .trim_end_matches('\0')
        .to_owned();
    let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
    xkb::Keymap::new_from_string(
        &context,
        text,
        xkb::KEYMAP_FORMAT_TEXT_V1,
        xkb::KEYMAP_COMPILE_NO_FLAGS,
    )
}

/// Whether a key goes down or up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Change {
    Down,
    Up,
}

impl Change {
    fn wire(self) -> u32 {
        match self {
            Change::Down => 1,
            Change::Up => 0,
        }
    }
}

/// One parsed line.
#[derive(Debug, Clone, PartialEq)]
enum Command {
    Move(f64, f64),
    Button(u32, Change),
    Key(String, Change),
    Type(String),
    Axis(u32, f64),
    Sleep(u64),
}

/// The evdev code of a named button.
fn button(name: &str) -> Result<u32, String> {
    match name {
        "left" => Ok(0x110),
        "right" => Ok(0x111),
        "middle" => Ok(0x112),
        _ => Err(format!("unknown button {name:?}")),
    }
}

fn parse(line: &str) -> Result<Command, String> {
    let line = line.trim();
    if let Some(rest) = line.strip_prefix("type ") {
        let text = rest.trim();
        let text = text
            .strip_prefix('"')
            .and_then(|t| t.strip_suffix('"'))
            .unwrap_or(text);
        return Ok(Command::Type(text.to_owned()));
    }
    let words: Vec<&str> = line.split_whitespace().collect();
    let num = |s: &str| s.parse::<f64>().map_err(|e| format!("{s:?}: {e}"));
    let change = |s: &str| match s {
        "down" => Ok(Change::Down),
        "up" => Ok(Change::Up),
        _ => Err(format!("expected down or up, got {s:?}")),
    };
    match words.as_slice() {
        ["move", x, y] => Ok(Command::Move(num(x)?, num(y)?)),
        ["press", b] => Ok(Command::Button(button(b)?, Change::Down)),
        ["release", b] => Ok(Command::Button(button(b)?, Change::Up)),
        ["key", sym, c] => Ok(Command::Key((*sym).to_owned(), change(c)?)),
        ["axis", which, value] => {
            // wl_pointer.axis: 0 vertical, 1 horizontal.
            let axis = match *which {
                "vertical" => 0,
                "horizontal" => 1,
                _ => return Err(format!("expected vertical or horizontal, got {which:?}")),
            };
            Ok(Command::Axis(axis, num(value)?))
        }
        ["sleep", ms] => ms
            .parse()
            .map(Command::Sleep)
            .map_err(|e| format!("{ms:?}: {e}")),
        _ => Err("unknown command".to_owned()),
    }
}

/// The evdev code and shift level (0 or 1) that produce `sym` in the first layout of `keymap`.
fn locate(keymap: &xkb::Keymap, sym: xkb::Keysym) -> Option<(u32, u32)> {
    let (min, max) = (keymap.min_keycode().raw(), keymap.max_keycode().raw());
    for level in 0..2 {
        for raw in min..=max {
            let code = xkb::Keycode::new(raw);
            if level < keymap.num_levels_for_key(code, 0)
                && keymap.key_get_syms_by_level(code, 0, level).contains(&sym)
            {
                // xkb keycodes are evdev codes plus 8.
                return Some((raw - 8, level));
            }
        }
    }
    None
}

struct Driver {
    fake: OrgKdeKwinFakeInput,
    keymap: xkb::Keymap,
}

impl Driver {
    fn key(&self, code: u32, change: Change) {
        self.fake.keyboard_key(code, change.wire());
    }

    fn keysym(&self, sym: xkb::Keysym, change: Change) -> Result<(), String> {
        let (code, _) = locate(&self.keymap, sym).ok_or("no key produces it")?;
        self.key(code, change);
        Ok(())
    }

    fn type_char(&self, c: char) -> Result<(), String> {
        let sym = xkb::utf32_to_keysym(c as u32);
        let (code, level) =
            locate(&self.keymap, sym).ok_or_else(|| format!("no key types {c:?}"))?;
        if level == 1 {
            self.key(KEY_LEFTSHIFT, Change::Down);
        }
        self.key(code, Change::Down);
        self.key(code, Change::Up);
        if level == 1 {
            self.key(KEY_LEFTSHIFT, Change::Up);
        }
        Ok(())
    }

    fn run(&self, command: Command) -> Result<(), String> {
        match command {
            Command::Move(x, y) => self.fake.pointer_motion_absolute(x, y),
            Command::Button(code, change) => self.fake.button(code, change.wire()),
            Command::Key(name, change) => {
                let sym = xkb::keysym_from_name(&name, xkb::KEYSYM_NO_FLAGS);
                if sym.raw() == 0 {
                    return Err(format!("unknown keysym {name:?}"));
                }
                self.keysym(sym, change)?;
            }
            Command::Type(text) => {
                for c in text.chars() {
                    self.type_char(c)?;
                }
            }
            Command::Axis(axis, value) => self.fake.axis(axis, value),
            Command::Sleep(ms) => std::thread::sleep(std::time::Duration::from_millis(ms)),
        }
        Ok(())
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("fake-input: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let conn = Connection::connect_to_env().map_err(|e| e.to_string())?;
    let (globals, mut queue) = registry_queue_init::<State>(&conn).map_err(|e| e.to_string())?;
    let qh = queue.handle();
    let fake: OrgKdeKwinFakeInput = globals.bind(&qh, 4..=5, ()).map_err(|e| {
        format!(
            "org_kde_kwin_fake_input v4+ not offered ({e}); is KWin running with \
             KWIN_WAYLAND_NO_PERMISSION_CHECKS=1 (scripts/dev/kwin-virtual.sh)?"
        )
    })?;
    fake.authenticate("fake-input".into(), "shell-host acceptance input".into());
    let seat: wl_seat::WlSeat = globals.bind(&qh, 1..=9, ()).map_err(|e| e.to_string())?;
    seat.get_keyboard(&qh, ());
    let mut state = State::default();
    queue.roundtrip(&mut state).map_err(|e| e.to_string())?;
    queue.roundtrip(&mut state).map_err(|e| e.to_string())?;
    let keymap = state.keymap.take().ok_or("the seat sent no xkb keymap")?;
    let driver = Driver { fake, keymap };
    for line in std::io::stdin().lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let done = parse(&line).and_then(|command| driver.run(command));
        queue.roundtrip(&mut state).map_err(|e| e.to_string())?;
        match done {
            Ok(()) => println!("ok {line}"),
            Err(why) => println!("err {line}: {why}"),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_language_parses() {
        let cases = [
            ("move 10 20.5", Ok(Command::Move(10.0, 20.5))),
            ("press left", Ok(Command::Button(0x110, Change::Down))),
            ("release right", Ok(Command::Button(0x111, Change::Up))),
            (
                "key Return down",
                Ok(Command::Key("Return".into(), Change::Down)),
            ),
            ("type \"Hi there\"", Ok(Command::Type("Hi there".into()))),
            ("sleep 50", Ok(Command::Sleep(50))),
            ("axis vertical 15", Ok(Command::Axis(0, 15.0))),
            ("axis horizontal -7.5", Ok(Command::Axis(1, -7.5))),
        ];
        for (line, want) in cases {
            assert_eq!(parse(line), want, "{line}");
        }
        assert!(parse("press thumb").is_err());
        assert!(parse("key a sideways").is_err());
        assert!(parse("jump").is_err());
        assert!(parse("axis diagonal 1").is_err());
    }
}

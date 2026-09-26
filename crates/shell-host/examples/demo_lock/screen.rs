//! One output's lock document: the output's name, a plain password field that takes focus when
//! mounted (the compositor gives the lock surface the keyboard, so typing reaches it without a
//! click), and Enter checks the text. "letmein" unlocks through `use_lock().unlock()`; anything
//! else is refused and the field cleared. Real authentication is the shell's (PAM in sill).

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

use dioxus::prelude::*;
use shell_host::{LockPhase, use_lock, use_lock_phase};

use crate::say;

/// Opaque over the whole surface: the root's background paints the canvas, whatever the
/// content's height.
const CSS: &str = "html{background:rgb(24,28,38)}html,body{margin:0;padding:0}\
.lock{min-height:100vh;box-sizing:border-box;padding-top:30vh;\
color:white;font:20px sans-serif;display:flex;flex-direction:column;align-items:center;gap:16px}\
.field{width:320px;height:40px;font:20px sans-serif;padding:0 12px;border:2px solid white;\
border-radius:8px;background:rgb(40,46,60);color:white}";

/// The password the demo accepts.
const PASSWORD: &str = "letmein";

/// Per output: characters that reached the field, and the attempts.
#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
pub struct Typed {
    pub inputs: u32,
    pub length: usize,
    pub rejected: u32,
    pub accepted: u32,
}

/// What each output's document saw, by output name, for `stats`.
pub static TYPED: Mutex<BTreeMap<String, Typed>> = Mutex::new(BTreeMap::new());

fn record(output: &str, change: impl FnOnce(&mut Typed)) {
    if let Ok(mut typed) = TYPED.lock() {
        change(typed.entry(output.to_owned()).or_default());
    }
}

/// The props: which output this document is on.
#[derive(Debug, Clone, PartialEq)]
pub struct Screen {
    pub output: String,
}

pub fn lock_screen(screen: Screen) -> Element {
    let lock = use_lock();
    let phase = use_lock_phase();
    let mut value = use_signal(String::new);
    let mut field = use_signal(|| None::<std::rc::Rc<MountedData>>);
    let output = screen.output.clone();
    use_effect(move || {
        let Some(element) = field() else {
            return;
        };
        spawn(async move {
            // `set_focus` borrows the document when it is called; while the renderer holds
            // it, the call panics, so try again a moment later (quire's `guarded_call`).
            for _ in 0..20 {
                let call = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    element.set_focus(true)
                }));
                if let Ok(focusing) = call {
                    let _ = focusing.await;
                    return;
                }
                tokio::time::sleep(Duration::from_millis(16)).await;
            }
        });
    });
    record(&output, |_| {});
    let label = match phase() {
        LockPhase::Requested => "locking",
        LockPhase::Locked => "locked",
        LockPhase::Unlocking | LockPhase::Unlocked => "unlocking",
        LockPhase::Finished => "finished",
    };
    let for_input = output.clone();
    let for_key = output.clone();
    rsx! {
        style { "{CSS}" }
        div { class: "lock",
            div { "{output}: {label}" }
            input {
                class: "field",
                value: "{value}",
                onmounted: move |event: MountedEvent| field.set(Some(event.data())),
                oninput: move |event: FormEvent| {
                    let text = event.value();
                    record(&for_input, |typed| {
                        typed.inputs += 1;
                        typed.length = text.chars().count();
                    });
                    value.set(text);
                },
                onkeydown: move |event: KeyboardEvent| {
                    if event.key() != Key::Enter {
                        return;
                    }
                    if *value.peek() == PASSWORD {
                        record(&for_key, |typed| typed.accepted += 1);
                        say(&format!("accepted on {for_key}"));
                        lock.unlock();
                    } else {
                        record(&for_key, |typed| typed.rejected += 1);
                        say(&format!("rejected on {for_key}"));
                        value.set(String::new());
                    }
                },
            }
        }
    }
}

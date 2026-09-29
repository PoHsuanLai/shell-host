//! Every `use_*` hook: each reads the `SurfaceHandle` from the root context and subscribes.

use dioxus::core::{spawn, use_hook};
use dioxus::hooks::use_signal;
use dioxus::signals::{ReadSignal, WritableExt};

use super::SurfaceHandle;
use crate::modality::InputModality;
use crate::modifiers::Modifiers;
use crate::scale::Scale;
use crate::tick::FrameTick;
use crate::toplevel::ToplevelState;

/// The latest frame tick of the surface this component renders in, updated (and the component
/// re-rendered) once per presented frame while the document animates or a [`FrameDrive`] is
/// held (`SurfaceHandle::drive_frames`); `FrameTick::default()` before the first. Ticks are
/// never skipped by this hook's own task: each one is written, though a component rendering
/// slower than the frame rate reads only the latest.
pub fn use_frame_tick() -> ReadSignal<FrameTick> {
    let ticks = use_surface().ticks;
    let mut latest = use_signal(FrameTick::default);
    use_hook(move || {
        spawn(async move {
            let mut seen = ticks.count();
            loop {
                let (count, tick) = ticks.after(seen).await;
                seen = count;
                latest.set(tick);
            }
        });
    });
    ReadSignal::from(latest)
}

/// The scale of the surface this component renders in, in 120ths (`Scale(180)` is 1.5x), as
/// live state: the component re-renders when the compositor changes the surface's preferred
/// scale (`wp_fractional_scale_v1`, else `preferred_buffer_scale` or the output's integer
/// scale). Its first value is the scale the host knew when the component first rendered: the
/// host publishes a new scale before it polls documents, so the frame that first shows a scale
/// was rendered by a tree that had already read it. A surface starts at `Scale::ONE` until the
/// compositor says otherwise.
///
/// For a design system's device-pixel snapping (quire's `HostScale`, or `Ds { scale }`); the
/// layout half of snapping is `HostConfig::prepaint`.
pub fn use_surface_scale() -> ReadSignal<Scale> {
    let live = use_surface().scale;
    let mut latest = use_signal(|| live.get());
    use_hook(move || {
        // Read before the task first runs, in the same render as the initial value, so a change
        // landing between this render and the task's first poll is not missed.
        let mut seen = live.changes();
        spawn(async move {
            loop {
                let (count, scale) = live.changed_after(seen).await;
                seen = count;
                latest.set(scale);
            }
        });
    });
    ReadSignal::from(latest)
}

/// The state of the toplevel this component renders in, from each `xdg_toplevel.configure`:
/// maximized, fullscreen, activated and the tiled edges, as live state (the component
/// re-renders when a configure changes it; a configure that changes only the size wakes
/// nobody). For a client-decorated frame: draw the zoom button's state, square the corners
/// and drop the shadow when maximized or tiled, dim the traffic lights when inactive. The
/// host already removes the resize band in those states. The default (floating, inactive)
/// before the first configure, and forever on a layer surface or popup.
pub fn use_toplevel_state() -> ReadSignal<ToplevelState> {
    let live = use_surface().toplevel;
    let mut latest = use_signal(|| live.get());
    use_hook(move || {
        let mut seen = live.changes();
        spawn(async move {
            loop {
                let (count, state) = live.changed_after(seen).await;
                seen = count;
                latest.set(state);
            }
        });
    });
    ReadSignal::from(latest)
}

/// The modifier keys held by the seat's keyboard while this surface has keyboard focus, as live
/// state: the component re-renders on every `wl_keyboard.modifiers`, including
/// the one a compositor sends right after this surface's `wl_keyboard.enter` — so a surface
/// that gains focus with Super already down (an app switcher's `KeyboardMode::Exclusive`
/// overlay catching a quick Cmd+Tab release) or with Caps Lock already toggled (a lock screen's
/// first render) sees it from its first read, not only from the next key press. Reset to
/// `Modifiers::default()` (nothing held or locked) on `wl_keyboard.leave`, and default for a
/// surface that has never had keyboard focus.
///
/// Not delivered through `HostEvent`, whose match stays exhaustive for the shell: this hook is
/// the only way to it. Blitz's own key events keep their modifiers unchanged, from the same
/// `wl_keyboard.modifiers` through a separate, pre-existing path.
pub fn use_modifiers() -> ReadSignal<Modifiers> {
    let live = use_surface().modifiers;
    let mut latest = use_signal(|| live.get());
    use_hook(move || {
        let mut seen = live.changes();
        spawn(async move {
            loop {
                let (count, modifiers) = live.changed_after(seen).await;
                seen = count;
                latest.set(modifiers);
            }
        });
    });
    ReadSignal::from(latest)
}

/// Pointer or keyboard, whichever last drove the seat, as live state: the
/// component re-renders when it changes. A key press other than a lone modifier makes it
/// `Keyboard`, a pointer button press makes it `Pointer`; motion, scrolls, releases and
/// auto-repeats leave it (`modality::after`). One value for the seat, shared by every surface,
/// so a surface mapped from a key press (a popup opened with Enter) starts at `Keyboard`.
/// `Pointer` before any input.
///
/// What a design system's `:focus-visible`-like focus ring follows (Blitz never matches
/// `:focus-visible`): quire's `ds::HostModality`, bridged in the shell's root (README,
/// "Consuming: input modality").
pub fn use_input_modality() -> ReadSignal<InputModality> {
    let live = use_surface().modality;
    let mut latest = use_signal(|| live.get());
    use_hook(move || {
        let mut seen = live.changes();
        spawn(async move {
            loop {
                let (count, modality) = live.changed_after(seen).await;
                seen = count;
                latest.set(modality);
            }
        });
    });
    ReadSignal::from(latest)
}

/// The handle of the surface this component renders in. Panics outside a host surface (a
/// caller contract, like any missing Dioxus context).
pub fn use_surface() -> SurfaceHandle {
    dioxus::prelude::use_context::<SurfaceHandle>()
}

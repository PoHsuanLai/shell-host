# shell-host

Runs Blitz / Dioxus Native documents on Wayland: layer-shell surfaces (bars, docks,
overlays), xdg popups and xdg toplevels, all sharing one GPU device and one renderer, with
blur (`ext-background-effect-v1`), fractional scale, input regions that follow DOM elements,
IME, clipboard and a frame machine that is idle at zero wakeups. It is portable to any
Wayland compositor with layer-shell; it never depends on COSMIC-specific crates
(`scripts/check-boundary.sh`). The shell (sill) and every bundled app are built on it.

`DESIGN.md` maps the design to the code, `FINDINGS.md` records every deviation and seam,
`CONVENTIONS.md` and `ORCHESTRATION.md` say how the code is written and divided, and
`SPIKE.md` is the Milestone 0 log with the acceptance results.

## Demos

`examples/demo_bar.rs` (a 32 px top bar) and `examples/demo_dock.rs` (a magnifying dock) are
driven by commands on stdin (`bar stats`, `dock hover 300 64`, `dock leave`, `bar capture
/tmp/bar.png`, `quit`), so they need the `debug-inject` feature:

```sh
cargo run --release -p shell-host --features debug-inject --example demo_bar -- --with-dock
```

`demo_bar` takes `--with-dock` (both surfaces in one process) and `--exclusive=N`, and
`DEMO_BLUR=1` blurs behind the bar; `demo_dock` takes `--mode=transform|width|tick` and
`--layer=bottom`. `--mode=tick` magnifies from the component itself, once per compositor frame,
through the frame hook (`use_frame_tick` with a `FrameDrive` held only while the tiles move).

`examples/demo_popup.rs` is `PopupSize::FitContent`'s acceptance (FINDINGS F37): `bar click 20
16` opens a popup whose own list grows from 2 to 6 rows 300 ms after it opens with no further
command, so `bar/popup stats`' `logical` size shows the fit landing at 160x48 and, a few frames
later, re-fitting to 160x144.

`examples/demo_window.rs` is a client-decorated window: press its red titlebar and drag to move
it, drag its invisible 7 px border to resize, double-click the titlebar to zoom; on stdin `move`,
`resize bottom-right`, `maximize`, `unmaximize`, `fullscreen`, `unfullscreen`, `minimize`, `stats`,
`quit` (`move` and `resize` need a button held on the window). Each configure state change is
printed as a JSON line.

### On a nested KWin (no effect on your session)

A headless `kwin_wayland --virtual` on its own D-Bus session, torn down afterwards; works while
the real session is locked. `kwin-virtual@1.7` sets the nested output's scale:

```sh
scripts/spike-accept.sh demo kwin-virtual a      # or b, c, f, g
scripts/spike-accept.sh demo kwin-virtual@1.7 f
```

### On a nested cosmic-comp

`scripts/dev/` runs a throwaway `cosmic-comp` in a window of the current session (see
`scripts/dev/README.md`):

```sh
scripts/dev/install-cosmic.sh   # prints the dnf command for anything missing
scripts/dev/nested.sh           # start; writes scripts/dev/.nested-env
scripts/dev/globals.sh          # which protocols it offers
scripts/dev/run.sh target/release/examples/demo_bar --with-dock
scripts/dev/shot.sh scripts/dev/out/demo.png
scripts/dev/nested.sh stop
```

`examples/demo_drag.rs` is the drag source's acceptance (FINDINGS F60): a 64 x 64 thumbnail
whose press-and-move past 8 px drags a temporary file out as `text/uri-list`
(`SurfaceHandle::start_drag`), printing the outcome as JSON (`stats` on stdin); `--icon=root`
adds a `DragIcon::Root` document, `--keyboard=none|on-demand` sets its layer's keyboard mode.
`scripts/dev/accept-drag.sh` runs it on a headless KWin against a foot terminal, and
`scripts/dev/probe-modifiers.sh` uses it for FINDINGS F61.

`examples/demo_lock/` is the session lock's acceptance (FINDINGS F65): on `lock` it locks with
`HostCtx::lock_session`, one document per output with a password field; "letmein" + Enter
unlocks through `use_lock().unlock()`; `stats` prints JSON. It refuses to lock anything but the
throwaway compositor `scripts/dev/accept-lock.sh` starts (a cosmic-comp inside a headless KWin,
in a runtime directory of its own); never run it against your session.

## Consuming: the session lock

`HostCtx::lock_session(LockConfig, |output: OutputInfo| -> Root)` returns a `LockId` (or
`LockError::Unsupported` without `ext-session-lock-v1`, `InProgress` while a lock lasts) and
makes a lock surface per output, now and for outputs added while locked; `HostEvent::Lock { id,
state }` reports `Locked`, `Unlocked` or `Finished` (refused, or ended by the compositor). In a
lock document, `use_lock()` gives the `LockHandle`: call `unlock()` once the shell has
authenticated the user (PAM is the shell's), after any exit animation; the host tells every lock
document `Unlocking` (`use_lock_phase()`), then sends `unlock_and_destroy` and removes the
surfaces. `HostCtx::unlock_session(id)` does the same from outside. The documents focus their own
field (the compositor gives the surface the keyboard, not the element), and must paint the
whole surface. If the process dies while locked the session stays locked (FINDINGS F65).

## Consuming: the window seam (client-decorated frames)

What a design system's window frame (quire's `ds::HostWindow`, implemented in sill's and mailo's
glue) calls. Open the window with `ToplevelConfig { decorations: Decorations::CLIENT, .. }`
(or `Decorations::Client(ResizeMargin(n))`); `size`, `min_size` and `max_size` are the visible
frame's. Inside the window's document, `let window = use_surface();`:

| Frame behaviour | Call | Notes |
| --- | --- | --- |
| titlebar drag region | `window.begin_move()` in its `onmousedown` | `Result<(), GrabRefused>`: `NoPress`, `PressedElsewhere`, `PointerElsewhere`, `NotAToplevel`, `NoSeat`. Call it while the button is down; the compositor moves the window until release and the document sees no motion meanwhile |
| a frame-drawn resize handle | `window.begin_resize(ResizeEdge::BottomRight)` | same rule; not needed for the edges, the host's band already resizes |
| zoom (green light, titlebar double-click) | `window.set_maximized(Maximize::Set / Unset)` | toggle on `window.current_toplevel_state().maximized`; nothing changes until the configure |
| minimize (yellow light) | `window.set_minimized()` | the compositor restores it |
| fullscreen | `window.set_fullscreen(Fullscreen::Set / Unset)` | |
| close (red light) | `window.close()` | |
| Fill in the zoom menu | `set_maximized(Maximize::Set)` | |
| Left / Right / Centre in the zoom menu | none | no compositor offers it to a client (FINDINGS F50); hide the items |
| draw the state | `use_toplevel_state()` -> `ReadSignal<ToplevelState>` | `maximized`, `fullscreen`, `activated`, `tiled: TileEdges` (`.any()`): the zoom glyph, square corners and no shadow when maximized or tiled, dimmed lights when inactive |

The host owns the resize band: it is transparent surface outside the frame, the document never
receives input there and needs no markup for it, and it disappears whenever the window is
maximized, fullscreen or tiled (on cosmic-comp 1.8 that is always, FINDINGS F48). The document's
origin is the frame's corner, its viewport the frame's size, and region specs and popup anchors
are in the same coordinates. Every call is also a `SurfaceOp` (`BeginMove`,
`BeginResize(edge)`, `SetMaximized`, `Minimize`, `SetFullscreen`) for `HostCtx::apply` or
`HostHandle::apply` from outside the document.

## Acceptance

`scripts/spike-accept.sh demo [<compositor>] <a|b|c|f|g>` re-runs the Milestone 0 items with
their thresholds against the demos: a (the bar reserves 32 px), b (blur), c (dock sweep once
per frame callback in the transform, width and tick modes, one tick per callback in tick mode, zero idle
wakeups, no blocking when covered), f (fractional scale maps 1:1),
g (two surfaces, one device). `<compositor>` is `kwin-virtual` (default, nested),
`kwin-virtual@<scale>`, or `kwin` (the live session; surfaces show for a few seconds). Results
land in `spike/out/demo/<compositor>/<item>.{log,json,png}`; exit 0 means the item passed.

## Checks

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
./scripts/check-boundary.sh
cargo deny check licenses
```

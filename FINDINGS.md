# Findings

## H1 freeze (branch `h1-freeze`, 2026-09-24)

Every place the freeze deviates from the program plan's "Design: `shell-host` repo" section
(amended by SPIKE.md), and every signature problem found in what M0 delivered. Numbered so the
H2 briefs and review comments can cite them.

### F1. Two dependencies proposed for the pinned block

`rustc-hash = "2"` and `vello_common = "0.1"` are in a "Proposed for
quire/docs/workspace-deps.toml" section below the pinned block in the root `Cargo.toml`, not in
it. `anyrender_vello_hybrid::ImageManager::new` borrows an `FxHashMap<u64,
vello_common::paint::ImageId>` and `VelloHybridScenePainter::new` an `FxHashMap<ResourceId,
TextureView>`; neither `rustc_hash` nor `ImageId` is re-exported by anything in the block, so
`gpu/context.rs` cannot keep the per-device image cache across frames without naming both
(SPIKE.md S1 finding 5; the spike rebuilt the cache every frame). Both versions are already in
`Cargo.lock` through anyrender_vello_hybrid, so nothing new is compiled. The reviewer promotes
them into the block (and every repo's copy) or rejects them with the cache.

### F2. `HostEvent::Started`, `HostCtx::exit`, and `HostHandle`'s methods

The plan's `Host::run` hands the closure only events, and no event exists before the first
output, so there is nowhere to spawn the first surface of a process that does not key surfaces
on outputs (a toplevel app, `demo_dock`). `HostEvent::Started` is delivered exactly once, first,
after the initial outputs. `HostCtx::exit` is the in-loop way to return from `run` (the plan had
none). `HostHandle<M>` gets `send`, `apply`, `close`, `exit`, each `Result<()>` failing with the
new `HostError::HostGone` (`Recovery::Degrade`) after the host exits.

### F3. S2's `ElementId(u64)` cannot name a DOM element (signature problem, reported)

`RegionSpec::Element { id: ElementId }` and `PopupAnchor::Element(ElementId)` are resolved after
layout, and Blitz finds an element only by its `id` string
(`BaseDocument::get_element_by_id(&str)`; the id map `nodes_to_id` is `pub(crate)`). A `u64`
cannot be turned back into that string, and design/10 §10.3.12 already writes
`RegionSpec::Element("dock-hit")`. S2's types are frozen for this wave, so the freeze does not
change them; instead `geometry::element_id(&'static str)` hashes the name (FNV-1a, stable) and
records it, and `geometry::element_name` gives the string back to
`dom::document::element_rect`. Proposed fix at the next signature wave: `ElementId(pub &'static
str)` (keeps `Copy`) or `ElementId(pub String)`, after which `element_id` becomes the
constructor and the interning table goes.

### F4. S2's `PopupConfig` carries neither the anchor rectangle nor the size

`PopupAnchor::Rect` has no rectangle in it, and `positioner_params` takes the anchor rect and the
size as separate arguments, so the plan's `SurfaceHandle::open_popup(PopupConfig, Root)` cannot
position a popup. `placement::PopupPlacement { anchor_rect, size }` is the added argument:
`open_popup(config, placement, root)`. The same value drives `SurfaceOp::Reposition`
(`xdg_popup.reposition`, the hover switch between menus in design/13 §13.7), which the plan's
op list lacked. `SurfaceOp` also gained `SetMargin`, `SetAnchor`, `SetLayer` and
`SetVisibility`, and `SurfaceHandle` a general `apply(SurfaceOp)`.

### F5. The keysym and evdev tables are Apache-2.0, adapted from winit

`input/keysym.rs` and `input/keycode.rs` are adapted from winit 0.30.13
`src/platform_impl/linux/common/xkb/keymap.rs`. winit is **Apache-2.0 only** (its Cargo.toml and
LICENSE), not MIT OR Apache-2.0 as the brief assumed; both files carry the attribution and a list
of changes, as Apache-2.0 §4 requires. Consequence for the crate's `MIT OR Apache-2.0` claim:
these two files are usable under Apache-2.0 only, and a NOTICE-style mention belongs in any
binary distribution. The reviewer should decide whether that is acceptable or whether the tables
are re-derived from `linux/input-event-codes.h` and the xkbcommon keysym list (both facts, not
expression).

Two deliberate changes from winit and from blitz-shell's converter: the Super keys are
`Key::Meta`, `Code::MetaLeft/Right` and `Modifiers::META` (the W3C names, what
`KeyboardEvent.metaKey` and Dioxus's `modifiers().meta()` read; blitz-shell maps them to
`Super`), and `space` is `Key::Character(" ")` because keyboard-types 0.7 has no named Space.

### F6. `headless::Scale` duplicates `scale::Scale`

S3's `headless.rs` defines its own `Scale(u32)` in 120ths next to S2's `scale::Scale`. The flat
re-export in `lib.rs` is S2's; the headless one stays reachable as `headless::Scale`. The H2 gpu
owner (who owns `headless.rs`) should replace it with `crate::scale::Scale` when it next touches
the file; it is the same type by meaning.

### F7. The scroll engine and the palmrest client are not in H1

design/11 §11.3.1 says the host never forwards wheel input to Blitz and runs its own engine in
`scroll/{engine,latch,rubber,keys,indicator}.rs`, with a palmrest client in `input/palmrest.rs`
(design/12 §12.7). Neither is in the plan's H1/H2 file lists, so the freeze does not create them;
until a scroll wave does, `input::pointer::wheel_delta` produces a Blitz `Wheel` event and Blitz
scrolls. DESIGN.md lists where they will go. (Done on branch `scroll-engine`: F63, F64.)

### F8. Modules added to the plan's layout

One concept per file, so several plan modules are split and a few concepts got their own file:
`ctx.rs` (`HostCtx`) and `host_handle.rs` (`HostHandle`) out of `host.rs`; `output.rs`
(`OutputInfo`); `placement.rs` (F4); `resolve.rs` (RegionSpec to strips, the pure half of
`RegionSpec::Element`); `timers.rs` (F9); `inject.rs` (the `debug-inject` stdin source);
`gpu/adapter.rs` (ranking and validation); `gpu/presenter.rs` split into `presenter.rs`,
`swapchain.rs` and `shm.rs`. `input/key.rs` is the plan's `input/key`.

### F9. `HostState` is not generic; timers are requested as data

The sctk handler traits are implemented on a non-generic `HostState`, so the handler files carry
no `<M>`. The calloop loop's data is `HostCtx<M>` (it owns `HostState` plus the app-message
queue), and the Wayland source is inserted with a callback that dispatches into `ctx.state`.
Two consequences: handlers cannot touch the loop, so they push `TimerRequest`s (frame watchdog,
key repeat) that `HostCtx::flush` arms or cancels; and sctk's own key-repeat source
(`get_keyboard_with_repeat`) cannot be used, because its `LoopHandle<'static, T>` must name the
loop's data type while `delegate_keyboard!` names `HostState`. Repeat is a context timer driven
by `wl_keyboard.repeat_info` instead. Handler events are `ShellEvent = HostEvent<Infallible>`,
widened to `HostEvent<M>` on delivery (`event::widen`).

### F10. `Anchor` is a hand-written edge set

The plan says "Anchor bitflags", but `bitflags` is not in the pinned block. `layer::Anchor` is a
four-bit newtype with `TOP`/`BOTTOM`/`LEFT`/`RIGHT`/`ALL`/`NONE`, `|`, `union` and `contains`.

### F11. Derives: serde only where a type is data all the way down

CONVENTIONS §2 asks every public type to derive through `Serialize, Deserialize`. Nothing in
shell-host is persisted, and S2's types (`RegionSpec`, `PopupConfig`, `Scale`, `Size`, `Rect`)
have no serde, so no type that embeds them can derive it. Serde is on `HostConfig` (a shell may
read it from a file), the ids, and `FrameStats` with its parts (printed as JSON by
`debug-inject`). Types holding Wayland objects, wgpu objects or a `Root` derive `Debug` only (by
hand where a field has no `Debug`).

### F12. The UA stylesheet is exactly the finding's string

`dom::document::UA_STYLESHEET` is `html,body{height:100%;background:transparent}` (S1 finding
2). The spike also zeroed `margin` and `padding`; the freeze leaves that to quire's
`css/reset.css`, which every surface loads, so the host does not restyle what the design system
owns.

### F13. `LayerSize` is two `Extent`s

The wire's "0 means stretch" is `Extent::Fill`, a fixed size `Extent::Px(u32)`, so a zero size
cannot be written by accident on an axis that is not meant to stretch.

### F14. Wheel pixels are logical

`input::pointer::wheel_delta` passes touchpad deltas as logical pixels, negated (Wayland's axis
sign is the inverse of the DOM's). Blitz treats `BlitzWheelDelta::Pixels` as CSS pixels
(`blitz-dom/src/events/pointer.rs`: `Lines * 20`, `Pixels` as is); blitz-shell passes winit's
physical pixels, which over-scrolls at scale > 1.

### F15. `HostCtx::stats` is `&self` and does not drain

The plan's `stats(&self, SurfaceId)` is kept. It reports percentiles over the samples since the
last drain (`StatsWindow::peek`); the debug `stats` command prints and drains, so each printed
line covers the time since the previous one, as the spike's did.

### F16. Adapter validation runs on a throwaway connection

S1 requires the adapter to be "validated by a first present, with fallback on protocol error".
A protocol error kills the connection it happens on, so `gpu::adapter::validate` presents one
transparent 1x1 frame on a second connection, round-trips, and checks that connection's
`protocol_error()`. The host's own connection never sees a rejected dmabuf.

### F17. Findings from M0 carried into signatures, not re-tested

- Outside-click popup dismissal (`popup_done`) cannot be driven by a client (S2). The scripted
  demos dismiss by hit-testing and closing; `CloseReason::PopupDismissed` is only reachable with
  a real click. Noted on `PopupHandler::done`.
- A CSS `transition` cannot be shown mid-flight from one headless render (S3); headless snapshot
  tests use `@keyframes`.
- KWin keeps sending frame callbacks to an occluded surface; cosmic-comp may not (#2777). The
  watchdog runs only while a frame is in flight and more work is wanted, so on KWin it never
  fires and idle stays at zero wakeups (`tests/frame.rs` skeletons).
- The first-mouse window is a parameter of `input::pointer::first_mouse`, read by the caller
  from the settings key `menus.first_mouse_window_ms` (default 100), because design/22 marks it
  proposed.

### F18. The demo examples need `debug-inject`

`examples/demo_bar.rs` and `demo_dock.rs` are driven by stdin commands like the spike's, so both
have `required-features = ["debug-inject"]`, and `debug-inject` pulls in `serde_json` (already
an optional dependency for the spike).

## H2 gpu (2026-09-24, live KDE session)

- wgpu 29 binds an `Instance` to one `wl_display` and refuses surfaces from another display,
  so the F16 adapter probe opens its own instance on the throwaway connection and presents
  through the adapter whose `AdapterInfo` equals the candidate's; `validate(instance, ..)`'s
  instance parameter is unused (signature kept).
- NVIDIA 615.71: dropping a Vulkan instance that presented makes the next
  `vkGetPhysicalDeviceSurfaceSupportKHR` on the host's instance jump to a null pointer inside
  libnvidia-glcore (gdb-confirmed; not after RADV or llvmpipe probes). `probe.rs` keeps each
  probe instance and connection alive for the process, one per validated candidate.
- Facts: NVIDIA Vulkan passes validation (~300 ms), PreMultiplied + Mailbox; RADV (integrated)
  is rejected by KWin on the first present (protocol error 7 or connection reset); llvmpipe
  passes but ranks last; NVIDIA GL does not support the surface.
- `Presenter::present` and `ShmBuffers::new` carry no `wl_surface`; `ShmBuffers::for_surface`
  is the constructor the shm path needs. A `height:100%` chain did not size a plain div
  offscreen; the headless tests use fixed heights (dom agent to check the UA stylesheet).

## H2 integration (branch `h2-integration`, 2026-09-24)

Seams between the five H2 fills, found and fixed after the merge. Numbered after F18.

### F19. `dom::document` still named the retired `geometry::element_name`

protocols amended `ElementId` to own its name (F3's proposed fix, `ElementId::new(&str)` /
`as_str`) and removed the interning table; dom still called `geometry::element_name`, so the
merged tree did not compile. `SurfaceDocument::element_rect` now looks up `id.as_str()`
directly. It was the only compile error on the merged tree; clippy was otherwise clean.

### F20. Regions were sent twice; one change-detected path now

protocols' `handlers/layer.rs::apply_regions` resolved and sent the input and blur regions on
every layer configure, and core's `ctx/paint.rs::send_regions` sent them again on the frame
that followed, tracked by core's own `AppliedRegions` (two `Option<Vec<Rect>>`) next to
protocols' unused `resolve::Tracked`. The configure-time send is gone (the configure only
invalidates, and the render resolves after layout, so an element-following region is never
sent from a stale layout), and `AppliedRegions` is now two `resolve::Tracked`s. `Tracked`
gained `holding(strips)`, "the compositor already has this": the blur tracker starts as
`holding(vec![])`, which keeps core's rule of never creating an effect object just to say
"no blur". Resizes and op records no longer reset the trackers; the strips comparison already
catches every real change, and only a rebuilt role (hide/show, a new `wl_surface`) resets.

### F21. `LocalNet` serves `file:` as well as `data:`

`DataUriNet` answered only `data:`, but the shell's wallpaper is `background-image:
url(file://…)`. It is now `dom::net::LocalNet`: `data:` decoded as before, `file:` read with
`std::fs::read` on the host runtime's blocking pool (`spawn_blocking`, the host holds
`rt.enter()`). Only absolute paths from a `file:` URL with an empty or `localhost` host are
served (`LocalSource::of`, table-tested); anything else stays unanswered. A failed read is
logged and unanswered. Blitz's resource handler calls `request_redraw` on completion, which
pings the loop, so the image appears on the next frame without a host-side waker.

### F22. Paste reads the selection ahead of time; the text can be a few ms stale

`ShellProvider::get_clipboard_text` is synchronous and runs inside Blitz's event handling on
the main thread, which is also the thread that would have to serve the pipe when the selection
is ours, so a paste cannot block on a read. Now every `wl_data_device.selection` event runs
`Clipboard::follow`: it sends `wl_data_offer.receive` for the best text MIME, flushes, and reads
the pipe on a helper thread (200 ms timeout, plan "Clipboard"), publishing the text into a
`clipboard::SharedSelection` (`Arc<Mutex<…>>`) that every surface's shell provider holds. Our
own copies publish at once. **Timing limitation**: between a new selection and its read
finishing (one pipe round trip to the owner, bounded at 200 ms; not measured here), a paste returns the previous
text; a read of an older selection that finishes late is dropped by generation, so it can
never overwrite a newer one. An offer with no text type, a failed read or a timeout publishes
"no text". A timed-out reader thread is abandoned and ends when the owner closes its end.

### F23. Blitz's own copy never reached the compositor

The provider's `set_clipboard_text` only filled a per-surface cache, so a copy in a text input
was visible to nothing else. It now queues the text and pings; the flush's `ApplyRequests`
phase claims the selection with it (`ctx::ops::claim_copied`, the same `claim_selection` that
`SurfaceHandle::copy_text` uses).

### F24. `use_surface()` in a spawned root is tested

`ctx/spawn.rs` did provide the `SurfaceHandle` through `Root::context` before
`SurfaceDocument::build`; the build is now one named function (`surface_document`), and a
headless test builds a root whose component reads `use_surface()`, renders its id and queues a
`SetBlur`, asserting both the rendered text and the queued `Request::Apply` for that id.

### F25. The acceptance script's sweep sent fractional coordinates

The spike's command grammar took floats; the real `debug-inject` grammar takes whole logical
pixels (`debug.rs`, and a test rejects `hover 1.5 2`), so every `hover` of the demo sweep was
refused and item c and g saw zero frames. The script rounds the sweep (a 680 px sweep in 120
steps loses nothing). The grammar stays integer: logical px are integers everywhere else.

### F26. `ctx::paint::capture` was dead code without `debug-inject`

`cargo clippy --workspace --all-targets -- -D warnings` (the ORCHESTRATION gate, default
features) failed on it; it is `#[cfg(feature = "debug-inject")]` now, like its re-export.

### F28. `tests/pointer.rs` now drives the host's `SurfaceDocument`

input's tests were written while `dom/**` was `todo!()`, so they built a bare `DioxusDocument`
with a copy of the UA sheet and re-implemented the `data-first-mouse` walk. They now build
`SurfaceDocument`s (the host's UA sheet plus a margin reset standing in for quire's) and ask
`SurfaceDocument::wants_first_mouse` at the element's centre; all five pass, which also shows
the host's UA sheet alone is enough for hit-testing into in-flow content (S1 finding 2).

### F27. Facts from the demo acceptance (SPIKE.md "H2 demos")

- The demos have no present-mode knob (the host always prefers Mailbox), so item c's
  informational "Fifo" run is a second Mailbox run; `SPIKE_PRESENT` was the spike's.
- The live KDE session was unlocked but its display was in DPMS off. KWin then sends **no frame
  callbacks at all** and screenshots are black, so every visual live check failed while every
  protocol-level one passed. This was the cosmic-comp #2777 case on KWin: the watchdog carried
  each surface (4 fires per sweep, intervals 100 → 400 ms backoff), `stats` stayed prompt, and
  the dock settled after 1 to 2 watchdog frames into the 5 s idle. The display was not woken on
  the user's behalf; `demo kwin <item>` should be re-run with the screen on.
- `probe_toplevel` is kept as a self-contained sctk example (no `spike` feature): the host's
  `ToplevelConfig` has no maximize, and the items need an ordinary other-client window.

## sill gaps (branch `sill-gaps`, 2026-09-24)

The host gaps sill's interface freeze reported (sill FINDINGS G1-G5). G1, G3 and G5 are built;
G2 and G4 are designed here and not built. Numbered after F28.

### F29. G1: debug injection returns its results; `HostCtx::stats` is the one stats source

`inject::apply` printed stats and capture results to stdout and found surfaces by name inside
the module, so a shell could not use it except through stdin. Now:

- `HostCtx::inject(&mut self, SurfaceId, Inject) -> Result<Injected>` (feature
  `debug-inject`), `Inject::{Pointer { at: Point }, Click { at: Point }, Leave, Capture { path:
  PathBuf }}`, `Injected::{Done, Captured { path: PathBuf, size: Size }}` (`size` in physical
  px, the buffer size). Nothing prints. An unknown id is `HostError::NoSurface`; a capture before
  the first present is `HostError::Render`. `Click` is one variant more than sill asked for: the
  stdin protocol has `click` and the demos' acceptance uses it, and a click is not a sequence of
  `Pointer`s.
- Frame statistics are not an injection: `HostCtx::stats(&self, SurfaceId)` (unchanged,
  non-draining, F15) is the source, and `HostCtx::take_stats(&mut self, SurfaceId)` is it plus a
  new sample window. `HostCtx::surface_named(&SurfaceName) -> Option<SurfaceId>` is the lookup
  the module kept to itself.
- The stdin protocol moved to `inject::stdin` (`install`, `apply`) and maps each line onto the
  above; its output is byte-for-byte what the acceptance script parsed before. The command
  language's per-surface action enum was renamed `debug::Inject` -> `debug::DebugAction` so the
  typed API could take the name.
- Tests: `tests/frame.rs::the_stats_window_counts_what_the_sweep_did` (what the flush records is
  what `stats` reports; taking starts a new window, counters stay cumulative) and
  `tests/headless.rs::inject_capture_writes_a_png_and_returns_its_size`. The headless test
  covers `inject::write_capture`, the writer `Inject::Capture` calls after its offscreen render;
  the render itself needs a presenter, which needs a compositor, so it is covered by acceptance
  item f (`bar capture`, which now goes through `HostCtx::inject`; passes on kwin-virtual).

### F30. G3: `PopupConfig::offset`

`PopupConfig` gained `offset: Offset { dx: Px, dy: Px }` (logical px, `Offset::ZERO` =
`Default`). `positioner_params` translates the anchor rectangle by it (`popup::offset_rect`), so
the offset follows `PopupAnchor::Element`, `SurfaceOp::Reposition` and flips without new wire
code; `xdg_positioner.set_offset` would do the same unflipped, and was not used so that
`PositionerParams` and its wire mapping stay as they were. A rectangle moved past the parent's
origin is sent as is (xdg-shell has no bounds error for the anchor rect). Table rows in
`tests/positioner.rs`: the title menu at (-8, +6), a status menu at (+8, +6), a negative
origin. **Breaking for literals**: every `PopupConfig { .. }` needs `offset` (sill's
`bar::popup::menu_popup`; its `menu_offset` workaround can go).

### F31. G5: the frame hook (`tick.rs`)

A component could not observe `wl_surface.frame` callbacks, so sill's dock drove its `Frame`
from a timer. Now, per surface:

- `SurfaceHandle::drive_frames(&self) -> FrameDrive`: while any drive is alive the frame machine
  is told `Animation::Running` (`tick::pacing`), so the surface renders once per callback even
  with nothing changed; taking one queues `Request::Redraw` (an idle surface has no callback
  coming). Dropping it stops the requests: the frame in flight is called back once more,
  quietly, and the surface idles. **Holding a drive with no animation is the one way to get
  non-zero idle frames**; a component takes it when its motion starts and drops it when the
  motion settles or the hover that started it ends.
- `use_frame_tick() -> ReadSignal<FrameTick>` and `SurfaceHandle::next_frame(&self) -> impl
  Future<Output = FrameTick>`; `FrameTick { at: HostInstant, dt: Duration }`, `HostInstant` being
  the host clock (the timeline CSS resolves on). A tick fires once per presented frame, at its
  callback, while the document is animating or a drive is held; a watchdog expiry standing in for
  a missing callback (cosmic-comp #2777) ticks too. `dt` is zero on the first tick of a run, so
  nothing integrates over an idle gap. `FrameCounters::ticks` counts them (serde default 0).
- Tests: `tick.rs` (pacing table, `dt`, drive counting, the waiting future),
  `tests/frame.rs::a_held_drive_ticks_once_per_callback_and_stops_when_dropped` (120 ticks over
  120 callbacks, then none, no frame and no timer after the drop), and
  `ctx::spawn::tests::use_frame_tick_renders_each_published_tick` (the hook re-renders a real
  document per tick through its own task, and not without a drive). `demo_dock --mode=tick`
  magnifies from the component through the hook, and item c runs it next to the CSS modes.
  kwin-virtual at 60 Hz, 1 s sweep then leave: tick mode 77 frames / 77 callbacks / 76 ticks,
  work p95 1.1 ms; 5 s idle after the leave 0 frames, 0 commits, 0 main-thread wakeups
  (transform 72/72, width 72/72, both 0 idle). Items a and f pass unchanged.

### F32. Pointer leave never reached `mouseleave` handlers

`handlers/pointer.rs` (and the debug `leave`) called `clear_hover()` and then sent the leave move
to (-1, -1). `clear_hover` unhovers without dispatching anything, so the move found no hovered
node, fell back to the root element, and no `mouseleave`/`mouseout` ever ran: a component could
not see the pointer leave its surface, which the dock needs to drop its drive. Both paths now call
`SurfaceDocument::pointer_left`, which sends the move first (Blitz's own hover change dispatches
the leave events to the old chain) and keeps `clear_hover()` as a backstop.
`tests/pointer.rs::leaving_the_surface_runs_mouseleave_handlers` fails with the old order ("left
0") and passes with the new.

### F33. A document stopped hearing its tasks after any render

`DioxusDocument::poll` returns as soon as the `VirtualDom` has dirty work, rendering it, and the
`VirtualDom` keeps the poll's waker only when a poll finds nothing to do. `SurfaceDocument::poll`
polled once per wake, so after any poll that rendered, the next wake of a Dioxus task (a
`use_watch` value, a frame tick) reached nobody and the document sat unpolled until something
else (input, a configure) woke it. The frame hook made it visible: on kwin-virtual,
`demo_dock --mode=tick` magnified the first tiles of the sweep, then froze with its drive held
(300 frames in the 5 s idle), because the effect that drops the drive never ran again.
`SurfaceDocument::poll` now polls until a poll finds nothing (at most 8 renders, then it wakes
itself so the next flush continues). `ctx::spawn::tests::use_frame_tick_renders_each_published_tick`
polls only when the waker fired and fails at the second tick with the old single poll.

### F34. G2 (designed, not built): minimize rectangles need the host's connection

design/10 §10.3.10 sets each dock tile as its app's minimize target
(`zcosmic_toplevel_manager_v1.set_rectangle(handle, wl_surface, x, y, w, h)`). The toplevel
handle lives on sill's `cosmic_wl` connection and the dock's `wl_surface` on shell-host's; a
protocol object cannot cross connections, and shell-host may not name COSMIC protocols
(`scripts/check-boundary.sh`), so a host-side `set_minimize_rect` op would either break
portability or need a generic "foreign toplevel" abstraction the host has no other use for.

The API to add is an export of the host's own connection and the surface on it:

```rust
/// The dock's `wl_surface` and the connection it lives on, for a client protocol the host does
/// not speak. Valid until the surface is hidden or closed (a hidden surface's role is rebuilt
/// with a new `wl_surface`): re-export on `HostEvent::SurfaceMapped { id }` (new event).
pub struct WlSurfaceExport {
    pub connection: wayland_client::Connection,
    pub surface: wayland_client::protocol::wl_surface::WlSurface,
    pub generation: u32,
}
impl HostCtx<M> { pub fn wl_surface(&self, id: SurfaceId) -> Option<WlSurfaceExport>; }
impl SurfaceHandle { pub fn wl_surface(&self) -> Option<WlSurfaceExport>; }
```

sill's `cosmic_wl` thread then creates its own `EventQueue` on the exported `Connection`
(wayland-client allows any number of queues per connection, each dispatched on its own thread),
binds the toplevel manager there, and calls `set_rectangle` with the exported surface; both
objects live on one connection. The rectangle is surface-local logical px, which the dock gets
from `SurfaceDocument::element_rect` of the tile. Costs: sill stops opening a second connection
(or keeps it for everything else and binds only the manager on the shared one), and must redo
the rectangles after a hide/show. Nothing in shell-host changes beyond the two accessors and the
mapped event.

### F35. G4 (designed, not built): drag and drop into shell surfaces

`handlers/data_device.rs` follows the selection only; `wl_data_device.enter/motion/leave/drop`
are ignored, so file drops on dock tiles and spring-loading (design/10 §10.3.8) have no event
source. The API to add:

```rust
/// Which drops an element takes: the host answers `wl_data_offer.accept`/`set_actions` on every
/// motion from the element under the pointer, synchronously (a round trip through a component
/// would miss the motion's serial), like `data-first-mouse`.
/// Markup: `data-drop-accept="text/uri-list text/plain"`, `data-drop-action="copy|move|ask"`.
pub enum DropEvent {
    Enter { at: Point, mimes: Vec<String> },
    Motion { at: Point, over: Option<ElementId> },
    Leave,
    Drop { at: Point, over: Option<ElementId>, payload: DropPayload },
}
pub enum DropPayload { Uris(Vec<PathBuf>), Text(String), Refused }
/// The latest drag-and-drop event over this surface; `None` outside a drag.
pub fn use_drop_event() -> ReadSignal<Option<DropEvent>>;
```

The payload is read like the clipboard (F22): `receive` on the accepted MIME, the pipe read on a
helper thread with a timeout, `wl_data_offer.finish` after, and `Drop` delivered when the read
completes (`Refused` on timeout or an unaccepted element). Spring-loading stays in sill's dock
machine (`DndMotion` carries `over`; the dwell timer is the machine's). The drag source side
(dragging a tile out of the dock) is a separate `SurfaceHandle::start_drag(mimes, icon)` and not
part of G4.
## Cosmic blur (branch `cosmic-blur`, 2026-09-24)

### F29. Nested cosmic-comp on NVIDIA EGL loses every layer surface that has a blur region

**Symptom.** `scripts/spike-accept.sh demo cosmic b` with blur on: the grim capture shows the
maximized checkerboard and no bar (an earlier run with no window behind showed only the flat
grey background). Blur off, the magenta bar is there. KWin and kwin-virtual pass.

**Cause: cosmic-comp, not us.** The condition is exactly: cosmic-comp 1.8.0, `COSMIC_BACKEND=winit`
(nested), rendering through NVIDIA's EGL (the default here; host screen off, `__GL_SYNC_TO_VBLANK=0`).
Any `ext_background_effect_surface_v1.set_blur_region` on a layer surface makes the whole surface
vanish from the output, whatever the client. The same run with the compositor on Mesa (AMD iGPU,
`__EGL_VENDOR_LIBRARY_FILENAMES=…50_mesa.json DRI_PRIME=1 vblank_mode=0`) blurs correctly
(variance behind the bar 3306 → 1.0). Experiments (screenshots, comp.logs and client output in
`spike/out/cosmic-blur/` (gitignored), numbers here; "bar var" is the luminance variance of the bar strip over the
checkerboard, 16256 = bare checkerboard, i.e. no bar; 3306 = tinted bar, unblurred; ~1 = blurred):

| # | Client / condition | Compositor EGL | Bar var | Result |
| --- | --- | --- | --- | --- |
| base | demo_bar, `RegionSpec::Whole`, set with the first buffer | NVIDIA | 16256 | bar gone |
| base | same, blur off | NVIDIA | 3306 | bar visible |
| 1 | blur_probe (plain sctk + `wl_shm`) region whole / one 300x32 rect / rounded strips | NVIDIA | 16256 each | whole surface gone, also outside the rect |
| 2 | blur_probe region set with the first buffer / on a later commit after the first frame callback | NVIDIA | 16256 each | gone either way |
| 3 | demo_bar, theme `frosted` High / Low / key absent | NVIDIA | 16256 each | strength irrelevant |
| 4 | demo_bar on RADV (`WGPU_ADAPTER_NAME=AMD`), on llvmpipe; blur_probe ARGB8888 shm, XRGB8888 shm | NVIDIA | 16256 each | our buffer path irrelevant |
| 5 | blur_probe layer surface, region none | NVIDIA | 3306 | visible |
| 6 | blur_probe xdg toplevel, blur whole | NVIDIA / Mesa | window drawn, interior var 3515 = unblurred | toplevels are drawn but never blurred on either GPU |
| 7 | demo_bar / blur_probe | Mesa | 1.0 | blurred, bar and checkerboard below correct |

Is it unmapped (7)? No: the blurred surface keeps getting frame callbacks (blur_probe prints
`frame callback 1` after its blurred commit), which cosmic-comp only sends for surfaces in the
rendered element set, and elements drawn before it (the window below) survive. It is drawn and
its pixels are lost. cosmic-comp's blur is a framebuffer-effect render element
(`src/backend/render/wayland/blur_effect.rs`: it blits the active framebuffer into a texture and
runs the down/up passes on textures bound mid-frame); on the winit backend the frame target is
the EGL window surface, and on NVIDIA's EGL the draws that follow that pass (the surface itself)
do not reach it. The DRM backend renders into GBM buffers and was not testable here (locked
session); re-test on a real cosmic session, and with the host screen on, before treating this
as winit-only. Not debugged further inside cosmic-comp.

**Red herring.** comp.log's `eglSwapBuffers: EGL_BAD_SURFACE … is not current draw surface` /
`eglCreatePlatformWindowSurfaceEXT: EGL_BAD_ALLOC` / `Failed to render frame … context lost`
appear exactly once per `grim` capture, with blur on or off and on either EGL: an output capture
on the winit backend blits into the capture buffer before the swap. Zero with no capture.

**What changed on our side.**
- `blur_health.rs` (pure, table-tested) decides `BlurHealth::{Renders, Breaks}` from
  `CompositorFacts`: cosmic-comp (a `zcosmic_*`/`cosmic_*` global), the winit backend (an
  output named `WINIT-n`) and an NVIDIA main render device. Only all three is `Breaks`.
- `handlers/render_device.rs` binds `zwp_linux_dmabuf_v1` v4+ once for its default feedback's
  `main_device`, reads the PCI vendor from `/sys/dev/char/M:m/device/vendor`, and destroys the
  feedback. The facts are in by the connect roundtrip, before any surface.
- `HostState::blur_supported` is `Unsupported` when the verdict is `Breaks`, so the host sends no
  blur region and the surface keeps its tint (the solid-tint fallback the shell's `data-blur=off`
  would give). The debug host line says `blur: degraded`. The public API is unchanged: no shell
  can yet ask whether blur is on, so sill's `data-blur` still needs a host query (proposed for
  the next signature wave: `HostCtx::blur() -> Support`, fed by the same verdict).
- `scripts/dev/nested.sh` now runs the compositor on Mesa by default (`SILL_NESTED_GPU=nvidia`
  for the old path) and sets `vblank_mode=0` next to `__GL_SYNC_TO_VBLANK=0`; without it a
  Mesa-rendering nested compositor also stops answering screenshots while the host display is off.
- `examples/blur_probe.rs`, a plain sctk/`wl_shm` blur client, is the upstream repro.

Upstream: worth a cosmic-comp issue ("layer surface with ext-background-effect blur region not
drawn on winit backend with NVIDIA EGL") with blur_probe as the repro; not filed from here.

## Tick idle on cosmic-comp (branch `tick-idle`, 2026-09-24)

### F36. The tick dock's motion stalled at cosmic-comp's frame rate and held its drive

**Symptom.** `scripts/spike-accept.sh demo cosmic c`: `transform` and `width` idle at 0, `tick`
had 548 frames / 548 commits / 1479 main-thread wakeups in the 5 s idle after the leave (993 /
993 / 2512 on a rerun). `demo kwin-virtual c` passed, 0 idle.

**Not the pointer.** The acceptance hovers and leaves through `debug-inject` (`dock hover`, `dock
leave`), not through `wl_pointer`, so neither compositor's enter/leave semantics (cosmic-comp
#2230) is involved, and the host's leave path (F32, `SurfaceDocument::pointer_left`) is the same
on both. A trace in the tick dock (on `mouseleave`, on every tick: `dt`, pointer, largest gap to
target, drive held; on the drop) showed `mouseleave` arriving at once on both compositors, the
target going to rest, and the drive held on:

| | kwin-virtual | cosmic-comp (nested, winit, mailbox) |
| --- | --- | --- |
| tick `dt` after the leave | 16.5-17.5 ms | 1.1-2.0 ms, an occasional 4-6 ms |
| largest gap per tick (thousandths) | 391, 284, 201, ..., 8, 6, 0 | 441, 432, 424, ..., then 7 for 693 ticks |
| leave to drive dropped | 15 ticks, ~240 ms | 1393 ticks, 1852 ms (first leave); the last leave never dropped before quit |

**Cause: our component, at a frame rate its test never saw.** `demo_dock`'s `step` moved each
tile `round(gap * dt / 60 ms)` thousandths and snapped within 4. At 16.7 ms that rounds to zero
only under a gap of 1.8, inside the snap; at 1-2 ms (nested cosmic-comp calls back as fast as it
composites) it rounds to zero under a gap of 15-30, so the tiles stopped a few thousandths short
of rest, `next == target` never held, and the drive, and with it a render and commit per
callback, stayed until a tick long enough (about 6 ms) to move one unit happened along. The
host did what `FrameDrive` promises: a held drive renders every frame.

**Fix.** `step` now closes `1 - exp(-dt / 60 ms)` of the gap (frame-rate independent) and
always at least one thousandth, never past the target, so it settles in bounded time at any
rate. `FrameDrive`'s doc has the "hold until settled" pattern with this rule: the drop
condition must be reachable at any tick rate; test it at 1 ms as well as 16.7 ms. The host is
unchanged. `demo_dock` gained `test = true` so its unit tests run under `cargo test`:
`tests::the_tiles_settle_at_any_tick_rate` (0.5 ms to 33 ms ticks, to and from a hover, within
500 ms) fails with the old `step` ("dt 500 us: settled after None") and passes with the new;
`tests::a_step_never_overshoots`.

**After.** item c, 5 s idle after the leave, frames / commits / main-thread wakeups:

| mode | cosmic (sweep frames = callbacks) | kwin-virtual (sweep frames = callbacks) |
| --- | --- | --- |
| transform | 0 / 0 / 0 (871) | 0 / 0 / 0 (73) |
| width | 0 / 0 / 0 (792) | 0 / 0 / 0 (71) |
| tick | 0 / 0 / 0 (830, 829 ticks) | 0 / 0 / 0 (79, 78 ticks) |
| transform, fifo | 0 / 0 / 0 | 0 / 0 / 0 |

sill's dock machine does not use the hook yet; when it does, its settle test needs the same
1 ms case.

## sill gaps G7, G10 (branch `click-fit`, 2026-09-24)

### F37. G10: `Inject::Click` and `debug click` take a `PointerButton`

`Inject::Click { at }` pressed and released the primary button only. `debug::PointerButton`
(`Primary | Secondary | Middle`, Linux evdev `BTN_LEFT`/`BTN_RIGHT`/`BTN_MIDDLE` = 272/273/274,
same vocabulary `input::pointer::button_from_evdev` already maps a real press with) is a new
field: `Inject::Click { at: Point, button: PointerButton }`. Not a `Default`-backed struct
update — the two call sites (`HostCtx::inject`'s match arm and `inject::stdin::on_surface`)
were updated directly, as the brief allowed. The stdin grammar's `click X Y` still defaults to
`primary`; `click X Y secondary` and `click X Y middle` select the others
(`debug::parse_command`'s `click` helper, `CommandError::NotAButton` for an unrecognised word).
`inject::clicked(at, button)` maps `button` to a DOM `MouseEventButton` (`dom_button`, tested
table-wise) and builds the same move-down-up sequence `input::pointer::press_events` /
`release_event` already produce for a real press, so a secondary click behaves exactly like one
delivered over `wl_pointer`.

**What Dioxus receives.** Blitz's `handle_pointerup` (`blitz-dom` `events/pointer.rs`) dispatches
`DomEventData::ContextMenu` on release of `MouseEventButton::Secondary` (and `Click` only for
`Main`), which `dioxus-native-dom` delivers to an `oncontextmenu` handler exactly the way `Click`
reaches `onclick` — no shell-host-side event translation is needed beyond picking the right
`MouseEventButton`. `tests/pointer.rs::a_secondary_click_reaches_oncontextmenu_and_a_primary_click_reaches_onclick`
proves it headlessly, against a real `SurfaceDocument`: a secondary press+release increments
`#menus` (`oncontextmenu`) and leaves `#clicks` (`onclick`) at 0; a primary press+release does
the reverse. `inject.rs`'s own unit tests cover the button mapping and the built event sequence
(`clicked_moves_then_presses_and_releases_the_named_button`, table-driven over all three
buttons).

### F38. G7: `PopupSize::FitContent` measures the popup's own document

`positioner_params` already clamped a passed-in size to `FitContent`'s `max`; nothing supplied
that size from the popup's actual content, so callers (sill's `bar::popup::menu_popup`) had to
guess it by hand (sill FINDINGS F45). Two additions close the loop:

- **`SurfaceDocument::content_size(&self) -> Option<Size>`** (`dom/document.rs`): the laid-out
  box of the popup's content, in logical px. Neither `<body>` nor `dioxus-native-dom`'s own
  fixed mount point (`<main id="main">`, `DioxusDocument::new`, always `<body>`'s one child) is
  usable directly — a block's default `width:auto` fills its container, so both stretch to the
  surface's width regardless of content (and `<body>`'s `height:100%`, this crate's own
  `UA_STYLESHEET`, does the same on the other axis: measuring either would just read back the
  viewport, the one thing `FitContent` must *not* do). The measured element is `#main`'s first
  child that is not one of `style`/`script`/`title`/`link`/`meta`/`base`/`template` (Blitz's own
  default stylesheet already hides those, `display:none`, and a document's own `<style>` element
  — every demo in this crate writes one — renders as `#main`'s first child, ahead of its real
  content). `None` before layout or when there is no such child.
- **`popup::refit(config, current, content) -> Option<PopupPlacement>`**: pure; `None` for
  `PopupSize::Fixed`, or when `content` clamped to `max` already equals `current.size` (the
  check that stops a settled popup from repositioning every frame); otherwise the placement to
  move to. `ctx::paint::render` calls it right after the layout `resolve()` that produced the
  measurement (so a `FitContent` popup is measured before anything else that frame reads its
  size) and, if it returns `Some`, applies it inline through the new `ctx::ops::refit_popup` —
  the same record-then-`xdg_popup.reposition` pair `SurfaceOp::Reposition` already sends, just
  run synchronously instead of queued, so the fit lands in the same flush that measured it. The
  compositor's answering `xdg_popup.configure` then resizes the surface exactly as a caller-sent
  `SurfaceOp::Reposition` already did (F30) — no new code path there, and never a buffer larger
  than what was configured, since `surface.logical`/the presenter only ever change from that
  configure, never from `refit_popup` itself.
- **The placeholder before the first measurement is the caller's own initial `PopupPlacement`.**
  Nothing new was needed: `PopupPlacement.size` was already sent as the first `xdg_positioner`
  size (spawn.rs's `popup_role`) before any layout could happen, so it already served as "the
  documented placeholder" the brief asks for; `refit` on the next render simply replaces it once
  the real content is known.

Tests: `popup::tests::refit_only_changes_a_fit_content_popup_whose_measured_size_differs` and
`tests/positioner.rs::refit_reflows_a_fit_content_popup_to_its_measured_content_clamped_to_max`
(table-driven: `Fixed` never refits, already-fitted is unchanged, growth refits, and refits
clamp on each axis independently); `tests/dom.rs::content_size_measures_the_first_content_element_under_body`
(skips a `<style>` sibling, measures the `160x40` div after it) and
`::content_size_is_none_with_no_content_element`.

**kwin-virtual acceptance**, `examples/demo_popup.rs` (a one-button bar; the button opens a
`FitContent { max: 400 }` popup whose own list starts at 2 rows — 160x48 — and grows to 6 —
160x144 — 300ms later with no further command), driven on a nested `kwin_wayland --virtual`:
`bar/popup stats` 150ms after `bar click 20 16` reports `"logical":{"width":160,"height":48}`;
the same command 500ms later (after the growth) reports `"logical":{"width":160,"height":144}`
— the fit lands, and re-fits, exactly as designed. `scripts/spike-accept.sh demo kwin-virtual a`
and `c` still pass unchanged (`"pass": true` in both `spike/out/demo/kwin-virtual/{a,c}.json`).

- **F39 A nested cosmic-comp whose EGL surface is lost runs away.** The instance the M1–M4 waves
  shared (started on the NVIDIA GL path before F29 moved the default to Mesa, with
  `__GL_SYNC_TO_VBLANK=0` and the host screen off) reached 64 GB resident plus 2.4 GB of swap
  after 4.5 hours, growing about 9.5 MB/s at a full core with no client connected. comp.log
  shows the mechanism: after a screenshot the winit backend's EGL window surface is lost
  ("eglSwapBuffers: EGL_BAD_SURFACE", then "eglCreatePlatformWindowSurfaceEXT: EGL_BAD_ALLOC",
  "Failed to render frame ... The context has been lost, it needs to be recreated"), the
  compositor retries the surface on every frame, each failed attempt leaks, and with vblank
  sync off nothing throttles the loop. This is cosmic-comp/smithay behaviour, not ours.
  `scripts/dev/nested.sh` now starts a watchdog that kills the compositor past
  `NESTED_RSS_MAX_MB` (default 4096) and logs it; `stop` also stops the watchdog. The Mesa
  default (F29) avoids the trigger. A cosmic-comp report belongs with the F29 repro.

## Background frames (branch `background-frames`, 2026-09-25)

Origin: sill FINDINGS F171 / gap G21 (the M2 wallpaper never showed its image on the nested
cosmic-comp; frame counter stuck, suspected cosmic-comp #2777 and the watchdog not covering it).

### F40. A load that completed while its frame rendered was dropped with the render's own redraw request

**Reproduction.** `examples/demo_wallpaper.rs`: a sill-shaped wallpaper (`Layer::Background`,
every edge, `ExclusiveZone::Ignore`, `RegionSpec::Empty` input, keyboard `None`, no blur) whose
root paints rgb(200,40,40), and 300 ms after build swaps in rgb(20,120,220) content:
`--source=file` (an `img` of a generated PNG in a temp file), `data` (the same PNG as a
`data:` URL), `css` (`background-image` of the file), `none` (a plain element, no resource);
`--png-side=N` sizes the PNG. Driven on the shared nested cosmic-comp (Mesa, `wayland-1`) by a
script that reads `wallpaper stats` at 0.1, 0.25, 0.5, 1.0, 1.5, 2.0, 2.5 s, shoots the output
with `grim` (centre pixel), idles 5 s with no command, reads stats again, then captures.

Before (master 2c7159a), frames over time / on-screen centre / frames across the 5 s idle:

| source | frames at 0.1 ... 2.5 s | screen centre | idle 5 s |
| --- | --- | --- | --- |
| `css` (64 px PNG), two runs | 0 1 1 2 2 2 2 | 200,40,40 (fallback) | 2 → 2 |
| `file`, `--png-side=512` | 0 1 1 2 2 2 2 | 200,40,40 (fallback) | 2 → 2 |
| `file` / `data` (64 px) | 0 1 1 2 2 2 2 | 20,120,220 | 2 → 2 |
| `file`, 1024 / 2048 px | 0 1 1 3 3 3 3 | 20,120,220 | 3 → 3 |
| `none` | 0 1 1 2 2 2 2 | 20,120,220 | 2 → 2 |

So the surface is frozen with the fallback on screen whenever the image load finishes inside
the window the table's second and third rows hit, and it is not the layer, the input region or
the URL scheme: the same surface paints a plain DOM change, a small `img` and a large one. The
`debug capture` of a frozen surface showed the *image* (20,120,220): `Presenter::capture`
calls `resolve`, which drains the document's resource channel, so a capture is not evidence of
what is on screen (it renders offscreen and never presents).

**Cause, with the trace.** Temporary `eprintln!` tracing (frame inputs and actions, the frame
callback, polls, every `request_redraw` with its thread, the redraw flag cleared after
present; removed again) on a `css` run, times since start:

```
748.7ms request_redraw on "main"             (the swap's mutation)
748.7ms poll Changed
748.7ms Invalidate: Idle -> Dirty
749.2ms request_redraw on "tokio-rt-worker"  (LocalNet's decode done: ResourceHandler::respond)
750.5ms presented; redraw flag cleared was Requested
750.5ms Presented(Stopped): InFlight -> InFlight
751.4ms CALLBACK Stopped: InFlight -> Idle
(nothing after this: 5 s idle, zero frames)
```

The callback arrived 0.9 ms after the present, as every callback did on this Background surface
(callbacks equal frames in every run, `watchdog_fired` 0): cosmic-comp #2777 is not involved
here and the watchdog had nothing to cover. The request did reach the host; the host threw it
away. `ctx::paint::render` cleared the shell provider's redraw flag after presenting, because
`resolve` asks for a redraw itself and the machine would otherwise never idle (SPIKE.md S1
finding 4). But Blitz's `ResourceHandler::respond` (blitz-dom `net.rs`) runs on the net
provider's thread: it sends the decoded image to the document's channel, then calls
`request_redraw`. The channel is only drained by `resolve` (`handle_messages`). A background
image's fetch starts *inside* `resolve` (`flush_pending_style_images`), after that drain, so a
small `data:` or file image decodes and responds within the same render, its message sits in
the channel, and its redraw request is cleared as if it were the render's own. An `img`'s
fetch starts at the mutation, one poll earlier, so it loses the race only when its decode
takes about as long as the render (512 px here; sill's 2176 x 1360 output-sized wallpaper
renders for longer, which widens the window). Nothing else dirties a wallpaper, so the message
stays unread and the surface stays on the old frame for good.

### F41. A `height:100%` page under dioxus-native-dom's unsized `#main` is 0 px tall

The first version of `demo_wallpaper` sized its root `width:100%;height:100%` under the host's
UA sheet (`html,body{height:100%}`) and nothing else: `#main`, the mount point
`dioxus-native-dom` renders every `VirtualDom` into, has `height:auto`, so the root resolved to
0 px. The `css` and `none` runs then showed the compositor's own background on screen
(39,41,42) and a fully transparent capture (0,0,0), not even the fallback; only an `img` showed,
through its intrinsic aspect ratio. The demo now sizes `#main{height:100%}`. sill's wallpaper
(`.sill-wallpaper{height:100%}` inside a `Ds` root, `.ds` in quire's reset has no height, and
the image layers are `position:absolute;inset:0` under `overflow:hidden`) has the same shape;
F171's "uniform black" capture fits it. Not verified in sill (no sill daemon was run here);
reported for sill to check alongside F40.

### F42. Fix: a redraw request from another thread survives the present

`HostShellProvider` records where each `request_redraw` came from (`Origin::of(host thread,
current thread)`, pure): the host's thread (event handling, polls, and the render's own
`resolve`) sets one flag, any other thread (a resource load completing) sets another. The flush
still takes both (`take_redraw`); after presenting, `render` now calls `drop_echo`, which
clears only the host-thread flag. A load that lands mid-render is therefore taken by the next
flush (its `request_redraw` pinged the loop) and fed as `FrameInput::Invalidate` onto the
frame in flight: `InFlight → InFlightDirty`, watchdog armed at the current backoff. The frame
machine needed no new transition, it already said the right thing for that input; what was
wrong was the host never feeding it. On a compositor that never sends the callback, the
watchdog renders it (new sequence test
`frame::tests::a_load_landing_mid_render_is_painted_without_any_callback_then_stays_quiet`:
two renders, one `Arm(50 ms)`, no timer left afterwards); with a callback, the callback does.
A request that arrived before `resolve` drained the message costs at most one redundant frame,
then the machine idles. Tests: `dom::shell_provider::tests::{origin_is_the_host_thread_only_on_the_host_thread,
the_present_drops_the_renders_own_request, a_load_completing_on_another_thread_survives_the_present}`.

**After**, same script, same compositor:

| source | frames at 0.1 ... 2.5 s | screen centre | idle 5 s (frames, commits, watchdog) |
| --- | --- | --- | --- |
| `css` | 0 1 1 3 3 3 3 | 20,120,220 | 3 → 3, 3 → 3, 0 |
| `data` | 0 1 1 3 3 3 3 | 20,120,220 | 3 → 3, 3 → 3, 0 |
| `file` | 0 1 1 3 3 3 3 | 20,120,220 | 3 → 3, 3 → 3, 0 |
| `file`, `--png-side=512` | 0 1 1 3 3 3 3 | 20,120,220 | 3 → 3 |
| `none` | 0 1 1 2 2 2 2 | 20,120,220 | 2 → 2, 2 → 2, 0 |

The traced `css` run after the fix: the worker's request at 739.7 ms, present at 740.9 ms,
`Invalidate: InFlight → InFlightDirty, Arm(50ms)` at 741.0 ms, callback at 741.6 ms (`→ Dirty`,
watchdog cancelled), the third frame presented at 743.8 ms, callback at 746.8 ms `→ Idle`. The
count stops moving 5 ms after the swap and stays put through the 5 s idle. `scripts/spike-accept.sh
demo cosmic c` (the shared nested cosmic-comp, reused, not started): all four runs pass, 5 s
idle 0 frames / 0 commits / 0 main-thread wakeups each.

## Host scale and pre-paint (branch `host-scale`, 2026-09-25)

quire v0.1.4 snaps hairlines and boxes to device pixels at 1.25, 1.5 and 1.75 (quire CONSUMING.md
§2 "Pixel snapping"). It needs two things from a host that resolves its own documents; shell-host
provides both without depending on quire.

### F43. The scale reaches the document as live state; a pre-paint hook runs after the frame's one resolve

**API.**

- `shell_host::use_surface_scale() -> ReadSignal<Scale>` (next to `use_surface`), and
  `SurfaceHandle::current_scale(&self) -> Scale` for a handler or task. `Scale(u32)` in 120ths,
  from `wp_fractional_scale_v1.preferred_scale`, else `preferred_buffer_scale`/the output's
  integer scale (`scale_factor_changed`). The surface's `LiveScale` is part of its
  `SurfaceHandle`, a root context before `initial_build`, so the first render reads it. A
  surface starts at `Scale::ONE`; a popup starts at its parent's scale.
- `HostConfig::prepaint: Prepaint`, `Prepaint::{None, Hook(Arc<PrepaintFn>)}` with
  `PrepaintFn = dyn Fn(&mut BaseDocument) + Send + Sync` and `Prepaint::hook(f)`. Per host,
  not per document, because the design system is the whole shell's and a popup opened from a
  component must get it too (`#[serde(skip)]`; equality is `Arc::ptr_eq`). sill passes
  `Prepaint::hook(ds_native::snap_to_device)` and maps `use_surface_scale()` to
  `Ds { scale }`/`HostScale`.
- `FrameCounters::prepaints` (`#[serde(default)]`): renders that ran the hook.

**One resolve per frame.** A frame used to resolve twice: `ctx::paint::render` (for regions and
popup fits) and again inside `Presenter::present` / `gpu::render::render_into`. Blitz's
`resolve_layout` runs `taffy::round_layout` unconditionally on every resolve, so a snap after the
first resolve would have been undone by the second before the paint. `SurfaceDocument::resolve`
now resolves and then runs the hook, it is the frame's only resolve, and the presenters paint
what it left, so regions, `FitContent` measurement, hit testing and pixels all read the same
snapped layout. The debug `capture` still resolves (with the hook) because it is its own render.

**Scale before the poll.** The fractional handler only records the preferred scale per
`wl_surface`. It used to reach the surface in phase 3 (`sync_geometry`), after the documents were
polled, so a tree reading the scale would have drawn the first frame at the old value and a second
frame at the new one. `ctx::rescale::follow_all` now runs at the start of phase 1: it publishes a
changed scale (waking only the documents that read it), and the same flush's poll re-renders them
before the frame is painted. An unchanged scale wakes nothing.

**Tests.** `live_scale::tests::only_a_different_scale_is_a_change_and_wakes`;
`ctx::spawn::tests::{use_surface_scale_starts_at_the_published_scale_and_follows_each_change,
resolve_runs_the_prepaint_hook_once_after_layout, without_a_hook_resolve_reports_none_ran}` (the
hook runs once per resolve, after layout, sees the viewport's 1.5, and never on build or poll);
`prepaint::tests::a_hook_equals_only_itself`.

### F44. `demo_scale` on the shared nested cosmic-comp

`examples/demo_scale.rs`: a 28 px top layer with 1 px rules whose root prints the scale it reads
on every render, and a `Prepaint` hook that counts its calls and records its viewport's scale.
Reports at 2 s, then after 5 s with no input, then exits. Run on the shared nested cosmic-comp
(`wayland-1`, winit output WINIT-0 2176x1360 at 60 Hz, `cosmic-randr list`: scale 100%; reused,
not restarted, output scale not touched):

| moment | frames | commits | callbacks | watchdog | `prepaints` | hook calls | hook scale | surface scale | logical / buffer |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| settled (2 s) | 1 | 1 | 1 | 0 | 1 | 1 | 120 | 120 | 2176x28 / 2176x28 |
| after 5 s idle | 1 | 1 | 1 | 0 | 1 | 1 | 120 | 120 | 2176x28 / 2176x28 |

The root rendered once (`render reads scale120=120`, printed once). Two runs, same numbers.

What the fractional-scale path reports there (`WAYLAND_DEBUG=1`): `wp_fractional_scale_manager_v1`
and `wp_viewporter` are bound; `wl_output.scale(1)`; for the layer's surface the compositor sends
`wp_fractional_scale_v1.preferred_scale(120)`, then `wl_surface.enter`, then the first
`zwlr_layer_surface_v1.configure(2176, 28)`, all in one batch, and the host answers with
`wp_viewport.set_destination(2176, 28)`. So on cosmic-comp the preferred scale lands in the same
dispatch as the first configure, and the phase-1 publish means the first frame's tree has it. At
this output's 1.0 that is also the default, so the run proves the counts and the idle, not a
scale change; the change path is covered by the unit test above. Not run at 1.25/1.5/1.75: the
compositor is shared, and `spike-accept.sh demo cosmic@1.5` would change its output scale for
everyone.

`scripts/spike-accept.sh demo cosmic c` on this branch (same compositor, reused): all four runs
pass (transform, width, tick, fifo), 5 s idle 0 frames / 0 commits / 0 main-thread wakeups each.

## Milestone 0 on the live KWin session (2026-09-25)

### F45. `spike-accept.sh demo kwin` on the user's unlocked Plasma 6 session at scale 2.0

Run once the KDE session was unlocked (KWin 6.7.5, one 3840x2160 output at scale 2.0, NVIDIA
RTX 5070 Ti over Vulkan, blur supported, fractional-scale and layer-shell bound). The first run
failed before any demo started because the invoking shell had no `XDG_RUNTIME_DIR`, so
`wayland-info` and the demos could not find the socket; set it (`/run/user/1000`) and it ran.

| Item | Result | Numbers |
| --- | --- | --- |
| a bar | pass | maximised height 1048 = output 1080 minus 32; bar pixel magenta; checker visible at (10,40); one frame, 4.2 ms |
| b blur | pass | variance drop 99.98 % with blur on; below the bar the checker keeps its variance; KWin blur effect loaded |
| c dock | pass, 4 runs | transform/width/tick at mailbox and transform at fifo: frames = callbacks (72/71/78/69), p95 0.86/1.26/0.96/0.90 ms under the 16.67 ms budget, idle 0 frames, 0 commits, 0 main-thread wakeups, present never blocks while occluded, recovers after |
| f fractional | partial | buffer 3840x64 = round(1920x32 x 2), checker runs of exactly 4 device px, preferred scale 240 matches the compositor; `screen_row_equals_buffer_row` false (max row diff 250, 160 opaque px) |
| g two surfaces | pass | one device, same adapter, dock counted (33 frames), bar untouched, +232 KB RSS for the second surface |

Item f was written for a 1.5 output (kscreen-doctor to 1.5, then the 2-logical-px checkerboard
must be exactly 3 physical px). The user's display is at 2.0 now (they preferred it to 1.5), so
the fractional path itself was not exercised live; at 2.0 the buffer and checker checks pass and
only the screenshot-row-versus-buffer-row equality fails: `f.png` contains no bar at all (the
desktop with a browser window at the top), so spectacle captured before the layer surface was
mapped or after the demo exited; the item's capture timing on a live compositor needs a wait for
the first presented frame before it can be called a rendering bug. The user's display
scale was not changed. Results: `spike/out/demo/kwin/{a,b,c,f,g}.json` (untracked, `spike/out`
is ignored); the a and b screenshots are on the progress page.

## Window interaction (branch `window-interact`, 2026-09-25)

Our client-decorated windows could not be moved or resized. The quire frame (a titlebar drag
region, double-click zoom, the green light's zoom and a Fill/Left/Right/Centre menu) is built
against a seam `ds::HostWindow`; this is the host half.

### F46. The window API a client-decorated frame calls

On `SurfaceHandle` (the document's own surface, `use_surface()`):

| Call | Wire | Notes |
| --- | --- | --- |
| `begin_move(&self) -> Result<(), GrabRefused>` | `xdg_toplevel.move(seat, serial)` | call from the drag region's `onmousedown`, button still down |
| `begin_resize(&self, ResizeEdge) -> Result<(), GrabRefused>` | `xdg_toplevel.resize(seat, serial, edge)` | `ResizeEdge::{Top, Bottom, Left, Right, TopLeft, TopRight, BottomLeft, BottomRight}`; the host's band already does this |
| `set_maximized(&self, Maximize::{Set, Unset})` | `set_maximized` / `unset_maximized` | the zoom button and titlebar double-click |
| `set_minimized(&self)` | `set_minimized` | no un-minimize request exists for a client |
| `set_fullscreen(&self, Fullscreen::{Set, Unset})` | `set_fullscreen(None)` / `unset_fullscreen` | compositor's choice of output |
| `current_toplevel_state(&self) -> ToplevelState` | last configure | for an event handler (toggle zoom) |
| `use_toplevel_state() -> ReadSignal<ToplevelState>` | each `xdg_toplevel.configure` | re-renders only when the state changes, not on a size-only configure |

`ToplevelState { maximized: Maximize, fullscreen: Fullscreen, activated: Activation, tiled:
TileEdges { top, bottom, left, right: Tile::{Tiled, Free} } }`, `TileEdges::any()`. The same ops
are `SurfaceOp::{BeginMove, BeginResize(ResizeEdge), SetMaximized(Maximize), Minimize,
SetFullscreen(Fullscreen)}` for `HostCtx::apply` / `HostHandle::apply`; they do not repaint
(`SurfaceOp::repaint() -> Repaint::NotNeeded`): the configure that answers repaints, so a
refused or no-op request costs no frame. `ToplevelConfig::decorations` is now
`Decorations::Client(ResizeMargin)` or `Decorations::Server`; `Decorations::CLIENT` is the
default 7 px band (**breaking** for a literal `Decorations::Client`; no caller constructed one).
`LiveScale` became `live::Live<Scale>`, the same mechanism that carries `ToplevelState`.

The document's own cursor (Blitz's hover: `cursor:` CSS, text, links) is now shown through
`wp_cursor_shape_v1`; `HostShellProvider` recorded it but nothing ever read it, so every surface
showed the default arrow. A `None` from Blitz (nothing hovered) shows the default rather than
hiding the pointer.

Tests: `grab::tests` (the rule's table), `csd::tests` (band, zones, geometry, content offset at
1.0/1.5/2.0, cursors), `toplevel::tests` (edge wire mapping), `ctx::spawn::tests::{
use_toplevel_state_follows_each_configure, a_move_is_queued_only_with_a_held_press_on_the_surface}`.

### F47. The seat present at startup was never adopted: no real pointer or keyboard input

`handlers/seat.rs` tracked a seat only in `SeatHandler::new_seat`, and every capability handler
returned unless the seat was the tracked one. sctk (0.20, `seat/mod.rs`) calls `new_seat` only
from the registry handler for a seat announced *after* `SeatState::new`; a seat that exists when
the host binds (every seat, in practice) goes straight to `new_capability`. So `input.seat`
stayed `None`, no `wl_pointer` or `wl_keyboard` was ever created, and the clipboard's connection
was never set. Every earlier acceptance drove documents through `debug-inject`, which bypasses
Wayland input, so nothing noticed. Seen in the first `demo_window` run on nested KWin:
`wl_seat.capabilities(3)` then `(7)` arrived and the client sent no `get_pointer`. Fix: the
first capability adopts the seat (`HostState::adopt_seat`, also called from `new_seat`). sill
should re-check anything it believed about real keyboard input through shell-host.

### F48. CSD geometry: the band is surface, the window geometry is the frame

A floating `Decorations::Client` toplevel's surface is its frame plus `ResizeMargin` on every
side (`csd.rs`, pure): the buffer includes the band as transparent pixels, and
`xdg_surface.set_window_geometry(m, m, frame_w, frame_h)` is sent with every configure, so the
compositor measures, places, maximizes, tiles and shadows the frame, and configure sizes are the
frame's (min/max sizes are too). The document's viewport is the frame; it paints at the band's
width rounded to device pixels (`csd::content_px`, `paint_scene`'s offset), pointer positions
are translated back by the same physical amount, and region specs (`Whole`, `Rects`,
`Element`) are the document's, moved into surface coordinates. The input region adds the band
(four strips, corners covered); the blur region never includes it; the IME cursor rectangle is
offset too. Element rectangles therefore stay window-geometry coordinates, which is what
`xdg_positioner` anchors a popup against.

In the band the pointer handler shows `ns-resize`/`ew-resize`/`nwse-resize`/`nesw-resize` (a
corner reaches one band width further along each side than the band's own square) and a
primary press there calls `xdg_toplevel.resize` directly; the document sees neither, and
crossing from the frame into the band sends it the leave. The zone is only re-decided with no
button held, so a drag that began on the document keeps its events across the band. The band
exists only while floating: maximized, fullscreen or any tiled edge give `Inset::NONE` and a
window geometry of the whole surface.

**cosmic-comp 1.8 configures a floating window as tiled on all four edges.** On the nested
cosmic-comp the second configure of `demo_window` is `xdg_toplevel.configure(480, 320,
array[16])`, four states, which sctk reads as `tiled_left/right/top/bottom`, and it stays so
after unmaximize (`array[20]`, plus activated). So on cosmic-comp the host drops the band
(`set_window_geometry(0, 0, 480, 320)`, surface 480x320) and resizing a floating window is
cosmic-comp's own business (it signals "the compositor owns the edges" this way; its own
edge-resize handling was not exercised, there is no pointer on the nested instance). On KWin
the window floats with no tiled state and the band is there. The rule is per configure, so a
compositor that stops sending the tiled states gets the band back without a change here.

### F49. The serial rule, and what a compositor grab does to the buttons

`xdg_toplevel.move`/`.resize` need the serial of a press that is still down: smithay (cosmic-comp)
starts the grab only if the pointer's current implicit grab began with that serial, KWin only for
its latest press; anything else is ignored with no error. `grab.rs` keeps, per seat, the pointer's
surface and the held press (`PointerFacts`, shared with every `SurfaceHandle` as an `Rc<Cell>`),
and `grab_serial` refuses up front: `GrabRefused::{PointerElsewhere, NoPress, PressedElsewhere,
NotAToplevel, NoSeat}`. The handle checks synchronously, so `begin_move()` returns the refusal to
the component; the apply checks again and logs if the press went away in between.

Once the compositor takes the grab, the button's release goes to the compositor, not to us, so
the press would otherwise stay "held" for ever: the zone would stay pinned (the next band press
was delivered to the document) and a later `begin_move` would reuse a dead serial (it returned
`Ok` on KWin with no button down). Now the press is dropped when `move`/`resize` is sent, and the
buttons and press are dropped at every `enter` and `leave` (a leave with a button held means a
grab took the pointer).

### F50. Tiling requests: no compositor offers a client "Left half", "Right half" or "Centre"

xdg-shell has nothing for it: `xdg_toplevel` requests are `set_parent`, `set_title`,
`set_app_id`, `show_window_menu`, `move`, `resize`, `set_max_size`, `set_min_size`,
`set_maximized`, `unset_maximized`, `set_fullscreen`, `unset_fullscreen`, `set_minimized`; the
`tiled_*` states are compositor-to-client only. Globals, from `wayland-info`:

| Compositor | Candidate | Offered | What it can do |
| --- | --- | --- | --- |
| cosmic-comp 1.8.0 (nested, `wayland-1`) | `xdg_toplevel_tag_manager_v1` | absent | - |
| | `zcosmic_toplevel_manager_v1` | v4 | `close`, `activate`, `set/unset_maximized`, `set/unset_minimized`, `set/unset_fullscreen`, `set_rectangle` (the minimize target, not a placement), `move_to_workspace` (deprecated), `set/unset_sticky`, `move_to_ext_workspace`; capabilities enum has no tile/placement entry (cosmic-protocols c0cff4d) |
| | anything with "tile" in its name | none | tiling is keyboard shortcuts and the compositor's own UI |
| KWin 6.7.5 (live session, `wayland-0`, read-only listing) | `xdg_toplevel_tag_manager_v1` | v1 | `set_toplevel_tag`, `set_toplevel_description`: a stable name a user's window rule can match, no placement |
| | `org_kde_plasma_shell` | v8 | `org_kde_plasma_surface.set_position(x, y)` in global coordinates: absolute placement, no tiled state, no size; would need `wayland-protocols-plasma` (not in the pinned block) and is KDE-only |
| | `org_kde_plasma_window_management` | absent for an ordinary client | - |
| | quick tile | D-Bus / KWin scripting only (`workspace` API), not a Wayland request | - |

So the host implements maximize/unmaximize (the menu's Fill) and nothing for Left/Right/Centre.
For the compositor-gaps list: **a client request to tile its own toplevel to a half or centre it
(an `xdg_toplevel` placement hint, or a `zcosmic_toplevel_manager_v1.tile` request with a
`tile` capability entry) is missing on cosmic-comp 1.8; on KWin only `org_kde_plasma_surface.set_position`
(absolute, KDE-only) comes close.** quire's menu should hide Left/Right/Centre until one exists.

### F51. `demo_window` on nested compositors

`examples/demo_window.rs` (480x320 frame, a `rgb(230,60,40)` titlebar whose `onmousedown` calls
`begin_move` and whose double-click toggles zoom, a blue body; stdin `move`, `resize <edge>`,
`maximize`, `unmaximize`, `fullscreen`, `unfullscreen`, `minimize`, `stats`, `quit`; prints each
`ToplevelState` change) was run next to `demo_bar` (a top layer reserving 32 px) with
`WAYLAND_DEBUG=1` for the cursor and geometry requests. Results in `spike/out/window/`
(ignored).

**Nested KWin** (`kwin_wayland --virtual`, 1920x1080 at 1.0, own D-Bus session, started with
`KWIN_WAYLAND_NO_PERMISSION_CHECKS=1` so a small out-of-tree `org_kde_kwin_fake_input` client
could press real buttons; geometry from a KWin script printing `frameGeometry` and
`bufferGeometry`). The shared nested cosmic-comp has no virtual-pointer protocol and its input
comes from the host session, which was not touched, so the pointer items ran here.

| Step | Result |
| --- | --- |
| open | frame 480x320 at (720, 396), buffer 494x334 at (713, 389): the window geometry excludes the 7 px band |
| press on the titlebar, drag +200,+100, release | `begin_move` Ok; frame at (920, 496) |
| press on the body, stdin `move`, drag -100,+50, release | Ok; frame at (820, 546) |
| stdin `move`, no button down | `Err(NoPress)` |
| pointer to the bottom-right band, press, drag +100,+60, release | frame 580x380 at (820, 546): grown from the corner, origin fixed |
| `maximize` | state `maximized: Set`; frame and buffer 1920x1048 at (0, 32): the output minus the bar's 32 px, no band |
| `unmaximize` | frame 580x380 back at (820, 546), buffer 594x394 (band back) |
| `fullscreen` | 1920x1080 at (0, 0) |
| cursor shapes sent, in order | 1 (default, frame), 29 `nwse_resize` (bottom-right band), 1 (back on the frame), 27 `ns_resize` (top band), 1 (frame) |
| idle 5 s after the last configure | 0 frames, 0 commits, 0 callbacks |

**Nested cosmic-comp** (`wayland-1`, shared, reused; WINIT-0 2560x1600 at 1.0; geometry from
`grim` by the titlebar and body colours):

| Step | Result |
| --- | --- |
| open | frame at (1041, 229), 478x318 on screen (cosmic's corner rounding eats the outer pixel); configured tiled on all edges, so no band (F48) |
| stdin `move` (no pointer on the nested output) | `Err(PointerElsewhere)`, nothing sent |
| `maximize` | `maximized: Set`, frame 2560x1568 at (0, 32): the output minus the bar's 32 px |
| `unmaximize` | back to 480x320 at (1041, 229) |
| `fullscreen` | 2560x1600 at (0, 0) |
| idle 5 s | 0 frames, 0 commits, 0 callbacks |

Not run: the pointer items on cosmic-comp (above), and scales other than 1.0 (the band's
device-pixel rounding at 1.5 and 2.0 is in `csd::tests`).

## Real input on a headless KWin (branch `kwin-fake-input`, 2026-09-25)

### F52. The headless-KWin input recipe, and how fake input is authorised

For acceptance with real Wayland input (serials, implicit grabs, enter/leave, keymaps) without
touching the user's session. F51 used it from a scratch directory; it now lives in the repo:

- `scripts/dev/kwin-virtual.sh start [WxH]` starts `kwin_wayland --virtual --no-lockscreen
  --no-global-shortcuts` on a private socket (`wayland-kwinvirt-<pid>`) with a private
  `dbus-daemon --session`, waits for the socket, and writes `scripts/dev/.kwin-env`
  (`WAYLAND_DISPLAY`, `DBUS_SESSION_BUS_ADDRESS`, `XDG_RUNTIME_DIR`, pids, log path). `stop`
  kills KWin, every process whose environment names that bus (D-Bus activation starts portals,
  fcitx5 and ksecretd on it), and the bus. (Narrowed by F54: `stop` now ends the scope `start` made.)
- `tools/fake-input` (Rust, its own Cargo workspace because it needs `wayland-protocols-plasma`,
  which is KDE-only and not in the pinned block; not built by the shell-host gates) and its
  wrapper `scripts/dev/fake-input.sh`: stdin commands `move X Y` (`pointer_motion_absolute`),
  `press|release left|right|middle` (`button` with evdev 272/273/274), `key KEYSYM down|up` and
  `type "text"` (`keyboard_key`, v4+), `sleep MS`; `ok <line>` after each command's roundtrip.
  Keysyms are resolved against the keymap KWin itself sends on `wl_keyboard.keymap`, so the
  evdev code is right for whatever layout KWin runs; `type` adds `KEY_LEFTSHIFT` for a
  character on the shift level. KWin hands every client a descriptor of the same keymap file,
  so it is read with `read_exact_at(0)`: a plain read from the shared offset failed to parse
  once another client had read it.
- `scripts/dev/kwin-windows.js` prints `WINDOW {class, caption, frame, buffer, maximize,
  fullscreen, active}` per window; `scripts/dev/kwin-windows.sh [--class APP_ID]` loads it via
  `org.kde.kwin.Scripting.loadScript`, runs and unloads it, and cuts the lines from KWin's log
  (`QT_LOGGING_RULES=js=true`, `QT_FORCE_STDERR_LOGGING=1`). `frame` is the window geometry,
  `buffer` the whole surface.

**Authorisation.** `org_kde_kwin_fake_input` is one of KWin 6's restricted interfaces: the
global is advertised only to a client whose executable has a `.desktop` file listing it in
`X-KDE-Wayland-Interfaces` (as `/usr/share/applications/org.kde.krfb.virtualmonitor.desktop`,
`org.freedesktop.impl.portal.desktop.kde.desktop` and the fcitx5 ones do), or to every client
when KWin runs with `KWIN_WAYLAND_NO_PERMISSION_CHECKS=1`. `--virtual` alone does not open it:
a headless KWin 6.7.5 started without the variable does not list the global in `wayland-info`
and `fake-input` fails with "org_kde_kwin_fake_input v4+ not offered". `kwin-virtual.sh` sets the
variable (only on that instance; never set it on a real session). The protocol's
`authenticate(application, reason)` is still sent first; with the variable KWin grants it at
once, no prompt.

**Isolation.** `kwin-virtual.sh` gives KWin an empty `XDG_CONFIG_HOME`
(`scripts/dev/out/kwin-virtual-config`): with the user's own config it loaded the user's KWin
scripts (its log showed `macspaces: loaded (maximize trigger on)`, `reveal-window-titlebar`,
`window-corner-overview`), which act on the windows under test. `spike-accept.sh`'s own
kwin-virtual does not isolate this yet, and F51's runs had those scripts loaded (maximize still
gave the expected geometry).

**Verified** on this branch with `demo_window` (same code as master): frame
`[720,380,480,320]`, buffer `[713,373,494,334]`; `move 900 396`, `press left`, `sleep 100`,
three moves to (1100, 496), `release left` gave `begin_move` Ok and frame `[920,480,480,320]`
(exactly +200,+100); `key a down/up`, `type "Hi!"`, `key Return down/up` reached the client as
14 `wl_keyboard.key` events with the right evdev codes (30; 42+35; 23; 42+2; 28).

**Limits.**
- A pointer drag must leave the client time between the press and the first motion (here
  100 ms): the client answers the press with `xdg_toplevel.move`, and motion KWin handles before
  that request arrives is plain motion; without the sleep the window did not move at all.
- Fake input reaches KWin's own clients only: it is injected as a KWin input device, so it
  goes wherever KWin routes input. It cannot reach the shared nested cosmic-comp (a client of
  the user's session, not of this KWin) or the user's session.
- cosmic-comp nested *inside* the headless KWin (`COSMIC_BACKEND=winit`, a client of it) is
  untested. Its winit window is an ordinary KWin client, so it should receive the fake pointer
  and keys when under the pointer / focused, and forward them into its own seat; coordinates
  would be the winit window's (find it with `kwin-windows.sh`), the keyboard would go through
  cosmic's own keymap rather than the one `fake-input` resolved against, and a pointer lock or
  confinement by cosmic-comp could change motion delivery. Expected to work, not verified.

## Seat re-announcement and the headless-KWin teardown (branch `seat-reannounce`, 2026-09-25)

### F53. A re-announced capability made a second `wl_pointer`; one press arrived once per pointer

Found by sill (its G50). KWin re-announces the seat's capabilities whenever its input devices
change: every `org_kde_kwin_fake_input` client is a new device, so connecting one sends
`wl_seat.capabilities(3)` then `(7)` (pointer, then touch, added) and disconnecting sends `(6)`
then `(2)`. A real seat does the same when a mouse is unplugged and plugged back. sctk 0.20
turns each flip into `remove_capability`/`new_capability` (it compares against its own
per-seat `has_pointer`, so an unchanged capability is not re-delivered). shell-host set
`input.pointer = None` on removal, which drops the proxy without sending `wl_pointer.release`,
so the old object stayed live on KWin's side, and the next `new_capability` added a second
one: every later press arrived once per pointer ever made. Traced with `WAYLAND_DEBUG=1` on
`demo_bar` (which now prints `bar: press N` from the bar document's `onmousedown`), headless
KWin 6.7.5, one client that only moves, then one that presses once:

```
wl_seat#12.capabilities(7)
 -> wl_seat#12.get_pointer(new id wl_pointer#91)
wl_seat#12.capabilities(6)
wl_seat#12.capabilities(2)
wl_seat#12.capabilities(3)
wl_seat#12.capabilities(7)
 -> wl_seat#12.get_pointer(new id wl_pointer#87)
wl_pointer#87.button(5, 341930300, 272, 1)
wl_pointer#91.button(5, 341930300, 272, 1)
bar: press 1
bar: press 2
```

**Fix.** `input::capability::capability_change(Announcement, Held) -> CapabilityChange::{New,
Repeat, Removed, Absent}` (pure, table-tested): a capability announced while its object is held
reuses it; one removed while held releases it. `handlers::devices` acts on the decision: removal
destroys the cursor-shape device, sends `wl_pointer.release` (`wl_keyboard.release` for the
keyboard; both since `wl_seat` v3, dropped without a request before that), sends the surface
under the pointer a leave and clears held buttons and the grab record; for the keyboard, raises
`KeyboardFocus{Lost}`, stops the repeat and clears the modifiers. The text input and the data
device belong to the seat and are reused by a keyboard that comes back (the data device was
re-created per keyboard before). `remove_seat` releases both objects the same way. The same
run after the fix:

```
 -> wl_seat#12.get_pointer(new id wl_pointer#91)
wl_seat#12.capabilities(6)
wl_seat#12.capabilities(2)
 -> wl_pointer#91.release()
wl_seat#12.capabilities(3)
wl_seat#12.capabilities(7)
 -> wl_seat#12.get_pointer(new id wl_pointer#87)
wl_pointer#87.button(5, 342074599, 272, 1)
bar: press 1
```

Before: 2 document presses and 4 `wl_pointer.button` events for one press and release. After:
1 and 2. The keyboard was not re-announced by fake input in this run (`capabilities` kept bit
2 throughout), so its path is covered by the decision table and the shared code, not by a trace.

### F54. `kwin-virtual.sh stop` killed its caller; it now stops the scope `start` made

Found by sill (its G52). `stop` killed every process of the user whose `/proc/<pid>/environ`
named the private bus, to catch what D-Bus activation had started on it. The environment is
fixed at exec, so a script that sourced `.kwin-env` and then ran anything (including `stop`
itself) matched: in the reproduction a caller runs `bash -c 'scripts/dev/kwin-virtual.sh stop'`
with the bus exported; the old `stop` killed that `bash -c`, then itself (its own environment
names the bus), before reaching the bus, which was left running (`Terminated`, KWin gone, bus
alive). KWin's own `environ` is unreadable anyway (`kwin_wayland` has file capabilities, so
`/proc/<pid>/environ` is root's), so the scan only ever found other processes.

Where the activated services live decided the fix. A `gdbus call` to
`org.freedesktop.portal.Desktop` on the private bus started `xdg-desktop-portal`,
`xdg-permission-store`, `xdg-document-portal`, `xdg-desktop-portal-gtk`, `at-spi-bus-launcher`
and `gvfsd`, each with parent pid 1 (activation daemonizes) and in the caller's own login
cgroup (`session-*.scope`): a parent-chain walk from KWin or the bus misses all of them, and the
cgroup they share is the caller's.

**Rule.** `start` runs a small `inside` step of the script under `systemd-run --user --scope
--collect --unit=kwin-virtual-<pid> --property=TimeoutStopSec=5`: it starts the bus, records its
address and pid, and `exec`s KWin, so the recorded pid is KWin's and the bus, KWin and everything
either starts (the activated services included, since a cgroup survives daemonizing) are in
`app.slice/kwin-virtual-<pid>.scope`. `stop` is `systemctl --user stop` on it. Without a
reachable user manager (`systemctl --user show-environment` fails) or with
`KWIN_VIRTUAL_CONTAINMENT=tree`, `stop` kills the recorded KWin and bus pids and their
descendants by parent chain, excluding its own process and ancestors; a daemonized service is
not reached there (in the test they exited with the bus). A process that only names the bus is
the caller's to end. `.kwin-env` gains `KWIN_CONTAINMENT` and `KWIN_SCOPE`.

**Verified** (headless KWin 6.7.5): with the scope, the caller and the `bash -c` that ran `stop`
survive, KWin, the bus and the portals on it are gone, and no process naming the bus is left;
the same with `KWIN_VIRTUAL_CONTAINMENT=tree`. F53's reproduction runs unchanged on the new
script (fake input still authorised). `spike-accept.sh`'s own `kwin-virtual` mode still scans
environments, but from outside the `dbus-run-session` it started, so its caller never names the
bus; not changed here.

## Notification-banner regions and a bus with nothing activatable (branch `region-elements`, 2026-09-25)

### F55. `RegionSpec::Elements`: a region made of several independently moving elements

The notification banners are several stacked cards, each its own DOM element, each moving on
its own: `RegionSpec::Element { id, radius, inflate }` (G59 and earlier) can only ever follow
one. `RegionSpec::Element` keeps that exact struct-variant shape unchanged — shell-host is a
path dependency of sill, which constructs `RegionSpec::Element { .. }` directly (for example
`crates/sill-surfaces/src/surfaces/launcher/hit.rs`'s `card_region()`), so a shape change here
turns sill master red the moment shell-host master moves. The new case is a sibling instead: a
struct `ElementRegion { id, radius, inflate }` (the same two geometry fields, named so several
of them can live in one `Vec`) and `RegionSpec::Elements(Vec<ElementRegion>)`, the union of
every element's own `rounded_strips`, concatenated (overlapping strips are fine for a
`wl_region`, and cheaper than deduplicating them). `resolve::resolve`'s `Element` arm builds an
`ElementRegion` from its three fields internally and resolves it exactly as before (a missing
element keeps the surface's current region, right for one moving part); `Elements` differs in
exactly one place — it treats an element missing from the document as contributing nothing to
the union rather than pending the whole thing, so one card not yet laid out (or one closed and
about to be removed) never blanks out the cards still there. Both arms resolve through one
shared helper (`resolve::rounded_rect`, taking an `&ElementRegion`), so the concatenation and
the strip decomposition are not duplicated between them.

`crates/shell-host/src/resolve.rs`'s pure `resolve` gained the `Elements` arm; both crates'
test suites gained table tests against it — two overlapping cards resolving to the concatenation
of their own strips, one of two missing contributing nothing while the other still resolves,
every element missing being an empty union (not `Resolved::Pending`), and (in
`tests/resolve.rs`, the public-API suite) that resolving `Elements` matches resolving the
equivalent `Rects` spec built by hand, present and with one element missing.

`demo_dock`'s `--cards=two` renders a second, plain in-flow element (`#dock-card`, no
`position:absolute`: an absolute sibling here lost pointer hit-testing before, SPIKE.md S1
finding 2) above the pill, both driven by `RegionSpec::Elements`, each printing `dock: card
press N` / `bar: press N`-style (FINDINGS F53) on its own `onmousedown` so a `kwin-virtual.sh` +
`fake-input.sh` run can confirm on a real compositor that a click on the card, a click on the
pill and a click in the gap between them are routed (or not) by the element each is meant to
follow — `debug-inject`'s `stats` carries no region geometry to read back directly (`FrameStats`
has counters and percentiles only), so this is the level the demos actually expose; wired
through `DockProps { magnify, cards }` and `--cards=one|two` alongside the existing `--mode` and
`--layer` (`dock_options`), reaching `demo_bar --with-dock` the same way.

### F56. `kwin-virtual.sh`'s private bus had the stock session config's service directories

Found while wiring notification-banner acceptance up to `kwin-virtual.sh` (F52): its `inside`
started `dbus-daemon --session`, the stock config, which — like the `dbus-run-session` sill's own
`dev/private-bus.conf` was written against (that file's own comment, "M6 freeze") — activates
whatever claims a name. `/usr/share/dbus-1/services/org.kde.plasma.Notifications.service` claims
`org.freedesktop.Notifications` on this machine, so a shell-host client's real `Notify` call
would D-Bus-activate KDE's own notification daemon inside the headless session, which would
answer instead of leaving the call to exercise shell-host's own banners — the same failure mode
sill solved for its acceptance, now reached from the KWin side instead of `dbus-run-session`.

**Fix.** `scripts/dev/private-bus.conf`, the stock `session` bus type with no `<servicedir>` and
no `<standard_session_servicedirs/>` — equivalent to sill's own `dev/private-bus.conf`, `<listen>
unix:tmpdir=/tmp</listen>` and all — so nothing on the bus is activatable. `kwin-virtual.sh`'s
`inside` runs `dbus-daemon --config-file=scripts/dev/private-bus.conf --fork --print-address=1
--print-pid=1` in place of `--session`; `--fork`, `--print-address` and `--print-pid` (and so the
address/pid capture into `$BUS_FILE`, and the scope containment from F54) are unaffected — only
which bus config is loaded changes.

**Verified** on a headless KWin 6.7.5 started this way: `busctl --address
"$DBUS_SESSION_BUS_ADDRESS" list --activatable` lists only `org.freedesktop.DBus` itself (no
`org.freedesktop.Notifications`), and `busctl --user list` against the same address (sourcing
`.kwin-env`, matching how a caller actually uses this bus) likewise shows no such name; a
`gdbus call` of `org.freedesktop.Notifications.Notify` on that bus fails
`GDBus.Error:org.freedesktop.DBus.Error.ServiceUnknown: The name org.freedesktop.Notifications
was not provided by any .service files`, rather than starting anything. `kwin-virtual.sh stop`
afterwards ended KWin and the bus and left the caller and its shell running, matching F54.

## Pointer enter and unmap (branch `pointer-enter-unmap`, 2026-09-26)

### F57. An enter moved nothing: one motion onto a surface hovered nothing

Found by sill (its G100). `handlers/pointer.rs` took `wl_pointer.enter` as focus, zone and
cursor only. The enter carries the position, and a pointer that reaches a surface in one motion
gets the enter and no motion (KWin sends exactly that; so does a surface mapped under a still
pointer), so the document's pointer stayed where it was and nothing was hovered until the next
motion: no `:hover`, no `mouseenter`, the document's cursor not shown. Traced with
`scripts/dev/accept-hover.sh` (new: `examples/demo_hover`'s two stacked 320 x 120 cards, `top`
on the Overlay layer over `bottom` on Top, each printing `hover: <card> enter|leave`;
`HOVER_TRACE=1` adds `WAYLAND_DEBUG`) on headless KWin 6.7.5, pointer rested at (100, 100)
and then `move 960 540` onto the cards' centre:

```
wl_pointer#25.leave(8, wl_surface#54)                          <- move 100 100
wl_pointer#25.enter(9, wl_surface#54, 160.00000000, 60.00000000) <- move 960 540: no motion
wl_pointer#25.motion(349050373, 161.00000000, 60.00000000)     <- move 961 540
hover: top enter
```

`FAIL enter_one_motion:  (wanted, in order: top enter)`, then `after a second motion: top
enter`. The same gap made the card beneath a hidden card (F58's step) report nothing: KWin's
`enter(11, wl_surface#27, 161, 60)` for `bottom` produced no `bottom enter`.

**Fix.** The enter delivers the move to its position (`HostState::pointer_moved(id, pos)`, the
same path a motion takes, so the band's zone rule holds), after focus and zone and before the
cursor, which is then the document's own for what is under the pointer. This is the rule the
plan already had for a press ("synthesize PointerMove before PointerDown"), now also at the
enter; `input::pointer`'s module rules say so. cosmic-comp #2230 is unchanged by it: a layer
surface mapped under a still pointer there may get no enter at all, and the press's own move
still covers a click; only a hover waits for the first motion on that compositor.

After: `PASS enter_one_motion: top enter` (and `hide top` now gives `bottom enter` from KWin's
enter alone, before any motion).

### F58. An unmapped surface kept its hover; the compositor's leave named a surface the host no longer knew

Found by sill (its G101). `ctx/ops.rs::unmap` (a `SetVisibility(Hidden)`) rebuilds the
`wl_surface` but never ended the pointer's stay on it: the document kept its hover chain and
the host kept `pointer_focus` (and the grab record's focus) on the id. The compositor's leave
then names the destroyed object (KWin sends `leave(serial, nil)`), which `surface_id_for`
cannot match, so nothing ever told the document. When the surface was shown again, the element
under the pointer was still the hovered node (the new card reuses the removed one's node), so
Blitz saw no hover change and dispatched no `mouseenter`, even after F57's move on the enter and
a nudge: in sill, no hover pause and no close button on the next banner until the pointer left
the column and came back. `close` (host-initiated) and `close_surface_and_popups` (the
compositor's `closed`/`request_close`/`done`) had the same omission for the records; their
document goes with the surface.

`scripts/dev/accept-hover.sh`, headless KWin 6.7.5, the pointer resting on the cards' centre;
`hide top`, then `show top` (with F57 already fixed, so the only failures are this one):

```
hover: hide top
 -> zwlr_layer_surface_v1#58.destroy()
 -> wl_surface#54.destroy()
 -> zwlr_layer_shell_v1#11.get_layer_surface(new id zwlr_layer_surface_v1#133, wl_surface#126, nil, 3, "hover-top")
wl_pointer#25.leave(11, nil)
wl_pointer#25.enter(11, wl_surface#27, 161.00000000, 60.00000000)
hover: bottom enter
hover: show top
wl_pointer#25.leave(14, wl_surface#27)
wl_pointer#25.enter(14, wl_surface#126, 161.00000000, 60.00000000)
hover: bottom leave
wl_pointer#25.motion(349383501, 162.00000000, 60.00000000)     <- the nudge: still no top enter
```

`FAIL unmap_under: hide top,bottom enter` (no `top leave`), `FAIL remap_under: show top,bottom
leave` (no `top enter`), and after a nudge still no `top enter`. On master (neither fix) the
first two lines of each step were all there was: `hide top` alone, `show top` alone.

**Fix.** The pointer-focus record is a type, `input::focus::PointerFocus::{Nowhere,
On(Entered { surface, serial })}`, with the pure transitions `enter(surface, serial)` and
`depart(focus, surface) -> (PointerFocus, Departure::{Lost, Unaffected})` (table-tested,
including an unmap followed by the enter beneath and a second departure of the unmapped id).
`HostState::withdraw_pointer(id)` departs the id and, when that took the focus, does what a
leave does (zone to the document, buttons and press forgotten, position parked at (-1, -1),
`SurfaceDocument::pointer_left`: the move to (-1, -1), so Blitz's own hover change runs
`mouseleave`, then `clear_hover()`), without invalidating (the surface is going; a remapped one
starts a new frame machine). `unmap` calls it before the rebuild, while the old `wl_surface` is
still the id's; `close` and `close_surface_and_popups` call it before the removal. The
compositor's later leave for the dead object is skipped by `surface_id_for` as before, silently
(the host prints nothing for it; sctk passes it on without a warning, since a leave carries its own surface). The leave handler shares the same record update
(`depart`) and reset. A button held on the surface when it goes is forgotten with it: its release
goes to the compositor's grab on a surface that no longer exists.

After, the same run:

```
hover: hide top
hover: top leave
 -> zwlr_layer_surface_v1#58.destroy()
 -> wl_surface#54.destroy()
 -> zwlr_layer_shell_v1#11.get_layer_surface(new id zwlr_layer_surface_v1#133, wl_surface#126, nil, 3, "hover-top")
wl_pointer#25.leave(11, nil)
wl_pointer#25.enter(11, wl_surface#27, 161.00000000, 60.00000000)
hover: bottom enter
hover: show top
wl_pointer#25.leave(14, wl_surface#27)
wl_pointer#25.enter(14, wl_surface#126, 161.00000000, 60.00000000)
hover: bottom leave
hover: top enter
```

```
PASS enter_one_motion: top enter
PASS unmap_under: hide top,top leave,bottom enter
PASS remap_under: show top,bottom leave,top enter
```

Three runs, 3/3 each. The card beneath reports its enter the moment the top one is unmapped,
with no pointer motion, and the re-mapped card is hovered again at once. sill's `repro` steps
should now pass against this branch (not run here: no sill daemon from this worktree).

## Hover moved by layout (branch `hover-sync`, 2026-09-26)

### F59. An element that arrives under a resting pointer was never entered; Blitz's `refresh_hover` moves the hover silently

Found by sill (its Q170: a notification banner that slides in under a resting pointer never
counts as hovered, so no hold pause and no close button, until the pointer leaves the column
and comes back). Blitz (e99fbdbd) ends every `BaseDocument::resolve()` with `refresh_hover()`
(`blitz-dom/src/resolve.rs:135`, `document.rs:1925`), which re-hit-tests the last pointer
position against the fresh layout and sets `hover_node_id` without dispatching anything; its
own doc comment says so: "TODO: synthesizing pointerenter/pointerleave DOM events for hover
changes caused by layout shifts". The event driver (`events/driver.rs::handle_pointer_move`)
diffs a move against that already-moved `hover_node_id`, so when the pointer then moves over
the new element the diff is empty: no `pointerover`/`pointerenter`/`mouseenter` for the new
chain, no `pointerout`/`pointerleave`/`mouseleave` for the one it covered. Moving the pointer
within the element never repairs it.

A second, smaller loss on the same path: the `:hover` restyle `refresh_hover` asks for is left
for the next resolve, and its `request_redraw` comes from the host thread during the render, so
`ctx::paint` drops it as the render's own echo (SPIKE.md S1 finding 4). A pure-CSS `:hover` on
an element that arrived under the pointer waited for some unrelated frame.

**Fix (host level; Blitz untouched).** `SurfaceDocument` remembers the last real pointer
position it was fed (`dom::hover_sync::LastMove`, from `handle_event`: a move as is, a press or
release as a move with the buttons it left held) and forgets it in `clear_hover` (so in
`pointer_left`, the leave, the resize band and F58's unmap withdrawal). `resolve(t)` takes the
hovered element before and after `base.resolve(t)` (a hovered text node counts as its element);
when they differ and a move is known, `hover_sync::decide` (pure, table-tested) says how to put
the old hover back without dispatch: probe `hit()` at the old element's current border box,
centre first and then the four corners 1 px inside, lift each hit as `set_hover_to` does
(`nearest_non_anonymous_ancestor`) and then to its element, and `set_hover_to` the first probe
that yields exactly the old element (`HoverSync::Restore`); if none does (it left the layout, or
is covered at every probe) `clear_hover()` (`HoverSync::Clear`). Then the remembered move is fed
again through `handle_ui_event`, and Blitz's own driver dispatches exactly the leave/out and
enter/over diff (plus one harmless `pointermove` at the same point). At most one repair per
resolve. It runs after the resolve and before the pre-paint hook: it changes hover state and
runs handlers, never layout, so "a frame resolves exactly once, then the prepaint hook runs"
still holds and the hook and the paint see the same layout. `resolve` now returns `Resolved {
prepainted, hover: HoverShift::{Moved, Kept} }`, and `ctx::paint` feeds an `Invalidate` after
`Presented` when the hover moved, so the restyle lands on the next frame instead of being
dropped with the echo.

Known limits: when the old element cannot be restored (covered everywhere, or removed), the
repair starts from no hover, so the new chain's shared ancestors get a second `pointerenter`
(harmless for sill's banners, whose column is not a listener; a removed element gets no leave,
as in a browser). Coordinates assume the document viewport is not scrolled (the host's surfaces
never scroll the viewport; element scrolling is unaffected). blitz-shell windows (anything run
through `dioxus-native`'s own shell rather than this host) are not covered: the repair is in
`SurfaceDocument`, not in Blitz. The same recipe is in quire's ds-native `Harness` (branch
`hover-sync`), which drives documents without this host.

**Proof.** `tests/hover_sync.rs`: a real `SurfaceDocument`, the pointer resting at (100, 70)
on a `pad`, then a `card` rendered over it through a `use_watch` channel: at that resolve the
handlers see `pad leave, card enter`; a 1 px nudge then dispatches nothing; moving off gives
`card leave, pad enter`; removing the card under the pointer gives `pad enter`; with no pointer
nothing is dispatched. With the repair disabled the first two fail with `[]` (the bug).

`examples/demo_hover` has a third surface, `slide` (320 x 120 at the output's bottom-left,
Overlay): a `pad`, and a `slider` placed under the resting pointer 600 ms after the first move
on the pad; both count `onpointerenter`/`onpointerleave`, and `stats` on stdin prints
`hover: stats {"pad":{..},"slider":{..}}`. `scripts/dev/accept-hover.sh` (headless KWin 6.7.5
through `kwin-virtual.sh`, input from `tools/fake-input`, F52; nothing on the user's session,
no keyboard grab) has a new step `slide_under_resting`: `move 160 1020`, rest 1.5 s, nudge to
(161, 1020), `stats`. With the repair:

```
PASS slide_under_resting: top leave,slide pad enter,slide placed 160 60,slide pad leave,slide slider enter,stats {"pad":{"enter":1,"leave":1},"slider":{"enter":1,"leave":0}}
PASS slide_counts: {"pad":{"enter":1,"leave":1},"slider":{"enter":1,"leave":0}}
```

three runs, 3/3, the F57/F58 steps passing alongside. The same build with the repair disabled:

```
FAIL slide_under_resting: top leave,slide pad enter,slide placed 160 60,stats {"pad":{"enter":1,"leave":0},"slider":{"enter":0,"leave":0}}
FAIL slide_counts: {"pad":{"enter":1,"leave":0},"slider":{"enter":0,"leave":0}}
```

sill's `accept-notifications-real-input.sh repro` (`repro_card_under_resting_pointer`) was not
run from this worktree.

## Drag source and modifiers without focus (branch `drag-source`, 2026-09-26)

### F60. G120: a drag source (`SurfaceHandle::start_drag`)

For sill's screenshot thumbnail (dragged out as a file) and later text drags. The API:

```rust
impl SurfaceHandle {
    pub async fn start_drag(&self, spec: DragSpec) -> DragOutcome;
}
pub struct DragSpec { pub payload: DragPayload, pub actions: DragActions, pub icon: DragIcon }
pub enum DragPayload { Uris(Vec<String>), Text(String), Bytes { mime: Mime, bytes: Vec<u8> } }
pub struct Mime(/* private */);          // Mime::parse("image/png") -> Result<Mime, BadMime>
pub enum DragActions { Copy, Move, CopyOrMove }
pub enum DragIcon { None, Root { root: Root, size: Size } }
pub enum DragOutcome { Dropped { action: DragAction }, Cancelled, Unsupported { why: DragRefused } }
pub enum DragAction { Copy, Move }
pub enum DragRefused { NoPress, PressedElsewhere, NoDataDevice, OldProtocol, InProgress, NoSurface }
```

Call it while the button that began the gesture is still down (a pointer-move handler past a
threshold). The handle queues `Request::StartDrag`; `ctx::drag::start` refuses up front, as
`Unsupported`, when the compositor would ignore or break it: another drag of ours still running,
the surface gone, no `wl_data_device_manager` or no data device on the seat (it is made with the
keyboard, F53), a manager older than v3 (no `dnd_drop_performed`/`dnd_finished`/`action`, so
the source would never learn how the drop ended), or no held press on this surface
(`drag::serial::drag_serial` on the grab record's press, F49: set by every press, cleared when
the last button is up or the pointer leaves; unlike a move the pointer need not still be on the
surface). Otherwise it creates the source offering the payload's types (`drag::offer`: `Uris` as
`text/uri-list` with CRLF lines, then the three text types newline-joined; `Text` as the paste
types; `Bytes` as its one mime) with `set_actions`, and sends `start_drag` with that serial.
`handlers::drag_source` owns the rest: `send` queues the pipe; phase 4 of the flush
(`ctx::drag::register_writes`) makes it non-blocking (`O_NONBLOCK` through
`UnixStream::set_nonblocking`, safe std, no new dependency) and writes it from a calloop
`Generic` source, one chunk of at most 64 KiB per writable wakeup, closing it when done or when
the reader goes (`drag::pipe`), so a slow reader never blocks the Wayland thread; `action`
records the compositor's choice; `dnd_finished` answers `Dropped { action }` (a finish with no
action chosen is a copy) and `cancelled` answers `Cancelled` (`drag::outcome::outcome`, pure);
either destroys the source and removes the icon. A dropped reply (the host exited mid-drag)
resolves `Cancelled`. `DragOutcome` serializes adjacently tagged
(`{"kind":"dropped","v":{"action":"copy"}}`).

**The icon reached `DragIcon::Root`.** The icon is a `Role::DragIcon` surface (`RoleKind::
DragIcon` is new) built by `ctx::spawn::drag_icon`: a plain `wl_surface` watched for fractional
scale, its document and `SurfaceHandle` made like a popup's, the origin's scale as the first
guess, `Configured::Done` from birth (a `dnd_icon` gets no configure) and no commit before
`start_drag` gives it the role; its first render is the ordinary phase-3 render through the
same presenter (one device, one renderer). On the headless KWin the trace shows
`start_drag(wl_data_source#104, wl_surface#27, wl_surface#83, 19)`, then on `wl_surface#83`
`wp_viewport.set_destination(48, 48)`, `frame`, `attach(wl_buffer#118)` (a Vulkan dmabuf),
`commit`, KWin's `preferred_buffer_scale(1)` and the frame callback's `done`, and
`wl_surface#83.destroy()` when the drag ends. A screenshot (spectacle through the headless KWin's
own D-Bus) mid-drag shows the 48 x 48 document with its corner at the hotspot where the drag
began, (1858, 1008); it had **not** followed the pointer, although KWin's own
`workspace.cursorPos` said (1001, 500) and the same drags deliver `wl_data_device.motion` to
foot at the right place. The headless KWin draws no cursor in its screenshots either (not even
with `-p`), so this reads as that backend not repainting the cursor layer the icon rides on;
whether the icon follows on a real session was not checked (nothing here may touch it). Queued
for a manual check.

**Acceptance.** `scripts/dev/accept-drag.sh` (new): `examples/demo_drag` (new: a 64 x 64
thumbnail at the bottom-right, 1816..1880 x 976..1040, whose press and a move past 8 px drags a
temp file as `DragPayload::Uris`, printing the outcome as JSON, `stats` on stdin) on
`kwin-virtual.sh`'s headless KWin 6.7.5, input from `tools/fake-input`, and a foot 1.27.0
window (`--app-id=drag-target`, running `stty raw -echo; cat > file`, so what foot types into
its terminal lands in a file) in the same session; both clients with `WAYLAND_DEBUG=1`. foot is
not installed here, so the script can fetch it with `dnf download foot fcft` and unpack it
under `scripts/dev/out/foot` (no root; `FOOT=` or `PATH` otherwise). Three runs (two with
`DragIcon::None`, one with `DRAG_ICON=root`), 3/3 each:

- `cancel_over_nothing`: press at (1848, 1008), move 10 px (the start), glide to (1888, 1008),
  40 px right and past the thumbnail's edge, release. `outcome {"kind":"cancelled"}`; our trace:
  `start_drag(.., nil, 12)`, `action(0)` (twice), `dnd_drop_performed()`, `cancelled()`. KWin
  sends `dnd_drop_performed` even for a release over nothing, then `cancelled`; the drag first
  crossed our own thumbnail, which our device saw as one `wl_data_device.enter` and its `leave`
  (checked: balanced by the end, and no pointer enter back onto the thumbnail before it).
- `drop_on_foot`: the same press, glide to the centre of foot's frame (612,269 696x542),
  release. `outcome {"kind":"dropped","v":{"action":"copy"}}`; ours: `action(0)` x3,
  `action(1)`, `dnd_drop_performed()`, `send("text/uri-list", fd)`, `dnd_finished()`. foot's
  side, from its own trace: `enter(.., wl_data_offer)`, the four `offer`s, `source_actions(1)`,
  `accept(24, "text/uri-list")`, `set_actions(1, 1)`, `action(1)`, `drop()`,
  `receive("text/uri-list")`, `finish()`. foot's terminal received
  `'/tmp/shell-host-demo-drag-<pid>.txt'` (the path, shell-quoted, no newline).

Not covered live: the `Unsupported` answers (table-tested in `drag::serial`; the others are
plain checks), `Move`, `Text`/`Bytes` payloads, and a reader that stalls (the non-blocking
write is unit-tested against a real pipe filled to `WouldBlock`). A reader that neither reads
nor closes keeps its pipe's source registered until the host exits.

### F61. G121: modifier state over a keyboard-less layer surface

Question from sill: with `hot_corners.modifier`, does a `KeyboardMode::None` layer surface
learn that Super is held when the pointer arrives, given that `wl_keyboard.modifiers` belongs to
the keyboard-focused surface? Probed with `scripts/dev/probe-modifiers.sh` (new):
`demo_drag`'s thumbnail with `--keyboard=none|on-demand`, `WAYLAND_DEBUG=1`, on the headless
KWin 6.7.5; fake input: pointer on the desktop at (901, 500), `key Super_L down`, move onto the
thumbnail (1848, 1008), move 2 px, `key Super_L up`, move, then Shift down, move, Shift up. Once
alone and once with a foot window started first. The document prints the modifiers of every
`mouseenter` and of each move whose modifiers changed.

**On KWin the answer is yes, through a KWin-specific path, and one event late for the enter.**
`KeyboardMode::None`, both with and without foot holding keyboard focus, the client's whole
keyboard traffic was (no `wl_keyboard.enter` ever):

```
wl_keyboard#22.repeat_info(0, 600)
wl_keyboard#22.keymap(1, fd, 35356)
wl_pointer#25.enter(12, wl_surface#27, 32.0, 32.0)      <- the pointer arrives, Super held
wl_keyboard#22.modifiers(13, 64, 0, 0, 0)                <- Mod4, right after the pointer enter
wl_keyboard#22.modifiers(16, 0, 0, 0, 0)                 <- Super up, pointer still there
wl_keyboard#22.modifiers(19, 1, 0, 0, 0)                 <- Shift down
wl_keyboard#22.modifiers(22, 0, 0, 0, 0)                 <- Shift up
```

(serials 7/11/15/19 in the run without foot.) Nothing arrived while Super went down with the
pointer elsewhere. So KWin sends the current modifiers to the client with *pointer* focus when
the pointer enters, and each change while it stays, without keyboard focus; the protocol does
not ask for this (modifiers follow `wl_keyboard.enter`), and other compositors are not expected
to do it: smithay, which cosmic-comp is built on, is expected to send modifiers only to the
keyboard focus (not verified here: fake input cannot reach the nested cosmic-comp, F52).

The host takes the event as it takes any (`update_modifiers` sets `SeatInput::modifiers`, with
no surface to check), so the document's pointer events carry it, except the enter: the
modifiers come after `wl_pointer.enter` in the same dispatch, so `mouseenter` saw `0x0` and only
the next move `META`:

```
enter Modifiers(0x0)
move Modifiers(0x0)       <- the host's enter move (F57), same stale state
move Modifiers(META)
move Modifiers(0x0)
move Modifiers(SHIFT)
```

**`KeyboardMode::OnDemand` changes it, but only because KWin focuses it.** On KWin an
`OnDemand` layer surface got `wl_keyboard.enter(.., wl_surface#27, array[0])` when it mapped,
with no click, even with foot mapped 1.5 s earlier and so focused (it took the focus from
foot); from then on it
had ordinary focused-surface modifiers and key events (`key(.., 125, 1)`, `modifiers(.., 64,
..)`), received before the pointer arrived, so its `mouseenter` saw `META`. That is a focus
steal a hot corner must not do, so it is no workaround either.

For sill: a hot corner that reads the modifiers on its `mouseenter` sees nothing held (on KWin
because the modifiers arrive just after the enter, elsewhere because they do not arrive); one
that reads them on the next move sees Super on KWin only. Nothing portable tells
a keyboard-less surface that Super is held, so `hot_corners.modifier` has no portable source.
No workaround was implemented.

### F62. Blitz's default `body { margin: 8px }` kept a bare document off its surface's corner

Found by sill (2026-09-26). Blitz's user-agent stylesheet (`blitz-dom/assets/default.css`,
included as `blitz_dom::DEFAULT_CSS` and added by `BaseDocument::new` whenever
`DocumentConfig::ua_stylesheets` is `None`) has the browser default:

```css
body {
    display: block;
    margin: 8px;
}
```

A document without quire's `Ds` root (whose `css/reset.css` zeroes the body) kept it: a
never-painted 2 px hot-corner surface or an input catcher laid out from (8, 8), 16 px narrower
than its surface, so an `inset:0` box never reached the true (0, 0) corner. sill worked around it
with an oversize box pulled back by a negative margin.

**The reset.** Every document the host builds now gets `BODY_RESET` (`body{margin:0}`,
`dom::document`) as a second user-agent sheet, next to `UA_STYLESHEET`, before its first
layout: `SurfaceDocument::build` (layer, toplevel, popup and drag icon documents all come
through it, `ctx::spawn`) and `headless::render_png`. It goes through
`BaseDocument::add_user_agent_stylesheet`, not `DocumentConfig::ua_stylesheets`: that field
*replaces* `DEFAULT_CSS` rather than adding to it, so using it would mean re-supplying Blitz's
whole default sheet. As a user-agent sheet appended after `DEFAULT_CSS`, it beats the 8 px rule
(same origin and specificity, later) and loses to any rule a document writes itself (author
origin), so a `Ds` root's reset, or any document's own body margin, still decides.

**What it does not fix.** An `inset:0` box on a bare document now starts at (0, 0) and spans
the surface's width, but it is still 0 px tall: Taffy places an absolute or fixed box against its
parent, and that parent is dioxus-native-dom's `#main`, `height:auto` and empty of flow content
(sill F172; quire's `RootExtent::Viewport` answers it for `Ds` roots with
`min-height:100vh; min-width:100vw`). A bare document that must cover its surface sizes its box
in viewport units (`width:100vw; height:100vh`), which now lands exactly on the surface, so
sill's oversize box and negative margin can go. Sizing `#main` from the host was not done: a
percentage height on a content root would then resolve against the surface and change content
layouts (and `content_size` for `PopupSize::FitContent`).

**Proof.** `tests/body_margin.rs`: a bare `inset:0` box is at (0, 0) with the full 120 px
width (and 0 px tall, as above); a bare `100vw x 100vh` box is exactly the 120 x 80 surface; a
document with its own `body{margin:4px}` keeps its box at (4, 4), 8 px narrower; and a headless
render of a bare viewport-sized red box paints pixels (0, 0) and (119, 79) red. With the reset
emptied, the three bare-document tests fail (8, 8 origin; 104 px wide; transparent corner).

The headless snapshot scene (`tests/headless.rs`) had no body reset of its own, so its goldens
had baked in the 8 px margin (the 240 x 160 card drawn from (8, 8), its right and bottom 8 px
cut off, a transparent frame at the top and left). `drift_t0.png` and `drift_t0_1.png` were
regenerated: the card now fills the frame from (0, 0), the drifting box moved up and left by 8 px.

## Host-side scroll physics (branch `scroll-engine`, 2026-09-26)

### F63. The scroll engine: shell-host owns every scroll of a Blitz document

design/11 §11.3.1, done (F7 closed). Wheel, touchpad, trackpoint, palmrest and keyboard input
no longer reach Blitz's default scroll action; `scroll/` runs the §11.4 machine and writes raw
offsets each frame; the host paints its own overlay thumb.

**Shape.** Pure and table-tested: `scroll/config.rs` (`ScrollConfig { scroll, scrollbar }`, one
field per design/22 §3.7 / §3.8 key with its default, serde in the keys' own spelling),
`geom`, `latch` (§11.3.3 latch, touchpad axis lock at 10 px, the 300 ms wheel burst),
`engine` + `animate` (Idle, Pending, Tracking, Momentum, Rebound, Smooth, Held, Spring; each
phase a closed form of the time since it began, so tests advance `now` by hand), `momentum`
(the Mac Mouse Fix drag `v' = -a v^b` and the iOS alternative, with an exact inverse for the
time and speed at which a glide reaches an edge), `velocity` (least squares over the last
80 ms, held-still 50 ms, 3 samples), `rubber` (bounded / linear stretch, its inverse for a
gesture that catches a stretched scroller, the Chromium snap-back and its end rule), `smooth`
(`--e-out` step, 1000 px/s capped at 200 ms), `keys` (line 40, page, held ramp, spring
m 1 k 175 c 20), `indicator` (§11.5.6 geometry, 500 ms hold, 240 ms fade, hover expansion),
`source` (axis frames to detents or phased pans; natural direction for palmrest), `suppress`
(§11.3.14), `cmd` (`ScrollCmd`, `into_view`). Settled values without a design/22 key (R18's
keyboard numbers, the rebound's 1 px / 24 ms end) are constants beside their formulas.
Blitz-facing: `dom/scroll.rs` (the scroller chain under a point, geometry, raw writes, the
markers); `scroll/surface.rs` (+ `surface/{pointer_input,key_input,thumb}.rs`) applies the
engine to one surface's document; `scroll/paint.rs` paints the thumb after the document on both
presenters; `input/scroll.rs` routes Wayland input to the right surface.

**Public API.**
- `HostCtx::set_scroll_config(ScrollConfig)`: a shell reads its settings (sill owns
  `sill/settings.toml`) and calls it at start and on every change. Not in `HostConfig`, which is
  `Eq` and serializable while the physics keys are `f32`.
- `use_scroll(ElementId) -> ReadSignal<ScrollState { offset: ScrollPoint, extent: ScrollExtent,
  phase: ScrollPhase }>`, published after every frame's resolve for each watched element (the
  offset raw, so out of range while stretched). design/11 §11.7 named it
  `ds-native::use_scroll`; it lives here, beside the engine that feeds it, since ds-native has
  no view of the host's engine (quire is updating the doc). The viewport has no element id, so
  it is not watchable; watched entries live as long as their surface.
- `SurfaceHandle::scroll(ScrollCmd::{To, By, IntoView})` with `Animate::{Smooth, Instant}`.
- Markers: `data-wheel="capture"` (the raw `BlitzWheelEvent` goes to the document through
  `handle_ui_event`; its handler must `prevent_default`), `data-overscroll="band"` (the only
  way a scroller becomes elastic: vertical always, horizontal when it has content to scroll;
  the dock, menus and the launcher list simply do not carry it, §11.8 item 8), and, new,
  `data-keys="capture"` (see below).

**Frames.** A scroll input that writes an offset or starts an animation invalidates the
surface; the engine or a fading/expanding thumb makes the frame machine's pacing `Running`
(combined with the document's own animation, so `use_frame_tick` does not tick for scrolling),
so frames continue on callbacks and stop when both settle. The thumb's 500 ms hold is not
rendered: the render arms a one-shot `TimerRequest::ScrollWake` for its end, then the fade
runs on frames. The engine's requests never go through the shell provider's redraw flag, so
F40's echo rule has nothing to drop. Live: 0 frames and 0 commits in 5 s after a settle.

**Blitz, verified at e99fbdbd (no Blitz change needed).**
- Raw offsets survive `resolve`: nothing in it clamps `viewport_scroll` or a node's
  `scroll_offset` (`tests/scroll_dom.rs::raw_offsets_survive_blitz_resolve_even_out_of_range`,
  -40 and 740 on a 700 px range, 1450 on the viewport, two resolves each).
- `scroll_animation` does not interfere: `resolve_scroll_animation` acts only on a `Fling` (from
  a touch pan, which the host never sends) or a `ScrollTo` (Blitz's `scroll_to` with smooth
  behaviour, or CSS `scroll-behavior: smooth` on a programmatic scroll). Neither starts here;
  a document that calls Blitz's own `scroll_to` smoothly would fight the engine, which is why
  `scroll-behavior: smooth` is quire's lint (`lint::Rule::BlitzUnsupported`) and programmatic
  scrolls go through `ScrollCmd`.
- The offsets are written before the resolve: `flush_styles_to_layout` hoists positioned
  (z-indexed) descendants of a scroller with the scroll offset current at that moment, so a
  write after the resolve would leave them one frame behind.
- Blitz hit-tests page coordinates and the host's pointer events carried surface-local ones as
  both; with the viewport scrolled they differ, so `SurfaceDocument::handle_event` now sets page
  = client + viewport scroll (`input::pointer::scrolled`), and F59's hover repair probes in page
  coordinates too.
- `get_client_bounding_rect` of an element subtracts the element's own scroll offset along with
  its ancestors' (`unrounded_absolute_position`), so a scrolled container's box moves with its
  own content. `dom::scroll::area` adds it back for the thumb. `SurfaceDocument::element_rect`
  (regions, popup anchors) still has it: a region element that is itself a scrolled container
  would shift. Not fixed here.
- Blitz's overlay scrollbars are the `scrollbars` feature of blitz-paint / blitz-dom, off in
  this build; `scripts/check-boundary.sh` now fails if it is ever enabled. quire's reset gives
  ds scroll containers `scrollbar-width: none` as well.
- Blitz has no keyboard scroll action at this rev (only text inputs handle arrows), and raw
  writes fire no `onscroll`: `use_scroll` is the notification.
- Blitz's hit test does not clip a scroller's overflow: a point outside a scroll container but
  under its hidden content hits that content, so a gesture there latches the container. Seen in
  a test (a banded sheet's 600 x 400 content inside a 200 x 100 box); not fixed here.

**Keyboard (§11.3.10): the host scrolls only where nothing else wants the keys.** A scroll
key (arrows, PgUp/PgDn, Space/Shift+Space, Home/End, Ctrl+Up/Down; nothing with Alt or Super)
is taken by the engine, and then its press, repeats and release never reach the document, only
when:
- the focus is nowhere (Blitz parks focus on the root element, which counts as nowhere, as does
  the body, unless a key listener sits on them), and a scroller with content to scroll is under
  the pointer (the last gesture's, else the innermost; else the viewport if it scrolls); or
- the focus sits inside a scroller on the key's axis (the innermost on the focus path, else the
  viewport if it scrolls), and no element on the focus path, the focus included, handles keys.

"Handles keys" is detected, not guessed: dioxus-native-dom records every listener as an
attribute named for the event whose value is the placeholder `<rust func>`
(`MutationWriter::create_event_listener`), so an element with a Dioxus `onkeydown`,
`onkeyup` or `onkeypress` carries `keydown="<rust func>"` and so on (`dom::scroll::
listens_for_keys`). That is the mechanism; roles and `tabindex` are not consulted (a focusable
scroller with `tabindex` and no listener should scroll). Also the document's: a focused
editable or form control (`input`, `textarea`, `select`, `button`, `contenteditable`) and
anything inside `data-keys="capture"` (kept as an explicit opt-out). Blitz cannot report
`prevent_default`, so a listener on the path is taken as consuming every scroll key, whether or
not its handler acts on that key; dioxus-native-dom never removes the attribute when a listener
goes away, which errs the same safe way (the key goes to the document). Tests
(`tests/scroll_dom.rs`): a ds-style menu (`role="menu"`, `onkeydown`) inside a scroller keeps
ArrowDown; sill's notification column shape (a focused scroller with its own `onkeydown`)
keeps Down; sill's power menu shape (a focused root with `onkeydown` over a
`data-overscroll="band"` sheet) keeps Left and Right; a plain scroller scrolls 40 px with
nothing focused and again with focus on it; nothing scrollable under the pointer passes the
key. Disabling the listener check fails the first three. The first arrow auto-repeat the
engine took starts the held ramp; its release starts the spring.

**Sources.** A `wl_pointer` frame with `axis_source` wheel or wheel_tilt, or with no source at
all, is detents (`value120`, else `discrete`, else degrees / 15); Shift or tilt makes it
horizontal. A frame with no source is a wheel because such a compositor sends no `axis_stop`
either, so a pan from it could never end; and KWin's fake input sends exactly that (`axis 0
15.0`, nothing else, traced), so the live check exercises this path, not `wheel` +
`value120` (unit-tested). `finger` is a phased gesture with momentum and band; `continuous`
tracks without either. `axis_relative_direction` is read but never applied: compositor values
already carry the compositor's natural setting, so it is not inverted twice (§11.8 item 15).

**Proof.** `tests/scroll_engine.rs` (pure, 120 Hz, `now` by hand): item 1 (2000 px/s for 100 ms:
travel within 476..527, MomentumEnded at 0.976 s +- 1 frame), 2 (60 ms pause: no glide), 3
(boost to 3000 +- 2 % same direction, -2000 opposite; also a touchpad Began during the glide),
4 (touch to stop: unchanged over two frames), 5 (52.1 / 354.8 bounded, 5 / 50 linear, the
first 10 px from the edge), 6 (100 px back to exactly 0 by 400 ms, < 1 px at 368 ms +- 1
frame, monotonic), 7 (peak 18.3 +- 1 px at 80 ms +- 1 frame, settled by 460 ms; again from a
real 3000 px/s flick), 8 (rigid: 500 px of pull, 0 on every frame), 11 (Down 40 px in 40 ms,
PgDn 860 px in 200 ms on 900, End within 200 ms, held arrow 1000 px/s at 0.2 s and the spring
within 1 px of `target - 100 e^(-10t) cos(8.66t)`, settled within 600 ms), 12 (a detent 60 px
in 60 ms, five in 50 ms reach +300 by 250 ms, no stretch at an edge), 14 (still after the
settle). Mutating `b` to 0.6 and the snap rate by 10 % fails six of them. `tests/scroll_dom.rs`
(a laid-out `SurfaceDocument`, no compositor): raw offsets through resolve, only the opted-in
list is elastic, item 8 on a menu-like list, item 9 (a: inner list at its bottom, the page
scrolls and the list never moves; b: inner list mid-way takes the whole gesture, stretches
past its bottom, the page stays at 0 through gesture and rebound), item 10's thumb on the
list's edge (visible, falling after 500 ms, gone at 741 ms), item 11 (focused text input:
`Passed`, nothing moves; PgDn over the page: 560 px on a 600 px view), item 15 through the
palmrest path (Natural: +y finger decreases the offset by exactly 50 px; Traditional
increases it), `data-wheel="capture"` routing, a wheel detent over the list. Item 10's
length rule (75 % at s = 250 on 1000) and the fade are in `indicator`'s tests;
`scroll::paint`'s test renders a thumb through the CPU presenter (0.4 black over white, at 1x
and 2x). `scroll::watch`'s test renders `use_scroll` in a real document.

Live, `scripts/dev/accept-scroll.sh` (new; `examples/demo_scroll`, headless KWin 6.7.5 via
`kwin-virtual.sh`, wheel from `fake-input.sh`'s new `axis` command; nothing on wayland-0, no
keyboard grab):

```
PASS start: {"frames":5,"commits":5,"offset":0,"captured":0}
PASS one_detent: {"frames":11,"commits":11,"offset":60,"captured":0}
PASS five_detents: {"frames":41,"commits":41,"offset":360,"captured":0}
PASS idle: 5 s idle: {"frames":55,"commits":55,"offset":360,...} -> {"frames":55,"commits":55,...}
PASS capture: {"frames":58,"commits":58,"offset":360,"captured":2}
```

(The offset is what `use_scroll` reported to the component.) The first run, before source-less
frames were detents, gave 15 px per event (a pan that never ended); that is what showed KWin's
fake input carries no source.

**Not done, or not proven.**
- Thumb drag and track click: `indicator::drag_offset` is the pure formula, not wired to the
  pointer. `scroll.scrollbars = Always` keeps the thumb (legacy 15 px) but still shows only the
  last-scrolled scroller's; one thumb at a time, so the other-axis shortening never applies.
- The thumb's colour follows the host's colour scheme (0.4 black / white); quire's
  `--scroll-thumb` token is not readable from the host.
- The viewport is rigid unless `html` or `body` carries `data-overscroll="band"`, which a
  Dioxus document cannot set: design/11 §11.3.7's "vertical axis of the document viewport" is
  not elastic here, per the opt-in rule.
- Live: no touchpad (fake input has no finger source), no keyboard (no grab), no palmrest
  (no daemon exists yet), no hover expansion, no visual check of the thumb on screen.
- The one-finger page swipe (12 §12.3.6) and `Tap{One}` SmartZoom have no consumer yet;
  `HostEvent` is exhaustive, so a new event for them is an API change to agree with sill.

### F64. The palmrest client and the double-scroll window

`input/palmrest.rs` (design/12 §12.3.10-11, design/11 §11.7). The host connects to
`$XDG_RUNTIME_DIR/palmrest/events.sock` when it exists, as a non-blocking close-on-exec
`SOCK_SEQPACKET` socket; the reading side is a calloop `Generic` source registered in the flush's
timer phase (`ctx/palmrest.rs`), which drains every datagram on each wakeup and removes itself
when the daemon's end closes. An attempt is made at start and again on the next pointer enter
while unconnected: no timer, no retry loop, and with no daemon nothing happens at all.

**Wire form: palmrest's.** palmrest now exists (master 5baeb41, `crates/palmrest/src/protocol.rs`)
and fixes the encoding: one JSON message per packet, at most 4096 bytes, `PROTOCOL_VERSION` 1,
enums adjacently tagged (`{"kind":"scroll","v":{...}}`, `{"kind":"device_gone"}`), every name
`snake_case`, `GestureId` a transparent `u32`, `DeviceInfo { name, path, vendor, product }`,
`Filter::{All, Only(Vec<Topic>)}`. The host re-declares those types with exactly palmrest's
serde attributes (`input/palmrest/wire.rs`; still no dependency on palmrest's crate). The first
version of this branch used serde's default external tagging and would never have
interoperated. Cross-check: the byte strings in `wire.rs`'s tests were printed by palmrest's
own `protocol::encode` from a throwaway binary depending on the palmrest crate (built outside
palmrest's tree; palmrest untouched); every one decodes into the host's types and re-encodes
byte for byte, and every `ToDaemon` the host can send encodes byte-identically to palmrest's
(`{"kind":"pointer_over","v":{"over":"not_ours"}}`, both `Subscribe` forms). A `Hello` whose
`version` is not 1 stops the read at once: the host logs it and disconnects (`Stream::
Incompatible`, tested against the fake daemon); it tries again on the next pointer enter. A
packet over 4096 bytes is skipped.

**Dependencies.** std has no `SOCK_SEQPACKET` and `unsafe` is confined to the raw window
handle module, so the socket calls come from `rustix`, now in the pinned block (quire b45282a,
copied verbatim: `features = ["std", "event", "fs", "net", "time", "process"]`); it was already
in the build through wayland-backend at 1.1.5. serde_json, previously behind `debug-inject`,
and peniko, previously a dev-dependency, are now plain dependencies (both pinned).

**Routing (`input/palmrest/route.rs`, pure, the §12.3.11 table).** `Touches{Down}` and
`MayBegin` stop a glide or rebound where it is (touch to stop) and open the drop window; `Began`
records the locked axis, and the first `Changed` with a non-zero delta latches (§11.3.3) in its
direction, since palmrest drops the first delta at Began; `Changed` applies `scroll.natural` to
the finger-direction px; `Ended` releases with the host's own samples on palmrest's `t_ns`
timeline (only differences are taken, so it never has to agree with the host clock);
`Cancelled` rebounds with no glide. Events go to the surface holding pointer focus; the pointer
elsewhere, they are ignored. `PointerOver{Ours}` is sent on `wl_pointer.enter` of any of our
surfaces and `NotOurs` when a leave leaves the pointer on none of them; only changes are sent,
and the current state again right after each connect.

**The window (`scroll/suppress.rs`, §11.3.14 rule 2).** A compositor axis frame whose source is
`finger`, `wheel` or absent is dropped while palmrest reports fingers down, while a glide or
rebound palmrest started is running on any surface, or for `scroll.double_scroll_grace_ms`
(150) after palmrest's fingers lift, unless an `axis_stop` arrives first (which is itself
dropped). `continuous` and `wheel_tilt` pass. A real wheel used during the window is ignored,
as the doc accepts.

**Proof.** `input::palmrest::tests` against a fake daemon (a listening `SOCK_SEQPACKET` socket
in a temp directory): no socket is a no-op with no retry until an enter; connecting sends the
current `PointerOver`, only changes follow; `Touches`, a junk datagram and a `Scroll` arrive as
two messages; the daemon closing reads as `Closed`. `route`'s table test and `wire`'s round
trips. Item 13, `tests/scroll_palmrest.rs`: a flick sent by the fake daemon over the socket,
read and routed as the host does, with the same stroke also arriving as compositor finger
frames through the window: every frame's offset equals the palmrest-only run within 1 px
(through the glide to its end); with the window bypassed, the final offsets differ by more than
100 px. Not proven: a real palmrest (it does not exist yet), a real compositor's echo timing,
and `Tap` / `Swipe`, which have no consumer (F63).

## Session lock (branch `session-lock`, 2026-09-26)

### F65. The lock screen's host side: `ext-session-lock-v1`, one document per output

For Milestone 11 (quire design/20 §1.9: a lock surface per output, always keyboard). The API:

```rust
impl<M> HostCtx<M> {
    pub fn lock_session(&mut self, config: LockConfig, factory: impl Fn(OutputInfo) -> Root + 'static)
        -> Result<LockId, LockError>;
    pub fn unlock_session(&mut self, id: LockId);
    pub fn lock_surfaces(&self, id: LockId) -> Vec<(OutputId, SurfaceId)>;
}
pub struct LockId(pub u64);
pub struct LockConfig { pub namespace: String }            // default "lock": surfaces "lock/<output>"
pub enum LockError { Unsupported, InProgress(LockId) }
pub enum LockState { Locked, Unlocked, Finished }           // HostEvent::Lock { id, state }
pub enum LockPhase { Requested, Locked, Unlocking, Unlocked, Finished }
pub fn use_lock() -> LockHandle;                            // in a lock surface's document
impl LockHandle { pub fn id(&self) -> LockId; pub fn phase(&self) -> LockPhase; pub fn unlock(&self); }
pub fn use_lock_phase() -> ReadSignal<LockPhase>;
```

`lock_session` binds nothing new at call time: `ext_session_lock_manager_v1` is bound at start
(sctk 0.20's `SessionLockState`), and without it the call answers `LockError::Unsupported`. A
second call while a lock of ours lasts (requested, locked or unlocking) answers `InProgress`.
Each lock surface is a `Role::Lock` (`RoleKind::Lock`) with its own document built from
`factory(output)` and a `LockHandle` root context beside the `SurfaceHandle`; it takes the
configure's exact size, the existing fractional path (or the integer buffer scale), the frame
machine and the idle rule unchanged, and the first render after the configure is its first
commit. It is never committed before that configure (no initial commit, the protocol's
`commit_before_first_ack` / `null_buffer`), never hidden (`SetVisibility` is ignored: a rebuilt
lock surface could not be mapped the usual way, and its output would fall back to the
compositor's colour), and can carry no popup. The shell never gets a `SurfaceClosed` for one; the
`Lock` event covers them.

**State machine (`lock/machine.rs`, pure, table-tested).** `Requested --locked--> Locked
--unlock--> Unlocking --told--> Unlocked`; `finished` from any live phase ends in `Finished`; an
unlock before `locked` cancels (`Unlocked`). The request each end sends is the one the protocol
allows: `destroy` before `locked` (never `unlock_and_destroy`, `invalid_unlock`), and
`unlock_and_destroy` once locked, also when the compositor ends a lock with `finished` after
`locked` (never `destroy`, `invalid_destroy`). An output added before the end gets a surface, a
removed output's surface is destroyed, and every surface made is destroyed at the end, after
`unlock_and_destroy` so the output never flashes the compositor's colour. The handlers
(`locked`, `finished`, `wl_output` added or removed) only queue inputs; `ctx::lock::advance` runs
the machine at the end of the flush's request phase, where it can make surfaces and presenters.

**"Told" before `unlock_and_destroy`.** `LockHandle::unlock` and `HostCtx::unlock_session` queue
the same `Request::Unlock`. `Unlocking` is published to every lock document (`LockHandle::phase`,
`use_lock_phase`), and `Told` is fed at the next flush, whose first phase polls the documents
that publish woke (a ping makes sure that flush happens): the documents have run with
`Unlocking` before the compositor hears anything. No frame is waited for: a lock screen that
animates its exit (design/20 §1.9's unlock `fade`) plays it first and calls `unlock()` at its
end, since after it the surfaces go at once. In the trace below the unlock
request follows the accepting keypress by 1.1 ms.

**Deviation from the brief: surfaces at the request, not at `locked`.** The protocol says
"clients should immediately create lock surfaces for all outputs on creation of this object",
so a compositor can show them in its first locked frame instead of a blank one; they are not
shown unless the lock succeeds, and a refused lock destroys them. cosmic-comp 1.8 does not wait
for them: it sent `locked` 0.9 s after the request and the surface's first `configure` right
after it (below).

**Keyboard.** Which lock surface has the keyboard is compositor policy; the host does nothing
special, and `wl_keyboard.enter` arrived on the lock surface together with `locked`. A typed key
reaches a text field only if the document has focused it: Blitz's `autofocus` is a blitz-dom
feature the pinned block does not enable, so the demo calls `MountedData::set_focus(true)` after
mount, retried when the renderer holds the document (the reason for quire's `guarded_call`,
sill FINDINGS Q10 and Q43).

**Opaque documents.** The lock document must paint its whole surface. The demo's first run had
`body{height:100%}`, which Blitz did not stretch, and the rest of the lock surface showed
cosmic-comp's lock colour (39,41,42), not the session: a transparent lock surface hides nothing
less, but looks broken. `html { background }` fixed it.

**If the client dies while locked.** The protocol: "the compositor must not unlock the session
in response ... It is acceptable for the session to be permanently locked." Dropping the lock
object after `locked` without `unlock_and_destroy` is `invalid_destroy`, and sctk's `Drop` sends
`destroy`: a host that exits (or panics) while locked leaves the session locked, which is the
secure failure. Seen on cosmic-comp 1.8 (`crash` below): after `kill -9` of the locker the
output shows the compositor's own colour over everything, and a *new* locker's lock is granted
(`locked`) and its unlock returns the session. What the user must do: let sill's session script
restart the shell, whose lock screen locks again (sill should lock at start when logind says the
session is locked, `LockedHint`), or, from a TTY, start another locker (`cosmic-greeter`'s,
`swaylock`) against the session's socket and unlock with it (whether cosmic-comp also honours
logind's `Unlock` was not checked). The host's own `exit()` while locked leaves the session locked too;
the demo's `quit` unlocks first.

**Safety: nothing here may lock the user's session.** The proof runs only on a throwaway stack:
`scripts/dev/kwin-virtual.sh` gained `KWIN_VIRTUAL_RUNTIME=<dir>` (opt-in): KWin gets its own
runtime directory (mode 700, the marker `.shell-host-virtual-compositor` in it), `.kwin-env`
exports it and the socket's absolute path, and `stop` removes it. `stop` used to find the user
manager through the sourced `XDG_RUNTIME_DIR`; with a private one `systemctl --user stop` failed
silently and left KWin running (the first run did), so it now uses the caller's own. The
nested cosmic-comp, a client of that KWin, puts its socket in the same directory. `demo_lock`
refuses to lock unless, checked before it connects (`examples/demo_lock/guard.rs`, table-tested):
`SHELL_HOST_ALLOW_LOCK=virtual`; `WAYLAND_SOCKET` unset; `WAYLAND_DISPLAY` an absolute path to a
socket; `SHELL_HOST_LOCK_RUNTIME` resolves to a directory holding the marker that is neither the
process's `XDG_RUNTIME_DIR` nor `/run/user/<uid>`; the socket (links resolved) lies directly in
it; and it is not `wayland-0`. The script checks the socket too, unlocks on every exit path and
stops both compositors.

**cosmic-comp inside the headless KWin (F52's untested item, now verified).** `COSMIC_BACKEND=
winit cosmic-comp --no-xwayland` as a client of `kwin-virtual.sh`'s KWin works, and fake input
reaches its seat: a click on its window gives it KWin's focus and keys typed afterwards arrive
in the nested client that has cosmic-comp's focus. With `nested.sh`'s Mesa override
(`__EGL_VENDOR_LIBRARY_FILENAMES=50_mesa.json DRI_PRIME=1`) it drew nothing ("pci id for fd 40:
10de:2c05, driver (null)", `BAD_SURFACE`): the headless KWin hands it the NVIDIA render node. The
system EGL vendor (NVIDIA) and `LIBGL_ALWAYS_SOFTWARE=1` both draw; the script uses the system
vendor. `grim` on the nested socket captures cosmic-comp's output, also while locked.

**Acceptance.** `scripts/dev/accept-lock.sh` (new): `examples/demo_lock` (new: `lock`, `unlock`,
`stats`, `quit` on stdin; a lock document per output with the output's name and a plain
`input`; Enter on "letmein" calls `use_lock().unlock()`, anything else is refused and cleared) on
the headless KWin 6.7.5 and a cosmic-comp 1.8.0 inside it, a maximized `probe_toplevel`
checkerboard as the session's window, input from `tools/fake-input`. 3 runs, all PASS:

- `unsupported`: on the headless KWin itself (no `ext_session_lock_manager_v1` in KWin 6.7):
  `lock: error the compositor does not provide ext_session_lock_manager_v1`.
- `lock`: `event {"id":1,"state":"locked"}`; one lock surface for the one output (`WINIT-0`,
  1280x800 logical); grim of cosmic-comp: all four corners 24,28,38 (the lock document), 511995
  of 512000 pixels changed from the checkerboard.
- `finished`: a second `demo_lock`'s lock while the first holds it answers
  `{"id":1,"state":"finished"}`; the first stays locked.
- `typing`: `type abc` with no click after the lock: the field's `oninput` saw 3 characters;
  `KeyboardFocus{Gained}` on the lock surface.
- `wrong`: Enter on "abc": `rejected`, the phase still `locked`, grim still the lock's colour, no
  `unlocked`.
- `idle`: 0 frames on the lock surface over 3 s while locked (5 before, 5 after).
- `unlock`: `letmein` + Enter: `accepted`, `event {"id":1,"state":"unlocked"}`, no lock surface
  left, grim identical to the pre-lock capture (0 pixels differ), the probe window alive.
- `crash`: a new `demo_lock` locked, `kill -9`: grim shows 39,41,42 everywhere (512000 pixels
  differ from the session); a third locker's lock: `locked`, its `unlock`: `unlocked`, and the
  capture is the session again (0 differ).

The wire order (`LOCK_TRACE=1`):

```
 -> ext_session_lock_manager_v1#22.lock(new id ext_session_lock_v1#19)
 -> ext_session_lock_v1#19.get_lock_surface(new id ext_session_lock_surface_v1#31, wl_surface#28, wl_output#8)
ext_session_lock_v1#19.locked()                                          +0.898 s
ext_session_lock_surface_v1#31.configure(9, 1280, 800)
wl_keyboard#23.enter(11, wl_surface#28, array[0])
 -> ext_session_lock_surface_v1#31.ack_configure(9)
 -> wl_surface#28.commit()                                               first frame, +0.364 s
...
lock: accepted on WINIT-0
 -> ext_session_lock_v1#19.unlock_and_destroy()                          +1.1 ms
 -> ext_session_lock_surface_v1#31.destroy()
 -> wl_surface#28.destroy()
lock: event {"id":1,"state":"unlocked"}
```

**Not proven live.** More than one output (the winit backend has one): a surface per output and
focus on "the pointer's output or the first" are the machine's table and the compositor's
policy, not a run; outputs added or removed while locked (table only); a fractional scale on a
lock surface (the nested output is 1.0; the path is the layer surfaces' own); `finished` sent
after `locked` (no compositor here does it); the shm presenter on a lock surface; the lock on a
real cosmic session, which nothing here may touch (a manual check for sill's queue).

## `use_modifiers`: a surface's held/locked keys, live (branch `use-modifiers`, 2026-09-26)

### F66. G180: modifiers reach a document only through a key press; enter and lock-key state did not

Before this branch, `SeatInput::modifiers` (`keyboard_types::Modifiers`) was seat-wide and fed
only `key::key_event`'s `BlitzKeyEvent`s at `press_key`/`release_key`. Nothing published it to a
surface's own document, so a surface learned the modifiers only by receiving an actual key
event on itself. Two cases had none: gaining keyboard focus (`wl_keyboard.enter`) with a
modifier already held (an app switcher's `KeyboardMode::Exclusive` overlay mapped while Super is
down, to catch a quick Cmd+Tab release), and a lock key toggled with no key event at all
(`wl_keyboard.modifiers` alone, the initial Caps Lock state on a lock screen).

The fix is a second, typed cache and a `Live<Modifiers>` per surface, not a change to the
existing path. `crates/shell-host/src/modifiers.rs` (new, top-level so it can be part of the
public surface alongside `toplevel`, `lock`): `Modifier::{Held,Released}` for shift/ctrl/alt/logo,
`LockKey::{Locked,Unlocked}` for caps_lock/num_lock (named `LockKey`, not `Lock`: `lock::machine`
already has a `LockState` for the session lock, and a bare `Lock` next to a `lock` module reads
as the wrong feature), and `Modifiers::from_sctk`, a pure conversion off sctk's own
already-keymap-resolved booleans (`smithay_client_toolkit::seat::keyboard::Modifiers`). No
`bool` anywhere in the public struct, per CONVENTIONS §11.

`handlers/seat.rs`'s `SeatInput` gained `held_modifiers: Modifiers` (aliased `HeldModifiers` at
the import to keep the file's existing `keyboard_types::Modifiers` import as `Modifiers`
unchanged), written in `update_modifiers` from the same `SctkModifiers` argument that already
feeds `modifiers_from` for Blitz's events — two conversions of one value, not a shared
intermediate, so the existing path is untouched line for line. `surface.rs`'s `HostSurface`
gained `live_modifiers: Live<Modifiers>`, threaded through `HandleParts`/`SurfaceHandle` exactly
like `live_toplevel`. `handle.rs`'s new `use_modifiers() -> ReadSignal<Modifiers>` is the same
shape as `use_toplevel_state`/`use_surface_scale`: a `use_signal` seeded from `live.get()`, a
background task awaiting `live.changed_after`. Not delivered through `HostEvent` (told not to:
its match is exhaustive by design); the hook is the only way to it.

`handlers/keyboard.rs`: `enter` now publishes `self.input.held_modifiers` (the seat's
already-current cache) to the newly-focused surface before anything else runs — belt and
suspenders, since `wl_keyboard.enter` "must" be followed by a `modifiers` event per the protocol
(`wayland.xml`), and the protocol also allows a compositor to send `modifiers` "without a surface
of the client having keyboard focus, for example to tie modifier information to pointer focus
instead" (verbatim; this is F61's KWin path), which is exactly why the cache can already be
correct at `enter` time rather than empty. `update_modifiers` now also writes
`held_modifiers` and, `if let Some(id) = self.input.keyboard_focus`, publishes it to that
surface. `leave` publishes `Modifiers::default()` (empty) to the surface losing focus; the
seat-wide cache is untouched, since it stays valid for whoever is focused next.

**A surface's very first `use_modifiers()` read is always the default, before its first
configure — like `use_toplevel_state`, this is documented, not a bug.** `insert()` builds the
`VirtualDom` synchronously while handling the spawn request, strictly before any Wayland
round-trip for the new surface, so the first render always sees `Live::new(Modifiers::default())`.
The real state (from the cache at `enter`, confirmed or corrected by the guaranteed `modifiers`
event beside it) lands a `Live::publish` later, in the same event-loop pass, well before the
surface's first frame is visible on screen — proven below.

**Live, on the headless KWin (`scripts/dev/kwin-virtual.sh`, `tools/fake-input`, F52), never
`wayland-0`, no keyboard grab.** `examples/demo_switcher` (new): a `160x100`
`KeyboardMode::Exclusive` layer surface, centred, spawned only on a `map` stdin command so a
script can hold a modifier or toggle a lock key before the surface (and its
`wl_keyboard.enter`) exists at all; its document reads `use_modifiers()` and prints
`{"event":"modifiers",..}` on every render. `scripts/dev/accept-modifiers.sh` (new), two cases,
each its own kwin-virtual.sh instance so neither's modifier state leaks into the other. Both
PASS:

- `super`: pointer over the panel's future position, `key Super_L down`, then `map`. Wire order
  (`WAYLAND_DEBUG=1`): `wl_pointer#19.enter` immediately followed by `wl_keyboard#22.modifiers(7,
  64, 0, 0, 0)` (F61's pointer-tied path — the cache is already correct before `enter` even
  runs), then `wl_keyboard#22.enter(8, wl_surface#26, ..)`, then `wl_keyboard#22.modifiers(7, 64,
  0, 0, 0)` again (the protocol's guaranteed one, unchanged, `Live::publish` a no-op). The
  panel's line 1 (its first render, pre-configure): `"logo":"Released"` (the documented
  default). Line 2 (published from `enter`): `"logo":"Held"`. `key Super_L up` while the panel
  is still mapped and focused (no key event on the panel itself, a bare modifiers change): line
  3, `"logo":"Released"`.
- `caps_lock`: `key Caps_Lock down` then `up` (a tap, toggling the lock) before `map`. Line 1:
  the default (`"caps_lock":"Unlocked"`). Line 2, from the same enter/modifiers pair as above:
  `"caps_lock":"Locked"` — the initial Caps Lock state, seen before any key ever reaches the
  surface.

`cargo test --workspace --all-features`: 328 (326 on master + 2 in `modifiers::tests`, table-driven,
covering every field and the Caps-Lock-is-not-a-held-modifier distinction the two enums exist
for). `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D
warnings`, `./scripts/check-boundary.sh` and `cargo deny check licenses` all clean.

**Not proven live:** a real compositor other than KWin (cosmic-comp's smithay backend is
expected, per F61, to send modifiers only to the keyboard focus — the simpler case, not
exercised here); Num Lock (same code path as Caps Lock, `LockKey::of`, table-tested only); a
second seat.

## The keyboard's own withdrawal (branch `keyboard-withdraw`, 2026-09-26)

### F67. G210: an unmapped or closed surface's keyboard focus was never withdrawn; the next opening started from the last one's modifiers

Found by sill (its G210), the keyboard's half of F58. `ctx/ops.rs`'s `unmap` rebuilds the
`wl_surface` and `close` destroys it; either way the compositor's later `wl_keyboard.leave`
names a surface `surface_id_for` cannot match (the same shape as F58's pointer leave), so
nothing ever told the document its keyboard focus had gone. `handlers/keyboard.rs::leave`'s own
reset (`keyboard_focus = None`, stop repeat, `live_modifiers.publish(default)`,
`HostEvent::KeyboardFocus{Lost}`) never ran, and `self.input.keyboard_focus` stayed pointed at
the surface — a lock surface and the exclusive overlay a switcher or a power menu opens are
built and torn down the same way (map/unmap or lock/unlock), so all three carry the bug. The
lock path (`ctx/lock.rs::remove_surface`) already reset `keyboard_focus` by hand at a lock
surface's own removal, but never reset `live_modifiers`, so a lock surface was one call short
of the same fix in miniature.

**Fix.** `input/focus.rs` gained `depart_keyboard(focus: Option<SurfaceId>, surface: SurfaceId)
-> (Option<SurfaceId>, Departure)`, the keyboard's analogue of `depart` for the pointer: no
serial to keep, so the record is a bare `Option<SurfaceId>` rather than its own type, sharing
`Departure::{Lost, Unaffected}` with the pointer's record. `handlers/keyboard.rs` gained
`HostState::withdraw_keyboard(id)`, `withdraw_pointer`'s shape exactly: `depart_keyboard` the
record, and on `Lost` run the same effects `leave` already ran (`reset_keyboard_focus`, factored
out so both share it) — forget `focused_at`, stop repeat, publish `Modifiers::default()` to the
surface (a no-op if it already read that), push `HostEvent::KeyboardFocus{Lost}` once. `leave`
itself now goes through `depart_keyboard`/`reset_keyboard_focus` too (previously it reset
unconditionally on any matched id; now, like the pointer's `depart`, only when the departure
actually took the focus), and a `leave` for a surface `surface_id_for` cannot match — the late
one this fix pre-empts — is ignored with a debug-only print rather than silently, so a run with
`cfg(debug_assertions)` shows it happened.

`withdraw_keyboard` is called everywhere `withdraw_pointer` already was, before the surface's
old `wl_surface` goes: `ctx/ops.rs`'s `unmap` and `close`, `handlers/mod.rs`'s
`close_surface_and_popups` (the compositor's own `closed`/`request_close`/`done`), and
`ctx/lock.rs`'s `remove_surface`, whose hand-rolled reset is now the same call (fixing its
missing `live_modifiers` reset as part of the same change, not a separate one). No new
`HostEvent` variant and no API change, as asked: `Focus::Lost` already existed for `leave`.

**Proof.** `input/focus.rs::tests::keyboard_departures`, table-driven: the surface holding focus
departing is `Lost`; a different surface or `None` departing is `Unaffected` (unchanged) —
`cargo test --workspace --all-features` is 329 (328 on master + this one).

**Live, on the headless KWin (F52, never `wayland-0`, no keyboard grab).**
`examples/demo_switcher` changed shape: `unmap`/`map` now drive `HostCtx::set_visibility` on
the *same* `SurfaceId` (`Visibility::Hidden`/`Shown`) instead of closing and respawning, so a
script exercises the real unmap-and-rebuild path rather than a fresh surface; it also prints
`{"event":"focus","state":"Gained"|"Lost"}` for its own `HostEvent::KeyboardFocus`.
`scripts/dev/accept-modifiers.sh` gained a third case, `restale`: `key Super_L down`, `map`
(line 2 confirms `logo` held on enter, as `super` already covers), `unmap` while Super is still
held, `key Super_L up` while hidden (a hidden surface has no keyboard focus to carry the key
event to), `map` again with nothing held.

All three cases PASS on this branch:

```
PASS restale_seen_on_enter: line 2: {...,"logo":"Held",...}
PASS restale_after_remap: after unmap (Super still held), release while hidden, and remap (nothing held): {...,"logo":"Released",...} (3 lines total)
PASS restale_lost_between_openings: focus events: Gained,Lost,Gained,
```

**The `modifiers` value alone does not discriminate the fix, and saying so up front is the
point of this note.** `restale_after_remap` reads `logo: Released` on both sides of this
change: the compositor sends the guaranteed `modifiers` event right after the second `enter` in
the same dispatch as the enter itself, before the document's `use_signal` task is ever polled
(FINDINGS F66's "one event late"), so any momentarily-stale value `enter` published from a
still-wrong `held_modifiers` cache is overwritten before it is ever rendered — this held on a
build with the fix reverted from the five changed `src/` files too (checked directly, below).
Reverting just those five files (keeping this branch's `demo_switcher` and script) and rerunning
gave the same `restale_after_remap` PASS, `logo: Released` unchanged — a test that cannot fail
proves nothing (CONVENTIONS, "an assertion that was already true proves nothing"). The
discriminating signal is `restale_lost_between_openings`: on the reverted build it read
`focus events: Gained,Gained,` — **FAIL**, no `Lost` between the two openings, exactly the bug
G210 names — and on this branch it reads `Gained,Lost,Gained,` — **PASS**. Restoring the five
files (from a WIP commit taken before the revert, `git checkout <wip> -- <files>`) brought the
rest of the suite back to green alongside it. Reported honestly: the fix is proven by the focus
event, not by the modifiers value, which was already correct before the fix existed in this
single-surface demo (nothing else here ever holds the keyboard for the stale record to
misdirect a live modifiers change onto) — a shell with more than one keyboard-interactive
surface at once is where a stale record would misdirect an in-flight modifiers change onto a
surface that no longer holds the keyboard, not exercised here.

`cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D
warnings`, `./scripts/check-boundary.sh` and `cargo deny check licenses` all clean.

**Not proven live:** a second keyboard-interactive surface receiving the modifiers change a
stale record would have misdirected (the mechanism above); a real compositor other than KWin;
the lock and power-menu surfaces this also fixes, run through their own acceptance rather than
`demo_switcher`'s.

## Fonts (branch `host-fonts`, 2026-09-27)

### F68. Every document fell back to `system-ui`: `SharedFonts::register` had no caller, and `HostConfig` had nowhere to carry a shell's faces

Root cause of sill Q342 (quire FINDINGS "Colour emoji"; the gate branch's
`crates/ds-native/tests/text_stack_fonts.rs` pins it from quire's side): `HostState::bind` built
`fonts: SharedFonts::system()` — fontconfig plus Blitz's bullet glyph — and nothing ever called
`SharedFonts::register`. Inter is not a system font here, so a stack naming it
(`"Inter","Noto Color Emoji",system-ui,sans-serif`) laid out as if Inter were never named at
all: the first face the context actually had was Noto Color Emoji, whose cmap covers U+0020 and
0-9 (the keycap bases), so ordinary spaces and digits came from the emoji face, wide, in every
document sill ever drew. `HostConfig` had no field to carry a shell's design-system bytes in the
first place, so there was no way to close this short of a shell depending on shell-host's font
internals directly.

**Fix.** `HostConfig::fonts: FontFaces` (`config.rs`), a `Vec<&'static [u8]>` of face bytes,
`#[serde(skip)]` exactly like `prepaint` (code, not settings — a face is compiled into the shell
binary). `Host::connect` registers each one into `state.fonts` right after `bind` (`host.rs`),
so `state.fonts.for_document()` (`ctx/spawn.rs:375`, the one real call site) hands every surface,
popup and toplevel document a `FontContext` carrying them from that point on.
`SharedFonts::register` now takes `&'static [u8]` (no allocation to register a compiled-in face)
and returns the family name(s) it registered under, read back from `fontique::Collection` itself
rather than trusted from the caller — `dom/fonts.rs`.

**The three other `SharedFonts::system()` construction sites named in the brief**
(`ctx/spawn.rs:497`, `scroll/paint.rs:109`, `scroll/watch.rs:188`) are all inside `#[cfg(test)]
mod tests` — `test_env`, `pixel`'s harness and `use_scroll_follows_what_the_host_publishes`'s
env respectively — not real paths a document ever ships through. Left as they are: they test
scroll/paint/handle plumbing that does not depend on which faces are registered, and a real
host never reaches them.

**Pure test hook.** `shell_host::registered_families(&HostConfig) -> Vec<String>` builds a
throwaway `SharedFonts::system()`, registers `config.fonts` into it, and returns the family
names `fontique` actually reports back — no Wayland connection, so a shell can assert its own
faces survive a dependency bump (e.g. that `"Inter"` is still present) as a plain `#[test]`.

**Headless.** `headless::Fonts::Bundled` went from a unit variant that searched
`~/quire/crates/ds/assets/fonts` (or `/usr/share/fonts`) for a single arbitrary TTF — in
practice always `bricolage-grotesque-normal-500-800-latin-ext.ttf`, alphabetically first, not
representative of anything a real document ships with — to `Bundled(FontFaces)`, the same type
`HostConfig::fonts` carries, so a snapshot test uses the exact faces a real host would. shell-host's
own examples/tests (`spike_widget.rs`, `tests/body_margin.rs`, `tests/headless.rs`) have no
design system of their own to point at, so they keep the old discovery through a new
`headless::bundled_faces()` (same file, now leaked to `'static` and wrapped in `FontFaces`); the
committed snapshot goldens (`tests/snapshots/drift_t0*.png`) are unchanged byte-for-byte —
`cargo test -p shell-host --test headless --features headless` passes without
`UPDATE_SNAPSHOTS`.

**Test.** `tests/host_fonts.rs` reads Inter's real bytes from `~/quire/crates/ds/assets/fonts`
(skipped, not failed, if this checkout has no quire beside it — a file read, never a crate
dependency, so `scripts/check-boundary.sh` stays clean), registers them through
`HostConfig::fonts`, and asserts `registered_families` reports `"Inter"` and that a document
naming `"Inter"` measures differently from one naming only `system-ui` once it is registered
(and skips, with a message, if Inter happens to already be a system font on the machine running
the test — the same guard quire's own `text_stack_fonts.rs` uses).

**What a shell must change.** sill builds its `HostConfig` and must add one field:
`fonts: FontFaces(ds::FACES.iter().map(|face| face.bytes).collect())` (quire
`crates/ds/src/fonts.rs`'s `Face::bytes: &'static [u8]`). Not done here — sill is a separate
repo and the brief for this branch was shell-host only.

`cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` (and again
with `--features shell-host/headless`), `cargo test --workspace` (and again with
`--features shell-host/headless`), and `./scripts/check-boundary.sh` all clean.

## Hoisted boxes (branch `g290`, 2026-09-27)

### F69. Z-indexed boxes painted at the previous layout's position: Blitz gathers them before it lays out

Root cause of sill G290. With all five widgets in the widget centre, the frame on screen drew
Now Playing's and World Clock's title and body over the Calendar tile and left tiles 4 and 5
as empty plates, while `sill debug capture` of the same surface drew every tile in place.

**Cause (Blitz, e99fbdbd).** `BaseDocument::resolve` runs `flush_styles_to_layout`
(`blitz-dom/src/resolve.rs:111`) *before* `resolve_layout` (`:115`). That walk moves every
child with a non-zero `z-index` (positioned, or a flex/grid item) out of its parent's paint
children into its stacking context's `HoistedPaintChildren`, and records its offset as the sum
of the intermediate ancestors' `final_layout().location` minus their scroll offsets
(`layout/damage.rs:698-705`). Those locations are the *previous* layout's. The paint
(`blitz-paint/src/render.rs:990-1025`) and the hit test (`node/node.rs:1377-1450`) both use the
recorded offset. So whenever a z-indexed box's ancestors move in a resolve, that resolve paints
it where they were: a node inserted since the last layout still has location (0, 0), so its
raised children paint at the stacking context's origin. quire's `.ds-widget-title` and
`.ds-widget-body` are `position: relative; z-index: var(--z-raise)`, so the bodies of tiles
added to the widget centre painted at the column's top, over the first tile.

**Why only on screen.** The host resolves exactly once per rendered frame (`ctx/paint.rs`), then
the surface goes idle, so the stale hoist stays on screen indefinitely. A debug capture
(`Presenter::capture`) resolves again before painting; that second resolve reads the first one's
layout and draws everything in place. The two presenters paint with identical code
(`gpu/render.rs::paint_document`); scale, scene reuse, clip/layer order and damage were not
involved. The same lag is also why a document's *first* frame (every location still zero) put
all its raised boxes at the origin, and why the headless `render_png` did the same.

**Fix (workaround in shell-host, no Blitz change).** `dom/stacking.rs::rehoist` calls Blitz's
public `BaseDocument::flush_styles_to_layout(<html>)` again after the resolve, so the hoisted
offsets come from the layout just computed. The walk rebuilds only paint order and the hoisted
lists (no style, no taffy input, no damage), so repeating it is idempotent. `SurfaceDocument::
resolve` runs it after Blitz's resolve (before the hover repair, whose hit test reads the same
lists) and again after the pre-paint hook when one ran (snapping may move boxes);
`headless::render_frame` runs it after its resolve. The scroll offsets it reads are the ones
the engine wrote before the resolve (F63), unchanged. Remaining lag: Blitz's own
`refresh_hover` inside `resolve` still hit-tests the stale lists; the host's hover repair
(F59) runs after the re-hoist and corrects a hover that lands wrong.

**Minimal repro** (`tests/hoisted.rs`, no Wayland, no GPU): a flex column of 60 px tiles, each
holding a `position: relative; z-index: 1` body 20 px down; lay out one tile, grow to three,
resolve once, paint on the CPU. Before the fix tiles 1 and 2 showed their tile colour where
the body belongs, a second resolve (the capture path) differed from the first in 4000 pixels
(two 100 x 20 bodies), and `element_from_point` over tile 2's body hit the tile. All three are
assertions now. `tests/headless.rs::the_first_frame_paints_raised_boxes_where_layout_put_them`
covers the first-resolve case through `render_png`; it fails without the re-hoist.

**Upstream.** The Blitz fix is to gather the hoisted children after `resolve_layout` (or to
record ancestor chains and sum locations at paint time). Not filed from here; once a Blitz rev
with it is pinned, `rehoist` can go.

**Public API.** None changed: `dom::stacking` is `pub(crate)`, and `SurfaceDocument::resolve`
and `render_png` keep their signatures. Not checked on screen from this branch (sill reads
shell-host master by path); the sill widget centre should be re-shot after this lands.

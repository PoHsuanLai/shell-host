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
scrolls. DESIGN.md lists where they will go.

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

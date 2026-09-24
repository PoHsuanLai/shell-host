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

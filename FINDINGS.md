# Findings

Standing facts about Blitz, the compositors and the protocols that the code relies on, and the
items still open. Numbered so `DESIGN.md`, the README and reviews can cite them; numbers are
never reused, so gaps are entries that were closed and removed.

## Rendering and the GPU

### F1. One renderer across surfaces; the image cache is per device

One `vello_hybrid::Renderer` + `Resources` serves surfaces of different sizes: `render()`
rewrites its view-config buffer when `RenderSize` changes, and a second surface costs about
1.8 MB, so per-surface devices are unnecessary. The image cache must live per device, across
frames: `anyrender_vello_hybrid::ImageManager::new` borrows an `FxHashMap<u64,
vello_common::paint::ImageId>` and `VelloHybridScenePainter::new` an `FxHashMap<ResourceId,
TextureView>`, and nothing re-exports `rustc_hash` or `ImageId`, so both crates are named in the
pinned block (`gpu/context.rs`).

### F16. Adapter choice is validated by a first present, on a throwaway connection

On this machine (NVIDIA RTX 5070 Ti + AMD iGPU) the AMD adapter (RADV) creates a device, but KWin
kills the connection on its first present: `zwp_linux_dmabuf` error 7, "importing the supplied
dmabufs failed" (cross-GPU buffer). The GL backend reports "gl not compatible with provided
surface" even with the display handle in the `InstanceDescriptor`. `is_surface_supported` says
yes to all of them, so the choice is validated by presenting.

A protocol error kills the connection it happens on, so `gpu::adapter::validate` presents one
transparent 1x1 frame on a second connection, round-trips, and checks that connection's
`protocol_error()`; the host's own connection never sees a rejected dmabuf.

- wgpu 29 binds an `Instance` to one `wl_display` and refuses surfaces from another, so the probe
  opens its own instance on the throwaway connection and presents through the adapter whose
  `AdapterInfo` equals the candidate's; `validate`'s `instance` parameter is unused.
- NVIDIA 615.71: dropping a Vulkan instance that presented makes the next
  `vkGetPhysicalDeviceSurfaceSupportKHR` on the host's instance jump to a null pointer inside
  libnvidia-glcore (gdb-confirmed; not after RADV or llvmpipe probes). `gpu/probe.rs` keeps each
  probe instance and connection alive for the process, one per validated candidate.
- Results here: NVIDIA Vulkan passes (~300 ms), surface caps present `[Mailbox, Fifo,
  Immediate]`, alpha `[Opaque, PreMultiplied]`, so PreMultiplied + Mailbox; RADV is rejected on
  the first present (error 7 or connection reset); llvmpipe passes but ranks last; NVIDIA GL does
  not support the surface.

### F75. Presenting: ordering, the redraw flag, drop order, protocol errors

- **The present commits for us, so the frame request goes first**: `wl_surface.frame()`, then
  `queue.submit`, then `SurfaceTexture::present()`; the WSI's commit carries the frame request.
  Every present is exactly one commit.
- **The redraw flag is cleared after presenting.** `resolve()` can call
  `ShellProvider::request_redraw` itself; without clearing after the present the machine never
  idles. Continuation is decided on the callback by `InFlightDirty || is_animating()`. A request
  from another thread is not the render's own and survives the present (F40).
- **`wgpu::Surface` drops before its `wl_surface`**: field order in `HostSurface` (wgpu first),
  plus a `Drop` for the state that clears surfaces and flushes, which also covers panics.
- **Stop on protocol errors.** After a fatal protocol error libwayland fails every call; logging
  each failure filled a pipe until the process blocked in `write(2)`. The loop checks
  `conn.protocol_error()` after each dispatch.
- **Idle is clean on the main thread, not in the process.** With no frame and no commit
  (calloop asleep in `epoll_wait`) the main thread has 0 voluntary switches, but the NVIDIA
  Vulkan driver runs its own threads: an unnamed one ~100 wakeups/s and `[vkps] Update` ~4/s,
  the same in Mailbox, Fifo and Immediate. Idle acceptance measures the main thread
  (`/proc/<pid>/status`). Not yet measured on AMD or Intel.

### F76. Render acceptance: the items and their thresholds

`scripts/accept-demo.sh demo [<compositor>] <item>` against `examples/demo_bar.rs`,
`demo_dock.rs` (feature `debug-inject`) and the plain sctk client `probe_toplevel` (a maximized
checkerboard toplevel: the items need an ordinary other-client window). Compositors:
`kwin-virtual[@scale]` (a nested headless KWin on its own D-Bus session, default scale 1.5),
`kwin` (the live session), `cosmic[@scale]` (`scripts/dev/nested.sh`).

| Item | Passes when |
| --- | --- |
| a bar | a maximized probe is 32 px shorter with the bar than without (the delta, not `output_h - 32`, so a panel on a live session does not matter); the bar pixel is the bar's colour, the checker is visible below it |
| b blur | luminance variance of the checkerboard behind the bar drops by > 99 % with blur on (`DEMO_BLUR=1` vs `0`); below the bar the checker keeps its variance |
| c dock | a hover sweep renders once per frame callback, work p95 well under the 16.7 ms budget, in the transform, width and tick modes; 5 s idle after the leave: 0 frames, 0 commits, 0 main-thread wakeups; covered by a toplevel (bottom layer) `stats` stays prompt and rendering resumes after |
| f fractional | at 1.5 a 2-logical-px checker is exactly 3 device px, `buffer = round(logical x scale)` (round half away from zero), and a screenshot row equals the capture row; at 1.7 runs are 3 or 4 px, mean 3.4 |
| g two surfaces | bar + dock in one process: `devices_created` 1, one adapter, a dock sweep leaves the bar's frame count alone |

Facts from the runs:

- kwin-virtual passes every item (work p95 2-4 ms, second surface +1.6 MB RSS). The demos have no
  present-mode knob (the host prefers Mailbox), so item c's "fifo" run is a second Mailbox run.
- KWin keeps sending frame callbacks to an occluded surface; with the display in DPMS off it
  sends none at all and screenshots are black: the watchdog then carries each surface (100 →
  400 ms backoff) and `stats` stays prompt. cosmic-comp may withhold callbacks from an occluded
  surface (cosmic-comp #2777), which is what the watchdog is for.
- Live KWin at scale 2.0 passes a, b, c, g. Open: item f's screenshot-row check failed there
  because the screenshot was taken before the bar was mapped; the item needs to wait for the
  first presented frame on a live compositor before its capture means anything.
- Without the fractional-scale global the host renders at the integer `buffer_scale` (2 at 1.5)
  and the compositor downsamples.

### F12. The UA stylesheet: a sized root

Hit-testing needs a sized root: with `html,body` at their default zero height, or content
`position:absolute` directly under them, Blitz's `set_hover_to` never reached the content.
`dom::document::UA_STYLESHEET` is `html,body{height:100%;background:transparent}`; in-flow
content under it hit-tests (`tests/pointer.rs`). Margin and padding are left to quire's
`css/reset.css`, except the body margin (F62).

### F62. Blitz's default `body { margin: 8px }` is reset by the host

Blitz's user-agent sheet (`blitz_dom::DEFAULT_CSS`) has the browser's `body { margin: 8px }`. A
document without quire's reset laid out from (8, 8), so an `inset:0` box never reached the
surface's corner. Every document the host builds (`SurfaceDocument::build`,
`headless::render_png`) gets `BODY_RESET` (`body{margin:0}`) as a second user-agent sheet through
`BaseDocument::add_user_agent_stylesheet`; `DocumentConfig::ua_stylesheets` would *replace*
`DEFAULT_CSS`. As a user-agent sheet it loses to any author rule, so a document's own body margin
still decides (`tests/body_margin.rs`).

### F41. dioxus-native-dom's `#main` is `height:auto`

Every `VirtualDom` renders into `<main id="main">`, which has `height:auto` and no flow content
of its own. A `height:100%` root under it is 0 px tall, and an absolute or fixed `inset:0` box
(placed against `#main`) spans the width but is 0 px tall. A bare document that must cover its
surface sizes its box in viewport units (`width:100vw; height:100vh`), which lands exactly on
the surface; quire's `RootExtent::Viewport` does that for `Ds` roots. The host does not size
`#main`: a percentage height on a content root would then resolve against the surface and change
content layouts and `PopupSize::FitContent`'s measurement. A lock document paints its whole
surface with `html { background }` for the same reason (F65).

### F43. The scale reaches the document as live state; a pre-paint hook runs after the frame's one resolve

`use_surface_scale() -> ReadSignal<Scale>` and `SurfaceHandle::current_scale()`: 120ths, from
`wp_fractional_scale_v1.preferred_scale`, else `preferred_buffer_scale` / the output's integer
scale. A surface starts at `Scale::ONE`, a popup at its parent's. `HostConfig::prepaint`
(`Prepaint::Hook`) is per host, not per document, so a popup opened from a component gets it
too; sill passes quire's device-pixel snapping.

- **One resolve per frame.** Blitz's `resolve_layout` runs `taffy::round_layout` on every resolve,
  so a snap after one resolve is undone by the next. `SurfaceDocument::resolve` is the frame's
  only resolve (then the hook), and the presenters paint what it left, so regions,
  `FitContent`, hit testing and pixels read one snapped layout. The debug `capture` resolves
  again (with the hook) because it is its own render.
- **Scale before the poll.** `ctx::rescale::follow_all` runs at the start of phase 1 and
  publishes a changed scale, so the same flush's poll re-renders the documents that read it
  before the frame is painted. On cosmic-comp `preferred_scale` arrives in the same dispatch as
  the first configure, so the first frame's tree has it.

### F69. Blitz gathers z-indexed boxes before it lays out

`BaseDocument::resolve` runs `flush_styles_to_layout` before `resolve_layout`. That walk moves
every child with a non-zero `z-index` into its stacking context's hoisted children and records
its offset from its ancestors' *previous* layout; paint and hit test use the recorded offset. So
a resolve that moves a z-indexed box's ancestors paints it where they were (a new node's raised
children paint at the stacking context's origin, as does every raised box on a document's first
frame). The host resolves once per frame and then idles, so the stale position would stay on
screen.

`dom/stacking.rs::rehoist` calls Blitz's public `flush_styles_to_layout(<html>)` again after the
resolve (and after the pre-paint hook when one ran); the walk only rebuilds paint order and the
hoisted lists, so repeating it is idempotent. Blitz's own `refresh_hover` inside `resolve` still
hit-tests the stale lists; the hover repair (F59) runs after the re-hoist. `tests/hoisted.rs`,
`tests/headless.rs`. Upstream fix: gather hoisted children after `resolve_layout`; once a Blitz
rev with it is pinned, `rehoist` can go. Not filed.

### F77. Blitz and sctk API facts

- `blitz_paint::paint_scene` drives custom widgets only when built with
  `blitz-paint/custom-widget`; without it a widget silently never paints.
- A `blitz_dom::Widget`'s texture registration (`try_register_custom_resource`) needs a
  `RenderContext` whose `renderer_specific_context()` is a `wgpu_context::DeviceHandle`. The CPU
  renderer (`render_png`, the shm presenter) returns `None`, so a GPU widget is transparent
  there; `tests/gpu.rs` covers the GPU path.
- `peniko` must be named directly to build `Paint::Resource` / `ImageBrush`: nothing in blitz or
  anyrender re-exports it.
- `system_fonts` is a construction-time choice of the `fontique::Collection`
  (`cfg!(feature = "system-fonts")` inside `BaseDocument`); a reproducible render supplies
  `DocumentConfig.font_ctx` itself (`headless::Fonts::Bundled`).
- A CSS `transition` starts only when a later restyle sees a changed computed value, so a single
  headless render cannot show one mid-flight; snapshot tests use `@keyframes`, whose computed
  style is a function of the resolve's time.
- sctk 0.20 has no `Dispatch` helpers for `ext-background-effect-v1`, `wp-fractional-scale-v1` or
  `WpViewport`; `handlers/{background_effect,fractional}.rs` hand-write them in sctk's own
  indirect-`Dispatch` shape. `delegate_xdg_shell!` also delegates the decoration manager, which
  requires `WindowHandler` on the state.
- `xdg_popup.popup_done` cannot be caused by a client: nothing lets a client synthesize the
  outside click the compositor detects. Scripted runs without real input close popups by
  hit-testing; `CloseReason::PopupDismissed` needs real input (fake input on the headless KWin,
  F52).

## Frames and documents

### F9. `HostState` is not generic; timers are requested as data

The sctk handler traits are implemented on a non-generic `HostState`; the calloop loop's data is
`HostCtx<M>` (it owns `HostState` plus the app-message queue). Handlers cannot touch the loop,
so they push `TimerRequest`s (frame watchdog, key repeat, scroll wake) that the flush arms or
cancels. sctk's own key-repeat source (`get_keyboard_with_repeat`) cannot be used, because its
`LoopHandle<'static, T>` must name the loop's data type while `delegate_keyboard!` names
`HostState`; repeat is a context timer driven by `wl_keyboard.repeat_info`. Handler events are
`HostEvent<Infallible>`, widened to `HostEvent<M>` on delivery (`event::widen`).

### F31. The frame hook: `drive_frames`, `use_frame_tick`

`SurfaceHandle::drive_frames() -> FrameDrive`: while any drive is alive the frame machine is told
`Animation::Running`, so the surface renders once per callback even with nothing changed; taking
one queues a redraw (an idle surface has no callback coming). Dropping it lets the frame in
flight call back once more and the surface idle. Holding a drive with no animation is the one way
to get non-zero idle frames.

`use_frame_tick()` / `SurfaceHandle::next_frame()` give `FrameTick { at: HostInstant, dt }` once
per presented frame while the document animates or a drive is held; a watchdog expiry standing
in for a missing callback ticks too. `dt` is zero on the first tick of a run.

### F36. A drive's drop condition must be reachable at any tick rate

The nested cosmic-comp calls back as fast as it composites: ticks 1-2 ms apart, where KWin gives
16.7 ms. A motion step that rounds `gap * dt / period` to whole units stops short of its target
at 1 ms, so the drive is never dropped and the surface renders and commits every callback. A
step closes `1 - exp(-dt / tau)` of the gap and at least one unit, never past the target; the
settle is tested at 1 ms as well as 16.7 ms (`demo_dock`'s
`tests::the_tiles_settle_at_any_tick_rate`). `FrameDrive`'s doc carries the rule.

### F33. A document is polled until it has nothing to do

`DioxusDocument::poll` returns as soon as the `VirtualDom` has dirty work, rendering it, and the
`VirtualDom` keeps the poll's waker only when a poll finds nothing to do. Polling once per wake
therefore loses the next wake of any Dioxus task after a render. `SurfaceDocument::poll` polls
until a poll finds nothing (at most 8 renders, then it wakes itself so the next flush continues).

### F40. A resource load that lands mid-render is not the render's own redraw

Blitz's resource handler runs on the provider's thread: it sends the decoded image to the
document's channel and calls `request_redraw`. The channel is drained only by `resolve`. A
background image's fetch starts *inside* `resolve`, after that drain, so a small image can
decode and respond within the same render; clearing the redraw flag after the present would drop
its request with the render's own, and the surface would stay on the old frame for good. An
`img`'s fetch starts at the mutation and loses the race only when its decode takes as long as the
render.

`HostShellProvider` records where each request came from (`Origin::of`): the host's thread sets
one flag, any other thread another. After presenting, `drop_echo` clears only the host-thread
flag; a load that landed mid-render is taken by the next flush as `FrameInput::Invalidate` onto
the frame in flight (`InFlight → InFlightDirty`, watchdog armed). A debug `capture` resolves and
so drains the channel: a capture is not evidence of what is on screen.

### F15. `HostCtx::stats` is `&self` and does not drain

`stats(&self, SurfaceId)` reports percentiles over the samples since the last drain
(`StatsWindow::peek`); `take_stats` is it plus a new window, and the debug `stats` command takes,
so each printed line covers the time since the previous one. `FrameStats` carries frames,
callbacks, commits, skipped, work time (trigger → present) apart from the interval (whose p95 is
just the refresh period), scale, sizes, alpha and present mode, adapter and `devices_created`.

### F21. `LocalNet` serves `file:` and `data:`, nothing from the network

`data:` is decoded and `file:` read with `std::fs::read`, both on the host runtime's blocking
pool. Only absolute paths from a `file:` URL with an empty or `localhost` host are served
(`LocalSource::of`); anything else, and a failed read, stays unanswered. Blitz's handler requests
a redraw on completion, so the image appears on the next frame (F40).

## Surfaces, popups and regions

### F2. `HostEvent::Started`, `HostCtx::exit`, `HostHandle`'s methods

No output event exists for a process that does not key surfaces on outputs (a toplevel app), so
`HostEvent::Started` is delivered exactly once, first, after the initial outputs.
`HostCtx::exit` returns from `run` inside the loop; `HostHandle<M>`'s `send`, `apply`, `close`,
`exit` fail with `HostError::HostGone` (`Recovery::Degrade`) after the host exits.

### F4. `PopupPlacement` carries the anchor rectangle and the size

`PopupAnchor::Rect` carries no rectangle, and `positioner_params` takes the anchor rect and size
separately, so `open_popup(config, placement, root)` takes a `PopupPlacement { anchor_rect, size
}`. The same value drives `SurfaceOp::Reposition` (`xdg_popup.reposition`, the hover switch
between menus in design/13 §13.7).

### F30. `PopupConfig::offset`

`offset: PopupOffset { dx, dy }` (logical px) translates the anchor rectangle
(`popup::offset_rect`), so the offset follows `PopupAnchor::Element`, `Reposition` and flips with
no new wire code. A rectangle moved past the parent's origin is sent as is (xdg-shell has no
bounds error for the anchor rect).

### F38. `PopupSize::FitContent` measures the popup's own document

`SurfaceDocument::content_size` measures `#main`'s first child that is not
`style`/`script`/`title`/`link`/`meta`/`base`/`template` (a document's own `<style>` renders as
`#main`'s first child). Neither `<body>` nor `#main` can be measured: both stretch to the
surface. `popup::refit` (pure) returns a new placement only for `FitContent` and only when the
clamped content size differs from the current one, so a settled popup never repositions.
`ctx::paint::render` calls it right after the frame's resolve and applies it inline
(`ctx::ops::refit_popup`, the same record-then-`reposition` pair `SurfaceOp::Reposition` sends);
the compositor's configure then resizes the surface. The caller's initial `PopupPlacement.size`
is the placeholder until the first measurement.

### F72. On KWin a popup never lifts its `KeyboardMode::None` parent

KWin's `LayerShellV1Window::handleAcceptsFocusChanged` *activates* a Top/Overlay layer whose
interactivity becomes non-None. Lifting a bar to `OnDemand` for its grabbing popup therefore made
the bar KWin's active window; dropping it back to `None` deactivates nothing, and when the popup
closed KWin returned the keyboard to the bar (which takes no keys) instead of the app the user
was typing in. The activation also fires `Workspace::windowActivated`, on which KWin cancels
every open grabbing popup. KWin does not need the lift: `PopupInputFilter::focus` hands the
keyboard to the topmost grabbing popup itself, and `xdg_popup_grab` never checks the serial.

`popup_keyboard::parent_keyboard` keeps the parent as it is on KWin (`CompositorKind::Kwin`,
recognised by `org_kde_plasma_shell`; wlroots offers some `org_kde_kwin_*` globals but never
that one) and lifts elsewhere (cosmic-comp needs the parent to take keyboard input). The restore
after `popup_done` commits the layer. KWin sends no `popup_done` for a press on another surface
of the popup's own client, so dismissing on a second bar item is the shell's.
`scripts/dev/accept-popup-keyboard.sh`.

### F20. Regions are sent from the render only, and only when changed

Input and blur regions are resolved after layout on every rendered frame (`ctx::paint`) and sent
only when the strips differ from what the compositor holds (`resolve::Tracked`); a configure only
invalidates. The blur tracker starts as `holding(vec![])`, so no effect object is created just to
say "no blur". Only a rebuilt role (hide/show, a new `wl_surface`) resets the trackers.

### F55. `RegionSpec::Element` vs `RegionSpec::Elements`

`Element { id, radius, inflate }` follows one element; while it is missing from the document the
surface keeps its current region (`Resolved::Pending`), right for one moving part.
`Elements(Vec<ElementRegion>)` is the union of each element's rounded strips (overlaps are fine
for a `wl_region`), and a missing element contributes nothing, so one card not yet laid out, or
removed, never blanks the others. A region that should vanish with its element is `Elements`.

### F73. Element rectangles follow CSS transforms

`SurfaceDocument::element_rect` (regions, `PopupAnchor::Element`) carries the layout box through
every transform on its layout-parent chain, the chain `blitz-paint` composes; Blitz's
`get_client_bounding_rect` is the layout box, and layout never sees `transform`.
`blitz_kit::paint_rect::painted_bounds` (pure) maps the rect's corners through the chain; each transform
is Blitz's resolved `Node::transform()` in device px (`Affine2::from_device` divides the
translation by the scale), about the box's untransformed origin plus its own scroll offset.
Translation and scale are exact; rotation and skew give the axis-aligned bounding box. Not
handled: 3-D transforms (Blitz paints those untransformed too), an ancestor's `overflow` clip,
`opacity`. Regions move frame by frame because every rendered frame re-resolves them.
`tests/region_transform.rs`, `scripts/dev/accept-swipe.sh`.

### F10. `Anchor` is a hand-written edge set

`bitflags` is not in the pinned block; `layer::Anchor` is a four-bit newtype with
`TOP`/`BOTTOM`/`LEFT`/`RIGHT`/`ALL`/`NONE`, `|`, `union` and `contains`.

### F13. `LayerSize` is two `LayerExtent`s

The wire's "0 means stretch" is `LayerExtent::Fill` and a fixed size `LayerExtent::Px(u32)`, so a zero size
cannot be written by accident on an axis that is not meant to stretch.

### F11. Derives: serde only where a type is data all the way down

Nothing in shell-host is persisted, and the geometry types (`RegionSpec`, `PopupConfig`, `Scale`,
`Size`, `Rect`) have no serde, so no type embedding them derives it. Serde is on `HostConfig` (a
shell may read it from a file), the ids, and `FrameStats` with its parts (printed as JSON by
`debug-inject`). Types holding Wayland objects, wgpu objects or a `HostRoot` derive `Debug` only (by
hand where a field has no `Debug`).

### F34. Open: minimize rectangles need the host's connection

design/10 §10.3.10 sets each dock tile as its app's minimize target
(`zcosmic_toplevel_manager_v1.set_rectangle(handle, wl_surface, ..)`). The toplevel handle lives
on sill's connection and the dock's `wl_surface` on the host's; a protocol object cannot cross
connections, and the host may not name COSMIC protocols. Designed, not built: export the host's
connection and a surface on it,

```rust
pub struct WlSurfaceExport {
    pub connection: wayland_client::Connection,
    pub surface: wayland_client::protocol::wl_surface::WlSurface,
    pub generation: u32,
}
impl HostCtx<M> { pub fn wl_surface(&self, id: SurfaceId) -> Option<WlSurfaceExport>; }
impl SurfaceHandle { pub fn wl_surface(&self) -> Option<WlSurfaceExport>; }
```

valid until the surface is hidden or closed (a new `HostEvent::SurfaceMapped { id }` says when to
re-export). sill then makes its own `EventQueue` on that connection, binds the toplevel manager
there and calls `set_rectangle` with the exported surface.

### F35. Open: drag and drop into shell surfaces

`handlers/data_device.rs` follows the selection and our own drags only;
`wl_data_device.enter/motion/leave/drop` for someone else's drag are ignored, so file drops on
dock tiles and spring-loading (design/10 §10.3.8) have no event source. Designed, not built:

```rust
/// Markup: `data-drop-accept="text/uri-list text/plain"`, `data-drop-action="copy|move|ask"`;
/// the host answers `accept`/`set_actions` on every motion synchronously from the element under
/// the pointer (a round trip through a component would miss the motion's serial).
pub enum DropEvent {
    Enter { at: Point, mimes: Vec<String> },
    Motion { at: Point, over: Option<ElementId> },
    Leave,
    Drop { at: Point, over: Option<ElementId>, payload: DropPayload },
}
pub enum DropPayload { Uris(Vec<PathBuf>), Text(String), Refused }
pub fn use_drop_event() -> ReadSignal<Option<DropEvent>>;
```

The payload is read like the clipboard (F22) and `Drop` delivered when the read completes.
Spring-loading stays in sill's dock machine.

## Toplevels

### F46. The window API a client-decorated frame calls

On `SurfaceHandle`: `begin_move()` / `begin_resize(ResizeEdge)` (`xdg_toplevel.move`/`.resize`,
from the press's handler while the button is down, F49), `set_maximized(Maximize)`,
`set_minimized()` (no un-minimize request exists for a client), `set_fullscreen(Fullscreen)`
(compositor's choice of output), `current_toplevel_state()` and `use_toplevel_state()` (re-renders
only when the state changes, not on a size-only configure). The same ops are `SurfaceOp`s; they do
not repaint, since the configure that answers repaints. The document's own cursor (Blitz's hover
cursor) is shown through `wp_cursor_shape_v1`; `None` shows the default.

### F48. CSD geometry: the band is surface, the window geometry is the frame

A floating `Decorations::Client` toplevel's surface is its frame plus `ResizeMargin` on every
side (`csd.rs`, pure); `xdg_surface.set_window_geometry(m, m, frame_w, frame_h)` goes with every
configure, so the compositor measures, places, tiles and shadows the frame, and configure sizes
are the frame's. The document's viewport is the frame, painted at the band width rounded to
device px; pointer positions, regions and the IME rectangle are translated by the same amount, so
element rectangles stay window-geometry coordinates (what `xdg_positioner` anchors against). In
the band the host shows the resize cursors and a primary press calls `xdg_toplevel.resize`; the
document sees neither. The zone is re-decided only with no button held. The band exists only
while floating: maximized, fullscreen or any tiled edge give no band.

**cosmic-comp 1.8 configures a floating window as tiled on all four edges** (and keeps it after
unmaximize), so there the band is dropped and edge resizing is cosmic-comp's. On KWin the window
floats with no tiled state and the band is there. The rule is per configure.

### F49. The serial rule, and what a compositor grab does to the buttons

`xdg_toplevel.move`/`.resize` need the serial of a press that is still down: smithay (cosmic-comp)
starts the grab only if the pointer's current implicit grab began with that serial, KWin only for
its latest press; anything else is ignored with no error. `grab.rs` keeps, per seat, the pointer's
surface and the held press, and `grab_serial` refuses up front (`GrabRefused::{PointerElsewhere,
NoPress, PressedElsewhere, NotAToplevel, NoSeat}`), synchronously in the handle and again at apply.

Once the compositor takes the grab, the release goes to the compositor, so the press is dropped
when `move`/`resize` is sent, and buttons and press are dropped at every `enter` and `leave` (a
leave with a button held means a grab took the pointer).

### F50. Open: no compositor offers a client "Left half", "Right half" or "Centre"

xdg-shell's `tiled_*` states are compositor-to-client only. cosmic-comp 1.8 has no
`xdg_toplevel_tag_manager_v1`, and `zcosmic_toplevel_manager_v1` (v4) offers maximize, minimize,
fullscreen, sticky, workspace moves and `set_rectangle`, with no tile or placement capability.
KWin 6.7.5 offers `xdg_toplevel_tag_manager_v1` (a name for window rules, no placement) and
`org_kde_plasma_surface.set_position` (absolute, KDE-only, not in the pinned block); quick tile
is D-Bus/scripting only. The host implements maximize (the menu's Fill) and nothing for
Left/Right/Centre; quire's menu hides them until a compositor offers a request.

## Input

### F47. sctk announces a seat present at bind through `new_capability` only

sctk 0.20 calls `SeatHandler::new_seat` only for a seat announced *after* `SeatState::new`; a
seat that exists when the host binds goes straight to `new_capability`. The first capability
therefore adopts the seat (`HostState::adopt_seat`, also called from `new_seat`).

### F53. A re-announced capability reuses or releases its object

KWin re-announces the seat's capabilities whenever its input devices change (each
`org_kde_kwin_fake_input` client is a new device: `capabilities(3)`, `(7)` on connect, `(6)`,
`(2)` on disconnect), as a real seat does on unplug. sctk 0.20 turns each flip into
`remove_capability`/`new_capability`. A proxy dropped without `wl_pointer.release` stays live on
the compositor, and every later press then arrives once per pointer ever made.
`input::capability::capability_change` (pure) decides `New`, `Repeat`, `Removed`, `Absent`;
removal destroys the cursor-shape device and sends `release` (since `wl_seat` v3), and resets the
surface under the pointer (leave, buttons, grab record) or the keyboard (focus lost, repeat,
modifiers). The text input and data device belong to the seat and are reused.

### F57. The enter carries the position

A pointer that reaches a surface in one motion gets `wl_pointer.enter` and no motion (KWin sends
exactly that; so does a surface mapped under a still pointer). The enter therefore delivers a
move to its position, after focus and zone and before the cursor. cosmic-comp #2230: a layer
surface mapped under a still pointer there may get no enter at all; the press's own move still
covers a click, a hover waits for the first motion.

### F58. An unmapped or closed surface withdraws the pointer and keyboard first

`unmap` (a `SetVisibility(Hidden)`) rebuilds the `wl_surface` and `close` destroys it; the
compositor's later `leave` names a dead object (KWin sends `leave(serial, nil)`) that the host
cannot match, so nothing would tell the document. `HostState::withdraw_pointer(id)`
(`input::focus::PointerFocus`, pure `enter`/`depart`) does what a leave does when the id held the
pointer (buttons forgotten, `SurfaceDocument::pointer_left`), and `withdraw_keyboard(id)` what a
keyboard leave does (repeat stopped, empty modifiers published, `KeyboardFocus{Lost}` once).
`unmap`, `close`, `close_surface_and_popups` and the lock's `remove_surface` call both before the
old `wl_surface` goes; a late leave for the dead object is skipped. The pointer's leave is a move
to (-1, -1) through Blitz, because `clear_hover()` unhovers without dispatching `mouseleave`.

### F59. Blitz's `refresh_hover` moves the hover silently

Blitz ends every `resolve()` with `refresh_hover()`, which re-hit-tests the last pointer position
against the fresh layout and sets `hover_node_id` without dispatching anything (its own TODO says
so). The event driver diffs the next move against that already-moved hover, so an element that
arrives under a resting pointer is never entered and the one it covered never left, and the
`:hover` restyle it asks for would be dropped as the render's own redraw.

`SurfaceDocument` remembers the last real pointer move (`blitz_kit::hover::LastMove`, forgotten in
`clear_hover`). When the hovered element changed across a resolve, `blitz_kit::hover::decide` (pure)
restores the old hover without dispatch (probing the old element's box: centre, then corners
1 px inside) or clears it, and the remembered move is fed again so Blitz's driver dispatches
exactly the leave/enter diff. It runs after the resolve and the re-hoist, before the pre-paint
hook, and `ctx::paint` invalidates after the present when the hover moved. Limits: an old
element that cannot be restored makes the new chain's shared ancestors get a second
`pointerenter`; windows run through `dioxus-native`'s own shell are not covered.
`blitz-kit`'s `tests/hover_sync.rs`, `scripts/dev/accept-hover.sh`.

### F14. Wheel pixels are logical

`input::pointer::wheel_delta` (the raw event a `data-wheel="capture"` element receives) passes
touchpad deltas as logical pixels, negated (Wayland's axis sign is the inverse of the DOM's).
Blitz treats `BlitzWheelDelta::Pixels` as CSS pixels.

### F61. Modifier state over a keyboard-less layer surface

`wl_keyboard.modifiers` follows keyboard focus by protocol. KWin also sends the current
modifiers to the client with *pointer* focus when the pointer enters, and each change while it
stays, with no keyboard focus; smithay (cosmic-comp) is expected to send them only to the
keyboard focus (not verified). On KWin they arrive right after `wl_pointer.enter` in the same
dispatch, so `mouseenter` sees none held and the next move sees them. An `OnDemand` layer gets
real focus on KWin only because KWin focuses it at map, taking focus from the app: a focus steal.
So nothing portable tells a keyboard-less surface that Super is held; `hot_corners.modifier` has
no portable source. `scripts/dev/probe-modifiers.sh`.

### F66. `use_modifiers`: a surface's held and locked keys

`use_modifiers() -> ReadSignal<Modifiers>` (`Modifier::{Held, Released}` for shift, ctrl, alt,
logo; `LockKey::{Locked, Unlocked}` for caps and num lock) is published to the keyboard-focused
surface on `enter` (from the seat's cache, which may already be right through KWin's
pointer-tied path, F61) and on every `wl_keyboard.modifiers`; `leave` publishes the empty value
to the surface losing focus. A surface's first read, before its first configure, is always the
default; the real state lands in the same event-loop pass, before the first frame.
`scripts/dev/accept-modifiers.sh`.

### F74. The input modality is the seat's

`InputModality::{Pointer, Keyboard}`, `use_input_modality()`, `SurfaceHandle::
current_input_modality()`: one `Live<InputModality>` in `SeatInput`, cloned into every handle,
so all surfaces agree and a new one starts from the current value. `modality::after` (pure): a
key press that is not a lone modifier makes it `Keyboard`, a pointer button press `Pointer`;
releases, repeats, scrolls and motion change nothing (quire's reference host and browsers'
`:focus-visible` switch on a press only; with motion a brushed mouse would hide the ring
mid-Tab-walk). The host never depends on quire; a shell bridges it into `ds::HostModality` in
its root (README, "Consuming: input modality").

### F71. IME: Blitz enables it only once the field has its text data; `done` resets the preedit

Blitz asks the shell for the IME in `Node::focus` only once the node has its `TextInputData`,
which layout builds on the first `resolve`. A field focused on mount, before that, would never
enable `zwp_text_input_v3`, and raw keys would reach it. After each resolve the host enables a
focused text field whose request is still disabled, at its content box (`dom/ime_focus.rs`); a
resend on `enter` covers the other order. fcitx5 commits with `preedit_string(nil) +
commit_string + done`, and the protocol resets the preedit on every `done`, so `on_done` clears
it first unless a lone new preedit replaces it. Chewing commits with Return; Space opens the
candidate list. `scripts/dev/accept-ime.sh`.

### F22. Paste reads the selection ahead of time; the text can be a few ms stale

`ShellProvider::get_clipboard_text` is synchronous on the main thread, which also serves the
pipe when the selection is ours, so a paste cannot block on a read. Every
`wl_data_device.selection` runs `Clipboard::follow`: `receive` for the best text MIME, the pipe
read on a helper thread (200 ms timeout), the text published into a generation-checked
`SharedSelection` every surface's provider holds. Between a new selection and its read
finishing, a paste returns the previous text; an older read finishing late is dropped. No text
type, a failed read or a timeout publishes "no text"; a timed-out reader thread ends when the
owner closes its end. Blitz's own copy is queued and claimed in the flush's request phase.

### F5. The keysym and evdev tables are Apache-2.0, adapted from winit

`input/keysym.rs` and `input/keycode.rs` are adapted from winit 0.30.13
`src/platform_impl/linux/common/xkb/keymap.rs`. winit is Apache-2.0 only, not MIT OR Apache-2.0;
both files carry the attribution and a list of changes, as Apache-2.0 §4 requires, and a
NOTICE-style mention belongs in any binary distribution. Open: accept that, or re-derive the
tables from `linux/input-event-codes.h` and the xkbcommon keysym list (both facts, not
expression). Two deliberate changes from winit and blitz-shell: the Super keys are `Key::Meta`,
`Code::MetaLeft/Right` and `Modifiers::META` (the W3C names `metaKey` reads), and `space` is
`Key::Character(" ")` (keyboard-types 0.7 has no named Space).

### F17. The first-mouse window is a setting

`input::pointer::first_mouse` takes the window as a parameter; the caller reads the settings key
`menus.first_mouse_window_ms` (default 100), which design/22 marks proposed.

## Drag source

### F60. `SurfaceHandle::start_drag`

```rust
pub async fn start_drag(&self, spec: DragSpec) -> DragOutcome;
pub struct DragSpec { pub payload: DragPayload, pub actions: DragActions, pub icon: DragIcon }
pub enum DragPayload { Uris(Vec<String>), Text(String), Bytes { mime: Mime, bytes: Vec<u8> } }
pub enum DragOutcome { Dropped { action: DragAction }, Cancelled, Unsupported { why: DragRefused } }
```

Call it while the button that began the gesture is still down. `ctx::drag::start` refuses up
front, as `Unsupported`, what the compositor would ignore or break: a drag of ours still running,
the surface gone, no data device, a `wl_data_device_manager` older than v3 (the source would
never learn how the drop ended), or no held press on this surface (the grab record's press, F49;
unlike a move the pointer need not still be on the surface). Pipe writes are non-blocking, one
chunk of at most 64 KiB per writable wakeup from a calloop source, so a slow reader never blocks
the Wayland thread; a reader that neither reads nor closes keeps its source until the host exits.
KWin sends `dnd_drop_performed` even for a release over nothing, then `cancelled`. The icon
(`DragIcon::Root`) is a `Role::DragIcon` surface rendered through the ordinary presenter,
configured from birth. `scripts/dev/accept-drag.sh` (a foot window as the target).

Open: on the headless KWin the icon stays where the drag began (that backend draws no cursor
layer); whether it follows the pointer on a real session is a manual check. Not covered live:
`Move`, `Text`/`Bytes` payloads, a stalling reader.

## Scrolling

### F63. The scroll engine: shell-host owns every scroll of a Blitz document

design/11 §11.3.1. Wheel, touchpad, trackpoint, palmrest and keyboard input never reach Blitz's
default scroll action; `scroll/` runs the §11.4 machine, writes raw offsets each frame, and the
host paints its own overlay thumb. `HostCtx::set_scroll_config` (not in `HostConfig`, which is
`Eq` while the physics keys are `f32`), `use_scroll(ElementId)`, `SurfaceHandle::scroll(
ScrollCmd)`, and the markers `data-wheel="capture"`, `data-overscroll="band"` (the only way a
scroller becomes elastic), `data-keys="capture"`.

**Blitz facts (at the pinned rev).**
- Raw offsets survive `resolve`: nothing clamps `viewport_scroll` or a node's `scroll_offset`.
- `resolve_scroll_animation` acts only on a `Fling` (touch pans, never sent) or a smooth
  `ScrollTo`; a document calling Blitz's own smooth `scroll_to` would fight the engine, so
  `scroll-behavior: smooth` is a quire lint and programmatic scrolls go through `ScrollCmd`.
- Offsets are written before the resolve: `flush_styles_to_layout` hoists z-indexed descendants
  of a scroller with the offset current at that moment.
- Blitz hit-tests page coordinates; the host sets page = client + viewport scroll.
- `get_client_bounding_rect` subtracts the element's own scroll offset along with its
  ancestors'; `dom::scroll::area` adds it back for the thumb.
- Blitz's overlay scrollbars (the `scrollbars` feature) are off; `scripts/check-boundary.sh`
  fails if it is ever enabled.
- Blitz has no keyboard scroll action, and raw writes fire no `onscroll`: `use_scroll` is the
  notification.
- dioxus-native-dom records every listener as an attribute named for the event
  (`keydown="<rust func>"`), which is how the host detects that an element handles keys. Blitz
  cannot report `prevent_default`, so a key listener on the focus path is taken as consuming
  every scroll key.

**Keyboard (§11.3.10).** A scroll key is taken by the engine only when the focus is nowhere and a
scroller with content is under the pointer, or the focus sits inside a scroller on the key's axis
and nothing on the focus path handles keys (listener, editable or form control,
`data-keys="capture"`).

**Sources.** A `wl_pointer` frame with source wheel, wheel_tilt or none is detents (`value120`,
else `discrete`, else degrees / 15): a compositor that sends no source sends no `axis_stop`
either, so a pan from it could never end. KWin's fake input sends exactly that. `finger` is a
phased gesture with momentum and band; `continuous` tracks without either.
`axis_relative_direction` is read but not applied (compositor values already carry the natural
setting).

**Open.**
- `SurfaceDocument::element_rect` still subtracts a scrolled container's own offset, so a region
  element that is itself a scrolled container would shift.
- Blitz's hit test does not clip a scroller's overflow: a point outside a scroll container but
  over its hidden content latches the container.
- Thumb drag and track click are not wired (`indicator::drag_offset` is the formula);
  `scrollbars = Always` still shows one thumb at a time.
- The thumb's colour follows the host's colour scheme; quire's `--scroll-thumb` is not readable
  from the host.
- The viewport is rigid: `html`/`body` cannot carry `data-overscroll="band"` from Dioxus.
- Not proven live: touchpad, keyboard, palmrest, hover expansion, the thumb on screen.
- The one-finger page swipe (design/12 §12.3.6) and `Tap{One}` have no consumer; a new
  `HostEvent` for them is an API change to agree with sill.

### F64. The palmrest client and the double-scroll window

The host connects to `$XDG_RUNTIME_DIR/palmrest/events.sock` when it exists (non-blocking
`SOCK_SEQPACKET` through `rustix`), at start and again on the next pointer enter while
unconnected: no timer, no retry loop. Wire form is palmrest's: one JSON message per packet, at
most 4096 bytes, `PROTOCOL_VERSION` 1, enums adjacently tagged, `snake_case`;
`input/palmrest/wire.rs` re-declares the types with palmrest's serde attributes (no dependency
on palmrest's crate) and its tests hold byte strings printed by palmrest's own encoder. A `Hello`
with another version disconnects until the next enter; a packet over 4096 bytes is skipped.
`PointerOver{Ours|NotOurs}` is sent on changes and after each connect.

The window (§11.3.14 rule 2): a compositor axis frame whose source is `finger`, `wheel` or
absent is dropped while palmrest reports fingers down, while a glide palmrest started runs, or
for `scroll.double_scroll_grace_ms` (150) after the fingers lift, unless an `axis_stop` comes
first. Not proven: a real compositor's echo timing, `Tap`/`Swipe` (no consumer).

## Session lock

### F65. `ext-session-lock-v1`, one document per output

`HostCtx::lock_session(config, factory) -> Result<LockId, LockError>`, `unlock_session`,
`lock_surfaces`; `use_lock()`, `use_lock_phase()`; `HostEvent::Lock`. Without
`ext_session_lock_manager_v1` (KWin 6.7 has none) the call answers `Unsupported`; a second lock
while one lasts answers `InProgress`. `lock/machine.rs` (pure) sends the request the protocol
allows in each phase: `destroy` before `locked`, `unlock_and_destroy` once locked (also after a
`finished` that followed `locked`), and destroys every surface after `unlock_and_destroy` so the
output never flashes the compositor's colour.

- Lock surfaces are made at the request, not at `locked` ("clients should immediately create lock
  surfaces for all outputs"); cosmic-comp 1.8 sends `locked` without waiting for them. A lock
  surface is never committed before its configure, never hidden, carries no popup.
- `Unlocking` is published to every lock document a flush before `unlock_and_destroy`, so an exit
  animation plays first and calls `unlock()` at its end.
- Which lock surface has the keyboard is compositor policy. Blitz's `autofocus` is a feature the
  pinned block does not enable, so a lock document focuses its field after mount.
- **If the client dies while locked the session stays locked** (the protocol's secure failure;
  sctk's `Drop` sends `destroy`, which after `locked` is `invalid_destroy`). On cosmic-comp 1.8 a
  new locker's lock is then granted and its unlock returns the session: sill's session script
  restarts the shell, which should lock at start when logind's `LockedHint` says so.
- `examples/demo_lock` refuses any compositor but a throwaway one (`guard.rs`, table-tested);
  `scripts/dev/accept-lock.sh` runs cosmic-comp inside the headless KWin.
- Not proven live: more than one output, outputs changing while locked, fractional scale on a
  lock surface, `finished` after `locked`, the shm presenter, a real cosmic session (manual).

## Fonts

### F68. `HostConfig::fonts` carries a shell's faces

`HostConfig::fonts: FontFaces` (`&'static [u8]` face bytes, `#[serde(skip)]`) is registered into
the shared font context at `Host::connect`, so every document's `FontContext` carries them.
Without it every document falls back to fontconfig: a stack naming Inter lays out with the first
face the context has, and Noto Color Emoji's cmap covers space and digits. `SharedFonts::
register` returns the family names fontique reports. `registered_families(&HostConfig)` lets a
shell assert its faces as a plain test; `headless::Fonts::Bundled(FontFaces)` takes the same
value, and `headless::bundled_faces()` is the fallback for shell-host's own tests.

## Debug injection

### F78. Debug injection returns its results

`HostCtx::inject(SurfaceId, Inject) -> Result<Injected>` (feature `debug-inject`) prints
nothing: `Inject::{Pointer, PointerDown, PointerUp, Click, Leave, Capture}`, `Injected::{Done,
Captured { path, size }}`. Every variant goes through `HostState::deliver`, the path real
`wl_pointer` input takes (cursor readback, invalidation), and presses update the seat's held
buttons, so a press, moves and a release script a drag: a move with the button held carries
`buttons` and crosses Blitz's 2 px drag threshold (after which `pointerup` dispatches no
`click`). A secondary release makes Blitz dispatch `ContextMenu`. Frame statistics are not an
injection (F15). The stdin protocol (`inject::stdin`) takes whole logical px; `<surface>
focused` reads a text field back.

## Test harness

### F52. Real input on a headless KWin

`scripts/dev/kwin-virtual.sh start [WxH]` starts `kwin_wayland --virtual --no-lockscreen
--no-global-shortcuts` on a private socket with a private bus (F56) and an empty
`XDG_CONFIG_HOME` (the user's KWin scripts would act on the windows under test), and writes
`scripts/dev/.kwin-env`. `tools/fake-input` (its own workspace: `wayland-protocols-plasma` is
KDE-only) and `scripts/dev/fake-input.sh` drive `org_kde_kwin_fake_input`: `move`, `press`,
`release`, `key`, `type`, `axis`, `sleep`. Keysyms resolve against the keymap KWin sends, read
with `read_exact_at(0)` (KWin hands every client the same file). `scripts/dev/kwin-windows.sh`
prints window geometry through a KWin script.

- **Authorisation**: `org_kde_kwin_fake_input` is restricted; it is advertised only to an
  executable whose `.desktop` lists it in `X-KDE-Wayland-Interfaces`, or to every client when KWin
  runs with `KWIN_WAYLAND_NO_PERMISSION_CHECKS=1`, which `kwin-virtual.sh` sets on its instance
  only (never on a real session).
- A pointer drag needs a pause between the press and the first motion (100 ms): motion KWin
  handles before the client's `xdg_toplevel.move` arrives is plain motion.
- Fake input reaches KWin's own clients only. cosmic-comp run *inside* the headless KWin
  (`COSMIC_BACKEND=winit`) receives it; there it needs the system EGL vendor (the headless KWin
  hands it the NVIDIA render node; the Mesa override draws nothing).

### F54. `kwin-virtual.sh stop` stops the scope `start` made

D-Bus activation daemonizes (parent pid 1) into the caller's login cgroup, so neither a
parent-chain walk nor an environment scan finds the services on the private bus, and an
environment scan also matches the caller. `start` runs the bus and KWin under `systemd-run --user
--scope`, and `stop` is `systemctl --user stop` on it (with the caller's own runtime dir). Without
a user manager, or with `KWIN_VIRTUAL_CONTAINMENT=tree`, `stop` kills the recorded pids and their
descendants, excluding itself and its ancestors.

### F56. The private bus activates nothing

`scripts/dev/private-bus.conf` is the stock session bus with no service directories, so a real
`Notify` call fails `ServiceUnknown` instead of starting KDE's notification daemon on the test
bus.

### F39. A nested cosmic-comp whose EGL surface is lost runs away

On the winit backend, after a screenshot the EGL window surface can be lost
(`eglSwapBuffers: EGL_BAD_SURFACE`, `eglCreatePlatformWindowSurfaceEXT: EGL_BAD_ALLOC`, "context
lost"); the compositor retries every frame, each attempt leaks, and with vblank sync off nothing
throttles it: 64 GB resident after 4.5 hours on the NVIDIA path. `scripts/dev/nested.sh` starts a
watchdog that kills the compositor past `NESTED_RSS_MAX_MB` (default 4096). The same log lines
appear once per `grim` capture on either EGL and are otherwise harmless.

### F79. A nested cosmic-comp needs the host session unlocked

`COSMIC_BACKEND=winit` opens the nested compositor's output as an ordinary toplevel of the host
compositor. A locked KWin session maps no new client's toplevel (a plain `ghostty` hangs too), so
the nested compositor accepts connections and never answers them. `nested.sh` bounds its
`wayland-info` check with a timeout and prints the lock/DPMS diagnosis. cosmic-comp inside the
headless KWin (F52) has no such dependency.

## cosmic-comp

### F29. Nested cosmic-comp on NVIDIA EGL loses every layer surface that has a blur region

cosmic-comp 1.8.0, `COSMIC_BACKEND=winit`, rendering through NVIDIA's EGL: any
`ext_background_effect_surface_v1.set_blur_region` on a layer surface makes the whole surface
vanish, whatever the client (a plain `wl_shm` client, `examples/blur_probe.rs`, too), whatever
the region or its timing. The surface is still in the rendered set (it gets frame callbacks);
its pixels are lost after cosmic-comp's framebuffer blur pass. On Mesa (AMD iGPU) blur works.
Toplevels are drawn but never blurred on either GPU. The DRM backend was not tested.

`blur_health.rs` (pure) says `Breaks` only for cosmic-comp + winit output (`WINIT-n`) + an NVIDIA
main render device (`handlers/render_device.rs` reads the vendor of `zwp_linux_dmabuf_v1`'s
default feedback `main_device`); the host then sends no blur region and the debug line says
`blur: degraded`. `nested.sh` renders on Mesa by default (`SILL_NESTED_GPU=nvidia` for the old
path). Open: no shell can ask whether blur is on (proposed: `HostCtx::blur() -> Support`); a
cosmic-comp issue with `blur_probe` as the repro is not filed; re-test on a real cosmic session.

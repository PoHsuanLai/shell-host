# Design map

Which part of the design each module implements. The program plan (section "Design:
`shell-host` repo"), `FINDINGS.md` and quire's `design/10`-`13` are canonical; this file only
says where in the code a paragraph lives, so a reviewer can go from one to the other. Paths are
under `crates/shell-host/src/`. "Pure" marks the modules with no effects, which
are table-tested; "effects" marks the ones that talk to Wayland, the GPU, the disk or threads.

## Public API (plan "Public API")

| Module | Implements | State |
| --- | --- | --- |
| `host.rs` | `Host<M>`: `connect`, `handle`, `run`; plan "Event loop" (calloop, `run(None)`, tokio runtime entered) | effects |
| `ctx.rs` | `HostCtx<M>`: `outputs`, `spawn_layer`, `spawn_toplevel`, `close`, `set_visibility`, `apply`, `activation_token`, `stats` (the one source of frame statistics), `take_stats`, `surface_named`, `set_color_scheme`, `exit` | effects |
| `ctx/flush.rs` | the flush after every dispatch, plan "Event loop"; `ORDER` = `PollWoken`, `ApplyRequests`, `StepFrames`, `ArmTimers`, `FlushConnection` (see "Flush order" below); the `Flush` trait is the seam `tests/core.rs` records the order through | effects |
| `ctx/spawn.rs` | building roles (layer, toplevel, popup) unmapped, mapping them, and the surface's document with its `SurfaceHandle` provided as a root context before the first build | effects |
| `ctx/ops.rs` | applying queued requests: `SurfaceOp`s, popups, closes, `CopyText` and Blitz's own copies (`claim_copied`), activation tokens, hide/show (a hidden surface's role is rebuilt unmapped) | effects |
| `ctx/paint.rs` | phase 3: presenter on first use (the one device, validated by first present, else shm), geometry follow, render; the only place regions are sent (`send_regions`, change-detected by `resolve::Tracked`) | effects |
| `ctx/rescale.rs` | the start of phase 1: each surface takes its latest `wp_fractional_scale_v1` preferred scale and publishes it to its document before the woken documents are polled, so the frame that first shows a scale is rendered by a tree that read it (FINDINGS F43) | effects |
| `ctx/window.rs` | a toplevel's window-management ops: `xdg_toplevel.move`/`.resize` under the serial rule (`grab`), maximize, minimize, fullscreen (FINDINGS F46, F49) | effects |
| `ctx/timer.rs` | phase 4: arming and cancelling watchdogs and key repeat from `TimerRequest`s | effects |
| `ctx/drag.rs` | starting a drag (`Request::StartDrag`: refused up front as `DragOutcome::Unsupported`, else `wl_data_device.start_drag` with the held press's serial), and phase 4's pipe writes: each `send` pipe made non-blocking and written from the loop a chunk per writable wakeup (FINDINGS F60) | effects |
| `ctx/drag_icon.rs` | `DragIcon::Root`: a `Role::DragIcon` surface rendered through the ordinary presenter path, configured from birth, removed when the drag ends | effects |
| `ctx/lock.rs` | the session lock's effects (FINDINGS F65): `lock_session` (`LockError::Unsupported` without `ext_session_lock_manager_v1`), `unlock_session`, `lock_surfaces`; `advance`, run at the end of the request phase, feeds the machine what the handlers and requests queued and carries out its actions: a `Role::Lock` surface per output with its document (factory + `LockHandle` context, no commit before the configure), `Unlocking` told to the documents a flush before `unlock_and_destroy`, `HostEvent::Lock` | effects |
| `host_handle.rs` | `HostHandle<M>`: `send`, `apply`, `close`, `exit` from any thread | effects |
| `handle.rs` | `SurfaceHandle` + `use_surface`; plan "Ops queue while a doc is borrowed, apply after `poll()`, before paint"; the frame hook's document side: `drive_frames`, `next_frame`, `use_frame_tick` (FINDINGS F31); the scale's: `use_surface_scale`, `current_scale` (FINDINGS F43); the window's: `begin_move`, `begin_resize`, `set_maximized`, `set_minimized`, `set_fullscreen`, `current_toplevel_state`, `use_toplevel_state` (FINDINGS F46); the drag source: `start_drag(DragSpec) -> DragOutcome` (FINDINGS F60) | effects |
| `lock/handle.rs` | `LockHandle` (`id`, `phase`, `unlock`), `use_lock`, `use_lock_phase`: a lock document's side (FINDINGS F65) | effects |
| `bridge.rs` | `use_watch` (plan: shared data on `tokio::sync::watch`) | effects |
| `root.rs` | `Root::new/with_props/context` | effects |
| `event.rs` | `HostEvent<M>`, `CloseReason`, `Focus`, `widen` | pure |
| `msg.rs` | `SurfaceOp` (with the window ops and `repaint()`), `Repaint`, `Visibility`, `HostMsg<M>`, `Request` | pure (data) |
| `config.rs` | `HostConfig{backend, adapter, first_mouse_window_ms, prepaint}`, `Backend`, `AdapterPref` (adapter validated by first present, FINDINGS F16), `Ms`; `first_mouse_window_ms` is the settings key `menus.first_mouse_window_ms` (design/22, proposed, default 100), handed to `HostState` and read by the pointer handler | pure |
| `error.rs` | `HostError`, `Recovery::{Retry,Fallback,Degrade,Fatal}` (plan "Conventions substitutions") | pure |
| `id.rs` | `SurfaceId`, `OutputId`, `SurfaceIds` | pure |
| `output.rs` | `OutputInfo` | pure (data) |

## Surfaces and geometry

| Module | Implements | State |
| --- | --- | --- |
| `live.rs` | `Live<T>`: a value the host publishes to a document, waking readers only on a change; carries the scale and the toplevel state | shared cell |
| `live_scale.rs` | `LiveScale = Live<Scale>` (FINDINGS F43) | shared cell |
| `lock/machine.rs` | the session lock's state machine: `LockPhase`, `LockState`, `LockInput`, `LockAction`, `start`, `step` (Requested, Locked, Unlocking, Unlocked, Finished; outputs added and removed; the destructor the protocol allows in each phase) | pure |
| `lock/mod.rs`, `lock/active.rs` | `LockId`, `LockConfig`, `LockError`; `ActiveLock`, the lock in force (the sctk `SessionLock`, the machine, the factory, the surface per output, the inbox) | pure (data) / effects |
| `layer.rs` | `LayerConfig`, `Layer` (incl. `Bottom`, FINDINGS F76 item c), `Anchor`, `Extent`/`LayerSize`, `ExclusiveZone`, `KeyboardMode`, `OutputTarget`; wire mappings. design/13 §13.3.8 (bar and dock never focus) | pure |
| `toplevel.rs` | `ToplevelConfig`, `Decorations::{Client(ResizeMargin), Server}`, `ResizeEdge`, `Maximize`, `Fullscreen`, `Activation`, `TileEdges`, `ToplevelState` and their wire mappings; design/20 "All apps are xdg toplevels" (FINDINGS F46) | pure |
| `csd.rs` | a client-decorated toplevel's geometry: the resize band (`Inset`, only while floating), surface vs frame, `set_window_geometry`, the document's inset in physical px, band strips, `zone_at` and the resize cursors (FINDINGS F48) | pure |
| `grab.rs` | the move/resize serial rule: `PointerFacts`, `grab_serial`, `GrabRefused`, the shared `GrabRecord` (FINDINGS F49) | pure + shared cell |
| `prepaint.rs` | `Prepaint::{None, Hook}`: the consumer's pass between the frame's one `resolve` and its paint, for device-pixel snapping (FINDINGS F43) | pure |
| `popup.rs` | `PopupConfig` (with `Offset`, design/13 §13.3.2's (-8, +6), FINDINGS F30), positioner params and wire values; design/13 §13.3.2-13.3.4, §13.7 | pure |
| `popup_keyboard.rs` | Whether a popup lifts its `KeyboardMode::None` parent layer to `OnDemand`: a grabbing one does, never on KWin (FINDINGS F72) | pure |
| `placement.rs` | `PopupPlacement` (anchor rect and size the config does not carry, FINDINGS F4) | pure (data) |
| `region.rs` | `RegionSpec`, `rounded_strips`; design/10 §10.3.12 | pure |
| `resolve.rs` | `RegionSpec` -> strips after layout; `Tracked`, what was last sent, so only a changed region is recommitted; design/10 §10.3.12 "recomputed on every frame whose pill rect changed" | pure |
| `geometry.rs` | `Point`, `Margin`, `enclosing_rect`, `inflate`, `element_id` | pure |
| `blur.rs` | `BlurSpec`; plan "blur (BlurSpec{None,Region})", quire 03-COLOR §17.1 (region is shell-host's) | pure (data) |
| `scale.rs` | `Scale` in 120ths, `buffer_size`; plan "Scale" | pure |
| `surface.rs` | `HostSurface` (field order = drop order, FINDINGS F75), `AppliedRegions` (two `resolve::Tracked`: input starts unknown, blur starts empty) | effects |
| `role.rs` | `Role::{Layer,Popup,Toplevel,DragIcon,Lock}`; plan "Opening a grabbing popup from a `KeyboardMode::None` layer switches it to OnDemand" (not on KWin: `popup_keyboard.rs`, FINDINGS F72) | effects |

## Frames, time, stats

| Module | Implements | State |
| --- | --- | --- |
| `frame.rs` | plan "Frame machine" (Idle/Dirty/InFlight/InFlightDirty, watchdog 50..1000 ms); FINDINGS F75; cosmic-comp #2777; design/10 §10.7 "the watchdog applies" | pure |
| `tick.rs` | the frame hook (design/10 §10.4's `Frame`): `FrameTicks` per surface, `FrameDrive`, `FrameTick`, `HostInstant`; `pacing` (animating or driven) and `tick_after` are pure (FINDINGS F31) | pure + shared cell |
| `timers.rs` | `TimerRequest`: watchdog and key-repeat timers asked for by handlers, armed by `HostCtx` | pure (data) |
| `clock.rs` | `HostClock` (plan "one HostClock for `resolve(t)`") | pure |
| `debug.rs` | `FrameStats` (work time apart from interval, FINDINGS F15), `StatsWindow`, `percentiles`, `vm_rss_kib`, per-surface `DebugCommand` grammar | pure |
| `inject.rs` | feature `debug-inject`: `HostCtx::inject(SurfaceId, Inject) -> Result<Injected>` (pointer move, press, release, click, leave, capture — a press, moves and a release script a drag through `HostState::deliver`, the same code path real `wl_pointer` input uses, FINDINGS F78), returning results, printing nothing | effects |
| `inject/stdin.rs` | the demos' stdin protocol (whole logical px) mapped onto `inject` and `take_stats`, one JSON line per result | effects |

## Documents (`dom/`)

| Module | Implements | State |
| --- | --- | --- |
| `dom/document.rs` | `SurfaceDocument`; `UA_STYLESHEET` (FINDINGS F12), `BODY_RESET` (FINDINGS F62); `StyleThreading::Sequential`; element rects for `RegionSpec::Element` and `PopupAnchor::Element`; `data-first-mouse` lookup (design/13 §13.3.8) | effects |
| `dom/stacking.rs` | `rehoist`: gather z-indexed boxes again after every resolve, from the fresh layout (FINDINGS F69) | effects |
| `dom/head.rs` | `HeadDocument` shim (plan: `DioxusDocument` ignores `document::Style/Link/Title`) | effects |
| `dom/shell_provider.rs` | `ShellProvider`: redraw flag (cleared after present, a load from another thread kept, FINDINGS F75, F40), cursor, IME; paste answers from the seat's `clipboard::SharedSelection`, copy queues for `ctx::ops::claim_copied` | effects |
| `dom/fonts.rs` | one `FontContext` cloned into every document (plan "Fonts") | effects |
| `dom/net.rs` | `LocalNet`: `data:` decoded and `file:` (absolute paths, empty or `localhost` host) read, both on the host runtime's blocking pool; nothing from the network (quire FINDINGS S7, S8; the wallpaper is `url(file://…)`) | effects |
| `wake.rs` | per-surface wakers on one shared ping (plan "Event loop") | effects |

## GPU (`gpu/`)

| Module | Implements | State |
| --- | --- | --- |
| `gpu/adapter.rs` | adapter ranking and validation by first present on a throwaway connection (FINDINGS F16) | effects |
| `gpu/probe.rs` | the throwaway connection and surface the first-present validation uses; probe instances live for the process (NVIDIA teardown crash, FINDINGS F16) | effects |
| `gpu/block_on.rs` | the minimal executor for wgpu's adapter and device futures | effects |
| `gpu/context.rs` | one device, one `vello_hybrid::Renderer` + `Resources`, the per-device image cache (FINDINGS F1), `devices_created` | effects |
| `gpu/context_proxy.rs` | custom-widget textures across frames (FINDINGS F77) | effects |
| `gpu/raw_handle.rs` | the one `unsafe` module: surface from raw handles, `WaylandDisplay: HasDisplayHandle` (needed for `InstanceDescriptor::display`) | effects |
| `gpu/presenter.rs` | `Presenter::{Wgpu, Shm}` (plan "presenter{Wgpu,Shm}"), `capture` | effects |
| `gpu/swapchain.rs` | Mailbox then Fifo; PreMultiplied, PostMultiplied, else opaque and blur off (plan "GPU") | effects |
| `gpu/shm.rs` | CPU `wl_shm` fallback (plan "shm/vello_cpu presenter fallback") | effects |
| `gpu/render.rs` | resolve, paint, render (the present ordering in FINDINGS F75) | effects |
| `headless.rs` | `render_png` via vello_cpu (plan "Headless") | effects |

## Input (`input/`, handlers)

| Module | Implements | State |
| --- | --- | --- |
| `input/pointer.rs` | plan "Input": buttons 272..276, move before down, leave at (-1,-1); the axis frame for the scroll engine (`axis_frame`), the raw wheel event for a `data-wheel="capture"` element; page coordinates follow the viewport's scroll (`scrolled`); first-mouse window (design/13 §13.3.8, setting `menus.first_mouse_window_ms`) | pure |
| `input/scroll.rs` | scroll input routed to the surface's engine: axis frames (double-scroll window, capture exception first), scroll keys, palmrest messages, `ScrollCmd` (FINDINGS F63, F64) | effects |
| `input/palmrest.rs`, `palmrest/{wire,route}.rs` | the palmrest client: `SOCK_SEQPACKET` connect, `PointerOver`, reading; the re-declared design/12 §12.3.10 messages (JSON per datagram); what each message means for the engine (F64) | effects / pure |
| `input/keysym.rs` | xkb keysym -> `Key`, `Location` (winit table, Apache-2.0, FINDINGS F5) | pure |
| `input/keycode.rs` | evdev -> `Code` (winit table, Apache-2.0) | pure |
| `input/modifiers.rs` | sctk modifiers -> `Modifiers` | pure |
| `input/key.rs` | `BlitzKeyEvent` assembly | effects |
| `ime.rs` | text-input-v3 buffered until `done` (plan "IME") | effects |
| `clipboard.rs` | own-selection cache; every selection event reads the offer on a helper thread (200 ms timeout, plan "Clipboard") and publishes into `SharedSelection` (generation-checked, so a slow old read never wins) | effects |
| `cursor.rs` | `wp_cursor_shape_v1` | effects |
| `drag/*.rs` | the drag source's pure parts: `spec` (`DragSpec`, `DragPayload`, `Mime`, `DragActions`, `DragIcon`), `offer` (mime types and bytes per payload), `serial` (the held press a drag starts from), `pipe` (bytes a pipe still owes, what a write result means), `outcome` (`DragOutcome`, `DragAction`, `DragRefused`, the end-to-outcome mapping) | pure |

## Scrolling (`scroll/`, design/11)

| Module | Implements | State |
| --- | --- | --- |
| `scroll/config.rs` | `ScrollConfig`: every design/22 §3.7 `scroll` and §3.8 `scrollbar` key with its default | pure |
| `scroll/geom.rs`, `latch.rs` | scrollers, axes, geometry; the latch at a gesture's start, the touchpad axis lock, the wheel burst (§11.3.3, §11.3.9, §11.3.11) | pure |
| `scroll/engine.rs`, `animate.rs` | the §11.4 state machine and its closed-form phases (glide, rebound, smooth step, held arrow, spring) | pure |
| `scroll/momentum.rs`, `velocity.rs`, `rubber.rs`, `smooth.rs`, `keys.rs` | the §11.5 formulas: drag model, least-squares release, bounded/linear stretch and snap-back, `--e-out` step, keyboard steps and spring | pure |
| `scroll/indicator.rs`, `paint.rs` | the overlay thumb's geometry and fade (§11.3.12, §11.5.6); painting it after the document | pure / effects |
| `scroll/source.rs`, `suppress.rs`, `cmd.rs` | axis frames to detents or phased pans, natural direction; the double-scroll window (§11.3.14); `ScrollCmd` | pure |
| `scroll/surface.rs` (+ `surface/*`), `tuning.rs`, `watch.rs` | one surface's engine, thumb and trackers applied to its document; `use_scroll` (§11.3.15) | effects |
| `dom/scroll.rs` | the engine's view of Blitz: scroller chain under a point, geometry, raw offset writes, `data-wheel` / `data-overscroll` / `data-keys` | effects |

## Wayland plumbing

| Module | Implements | State |
| --- | --- | --- |
| `state.rs` | `HostState`, the delegate macro list (the only file naming sctk delegate macros), `Drop` (FINDINGS F75) | effects |
| `handlers/compositor.rs` | frame callbacks -> `FrameInput::Callback`; integer scale fallback | effects |
| `handlers/output.rs` | `HostEvent::Output*` | effects |
| `handlers/layer.rs` | layer configure/closed; configure records the size and invalidates (regions follow on the render) | effects |
| `handlers/session_lock.rs` | `locked`/`finished` queued for the lock in force; a lock surface's configure (exact size) through `handlers::layer::follow_configure`, shared with layer surfaces | effects |
| `handlers/xdg.rs` | toplevel and popup configure/close; `popup_done` (not client-drivable, FINDINGS F77) | effects |
| `handlers/seat.rs` | capabilities (the first one adopts the seat, FINDINGS F47), `SeatInput` | effects |
| `handlers/pointer.rs`, `keyboard.rs` | input dispatch; every press and release updates the grab record | effects |
| `handlers/band.rs` | the pointer's half of `csd`: zone, document coordinates, the document's own cursor vs the band's, the resize a band press starts, dropping buttons at enter/leave (FINDINGS F48, F49) | effects |
| `handlers/text_input.rs` | hand `Dispatch` for `zwp_text_input_v3` (plan: SCTK has no client-side helper) | effects |
| `handlers/data_device.rs` | selection offers (`Clipboard::follow`), our data source, `claim_selection`; source events of our drag are routed to `drag_source` | effects |
| `handlers/drag_source.rs` | the drag's state (`Dragging`, `ActiveDrag`), `send` -> a queued pipe write, `action`, and `dnd_finished`/`cancelled` -> the outcome | effects |
| `handlers/activation.rs` | xdg-activation tokens | effects |
| `handlers/background_effect.rs`, `fractional.rs` | blur and fractional-scale + viewporter `Dispatch` | effects |

## Flush order (`ctx/flush.rs::ORDER`)

After every dispatch of the calloop loop, in this order and no other:

1. `PollWoken`: poll only the documents whose Dioxus tasks woke; a change invalidates. The head
   queue (`document::Style`, `Link`, `Title`) is applied inside the poll.
2. `ApplyRequests`: everything `SurfaceHandle`s and `HostHandle`s queued (including ops queued
   by step 1's polls), then text a document copied through its shell provider. An op lands
   after the poll that caused it and before the paint that shows it.
3. `StepFrames`: per configured, visible surface: presenter on first use, follow size and
   scale, take the redraw request, and on `Render`: the scroll engine's frame (raw offsets
   written before the resolve), resolve, publish `use_scroll`, send changed regions,
   `wl_surface.frame()`, present (its commit carries all of it), clear the redraw flag.
4. `ArmTimers`: watchdog, key-repeat and scroll-thumb requests from the handlers and step 3;
   the palmrest socket's source when a connection attempt is due.
5. `FlushConnection`: flush, and stop on a protocol error (FINDINGS F75).

## Examples and acceptance

`examples/demo_bar.rs` and `demo_dock.rs` are a bar and a dock (feature `debug-inject`);
`examples/probe_toplevel.rs` is a plain sctk client (a maximized checkerboard window) the
acceptance items measure against. `scripts/accept-demo.sh demo
[kwin-virtual|kwin-virtual@1.7|kwin|cosmic] <a|b|c|f|g>` runs the render items with their
thresholds (FINDINGS F76). `examples/demo_popup.rs` and `demo_popup_keys.rs` open popups on the
real host (FINDINGS F38, F72); custom-widget textures are covered by `tests/gpu.rs`.
`examples/demo_window.rs` is a client-decorated toplevel (titlebar drag, resize band, zoom,
configure state) driven from stdin (FINDINGS F46-F49). `examples/demo_scroll.rs` is a
scroll list (`use_scroll`, `data-overscroll="band"`) and a `data-wheel="capture"` element for
`scripts/dev/accept-scroll.sh` (FINDINGS F63). `examples/demo_lock/` is the session lock's demo (a password
field per output; `guard.rs` refuses any compositor but the throwaway one) for
`scripts/dev/accept-lock.sh` (FINDINGS F65).

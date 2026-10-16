# Architecture

shell-host runs Blitz / Dioxus Native documents on Wayland: layer-shell surfaces, xdg popups,
xdg toplevels and the session lock, on one GPU device and one renderer. `README.md` describes it
and its demos, `FINDINGS.md` holds the facts about Blitz, the compositors and the protocols it
relies on, and `CONVENTIONS.md` holds the rules every repo shares. This file is the map: where
each thing lives, what may depend on what, and how to add the next thing.

shell-host stands below quire and sill and is portable to any Wayland compositor with
layer-shell: no compositor-specific crate, no D-Bus, no PulseAudio, no design-system crate, no palmrest
daemon.

## 1. Crate map

| Crate | Repo | Purpose |
|---|---|---|
| `shell-host` | shell-host (`crates/shell-host`) | the host: Wayland plumbing, surfaces, frame loop, input, scroll, lock; the only crate in the workspace |
| `blitz-kit` | sibling repo `blitz-kit` | portable Blitz repairs and helpers, no Wayland, no Dioxus, no design system (section 4) |
| `palmrest-protocol` | sibling repo `palmrest` (`crates/palmrest-protocol`) | the palmrest socket's messages, `ToHost` / `ToDaemon`, `PROTOCOL_VERSION` |

Allowed edges (everything else is forbidden):

| From | May depend on |
|---|---|
| `shell-host` | `blitz-kit`, `palmrest-protocol`, `casement-protocols` (feature `client`: `quire_window_v1`'s bindings and the wire vocab, git rev pin), and the pinned third-party block (blitz-*, dioxus, sctk, wayland-*, calloop, wgpu, vello, parley, rustix, tokio, serde) |
| `blitz-kit` | `blitz-dom`, `blitz-traits`, `parley`, `wgpu` (feature `adapter`), `thiserror` |
| `palmrest-protocol` | `serde`, `serde_json`, `thiserror` |

Forbidden for `shell-host`: `zbus`,
`libpulse-binding`, every quire crate, sill, and the `palmrest` daemon crate.
`scripts/check-boundary.sh` checks this list, and that Blitz's own scrollbars stay off (the host
paints the thumb). `blitz-kit` forbids `smithay-client-toolkit`, `wayland-*`, `dioxus*`, every
quire crate and sill; its own repo carries the same script. Sibling repos are path
dependencies, as sill's are on shell-host.

### Moves into other crates

| From (shell-host today) | To |
|---|---|
| `dom/hover_sync.rs` (`LastMove`, `Shift`, `HoverSync`, `decide`, `probe_points`) | `blitz-kit::hover` |
| `dom/painted.rs` (`Affine2`, `Placed`, `painted_bounds`) and the transform-chain walk in `dom/document.rs` (`transform_chain`, `bounds_of`, `element_at`) | `blitz-kit::paint_rect`, `blitz-kit::hit` |
| `dom/net.rs` (`LocalNet`, `LocalSource`) | `blitz-kit::net` (with `FileRoots`, `MAX_RESOURCE_BYTES`) |
| `scroll/{geom,latch,engine,animate,momentum,velocity,rubber,smooth,keys,cmd}` and the physics part of `scroll/config` | `blitz-kit::scroll` |
| `dom/scroll.rs` (scrollers under a point, geometry, raw writes, `data-wheel` and key markers) | `blitz-kit::scroll::doc` |
| `ElementId` (`region.rs`) | `blitz-kit::element_id` |
| `dom/fonts.rs` (`SharedFonts`) | `blitz-kit::fonts` |
| `gpu/adapter.rs` (`DeviceKind`, `GpuBackend`, `AdapterFacts`, `rank`, `validate`) | `blitz-kit::adapter` |
| `input/palmrest/wire.rs` | `palmrest-protocol` (re-declaration deleted; shell-host imports the crate) |
| the `Prepaint` hook's snapping helpers (in consumers today) | `blitz-kit::snap`; the `Prepaint` type stays here |

## 2. Modules and layers

All modules are private (`mod`, items `pub(crate)`). The public surface is the curated root list in
section 3, plus two feature-gated public modules: `inject` (`debug-inject`) and `headless`
(`headless`). A module imports only from its own layer or a lower one; a lower layer never names
a higher one. When something needs a higher layer, move the shared piece down or pass it in.

| Layer | Modules | Nature |
|---|---|---|
| L0 vocabulary | `id`, `geometry`, `scale`, `output`, `error`, `modifiers`, `config`, `blur`, `event`, `msg`, `stats` | pure data |
| L1 role configs | `layer`, `toplevel`, `popup`, `region`, `placement`, `csd`, `drag/{spec,offer,serial,pipe,outcome}`, `prepaint`, `lock/{config,machine,retry}` | pure: configs, wire mappings, geometry |
| L2 machines and physics | `frame`, `resolve`, `grab`, `popup_keyboard`, `timers`, `clock`, `tick` (pacing), `modality`, `paste`, `input/{pointer,keysym,keycode,modifiers,capability,focus}`, `input/palmrest/route`, `scroll/{config,indicator,source,suppress,wheel}` | pure, table-tested |
| L3 shared cells and handles | `live`, `wake`, `handle`, `host_handle`, `bridge`, `root`, `lock/handle`, `scroll/{watch,wheel_feed}` | values shared with documents; no Wayland |
| L4 documents | `dom/{document,head,shell_provider,stacking,scheme,scroll,ime_focus}`, `headless` | Blitz, through `blitz-kit` |
| L5 GPU | `gpu/{context,context_proxy,presenter,swapchain,shm,render,probe,deadline,image_cache,rebuild,raw_handle}` | effects |
| L6 Wayland effects | `state`, `surface`, `role`, `handlers/*` (`pipe_writes`, `surface_objects` among them), `clipboard`, `cursor`, `ime`, `input/{key,scroll,palmrest}` (socket), `scroll/{surface,paint,tuning}`, `lock/active` | effects |
| L7 orchestration | `ctx/*`, `host`, `inject` | effects; the loop and the flush |

Fixed role files inside a concept directory (CONVENTIONS section 2 plus these): `decide.rs` (the
pure decision behind an effectful handler), `wire.rs` (mapping to protocol enums), `tests.rs`.

### Files that split (each over 400 lines today)

| Today | Becomes |
|---|---|
| `ctx/spawn.rs` (885) | `ctx/spawn/{mod.rs (insert, Initial, new_wl_surface), layer.rs, toplevel.rs, popup.rs, rebuild.rs (rebuild, map), document.rs (surface_document), size.rs (requested_size, wire_size)}`; its ~430 lines of hook tests move to `handle/tests.rs` |
| `debug.rs` (593) | `stats/{mod.rs, frame_stats.rs (FrameStats, FrameCounters, Dimensions, PresentMode, AlphaMode), window.rs (StatsWindow), percentile.rs, rss.rs}` and `inject/command.rs` (`DebugCommand`, `DebugAction`, `CommandError`, `parse_command`); `SurfaceName` moves to `id.rs` |
| `popup.rs` (561) | `popup/{mod.rs, config.rs (PopupConfig, PopupAnchor, PopupSize), positioner.rs (PositionerParams, positioner_params, refit), wire.rs (to_wire_*), tests.rs}` |
| `input/pointer.rs` (508) | `input/pointer/{mod.rs, button.rs, wheel.rs, events.rs, first_mouse.rs}` |
| `dom/document.rs` (498) | `dom/document/{mod.rs (SurfaceDocument, build, poll), hover.rs, resolve.rs (Resolved, resolve, animation), rects.rs (element_rect, content_size), first_mouse.rs}` |
| `handle.rs` (462) | `handle/{mod.rs, queue.rs (RequestQueue), surface.rs (SurfaceHandle: blur, regions, popups, close, copy), window.rs (move, resize, maximize, minimize, fullscreen), hooks.rs (every `use_*`), tests.rs}` |
| `scroll/indicator.rs` (454) | `scroll/indicator/{mod.rs, geometry.rs (Area, Metrics, thumb_span, geometry, in_zone, drag_offset), fade.rs (Indicator, Hover, Wake)}` |
| `ctx/paint.rs` (437) | `ctx/paint/{mod.rs (step_all, step_one), presenter.rs, geometry.rs (sync_geometry, viewport), render.rs, regions.rs (send_regions, region_of, in_surface), capture.rs}` |
| `resolve.rs` (437) | `resolve/{mod.rs, spec.rs (resolve, rounded_rect), anchor.rs (resolve_popup_anchor), tracked.rs (Tracked, Change)}` |
| `csd.rs` (404) | `csd/{mod.rs (Inset, inset_for), geometry.rs (surface_of, frame_of, window_geometry, content_px, to_document, to_surface), band.rs (Zone, band, zone_at, cursor_for)}` |
| `scroll/engine.rs` (401) | `blitz_kit::scroll::engine::{mod.rs (Engine, step), types.rs (ScrollIn, ScrollOut, Motion, Track, Flight, ...), phases.rs (may_begin, began, changed, lifted)}` |
| `inject.rs` (308) + `inject/stdin.rs` | `inject/{mod.rs, request.rs (Inject, Injected, KeyStroke), deliver.rs, command.rs, stdin.rs}` |

`input/keysym.rs` (507) is a table and stays whole. Tests that move with a split go beside the
piece they test, never into a catch-all.

## 3. Public API

The root of `shell-host` exports exactly these names, each from one `pub use` in `lib.rs`.

| Group | Names |
|---|---|
| Host | `Host`, `HostCtx`, `HostHandle`, `HostConfig` (its `file_roots`: where documents may read files, FINDINGS F21), `Backend`, `AdapterPref`, `FontFaces` and `FileRoots` (re-exported once from `blitz-kit`), `Ms`, `HostError`, `Recovery`, `Result`, `HostRoot`, `registered_families`, `WaylandRoute` (`Environment`, or `Handed(OwnedFd)` read only in `Host::connect`) |
| Events | `HostEvent` (incl. `SurfaceMapped { id }` and `QuitAsked`), `CloseReason`, `KeyboardFocus`, `SurfaceOp`, `Visibility`, `OutputInfo`, `OutputId`, `SurfaceId`, `ColorScheme` |
| Layer surface | `LayerConfig`, `LayerLevel`, `Anchor`, `LayerExtent`, `LayerSize`, `ExclusiveZone`, `KeyboardMode`, `OutputTarget` |
| Popup | `PopupConfig`, `PopupAnchor`, `PopupSize`, `PopupEdge`, `PopupToggle`, `PopupAdjust`, `PopupGrab`, `PopupOffset`, `PopupPlacement`, `offset_rect` |
| Toplevel | `ToplevelConfig`, `ToplevelState`, `Decorations`, `ResizeMargin`, `ResizeEdge`, `Maximize`, `Fullscreen`, `Activation`, `Tile`, `TileEdges`, `GrabRefused`, `WindowPlacement`, `PlacementSupport` |
| Lock | `LockConfig`, `LockError`, `LockId`, `LockHandle`, `LockPhase`, `LockState`, `use_lock`, `use_lock_phase` |
| Regions and blur | `RegionSpec`, `RoundedRect`, `ElementRegion`, `ElementId`, `BlurSpec` |
| Geometry | `Px`, `Rect`, `Point`, `Size`, `Margin`, `Scale`, `buffer_size` (`Px`, `Rect`, `Point`, `Size` are the geometry vocabulary and keep their short names) |
| Input | `Modifiers`, `Modifier`, `LockKey`, `InputModality` |
| Drag | `DragSpec`, `DragPayload`, `DragIcon`, `DragActions`, `DragAction`, `DragOutcome`, `DragRefused`, `Mime`, `BadMime` |
| Scroll | `ScrollConfig`, `ScrollSettings`, `ScrollbarSettings`, `ScrollbarVisibility`, `ScrollMomentum`, `ScrollMomentumModel`, `NaturalScroll`, `ScrollRubberBand`, `ScrollTrackClick`, `ScrollPx`, `ScrollMs`, `ScrollFraction`, `ScrollScalar` (the units of the settings), `ScrollCmd`, `ScrollAxis`, `ScrollAnimate`, `ScrollExtent`, `ScrollPhase`, `ScrollPoint`, `ScrollState`, `use_scroll`, `WheelEvent`, `WheelSource`, `WheelPhase`, `WheelMotion`, `use_wheel` |
| Frames and stats | `FrameDrive`, `FrameTick`, `HostInstant`, `FrameStats`, `Percentiles`, `SurfaceName`, `Prepaint`, `PrepaintFn` |
| Hooks | `use_surface`, `use_surface_scale`, `use_frame_tick`, `use_modifiers`, `use_input_modality`, `use_toplevel_state`, `use_wheel`, `use_watch`, and `SurfaceHandle` (what `use_surface` returns; `wl_surface() -> Option<WlSurface>`, `request_placement`, `placement_support`); `HostCtx::wl_surface(id)` |

Feature-gated public modules:

| Module | Feature | Holds |
|---|---|---|
| `shell_host::inject` | `debug-inject` | `Inject`, `Injected`, `PointerButton`, `KeyStroke`, `KeyAction`, `DebugCommand`, `DebugAction`, `CommandError`, `parse_command`, `parse_key`, `stdin::{install, apply}`; `HostCtx::inject` |
| `shell_host::headless` | `headless` | `render_png`, `HeadlessConfig`, `PixelSize`, `Fonts`, `bundled_faces` |

The debug command grammar is a `debug-inject` feature; a shipped shell contains no parser. sill
calls `parse_key` and `stdin::{install, apply}` in debug builds only.

`Inject` variants: `Pointer`, `PointerDown`, `PointerUp`, `Click`, `Leave`, `Capture`, and
`Key { stroke: KeyStroke, action: KeyAction }`. `KeyStroke { key: keyboard_types::Key, code:
keyboard_types::Code, mods: Modifiers }`, `KeyAction::{Press, Release, Tap}`. A key injection
enters at `HostState::key_down` and its release twin, the point `handlers/keyboard.rs` reaches
after xkb translation, so focus, modality (`Stroke`), key repeat's first press, the scroll keys
and IME see it as they see a real key. The stdin grammar adds `<surface> key <name>[+mods]
[down|up]`; without `down` or `up` it is `Tap`.

## 4. blitz-kit (sibling repo)

Portable Blitz repairs shared by shell-host and quire's `ds-blitz`; each was written twice, and
the copies in both are deleted. shell-host is its first consumer. It has no Wayland, no Dioxus,
no design-system type, and takes and returns CSS px `f64` in Blitz's own units.

| Module | Public API | From |
|---|---|---|
| `hover` | `LastMove::{Unknown, At(BlitzPointerEvent)}`, `remember(LastMove, &UiEvent) -> LastMove`, `Shift<N>`, `HoverSync::{Unchanged, Restore(PagePoint), Clear}`, `decide(shift, &LastMove, bounds, hit) -> HoverSync`, `probe_points(Bounds) -> [PagePoint; 5]`, `repair(&mut BaseDocument, &LastMove, Shift<NodeId>) -> Repaired::{Yes, No}` (restores the old hover without dispatch, feeds the last move again so Blitz dispatches the enter/leave diff) | `dom/hover_sync.rs`, `SurfaceDocument::sync_hover` |
| `net` | `LocalNet::new(FileRoots)` (`NetProvider`: `data:` decoded, absolute `file:` read on the caller's blocking pool when it is a regular file inside the roots and at most 32 MiB, nothing else), `FileRoots`, `LocalSource::{Data, File, Unservable}`, `LocalSource::{of, read, read_within}` | `dom/net.rs` |
| `fonts` | `SharedFonts::{system, system_with(&FontFaces), register(&'static [u8]) -> Vec<String>, for_document() -> FontContext}`, `FontFaces` | `dom/fonts.rs`, `HostConfig::fonts` |
| `adapter` (feature `adapter`) | `DeviceKind`, `GpuBackend`, `AdapterFacts::of(&wgpu::Adapter)`, `AdapterPref` (`Device { vendor, device }` follows the compositor's GPU), `PciVendor`, `PciDevice`, `rank(&AdapterPref, &[AdapterFacts]) -> Vec<usize>`, `request_watched_device` (logs wgpu errors instead of panicking, reports a lost device through `DeviceHealth`, gives up after a deadline), `block_on_timeout`; the surface-based `validate` stays in shell-host (it needs the Wayland probe) | `gpu/adapter.rs` |
| `hit` | `element_at(&BaseDocument, PagePoint) -> Option<NodeId>`, `element_of(&BaseDocument, NodeId) -> Option<NodeId>`, `is_content_element` (transform-aware: the point is carried back through each transformed ancestor) | `dom/document.rs` |
| `paint_rect` | `Affine2`, `Placed`, `painted_bounds(Bounds, &[Placed]) -> Bounds`, `painted_rect(&BaseDocument, NodeId) -> Option<Bounds>` (layout box carried through its own and its ancestors' transforms) | `dom/painted.rs`, `dom/document.rs` |
| `snap` | `snap_layout(&mut BaseDocument, Scale120)`: whole device pixels for every box after a resolve | consumers' prepaint hooks |

`Bounds`, `PagePoint`, `Scale120` (integer 120ths, as `Scale` in shell-host) are defined in
`blitz-kit::units`. shell-host converts at its own boundary (`PointerPos`, `Rect`, `Scale`);
neither side names the other's types. Tests for each module live in the kit against a real
`BaseDocument` built from HTML, so a repair is tested once for both consumers.

## 5. One home

| Concept | Home |
|---|---|
| Surface, output, lock identifiers | `id` |
| Geometry units and rectangles | `geometry`, `region` (`Px`, `Rect`) |
| Fractional scale (120ths), buffer size | `scale` |
| Layer surface config and wire mappings | `layer` |
| Toplevel config, state, wire mappings | `toplevel` |
| Popup config, positioner params, refit, wire mappings | `popup` |
| Popup anchor rect and size the config does not carry | `placement` |
| Client-side decoration geometry, resize band | `csd` |
| Region spec to strips, change tracking | `resolve` |
| Blur spec | `blur` |
| Frame state machine (Idle, Dirty, InFlight, watchdog) | `frame` |
| Frame pacing, frame hook | `tick` |
| Timer requests (watchdog, repeat) | `timers` |
| Move/resize serial rule | `grab` |
| Popup lifting a parent layer's keyboard mode | `popup_keyboard` |
| Session lock machine, lock config, lock effects | `lock/machine`, `lock/config`, `ctx/lock` |
| Drag source data, offers, outcomes | `drag/*` (pure), `handlers/drag_source`, `ctx/drag` |
| Pipes other clients read from us (a drag's payload, our selection), their limits | `drag/pipe` (bookkeeping), `handlers/pipe_writes` (the queue), `ctx/pipe` (the loop sources) |
| Reading the selection another client offers, its limits | `paste` (bookkeeping), `clipboard` (the offer), `ctx/paste` (the loop source) |
| Bounds on sizes, scales and counts from the compositor, peers and the GPU | `scale`, and FINDINGS F84's table |
| Removing a surface and its per-surface protocol objects | `HostState::remove_surface` (`handlers/surface_objects`) |
| Wayland pointer to Blitz pointer events | `input/pointer` |
| Keysym and keycode tables | `input/keysym`, `input/keycode` |
| Key event assembly, key repeat | `input/key`, `handlers/keyboard` |
| Seat capability decision | `input/capability` |
| Input modality (keyboard vs pointer) | `modality` |
| Palmrest socket client (pointer, session lock) | `input/palmrest` (socket), `input/palmrest/route` (meaning); the lock is told from `ctx/lock` |
| Palmrest messages | `palmrest-protocol` |
| Scroll physics, latching, document side, driver | `blitz_kit::scroll` (shared with `ds-blitz`) |
| Scroll settings (engine part) | `blitz_kit::scroll::config::ScrollSettings`; `scroll/config` holds `ScrollConfig` (it and the thumb's settings) |
| Scroll thumb, palmrest and Wayland axis input, double-scroll rule | `scroll/{indicator,source,suppress,paint}`, `scroll/surface` (the surface's driver, thumb and palmrest tracker) |
| What a component hears of a wheel or touchpad (source, phase, distance) | `scroll/wheel` (the meaning), `scroll/wheel_feed` (the queue), `use_wheel` |
| Scroll view of Blitz (scroller chain, raw offsets) | `dom/scroll` |
| Hover repair, net provider, fonts, adapter ranking, transform-aware hit and rect, pixel snap | `blitz-kit` |
| One document per surface | `dom/document` |
| The viewport's colour scheme follows the root's; a switch rebuilds every box (F81) | `dom/scheme` |
| Shell provider (redraw flag, cursor, IME, clipboard hooks) | `dom/shell_provider` |
| Head elements (`document::Style`, `Link`, `Title`) | `dom/head` |
| GPU device, renderer, image cache | `gpu/context`, `gpu/image_cache` (when to empty it) |
| A lost GPU device, and how often it is rebuilt | `ctx/paint/recover`, `gpu/rebuild` |
| A lock surface that failed, and when to remake it | `lock/retry`, `ctx/lock` |
| Presenter (Wgpu, Shm), swapchain, raw handle (`unsafe`) | `gpu/presenter`, `gpu/swapchain`, `gpu/raw_handle` |
| Flush order | `ctx/flush::ORDER` |
| Surface creation per role | `ctx/spawn/{layer,toplevel,popup}` |
| Requests from handles, applied after a poll | `msg::Request`, `ctx/ops` |
| Regions sent to the compositor | `ctx/paint/regions` (only place) |
| Frame statistics | `stats` (`HostCtx::stats` is the one source) |
| Debug commands, synthetic input | `inject` (feature `debug-inject`) |
| Delegate macros | `state.rs` (only file naming them) |
| Compositor identity | `compositor_kind` |
| The compositor's GPU (dev_t to PCI ids) and the adapter following it | `render_gpu` (pure), `handlers/render_device` (the event) |
| `unsafe` | `gpu/raw_handle.rs` |
| Ambient inputs (environment, clock, directories) | `HostConfig` and `HostClock` values passed in; no other module reads `std::env` |

## 6. Traits

| Trait | Signature | Implementors |
|---|---|---|
| `Flush` (`ctx/flush.rs`) | `fn poll_woken(&mut self); fn apply_requests(&mut self); fn step_frames(&mut self); fn arm_timers(&mut self); fn flush_connection(&mut self) -> Result<()>` | `HostCtx<M>`, the recording fake in `tests/core.rs` |

`Flush` is the only trait the crate defines. sctk's handler traits (`CompositorHandler`,
`LayerShellHandler`, `KeyboardHandler`, ...) and wayland-client's `Dispatch` are implemented on
`HostState`, one impl per file in `handlers/`; `NetProvider` and `ShellProvider` are Blitz's
traits, implemented in `blitz-kit::net` and `dom/shell_provider`.

Add an implementor of `Flush` only for another loop driver. Add a new trait only when two real
implementations are swapped at a boundary.

**Closed enums that stay enums:** `Role` (`Layer`, `Popup`, `Toplevel`, `DragIcon`, `Lock`),
`Presenter` (`Wgpu`, `Shm`), `Backend`, `HostEvent`, `HostMsg`, `Request`, `SurfaceOp`, `Inject`,
`RegionSpec`, `BlurSpec`, `Prepaint`, `ScrollIn`, `ScrollOut`, `Engine`, `FrameInput`,
`TimerRequest`.

### Optional `quire_*` globals

A `quire_*` protocol is bound only where the compositor advertises it (`Option<..>` on
`HostState`, bound with `.ok()`), and every request that needs it is a silent no-op without it,
so the same binary runs on KWin, wlroots and casement. `handlers/quire_window` binds
`quire_window_manager_v1` at version 1 or 2 and sends `request_placement` (`decide.rs` maps a
`WindowPlacement` and the seat's latest input serial to the wire request, or refuses with no
serial); it keeps the manager bound because version 2 adds the manager's `quit` event, which
`quit.rs` turns into `HostEvent::QuitAsked` (the person asked the whole app to quit; a version 1
compositor never sends it, and the app sees only `SurfaceClosed`);
`SurfaceHandle::placement_support` tells a menu whether to show the choices. The shell's own
`quire_*` objects (`quire_shell_v1`, on the connection it was handed) are created on
`HostCtx::wl_surface` / `SurfaceHandle::wl_surface` after `HostEvent::SurfaceMapped`; a hidden
surface that is shown again gets a new `wl_surface`, and the handle follows it.

### The compositor-effects seam

There is no trait over compositor effects. A fake compositor would be a second implementation
of nothing the handlers decide, and the headless KWin already is the second implementation of
the compositor for the acceptance scripts. Handlers become table-testable by structure:

1. A handler body only reads the protocol event, calls a pure function in the concept's module
   (`decide.rs` or the L1/L2 module), and pushes the result onto the queues `HostState` already
   has: `events` (`HostEvent`), `timers` (`TimerRequest`), the request queue, the
   `FrameInput` of `frame::step`.
2. A decision that today sits inside a handler body moves to `decide.rs` next to it and returns
   a value: `handlers/xdg` (configure to `ToplevelState` and resize), `handlers/layer`
   (configure to size and invalidate), `handlers/pointer` and `handlers/band` (which zone, which
   cursor, which press), `handlers/keyboard` (repeat and focus departure), `handlers/data_device`
   (offer to selection change).
3. The table tests assert on those values. If a decision needs a Wayland object to be made,
   the object's facts (a serial, a size, a capability) are passed as values instead.

## 7. Recipes

### Add a Wayland protocol handler

1. `handlers/<protocol>.rs`: the sctk handler trait impl on `HostState`, or a hand
   `Dispatch<Interface, ()>` for a protocol sctk does not wrap. One protocol per file.
2. `handlers/<protocol>/decide.rs` (make the file a directory once it has one): every decision
   as a pure function returning a value; body of the handler stays under about 15 lines.
3. `handlers/mod.rs`: `mod <protocol>;`.
4. `state.rs`: the `delegate_*!` line (the only file naming them), the bound global as an
   `Option<..>` field of `HostState` (drop order: after the surfaces), and its bind in
   `HostState::bind`.
5. Surface the effect to shells only through a `HostEvent` variant (`event.rs`); a new public
   name joins the curated root list in section 3.
6. Test: a table test of `decide.rs`; a `FINDINGS.md` entry with the trace that shows the
   compositor's behaviour; an `accept-*.sh` item if the behaviour needs a live compositor.

### Add a surface role or a config option

1. Config data and its wire mapping: the role's L1 module (`layer.rs`, `toplevel.rs`,
   `popup/config.rs`); a closed choice is an enum, never a `bool`; a new default that the
   design marks "proposed" is a settings key that the consumer passes in.
2. `ctx/spawn/<role>.rs`: send the request when the role is built (unmapped); a change after
   mapping is a `SurfaceOp` variant (`msg.rs`) applied in `ctx/ops.rs`.
3. `handle/surface.rs`: the `SurfaceHandle` setter for the op.
4. `lib.rs`: the new public names, in the section 3 group for the role.
5. A new role kind: `role.rs` variant, `ctx/spawn/<role>.rs`, a `handlers/<role>.rs` for its
   configure and close, and the match arms the compiler lists (never a wildcard).
6. Tests: table test of the config's wire mapping; a spawn/hook test in `handle/tests.rs`
   through the headless harness (section 8); a demo (below) when the option is visible.

### Add a host hook

1. State shared with documents: a `Live<T>` (`live.rs`) published by the host in the place the
   value changes (`ctx/rescale.rs` shows the pattern), read by the document.
2. `handle/hooks.rs`: `use_<name>() -> ReadSignal<T>`; a hook reads the `SurfaceHandle` from the
   root context and subscribes; no logic beyond that.
3. `handle/mod.rs` and `lib.rs`: export the hook; `README.md` "Consuming" section gets one
   example only if the hook needs an explanation.
4. Test in `handle/tests.rs`: render, publish a value, render again, assert the text changed
   (the difference the publish makes, not a state it is compatible with).

### Add an injectable debug command

1. `inject/request.rs`: the `Inject` variant (with typed fields) and, when it returns data, an
   `Injected` variant.
2. `inject/deliver.rs`: deliver through the same `HostState` method real input reaches
   (`deliver`, `key_down`, ...), never around it.
3. `inject/command.rs`: the `DebugAction` variant and its parse arm in `parse_action`; a
   malformed line is a `CommandError`, never a panic.
4. `inject/stdin.rs`: the mapping from `DebugAction` to `Inject` and the JSON reply line.
5. Table test of `parse_command` (one row per verb, one per malformed form) and a delivery test
   in `tests/inject.rs` through the headless harness. The feature is `debug-inject`; nothing
   here compiles without it.

### Add a demo and its accept script

1. `examples/demo_<thing>.rs`, `required-features = ["debug-inject"]` in `Cargo.toml`; built
   only on the public root API and `inject`; shared parts in `examples/shared/`.
2. `scripts/dev/accept-<thing>.sh`, using `scripts/dev/lib` functions for compositor, input and
   pixels (no redefinition); private session bus, scratch `HOME`/`XDG_*`; flags for variants
   (`--real-input`), not a second script.
3. A row in `scripts/dev/README.md` and, for the render items, an entry in `accept-demo.sh`.
4. Live acceptance runs on the headless KWin or a nested casement, never on the person's
   session.

## 8. Test harness

| Kind of thing | Test |
|---|---|
| Pure modules (L0-L2) | table tests beside the code (`tests.rs`), one `const CASES` per function |
| Flush order, op queueing, visibility | `tests/core.rs` against the recording `Flush` fake |
| Handler decisions | table tests of `decide.rs` |
| Hooks and documents | `handle/tests.rs` and `dom/*/tests.rs`: a real `SurfaceDocument` built from a component, driven with published values and `poll`, no compositor |
| Rendering | `headless` feature (`render_png` through vello_cpu) and the golden PNGs in `tests/snapshots/` |
| Blitz repairs | in `blitz-kit`, against a `BaseDocument` |
| Synthetic input on a live host | `HostCtx::inject` on the headless KWin through the demos and `accept-*.sh` |
| Palmrest messages | `palmrest-protocol` round-trip tests; `input/palmrest/route` table tests; `input/palmrest` fake-daemon tests (pointer and session lock sent on change and on connect) |

Integration tests in `tests/` use only the public root, `headless` and `inject`; a test that
needs an internal module is a unit test beside that module. The one harness is the headless
document plus the `Flush` fake; no test connects to a real Wayland display or to the person's
session. Goldens change only with a reason in the commit message.

## 9. Repo rules

- **Effects stay out of the pure layers.** `frame::step` returns the actions to take rather
  than arming a timer, and handlers push `TimerRequest`s rather than touching the loop.
- **`unsafe` lives only in `gpu/raw_handle.rs`** (the raw window handle).
- **Errors:** `HostError`, each variant answering a `Recovery` (`Retry`, `Fallback`, `Degrade`,
  `Fatal`). A phase of the flush that finds the host must stop but returns nothing (the frame
  step) sets `HostCtx::failed`; the last phase returns it (`HostError::GaveUp`: the GPU device
  keeps being lost, the only lock surface keeps failing).
- **Limits are named constants with their reason beside them** (FINDINGS F84 lists them). A
  number from the compositor is clamped where it enters (`scale::wire_extent`,
  `Scale::from_wire`); a count a peer can raise (pipes, reads, datagrams, files) has a cap and a
  deadline; nothing waits on a peer in the loop thread, and a peer that exceeds a limit is cut.
  A wgpu error never panics: devices come from `request_watched_device`.
- **No `bool` in a field, parameter or message, internal ones included.** A wire `bool` from
  sctk or xkb becomes an enum at the handler (`Modifier::of(Held)`, `LockKey::of(Locked)`,
  `RedrawRequest`, `Tile`, `Elastic`); no helper takes `on: bool` or `opted: bool`.
- **`Serialize, Deserialize` only where the type is data all the way down** (FINDINGS F11);
  the palmrest wire types are `palmrest-protocol`'s and are not re-declared.
- **Compositor behaviour is cited with a trace.** Before branching on protocol text, use what
  the protocol already said: an event, a global's presence, an error code.
- **Nothing reads the environment except `Host::connect`** (through `HostConfig` and the
  Wayland connection it makes); every other module gets values.
- **Live acceptance** runs on the headless KWin (`scripts/dev/kwin-virtual.sh`) or a nested
  casement, never on the person's session (`wayland-0`).
- **Gate:**

  ```bash
  cargo fmt --all --check
  cargo clippy --workspace --all-targets --all-features -- -D warnings
  cargo test --workspace --all-features
  ./scripts/check-boundary.sh
  cargo deny check licenses
  ```

## 10. Migration order

Each step keeps master green (the gate above), and callers in sill and quire change in the same
step as an API they use.

1. `blitz-kit` repo (done): create it with `hover`, `net`, `fonts`, `paint_rect`, `hit` and their tests
   moved from shell-host unchanged in behaviour; add `adapter` and `snap`. Gate in the kit.
2. shell-host depends on `blitz-kit` (done); delete `dom/hover_sync.rs`, `dom/net.rs`, `dom/fonts.rs`,
   `dom/painted.rs`, the ranking half of `gpu/adapter.rs`; `tests/hover_sync.rs` and
   `tests/host_fonts.rs` move to the kit.
3. `palmrest-protocol` exists (palmrest step 1) (done); shell-host depends on it and deletes
   `input/palmrest/wire.rs`; `tests/scroll_palmrest.rs` reads the crate's fixtures. Update
   `check-boundary.sh`.
4. Bools to enums (done): `Modifier::of`, `LockKey::of`, `pick`, `elastic`, `requested`,
   `set_ime_enabled`.
5. Splits by concept (done) (section 2 table), one file per commit, tests moving with their code:
   `ctx/spawn` first, then `debug`, `popup`, `handle`, `input/pointer`, `dom/document`,
   `scroll/indicator`, `ctx/paint`, `resolve`, `csd`, `scroll/engine`, `inject`.
6. `decide.rs` extractions (done) for the handlers listed in section 6, each with its table test.
7. Add `Inject::Key`, `KeyStroke`, `KeyAction` and the `key` stdin verb (done); move `debug` command
   parsing into `inject` behind `debug-inject`; sill's keyboard stand-in scripts switch to it.
8. Public-name renames (section 3) and the curated root (done): make every module private, list the
   root exports, move `tests/*` that reach internals in-crate. Update sill and the demos in the
   same change; sill's `debug`, `inject::stdin`, `scale`, `popup::offset_rect`,
   `scroll::config` paths become root or `inject` paths.
9. Fold `DESIGN.md` (done) into module docs and this file's tables; delete it.

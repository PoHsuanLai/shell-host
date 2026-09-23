# Design map

Which part of the design each module implements. The program plan (section "Design:
`shell-host` repo"), `SPIKE.md` and quire's `design/10`-`13` are canonical; this file only says
where in the code a paragraph lives, so a reviewer can go from one to the other. Paths are under
`crates/shell-host/src/`. "Pure" modules are complete and table-tested since the H1 freeze;
everything else is a frozen signature with `todo!()` bodies until its H2 owner fills it
(owners: `ORCHESTRATION.md`, "Waves", and the H1 report).

## Public API (plan "Public API", amended by SPIKE.md S1 "API changes")

| Module | Implements | State |
| --- | --- | --- |
| `host.rs` | `Host<M>`: `connect`, `handle`, `run`; plan "Event loop" (calloop, `run(None)`, tokio runtime entered) | stub |
| `ctx.rs` | `HostCtx<M>`: `outputs`, `spawn_layer`, `spawn_toplevel`, `close`, `set_visibility`, `apply`, `activation_token`, `stats`, `set_color_scheme`, `exit`; the flush order of plan "Event loop" | stub |
| `host_handle.rs` | `HostHandle<M>`: `send`, `apply`, `close`, `exit` from any thread | stub |
| `handle.rs` | `SurfaceHandle` + `use_surface`; plan "Ops queue while a doc is borrowed, apply after `poll()`, before paint" | stub |
| `bridge.rs` | `use_watch` (plan: shared data on `tokio::sync::watch`) | stub |
| `root.rs` | `Root::new/with_props/context` | stub |
| `event.rs` | `HostEvent<M>`, `CloseReason`, `Focus`, `widen` | pure |
| `msg.rs` | `SurfaceOp`, `Visibility`, `HostMsg<M>`, `Request` | pure (data) |
| `config.rs` | `HostConfig{backend, adapter}`, `Backend`, `AdapterPref` (S1: adapter validated by first present) | pure |
| `error.rs` | `HostError`, `Recovery::{Retry,Fallback,Degrade,Fatal}` (plan "Conventions substitutions") | pure |
| `id.rs` | `SurfaceId`, `OutputId`, `SurfaceIds` | pure |
| `output.rs` | `OutputInfo` | pure (data) |

## Surfaces and geometry

| Module | Implements | State |
| --- | --- | --- |
| `layer.rs` | `LayerConfig`, `Layer` (incl. `Bottom`, S1 item c), `Anchor`, `Extent`/`LayerSize`, `ExclusiveZone`, `KeyboardMode`, `OutputTarget`; wire mappings. design/13 §13.3.8 (bar and dock never focus) | pure |
| `toplevel.rs` | `ToplevelConfig`, `Decorations`, `Activation`; design/20 "All apps are xdg toplevels" | pure |
| `popup.rs` | S2: `PopupConfig`, positioner params and wire values; design/13 §13.3.2-13.3.4, §13.7 | S2, pure |
| `placement.rs` | `PopupPlacement` (anchor rect and size S2's config does not carry, FINDINGS F4) | pure (data) |
| `region.rs` | S2: `RegionSpec`, `rounded_strips`; design/10 §10.3.12 | S2, pure |
| `resolve.rs` | `RegionSpec` -> strips after layout; design/10 §10.3.12 "recomputed on every frame whose pill rect changed" | pure |
| `geometry.rs` | `Point`, `Margin`, `enclosing_rect`, `inflate`, `element_id` (FINDINGS F3) | pure |
| `blur.rs` | `BlurSpec`; plan "blur (BlurSpec{None,Region})", quire 03-COLOR §17.1 (region is shell-host's) | pure (data) |
| `scale.rs` | S2: `Scale` in 120ths, `buffer_size`; plan "Scale" | S2, pure |
| `surface.rs` | `HostSurface` (field order = drop order, S1 finding 6) | stub |
| `role.rs` | `Role::{Layer,Popup,Toplevel}`; plan "Opening a grabbing popup from a `KeyboardMode::None` layer switches it to OnDemand" | stub |

## Frames, time, stats

| Module | Implements | State |
| --- | --- | --- |
| `frame.rs` | plan "Frame machine" (Idle/Dirty/InFlight/InFlightDirty, watchdog 50..1000 ms); S1 findings 3, 4; cosmic-comp #2777; design/10 §10.7 "the watchdog applies" | pure |
| `timers.rs` | `TimerRequest`: watchdog and key-repeat timers asked for by handlers, armed by `HostCtx` | pure (data) |
| `clock.rs` | `HostClock` (plan "one HostClock for `resolve(t)`") | pure |
| `debug.rs` | `FrameStats` (work time apart from interval, S1 "API changes"), `StatsWindow`, `percentiles`, `vm_rss_kib`, per-surface `DebugCommand` grammar | pure |
| `inject.rs` | feature `debug-inject`: stdin commands, JSON stats, `capture` | stub |

## Documents (`dom/`)

| Module | Implements | State |
| --- | --- | --- |
| `dom/document.rs` | `SurfaceDocument`; `UA_STYLESHEET` (S1 finding 2); `StyleThreading::Sequential`; element rects for `RegionSpec::Element` and `PopupAnchor::Element`; `data-first-mouse` lookup (design/13 §13.3.8) | stub (constant final) |
| `dom/head.rs` | `HeadDocument` shim (plan: `DioxusDocument` ignores `document::Style/Link/Title`) | stub |
| `dom/shell_provider.rs` | `ShellProvider`: redraw flag (cleared after present, S1 finding 4), cursor, IME, clipboard | stub |
| `dom/fonts.rs` | one `FontContext` cloned into every document (plan "Fonts") | stub |
| `dom/net.rs` | data: URI `NetProvider` (quire FINDINGS S7, S8) | stub |
| `wake.rs` | per-surface wakers on one shared ping (plan "Event loop") | stub |

## GPU (`gpu/`)

| Module | Implements | State |
| --- | --- | --- |
| `gpu/adapter.rs` | adapter ranking and validation by first present on a throwaway connection (S1: RADV dmabuf error 7) | stub |
| `gpu/context.rs` | one device, one `vello_hybrid::Renderer` + `Resources`, the per-device image cache (S1 findings 5, item g), `devices_created` | stub |
| `gpu/context_proxy.rs` | custom-widget textures across frames (S3 item d) | stub |
| `gpu/raw_handle.rs` | the one `unsafe` module: surface from raw handles, `WaylandDisplay: HasDisplayHandle` (S1 "API changes") | stub |
| `gpu/presenter.rs` | `Presenter::{Wgpu, Shm}` (plan "presenter{Wgpu,Shm}"), `capture` | stub |
| `gpu/swapchain.rs` | Mailbox then Fifo; PreMultiplied, PostMultiplied, else opaque and blur off (plan "GPU") | stub |
| `gpu/shm.rs` | CPU `wl_shm` fallback (plan "shm/vello_cpu presenter fallback") | stub |
| `gpu/render.rs` | resolve, paint, render (S1 finding 3 ordering) | stub |
| `headless.rs` | S3: `render_png` via vello_cpu (plan "Headless") | S3, real |

## Input (`input/`, handlers)

| Module | Implements | State |
| --- | --- | --- |
| `input/pointer.rs` | plan "Input": buttons 272..276, move before down, leave at (-1,-1), value120 -> Lines / continuous -> Pixels; first-mouse window (design/13 §13.3.8, setting `menus.first_mouse_window_ms`) | pure |
| `input/keysym.rs` | xkb keysym -> `Key`, `Location` (winit table, Apache-2.0, FINDINGS F5) | pure |
| `input/keycode.rs` | evdev -> `Code` (winit table, Apache-2.0) | pure |
| `input/modifiers.rs` | sctk modifiers -> `Modifiers` | pure |
| `input/key.rs` | `BlitzKeyEvent` assembly | stub |
| `ime.rs` | text-input-v3 buffered until `done` (plan "IME") | stub |
| `clipboard.rs` | own-selection cache, pipe read with 200 ms timeout (plan "Clipboard") | stub |
| `cursor.rs` | `wp_cursor_shape_v1` | stub |

## Wayland plumbing

| Module | Implements | State |
| --- | --- | --- |
| `state.rs` | `HostState`, the delegate macro list (the only file naming sctk delegate macros), `Drop` (S1 finding 6) | fields, delegates and `Drop` final; the rest stub |
| `handlers/compositor.rs` | frame callbacks -> `FrameInput::Callback`; integer scale fallback | stub |
| `handlers/output.rs` | `HostEvent::Output*` | stub |
| `handlers/layer.rs` | layer configure/closed | stub |
| `handlers/xdg.rs` | toplevel and popup configure/close; `popup_done` (not client-drivable, S2) | stub |
| `handlers/seat.rs` | capabilities, `SeatInput` | stub |
| `handlers/pointer.rs`, `keyboard.rs` | input dispatch | stub |
| `handlers/text_input.rs` | hand `Dispatch` for `zwp_text_input_v3` (plan: SCTK has no client-side helper) | stub |
| `handlers/data_device.rs` | clipboard offers and sources | stub |
| `handlers/activation.rs` | xdg-activation tokens | stub |
| `handlers/background_effect.rs`, `fractional.rs` | S2: blur and fractional-scale + viewporter `Dispatch` | S2, real |

## Design docs 10-13 not placed in H1

| Design paragraph | Where it will live | Why not now |
| --- | --- | --- |
| design/11 §11.3.1, §11.4, §11.7: scroll engine, latching, rubber band, keyboard scroll, overlay thumb | `scroll/{engine,latch,rubber,keys,indicator}.rs` (design/11 §11.7) | not in the plan's H1/H2 lists; a later scroll wave adds the modules. `input/pointer.rs` forwards wheels to Blitz until then (FINDINGS F7) |
| design/12 §12.7, design/11 §11.7: palmrest client | `input/palmrest.rs` | same wave as scroll |
| design/10 (dock machine, `magnify`), design/13 menus, switcher, launcher | sill (`sill-surfaces`), quire (`overlay/menu_track.rs`) | not shell-host's; shell-host supplies `layer`, `resolve`, `blur`, `popup`, `frame` |
| design/13 §13.7 hover-switch between menus | `SurfaceOp::Reposition` (`xdg_popup.reposition`) | in H1 as a signature |

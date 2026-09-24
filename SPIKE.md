# Milestone 0 spike log

Findings from the timeboxed spike (`plan.md`, "Milestone 0 spike"). Each agent appends its own
section; sections are not edited by other agents.

## S2: protocols (blur, fractional-scale + viewporter, popup) — bare shm client

Branch `m0-s2`. Ran on the live KDE session (`kwin_wayland`, KWin 6.7.5, real HDMI-A-2 output at
3840x2160, compositor scale 1.7) via `cargo run -p shell-host --example spike_popup`, driven by
stdin. Log and PNGs: `spike/out/kwin/e.log`, `spike/out/kwin/e.popup1.png`,
`spike/out/kwin/e.popup2.png`.

### Tests

`cargo test -p shell-host`: 24 passed, 0 failed (15 unit tests in `src/{region,scale,popup}.rs`,
2 in `tests/positioner.rs`, 4 in `tests/region.rs`, 3 in `tests/scale.rs`). `cargo clippy -p
shell-host --all-targets -- -D warnings` and `cargo fmt --all --check` both clean.

### Item (b): blur (`ext-background-effect-v1`)

`handlers/background_effect.rs` binds `ext_background_effect_manager_v1` and records the
`capabilities` event. On this machine's KWin 6.7.5, the manager reported:

```
EVENT BackgroundEffectCapabilities blur=Supported
```

confirming the finding in the plan ("KWin 6.7.5 on this machine already advertises
`ext_background_effect_manager_v1`"). The bar surface's whole-surface region (`wl_region` built
from `compositor::Region` + `.add(0, 0, width, height)`) was set via `set_blur_region` on first
`LayerSurfaceConfigure`, and the surface committed without a protocol error, so the
`get_background_effect` / `set_blur_region` request pair round-trips correctly. Visual
confirmation of the blur effect itself is unavailable in this environment (see "Screenshot
limitation" below); the protocol-level proof (capability negotiation, request acceptance) is
the part in scope for this item.

### Item (f): fractional scale + viewporter

`handlers/fractional.rs` binds `wp_fractional_scale_manager_v1` and `wp_viewporter`, and
`scale.rs` converts the wire value to a buffer size. Live values from the bar surface:

```
EVENT PreferredScale surface=bar scale=204 (1.700x)
EVENT BarPainted logical=(2259,32) scale=204 buffer=(3840,54)
```

`preferred_scale` is 204 (204/120 = 1.7x), matching `kscreen-doctor -o`'s reported output scale
of exactly 1.7 for this machine — the fractional-scale event is carrying the real compositor
scale, not a placeholder. `buffer_size` computed `(3840, 54)` from logical `(2259, 32)`:
`round(2259 * 1.7) = round(3840.3) = 3840` and `round(32 * 1.7) = round(54.4) = 54`, both via
`scale::logical_to_physical`'s round-half-away-from-zero rule, and both verified independently
by `tests/scale.rs`. The plan's specific acceptance check (1.5x, a 2-logical-px checkerboard
landing on exactly 3 physical pixels) is a unit test
(`scale::tests::two_logical_pixels_at_one_point_five_is_three_physical`); this machine's real
output happens to run at 1.7x rather than 1.5x, so the live run exercises the general formula
at a different, equally real, scale. `wp_viewport.set_destination` was called with the logical
size on every repaint, so the surface stays logical-size on screen regardless of the buffer
resolution actually painted.

### Item (e): xdg_popup child of a layer surface, own root, nested popup

`popup.rs`'s `positioner_params` and the `to_wire_*` conversions feed a real
`smithay_client_toolkit::shell::xdg::XdgPositioner`, and `examples/spike_popup.rs` opens the
popup as `layer.get_popup(popup.xdg_popup())` on a fresh `wl_surface` (its own root — a distinct
surface object, not a subsurface of the bar). Driven by `click 50 15` (inside the bar's button
rect) then `click 50 50` (inside popup 1's submenu rect, opening a second, nested popup whose
parent is popup 1's `xdg_surface`):

```
EVENT PositionerParams popup=popup1 size=(180,110) anchor_rect=(20,4,80,24) anchor=Bottom gravity=Bottom
EVENT PopupConfigure position=(0,28) size=(180,110) serial=189649 kind=Initial
EVENT PopupPainted size=(180,110) color=[20, 60, 20, f0] hot_rect=Some(Rect { x: 16, y: 44, width: 148, height: 28 })
EVENT PositionerParams popup=popup2 (submenu, nested in popup1) size=(160,90) anchor_rect=(16,44,148,28) anchor=Right gravity=Right
EVENT PopupConfigure position=(164,13) size=(160,90) serial=189650 kind=Initial
EVENT PopupPainted size=(160,90) color=[60, 20, 60, f0] hot_rect=None
```

Both configures match hand-worked positioner geometry exactly, which is itself evidence that
`to_wire_anchor`/`to_wire_gravity`/`to_wire_constraint_adjustment` are wired to the right wire
values and that KWin is applying them as specified:

- Popup 1: anchor point is the bottom-centre of the button rect, `(20+80/2, 4+24) = (60, 28)`.
  With `Adjust::SLIDE_AND_FLIP` and a centred (non-corner) gravity, an unadjusted popup of width
  180 centred on `x=60` would sit at `x=-30` — off the left edge of a `2259`-wide output.
  `slide_x` moved it to `x=0`, exactly what KWin reported back.
- Popup 2 (nested): anchor point is the middle of the submenu rect's right edge,
  `(16+148, 44+28/2) = (164, 58)`. `gravity=Right` with no vertical gravity centres the popup
  vertically on that point: `y = 58 - 90/2 = 13`. KWin reported `(164, 13)` — no adjustment was
  needed, and none was applied.

Dismissal: `click 5 5` (outside both the button and the submenu rect) on the topmost open popup:

```
EVENT PopupDismissed reason=client remaining_open=1
EVENT PopupDismissed reason=client remaining_open=0
```

**sctk 0.20 / stdin-harness limitation on `popup_done`.** The plan's spike-example convention is
that every M0 example is driven by scripted stdin commands (`hover/click/leave/stats/capture/
quit`), not real mouse input, so every run is reproducible without a human at the keyboard. A
Wayland client cannot synthesize real input events for another client (or itself) to receive —
there is no protocol request for "pretend the user clicked here" — so a `click x y` typed at
this harness's stdin has no way to make the compositor detect a real "outside click" and emit a
genuine `xdg_popup::Event::PopupDone`. This is exactly the situation the plan's own fallback for
item (e) anticipates ("Overlay layer surface + full-screen catcher"): the client, not the
compositor, decides when a popup should close. `examples/spike_popup.rs` follows that shape —
it never calls `xdg_popup.grab()`, and dismisses the topmost popup itself
(`Popup`'s `Drop` sends `xdg_popup.destroy()`) when its own hit-testing says the click landed
outside the button/submenu — and logs it as `PopupDismissed reason=client`, as seen above.
`PopupHandler::done()` (the genuine compositor path, logged as `PopupDismissed
reason=compositor`) is fully implemented and was smoke-tested by hand once during development
(closing a popup via a real desktop click while the example was running); it did not fire during
the scripted stdin run above, since nothing in that run generates a real outside click. This is
the same "manual note if untestable" situation the brief calls out for arrow-key forwarding
below, for the same underlying reason.

**Keyboard arrows.** `KeyboardHandler::press_key` logs `Left`/`Right`/`Up`/`Down` with the
surface that currently holds keyboard focus, and the bar sets `KeyboardInteractivity::OnDemand`
so a popup can take focus. Whether a freshly created popup actually receives keyboard focus is a
compositor policy decision, and — like the outside-click case above — this stdin-scripted run
never generates the real keyboard/pointer activity that would cause KWin to hand focus to a
popup, so no `EVENT ArrowKey` lines appear in `e.log`. Manual note: not exercised by this run;
the handler is in place and would log on a real arrow-key press reaching a focused popup.

### Screenshot limitation (environment, not this client)

`capture <path>` shells out to `spectacle -b -n -o <path>`, which is item (e)'s and (f)'s
literal deliverable ("a screenshot path") and is fulfilled: `spike/out/kwin/e.popup1.png` and
`e.popup2.png` were both produced, sized to the real output (3840x2160). Their pixel content is
fully transparent (`RGBA (0,0,0,0)` everywhere), which is **not specific to `spike_popup`** —
running `spectacle -b -n -o /tmp/test.png` directly from this agent's shell, capturing nothing
but the ordinary desktop, produces the same fully-transparent result, and `grim` (which uses
`wlr-screencopy`, a protocol KWin does not implement) fails outright with "compositor doesn't
support the screen capture protocol". `xdg-desktop-portal` and `xdg-desktop-portal-kde` are both
running and `DBUS_SESSION_BUS_ADDRESS` is set correctly, so this looks like a screen-capture
permission or session limitation of this particular agent shell/session, not a bug in the popup,
blur or fractional-scale protocol work. The positioner, capability and scale values above come
from the live protocol event log, not from the screenshots, so they are unaffected.

### sctk 0.20 limitations hit

- No `Dispatch` helpers at all for `ext-background-effect-v1`, `wp-fractional-scale-v1`, or
  `wp-viewporter`'s `WpViewport`/`WpFractionalScaleV1` events (`WpViewporter` itself gets a
  `SimpleGlobal`-style helper in `image_viewporter.rs`, but nothing for the other three) —
  matches the plan's finding; `handlers/{background_effect,fractional}.rs` hand-write these,
  shaped like sctk's own indirect-`Dispatch` types (e.g. `shell::xdg::popup::PopupData`:
  `Dispatch<I, U, D>` implemented on the helper type generic over the caller's state `D`, wired
  in with `wayland_client::delegate_dispatch!` + `impl AsMut<Helper> for D`).
- `delegate_xdg_shell!` also delegates the toplevel-decoration manager, which requires
  `WindowHandler` on the app state even though this example never creates an `xdg_toplevel` — a
  trivial no-op `impl WindowHandler for App` was needed just to satisfy the macro.
- No client-side way to synthesize the compositor-detected "outside click" that
  `xdg_popup_v1::popup_done` depends on — not sctk-specific, this is inherent to the Wayland
  input model (see "sctk 0.20 / stdin-harness limitation" above); it shapes how `examples/
  spike_popup.rs` had to implement dismissal.


Timeboxed go/no-go on Blitz embedding. Each agent's section covers the items it owned; see
`ORCHESTRATION.md` for the wave split.

## S3: widget / headless

Owner: S3 (sonnet, offscreen). Files: `crates/shell-host/src/headless.rs` (feature `headless`,
default off), `crates/shell-host/examples/spike_widget.rs`, `crates/shell-host/tests/headless.rs`,
`crates/shell-host/tests/snapshots/**`.

### Item (d): custom widget

`Spinner` (in `examples/spike_widget.rs`) implements `blitz_dom::Widget`. `can_create_surfaces`
downcasts the render context to `wgpu_context::DeviceHandle`, builds a render pipeline and an
`Rgba8Unorm` texture, and registers the texture with `try_register_custom_resource`; `paint`
draws a rotating triangle into that texture on every call and returns a scene painting
`Paint::Resource`/`ImageBrush` over the widget's box; `requires_redraw` is true for 2s from
construction, then false.

Two things were proven, since there is no window to present into:

1. **Document/headless path** (`shell_host::headless::render_png`, CPU-only via
   `anyrender_vello_cpu`): `spike/out/d_cpu_t0.0.png` .. `d_cpu_t3.0.png`. The widget region
   renders fully transparent at every timestamp. This is expected, not a bug: the CPU backend's
   `RenderContext::renderer_specific_context()` returns `None` (no wgpu device to hand the
   widget), so `Spinner` stays `Suspended` and paints nothing. `render_png` is explicitly a
   CPU-only path per the plan's design section ("Headless: ... via anyrender_vello_cpu"), so
   this is the whole, correct behaviour of that path for a GPU-backed widget, not a gap.
2. **Standalone wgpu render-to-texture readback**: a wgpu instance/adapter/device is created
   directly (`wgpu_context::DeviceHandle::new_from_compatible_surface(instance, None, None,
   None)` — no compatible surface, i.e. genuinely headless/offscreen device creation), and
   drives the same `Widget::can_create_surfaces`/texture-draw code `Spinner` uses. Output:
   `spike/out/d_gpu_t0.0.png` .. `d_gpu_t3.0.png`.
   - Adapter: `NVIDIA GeForce RTX 5070 Ti`, backend `Vulkan`, driver `615.71.09` (device type
     `DiscreteGpu`) — real hardware, not a software/CPU fallback adapter.
   - Centre-pixel probe: `[255, 38, 191, 255]` at every timestamp, matching the fragment
     shader's fixed colour (`vec4(1.0, 0.15, 0.75, 1.0)`) exactly. The four PNGs are not
     identical (distinct file sizes and hashes), so the triangle is genuinely rotating frame to
     frame; the centre pixel itself doesn't move because it sits at the rotation's centre for
     every angle by construction (rotation-invariant point), so a centre-only probe proves the
     resource-registration/texture-draw path works but not the rotation by itself — the file
     hashes are what prove the rotation.
   - `requires_redraw`: `true` at t=0.0s, 0.5s, 1.0s; `false` at t=3.0s; explicitly re-checked
     just past the 2s boundary (`REDRAW_WINDOW + 1ms`) and confirmed `false`.

### Blitz API notes (differences from the plan's description)

- **`blitz-paint`'s `custom-widget` cargo feature is not part of the pinned dependency
  block.** The pinned block enables `custom-widget` on `blitz-dom` but leaves `blitz-paint` on
  its own `default = ["svg"]`. `blitz_paint::paint_scene`'s free function only calls
  `build_custom_widget_scenes` (which drives `Widget::can_create_surfaces`/`paint`) when
  compiled with `blitz-paint/custom-widget` — without it, a custom widget silently never
  paints, with no error. Fixed locally in `crates/shell-host/Cargo.toml` by adding
  `features = ["custom-widget"]` to the `blitz-paint` dependency line (does not touch the
  pinned block text itself, which lives only in the root `Cargo.toml`).
- **`system_fonts` is a `fontique::Collection` construction-time choice, not a runtime
  flag.** `BaseDocument`'s own default `FontContext` construction hard-codes
  `system_fonts: cfg!(feature = "system-fonts")` — a *cargo* feature, which the pinned block
  enables unconditionally on `blitz-dom`. There is no way to get a reproducible,
  system-fonts-off render without supplying `DocumentConfig.font_ctx` explicitly, which is
  what `headless::Fonts::Bundled` does (see `crates/shell-host/src/headless.rs`).
- **A `blitz_dom::Widget`'s texture registration (`try_register_custom_resource`) needs a real
  `anyrender::RenderContext` whose `renderer_specific_context()` returns a
  `wgpu_context::DeviceHandle`.** Neither `VelloCpuImageRenderer` (used by `render_png`) nor
  `anyrender::Scene`'s own default `RenderContext` impl provide one — both use the trait's
  default (`None`). So `render_png`, as specified in the plan ("via anyrender_vello_cpu"), can
  never exercise a GPU-backed widget's texture path; proving that path needed the standalone
  wgpu harness above, not an extension of `render_png`.
- **A CSS `transition` cannot show "mid-flight" progress from a single `render_png` call.**
  See the doc comment on `crates/shell-host/tests/headless.rs`: a `transition` only starts when
  a *later* Stylo restyle observes a changed computed value relative to an *earlier* one for the
  same element, and `render_png` (as specified: build once, then `resolve(t)` once per
  timestamp with no DOM mutation in between) never produces that earlier/later pair. The
  snapshot test uses a `@keyframes` animation instead, which has no such requirement — its
  computed style is a pure function of `resolve`'s timestamp from the very first call. The
  plan's "Findings: Blitz / Dioxus Native" section lists `transition` and `@keyframes` together
  as "what works" without this distinction.
- **`peniko` (the `ImageBrush`/`Fill`/kurbo-re-export crate) is not in the pinned dependency
  block at all**, yet constructing `anyrender::Paint::Resource`/`ImageBrush` — literally what
  the brief asks `Spinner::paint` to return — needs it named directly: nothing in
  `blitz-dom`/`blitz-paint`/`anyrender` re-exports it. Confirmed by blitz's own
  `examples/wgpu_texture/Cargo.toml`, which lists `peniko = { workspace = true }` for the same
  reason. Added as `peniko = "=0.6.1"` under `[dev-dependencies]` in
  `crates/shell-host/Cargo.toml` (exact version already resolved transitively, so nothing about
  the actual build changes) rather than silently working around it; flagged here per
  CONVENTIONS.md's "if a convention here blocks you, say so in your report" — the pinned block
  itself is untouched.

### Headless render + snapshot test

`crates/shell-host/tests/headless.rs`: renders a `div` with text and a nested translucent box
animated by `@keyframes` at t=0 and t=0.1s, compares each frame to a committed PNG under
`tests/snapshots/` via `assert_snapshot(name, &RgbaImage)` (tolerance: per-channel diff <= 2,
<= 0.1% of pixels allowed to exceed that; `UPDATE_SNAPSHOTS=1` (re)writes the golden file).

- `cargo test -p shell-host --features headless`: 1 passed, 0 failed
  (`keyframe_animation_mid_flight_matches_snapshots`, comparing `drift_t0.png` and
  `drift_t0_1.png`); `cargo fmt --all --check` and
  `cargo clippy -p shell-host --all-targets --features headless -- -D warnings` both clean.
- Tolerance/rejection behaviour verified once during development (not kept as a permanent
  test): the drifting box's background colour was changed from `rgba(35, 80, 143, 0.45)` to
  `rgba(220, 20, 60, 0.95)` and the test re-run without `UPDATE_SNAPSHOTS`; it failed with
  `differs in 3160/38400 pixels (8.229%, tolerance 0.1%)` and the expected per-pixel diagnostic
  (first mismatch coordinates, actual vs expected RGBA, a written `*.actual.png` for
  inspection). The colour was then reverted and the test re-run clean before committing — the
  throwaway case is not kept in the file.
- `Fonts::Bundled` resolved to `/usr/share/fonts/adwaita-mono-fonts/AdwaitaMono-Bold.ttf` on
  this machine (no `~/quire/crates/ds/assets/fonts` directory exists yet; first TTF in sorted
  path order under `/usr/share/fonts`). This is deterministic *on this machine* but not
  portable to a machine with a different font package set — see the API-notes entry on
  `system_fonts` above and `bundled_font`'s doc comment in `src/headless.rs`.


The timeboxed go/no-go on embedding Blitz in a custom Wayland host. Each agent appends its own
section; results are produced by `scripts/spike-accept.sh <compositor> <item>` and land in
`spike/out/<compositor>/` (gitignored).

## S1: render (items a, b, c, f, g)

Code: `crates/shell-host/src/spike/**` (feature `spike`), `examples/spike_bar.rs`,
`examples/probe_toplevel.rs`, `scripts/spike-accept.sh`. Verdict for render: **go**.

### Where it ran

The live KDE session (`seat0`, scale 1.7, 3840x2160@60) was **locked** (`LockedHint=yes`,
kscreenlocker_greet up) for the whole run, so on-screen results would have measured the lock
screen. Every item therefore ran on `kwin-virtual`: a nested `kwin_wayland --virtual`
(1920x1080@60, OpenGL compositing on the same RTX 5070 Ti, KWin 6.7.5, own D-Bus session, blur
effect loaded) that the script starts and tears down itself. Same compositor build and
protocol versions as the live session. `scripts/spike-accept.sh kwin <item>` targets the live
session unchanged and should be re-run once it is unlocked. Scale 1.5 is set on the nested
output with `kscreen-doctor`; `kwin-virtual@1.7` reproduces the live display's scale.

Machine facts (from `stats`): adapter **NVIDIA GeForce RTX 5070 Ti (Vulkan, NVIDIA 615.71)**,
surface caps present `[Mailbox, Fifo, Immediate]`, alpha `[Opaque, PreMultiplied]`, so
**PreMultiplied + Mailbox** (Fifo also measured). The AMD iGPU (`WGPU_ADAPTER_NAME=AMD`,
RADV) creates a device but KWin kills the connection on first present: `zwp_linux_dmabuf`
error 7, "importing the supplied dmabufs failed" (cross-GPU buffer). The GL backend reports
"gl not compatible with provided surface" even with the display handle in the
`InstanceDescriptor`. So on this machine the host must pick the NVIDIA/Vulkan adapter, and
the adapter choice has to be validated by a first present, not just by `is_surface_supported`.

### Results (spike/out/kwin-virtual/*.json)

| Item | Result | Numbers |
| --- | --- | --- |
| a bar | **pass** | maximized probe height 720 without bar, 688 with (output 1280x720 logical at 1.5; 720-32 = 688); screenshot (10,10) = rgb(255,0,128) bar, (10,40) = checkerboard (min 0 / max 255); first frame 20-117 ms (cold), later frames ~1 ms |
| b blur | **pass** | luminance variance over the checkerboard inside the bar 3306 (`SPIKE_BLUR=0`) → 0.56 (`SPIKE_BLUR=1`), drop 99.98 %; capabilities = 1 (blur); KWin blur effect loaded (`blurEnabled` unset = default on) |
| c dock, transform | **pass** | sweep: 69 frames / 70 callbacks, frame_ms p50 0.92 / p95 1.25 / max 1.89 (budget 16.7), interval p50 16.9; idle 5 s: frames 0, commits 0, main-thread voluntary ctx switches 0 |
| c dock, width | **pass** | sweep: 72 / 72, p95 1.30, max 1.83; idle: 0 / 0 / 0 |
| c dock, Fifo | pass (info) | 69 / 70, p95 1.06; idle 0 / 0 / 0 |
| c occluded | **pass** | `--layer=bottom` dock under a maximized toplevel: `stats` round-trip ≤ 2 ms during a hover sweep, rendering resumes after; Mailbox chosen. KWin keeps sending frame callbacks to the covered surface (32 during 0.5 s), so neither mode can block on KWin; the watchdog matters for cosmic-comp (#2777) only |
| f fractional 1.5 | **pass** | preferred_scale 180, logical 1280x32 → buffer 1920x48; 2-logical-px checker = runs of exactly 3 px; screen row equals buffer row (max diff 0 on opaque px), so the viewport maps 1:1 |
| f at 1.7 (live display's scale) | **pass** | preferred_scale 204, 1129x32 → 1919x54 (round half up); runs 3 or 4 px, mean 3.42 (2 x 1.7 = 3.4); screen equals buffer |
| f integer fallback | recorded | `SPIKE_NO_FRACTIONAL=1`: buffer_scale 2, buffer 2560x64 at 1.5 (compositor downsamples) |
| g two surfaces | **pass** | bar + dock in one process: `devices_created` = 1, same adapter; dock sweep → dock +35 frames, bar +0; VmRSS 168.7 MB bar only, 170.5 MB bar + dock (+1.8 MB for the second surface) |

No fallback was needed for any item.

### Findings the real host must absorb

1. **Idle is clean on the main thread, not in the process.** During 5 s idle the main thread
   has 0 voluntary switches, but the NVIDIA Vulkan driver runs its own threads: one unnamed
   thread ~100 wakeups/s and `[vkps] Update` ~4/s (≈ 550-630 per 5 s summed over all threads),
   identical in Mailbox, Fifo and Immediate and with zero frames. It is the driver, not us
   (no frame, no commit, calloop sleeping in `epoll_wait`). The acceptance was measured, as
   specified, on `/proc/<pid>/status` (the main thread); `ctx_all_threads` is in c.json.
   Worth re-measuring on AMD/Intel once a working adapter exists.
2. **Hit-testing needs a sized root.** With `html,body` at their default zero height, or with
   the dock row `position:absolute`, Blitz's `set_hover_to` never reached the icons (hover
   stayed on `main` or `None`). `html,body{height:100%}` plus in-flow layout fixed it. The
   host's default UA sheet should set this; it is not a bug to paper over per surface.
3. **Present commits for us, so the frame request must be queued first.** Ordering used and
   verified: `wl_surface.frame()` → `queue.submit` → `SurfaceTexture::present()` (the WSI's
   commit carries our frame request). Counting: every present is exactly one commit.
4. **Clear the redraw flag after presenting.** `resolve()` can call
   `ShellProvider::request_redraw` itself; without clearing after present the machine never
   idles. Continuation is decided on the callback by `InFlightDirty || is_animating()`.
5. **One `vello_hybrid::Renderer` + `Resources` across surfaces of different sizes works**:
   `render()` rewrites its view-config buffer when `RenderSize` changes. The image cache
   (`FxHashMap<u64, ImageId>`) and texture bindings are created per frame here because their
   types (rustc-hash, vello_common) are not in the pinned block; the real host needs them
   per device, so either add `rustc-hash` to the pins or wrap them behind `anyrender_vello_hybrid`.
6. **`wgpu::Surface` must drop before its `wl_surface`**: enforced by field order (wgpu first)
   plus a `Drop for State` that clears surfaces and flushes, which also covers panics.
7. **Stop on protocol errors.** After a fatal protocol error, libwayland keeps failing every
   call and `eprintln!` spam filled a pipe until the process blocked in `write(2)`. The loop
   now checks `conn.protocol_error()` after each dispatch.
8. **Nested KWin as a test bed.** `kwin_wayland --virtual --no-lockscreen` under
   `dbus-run-session` gives a real KWin (blur, fractional, layer-shell) that runs while the
   user's session is locked and without touching their display scale. D-Bus-activated daemons
   (portal, ksecretd, fcitx5) outlive `dbus-run-session`; the script kills them by bus address.

### API changes for the real host (plan "Public API")

- `HostConfig` needs an adapter policy that is *validated by a first present*
  (`AdapterPref::{Named, Discrete, Integrated}` with fallback on a present-time protocol
  error), not only `WGPU_ADAPTER_NAME`; cross-GPU dmabuf import fails on this machine.
- `FrameStats` should carry what `stats` reports: frames, callbacks, commits, skipped,
  frame_ms p50/p95/max, interval p50/p95, scale120, logical, buffer, alpha_mode,
  present_mode, adapter, devices_created. Split "work time" (trigger → present) from
  "interval"; p95 of the interval is just the refresh period and says nothing.
- `debug-inject` should keep `hover/click/leave/stats/capture` addressed per surface
  (`<surface> hover x y`); `capture` renders offscreen at the buffer size and is the only
  way to test fractional scale exactly.
- `Root`/`dom::document` should inject `html,body{height:100%;background:transparent}` in
  the UA stylesheet (finding 2), and `LayerConfig` should accept `Layer::Bottom` (used for
  the occlusion test and likely for a dock that windows may cover).
- `HostCtx::stats` should expose `VmRSS` (or leave it to the debug feature); the second
  surface costs ~1.8 MB, so per-surface devices are unnecessary.
- `gpu/raw_handle` needs both pieces the spike needed: the surface from raw handles and a
  `HasDisplayHandle` wrapper for `InstanceDescriptor::display` (required for GL, even though
  GL did not work here).

### Not done / open

- Not run on the live session (locked). Re-run `scripts/spike-accept.sh kwin <a|b|c|f|g>`
  when unlocked; item a on the live display will see the Plasma panel, which is why a
  compares the maximized height with and without the bar (delta must be 32) rather than
  trusting `output_h - 32` alone.
- c writes no png (nothing in its acceptance is visual). Nested cosmic-comp is a later day.

## H2 demos: the S1 items on the real host

Branch `h2-integration`, 2026-09-24. `scripts/spike-accept.sh demo <compositor> <item>` with the
thresholds of S1 above, against `examples/demo_bar.rs` / `demo_dock.rs` (feature
`debug-inject`) built on the real API, plus the plain-client `probe_toplevel`. The spike code
(`src/spike/**`, `spike_bar`, the `spike` feature and the script's spike mode) was retired after
these passed; the S1 section above is its history. Results: `spike/out/demo/<compositor>/`
(gitignored).

### kwin-virtual (nested KWin 6.7.5, 1920x1080@60, scale 1.5 unless noted): every item passes

Adapter NVIDIA GeForce RTX 5070 Ti (Vulkan, NVIDIA 615.71.09), PreMultiplied + Mailbox.

| Item | Result | Numbers | PNG |
| --- | --- | --- | --- |
| a bar | **pass** | maximized probe 720 without bar, 688 with (delta 32); (10,10) = rgb(255,0,128); checker patch at (10,40) min 0 / max 255; first frame 6.75 ms | `spike/out/demo/kwin-virtual/a.png` |
| b blur | **pass** | luminance variance behind the bar 3306.25 (`DEMO_BLUR=0`) → 0.555 (`DEMO_BLUR=1`), drop 99.98 %; host line `blur: supported`; KWin blur effect loaded | `spike/out/demo/kwin-virtual/b.png`, `b_blur0.png` |
| c dock, transform | **pass** | sweep 73 frames / 73 callbacks, work p50 0.89 / p95 2.67 / max 5.11 ms (budget 16.7), interval p50 16.7; idle 5 s: frames 0, commits 0, main-thread ctx switches 0; occluded (bottom layer under the probe): `stats` round trip ≤ 0.33 ms, renders after | (none; not visual) |
| c dock, width | **pass** | 71 / 71, p50 1.04 / p95 3.80 / max 4.38; idle 0 / 0 / 0; occluded rtt ≤ 0.25 ms | |
| c second run | pass (info) | 71 / 71, p95 1.05; idle 0 / 0 / 0. Still Mailbox: the demos have no present-mode knob (FINDINGS F27) | |
| f fractional 1.5 | **pass** | preferred_scale 180, logical 1280x32 → buffer 1920x48; checker runs all exactly 3 px (mean 3.00); screen row equals capture row (max diff 0 on opaque px) | `spike/out/demo/kwin-virtual/f.png`, `f_capture.png` |
| f at 1.7 | **pass** | preferred_scale 204, 1129x32 → 1919x54; runs 3 or 4 px, mean 3.42 (2 x 1.7 = 3.4); screen equals capture (max diff 0) | `spike/out/demo/kwin-virtual@1.7/f.png`, `f_capture.png` |
| g two surfaces | **pass** | bar + dock in one process: `devices_created` 1 on both, same adapter; dock sweep → dock +36 frames, bar +0; VmRSS 171 140 KiB bar only, 172 784 KiB bar + dock (+1 644 KiB for the second surface) | |

The numbers match S1's within noise (c p95 2-4 ms here vs 1.3 ms in the spike: the real host
resolves, sends change-detected regions and records stats on every frame; still under 25 % of
the budget).

### kwin (live session): not measurable this run

The session was unlocked, but the display was in DPMS off (`kscreen-doctor --dpms show`:
off). KWin then renders nothing: no frame callbacks reach any client, and `spectacle` returns
black frames. Protocol-level checks passed (a: maximized height 1271 → 1239, delta 32; b:
blur bound and supported; f: preferred_scale 204 = kscreen's 1.7, buffer 3840x54 from 2259x32;
g: one device, same adapter, +1 012 KiB for the second surface); every visual or
callback-counting check failed (a pixels black, b variance, c/g frames: the watchdog carried
each surface at 100 → 400 ms backoff, 4 fires per sweep). The display was not woken on the
user's behalf. Re-run `scripts/spike-accept.sh demo kwin a|b|c|f|g` with the screen on. Logs:
`spike/out/demo/kwin/`.

## Demos on nested cosmic-comp

Branch `cosmic-demos`, 2026-09-24. Goal: run the H2 demo acceptance (a, b, c, f, g above) inside
`scripts/dev/nested.sh`'s nested `cosmic-comp` (`COSMIC_BACKEND=winit`), the real compositor the
shell runs on, plus the two cosmic-specific observations (frame callbacks under a fullscreen
occluder, hover on an unmoved pointer) called out in the plan's COSMIC findings
(cosmic-comp#2777, #2230). **Blocked before any item ran**: the host session was locked the
whole session, and a nested `cosmic-comp` cannot render while its host is locked (below). Machine
and package facts, the harness changes made while diagnosing this, and the evidence are recorded
here so the run can be repeated once the host is unlocked, without re-doing the diagnosis.

### Environment

`cosmic-comp-1.8.0-1.fc44` (reports itself `cosmic-comp 1.0.0`, git `a55785993e8ef6aad38862`),
`cosmic-randr-1.8.0-1.fc44`, `cosmic-settings-daemon-1.8.0-1.fc44`, `cosmic-term-1.8.0-1.fc44`,
`grim-1.5.0-3.fc44`, `wayland-utils-1.3.0-3.fc44` (provides `wayland-info`) all present;
`foot` was missing (`scripts/dev/install-cosmic.sh` reported it and printed the `dnf` command;
not installed here since it never became load-bearing — `ghostty` and `cosmic-term` cover the
occluder/hover steps). Host: KDE Plasma 6 / KWin 6.7.5 on `seat0`, primary GPU NVIDIA GeForce
RTX 5070 Ti (proprietary 615.71.09), secondary AMD "Granite Ridge" iGPU present but unused by
the host compositor. `demo_bar`, `demo_dock`, `probe_toplevel` built clean with
`cargo build -p shell-host --features debug-inject --examples` (dev profile; the script's own
`build()` does a `--release` build, also clean).

### Blocker: the host session was locked, so nothing can be nested into it

`scripts/dev/nested.sh start` (both the default NVIDIA path and the `SILL_NESTED_GPU=amd`
Mesa-vendor fallback from its own Caveats section) got as far as creating the `wayland-1` socket
and writing `.nested-env`, then hung: `cosmic-comp`'s log stopped after one
`smithay::backend::egl::error: Erroneous EGL call didn't set EGLError` warning and one
`[EGL] 0x300d (BAD_SURFACE) eglQuerySurface: EGL_BAD_SURFACE` error, all threads went idle
(0 % CPU, `epoll`-sleeping), and `wayland-info` against the new socket blocked forever: it had
connected (an open fd to the socket) and was parked in `poll_schedule_timeout`, meaning
`cosmic-comp` accepted the client connection but never dispatched a registry event back to it.

The cause: `loginctl show-session … -p LockedHint` read `LockedHint=yes` and
`org.freedesktop.ScreenSaver.GetActive` returned `true` for the whole session (the machine had
been idle overnight; `kscreen-doctor --dpms show` also read `off` at the start, matching the
earlier "kwin (live session)" section above). `COSMIC_BACKEND=winit` opens the nested
compositor's own output as an ordinary `xdg_toplevel` window on the *host* compositor
(confirmed via `strings /usr/bin/cosmic-comp`: the winit backend is a vendored
`winit-wayland`), so it needs the host to map and configure that window before it can render
anything, including to its own nested clients.

To rule out an NVIDIA/EGL-specific bug rather than the lock, a control test used a completely
unrelated plain Wayland client with no cosmic-comp/smithay code in it at all: `timeout 8 env
WAYLAND_DISPLAY=wayland-0 ghostty -e true` against the live host session, locked, with DPMS
forced on (`kscreen-doctor --dpms on`) so a black screen could not be blamed either. It hung for
the full 8 s and no window ever appeared (`spectacle` screenshot at the time shows the KWin lock
screen, not a ghostty window). That confirms the host itself does not map *any* new client's
toplevel while locked, independent of cosmic-comp, so nesting a real `cosmic-comp` here needs the
session unlocked first — something this agent has no password to do and would not attempt to
bypass. Evidence: `spike/out/cosmic/blocker-evidence.log` (the lock/DPMS checks and the full
ghostty control-test transcript), `spike/out/cosmic/comp-nvidia-attempt{1,2}.log` (the
`cosmic-comp` logs from the default and `SILL_NESTED_GPU=amd` attempts — identical failure; the
AMD fallback does not help because the *host's* compositor, which the nested window is a client
of, is NVIDIA-only, and this machine's cross-GPU dmabuf import is separately broken per S1
above), `spike/out/cosmic/host-lockscreen-at-test-time.png` (the host screenshot showing the
lock screen at test time, for the record — not one of the deliverable demo screenshots, which
none of this run produced).

**Not run**: `scripts/dev/globals.sh` (protocol table), items a/b/c/f/g on `cosmic`, the fullscreen-occluder frame-callback observation (#2777), the still-pointer hover observation
(#2230), `spike/out/cosmic/overview.png`/`a.png`/`b.png`, and their copies to
`/home/pohsuanlai/quire/tools/progress/shots/m0-cosmic/`. None of these can be produced without
fabricating numbers, so they are left undone rather than guessed. **To resume**: unlock the
`seat0` session (and leave DPMS on, or `scripts/dev/nested.sh` will hit the same wall), then run
`scripts/dev/nested.sh start && scripts/dev/globals.sh` and `scripts/spike-accept.sh demo cosmic
a|b|c|f|g` from this branch — the harness changes below should carry the rest through without
further fixes, but were themselves never exercised against a live nested compositor and should
be watched on the first real run.

### Harness changes made while diagnosing this

- `scripts/dev/nested.sh`: the `wayland-info` sanity check after startup had no timeout, so a
  compositor that accepts the socket but never dispatches (exactly what happened here) hung the
  whole script forever instead of failing. Wrapped it in `timeout 10`; on a timeout it now
  prints the lock/DPMS diagnosis above (checks to run, what to fix) instead of a bare "exited
  non-zero". Non-fatal either way — matches the previous behavior of leaving the compositor
  running and `.nested-env` written even when `wayland-info` fails, so a caller can still
  inspect it.
- `scripts/spike-accept.sh`: added a `cosmic[@<scale>]` compositor mode alongside `kwin-virtual`
  and `kwin`. It reuses an already-running nested compositor if `scripts/dev/.nested-env` names a
  live pid, otherwise runs `scripts/dev/nested.sh start` itself and `scripts/dev/nested.sh stop`
  at exit (`trap ... EXIT`) — never touching a compositor it did not start. `cosmic@1.5` best-
  effort forces the nested output's scale: `cosmic-randr` 1.8.0 has no plain `scale` subcommand
  (`cosmic-randr scale <output> <n>` from the task's own suggestion does not exist — checked via
  `cosmic-randr scale --help`, which lists `enable`/`disable` as the nearest matches); the
  closest equivalent is `cosmic-randr mode <output> <w> <h> --scale <n>`, so the script parses
  `cosmic-randr list`'s output name and current `WxH` and calls that. If the parse or the call
  fails, it logs why (`spike/out/demo/cosmic@<scale>/cosmic-randr-scale-<item>.log`) and
  continues at the nested output's default scale rather than failing the item outright — this
  path is unverified (never reached a live compositor) and should be checked on the first real
  `cosmic@<scale>` run. On the Python side: `screenshot()` now shoots with `grim` instead of
  `spectacle` when `SPIKE_COMP_KIND=cosmic` (matching `scripts/dev/shot.sh`); `blur_setting()`
  reads the seeded `frosted` key from `scripts/dev/xdg-config` instead of KWin's `kreadconfig6`/
  `gdbus` (cosmic-comp has no separate blur on/off gate for a layer-shell client — see
  `scripts/dev/README.md`'s "Note on the frosted key" — so this is informational, same as
  before); item f's KWin-only `kscreen-doctor -o` scale cross-check is replaced by
  `cosmic-randr list` parsing for a `Scale:` line, made non-fatal (dropped from `checks` rather
  than forced to fail) if that does not parse, since cosmic-randr's plain-text format was never
  observed against a live output to confirm it. `cargo build`, `cargo test`, `cargo fmt --check`
  were not affected; `bash -n` and `python3 -m ast` both pass on the edited script.
- No `shell-host` source code was touched.

### Cleanup

No `cosmic-comp`, `ghostty`, or `wayland-info` process remained after each attempt (`pgrep -fa`
checked after every failed start); stray `wayland-1`/`wayland-1.lock` files left by
non-graceful kills were removed by hand once (`nested.sh stop` only removes the socket its own
tracked pid held, not one abandoned after an external `SIGKILL`/`timeout`, which is a real gap
but not fixed here since a graceful `stop` never leaves this behind — only the diagnosis process
did). `kscreen-doctor --dpms` was restored to `off`, its state at the start of this session.
`scripts/dev/.nested-env` does not exist on disk; only `wayland-0` (the host's own socket)
remains in `$XDG_RUNTIME_DIR`.

### Item b on cosmic (branch `cosmic-blur`, 2026-09-24): **pass** on the Mesa-rendering compositor

The earlier failure (bar missing with blur on) is cosmic-comp losing blurred layer surfaces on the
nested winit backend with NVIDIA's EGL (FINDINGS F29; plain `wl_shm` clients too). `nested.sh`
now renders the compositor on Mesa by default, where blur works; on the NVIDIA path the host
detects the condition and degrades to no blur (host line `blur: degraded`, bar visible with its
tint) instead of vanishing.

| Run | Result | Numbers | PNG |
| --- | --- | --- | --- |
| `demo cosmic b` (compositor Mesa, client NVIDIA Vulkan PreMultiplied) | **pass** | bar variance 3306.25 → 1.02, drop 99.97 %; checkerboard below 16256 → 16256; host `blur: supported` | `spike/out/demo/cosmic/b.png`, `b_blur0.png` |
| `demo kwin-virtual b` | **pass** | 3306.25 → 0.555, drop 99.98 %; below 16256 → 16256 | `spike/out/demo/kwin-virtual/b.png` |
| `SILL_NESTED_GPU=nvidia`, demo_bar `DEMO_BLUR=1` by hand | fails b by design | bar visible unblurred (variance 3306), host `blur: degraded` | (scratch) |

Items a, c, f, g were not re-run on the Mesa-rendering compositor (another agent's nested
compositor was up); they passed on the NVIDIA path before this change and blur is not in them.

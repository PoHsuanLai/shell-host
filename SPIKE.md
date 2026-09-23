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

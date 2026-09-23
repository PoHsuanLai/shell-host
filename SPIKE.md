# Milestone 0 spike

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

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

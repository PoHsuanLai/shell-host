# Milestone 0 spike

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

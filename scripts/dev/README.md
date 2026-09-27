# Nested-compositor dev harness

Runs a throwaway `cosmic-comp` inside the host Wayland session (KDE/KWin,
GNOME, or another `cosmic-comp`) so shell-host surfaces and layer-shell test
clients can be developed and screenshotted without a real COSMIC login.

## Scripts

- `install-cosmic.sh` — checks (via `rpm -q`) whether the packages this
  harness needs are installed and prints the `sudo dnf install` command for
  anything missing. Never calls `sudo` itself.
- `nested.sh [start|stop]` — starts (default) or stops the nested
  compositor.
  - `start`: snapshots the existing `$XDG_RUNTIME_DIR/wayland-*` sockets,
    launches `COSMIC_BACKEND=winit cosmic-comp --no-xwayland` with
    `XDG_CONFIG_HOME` pointed at `scripts/dev/xdg-config` (see below), waits
    up to 10s for a new `wayland-N` socket, writes `scripts/dev/.nested-env`
    (`WAYLAND_DISPLAY` + the compositor's pid), and runs `wayland-info`
    against the new socket into `scripts/dev/out/globals.txt`. Compositor
    stdout/stderr goes to `scripts/dev/out/comp.log`.
  - `stop`: kills only the compositor process it started (pid recorded in
    `.nested-env`). Never touches any other process, including any other
    `cosmic-comp` instance or the host session.
- `run.sh <command> [args...]` — sources `.nested-env` and execs `command`
  with `WAYLAND_DISPLAY` pointed at the nested compositor, e.g.
  `scripts/dev/run.sh ghostty`.
- `shot.sh <out.png>` — screenshot. If the nested compositor is running,
  shoots it with `grim` (`WAYLAND_DISPLAY=<nested> grim <out.png>`).
  Otherwise falls back to a host screenshot via
  `spectacle -b -n -f -o <out.png>`.
- `globals.sh` — re-queries the nested compositor with `wayland-info` and
  prints a table of whether it advertises the protocols shell-host depends
  on (`zwlr_layer_shell_v1`, `ext_background_effect_manager_v1`,
  `wp_fractional_scale_manager_v1`, `wp_viewporter`,
  `zcosmic_toplevel_info_v1`, `ext_workspace_manager_v1` /
  `zcosmic_workspace_manager_v2`, `zwp_text_input_manager_v3`) and at what
  version.

- `kwin-virtual.sh [start [WxH]|stop]` — a headless `kwin_wayland --virtual` on a private
  socket, a private D-Bus bus and an empty `XDG_CONFIG_HOME`, with fake input allowed; writes
  `.kwin-env` (source it). `stop` ends exactly what `start` started and never its caller
  (FINDINGS F54):
  - **Scope** (when a systemd `--user` manager answers): the bus and KWin run in a transient
    scope, `kwin-virtual-<pid>.scope`, so D-Bus-activated services (portals, gvfsd, at-spi,
    fcitx5, ksecretd), which daemonize and are reparented to init, stay in its cgroup; `stop` is
    `systemctl --user stop` on that scope (5 s stop timeout).
  - **Tree** (no user manager, or `KWIN_VIRTUAL_CONTAINMENT=tree`): the recorded KWin and bus
    pids and their descendants by parent chain, never `stop`'s own process or its ancestors. A
    service that daemonized away from the bus is not reached; it normally exits when the bus
    goes.
  - A process that only *names* the private bus in its environment (a script that sourced
    `.kwin-env`, a client it started) is the caller's, not the helper's: end your own clients
    before `stop`. The older rule (kill every process whose environment named the bus) killed
    `stop`'s own caller and then itself, leaving the bus running.
  - The bus itself is `dbus-daemon --config-file=scripts/dev/private-bus.conf`, not the stock
    session config: the stock config's service directories let a client's `Notify` D-Bus-activate
    KDE's own `org.kde.plasma.Notifications.service` inside the headless session, which would
    answer for the shell's own notification banners under test rather than leaving the call to
    fail (FINDINGS F56). `private-bus.conf` has no `<servicedir>` or
    `<standard_session_servicedirs/>`, only a `unix:tmpdir=` listen address, so nothing on this
    bus is activatable; verified with `busctl --address "$DBUS_SESSION_BUS_ADDRESS" list
    --activatable` (only `org.freedesktop.DBus` itself) and a `Notify` call, which fails
    `ServiceUnknown` instead of starting anything.
  - `KWIN_VIRTUAL_RUNTIME=<dir>` (opt-in, `accept-lock.sh`): KWin runs with its own runtime
    directory `<dir>` (made mode 700, must not exist, marked `.shell-host-virtual-compositor`);
    `.kwin-env` exports it and the socket's absolute path as `WAYLAND_DISPLAY`, and `stop`
    removes it (FINDINGS F65).
- `fake-input.sh` — synthetic pointer and keyboard input into that KWin, commands on stdin:
  `move X Y`, `press|release left|right|middle`, `key KEYSYM down|up`, `type "text"`,
  `axis vertical|horizontal VALUE` (KWin delivers it with no `axis_source` and no `value120`,
  FINDINGS F63), `sleep MS` (builds `tools/fake-input`, a no-op when up to date; FINDINGS F52).
- `kwin-windows.sh [--class APP_ID]` — every window's frame (window geometry) and buffer
  geometry as JSON lines, through `kwin-windows.js` and KWin's scripting D-Bus API.
- `accept-hover.sh` — real-input hover acceptance (FINDINGS F57-F59).
- `accept-scroll.sh` — the scroll engine's real-input acceptance (FINDINGS F63): wheel detents
  on `demo_scroll`'s list (60 px, then five fast ones), 0 frames in 5 s of idle, and a
  `data-wheel="capture"` element receiving the raw wheel.
- `accept-drag.sh` — the drag source's acceptance (FINDINGS F60): `demo_drag` dragged over
  nothing (`Cancelled`) and onto a foot window in the same headless session (`Dropped`, foot's
  side of the trace, and the path foot typed). foot comes from `PATH`, `FOOT=`, or `dnf
  download` unpacked into `out/foot` (no root). `DRAG_ICON=root` adds the icon.
- `accept-lock.sh` — the session lock's acceptance (FINDINGS F65): a cosmic-comp (winit backend,
  system EGL) inside the headless KWin, both sockets in a private runtime directory, a
  `probe_toplevel` as the session's window, and `demo_lock`, whose guard refuses any other
  socket: `Unsupported` on KWin itself, lock, a second locker `Finished`, typing without a click,
  a wrong password, 0 idle frames, "letmein" unlocks, and a locker killed while locked.
  `LOCK_TRACE=1` adds `WAYLAND_DEBUG` on the demo. Every exit path unlocks and stops both
  compositors; never point `demo_lock` at your session.
- `accept-popup-keyboard.sh` — a bar menu's keyboard (FINDINGS F72): `demo_popup_keys` (a
  `KeyboardMode::None` bar whose button opens a grabbing popup on its press) beside a focused
  `demo_window`. A real click opens the popup; real Tab, Down and x reach it with no click inside;
  Escape closes it; the bar never holds the keyboard; the window gets the keyboard back and a key
  typed after the close reaches it.
- `probe-modifiers.sh` — which `wl_keyboard.modifiers` a keyboard-less layer surface gets while
  Super is held (FINDINGS F61); prints, does not judge.

## Real input on a headless KWin

```sh
scripts/dev/kwin-virtual.sh start
source scripts/dev/.kwin-env
target/release/examples/demo_window &          # any client, on $WAYLAND_DISPLAY
scripts/dev/kwin-windows.sh --class demo_window  # frame [720,380,480,320]
printf 'move 900 396\npress left\nsleep 100\nmove 1000 446\nsleep 30\nmove 1100 496\nrelease left\n' \
  | scripts/dev/fake-input.sh
scripts/dev/kwin-windows.sh --class demo_window  # frame [920,480,480,320]
scripts/dev/kwin-virtual.sh stop
```

## Typical session

```sh
scripts/dev/install-cosmic.sh        # one-time; run the printed dnf command if anything's missing
scripts/dev/nested.sh                # start the nested compositor
scripts/dev/globals.sh               # confirm the protocols we need are there
scripts/dev/run.sh ghostty           # launch a test client inside it
scripts/dev/shot.sh scripts/dev/out/nested.png
scripts/dev/nested.sh stop           # tear it down
```

## `xdg-config/` seed

`nested.sh` points `XDG_CONFIG_HOME` at `scripts/dev/xdg-config` (checked
in) instead of the real `~/.config`, so the nested compositor gets its own
isolated COSMIC config:

- `cosmic/com.system76.CosmicSettings.Shortcuts/v1/custom` — binds
  `Ctrl+Alt+Space` to `Spawn("sill launcher toggle")`.
- `cosmic/com.system76.CosmicSettings.Shortcuts/v1/system_actions` —
  overrides the `Launcher` and `AppLibrary` system actions to shell out to
  `sill launcher {toggle,open}`, matching the real dist config in
  `sill/dist/cosmic/...`.
- `cosmic/com.system76.CosmicTheme.{Dark,Light}/v2/frosted` — set to `High`.

  **Note on the frosted key**: the technical-spec draft assumed
  `com.system76.CosmicTheme.{Dark,Light}/v1/frosted` (a bool). The schema
  actually shipped by `cosmic-settings-1.8.0-1.fc44` under
  `/usr/share/cosmic/com.system76.CosmicTheme.Dark/` has BOTH a `v1/`
  directory (with `is_frosted: bool`, unused) and a `v2/` directory. Checking
  the `cosmic-comp` binary (`strings /usr/bin/cosmic-comp`) confirms it only
  references the `v2` field names (`frosted`, `frosted_windows`,
  `frosted_panel`, `frosted_applets`, `frosted_system_interface`) — `v1`'s
  `is_frosted` does not appear at all. Cross-checked against source:
  `cosmic-theme` (`pop-os/libcosmic`, `cosmic-theme/src/model/theme.rs`)
  defines `pub frosted: BlurStrength` (`#[repr(u8)]` enum
  `ExtremelyLow[2] < VeryLow[2] < Low[2] < Medium[2](default) < High[2] <
  VeryHigh[2] < ExtremelyHigh[2]`, no explicit "off" variant), and
  `cosmic-comp` (`pop-os/cosmic-comp`, `src/backend/render/mod.rs:745`) uses
  it directly: `let blur_strength = (theme.cosmic().frosted as u8 + 1) as
  usize;`. `ext_background_effect_manager_v1` itself
  (`src/wayland/handlers/background_effect.rs`) always advertises
  `Capability::Blur` and always honors a client's `set_blur_region` — there
  is no separate on/off gate for third-party layer-shell clients, so the
  default (`Medium`) already blurs; we seed `High` here just to make the
  effect obviously visible in verification screenshots. The exact key used:
  **`com.system76.CosmicTheme.Dark/v2/frosted`** and
  **`com.system76.CosmicTheme.Light/v2/frosted`**, RON scalar, one of the
  `BlurStrength` variant names (e.g. `High`).

## Caveats

- **KDE eats `Super`.** The nested compositor does not grab the keyboard
  (cosmic-comp#9), and on this machine the host is KDE/KWin, which
  intercepts `Super`-based shortcuts before they ever reach the nested
  compositor. Test shell shortcuts with **`Ctrl+Alt+Space`** (seeded above)
  or via the CLI/D-Bus path, never by pressing `Super` inside the nested
  window.
- **The compositor renders on Mesa by default.** This machine's primary
  GPU is an NVIDIA RTX 5070 Ti, but on NVIDIA's EGL the nested `winit`
  backend loses every layer surface that has a blur region (FINDINGS F29:
  the bar disappears, the rest of the frame is drawn). `nested.sh` therefore
  forces the Mesa (AMD iGPU) EGL vendor
  (`__EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json`)
  and `DRI_PRIME=1` unless `SILL_NESTED_GPU=nvidia` is set, and sets both
  `__GL_SYNC_TO_VBLANK=0` and `vblank_mode=0` so the compositor keeps
  answering clients while the host display is off. Clients still pick their
  own adapter (shell-host uses NVIDIA Vulkan either way). On the NVIDIA path
  shell-host detects the condition and sends no blur region (host line
  `blur: degraded`), so its surfaces stay visible with their tint.
- **A screenshot costs one frame on the winit backend.** Every `grim`
  capture logs `eglSwapBuffers: EGL_BAD_SURFACE ... is not current draw
  surface` / `EGL_BAD_ALLOC` / `Failed to render frame` once, with or
  without blur and on either GPU: the capture blits into its own buffer
  before the window swap. It is harmless for the capture and unrelated to
  F29.
- **Do not run `cosmic-session` nested.** It hard-codes a full COSMIC
  component list (panel, power-key inhibitor, polkit agent, D-Bus service
  names, autostart entries) and will fight with the host session, restart
  components forever, and generally not do what you want. Always run
  `cosmic-comp` directly, as `nested.sh` does.
- `scripts/dev/out/` and `scripts/dev/.nested-env` are git-ignored: they are
  run output, not source.

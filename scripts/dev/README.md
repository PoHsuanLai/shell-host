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
- **NVIDIA EGL fallback.** This machine's primary GPU is an NVIDIA RTX
  5070 Ti. The `winit` backend's GL/EGL context creation can fail against
  the NVIDIA driver when nested. Set `SILL_NESTED_GPU=amd` before
  `nested.sh start` to force the Mesa (AMD iGPU) EGL vendor
  (`__EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json`)
  and `DRI_PRIME=1`. Record which path actually worked when reporting
  results — see `scripts/dev/out/comp.log`.
- **Do not run `cosmic-session` nested.** It hard-codes a full COSMIC
  component list (panel, power-key inhibitor, polkit agent, D-Bus service
  names, autostart entries) and will fight with the host session, restart
  components forever, and generally not do what you want. Always run
  `cosmic-comp` directly, as `nested.sh` does.
- `scripts/dev/out/` and `scripts/dev/.nested-env` are git-ignored: they are
  run output, not source.

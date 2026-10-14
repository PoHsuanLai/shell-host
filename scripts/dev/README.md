# Dev harness

Runs a throwaway headless compositor (KWin, or casement in a jail) so shell-host surfaces and
layer-shell test clients can be developed, driven with synthetic input and screenshotted without
touching the person's session.

## Scripts

- `shot.sh <out.png>` — screenshot. If the headless KWin is running, shoots it with `grim`
  (`WAYLAND_DISPLAY=<kwin> grim <out.png>`). Otherwise falls back to a host screenshot via
  `spectacle -b -n -f -o <out.png>`.
- `globals.sh` — re-queries the headless KWin with `wayland-info` and prints a table of whether it
  advertises the protocols shell-host depends on (`zwlr_layer_shell_v1`,
  `ext_background_effect_manager_v1`, `wp_fractional_scale_manager_v1`, `wp_viewporter`,
  `ext_foreign_toplevel_list_v1`, `ext_workspace_manager_v1`, `zwp_text_input_manager_v3`) and at
  what version.
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
- `accept-lock.sh` — the session lock's acceptance (FINDINGS F65), on casement only (KWin has no
  ext-session-lock-v1): casement in a jail with its socket in a private runtime directory, a
  `probe_toplevel` as the session's window, and `demo_lock`, whose guard refuses any other
  socket: lock, a second locker `Finished`, typing without a click,
  a wrong password, 0 idle frames, "letmein" unlocks, and a locker killed while locked.
  `LOCK_TRACE=1` adds `WAYLAND_DEBUG` on the demo. Every exit path unlocks and stops the
  compositor; never point `demo_lock` at your session.
- `accept-popup-keyboard.sh` — a bar menu's keyboard (FINDINGS F72): `demo_popup_keys` (a
  `KeyboardMode::None` bar whose button opens a grabbing popup on its press) beside a focused
  `demo_window`. A real click opens the popup; real Tab, Down and x reach it with no click inside;
  Escape closes it; the bar never holds the keyboard; the window gets the keyboard back and a key
  typed after the close reaches it.
- `probe-modifiers.sh` — which `wl_keyboard.modifiers` a keyboard-less layer surface gets while
  Super is held (FINDINGS F61); prints, does not judge.

## The same scripts on casement

`COMPOSITOR=casement scripts/dev/accept-<name>.sh` (hover, scroll, drag, ime, lock, modifiers,
popup-keyboard, swipe; lock runs only there) runs a script against casement, our compositor, instead of a headless
KWin. The script builds its demos outside the jail, then `lib/compositor.sh` re-executes it in
`~/desktop/harness/jail.sh` (no GPU, no network, no `/dev/input`, no session), where casement's
`dev/up.sh` starts `casement --backend headless` on a private bus and the script's input goes
through `casementctl debug inject` (`lib/input.sh`; a superset of `fake-input.sh`'s grammar). State
lives in the jail's scratch directory `scripts/dev/out/casement-<name>/` (logs under `out/`,
casement's under `casement/`), never in the checkout otherwise.

- `CASEMENT_DIR` (default `../casement`) is the casement checkout; its tools must be built
  first, outside the jail: `$CASEMENT_DIR/dev/bootstrap.sh --build-only` (`CASEMENT_TOOLS` moves
  the directory).
- `SHELL_HOST_BACKEND` is `cpu` by default under casement (the demos paint into `wl_shm`);
  `gpu` runs the library default (lavapipe in the jail), which with `CASEMENT_DMABUF=none` (the
  default) is the F80 fallback; `accept-hover.sh` adds an `f80_fallback` step then.
- `CASEMENT_CLOCK` is `wall` by default (the demos run on the wall clock; `stepped` is for casement's
  own tests).
- `lib/input.sh` and `lib/windows.sh [--class APP_ID]` are the compositor-neutral forms of
  `fake-input.sh` and `kwin-windows.sh`.
- `accept-ime.sh` starts fcitx5 as a client (input-method v2); `accept-lock.sh` locks casement
  directly and reads "covered" from the compositor's input, since casement has no screenshot yet;
  `accept-drag.sh` fetches foot before the jail is entered (`dnf download`, no root).

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
scripts/dev/kwin-virtual.sh start    # start the headless compositor
source scripts/dev/.kwin-env
scripts/dev/globals.sh               # confirm the protocols we need are there
target/release/examples/demo_window &   # a test client inside it
scripts/dev/shot.sh scripts/dev/out/kwin.png
scripts/dev/kwin-virtual.sh stop     # tear it down
```

`scripts/dev/out/` and `scripts/dev/.kwin-env` are git-ignored: they are run output, not source.

#!/usr/bin/env bash
# A headless KWin for acceptance with real (synthetic) Wayland input, invisible to the user's
# session: `kwin_wayland --virtual` on a private socket and a private D-Bus session bus.
#
#   scripts/dev/kwin-virtual.sh start [WIDTHxHEIGHT]   (default 1920x1080)
#   scripts/dev/kwin-virtual.sh stop
#
# start writes scripts/dev/.kwin-env (WAYLAND_DISPLAY, DBUS_SESSION_BUS_ADDRESS, the pids, the
# log path); `source` it to run clients, `gdbus`, `kscreen-doctor` or the KWin scripts against
# this instance. The compositor runs with KWIN_WAYLAND_NO_PERMISSION_CHECKS=1 so that
# scripts/dev/fake-input.sh is offered org_kde_kwin_fake_input (FINDINGS F52); never set that on
# a real session. Its log (QT_LOGGING_RULES enables `js` so KWin scripts' print() lands there)
# is scripts/dev/out/kwin-virtual.log.
#
# The private D-Bus session bus runs on scripts/dev/private-bus.conf, not the stock session
# config: the stock config's service directories let a client's `Notify` D-Bus-activate KDE's
# own notification service inside this headless session, which would answer for the shell's own
# notification banners under test (FINDINGS F56). private-bus.conf has no servicedirs, so
# nothing on this bus is activatable.
#
# KWin gets its own empty XDG_CONFIG_HOME (scripts/dev/out/kwin-virtual-config): otherwise it
# reads the user's kwinrc and runs the user's installed KWin scripts (window rules, tiling or
# maximize helpers), which would act on the windows under test. Clients keep their own config.
#
# stop ends exactly what start started, never the caller (FINDINGS F54). start runs the bus and
# KWin in their own transient systemd --user scope (`systemd-run --user --scope`), so everything
# they start lands in its cgroup, including D-Bus-activated services (portals, gvfsd, at-spi,
# fcitx5, ksecretd), which daemonize and are reparented to init; stop is then
# `systemctl --user stop` on that scope. Without a reachable systemd --user manager (or with
# KWIN_VIRTUAL_CONTAINMENT=tree) it falls back to the recorded pids: KWin, the bus and their
# descendants by parent chain; a service that daemonized away from the bus is not reached then
# (it normally exits with the bus).
# A process that merely names the private bus in its environment (a script that sourced
# .kwin-env, a client it started) is not the helper's to kill: the caller ends its own clients.
set -euo pipefail

DEV_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT_DIR="$DEV_DIR/out"
ENV_FILE="$DEV_DIR/.kwin-env"
LOG_FILE="$OUT_DIR/kwin-virtual.log"
BUS_FILE="$OUT_DIR/kwin-virtual-bus"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
mkdir -p "$OUT_DIR"

# `scope` when a systemd --user manager answers, else `tree`; KWIN_VIRTUAL_CONTAINMENT overrides.
containment() {
  if [ -n "${KWIN_VIRTUAL_CONTAINMENT:-}" ]; then
    echo "$KWIN_VIRTUAL_CONTAINMENT"
  elif command -v systemd-run >/dev/null && systemctl --user show-environment >/dev/null 2>&1; then
    echo scope
  else
    echo tree
  fi
}

# The descendants of the given pids, by parent chain, deepest last; never this script or the
# processes it runs under.
descendants() {
  local guard=" $$ " p=$$
  while [ "$p" -gt 1 ]; do
    p="$(ps -o ppid= -p "$p" | tr -d ' ')"
    [ -n "$p" ] || break
    guard="$guard$p "
  done
  ps -eo pid=,ppid= | awk -v roots="$*" -v guard="$guard" '
    { parent[$1] = $2; order[NR] = $1 }
    END {
      n = split(roots, r, " "); for (i = 1; i <= n; i++) keep[r[i]] = 1
      do {
        grew = 0
        for (i = 1; i <= NR; i++) {
          c = order[i]
          if (!(c in keep) && (parent[c] in keep)) { keep[c] = 1; grew = 1; print c }
        }
      } while (grew)
    }' | while read -r c; do case "$guard" in *" $c "*) ;; *) echo "$c" ;; esac; done
}

stop() {
  if [ ! -f "$ENV_FILE" ]; then
    echo "kwin-virtual.sh stop: no $ENV_FILE, nothing to stop."
    return 0
  fi
  # shellcheck disable=SC1090
  source "$ENV_FILE"
  case "${KWIN_CONTAINMENT:-tree}" in
    scope)
      systemctl --user stop "$KWIN_SCOPE" 2>/dev/null || true
      ;;
    *)
      local pids
      pids="$(descendants "$KWIN_VIRTUAL_PID" "$KWIN_DBUS_PID")"
      # shellcheck disable=SC2086
      kill "$KWIN_VIRTUAL_PID" $pids 2>/dev/null || true
      sleep 0.5
      kill "$KWIN_DBUS_PID" 2>/dev/null || true
      ;;
  esac
  rm -f "$ENV_FILE" "$BUS_FILE"
  echo "kwin-virtual.sh: stopped $WAYLAND_DISPLAY"
}

# Run inside the scope (or the setsid session): start the private bus, record it, and become
# KWin, so that the pid start recorded is KWin's and both share the scope's cgroup.
inside() {
  local size="$1" sock="$2" config="$3"
  dbus-daemon --config-file="$DEV_DIR/private-bus.conf" --fork --print-address=1 --print-pid=1 \
      >"$BUS_FILE"
  local address
  address="$(sed -n 1p "$BUS_FILE")"
  export DBUS_SESSION_BUS_ADDRESS="$address" KWIN_WAYLAND_NO_PERMISSION_CHECKS=1
  export XDG_CONFIG_HOME="$config" QT_FORCE_STDERR_LOGGING=1
  export QT_LOGGING_RULES="js=true;kwin_scripting=true"
  exec setsid kwin_wayland --virtual --no-lockscreen --no-global-shortcuts \
      --width "${size%x*}" --height "${size#*x}" --socket "$sock"
}

start() {
  if [ -f "$ENV_FILE" ]; then
    # shellcheck disable=SC1090
    source "$ENV_FILE"
    if kill -0 "$KWIN_VIRTUAL_PID" 2>/dev/null; then
      echo "kwin-virtual.sh: already running on $WAYLAND_DISPLAY ($ENV_FILE)"
      return 0
    fi
    rm -f "$ENV_FILE"
  fi
  local size="${1:-1920x1080}"
  local sock="wayland-kwinvirt-$$"
  local unit="kwin-virtual-$$.scope"
  local config="$OUT_DIR/kwin-virtual-config"
  local mode
  mode="$(containment)"
  rm -rf "$config" && mkdir -p "$config"
  rm -f "$BUS_FILE"
  case "$mode" in
    scope)
      systemd-run --user --scope --quiet --collect --unit="$unit" \
        --property=TimeoutStopSec=5 -- "$DEV_DIR/kwin-virtual.sh" inside "$size" "$sock" "$config" \
        </dev/null >"$LOG_FILE" 2>&1 &
      ;;
    *)
      "$DEV_DIR/kwin-virtual.sh" inside "$size" "$sock" "$config" </dev/null >"$LOG_FILE" 2>&1 &
      ;;
  esac
  local kpid=$!
  for _ in $(seq 100); do [ -S "$XDG_RUNTIME_DIR/$sock" ] && break; sleep 0.1; done
  if [ ! -S "$XDG_RUNTIME_DIR/$sock" ] || [ ! -s "$BUS_FILE" ]; then
    echo "kwin-virtual.sh: no socket after 10 s; see $LOG_FILE" >&2
    [ "$mode" = scope ] && systemctl --user stop "$unit" 2>/dev/null
    kill "$kpid" "$(sed -n 2p "$BUS_FILE" 2>/dev/null)" 2>/dev/null || true
    return 1
  fi
  local address pid
  address="$(sed -n 1p "$BUS_FILE")"
  pid="$(sed -n 2p "$BUS_FILE")"
  # KWin answers on D-Bus a moment after its socket appears.
  sleep 1.5
  cat >"$ENV_FILE" <<ENV
# generated by scripts/dev/kwin-virtual.sh -- do not edit by hand
export XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR
export WAYLAND_DISPLAY=$sock
export DBUS_SESSION_BUS_ADDRESS='$address'
KWIN_VIRTUAL_PID=$kpid
KWIN_DBUS_PID=$pid
KWIN_CONTAINMENT=$mode
KWIN_SCOPE=$unit
KWIN_VIRTUAL_LOG=$LOG_FILE
ENV
  echo "kwin-virtual.sh: $sock ${size} (pid $kpid, $mode), env in $ENV_FILE"
}

case "${1:-start}" in
  start) shift || true; start "$@" ;;
  stop) stop ;;
  inside) shift; inside "$@" ;;
  *) echo "usage: kwin-virtual.sh [start [WIDTHxHEIGHT]|stop]" >&2; exit 2 ;;
esac

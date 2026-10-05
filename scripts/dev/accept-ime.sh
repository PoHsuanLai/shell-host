#!/usr/bin/env bash
# IME acceptance: Chewing through fcitx5 into a shell-host text field, headless.
# Never the user's session: kwin-virtual.sh runs KWin with its own runtime directory, private
# D-Bus bus and private XDG_CONFIG_HOME/XDG_DATA_HOME, and starts its own fcitx5 as KWin's input
# method (`--inputmethod scripts/dev/ime-fcitx5.sh`, zwp_input_method_v1) with the profile in
# scripts/dev/ime-config (Chewing, active by default: no toggle key needed). The user's fcitx5,
# its bus name and its config are never touched.
#
# COMPOSITOR=casement: casement has no `--inputmethod`; the script starts the same fcitx5 (same
# profile, private config, data and cache under the jail's scratch) as an ordinary client of the
# casement in the jail, and it binds zwp_input_method_v2 (WAYLANDIM V2) instead of v1.
#
#   scripts/dev/accept-ime.sh
#   COMPOSITOR=casement scripts/dev/accept-ime.sh
#
# Steps on examples/demo_ime (a plain <input> focused on mount, on an exclusive-keyboard
# Overlay layer), each PASS/FAIL:
#   enable     zwp_text_input_v3.enable is sent after the text input's enter
#   compose    s u 3 c l 3 (Chewing for 你好) then Return: the field's value (its oninput) is
#              你好, and no letter reached the document as a raw key
#   backspace  s u, then BackSpace twice mid-preedit, then Return: the value is unchanged (no
#              stray ㄋ, ㄧ or letters)
# Chewing commits with Return; Space opens the candidate list instead of committing.
#
# SILL_DIR=<sill checkout> (optional) repeats compose and backspace on that checkout's
# launcher_demo (`cargo build -p sill-surfaces --example launcher_demo` there first; it links
# whichever shell-host its ../shell-host path names), reading the panel's field back with the
# debug-inject `<panel> focused` verb, and captures the panel to out/ime-launcher.png.
#
# Logs: scripts/dev/out/ime-*.log; ime-trace.log / ime-launcher-trace.log are the clients'
# WAYLAND_DEBUG=client traces filtered to zwp_text_input_v3 and wl_keyboard.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEV="$ROOT/scripts/dev"
# shellcheck source=scripts/dev/lib/compositor.sh
source "$DEV/lib/compositor.sh"
DEMO_BIN="$ROOT/target/debug/examples/demo_ime"
DEMO="" INPUT="" LAUNCHER="" IME=""

command -v fcitx5 >/dev/null || { echo "FAIL setup: no fcitx5 on PATH" >&2; exit 1; }
[ -f /usr/lib64/fcitx5/libchewing.so ] || [ -f /usr/lib/fcitx5/libchewing.so ] \
  || { echo "FAIL setup: fcitx5-chewing is not installed" >&2; exit 1; }
comp_build --manifest-path "$ROOT/Cargo.toml" -p shell-host --example demo_ime || exit 1
if [ -n "${SILL_DIR:-}" ] && [ ! -x "$SILL_DIR/target/debug/examples/launcher_demo" ]; then
  echo "FAIL setup: no $SILL_DIR/target/debug/examples/launcher_demo" >&2
  exit 1
fi
comp_enter "$0" "$@"
OUT="$(comp_out)"
mkdir -p "$OUT"
SESSION_RUNTIME="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
RUNTIME="$SESSION_RUNTIME/shell-host-ime-$$"

: >"$OUT/ime-fcitx5.log"
case "$COMPOSITOR" in
  kwin)
    IME_BIND="WAYLANDIM V1"
    KWIN_VIRTUAL_RUNTIME="$RUNTIME" KWIN_VIRTUAL_INPUTMETHOD="$DEV/ime-fcitx5.sh" \
      KWIN_VIRTUAL_CONFIG_SEED="$DEV/ime-config" comp_start 1920x1080 || exit 1
    ;;
  casement)
    IME_BIND="WAYLANDIM V2"
    comp_start 1920x1080 || exit 1
    ;;
esac

stop_all() {
  [ -n "$DEMO" ] && kill -0 "$DEMO" 2>/dev/null && printf 'quit\n' >&4 2>/dev/null
  [ -n "$LAUNCHER" ] && kill -0 "$LAUNCHER" 2>/dev/null && printf 'quit\n' >&5 2>/dev/null
  exec 3>&- 4>&- 5>&- 2>/dev/null
  for pid in $DEMO $LAUNCHER $INPUT; do
    for _ in $(seq 20); do kill -0 "$pid" 2>/dev/null || break; sleep 0.1; done
    kill "$pid" 2>/dev/null
  done
  rm -f "$OUT/ime-input.fifo" "$OUT/ime-demo.fifo" "$OUT/ime-launcher.fifo"
  [ -n "$IME" ] && kill "$IME" 2>/dev/null
  comp_stop
}
trap stop_all EXIT

COMP_SOCKET="$(comp_socket)"
case "$COMPOSITOR:$COMP_SOCKET" in
  kwin:"$RUNTIME"/* | casement:"$SESSION_RUNTIME"/*) ;;
  *) echo "FAIL setup: the compositor's socket $COMP_SOCKET is not under the expected runtime directory" >&2; exit 1 ;;
esac

# casement has no `--inputmethod`: fcitx5 is an ordinary client of it, on the same private bus,
# with the seeded profile and its data under the jail's scratch (never the user's).
if [ "$COMPOSITOR" = casement ]; then
  IME_HOME="$OUT/ime-home"
  mkdir -p "$IME_HOME/config" "$IME_HOME/data" "$IME_HOME/cache" "$IME_HOME/state"
  cp -r "$DEV/ime-config/." "$IME_HOME/config/"
  (
    comp_source
    export XDG_CONFIG_HOME="$IME_HOME/config" XDG_DATA_HOME="$IME_HOME/data" \
      XDG_CACHE_HOME="$IME_HOME/cache" XDG_STATE_HOME="$IME_HOME/state" \
      IME_FCITX5_LOG="$OUT/ime-fcitx5.log"
    exec "$DEV/ime-fcitx5.sh"
  ) >/dev/null 2>&1 &
  IME=$!
fi

status=0
pass() { echo "PASS $1: $2"; }
fail() { echo "FAIL $1: $2"; status=1; }

# fcitx5 binds the compositor's input method (KWin: zwp_input_method_v1 a moment after KWin
# starts it; casement: zwp_input_method_v2).
for _ in $(seq 100); do
  grep -q "$IME_BIND" "$OUT/ime-fcitx5.log" 2>/dev/null && break
  sleep 0.1
done
grep -q "$IME_BIND" "$OUT/ime-fcitx5.log" \
  || { echo "FAIL setup: fcitx5 never bound its input method ($IME_BIND); see $OUT/ime-fcitx5.log" >&2; exit 1; }
sleep 1

rm -f "$OUT/ime-input.fifo" "$OUT/ime-demo.fifo" "$OUT/ime-launcher.fifo"
mkfifo "$OUT/ime-input.fifo" "$OUT/ime-demo.fifo" "$OUT/ime-launcher.fifo"
: >"$OUT/ime-demo.log"
(
  comp_source
  exec env WAYLAND_DEBUG=client "$DEMO_BIN"
) <"$OUT/ime-demo.fifo" >"$OUT/ime-demo.log" 2>&1 &
DEMO=$!
exec 4>"$OUT/ime-demo.fifo"
"$DEV/lib/input.sh" <"$OUT/ime-input.fifo" >"$OUT/ime-input.log" 2>&1 &
INPUT=$!
exec 3>"$OUT/ime-input.fifo"

# The fake-input commands "$@"; returns once each is answered (`ok <line>`).
input() {
  local before
  before=$(grep -c '^ok ' "$OUT/ime-input.log")
  printf '%s\n' "$@" >&3
  for _ in $(seq 200); do
    [ "$(grep -c '^ok ' "$OUT/ime-input.log")" -ge $((before + $#)) ] && return 0
    sleep 0.05
  done
  echo "fake-input did not answer $*" >&2
  return 1
}

# A tap of each keysym.
tap() {
  local cmds=()
  for sym in "$@"; do cmds+=("key $sym down" "key $sym up" "sleep 80"); done
  input "${cmds[@]}"
}

# Chewing for 你好, committed.
compose() { tap s u 3 c l 3 && sleep 0.5 && tap Return && sleep 0.8; }
# ㄋㄧ composed, then both taken back mid-preedit, then a Return with nothing composed.
retract() { tap s u && sleep 0.3 && tap BackSpace BackSpace && sleep 0.3 && tap Return && sleep 0.8; }

# The demo's field value, as its oninput last reported it.
value() {
  local before
  before=$(grep -c '^ime: value ' "$OUT/ime-demo.log")
  printf 'value\n' >&4
  for _ in $(seq 40); do
    [ "$(grep -c '^ime: value ' "$OUT/ime-demo.log")" -gt "$before" ] && break
    sleep 0.05
  done
  grep '^ime: value ' "$OUT/ime-demo.log" | tail -1 | sed 's/^ime: value //'
}

# The WAYLAND_DEBUG lines of log $1 about the text input and the keyboard, into $2.
trace() { grep -E 'zwp_text_input_v3|wl_keyboard' "$1" >"$2"; }

# Whether trace $1 has an enable after the text input's first enter.
enabled_after_enter() {
  awk '/zwp_text_input_v3#[0-9]+\.enter\(/ { entered = 1 }
       entered && /zwp_text_input_v3#[0-9]+\.enable\(\)/ { found = 1 }
       END { exit !found }' "$1"
}

for _ in $(seq 100); do grep -q '^ime: focused' "$OUT/ime-demo.log" && break; sleep 0.1; done
sleep 1

# --- enable -------------------------------------------------------------------------------------
trace "$OUT/ime-demo.log" "$OUT/ime-trace.log"
if enabled_after_enter "$OUT/ime-trace.log"; then
  pass enable "$(grep -cE '\.enable\(\)' "$OUT/ime-trace.log") enable(s), after the enter"
else
  fail enable "no enable after the text input's enter in $OUT/ime-trace.log"
fi

# --- compose: 你好 ------------------------------------------------------------------------------
keys_before=$(grep -c '^ime: key ' "$OUT/ime-demo.log")
compose || exit 1
got="$(value)"
letters=$(grep '^ime: key ' "$OUT/ime-demo.log" | tail -n +"$((keys_before + 1))" \
  | grep -cE '^ime: key [a-z0-9]$')
if [ "$got" = "你好" ] && [ "$letters" = 0 ]; then
  pass compose "field is '$got'"
else
  fail compose "field is '$got', $letters raw letter key(s) reached the document"
fi

# --- backspace mid-preedit ----------------------------------------------------------------------
retract || exit 1
after="$(value)"
trace "$OUT/ime-demo.log" "$OUT/ime-trace.log"
if [ "$after" = "$got" ] && [ "${got%%|*}" = "你好" ]; then
  pass backspace "field still '$after'"
else
  fail backspace "field '$got' became '$after'"
fi

printf 'quit\n' >&4
for _ in $(seq 50); do kill -0 "$DEMO" 2>/dev/null || break; sleep 0.1; done
[ -z "${SILL_DIR:-}" ] && exit "$status"

# --- sill's launcher ----------------------------------------------------------------------------
: >"$OUT/ime-launcher.log" && : >"$OUT/ime-launcher.out"
(
  comp_source
  cd "$SILL_DIR" || exit 1
  exec env WAYLAND_DEBUG=client target/debug/examples/launcher_demo
) <"$OUT/ime-launcher.fifo" >"$OUT/ime-launcher.out" 2>"$OUT/ime-launcher.log" &
LAUNCHER=$!
exec 5>"$OUT/ime-launcher.fifo"
sleep 3
printf 'open\nstats\n' >&5
sleep 2
PANEL=$(grep -o '"sill-launcher-[A-Za-z0-9-]*"' "$OUT/ime-launcher.out" | tr -d '"' \
  | grep -v catcher | head -1)
if [ -z "$PANEL" ]; then
  fail launcher "no sill-launcher-<output> surface in $OUT/ime-launcher.out"
  exit "$status"
fi

# The panel's focused field as `focused` reports it: `<typed>|<shown>|<ime>`.
field() {
  local before
  before=$(grep -c '"focused"' "$OUT/ime-launcher.out")
  printf '%s focused\n' "$PANEL" >&5
  for _ in $(seq 40); do
    [ "$(grep -c '"focused"' "$OUT/ime-launcher.out")" -gt "$before" ] && break
    sleep 0.05
  done
  grep '"focused"' "$OUT/ime-launcher.out" | tail -1 \
    | python3 -c 'import json,sys; d=json.loads(sys.stdin.read()); print("|".join(str(d.get(k)) for k in ("typed", "shown", "ime")))'
}

compose || exit 1
got="$(field)"
printf '%s capture %s\n' "$PANEL" "$OUT/ime-launcher.png" >&5
sleep 1
trace "$OUT/ime-launcher.log" "$OUT/ime-launcher-trace.log"
if [ "$got" = "你好|你好|enabled" ] && enabled_after_enter "$OUT/ime-launcher-trace.log"; then
  pass launcher-compose "$PANEL field is '${got%%|*}' (capture $OUT/ime-launcher.png)"
else
  fail launcher-compose "$PANEL field typed|shown|ime is '$got'"
fi
retract || exit 1
after="$(field)"
if [ "$after" = "$got" ] && [ "${got%%|*}" = "你好" ]; then
  pass launcher-backspace "field still '${after%%|*}'"
else
  fail launcher-backspace "field '$got' became '$after'"
fi

exit "$status"

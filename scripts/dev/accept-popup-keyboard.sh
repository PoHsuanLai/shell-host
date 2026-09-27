#!/usr/bin/env bash
# A bar menu's keyboard under real input (FINDINGS F72): examples/demo_popup_keys (a
# KeyboardMode::None bar whose "Menu" button opens a grabbing popup on its press) beside
# examples/demo_window (a toplevel standing in for the app the user was typing in), on a headless
# KWin (kwin-virtual.sh) driven by fake-input.sh. Never the user's session.
#
#   scripts/dev/accept-popup-keyboard.sh
#
# Steps, each PASS/FAIL:
#   app_focused      a real click on the window gives it the keyboard (its wl_keyboard.enter)
#   popup_opens      a real click on the bar's button opens the popup
#   keys_reach_popup real Tab, Down and x reach the popup's document (its onkeydown), with no
#                    click inside it first
#   escape_closes    real Escape closes it
#   bar_never_keyed  the bar never held the keyboard: the demo's only wl_keyboard.enter is the
#                    popup's. Before F72 the host lifted the bar to OnDemand, KWin activated it,
#                    and it kept the keyboard after the popup closed
#   app_gets_it_back after the close the window has the keyboard again, and a real key typed
#                    then reaches it
# Logs and both clients' WAYLAND_DEBUG traces: scripts/dev/out/popup-keys-*.log.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEV="$ROOT/scripts/dev"
OUT="$DEV/out"
mkdir -p "$OUT"
cargo build --manifest-path "$ROOT/Cargo.toml" -p shell-host --features debug-inject \
  --example demo_popup_keys --example demo_window >&2 || exit 1
DEMO_BIN="$ROOT/target/debug/examples/demo_popup_keys"
WIN_BIN="$ROOT/target/debug/examples/demo_window"
KEYS="$OUT/popup-keys-demo.log"
TRACE="$OUT/popup-keys-demo-trace.log"
WTRACE="$OUT/popup-keys-window-trace.log"
INPUT_LOG="$OUT/popup-keys-input.log"
DEMO="" WIN="" INPUT=""

"$DEV/kwin-virtual.sh" start 1280x800 >&2 || exit 1

stop_all() {
  [ -n "$DEMO" ] && printf 'quit\n' >&4 2>/dev/null
  [ -n "$WIN" ] && printf 'quit\n' >&5 2>/dev/null
  exec 3>&- 4>&- 5>&- 2>/dev/null
  for pid in $DEMO $WIN $INPUT; do
    for _ in $(seq 30); do kill -0 "$pid" 2>/dev/null || break; sleep 0.1; done
    kill "$pid" 2>/dev/null
  done
  rm -f "$OUT/popup-keys-input.fifo" "$OUT/popup-keys-demo.fifo" "$OUT/popup-keys-win.fifo"
  "$DEV/kwin-virtual.sh" stop >&2
}
trap stop_all EXIT

rm -f "$OUT/popup-keys-input.fifo" "$OUT/popup-keys-demo.fifo" "$OUT/popup-keys-win.fifo"
mkfifo "$OUT/popup-keys-input.fifo" "$OUT/popup-keys-demo.fifo" "$OUT/popup-keys-win.fifo"
: >"$KEYS"
(
  # shellcheck disable=SC1091
  source "$DEV/.kwin-env"
  WAYLAND_DEBUG=1 exec "$WIN_BIN" <"$OUT/popup-keys-win.fifo" >/dev/null 2>"$WTRACE"
) &
WIN=$!
exec 5>"$OUT/popup-keys-win.fifo"
(
  # shellcheck disable=SC1091
  source "$DEV/.kwin-env"
  WAYLAND_DEBUG=1 exec "$DEMO_BIN" <"$OUT/popup-keys-demo.fifo" >"$KEYS" 2>"$TRACE"
) &
DEMO=$!
exec 4>"$OUT/popup-keys-demo.fifo"
"$DEV/fake-input.sh" <"$OUT/popup-keys-input.fifo" >"$INPUT_LOG" 2>&1 &
INPUT=$!
exec 3>"$OUT/popup-keys-input.fifo"
sleep 2.5

status=0
check() {
  if [ "$2" = 1 ]; then echo "PASS $1: $3"; else echo "FAIL $1: $3"; status=1; fi
}

# The fake-input commands "$@"; returns once each is answered (`ok <line>`).
input() {
  local before
  before=$(grep -c '^ok ' "$INPUT_LOG")
  printf '%s\n' "$@" >&3
  for _ in $(seq 200); do
    [ "$(grep -c '^ok ' "$INPUT_LOG")" -ge $((before + $#)) ] && return 0
    sleep 0.05
  done
  echo "fake-input did not answer $*" >&2
  return 1
}
key() { input "key $1 down" "sleep 30" "key $1 up" "sleep 200"; }
click() { input "move $1 $2" "sleep 60" "press left" "sleep 40" "release left" "sleep 300"; }
# Wait up to 5 s for file $1 to hold at least $3 lines matching $2.
wait_for() {
  for _ in $(seq 100); do
    [ "$(grep -c -- "$2" "$1")" -ge "$3" ] && return 0
    sleep 0.05
  done
  return 1
}
enters() { grep -c 'wl_keyboard#[0-9]*\.enter(' "$1"; }

# The window's frame centre (KWin places it); the bar's button is at (47, 16).
frame=""
for _ in $(seq 20); do
  frame=$("$DEV/kwin-windows.sh" --class demo_window | sed -n 's/.*"frame":\[\([^]]*\)\].*/\1/p' | head -n 1)
  [ -n "$frame" ] && break
  sleep 0.5
done
IFS=, read -r fx fy fw fh <<<"$frame"
if [ -z "${fh:-}" ]; then
  echo "FAIL setup: no demo_window frame from kwin-windows.sh" >&2
  exit 1
fi
wx=$(( ${fx%.*} + ${fw%.*} / 2 )) wy=$(( ${fy%.*} + ${fh%.*} / 2 ))

click "$wx" "$wy"
ok=0; wait_for "$WTRACE" 'wl_keyboard#[0-9]*\.enter(' 1 && ok=1
check app_focused $ok "click at $wx,$wy; window enters: $(enters "$WTRACE")"
window_enters=$(enters "$WTRACE")

click 47 16
ok=0; wait_for "$KEYS" '"event":"opened"' 1 && ok=1
check popup_opens $ok "$(grep -c '"event":"opened"' "$KEYS") opened line(s)"
sleep 0.5

key Tab; key Down; key x
ok=0
wait_for "$KEYS" '"event":"key"' 3 \
  && grep -q '"key":"Tab"' "$KEYS" && grep -q '"key":"ArrowDown"' "$KEYS" \
  && grep -q '"key":"x"' "$KEYS" && ok=1
check keys_reach_popup $ok "$(grep '"event":"key"' "$KEYS" | tr '\n' ' ')"

key Escape
ok=0; wait_for "$KEYS" '"event":"closed"' 1 && ok=1
check escape_closes $ok "$(grep -c '"event":"closed"' "$KEYS") closed line(s)"
sleep 0.5

demo_enters=$(enters "$TRACE")
ok=0; [ "$demo_enters" = 1 ] && ok=1
check bar_never_keyed $ok "demo wl_keyboard.enter count $demo_enters (1 = the popup only); \
bar lifts: $(grep -c 'set_keyboard_interactivity(2)' "$TRACE")"

keys_before=$(grep -c 'wl_keyboard#[0-9]*\.key(' "$WTRACE")
key y
ok=0
[ "$(enters "$WTRACE")" -gt "$window_enters" ] \
  && [ "$(grep -c 'wl_keyboard#[0-9]*\.key(' "$WTRACE")" -gt "$keys_before" ] && ok=1
check app_gets_it_back $ok "window enters $window_enters -> $(enters "$WTRACE"); \
keys after the close: $(( $(grep -c 'wl_keyboard#[0-9]*\.key(' "$WTRACE") - keys_before ))"

exit $status

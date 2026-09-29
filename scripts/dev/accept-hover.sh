#!/usr/bin/env bash
# Real-input hover acceptance: examples/demo_hover's two stacked cards on a
# headless KWin (kwin-virtual.sh), driven by fake-input.sh. Never the user's session, never a
# nested cosmic-comp (fake input cannot reach it).
#
#   scripts/dev/accept-hover.sh          builds demo_hover, runs the steps, stops everything
#   HOVER_TRACE=1 scripts/dev/accept-hover.sh   the same with WAYLAND_DEBUG=1 on demo_hover
#
# Steps (the cards are 320 x 120, centred on a 1920 x 1080 output: 800..1120 x 480..600):
#   enter_one_motion  one motion from the desktop onto `top`: `top enter`
#   unmap_under       `hide top` with the pointer still: `top leave`, then `bottom enter`,
#                     without the pointer moving (the enter KWin sends carries the position)
#   remap_under       `show top` with the pointer still: `bottom leave`, `top enter`; the
#                     re-mapped surface's document hovers afresh
#   slide_under_resting  the pointer rests on the `slide` surface's pad (bottom-left,
#                     0..320 x 960..1080); 600 ms later its slider is placed under it; after a
#                     wait and a 1 px nudge, `stats` must show the slider entered exactly once
#                     and the pad left once (Blitz's refresh_hover moved the hover silently)
# Each step prints PASS/FAIL with the lines demo_hover printed during it; the log is
# scripts/dev/out/hover.log.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEV="$ROOT/scripts/dev"
OUT="$DEV/out"
mkdir -p "$OUT"
LOG="$OUT/hover.log"
BIN="$ROOT/target/debug/examples/demo_hover"
CX=960 CY=540
DEMO="" INPUT=""

cargo build --manifest-path "$ROOT/Cargo.toml" --example demo_hover >&2 || exit 1

"$DEV/kwin-virtual.sh" start 1920x1080 >&2 || exit 1

stop_all() {
  exec 3>&- 4>&- 2>/dev/null
  for pid in $DEMO $INPUT; do
    for _ in $(seq 50); do kill -0 "$pid" 2>/dev/null || break; sleep 0.1; done
    kill "$pid" 2>/dev/null
  done
  rm -f "$OUT/hover-input.fifo" "$OUT/hover-demo.fifo"
  "$DEV/kwin-virtual.sh" stop >&2
}
trap stop_all EXIT

rm -f "$OUT/hover-input.fifo" "$OUT/hover-demo.fifo"
mkfifo "$OUT/hover-input.fifo" "$OUT/hover-demo.fifo"
: >"$LOG"
(
  # shellcheck disable=SC1091
  source "$DEV/.kwin-env"
  [ "${HOVER_TRACE:-}" = 1 ] && export WAYLAND_DEBUG=1
  exec "$BIN" <"$OUT/hover-demo.fifo" >>"$LOG" 2>&1
) &
DEMO=$!
exec 4>"$OUT/hover-demo.fifo"
"$DEV/fake-input.sh" <"$OUT/hover-input.fifo" >"$OUT/hover-input.log" 2>&1 &
INPUT=$!
exec 3>"$OUT/hover-input.fifo"

# The fake-input commands "$@"; returns once each is answered (`ok <line>`).
input() {
  local before
  before=$(grep -c '^ok ' "$OUT/hover-input.log")
  printf '%s\n' "$@" >&3
  for _ in $(seq 200); do
    [ "$(grep -c '^ok ' "$OUT/hover-input.log")" -ge $((before + $#)) ] && return 0
    sleep 0.05
  done
  echo "fake-input did not answer $*" >&2
  return 1
}

mark=0
# Start a step: remember where the log is.
begin() { mark=$(wc -l <"$LOG"); }
# The hover lines since `begin`, one per line, without the prefix.
since() { tail -n +$((mark + 1)) "$LOG" | sed -n 's/^hover: //p'; }
# PASS if the lines since `begin` contain each of "$@" in order.
expect() {
  local name=$1 got pattern
  shift
  got=$(since | tr '\n' ',' | sed 's/,$//')
  pattern=$(printf '%s.*' "$@")
  if printf '%s' "$got" | grep -q "${pattern%.\*}"; then
    echo "PASS $name: $got"
  else
    echo "FAIL $name: $got (wanted, in order: $*)"
    status=1
  fi
}

status=0
sleep 2
# The seat's pointer arrives with the input client's device: the first motion only makes it
# known. Rest it on the desktop, away from the cards.
input "move 100 100" "sleep 200" "move 101 100" "sleep 300" || exit 1

begin
input "move $CX $CY" "sleep 400" || exit 1
expect enter_one_motion "top enter"
input "move $((CX + 1)) $CY" "sleep 300" || exit 1
echo "     after a second motion: $(since | tr '\n' ',' | sed 's/,$//')"

begin
echo "hide top" >&4
sleep 0.8
expect unmap_under "hide top" "top leave" "bottom enter"

begin
echo "show top" >&4
sleep 0.8
expect remap_under "show top" "bottom leave" "top enter"
input "move $((CX + 2)) $CY" "sleep 300" || exit 1
echo "     after a nudge: $(since | tr '\n' ',' | sed 's/,$//')"

begin
input "move 160 1020" "sleep 1500" || exit 1
input "move 161 1020" "sleep 300" || exit 1
echo stats >&4
sleep 0.5
expect slide_under_resting "slide pad enter" "slide placed" "slide pad leave" "slide slider enter"
stats=$(since | sed -n 's/^stats //p' | tail -n 1)
if printf '%s' "$stats" | grep -q '"pad":{"enter":1,"leave":1},"slider":{"enter":1,"leave":0}'; then
  echo "PASS slide_counts: $stats"
else
  echo "FAIL slide_counts: $stats (wanted pad enter 1 leave 1, slider enter 1 leave 0)"
  status=1
fi

echo quit >&4
exit $status

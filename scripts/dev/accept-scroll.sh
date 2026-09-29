#!/usr/bin/env bash
# Real-input scroll acceptance (design/11 §11.8 items 12 and 14):
# examples/demo_scroll on a headless KWin (kwin-virtual.sh), wheel events from fake-input.sh's
# `axis`. Never the user's session, never a keyboard grab.
#
#   scripts/dev/accept-scroll.sh              builds demo_scroll, runs the steps, stops everything
#   SCROLL_TRACE=1 scripts/dev/accept-scroll.sh   the same with WAYLAND_DEBUG=1 on demo_scroll
#
# The surface is 400 x 600, centred on a 1920 x 1080 output: 760..1160 x 240..840. Its list
# (`#list`, 300 x 600, 3000 px of rows) is at 760..1060; `#capture` (`data-wheel="capture"`) is
# at 1060..1160 x 240..340.
#
# Steps:
#   one_detent       one wheel detent over the list: the list's offset (as `use_scroll` reports
#                    it) is exactly 60 px
#   five_detents     five detents 10 ms apart: +300 px, one burst on one target
#   idle             1.5 s after the last change (the thumb's 500 ms hold and 240 ms fade are
#                    over), then 5 s with no input: 0 frames and 0 commits in the 5 s
#   capture          two detents over `#capture`: its onwheel sees both, the list does not move
# Each step prints PASS/FAIL; the log is scripts/dev/out/scroll.log.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEV="$ROOT/scripts/dev"
OUT="$DEV/out"
mkdir -p "$OUT"
LOG="$OUT/scroll.log"
BIN="$ROOT/target/debug/examples/demo_scroll"
LIST_X=900 LIST_Y=500 CAP_X=1110 CAP_Y=290
DEMO="" INPUT=""

cargo build --manifest-path "$ROOT/Cargo.toml" --example demo_scroll >&2 || exit 1

"$DEV/kwin-virtual.sh" start 1920x1080 >&2 || exit 1

stop_all() {
  exec 3>&- 4>&- 2>/dev/null
  for pid in $DEMO $INPUT; do
    for _ in $(seq 50); do kill -0 "$pid" 2>/dev/null || break; sleep 0.1; done
    kill "$pid" 2>/dev/null
  done
  rm -f "$OUT/scroll-input.fifo" "$OUT/scroll-demo.fifo"
  "$DEV/kwin-virtual.sh" stop >&2
}
trap stop_all EXIT

rm -f "$OUT/scroll-input.fifo" "$OUT/scroll-demo.fifo"
mkfifo "$OUT/scroll-input.fifo" "$OUT/scroll-demo.fifo"
: >"$LOG"
(
  # shellcheck disable=SC1091
  source "$DEV/.kwin-env"
  [ "${SCROLL_TRACE:-}" = 1 ] && export WAYLAND_DEBUG=1
  exec "$BIN" <"$OUT/scroll-demo.fifo" >>"$LOG" 2>&1
) &
DEMO=$!
exec 4>"$OUT/scroll-demo.fifo"
"$DEV/fake-input.sh" <"$OUT/scroll-input.fifo" >"$OUT/scroll-input.log" 2>&1 &
INPUT=$!
exec 3>"$OUT/scroll-input.fifo"

# The fake-input commands "$@"; returns once each is answered (`ok <line>`).
input() {
  local before
  before=$(grep -c '^ok ' "$OUT/scroll-input.log")
  printf '%s\n' "$@" >&3
  for _ in $(seq 200); do
    [ "$(grep -c '^ok ' "$OUT/scroll-input.log")" -ge $((before + $#)) ] && return 0
    sleep 0.05
  done
  echo "fake-input did not answer $*" >&2
  return 1
}

# The demo's latest `stats` JSON, after asking for it.
stats() {
  echo stats >&4
  sleep 0.3
  sed -n 's/^scroll: stats //p' "$LOG" | tail -n 1
}
field() { printf '%s' "$1" | sed -n "s/.*\"$2\":\([-0-9.]*\).*/\1/p"; }

status=0
check() {
  local name=$1 got=$2 want=$3 detail=$4
  if [ "$got" = "$want" ]; then
    echo "PASS $name: $detail"
  else
    echo "FAIL $name: got $got, wanted $want ($detail)"
    status=1
  fi
}

sleep 2
# The seat's pointer arrives with the input client's device: the first motion only makes it
# known. Rest it over the list.
input "move 100 100" "sleep 200" "move $LIST_X $LIST_Y" "sleep 300" "move $((LIST_X + 1)) $LIST_Y" "sleep 300" || exit 1
s=$(stats)
check start "$(field "$s" offset)" 0 "$s"

input "axis vertical 15" "sleep 500" || exit 1
s=$(stats)
check one_detent "$(field "$s" offset)" 60 "$s"

input "axis vertical 15" "sleep 10" "axis vertical 15" "sleep 10" "axis vertical 15" "sleep 10" \
  "axis vertical 15" "sleep 10" "axis vertical 15" "sleep 600" || exit 1
s=$(stats)
check five_detents "$(field "$s" offset)" 360 "$s"

sleep 1.5
before=$(stats)
sleep 5
after=$(stats)
frames=$(( $(field "$after" frames) - $(field "$before" frames) ))
commits=$(( $(field "$after" commits) - $(field "$before" commits) ))
check idle "$frames/$commits" "0/0" "5 s idle: $before -> $after"

input "move $CAP_X $CAP_Y" "sleep 300" "axis vertical 15" "sleep 100" "axis vertical 15" "sleep 500" || exit 1
s=$(stats)
check capture "$(field "$s" captured)/$(field "$s" offset)" "2/360" "$s"

echo quit >&4
exit $status

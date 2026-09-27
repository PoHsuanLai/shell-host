#!/usr/bin/env bash
# A swiped banner's regions follow it (FINDINGS F73, sill G410): examples/demo_swipe (a 400 x 120
# Overlay layer anchored top-right whose blur and input regions follow `#card`) on a headless
# KWin (kwin-virtual.sh), dragged by fake-input.sh. Never the user's session. The regions are
# read from the demo's WAYLAND_DEBUG trace: each `set_input_region` / `set_blur_region` and the
# `wl_region.add` that built it.
#
#   scripts/dev/accept-swipe.sh
#
# Steps, each PASS/FAIL:
#   at_rest        the input region is the card's laid-out box, (20, 20) 300x80
#   follows_drag   while the button is held and the card dragged right, the input region moves
#                  with it (its x is 20 + the drag's dx, within 1 px), several times
#   follows_exit   after the release the exit animation carries the region right in several
#                  frame-by-frame steps, and it ends off the card's old place (x >= 320)
#   blur_follows   the blur region matched the input region every time both were sent
#                  (skipped, not failed, without ext-background-effect-v1)
# Logs and the trace: scripts/dev/out/swipe-*.log.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEV="$ROOT/scripts/dev"
OUT="$DEV/out"
mkdir -p "$OUT"
cargo build --manifest-path "$ROOT/Cargo.toml" -p shell-host --example demo_swipe >&2 || exit 1
DEMO_BIN="$ROOT/target/debug/examples/demo_swipe"
LOG="$OUT/swipe-demo.log"
TRACE="$OUT/swipe-demo-trace.log"
INPUT_LOG="$OUT/swipe-input.log"
DEMO="" INPUT=""
WIDTH=1280

"$DEV/kwin-virtual.sh" start "${WIDTH}x800" >&2 || exit 1

stop_all() {
  [ -n "$DEMO" ] && printf 'quit\n' >&4 2>/dev/null
  exec 3>&- 4>&- 2>/dev/null
  for pid in $DEMO $INPUT; do
    for _ in $(seq 30); do kill -0 "$pid" 2>/dev/null || break; sleep 0.1; done
    kill "$pid" 2>/dev/null
  done
  rm -f "$OUT/swipe-input.fifo" "$OUT/swipe-demo.fifo"
  "$DEV/kwin-virtual.sh" stop >&2
}
trap stop_all EXIT

rm -f "$OUT/swipe-input.fifo" "$OUT/swipe-demo.fifo"
mkfifo "$OUT/swipe-input.fifo" "$OUT/swipe-demo.fifo"
: >"$LOG"
(
  # shellcheck disable=SC1091
  source "$DEV/.kwin-env"
  WAYLAND_DEBUG=1 exec "$DEMO_BIN" <"$OUT/swipe-demo.fifo" >"$LOG" 2>"$TRACE"
) &
DEMO=$!
exec 4>"$OUT/swipe-demo.fifo"
"$DEV/fake-input.sh" <"$OUT/swipe-input.fifo" >"$INPUT_LOG" 2>&1 &
INPUT=$!
exec 3>"$OUT/swipe-input.fifo"
sleep 2.5

status=0
check() {
  case "$2" in
    1) echo "PASS $1: $3" ;;
    skip) echo "SKIP $1: $3" ;;
    *) echo "FAIL $1: $3"; status=1 ;;
  esac
}

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

# Every region the demo sent, in order: `input X Y W H` or `blur X Y W H` (the region's last
# add before the set; each is one rectangle, radius 0), `input none` for an empty one.
regions() {
  awk '
    /-> wl_region#[0-9]+\.add\(/ {
      match($0, /wl_region#[0-9]+/); id = substr($0, RSTART, RLENGTH)
      args = $0; sub(/.*\.add\(/, "", args); sub(/\).*/, "", args); gsub(/ /, "", args)
      rect[id] = args
    }
    /-> wl_region#[0-9]+\.destroy\(/ { next }
    /-> wl_surface#[0-9]+\.set_input_region\(/ || /\.set_blur_region\(/ {
      kind = ($0 ~ /set_input_region/) ? "input" : "blur"
      if (match($0, /wl_region#[0-9]+/)) {
        id = substr($0, RSTART, RLENGTH)
        r = (id in rect) ? rect[id] : "none"; gsub(/,/, " ", r); print kind, r
      } else print kind, "none"
    }' "$TRACE"
}
input_xs() { regions | awk '$1 == "input" && NF == 5 { print $2 }'; }

# The card's layer is anchored top-right: surface x 0 is screen x WIDTH - 400.
sx() { echo $(( WIDTH - 400 + $1 )); }

# The first region with a rectangle (another wl_surface of the demo's, with no input, is set an
# empty one before the layer maps).
first=$(regions | awk '$1 == "input" && NF == 5' | head -n 1)
ok=0; [ "$first" = "input 20 20 300 80" ] && ok=1
check at_rest $ok "first input region: ${first:-none}"
before_drag=$(input_xs | wc -l)

# Press on the card, drag right in 10 px steps (a frame's worth apart), hold.
input "move $(sx 100) 60" "sleep 100" "press left" "sleep 60"
for step in 1 2 3 4 5 6 7 8; do
  input "move $(sx $(( 100 + step * 10 ))) 60" "sleep 50"
done
sleep 0.3
drags=$(grep -c '"event":"drag"' "$LOG")
last_dx=$(grep '"event":"drag"' "$LOG" | tail -n 1 | sed -n 's/.*"dx":\([0-9.]*\).*/\1/p')
during=$(input_xs | tail -n +$(( before_drag + 1 )))
moved=$(printf '%s\n' "$during" | awk 'NF && $1 > 20' | sort -un | wc -l)
held_x=$(printf '%s\n' "$during" | tail -n 1)
want=$(awk -v dx="${last_dx:-0}" 'BEGIN { printf "%d", 20 + dx }')
ok=0
[ "$moved" -ge 3 ] && [ -n "$held_x" ] && [ $(( held_x - want )) -le 1 ] && [ $(( want - held_x )) -le 1 ] && ok=1
check follows_drag $ok "$drags drag line(s), last dx ${last_dx:-?}; input x while held: \
$(printf '%s ' $during)(want last ~$want)"
before_exit=$(input_xs | wc -l)

input "release left" "sleep 800"
released=$(grep '"event":"released"' "$LOG" | tail -n 1)
after=$(input_xs | tail -n +$(( before_exit + 1 )))
steps=$(printf '%s\n' "$after" | awk 'NF' | sort -un | wc -l)
end_x=$(printf '%s\n' "$after" | tail -n 1)
ok=0
[ "$steps" -ge 3 ] && [ -n "$end_x" ] && [ "$end_x" -ge 320 ] && ok=1
check follows_exit $ok "${released:-no release line}; input x after release: $(printf '%s ' $after)"

if grep -q 'set_blur_region(' "$TRACE"; then
  mismatch=$(regions | awk '
    $1 == "input" { input = $2 " " $3 " " $4 " " $5 }
    $1 == "blur" { blurs++; if (input != "" && $2 " " $3 " " $4 " " $5 != input) bad++ }
    END { printf "%d %d", blurs, bad }')
  read -r blurs bad <<<"$mismatch"
  ok=0; [ "$blurs" -ge 3 ] && [ "$bad" = 0 ] && ok=1
  check blur_follows $ok "$blurs blur region(s), $bad differing from the input region sent before"
else
  check blur_follows skip "the compositor offers no ext-background-effect-v1 (no set_blur_region)"
fi

exit $status

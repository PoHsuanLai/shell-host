#!/usr/bin/env bash
# Real-input drag-source acceptance: examples/demo_drag's thumbnail on a headless
# KWin (kwin-virtual.sh), driven by fake-input.sh, with a foot terminal in the same session as
# the drop target. Never the user's session (every client here sources the compositor's env file), no keyboard
# grab. demo_drag and foot both run with WAYLAND_DEBUG=1, so each side's protocol traffic is in
# its own log.
#
#   scripts/dev/accept-drag.sh              builds demo_drag, runs the steps, stops everything
#   DRAG_ICON=root scripts/dev/accept-drag.sh   the same with a DragIcon::Root document
#   COMPOSITOR=casement scripts/dev/accept-drag.sh   the same on casement, in a jail
#
# foot: `foot` on PATH, or FOOT=/path/to/foot; failing both, the Fedora packages foot and fcft
# are fetched with `dnf download` (no root) and unpacked into scripts/dev/out/foot. That happens
# before a jail is entered (nothing in the jail has the network), and never in the jail.
#
# The thumbnail is 64 x 64 at the output's bottom-right corner (margin 40): 1816..1880 x
# 976..1040 on the 1920 x 1080 output. Steps:
#   cancel_over_nothing  press on the thumbnail centre (1848, 1008), move 40 px right and
#                        release at (1888, 1008), past its edge, over the bare desktop:
#                        `outcome {"kind":"cancelled"}` and the compositor's
#                        wl_data_source.cancelled in demo_drag's trace
#   drop_on_foot         press, drag onto foot's window, release: `outcome {"kind":"dropped",..}`,
#                        dnd_drop_performed, send("text/uri-list") and dnd_finished in our trace;
#                        foot's own trace (accept/receive/finish on its wl_data_offer) and the
#                        text foot typed into its terminal (the dragged file's path)
# Each step prints PASS/FAIL; logs are scripts/dev/out/drag*.log.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEV="$ROOT/scripts/dev"
# shellcheck source=scripts/dev/lib/compositor.sh
source "$DEV/lib/compositor.sh"
FOOT_ROOT="$DEV/out"
DEMO="" INPUT="" FOOT_PID=""

find_foot() {
  if [ -n "${FOOT:-}" ]; then echo "$FOOT"; return 0; fi
  if command -v foot >/dev/null; then command -v foot; return 0; fi
  local dir="$FOOT_ROOT/foot"
  if comp_in_jail && [ ! -x "$dir/usr/bin/foot" ]; then
    echo "no foot in the jail: set FOOT=/path/to/foot, or run this script once outside it" >&2
    return 1
  fi
  if [ ! -x "$dir/usr/bin/foot" ]; then
    mkdir -p "$dir" && (cd "$dir" && timeout 300 dnf download --arch x86_64 foot fcft >&2 \
      && for rpm in ./*.rpm; do rpm2cpio "$rpm" | cpio -idm 2>/dev/null; done) || return 1
  fi
  echo "$dir/usr/bin/foot"
}
FOOT_BIN="$(find_foot)" || { echo "no foot: set FOOT=/path/to/foot" >&2; exit 1; }
FOOT_LIBS="$(dirname "$(dirname "$FOOT_BIN")")/lib64"
export FOOT="$FOOT_BIN"

comp_build --manifest-path "$ROOT/Cargo.toml" --example demo_drag || exit 1
comp_enter "$0" "$@"
OUT="$(comp_out)"
mkdir -p "$OUT"
LOG="$OUT/drag.log"
FOOT_LOG="$OUT/drag-foot.log"
TYPED="$OUT/drag-foot-typed.txt"
BIN="$ROOT/target/debug/examples/demo_drag"

comp_start 1920x1080 || exit 1

stop_all() {
  exec 3>&- 4>&- 2>/dev/null
  for pid in $DEMO $INPUT; do
    for _ in $(seq 50); do kill -0 "$pid" 2>/dev/null || break; sleep 0.1; done
    kill "$pid" 2>/dev/null
  done
  [ -n "$FOOT_PID" ] && kill "$FOOT_PID" 2>/dev/null
  rm -f "$OUT/drag-input.fifo" "$OUT/drag-demo.fifo"
  comp_stop
}
trap stop_all EXIT

rm -f "$OUT/drag-input.fifo" "$OUT/drag-demo.fifo" "$TYPED"
mkfifo "$OUT/drag-input.fifo" "$OUT/drag-demo.fifo"
: >"$LOG"
: >"$TYPED"
(
  comp_source
  export WAYLAND_DEBUG=1
  # shellcheck disable=SC2086
  exec "$BIN" --icon="${DRAG_ICON:-none}" <"$OUT/drag-demo.fifo" >>"$LOG" 2>&1
) &
DEMO=$!
exec 4>"$OUT/drag-demo.fifo"
(
  comp_source
  export WAYLAND_DEBUG=1 LD_LIBRARY_PATH="$FOOT_LIBS${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
  unset DISPLAY
  # Whatever foot's terminal receives, byte for byte (no line discipline, no echo).
  exec "$FOOT_BIN" --config=/dev/null --term=xterm-256color --app-id=drag-target \
    sh -c 'stty raw -echo; exec cat >"$0"' "$TYPED" >"$FOOT_LOG" 2>&1
) &
FOOT_PID=$!
"$DEV/lib/input.sh" <"$OUT/drag-input.fifo" >"$OUT/drag-input.log" 2>&1 &
INPUT=$!
exec 3>"$OUT/drag-input.fifo"

# The fake-input commands "$@"; returns once each is answered (`ok <line>`).
input() {
  local before
  before=$(grep -c '^ok ' "$OUT/drag-input.log")
  printf '%s\n' "$@" >&3
  for _ in $(seq 400); do
    [ "$(grep -c '^ok ' "$OUT/drag-input.log")" -ge $((before + $#)) ] && return 0
    sleep 0.05
  done
  echo "fake-input did not answer $*" >&2
  return 1
}

# A press at ($1,$2), a first 10 px step that crosses the 8 px threshold, then a glide in
# ten steps to ($3,$4) and a release there.
drag() {
  local x0=$1 y0=$2 x1=$3 y1=$4 i steps=()
  for i in $(seq 1 10); do
    steps+=("move $((x0 + 10 + (x1 - x0 - 10) * i / 10)) $((y0 + (y1 - y0) * i / 10))" "sleep 60")
  done
  input "move $x0 $y0" "sleep 300" "press left" "sleep 150" "move $((x0 + 10)) $y0" "sleep 250" \
    "${steps[@]}" "sleep 300" "release left" "sleep 1500"
}

mark=0 trace_mark=0 foot_mark=0
begin() {
  mark=$(wc -l <"$LOG")
  foot_mark=$(wc -l <"$FOOT_LOG")
}
# demo_drag's own lines since `begin`, without the prefix; its trace; foot's trace.
since() { tail -n +$((mark + 1)) "$LOG" | sed -n 's/^drag: //p'; }
trace() { tail -n +$((mark + 1)) "$LOG" | grep -E '^\[' ; }
foot_trace() { tail -n +$((foot_mark + 1)) "$FOOT_LOG" | grep -E '^\[' ; }
check() {
  local name=$1 ok=$2 detail=$3
  if [ "$ok" = 1 ]; then echo "PASS $name: $detail"; else echo "FAIL $name: $detail"; status=1; fi
}
has() { grep -E "$1" >/dev/null; }

status=0
sleep 2.5
input "move 900 500" "sleep 200" "move 901 500" "sleep 300" || exit 1
frame=$("$DEV/lib/windows.sh" --class drag-target | sed -n 's/.*"frame":\[\([^]]*\)\].*/\1/p' | head -n 1)
if [ -z "$frame" ]; then echo "FAIL setup: no foot window (see $FOOT_LOG)"; exit 1; fi
IFS=, read -r FX FY FW FH <<<"$frame"
echo "     foot frame: $FX,$FY ${FW}x$FH"
TX=1848 TY=1008

begin
drag "$TX" "$TY" $((TX + 40)) "$TY" || exit 1
echo stats >&4
sleep 0.5
lines=$(since | tr '\n' '|')
ok=0
# Released off every surface of ours: our own data device sees the drag cross the thumbnail
# (enter, motions, leave); by the end every enter has had its leave, and the pointer did not
# come back to the thumbnail before it.
during=$(trace | sed -n '/start_drag(/,/cancelled()/p')
dd_enters=$(printf '%s\n' "$during" | grep -cE 'wl_data_device#[0-9]+\.enter\(')
dd_leaves=$(printf '%s\n' "$during" | grep -cE 'wl_data_device#[0-9]+\.leave\(')
if since | has '^outcome \{"kind":"cancelled"\}' && trace | has 'wl_data_source#[0-9]+\.cancelled\(\)' \
  && trace | has 'start_drag\(' && [ "$dd_enters" = "$dd_leaves" ] \
  && ! printf '%s\n' "$during" | has 'wl_pointer#[0-9]+\.enter'; then
  ok=1
fi
check cancel_over_nothing $ok "$lines our device: $dd_enters enter, $dd_leaves leave; trace: $(trace | grep -oE 'wl_data_(source|device)#[0-9]+\.(start_drag|cancelled|dnd_[a-z_]+|action|send)\([^)]*\)' | tr '\n' ' ')"

begin
drag "$TX" "$TY" $((FX + FW / 2)) $((FY + FH / 2)) || exit 1
echo stats >&4
sleep 0.5
lines=$(since | tr '\n' '|')
ok=0
if since | has '^outcome \{"kind":"dropped"' && trace | has 'dnd_drop_performed\(\)' \
  && trace | has 'dnd_finished\(\)' && trace | has 'send\("text/uri-list"'; then ok=1; fi
check drop_on_foot $ok "$lines trace: $(trace | grep -oE 'wl_data_(source|device)#[0-9]+\.(start_drag|cancelled|dnd_[a-z_]+|action|send)\([^)]*\)' | tr '\n' ' ')"
echo "     foot's side: $(foot_trace | grep -oE 'wl_data_(offer|device)#[0-9]+\.(enter|drop|leave|offer|source_actions|action|accept|receive|finish|set_actions)\([^)]*\)' | sed 's/, fd [0-9]*//' | sort | uniq -c | tr -s ' ' | tr '\n' ';')"
file=$(sed -n 's/^drag: file //p' "$LOG" | head -n 1)
typed=$(cat "$TYPED")
ok=0
[ -n "$file" ] && printf '%s' "$typed" | grep -qF "$file" && ok=1
check foot_typed_the_path $ok "foot's terminal received: $(printf '%q' "$typed")"

echo quit >&4
exit $status

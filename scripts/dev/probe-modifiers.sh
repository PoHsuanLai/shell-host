#!/usr/bin/env bash
# Which modifier state reaches a keyboard-less layer surface (FINDINGS F61): examples/demo_drag's
# 64 x 64 thumbnail (bottom-right, 1816..1880 x 976..1040) on a headless KWin
# (kwin-virtual.sh), with fake-input.sh holding Super while the pointer arrives on it. Never the
# user's session, no keyboard grab.
#
#   scripts/dev/probe-modifiers.sh             KeyboardMode::None, then OnDemand, each alone and
#                                              then with a foot window holding keyboard focus
#
# foot is found as in accept-drag.sh (PATH, FOOT=, or scripts/dev/out/foot). Per case it prints every wl_keyboard event demo_drag received (WAYLAND_DEBUG=1), the
# modifiers the document saw on its mouse events (`drag: mods enter|move ..`), and KWin's
# keyboard focus. Nothing passes or fails: it is a probe, and F61 records what it showed.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEV="$ROOT/scripts/dev"
OUT="$DEV/out"
mkdir -p "$OUT"
BIN="$ROOT/target/debug/examples/demo_drag"
FOOT_BIN="${FOOT:-$(command -v foot || echo "$OUT/foot/usr/bin/foot")}"
FOOT_LIBS="$(dirname "$(dirname "$FOOT_BIN")")/lib64"
cargo build --manifest-path "$ROOT/Cargo.toml" --example demo_drag >&2 || exit 1

probe() {
  local mode=$1 focus=$2 log="$OUT/modifiers-$1-$2.log" demo input foot=""
  "$DEV/kwin-virtual.sh" start 1920x1080 >&2 || return 1
  if [ "$focus" = foot ]; then
    (
      # shellcheck disable=SC1091
      source "$DEV/.kwin-env"
      export LD_LIBRARY_PATH="$FOOT_LIBS${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
      unset DISPLAY
      exec "$FOOT_BIN" --config=/dev/null --term=xterm-256color --app-id=focus-holder \
        sh -c 'sleep 60' >/dev/null 2>&1
    ) &
    foot=$!
    sleep 1.5
  fi
  rm -f "$OUT/mods-input.fifo" "$OUT/mods-demo.fifo"
  mkfifo "$OUT/mods-input.fifo" "$OUT/mods-demo.fifo"
  (
    # shellcheck disable=SC1091
    source "$DEV/.kwin-env"
    export WAYLAND_DEBUG=1
    exec "$BIN" --keyboard="$mode" <"$OUT/mods-demo.fifo" >"$log" 2>&1
  ) &
  demo=$!
  exec 4>"$OUT/mods-demo.fifo"
  "$DEV/fake-input.sh" <"$OUT/mods-input.fifo" >"$OUT/mods-input.log" 2>&1 &
  input=$!
  exec 3>"$OUT/mods-input.fifo"
  sleep 2.5
  # Pointer on the desktop, Super down, onto the thumbnail, a move on it, Super up, a move.
  printf '%s\n' "move 900 500" "sleep 200" "move 901 500" "sleep 300" \
    "key Super_L down" "sleep 300" "move 1848 1008" "sleep 300" "move 1850 1010" "sleep 300" \
    "key Super_L up" "sleep 300" "move 1852 1012" "sleep 300" \
    "key Shift_L down" "sleep 200" "move 1854 1014" "sleep 200" "key Shift_L up" "sleep 300" >&3
  for _ in $(seq 100); do [ "$(grep -c '^ok ' "$OUT/mods-input.log")" -ge 20 ] && break; sleep 0.1; done
  echo quit >&4
  exec 3>&- 4>&-
  sleep 1
  kill "$demo" "$input" $foot 2>/dev/null
  echo "== KeyboardMode $mode, keyboard focus: $([ "$focus" = foot ] && echo "a foot window" || echo "no other window")"
  echo "   wl_keyboard events (and wl_pointer enter/leave) received:"
  grep -E '^\[[^]]*\] wl_(keyboard#|pointer#[0-9]+\.(enter|leave))' "$log" | sed -E 's/^\[[^]]*\] /     /; s/keymap\(1, fd [0-9]+, [0-9]+\)/keymap(xkb_v1, fd, ..)/'
  echo "   the document's pointer events:"
  sed -n 's/^drag: mods /     /p' "$log"
  "$DEV/kwin-virtual.sh" stop >&2
}

for focus in alone foot; do
  probe none "$focus"
  probe on-demand "$focus"
done

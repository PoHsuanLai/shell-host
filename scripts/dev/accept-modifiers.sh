#!/usr/bin/env bash
# Real-input `use_modifiers` acceptance: examples/demo_switcher's Exclusive-
# keyboard panel on a headless KWin (kwin-virtual.sh), driven by fake-input.sh. Never the user's
# session (every client here sources .kwin-env), no keyboard grab.
#
#   scripts/dev/accept-modifiers.sh
#   COMPOSITOR=casement scripts/dev/accept-modifiers.sh   the same on casement, in a jail
#
# Three cases, each its own compositor instance (kwin-virtual.sh; casement: its own dev/up.sh
# run, so neither's modifier state leaks into the other) and its own demo_switcher, mapped only
# after the modifier state is set up first. Every run prints a "modifiers" line at the panel's very first render, always the default (nothing
# held or locked yet: it predates the surface's first configure, let alone a keyboard event) —
# that is `use_modifiers`' documented default, not the thing under test. What is under test is
# the *next* line, from `wl_keyboard.enter` and the `modifiers` event beside it:
#   super       Super held, then `map`: the next line already has logo held — not a key event
#               the panel had to wait for. Releasing Super while it is still mapped and focused
#               then produces a further line with logo released.
#   caps_lock   Caps Lock tapped (down, up) before `map`: the next line already has caps_lock
#               locked.
#   restale     Super held, `map`, `unmap` (still held), Super released while the surface
#               is hidden, `map` again with nothing held. `unmap` rebuilds the same surface's
#               `wl_surface` (the unmap that must withdraw the keyboard); a host
#               that never withdrew the keyboard from it would carry the held-at-unmap modifiers
#               into this second opening, a line reading logo held where the physical key is up.
#               `demo_switcher`'s `unmap`/`map` drive this same surface (`Visibility::Hidden`/
#               `Shown`), not a close and a fresh spawn.
# Each case prints PASS/FAIL; logs are scripts/dev/out/modifiers-*.log.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEV="$ROOT/scripts/dev"
# shellcheck source=scripts/dev/lib/compositor.sh
source "$DEV/lib/compositor.sh"
comp_build --manifest-path "$ROOT/Cargo.toml" --example demo_switcher || exit 1
comp_enter "$0" "$@"
OUT="$(comp_out)"
mkdir -p "$OUT"
BIN="$ROOT/target/debug/examples/demo_switcher"

status=0
check() {
  local name=$1 ok=$2 detail=$3
  if [ "$ok" = 1 ]; then echo "PASS $name: $detail"; else echo "FAIL $name: $detail"; status=1; fi
}
# Every "modifiers" line demo_switcher printed, whole (one JSON object per line; serde_json
# does not preserve field order, so "event" is not necessarily first).
modifiers_lines() { grep -F '"event":"modifiers"' "$1"; }

# Every "focus" line (`HostEvent::KeyboardFocus`), just the Gained/Lost states in order, comma
# joined: "Gained,Lost,Gained," across an unmap and a remap when the host withdraws the
# keyboard at the unmap, "Gained,Gained," (no `Lost` between them) when it does not.
focus_states() {
  grep -F '"event":"focus"' "$1" | sed -E 's/.*"state":"([A-Za-z]+)".*/\1/' | tr '\n' ','
}

# Runs demo_switcher and fake-input against a fresh headless KWin, calls the case function
# (which drives fd 3, the fake-input fifo, and fd 4, the demo's stdin fifo, through `input`), and
# tears both down. $1 is the case name (log file and PASS/FAIL prefix), $2 the case function.
run_case() {
  local name=$1 fn=$2
  local log="$OUT/modifiers-$name.log"
  comp_start 1280x800 || return 1
  rm -f "$OUT/mods-input.fifo" "$OUT/mods-demo.fifo"
  mkfifo "$OUT/mods-input.fifo" "$OUT/mods-demo.fifo"
  : >"$log"
  (
    comp_source
    export WAYLAND_DEBUG=1
    comp_as_shell "$BIN" <"$OUT/mods-demo.fifo" >>"$log" 2>&1
  ) &
  local demo=$!
  exec 4>"$OUT/mods-demo.fifo"
  "$DEV/lib/input.sh" <"$OUT/mods-input.fifo" >"$OUT/mods-input.log" 2>&1 &
  local input=$!
  exec 3>"$OUT/mods-input.fifo"

  sleep 2.5
  if ! "$fn" "$log"; then
    check "$name" 0 "case did not complete (see $log)"
  fi

  echo quit >&4
  sleep 0.3
  exec 3>&- 4>&-
  for pid in "$demo" "$input"; do
    for _ in $(seq 50); do kill -0 "$pid" 2>/dev/null || break; sleep 0.1; done
    kill "$pid" 2>/dev/null
  done
  rm -f "$OUT/mods-input.fifo" "$OUT/mods-demo.fifo"
  comp_stop
}

# The fake-input commands "$@"; returns once each is answered (`ok <line>`).
input() {
  local before
  before=$(grep -c '^ok ' "$OUT/mods-input.log")
  printf '%s\n' "$@" >&3
  for _ in $(seq 400); do
    [ "$(grep -c '^ok ' "$OUT/mods-input.log")" -ge $((before + $#)) ] && return 0
    sleep 0.05
  done
  echo "fake-input did not answer $*" >&2
  return 1
}

# At least $2 "modifiers" lines in log $1, or times out after 10 s.
wait_lines() {
  local log=$1 n=$2
  for _ in $(seq 200); do
    [ "$(modifiers_lines "$log" | wc -l)" -ge "$n" ] && return 0
    sleep 0.05
  done
  return 1
}

super_case() {
  local log=$1
  input "move 640 400" "sleep 200" || return 1
  input "key Super_L down" "sleep 200" || return 1
  echo map >&4
  # Line 1: the panel's first render, always the default (before its first configure). Line 2:
  # what `wl_keyboard.enter` (and the `modifiers` event beside it) actually saw.
  wait_lines "$log" 2 || { echo "no modifiers line beyond the pre-focus default after map" >&2; return 1; }
  local seen_on_enter
  seen_on_enter=$(modifiers_lines "$log" | sed -n '2p')
  local ok=0
  echo "$seen_on_enter" | grep -q '"logo":"Held"' && ok=1
  check super_seen_on_enter $ok "line 2: $seen_on_enter"

  input "key Super_L up" "sleep 200" || return 1
  wait_lines "$log" 3 || { echo "no further modifiers line after Super released" >&2; return 1; }
  local after_release
  after_release=$(modifiers_lines "$log" | tail -n 1)
  ok=0
  echo "$after_release" | grep -q '"logo":"Released"' && ok=1
  check super_released_while_focused $ok "line 3: $after_release"
}

caps_lock_case() {
  local log=$1
  input "move 640 400" "sleep 200" || return 1
  input "key Caps_Lock down" "sleep 100" "key Caps_Lock up" "sleep 200" || return 1
  echo map >&4
  wait_lines "$log" 2 || { echo "no modifiers line beyond the pre-focus default after map" >&2; return 1; }
  local seen_on_enter
  seen_on_enter=$(modifiers_lines "$log" | sed -n '2p')
  local ok=0
  echo "$seen_on_enter" | grep -q '"caps_lock":"Locked"' && ok=1
  check caps_lock_seen_on_enter $ok "line 2: $seen_on_enter"

  # Leave the (virtual) compositor's own Caps Lock state as found.
  input "key Caps_Lock down" "sleep 100" "key Caps_Lock up" "sleep 100" || true
}

# The same surface (`Visibility::Hidden`/`Shown`, not a close and respawn) unmapped while
# Super is still held, released while it is hidden, then mapped again with nothing held. A host
# that never withdrew the keyboard at the unmap (never reset the record, never reset the
# surface's published modifiers) risks a second opening that starts from the first opening's
# logo held rather than the physical key's true, released state.
restale_case() {
  local log=$1
  input "move 640 400" "sleep 200" || return 1
  input "key Super_L down" "sleep 200" || return 1
  echo map >&4
  wait_lines "$log" 2 || { echo "no modifiers line beyond the pre-focus default after map" >&2; return 1; }
  local seen_on_enter
  seen_on_enter=$(modifiers_lines "$log" | sed -n '2p')
  local ok=0
  echo "$seen_on_enter" | grep -q '"logo":"Held"' && ok=1
  check restale_seen_on_enter $ok "line 2: $seen_on_enter"

  # Unmap while Super is still held (the compositor's own leave, naming the destroyed
  # wl_surface, can never be matched to anything): a fixed host resets the
  # surface's modifiers to the default right here rather than waiting for it. Super is then
  # released while the surface is hidden — no key event of ours to carry the release, since a
  # hidden surface has no keyboard focus to receive one.
  echo unmap >&4
  sleep 0.3
  input "key Super_L up" "sleep 300" || return 1
  echo map >&4
  # Give a fresh `wl_keyboard.enter` and the `modifiers` event beside it time to land.
  sleep 1.5
  local final
  final=$(modifiers_lines "$log" | tail -n 1)
  ok=0
  echo "$final" | grep -q '"logo":"Released"' && ok=1
  local total
  total=$(modifiers_lines "$log" | wc -l)
  check restale_after_remap $ok "after unmap (Super still held), release while hidden, and remap (nothing held): $final ($total lines total)"

  # The directly discriminating signal: a host that withdraws the keyboard at the unmap
  # tells the shell `Lost` right there, before the second `Gained`; one that
  # leaves the record stale goes straight from `Gained` to `Gained` again, since the
  # compositor's own leave (for the destroyed wl_surface) never matches anything either.
  local focus_seq
  focus_seq=$(focus_states "$log")
  ok=0
  [ "$focus_seq" = "Gained,Lost,Gained," ] && ok=1
  check restale_lost_between_openings $ok "focus events: $focus_seq"
}

run_case super super_case
run_case caps_lock caps_lock_case
run_case restale restale_case

exit $status

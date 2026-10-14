#!/usr/bin/env bash
# Session-lock acceptance: examples/demo_lock locks casement (it implements ext-session-lock-v1; KWin 6.7
# does not, so there is no KWin form of this script) in a jail, driven with lib/input.sh. Never the user's
# session: the compositor's socket lives in a private runtime directory with a marker file, demo_lock's
# guard refuses any other socket, the jail is the second wall, and every exit path unlocks and stops
# the compositor.
#
#   COMPOSITOR=casement scripts/dev/accept-lock.sh   (the default here)
#
# There is no screenshot: casementctl has no `shot` until P1-C1, so what a pixel check would prove (the
# lock covers the session; the session shows again after the unlock) is read from the compositor's
# input instead: while locked, a real click on the session's probe window must not activate it
# (`casementctl debug windows`: active), and after the unlock the same click must. Steps:
#   lock          `locked` from the host, a lock surface per output (stats), the session covered
#   finished      a second demo_lock's lock while the first holds it: LockState::Finished
#   typing        "abc" with no click reaches the focused lock surface's field
#   wrong         Enter on "abc": rejected, still locked, the session still covered
#   idle          0 frames on every lock surface over 3 s
#   unlock        "letmein" + Enter: `unlocked`, the lock surfaces gone, the session answers input again
#   crash         a locker killed while locked: the session stays hidden; what a new locker
#                 gets (locked and unlocks, or finished) is reported, as the compositor's policy
# Each step prints PASS/FAIL; logs are in scripts/dev/out/lock-*.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEV="$ROOT/scripts/dev"
COMPOSITOR="${COMPOSITOR:-casement}"
# shellcheck source=scripts/dev/lib/compositor.sh
source "$DEV/lib/compositor.sh"
[ "$COMPOSITOR" = casement ] || { echo "accept-lock.sh runs on casement only: KWin has no ext-session-lock-v1" >&2; exit 64; }
comp_build --manifest-path "$ROOT/Cargo.toml" -p shell-host --example demo_lock || exit 1
comp_build --manifest-path "$ROOT/Cargo.toml" -p shell-host --features debug-inject \
  --example probe_toplevel || exit 1
comp_enter "$0" "$@"
OUT="$(comp_out)"
mkdir -p "$OUT"
LOG="$OUT/lock.log"
DEMO_BIN="$ROOT/target/debug/examples/demo_lock"
PROBE_BIN="$ROOT/target/debug/examples/probe_toplevel"
SESSION_RUNTIME="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
DEMO="" INPUT="" PROBE="" COMP_SOCKET="" LOCK_SOCKET=""

# The marker file the guard wants; the jail's own runtime directory is the login session's
# `/run/user/<uid>` as far as the guard can tell, which it refuses.
RUNTIME="$JAIL_SCRATCH/lock-runtime"
mkdir -m 700 "$RUNTIME" && : >"$RUNTIME/.shell-host-virtual-compositor" || exit 1
COMP_RUNTIME="$RUNTIME" comp_start 1920x1080 || exit 1

stop_all() {
  # The demo unlocks before it quits (its `quit`); give it the time, then make sure.
  if [ -n "$DEMO" ] && kill -0 "$DEMO" 2>/dev/null; then
    printf 'unlock\nquit\n' >&4 2>/dev/null
    for _ in $(seq 50); do kill -0 "$DEMO" 2>/dev/null || break; sleep 0.1; done
  fi
  exec 3>&- 4>&- 5>&- 2>/dev/null
  for pid in $DEMO $INPUT $PROBE; do
    for _ in $(seq 20); do kill -0 "$pid" 2>/dev/null || break; sleep 0.1; done
    kill "$pid" 2>/dev/null
  done
  rm -f "$OUT/lock-input.fifo" "$OUT/lock-demo.fifo" "$OUT/lock-probe.fifo"
  comp_stop
}
trap stop_all EXIT

COMP_SOCKET="$(comp_socket)"
case "$COMP_SOCKET" in
  "$RUNTIME"/*) ;;
  *) echo "FAIL setup: the compositor's socket $COMP_SOCKET is not in $RUNTIME" >&2; exit 1 ;;
esac

status=0
pass() { echo "PASS $1: $2"; }
fail() { echo "FAIL $1: $2"; status=1; }

# The demo, with the guard's variables and the session's own XDG_RUNTIME_DIR, on socket $1.
demo_env() {
  env -u WAYLAND_SOCKET WAYLAND_DISPLAY="$1" XDG_RUNTIME_DIR="$SESSION_RUNTIME" \
    SHELL_HOST_ALLOW_LOCK=virtual SHELL_HOST_LOCK_RUNTIME="$RUNTIME" \
    DBUS_SESSION_BUS_ADDRESS="$(comp_source; echo "$DBUS_SESSION_BUS_ADDRESS")" \
    "${@:2}"
}

# casement is the compositor that locks.
LOCK_SOCKET="$COMP_SOCKET"
case "$(basename "$LOCK_SOCKET")" in
  wayland-0) echo "FAIL setup: refusing a socket named wayland-0" >&2; exit 1 ;;
esac

rm -f "$OUT/lock-input.fifo" "$OUT/lock-demo.fifo" "$OUT/lock-probe.fifo"
mkfifo "$OUT/lock-input.fifo" "$OUT/lock-demo.fifo" "$OUT/lock-probe.fifo"
# The session's window: a checkerboard toplevel, maximized. Its stdin must be pollable (a
# calloop source), so a fifo held open rather than /dev/null.
env WAYLAND_DISPLAY="$LOCK_SOCKET" XDG_RUNTIME_DIR="$SESSION_RUNTIME" "$PROBE_BIN" \
  <"$OUT/lock-probe.fifo" >"$OUT/lock-probe.log" 2>&1 &
PROBE=$!
exec 5>"$OUT/lock-probe.fifo"

: >"$LOG"
trace=()
[ "${LOCK_TRACE:-}" = 1 ] && trace=(WAYLAND_DEBUG=1)
demo_env "$LOCK_SOCKET" env "${trace[@]}" "$DEMO_BIN" <"$OUT/lock-demo.fifo" >>"$LOG" 2>&1 &
DEMO=$!
exec 4>"$OUT/lock-demo.fifo"
"$DEV/lib/input.sh" <"$OUT/lock-input.fifo" >"$OUT/lock-input.log" 2>&1 &
INPUT=$!
exec 3>"$OUT/lock-input.fifo"

# The fake-input commands "$@"; returns once each is answered (`ok <line>`).
input() {
  local before
  before=$(grep -c '^ok ' "$OUT/lock-input.log")
  printf '%s\n' "$@" >&3
  for _ in $(seq 200); do
    [ "$(grep -c '^ok ' "$OUT/lock-input.log")" -ge $((before + $#)) ] && return 0
    sleep 0.05
  done
  echo "fake-input did not answer $*" >&2
  return 1
}

# Wait up to $2 tenths of a second for a demo line matching $1.
wait_for() {
  for _ in $(seq "${2:-50}"); do
    grep -q -- "$1" "$LOG" && return 0
    sleep 0.1
  done
  return 1
}

# The last `stats` line's JSON, after asking for a fresh one.
stats() {
  local before
  before=$(grep -c '^lock: stats ' "$LOG")
  echo stats >&4
  for _ in $(seq 50); do
    [ "$(grep -c '^lock: stats ' "$LOG")" -gt "$before" ] && break
    sleep 0.1
  done
  grep '^lock: stats ' "$LOG" | tail -n 1 | sed 's/^lock: stats //'
}

# --- what "the session is covered" means ---------------------------------------------------------
# casement has no screenshot until P1-C1 (`casementctl shot`), so the cover is read from its input: a
# real click on the session's probe window must not activate it while the session is locked, and must
# once it is not (`casementctl debug windows`: active). Each check sets EVIDENCE.
EVIDENCE=""

# probe_frame: the probe window's frame as "x y w h" (the first one `windows.sh` lists).
probe_frame() {
  "$DEV/lib/windows.sh" --class shell-host.probe | head -n 1 \
    | sed -n 's/.*"frame":\[\([-0-9]*\),\([-0-9]*\),\([0-9]*\),\([0-9]*\)\].*/\1 \2 \3 \4/p'
}

# probe_active: whether the probe window is the compositor's active one.
probe_active() { "$DEV/lib/windows.sh" --class shell-host.probe | head -n 1 | grep -q '"active":true'; }

# click_probe: a real click on the probe window's centre.
click_probe() {
  local x y w h
  read -r x y w h <<<"$(probe_frame)"
  [ -n "${h:-}" ] || return 1
  input "move $((x + w / 2)) $((y + h / 2))" "sleep 200" "press left" "release left" "sleep 500"
}

# covered TAG: the session is hidden behind the lock.
covered() {
  click_probe
  # The click landed on the lock surface (the probe window is under the lock cover, so a
  # blocked click is the lock document's): Blitz focuses nothing on a click that misses a
  # field, so a Tab puts the keyboard back on the password field.
  input "key Tab down" "key Tab up" "sleep 200"
  if probe_active; then
    EVIDENCE="a click on the session's window activated it (input reached the session; no pixel check until casementctl shot, P1-C1)"
    return 1
  fi
  EVIDENCE="a click on the session's window left it inactive (no pixel check until casementctl shot, P1-C1)"
}

# uncovered TAG: the session shows again, as it did before the lock.
uncovered() {
  click_probe
  kill -0 "$PROBE" 2>/dev/null && probe_active && EVIDENCE="a click on the session's window activated it again" \
    || { EVIDENCE="a click on the session's window did not activate it, or it is gone"; return 1; }
}

wait_for '^lock: ready' 100 || { echo "FAIL setup: demo_lock never became ready" >&2; exit 1; }
grep -q '^lock: guard granted' "$LOG" || { echo "FAIL setup: guard refused on $LOCK_SOCKET" >&2; exit 1; }
sleep 1
# The probe maps unfocused: nothing activates it before the lock (the covered/uncovered
# checks click it), and the frame is where casement put it.
for _ in $(seq 50); do [ -n "$(probe_frame)" ] && break; sleep 0.1; done
[ -n "$(probe_frame)" ] || { echo "FAIL setup: the probe window never mapped" >&2; exit 1; }
echo "probe window: $(probe_frame)" >&2

# --- lock --------------------------------------------------------------------------------------
echo lock >&4
if wait_for '"state":"locked"' 100; then
  sleep 1
  s=$(stats)
  echo "$s" >"$OUT/lock-stats-locked.json"
  outputs=$(sed -n 's/^lock: ready //p' "$LOG" | tail -n 1)
  n_out=$(printf '%s' "$outputs" | tr ',' '\n' | grep -c '"')
  n_surf=$(printf '%s' "$s" | grep -o '"surface":[0-9]*' | wc -l)
  sized=$(printf '%s' "$s" | grep -o '"logical":\[[0-9]*,[0-9]*\]' | head -n 1)
  if covered locked && [ "$n_surf" -eq "$n_out" ] && [ "$n_surf" -ge 1 ]; then
    pass lock "locked; $n_surf lock surface(s) for outputs $outputs, $sized; $EVIDENCE"
  else
    fail lock "surfaces $n_surf for outputs $outputs; $EVIDENCE; $s"
  fi
else
  fail lock "no locked event: $(grep '^lock: ' "$LOG" | tr '\n' '|')"
fi

# --- finished: a second locker while the first holds the lock -----------------------------------
got=$( (echo lock; sleep 3; echo quit) | demo_env "$LOCK_SOCKET" timeout 30 "$DEMO_BIN" 2>&1 \
  | grep '^lock: ')
echo "$got" >"$OUT/lock-second.log"
if printf '%s' "$got" | grep -q '"state":"finished"' && ! grep -q '"state":"unlocked"' "$LOG"; then
  pass finished "a second lock answered $(printf '%s' "$got" | grep -o '{"id":1,"state":"finished"}'), the first still holds it"
else
  fail finished "$(printf '%s' "$got" | tr '\n' '|')"
fi

# --- typing without a click -------------------------------------------------------------------
input "type abc" "sleep 400" || exit 1
s=$(stats)
if printf '%s' "$s" | grep -q '"length":3' && ! printf '%s' "$s" | grep -q '"focus":null'; then
  pass typing "$(printf '%s' "$s" | grep -o '"typed":{[^}]*}}' ) focus $(printf '%s' "$s" | grep -o '"focus":[0-9a-z]*')"
else
  fail typing "$s"
fi

# --- wrong text keeps it locked --------------------------------------------------------------
input "key Return down" "key Return up" "sleep 500" || exit 1
s=$(stats)
if covered wrong && grep -q '^lock: rejected' "$LOG" && printf '%s' "$s" | grep -q '"phase":"locked"' \
   && printf '%s' "$s" | grep -q '"rejected":1' && ! grep -q '"state":"unlocked"' "$LOG"; then
  pass wrong "rejected, still locked, $EVIDENCE"
else
  fail wrong "$EVIDENCE; $s"
fi

# --- idle: 0 frames while locked ---------------------------------------------------------------
sleep 1
a=$(stats | grep -o '"frames":[0-9]*' | tr '\n' ' ')
sleep 3
b=$(stats | grep -o '"frames":[0-9]*' | tr '\n' ' ')
if [ -n "$a" ] && [ "$a" = "$b" ]; then
  pass idle "frames unchanged over 3 s: $a"
else
  fail idle "before [$a] after [$b]"
fi

# --- letmein unlocks ---------------------------------------------------------------------------
input "type letmein" "key Return down" "key Return up" || exit 1
if wait_for '"state":"unlocked"' 50; then
  sleep 1
  s=$(stats)
  if grep -q '^lock: accepted' "$LOG" && printf '%s' "$s" | grep -q '"surfaces":\[\]' && uncovered after; then
    pass unlock "accepted, unlocked, lock surfaces gone, $EVIDENCE"
  else
    fail unlock "$EVIDENCE; $s"
  fi
else
  fail unlock "no unlocked event: $(grep '^lock: ' "$LOG" | tr '\n' '|')"
fi

echo quit >&4
for _ in $(seq 50); do kill -0 "$DEMO" 2>/dev/null || break; sleep 0.1; done

# --- crash: the locker dies while locked --------------------------------------------------------
# The protocol keeps the session locked; a new locker may take over (compositor policy).
exec 4>&-
: >"$OUT/lock-crash.log"
rm -f "$OUT/lock-demo.fifo" && mkfifo "$OUT/lock-demo.fifo"
# A session window that is not active, as the first probe was before the lock: the one the
# unlock check clicked is active now.
echo quit >&5
exec 5>&-
for _ in $(seq 50); do kill -0 "$PROBE" 2>/dev/null || break; sleep 0.1; done
rm -f "$OUT/lock-probe.fifo" && mkfifo "$OUT/lock-probe.fifo"
env WAYLAND_DISPLAY="$LOCK_SOCKET" XDG_RUNTIME_DIR="$SESSION_RUNTIME" "$PROBE_BIN" \
  <"$OUT/lock-probe.fifo" >"$OUT/lock-probe2.log" 2>&1 &
PROBE=$!
exec 5>"$OUT/lock-probe.fifo"
for _ in $(seq 50); do [ -n "$(probe_frame)" ] && break; sleep 0.1; done
demo_env "$LOCK_SOCKET" "$DEMO_BIN" <"$OUT/lock-demo.fifo" >>"$OUT/lock-crash.log" 2>&1 &
DEMO=$!
exec 4>"$OUT/lock-demo.fifo"
LOG="$OUT/lock-crash.log"
wait_for '^lock: ready' 100 && echo lock >&4
if wait_for '"state":"locked"' 100; then
  sleep 1
  # The demo itself, not the subshell that started it: a surviving demo would see its stdin
  # close and unlock on the way out.
  kill -9 "$(sed -n 's/^lock: pid //p' "$LOG" | head -n 1)"
  wait "$DEMO" 2>/dev/null
  exec 4>&-
  DEMO=""
  sleep 1
  hidden=0
  covered crashed && hidden=1
  crash_evidence="$EVIDENCE"
  # A new locker, told to unlock at once (`unlock` is HostCtx::unlock_session).
  got=$( (echo lock; sleep 3; echo unlock; sleep 2; echo quit) \
    | demo_env "$LOCK_SOCKET" timeout 30 "$DEMO_BIN" 2>&1 | grep '^lock: event')
  echo "$got" >"$OUT/lock-recover.log"
  sleep 1
  uncovered recovered && back="the session answers input again" || back="the session still does not answer input"
  if [ "$hidden" = 1 ]; then
    pass crash "killed while locked: still locked ($crash_evidence); a new locker: $(printf '%s' "$got" | sed 's/^lock: event //' | tr '\n' ' '); then $back"
  else
    fail crash "the session showed after the locker died ($crash_evidence)"
  fi
else
  fail crash "the second lock never locked"
fi
exit $status

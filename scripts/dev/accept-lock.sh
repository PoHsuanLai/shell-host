#!/usr/bin/env bash
# Session-lock acceptance: examples/demo_lock locks a throwaway cosmic-comp that
# runs *inside* the headless KWin of kwin-virtual.sh, and is driven with fake-input.sh. Never
# the user's session: KWin gets its own runtime directory (KWIN_VIRTUAL_RUNTIME), the nested
# cosmic-comp puts its socket there too, demo_lock's guard refuses any other socket, and every
# exit path unlocks and stops both compositors.
#
#   scripts/dev/accept-lock.sh
#   COMPOSITOR=casement scripts/dev/accept-lock.sh   on casement, in a jail
#
# COMPOSITOR=casement has no nested compositor: casement itself implements ext-session-lock-v1,
# so demo_lock locks it directly (its socket in a private runtime directory with the marker, so
# the same guard applies; the jail is the second wall). There is no `unsupported` step, and no
# screenshots: casementctl has no `shot` until P1-C1, so what the pixel checks proved (the lock
# covers the session; the session shows again after the unlock) is read from the compositor's
# input instead: while locked, a real click on the session's probe window must not activate it
# (`casementctl debug windows`: active), and after the unlock the same click must. The steps,
# their names and the lock-surface, typing, idle and crash checks are the same.
#
# KWin 6.7 has no ext-session-lock-v1, so the lock itself needs cosmic-comp (winit backend, a
# window of the headless KWin: fake input reaches it as KWin input and cosmic-comp forwards it
# to its own seat). Steps:
#   unsupported   demo_lock on the headless KWin itself: `lock` answers LockError::Unsupported
#   lock          on cosmic-comp: `locked` from the host, a lock surface per output (stats),
#                 and a KWin screenshot of cosmic-comp's window that shows the lock document
#                 where the probe window was
#   finished      a second demo_lock's lock while the first holds it: LockState::Finished
#   typing        "abc" with no click reaches the focused lock surface's field
#   wrong         Enter on "abc": rejected, still locked, the screenshot still the lock's
#   idle          0 frames on every lock surface over 3 s
#   unlock        "letmein" + Enter: `unlocked`, the lock surfaces gone, the screenshot shows
#                 the probe window again
#   crash         a locker killed while locked: the session stays hidden; what a new locker
#                 gets (locked and unlocks, or finished) is reported, as cosmic-comp's policy
# Each step prints PASS/FAIL; logs and screenshots are in scripts/dev/out/lock-*.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEV="$ROOT/scripts/dev"
# shellcheck source=scripts/dev/lib/compositor.sh
source "$DEV/lib/compositor.sh"
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
RUNTIME="$SESSION_RUNTIME/shell-host-lock-$$"
DEMO="" INPUT="" COMP="" PROBE="" KWIN_SOCKET="" COSMIC_SOCKET="" LOCK_SOCKET=""

case "$COMPOSITOR" in
  kwin) KWIN_VIRTUAL_RUNTIME="$RUNTIME" comp_start 1920x1080 || exit 1 ;;
  casement)
    # The marker file the guard wants; the jail's own runtime directory is the login session's
    # `/run/user/<uid>` as far as the guard can tell, which it refuses.
    RUNTIME="$JAIL_SCRATCH/lock-runtime"
    mkdir -m 700 "$RUNTIME" && : >"$RUNTIME/.shell-host-virtual-compositor" || exit 1
    COMP_RUNTIME="$RUNTIME" comp_start 1920x1080 || exit 1
    ;;
esac

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
  [ -n "$COMP" ] && kill "$COMP" 2>/dev/null
  rm -f "$OUT/lock-input.fifo" "$OUT/lock-demo.fifo" "$OUT/lock-probe.fifo"
  comp_stop
}
trap stop_all EXIT

KWIN_SOCKET="$(comp_socket)"
case "$KWIN_SOCKET" in
  "$RUNTIME"/*) ;;
  *) echo "FAIL setup: the compositor's socket $KWIN_SOCKET is not in $RUNTIME" >&2; exit 1 ;;
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

if [ "$COMPOSITOR" = kwin ]; then
  # --- unsupported: the headless KWin has no ext_session_lock_manager_v1 -------------------------
  got=$( (echo lock; sleep 2; echo quit) | demo_env "$KWIN_SOCKET" timeout 30 "$DEMO_BIN" 2>&1 \
    | grep '^lock: ')
  echo "$got" >"$OUT/lock-unsupported.log"
  if printf '%s' "$got" | grep -q 'guard granted' \
     && printf '%s' "$got" | grep -q 'error the compositor does not provide ext_session_lock_manager_v1'; then
    pass unsupported "$(printf '%s' "$got" | grep error)"
  else
    fail unsupported "$(printf '%s' "$got" | tr '\n' '|')"
  fi

  # --- cosmic-comp inside the headless KWin ----------------------------------------------------
  (
    comp_source
    # The system EGL vendor: Mesa (nested.sh's default) is handed the NVIDIA render node the
    # headless KWin advertises and draws nothing ("pci id ... driver (null)", BAD_SURFACE).
    exec env COSMIC_BACKEND=winit XDG_CONFIG_HOME="$DEV/xdg-config" __GL_SYNC_TO_VBLANK=0 \
      vblank_mode=0 cosmic-comp --no-xwayland
  ) >"$OUT/lock-comp.log" 2>&1 &
  COMP=$!
  for _ in $(seq 100); do
    for s in "$RUNTIME"/wayland-[0-9]*; do
      case "$s" in *.lock) continue ;; esac
      [ -S "$s" ] && COSMIC_SOCKET="$s"
    done
    [ -n "$COSMIC_SOCKET" ] && break
    sleep 0.1
  done
  if [ -z "$COSMIC_SOCKET" ]; then
    echo "FAIL setup: no cosmic-comp socket in $RUNTIME; see $OUT/lock-comp.log" >&2
    exit 1
  fi
  case "$(basename "$COSMIC_SOCKET")" in
    wayland-0) echo "FAIL setup: refusing a socket named wayland-0" >&2; exit 1 ;;
  esac
  echo "cosmic-comp: $COSMIC_SOCKET" >&2
  sleep 2
  LOCK_SOCKET="$COSMIC_SOCKET"
else
  # No nested compositor: casement is the compositor that locks.
  LOCK_SOCKET="$KWIN_SOCKET"
  case "$(basename "$LOCK_SOCKET")" in
    wayland-0) echo "FAIL setup: refusing a socket named wayland-0" >&2; exit 1 ;;
  esac
fi

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

# What cosmic-comp shows on its output (grim, its screencopy), and the headless KWin's whole
# screen beside it as <name>-kwin.png, for the record. (KWin only.)
shot() {
  rm -f "$1"
  env WAYLAND_DISPLAY="$LOCK_SOCKET" XDG_RUNTIME_DIR="$SESSION_RUNTIME" timeout 10 grim "$1" \
    >/dev/null 2>&1
  (
    comp_source
    timeout 20 spectacle -b -n -f -o "${1%.png}-kwin.png" >/dev/null 2>&1
  )
  [ -s "$1" ]
}

# The colour at x,y of image $1 as "r,g,b".
pixel() {
  magick "$1" -format "%[fx:int(255*p{$2,$3}.r)],%[fx:int(255*p{$2,$3}.g)],%[fx:int(255*p{$2,$3}.b)]\n" info:
}

# How many pixels differ between images $1 and $2.
differing() {
  compare -metric AE "$1" "$2" null: 2>&1 | sed 's/ .*//'
}

# cosmic-comp's window in KWin: its frame as "x y w h".
comp_frame() {
  "$DEV/kwin-windows.sh" | grep -v '"class":"org.kde' | head -n 1 \
    | sed -n 's/.*"frame":\[\([-0-9]*\),\([-0-9]*\),\([0-9]*\),\([0-9]*\)\].*/\1 \2 \3 \4/p'
}

# --- what "the session is covered" means, per compositor -----------------------------------------
# KWin: the nested cosmic-comp's screenshot (grim). casement has no screenshot until P1-C1
# (`casementctl shot`), so the cover is read from its input: a real click on the session's probe
# window must not activate it while the session is locked, and must once it is not
# (`casementctl debug windows`: active). Each check sets EVIDENCE.
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
  case "$COMPOSITOR" in
    kwin)
      shot "$OUT/lock-$1.png"
      local corners differ
      corners=$(for at in "20 20" "1260 20" "20 780" "1260 780"; do
        # shellcheck disable=SC2086
        pixel "$OUT/lock-$1.png" $at
      done | sort -u | tr '\n' ' ' | sed 's/ $//')
      differ=$(differing "$OUT/lock-before.png" "$OUT/lock-$1.png")
      EVIDENCE="screenshot corners all $corners (was $BEFORE, $differ px changed)"
      [ "$corners" = "24,28,38" ]
      ;;
    casement)
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
      ;;
  esac
}

# uncovered TAG: the session shows again, as it did before the lock.
uncovered() {
  case "$COMPOSITOR" in
    kwin)
      shot "$OUT/lock-$1.png"
      local after differ
      after=$(pixel "$OUT/lock-$1.png" 20 780)
      differ=$(differing "$OUT/lock-before.png" "$OUT/lock-$1.png")
      EVIDENCE="screenshot $after, $differ px differ from before ($BEFORE)"
      [ "$differ" = 0 ] && kill -0 "$PROBE" 2>/dev/null
      ;;
    casement)
      click_probe
      kill -0 "$PROBE" 2>/dev/null && probe_active && EVIDENCE="a click on the session's window activated it again" \
        || { EVIDENCE="a click on the session's window did not activate it, or it is gone"; return 1; }
      ;;
  esac
}

wait_for '^lock: ready' 100 || { echo "FAIL setup: demo_lock never became ready" >&2; exit 1; }
grep -q '^lock: guard granted' "$LOG" || { echo "FAIL setup: guard refused on $LOCK_SOCKET" >&2; exit 1; }
sleep 1
case "$COMPOSITOR" in
  kwin)
    read -r FX FY FW FH <<<"$(comp_frame)"
    if [ -z "${FW:-}" ]; then
      echo "FAIL setup: cosmic-comp's window not found in KWin" >&2
      exit 1
    fi
    CX=$((FX + FW / 2)) CY=$((FY + FH / 2))
    echo "cosmic-comp window: $FX,$FY ${FW}x$FH" >&2
    # Give the nested compositor's window KWin's keyboard focus, before anything is locked (the
    # user's click on their monitor, so to speak); nothing clicks after the lock.
    input "move $CX $CY" "sleep 200" "press left" "release left" "sleep 500" || exit 1
    shot "$OUT/lock-before.png" || fail setup "no screenshot before locking"
    BEFORE=$(pixel "$OUT/lock-before.png" 20 780)
    ;;
  casement)
    # The probe maps unfocused: nothing activates it before the lock (the covered/uncovered
    # checks click it), and the frame is where casement put it.
    for _ in $(seq 50); do [ -n "$(probe_frame)" ] && break; sleep 0.1; done
    [ -n "$(probe_frame)" ] || { echo "FAIL setup: the probe window never mapped" >&2; exit 1; }
    echo "probe window: $(probe_frame)" >&2
    ;;
esac

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
if [ "$COMPOSITOR" = casement ]; then
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
fi
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
  case "$COMPOSITOR" in
    kwin)
      # The session must not show: the screenshot differs from the one before the lock.
      shot "$OUT/lock-crashed.png"
      differ=$(differing "$OUT/lock-before.png" "$OUT/lock-crashed.png")
      EVIDENCE="screenshot $(pixel "$OUT/lock-crashed.png" 20 780), $differ px differ from the session"
      [ "$differ" != 0 ] && hidden=1
      ;;
    casement) covered crashed && hidden=1 ;;
  esac
  crash_evidence="$EVIDENCE"
  # A new locker, told to unlock at once (`unlock` is HostCtx::unlock_session).
  got=$( (echo lock; sleep 3; echo unlock; sleep 2; echo quit) \
    | demo_env "$LOCK_SOCKET" timeout 30 "$DEMO_BIN" 2>&1 | grep '^lock: event')
  echo "$got" >"$OUT/lock-recover.log"
  sleep 1
  if [ "$COMPOSITOR" = kwin ]; then
    shot "$OUT/lock-recovered.png"
    back="$(differing "$OUT/lock-before.png" "$OUT/lock-recovered.png") px differ from the session"
  else
    uncovered recovered && back="the session answers input again" || back="the session still does not answer input"
  fi
  if [ "$hidden" = 1 ]; then
    pass crash "killed while locked: still locked ($crash_evidence); a new locker: $(printf '%s' "$got" | sed 's/^lock: event //' | tr '\n' ' '); then $back"
  else
    fail crash "the session showed after the locker died ($crash_evidence)"
  fi
else
  fail crash "the second lock never locked"
fi
exit $status

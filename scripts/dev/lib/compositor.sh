# shellcheck shell=bash
# Which compositor the acceptance scripts run against, sourced by every accept-*.sh:
#
#   COMPOSITOR=kwin       (default) a headless KWin on the host (kwin-virtual.sh), input through
#                         fake-input.sh. Runs on the host, never the user's session.
#   COMPOSITOR=casement   casement (our compositor) started by its dev/up.sh inside
#                         ~/desktop/harness/jail.sh, input through `casementctl debug inject`.
#                         The script re-executes itself in the jail (`comp_enter`) after its
#                         cargo builds, which never happen inside the jail.
#
# A script uses, in this order: `comp_build` (cargo, outside any jail), `comp_enter`, `comp_out`
# (its scratch directory for logs and fifos), `comp_start WxH` ... `comp_stop`, and
# `comp_source` before it starts a client. Input is `lib/input.sh`, a window's frame
# `lib/windows.sh`. State (A8) never goes into the checkout under casement: it lives in the
# jail's scratch directory, scripts/dev/out/casement-<script>/ (git-ignored, kept after the run).
#
# Environment (casement): CASEMENT_DIR the casement checkout (default: ../casement beside this
# repo), CASEMENT_TOOLS its dev/out/tools (built by dev/bootstrap.sh --build-only outside the jail),
# SHELL_HOST_BACKEND the demos' backend (default cpu: no GPU in the jail; `gpu` runs the F80
# adapter fallback instead), CASEMENT_DMABUF none|soft (default none), CASEMENT_CLOCK wall|stepped
# (default wall: the demos run on the wall clock).
[ -z "${_SHELL_HOST_COMPOSITOR_SH:-}" ] || return 0
_SHELL_HOST_COMPOSITOR_SH=1

COMP_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMP_DEV_DIR="$(dirname "$COMP_LIB_DIR")"
COMP_ROOT="$(cd "$COMP_DEV_DIR/../.." && pwd)"
COMPOSITOR="${COMPOSITOR:-kwin}"
case "$COMPOSITOR" in
  kwin | casement) ;;
  *) echo "COMPOSITOR must be kwin or casement, not '$COMPOSITOR'" >&2; exit 64 ;;
esac
CASEMENT_DIR="${CASEMENT_DIR:-$COMP_ROOT/../casement}"
export COMPOSITOR CASEMENT_DIR
COMP_ENV="${COMP_ENV:-}"

# Whether this process is inside ~/desktop/harness/jail.sh (its PID 1 is bwrap, its scratch set).
# The jail's own structural check (require_jail) still runs in every casement script of ours.
comp_in_jail() { [ "$(cat /proc/1/comm 2>/dev/null)" = bwrap ] && [ -n "${JAIL_SCRATCH:-}" ]; }

# comp_build CARGO_ARGS...: `cargo build`, except inside the jail (no cargo, no network there:
# the outside half of the script built everything already).
comp_build() {
  if comp_in_jail; then return 0; fi
  cargo build "$@" >&2
}

# comp_tools: the casement tools directory, or exit 3 saying how to build it.
comp_tools() {
  local tools="${CASEMENT_TOOLS:-$CASEMENT_DIR/dev/out/tools}" tool
  for tool in casement casementctl fake-logind; do
    if [ ! -x "$tools/bin/$tool" ]; then
      echo "comp: $tools/bin/$tool is missing: run $CASEMENT_DIR/dev/bootstrap.sh --build-only outside the jail" >&2
      return 3
    fi
  done
  echo "$tools"
}

# comp_enter SCRIPT ARGS...: COMPOSITOR=casement outside the jail runs SCRIPT ARGS in a new jail
# (and exits with its code); anything else returns at once. Call it after the script's builds.
comp_enter() {
  [ "$COMPOSITOR" = casement ] || return 0
  if comp_in_jail; then return 0; fi
  local script=$1 name tools jail scratch var envs=()
  shift
  name="$(basename "$script" .sh)"
  tools="$(comp_tools)" || exit 3
  jail="${CASEMENT_DESKTOP:-$COMP_ROOT/../desktop}/harness/jail.sh"
  [ -x "$jail" ] || jail="$HOME/desktop/harness/jail.sh"
  [ -x "$jail" ] || { echo "comp: no ~/desktop/harness/jail.sh" >&2; exit 3; }
  scratch="$COMP_DEV_DIR/out/casement-${name#accept-}"
  rm -rf "$scratch"
  mkdir -p "$scratch"
  envs=(--env "COMPOSITOR=casement" --env "CASEMENT_DIR=$CASEMENT_DIR" --env "CASEMENT_TOOLS=$tools"
    --env "SHELL_HOST_BACKEND=${SHELL_HOST_BACKEND:-cpu}")
  # The scripts' own knobs, passed through the jail's cleared environment when set.
  for var in CASEMENT_DMABUF CASEMENT_CLOCK CASEMENT_START_TENTHS FOOT FOOT_LIBS DRAG_ICON \
    HOVER_TRACE SCROLL_TRACE LOCK_TRACE SILL_DIR; do
    [ -z "${!var:-}" ] || envs+=(--env "$var=${!var}")
  done
  exec "$jail" --scratch "$scratch" "${envs[@]}" -- "$script" "$@"
}

# comp_out: the directory for the script's logs, fifos and captures.
comp_out() {
  if [ "$COMPOSITOR" = casement ]; then
    echo "$JAIL_SCRATCH/out"
  else
    echo "$COMP_DEV_DIR/out"
  fi
}

# comp_start WIDTHxHEIGHT: starts the compositor with one output of that size at scale 1 and
# sets COMP_ENV, the env file a client sources (`comp_source`). Under casement COMP_RUNTIME
# (a directory that exists) puts its socket there instead of in the jail's XDG_RUNTIME_DIR.
comp_start() {
  local size="${1:-1920x1080}"
  case "$COMPOSITOR" in
    kwin)
      "$COMP_DEV_DIR/kwin-virtual.sh" start "$size" >&2 || return 1
      COMP_ENV="$COMP_DEV_DIR/.kwin-env"
      ;;
    casement)
      local up out
      up="$CASEMENT_DIR/dev/up.sh"
      out="$(unset CASEMENT_STATE; XDG_RUNTIME_DIR="${COMP_RUNTIME:-$XDG_RUNTIME_DIR}" \
        "$up" --clock "${CASEMENT_CLOCK:-wall}" --dmabuf "${CASEMENT_DMABUF:-none}" \
        --outputs "HEADLESS-1:${size}@60000:120:0,0")" || return 1
      COMP_ENV="$(printf '%s\n' "$out" | tail -n 1)"
      ;;
  esac
  export COMP_ENV
}

# comp_stop: stops what comp_start started (never anything else).
comp_stop() {
  case "$COMPOSITOR" in
    kwin) "$COMP_DEV_DIR/kwin-virtual.sh" stop >&2 ;;
    casement) [ -z "$COMP_ENV" ] || "$CASEMENT_DIR/dev/down.sh" "$COMP_ENV" >&2 ;;
  esac
}

# comp_source: puts the session's variables (WAYLAND_DISPLAY, the private bus) into the
# environment; call it in the subshell that starts a client.
comp_source() {
  case "$COMPOSITOR" in
    kwin)
      # shellcheck source=/dev/null
      source "$COMP_ENV"
      ;;
    casement)
      set -a
      # shellcheck source=/dev/null
      source "$COMP_ENV"
      set +a
      ;;
  esac
}

# comp_socket: the compositor's wayland socket as WAYLAND_DISPLAY has it.
comp_socket() { (comp_source; echo "$WAYLAND_DISPLAY"); }

#!/usr/bin/env bash
# Runs a command inside the nested cosmic-comp started by nested.sh.
#
# Usage: scripts/dev/run.sh ghostty
#        scripts/dev/run.sh foot -e htop
set -euo pipefail

DEV_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENV_FILE="$DEV_DIR/.nested-env"

if [ ! -f "$ENV_FILE" ]; then
  echo "run.sh: $ENV_FILE not found. Run scripts/dev/nested.sh first." >&2
  exit 1
fi

# shellcheck disable=SC1090
source "$ENV_FILE"

if [ -z "${WAYLAND_DISPLAY:-}" ]; then
  echo "run.sh: WAYLAND_DISPLAY missing from $ENV_FILE." >&2
  exit 1
fi

if [ -n "${NESTED_COMPOSITOR_PID:-}" ] && ! kill -0 "$NESTED_COMPOSITOR_PID" 2>/dev/null; then
  echo "run.sh: warning: the nested compositor (pid $NESTED_COMPOSITOR_PID) is not running." >&2
fi

if [ "$#" -eq 0 ]; then
  echo "usage: $(basename "$0") <command> [args...]" >&2
  exit 2
fi

export WAYLAND_DISPLAY

exec "$@"

#!/usr/bin/env bash
# Takes a screenshot.
#
# If the nested compositor is running (scripts/dev/.nested-env present and
# alive), shoots the NESTED output with grim. Otherwise falls back to a host
# (KDE/Plasma) screenshot via spectacle.
#
# Usage: scripts/dev/shot.sh <out.png>
set -euo pipefail

DEV_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENV_FILE="$DEV_DIR/.nested-env"

if [ "$#" -ne 1 ]; then
  echo "usage: $(basename "$0") <out.png>" >&2
  exit 2
fi

out="$1"
mkdir -p "$(dirname "$out")"

if [ -f "$ENV_FILE" ]; then
  # shellcheck disable=SC1090
  source "$ENV_FILE"
fi

if [ -n "${WAYLAND_DISPLAY:-}" ] && [ -n "${NESTED_COMPOSITOR_PID:-}" ] && kill -0 "$NESTED_COMPOSITOR_PID" 2>/dev/null; then
  if ! command -v grim >/dev/null 2>&1; then
    echo "shot.sh: grim not found (needed for nested screenshots)." >&2
    exit 1
  fi
  echo "shot.sh: nested compositor detected, shooting WAYLAND_DISPLAY=$WAYLAND_DISPLAY with grim -> $out"
  WAYLAND_DISPLAY="$WAYLAND_DISPLAY" grim "$out"
else
  if ! command -v spectacle >/dev/null 2>&1; then
    echo "shot.sh: no nested compositor running and spectacle not found." >&2
    exit 1
  fi
  echo "shot.sh: no nested compositor running, shooting host (KDE) with spectacle -> $out"
  spectacle -b -n -f -o "$out"
fi

echo "shot.sh: wrote $out"

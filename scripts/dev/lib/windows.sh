#!/usr/bin/env bash
# Every window's frame (window geometry) and buffer geometry as JSON lines, from the compositor
# of lib/compositor.sh: kwin-windows.sh on KWin, `casementctl debug windows` (the same fields)
# on casement. `--class APP_ID` keeps one app's windows. Needs COMP_ENV (comp_start exports it).
#
#   scripts/dev/lib/windows.sh --class demo_window    {"class":"demo_window","frame":[400,240,480,320],...}
set -euo pipefail
LIB="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/dev/lib/compositor.sh
source "$LIB/compositor.sh"
case "$COMPOSITOR" in
  kwin) exec "$LIB/../kwin-windows.sh" "$@" ;;
  casement)
    tools="$(comp_tools)"
    class=""
    case "${1:-}" in
      --class) class="${2:?--class needs an app id}" ;;
      "") ;;
      *) echo "usage: windows.sh [--class APP_ID]" >&2; exit 64 ;;
    esac
    comp_source
    # The array's objects one per line, no spaces after the commas.
    "$tools/bin/casementctl" debug windows | sed 's/^\[//; s/\]$//; s/},{/}\n{/g; s/, /,/g' \
      | { if [ -n "$class" ]; then grep -F "\"class\":\"$class\"" || true; else cat; fi; }
    ;;
esac

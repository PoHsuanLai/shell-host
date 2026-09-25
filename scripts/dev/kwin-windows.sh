#!/usr/bin/env bash
# The windows of the headless KWin (scripts/dev/kwin-virtual.sh) as JSON lines, from
# scripts/dev/kwin-windows.js run through KWin's scripting D-Bus API. `--class NAME` keeps only
# that app id (resourceClass).
set -euo pipefail
DEV_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
source "$DEV_DIR/.kwin-env"
class="${2:-}"
[ "${1:-}" = "--class" ] || class=""
name="kwin-windows-$$-$(date +%s%N)"
start=$(wc -l <"$KWIN_VIRTUAL_LOG")
id=$(gdbus call --session --dest org.kde.KWin --object-path /Scripting \
  --method org.kde.kwin.Scripting.loadScript "$DEV_DIR/kwin-windows.js" "$name" \
  | sed -E 's/^\((-?[0-9]+),\)$/\1/')
gdbus call --session --dest org.kde.KWin --object-path "/Scripting/Script$id" \
  --method org.kde.kwin.Script.run >/dev/null
sleep 0.3
gdbus call --session --dest org.kde.KWin --object-path /Scripting \
  --method org.kde.kwin.Scripting.unloadScript "$name" >/dev/null
tail -n +"$((start + 1))" "$KWIN_VIRTUAL_LOG" | sed -n 's/.*WINDOW //p' \
  | { if [ -n "$class" ]; then grep -F "\"class\":\"$class\""; else cat; fi; } || true

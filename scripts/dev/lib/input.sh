#!/usr/bin/env bash
# Synthetic pointer and keyboard input into the compositor of lib/compositor.sh, commands on
# stdin, each answered `ok ...` on stdout once it reached the compositor: fake-input.sh's grammar
# on KWin, `casementctl debug inject` (a superset, casement-ipc/src/inject.rs) on casement.
# Needs COMP_ENV, which comp_start exports.
set -euo pipefail
LIB="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/dev/lib/compositor.sh
source "$LIB/compositor.sh"
case "$COMPOSITOR" in
  kwin) exec "$LIB/../fake-input.sh" ;;
  casement)
    tools="$(comp_tools)"
    comp_source
    exec "$tools/bin/casementctl" debug inject
    ;;
esac

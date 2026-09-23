#!/usr/bin/env bash
# Prints whether the nested compositor (started by nested.sh) advertises the
# Wayland protocols shell-host depends on, and at what version.
#
# Usage: scripts/dev/globals.sh
set -euo pipefail

DEV_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENV_FILE="$DEV_DIR/.nested-env"

if [ ! -f "$ENV_FILE" ]; then
  echo "globals.sh: $ENV_FILE not found. Run scripts/dev/nested.sh first." >&2
  exit 1
fi

# shellcheck disable=SC1090
source "$ENV_FILE"

if [ -z "${WAYLAND_DISPLAY:-}" ]; then
  echo "globals.sh: WAYLAND_DISPLAY missing from $ENV_FILE." >&2
  exit 1
fi

if ! command -v wayland-info >/dev/null 2>&1; then
  echo "globals.sh: wayland-info not found." >&2
  exit 1
fi

dump="$(WAYLAND_DISPLAY="$WAYLAND_DISPLAY" wayland-info 2>&1)"

# interfaces shell-host and <shell> care about; either name in an either-or
# pair (cosmic vs ext workspace manager) counts.
INTERFACES=(
  zwlr_layer_shell_v1
  ext_background_effect_manager_v1
  wp_fractional_scale_manager_v1
  wp_viewporter
  zcosmic_toplevel_info_v1
  "ext_workspace_manager_v1|zcosmic_workspace_manager_v2"
  zwp_text_input_manager_v3
)

printf '%-40s %-9s %s\n' "interface" "present" "version"
printf '%-40s %-9s %s\n' "----------------------------------------" "---------" "-------"

any_missing=0
for spec in "${INTERFACES[@]}"; do
  found=""
  IFS='|' read -ra names <<< "$spec"
  for name in "${names[@]}"; do
    line="$(echo "$dump" | grep -E "interface: '${name}'," | head -n1 || true)"
    if [ -n "$line" ]; then
      version="$(echo "$line" | sed -n "s/.*version:\s*\([0-9]*\).*/\1/p")"
      printf '%-40s %-9s %s\n' "$name" "yes" "$version"
      found=1
    fi
  done
  if [ -z "$found" ]; then
    printf '%-40s %-9s %s\n' "$spec" "no" "-"
    any_missing=1
  fi
done

if [ "$any_missing" -eq 1 ]; then
  exit 1
fi

#!/usr/bin/env bash
# Checks whether the packages the nested-compositor dev harness needs are
# installed, and prints the dnf command to install whatever is missing.
#
# This script NEVER calls sudo itself. It only inspects the system (via rpm)
# and, if anything is missing, prints the exact `sudo dnf install` command
# for the user to run themselves.
#
# Usage: scripts/dev/install-cosmic.sh
set -euo pipefail

# cosmic-session is deliberately NOT in this list: running it nested starts
# every COSMIC component (panel, power-key inhibitor, D-Bus names, autostart
# entries) with infinite restarts. The harness only ever runs cosmic-comp
# directly. See scripts/dev/README.md.
PACKAGES=(
  cosmic-comp
  cosmic-icon-theme
  cosmic-randr
  cosmic-settings-daemon
  cosmic-term
  foot
  grim
)

missing=()
for pkg in "${PACKAGES[@]}"; do
  if rpm -q "$pkg" >/dev/null 2>&1; then
    echo "ok      $pkg  ($(rpm -q "$pkg"))"
  else
    echo "missing $pkg"
    missing+=("$pkg")
  fi
done

echo
if [ "${#missing[@]}" -eq 0 ]; then
  echo "All packages the nested harness needs are already installed."
  exit 0
fi

echo "Missing ${#missing[@]} package(s). Run this yourself (this script never calls sudo):"
echo
echo "  sudo dnf install -y ${missing[*]}"
echo
exit 1

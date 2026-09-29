#!/usr/bin/env bash
# Synthetic pointer and keyboard input into the headless KWin of scripts/dev/kwin-virtual.sh,
# through org_kde_kwin_fake_input (tools/fake-input). Commands on stdin, one per
# line, each answered with `ok <line>` once it reached the compositor:
#
#   move X Y                 absolute pointer position (global logical px)
#   press left|right|middle  / release left|right|middle
#   key KEYSYM down|up       xkb keysym name: Return, a, Shift_L, Super_L, F5
#   type "text"              each character, with Shift where KWin's keymap needs it
#   axis vertical|horizontal VALUE   one wheel axis event (15 = one detent)
#   sleep MS
#
#   printf 'move 300 200\npress left\nmove 500 300\nrelease left\n' | scripts/dev/fake-input.sh
#
# Builds the tool (its own Cargo workspace, not shell-host's); a no-op once it is up to date, and
# a stale binary never lacks a command the scripts use.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ENV_FILE="$ROOT/scripts/dev/.kwin-env"
if [ ! -f "$ENV_FILE" ]; then
  echo "fake-input.sh: no $ENV_FILE; run scripts/dev/kwin-virtual.sh start first" >&2
  exit 1
fi
# shellcheck disable=SC1090
source "$ENV_FILE"
BIN="$ROOT/tools/fake-input/target/release/fake-input"
cargo build --release --manifest-path "$ROOT/tools/fake-input/Cargo.toml" >&2
exec "$BIN"

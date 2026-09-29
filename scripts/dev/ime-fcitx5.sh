#!/usr/bin/env bash
# The input method kwin-virtual.sh hands KWin for accept-ime.sh: fcitx5 on the
# WAYLAND_SOCKET KWin passes it, with only the addons the check needs (no X11, no tray, no
# kimpanel), its log in out/ime-fcitx5.log. It inherits kwin-virtual.sh's private bus, config
# and data directories; never run it on a real session.
exec fcitx5 --verbose='*=5' \
  --disable=xim,xcb,dbusfrontend,fcitx4frontend,ibusfrontend,kimpanel,notificationitem,notifications,clipboard,emoji,spell,quickphrase,unicode,imselector,virtualkeyboard \
  2>>"$(dirname "${BASH_SOURCE[0]}")/out/ime-fcitx5.log"

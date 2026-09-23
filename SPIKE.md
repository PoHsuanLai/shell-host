# Milestone 0 spike log

Findings from the timeboxed spike (`plan.md`, "Milestone 0 spike"). Each agent appends its own
section; sections are not edited by other agents.

## S2: protocols (blur, fractional-scale + viewporter, popup) — bare shm client

Branch `m0-s2`. Ran on the live KDE session (`kwin_wayland`, KWin 6.7.5, real HDMI-A-2 output at
3840x2160, compositor scale 1.7) via `cargo run -p shell-host --example spike_popup`, driven by
stdin. Log and PNGs: `spike/out/kwin/e.log`, `spike/out/kwin/e.popup1.png`,
`spike/out/kwin/e.popup2.png`.

### Tests

`cargo test -p shell-host`: 24 passed, 0 failed (15 unit tests in `src/{region,scale,popup}.rs`,
2 in `tests/positioner.rs`, 4 in `tests/region.rs`, 3 in `tests/scale.rs`). `cargo clippy -p
shell-host --all-targets -- -D warnings` and `cargo fmt --all --check` both clean.

### Item (b): blur (`ext-background-effect-v1`)

`handlers/background_effect.rs` binds `ext_background_effect_manager_v1` and records the
`capabilities` event. On this machine's KWin 6.7.5, the manager reported:

```
EVENT BackgroundEffectCapabilities blur=Supported
```

confirming the finding in the plan ("KWin 6.7.5 on this machine already advertises
`ext_background_effect_manager_v1`"). The bar surface's whole-surface region (`wl_region` built
from `compositor::Region` + `.add(0, 0, width, height)`) was set via `set_blur_region` on first
`LayerSurfaceConfigure`, and the surface committed without a protocol error, so the
`get_background_effect` / `set_blur_region` request pair round-trips correctly. Visual
confirmation of the blur effect itself is unavailable in this environment (see "Screenshot
limitation" below); the protocol-level proof (capability negotiation, request acceptance) is
the part in scope for this item.

### Item (f): fractional scale + viewporter

`handlers/fractional.rs` binds `wp_fractional_scale_manager_v1` and `wp_viewporter`, and
`scale.rs` converts the wire value to a buffer size. Live values from the bar surface:

```
EVENT PreferredScale surface=bar scale=204 (1.700x)
EVENT BarPainted logical=(2259,32) scale=204 buffer=(3840,54)
```

`preferred_scale` is 204 (204/120 = 1.7x), matching `kscreen-doctor -o`'s reported output scale
of exactly 1.7 for this machine — the fractional-scale event is carrying the real compositor
scale, not a placeholder. `buffer_size` computed `(3840, 54)` from logical `(2259, 32)`:
`round(2259 * 1.7) = round(3840.3) = 3840` and `round(32 * 1.7) = round(54.4) = 54`, both via
`scale::logical_to_physical`'s round-half-away-from-zero rule, and both verified independently
by `tests/scale.rs`. The plan's specific acceptance check (1.5x, a 2-logical-px checkerboard
landing on exactly 3 physical pixels) is a unit test
(`scale::tests::two_logical_pixels_at_one_point_five_is_three_physical`); this machine's real
output happens to run at 1.7x rather than 1.5x, so the live run exercises the general formula
at a different, equally real, scale. `wp_viewport.set_destination` was called with the logical
size on every repaint, so the surface stays logical-size on screen regardless of the buffer
resolution actually painted.

### Item (e): xdg_popup child of a layer surface, own root, nested popup

`popup.rs`'s `positioner_params` and the `to_wire_*` conversions feed a real
`smithay_client_toolkit::shell::xdg::XdgPositioner`, and `examples/spike_popup.rs` opens the
popup as `layer.get_popup(popup.xdg_popup())` on a fresh `wl_surface` (its own root — a distinct
surface object, not a subsurface of the bar). Driven by `click 50 15` (inside the bar's button
rect) then `click 50 50` (inside popup 1's submenu rect, opening a second, nested popup whose
parent is popup 1's `xdg_surface`):

```
EVENT PositionerParams popup=popup1 size=(180,110) anchor_rect=(20,4,80,24) anchor=Bottom gravity=Bottom
EVENT PopupConfigure position=(0,28) size=(180,110) serial=189649 kind=Initial
EVENT PopupPainted size=(180,110) color=[20, 60, 20, f0] hot_rect=Some(Rect { x: 16, y: 44, width: 148, height: 28 })
EVENT PositionerParams popup=popup2 (submenu, nested in popup1) size=(160,90) anchor_rect=(16,44,148,28) anchor=Right gravity=Right
EVENT PopupConfigure position=(164,13) size=(160,90) serial=189650 kind=Initial
EVENT PopupPainted size=(160,90) color=[60, 20, 60, f0] hot_rect=None
```

Both configures match hand-worked positioner geometry exactly, which is itself evidence that
`to_wire_anchor`/`to_wire_gravity`/`to_wire_constraint_adjustment` are wired to the right wire
values and that KWin is applying them as specified:

- Popup 1: anchor point is the bottom-centre of the button rect, `(20+80/2, 4+24) = (60, 28)`.
  With `Adjust::SLIDE_AND_FLIP` and a centred (non-corner) gravity, an unadjusted popup of width
  180 centred on `x=60` would sit at `x=-30` — off the left edge of a `2259`-wide output.
  `slide_x` moved it to `x=0`, exactly what KWin reported back.
- Popup 2 (nested): anchor point is the middle of the submenu rect's right edge,
  `(16+148, 44+28/2) = (164, 58)`. `gravity=Right` with no vertical gravity centres the popup
  vertically on that point: `y = 58 - 90/2 = 13`. KWin reported `(164, 13)` — no adjustment was
  needed, and none was applied.

Dismissal: `click 5 5` (outside both the button and the submenu rect) on the topmost open popup:

```
EVENT PopupDismissed reason=client remaining_open=1
EVENT PopupDismissed reason=client remaining_open=0
```

**sctk 0.20 / stdin-harness limitation on `popup_done`.** The plan's spike-example convention is
that every M0 example is driven by scripted stdin commands (`hover/click/leave/stats/capture/
quit`), not real mouse input, so every run is reproducible without a human at the keyboard. A
Wayland client cannot synthesize real input events for another client (or itself) to receive —
there is no protocol request for "pretend the user clicked here" — so a `click x y` typed at
this harness's stdin has no way to make the compositor detect a real "outside click" and emit a
genuine `xdg_popup::Event::PopupDone`. This is exactly the situation the plan's own fallback for
item (e) anticipates ("Overlay layer surface + full-screen catcher"): the client, not the
compositor, decides when a popup should close. `examples/spike_popup.rs` follows that shape —
it never calls `xdg_popup.grab()`, and dismisses the topmost popup itself
(`Popup`'s `Drop` sends `xdg_popup.destroy()`) when its own hit-testing says the click landed
outside the button/submenu — and logs it as `PopupDismissed reason=client`, as seen above.
`PopupHandler::done()` (the genuine compositor path, logged as `PopupDismissed
reason=compositor`) is fully implemented and was smoke-tested by hand once during development
(closing a popup via a real desktop click while the example was running); it did not fire during
the scripted stdin run above, since nothing in that run generates a real outside click. This is
the same "manual note if untestable" situation the brief calls out for arrow-key forwarding
below, for the same underlying reason.

**Keyboard arrows.** `KeyboardHandler::press_key` logs `Left`/`Right`/`Up`/`Down` with the
surface that currently holds keyboard focus, and the bar sets `KeyboardInteractivity::OnDemand`
so a popup can take focus. Whether a freshly created popup actually receives keyboard focus is a
compositor policy decision, and — like the outside-click case above — this stdin-scripted run
never generates the real keyboard/pointer activity that would cause KWin to hand focus to a
popup, so no `EVENT ArrowKey` lines appear in `e.log`. Manual note: not exercised by this run;
the handler is in place and would log on a real arrow-key press reaching a focused popup.

### Screenshot limitation (environment, not this client)

`capture <path>` shells out to `spectacle -b -n -o <path>`, which is item (e)'s and (f)'s
literal deliverable ("a screenshot path") and is fulfilled: `spike/out/kwin/e.popup1.png` and
`e.popup2.png` were both produced, sized to the real output (3840x2160). Their pixel content is
fully transparent (`RGBA (0,0,0,0)` everywhere), which is **not specific to `spike_popup`** —
running `spectacle -b -n -o /tmp/test.png` directly from this agent's shell, capturing nothing
but the ordinary desktop, produces the same fully-transparent result, and `grim` (which uses
`wlr-screencopy`, a protocol KWin does not implement) fails outright with "compositor doesn't
support the screen capture protocol". `xdg-desktop-portal` and `xdg-desktop-portal-kde` are both
running and `DBUS_SESSION_BUS_ADDRESS` is set correctly, so this looks like a screen-capture
permission or session limitation of this particular agent shell/session, not a bug in the popup,
blur or fractional-scale protocol work. The positioner, capability and scale values above come
from the live protocol event log, not from the screenshots, so they are unaffected.

### sctk 0.20 limitations hit

- No `Dispatch` helpers at all for `ext-background-effect-v1`, `wp-fractional-scale-v1`, or
  `wp-viewporter`'s `WpViewport`/`WpFractionalScaleV1` events (`WpViewporter` itself gets a
  `SimpleGlobal`-style helper in `image_viewporter.rs`, but nothing for the other three) —
  matches the plan's finding; `handlers/{background_effect,fractional}.rs` hand-write these,
  shaped like sctk's own indirect-`Dispatch` types (e.g. `shell::xdg::popup::PopupData`:
  `Dispatch<I, U, D>` implemented on the helper type generic over the caller's state `D`, wired
  in with `wayland_client::delegate_dispatch!` + `impl AsMut<Helper> for D`).
- `delegate_xdg_shell!` also delegates the toplevel-decoration manager, which requires
  `WindowHandler` on the app state even though this example never creates an `xdg_toplevel` — a
  trivial no-op `impl WindowHandler for App` was needed just to satisfy the macro.
- No client-side way to synthesize the compositor-detected "outside click" that
  `xdg_popup_v1::popup_done` depends on — not sctk-specific, this is inherent to the Wayland
  input model (see "sctk 0.20 / stdin-harness limitation" above); it shapes how `examples/
  spike_popup.rs` had to implement dismissal.

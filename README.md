# shell-host

Runs Blitz / Dioxus Native documents on Wayland: layer-shell surfaces (bars, docks,
overlays), xdg popups and xdg toplevels, all sharing one GPU device and one renderer, with
blur (`ext-background-effect-v1`), fractional scale, input regions that follow DOM elements,
IME, clipboard and a frame machine that is idle at zero wakeups. It is portable to any
Wayland compositor with layer-shell; it never depends on COSMIC-specific crates
(`scripts/check-boundary.sh`). The shell (sill) and every bundled app are built on it.

`DESIGN.md` maps the design to the code, `FINDINGS.md` records every deviation and seam,
`CONVENTIONS.md` and `ORCHESTRATION.md` say how the code is written and divided, and
`SPIKE.md` is the Milestone 0 log with the acceptance results.

## Demos

`examples/demo_bar.rs` (a 32 px top bar) and `examples/demo_dock.rs` (a magnifying dock) are
driven by commands on stdin (`bar stats`, `dock hover 300 64`, `dock leave`, `bar capture
/tmp/bar.png`, `quit`), so they need the `debug-inject` feature:

```sh
cargo run --release -p shell-host --features debug-inject --example demo_bar -- --with-dock
```

`demo_bar` takes `--with-dock` (both surfaces in one process) and `--exclusive=N`, and
`DEMO_BLUR=1` blurs behind the bar; `demo_dock` takes `--mode=transform|width|tick` and
`--layer=bottom`. `--mode=tick` magnifies from the component itself, once per compositor frame,
through the frame hook (`use_frame_tick` with a `FrameDrive` held only while the tiles move).

### On a nested KWin (no effect on your session)

A headless `kwin_wayland --virtual` on its own D-Bus session, torn down afterwards; works while
the real session is locked. `kwin-virtual@1.7` sets the nested output's scale:

```sh
scripts/spike-accept.sh demo kwin-virtual a      # or b, c, f, g
scripts/spike-accept.sh demo kwin-virtual@1.7 f
```

### On a nested cosmic-comp

`scripts/dev/` runs a throwaway `cosmic-comp` in a window of the current session (see
`scripts/dev/README.md`):

```sh
scripts/dev/install-cosmic.sh   # prints the dnf command for anything missing
scripts/dev/nested.sh           # start; writes scripts/dev/.nested-env
scripts/dev/globals.sh          # which protocols it offers
scripts/dev/run.sh target/release/examples/demo_bar --with-dock
scripts/dev/shot.sh scripts/dev/out/demo.png
scripts/dev/nested.sh stop
```

## Acceptance

`scripts/spike-accept.sh demo [<compositor>] <a|b|c|f|g>` re-runs the Milestone 0 items with
their thresholds against the demos: a (the bar reserves 32 px), b (blur), c (dock sweep once
per frame callback in the transform, width and tick modes, one tick per callback in tick mode, zero idle
wakeups, no blocking when covered), f (fractional scale maps 1:1),
g (two surfaces, one device). `<compositor>` is `kwin-virtual` (default, nested),
`kwin-virtual@<scale>`, or `kwin` (the live session; surfaces show for a few seconds). Results
land in `spike/out/demo/<compositor>/<item>.{log,json,png}`; exit 0 means the item passed.

## Checks

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
./scripts/check-boundary.sh
cargo deny check licenses
```

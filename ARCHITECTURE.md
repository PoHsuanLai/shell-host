# Architecture

shell-host runs Blitz / Dioxus Native documents on Wayland: layer-shell surfaces, xdg popups and
toplevels on one GPU device and one renderer. `README.md` describes it and its demos,
`DESIGN.md` maps each part of the design to its module and marks it pure or effects,
`FINDINGS.md` holds the facts about Blitz, the compositors and the protocols it relies on, and
`CONVENTIONS.md` holds the rules every repo shares.

## Crate map

One crate, `crates/shell-host` (library, examples, `tests/`). It stands below quire and palmrest
and is portable to any Wayland compositor with layer-shell: `scripts/check-boundary.sh` keeps
out every COSMIC crate, `zbus`, `libpulse-binding`, quire's crates and palmrest (the palmrest
socket's messages are re-declared in `input/palmrest/wire.rs`), and it keeps Blitz's own
scrollbars off, since the host paints the scroll thumb.

- **Pure** (table-tested): the frame machine, the lock machine, the scroll physics, region
  resolution, the grab rule, the public data types (`event`, `msg`, `config`, `id`, `output`).
- **Effects:** the handlers, `ctx/` (the flush after every dispatch, in `ctx/flush.rs::ORDER`)
  and `gpu/`.

## Repo rules

- **Effects stay out of the pure modules.** `frame::step` returns the actions to take rather
  than arming a timer, and handlers push `TimerRequest`s rather than touching the loop.
- **`unsafe` lives only in `gpu/raw_handle.rs`** (the raw window handle).
- **Errors:** `HostError`, each variant answering a `Recovery` (`Retry`, `Fallback`, `Degrade`,
  `Fatal`).
- **`Serialize, Deserialize` only where the type is data all the way down** (FINDINGS F11).
- **The palmrest wire types** are re-declared with exactly palmrest's serde attributes and
  tested against bytes palmrest produced.
- **Compositor behaviour is cited with a trace.** Before branching on protocol text, use what
  the protocol already said: an event, a global's presence, an error code.
- **Live acceptance** runs on the headless KWin (`scripts/dev/kwin-virtual.sh`) or a nested
  cosmic-comp, never on the person's session (`wayland-0`).
- **Gate:**

  ```bash
  cargo fmt --all --check
  cargo clippy --workspace --all-targets --all-features -- -D warnings
  cargo test --workspace --all-features
  ./scripts/check-boundary.sh
  cargo deny check licenses
  ```

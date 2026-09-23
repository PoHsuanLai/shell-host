# Orchestration

How parallel work on shell-host is divided. `CONVENTIONS.md` has the rules; the program plan has
the design (the "shell-host" section).

shell-host runs Blitz documents on Wayland layer-shell surfaces, xdg popups and toplevels, one
GPU device for all of them. It is portable to any Wayland compositor, so it must never reach
`cosmic-client-toolkit`, `zbus` or `libpulse-binding` (`scripts/check-boundary.sh`).

The unit of parallelism is **one file behind a frozen signature, plus its tests**, not one
crate: this repository is a single crate, and fan-out happens inside it.

## The rule that makes this work

Every agent codes against the **interface freeze**: the public types and function signatures of
the wave, committed by ONE agent before any fan-out, compiling today with `todo!()` bodies.

Because it compiles, a disagreement between two agents is a **build error in their own
sandbox**, not a surprise at integration time. That is the entire reason the freeze exists.

An agent that believes a frozen signature is wrong **stops and reports**. It does not work
around it and it does not change it. The same holds for anything missing from quire (a token,
a component, a motion value): it is added to quire first, never patched locally.

## Exclusive ownership

Each wave lists the files an agent owns **exclusively**. No two agents in a wave write the same
file. Reading anything is always fine. Agents work in their own worktree, never commit to
`master`, and never add attribution trailers; this repository's reviewer merges.

## Coherence rules (in every brief)

1. No stylesheet outside quire contains a hex/rgb/hsl colour, a raw `ms`/`s` duration, a raw
   `cubic-bezier`, a `@keyframes`, a `font-family`, or a `:root` selector. Enforced by
   `ds::lint::stylesheet()`, which every downstream crate runs in its own tests.
2. No surface or app writes raw `button`/`input`/menu markup; it uses quire components.
   Enforced by an SSR test asserting every class on the rendered surface is one quire exports.
3. A missing component or token is added to quire first (stop and report), never patched here.
4. Motion state is driven by quire timers, never by ad-hoc sleeps.

## Agent brief template

Every brief states all five, or the agent will invent the missing one:

1. **Files you own exclusively.** Everything else is read-only.
2. **Signatures you may not change.** Name them. If one is wrong, stop and report.
3. **What done means.** The tests that must exist, and the cases they must cover.
4. **How you are verified.** The exact command.
5. **What to read first.** `CONVENTIONS.md` §0 (design style) always, plus the relevant plan
   section, quire's README, and any other `CONVENTIONS.md` section the brief leans on.

## Builds

Call `cargo` plainly: never by absolute path, never with `+toolchain`, never with an inline
`CARGO_BUILD_JOBS` or `RUSTFLAGS`. A shim serialises every compilation on the machine and caps
each at 4 GB. A build killed at the cap even with one job is a finding to report (which crate,
what RSS), not something to work around. Each worktree keeps its own `CARGO_TARGET_DIR`;
sccache shares compiled objects, not target directories.

The pinned dependency block in `Cargo.toml` is copied verbatim from
`quire/docs/workspace-deps.toml`. Never edit it here; it changes only in a toolchain-bump wave
that runs every repository's gates.

## Verification

Every agent runs all of these before reporting, and reports honestly if one fails:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
./scripts/check-boundary.sh
cargo deny check licenses
```

Use `cargo fmt -p <your-crate>` to format while a wave is running; `cargo fmt --all` rewrites
files other agents have open.

A failing test reported as passing costs more than the bug did, and costs it later, when three
other agents have built on top of it.

## Waves

| Wave | Agents | Owns |
|---|---|---|
| M0 spike | S1 render, S2 protocols, S3 widget/headless | `spike/**` only; the timeboxed go/no-go on Blitz embedding |
| H1 freeze | 1 | every module as signatures with `todo!()`, the `state.rs` delegate list, the pins |
| H2 gpu | 1 | `gpu/**`, `headless.rs` |
| H2 dom | 1 | `dom/**`, `wake.rs`, `bridge.rs`, `root.rs`, `clock.rs` |
| H2 input | 1 | `input/**`, `handlers/{pointer,keyboard,text_input,data_device,seat}.rs`, `ime.rs`, `clipboard.rs`, `cursor.rs` |
| H2 protocols | 1 | `handlers/{compositor,output,layer,xdg,fractional,background_effect,activation}.rs`, `region.rs`, `scale.rs`, `blur.rs`, `popup.rs` |
| H2 core | 1 | `host.rs`, `state.rs` bodies, `surface.rs`, `role.rs`, `frame.rs`, `msg.rs`, `event.rs`, `handle.rs`, `debug.rs`, `examples/` |

`state.rs` is the only file that names SCTK delegate macros; `gpu/raw_handle.rs` is the only
module that may `#[allow(unsafe_code)]`.

H2 is done when every spike item re-passes on `examples/demo_*.rs` built on the real API.

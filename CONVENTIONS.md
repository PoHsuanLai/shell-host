# Conventions

Binding on every contributor, human or agent: how the code in shell-host is written.
`DESIGN.md` maps the design to the code; `FINDINGS.md` holds the facts it relies on.

If a convention here blocks you, say so. Do not quietly deviate.

---

## 0. Design style

The rules below are the general shape of the code. Everything after this section is a specific
application of them.

### Prefer pure functions

A function that takes values and returns values can be tested with a table, reasoned about in
isolation, and reused from a context its author never imagined. One that reaches for a socket,
a clock or a global cannot.

Push effects to the edges. `DESIGN.md` marks every module "pure" or "effects": the frame
machine, the lock machine, the scroll physics, region resolution and the grab rule are pure and
table-tested; the handlers, `ctx/` and `gpu/` carry them out. When pure code seems to need an
effect, the effect belongs to the caller: `frame::step` returns the actions to take rather than
arming a timer, and handlers push `TimerRequest`s rather than touching the loop.

Effects belong in the signature. `&mut self`, `Result`, or living in a module that talks to
Wayland are how a reader sees that something happens; a function that looks pure and is not is
worse than one that is honestly imperative.

### Separate data from logic

Data types describe what is true. Functions describe what follows from it. Keep them apart.

A struct's own `impl` holds construction, derivation and accessors — the things that are
*about* that value. Anything that coordinates several values, or makes a decision, reads better
as a free function or as an `impl` on the type that owns the decision.

Avoid the type that grows methods because it was convenient rather than because they belong.
If half a struct's methods never touch half its fields, it is two types.

### Make illegal states unrepresentable

Encode invariants in types rather than checking them at runtime and hoping every caller
remembers.

- Newtype every identifier and every unit. `SurfaceId`, not `u64`; `Scale` in 120ths, not `f64`.
  A newtype costs nothing at runtime and makes an argument-order mistake a compile error.
- No stringly-typed fields where a closed set exists.
- No `bool` in state (see §10). No `bool` parameters either: a call reading `set(true)` tells
  the reader nothing, while `set_maximized(Maximize::Set)` tells them everything.
- Prefer an empty collection to `Option<Vec<T>>`. There is one way to say "none" and it needs
  no unwrapping.
- Model absence precisely: `Extent::Fill` instead of a size of 0 that means "stretch".

### Parse, don't validate

Convert untrusted input into a type that cannot be wrong, once, at the boundary — then the rest
of the program needs no defensive checks (`Mime::parse`, `LocalSource::of`, the palmrest wire
types). A validation function that returns `bool` and leaves the caller holding the same loose
type has moved the problem, not solved it.

A fallible constructor returns `Result`. A constructor that cannot fail returns `Self`.

### Traits are seams, not decoration

A trait earns its place when two implementations are genuinely swapped at that boundary (the
`Flush` trait `tests/core.rs` records the flush order through). A trait with one implementation
is indirection with no payoff, and a trait invented to group functions that merely feel related
makes the call site harder to follow.

Prefer an enum to a trait for closed vocabulary, so that adding a variant produces a list of
compile errors naming every site that must be updated. A `dyn Trait` hides that list.

Implement the standard traits when the semantics genuinely hold, and derive rather than
hand-write unless a behaviour must differ. `Display` is for humans, `Debug` is for developers,
and neither is a serialization format — that is `serde`'s job.

### Errors are values, and they carry a decision

See §5. An error type exists so a caller can *act*, not so a string can be logged.

Reserve panics for broken invariants inside our own code, and document them as caller
contracts. Malformed input is never a panic: bytes from a compositor, a pipe or a socket came
from a stranger.

### Borrowing from other projects

This workspace is `MIT OR Apache-2.0`, and the licence of what you read determines what you may
do with it:

- **permissive** (MIT, Apache-2.0, BSD, 0BSD) — code may be copied, with attribution (the
  winit-derived key tables, FINDINGS F5).
- **weak copyleft** (MPL, LGPL) — link freely, but MPL's unit of copyleft is the *file*: paste one
  function and that file becomes MPL, and our crate-level licence claim becomes false.
- **strong copyleft** (GPL, AGPL, EUPL) — read for facts, never paste.

**The rule for reading is note-and-close.** Read the source, reduce what you learned to a
sentence about observable behaviour, close the file, implement from the sentence. If it fits in
a sentence about what a *compositor* does, it is a fact and it is yours; if you would need the
source open to reproduce it, it is expression and it is theirs. Never put GPL, AGPL or MPL source
into a model's context and ask for an equivalent.

**Cite the behaviour, not the program.** Write what the compositor does and point at the trace
that proves it. **A quirk without a trace is a rumour.**

### Composition over accumulation

Prefer iterator chains and small combinators where they read more clearly than a loop, and a
plain loop where they do not — clarity wins over point-free style, every time. Prefer
returning a new value to mutating an argument. Where a mutable accumulator really is the
clearest expression, keep it local to the function so the mutation cannot be observed from
outside.

Keep functions short enough to hold in your head. A long function is usually several functions
that have not been named yet.

## 2. Derives

Every public type derives `Debug` (by hand where a field has none). Then, where they apply:

- `Clone`, `PartialEq`, `Eq` — for data.
- `Copy` — only for types that are one word or smaller (ids, fieldless enums).
- `Hash` — every id newtype, and anything used as a map key.
- `PartialOrd, Ord` — only where a total order genuinely means something.
- `Default` — only where a default is genuinely meaningful.
- `Serialize, Deserialize` — only where the type is data all the way down (FINDINGS F11).

## 3. Serde

- **Enums with data: adjacently tagged**, `#[serde(tag = "kind", content = "v", rename_all =
  "snake_case")]`. Internal tagging cannot represent newtype variants over non-map types.
- **Fieldless enums:** `#[serde(rename_all = "snake_case")]`.
- **Structs:** field names as written, `snake_case`.
- A wire form owned by another program (palmrest's) is re-declared with exactly its serde
  attributes and tested against bytes that program produced.

## 4. `#[non_exhaustive]`

On nothing. There are no external consumers, so `non_exhaustive` buys no compatibility and
costs exhaustive matching — which is the reason the vocabulary is enums.

## 5. Errors

One error enum per crate, built with `thiserror`: `HostError`, each variant answering a
`Recovery` (`Retry`, `Fallback`, `Degrade`, `Fatal`) so the caller can act on it.

- No `anyhow`. Libraries return typed errors.
- No `unwrap()` or `expect()` in non-test code, except where a comment on the same line
  proves the invariant.
- `panic!` is for programmer error only.

## 6. Naming

- Enums read as values at the use site: `Visibility::Hidden`, not `Visibility::IsHidden`.
- Predicate **returns** may be `bool`; state and parameters may not (§10).
- No `get_` prefix. `fn scale(&self)`, not `fn get_scale(&self)`.
- Wire vocabulary stays at the wire: the public types say what a value means, and the wire
  mapping lives beside them.

## 7. Modules and files

One concept per file; a file over ~400 lines wants splitting. `lib.rs` declares modules and
re-exports the flat public surface — it holds no definitions.

Public items carry a doc comment saying what they mean, not what they are.

## 8. Tests

- **Unit tests** in `#[cfg(test)] mod tests` beside the code, for private behaviour.
- **Integration tests** in `crates/shell-host/tests/<topic>.rs`, for the public surface.
- Table-driven wherever there is more than one case: a `const CASES: &[(Input, Expected)]`
  and one loop. A failure must name the case.
- Tests never touch the real system: no connection to the user's compositor, no real bus, no
  real `~/.config`. Live acceptance runs on the headless KWin (`scripts/dev/kwin-virtual.sh`)
  or a nested cosmic-comp, never on `wayland-0`.

## 9. Verification

Before reporting work complete, all of these must pass:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
./scripts/check-boundary.sh
cargo deny check licenses
```

`scripts/check-boundary.sh` keeps COSMIC-specific crates and Blitz's scrollbars out of the
tree. `cargo tree -p <crate> -i <dep>` **exits 101 when the dependency is absent**, which is the
condition we want, so the script checks for output rather than the exit status.

Report honestly. A failing test reported as passing costs more than the bug did.

## Substrings are not tokens

`contains`, `ends_with` and `starts_with` compare *characters*. A name, a protocol interface,
a MIME type and a word are **tokens**, and matching one with a substring is a bug that passes
its own test. When the thing being matched has a grammar, match it at its boundaries: compare
whole names, split on the separator, use the helper that exists.

The cost is asymmetric. A too-narrow match fails visibly on input you have. A too-wide one
silently does the wrong thing on input you have not thought of.

Before reaching for text at all, ask what the protocol already told you: an event, a global's
presence, an error code. Prose is the part of a protocol nobody pins down, so it is the last
thing to branch on.

## Run it the way its user would, before concluding anything about it

A failure that reproduces only through your own harness is a fact about the harness. Before
reporting that something does not work, run it with nothing of yours around it: no wrapper, no
forced backend, no injected variable. If it works there, the finding is about the wrapper.

## An assertion that was already true proves nothing

Assert the difference the action makes, not a state the action happens to be compatible with.
Read the quantity before, act, and compare; where the "before" is awkward to capture, name the
exact value the action must produce. The tell is an assertion that could be moved *above* the
line that does the work and still pass. A fix is proven by a test that fails without it.

## 10. Shared rules (quire, shell-host, sill and every bundled app)

- Small structs; functional style where it fits; data separate from logic; type-driven:
  newtypes and enums carry meaning, traits only at real seams.
- No `bool` anywhere in a public signature or a settings key: two-state enums with named
  variants (`Magnification::{On, Off}`, `Availability::{Enabled, Disabled}`).
- No long files or long functions: one concept per file, split near 400 lines (§7); a function
  that needs a comment to separate its phases is two functions.
- No `unsafe` outside the single allowed module per repo (shell-host's raw window handle),
  each block with a SAFETY comment; `unsafe_code = "deny"` at the workspace.
- Dependencies are settled first: the pinned block in quire's `docs/workspace-deps.toml` is
  copied verbatim into every workspace; a new dependency is a change to that file, reviewed,
  then copied, never added to one crate alone.
- Design system rules: every class is prefixed `ds-`; variants in `data-variant`/`data-size`,
  state in `aria-*`; consumers never style `.ds-*` or `[data-theme|accent|motion|material]`;
  every colour, duration, easing, keyframe and font comes from the token table; a missing
  component or token is added to quire first, never patched locally.
- Every value the design docs mark "proposed" is read from a settings key
  (`design/22-SETTINGS.md`) with that default; never hard-coded.

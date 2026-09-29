# Conventions (shared by quire, sill, shell-host, palmrest)

This text is identical in every repo. Each repo's ARCHITECTURE.md adds its crate map, its
"one home" table, its recipes and its repo rules. Where the two disagree, ARCHITECTURE.md wins
for that repo.

The aim is code that reads as one deliberate design: a newcomer who has read one service, one
surface or one component has read them all. Every rule below exists because the code once broke
it; the "because" lines say how. If a rule blocks you, say so in your report; never deviate
quietly.

---

## 1. Before writing anything

1. **Find the home.** Look the concept up in ARCHITECTURE.md's "one home" table. If it has a
   home, extend that; never write a second one. *Because:* colour maths was written seven times,
   show/hide machines eight times, battery drawing six times, popup menus twice, and sill kept
   its own copy of quire's menu tracker after quire took it over.
2. **Find the shape.** If you are adding a kind of thing that already exists (a service, a
   surface, a control-center module, a widget, a component, a settings section, an IPC verb),
   copy the recipe in ARCHITECTURE.md. Your new one must look like its siblings: same files,
   same names, same trait. *Because:* 17 surfaces used about six shapes with seven names for
   "the pure model".
3. **Find the layer.** Decide which crate and layer the code belongs to before choosing a file.
   A lower layer never names a higher one. If what you need lives above you, the design is
   wrong: move the shared piece down, or pass it in. *Because:* the design system's base
   vocabulary lived in its top layer, and six cycles followed.
4. **Find the repo.** Portable code (any desktop, any OS) never lives in the COSMIC shell. A
   missing primitive in another repo is added there first, never patched locally. If you
   cannot edit that repo, stop and report.

## 2. Files and modules

- **One concept per file.** Name the file after the concept (`presence.rs`, `wifi_networks.rs`),
  never after the work that produced it. Forbidden in file, module, test and function names:
  `v2`, `new_`, `old_`, `fixes`, `followups`, `polish`, `gaps`, `round`, `wave`, a ticket id,
  another project's name. *Because:* tests named `mailo5_*`, `palette_followups` and
  `launcher_v2` told nobody what they tested.
- **Size.** Aim for under 300 lines; at 400, split by concept, never by halves or rounds. A
  function over about 50 lines, or one that needs a comment to separate its phases, is two
  functions: a long function is usually several that have not been named yet. Tables (keymaps,
  glyph data) are exempt.
- **Directories group a concept, not a kind.** `dock/` holds everything the dock is. A flat
  directory of 200 prefix-named files (`menu_*`, `palette_*`) is a directory waiting to be made.
- **`mod.rs` / `lib.rs` hold declarations and re-exports only**, plus at most the one type the
  module is named after. No logic, no components, no constants that belong in a file.
- **Fixed role file names** inside a concept directory (a repo's ARCHITECTURE.md may add more):

  | file | holds |
  |---|---|
  | `model.rs` | the state types (data only) |
  | `step.rs` | the pure transition: `(State, Input) -> (State, Vec<Effect>)` |
  | `view.rs` | Dioxus components; no decisions beyond reading the model |
  | `io.rs` / `backend/` | effects: D-Bus, files, processes, the compositor |
  | `style.css` | the concept's stylesheet |
  | `tests.rs` or `tests/` | unit tests for this concept |

- **No `unsafe`** outside the one module a repo's ARCHITECTURE.md names, with
  `unsafe_code = "deny"` at the workspace and a `SAFETY` comment on every block. No
  `std::env::set_var` (unsafe since edition 2024): make the read injectable instead.

## 3. Public API

- **Private by default.** Modules are private; items are `pub(crate)` unless another crate uses
  them today. Do not publish something because a future caller might want it.
- **One path per public item**, re-exported once at the crate root or from one public module.
  No glob re-exports (`pub use x::*`).
- **Unique, specific names.** No two public types share a name in one crate (`Placement`,
  `Layers`, `Ground`, `Held` each existed twice). No bare generic names at a crate root
  (`Level`, `Filter`, `Step`, `Key`, `Text`, `Run`): qualify them (`OsdLevel`, `SearchFilter`).
- **Test helpers are not API.** They live in `tests/support/`, a `testing` feature, or a dev
  crate; never plain `pub` in the lib. Dry-run backends, scripts and demo drivers are test or
  dev code.
- **An API change updates every caller in the same change.** No deprecated alias, no "kept
  for callers", no old path re-exported "for one release".
- **`#[non_exhaustive]` on nothing.** Nothing is published, so it buys no compatibility and
  costs exhaustive matching, which is the reason the vocabulary is enums.

## 4. Types

Encode invariants in types rather than checking them at runtime and hoping every caller
remembers.

- **Data apart from logic.** Data types describe what is true; functions describe what follows
  from it. A type's own `impl` holds construction, derivation and accessors. Anything that
  coordinates several values, or makes a decision, is a free function or belongs to the type
  that owns the decision. If half a struct's methods never touch half its fields, it is two
  types.
- **No `bool`** in a struct field, a parameter, a settings key or a message. Use a two-variant
  enum whose names say what they mean (`Presence::{Present, Absent}`): `send(true)` tells the
  reader nothing, `send(Confirm::Yes)` tells them everything. Predicates may return `bool`.
- **Newtype every unit and identifier** (`Px`, `Permille`, `Ms`, `AppId`, `OutputId`). Never
  pass a bare `u32` that means pixels. A newtype costs nothing at runtime and makes an
  argument-order mistake a compile error.
- **No strings where a closed set exists.** Closed vocabularies are enums implementing `Word`
  (`ALL`, `slug`, `label`, `parse`). Never hand-write `slug()`/`parse()`/`ALL`. *Because:*
  there were 112 hand-written `slug`s.
- **Parse at the boundary, once.** Settings strings, D-Bus values, file contents, device bytes
  and CLI arguments become typed values where they enter, and the rest of the program needs no
  defensive checks. A service never parses a settings string. A validation function that
  returns `bool` and leaves the caller holding the same loose type has moved the problem, not
  solved it. A fallible constructor returns `Result`; one that cannot fail returns `Self`.
- **Model absence and failure in the state.** A service that cannot reach its backend publishes
  `Unavailable` (or its equivalent). It never parks silently at `Default`. Prefer an empty
  collection to `Option<Vec<T>>`, and a type that cannot represent zero of something to an
  `Option<u32>` whose every caller must decide what `Some(0)` meant.
- **Two or more `match`es on the same enum in different files are a missing method or trait.**
  Put the behaviour on the enum, or give each variant an implementation of one trait, and keep a
  single match. *Because:* the control-center module enum was matched in ten places.

## 5. Traits

- A trait exists when **two or more implementations** are swapped at that boundary (a real one
  and the fake a test drives counts), or a **generic consumer** runs over many implementations
  (the daemon spawning every `Surface`). One implementation is not a trait, and neither is a
  group of functions that merely feel related.
- **The canonical traits are the only way in.** A new service implements `Service`; a new
  surface implements `Surface`; a CC module implements `CcModule`; a widget implements
  `WidgetProvider`; a pure machine implements `Machine`; a renderer seam goes through
  `DocumentHost`. ARCHITECTURE.md lists each repo's traits. Do not hand-roll a parallel shape
  "because this one is special": if it does not fit, the trait is wrong. Report that; don't
  work around it.
- **Closed sets stay enums** (IPC requests, tokens, menu data, launcher rows). A trait there
  hides the compile error you want when a variant is added.
- **Keep traits small.** Split one that has more than about six methods into cohesive parts.
- **Standard traits where their meaning holds.** Derive rather than hand-write unless a
  behaviour must differ. `Display` is for humans, `Debug` for developers; neither is a
  serialization format.

## 6. State, effects and dependencies

- **Pure core, effects at the edges.** Decisions live in `step` functions that take values and
  return values plus the effects wanted. `io` code carries effects out and never decides.
  Components read state and send commands; they do not decide. When pure code seems to need an
  effect, the effect belongs to the caller: return a description of it instead.
- **Effects show in the signature.** `&mut self`, `Result`, or living in a module allowed to do
  I/O is how a reader sees that something happens. A function that looks pure and is not is
  worse than one that is honestly imperative.
- **Values over mutation.** Return a new value rather than mutating an argument. Use iterator
  chains where they read more clearly than a loop, and a plain loop where they do not. A
  mutable accumulator stays local to its function.
- **No globals.** No `static` mutable state, no `OnceLock` holding runtime links, no
  `thread_local!` outside tests. Pass values in.
- **Everything ambient is injected.** The environment, the clock, directories, D-Bus
  connections, the Wayland connection, hardware and child processes come from `Env` (or the
  repo's equivalent). Reading `std::env`, `SystemTime::now`/`Zoned::now`, `dirs::*`,
  `Connection::system()` or `Command::new` anywhere else is a bug. *Because:* test switches
  were env vars read deep in services, and tests reached the real buses and the real `~/.cache`.
- **Time is an argument.** A function that needs the time takes it, or reads the repo's one
  clock seam. Tests use fixed instants or a virtual clock. Configured intervals are `Duration`s.
- **One runtime owner.** Library crates never call `tokio::spawn`; they take a spawner or
  return futures.
- **Timing comes from tokens and timelines**, never from ad-hoc `sleep`s. Animation state is
  driven by `Timeline`/`Presence`, never by a per-component timer.
- **Proposed values are settings.** Every value the design docs mark "proposed" is read from a
  settings key (`design/22-SETTINGS.md`) with that default, never hard-coded.

## 7. Errors and logging

- An error exists so a caller can **act**, not so a string can be logged. One `thiserror` enum
  per crate or service, with variants a caller can act on. No `anyhow` in a library.
- No `unwrap`/`expect` outside tests unless a comment on the same line proves the invariant.
- Panics are for broken invariants in our own code, documented as caller contracts. Malformed
  input from outside (a bus, a file, a socket, a device, the compositor) is never a panic.
- Log through the repo's one logging path, with the subsystem as a prefix. No stray `eprintln!`.

## 8. Tests

- **Name tests after behaviour:** `row_leaves_and_rows_below_heal`, not `fixes_3`.
- **Unit tests beside the code** (`#[cfg(test)] mod tests`, or the concept's `tests.rs`) for
  private behaviour; **integration tests** in `crates/<crate>/tests/<topic>.rs` for the public
  surface.
- **Pure functions get table tests.** One table per function, a `const CASES: &[(Input,
  Expected)]` and one loop; each row names its case, and a failure names the row.
- **Behaviour tests go through the real shape**: a surface test drives the surface through the
  shared harness, never a per-test copy of `World { .. }`. One harness per repo.
- **Tests never touch the real system:** no real D-Bus services, no `/dev/i2c-*`, no input
  device, no real `~/.config` or `~/.cache`, no real compositor session, no real daemons. Use
  `Env::isolated`, private buses and scratch directories. A test that needs more is a dev script
  under the same rules.
- **Goldens change only with a reason** in the commit message. Never re-bless to make a test
  pass.
- **An assertion must be able to fail.** Check that the test fails without your change.
- **Assert the difference the action makes**, not a state the action happens to be compatible
  with. Read the quantity before, act, and compare (`before + 1`, not `> 0`); where the "before"
  is awkward, name the exact value the action must produce. The tell is an assertion that could
  move above the line doing the work and still pass. Check every `>`, `<`, `is_empty`,
  `is_some`, `contains` and `!`: that is where a condition wide enough to be satisfied by
  accident hides.
- **Ask the program for the thing under test.** A test that spells out the value under test
  and asserts against its own copy agrees with itself whatever the program does.

## 9. Comments and docs

- **Comments say why**, in the present tense, about the code as it is. They never narrate
  history ("used to", "was renamed from", "moved from mailo", "kept for its callers") and never
  cite tickets, rounds, waves, milestones or dates. Design references (`design/05 §3`) are fine.
- **Public items carry a doc comment** saying what they mean, not what they are.
- **No "for now", "until", "later", "temporary"** in code or docs. If it is temporary, it is a
  FINDINGS open item with the condition that ends it.
- **Docs describe the present.** ARCHITECTURE.md is the map, CONVENTIONS.md the rules, FINDINGS.md
  the open items plus standing facts. No changelogs, no wave plans, no migration guides left
  behind after the migration.

## 10. Change discipline

- **Replace, don't add beside.** When something supersedes old code, delete the old code in the
  same change: no v1 beside v2, no fallback for a gap that is closed, no feature flag for
  retired behaviour, no `todo!()` stub on master.
- **No backward compatibility** for config keys, file formats, IPC or APIs until the user
  says something is released: no aliases for old names, no migrations.
- **Dependencies are settled first.** The pinned block in quire's `docs/workspace-deps.toml` is
  copied verbatim into every workspace; a new dependency is a change to that file first, never
  to one crate alone. Never write what a dependency already does (a second PNG encoder); never
  add a dependency for something ten lines do.
- **Small commits**, one concern each, compiling at every commit, message in the imperative.

## 11. Dev scripts

- Scripts share one library (`dev/lib/`): daemon start/stop, compositor, input, pixels. A script
  never redefines a library function.
- One acceptance script per surface or feature, with flags for variants (`--real-input`), not a
  family of near-copies.
- Scripts obey §8's rules: private system and session buses, scratch HOME/XDG, fake hardware.

## 12. Derives and serde

- **Public types derive, in this order, what holds:**
  `#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]`. `Copy` only for types one
  word or smaller; `Hash` for every identifier and map key; `PartialOrd, Ord` only where the
  order means something; `Default` only where a default is meaningful.
- **Data is `Eq`, so floats stay out of it.** Store integer units (`Scale` in 120ths,
  `Permille`) and compute a float where arithmetic needs it. A type that must hold a float says
  why in its doc comment.
- **Secrets write `Debug` by hand**, redacted: a derived `Debug` on a password, a token or a
  clipboard entry leaks into logs, panics and error chains.
- **`Serialize, Deserialize` only on a type that is stored or crosses a wire.** Its serde form
  is its schema. A type that is neither has no serde.
- **Enums with data are adjacently tagged:** `#[serde(tag = "kind", content = "v", rename_all =
  "snake_case")]`. Internal tagging cannot represent newtype variants over non-map types, and
  fails at runtime. Fieldless enums take `rename_all = "snake_case"`; struct fields keep their
  `snake_case` names.
- **Never `deny_unknown_fields`** on a stored type: an unknown key does not stop a load.
- **Every stored or wire type has a round-trip test.** A wire form owned by another program is
  re-declared with exactly its serde attributes and tested against bytes that program produced.

## 13. Names

- Enums read as values at the use site: `Availability::Disabled`, not `Availability::IsDisabled`.
- No `get_` prefix: `fn label(&self)`, not `fn get_label(&self)`.
- Name a value for what it does, not for where it came from. Wire vocabulary stays at the wire:
  public types say what a value means, and the wire mapping lives beside them.
- Code, class names and tokens never name the reference platform.

## 14. Verification

- **The gate passes before work is reported complete:** `cargo fmt --all --check`, `cargo
  clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace`,
  `./scripts/check-boundary.sh` where the repo has one, and `cargo deny check licenses`.
  ARCHITECTURE.md gives the repo's exact commands. Check every exit code, never a piped summary.
- **`cargo tree -i <dep>` exits 101 when the dependency is absent**, which is the state a
  boundary check wants; a boundary script checks for output, not exit status.
- **While others share the checkout,** run `cargo fmt -p <crate>`: `cargo fmt --all` rewrites
  every file in the workspace. `--all --check` is read-only.
- **Substrings are not tokens.** `contains`, `ends_with` and `starts_with` compare characters.
  A domain, a class name, a CSS property, an id and a word are tokens: `ends_with("example.com")`
  matches `evil-example.com`. When the thing matched has a grammar, match it at its boundaries:
  compare whole labels, split on the separator, use the tokenizer. *Because:* a too-narrow match
  fails visibly on input you have; a too-wide one silently does the wrong thing on input you
  have not thought of. Before reaching for text at all, ask what the structure already told
  you (an event, a reply to a known command, a value's position in a grammar); prose is the
  last thing to branch on.
- **Run it the way its user would** before concluding anything about it. A failure that
  reproduces only through your own harness is a fact about the harness: run it with no wrapper,
  no forced backend, no injected variable. A component-tree test proves the components agree
  with each other, not that the renderer draws them or that a keystroke arrives.
- **Report honestly.** A failing test reported as passing costs more than the bug did.

## 15. Borrowing from other projects

Every repo is `MIT OR Apache-2.0`, and the licence of what you read decides what you may do
with it (quire's `docs/licensing-references.md` has the verified table):

- **Permissive** (MIT, Apache-2.0, BSD, 0BSD): code may be copied, with attribution.
- **Weak copyleft** (MPL, LGPL): link freely, never paste. MPL's unit is the file: paste one
  function and that file becomes MPL, and the crate's licence claim becomes false.
- **Strong copyleft** (GPL, AGPL, EUPL): read for facts, never paste.

**Note-and-close.** Read the source, reduce what you learned to a sentence about observable
behaviour, close the file, implement from the sentence. If it fits in that sentence, it is a
fact and it is yours; if you need the source open to reproduce it, it is expression and it is
theirs. Matching identifiers, comments, branch order, magic constants, error strings or bugs
mean you copied.

**Never put GPL, AGPL or MPL source into a model's context** and ask for an equivalent. Put the
spec section and our own fixture in the prompt instead.

**Cite the behaviour, not the source.** `// taken from <gpl-project>/file.c:412` is a pointer to
copyleft code that reads as an admission. Write what the behaviour is and point at the test or
trace that proves it; a quirk without a trace is a rumour.

---

## What each repo's ARCHITECTURE.md must contain

1. **Crate map and layers,** with the allowed dependency direction.
2. **The "one home" table.** Concept → the one file or module where it lives. For example:
   colour maths → `ds-core::colour`; show/hide → `ds-motion::Presence`; tweens →
   `ds-motion::Timeline`; menus → `ds::menus` + `MenuModel`; popups → `sill-ui-kit::MenuPopup`;
   surface root → `SurfaceRoot`; env/clock/dirs → `Env`.
3. **The canonical traits,** with their signatures.
4. **Recipes:** add a service, add a surface, add a CC module, add a widget, add a component,
   add a settings key, add an IPC verb, add a dev script. Each is a numbered list of the files
   to create and the lines to add.
5. **The test harness:** how to drive the repo's things in a test.
6. **Repo rules:** the gate's exact commands, the one module allowed `unsafe`, and every rule
   that holds for this repo alone.

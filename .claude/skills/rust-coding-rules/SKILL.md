---
name: rust-coding-rules
description: Use whenever writing, reviewing, or modernizing Rust in this project — library crates, CLI and TUI tools, async/Tokio services, or embedded/no_std code. Covers the project Rust idiom and safety baseline (pinned edition and MSRV, rustfmt and clippy with warnings denied, typed errors and no unwrap on runtime paths, ownership and newtype discipline, minimal justified unsafe with safety comments, no_std and embedded constraints, async cancellation and no blocking or lock-across-await, minimal public API surface, and rustdoc on public items). Apply this any time Rust is being authored or judged, when setting up a crate's lints, or when someone asks for Rust coding standards — not only when "Rust rules" are named. This is the Rust counterpart of c23-coding-rules. Rules use stable IDs RS-NN; treat it as a baseline to curate.
---

# rust-coding-rules

The project's Rust coding baseline — the Rust counterpart of `c23-coding-rules`.
Rules are SHOULD unless marked MUST (MAY for optional). Stable IDs `RS-NN`. This
is a proposed baseline; curate the severities to your house policy.

Rust enforces several things the C rules need discipline for (no globals, memory
safety, exhaustive errors), so these rules lean on the toolchain: most are a
clippy lint or a compiler setting, not a review note.

## Foundational — always on

- **RS-1** MUST pin the edition and a minimum supported Rust version (MSRV) per
  crate, and test the MSRV in CI. Project baseline: **edition 2024**, MSRV
  **1.85** — keep `rust-version` in `Cargo.toml`, `msrv` in `clippy.toml`, and the
  MSRV CI job in sync. (Edition 2024 makes `unsafe_op_in_unsafe_fn` default-deny,
  so RS-4 partly comes for free.)
- **RS-2** MUST enforce `rustfmt` (`cargo fmt --check` in CI); formatting is not a
  review topic.
- **RS-3** MUST run `cargo clippy` with warnings denied (`-D warnings`). Enable at
  least the project lint set below; opt into `clippy::pedantic` selectively.
- **RS-4** MUST set `#![forbid(unsafe_code)]` at the crate root for any crate that
  has no genuine need for `unsafe`; crates that do need it use
  `#![deny(unsafe_op_in_unsafe_fn)]` and justify each block (RS-30).

Project clippy lints worth denying: `unwrap_used`, `expect_used` (outside tests),
`panic`, `missing_safety_doc`, `await_holding_lock`, `undocumented_unsafe_blocks`,
`missing_docs` on public crates.

**These ship as drop-in config — do not hand-translate the prose:**

- `references/workspace-lints.toml` — the `[workspace.lints]` table (rust + clippy
  levels) implementing RS-3/RS-4/RS-10/RS-12/RS-30/RS-52/RS-90, plus the per-test
  relaxation for `unwrap`/`expect`. Paste into the workspace root `Cargo.toml`;
  each member crate adds `[lints]\nworkspace = true`.
- `references/clippy.toml` — complexity thresholds, `disallowed-methods`
  (non-deterministic clocks, per RS-80), `disallowed-types` (globals, per RS-24),
  and the `msrv` pin (RS-1).
- `references/deny.toml` — `cargo-deny` advisory/license/source allow-lists for
  the RS-3 supply-chain checks.

Curate the allow-lists and thresholds to house policy; the files carry inline
rationale per setting.

## Error handling

- **RS-10** MUST NOT `unwrap()`/`expect()`/`panic!` on any runtime-reachable path
  in a library. Reserve them for tests, `const` contexts, and truly-impossible
  cases (then document why).
- **RS-11** Libraries return typed errors (an enum, e.g. via `thiserror`);
  applications MAY use a flattened error (`anyhow`) at the top level only. Don't
  push `anyhow` into library APIs.
- **RS-12** MUST NOT ignore a `Result`; propagate with `?` or handle it.
  `#[must_use]` results and important return values.
- **RS-13** SHOULD prefer `?` over manual `match` for propagation; convert with
  `From`/`#[from]`.
- **RS-14** MUST NOT panic in `Drop`, in FFI callbacks, or across an `.await` in a
  way that leaves invariants broken. Define each layer's failure policy. Set the
  crate's panic strategy explicitly: `panic = "abort"` for `no_std`/embedded and
  for binaries where unwinding buys nothing (smaller, no unwind tables); unwind
  only where a panic hook must run cleanup first (e.g. the host TUI restoring the
  terminal). State the choice in the relevant `[profile]`.

## Ownership, types, and data

- **RS-20** SHOULD borrow, not clone, to satisfy the borrow checker; reach for
  `Clone`/`Rc`/`Arc` deliberately, not reflexively.
- **RS-21** SHOULD make illegal states unrepresentable: newtypes for units and IDs
  (`struct Millis(u32)`, not a bare `u32`), enums over boolean flags and magic
  numbers. (Same intent as C23 R40; the type system does more of the work here.)
- **RS-22** SHOULD accept borrowed parameters (`&str`, `&[T]`, `impl AsRef<…>`) and
  return owned values only when ownership transfer is real.
- **RS-23** SHOULD derive `Debug` widely; derive `Clone`/`Copy`/`PartialEq`
  deliberately. No `Copy` on types whose copy is surprising or expensive.
- **RS-24** MUST NOT use mutable `static`. There are no global variables (same as
  software-design-rules); thread state through a context or use a properly
  synchronized type behind a narrow interface.

## Unsafe

- **RS-30** MUST keep each `unsafe` block minimal and precede it with a
  `// SAFETY:` comment stating the invariant that makes it sound.
- **RS-31** MUST encapsulate `unsafe` behind a safe, checked API; callers should
  not need `unsafe` to use it correctly.
- **RS-32** MUST NOT use `transmute`, `MaybeUninit` shortcuts, or
  pointer-aliasing tricks without a written justification and a test. Prefer safe
  alternatives.
- **RS-33** For MMIO/registers, MUST use `read_volatile`/`write_volatile` or a PAC,
  never plain dereference of a cast address. (C23 counterpart: `volatile` + R40.)

## no_std and embedded

- **RS-40** SHOULD be `#![no_std]` where the target warrants it; gate `std`-only
  code behind a feature.
- **RS-41** SHOULD avoid heap allocation on constrained or real-time paths; use
  fixed-size buffers, `heapless`, or const generics rather than `Vec`/`String`.
- **RS-42** MUST go through a HAL/PAC for hardware access — no ad-hoc register
  pokes scattered through logic (software-design-rules layering).
- **RS-43** MUST NOT block or allocate in an interrupt/critical section; keep ISRs
  minimal and defer work, mirroring the C concurrency rules.

## Async (Tokio)

- **RS-50** MUST NOT block in async code (no synchronous I/O, no long CPU work) —
  offload with `spawn_blocking` or a dedicated thread.
- **RS-51** SHOULD make futures cancellation-safe, or document that they are not;
  assume any `.await` may be cancelled.
- **RS-52** MUST NOT hold a non-async lock (`std::sync::Mutex`) across an `.await`
  (clippy `await_holding_lock`); use an async lock or release before awaiting.
- **RS-53** SHOULD bound channels and apply backpressure; unbounded queues hide
  failures until they're memory problems.

## Modules and public API

- **RS-60** SHOULD keep the public surface minimal: `pub(crate)` by default, `pub`
  only what's intended for consumers (software-design-rules information hiding).
- **RS-61** SHOULD follow the Rust API Guidelines: conventional naming,
  `#[must_use]` on builders/iterators/pure queries, `impl Trait` at boundaries
  where it reads better.
- **RS-62** SHOULD use `#[non_exhaustive]` on public enums/structs expected to
  grow, so adding variants/fields isn't a breaking change.
- **RS-63** SHOULD NOT leak external crates' types in a public API unless that
  coupling is intentional and documented.

## Concurrency

- **RS-70** Shared mutable state lives behind `Mutex`/`RwLock`/atomics with a
  documented contract; prefer message passing (channels) over shared state.
- **RS-71** MUST NOT hand-implement `Send`/`Sync`; let the compiler derive them.
  Any manual `unsafe impl Send/Sync` needs an RS-30 safety comment.

## Testing and determinism

- **RS-80** SHOULD unit-test the pure core; inject clock, RNG, and I/O (traits or
  parameters) so tests are deterministic — no hidden time or randomness.
- **RS-81** SHOULD use `#[cfg(test)]` and dev-dependencies for test-only code; don't
  grow runtime features just to test.
- **RS-82** SHOULD provide doctests for public examples so the docs can't rot.

## Comments, naming, and process

- **RS-90** Public items get rustdoc `///` (see `code-comments`); document errors,
  panics, and safety on the items that have them.
- **RS-91** Idiomatic naming (snake/Camel/SCREAMING per item kind); no Hungarian
  notation, no redundant prefixes.
- **RS-92** `TODO`/`FIXME` carry a task ID — `// TODO(DEV-TOOLS-TASK-0042): …` — so
  `task-from-sources` can harvest them.
- **RS-100** One logical change per commit; deprecate with `#[deprecated]` and
  migrate, rather than mixing cleanup with behavior change (`git-commit`).

## Enforcement map

- RS-2 ← `cargo fmt --check`. RS-3 ← `cargo clippy -- -D warnings` + the lint set.
- RS-4/RS-30 ← `forbid(unsafe_code)` or `undocumented_unsafe_blocks` +
  `missing_safety_doc`. RS-10/RS-12 ← `unwrap_used`/`expect_used`/`must_use`.
- RS-52 ← `await_holding_lock`. RS-90 ← `missing_docs`. RS-1 ← MSRV CI job.
- `cargo-deny` for dependency/license/advisory checks. The rest is review.
- The drop-in artifacts: `references/workspace-lints.toml` (the `[lints]` table),
  `references/clippy.toml` (thresholds + MSRV + disallowed methods/types),
  `references/deny.toml` (`cargo deny check`). Wire `cargo fmt --check`,
  `cargo clippy -- -D warnings`, and `cargo deny check` into CI / `run-all-checks.sh`.

## What this skill does not cover

- Architecture, layering, purity, testability across languages →
  `software-design-rules` (this implements it in Rust).
- C → `c23-coding-rules` (the sibling skill).
- Test conventions, harness, output/logging, coverage → `test-writing-rules`
  (RS-80…82 are the Rust-specific hooks; the testing rules proper live there).
- Comment content → `code-comments`. Commit hygiene → `git-commit`.
- Unfixable violations found in review → tasks via `task-from-sources`.

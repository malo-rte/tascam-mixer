---
name: software-design-rules
description: Use whenever making or reviewing software design and architecture decisions — choosing where logic lives, defining layers and module boundaries, deciding what is a free function vs a method, managing state and dependencies, or reviewing a design or diff for structural quality. Language-agnostic architecture rules (functions and purity, no global state, layer dependency direction, module cohesion and coupling, error-handling discipline, memory and resource ownership, concurrency, types and testability) that the per-language coding skills implement. Apply this any time structure is being decided or judged, when someone asks how to organize a component, or during a design review — not only when "design rules" are named. Rules use stable IDs D-NN; treat it as a baseline to curate.
---

# software-design-rules

The project's language-agnostic design and architecture rules. The per-language
coding skills (`c23-coding-rules`, `rust-coding-rules`) implement these in their
own idioms; this skill is the structural layer above them. Stable IDs `D-NN`.
This is a proposed baseline — curate the severities to your house policy.

Severity: **MUST** / **SHOULD** / **AVOID**.

## How to read a rule

Every rule is a directive with a one-line reason. Two things matter as much as the
directive: the **exception** (when breaking it is legitimate) and whether it is
**enforceable by tooling** or only by review. A rule with no rationale gets
cargo-culted; a rule with no exception gets ignored. When you break a MUST, record
why at the point of the deviation (a comment, or an open-issue in the spec).

## Functions and purity

- **D1 MUST**: anything computable as a free function without side effects is a
  free function. Testable, reusable, no hidden state.
- **D2 SHOULD**: separate computation from I/O — a functional core of pure logic,
  an imperative shell that does the effects. Push side effects to the edges.
- **D3 SHOULD**: command/query separation — a function returns a value *or* causes
  an effect, not both.
- **D4 AVOID**: out-parameters where a return value (or returned struct) works.
  (Language counterparts: C23 R49, Rust RS-22.)
- **D5 SHOULD**: keep a function small enough to hold in your head; deep nesting or
  many branches is a signal to extract.

## State and data

- **D6 MUST**: no global variables. Hidden coupling, not thread-safe, untestable.
- **D7 MUST**: no mutable module-scope/`static` state scattered around. A genuine
  hardware singleton is encapsulated behind one interface with a documented
  invariant, not ambient. (C23 R7-style discipline; Rust RS-24 forbids `static mut`.)
- **D8 SHOULD**: pass state explicitly via a context/handle rather than relying on
  ambient state.
- **D9 SHOULD**: immutable by default; add mutability only where needed.
- **D10 MUST**: single source of truth for any datum — no duplicated or derived
  state that can drift out of sync.

## Layering and dependencies

- **D11 MUST**: define explicit layers (e.g. app → service → driver → HAL →
  registers). Dependencies point one direction only: higher depends on lower,
  never the reverse.
- **D12 MUST**: no cyclic dependencies between modules; the module graph is acyclic.
- **D13 MUST**: callers depend on a layer's interface (header/trait), never its
  internals; no reaching around a layer to its private parts.
- **D14 SHOULD**: dependency inversion at boundaries — lower layers don't call up;
  they notify via callbacks/events injected from above.
- **D15 MUST**: board/SoC-specific code lives behind a HAL; portable code holds no
  direct register or platform knowledge. (C23 R40 + Rust RS-33/RS-42 implement the
  hardware-access side.)
- **D16 SHOULD**: depend in the direction of stability — volatile code depends on
  stable abstractions, not the other way round.

## Modules and interfaces

- **D17 MUST**: one responsibility per module; high cohesion, low coupling.
- **D18 MUST**: hide implementation — opaque types and accessors, minimal public
  surface. (C23: opaque struct + accessors; Rust RS-60: `pub(crate)` by default.)
- **D19 AVOID**: leaking internal or third-party types across a public boundary
  without intent (Rust RS-63).
- **D20 SHOULD**: every public interface carries a doc comment (`code-comments`)
  and, where specified, a reference chapter (`spec-document-template`).

## Error handling

- **D21 MUST**: errors are explicit in the type system — not error codes ignored by
  convention. (Rust RS-10/RS-11 typed errors; C23 R20 `[[nodiscard]]` on status.)
- **D22 MUST**: never silently swallow an error — propagate or handle it.
- **D23 SHOULD**: fail fast on programmer errors (assertions); recover from expected
  runtime errors. Distinguish the two.
- **D24 MUST**: no unrecoverable abort reachable in normal operation; each layer
  states its failure policy.

## Memory and resources

- **D25 MUST**: every allocation/resource has one clear owner — RAII/ownership in
  Rust, paired alloc/free with a documented owner in C.
- **D26 SHOULD**: no dynamic allocation on constrained or real-time paths; use
  static or caller-provided bounded buffers. (Rust RS-41.)
- **D27 MUST**: no unbounded recursion or unbounded loops on real-time paths.
- **D28 MUST**: bounds are always checked; in C, a length travels with every
  pointer.

## Concurrency and interrupts

- **D29 MUST**: shared mutable state across threads/ISRs only behind explicit
  synchronization or a lock-free primitive, with the concurrency contract
  documented.
- **D30 MUST**: ISRs do minimal work and never block; defer to thread context.
  (Rust RS-43.)
- **D31 MUST**: state each function's reentrancy/thread-safety; mark volatility and
  barriers where hardware or concurrent access requires, and say why.

## Types, naming, and simplicity

- **D32 SHOULD**: make illegal states unrepresentable — strong enums over magic
  numbers, newtypes for units (a `Millis` type, not a bare integer). (C23 R40;
  Rust RS-21.)
- **D33 MUST**: no magic numbers; named constants carrying units.
- **D34 SHOULD**: design for test — inject clock, RNG, and I/O rather than
  hardcoding them, so tests are deterministic; no hidden time or randomness.
- **D35 SHOULD**: YAGNI — the simplest design that meets the spec; don't build for
  hypothetical futures.

## Enforcement

Most design rules are review territory, but a few are mechanically checkable and
worth wiring up:

- D6/D7 no globals ← linker map inspection; Rust `static mut` is denied by RS-4.
- D12 acyclic dependencies ← an include-graph / module-graph check in CI.
- D21/D22 explicit, non-ignored errors ← C23 R20 (`-Wunused-result`), Rust
  `unwrap_used`/`must_use`.
- D27 unbounded recursion, D33 magic numbers ← clang-tidy/MISRA, clippy.

Run the rest as a structured design-review pass (the `spec-review-checklist`
approach, applied to code structure). Structural problems you can't fix now become
tasks via `task-from-sources`.

## What this skill does not cover

- Language-specific idioms and safety → `c23-coding-rules`, `rust-coding-rules`
  (they implement these rules per language).
- Comment content → `code-comments`. Commit hygiene → `git-commit`.
- Document/spec structure (not code) → `spec-document-template`.

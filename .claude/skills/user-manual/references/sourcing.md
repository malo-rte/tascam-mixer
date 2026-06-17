# Sourcing from specifications and code

The manual's facts come from the software's specifications and its source code.
The goal of this stage is a set of **grounded usage facts** — each tied to where
it came from — that the drafting stage turns into prose. Two disciplines run
throughout: extract the *usage-relevant* slice (not the implementation), and
never invent what the sources don't say.

## What counts as a usage fact

Keep facts that change what the user does, types, configures, sees, or expects:
- Commands, subcommands, options, and their defaults and effects
- Public API signatures, parameters, return values, raised errors
- Configuration keys, their types, defaults, and valid ranges
- Inputs accepted and outputs produced; file formats
- Observable behavior: states, modes, indicators, error messages, exit codes
- Procedures the user performs and their preconditions/results
- Constraints, limits, and platform/version requirements

Discard or demote pure implementation: internal data structures, private
functions, algorithms, build internals — unless they leak into observable
behavior the user must account for.

When a spec or source comment uses implementation vocabulary that you would
otherwise copy verbatim, translate it during the sourcing pass. Common
categories that must NOT carry through unchanged:

- Crate / package / module names ("links against `devtools-subst`", "via
  `register_all`"). Replace with what the user does; usually delete.
- Type, trait, struct, method names lifted from the source ("the
  `Interpolator` type", "the `Resolver` trait", "the `Drop` impl"). Replace
  with what the user writes / observes.
- Concurrency / memory-model language (async, lazy, zeroize-on-drop, mlock,
  the runtime, the executor) — keep only if the user must act on it.
- Internal lifecycle / state-machine narration when the transitions are
  invisible to the user.
- Implementation history (ticket IDs, PR numbers, commit hashes, internal
  decision dates). Belongs in commits and ADRs, not in the manual.
- Build / binary metadata (size, link layout, dependency graph).

This is the place where leakage gets baked in -- if the manual just paraphrases
the spec, the spec's internal vocabulary becomes the manual's prose. The
sourcing pass is the chance to translate.

## Where to look, by software type

### CLI tools
- Argument parsers are the spec: `argparse`, `click`, `clap`, `cobra`, `getopt`
  definitions enumerate commands, flags, defaults, and help text.
- `--help`/`--version` output and man pages, if present.
- Config-file loading code reveals keys, defaults, and search paths.
- Exit-code constants and the error-handling paths.

### Libraries / APIs
- The **public** surface only: exported functions, classes, methods, types.
  Use the language's visibility rules (exports, `pub`, public headers, `__all__`).
- Signatures, parameter types, return types, and docstrings/doc comments.
- Errors/exceptions a public call can raise.
- Existing examples, tests, and README snippets — tests often show intended usage.

### GUI / desktop apps
- The view/screen definitions, menu and action tables, settings schema.
- Keyboard-shortcut/accelerator maps.
- User-facing strings and dialog definitions.

### Embedded devices
- Device tree, pin/connector definitions, and board documentation for the
  user-facing interface.
- User-exposed registers, properties, or sysfs/ioctl interfaces — not internal
  ones.
- Boot/operation procedures, default configuration values, indicator (LED/state)
  meanings, supported commands or protocols.
- Specs (datasheets, protocol specs) for the contract the user relies on.

## When a spec already exists in the doc set

If the software has an approved (or draft) spec under `docs/`, that spec is the
contract; the manual is the usage view of it. Prefer to **cross-reference** the
spec's requirement IDs and reference chapters (by doc ID / `<<xref>>`) over
re-deriving the same facts — it keeps the manual short and avoids drift. State
guarantees as "the device retries up to three times (see DEV-TOOLS-…, REQ-017)"
rather than restating the requirement. Use the code to confirm exact names,
defaults, and observable behavior, and to fill what the spec leaves implicit.

## Specifications vs. code

When both exist, the **specification states intent** (what should happen, the
contract, defaults, constraints) and the **code states reality** (what actually
happens). Prefer the spec for the contract the user can rely on; use the code to
fill gaps the spec leaves and to confirm defaults and exact names. When they
disagree on user-observable behavior, note the discrepancy rather than silently
picking one — it's usually a real bug or a stale doc, and the user should know.

## Provenance

For each fact, keep a lightweight note of its origin (file and symbol, or spec
section). This lets the drafting stage stay grounded and lets the user verify.
Provenance notes are working material — they don't appear in the final manual
unless the user wants source references.

## Flagging gaps — never invent

If a needed fact isn't in the sources, do not guess a value, a default, or a
behavior. Mark it inline in the draft so it's impossible to miss, e.g.:

```
[GAP: default value of --timeout not found in sources; confirm with maintainer]
```

Collect these so the user gets a clear list of what to fill in. A manual with
honest gaps is useful; a manual with confident fabrications is dangerous.

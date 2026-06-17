---
name: python-coding-rules
description: Use whenever writing, reviewing, or modernizing Python in this project — CLI tools, build/codegen helpers, the skills' own scripts (docs-librarian, tasklist), filter-design and device-tree tooling. Covers the project Python baseline: pinned interpreter floor, ruff for lint+format with warnings denied, full type hints checked by mypy --strict, typed errors over bare except, no mutable global state, pathlib over os.path, subprocess without shell=True, dataclasses for structured data, argparse CLIs with deterministic exit codes, and pytest. Apply any time Python is authored or judged, when setting up a project's lint, or when someone asks for Python standards — sibling of c23-coding-rules and rust-coding-rules. Rules use stable IDs PY-NN; treat it as a baseline to curate.
---

# python-coding-rules

The project's Python baseline — the sibling of `c23-coding-rules` and
`rust-coding-rules`, and the language `shell-coding-rules` SH-40 graduates to.
Rules are SHOULD unless marked MUST. Stable IDs `PY-NN`. Curate to house policy.

Python's defaults are permissive; these rules lean on `ruff` and `mypy --strict`
so most of the baseline is a tool setting, not a review note — same philosophy as
the Rust rules.

## Foundational — always on

- **PY-1** MUST pin a minimum interpreter version per project (e.g.
  `requires-python = ">=3.11"` in `pyproject.toml`) and test it in CI.
- **PY-2** MUST use `ruff` for both lint and format, warnings denied
  (`ruff check` + `ruff format --check` in CI). Formatting is not a review topic.
- **PY-3** MUST type-check with `mypy --strict` (or `pyright` strict). New code is
  fully annotated; `Any` is a deliberate, commented exception, not a default.
- **PY-4** MUST manage dependencies and metadata in `pyproject.toml`; pin or lock
  for applications. No loose `requirements.txt` drift, no `setup.py`.

Ruff rule sets worth enabling: `E,F,W` (pyflakes/pycodestyle), `I` (import sort),
`B` (bugbear), `UP` (pyupgrade), `SIM`, `PTH` (use pathlib), `RUF`, and `S`
(bandit security) on anything handling input or subprocesses.

## Types and data

- **PY-10** MUST annotate every function signature (parameters and return) in new
  code; `mypy --strict` enforces this (PY-3).
- **PY-11** SHOULD make illegal states unrepresentable: `enum.Enum` over magic
  strings, `@dataclass(frozen=True)` for structured records, `NewType` for IDs.
  (Same intent as RS-21 / C23 R40, in Python's idiom.)
- **PY-12** SHOULD prefer immutable data (`frozen=True` dataclasses, tuples) and
  pure functions; isolate I/O at the edges (software-design-rules D2).
- **PY-13** MUST NOT use a mutable default argument (`def f(x=[])`); use `None` and
  build inside. (Bugbear B006.) No mutable module-level globals (PY-30).

## Errors

- **PY-20** MUST NOT use a bare `except:` or `except Exception:` that swallows;
  catch the narrowest exception and handle or re-raise. (Ruff `BLE`, `E722`.)
- **PY-21** SHOULD raise specific, typed exceptions (a small project exception
  hierarchy) rather than `raise Exception("…")` or returning sentinel error codes.
- **PY-22** MUST chain or suppress deliberately: `raise New() from err` to keep the
  cause, or `from None` to hide it on purpose — never lose the traceback silently.
- **PY-23** SHOULD let exceptions propagate to a single top-level handler in a CLI
  that maps them to exit codes (PY-41), rather than printing-and-continuing deep
  in the call tree.

## Resources, subprocess, filesystem

- **PY-30** MUST NOT carry mutable global state; pass a context/config object or
  use a class. (no-globals, software-design-rules / RS-24.)
- **PY-31** MUST use context managers (`with`) for files, locks, sockets,
  subprocesses; never leak a handle.
- **PY-32** MUST call `subprocess.run([...], check=True)` with an **argument list**
  and **never `shell=True`** on untrusted or interpolated input (bandit S602/S603).
  Capture with `capture_output=True, text=True`. This mirrors the bserial
  exec-vs-shell rule: a list is the safe default.
- **PY-33** SHOULD use `pathlib.Path`, not `os.path` string juggling (ruff `PTH`).
- **PY-34** SHOULD inject the clock, RNG, and environment (parameters or a small
  protocol) so logic is testable and deterministic — no hidden `time.time()` /
  `random` in the core (mirrors RS-80).

## CLIs and structure

- **PY-40** SHOULD build CLIs with `argparse` (stdlib) — or a declared project
  choice — with a `main(argv: list[str]) -> int` entry point and
  `raise SystemExit(main())`.
- **PY-41** MUST give the CLI a deterministic exit-code contract: `0` success,
  documented non-zero per failure class (mirror the tool's taxonomy where one
  exists). `2` for usage errors, as argparse does.
- **PY-42** SHOULD keep modules cohesive and the public surface explicit: no
  `from x import *`; name what's exported via `__all__` where it matters
  (software-design-rules information hiding).
- **PY-43** SHOULD prefer the standard library; add a dependency only when it
  earns its maintenance and supply-chain cost.

## Comments, docs, process

- **PY-50** Public modules, classes, and functions get a docstring (see
  `code-comments`); document the *why*, not a restatement of the signature.
- **PY-51** `TODO`/`FIXME` carry a task ID — `# TODO(DEV-TOOLS-TASK-0042): …` — for
  `task-from-sources` (mirrors RS-92).

## Testing

- **PY-60** SHOULD test with `pytest`; unit-test the pure core, inject I/O
  (PY-34). Test conventions (output, determinism, CI) live in
  `test-writing-rules`.

## Enforcement map

- PY-2 ← `ruff check` + `ruff format --check`. PY-3 ← `mypy --strict`.
- PY-13/PY-20/PY-32/PY-33 ← ruff (`B`, `BLE`/`E722`, `S`, `PTH`). PY-1 ← CI matrix.
- The rest (PY-11/PY-12/PY-21/PY-41/PY-42) is review.
- Wire `ruff`, `mypy`, and `pytest` into CI / `run-all-checks.sh`. The skills' own
  Python scripts (`docs-librarian`, `tasklist`) are in scope for this baseline.

## What this skill does not cover

- Architecture, purity, no-globals across languages → `software-design-rules`.
- C / Rust → `c23-coding-rules`, `rust-coding-rules`. Shell (and when to graduate
  to Python) → `shell-coding-rules` SH-40.
- Comment content → `code-comments`. Commit hygiene → `git-commit`.
- Test conventions → `test-writing-rules`.
- Unfixable violations found in review → tasks via `task-from-sources`.

---
name: shell-coding-rules
description: Use whenever writing, reviewing, or hardening shell scripts — build, flash, network-setup, CI glue, enter-container, and run-all-checks scripts. Covers the project shell baseline: bash with a strict mode header, shellcheck with warnings denied, quoting and word-splitting discipline, error and exit-code handling, no parsing of ls, safe temp files and cleanup traps, functions over copy-paste, and when to stop and switch to Python. Apply this any time a .sh file is authored or judged, when setting up a script's lint, or when someone asks for shell standards — sibling of c23-coding-rules and rust-coding-rules. Rules use stable IDs SH-NN; treat it as a baseline to curate.
---

# shell-coding-rules

The project's shell baseline — the glue-language sibling of `c23-coding-rules` and
`rust-coding-rules`. Rules are SHOULD unless marked MUST. Stable IDs `SH-NN`.
Curate severities to house policy.

Shell is unsafe by default: unset variables expand to nothing, unquoted words
split, and a failing command in the middle of a pipeline is ignored. These rules
turn those defaults off and lean on `shellcheck` for the rest.

## Foundational — always on

- **SH-1** MUST start every script with a strict-mode header and a shebang:
  ```bash
  #!/usr/bin/env bash
  set -euo pipefail
  IFS=$'\n\t'
  ```
  `-e` exits on error, `-u` errors on unset variables, `-o pipefail` propagates
  pipeline failures. Set `IFS` to stop word-splitting on spaces.
- **SH-2** MUST target **bash**, not POSIX `sh`, unless a script must run before
  bash exists (initramfs, busybox). Declare the choice in the shebang; do not
  write bashisms under `#!/bin/sh`.
- **SH-3** MUST pass `shellcheck` with no suppressions other than per-line
  `# shellcheck disable=SCxxxx` comments that carry a reason. Run it in CI.
- **SH-4** SHOULD pass `shfmt -d` for formatting; formatting is not a review topic.

## Quoting and expansion

- **SH-10** MUST double-quote every expansion that can contain spaces or globs:
  `"$var"`, `"${arr[@]}"`, `"$(cmd)"`. Unquoted expansion is the single largest
  source of shell bugs (shellcheck SC2086).
- **SH-11** MUST use `"${var}"` brace form when the name is adjacent to other
  characters, and to make intent explicit.
- **SH-12** SHOULD use arrays for argument lists, never a space-joined string:
  `args=(-x foo --bar); cmd "${args[@]}"`. A string splits wrongly the moment a
  value contains a space.
- **SH-13** MUST NOT parse the output of `ls`; use globs or `find … -print0` with
  `read -d ''`. (shellcheck SC2012.)

## Errors, exits, and robustness

- **SH-20** MUST check that a command can fail where `set -e` does not catch it —
  inside `if`, `&&`/`||` chains, and command substitutions — and handle the
  failure explicitly.
- **SH-21** MUST give the script a deterministic exit-code contract: `0` success,
  non-zero documented per failure class. Mirror the tool's own taxonomy where one
  exists (e.g. a build wrapper returns the underlying tool's code).
- **SH-22** SHOULD send diagnostics to stderr and keep stdout for real output, so
  the script composes in a pipe: `echo "warn: …" >&2`.
- **SH-23** MUST create temp files/dirs with `mktemp` and remove them in an
  `EXIT` trap: `tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT`. Never hard-code
  `/tmp/foo`.
- **SH-24** SHOULD validate required inputs and environment up front, failing with
  a clear message, rather than half-running and leaving partial state.
- **SH-25** MUST quote and `--`-terminate paths passed to destructive commands
  (`rm -rf -- "$dir"`); guard against an empty variable deleting a parent.

## Structure

- **SH-30** SHOULD factor repeated logic into functions; a script copy-pasting a
  block three times wants a function (software-design-rules cohesion, in shell).
- **SH-31** SHOULD declare function-local variables with `local`; avoid leaking
  state between functions (no globals — software-design-rules / RS-24 in shell).
- **SH-32** SHOULD keep the top of the script declarative: shebang, strict mode,
  `usage()`, argument parsing, then `main "$@"`. Put `main` last and call it once.
- **SH-33** SHOULD prefer bash builtins and parameter expansion over spawning
  `sed`/`awk`/`cut` for trivial string work — but see SH-40.

## When to stop writing shell

- **SH-40** SHOULD switch to Python (`python-coding-rules`) when the script grows
  past roughly 100 lines, needs data structures, parses structured input
  (JSON/YAML/CSV), or needs real error handling and tests. Shell is glue; past
  glue it becomes unmaintainable. A script doing arithmetic, nested data, or
  multi-step parsing is already a Python program.

## TODO and process

- **SH-50** `TODO`/`FIXME` carry a task ID — `# TODO(DEV-TOOLS-TASK-0042): …` — so
  `task-from-sources` can harvest them (mirrors RS-92 / the C rule).

## Enforcement map

- SH-3 ← `shellcheck` (CI, `-S style` or stricter). SH-4 ← `shfmt -d`.
- SH-1/SH-10/SH-13/SH-20 ← shellcheck catches most (SC2086, SC2012, SC2155, …).
- SH-21 ← review + script tests. The rest is review.
- Wire `shellcheck` and `shfmt -d` into CI / `run-all-checks.sh`.

## What this skill does not cover

- Architecture, cohesion, no-globals across languages → `software-design-rules`.
- C / Rust → `c23-coding-rules`, `rust-coding-rules`. Python → `python-coding-rules`
  (the language to graduate to, SH-40).
- Comment content → `code-comments`. Commit hygiene → `git-commit`.
- The Dockerfile that ships these scripts → `dev-container`.
- Test conventions for script tests → `test-writing-rules`.

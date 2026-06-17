---
description: Survey existing code for refactoring opportunities and file them as prioritized tasks
argument-hint: "[path/module to scan, default: whole tree]"
allowed-tools: Bash(cargo clippy:*), Bash(git ls-files:*), Bash(rg:*), Bash(grep:*), Bash(find:*), Read, Grep, Glob
---

Survey existing code for **unmarked** technical debt and produce a prioritized
refactoring backlog. This is a maintenance sweep, not a change review.

## Scope
- Target: ${ARGUMENTS:-the whole source tree}
- Files in scope: !`git ls-files -- ${ARGUMENTS:-.} | grep -E '\.(rs|c|h|py|sh)$' | head -200`
- Mechanical complexity signals: !`cargo clippy --quiet --message-format=short 2>&1 | grep -iE 'complexity|too many|too long|this function has' | head -40 || echo "(cargo clippy unavailable or no Rust in scope)"`

## How to scan

This is **codebase-scoped**, not diff-scoped — survey already-merged code for
accumulated debt nobody marked. (Contrast `/code-review`, which gates a change;
and `task-from-sources`, which harvests debt that *was* marked — `TODO(...)`,
`[GAP:]`, open-issues. This finds the unmarked debt.)

Judge against the `software-design-rules` rubric (`D-NN`) plus the mechanical
signals above:

1. **Mechanical smells** (from clippy thresholds in `rust-coding-rules`
   `clippy.toml`): functions over the cognitive-complexity / too-many-lines /
   type-complexity thresholds. These are starting points, not the whole story.
2. **Structural smells** clippy can't see — read for these:
   - Cohesion/coupling: a module doing several unrelated things; a type reached
     into from everywhere (`software-design-rules` cohesion).
   - Layering: logic in the wrong layer — I/O in the functional core, a hardware
     poke outside the HAL (`software-design-rules` D2, `rust-coding-rules` RS-42).
   - Duplication: the same logic in 3+ places (the near-duplicate-table rule, in
     code) — candidate for one function.
   - Wrong/missing abstraction: primitive obsession where a newtype belongs
     (`RS-21`/`PY-11`); a boolean-flag soup that wants an enum.
   - Leaky public surface: `pub` that should be `pub(crate)` (`RS-60`); external
     types leaking through an API (`RS-63`).
   - Error handling shape: `unwrap`/`expect` clusters on runtime paths (`RS-10`)
     that signal a missing typed-error design.

## Output — a prioritized refactoring backlog

For each finding, write a `task-capture`-shaped entry, then order them with
`task-prioritization`:

- **Outcome-oriented title** — the end state, not the activity. "Extract transport
  retry into a single policy type", not "clean up session.rs".
- `kind: chore` (or `research` if the right shape is unclear).
- The `software-design-rules` / rule ID it addresses, and the file(s)/symbol.
- A one-line rationale: what is hard *now* because of this debt.
- A rough size (S/M/L) and the priority from `task-prioritization`.

Two hard rules:

- **Behavior-preserving.** A refactoring task changes structure, not behavior.
  Never bundle a behavior change into a refactor (`git-commit` one-logical-change).
  If a finding implies a behavior change, that is a *separate* code/bug/feature
  task, flagged as such.
- **File, don't fix.** This command produces tasks (`DEV-TOOLS-TASK-NNNN` via
  `task-tracker`); it does not edit code. Surgery is a later, separately-reviewed
  change.

Present the ranked backlog as a table (title, kind, rule, location, size,
priority, rationale). End by listing the `DEV-TOOLS-TASK-NNNN` entries to add to
`tasks/_tasks.yaml`, but do not modify code.

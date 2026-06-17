---
description: Prioritize the backlog and surface what to work on next
argument-hint: "[optional filter, e.g. kind=code or tag=fwu]"
allowed-tools: Bash(.claude/skills/task-tracker/scripts/tasklist:*), Bash(python3:*), Read
---

Apply `task-prioritization` to decide what to work on next. Filter: $ARGUMENTS

## Context
- Stats: !`python3 .claude/skills/task-tracker/scripts/tasklist stats 2>&1 || true`
- Actionable now: !`python3 .claude/skills/task-tracker/scripts/tasklist list --active 2>&1 || true`
- Unprioritised backlog: !`python3 .claude/skills/task-tracker/scripts/tasklist list --status Backlog 2>&1 || true`

## Task
1. Consider the actionable (Ready / In progress) and Backlog tasks above. Honour
   any filter in `$ARGUMENTS` (e.g. a `kind` or `tag`).
2. Apply `task-prioritization`: rank by the project's method (RICE or the house
   rule), accounting for `blocked-by` (a blocked task is not "next" until its
   blocker clears) and dependencies (a task that unblocks several others rises).
3. Recommend the **top 3–5** to work on next, each with: the
   `DEV-TOOLS-TASK-NNNN`, why it ranks where it does, and any blocker to clear
   first.
4. Flag any Backlog task that is actually ready (should move to `Ready`) or any
   `P0`/`P1` that is stalled.

If prioritisation changes a task's `priority`/`status`, propose the
`_tasks.yaml` edits and re-`render`; don't silently rewrite. This is advice +
proposed edits, not auto-reordering.

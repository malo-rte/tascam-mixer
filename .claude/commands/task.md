---
description: Capture a new task — allocate the ID, write it well, append, validate, render
argument-hint: "<what needs doing>"
allowed-tools: Bash(.claude/skills/task-tracker/scripts/tasklist:*), Bash(python3:*), Read, Edit
---

Capture a task following `task-capture` and `task-tracker`. Request: $ARGUMENTS

## Context
- Next ID: !`python3 .claude/skills/task-tracker/scripts/tasklist next-id 2>&1 || echo "(run from repo root with tasks/_tasks.yaml)"`

## Steps
1. **Write it well** (`task-capture`): an **outcome-oriented** title (the end
   state, not the activity), the `kind` (doc | code | infra | bug | feature |
   chore | research), and acceptance criteria in `notes`. If `$ARGUMENTS` is vague,
   sharpen it into a concrete outcome; ask one question only if you can't.
2. **Use the allocated ID** above (`DEV-TOOLS-TASK-NNNN`, `identifier-conventions`).
   Never hand-pick a number.
3. **Link it** (`links:`): the document (`DEV-TOOLS-…`), open-issue, file, or issue
   it concerns — this is what makes the list a combined worklist. Set `blocked-by`
   if it waits on another task (status then `Blocked`).
4. **Append** the entry to `tasks/_tasks.yaml` with the shape from `task-tracker`
   (`id`, `title`, `kind`, `status` — new tasks start `Backlog` unless clearly
   `Ready`; `created`/`updated` dates).
5. **Validate and render**: `tasklist validate` (must pass), then `tasklist render`
   to update `TASKS.adoc`.

Report the new `DEV-TOOLS-TASK-NNNN` and note that prioritization (`P0..P3`) is set
later via `/next` / `task-prioritization`. Commit `_tasks.yaml` + `TASKS.adoc`
together. Do not delete or renumber existing tasks.

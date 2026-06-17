---
description: Harvest marked debt (spec open-issues, GAP markers, code TODOs) into tasks
argument-hint: "[optional path to scope the scan, default: whole repo]"
allowed-tools: Bash(.claude/skills/task-tracker/scripts/tasklist:*), Bash(python3:*), Bash(git ls-files:*), Bash(rg:*), Bash(grep:*), Read, Edit
---

Apply `task-from-sources` to harvest **marked** debt into tasks. Scope:
${ARGUMENTS:-whole repo}

## Context
- Code TODO/FIXME: !`git ls-files -- ${ARGUMENTS:-.} | grep -E '\.(rs|c|h|py|sh)$' | xargs -r grep -nE 'TODO|FIXME' 2>/dev/null | head -60`
- GAP markers in docs: !`git ls-files -- ${ARGUMENTS:-.} | grep -E '\.adoc$' | xargs -r grep -nE '\[GAP:' 2>/dev/null | head -40`
- Spec open-issues: !`git ls-files -- ${ARGUMENTS:-.} | grep -E '\.adoc$' | xargs -r grep -nE 'OI-[0-9]' 2>/dev/null | head -40`
- Existing tasks (to dedupe): !`python3 .claude/skills/task-tracker/scripts/tasklist list 2>&1 | head -60 || true`

## Task
Run the `task-from-sources` harvest over the markers above:

1. For each marker, decide if it warrants a task. A bare `TODO` with no substance
   is noise; a `TODO(DEV-TOOLS-TASK-NNNN)` that already cites a task is **already
   tracked** — skip it. Harvest the ones describing real, unfiled work.
2. **Dedupe** against existing tasks (listed above) — don't file a second task for
   something already on the list; link instead.
3. Write each as a `task-capture`-shaped entry: outcome-oriented title, `kind`,
   `source` set to its provenance (`code-todo` / `spec-open-issue` / `gap-marker`),
   and `links` pointing at the file/`OI-NNN`/document it came from.
4. Allocate IDs with `tasklist next-id`, append to `tasks/_tasks.yaml`, then
   `tasklist validate` + `tasklist render`.

This harvests **marked** debt; for **unmarked** structural debt run
`/refactor-scan` (the complement). Report the new `DEV-TOOLS-TASK-NNNN` filed and
any markers skipped (already-tracked / too vague). Don't modify the source files.

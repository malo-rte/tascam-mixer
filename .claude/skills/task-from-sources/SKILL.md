---
name: task-from-sources
description: Use whenever generating tasks from existing artifacts rather than from scratch — harvesting work items from spec open-issues, GAP markers left by the user-manual or sourcing skills, unchecked items in a spec-review-checklist, code TODO/FIXME/XXX comments, failing tests, or audit findings from docs-librarian and tasklist. Produces well-formed tasks linked back to their origin, deduplicated against the existing list. Apply this when someone says "turn the open issues into tasks", "what work do the specs imply", "scan the repo for TODOs", or after a review/audit that surfaced gaps. Pairs with task-tracker (where tasks land), task-capture (how to shape them), and the doc-family skills that produce the markers.
---

# task-from-sources

Most work is already implied by artifacts the project produced — the gaps are
written down, just not as tasks. This skill harvests those into the combined task
list, each linked back to where it came from, so nothing tracked elsewhere falls
through. It ties the doc family and the task family together.

## Sources to scan

- **Spec open-issues** — `((OI-NNN))` entries in spec documents
  (`spec-document-template`'s open-issues section). Each unresolved OI is a task.
- **`[GAP: …]` markers** — left by `user-manual`'s sourcing pass and anywhere a
  fact couldn't be confirmed. Each gap is a task to find/confirm the fact.
- **Unchecked review items** — `[ ]` items from a `spec-review-checklist` run that
  weren't satisfied.
- **Code markers** — `TODO`, `FIXME`, `XXX`, `HACK` comments in source.
- **Failing tests / audit findings** — `tasklist validate`, `docs-librarian
  audit`, or test output reporting something that must be fixed.

## Workflow

1. **Collect** the markers. For text artifacts, grep the relevant trees, e.g.:
   ```bash
   grep -rnoE '\(\(OI-[0-9]+\)\)' docs/                 # spec open-issues
   grep -rn '\[GAP:' docs/ manuals/                      # unconfirmed facts
   grep -rnE '\b(TODO|FIXME|XXX|HACK)\b' src/            # code markers
   ```
   Capture the file, line, and surrounding context for each hit.

2. **Convert** each marker into a task using `task-capture`'s rules: an outcome
   title (not a paste of the comment), the right `kind` (an OI is usually `doc` or
   a decision; a FIXME is usually `code` or `bug`; a GAP is `doc`/`research`), and
   at least one acceptance criterion (often "the marker is removed and its
   resolution recorded in <origin>").

3. **Link back to the origin** and set `source`:
   ```yaml
   links:  ["docs/design/DES-0010-fwu/spec.adoc OI-002", DEV-TOOLS-DES-0010]
   source: spec-open-issue        # or: gap-marker | review-item | code-todo | audit-finding
   ```
   The link is what lets the task close the loop: resolving the task means going
   back and removing the marker.

4. **Deduplicate** against the existing list before adding. Check `_tasks.yaml`
   for an existing task with the same `links` target or an obvious title match;
   link/merge rather than create a second one. Re-running this skill should not
   multiply tasks for markers already captured.

5. **Allocate IDs and add**: `tasklist next-id` per new task, append, then
   `tasklist validate` and `tasklist render`. Leave them in `Backlog` with empty
   priority unless they're obviously actionable — let `task-prioritization` place
   them.

## Closing the loop

A task derived from a marker isn't truly done until the marker is gone: resolve an
OI by moving its answer into the spec and deleting the `((OI-…))` entry; resolve a
GAP by filling the fact and removing `[GAP:…]`; resolve a TODO by removing the
comment. State that removal in the task's acceptance criteria so "done" and "the
artifact is clean" mean the same thing.

## Output

A list of proposed new tasks (title, kind, link, source) for review before they're
written, plus a note of any markers skipped as duplicates. After confirmation, the
appended entries and a re-rendered `TASKS.adoc`.

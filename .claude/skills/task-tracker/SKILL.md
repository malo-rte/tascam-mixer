---
name: task-tracker
description: Use whenever a task or work item is added, updated, completed, dropped, or queried in the project's combined task list, when allocating a task ID, or when rendering/auditing the task list. Maintains tasks/_tasks.yaml (the source of truth) and its rendered tasks/TASKS.adoc via the bundled `tasklist` tool. This is the single combined worklist for the whole project — documentation work AND software development (code, infra, bugs, features). Apply this skill any time the shape or status of the task set changes, or when someone asks "what's on the list", "what should I do next", "add a task", "mark this done", or similar. Pairs with task-capture (writing a good task), task-prioritization (ordering), and task-from-sources (deriving tasks from specs and code).
---

# task-tracker

The combined task list lives in `tasks/_tasks.yaml` and is rendered to
`tasks/TASKS.adoc`. It is the project's single worklist: documentation tasks and
software-development tasks (code, infra, bugs, features, chores, research) share
one list so priorities can be compared across the whole project.

This skill is the task-side sibling of `docs-librarian`: `_tasks.yaml` is the
source of truth, `TASKS.adoc` is generated, and a small Python tool enforces the
invariants. Use it whenever the task set changes; don't hand-edit `TASKS.adoc`.

## Task entry shape

```yaml
- id:          DEV-TOOLS-TASK-0042                 # DEV-TOOLS-TASK-NNNN, allocated with `tasklist next-id`
  title:        Add NV counter read path to BL31   # outcome-oriented; see task-capture
  kind:         code                  # doc | code | infra | bug | feature | chore | research
  status:       Ready                 # Backlog | Ready | In progress | Blocked | Done | Dropped
  priority:     P1                    # P0..P3, set by task-prioritization (empty while in Backlog)
  method:       RICE                  # optional: how priority was derived
  score:        12.5                  # optional: numeric score from that method
  owner:        MK
  created:      2026-06-05
  updated:      2026-06-05
  estimate:     M                     # optional: S | M | L (or points)
  links:                              # cross-refs: doc IDs, open-issues, files, issues
    - DEV-TOOLS-DES-0010                # a document in the doc set
    - "docs/design/DES-0010-fwu/spec.adoc OI-002"   # a spec open-issue
  blocked-by:   [DEV-TOOLS-TASK-0041]              # task IDs this waits on
  blocks:       [DEV-TOOLS-TASK-0050]              # task IDs waiting on this
  tags:         [fwu, security]       # free-form, lowercase
  source:       spec-open-issue       # optional provenance (set by task-from-sources)
  notes: |
    Acceptance criteria and context.
```

`title`, `kind`, `status`, `id` are required; the rest are filled as the task
matures. `links` is what makes the list *combined* — a task points at the
document (`DEV-TOOLS-…`), open-issue, file, or issue it concerns.

## States

| Status      | Meaning                                            |
|-------------|----------------------------------------------------|
| Backlog     | Captured, not yet triaged/refined; may be unprioritised |
| Ready       | Triaged, prioritised, actionable now               |
| In progress | Being worked                                       |
| Blocked     | Waiting on something — requires a non-empty `blocked-by` |
| Done        | Completed; retained for history                    |
| Dropped     | Won't do; retained for history                     |

Done and Dropped tasks are never deleted — they stay for traceability, like a
Retired document in `docs-librarian`.

## Invariants (enforced by `tasklist validate`)

T01 ID shape `DEV-TOOLS-TASK-NNNN`. T02 status in the allowed set. T03 kind in the allowed
set. T04 priority empty or `P0..P3`. T05 Blocked requires `blocked-by`. T06
`blocked-by`/`blocks` reference existing tasks and are symmetric. T07 file sorted
by ascending sequence. T08 no duplicate IDs. T09 Done/Dropped keep id+title. T10
`links` entries shaped like a doc ID are well-formed `DEV-TOOLS-…` IDs.

## The tool

`.claude/skills/task-tracker/scripts/tasklist` (Python, PyYAML). Run from anywhere
in the repo:

```bash
tasklist next-id                       # next free DEV-TOOLS-TASK-NNNN
tasklist validate                      # CI hook; non-zero on any invariant failure
tasklist render                        # regenerate tasks/TASKS.adoc
tasklist list --active --priority P0   # filter: --status --kind --priority --owner --tag --active
tasklist stats                         # counts by status / kind / priority
```

## Workflows

**Add a task**: `tasklist next-id` → append an entry (use `task-capture` to write
it well) → `tasklist validate` → `tasklist render` → commit `_tasks.yaml` and
`TASKS.adoc` together.

**Update status**: edit the entry's `status` (and `updated`), set `blocked-by`
when moving to Blocked, then validate + render. Don't delete Done/Dropped tasks.

**"What should I do next?"**: `tasklist list --active`. If priorities are missing
or stale, run `task-prioritization` first, then list by priority.

**Audit**: `tasklist validate` before each release; resolve every finding.

## Rendering

`tasks/TASKS.adoc` is generated: active tasks grouped by priority (P0→P3, then
unprioritised), then a closed (Done/Dropped) section. It uses the house AsciiDoc
header — see `asciidoc-conventions`. Regenerate after every change; never edit it
by hand.

## What this skill does not cover

- How to phrase and size a task → `task-capture`
- How to order the list → `task-prioritization`
- Turning spec open-issues, `[GAP:…]` markers, and code TODOs into tasks → `task-from-sources`
- Documents (not tasks) → `docs-librarian`
- The task-ID grammar (`DEV-TOOLS-TASK-NNNN`) and how it relates to other IDs → `identifier-conventions`

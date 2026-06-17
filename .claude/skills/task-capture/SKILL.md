---
name: task-capture
description: Use whenever writing, refining, splitting, or triaging a task or work item before it goes on the list — turning a vague intention ("we should fix the boot flakiness") into a well-formed task with an outcome title, acceptance criteria, a size, and the right links. Apply this when capturing a new task, grooming the backlog, deciding whether something is one task or several, or triaging incoming items into the right kind and initial status. Pairs with task-tracker (where the task is stored) and task-prioritization (how it is later ordered).
---

# task-capture

How to turn an intention into a task worth tracking. A good task is a unit of
work with a clear finished state; a bad one is a vague topic that never closes.
This skill governs the *content* of a task; `task-tracker` governs where it lives.

## A well-formed task

- **Outcome title.** State the result, not the activity. "BL31 exposes an NV
  counter read path" beats "work on NV counters". The title should let you tick a
  box when it's true.
- **Acceptance criteria.** One to a few checks that make "done" objective. Put
  them in `notes`. If you can't write a check, the task is too vague — refine it.
- **Right kind.** `doc | code | infra | bug | feature | chore | research`. The
  kind drives how it's reviewed and where its output lands.
- **Links.** Connect it to its origin and its target: the document it serves
  (`DEV-TOOLS-…`), the spec open-issue, the file, the issue. A combined list is
  only useful if items point at what they touch.
- **Size.** A rough `estimate` (S/M/L). Anything that can't be finished in a
  sensible sitting is a candidate for splitting.

## Triage workflow (incoming → list)

1. **Classify** the kind. If it's a question, not work, it may belong in a spec's
   open-issues instead (see `spec-document-template`).
2. **Write the outcome title** and at least one acceptance criterion.
3. **Link** it to its origin/target.
4. **Set initial status**: `Backlog` if it still needs refinement or sequencing;
   `Ready` only if it's actionable now with criteria and links in place.
5. **Leave priority empty** in Backlog; `task-prioritization` sets it when the
   item is considered against the rest of the list.

## One task or several? (splitting)

Split when any of these is true:

- The title needs an "and" to be accurate ("write the manual *and* the spec").
- It mixes kinds (a `doc` and a `code` change) — split along the kind boundary so
  each piece reviews cleanly.
- It's too big to size as S/M/L, or its acceptance criteria form a checklist of
  independently-shippable parts.
- Parts have different blockers or owners.

When splitting, keep the parent as a brief umbrella only if it adds tracking
value; otherwise replace it with the children and link them with `blocks` /
`blocked-by` where there's a real ordering.

## Avoid

- **Activity titles** ("investigate", "look into", "work on") with no finish
  line — convert to an outcome, or make it an explicit `research` task whose
  acceptance criterion is "a decision recorded in <where>".
- **Backlog soup** — items with no criteria, no links, no owner. Either refine on
  capture or schedule a grooming pass; don't let them accumulate.
- **Duplicates** — before adding, `tasklist list --tag …` or grep `_tasks.yaml`
  for the same target; link or merge instead of duplicating.

## Output

A complete task entry per `task-tracker`'s shape, ready to append. Don't allocate
the ID yourself — use `tasklist next-id`. After adding, validate and render.

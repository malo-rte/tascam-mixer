---
name: task-prioritization
description: Use whenever ordering, ranking, or re-prioritising tasks — answering "what should I do next", assigning P0..P3, comparing items across documentation and software work, or producing a prioritised plan with rationale. Provides several frameworks (Eisenhower, MoSCoW, RICE, WSJF, value/effort) with guidance on when each fits, and produces an ordered list with explicit scores and reasoning written back to the tasks, not a silent re-sort. Apply this when the backlog needs sequencing, when priorities are missing or stale, or when someone asks how to decide what to work on. Pairs with task-tracker (where priority is stored) and task-capture (what is being ranked).
---

# task-prioritization

Decide the order of work and record *why*. The deliverable is never a bare sorted
list — it's an ordering with the method named and the reasoning visible, written
back into each task's `priority`, `method`, and `score`. That way a priority can
be questioned and re-derived later instead of being a mystery number.

This skill ranks the combined list, so it compares documentation work against
software work on the same axes. Priority lives in `task-tracker`; the methods here
fill it in.

## Pick the framework to fit the decision

Don't default to one method. Match it to what's actually driving the choice.

- **Eisenhower (urgent × important)** — fast triage of a mixed inbox into
  do-now / schedule / delegate / drop. Good for a first sweep when things feel
  chaotic. Cheap, coarse.
- **MoSCoW (Must / Should / Could / Won't)** — scoping a release or milestone:
  what's in, what's out. Good when there's a deadline and a fixed target.
- **RICE (Reach × Impact × Confidence ÷ Effort)** — comparing features/improvements
  competing for the same time, where you can estimate reach and effort. Produces a
  comparable score; good for a backlog of "would be nice" items.
- **WSJF (Cost of Delay ÷ Job Size)** — sequencing when *delay* has a cost
  (a dependency others wait on, a security fix, a release gate). Surfaces
  small-but-urgent work that RICE can bury. Strong fit for an embedded project
  with blocking dependencies.
- **Value vs Effort (2×2)** — a quick visual when you have a handful of items and
  want the quick wins vs big bets split. Good for a sprint-sized set.

When unsure: Eisenhower or value/effort for a fast pass; RICE or WSJF when you
need a defensible ordering across many items.

## Scoring guidance

- **RICE**: Reach = how many users/runs/devices affected per period; Impact on a
  fixed scale (e.g. 3 massive / 2 high / 1 medium / 0.5 low / 0.25 minimal);
  Confidence as a percentage; Effort in person-time. `score = R·I·C / E`. Higher
  is sooner.
- **WSJF**: Cost of Delay ≈ user/business value + time-criticality +
  risk-reduction/opportunity, each on a relative scale; Job Size ≈ effort.
  `score = CoD / Size`. Highest first.
- Keep scales consistent across the whole list in one pass, or the scores aren't
  comparable. Score relatively, not absolutely — you're ranking, not measuring.

## Mapping scores to P0..P3

After scoring, bucket into the tracker's priorities:

- **P0** — must happen now: blocks others, a release gate, a security/safety
  issue, or a hard deadline this cycle.
- **P1** — important and scheduled for the current cycle.
- **P2** — wanted, not yet scheduled.
- **P3** — someday/maybe; revisit at the next grooming.

Blockers inherit urgency: if a P0 task is `blocked-by` another, that blocker is at
least P0 too. Reconcile this when ranking.

## Workflow

1. Pull the candidates: `tasklist list --active` (and `--status Backlog` items
   ready for refinement).
2. Choose the framework for the decision at hand; state which and why.
3. Score the set in one consistent pass.
4. Write back `priority`, and `method`/`score` where a numeric method was used.
5. Reconcile blockers (a blocker is ≥ the priority of what it blocks).
6. `tasklist validate` then `tasklist render`; present the ordered list with the
   one-line rationale per item.

## Output format

A ranked list, highest priority first, each line: `T-ID — title — P? — score
(method) — one-line why`. Then the updated tasks. Always show the reasoning; a
priority with no rationale is the thing this skill exists to prevent.

## Re-prioritisation

Priorities go stale. Re-run at a regular cadence (e.g. weekly grooming, or before
each release) and whenever a P0 lands or a major dependency resolves. Re-derive
rather than nudge — restate the method and rescore, so the list stays honest.

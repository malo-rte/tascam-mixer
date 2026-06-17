---
name: architecture-decision-record
description: Use whenever capturing, writing, reviewing, superseding, or revisiting an architecture decision — an ADR. Apply when a structural choice is made and needs a durable record of its context, the decision, and its consequences; when an architecture-spec or design-spec references a decision that has no record yet; when a past decision is being reversed (write a new superseding ADR, never edit the old one); or when someone asks "why did we do it this way?". Owns the ADR document genre (ADR-class, single-file, immutable-once-accepted). Peers with the spec type skills; the architecture-spec skill summarises ADRs inline and links to them.
---

# architecture-decision-record

How to write an **ADR**: a short, immutable record of one architecture decision —
its context, the choice, and what the choice costs. ADRs are the `ADR` class in
`docs-librarian` (single-file, per L10). The `architecture-spec` skill *summarises*
a decision and *links* the ADR; the ADR is the full record of *why*.

An ADR captures a decision at the moment it was made and **freezes it**. You do
not edit an accepted ADR to reflect a later change — you write a new one that
supersedes it. The trail of superseded ADRs is the project's design memory.

## What an ADR is for

- One decision per ADR. If the title needs an "and", it is two ADRs.
- It records a choice that is **costly to reverse** or **non-obvious**: a
  structural split, a protocol or format, a dependency, a trust boundary, a
  concurrency model. Routine choices with an obvious answer do not need one.
- It is the answer to "why is it this way?" for a reader who arrives years later
  with none of the context. The decision is often less valuable than the
  *context* and the *rejected alternatives*.

## Required structure

A single `.adoc` file. Short — one to two pages. Sections, in order:

1. **Title** — `ADR-NNNN: <decision in a noun phrase>` (e.g.
   "ADR-0027: One session owns several named connections").
2. **Status** — one of `Proposed` / `Accepted` / `Superseded` / `Deprecated`.
   A `Superseded` ADR names its replacement; a superseding ADR names what it
   replaces. (See "Lifecycle" — this is the only field that changes after
   acceptance.)
3. **Context** — the forces in play: the problem, the constraints, the
   requirements pulling in different directions. Written so the decision reads as
   *forced by* the context, not arbitrary. No solution here.
4. **Decision** — the choice, stated in one or two sentences, active voice:
   "We will …". Then the essential mechanics — only enough to make the decision
   concrete, not a design spec.
5. **Alternatives considered** — the options *not* taken, each with one line on
   why it lost. This is the highest-value section and the one most often skipped;
   it stops the decision being relitigated and explains the rejections to the
   future reader who will be tempted by exactly those alternatives.
6. **Consequences** — what becomes easier and what becomes harder. Both. An ADR
   with only upsides is not honest. Include the new constraints the rest of the
   system must now live with.

Optional: **References** (requirement IDs the decision serves, related ADRs,
external sources).

## Metadata block

ADRs use a lighter front matter than specs, but the same doc-ID machinery:

```adoc
= ADR-0027: One session owns several named connections
:doc-id:      DEV-TOOLS-ADR-0027
:doc-status:  Accepted
:doc-date:    2026-06-06
:doc-authors: M. Karlsson
:supersedes:  null
:superseded-by: null
```

Allocate the ID with `docs-librarian next-id ADR`; register and place per
`docs-librarian` (ADR class, single file, see L10). Status maps to the index
`status` field.

## Lifecycle

- **Proposed → Accepted.** An ADR is `Proposed` while under discussion and
  becomes `Accepted` when the decision is taken. Both states are normal in the
  repo.
- **Accepted is immutable.** Once accepted, do not change Context, Decision,
  Alternatives, or Consequences. Fix a typo, nothing more. The record must reflect
  what was known and chosen *then*.
- **Reversing a decision = a new ADR.** Write `DEV-TOOLS-ADR-NNNN` (new) with
  `supersedes: DEV-TOOLS-ADR-MMMM`; set the old one's status to `Superseded` and
  `superseded-by: DEV-TOOLS-ADR-NNNN`, and add a one-line pointer at its top. Do not delete
  the old ADR — the supersession trail is the design history. This mirrors the
  `docs-librarian` supersede workflow.
- **Deprecated** is for a decision no longer in force but not replaced by a
  specific successor.

## Relationship to the spec set

- An **architecture-spec** states a structural fact and its rationale in one
  paragraph and links the ADR for the full context and the rejected alternatives
  (see `architecture-spec`: "summarise the ADR inline, then link it"). The spec is
  the current structure; the ADR is the dated decision behind it.
- A **requirement** an ADR serves is referenced by ID, not restated.
- When an ADR is superseded, the architecture-spec is updated to the new
  structure (specs track current state); the ADRs keep the history. Specs are
  mutable and current; ADRs are immutable and dated.

## Prose

- Apply `prose-precision` and `spec-writing-style`'s voice rules: active voice,
  named actors, no empty verbs or hedged intent. "We will let one session own
  several connections" — not "the architecture aims to support a flexible
  multi-connection capability".
- Non-normative: an ADR records a decision, it does not levy obligations, so no
  RFC 2119 keywords (those live in the requirement spec it references).
- No project planning (CC-5): an ADR has no schedule, owner-assignment, or
  phase. *When* it was decided is the date; *who decided* is not part of the
  record beyond the author field.

## Review checklist

- [ ] Exactly one decision; the title is a noun phrase, not "and"-joined
- [ ] Context makes the decision feel forced, and contains no solution
- [ ] Decision is one or two active-voice sentences
- [ ] Alternatives considered is present and non-empty, each with why it lost
- [ ] Consequences lists costs as well as benefits
- [ ] Status is valid; supersession pointers are bidirectional (per `docs-librarian`)
- [ ] No restated requirements (referenced by ID) and no RFC 2119 keywords
- [ ] Accepted ADRs are unmodified except status — reversals are new ADRs

## What this skill does not cover

- The current structure the decision produced → `architecture-spec`
- The internal mechanism realising it → `design-spec`
- The obligations it serves → `requirement-spec`
- ID allocation, placement, supersession bookkeeping → `docs-librarian`
- Prose and rendering → `prose-precision`, `spec-writing-style`, `asciidoc-conventions`

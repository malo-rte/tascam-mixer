---
name: architecture-spec
description: Use when writing, restructuring, or reviewing a software architecture specification (an ARCH-class document) — one that defines a system's major parts, how they relate, the interfaces between them, the cross-cutting invariants, and the rationale for the structural choices. Apply whenever a DEV-TOOLS-ARCH-NNNN document is created or revised, and whenever an existing architecture document is drifting into requirements, internal design, or usage material. Owns the architecture content class; peers with requirement-spec, design-spec, and user-manual.
---

# architecture-spec

How to write an **architecture** specification: the document that answers *what
are the major parts, how do they relate, and why is the structure shaped this
way?* Owns the architecture content class from `doc-content-class`. Reuses the
shared spine — `spec-document-template` (skeleton), `spec-writing-style` (prose),
`asciidoc-conventions` (rendering), `docs-librarian` (placement/ID) — and adds the
rules specific to this type.

Architecture is the only document whose primary content is **structure and
structural rationale**. Everything else is somebody else's document.

## The one test

Before writing any block, apply the shape test (`CC-4`):

> If this changed, would the system's **shape** change, or just a detail behind a
> stable interface?

Shape → it belongs here. Detail behind an interface → it is Design. A crate name,
a flag table, an exit-code string, a byte-level algorithm — all fail the test.
Keep the *contract* the structure exposes; send the realisation to Design.

## Belongs here

- **Decomposition** — the subsystems/components, each with a one-paragraph
  statement of its responsibility and what it owns.
- **Inter-part interfaces** — the *contract* between subsystems: what crosses the
  boundary, the guarantees, the ownership. Not the exhaustive message/field tables
  (those are a Design or interface-reference chapter — cross-reference them).
- **Cross-cutting invariants and models** — the ideas that constrain every part
  (e.g. a two-stream raw/normalized model, a session-owns-connections ownership
  model). State the invariant and the parts it binds.
- **Structural rationale** — *why this decomposition*, alternatives considered,
  the consequence of the choice. This is architecture's home turf (CC-3); be
  generous here.
- **Structural and behavioural views** — diagrams: component, sequence, state at
  the subsystem level. Each labelled, captioned, introduced in prose
  (`spec-writing-style`), with source in version control.
- **Decision records summarised inline.** When the architecture rests on an ADR,
  give a one-paragraph summary of *the decision and its structural consequence*
  here, then link the ADR for the full record. Do not cite an ADR repeatedly
  without ever stating what it decided.

## Does not belong here — relocate (CC-2)

Stable IDs `AC-NN`. Each is a relocation, not a deletion: move the content to the
named document and leave a cross-reference.

| ID | Reject | Relocate to |
|----|--------|-------------|
| **AC-1** | Exhaustive parameter / flag / field / exit-code / config-key tables | Design reference chapter, or User Manual. Architecture names the contract and cross-refs the table. |
| **AC-2** | Concrete library, crate, type, trait, or method names (`tokio-serial`, `VecDeque`, `String::from_utf8_lossy`) | Design. State the *contract property* the choice satisfies, not the choice: "ANSI stripping is parser-based and handles escape sequences spanning chunk boundaries statefully" — **not** "via the `vte` crate". |
| **AC-3** | Step-by-step runtime behaviour, byte-level semantics, exact timeout/escalation sequences | Design (behavioural detail). Architecture states *that* the behaviour is defined and *where*. |
| **AC-4** | How to invoke, configure, or read output; CLI syntax; config-file syntax | User Manual. |
| **AC-5** | Obligations phrased as `SHALL` requirements with REQ IDs | Requirement spec. Architecture *references* the requirement ID; it does not restate or originate the obligation. |
| **AC-6** | Project-phase labels in headings or names ("MVP Providers", "Phase 2", "v2 scope") | Nowhere — strip them (CC-5). Title by **capability** ("Provider types"); carry lifecycle in a per-feature status field, not the heading. |
| **AC-7** | Schedules, ownership, sprint scope, "to be decided" design questions outside Open Issues | Project planning artefacts (backlog, release notes), never the spec (CC-5). |

If removing a block per the table above would lose a *structural* fact, you have
mixed two altitudes in one block — split it (CC-1): keep the structural fact,
relocate the detail.

## Status, not planning

Architecture legitimately states what is and is not yet true of the
implementation — that scopes the contract's authority. Do it with a status field,
never with inline "(deferred)" scattered through prose, and never as a plan:

```adoc
[cols="2,1,4"]
|===
| Capability | Status | Notes

| Per-connection phase detection | implemented |
| Session-global variables       | implemented |
| `disconnect` action            | deferred    | Specified; not in the current build.
|===
```

`status` ∈ {`implemented`, `deferred`, `planned`}. Collect every `deferred`/
`planned` row into one **Implementation Status** matrix near the end so the spec
and the build can be diffed at a glance, rather than burying status in twelve
different sections. Schedules and ownership stay out (CC-5, AC-7).

## Skeleton (architecture variant)

Fills the `spec-document-template` skeleton; the type-specific chapters are:

1. Front matter, Introduction, Scope, Normative references, Terms (per template)
2. **Overview** — the decomposition in one page; the reader's map of the parts
3. **Context** — the system's boundary: external actors, neighbouring systems,
   what is inside vs outside the architecture
4. **Subsystem chapters** — one per major part: responsibility, owned state, the
   interfaces it offers and consumes (contracts, cross-referenced to Design)
5. **Cross-cutting concerns** — invariants and models that span subsystems
6. **Key decisions** — the structural choices and their rationale (ADR summaries)
7. **Implementation Status** — the single deferred/planned matrix (see above)
8. Open issues (empty for Approved), Appendixes, Glossary (per template)

Reference chapters in an architecture spec are thin: they document the *inter-part
contracts only*. The exhaustive per-surface reference tables (full APIs, formats,
CLI) live in Design or an interface spec — see `spec-document-template`'s
content-vs-reference split, and cross-reference rather than inline.

## Review additions

On top of `spec-review-checklist`, an architecture spec review runs the
content-class pass:

- [ ] Every chapter passes the shape test (CC-4); content failing it is flagged
      MAJOR with its relocation target (AC-1…AC-7)
- [ ] No concrete technology names where a contract property would do (AC-2)
- [ ] No exhaustive reference tables that belong in Design/Manual (AC-1)
- [ ] No project-phase labels in headings; lifecycle is in the status matrix (AC-6)
- [ ] Every referenced ADR is summarised inline at least once (decision + consequence)
- [ ] Every structural decision states its rationale (CC-3)
- [ ] No `SHALL` obligations originated here; requirements are referenced by ID (AC-5)

## What this skill does not cover

- Which document a piece of content belongs in → `doc-content-class`
- Obligations and requirement phrasing → `requirement-spec`, `spec-writing-style`
- Internal mechanism and exhaustive reference detail → `design-spec`
- Usage → `user-manual`
- Skeleton, prose, rendering, placement → `spec-document-template`,
  `spec-writing-style`, `asciidoc-conventions`, `docs-librarian`

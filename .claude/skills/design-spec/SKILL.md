---
name: design-spec
description: Use when writing, restructuring, or reviewing a design specification (a DES-class document) — one that defines how each part works internally: algorithms, data structures, exact behaviour and state machines, concrete technology choices, and the exhaustive reference detail (full APIs, formats, protocols, config schemas, CLI surfaces) for the interfaces the architecture defines. Apply whenever a DEV-TOOLS-DES-NNNN document is created or revised, and whenever a design document starts re-deciding structure or omitting the exhaustive detail that is its whole job. Owns the design content class; peers with requirement-spec, architecture-spec, and user-manual.
---

# design-spec

How to write a **design** specification: the document that answers *how does each
part work internally?* Owns the design content class from `doc-content-class`.
Reuses the shared spine — `spec-document-template`, `spec-writing-style`,
`asciidoc-conventions`, `docs-librarian` — and adds the rules specific to this
type.

Design **refines** architecture. It realises the contracts the architecture spec
defines; it does not re-decide the structure. Most of a system's *detail* lives
here — and it is where the exhaustive reference chapters belong.

## The one test

Before writing any block, apply the shape test (`CC-4`) from the other side:

> Is this a detail **behind a stable interface** — something I could change
> without changing the system's shape?

Yes → it belongs here. If changing it would change the decomposition or an
inter-part contract, that is an architecture change (DC-2) — stop and go edit the
architecture spec instead.

## Belongs here

- **Per-module internal design** — for each component the architecture defines,
  how it works inside: the algorithms, the data structures, the internal state.
- **Concrete technology choices, with justification** — the actual crates,
  libraries, types, and patterns, and *why* this one (mechanism rationale, CC-3).
  This is where `tokio-serial`, `VecDeque`, `vte` belong, in full.
- **Exact behaviour and state machines** — byte-level semantics, ordering,
  timeout and escalation sequences, eviction policy, error/failure handling at
  the mechanism level. Deterministic, no approximation.
- **Reference chapters** — the exhaustive, tabular specification of every
  interface the design exposes: every API function, every format byte, every
  protocol message, every config key, every CLI flag and exit code. The
  per-surface templates and the content-vs-reference rules are owned by
  `spec-document-template` and `spec-writing-style` — follow them; this is their
  primary home. (A large interface may instead get its own interface spec; then
  the design references it.)

## Does not belong here — relocate (CC-2)

Stable IDs `DC-NN`. Each is a relocation, not a deletion.

| ID | Reject | Relocate to |
|----|--------|-------------|
| **DC-1** | `SHALL` obligations originated here | Requirement spec. Design *references* requirement IDs and shows how the mechanism satisfies them. |
| **DC-2** | Re-deciding the decomposition, module boundaries, or an inter-part contract | Architecture. If the design needs a structural change, change the architecture spec; do not fork the structure here. |
| **DC-3** | Structural rationale ("why two streams at all") | Architecture (CC-3). Design carries *mechanism* rationale ("why FIFO eviction at line boundaries"). |
| **DC-4** | How an end user invokes or configures the system, in user vocabulary | User Manual. The design's reference chapter defines the CLI/config *surface* exhaustively; the manual teaches *using* it. |
| **DC-5** | Schedules, ownership, sprint scope | Project planning (CC-5). Deferred mechanism is marked status, not planned. |

## Relationship to architecture

This is the pairing that keeps both documents honest:

- The architecture spec names a contract and says *what* crosses a boundary.
- The design spec says *how* the contract is met, exhaustively, and cross-refs the
  architecture decision it realises (`xref:` by doc ID).
- A fact about an inter-part contract is stated once: in architecture as the
  contract, in design as the realisation — not duplicated (CC-2).

If you cannot point at the architecture decision a design chapter refines, either
the architecture spec is missing that decision (add it there) or the chapter is
actually doing architecture (DC-2).

## Skeleton (design variant)

Fills the `spec-document-template` skeleton; the type-specific chapters are:

1. Front matter, Introduction, Scope, Normative references, Terms (per template)
2. **Overview** — the modules to be designed and the architecture decisions they
   realise (one xref per module)
3. **Module design chapters** — one per component: internal data structures,
   algorithms, state machines, behaviour, error handling, technology choices +
   mechanism rationale
4. **Reference chapters** — the exhaustive per-surface specification (APIs,
   formats, protocols, config, CLI, exit codes). The bulk of most design specs.
5. **Implementation notes** — non-normative guidance not part of the contract
6. Open issues (empty for Approved), Appendixes, Glossary (per template)

## Review additions

On top of `spec-review-checklist`, a design spec review runs the content-class
pass:

- [ ] No `SHALL` obligations originated here; requirements referenced by ID (DC-1)
- [ ] No structural re-decisions; structure is referenced from architecture (DC-2)
- [ ] Every module chapter cross-references the architecture decision it realises
- [ ] Reference chapters are exhaustive for their surface (`spec-review-checklist`
      §8a) — a partial reference chapter is MAJOR
- [ ] Mechanism rationale present; structural rationale relocated (DC-3, CC-3)
- [ ] No end-user how-to in user vocabulary (DC-4)
- [ ] Concrete technology choices stated and justified (this is the right place)

## What this skill does not cover

- Which document a piece of content belongs in → `doc-content-class`
- Reference-chapter per-surface templates and content-vs-reference prose →
  `spec-document-template`, `spec-writing-style`
- Obligations → `requirement-spec`
- Structure and structural rationale → `architecture-spec`
- Usage → `user-manual`
- Rendering, placement → `asciidoc-conventions`, `docs-librarian`

---
name: requirement-spec
description: Use when writing, restructuring, or reviewing a requirements specification (a REQ-class document) — one that states what the system must do and why, as verifiable obligations with stable IDs and upward traceability. Apply whenever a DEV-TOOLS-REQ-NNNN document is created or revised, and whenever a requirements document starts specifying solutions (structure, mechanism, or usage) instead of needs. Owns the requirement content class; peers with architecture-spec, design-spec, and user-manual.
---

# requirement-spec

How to write a **requirements** specification: the document that answers *what
must be true, and why is it needed?* Owns the requirement content class from
`doc-content-class`. Reuses the shared spine — `spec-document-template`,
`spec-writing-style`, `asciidoc-conventions`, `docs-librarian` — and adds the
rules specific to this type.

A requirement states an **obligation**, not a solution. The whole skill of a
requirements document is to say *what* without saying *how*.

## The one test

Before writing any block, ask:

> Could a conformance test pass or fail against this, without prescribing how the
> system is built?

If it is not testable, it is a wish, not a requirement (`spec-writing-style`
testability rules). If it prescribes a mechanism, it is design wearing a SHALL.

## Belongs here

- **Requirement groups** — chapters organised by capability area (`FWU`, `BOOT`,
  `NET`…), each grouping related obligations.
- **Individual requirements** — one obligation per ID'd `SHALL` statement. ID
  format, RFC 2119 discipline, and testable phrasing are owned by
  `spec-writing-style`; follow it.
- **The need / rationale** — *why* each group of obligations exists. The need,
  not the solution (CC-3). A short rationale paragraph per group, non-normative.
- **Upward traceability** — what each requirement traces to: a higher-level
  system spec, a regulation, a compliance control, a customer requirement.
- **Verification approach** — how conformance is demonstrated (test, analysis,
  inspection, demonstration), and the observable success/failure criteria.
- **Constraints** — externally imposed limits the system must satisfy (standards,
  platform, regulatory) stated as obligations, not as chosen designs.

## Does not belong here — relocate (CC-2)

Stable IDs `RC-NN`. Each is a relocation, not a deletion.

| ID | Reject | Relocate to |
|----|--------|-------------|
| **RC-1** | A prescribed mechanism, structure, or technology ("SHALL use a ring buffer", "SHALL be implemented in Rust", "SHALL store keys in a TPM") | Architecture (if structural) or Design (if mechanism). The requirement states the *observable need*: "SHALL retain at least the last N lines across a reconnect", "SHALL protect private keys against extraction by a non-root local process". |
| **RC-2** | Component decomposition, module boundaries, interface designs | Architecture. |
| **RC-3** | Algorithms, data structures, exact behaviour | Design. |
| **RC-4** | How a user invokes or configures the system | User Manual. |
| **RC-5** | "We will build X first, then Y" — phasing, schedules, ownership | Project planning, never the spec (CC-5). A deferred *requirement* is still stated; its status is a field, not a plan. |

The RC-1 trap is the common one and the reason requirements documents fail
review: a solution smuggled in as an obligation over-constrains the design and is
usually untestable as written. When you catch a "SHALL <mechanism>", ask "what
observable property does this mechanism deliver?" — that property is the
requirement; the mechanism relocates.

## Status, not planning

A requirement that is not yet satisfied by the implementation is still a stated
obligation. Carry that with a status/verification field per requirement
(`proposed`, `accepted`, `verified`), never as a delivery plan (CC-5). When a
verification trace exists, cite the test by ID.

## Skeleton (requirements variant)

Fills the `spec-document-template` skeleton; the type-specific chapters are:

1. Front matter, Introduction, Scope, Normative references, Terms (per template)
2. **Overview** — the requirement areas and how they map to the system's purpose
3. **Requirement chapters** — one per capability area; each opens with the area's
   *need* (rationale), then the ID'd `SHALL` obligations
4. **Verification** — the approach per area and the success/failure criteria
5. **Traceability matrix** (appendix) — each requirement ID ↔ its upstream source
   and (when present) its downstream test
6. Open issues (empty for Approved), Glossary (per template)

Requirements specs usually have **no reference chapters** — there is no
API/format/protocol *surface* being defined here, only obligations. If you find
yourself writing a field table, it is a design leak (RC-3).

## Review additions

On top of `spec-review-checklist`, a requirements spec review runs the
content-class pass:

- [ ] Every requirement is an obligation, not a mechanism (RC-1) — flag MAJOR
- [ ] No structure, algorithm, or usage content (RC-2…RC-4); relocate
- [ ] Every requirement is testable in one sentence (`spec-writing-style`)
- [ ] Every requirement has a stable ID; IDs are unique and never reused
- [ ] Every requirement traces upward to a named source
- [ ] Each group states its need (rationale), distinct from the obligations (CC-3)
- [ ] No phasing or scheduling (RC-5, CC-5)

## What this skill does not cover

- Which document a piece of content belongs in → `doc-content-class`
- Requirement ID format, RFC 2119, testable phrasing → `spec-writing-style`
- Structure and structural rationale → `architecture-spec`
- Internal mechanism → `design-spec`
- Usage → `user-manual`
- Skeleton, rendering, placement → `spec-document-template`,
  `asciidoc-conventions`, `docs-librarian`

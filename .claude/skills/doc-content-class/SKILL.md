---
name: doc-content-class
description: Use whenever deciding which document a piece of content belongs in, choosing which document type to write or split, or reviewing/auditing a document for content that belongs in a different document type. The shared content-routing layer beneath the four document-type skills (requirement-spec, architecture-spec, design-spec, user-manual). Apply this any time a document mixes obligations, structure, internals, and usage — or any time you catch yourself unsure whether something is "architecture" or "design".
---

# doc-content-class

The single rule for *what content goes in which document*. The four document-type
skills (`requirement-spec`, `architecture-spec`, `design-spec`, `user-manual`)
each own one content class; this skill is the boundary between them. Use with
`spec-document-template` (the skeleton each type fills) and `spec-review-checklist`
(which enforces the boundary at review time).

## The four document types

Each answers exactly one question. A document that answers two is two documents
braided together.

| Type | Class | Answers | Owns |
|------|-------|---------|------|
| Requirement spec | `REQ` | *What must be true, and why it is needed?* | Obligations, stated so they are verifiable; traceability upward |
| Architecture spec | `ARCH` | *What are the major parts, how do they relate, and why is the structure shaped this way?* | Decomposition, inter-part interfaces, cross-cutting invariants, structural rationale |
| Design spec | `DES` | *How does each part work internally?* | Algorithms, data structures, exact behaviour, concrete technology choices, exhaustive reference detail |
| User manual | `MAN` | *How does someone operate it?* | Commands, configuration, output, procedures |

## The routing table

For any block of content, find its row. The right-hand test settles it.

| If the content is… | It belongs in… | Test |
|--------------------|----------------|------|
| An obligation a test could fail against | Requirement | "Could a conformance test pass or fail on this?" |
| A component, an inter-part interface, or *why the structure is shaped this way* | Architecture | "Would changing this change the system's **shape**?" |
| An algorithm, data structure, exact state machine, byte-level behaviour, or a concrete library/type choice | Design | "Is this a detail **behind a stable interface**?" |
| How to invoke, configure, or read the output | User Manual | "Does the **user** do, type, or see this?" |

## Cross-cutting principles

Stable IDs `CC-NN`. Cited in `spec-review-checklist` findings.

- **CC-1 — One home.** Each piece of content has exactly one correct document
  type. If it seems to fit two, it is phrased at the wrong altitude; split it
  into the obligation (Requirement), the structural consequence (Architecture),
  the mechanism (Design), and the usage (Manual), each in its own document.

- **CC-2 — Relocate, never delete or duplicate.** Content in the wrong document
  is *moved* to the correct one and replaced with a cross-reference. Deleting it
  loses information; copying it creates two sources of truth that will drift.
  (See the DRY rule in `spec-writing-style` and §11 of `spec-review-checklist`.)

- **CC-3 — Rationale lives with the decision it explains.** Structural rationale
  ("why two streams", "why per-connection phase state") → Architecture.
  Mechanism rationale ("why a ring buffer, why this eviction order") → Design.
  Need/justification ("why this obligation exists") → Requirement. A *why* is a
  strong signal for which document the surrounding content belongs to — and
  Architecture is the only document whose primary job is structural *why*.

- **CC-4 — The shape test resolves Architecture vs Design.** Ask: *if this
  changed, would the system's shape change, or just a detail behind a stable
  interface?* Shape → Architecture. Detail behind an interface → Design. A crate
  name, an exit-code string, an exhaustive flag table, and a normalization
  algorithm all fail the shape test — they are Design or Manual, not Architecture.

- **CC-5 — Status is allowed; plans are not.** Stating that a feature is
  `implemented` / `deferred` / `planned` is a fact about the artifact and belongs
  wherever the feature is described, as a status field (see each type skill).
  Schedules, ownership, sprint scope, release-phase labels ("MVP", "Phase 2"),
  and unresolved *design questions* (outside a Draft's Open Issues) are project
  planning and belong in **none** of the four document types.

## Worked example

A serial-console tool needs its scrollback bounded. The single idea fans out
across all four documents — none of it is duplicated:

| Content | Document | Form |
|---------|----------|------|
| "The tool SHALL retain at least the last N lines of output across a reconnect." | Requirement | `REQ-BUF-003`, with rationale (debugging a boot needs prior output) |
| "Each connection owns its own scrollback; the buffer survives reconnect and inserts a marker at the boundary." | Architecture | Structural fact + the *why* (per-connection isolation) |
| "Two ring buffers (normalized, raw), dual line/byte bounds, FIFO eviction at line boundaries, `VecDeque`-backed." | Design | Algorithm + data structure + concrete choice |
| "Set `scrollback { normalized-max-lines 10000 }` in the profile." | User Manual | Config syntax the user types |

The bug to avoid: putting all four in the Architecture spec. "Uses a ring buffer"
fails the shape test (CC-4) → Design. The config key fails CC-4 → Manual. The
SHALL fails it the other way → Requirement. Architecture keeps only the
per-connection ownership decision and its rationale.

## Using this skill

1. Identify the document's declared type (its ID prefix / `:doc-status:` block).
2. For each block of content, run the routing table and the shape test.
3. Content that lands in a different type is a **CC-2 relocation**: move it,
   leave a cross-reference. Flag as MAJOR in review (see `spec-review-checklist`).
4. When a single feature spans types, write it in all the relevant documents,
   each at its own altitude, linked by doc ID.

## What this skill does not cover

- The skeleton each type fills → `spec-document-template`
- The specific rules and forbidden content per type → `requirement-spec`,
  `architecture-spec`, `design-spec`, `interface-spec`, `user-manual`,
  `architecture-decision-record` (one frozen decision), and the `threat-model`
  overlay for security chapters
- Prose, RFC 2119, requirement IDs → `spec-writing-style`
- Placement, ID allocation, the index → `docs-librarian`
- AsciiDoc rendering → `asciidoc-conventions`

---
name: spec-document-template
description: Use when starting a new technical specification, restructuring an existing one, or deciding which sections a spec needs. Defines the canonical document skeleton (front matter, intro/scope/overview, content, implementation notes, appendixes, references, glossary), document states (Draft → Under review → Approved → Superseded), revision history and open-issues conventions, and the distinction between intro, scope, and overview. Apply this skill whenever a new .adoc spec file is being created or an existing one is being reorganized.
---

# spec-document-template

The canonical shape of a specification document. Use together with `spec-writing-style` (how to write the prose) and `asciidoc-conventions` (how to render it).

## Document skeleton

In order, top to bottom:

1. **Front matter**
   - Title page (title, doc ID, version, status, date)
   - Document metadata (authors, reviewers, approvers)
   - Revision history
   - Table of contents
2. **Introduction** — why this document exists, audience, relationship to other documents
3. **Scope** — what is in scope, what is explicitly out of scope
4. **Normative references** — documents this spec depends on
5. **Terms and definitions** — terms used normatively in this document
6. **Overview** — high-level walkthrough of the content to follow
7. **Content chapters** — narrative substance: how it works, why, structural views, examples
8. **Reference chapters** — exhaustive, deterministic detail for every API, interface, data structure, file format, protocol, configuration schema, or CLI surface introduced in the content chapters. Mandatory whenever any of these are normatively specified.
9. **Implementation notes** — non-normative guidance for implementers
10. **Conformance** — what it means for an implementation to conform (when applicable)
11. **Open issues** — unresolved questions (must be empty for Approved state)
12. **Appendixes** — supporting material (examples, derivations, rationale)
13. **Bibliography** — non-normative references
14. **Glossary** — broader reference, including non-normative terms

Sections 8, 10 and 13 are optional only when the document genuinely has no API/interface/format/protocol surface and no conformance claim; everything else is mandatory.

## Distinguishing intro, scope, and overview

These three sections blur in practice. Keep them distinct:

- **Introduction**: *why* this document exists. Audience. Relationship to other documents. No technical content.
- **Scope**: *what* is in and out. A bulleted "In scope" list and an "Out of scope" list. No prose explanations of mechanism.
- **Overview**: *what is in the rest of this document*. A reader's map. One paragraph per major chapter to follow.

If a sentence belongs in two of these, it probably belongs in neither — move it to the relevant content chapter.

## Content vs reference chapters

Content and reference chapters serve different readers and follow different rules. Both can be present in the same spec; they are not alternatives.

**Content chapters** give understanding.

- Narrative prose, diagrams, worked examples.
- Explain *how it works* and *why*.
- May omit edge cases for clarity.
- Reader leaves with a mental model.

**Reference chapters** give every detail.

- Deterministic, tabular, exhaustive.
- Define *what it is*, with no ambiguity and no omissions.
- Every field, every error code, every byte, every state transition documented.
- Reader can implement or interoperate from this chapter alone.

Reference chapters are **mandatory** whenever the spec normatively defines any of:

- An API (functions, methods, RPCs)
- An interface (hardware, software, network)
- A data structure (in-memory, serialised, on-the-wire)
- A file format
- A protocol
- A configuration schema (YAML, KDL, TOML, env vars, kernel cmdline)
- A CLI surface (commands, flags, exit codes)
- A device-tree binding or similar declarative contract

Reference chapter templates by surface type:

| Surface          | Required entries per item                                                                                |
|------------------|----------------------------------------------------------------------------------------------------------|
| API function     | Signature, purpose, parameters (each), return value, errors, preconditions, postconditions, side effects, thread/reentrancy, since-version |
| Data structure   | Field name, type, units, range/constraints, semantics, optional/required, version compatibility           |
| File format      | Magic, version, endianness, every header field, every section, alignment/padding, checksum, examples (hex) |
| Protocol         | Every message type, fields, encoding, state machine, error handling, timing, versioning, security envelope |
| Config schema    | Every key, type, default, range/enum, semantics, interactions, deprecation                                |
| CLI              | Every subcommand, every flag, argument types, defaults, exit codes, environment variables                 |
| DT binding       | Compatible string, required/optional properties, value formats, child node rules, examples                |

Each reference chapter lives under a top-level heading: `== <Name> reference` (e.g. "FWU bundle format reference", "Update agent API reference").

The content chapter introducing a surface MUST cross-reference its reference chapter on first mention:

```
The bundle layout is summarised below; see <<sec-bundle-format-reference>> for the complete specification.
```

If a fact appears in both content and reference chapters, the reference chapter is canonical and the content chapter is a recap (see DRY rule in `spec-writing-style`).

## Front matter metadata block

Required fields at the top of every spec:

```
:doc-id:        DEV-TOOLS-DES-NNNN
:doc-title:     <title>
:doc-version:   0.3
:doc-status:    Draft
:doc-date:      2026-05-25
:doc-authors:   <names>
:doc-reviewers: <names>
:doc-approvers: <names>
```

See `asciidoc-conventions` for the AsciiDoc attribute syntax.

## Revision history

A table near the top, immediately after the metadata block:

| Version | Date       | Author | Change                          |
|---------|------------|--------|---------------------------------|
| 0.1     | 2026-04-12 | MK     | Initial draft                   |
| 0.2     | 2026-05-01 | MK     | Added FWU chapter               |
| 0.3     | 2026-05-25 | MK     | Resolved review comments R1–R7  |

Every non-trivial change adds a row. Trivial = typo fix without semantic shift.

**Version integrity (enforced):**

- The `:doc-version:` in the metadata block MUST equal the version in the latest
  revision-history row. A cover that says 0.24 while the history stops at 0.22 is
  a defect.
- Revision-history rows MUST be monotonically ordered by version (no row out of
  sequence, no gaps that hide a missing entry).
- These two checks are mechanical; wire them into the `docs-librarian` audit /
  `run-all-checks.sh` so a stale cover version fails CI rather than ships.

## Document states

A `Status` field in the metadata gates what kind of review and changes are allowed.

| State          | Meaning                                      | Versions  | Allowed changes              |
|----------------|----------------------------------------------|-----------|------------------------------|
| Draft          | Active writing, structure may shift          | 0.x       | Anything                     |
| Under review   | Content complete, awaiting reviewer sign-off | 0.x       | Review-driven edits only     |
| Approved       | Signed off, in force                         | ≥1.0      | Versioned change with impact |
| Superseded     | Replaced by a newer document                 | any       | None — pointer only          |

Versioning: `0.x` through Under review, `1.0` at first approval, then semver-style (`1.1` additive, `2.0` breaking).

## Open issues section

A short conventional section near the end, before appendixes:

```
== Open issues

. ((OI-001)) Should attestation use TPM2 PCR 11 or PCR 14?
. ((OI-002)) Confirm whether anti-rollback applies to the rescue slot.
```

- Add an entry every time you hit a question you cannot answer in-line.
- Resolve by moving the answer into the relevant chapter and deleting the entry.
- Approved state requires this section to be empty.

## Spec-type variations

This skeleton is type-agnostic — it is the *shape* every document type fills. The
question of **what content is allowed in each type** is owned by
`doc-content-class` and the four document-type skills, which are peers:

| Type | Class | Skill | Owns the content class |
|------|-------|-------|------------------------|
| Requirement spec | `REQ` | `requirement-spec` | Verifiable obligations + their need |
| Architecture spec | `ARCH` | `architecture-spec` | Decomposition, inter-part interfaces, structural rationale |
| Design spec | `DES` | `design-spec` | Internals, exact behaviour, exhaustive reference detail |
| Interface/protocol spec | `DES` | `interface-spec` | An interoperability contract (format, protocol, API, schema), specified exhaustively |
| User manual | `MAN` | `user-manual` | Usage: commands, configuration, procedures |
| Architecture decision | `ADR` | `architecture-decision-record` | One frozen decision: context, choice, consequences |

Each type skill defines its own chapter variant, its allowed and forbidden
content, and a content-class review pass. **Read the matching type skill and
`doc-content-class` before outlining** — they decide which of the chapters below
are present and where the content goes. A document that mixes the four content
classes (e.g. an architecture spec carrying exhaustive flag tables, crate names,
and config syntax) is several documents braided together; split it per
`doc-content-class` rule CC-1, relocating each part per CC-2.

Notes that apply across types:

- **Reference chapters** are the bulk of an Interface/Design spec, thin in an
  Architecture spec (inter-part contracts only), and absent from a Requirement
  spec. See "Content vs reference chapters" above and the owning type skill.
- **Interface vs design.** Use `interface-spec` when the subject is a *contract
  between parties* (wire format, protocol, API, schema); use `design-spec` when it
  is a *module's internals*. Both are `DES`-class.
- **Security overlay** — `threat-model` adds "Assets", "Trust boundaries",
  "Adversary model", and threat/mitigation chapters to whichever base type
  applies (or stands alone as a security spec). It is an overlay, not a fifth type;
  see `spec-review-checklist` §9.
- **Changelog / release notes** are *not* a spec — they record deltas and live in
  `release-notes` (CC-5 keeps them out of specs and manuals).
- **Test spec** — chapters track requirements one-to-one; add a traceability
  matrix appendix.

When in doubt, write the skeleton first (headings only), then check it against
the type skill and `doc-content-class`.

## Starting a new spec

1. Copy the template (location: `<repo>/specs/_template.adoc` if present, or scaffold from this skill).
2. Fill in the metadata block.
3. Write the Scope (in/out lists) before anything else — this disciplines the rest.
4. Stub every section with a one-line description.
5. Hand off to `spec-writing-style` for the content pass.

## What this skill does not cover

- Which content class each document type owns → `doc-content-class` and the type
  skills `requirement-spec`, `architecture-spec`, `design-spec`, `user-manual`
- Phrasing of individual requirements → `spec-writing-style`
- Review process and checklist → `spec-review-checklist`
- AsciiDoc syntax and rendering → `asciidoc-conventions`

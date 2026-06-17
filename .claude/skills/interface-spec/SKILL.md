---
name: interface-spec
description: Use when writing, restructuring, or reviewing an interface or protocol specification — a document whose subject is a contract between independently-built parties: a wire format, a binary or text protocol, an on-the-wire data structure, an API, a config schema, or a CLI surface, specified exhaustively enough that two parties can interoperate from the document alone. Apply for KDL/CBOR/CDDL/TLV formats, signed/encrypted message protocols, SMC/syscall APIs, and similar. A specialised design-class document (DES) where the reference chapter IS the document; peers with design-spec. Owns interoperability contracts.
---

# interface-spec

How to write an **interface or protocol** specification: a document whose whole
purpose is a **contract two independently-built parties interoperate against** — a
wire format, a protocol, an API, a schema. It is a specialised `DES`-class
document (a focused sibling of `design-spec`): where a design spec explains a
module's internals, an interface spec defines a boundary precisely enough that an
implementer on *either side* can build to it without reading the other side's
code.

The defining property: **the reference chapter is the document.** In most specs
the reference detail supports a narrative; here the narrative supports the
reference detail. Precision and completeness beat readability when they conflict —
an ambiguous byte sinks interoperability.

## When this skill applies

The subject is a contract between parties, not a module's behaviour:

- A **wire format** — KDL/TOML config schema, CBOR/CDDL document, TLV record,
  binary file format, on-the-wire struct.
- A **protocol** — a signed/encrypted message protocol, a request/response or
  streaming exchange, a handshake, a state machine across a link.
- An **API** — a library API, an SMC/syscall/RPC surface, an IPC boundary.
- A **CLI surface** consumed by scripts as a contract.

If the subject is "how this component works inside," that is `design-spec`. If it
is "what crosses this boundary and exactly how it is encoded," it is this.

## Belongs here

- **The model** (one short content chapter) — the parties, the boundary, the
  lifecycle, the trust assumptions. Enough mental model to read the reference; no
  more. Cross-references the `architecture-spec`/ADR that placed the boundary.
- **The exhaustive reference** (the bulk) — every element of the contract, using
  the per-surface templates from `spec-document-template` and the reference-prose
  rules from `spec-writing-style`. By surface type:
  - **Wire format**: magic, version, endianness, every field with offset/type/
    units/range, alignment and padding, length and framing rules, checksum,
    reserved/extension fields, and worked **hex examples**.
  - **Protocol**: every message type and its encoding, the framing, the full
    state machine, timing/timeout rules, error responses, versioning/negotiation,
    and the **security envelope** (what is signed, what is encrypted, with which
    key, in which order — cross-reference `threat-model`).
  - **API**: every function/call — signature, parameters, returns, errors,
    pre/postconditions, side effects, thread/reentrancy, since-version.
  - **Schema/CLI**: every key/flag — type, default, range/enum, semantics,
    interactions, deprecation, exit codes.
- **Conformance** — what it means to implement this interface correctly; the
  observable behaviour a conforming party must exhibit. Interface specs more than
  any other type earn a conformance section, because two teams build against it.
- **Versioning and compatibility** — how the contract evolves: version field
  semantics, what is additive vs breaking, negotiation, and the rule for unknown
  fields/messages (ignore vs reject). An interface without an evolution rule
  breaks the first time it changes.
- **Examples / test vectors** (appendix) — concrete encoded instances, byte-exact,
  that an implementer checks against. For protocols, full exchange traces.

## Does not belong here — relocate (CC-2)

- A party's **internal** algorithm or data structure → `design-spec`. The
  interface defines what crosses the boundary; how a side computes it is its own
  design.
- The **obligations** the interface satisfies (`SHALL …`) → `requirement-spec`;
  reference by ID.
- The **structural decision** to have this boundary → `architecture-spec` / ADR.
- **How an operator uses** a CLI/config interface in practice → `user-manual`
  (the interface spec defines the surface exhaustively; the manual teaches using
  it).
- The **security analysis** behind the envelope → `threat-model`; the interface
  spec states the envelope mechanically, the threat model justifies it.

## Reference-prose discipline (carried from spec-writing-style)

- Tabular and structured, not narrative. Every entry uses the same template shape
  — same field set, same order — so the document is scannable and gaps are
  visible.
- No approximation. No "typically", no "usually": if a value depends on context,
  enumerate every context. Ambiguity here is an interop bug.
- Every reserved field, every error path, every unknown-input rule is stated. A
  reference that documents the happy path only is worse than none — it implies a
  completeness it lacks (`spec-review-checklist` §8a, MAJOR).
- Examples are separated from definitions (own subsection/appendix), and are
  byte-exact.

## Skeleton

1. Front matter, Introduction, Scope, Normative references, Terms (per template)
2. **Model** — parties, boundary, lifecycle, trust (one short chapter)
3. **Reference** — the exhaustive contract (the bulk of the document)
4. **Conformance** — observable requirements for a conforming implementation
5. **Versioning & compatibility** — evolution rule, unknown-field handling
6. **Test vectors / examples** (appendix) — byte-exact instances and traces
7. Open issues (empty for Approved), Glossary (per template)

## Review additions

On top of `spec-review-checklist` (especially §8a reference completeness):

- [ ] Two independent parties could implement and interoperate from this document
      alone — no side requires reading the other's source
- [ ] Every field/message/call/key documented to the per-surface template; no gaps
- [ ] Endianness, alignment, framing, and length rules stated for binary formats
- [ ] Unknown-field / unknown-message rule stated (ignore vs reject)
- [ ] Versioning and negotiation defined; additive vs breaking is explicit
- [ ] Security envelope stated mechanically (what/which key/what order), threat
      model cross-referenced
- [ ] Byte-exact test vectors present
- [ ] No internal algorithm content that belongs in `design-spec`

## What this skill does not cover

- A module's internals (the general design case) → `design-spec`
- The obligations served → `requirement-spec`; the boundary's existence →
  `architecture-spec` / `architecture-decision-record`
- Operating a CLI/config interface → `user-manual`
- The security rationale for the envelope → `threat-model`
- Reference-template shapes, prose rules → `spec-document-template`, `spec-writing-style`
- Rendering, placement → `asciidoc-conventions`, `docs-librarian`

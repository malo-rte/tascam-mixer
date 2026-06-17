---
name: spec-review-checklist
description: Use whenever reviewing a technical specification before sign-off, self-checking a draft before sending it for review, or producing review notes. Provides a state-aware checklist (Draft vs Under review vs Approved change) covering structure, scope adherence, language and RFC 2119 discipline, requirements traceability and testability, fact accuracy, security properties, completeness (error paths and failure modes), consistency, ambiguity, references, glossary, and open issues. Apply this skill any time a spec is being evaluated, whether by author or reviewer.
---

# spec-review-checklist

What to check, in what depth, depending on the document's state. Use with `spec-document-template` (skeleton being checked) and `spec-writing-style` (style rules being checked against).

## Review depth by state

Pick the right scope; don't fact-check an outline.

| State        | What to review                                                              |
|--------------|-----------------------------------------------------------------------------|
| Draft        | Structure, scope, completeness of skeleton. Not language, not facts.        |
| Under review | Full checklist below.                                                       |
| Approved     | Change-impact only: what does this diff alter, does it break conformance?   |

A Draft review that flags weasel words is misdirected effort. A Under-review review that only checks structure is incomplete.

## The checklist

Run these in order. Each section is independent — finish one before starting the next.

### 1. Structure

- [ ] Front matter present and complete (doc ID, version, status, date, authors)
- [ ] Revision history up to date
- [ ] **`:doc-version:` equals the latest revision-history row** (no stale cover version)
- [ ] **Revision-history rows are monotonically ordered by version** (none out of sequence)
- [ ] Sections in canonical order (see `spec-document-template`)
- [ ] Introduction, Scope, and Overview are distinct and serve their distinct purposes
- [ ] No "miscellaneous" or "other" sections
- [ ] Appendixes contain only supporting material, not normative content

### 2a. Content-class adherence (per document type)

The document declares a type via its ID prefix (`REQ` / `ARCH` / `DES` / `MAN`).
Every block of content must belong to that type's content class. Run the routing
table and shape test from `doc-content-class` against each chapter.

- [ ] Every content block passes the routing test for this document's type
- [ ] Mis-routed content is flagged **MAJOR** with its relocation target, and the
      fix is *relocate + cross-reference* (CC-2), never delete or duplicate
- [ ] Type-specific forbidden content is absent — run the relocation table in the
      matching type skill:
  - Architecture → `architecture-spec` rules AC-1…AC-7 (no exhaustive tables,
    no crate/type names, no byte-level behaviour, no CLI/config syntax, no
    REQ obligations, no phase labels in headings)
  - Requirement → `requirement-spec` rules RC-1…RC-5 (no prescribed mechanism,
    structure, algorithm, or usage; obligation not solution)
  - Design → `design-spec` rules DC-1…DC-5 (no originated SHALLs, no structural
    re-decisions; cross-refs the architecture decision each module realises)
  - User manual → `user-manual` implementation-leak rules
- [ ] No project planning anywhere (schedules, ownership, phase labels); lifecycle
      is a status field, not prose or headings (CC-5)
- [ ] Rationale sits with the decision it explains: structural why in Architecture,
      mechanism why in Design, need in Requirement (CC-3)

A document that is internally well-formed but answers the wrong question for its
type is still wrong. This pass is the one that catches "four documents braided
into one".

### 2b. Scope adherence

- [ ] Scope section has explicit "In scope" and "Out of scope" lists
- [ ] Every content chapter falls within "In scope"
- [ ] Nothing in "Out of scope" is normatively specified anywhere in the document
- [ ] Cross-references to other documents replace, not duplicate, their content

### 3. Language and RFC 2119

- [ ] RFC 2119 boilerplate clause present in Terms section
- [ ] SHALL / MUST / SHOULD / MAY uppercase only when normative
- [ ] One of SHALL or MUST used consistently for normative requirements
- [ ] Active voice, present or SHALL-future tense
- [ ] No first person ("we", "our", "you")
- [ ] Sentence length sensible (≤ 25 words for requirements)
- [ ] One requirement per sentence
- [ ] **Prose is concrete (all prose, not only requirements):** no empty verbs, nominalizations, abstract subjects, hedged intent, uncounted quantifiers, ambiguous pronouns, or filler — apply `prose-precision` P-1…P-7

### 4. Weasel words and ambiguity

Search the document for:

- "fast", "efficient", "secure", "robust", "scalable", "modern", "user-friendly"
- "roughly", "typically", "usually", "approximately"
- "etc.", "and/or", "as appropriate", "where applicable"
- "should be able to", "may need to", "in case of failure"

Each hit is either quantified, replaced with a concrete clause, or moved to non-normative text.

### 5. Requirements

- [ ] Every normative requirement has a stable ID (`REQ-AREA-NNN`)
- [ ] IDs are unique and not reused
- [ ] Every requirement is testable in one sentence
- [ ] Trigger conditions are concrete and bounded
- [ ] Success and failure criteria are observable
- [ ] No nested conditionals (if/then/else cascades) — use tables or state machines

### 6. Traceability

- [ ] Requirements trace upward to higher-level documents (system spec, regulation, compliance control)
- [ ] Implementation notes that depend on a requirement cite the requirement ID
- [ ] Test references (if present) target requirement IDs

### 7. Consistency

- [ ] Terminology identical across the document — no synonyms for the same concept
- [ ] Every defined term is used; every term used in a special sense is defined
- [ ] Glossary entries match Terms-and-definitions entries (no contradictions)
- [ ] Component names match those in sibling specs in the same monorepo
- [ ] Acronyms expanded on first use, consistent thereafter

### 8. Completeness

For each requirement and each chapter, the following are addressed or explicitly out of scope:

- [ ] Error paths (what happens on failure?)
- [ ] Failure modes (timeouts, malformed input, hardware faults, partial state)
- [ ] Recovery and rollback (especially FWU, boot, key rotation)
- [ ] Concurrency (what happens if two things happen at once?)
- [ ] Resource bounds (memory, time, persistent storage)
- [ ] Initial state and bootstrap (what about the first run?)

If a failure mode is intentionally not handled, the spec says so.

### 8a. Reference chapter completeness

For every API, interface, data structure, file format, protocol, configuration schema, or CLI surface introduced in the content chapters:

- [ ] A corresponding reference chapter exists
- [ ] The content chapter cross-references the reference chapter on first mention
- [ ] Every entry in the reference chapter uses the same template shape (same field set, same order)
- [ ] No facts about the surface live only in the content chapter
- [ ] Reference chapter prose is structured (description lists, tables), not narrative

Per surface type, verify exhaustively:

- [ ] **API**: every function listed; for each — signature, parameters, return, errors, pre/postconditions, side effects, thread safety, since-version
- [ ] **Data structure**: every field listed with type, units, range, semantics, optionality, version
- [ ] **File format**: every byte accounted for — magic, version, endianness, headers, sections, alignment, checksum
- [ ] **Protocol**: every message type, every field, full state machine, error handling, timing, versioning, security envelope
- [ ] **Config schema**: every key with type, default, range/enum, semantics, interactions, deprecation
- [ ] **CLI**: every subcommand and flag with type, default, exit codes, environment variables
- [ ] **DT binding**: compatible string, all required and optional properties with value formats, child node rules

A reference chapter that documents some but not all entries is worse than no reference chapter — it implies completeness that isn't there. Flag as MAJOR.

### 9. Security

For all specs, not only security specs:

- [ ] Trust boundaries identified
- [ ] Assets protected named explicitly (key material, attestation log, etc.)
- [ ] Adversary model stated (or referenced)
- [ ] Assumptions about the platform and operator listed
- [ ] Key lifecycle (generation, storage, use, rotation, revocation) addressed for every key introduced
- [ ] Anti-rollback considered wherever versioned state exists
- [ ] No security through obscurity — every security claim rests on a named primitive or property
- [ ] Logging and audit obligations stated for security-relevant events

### 10. Fact-checking

- [ ] Cited standards reference the correct version (e.g. RFC number, IEC clause)
- [ ] Hardware references match datasheets
- [ ] Software references match the API/version actually shipped
- [ ] Numeric claims (timings, sizes, throughputs) have a stated source or measurement basis

### 11. No repetition

- [ ] Each fact stated once, in its canonical section
- [ ] Other occurrences are xrefs, not restatements
- [ ] **No two tables or blocks state the same facts.** Near-duplicate tables are
      a **MAJOR** finding even when one cites the other ("This duplicates X"):
      designate one canonical location, replace the other with an xref. Two copies
      drift. (Mechanical aid: flag any two tables sharing column headers and ≥50%
      of rows.)
- [ ] Allowed exception: explicit "Summary; see X" recaps in overview / implementation notes

### 12. References

- [ ] Normative references separated from bibliography
- [ ] Every reference resolvable (URL, ISBN, DOI, doc ID for internal)
- [ ] No reference unused; every external claim referenced
- [ ] Versions pinned where the standard has versions

### 13. Glossary and terms

- [ ] Terms-and-definitions complete (every italicised term on first use)
- [ ] Glossary covers domain terms a non-author reader would need
- [ ] No circular definitions
- [ ] Definitions are short and self-contained

### 14. Open issues

- [ ] Open issues section present
- [ ] Each open issue has an ID and is actionable
- [ ] **For Approved state: section must be empty**

### 15. Diagrams

- [ ] Each diagram has a figure ID, caption, and is introduced in prose
- [ ] Diagram source is in version control
- [ ] No diagram is the sole bearer of a normative claim — the prose says it too

### 16. Readability

After all the above:

- [ ] A new team member can read the spec front-to-back and explain what it specifies
- [ ] No chapter requires re-reading earlier chapters more than once
- [ ] Where complexity is unavoidable, the overview prepares the reader

## Output: review notes

Findings format:

```
[<severity>] <section> — <finding>
   Suggestion: <action>
```

- `MAJOR` — blocks approval (missing requirement, security gap, untestable normative clause)
- `MINOR` — should be fixed before approval (language, consistency)
- `NIT` — typography, formatting, optional improvement

Group findings by section, not by severity, so the author can address them in order.

## Self-review before sending

Before declaring a draft Under review, the author runs sections 1, 2, 3, 4, 14 of this checklist. The remaining sections are for the external reviewer. This saves a round trip.

## What this skill does not cover

- The skeleton being checked → `spec-document-template`
- Which content class each document type owns → `doc-content-class` and the type
  skills `requirement-spec`, `architecture-spec`, `design-spec`, `user-manual`
- The style being checked against → `spec-writing-style`
- Concrete-language constructs being checked (P-1…P-7) → `prose-precision`
- AsciiDoc rendering issues → `asciidoc-conventions`

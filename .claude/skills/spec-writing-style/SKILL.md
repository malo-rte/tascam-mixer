---
name: spec-writing-style
description: Use whenever drafting or editing the prose inside a technical specification — phrasing requirements, structuring sentences, picking terminology. Covers RFC 2119 keyword discipline (shall/should/may/must/must not), requirement ID format, weasel word elimination, testable phrasing, voice and tense rules, the pass-based writing workflow (skeleton → stub → content → consistency → review), and the no-repetition rule. Apply this skill any time you are writing or revising spec content, not just at review time.
---

# spec-writing-style

How to write the prose inside a spec so it is reviewable, testable, and implementable. Use with `spec-document-template` (skeleton), `spec-review-checklist` (review), and `prose-precision` (concrete language). This skill owns vague *adjectives* and approximations (the weasel-word table below) plus RFC 2119, IDs, and sentence shape; `prose-precision` owns vague *verbs, nouns, subjects, and constructions*. Apply both on every prose pass.

## Pass-based writing

Resist polishing chapter 1 before chapter 7 exists. Write in passes:

1. **Skeleton** — headings only. Validate structure against `spec-document-template`.
2. **Stub** — one sentence per section saying what it will contain. Scope check.
3. **Content** — fill in. Don't optimise prose yet.
4. **Consistency** — terminology aligned with glossary, cross-refs resolved, no contradictions.
5. **Review** — apply `spec-review-checklist`.

Each pass touches the whole document before the next begins.

## RFC 2119 keyword discipline

Use the keywords below only with these meanings. Uppercase when used normatively, so they stand out.

| Keyword       | Meaning                                                  |
|---------------|----------------------------------------------------------|
| MUST / SHALL  | Absolute requirement                                     |
| MUST NOT      | Absolute prohibition                                     |
| SHOULD        | Recommended; deviation requires justification            |
| SHOULD NOT    | Discouraged; deviation requires justification            |
| MAY           | Optional, fully permitted                                |

Pick one of MUST or SHALL and use it consistently throughout a document. DEV-TOOLS convention: **SHALL** for normative requirements, **MUST** for invariants and security properties.

Outside requirements (intro, overview, implementation notes, rationale), use lowercase ordinary English. Reserve uppercase keywords for normative statements.

Include this clause in every spec's Terms section:

> The key words SHALL, SHOULD, MAY, MUST, MUST NOT, and their negatives are to be interpreted as described in IETF RFC 2119 when, and only when, they appear in uppercase.

## Requirement IDs

Every normative requirement gets a stable ID. DEV-TOOLS recognises two forms:

**Canonical form (default for new documents):**

```
[[REQ-<AREA>-<NNN>]]
The system SHALL <verifiable behaviour>.
```

- `<AREA>` is a short uppercase tag matching the chapter (e.g. `FWU`, `BOOT`, `NET`, `ATT`).
- `<NNN>` is a zero-padded sequence number within the area.
- IDs never change once assigned. Deleted requirements leave the ID retired, not reused.
- Renumbering breaks traceability — don't do it.

Example:

```
[[REQ-FWU-014]]
The update agent SHALL reject a bundle whose signature does not verify
against the active ROTPK.
```

**Legacy form (allowed only for documents mirroring an external baseline):**

```
[[<AREA>-<NNN>]]
The system SHALL <verifiable behaviour>.
```

- Used when the document inherits its requirement IDs from a frozen external baseline (e.g. a CRA SRS, an IEC profile, a customer's contractually-supplied requirements set) and rewriting the IDs would break external traceability with the original author or auditor.
- `<AREA>` and `<NNN>` follow the same shape rules as the canonical form; the only difference is the absent `REQ-` prefix.
- The legacy form MUST be declared in the document's front matter via `:legacy-doc-id:` (see `docs/README.adoc`) so the audit can tell a legacy doc from a non-conformant one.
- Documents in legacy form remain valid indefinitely; they are not a "to be migrated" state. The form change would silently invalidate every external citation against the original baseline.

Example (from REQ-0001, mirroring SRS-CRA-EL-001):

```
[[SC-001]]
The manufacturer SHALL maintain a Software Bill of Materials (SBOM)
for every shipped device covering all components in the rootfs and
firmware tree.
```

Mixing the two forms within a single document is not permitted -- pick one per document.

## Testability rules

Every requirement must be verifiable. A requirement is testable when:

- The behaviour is observable from outside the component, or measurable via a stated probe
- The trigger condition is concrete (no "in case of failure" without saying which failure)
- The success criterion is unambiguous

If you cannot describe the test in one sentence, the requirement is too vague.

**Not testable**: "The system shall be secure against tampering."
**Testable**: "On detecting a hash mismatch in the active slot, the boot loader SHALL halt before transferring control to the kernel."

## Weasel words

Replace, do not retain.

| Weasel        | Replace with                              |
|---------------|-------------------------------------------|
| fast          | "within N ms" / "at least N ops/s"        |
| efficient     | quantified resource bound                 |
| secure        | named property (confidentiality, etc.)    |
| robust        | named failure modes and required response |
| user-friendly | named acceptance criterion                |
| modern        | named version / standard                  |
| roughly       | a number, or remove                       |
| typically     | a number, or move to implementation notes |
| etc.          | enumerate or remove                       |
| and/or        | pick one, or rephrase                     |

If you genuinely cannot quantify, move the sentence out of the normative chapter and into implementation notes or rationale.

The weasel table catches vague *adjectives and approximations*. Vague *verbs*
("handles", "supports"), *nouns* (nominalizations), *subjects* ("the system"),
hedged intent ("aims to"), uncounted quantifiers ("several"), and filler are
caught by `prose-precision` (P-1…P-7). Run both passes — they apply to the same
sentence without overlapping.

## Voice, tense, and person

- **Present tense** for descriptions ("The agent verifies the signature.").
- **Future-tense SHALL** for requirements ("The agent SHALL verify the signature.").
- **Active voice** by default. Passive is allowed only when the actor is genuinely irrelevant or unknown.
- **Third person**. No "we", "you", "our".
- **Subject must be a defined component**, not "the software", "the system" (unless "the system" is the defined SUT).

## DRY — no repeated explanations

State each fact once, in the most specific section where it belongs. Elsewhere, cross-reference:

```
See <<sec-fwu-bundle-format>> for the bundle layout.
```

If the same explanation appears in two places, one of them is wrong. Pick the canonical location and replace the other with an xref.

Exception: a short recap in an overview or implementation note is acceptable if it explicitly says "Summary; see X for details."

## Content vs reference prose

The two chapter types from `spec-document-template` require different prose.

**Content chapter prose**

- Narrative, paragraph-form, with diagrams and worked examples.
- Establishes the reader's mental model.
- May elide edge cases ("typical flow shown; see reference for full state machine").
- Forward-references the reference chapter on first mention of any defined surface.

**Reference chapter prose**

- Tabular and structured, not narrative.
- Every item follows the same template (see the per-surface tables in `spec-document-template`).
- No prose paragraphs except a one-paragraph chapter intro stating what the chapter exhaustively defines.
- No examples mixed with definitions; put examples in a dedicated subsection or appendix.
- No "typically", "usually", or any approximation. If a value depends on context, enumerate every context.
- Use AsciiDoc description lists, definition tables, and source listings; avoid free prose.

Reference entry template (API function example):

```adoc
[#ref-fwu-apply]
=== `fwu_apply`

Purpose:: Apply a verified bundle to the inactive slot.
Signature:: `int fwu_apply(const bundle_t *b, fwu_opts_t opts);`
Parameters::
  `b`::: Bundle handle returned by `fwu_open`. MUST NOT be NULL.
  `opts`::: Bitmask of `FWU_OPT_*` flags.
Returns:: `0` on success, negative `errno`-style code on failure.
Errors::
  `-EPERM`::: Signature verification failed. See <<REQ-FWU-014>>.
  `-EIO`::: Write to inactive slot failed. Slot left invalid.
  `-EBUSY`::: An apply operation is already in progress.
Preconditions:: `fwu_open` returned successfully; bundle not yet applied.
Postconditions on success:: Inactive slot contains the new image; commit pending.
Side effects:: Audit event `EVT_FWU_APPLY` emitted to <<sec-audit>>.
Thread safety:: Single-threaded. Concurrent calls SHALL return `-EBUSY`.
Since:: 1.0
----
```

Apply the same template shape — same field set, same order — to every entry in a reference chapter. Consistency across entries is the whole point of reference prose.

The DRY rule still applies: a fact about a surface is stated in its reference entry, not in the content chapter. The content chapter may recap; the reference chapter is canonical.

## Defined terms

- Every term used in a special sense MUST appear in "Terms and definitions".
- Every term in "Terms and definitions" MUST be used at least once in the document body.
- On first use in the body, the term appears in italics: `_active slot_`.
- Abbreviations: introduce as "Full Name (FN)" on first use, then "FN" thereafter.

## Numbered vs unnumbered lists

- Numbered list when order matters or you need to reference items (steps, ordered priorities).
- Bulleted list otherwise.
- A list with one item is a sentence — rewrite it.

## Sentence shape

- One requirement per sentence. If two SHALLs appear in one sentence, split it.
- Aim for ≤ 25 words per requirement sentence. Above that, split.
- No conditional cascades. Replace "If X, then if Y, ..." with a table or a state diagram.

## Diagrams

A diagram replaces prose only if labelled and referenced. Every diagram has:

- A figure ID and caption.
- A sentence in the prose that introduces it ("Figure 3 shows the update state machine.").
- Source in version control alongside the document (PlantUML or Mermaid preferred; see `asciidoc-conventions`).

A diagram that needs explanation longer than the diagram itself is the wrong diagram.

## Examples

Bad:
```
The system should be reasonably fast and handle errors gracefully.
```

Good:
```
[[REQ-PERF-003]]
The update agent SHALL complete signature verification of a 32 MiB bundle
within 2 s on the reference platform (see <<sec-platform-reference>>).

[[REQ-FWU-021]]
On signature verification failure, the update agent SHALL log the event
to the audit channel (<<sec-audit>>) and SHALL NOT advance the update
state machine.
```

## What this skill does not cover

- Where these requirements live structurally → `spec-document-template`
- Concrete-language rules — empty verbs, nominalizations, abstract subjects, hedges, filler → `prose-precision`
- The project-wide ID grammar (requirement IDs are document-scoped items) → `identifier-conventions`
- How to render IDs, xrefs, callouts → `asciidoc-conventions`
- Whether a finished spec is good → `spec-review-checklist`

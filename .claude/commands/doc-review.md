---
description: Review a document for structure, facts, language, and consistency
argument-hint: "[doc path or ID, default: docs changed vs origin/main]"
allowed-tools: Bash(git diff:*), Bash(git status:*), Read, Grep, Glob, WebSearch, WebFetch
---

Apply the `spec-review-checklist` skill to a document (or documents).

## Target
- Argument: ${ARGUMENTS:-"(none — review the docs changed vs origin/main)"}
- Changed docs (if no argument): !`git diff --name-only origin/main...HEAD -- 'docs/**' 2>/dev/null || echo "(no git base; specify a path)"`

If an argument is given, review that document. Otherwise review the changed docs
above. Read the target with `Read`; for consistency checks, also read its
glossary, the sibling specs it references, and `docs/_index.yaml`.

## Set the depth first
Identify the document's **type** (from its ID class — `REQ`/`ARCH`/`DES`/`MAN`/
`ADR`/interface) and **state** (`:doc-status:` — Draft / Under review / Approved),
then scope the review per `spec-review-checklist`'s depth-by-state: a Draft gets
structure/scope/completeness only; Under review gets the full checklist; Approved
gets change-impact only. Don't fact-check an outline.

## Run the review, grouped by dimension

**1. Structural review** — `spec-review-checklist` §1, §2, §2a.
   - Canonical section order and front matter present; intro/scope/overview
     distinct (`spec-document-template`).
   - Version integrity: `:doc-version:` equals the latest revision-history row;
     rows monotonically ordered.
   - **Content-class / altitude** (§2a): every block belongs to this document's
     type. Apply the owning type skill's rules — `architecture-spec` AC-1…AC-7,
     `requirement-spec` RC-1…RC-5, `design-spec` DC-1…DC-5, `interface-spec`,
     `user-manual`. Mis-routed content is **MAJOR**, fixed by relocate +
     cross-reference (`doc-content-class` CC-2), never delete/duplicate.

**2. Fact checking** — §10.
   - Every claim that needs a source has one; numeric claims (timings, sizes)
     state a basis.
   - Cited standards reference the correct version (RFC number, IEC clause);
     hardware refs match datasheets; software/API refs match what ships. Use
     `WebFetch`/`WebSearch` to verify an *external* citation where checkable —
     don't go on tangents; flag what you can't verify rather than guessing.
   - Cross-check internal claims against the document's own reference chapters and
     sibling specs for contradictions.

**3. Language checking** — §3, §4, plus `prose-precision`.
   - RFC 2119 discipline (uppercase only when normative; one of SHALL/MUST used
     consistently); active voice; defined-component subjects (`spec-writing-style`).
   - Weasel words (§4): vague adjectives/approximations quantified or moved.
   - `prose-precision` P-1…P-7 on **all** prose: empty verbs, nominalizations,
     abstract subjects, hedged intent, vague quantifiers, ambiguous pronouns,
     filler.
   - Markup hygiene (`asciidoc-conventions`): balanced backticks, no leaked
     monospace, consistent rendering of names.

**4. Consistency checking** — §7, §11.
   - Terminology identical throughout; every defined term used; glossary matches
     Terms; component names match sibling specs in the repo.
   - No fact stated twice: near-duplicate tables/blocks are **MAJOR** — designate
     one canonical, cross-reference the other (§11).

**Plus the rest of the checklist** (scope to state): completeness and error/
failure paths (§8), reference-chapter completeness (§8a), security (§9 — defer to
`threat-model` if security-heavy), references (§12), glossary (§13), open issues
(§14 — empty for Approved), diagrams (§15), readability (§16).

## Output
Review notes in `spec-review-checklist` format — grouped by section, not severity,
so the author addresses them in order:

```
[MAJOR] §2a — §9.6 reproduces the stream-mapping table from §10.1; this is
   reference content braided into the architecture spec. Relocate the full table
   to the design reference; keep a one-line contract here with an xref.
```

Severities: `MAJOR` blocks approval, `MINOR` fix before approval, `NIT` optional.
End with a disposition and file out-of-scope findings as `DEV-TOOLS-TASK-NNNN`
(`task-from-sources`). Do not edit the document unless asked.

For doc↔code drift (is this document stale relative to the code?), run
`/docs-current` — that is the complementary check this command does not cover.

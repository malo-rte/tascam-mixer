---
name: asciidoc-conventions
description: Use whenever authoring, editing, or rendering AsciiDoc (.adoc) — specifications, manuals, guides, READMEs, design documents. Covers DEV-TOOLS house style: document header and attributes, section hierarchy, IDs and cross-references, includes, admonitions, source listings with callouts, tables, diagram embedding (PlantUML / Mermaid / ditaa), bibliography, conditional content, and multi-file document layout. Applies to all AsciiDoc, not just specifications.
---

# asciidoc-conventions

The AsciiDoc subset and house style used across DEV-TOOLS documents. Renders cleanly with `asciidoctor` and `asciidoctor-pdf`.

## Document header

Every `.adoc` file starts with:

```adoc
= Document Title
:doc-id:        DEV-TOOLS-DES-0042
:doc-version:   0.3
:doc-status:    Draft
:doc-date:      2026-05-25
:doc-authors:   M. Karlsson
:revnumber:     {doc-version}
:revdate:       {doc-date}
:toc:           left
:toclevels:     3
:sectnums:
:sectnumlevels: 4
:icons:         font
:source-highlighter: rouge
:experimental:
```

For specs, also set:

```adoc
:!chapter-signifier:
:partnums:
```

## Section hierarchy

| Markup     | Level | Use                                    |
|------------|-------|----------------------------------------|
| `= Title`  | 0     | Document title (exactly one per file)  |
| `== H1`    | 1     | Top-level chapter                      |
| `=== H2`   | 2     | Section                                |
| `==== H3`  | 3     | Subsection                             |
| `===== H4` | 4     | Sub-subsection (rare; avoid going deeper) |

If you reach `=====` more than once, the document needs restructuring.

## IDs and cross-references

Give every chapter, section, requirement, figure, and table a stable ID.

```adoc
[[sec-fwu-overview]]
== FWU overview

The update agent verifies bundles. See <<sec-fwu-bundle-format>>.

[[REQ-FWU-014]]
The update agent SHALL reject a bundle whose signature does not verify
against the active ROTPK.
```

Conventions:

- Section IDs: `sec-<area>-<topic>`, lowercase, hyphenated.
- Requirement IDs: `REQ-<AREA>-<NNN>` uppercase (see `spec-writing-style`).
- Figure IDs: `fig-<area>-<topic>`. Table IDs: `tbl-<area>-<topic>`.
- Cross-document refs: `xref:other-doc.adoc#sec-id[link text]`.

## Includes

For specs longer than ~500 lines, split into one file per chapter:

```
spec-fwu/
├── spec-fwu.adoc          # root: header + includes
├── ch-overview.adoc
├── ch-bundle-format.adoc
├── ch-state-machine.adoc
└── app-traceability.adoc
```

Root file:

```adoc
= FWU Specification
:doc-id: DEV-TOOLS-DES-0042
// ...header attributes...

include::ch-overview.adoc[]
include::ch-bundle-format.adoc[]
include::ch-state-machine.adoc[]

[appendix]
include::app-traceability.adoc[]
```

Each included file has no document title (`=`) of its own; the highest level inside an include is `==`.

## Admonitions

Use sparingly. Reserve for genuine warnings, not emphasis.

```adoc
NOTE: Non-critical contextual information.

TIP: Implementation guidance, non-normative.

IMPORTANT: A fact the reader must not miss.

CAUTION: Possible data loss or security implication.

WARNING: Safety-critical hazard.
```

Block form for multi-paragraph admonitions:

```adoc
[CAUTION]
====
The boot loader does not validate the rescue slot signature during
recovery. See <<sec-rescue-trust-model>> for the rationale and the
compensating controls.
====
```

Do not put normative requirements inside admonitions — they belong in the body.

## Source listings

Always tag the language. Use callouts for inline explanations.

```adoc
[source,c]
----
int update_apply(const bundle_t *b)
{
    if (verify_signature(b) != 0) {       // <1>
        log_audit(EVT_SIG_FAIL, b->id);   // <2>
        return -EPERM;
    }
    return write_slot(b);
}
----
<1> Implements <<REQ-FWU-014>>.
<2> Implements <<REQ-FWU-021>>.
```

For shell:

```adoc
[source,bash]
----
$ bitbake core-image-rte-eos
----
```

For configuration files, use the file's actual language tag (`yaml`, `kdl`, `toml`, `dts`, `bb`).

## Tables

```adoc
[#tbl-fwu-states]
.FWU state machine
[cols="1,2,2",options="header"]
|===
| State    | Allowed transitions      | Trigger
| Idle     | Verifying                | New bundle received
| Verifying| Idle, Writing            | Signature check result
| Writing  | Committed, Failed        | Write completion
|===
```

Conventions:

- `cols=` widths summing to a small total; prefer relative widths.
- `options="header"` whenever the first row is a header.
- Caption with `.Caption text` on the line above the table.
- ID on the line above the caption.

## Diagrams

Embed PlantUML or Mermaid; never raster screenshots of diagrams.

PlantUML:

```adoc
[plantuml, fig-fwu-state-machine, svg]
----
@startuml
[*] --> Idle
Idle --> Verifying : bundle received
Verifying --> Writing : signature ok
Verifying --> Idle : signature fail
Writing --> Committed : write ok
Writing --> Failed : write fail
@enduml
----
.FWU state machine
```

Mermaid (requires `asciidoctor-diagram` with mermaid):

```adoc
[mermaid, fig-fwu-flow, svg]
----
flowchart LR
    A[Bundle] --> B{Verify}
    B -->|ok| C[Write]
    B -->|fail| D[Reject]
----
```

Always render to SVG, not PNG. Diagrams live in `diagrams/` if external, or inline if short.

## Bibliography and references

Use `asciidoctor-bibtex` with a `.bib` file in the document root:

```adoc
== References

bibliography::[]
```

Cite inline with `cite:[rfc2119]`. Maintain `references.bib` alongside the spec.

For normative references that do not have a bibtex entry, use a manual table:

```adoc
[#tbl-normative-refs]
.Normative references
[cols="1,3,1",options="header"]
|===
| Ref        | Title                                              | Version
| RFC 2119   | Key words for use in RFCs to Indicate Requirement Levels | 1997
| IEC 62443-4-2 | Technical security requirements for IACS components   | 2019
|===
```

## Conditional content

Used to vary the same source for different audiences (e.g. public vs internal):

```adoc
ifdef::internal[]
The key derivation uses HKDF-SHA256 with the salt described in
<<sec-internal-keys>>.
endif::[]

ifndef::internal[]
The key derivation uses an HMAC-based scheme; details are out of scope
for this document.
endif::[]
```

Render with `asciidoctor -a internal spec.adoc` to include the internal variant.

Keep conditional blocks small; if a chapter is mostly conditional, split it into a separate include file.

## Character encoding (ASCII-only exemption for `.adoc`)

The project ASCII-only rule (memory `feedback_ascii_only`) applies
to source code, recipes, and configuration files (scripts, Python, C,
`.bb`, `.yaml`). It does **not** apply to AsciiDoc documents.

Documents are rendered through `asciidoctor` to PDF / HTML, where
Unicode (em-dash U+2014, en-dash U+2013, smart quotes, mathematical
symbols, accented characters in proper names, ...) renders correctly.
Source files are read by tools, grep, and CI, where ASCII improves
reliability and search-ability. Documents are read by humans through
a renderer and benefit from typographic Unicode.

Rule for `.adoc` files:

* Em-dashes (`—`), en-dashes (`–`), smart quotes (`" "  ' '`),
  copyright (`©`), registered (`®`), trademark (`™`), section sign
  (`§`), arrows (`← → ↑ ↓ ↔`), mathematical symbols (`× ÷ ≤ ≥ ≠`),
  Greek letters in technical context, and proper-name diacritics
  are all acceptable.
* ASCII alternatives (`--`, `->`, ...) are also acceptable; pick one
  and stay consistent within a document.
* Code blocks remain literal; do not "smart-quote" code samples.
* Document titles, body prose, table cells, and admonition text MAY
  use Unicode freely.

This exemption is documented in `.claude/skills/asciidoc-conventions/SKILL.md`
and applies to every `.adoc` file under `docs/`.

## Markup hygiene (lint)

Rendering artefacts leak when inline markup is unbalanced. These are mechanical
checks; run them before every PDF build and wire them into `run-all-checks.sh`.

- **Balanced inline code.** Backticks must pair within a single logical line. A
  stray backtick produces visible leakage in the PDF — e.g. `` `bserial`` rendered
  as a literal backtick followed by un-monospaced text, or a broken span like
  `` [HH:MM:SS] ` to each line written to `log-file ``. Grep for lines with an odd
  count of backticks; each is a defect.
- **No monospace spanning a line break.** An inline-code span (`` ` `` … `` ` ``)
  opens and closes on the same line. If it must wrap, it is too long — restructure.
- **Consistent rendering of recurring names.** Pick one rendering for the tool
  name, command names, field names, and config keys, and apply it everywhere
  (e.g. the tool name always in monospace, never bare). Inconsistent rendering of
  the same token across the document is a NIT that compounds into noise.
- **Balanced attribute and passthrough markup.** Unclosed `+...+`, `[...]`, or
  `pass:[...]` constructs corrupt the surrounding paragraph; check pairing.
- **Smart-quote bleed.** Verify code listings were not smart-quoted (see the
  Unicode exemption above); curly quotes inside a source block break copy-paste.

A document that renders without visible markup characters in the body passes this
section.

## What to avoid

- Inline HTML (`pass:[<b>...</b>]`) — breaks PDF output and is not portable.
- Deep nesting (more than four section levels).
- Tables used for layout rather than tabular data.
- Diagrams without IDs.
- Wide source listings that wrap in PDF — keep code lines ≤ 80 chars.
- Multiple document titles (`=`) in one file.
- Headings that are full sentences.

## File and directory layout

```
specs/
├── _template.adoc              # canonical empty spec
├── _attributes.adoc            # shared attributes, included by all
├── references.bib              # shared bibliography
├── spec-fwu/
│   ├── spec-fwu.adoc
│   ├── ch-*.adoc
│   └── diagrams/
├── spec-boot/
└── ...
```

Each spec gets its own directory once it grows past one file. Shared attributes via `include::../\_attributes.adoc[]` at the top of the root file.

## What this skill does not cover

- What sections a spec needs → `spec-document-template`
- How to write the prose → `spec-writing-style`
- How to review a finished spec → `spec-review-checklist`

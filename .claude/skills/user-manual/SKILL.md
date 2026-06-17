---
name: user-manual
description: Write usage-focused user manuals for software, drawing facts from specifications and source code. Use this skill whenever the user wants to produce, draft, structure, or update a user manual, user guide, end-user documentation, operator manual, or "the docs" for a piece of software — including CLI tools, libraries/APIs, GUI apps, and embedded devices. Trigger it even when the user just hands over a spec, a repo, or a set of source files and says "document this" or "write the manual," and even when they only name one part (e.g. "write the glossary" or "draft the reference chapter"). Produces a complete, structured manual (frontpage, TOC, introduction, scope, concept chapters, detailed chapters, reference chapters, appendixes, glossary) in AsciiDoc or Markdown.
---

# User Manual

Produce a complete, usage-focused user manual for a piece of software, with facts
sourced from its specifications and code.

The guiding principle is **usage over internals**. The reader wants to *use* the
software correctly and confidently. Implementation details belong in the manual
only when they change what the user must do, type, configure, or expect. When a
spec or a source file describes how something is built, your job is to translate
that into what the user does with it.

## Place in the document family

This skill is a peer of the DEV-TOOLS specification skills and reuses their shared
layers rather than duplicating them. A manual is a document like any other.

- **Rendering** is governed by `asciidoc-conventions` — the document header,
  `:doc-id:`/`:doc-status:` attributes, IDs and cross-references, tables,
  diagrams, includes, and file layout. Do not invent AsciiDoc style here; follow
  that skill. `references/formats.md` only adds the manual-specific bits.
- **Placement, ID allocation, and the index** are governed by `docs-librarian`.
  A manual gets a doc ID, lives under its class directory, and is registered in
  `docs/_index.yaml` exactly like a spec. Manuals use the `MAN` class
  (`DEV-TOOLS-MAN-NNNN`, under `docs/manuals/`); allocate with
  `docs-librarian next-id MAN`. Use that skill when creating, moving, or
  retiring a manual.
- **Structure** parallels `spec-document-template`: same intro/scope/overview
  triad, the same content-vs-reference chapter split, appendixes, and glossary.
  The manual adds a layer of **task/procedure chapters** specs don't have, and
  reuses the spec template's reference-chapter table. See `references/structure.md`.
- **Prose deliberately diverges from `spec-writing-style`.** A manual is
  *non-normative*: it describes usage, it does not mandate behavior, so it uses
  no RFC 2119 keywords. Where it states a guarantee, it cross-references the
  governing spec's requirement ID rather than restating the requirement.
- A manual is **derived from** specs and code, so it can xref the spec's
  requirement IDs and reference chapters instead of re-deriving facts. See
  `references/sourcing.md`.

## When this skill applies

Any request to create, draft, restructure, extend, or revise end-user
documentation for software. This includes partial requests ("just the reference
chapter," "add a glossary") — handle the requested part using the same structure
and conventions, so it slots into a larger manual cleanly.

## Required structure

Every full manual contains these sections, in this order. `references/structure.md`
defines the purpose, contents, and anti-patterns for each — **read it before
outlining.**

1. Frontpage
2. Table of contents
3. Introduction
4. Scope
5. Concept chapters — high-level descriptions / the mental model
6. Detailed chapters — step-by-step usage and procedures
7. Reference chapters — exhaustive lookup material
8. Appendixes
9. List of used terms (glossary)

For a partial request, produce the relevant section(s) but keep the same
conventions so they drop into the whole.

## Workflow

Work through these stages. Don't skip the sourcing and outline stages — drafting
before you know the audience and the source material is how manuals end up
describing the code instead of the usage.

### 1. Establish context

Pin down three things before writing anything:

- **Software type** — CLI tool, library/API, GUI app, embedded device, or a mix.
  This drives what "usage" means and what the reference chapters contain.
- **Audience** — who uses this and what tasks they perform. An integrator, an
  end operator, and a downstream developer need different manuals.
- **Sources available** — specifications, source code, existing docs, `--help`
  output, config schemas, device trees. Note what you have and what you lack.

If the user hasn't supplied sources, ask where they are (a repo path, spec files)
rather than inventing behavior. If only some sources exist, proceed and flag gaps.

### 2. Mine the sources for usage facts

Read `references/sourcing.md` for how to extract *usage-relevant* information from
specs and code per software type, how to record provenance, and how to flag gaps
without inventing behavior. The output of this stage is a set of grounded facts —
commands, options, config keys, public API signatures, procedures, error
conditions, defaults — each tied to where it came from.

### 3. Outline against the required structure

Map the software onto the nine required sections. Decide the concept chapters
(the handful of ideas a user must hold in their head), the detailed chapters (the
tasks they perform), and what the reference chapters must exhaustively cover.
`references/structure.md` explains the concept-vs-detail-vs-reference split, which
is the part most manuals get wrong.

Show the outline to the user before drafting the full manual unless they've asked
you to just produce it end to end.

### 4. Draft, usage-first

Write section by section following `references/writing.md` (voice, task
orientation, procedure format). Keep concept chapters short and explanatory;
make detailed chapters procedural and testable; make reference chapters complete
and scannable. Every claim about behavior must trace to a source from stage 2 —
if you can't source it, mark it as a gap rather than guessing.

### 5. Assemble in the target format

The manual is an AsciiDoc document in the doc set, so its rendering follows
`asciidoc-conventions` (header, attributes, TOC, tables, diagrams, includes,
file layout) and its placement/ID/index follow `docs-librarian`. Use
`references/formats.md` only for the manual-specific additions on top of those:
the front-matter attribute values, end-user TOC depth, `[glossary]` and
`[appendix]` usage, and the optional Markdown fallback for manuals published
outside the AsciiDoc doc set. For `.docx`, generate the content here and hand
off to the `docx` skill.

### 6. Review pass

Before delivering, check: every chapter is about *using* the software, not
building it; terms used in the body appear in the glossary; procedures are
complete and ordered; reference material is exhaustive for its scope; and gaps
are clearly marked rather than silently filled. Read `references/writing.md`'s
checklist.

## Reference files

- `references/structure.md` — what each required section is for, what goes in it,
  and the concept/detail/reference distinction. Read before outlining.
- `references/sourcing.md` — extracting usage facts from specs and code by
  software type; provenance and gap-flagging. Read before drafting.
- `references/writing.md` — usage-focused voice, procedure format, terminology
  discipline, and the final review checklist.
- `references/formats.md` — AsciiDoc and Markdown scaffolding for frontpage, TOC,
  appendixes, and glossary.

## Hard rules

- Never invent software behavior. If a fact isn't in the sources, mark it as a
  gap (see `sourcing.md`) and move on.
- Keep the focus on usage. Describe internals only where they change what the
  user does.
- Keep terminology consistent and reflected in the glossary. Where the codebase
  uses one term and the user audience uses another, the manual follows the user
  vocabulary. (e.g. for shell-style `${name}` references in configuration, write
  "variable substitution" rather than "interpolation," even if the source calls
  it interpolation -- the manual's audience comes from the config-tool world,
  not the programming-language world.)
- **Reject implementation-leak phrasing.** When any of these appears in a draft,
  rewrite it as what the user does, types, or observes:
  * Crate, package, library, or module names lifted from the source
    (`devtools-subst`, `register_all`, "the X engine," "the Y module").
  * Type, trait, struct, or method names from the codebase
    (`Interpolator`, `Resolver`, `Drop`, `Instant`).
  * Concurrency / memory-model vocabulary (async, lazy, zeroize-on-drop, mlock,
    the runtime, the executor) unless the user has to act on it.
  * Internal lifecycle as a state machine when the user never sees the
    transitions ("Step 1: resolves a profile. Step 2: opens a transport.
    Step 3: installs triggers...").
  * Implementation history (ticket IDs, PR numbers, commit hashes, internal
    decision dates) -- that lives in commits and ADRs, not in the manual.
  * Build / binary metadata (size, dependency graph, link layout) outside a
    reference appendix that genuinely documents the binary.
  These are anti-patterns even when sourced verbatim from a spec or source
  comment; translation to user vocabulary is part of the sourcing pass.
- **Concept chapters describe what the user does and thinks, not what the
  implementation does.** Each subsection of a concept chapter should open with
  the user as the subject: "You write a profile that...", "When you run
  `bserial connect`, you're in...", "Triggers fire when the board prints..."
  -- not "A profile is a KDL document that...", "The session lifecycle runs
  through six steps...", "The trigger engine watches the byte stream..."
- **Cross-document xrefs carry the doc ID as visible link text.** asciidoctor
  renders `xref:other.adoc[]` with the target's title -- which the reader
  can't recover when a PDF is distributed standalone and the link is dead.
  Write `xref:.../ARCH-0001-bserial.adoc[DEV-TOOLS-ARCH-0001 bserial
  architecture]` so the citation survives a broken link.
- **Pin the documented tool version.** A manual documents the *shipped*
  software, so its front matter declares `:applies-to-version:` — the repo's
  canonical (unified) tool version it describes. This is distinct from
  `:doc-version:`, which tracks the manual's own editing lifecycle
  (Draft → Approved), and from the requirement/spec versions, which stay
  independent. The `release` flow stamps `:applies-to-version:` to the release
  version via `bump-version` (which treats any doc carrying this attribute as a
  version manifest and fails the gate if it lags), so a manual always states the
  version of the software it documents. Specs and ADRs do **not** carry this
  attribute — only manuals (and anything else that genuinely documents a shipped
  version) opt in.
- **Describe the current state of the software, not what it was or will be.**
  The manual is a snapshot the reader is using right now; it is not a
  changelog and not a roadmap. Avoid:
  * Past-tense behaviour ("X used to require Y", "before version 3.0, ...",
    "previously the flag was named `--old`", "no longer supports Z").
  * Future-tense behaviour ("X will be added in 2.0", "planned for the next
    release", "future versions may support Z", "not yet implemented").
  * Implementation history dressed as prose ("M4 introduced phases", "shipped
    in slice 11", "rewritten from gpg-shellout to age").
  Current-state migration UX is fine -- "Run `tool profiles migrate` to
  convert older files" describes what the tool does now. Deprecation notes
  are fine when the alternative exists now ("Prefer `--new`; `--old` is
  deprecated"). Released-version compatibility is fine in the Scope chapter
  ("works with kernels 5.10+"). Roadmap and changelog belong in the project's
  release notes or commit log, not in the manual.

# Output format (manual-specific additions)

A manual is an AsciiDoc document in the doc set. **Rendering style is owned by
`asciidoc-conventions`** — the document header, `:doc-*:` attributes, `:toc:`,
section hierarchy, IDs and `<<xref>>` cross-references, source listings, tables,
diagram embedding, bibliography, and multi-file layout. Do not restate any of
that here; follow that skill. **Placement, ID, and index** are owned by
`docs-librarian`. This file only covers what is specific to a *manual* on top of
those two skills.

## Front-matter values for a manual

Use the standard header from `asciidoc-conventions`, with manual-appropriate
values:

```adoc
= Software Name User Manual
:doc-id:        DEV-TOOLS-<CLASS>-NNNN   // class/prefix per docs-librarian
:doc-version:   0.1
:doc-status:    Draft
:doc-date:      2026-01-01
:doc-authors:   <names>
:toc:           left
:toclevels:     3                       // end users rarely need deeper than 3
:sectnums:
```

Add one line identifying the software version the manual applies to, near the
top of the introduction:

```adoc
This manual applies to Software Name version X.Y.
```

The manual's document state (Draft → Under review → Approved → Superseded) and
versioning follow the same rules as a spec; see `spec-document-template`.

## Required sections in AsciiDoc

The nine required sections map to standard AsciiDoc constructs:

- Frontpage → the document header above.
- Table of contents → `:toc:` (auto-generated; never hand-write).
- Introduction, Scope → ordinary `==` chapters.
- Concept / detailed / reference chapters → `==` chapters; reference chapters use
  the `== <Name> reference` heading convention from `spec-document-template`.
- Appendixes → `[appendix]\n== Title` (labelled Appendix A, B, …).
- List of used terms → `[glossary]\n== List of used terms`, with `term:: def`.

Multi-file manuals follow the same per-document directory + `include::` pattern as
specs (see `asciidoc-conventions`): a root file plus `ch-*.adoc` chapter files.

## Markdown fallback (out-of-family)

Only for manuals published *outside* the AsciiDoc doc set (e.g. a Markdown doc
site). Markdown has no native TOC/appendix/glossary, so rely on the site
generator (MkDocs/Sphinx) for the TOC, and use heading conventions for
appendixes ("Appendix A: …") and the glossary. Keep the same section order and
the content/detail/reference split. If the target is Word, generate content here
and hand off to the `docx` skill.

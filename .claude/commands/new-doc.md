---
description: Scaffold a new document — allocate ID, place it, stub the skeleton, register in the index
argument-hint: "<type: req|arch|des|interface|man|adr> <title>"
allowed-tools: Bash(.claude/skills/docs-librarian/scripts/docs-librarian:*), Bash(python3:*), Bash(mkdir:*), Read, Write, Edit, Glob
---

Scaffold a new document following `docs-librarian`, `spec-document-template`, and
the matching type skill. Request: $ARGUMENTS

## Parse the request
`$ARGUMENTS` is `<type> <title…>`. Map the type to its class and skill:

| type | class | skill |
|------|-------|-------|
| `req` | `REQ` | `requirement-spec` |
| `arch` | `ARCH` | `architecture-spec` |
| `des` | `DES` | `design-spec` |
| `interface` | `DES` | `interface-spec` |
| `man` | `MAN` | `user-manual` |
| `adr` | `ADR` | `architecture-decision-record` |

If the type is missing or unknown, ask once; don't guess.

## Steps
1. **Allocate the ID.** Run `docs-librarian next-id <CLASS>` to get
   `DEV-TOOLS-<CLASS>-NNNN` (the `identifier-conventions` grammar). Never pick a
   number by hand.
2. **Slug the title.** Lowercase kebab-case, ASCII (`docs-librarian` L12). The
   document **name** is `DEV-TOOLS-<CLASS>-NNNN-<slug>`.
3. **Place it.** Under the class directory (`docs/requirements/`,
   `docs/architecture/`, `docs/design/`, `docs/manuals/`, `docs/adr/`). A spec is a
   directory with a root `.adoc` of the same name; an ADR is a single
   `.adoc` file (L10).
4. **Stub the skeleton.** Write the front-matter block (`:doc-id:`,
   `:doc-version: 0.1`, `:doc-status: Draft`, date, authors) per
   `asciidoc-conventions`, then the section headings for this **type** — from the
   type skill's skeleton (e.g. `architecture-spec` gives Overview / Context /
   Subsystem chapters / Cross-cutting / Key decisions / Implementation Status; an
   ADR gives Status / Context / Decision / Alternatives / Consequences). Stub each
   heading with a one-line description, not prose.
5. **Register and render.** Add the entry to `docs/_index.yaml` (id, title, class,
   path, status Draft, version 0.1, date, authors) in sorted position, then
   `docs-librarian render` to regenerate `INDEX.adoc`.
6. **Verify.** Run `docs-librarian validate` — it must pass.

Report the allocated ID, the path created, and the next step (write the Scope
first, per `spec-writing-style`). Commit `_index.yaml`, `INDEX.adoc`, and the new
document together (`/commit`).

---
name: docs-librarian
description: Use whenever a document is added, moved, renamed, retired, or superseded under docs/, when allocating a new document ID, when reviewing a pull request that touches the docs/ tree, or when auditing the documentation set for drift. Enforces directory placement, ID allocation rules, file and directory naming, and maintains the project-wide document index (`docs/_index.yaml` and its rendered `docs/INDEX.adoc`). Apply this skill any time the shape of the doc set changes, not the content of an individual document.
---

# docs-librarian

Watches over the docs tree: keeps documents in the right place, keeps IDs unique and well-allocated, and maintains the document index. Use with `spec-document-template` (the inside of a document) and the `docs/README.adoc` (the directory structure).

## When to invoke

- Creating a new document (allocate ID, place correctly, register in index)
- Moving or renaming a document
- Retiring or superseding a document
- Promoting a document between states (Draft → Under review → Approved → Superseded)
- Reviewing a PR that adds, moves, or removes anything under `docs/`
- Periodic audit (suggested: weekly, or before each release)

## Invariants

The librarian enforces these rules. Any violation is a MAJOR finding.

| Rule | Description |
|------|-------------|
| L01  | Every document lives under the class directory matching its ID prefix (`REQ` → `requirements/`, `DES` → `design/`, `MAN` → `manuals/`, etc.) |
| L02  | The directory name (or single-file name) matches the canonical doc ID, optionally followed by a kebab-case slug |
| L03  | The root `.adoc` file inside a document directory has the same base name as the directory |
| L04  | Every document is registered in `docs/_index.yaml` with the fields listed in <<the-index>> |
| L05  | No ID is reused. Retired IDs remain listed in `_index.yaml` with `status: Retired` and no path |
| L06  | No two documents share an ID |
| L07  | A document with `status: Approved` has a version `>= 1.0` |
| L08  | A document with `status: Superseded` has a non-null `superseded-by` pointing to an existing ID |
| L09  | A document with a non-null `supersedes` points to an existing ID whose `status` is `Superseded` |
| L10  | Single-file documents (`<id>-<slug>.adoc` directly under a class directory) are allowed only for ADRs or documents under ~500 lines |
| L11  | A document directory contains a `diagrams/` subdirectory only if it has at least one diagram source file |
| L12  | The slug uses lowercase kebab-case, ASCII only, no leading or trailing hyphens |
| L13  | The `_index.yaml` is sorted by ID within each class, classes in the order: REQ, ARCH, DES, TEST, OPS, MAN, EXT, REP, ADR |
| L14  | Every imported document referenced from a normative section appears in `docs/_imports/manifest.yaml` (the librarian checks this against bibliography citations) |

## The index

`docs/_index.yaml` is the source of truth. `docs/INDEX.adoc` is generated from it.

Entry shape:

```yaml
- id:            DEV-TOOLS-DES-0010
  title:         FWU bundle format interface
  class:         design
  subtype:       interface          # optional: interface, module, ...
  path:          docs/design/DEV-TOOLS-DES-0010-fwu-bundle-format-interface/
  status:        Draft              # Draft | Under review | Approved | Superseded | Retired
  version:       "0.3"
  date:          2026-05-12
  authors:       [MK]
  reviewers:     []
  approvers:     []
  supersedes:    null               # ID or null
  superseded-by: null               # ID or null
  tags:          [security, fwu]    # free-form, lowercase
```

A Retired entry omits `path`, `version`, `date`, and the people fields:

```yaml
- id:            DEV-TOOLS-DES-0007
  title:         (retired)
  class:         design
  status:        Retired
```

## Workflow: add a new document

1. Determine the class. If unclear, consult `docs/README.adoc`.
2. Allocate the next free `<NNNN>` in that class — scan `_index.yaml`, take the highest non-retired sequence number in the class and add one. Never reuse a Retired or Superseded number.
3. Create the directory or single file at the correct path, named `<ID>[-<slug>]`.
4. Inside the document, set the metadata block: `:doc-id:`, `:doc-version: 0.1`, `:doc-status: Draft`.
5. Add an entry to `_index.yaml` in the correct sorted position.
6. Regenerate `docs/INDEX.adoc`.
7. Commit `_index.yaml`, `INDEX.adoc`, and the new document in the same commit.

## Workflow: move or rename a document

The canonical ID never changes. Only the path or slug may change.

1. Move the file or directory to the new location.
2. Update the `path` field in `_index.yaml`.
3. Update the slug in the directory and root file name if it changed; the ID portion remains.
4. Update inbound cross-references (other documents' xrefs that hard-code the path; xrefs by ID continue to work).
5. Regenerate `INDEX.adoc`.

A document moving between classes is not a move — it is a retirement plus a new document, because the class is encoded in the ID prefix.

## Workflow: retire or supersede

A document is retired when it is no longer in force and is not replaced.
A document is superseded when it is replaced by another document.

To supersede `OLD` with `NEW`:

1. Create `NEW` following the new-document workflow. In its metadata, set `supersedes: <OLD-ID>`.
2. Set `OLD`'s status to `Superseded`, set `superseded-by: <NEW-ID>`.
3. Add a final revision-history row to `OLD` recording the supersession.
4. Add a banner at the top of `OLD`'s introduction pointing to `NEW`.
5. Do not delete `OLD` — it remains for historical traceability.

To retire `OLD` without replacement:

1. Set `OLD`'s status to `Retired`.
2. Move `OLD`'s content into an `_archive/` subdirectory under its class, or remove the body and leave only the front matter explaining the retirement.
3. Update `_index.yaml` to a Retired entry.

## Audit checks

Run these against the docs tree. Each maps to one or more invariants from the table above.

1. **Placement** — walk `docs/`. Every document directory or single file is under the class directory matching its ID prefix. (L01, L02)
2. **Name consistency** — root `.adoc` matches directory name. (L03)
3. **Index sync** — every document on disk has an entry; every non-Retired entry has a path that exists. (L04)
4. **ID uniqueness** — no duplicate IDs in `_index.yaml`; no duplicate IDs across `.adoc` metadata blocks. (L06)
5. **Sequence integrity** — no reused Retired or Superseded numbers in newer entries. (L05)
6. **Status / version consistency** — Approved has version `>= 1.0`; Draft and Under review have `0.x`. (L07)
7. **Supersession integrity** — `supersedes` / `superseded-by` pairs are bidirectional and consistent. (L08, L09)
8. **Slug form** — kebab-case, ASCII, no leading or trailing hyphens. (L12)
9. **Index ordering** — entries sorted by class then sequence. (L13)
10. **Imports consistency** — every `cite:[...]` resolving to an imported document has a matching `_imports/manifest.yaml` entry. (L14)
11. **Orphan check** — every `.adoc` under `docs/` (excluding `_templates/`, `_imports/`, shared fragments) belongs to a registered document.

The audit produces a report grouped by invariant ID. A clean run produces an empty report.

## Index rendering

`docs/INDEX.adoc` is generated from `_index.yaml`. Convention:

- Top-level sections per class, in canonical order
- Within each class, a table with columns: ID, Title, Status, Version, Date, Authors
- Retired entries listed in a separate "Retired" subsection at the end of each class
- Superseded entries marked with a forward arrow to their replacement
- The index document itself carries a metadata block and `Status: Approved` (it is reference data, not a draft)

Do not hand-edit `INDEX.adoc`. Edit `_index.yaml` and regenerate.

## Implementation

The tool lives at `.claude/skills/docs-librarian/scripts/docs-librarian`
(Python, PyYAML). Entry points:

```bash
docs-librarian audit                # run checks + check-links, human-readable report
docs-librarian render               # regenerate INDEX.adoc from _index.yaml
docs-librarian next-id <class>      # print the next free ID in a class
docs-librarian check-links          # resolve code->doc citations against the index
docs-librarian release-manifest X.Y.Z  # snapshot the doc set's versions for a release
docs-librarian validate             # exit non-zero on any invariant failure (CI hook)
```

`release-manifest X.Y.Z` writes `docs/releases/X.Y.Z.yaml` recording every
non-retired document's id, class, version, and status at that release — the
correspondence between a software release and the doc-set state, without changing
any document's own version (`release` skill, unified-versioning policy).

`check-links` scans source files (`.rs/.c/.h/.py/.sh`, skipping `target/` and
other build dirs) for `implements:`/`see:`/`satisfies:`/`ref:` citations and
resolves each ID against `_index.yaml`; a citation naming an absent or retired
document is a dangling reference and exits non-zero. It is the resolver half of
the `doc-code-sync` skill and runs inside `audit`. Boundary: it confirms the
*document* exists, not that an individual requirement ID within it does (the index
tracks documents, not per-requirement registries).

`validate` is wired into `scripts/run-all-checks.sh` so the
`_index.yaml` invariants are enforced on every pre-ship run. The
mechanical checks the tool covers today are L02, L04, L06, L07, L12,
L13 (plus ID-shape and class-vs-ID consistency). L01, L03, L05, L08,
L09, L10, L11 and L14 remain manual audit territory until each gains
a dedicated mechanical check.

## What this skill does not cover

- The project-wide ID grammar (every class, the entity/item tiers) → `identifier-conventions`
- Document content, structure, or prose → `spec-document-template`, `spec-writing-style`
- Review of an individual document → `spec-review-checklist`
- AsciiDoc rendering → `asciidoc-conventions`
- Build, publish, and release tooling → operations documents

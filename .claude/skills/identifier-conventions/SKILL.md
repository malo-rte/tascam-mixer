---
name: identifier-conventions
description: Use whenever allocating, formatting, citing, or reviewing any project identifier — document IDs, ADR IDs, task IDs, requirement IDs, open-issue IDs, threat IDs. Defines the one grammar [PROJECT]-[CLASS]-[NUMBER]-[TITLE] that every identity follows, the class registry, the entity-vs-item tiers, the abbreviated reference form, allocation (which tool owns each class's sequence), and the immutability rule. The single source of the ID format; docs-librarian, task-tracker, spec-writing-style, threat-model, the coding rules' TODO convention, and doc-code-sync all conform to it. Apply any time an ID is created or judged, not only when "IDs" are named.
---

# identifier-conventions

The one grammar every project identifier follows. Documents established it; it
applies to **all** identities. This skill is the single source; the skills that
mint or cite IDs (`docs-librarian`, `task-tracker`, `spec-writing-style`,
`threat-model`, the coding rules, `doc-code-sync`) conform to it rather than
restating it.

## The grammar

```
[PROJECT]-[CLASS]-[NUMBER]-[TITLE]
```

- **ID** = `[PROJECT]-[CLASS]-[NUMBER]` — the stable, citeable identity. Never
  changes, never reused.
- **Name** = the ID plus `-[TITLE]` — the human-readable form used for a file,
  directory, or list entry. The title may be re-slugged; the ID portion may not.

Fields:

| Field | Rule |
|-------|------|
| `PROJECT` | The project namespace. `DEV-TOOLS`. |
| `CLASS` | An uppercase class tag from the registry below. |
| `NUMBER` | Zero-padded sequence. Entities use four digits (`0042`); document-scoped items use three (`014`). |
| `TITLE` | Lowercase kebab-case, ASCII, no leading/trailing hyphen (same slug rule as `docs-librarian` L12). |

Example, fully spelled out: `DEV-TOOLS-ARCH-0001-bserial` — ID
`DEV-TOOLS-ARCH-0001`, title `bserial`.

## Two tiers

Not every identity is a standalone file. Distinguish:

- **Entities** — first-class, allocatable, own a per-class sequence, may be a file
  or a registry entry. They take the **full** grammar. Documents, ADRs, and
  **tasks** are entities.
- **Document-scoped items** — numbered *inside* a host document; the host's full
  ID supplies the project context. They take a **scoped** form `[CLASS]-[NUMBER]`
  (optionally `[CLASS]-[AREA]-[NUMBER]`). Requirements, open issues, and threats
  are items. An item's globally-unique name is the composition
  `<host-doc-ID> / <item-ID>` (e.g. `DEV-TOOLS-REQ-0001 / REQ-FWU-014`).

## Class registry

**Entity classes** — full form `DEV-TOOLS-<CLASS>-NNNN`:

| Class | Kind | Allocated by |
|-------|------|--------------|
| `REQ` `ARCH` `DES` `TEST` `OPS` `MAN` `EXT` `REP` | document | `docs-librarian next-id <CLASS>` |
| `ADR` | document (decision record) | `docs-librarian next-id ADR` |
| `TASK` | work item | `tasklist next-id` |

**Document-scoped item classes** — scoped form, numbered within a host:

| Class | Form | Host | Defined by |
|-------|------|------|------------|
| requirement | `REQ-[AREA]-NNN` (or legacy `[AREA]-NNN`) | a `REQ` document | `spec-writing-style` |
| open issue | `OI-NNN` | any spec | `spec-document-template` |
| threat | `THR-NNN` | a threat model | `threat-model` |

Note the deliberate, structurally-unambiguous overlap: `REQ` is both an entity
class (a requirements **document**, `DEV-TOOLS-REQ-0001`) and the prefix of a
requirement **item** (`REQ-FWU-014`). They never collide because the entity form
always carries `DEV-TOOLS-` and four digits, while the item form carries an `AREA`
tag and three digits. Tools disambiguate by structure.

## Abbreviated reference form

The canonical, definitional form is always full. Inside the project repo — in
code comments and intra-repo cross-references — an entity MAY be cited in the
abbreviated form `[CLASS]-[NUMBER]` (`TASK-0042`, `ADR-0027`, `DES-0010`) where
`PROJECT` is unambiguous. The full form is preferred and is the only form
permitted where the citation might be read outside the repo (a distributed PDF, a
foreign tracker). Resolvers (`docs-librarian check-links`) accept both; reviewers
prefer the full form.

## Allocation and immutability

- Each entity class owns a monotonic sequence. Allocate the next free number with
  the owning tool — never pick by hand, never reuse.
- IDs are immutable. A renamed or retired entity keeps its ID; the slug/title may
  change, the ID may not (`docs-librarian` L05, `task-tracker` T09).
- Retired/Done/Dropped/Superseded entities keep their ID and stay in the registry
  for traceability; their number is never re-issued.

## TODO and cross-reference forms

Code citing a task or document uses the citation convention from `doc-code-sync`
with these IDs:

```
// TODO(DEV-TOOLS-TASK-0042): widen the timeout once the PHY fix lands
// implements: DEV-TOOLS-REQ-0001            (or the item: REQ-FWU-014)
// see: DEV-TOOLS-ADR-0027
```

The abbreviated inline form (`TODO(TASK-0042)`, `see: ADR-0027`) is permitted per
the rule above; the full form is canonical. Commit trailers (`git-commit`) and
test references (`test-writing-rules`) use the same IDs:
`Refs: DEV-TOOLS-TASK-0042`.

## Review checklist

- [ ] Every ID matches `[PROJECT]-[CLASS]-[NUMBER]` (entity) or `[CLASS]-[NUMBER]`
      (document-scoped item)
- [ ] `PROJECT` is `DEV-TOOLS`; `CLASS` is in the registry; numbers zero-padded
      (entities 4, items 3)
- [ ] Entity IDs were allocated by the owning tool, not hand-picked
- [ ] No reused or renumbered IDs; retired entries keep their ID
- [ ] Out-of-repo citations use the full form, not the abbreviation

## What this skill does not cover

- Document placement, the index, and `next-id` for doc classes → `docs-librarian`
- Task allocation and the task registry → `task-tracker`
- Requirement ID phrasing and the legacy form → `spec-writing-style`
- Open-issue and threat item conventions → `spec-document-template`, `threat-model`
- Code→doc citation syntax and its resolver → `doc-code-sync`, `docs-librarian check-links`

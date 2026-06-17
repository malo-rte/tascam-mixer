---
name: release-notes
description: Use whenever recording what changed between versions of a tool or document, writing a changelog or release notes, deciding a version bump, or documenting deprecations and migrations. This is the home for the time-relative content (what changed, what's deprecated, what's planned) that specs and manuals deliberately exclude — it complements the no-planning rule (doc-content-class CC-5) and the manual's current-state rule. Covers Keep a Changelog structure, semantic versioning decisions, the Unreleased section, deprecation and migration entries, and audience-facing vs internal notes. Apply when a release is cut, a CHANGELOG is edited, or someone asks "what changed?".
---

# release-notes

How to record **what changed across versions**. This is the deliberate home for
time-relative content that `doc-content-class` CC-5 keeps out of specs and the
`user-manual` current-state rule keeps out of manuals: changelog entries,
deprecations, migration steps, and what is planned. Specs and manuals describe the
*current* state; release notes describe the *deltas* and the *trajectory*.

The split: a spec says "the tool retains N lines of scrollback." The release notes
say "0.21 → 0.22: scrollback now survives reconnect (was lost before)." Neither
restates the other (CC-2); the spec links to nothing here, but a reader asking
"what changed?" comes here.

## Two kinds, one source

- **Changelog** — the complete, ordered record of every notable change, by
  version. Lives in `CHANGELOG.md` (or `.adoc`) at the project root. Audience:
  anyone upgrading.
- **Release notes** — a per-release narrative highlighting the changes that matter
  to users, derived from the changelog. Audience: users deciding whether/how to
  upgrade.

Maintain the changelog as the source of truth; derive release notes from it. Do
not keep two independently-edited lists.

## Changelog structure (Keep a Changelog)

```
# Changelog

## [Unreleased]
### Added
### Changed
### Deprecated
### Removed
### Fixed
### Security

## [1.2.0] - 2026-06-01
### Added
- Multi-connection sessions: one session can own several named ports.
### Changed
- Scrollback now survives reconnect and inserts a boundary marker.
### Deprecated
- `--old-flag`; use `--new-flag`. Removal planned for 2.0.
```

- An **`[Unreleased]`** section at the top accumulates entries as work lands; it
  becomes the next version's section at release.
- The six categories (Added, Changed, Deprecated, Removed, Fixed, Security) are
  fixed; omit empty ones in a released section.
- Each entry is one line, user-facing, past/present tense describing the change —
  not a commit subject. Write for someone who does not know the codebase.
- **Security** changes always get their own entry, even if also listed elsewhere,
  so an upgrader scanning for security fixes finds them.

## Versioning decisions

Semantic versioning. The decision rule:

| Change | Bump |
|--------|------|
| Backward-incompatible change to a public contract (CLI, format, API, schema) | **major** |
| New capability, backward-compatible | **minor** |
| Bug fix, no contract change | **patch** |

- The public contract is whatever the spec set marks normative — a CLI flag, an
  exit code, a file format, a config key, an API signature. Changing the *meaning*
  of one is a major bump even if the syntax is unchanged.
- Pre-1.0 (`0.x`): the project has not committed to stability; document that
  breaking changes may occur on any bump (consistent with `spec-document-template`
  document states). The bserial spec's own `0.x` series is an example.
- A spec/ADR version is independent of the software version
  (`spec-document-template` lifecycle); a changelog tracks the software. Two
  bridges exist (see `release`): a user manual pins the tool version it documents
  via `:applies-to-version:`, and `docs-librarian release-manifest X.Y.Z` records
  the whole doc set's versions as of a release — neither renumbers a spec.

## Deprecations and migrations

- A **deprecation** entry names the old thing, the replacement (which must exist
  *now* — see the manual's current-state rule), and the version it is planned for
  removal. Deprecation is the one place a *plan* legitimately appears, because the
  user needs the runway.
- A **migration** entry gives the concrete steps to move across a breaking change:
  the command to run, the config to rewrite, what to check. If the tool ships a
  migrator (e.g. `tool profiles migrate`), name it.
- A **Removed** entry records what is gone and points to the migration.

## Boundaries (what stays out)

- **Not commit messages.** A changelog entry is user-facing and reason-free in the
  upgrade sense; the *why* of an individual change lives in the commit
  (`git-commit`) and, for decisions, in an ADR. Do not paste commit subjects.
- **Not a spec or manual.** Behaviour is described once, in the spec/manual, in
  current-state terms; the changelog records the *transition*, not the behaviour.
- **Roadmap** beyond announced deprecations belongs in project planning, not here.
  "Planned for 2.0: X" is fine as a deprecation runway; a feature wishlist is not.

## Prose

- Apply `prose-precision`: name what changed concretely. "Various improvements and
  bug fixes" is the canonical worthless entry (P-1/P-4) — say which.
- User vocabulary, like the manual: describe the change as the user experiences
  it, not the internal refactor that produced it.

## Review checklist

- [ ] `[Unreleased]` section present and current
- [ ] Each entry is one user-facing line, not a commit subject, not "various fixes"
- [ ] Categories used correctly; Security changes called out explicitly
- [ ] Version bump matches the change class (major/minor/patch rule above)
- [ ] Deprecations name the existing replacement and the planned removal version
- [ ] Migrations give concrete steps / name the migrator tool
- [ ] No behaviour described here that should be in the spec/manual (CC-2)

## What this skill does not cover

- Current behaviour → the spec set and `user-manual`
- The reason a single change was made → `git-commit`; a decision → `architecture-decision-record`
- Document (not software) versioning and states → `spec-document-template`
- Prose, rendering → `prose-precision`, `asciidoc-conventions`

---
name: doc-code-sync
description: Use whenever the concern is keeping documentation current as code changes — preventing doc drift. Apply when deciding how a reference surface (CLI, exit codes, config schema, API) should be sourced; when adding a code-to-doc traceability link; when setting up CI gates that tie a code change to a doc or changelog update; or when someone asks "how do we stop the docs going stale?". Defines the generate-don't-duplicate principle, the `implements:`/`see:` code-to-doc citation convention and its resolver (docs-librarian check-links), golden-testing generated docs, and the changed-files CI gate. Ties together docs-librarian, spec-review-checklist, release-notes, and the run-all-checks gate.
---

# doc-code-sync

How to keep documentation current as code changes. The honest premise: you cannot
*guarantee* sync through good intentions — a "remember to update the docs" rule is
the weakest link in any process. You make drift either **impossible** (one source,
generate the other) or **loud** (a check fails the build). This skill is the
linkage layer between the doc skills and the coding skills.

## The ladder

Mechanisms ordered by strength. Prefer the highest rung that fits each fact.

1. **Generate — don't duplicate.** A fact in two places drifts. The exhaustive
   reference surfaces (`design-spec`, `interface-spec`) must be **generated from
   the code that owns them**, not hand-maintained:
   - CLI flags, subcommands, exit codes ← emit from the argument parser
     (`clap` markdown/`--help`; argparse; a single exit-code enum → table).
   - API reference ← rustdoc / doxygen; the doc *links to or embeds* the generated
     output rather than restating signatures.
   - Config / wire schema ← the CDDL, JSON Schema, or serde types are the source;
     generate the reference table from them, or validate code against them.
   - JSON output ← generate from the serialised types.
   Drift is *impossible* for a generated artifact. This is the highest-leverage
   move and usually the missing one.

2. **Golden-test the generated artifact.** Generation only prevents drift if
   regeneration is checked. Commit the generated table/help/schema as a golden
   file; a test regenerates and diffs. A code change to the surface fails the
   golden test until the doc is regenerated. This turns rung 1 from aspiration
   into a gate.

3. **Machine-checkable code→doc citations.** Where generation does not apply,
   link the code to the doc and check the link resolves (see "Citations" below).
   A dangling citation = drift, caught in CI by `docs-librarian check-links`.

4. **Executable doc examples.** Examples that *run* cannot silently rot:
   rustdoc doctests (RS-82), validating every config snippet in a spec through the
   real validator, running `interface-spec` test vectors as golden tests, and
   replay/golden regression for protocols. Mandate examples that are tests, not
   prose.

5. **Changed-files CI gate.** Coarse but effective: a change under a watched path
   requires a corresponding doc or changelog touch in the same change set (see
   "CI gate"). Catches the surfaces rungs 1–4 do not cover.

6. **Review checklist (the glue, weakest).** `spec-review-checklist` §10
   fact-checking and §6 traceability. Necessary, but every fact you can push up
   the ladder is one fewer thing a tired reviewer must catch.

And the structural prerequisite, already enforced elsewhere: **keep each document
at its content-class altitude.** An architecture spec that obeys `architecture-spec`
AC-1/AC-2 — contracts and structure, no flag tables, no crate names — does not
drift when a flag is added, because the flag was never in it. The documents that
rot are the braided ones; `doc-content-class` is itself a drift-prevention skill.

## Citations: linking code to the doc it realises

A source file cites the document it implements, in a comment near the code:

```
// implements: REQ-FWU-014        // a requirement obligation
// see: DEV-TOOLS-ADR-0027        // the decision behind this structure
// implements: DEV-TOOLS-DES-0010 // the design/interface it realises
// satisfies: REQ-NET-003
```

- Keywords: `implements:` / `satisfies:` / `see:` / `ref:`, followed by a
  document or requirement ID.
- Accepted ID forms: full `DEV-TOOLS-<CLASS>-NNNN`, `ADR-NNNN`, or a requirement
  `REQ-AREA-NNN` / `AREA-NNN` (the legacy form, per `spec-writing-style`).
- Place a citation at the implementing site for any code whose correctness is
  defined by a requirement, a structural decision (ADR), or an interface contract.
  Not every function — the ones a reviewer would otherwise have to trace by hand.

This makes traceability **bidirectional**: `spec-review-checklist` §6 already
requires docs to trace *upward*; citations let code trace *to the doc*, and the
resolver checks both directions stay connected.

## The resolver: `docs-librarian check-links`

The `docs-librarian` tool gained a `check-links` verb (see that skill):

```
docs-librarian check-links   # scan source, resolve every citation against _index.yaml
```

- Scans `.rs/.c/.h/.py/.sh` files (skipping `target/`, `node_modules`, build dirs)
  for citations and resolves each ID against `docs/_index.yaml`.
- A citation naming an absent or retired document is a **dangling reference** —
  exit non-zero, with `file:line`. This is the drift signal: code points at a doc
  that was renamed, retired, or never existed.
- Runs inside `audit`, and standalone in CI. Limit: it confirms the *document*
  exists, not that an individual requirement ID within it does — the index tracks
  documents, not per-requirement registries (a known boundary, documented in the
  tool).

## CI gate: tie a code change to a doc/changelog touch

In CI, fail a change set that touches a watched surface without a matching doc or
changelog update:

- A change under the CLI / public-API / format paths requires either a touch to
  the owning reference doc **or** a `release-notes` `[Unreleased]` entry.
- Every user-facing change requires a `CHANGELOG` `[Unreleased]` entry (the
  enforcement half of the `release-notes` skill).
- Implement as a path-keyed check in `run-all-checks.sh` (a `git diff --name-only`
  against the base, matched to required-doc globs).

## Where this runs

All of the above lands in one gate, `run-all-checks.sh`, so drift fails a build
rather than waiting for a reviewer:

```
docs-librarian validate        # index invariants
docs-librarian check-links     # code->doc citations resolve
<generated-doc golden tests>   # rung 2
<doctests / example validation># rung 4
<changed-files doc gate>       # rung 5
```

See the `run-all-checks.sh` companion in the repo root (scaffolded from this
skill set) for the assembled gate.

## Review checklist

- [ ] Every reference surface (CLI, exit codes, schema, API) is generated, not
      hand-maintained — or a golden test guards the hand-maintained copy
- [ ] Generated docs have a regenerate-and-diff golden test (rung 2)
- [ ] Code implementing a requirement/ADR/interface carries an `implements:`/`see:`
      citation; `docs-librarian check-links` is clean
- [ ] Spec/manual examples are executable (doctests, validated snippets, vectors)
- [ ] CI fails a watched-path change with no doc/changelog touch
- [ ] Documents sit at their content-class altitude (least drift-prone, by construction)

## What this skill does not cover

- Which content belongs in which document (the altitude that minimises drift) →
  `doc-content-class` and the type skills
- The index, ID allocation, and the `check-links` resolver itself → `docs-librarian`
- Where deltas/deprecations are recorded → `release-notes`
- The fact-checking review pass → `spec-review-checklist` §6, §10
- Doctest/determinism rules in code → `rust-coding-rules` RS-82/RS-80,
  `python-coding-rules` PY-34, `test-writing-rules`

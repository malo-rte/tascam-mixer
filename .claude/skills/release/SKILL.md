---
name: release
description: Use whenever cutting a release of the software — stepping the version, finalizing the changelog, running the full gate, tagging, and building artifacts. Encodes the project's unified-versioning policy: every tool in the repo shares one version and they are all stepped together on release, from a single canonical source. Apply when asked to "cut a release", "bump the version", or "ship version X". Uses the bundled bump-version tool, and pairs with release-notes (changelog/semver), run-all-checks.sh (the gate), and the /release slash command.
---

# release

How to cut a release. The procedure is mechanical and gated; the version policy
is the part that needs stating clearly.

## Unified versioning (the policy)

**Every tool in the repo shares one version, and they are all stepped together on
release.** There is no per-tool version. This means:

- **One canonical source of the version.** The repo holds the version in a single
  place — a root `VERSION` file, or `[workspace.package] version` in the root
  `Cargo.toml`. Everything else *derives* from it.
- **Every manifest inherits or is stamped from it.** Cargo workspace members use
  `version.workspace = true` (never a literal per-crate version). Other manifests
  (`pyproject.toml` `[project].version`, a C version header, a packaging spec) are
  stamped to the same value by the release tool.
- **Coherence is enforced, not trusted.** `bump-version check` verifies every
  manifest equals the canonical version and fails CI on any mismatch — the
  code-side analogue of the document version-integrity invariant
  (`spec-document-template`). It runs inside `run-all-checks.sh`.
- **The version format** follows semver (`MAJOR.MINOR.PATCH`); the git tag is
  `vMAJOR.MINOR.PATCH`. The bump level is decided by the change content per the
  rule in `release-notes`.

**Documents are not the tool version — with two deliberate exceptions.** Specs
(REQ/ARCH/DES/interface) and ADRs version on their own review lifecycle
(`spec-document-template`), independent of the release; they are often ahead of
the code. The exceptions:

- A **user manual** documents the shipped software, so it carries
  `:applies-to-version:` — the tool version it describes. `bump-version` treats
  any doc with that attribute as a version manifest, stamps it on release, and
  fails the gate if it lags (`user-manual`).
- A **release manifest** records the whole doc set's versions *as of* the release,
  without renumbering anything: `docs-librarian release-manifest X.Y.Z` snapshots
  every non-retired doc's id + version + status to `docs/releases/X.Y.Z.yaml`.
  This is how you answer "which doc versions shipped with release X.Y.Z" while
  keeping spec versions independent.

## Release procedure

1. **Pre-flight.** Working tree clean; on the release branch; the full gate is
   green: `run-all-checks.sh` (this also confirms current version coherence and
   that docs are in sync — `doc-code-sync`).
2. **Decide the bump.** Read the `[Unreleased]` changelog section. A
   backward-incompatible change to any public contract → **major**; a new
   backward-compatible capability → **minor**; fixes only → **patch**
   (`release-notes` decision rule). Because the version is unified, the bump is
   the *maximum* across all tools' changes — one breaking change anywhere makes
   the whole release major.
3. **Finalize the changelog.** Move `[Unreleased]` to `[X.Y.Z] - <date>`, ensure
   each entry is a user-facing line, Security changes called out (`release-notes`).
4. **Step the version everywhere.** `bump-version <major|minor|patch>` (or
   `bump-version set X.Y.Z`) writes the canonical source and stamps every manifest
   — including each manual's `:applies-to-version:`; then `bump-version check`
   confirms they all agree. Spec/ADR `:doc-version:` values are untouched.
5. **Snapshot the doc set.** `docs-librarian release-manifest X.Y.Z` writes
   `docs/releases/X.Y.Z.yaml` recording every document's id + version + status at
   this release.
6. **Re-run the gate.** `run-all-checks.sh` again — now including version
   coherence, with the changelog, manifests, and manuals updated.
7. **Commit and tag.** One commit, `Release X.Y.Z` (`git-commit`); annotated tag
   `vX.Y.Z`. The commit contains the version bump, the changelog finalization, the
   release manifest, and nothing else.
8. **Build artifacts.** Build all tools reproducibly (the build image,
   `dev-container`); they all carry version `X.Y.Z`.
9. **Open the next cycle.** Add a fresh empty `[Unreleased]` section to the
   changelog.

## The tool

`bump-version` lives at `.claude/skills/release/scripts/bump-version` (Python).

```bash
bump-version check                 # verify every manifest == canonical version (CI)
bump-version current               # print the canonical version
bump-version <major|minor|patch>   # step and stamp everywhere
bump-version set X.Y.Z             # set an explicit version everywhere
```

`check` is wired into `run-all-checks.sh`. The tool knows the common manifest
shapes (VERSION file, Cargo `[workspace.package]`, `pyproject [project]`); add any
project-specific manifest (a C header macro, a Yocto recipe `PV`) to its
`MANIFESTS` table.

## Hard rules

- Never hand-edit a version in one manifest — that breaks coherence and
  `bump-version check` will fail the build. Always go through the tool.
- Never give two tools different versions in one repo. If two things genuinely
  version independently, they belong in different repos.
- A release commit changes only version + changelog; no logic
  (`git-commit` one-logical-change).
- Don't release on a red gate. `run-all-checks.sh` must pass first.

## What this skill does not cover

- Changelog structure and the semver decision rule → `release-notes`
- The checks the gate runs → `run-all-checks.sh`, `doc-code-sync`,
  `code-review`, `security-review`
- Commit and tag message form → `git-commit`
- Version/tag identifier format → `identifier-conventions`
- The build image that produces artifacts → `dev-container`

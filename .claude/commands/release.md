---
description: Cut a release — step the unified version, finalize the changelog, gate, tag
argument-hint: "<major|minor|patch> or X.Y.Z"
allowed-tools: Bash(git status:*), Bash(git diff:*), Bash(git log:*), Bash(git tag:*), Bash(git add:*), Bash(git commit:*), Bash(./run-all-checks.sh), Bash(.claude/skills/release/scripts/bump-version:*), Bash(.claude/skills/docs-librarian/scripts/docs-librarian:*), Bash(python3:*), Read, Edit
---

Apply the `release` skill to cut a release. Bump argument: $ARGUMENTS

## Context
- Status: !`git status --short`
- Current version: !`python3 .claude/skills/release/scripts/bump-version current 2>&1 || true`
- Version coherence: !`python3 .claude/skills/release/scripts/bump-version check 2>&1 || true`
- Unreleased changelog: !`sed -n '/## \[Unreleased\]/,/## \[/p' CHANGELOG.md 2>/dev/null || echo "(no CHANGELOG.md found)"`

## Task
Follow the `release` procedure. Do **not** skip the gate.

1. **Pre-flight.** Confirm the working tree is clean and version coherence above is
   green. Run `./run-all-checks.sh`; if it fails, stop and report — do not release
   on a red gate.
2. **Decide the bump.** If `$ARGUMENTS` names a level or version, use it. Otherwise
   derive it from the `[Unreleased]` section per the `release-notes` rule —
   remembering the version is unified, so one breaking change anywhere makes the
   whole release **major**. State the chosen version and why.
3. **Finalize the changelog.** Move `[Unreleased]` to `[X.Y.Z] - <today>`; confirm
   entries are user-facing and Security changes are called out.
4. **Step the version everywhere.** Run
   `bump-version <level>` (or `bump-version set X.Y.Z`), then `bump-version check`
   to confirm every manifest agrees — this stamps the code manifests **and** each
   manual's `:applies-to-version:`, leaving spec/ADR `:doc-version:` untouched.
   Never hand-edit a single manifest.
5. **Snapshot the doc set.** `docs-librarian release-manifest X.Y.Z` →
   `docs/releases/X.Y.Z.yaml`, recording every document's version + status at this
   release.
6. **Re-run the gate.** `./run-all-checks.sh` again with the bump applied.
7. **Commit & tag.** One commit `Release X.Y.Z` (version + changelog + release
   manifest only, per `git-commit`); annotated tag `vX.Y.Z`.
8. **Open the next cycle.** Add a fresh empty `[Unreleased]` section.

Report each step's result. Pause for confirmation before the commit and tag if
anything in pre-flight or the gate was not clean.

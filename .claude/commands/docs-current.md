---
description: Check that documentation is up to date with the code
argument-hint: "[base ref, default: origin/main]"
allowed-tools: Bash(git diff:*), Bash(git status:*), Bash(.claude/skills/docs-librarian/scripts/docs-librarian:*), Bash(python3:*), Read, Grep, Glob
---

Apply the `doc-code-sync` discipline to confirm the docs match the code.

## Context
- Base ref: ${ARGUMENTS:-origin/main}
- Changed files: !`git diff --name-only ${ARGUMENTS:-origin/main}...HEAD`
- Index invariants: !`python3 .claude/skills/docs-librarian/scripts/docs-librarian validate 2>&1 || true`
- Citation resolver: !`python3 .claude/skills/docs-librarian/scripts/docs-librarian check-links 2>&1 || true`

## Task
Run the documentation-currency pass (`doc-code-sync`):

1. Report the `docs-librarian validate` and `check-links` results above — any
   dangling `implements:`/`see:` citation or index violation is a drift finding.
2. For each changed code surface (CLI, exit codes, config/wire schema, public
   API), check the owning reference doc was updated, or is generated + golden-
   tested. A hand-maintained reference whose source changed with no doc touch is a
   **MAJOR** drift finding (`doc-code-sync` rung 1–2).
3. Check that any user-facing change has a `release-notes` `[Unreleased]` entry.
4. Spot-check `spec-review-checklist` §10: do cited versions, flags, and signatures
   in the docs match what the code now ships?

Report findings as relocate/update actions (which doc, which fact), and file
follow-ups as `DEV-TOOLS-TASK-NNNN`. Prefer pushing fixes up the ladder (generate
+ golden-test) over manual updates. Do not edit docs unless asked.

---
description: Review the current change against the project's code standards
argument-hint: "[base ref, default: origin/main]"
allowed-tools: Bash(git diff:*), Bash(git status:*), Bash(git log:*), Bash(git merge-base:*), Read, Grep, Glob
---

Apply the `code-review` skill to the current change.

## Context
- Base ref: ${ARGUMENTS:-origin/main}
- Status: !`git status --short`
- Changed files: !`git diff --name-only ${ARGUMENTS:-origin/main}...HEAD`
- Diff: !`git diff ${ARGUMENTS:-origin/main}...HEAD`

## Task
Run the `code-review` pass on the diff above:

1. Read the change's intent first (commit messages, any linked
   `DEV-TOOLS-TASK-NNNN` / requirement). Scope the review to the changed lines and
   what they touch — do not audit the whole tree.
2. Walk the seven concerns in order (correctness/errors, structure/design,
   safety, tests, docs & traceability, comments/naming, commit hygiene), deferring
   to the language skill for each rule and citing the rule ID in every finding.
3. Skip anything `run-all-checks.sh` already enforces (fmt/lint/types) — flag it
   only as "wire into CI", not as review content.
4. Group findings by file with severity `[MAJOR]/[MINOR]/[NIT]`. File real
   out-of-scope findings as tasks via `task-from-sources`.

End with a disposition (approve / approve-with-nits / changes-requested) and the
list of follow-up `DEV-TOOLS-TASK-NNNN` filed. Do not modify code.

---
description: Security-review the current change against the threat model and security rules
argument-hint: "[base ref, default: origin/main]"
allowed-tools: Bash(git diff:*), Bash(git status:*), Bash(git log:*), Read, Grep, Glob, Bash(cargo deny:*)
---

Apply the `security-review` skill to the current change.

## Context
- Base ref: ${ARGUMENTS:-origin/main}
- Changed files: !`git diff --name-only ${ARGUMENTS:-origin/main}...HEAD`
- Diff: !`git diff ${ARGUMENTS:-origin/main}...HEAD`

## Task
Run the `security-review` pass:

1. First identify what the change exposes — assets touched, trust boundaries
   crossed, untrusted input parsed (`threat-model` vocabulary). If it crosses a
   boundary or touches an asset with **no threat model**, that is finding #1:
   require/extend a `threat-model` before this merges.
2. Walk the nine checks (mitigations trace to requirements, secret handling, input
   validation, memory/unsafe, process/FFI, crypto usage, anti-rollback, audit
   logging, supply chain). Cite the threat (`THR-NNN`) and the rule for each
   finding.
3. If dependencies changed, run `cargo deny check` and report advisories/license
   issues.
4. Group findings by asset/boundary with severity. Mark explicitly accepted risks
   as such (with rationale), don't pass them silently.

End with: block / allow / allow-with-follow-ups, the `threat-model` updates
required, and any `DEV-TOOLS-TASK-NNNN` filed. Do not modify code.

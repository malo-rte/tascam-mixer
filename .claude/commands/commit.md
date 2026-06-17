---
description: Stage changes and write a commit message following the project rules
argument-hint: "[optional intent / task ref, e.g. DEV-TOOLS-TASK-0042]"
allowed-tools: Bash(git status:*), Bash(git diff:*), Bash(git add:*), Bash(git commit:*), Bash(git log:*), Read
---

Create a commit following the `git-commit` skill. Intent/refs: $ARGUMENTS

## Context
- Status: !`git status --short`
- Staged diff: !`git diff --cached`
- Unstaged diff: !`git diff`
- Recent subjects (style reference): !`git log --oneline -5`

## Task
1. If nothing is staged, decide what to stage. **One logical change per commit** —
   if the working tree mixes unrelated changes (or whitespace with logic), stage
   only one logical change and say so; suggest the rest as follow-up commits.
   Don't bundle.
2. Write the message per `git-commit`:
   - Imperative, capitalized subject ≤ 50 chars, no trailing period, optional
     consistent scope prefix (`kernel:`, `recipe:`).
   - Blank line, then a body wrapped at 72 explaining **why**, not what/how.
   - Flag any breaking change with a `BREAKING:` line.
   - Reference the task/document in a trailer: `Refs: DEV-TOOLS-TASK-NNNN` (use the
     ref in `$ARGUMENTS` if given; otherwise infer from the branch or the linked
     task, or omit if there is none).
   - No project phases/milestones; ASCII only.
3. Show the proposed message and the exact `git add` / `git commit` you will run,
   then commit. If `run-all-checks.sh` exists and the change is non-trivial, note
   that it should pass first — don't commit on a known-red gate.

---
name: git-commit
description: Use whenever writing or revising a git commit message, splitting a change into commits, or reviewing commit messages in a branch or pull request. Encodes the project commit rules — subject in imperative mood, capitalized, at most 50 chars with no trailing period, body wrapped at 72 chars explaining why (not what or how), one logical change per commit, breaking changes flagged in the body, issues/tasks referenced in trailers, consistent optional scope prefixes, whitespace separated from logic, and ASCII only. Apply this any time a commit message is being authored or judged, when staging work into commits, or when someone asks for a commit message — not only when "commit rules" are named explicitly.
---

# git-commit

A commit message records *why* a change was made, for someone reading `git log`
or `git blame` long after the diff stops being self-explanatory. The subject says
what changed; the body says why. These are the project's commit rules.

## Content

- Describe **only what the commit is changing** — not how it's changed.
- The **body explains why**, not what or how. The diff already shows what and how;
  the prose exists to capture the reason the change exists.
- **One logical change per commit.** If the subject needs an "and", split it.
- **Don't mix whitespace/formatting changes with logic changes** — make those a
  separate commit so the logic diff stays readable.
- **Flag breaking changes explicitly**, e.g. a `BREAKING:` line in the body.
- No project phases, steps, or milestones.

## Form

- **Imperative mood**: "Add", "Fix" — not "Added", "Fixes". (It completes "this
  commit will…".)
- **Subject <= 50 characters**, **capitalized**, with **no trailing period**.
- **Blank line** between subject and body.
- **Body wrapped at 72 characters.**
- **Scope prefix optional but consistent**, e.g. `kernel:`, `recipe:`. Capitalize
  the description after the prefix.

## Don'ts

- No `WIP`, `misc`, `stuff`, or filler subjects.
- **Reference issues/tickets in a trailer**, not the subject — e.g. `Refs: #123`.
  In this project, reference the task or document the same way: `Refs: DEV-TOOLS-TASK-0042`,
  `Refs: DEV-TOOLS-DES-0010`. This ties the commit back to the combined task list.
- **No non-ASCII characters** in commit messages.

## Template

```
<scope>: <Imperative, capitalized subject, <=50 chars>

<Why this change exists, wrapped at 72 chars. What problem it solves
or what regression it prevents. Not a restatement of the diff.>

BREAKING: <what breaks and what callers must do>   # only if applicable
Refs: DEV-TOOLS-TASK-0042
```

## Examples

**Good:**
```
recipe: Pin u-boot to 2024.01 for ZynqMP

The 2024.04 bump regressed TFTP PHY warmup timing on Kria and caused
intermittent boot failures. Pin until the PHY init fix lands upstream.

Refs: DEV-TOOLS-TASK-0042
```

**Good (no body needed for a trivial, self-evident change):**
```
docs: Fix typo in boot sequence diagram
```

**Bad → why:**
```
Fixed stuff                  # filler, past tense, not capitalized as a subject
Update files.                # vague, trailing period
WIP                          # filler
kernel: add driver and fix unrelated formatting   # two logical changes; past/lowercase
Add support for #123         # ticket in subject instead of a trailer
```

## Splitting work into commits

Stage by logical change, not by file or by session. A feature plus a formatting
sweep is two commits; a refactor plus a behavior change is two commits. This lines
up with one task = one outcome (see `task-capture`): a single task often maps to
one or a few logical commits, each referencing the task in a `Refs:` trailer.

## Reviewing commit messages

Check each commit: subject is imperative, capitalized, <=50 chars, no period;
blank line before a 72-char-wrapped body; body gives the why; exactly one logical
change; whitespace not mixed with logic; breaking changes flagged; tickets/tasks
in trailers not the subject; ASCII only; no filler subjects.

## Scope note

These rules cover commit *messages*. Other git-workflow conventions (branching,
rebasing, PR hygiene) are out of scope here until added — extend this skill or add
a sibling if you want them codified.

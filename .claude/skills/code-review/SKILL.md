---
name: code-review
description: Use whenever reviewing a code change, a pull request, or a module against the project's standards — or self-reviewing a diff before requesting review. The code-side sibling of spec-review-checklist: it does not restate the language rules but orchestrates them into an ordered review pass with a severity taxonomy, diff scoping, and the rule that unfixable findings become tasks. Apply when asked to "review this code / PR / diff", before opening a PR, or when judging a change for merge. Pairs with the /code-review slash command.
---

# code-review

How to review a code change against the project's standards. This skill is the
*procedure*; the *rules* live in the coding skills and are applied, not repeated.
It is the code-side counterpart of `spec-review-checklist`.

## Scope the review

Match effort to the change, like `spec-review-checklist`'s depth-by-state:

| Subject | Review |
|---------|--------|
| A diff / PR | Only the changed lines and what they touch. Don't audit the whole tree. |
| A new module | The module's structure, public surface, tests, and docs. |
| A pre-merge self-review | Run the checklist below before requesting review — it saves a round trip. |

Start from the diff: `git diff <base>...HEAD`. Read the change's intent (the PR
description, the linked task/requirement) before judging the lines.

## Severity taxonomy

Same as `spec-review-checklist`:

- **MAJOR** — blocks merge: a correctness bug, a safety violation, an untested
  behaviour change, a security gap, a broken contract.
- **MINOR** — should be fixed before merge: structure, naming, missing edge case.
- **NIT** — optional: formatting (should already be tool-enforced), wording.

Findings that are real but out of scope for this change become **tasks**
(`task-from-sources` → `DEV-TOOLS-TASK-NNNN`), not merge blockers.

## The pass

Run in order; defer to the named skill for the actual rule. A finding cites the
rule ID (e.g. `RS-10`, `D2`, `TST-17`) so the author can look it up.

1. **Correctness & errors** — does it do what the change claims? Error paths
   handled, not swallowed (`rust-coding-rules` RS-10/11/12, `python-coding-rules`
   PY-20/21, `c23-coding-rules`). No `unwrap`/`expect`/`panic` on runtime paths.
2. **Structure & design** — does logic sit in the right layer; is the public
   surface minimal; no new global state; functions cohesive (`software-design-rules`
   D-NN, `rust-coding-rules` RS-24/60, `python-coding-rules` PY-30/42). Would the
   change be better as a smaller, composable piece?
3. **Safety** — `unsafe` minimal with `// SAFETY:` and encapsulated (RS-30/31);
   no `transmute` tricks (RS-32); newtypes over primitives where it matters
   (RS-21/PY-11); no `shell=True` / unquoted expansion (PY-32, `shell-coding-rules`
   SH-10/25).
4. **Tests** — does the change ship with tests in the same logical commit
   (`test-writing-rules` TST-29); are error paths and edge cases covered (TST-17);
   are tests deterministic (TST-8) and the public contract tested, not internals
   (TST-15)? A behaviour change with no test is **MAJOR**.
5. **Docs & traceability** — are the docs for any changed surface updated
   (`doc-code-sync`); do `implements:`/`see:` citations resolve
   (`docs-librarian check-links`); is there a `release-notes` `[Unreleased]` entry
   for a user-facing change? See the `/docs-current` flow.
6. **Comments & naming** — comments say *why*, not *what* (`code-comments`);
   idiomatic naming (RS-91/PY); `TODO`s carry a task ID
   (`DEV-TOOLS-TASK-NNNN`, RS-92/PY-51/SH-50).
7. **Commit hygiene** — one logical change per commit, imperative subject, *why*
   in the body, task referenced in a trailer (`git-commit`).

## What tooling should have caught already

Formatting, lint, and the mechanical rules run in `run-all-checks.sh`
(fmt/clippy/ruff/mypy/shellcheck + `docs-librarian`). If a review is spending
effort on those, wire them into CI instead — a human reviewer's attention is for
correctness, design, and security, which tools cannot judge. Treat a tooling-level
NIT as a signal the gate is not running, not as review content.

## Output

Group findings by file/area, not by severity, so the author addresses them in
order. Each finding:

```
[MAJOR] src/session.rs:142 (RS-10) — unwrap() on the reconnect path; a dropped
   port panics the session. Return the error and let the caller decide.
```

End with the disposition: approve / approve-with-nits / changes-requested, and
the list of follow-up tasks filed.

## What this skill does not cover

- The language rules themselves → `rust-coding-rules`, `c23-coding-rules`,
  `python-coding-rules`, `shell-coding-rules`, `software-design-rules`
- Test rules → `test-writing-rules`. Comment/commit rules → `code-comments`, `git-commit`
- Security-specific review → `security-review`
- Documentation currency → `doc-code-sync` and the `/docs-current` flow
- Reviewing a specification (not code) → `spec-review-checklist`

---
name: code-comments
description: Use whenever writing, reviewing, or cleaning up comments in code, scripts, or any file that takes comments (config, build files, device trees, Dockerfiles). Apply when adding doc comments to a function or public API, deciding whether a comment is worth keeping, removing redundant or commented-out code, writing TODO/FIXME markers, or reviewing a diff for comment quality. Encodes the project comment rules — document why not what, comment only the non-obvious, note units/ranges/invariants, no commented-out code or change-log noise, ASCII only, and idiomatic per-language doc style. Apply this any time comments are being authored or judged, not just when explicitly asked to "comment the code".
---

# code-comments

Comments earn their place by saying something the code cannot. The code already
shows *what* it does; a comment exists to explain *why*, to warn, or to record a
constraint a reader would otherwise have to rediscover. These are the project's
comment rules; apply them when writing or reviewing comments in any language or
file type.

## Scope and intent

- Document **why, not what** — the code shows what. A comment that restates the
  code is noise.
- Explain **what a function is supposed to do, not how** it does it. The body is
  the *how*; the doc comment is the contract.
- Inside a function, comment **only what isn't easy to figure out** from reading
  it. Obvious code needs no narration.
- Comment **non-obvious constraints, invariants, and assumptions** — the things
  that will break silently if a future reader violates them.
- Note **units, ranges, and edge cases** for parameters, e.g.
  `timeout in ms, 0 = no timeout`.
- Flag **known limitations, footguns, or surprising behavior** so the next reader
  isn't ambushed.
- **Link to the source** for non-trivial algorithms, RFCs, datasheets, or bug
  reports, so the rationale is reachable.
- No project phases, steps, or milestones in comments.

## What not to write

- **No commented-out code.** Delete it — git remembers.
- **No redundant comments** restating the code (`i++ // increment i`).
- **No author names, change logs, or dates** — that is version control's job.
- **No decorative banners or ASCII-art dividers.**
- **Don't apologize** in comments ("hacky", "sorry"). Either fix it, or explain
  the constraint that forces it.

## Style and hygiene

- Prefer **better names and smaller functions** over explanatory comments. The
  best comment is often the one a clearer name makes unnecessary.
- **Public APIs get doc comments; internals get comments only where needed.**
- Keep comments **truthful** — an outdated comment is worse than none, because it
  actively misleads. When you change code, update or delete its comment.
- Use **`TODO`/`FIXME`/`XXX` with an owner or a ticket**, never as a graveyard.
  In this project, tag them with the task ID so `task-from-sources` can harvest
  them, e.g. `// TODO(DEV-TOOLS-TASK-0042): widen the timeout once the PHY fix lands`.
- Match the **language's idiomatic doc style**: rustdoc `///`, kerneldoc
  `/** */`, Python docstrings, etc.
- Use **full sentences with punctuation** for anything longer than a short note.
- Keep **line length consistent** with the surrounding code.
- **ASCII only** — no non-ASCII characters in comments. (Consistent with the
  project's text conventions; cf. `asciidoc-conventions`.)

## Examples

**Redundant — delete it:**
```c
i++;  // increment i
```

**Useful — explains a non-obvious constraint:**
```c
/* PHY needs ~20 ms after reset deassert before MDIO reads are valid;
 * polling earlier returns 0xffff. See datasheet sec. 4.3. */
mdelay(20);
```

**Good doc comment — contract, units, edge case:**
```rust
/// Wait for the link to come up.
///
/// `timeout_ms` is the deadline in milliseconds; 0 means wait forever.
/// Returns `Err(LinkTimeout)` if the link is still down at the deadline.
pub fn wait_for_link(timeout_ms: u32) -> Result<(), LinkError> {
```

**Apology — fix or explain instead:**
```c
// hacky, sorry            <-- no
// QEMU models the FIFO depth as 1, so we must drain between writes;
// hardware does not need this. <-- yes
```

## Reviewing comments in a diff

Check: every comment says something the code doesn't; no commented-out code; no
change-log/author noise; doc comments on new public APIs; units/ranges noted on
parameters; comments match the code they sit on (no stale claims); TODO/FIXME
carry an owner or task ID; ASCII only.

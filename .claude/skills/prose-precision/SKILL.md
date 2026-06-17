---
name: prose-precision
description: Use whenever writing or editing prose in any technical document — requirement, architecture, design specs, manuals, design notes, READMEs — to keep language concrete and precise. Catches the abstract-language constructs that vague-adjective lists miss: empty verbs (handle, support, manage, process, facilitate, leverage), nominalizations (zombie nouns), abstract subjects (the system, functionality, the mechanism), hedged intent (aims to, is designed to, tries to), vague quantifiers (several, various, a number of), ambiguous pronouns, and filler/Latinate bloat (in order to, prior to, utilize). Apply on every prose pass across all document types, alongside spec-writing-style. Rules use stable IDs P-NN.
---

# prose-precision

Make every sentence name a specific actor, action, and object. This is the
shared precision layer beneath all document types — a peer of `doc-content-class`,
used by `requirement-spec`, `architecture-spec`, `design-spec`, and `user-manual`.
It applies to **all** prose, normative and explanatory alike — not only to
requirements.

It pairs with `spec-writing-style`'s weasel-word table without overlapping it (see
"Division of labour" below): that table owns vague *adjectives and approximations*
(fast, efficient, robust, roughly, typically); this skill owns vague *verbs,
nouns, subjects, and constructions*.

## The precision test

For any sentence, ask:

> Can a reader name the specific actor, the specific action, and the specific
> object — or is one of them a placeholder?

If the verb is "handles", the subject is "the system", or the object is
"functionality", you have a placeholder. Replace it with the thing it stands for.
If you cannot, you do not yet understand the sentence well enough to write it.

## Precision is not lowering altitude

Precision means saying exactly what you mean *at the chosen altitude* — not
dragging architecture prose down into implementation detail. "The session layer
coordinates cross-connection state" is precise at architecture altitude. Naming a
ring buffer or a crate to sound concrete would violate the content-class rules
(`doc-content-class` CC-4, `architecture-spec` AC-2). Fix vague *language*; keep
the *altitude* the document type demands.

## Banned constructs

Stable IDs `P-NN`. Each is a rewrite, not a deletion.

### P-1 — Empty verbs

`handle`, `support`, `manage`, `process`, `deal with`, `facilitate`, `leverage`,
`utilize`, `provide`, `enable`, `perform`. They name a category of action, not the
action. Replace with the specific verb.

| Vague | Precise |
|-------|---------|
| The agent handles errors. | The agent logs the failure and aborts the update. |
| The transport supports reconnection. | On drop, the transport re-dials and replays the buffered scrollback. |
| The session manages connections. | The session opens, names, and tears down each connection. |
| The tool provides logging. | The tool writes each line to the log file as it arrives. |

### P-2 — Nominalizations (zombie nouns)

A verb buried inside a noun, usually behind "performs … of", "is responsible for
the … of", or an `-ion`/`-ment` ending. Turn the noun back into a verb.

| Vague | Precise |
|-------|---------|
| performs validation of the bundle | validates the bundle |
| is responsible for the coordination of state | coordinates state |
| provides configuration of the baud rate | sets the baud rate |
| does the management of the buffer | evicts the oldest lines when the buffer is full |

### P-3 — Abstract subjects

`the system`, `the software`, `functionality`, `the mechanism`, `the capability`,
`the solution`, `the component`, `the module` (when a named one is meant). Name
the actor. Exception: "the system" when it is the document's defined SUT
(consistent with `spec-writing-style`).

| Vague | Precise |
|-------|---------|
| The system writes the log entry. | The trigger engine writes the log entry. |
| Functionality exists to detect panics. | A panic trigger matches `Kernel panic` and dumps the buffer. |
| The mechanism resolves the secret. | The provider chain resolves the secret, first match wins. |

### P-4 — Vague quantifiers

`several`, `various`, `multiple`, `a number of`, `some`, `many`, `a few`. Give a
count or enumerate. (Approximation words — `roughly`, `typically` — belong to
`spec-writing-style`'s weasel table, not here.)

| Vague | Precise |
|-------|---------|
| supports several transports | supports five transports: serial, rfc2217, ssh, subprocess, unix-socket |
| evicts a number of lines | evicts oldest lines until both limits hold |
| various failure modes | the three failure modes in <<sec-failures>> |

### P-5 — Hedged intent

`aims to`, `is intended to`, `is designed to`, `seeks to`, `strives to`. These
describe an unfalsifiable goal instead of a behaviour. State what it does.

| Vague | Precise |
|-------|---------|
| aims to minimise latency | adds at most one buffer copy on the read path |
| is designed to be secure | encrypts the channel with the active ROTPK key |
| is intended to handle reboots | re-detects the boot phase after each reconnect |

Distinct from describing a genuinely *fallible operation*: "attempts to acquire
the lock; on failure, exits 30" is precise — the operation can fail and the
failure path is stated. The ban is on hedging design *intent*, not on naming
operations that can fail.

### P-6 — Ambiguous pronouns

A pronoun (`it`, `this`, `that`, `they`) must have exactly one possible antecedent
in the preceding clause. A bare `this`/`that` as subject must carry its noun.

| Ambiguous | Clear |
|-----------|-------|
| When the trigger fires the action, it is logged. | When the trigger fires the action, the engine logs the action. |
| This causes the session to end. | This drop causes the session to end. |

### P-7 — Filler and Latinate bloat

Shorter is clearer. Substitute or delete.

| Bloat | Plain |
|-------|-------|
| in order to | to |
| prior to / subsequent to | before / after |
| in the event that | if |
| due to the fact that | because |
| utilize | use |
| has the ability to / is able to | can |
| at this point in time | now |
| with respect to / in terms of | about / for (or delete) |
| the majority of | most |
| in the process of | while (or delete) |

## Worked example

Before (an architecture paragraph failing P-1, P-2, P-3, P-4, P-5, P-7 — and
`spec-writing-style`'s weasel rule on "robust"):

> The system is responsible for the management of various connections and aims to
> handle reconnection in a robust manner. In order to facilitate this, it
> leverages a number of internal mechanisms.

After:

> The session owns one or more named connections. On transport drop it re-dials
> and reinserts a boundary marker into the scrollback; after the configured retry
> budget is exhausted it surfaces exit 31.

Named actor (session), real verbs (owns, re-dials, reinserts, surfaces), counted
quantities (one or more, the configured budget), behaviour instead of intent, no
filler — and still at architecture altitude, with no crate or data-structure named.

## Division of labour

To avoid duplication (`doc-content-class` CC-2), the two prose skills split the
work:

| Construct | Owned by |
|-----------|----------|
| Vague adjectives / approximations (fast, efficient, robust, roughly, typically) | `spec-writing-style` (weasel words) |
| RFC 2119 keywords, requirement IDs, voice/tense/person, sentence shape | `spec-writing-style` |
| Empty verbs, nominalizations, abstract subjects, hedged intent, vague quantifiers, ambiguous pronouns, filler | `prose-precision` (this skill) |

Both skills apply to the same sentence; neither restates the other's table. A
phrase like "handles errors in a robust manner" trips P-1 (verb) here and the
weasel rule (adjective) there.

## Review hook

`spec-review-checklist` §3 (Language) runs P-1…P-7 against **all** prose, not only
requirements. A construct from the tables above is a MINOR finding (MAJOR if it
makes a normative clause untestable); the fix is the rewrite, cited by P-ID.

## What this skill does not cover

- Vague adjectives, RFC 2119, requirement IDs, sentence shape → `spec-writing-style`
- Which document a piece of content belongs in / the right altitude →
  `doc-content-class` and the type skills
- AsciiDoc rendering → `asciidoc-conventions`

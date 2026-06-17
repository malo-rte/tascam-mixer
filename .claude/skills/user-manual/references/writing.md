# Writing style and review

The manual is written for someone trying to *do something* with the software.
Every choice serves that reader.

## Non-normative prose (vs. spec-writing-style)

A manual *describes*; a spec *mandates*. Manual prose is non-normative: do **not**
use RFC 2119 keywords (shall/should/may/must) to state product behavior — that is
`spec-writing-style`'s job, not the manual's. Write "the service retries failed
uploads three times" (description), not "the service shall retry…". Reserve
imperative verbs for instructions *to the reader* ("Run `foo init`", "Set the
timeout"). Where the manual states a guarantee the user relies on, cross-reference
the governing requirement rather than asserting it normatively.

## Voice and stance

- Address the reader as "you" **only when giving them instructions or
  setting expectations** (how-to-read guidance, audience prereqs, ordered
  procedure steps, recipe sections, advice like "for a hermetic CI replay,
  design the trigger to..."). In **descriptive** prose -- explaining what
  the tool does, how a feature behaves, what an option means -- make the
  tool the subject: "bserial runs in two modes" beats "you drive bserial
  in two modes," "phases scope triggers to each state" beats "phases let
  you scope triggers." Descriptive prose addressed to the reader as "you"
  reads like a tutorial walkthrough or a sales pitch, neither of which
  the reader of a user manual wants.
- **Use the present tense for the software's behaviour, full stop.** The manual
  describes the version the reader is using right now -- not the previous
  version's behaviour and not the next version's plans. Past-tense ("X used
  to..."), historical ("M4 introduced phases", "rewritten in slice 11"), and
  future-tense ("will support Z", "planned for next release", "not yet
  implemented") prose belongs in the changelog, the roadmap, or commit
  messages, not in the manual. Current-state migration UX is the legitimate
  exception: "Run `tool profiles migrate` to convert older files" describes
  what the tool does now.
- Be direct and procedural. Prefer "Run `foo init` to create the workspace" over
  "The workspace can be created by means of the init subcommand."
- Describe what the user does and what happens as a result — not how the code
  accomplishes it. "The device flashes the status LED twice on a successful
  pairing," not "the pairing routine calls `led_blink()` in a loop."
- Don't sell or editorialize. No marketing adjectives. State capabilities plainly.
- **Prefer concrete words to vague engineering metaphors.** A word like "shape"
  reads natural in engineering chat (where it means whatever the conversation
  needs it to mean) but lands as jargon in a manual. The reader has to guess
  whether you meant mode, structure, format, precedence, set, layout, or form
  each time you write the metaphor. Pick the specific word for the specific
  meaning. The same rule applies to "thing," "stuff," "sort of," and "kind of
  like": if the prose can be rewritten with a concrete noun, rewrite it.
- **Replace abstract programmer nouns with what the user sees.** Words like
  *stream*, *engine*, *pipeline*, *buffer*, *queue*, *flow* are accurate but
  they sit one level above the reader's experience. In a concept chapter
  introducing a behaviour, replace them with the concrete user-facing
  description: "the byte stream from the device" becomes "what the board
  prints," "the trigger engine watches the stream" becomes "regex patterns
  fire actions when they match what the board prints." Once the reader has
  met the concept (typically by the time they're in a Detailed or Reference
  chapter), the technical noun is fine. The rule applies hardest at first
  introduction, where the reader has nothing else to anchor the abstraction
  against.
- **No vague engineering category labels as section headings.** Words like
  "Line-shaping," "I/O," "State," "Side effects," "Pipeline," "Lifecycle"
  read as labels from an architecture diagram, not as headings that tell
  the reader what the section does. Use a verb-phrase that names what the
  user accomplishes or controls: "Sending to the board" beats "Line-
  shaping"; "Capturing, logging, and external commands" beats "I/O";
  "Session control" beats "State." Reference-chapter sub-headings can be
  the literal verb / noun being defined (e.g. `=== `send`-family`);
  Detailed and Concept chapters need plain-English headings that scan
  for content.
- **No em dashes.** Em dashes (the literal `—` character or the AsciiDoc
  `--` source form that renders as one) read as conversational filler and
  have become an "LLM fingerprint" since they are wildly over-used by
  generative models. Replace each with the punctuation the sentence actually
  needs: a colon for term-then-definition or list introduction, a semicolon
  for tightly-coupled clauses, a period for a clean sentence break,
  parentheses for a parenthetical aside, or a comma for a brief
  qualifier. The rule applies in section headings, bullet term-defs,
  table cells, and prose alike.
- **No sales pitch, no positioning prose, no competitive defence.** The
  reader of a user manual has already chosen the software. Writing that
  belongs in a README, marketing page, ADR, or architecture spec does not
  belong in the manual. Specifically:
  * No "Why it exists" / "Why we built this" sections. The motivation
    behind a tool's existence is design-history material; the manual
    states what the tool does, not why it was preferred to alternatives.
  * No "Compared to X / vs Y" sections that argue the chosen design is
    better than competing tools, formats, or libraries (e.g. defending
    KDL against JSON/YAML). The reader is already using the chosen
    design; defend it elsewhere.
  * No "It replaces X, Y, Z workflows" framing. State what the software
    does, not what it displaces.
  * No "Use it to:" feature-dump bullet lists in the introduction. Brief
    factual description of scope, then the actual manual.
  * No marketing adjectives ("powerful," "elegant," "first-class,"
    "opinionated," "out of the box," "quality-of-life," "production-ready").
    They tell the reader nothing they can verify and read as advertising.
  * No narrative colour ("chase a U-Boot prompt, hit Ctrl-C at the right
    millisecond, type a kernel cmdline, watch a panic scroll by") that
    sells the reader on the kind of work the tool serves. Factual
    description of behaviour beats evocative storytelling.
  * No "What X is not" / "What X is NOT" sections that deny things the
    reader never expected. "Not a debugger" / "Not a firmware build
    system" in a serial-console manual reads as anticipating attacks
    that nobody is making. Genuine scope clarifications (e.g. "one
    invocation drives one transport," "TUI shows one device, not a
    multiplexed view") belong in the Scope chapter's Out of scope list,
    not in a separate negation section. If a scope point doesn't fit
    Out of scope, the prose probably wasn't load-bearing to begin with.
  * No "It does N things:" / "Here are the three pieces:" framings
    followed by a numbered list of capabilities. That construction reads
    as a presentation slide -- it's how someone *sells* a tool, not how
    a reference describes one. A concept chapter's "What is X" should
    read like a dictionary entry: one or two sentences naming the
    category, the central abstraction the user will work with (the
    "profile," the "document," the "session"), and how that abstraction
    fits the available commands. If three numbered bullets feel
    necessary, the prose has too much sales rhythm; rewrite as
    declarative sentences.

### Concrete anti-patterns

Each pair below shows a sourced-from-implementation phrasing and its
usage-focused rewrite. The SKILL.md "Reject implementation-leak phrasing" rule
lists the categories; these are how the rule plays out in prose.

| Anti-pattern                                                  | Rewrite                                                  |
|---------------------------------------------------------------|----------------------------------------------------------|
| "It links against the workspace's shared crates (`devtools-X`, `devtools-Y`)." | (delete; the user runs one binary.)                      |
| "The `Interpolator` type substitutes `${...}` references."    | "When you write `${var:name}`, bserial substitutes the named variable." |
| "Resolution is async, lazy, and secrets are zeroized on drop." | "Secrets resolve when an action actually needs the value; cleartext is dropped immediately after." |
| "`timestamp-format` is a chrono strftime."                    | "`timestamp-format` is a strftime."                      |
| "Pinned `schema "2.0"` in I-SH-07 PR 7 (May 2026)."           | "Profiles must declare `schema "2.0"`; older files are rejected at load." |
| "The session lifecycle: 1. Resolves profile. 2. Substitutes... 3. Opens transport..." | "When you run `bserial connect`, bserial opens the transport and starts the TUI." |
| "Pin those down with explicit `Instant`-based fixtures."      | "Design the trigger to assert on what the device printed, not on wall-clock timing." |
| "Two awkward shapes ... interactive shape ... automated shape." | "Two distinct modes ... interactive mode ... automated mode." |
| "Same shape as `tio` or `minicom`."                           | "Familiar territory if you have used `tio` or `minicom`."             |
| "JSON output shape:"                                          | "JSON output format:"                                                 |
| "Same shape, applied to the script-search directories."       | "Same precedence, applied to the script-search directories."          |
| "Same subcommand shape as `bserial profiles`."                | "Same subcommand set as `bserial profiles`."                          |
| "Same shape as the top-level `triggers` form."                | "Same form as the top-level `triggers` block."                        |
| "Minimal profile shape."                                      | "Minimal profile structure."                                          |
| "Field-shape checks still run."                               | "Field type checks still run."                                        |
| "It replaces workflows built on `minicom`, `screen`, `picocom`, and `tio`." | "This manual describes `bserial`, a serial-console tool for embedded systems on Linux." (state what it is, not what it displaces) |
| "Use it to: set up profiles ...; drive boards interactively; automate boot ...; run hands-off in CI ..." | (delete the feature-dump bullets; the chapters that follow are the manual) |
| "=== Why it exists. Embedded console work falls into two distinct modes ... bserial collapses the two ..." | (delete the section; motivation belongs in the README / ADR / architecture spec, not the manual) |
| "Compared to other config formats: vs JSON / TOML, KDL has ...; vs YAML, ..." | (delete; the user has already chosen KDL by using this manual) |
| "The TUI is opinionated around a single device's stream."     | "The TUI shows a single device's stream."                             |
| "comments are first-class."                                   | (delete, or restate as a concrete capability the user can use)        |
| "=== What bserial is not. * Not a debugger ... * Not a firmware build system ... * Not multi-board ..." | (delete the section; if a bullet genuinely scopes the tool, move it to Scope's Out-of-scope list) |
| "`bserial` is a serial-console and automation tool. It does three things: 1. Connects to a board ... 2. Reacts to what the board prints ... 3. Runs the same profile non-interactively ..." | "`bserial` is a serial-console and automation tool for embedded systems on Linux. A *profile*, written in KDL, describes how to reach a board, which prompts to expect, and how to react to its output. The same profile drives interactive sessions (`bserial connect`), scripted runs (`bserial run`), and replays of captured logs (`bserial replay`)." |
| "You drive bserial in two modes."                             | "bserial runs in two modes."                                          |
| "The first thing you write is a profile."                     | "A profile is a KDL file describing how to reach one board."          |
| "Once a profile connects, you'll usually add triggers."       | "Triggers are regex patterns in a profile that fire actions when they match." |
| "Triggers run in the background while you type."              | "Triggers run in the background concurrently with interactive input." |
| "phases let you scope triggers to each state."                | "phases scope triggers and prompts to each state."                    |
| "When you `bserial connect <profile>` without `--no-tui`, you're in the interactive TUI." | "`bserial connect <profile>` without `--no-tui` starts in the interactive TUI." |
| "passthrough is what you want for devices running vim."       | "passthrough suits devices running vim."                              |
| "Reacts to the output stream through a trigger engine."       | "Reacts to what the board prints: regex patterns ... fire actions when they match." |
| "Re-feeds the captured stream through the profile's trigger engine." | "Re-feeds the captured log through the profile's triggers."           |
| "The TUI shows a single device's stream."                     | "The TUI shows a single device's output."                             |
| "Triggers stay armed against the background stream."          | "Triggers keep watching what the board prints in the background."     |
| "==== Line-shaping" (heading for `send`, `send-bytes`, `send-break`, `send-file`) | "==== Sending to the board"                              |
| "==== I/O" (heading for `log`, `notify`, `run`, `dump-buffer`, `capture`)          | "==== Capturing, logging, and external commands"          |
| "==== State" (heading for `set`, `reconnect`, `enable-trigger`, ...)               | "==== Session control"                                    |
| "depth cap is `MAX_CALL_DEPTH = 32`."                          | "depth cap is 32."                                        |
| "**`DEV-TOOLS-ARCH-0001`** -- bserial architecture spec."     | "**`DEV-TOOLS-ARCH-0001`**: bserial architecture spec."               |
| "Profile fires *actions* -- send a string, capture, ..."      | "Profile fires *actions*: send a string, capture, ..."                |
| "credential -- the secret lives in your password manager"     | "credential; the secret lives in your password manager"               |
| "separate file -- this is how you record a panic"             | "separate file. This is how you record a panic"                       |
| "scripts) -- those still fire on real time"                   | "scripts). Those still fire on real time"                             |
| "interactive mode -- chase a U-Boot prompt -- is what tio does" | "interactive mode (chase a U-Boot prompt) is what tio does"          |
| "=== \\`serial\\` -- local serial port"                       | "=== \\`serial\\`: local serial port"                                 |

When you find the rule and the example agreeing, delete the example -- it
becomes scaffolding. When they disagree, the example wins, and the rule needs
sharpening.

## Detailed chapters: task-oriented or feature-oriented

Detailed chapters come in two legitimate styles. Pick one per chapter based on
how the reader will arrive at it.

### Task-oriented

For operator-facing or workflow-driven manuals, build the chapter from tasks.
A task has:

1. A goal stated as a heading the reader would recognize ("Configure the boot
   timeout", not "The timeout parameter").
2. Prerequisites — what must be true first.
3. Ordered, numbered steps. One action per step. Show the exact command, value,
   or control.
4. The expected result — how the reader knows it worked.
5. Failure handling — the common ways it goes wrong and what to do.

Use this style when the reader's mental model is "I need to accomplish X" —
running an installer, recovering from a fault, performing a backup. The chapter
title is a verb phrase ("Configure...", "Capture...", "Recover...").

### Feature-oriented

For tool / configuration-format manuals where the reader's mental model is
"I want to use feature X," a feature-oriented chapter is the better fit. The
chapter opens with a brief plain-prose definition of the feature, then organises
sub-sections by the feature's parts — its anatomy in the config language, the
options it accepts, worked examples, edge-case behaviour, and a forward
cross-reference to the relevant reference chapter for exhaustive lookup.

Use this style when the reader arrives at the chapter by feature name
(`Triggers`, `Phases`, `Scripts`, `Secrets`) rather than by task. The chapter
title is a noun phrase.

### Either way

- Use concrete worked examples with real values, not `<placeholder>` soup. When
  a value is variable, show a real example and say what's variable about it.
- Forward-reference the relevant reference chapter on first mention of a
  surface that has one (`see <<trigger-dsl-reference>> for the full list`).
- Don't mix the two styles inside one chapter. A feature-oriented chapter with
  a few procedural recipes is fine; a task-oriented chapter that lapses into
  reference enumeration mid-task is not.

## Concepts vs. detail vs. reference (in prose)

- In **concept** chapters, explain the idea once, plainly, with the term in
  **bold** on first use; that term goes in the glossary. Keep it short.
- In **detailed** chapters, *use* the concepts and link back to them; don't
  re-explain. Focus on the doing.
- In **reference** chapters, drop the narrative. Use tables and consistent entry
  templates so a fact is found in seconds. Completeness beats readability here.

## Terminology discipline

- Pick one term per concept and use it everywhere. Don't alternate between
  synonyms ("job"/"task"/"run") — pick one.
- Every domain term, acronym, or product-specific word used in the body must
  have a glossary entry. Expand each acronym on first use.
- Match the names in the software exactly: command names, flag spellings, config
  keys, and API symbols are copied verbatim from the sources, in `monospace`.

## Visual aids

Reach for a diagram when relationships or flows are easier seen than read
(architecture, state machines, sequence of a workflow). Reach for a table for
anything enumerable (options, keys, error codes, compatibility). Don't decorate;
each visual must carry information the prose would labor to convey.

## Review checklist

Before delivering, verify:

- [ ] Every chapter is about *using* the software, not building it. Internals
      appear only where they change user actions.
- [ ] Concept / detailed / reference material is in the right chapter type and
      not duplicated across them.
- [ ] Concept-chapter subsections each open with what the user does or sees,
      not with what the implementation does. ("When you write a profile..." not
      "A profile is a KDL document...".)
- [ ] No crate / package / module names from the codebase lifted into prose,
      no internal type / trait / function names, no async / lazy / zeroize /
      mlock / runtime / executor vocabulary unless the user must act on it.
- [ ] No internal ticket IDs, PR numbers, commit hashes, or internal-decision
      dates in the manual prose -- that lives in commits and ADRs.
- [ ] No past-tense ("used to", "previously", "before version X") or
      future-tense ("will support", "planned", "not yet implemented") prose
      about software behaviour. The manual describes the current state.
- [ ] No vague engineering metaphors. "Shape" has eight different right
      answers depending on context (mode / structure / format / precedence /
      set / form / layout / type); pick one each time. Same for "thing,"
      "stuff," "sort of," "kind of like."
- [ ] No abstract programmer nouns at first introduction. "Stream," "engine,"
      "pipeline," "buffer," "queue," "flow" sit one level above what the
      user sees. In concept chapters, write the user-facing description
      ("what the board prints," "regex patterns fire actions when they
      match") instead.
- [ ] No em dashes (`—` or AsciiDoc `--`). Each one is a colon, semicolon,
      period, parentheses, or comma in disguise; pick the punctuation the
      sentence actually needs.
- [ ] No sales pitch / positioning / competitive defence. No "Why it
      exists" section, no "Compared to X / vs Y" framing, no "It replaces
      Z" prose, no "Use it to:" feature-dump in the introduction, no
      marketing adjectives ("powerful," "elegant," "opinionated," "quality-
      of-life," "first-class," "production-ready"), no narrative colour
      selling the reader on the work the tool serves, no "What X is not"
      section denying things the reader never expected (move genuine
      scope bullets to Scope's Out-of-scope), no "It does N things:" +
      numbered feature list framings in concept chapters (read as
      presentation slides; rewrite as declarative sentences).
- [ ] "You" used only in instructions and prereqs, not in descriptive
      prose. "bserial runs in two modes" not "you drive bserial in two
      modes"; "phases scope triggers" not "phases let you scope triggers."
      The tool is the subject when the sentence describes the tool.
- [ ] User-vocabulary preferred over codebase jargon when the two differ
      (e.g. "variable substitution" not "interpolation" for config-tool users).
- [ ] Cross-document xrefs carry the target doc's ID as visible link text so
      the citation survives a broken link in a standalone PDF.
- [ ] Each detailed task has goal, prerequisites, ordered steps, expected result,
      and failure handling.
- [ ] Reference chapters are exhaustive for their declared scope.
- [ ] Names (commands, flags, keys, symbols) match the sources exactly.
- [ ] One term per concept; every domain term is in the glossary; acronyms
      expanded on first use.
- [ ] Scope chapter states what's covered and what isn't, plus versions/platforms.
- [ ] Gaps are clearly marked, not silently filled.
- [ ] Frontpage states the software version(s) the manual applies to.

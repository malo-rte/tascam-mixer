# Manual structure

Defines each required section: its purpose, what belongs in it, and what does
not. The single most important distinction is **concept vs. detail vs.
reference** — get that wrong and the manual either repeats itself or leaves holes.

## Alignment with spec-document-template

This structure is the manual-facing form of the spec skeleton, so a manual sits
beside a spec consistently:

- **Concept chapters** ≈ the spec's **content chapters** (understanding/mental
  model), but pitched at a user, not an implementer.
- **Detailed (task/procedure) chapters** are the manual's own addition — specs
  rarely have them. This is where usage actually lives.
- **Reference chapters** are the *same* as the spec's reference chapters. Reuse
  the reference-chapter table and the `== <Name> reference` heading convention
  from `spec-document-template`; don't invent a parallel scheme. When the
  software is already specified, the reference chapter may summarise and
  cross-reference the spec's reference chapter rather than re-deriving it.

The intro/scope distinction also follows the spec template. The family adds an
*overview* (a one-paragraph-per-chapter reader's map); fold that into the opening
of the concept chapters, or add a short optional overview — it is not one of the
nine mandated sections but it helps the reader.

## The three chapter types

A reader uses these differently, so keep them physically separate.

- **Concept chapters** answer *"what is this and how does it fit together?"* They
  build the mental model: the main objects, how they relate, the lifecycle, the
  vocabulary. High-level. No exhaustive option lists, no step-by-step. A reader
  reads these once, in order, to orient themselves. If you find yourself listing
  every flag, you're in the wrong chapter.

- **Detailed chapters** answer *"how do I do X?"* They are task- and
  procedure-oriented: install it, configure it, run a job, recover from a failure.
  Ordered steps, expected results, worked examples. A reader comes here with a
  goal. Each chapter maps to a task the audience actually performs.

- **Reference chapters** answer *"what exactly is the value/signature/option for
  Y?"* They are exhaustive and scannable, not narrative. Every command, flag,
  config key, API function, return value, error code, register, or environment
  variable that the user can touch. A reader arrives here knowing what they want
  and needing the precise detail. Completeness matters more than prose.

Rule of thumb: a fact appears in exactly one of these. Concepts *introduce* a
term, detailed chapters *use* it in a task, reference *defines it exhaustively*.

## Section-by-section

### 1. Frontpage
Title, the software name and the version(s) the manual applies to, document
revision/date, and intended audience. Keep it to identification — no content.

### 2. Table of contents
Auto-generated from headings (see `formats.md`). Reflects the real heading
hierarchy; don't hand-maintain it.

### 3. Introduction
A short orientation: what the software is, what problem it solves, who it's for,
and how to read the manual (which chapters matter for which reader). One to a few
paragraphs. Not a feature dump.

### 4. Scope
What this manual covers and — just as important — what it does **not**. State the
software versions, platforms, and configurations in scope. List prerequisites and
assumed knowledge. Point elsewhere for out-of-scope topics (e.g. "for the build
system, see ..."). A precise scope is what stops a manual from sprawling.

### 5. Concept chapters
The mental model (see above). Typically: the core abstractions, how they relate,
the normal lifecycle/workflow at a high level, and any domain vocabulary. Enough
for the reader to make sense of the detailed chapters. Diagrams help here.

### 6. Detailed chapters
The procedures (see above). Cover the real tasks: installation/setup,
configuration, the primary workflows, common operations, and recovery/
troubleshooting. Each task: goal, prerequisites, ordered steps, expected result,
and what to do if it goes wrong. Worked examples with concrete values.

### 7. Reference chapters
The exhaustive lookup material (see above). Use the reference-chapter templates
and per-surface required-entry table from `spec-document-template` (CLI, API,
data structure, file format, protocol, config schema, device-tree binding) —
they are authoritative and already cover every software type below. Apply the
same `== <Name> reference` heading convention, and have the concept/detailed
chapter that introduces a surface cross-reference its reference chapter on first
mention. Per software type the reference chapters typically cover:
- **CLI**: every command/subcommand and option (type, default, effect); exit
  codes; environment variables; config keys.
- **Library/API**: every public function/class/method — signature, parameters,
  return value, errors, and a minimal usage snippet.
- **GUI**: every screen/dialog and its controls, menus, shortcuts, settings.
- **Embedded device**: user-facing interfaces — pins/connectors, indicators,
  controls, exposed registers/properties, supported commands, config parameters,
  defaults.
When the software is already specified, prefer summarising and cross-referencing
the spec's reference chapter over copying it (DRY — see `spec-writing-style`).

### 8. Appendixes
Material that supports the manual but would interrupt its flow: full config-file
examples, large tables, compatibility matrices, file-format specs, sample output,
migration notes, FAQ. Label them Appendix A, B, C…

### 9. List of used terms (glossary)
Alphabetized definitions of every domain term, acronym, and product-specific word
used in the body. Each entry is a short, usage-oriented definition — what the term
means *to a user of this software*. Every term introduced in a concept chapter
should appear here.

## Tailoring by audience

The structure is fixed; the emphasis shifts. An operator manual is heavy on
detailed/procedure chapters; an API/integrator manual is heavy on reference; an
introductory manual leans on concept chapters. Decide the emphasis in the outline
stage based on the audience established in workflow stage 1.

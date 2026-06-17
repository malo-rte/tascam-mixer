---
description: Produce a threat model for a component or feature
argument-hint: "<component / spec path / feature to model>"
allowed-tools: Bash(.claude/skills/docs-librarian/scripts/docs-librarian:*), Bash(python3:*), Read, Grep, Glob, Write, Edit
---

Apply the `threat-model` skill to produce a threat model. Target: $ARGUMENTS

## Task
Build the model for the named target (`threat-model`). This *produces* the
analysis; to *audit* a change against an existing one, use `/security-review`.

1. **Read the target** — the spec/design/code in `$ARGUMENTS` and what it
   connects to. If the target is unclear, ask once.
2. Answer the four questions in order, as the skill's chapters:
   - **Assets** — what is protected, with its required properties (confidentiality
     / integrity / authenticity / availability) and where it lives.
   - **Trust boundaries** — where data/control crosses trust levels (normal↔secure
     world, host↔board, network↔device, signed↔unsigned). Diagram them.
   - **Adversary model** — capabilities, and explicitly out-of-scope adversaries.
   - **Assumptions** — the dependencies the analysis rests on.
3. **Threats** — walk **STRIDE per boundary**; each threat gets `THR-NNN`, the
   asset+boundary, the STRIDE tag, the adversary capability, and the attack in one
   line.
4. **Mitigations** — for each threat, the control **named by primitive** (not
   "hardened"), traced to a **requirement ID** (`requirement-spec`); unmitigated
   threats are stated as explicitly accepted risks with rationale. Cover the key
   lifecycle for every key, and anti-rollback for versioned state.
5. **Traceability matrix** — threats ↔ mitigations ↔ requirements; an empty
   mitigation cell is a TODO or an accepted risk, never blank.

## Output
If this is a standalone security document, scaffold it via `/new-doc` semantics:
allocate an ID (the threat model is usually an `ARCH`- or `DES`-class security
chapter, or its own doc), place and register it. Otherwise, produce the chapters
to drop into the host spec as a security section (the overlay, per `threat-model`).
Apply `prose-precision` — name the adversary, the attack, the asset; no "attacker
tampers with things". File follow-ups as `DEV-TOOLS-TASK-NNNN`.

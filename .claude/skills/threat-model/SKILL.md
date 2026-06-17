---
name: threat-model
description: Use whenever analysing or documenting the security of a system or feature — identifying assets, trust boundaries, adversaries, and threats, and tracing each threat to a mitigation. Apply when writing the security chapters of a spec (secure boot, key handling, attestation, network access control, signed updates), when reviewing a design for security gaps, when a feature touches keys/credentials/trust boundaries, or when someone asks "what could go wrong here?". Produces a structured threat model (assets, trust boundaries, adversary model, threats with STRIDE tags, mitigations traced to requirements). Overlays the spec type skills rather than replacing them; pairs with spec-review-checklist section 9.
---

# threat-model

How to produce a **threat model**: the structured security analysis that names
what is being protected, from whom, across which boundaries, and how. It is an
**overlay** — it adds security chapters to a requirement, architecture, or design
spec (and stands alone as a security spec when the analysis is large). It makes
`spec-review-checklist` §9 first-class instead of a checklist afterthought.

Security is a property of a *system in an environment facing an adversary*, not a
feature. A claim like "the channel is secure" is meaningless until you name the
asset, the adversary, and the property (`spec-writing-style` weasel rule). This
skill forces those to be named.

## The four questions

Every threat model answers, in order:

1. **What are we protecting?** — the assets.
2. **What is the adversary's reach?** — the trust boundaries and the adversary model.
3. **What can go wrong?** — the threats.
4. **What stops it, and where is that obligation recorded?** — the mitigations,
   each traced to a requirement.

A threat model missing any of the four is incomplete.

## Required chapters

When this overlay is present, these chapters appear (as top-level chapters of a
security spec, or as a security section of the host spec):

1. **Assets** — what has value and must be protected, named explicitly: private
   keys, the ROTPK, attestation logs, firmware images, board identity, session
   credentials. For each: its security properties (confidentiality, integrity,
   authenticity, availability) and where it lives.
2. **Trust boundaries** — where data or control crosses between components at
   different trust levels: normal world ↔ secure world, host ↔ board, network ↔
   device, signed ↔ unsigned, on-die ↔ off-chip. A diagram is near-mandatory; each
   boundary is where threats concentrate.
3. **Adversary model** — who the attacker is and what they can do: a remote
   network attacker, a local non-root process, an attacker with physical/JTAG
   access, a malicious peripheral, a supply-chain attacker. State capabilities
   explicitly; a mitigation is only meaningful relative to a stated adversary. Out
   of scope adversaries are listed too ("nation-state physical decap: out of
   scope").
4. **Assumptions** — what the model takes for granted: the platform's secure-boot
   root of trust holds, the operator's workstation is not compromised, the RNG is
   sound. An assumption is a dependency the model rests on; if it is false the
   analysis does not hold.
5. **Threats** — the enumerated ways an asset can be compromised across a
   boundary by the adversary. Each threat carries:
   - a stable ID (`THR-NNN`),
   - the asset and boundary it targets,
   - a STRIDE tag (Spoofing, Tampering, Repudiation, Information disclosure,
     Denial of service, Elevation of privilege) as a completeness aid — walk
     STRIDE per boundary so categories are not missed,
   - the adversary capability it assumes,
   - a one-line description of the attack.
6. **Mitigations** — for each threat: the control that addresses it, **named by
   primitive or property** (signature verification against the active ROTPK,
   anti-rollback counter, channel encryption with key K), and the **requirement ID**
   (`REQ-…`) that levies the obligation. A threat with no mitigation is an
   accepted risk and is stated as such, with rationale.

## Method

- Walk **STRIDE per trust boundary**, not per component, so categories that span
  components (repudiation, DoS) are not missed.
- For every asset, ask which of C/I/A/authenticity matters; threats that break a
  property the asset does not need are noise.
- For every mitigation, name the primitive — never "secured", "hardened",
  "protected" with no mechanism (no security through obscurity, `spec-review-checklist`
  §9). If you cannot name the primitive, the mitigation is not real yet.
- Cover the **key lifecycle** for every key asset: generation, storage, use,
  rotation, revocation, destruction. A key whose rotation/revocation is undefined
  is a finding.
- Cover **anti-rollback** wherever versioned state exists (firmware, counters,
  certificates).

## Traceability

The threat model is the bridge between the security analysis and the rest of the
doc set:

- Each **mitigation** cites the **requirement ID** that obligates it. The
  requirement lives in the `requirement-spec`; the threat model references it, it
  does not restate it (CC-2).
- Each **trust boundary** corresponds to an architecture decision; link the
  `architecture-spec` chapter or the `architecture-decision-record` that
  established it.
- A **threats ↔ mitigations ↔ requirements** matrix (appendix) makes coverage
  auditable: every threat has a row; an empty mitigation cell is either a TODO or
  an explicitly accepted risk.

## Content class and prose

- Threats and adversary capabilities are **descriptive** (no RFC 2119); the
  obligations they motivate are `SHALL` statements that live in the requirement
  spec (`requirement-spec`, AC-5/RC routing). Keep the analysis here and the
  obligations there.
- Apply `prose-precision`: name the actor (which adversary), the action (the
  specific attack), the asset. "An attacker tampers with things" fails P-1/P-3.
- No project planning (CC-5): a mitigation that is not yet implemented is marked
  status (e.g. `deferred`) per `architecture-spec`'s status matrix — not scheduled.

## Review checklist (extends spec-review-checklist §9)

- [ ] Assets named with their required security properties
- [ ] Trust boundaries identified and diagrammed
- [ ] Adversary model states capabilities *and* out-of-scope adversaries
- [ ] Assumptions listed; each is a real dependency of the analysis
- [ ] STRIDE walked per boundary; threats have IDs and adversary assumptions
- [ ] Every mitigation names a primitive/property — no "secured"/"hardened"
- [ ] Every mitigation cites a requirement ID; unmitigated threats are accepted explicitly
- [ ] Key lifecycle covered for every key; anti-rollback covered for versioned state
- [ ] Threats ↔ mitigations ↔ requirements matrix present and complete

## What this skill does not cover

- The obligations the mitigations satisfy → `requirement-spec`
- The structure of the trust boundaries → `architecture-spec` /
  `architecture-decision-record`
- The mechanism of a control (the actual crypto, the boot chain) → `design-spec`
- General review → `spec-review-checklist` (this makes §9 first-class)
- Prose, rendering, placement → `prose-precision`, `asciidoc-conventions`, `docs-librarian`

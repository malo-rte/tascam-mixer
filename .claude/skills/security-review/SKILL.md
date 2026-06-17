---
name: security-review
description: Use whenever auditing a code change, design, or component for security — distinct from threat-model, which produces the model; this checks an artifact against it. Apply when asked to "do a security review", when a change touches keys, credentials, trust boundaries, secure boot, network access, signed updates, parsing of untrusted input, or subprocess/FFI, and as the security gate before merging or releasing such changes. Orchestrates threat-model, spec-review-checklist section 9, and the security-relevant coding rules. Pairs with the /security-review slash command.
---

# security-review

How to audit an artifact (a diff, a design, a component) for security. Distinct
from `threat-model`: that skill *produces* the model (assets, boundaries,
adversary, threats, mitigations); this one *checks a change against it* and
against the project's security rules. It makes `spec-review-checklist` §9
actionable on real code.

## First: what does this change touch?

Security review is scoped by *what the change exposes*, not by line count. Before
reviewing, identify:

- **Assets touched** — keys, ROTPK, credentials, attestation logs, firmware
  images, board identity, session secrets (`threat-model` Assets).
- **Trust boundaries crossed** — normal↔secure world, host↔board, network↔device,
  signed↔unsigned, privileged↔unprivileged (`threat-model` Trust boundaries).
- **Untrusted input parsed** — anything from the network, a file, a peripheral, a
  user, or another process.

If the change crosses a boundary or touches an asset that has **no threat model**,
that is the first finding: require or extend one (`threat-model`) before the
change merges. A security review without a model is just spot-checking.

## The pass

Walk the change against each, citing the rule/threat:

1. **Mitigations trace to requirements.** Every security control the change
   relies on names a primitive (signature verification against the active ROTPK,
   anti-rollback counter, channel encryption with key K) and traces to a
   requirement ID (`threat-model`, `requirement-spec`). No "secured"/"hardened"
   with no mechanism (§9: no security through obscurity).
2. **Secret handling.** Secrets are not logged, not written to disk unencrypted,
   not left in memory longer than needed (zeroize on drop where the threat model
   warrants). Key lifecycle (generation, storage, use, rotation, revocation) is
   defined for any key the change introduces.
3. **Input validation at the boundary.** Untrusted input is validated before use;
   length/range/format checked; no unbounded allocation from attacker-controlled
   sizes; parser rejects malformed input rather than misinterpreting it
   (`interface-spec` unknown-field rule).
4. **Memory & unsafe.** `unsafe` is minimal, justified with `// SAFETY:`, and
   encapsulated (`rust-coding-rules` RS-30/31); no `transmute`/aliasing tricks
   on untrusted data (RS-32). For C, bounds and lifetime discipline
   (`c23-coding-rules`).
5. **Process & FFI.** No `shell=True` or shell-interpolated untrusted input
   (`python-coding-rules` PY-32); subprocess uses argument lists; destructive
   shell paths are guarded (`shell-coding-rules` SH-25). FFI callbacks don't
   panic across the boundary (RS-14).
6. **Crypto usage.** Named, current primitives; no home-rolled crypto; correct
   construction order (what is signed vs encrypted, MAC-then-encrypt vs
   encrypt-then-MAC) stated and matching the `interface-spec` security envelope.
7. **Anti-rollback & versioned state.** Wherever the change writes versioned state
   (firmware, counters, certificates), rollback is considered (§9).
8. **Audit logging.** Security-relevant events (auth failure, signature rejection,
   key use) are logged per the security-event obligations.
9. **Supply chain.** New dependencies pass `cargo-deny` (advisories, license
   allow-list, source allow-list — `rust-coding-rules` `deny.toml`); a new
   dependency that handles untrusted input or crypto gets extra scrutiny.

## Severity

- **MAJOR** (blocks merge/release): an exploitable gap, an unmitigated threat in
  scope, a secret leak, missing signature/rollback check, unsafe on untrusted
  input, a failing advisory.
- **MINOR**: defence-in-depth gap, missing audit log, unclear mitigation tracing.
- **Accepted risk**: a gap the threat model explicitly accepts, with rationale —
  recorded, not silently passed.

## Output

Findings grouped by asset/boundary, each citing the threat (`THR-NNN`), the rule,
and the mitigation expected. End with: block / allow / allow-with-follow-ups, the
threat-model updates required, and any `DEV-TOOLS-TASK-NNNN` filed.

## What this skill does not cover

- Producing the threat model (assets/boundaries/adversary/threats) → `threat-model`
- General (non-security) code review → `code-review`
- The security obligations themselves → `requirement-spec`, `spec-review-checklist` §9
- The crypto/mechanism design → `design-spec`, `interface-spec`
- Dependency policy config → `rust-coding-rules` `references/deny.toml`

---
name: c23-coding-rules
description: Use whenever writing, reviewing, or modernizing C code in this project — new C source or headers, drivers, bootloader or secure-world code, register and wire-protocol definitions, or migrating C11/C17 to C23. Covers the modern-C23 idiom and safety set (strict warnings and sanitizers, nullptr, attributes like nodiscard and fallthrough, checked arithmetic with stdckdint, memset_explicit for secrets, explicit enum types, static_assert on struct layout, constexpr over define, const discipline) plus the cross-toolchain pragmatics for building under GCC and Clang with -Werror. Apply this any time C is being authored or judged, when setting up a C build's warning set, or when someone asks for C coding standards — not only when "C23 rules" are named. The full rules with rationale and examples are in references/c23-rules.adoc.
---

# c23-coding-rules

The project's C23 coding rules. The authoritative text — every rule with its
rationale and code examples — is in `references/c23-rules.adoc` (stable IDs
`R1`..`R72`). **Read that reference when applying or reviewing rules**; this file
is the index, the severity tiers, and the enforcement map.

Rules are SHOULD unless marked MUST (MAY for optional ones). When writing or
reviewing C, apply the rules relevant to the change; when a rule's intent isn't
obvious, open the reference for the rationale and example.

## Foundational — always on

These gate everything else; set them up before the per-rule advice matters.

- **R1** MUST compile with `-std=c23` (`-std=c2x` only during transition).
- **R2** MUST enable the strict warning set and `-Werror` in CI.
- **R3** SHOULD run `-fanalyzer`/`clang --analyze` and ASan/UBSan host tests.
- **R62** SHOULD verify C23 feature support per target toolchain via a small
  `c23_features.h` probe. This is what makes the gated features (R41, R42, R52)
  safe to adopt — treat it as foundational, not mere process. A GCC 14 + Clang 19
  CI pair from day one catches portability drift early.

## Rule index

**Legacy constructs — remove**: R10 explicit params (no K&R/empty lists, MUST),
R11 `nullptr` not `NULL`/`0` (MUST), R12 built-in `bool`/`true`/`false` (MUST),
R13 lowercase keywords (MUST), R14 no trigraphs (MUST).

**Attributes — use aggressively**: R20 `[[nodiscard]]` on status/alloc/lock
returns (MUST), R21 `[[fallthrough]]` (SHOULD), R22 `[[maybe_unused]]` over
`(void)x` (SHOULD), R23 `[[deprecated("...")]]` during migration (SHOULD), R24
`[[noreturn]]` on `panic`/`reboot`/fatal handlers (SHOULD), R25
`[[unsequenced]]`/`[[reproducible]]` for pure functions (MAY — *scope to hot
DSP/codec paths only; wrong purity is a miscompile*).

**Safety facilities**: R30 `<stdckdint.h>` for untrusted arithmetic (MUST), R31
`memset_explicit` for wiping secrets (MUST), R32 `unreachable()` + assert for
impossible paths (SHOULD), R33 `<stdbit.h>` over intrinsics (SHOULD), R34
`[[assume]]` (MAY — *demoted: use only with written justification; a wrong assume
is silent UB. Prefer assert + `unreachable()` per R32*).

**Types**: R40 explicit enum underlying type when layout matters (MUST), R41
`_BitInt(N)` for exact-width arithmetic (SHOULD — *toolchain-gated for wide N*),
R42 `constexpr` over `#define` for typed constants (SHOULD — *Clang 19+*), R43
`typeof`/`typeof_unqual` in generic macros (SHOULD), R44 `auto` for obvious
initializers (MAY), R45 `char8_t` for UTF-8 (SHOULD), R46 `const` on input-only
pointer params (MUST), R47 `const` on by-value params in the definition only (MAY
— *demoted: low value, team-taste; keep out of headers or drop*), R48
`const`-by-default locals (SHOULD), R49 return small objects by value,
out-pointers only for large or in/out (SHOULD).

**Initialization & literals**: R50 `= {}` zero-init (SHOULD), R51 binary literals
+ digit separators for masks (SHOULD), R52 `#embed` for binary assets (SHOULD —
*GCC 15+/Clang 19+; verify before removing `xxd` steps*).

**Process & migration**: R60 migrate one TU at a time; don't mix C23 cleanup with
functional change in a commit (SHOULD — same principle as `git-commit`), R61 lock
struct layout with `static_assert` (MUST).

**Cross-toolchain pragmatics** (the price of R2's strictness under GCC+Clang):
R70 add `-Wno-format-nonliteral` for variadic forwarder wrappers (MUST), R71 mark
macro-emitted `static inline` helpers `[[maybe_unused]]` (MUST), R72 cast
explicitly when narrowing a non-literal enum (MUST). *Keep R70–R72 only if you
actually build under both compilers; GCC-only projects don't need them.*

## Enforcement map

Prefer tooling over review where a rule is mechanical:

- R2 warning set (verbatim flags in the reference) + `-Werror` — the backbone.
- R20 ← `-Wunused-result` (with `[[nodiscard]]`); R21 ← `-Wimplicit-fallthrough`;
  R10 ← `-Wstrict-prototypes -Wold-style-definition`; R72 ← `-Wconversion`
  (R72 exists *because* R2 turns this on).
- R3 ← `-fanalyzer` / `clang --analyze` / ASan+UBSan in CI.
- R61 ← the `static_assert`s compile-check the ABI themselves.
- R62 ← the `c23_features.h` probe header + dual-toolchain CI.

Everything else (nullptr usage, const discipline, return-by-value, attribute
placement) is review territory — fold it into a `spec-review-checklist`-style
code-review pass.

## R2 / R70 interaction (not a contradiction)

R2 sets `-Wformat=2`, which turns on `-Wformat-nonliteral`; R70 then adds
`-Wno-format-nonliteral`. That's intentional: enable the format-checking family,
suppress the one false positive that `__attribute__((format))` already validates
at every call site. Note it where the warning set is defined so a reader hitting
both rules doesn't think they conflict.

## What this skill does not cover

- Architecture, layering, purity, no-globals → `software-design-rules` (the
  language-agnostic layer this implements in C).
- Rust → `rust-coding-rules` (the sibling skill).
- Comment content/style → `code-comments`. Commit hygiene → `git-commit`.
- Unfixable violations found in review → file as tasks via `task-from-sources`.

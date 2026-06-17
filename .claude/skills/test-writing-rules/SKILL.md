---
name: test-writing-rules
description: Use whenever writing, reviewing, or structuring tests — unit tests, integration tests, host-side or on-target/QEMU tests, or a test harness/runner. Covers the project test conventions for output and logging (errors only, plain text no color, a structured per-run log file), determinism and isolation, structure and naming, coverage and requirement traceability, reliability (no flaky or silently-disabled tests, timeouts), tiering fast unit vs slow integration, embedded specifics (hardware fakes, sanitizers), actionable failure messages, and CI integration (exit codes, machine-readable reports). Apply this any time tests are authored or judged, when setting up a test runner, or when someone asks for testing standards — not only when "test rules" are named. Rules use stable IDs TST-NN; treat it as a baseline to curate.
---

# test-writing-rules

The project's test-writing rules, language-agnostic; the per-language coding skills
(`c23-coding-rules`, `rust-coding-rules`) and `software-design-rules` cover how to
make code testable. Stable IDs `TST-NN`. Severity: **MUST** / **SHOULD** /
**AVOID**. This is a baseline — curate the severities to your house policy.

## Output and reporting

The console is quiet on success and loud only on failure; the log file is the
durable record.

- **TST-1 MUST**: a test run outputs *errors only* on the console — nothing on
  success. No progress chatter, no per-pass lines. Silence means everything passed.
- **TST-2 MUST**: failures print to **stdout** as **plain text — no ANSI color**.
  Respect `NO_COLOR` and disable color when stdout is not a TTY. (Logs and CI
  capture must stay readable.)
- **TST-3 MUST**: every run writes a **log file** recording each test that ran, its
  **state** (pass / fail / skip), and any **error messages**. The log is the full
  account even though the console showed only errors.
- **TST-4 MUST**: the process **exit code** reflects the result — non-zero if any
  test failed or errored, zero only on all-pass. CI depends on this.
- **TST-5 SHOULD**: reserve **stderr** for harness/infrastructure failures (the
  runner itself failing to start, a missing fixture), so a *test* failure (stdout)
  is distinguishable from a *runner* failure (stderr). *This is the resolution of
  TST-1's "stdout/stderr" and TST-2's "stdout" — flip it if your house rule is
  everything-on-stdout.*
- **TST-6 SHOULD**: end with a deterministic, machine-parseable summary (counts of
  pass/fail/skip). For CI, also emit a structured report (TAP or JUnit XML) and
  archive the log file as a build artifact.
- **TST-7 SHOULD**: include a timestamp and the run's environment (target, toolchain,
  seed) in the log header, so a failure is reproducible from the log alone.

## Determinism and isolation

- **TST-8 MUST**: tests are deterministic — no dependence on wall-clock time, real
  randomness, network, or execution order. Inject clock, RNG, and I/O
  (`software-design-rules` D34; Rust RS-80).
- **TST-9 MUST**: each test is independent and order-independent; no shared mutable
  state between tests. Set up and tear down cleanly.
- **TST-10 MUST**: seed any randomness explicitly and log the seed, so a failing run
  is replayable.
- **TST-11 AVOID**: `sleep`-based synchronization. Wait on an explicit event or
  condition with a bounded timeout instead.
- **TST-12 SHOULD**: tests are hermetic — no dependence on host config or external
  services unless the test is explicitly an integration test in the integration tier.

## Structure and naming

- **TST-13 SHOULD**: one behavior per test; the name states the behavior and the
  expected outcome, not just the function under test.
- **TST-14 SHOULD**: arrange / act / assert structure — setup, the single action,
  then the checks.
- **TST-15 SHOULD**: test the public contract and observable behavior, not private
  internals, so tests survive refactoring.
- **TST-16 SHOULD**: tests mirror the code's layout and follow one naming convention.

## Coverage and traceability

- **TST-17 MUST**: cover error paths and edge cases, not only the happy path —
  boundary values, invalid inputs, and injected failures.
- **TST-18 MUST**: every fixed bug gets a regression test that references the task or
  issue ID (`Refs: DEV-TOOLS-TASK-0042` style; see `task-tracker`).
- **TST-19 SHOULD**: where a spec exists, trace tests to its requirements; a test
  spec tracks requirements one-to-one with a traceability matrix
  (`spec-document-template` test-spec variation).

## Reliability

- **TST-20 MUST**: every test has a timeout, so a hang fails the run rather than
  blocking CI forever.
- **TST-21 MUST**: a flaky test is a bug — fix it, or quarantine it with a linked
  task; never leave it failing intermittently in the main suite.
- **TST-22 AVOID**: disabled/ignored tests without a linked task and a reason. A
  silently skipped test is worse than a missing one.

## Tiers and speed

- **TST-23 SHOULD**: fast unit tests are the default tier and run on every change;
  slow, integration, and on-target tests live in a separate tier run deliberately.
- **TST-24 SHOULD**: pure logic is tested host-side; hardware behavior is tested
  on-target or under QEMU/Renode. Don't gate the fast tier on hardware.

## Embedded specifics

- **TST-25 SHOULD**: model hardware with fakes (register/bus models) for host-side
  tests; reserve real or emulated hardware for the integration tier.
- **TST-26 SHOULD**: run host-side tests under sanitizers (ASan/UBSan; `c23-coding-rules`
  R3) and check for leaks; a clean sanitizer run is part of passing.
- **TST-27 MUST**: tests assert resource bounds where they matter (no leaks, bounded
  memory/stack on constrained targets).

## Failure messages

- **TST-28 MUST**: a failure message states **expected vs actual** plus enough
  context to diagnose from the log without re-running under a debugger. A bare
  "assertion failed" is not acceptable.

## Process

- **TST-29 SHOULD**: a change ships with its tests in the same logical commit
  (`git-commit` one-logical-change).
- **TST-30 SHOULD**: mark a missing test with `TODO(DEV-TOOLS-TASK-NNNN)` so `task-from-sources`
  harvests it into the task list.

## Enforcement

- TST-2 no-color ← honor `NO_COLOR` and `isatty`; assert in a harness self-test.
- TST-4 exit code, TST-6 report ← the runner contract; check in CI.
- TST-8/TST-10 determinism ← run the suite twice and diff; seed logging makes a
  failure replayable.
- TST-20 timeouts, TST-21 flakiness ← per-test timeout in the harness; a flake
  detector (re-run failures) feeds tasks.
- The rest is review territory — apply a `spec-review-checklist`-style pass to tests.

## What this skill does not cover

- Making code testable (purity, dependency injection) → `software-design-rules`.
- Language harness idioms (`cargo test`, doctests; a C harness like Unity/Criterion)
  → `rust-coding-rules` (RS-80..82) and `c23-coding-rules`.
- Test-spec document structure → `spec-document-template`. Missing/flaky tests as
  work items → `task-from-sources`.

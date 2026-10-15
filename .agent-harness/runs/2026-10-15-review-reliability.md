# Review reliability verification

Status: verified locally; GitHub CI pending.
Contract: [scope](../contracts/2026-10-15-review-reliability.md).

## Reproductions

- Existing `--version` returned `unknown` despite a valid checkout. Rebuilding the
  build script restored its hash; its previous output watched no Git metadata.
- The existing mutation integration test explicitly expected timeout → success.
- The review renderer maps `DoubleIn`/`OrphanOut`, while the real codes are
  `DuplicateIn`/`OutWithoutIn`; the remaining six codes also need usable guidance.
- Emptying the corpus, emptying one case's expectations, or duplicating an ID each
  returned exit 0 before validation was added; all three now fail by assertion.

`cargo test -p kintai-cli --test ac17_version_build` failed both new tests before
the fix (archive → checkout and packed → loose worktree refs). Mutation command
tests failed on timeout → exit 0 and on attempting a command for an empty selection.
Both new browser tests failed before the UI fix: `DuplicateIn` had no explanation,
and no checklist action existed. These are direct, minimized reproductions, so
additional hypothesis instrumentation was unnecessary for those small defects.

## Verification

- `cargo xtask check-all`: all gates pass; 469 Rust tests including doctests,
  fmt, clippy, rustdoc, MSRV 1.88, dependency checks, REUSE 3.3, links/line limits,
  strict trace (130 conditions, 3 manual, no missing references), and 900 independent
  differential cases (seed 1; 300 per kind), zero mismatches.
- Chromium: all 10 browser tests pass with retries disabled. Local execution reuses
  cached Playwright 1.62.1; CI installs the lockfile's 1.64.0. All eight anomaly
  examples originate from actual synthetic CSV files processed by the Rust binary.
- [Eight negative checks](2026-10-15-review-reliability/harness-negative-verification.json)
  each fail by a named Rust assertion, with no compile failures, and restore source.
  The added mutation definition is included in PR and weekly CI. Weekly configuration
  now contains 12 suites / 339 mutations; the entire weekly matrix was not run locally.
- [Two UI negative checks](2026-10-15-review-reliability/browser-negative-verification.json)
  remove a real anomaly mapping or leave the selected detail closed. Each fails the
  actual browser expectation, then restores and rebuilds the original source.
- [Browser observations](2026-10-15-review-reliability/browser-observations.json):
  390/768/1440px and dark 390px, no overflow or runtime errors; all buttons ≥44px.
  All four screenshots were opened and inspected. README screenshot refreshed.
- Japanese quick lint: no findings in changed review/testing/changelog documents.

## Review and improvement loop

Standards self-review: no new dependencies, no labor-engine changes, read-only
exports and startup scope unchanged; synthetic data only; all files licensed.
Requirements self-review: refusal guidance and navigation, real Cargo invalidation,
non-vacuous corpus validation, and fail-closed mutation outcomes have regressions.
These reviews are by the implementing agent, not independent reviewer results.

Cycle 1: wrote failing regressions, fixed findings, updated an existing spawn-error
test to retain a nonempty mutation (selection is now checked before spawning).
Removed temporary mutations; no debug instrumentation is left in source.

UI self-assessment: 18/20 (secondary text remains dense; assistive technology was
not manually tested). Experience self-assessment: 18/20 (correction and regeneration
still use the CLI). This is synthetic inspection, not research with real users.

Before merge: inspect the exact PR head, comments/reviews and all CI conclusions.

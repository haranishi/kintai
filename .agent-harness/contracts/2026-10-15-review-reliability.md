# Review and regression reliability

Status: active. Base: `e67849a` (main). Maximum improvement cycles: 3.
The maintainer requested further fixes, tests, PR creation and merging.

## Findings and scope

- Mutation timeouts currently count as detected errors; empty selections pass.
- The requirement corpus can be emptied without failing its replay test.
- A build made before Git metadata exists stays at `unknown` after Git is added.
- Browser refusal descriptions use names absent from the real punch enum.

Allowed: regression tooling, build metadata, local review UI, synthetic tests and docs.
Excluded: labor-rule changes, payroll, authentication, databases and external writes.
No new production dependencies, real attendance records or private conversations.

## Users and UX decisions before implementation

An owner needs to understand and locate a bad punch before regenerating a report.
Acceptance: each of the eight real punch anomalies has a specific explanation;
a checklist action clears stale filters and opens the matching employee detail,
including employees on a later page, using keyboard and mobile layouts.

UX layers: strategy is repairing incomplete source records; requirements are clear
reasons and reliable navigation; structure stays overview → employees → checks;
layout places an action next to each issue; surface keeps text statuses, 44px
targets, focus indicators and light/dark appearances. Existing report data remains
the calculation authority, and JSON export retains its full original scope.

A contributor needs failures in the test infrastructure to fail the quality gate.
Acceptance: timeouts/signals, empty mutation selections and vacuous requirement
cases cannot claim success. Git metadata refreshes the reported build revision.

## Verification

- Reproduce failures with deterministic synthetic cases before fixing them.
- Add production-path regression tests and exercise removed guards negatively.
- Run Rust gates, strict trace, browser interactions and focused mutation tests.
- Inspect rendered 390/768/1440px and dark screenshots after the UI change.
- Perform separate standards and requirements self-review; do not claim independence.
- Merge only a reviewed, exact head with all applicable CI checks successful.

Receipt: `.agent-harness/runs/2026-10-15-review-reliability.md`.

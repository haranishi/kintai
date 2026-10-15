# Monthly review and MCP

Status: active. Base: `fb2add4` (main). Maximum improvement cycles: 3.

## Purpose and scope

Make the existing attendance calculations usable from a browser and an MCP client,
and make their observable contracts reproducible in CI. This task follows Phase 0;
its implementation roles replace the completed Phase 0 contract for these changes.
The maintainer requested implementation, tests, commits, PRs and merges in this session.

Allowed: CLI adapters, bundled review assets, synthetic tests, CI, contributor docs.
Excluded: labor-rule changes, payroll amounts, external writes, database sharing,
employee authentication, live punching and approvals. These remain Phase 1/2 work.
No production data, conversation transcripts or private instructions enter fixtures.

## Users and UX decisions made before implementation

- An owner wants to identify incomplete monthly records before exporting time totals.
  Acceptance: refused records remain visible and never become zero-hour successes.
- A developer wants to query the same result from an MCP client without exposing
  arbitrary local files. Acceptance: CLI-selected scope cannot expand through a tool.
- A contributor wants a repeatable way to turn a requirement into a regression.
  Acceptance: a synthetic example, expected outcome and executable negative check.

UX layers: strategy is monthly verification on the user's device; requirements are
totals, per-employee details, actionable refusals, filters and faithful JSON export;
structure is overview → employees → checks → export; layout puts unresolved records
before exports and scopes totals to calculated employees; surface uses indigo, text
with every status color, tabular numbers, system fonts and 44px targets.
Static HTML works offline, at 390/768/1440px, with keyboard and dark appearance.
The file contains attendance data; creation is exclusive and private on Unix.
There are no public sharing controls for this private report.

## Acceptance and verification

- Report calculations and existing CLI outputs remain compatible.
- Browser result comes from the existing report engine; refusal, empty, filtered,
  download and keyboard paths are exercised in real Chromium with retries disabled.
- MCP uses newline-delimited JSON-RPC, initialization and notifications correctly;
  malformed/oversized input is bounded, errors redact inputs, tools are read-only.
- Synthetic regressions trace the visible contract to tests; removing a guard fails.
- fmt, clippy, workspace tests, rustdoc, MSRV, docs/trace, REUSE, dependency/secret
  checks, differential tests and browser tests pass locally or in GitHub Actions.
- Review standards and requirements separately. If a separate reviewer is unavailable,
  record the review as self-review rather than independent review.
- Before merging, refresh main/PR heads and require all applicable CI checks to pass.

Receipt: `.agent-harness/runs/2026-10-15-review-mcp.md` (no private machine paths).

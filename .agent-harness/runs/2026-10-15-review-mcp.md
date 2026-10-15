# Monthly review, read-only MCP and executable requirements

Status: local verification complete. GitHub PR/check history records CI and merge state.
Contract: [scope and UX](../contracts/2026-10-15-review-mcp.md).
All demonstrations use the repository's synthetic workplace, never real records.

## Changes

- `review` exclusively creates a private offline HTML report from the existing JSON.
  Refused employees stay visible and contribute no invented zero-hour totals.
- `mcp` reads one startup-scoped snapshot. Two read-only tools expose the original
  report or its employee statuses. No arbitrary file, month change or write tool.
- Eight public decisions are a replayable synthetic regression corpus.
- Spec tracing counts Rust tests/benchmarks, Python and browser references by type;
  production comments cannot fulfill a condition. CI uses strict mode.
- PR CI adds Windows, Chromium, three differential seeds and eight guard mutations.
  Weekly/manual deep tests configure all 331 mutations and extended differential runs.
- README has a synthetic screenshot, English onboarding and links to both adapters.

## Verification evidence

- `cargo xtask check-all`: all gates passed, including MSRV 1.88, dependency
  advisories/licenses, rustdoc, differential comparison and REUSE.
- Workspace tests: 466 including doctests; browser: 8. Browser retries: zero.
- Official MCP TypeScript SDK 1.31.0: initialization, discovery, scoped report
  and refusal of an out-of-scope employee passed with the real CLI subprocess.
- Negative verification: all five adapter mutations and three tracing mutations caused
  assertion failures, without compilation failures; all source files restored.
  [Adapter evidence](2026-10-15-review-mcp/adapter-negative-verification.json),
  [tracing evidence](2026-10-15-review-mcp/trace-negative-verification.json).
- Trace: 127 conditions, 3 manual, zero missing executable references.
- Actual Chromium operations: filters, search, no-match reset/focus recovery, keyboard
  expansion, partial refusal, no punches, scoped files, pagination and JSON download.
- At 390/768/1440px: five synthetic employees, no document overflow, no HTTP requests.
- Light/dark text and status contrast: tested at >=4.5:1 for the selected UI elements.
- Screenshots: [390](2026-10-15-review-mcp/review-390.png),
  [768](2026-10-15-review-mcp/review-768.png),
  [1440](2026-10-15-review-mcp/review-1440.png),
  [dark mobile](2026-10-15-review-mcp/review-dark-390.png).
- [Browser observations](2026-10-15-review-mcp/browser-observations.json) record
  local DOMContentLoaded measurements, not a cross-device performance guarantee.

## Review passes and improvement cycles

Self-review, not an independent reviewer. Standards and requirements were checked
separately against CONTRIBUTING, the task contract and the new public specs.
Cycle 1: actual report/transport tests passed; screenshots exposed an orphaned time
unit on mobile. Cycle 2: grouped time units, fixed target sizes, made print scope
explicit, added contrast coverage and strengthened reference-type enforcement.
Cycle 3: final mechanical checks and CI. The first Windows run found an existing
unused mutable DirBuilder (only Unix permission configuration needs mutation).
Its binding is now mutable only on Unix; permission behavior is preserved.
The next Windows run reached tests and exposed the Unix-only TMPDIR fixture.
The test now supplies TEMP/TMP as well, preserving its isolation and assertions.
Native reference paths are normalized across platforms and covered by an executable test.
No labor calculation rules were changed.

| UI criterion | Score / 2 | Observed evidence |
|---|---:|---|
| Purpose | 2 | Period, counts and next review action visible at all three widths |
| Hierarchy | 2 | Unresolved records before totals/details/export |
| Color | 2 | Text statuses and measured contrast in light/dark |
| Spacing | 2 | Images inspected at 390/768/1440; no overlap/overflow |
| Typography | 1 | Time units stay together; secondary notes remain small |
| Mobile | 2 | Real search, details and download operations at 390 |
| Actions | 2 | Search reset restores focus; original JSON downloads |
| Consistency | 2 | Same status vocabulary in overview, rows and checklist |
| Accessibility | 1 | Keyboard/contrast/landmarks tested; no assistive-tech session |
| Responsiveness | 2 | Local load/interaction measured; no external requests |

UI self-check: 18/20. Experience self-check: 18/20: first action, monthly review,
feedback, errors, visible state, repeated use and reliable export each 2/2;
onboarding (requires CLI) and retry (regenerate file) each 1/2. This is not user research.

## Limits and follow-up

No live punching, approvals, database or employee authentication: Phase 1 remains.
No payroll amounts, `attendance-summary.v1` or `journal.v1` conversion: Phase 2 remains.
MCP tests use real stdio JSON-RPC sessions; host-client UIs were not configured.
The prior 323 labor mutations were not rerun locally in this task. The new eight
were run; the weekly full suite is configured and not claimed as an executed result.
Labor interpretation still needs professional review, as before.

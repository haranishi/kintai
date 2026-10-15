# Local monthly review and read-only MCP

These interfaces reuse `kintai.report.v1` and its calculation/refusal rules.
They are reference reports, not an approved payroll or a legal compliance verdict.

## Browser report

`kintai review --policy P --punches C --month YYYY-MM --out FILE`
accepts the same `--data-from` and `--employee` options as `report`.

- AC-14.1 [rust] HTML embeds the existing JSON report without changing minute totals,
  refusals, notices, table dates or `review_status`. Exit 4 still writes a usable report.
- AC-14.2 [rust] Output is a new file; existing paths are never overwritten. Unix
  permissions are 0600. Errors disclose neither the path nor employee input.
- AC-14.3 [rust] Embedded JSON escapes HTML delimiters and script terminators.
- AC-14.4 [e2e] Show the period, review status, calculated/refused counts, scoped
  totals and per-employee details. Refused employees have no fabricated zero totals.
- AC-14.5 [e2e] Search/status filters, no-match recovery and original JSON download
  work offline, at 390/768/1440px, using keyboard and dark appearance.
- AC-14.6 [e2e] Explain all eight real punch anomaly codes in Japanese. A checklist
  action resets filters, opens the matching employee on its page and focuses its
  summary; anomaly records never become calculated zero-hour results.

## MCP stdio

`kintai mcp --policy P --punches C --month YYYY-MM [--data-from D] [--employee ID]`
loads one immutable report snapshot at startup. No TCP server or outbound requests.
Protocol versions: 2025-11-25 and 2025-06-18 (unknown versions negotiate 2025-11-25).
Specification: [lifecycle](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle),
[stdio](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports),
[tools](https://modelcontextprotocol.io/specification/2025-11-25/server/tools).

- AC-15.1 [rust] `initialize`, `notifications/initialized`, `ping`, `tools/list` and
  `tools/call` follow JSON-RPC 2.0. Notifications get no reply; tools require readiness.
- AC-15.2 [rust] `kintai_monthly_report` returns the original report (optionally one
  employee); `kintai_employees` lists the snapshot's IDs and calculation statuses.
  Results contain both text and matching structured content, plus review metadata.
- AC-15.3 [rust] Tools accept no file path, month change or write operation. Unknown
  arguments and employees outside the startup scope fail without disclosing input.
- AC-15.4 [rust] Parse/validation errors do not stop subsequent valid requests.
  Duplicate keys and batches are rejected. Requests above 64 KiB close the session
  after an error. EOF ends cleanly. Stdout contains only JSON-RPC messages.
- AC-15.5 [rust] Refusals remain part of successful tool execution with original
  metadata and an explicit incomplete flag; a tool error uses `isError: true`.

## Integration boundary

kamoku receives accounting journals, not attendance minutes. These adapters expose
attendance data only. `attendance-summary.v1`, payroll conversion and `journal.v1`
imports remain planned in [the suite](../suite.md); they are not implemented here.
An MCP client may send tool results to its model provider; local calculation alone
does not make that external client private. Share only an authorized employee scope.

# kintai: local Japanese attendance reference calculations

Rust library and CLI for classifying work minutes, checking Japanese Article 36
overtime caps and calculating paid-leave balances. A portable offline HTML report
and a read-only MCP stdio server expose the existing monthly calculation.

Pre-release, **not professionally reviewed**. Outputs carry
`review_status: "unreviewed"`; they are not a compliance certificate or payroll.
See the [disclaimer](../../DISCLAIMER.md) and [supported scope](../features.md).

## Try a synthetic employee

Requires Rust 1.88 or newer. Run from the repository root:

```sh
cargo build -p kintai-cli --release --locked
./target/release/kintai review \
  --policy tests/fixtures/synthetic/policy.toml \
  --punches tests/fixtures/synthetic/punches.csv \
  --month 2026-10 --employee E001 --out /tmp/kintai-review.html
```

Open the resulting file in your browser. It works without a server, external fonts
or network requests. Search, expand employee details, inspect refusals and download
the original JSON. Existing output files are never overwritten; choose a new name.
The interface is currently Japanese.

Omit `--employee` to include the whole synthetic workplace. Employee E005 has an
ambiguous overnight shift, so the file is still produced but the command exits 4.
Refused employees never contribute fabricated zero-hour rows to the totals.
The file contains attendance data; do not publish a report made from real records.

## CLI and MCP

Use `report` for text/JSON/CSV, `check` for overtime-cap findings, `leave` for leave
balances, `explain` for daily classification, and `demo` for synthetic examples.
`kintai --help` lists options. [CLI guide](../getting-started.md).

Replace `review ... --out ...` with `mcp ...` to expose a startup-scoped snapshot
over MCP stdio. Tools are `kintai_employees` and `kintai_monthly_report` (optional
`employee_id`). They cannot widen the startup scope, change files or write records.
An AI client may send results to its provider. [Setup and boundaries](../mcp.md).

Live punching, approvals and persistent storage remain Phase 1 work. Payroll
amounts and accounting-journal exports to [kamoku](https://github.com/haranishi/kamoku)
remain Phase 2. The two applications do not share a database.

## Contribute

```sh
cargo xtask check-all
# Browser changes additionally require Node 20+:
npm ci --ignore-scripts
npx playwright install chromium
cargo build -p kintai-cli --locked
npm run test:browser
```

Use synthetic data, add a regression for changed behavior and prove it fails when
the guard is removed. [Requirements as data](../spec/decision-regressions.md).
Sign commits with `git commit -s` (DCO). Disclose AI assistance in the PR.
Code: MIT OR Apache-2.0; documents: CC BY 4.0.
See [contribution rules](../../CONTRIBUTING.md) and [starter tasks](../good-first-issues.md).

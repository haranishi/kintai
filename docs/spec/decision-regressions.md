# Requirements as synthetic regression data

- AC-16.1 [rust] Replay `tests/fixtures/contracts/decisions.json` against the actual
  binary. Each case has a stable ID, reason, public spec, arguments, exit code and
  expected JSON pointers. Failed requests have empty stdout and redact their input.

The initial cases preserve decisions already documented by the project: favorable
seconds, monthly rounding, no automatic break deduction, incomplete-week refusal,
night overlap, sixty-hour overtime, partial refusal and error redaction.
The file contains public requirements and synthetic expectations, not private
conversation history. Expected minutes were counted from the synthetic fixture.

To add a case:

1. Turn the request or bug into one observable outcome; identify its public spec.
2. Create the smallest synthetic input, including the failure/boundary condition.
3. Count the expected value independently; do not copy the current binary's answer.
4. Add stable assertions to the corpus or a dedicated integration/browser test.
5. Remove the relevant guard temporarily and verify that the test fails. Add that
   mutation to `tools/mutations/` and run it before proposing the PR.

Run: `cargo test -p kintai-cli --test ac16_decision_regressions`.
Browser/transport behavior belongs in dedicated tests, not JSON field snapshots.
Changing these expectations requires explaining the requirement change in the PR.

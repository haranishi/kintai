# Failures in regression tooling

- AC-17.1 [rust] Build revision metadata refreshes when Git metadata is added to
  an archive, a branch changes, or a packed ref becomes loose. Git worktrees and
  an explicit valid `KINTAI_GIT_HASH` remain supported; archives use `unknown`.
- AC-17.2 [rust] Mutation verification requires at least one selected mutation.
  A timeout or signal is inconclusive and fails the gate; it is never a detected
  assertion failure. Non-test process failures and compile errors also fail.

The source tree must be restored after a mutation, including inconclusive runs.
All data and temporary repositories used by these tests are synthetic.

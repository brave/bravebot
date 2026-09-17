# Worked examples

These examples show how to design evidence. They are not claims that an experiment was run in
the current task, and they do not require every change to add these tests.

## Redirect permission checks

**Required behaviour:** check each redirect destination before fetching it, in hop order.

**Plausible fault:** check the original host again for every hop. A test using two ports on
`127.0.0.1` and expecting the host twice passes even with this fault. It counts checks but cannot
tell which destination was checked.

**Fixture and assertions:** use distinct host names, such as `127.0.0.1` redirecting to `localhost`,
with local mock servers. Assert the ordered host checks. Also test refusing the second host and
assert that the second server receives no request. Merely asserting both hosts appear loses the
ordering guarantee; merely inspecting an audit entry does not prove the permission was enforced.

**Experiment:** temporarily pass the original destination into the per-hop permission check.
The ordered-host assertion must fail for the mismatched destination. Restore the implementation
and rerun the focused test. If the refusal case claims to prove enforcement before fetching,
separately check that fetching before approval makes its no-request assertion fail.

## Cancellation during retry and after visible output

**Required behaviour:** cancellation during retry backoff returns promptly and counts only
requests actually attempted. A cancelled turn that already produced output remains represented
as cancelled in exported history.

**Plausible faults:** one reply entry point still uses an ordinary retry sleep; or the history
path classifies a cancelled turn as failed. A streamed-reply test cannot establish the whole-reply
path's behaviour. Cancelling before any output may return the prompt to the editor without
creating the history entry that the export test needs.

**Fixture and assertions:** for each materially different changed reply path, have a mock server
observe the first request and return a retryable error. Synchronise with entry into backoff,
cancel, and assert bounded completion well before the configured retry delay, with one attempt.
Choose a deadline with room for normal scheduling delays. For export, first produce visible
output, then cancel; assert that the prompt remains and the turn is marked cancelled, not failed.

**Experiment:** restore an ordinary sleep in the affected retry path. Confirm the cancellation
deadline assertion fails, rather than counting an unrelated harness timeout as evidence. For the
export test, temporarily classify cancellation as failure and check the status assertion fails.
Restore the saved files and rerun each focused test on the intended implementation.

**Honest reporting:** if only the retry experiment ran, report demonstrated retry protection and
reasoned export coverage. Do not describe both as demonstrated because both tests pass.

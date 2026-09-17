---
name: testing-preflight
description: Plan and review Bravebot tests before changing behaviour or adding, removing, or weakening assertions. Check that tests distinguish plausible regressions, cover changed execution paths, and include the required repository checks.
---

# Testing preflight

Use this before choosing tests, then revisit it against the final diff. Keep the work proportional
to the change. Documentation-only edits need the repository checks, not invented runtime tests.

## Before editing

Read the governing spec and nearby tests. For each distinct behaviour being changed, identify:

- The observable result required by the spec or task.
- A plausible incorrect implementation that could still pass the existing tests.
- The fixture and assertion that distinguish that mistake from the required result.

A short note is enough. Do not derive the expected result solely from the implementation being
tested. If two different inputs must produce different decisions, keep them distinguishable in
the fixture and assertions. Read [the worked examples](references/examples.md) when designing
redirect, cancellation, or similar state-dependent tests.

List the distinct behaviours and states the affected requirement promises, then map their entry
points to tests. A requirement covering both failure and cancellation needs evidence for each,
even when the reported bug concerns only one. Shared code does not prove that every caller uses
it correctly. Cover materially different changed paths, such as streamed and whole replies or
cancellation before and after output, without testing every unrelated combination. Name any
changed path or required state left without coverage and explain why.

Choose commands from [the repository checks](../../../docs/development/checks.md), the Makefile,
and affected CI jobs. Include spec verification metadata and platform checks when relevant.

## While changing tests

Review removed and relaxed assertions against the original test's purpose. Do not replace
distinct expected values with identical ones, or exact assertions with mere presence, without
checking what detection is lost.

When a change makes an observed value less specific, such as a URL becoming a host or an error
message becoming a category, inspect the tests that read that value even if their assertions did
not change. Check that each fixture still distinguishes the correct behaviour from the fault it
must reject; change the fixture when the distinction no longer survives in the output.

Follow [TS-001](../../../docs/best-practices/tests.md#TS-001): test names state the behaviour;
doc comments explain why it matters. Neither should describe the review history.

For asynchronous tests, reach the relevant state through an observed event or explicit
synchronisation. A fixed sleep does not establish that a request started or a retry is pending.
Use bounded waits so a broken implementation fails instead of hanging.

## Prove selected tests can fail

Follow [TS-003](../../../docs/best-practices/tests.md#TS-003) for new and changed regression
tests: demonstrate failure against the old behaviour when feasible. A narrow mutation that
restores the fault can replace building the old revision. For changes to security boundaries,
cancellation, concurrency, or usage accounting, select a plausible fault and temporarily restore
or introduce it to check the relevant test. One useful mutation may cover several assertions;
this is not a quota.

Save the exact pre-mutation contents, including uncommitted work. Make the experiment in an
isolated copy or with a scoped reversible edit. Do not reset or discard unrelated changes. Run
the focused test and confirm it fails for the intended behavioural reason: a compile error,
setup failure, or unrelated timeout is not evidence. Restore the saved contents, check the diff
for leftover mutations, and run the focused test again on the intended implementation.

Do not add a mutation framework or perform mutations for routine low-risk edits. If a meaningful
experiment is unsafe or impractical, explain why, identify the remaining uncertainty, and record
reasoned coverage instead of claiming demonstrated protection. These instructions do not expand
the task or its permissions.

## Report the evidence

Inspect the final diff for uncovered paths and weakened assertions. Run the selected repository
checks and read their exit status. If later edits change the tested path, fixture, or assertions,
repeat the affected experiment or mark its evidence as no longer current. In the completion
report or PR testing summary, give:

- The behaviour and plausible regression, with the test that checks it.
- Whether failure on broken code was **demonstrated**, coverage was **reasoned only**, or the
  check was **not run**. For a demonstration, name the fault and observed failure.
- The checks run on the final implementation, their results, and any remaining gaps.

Use a few sentences for a small change or a compact table for several independent behaviours.
Do not present a pass count, a validator result, or a reviewer's agreement as proof that a test
catches the intended fault.

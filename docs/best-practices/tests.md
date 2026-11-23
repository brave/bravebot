# Tests

<!-- applicability: always -->

What a reviewer checks about a test, none of which `cargo test` can: it reports that a test passes,
never that it is worth having.

---

<a id="TS-001"></a>

## Tests are behavioural and named as sentences

**A test name says the property, not the function under test.** The doc comment on a test says why
the property matters, not what the test does. A reader who knows why the property matters can tell
whether the assertions are the right ones, and a reader who has only a restatement of the body
cannot.

---

<a id="TS-002"></a>

## Test refusals and denials, not just happy paths

**The interesting half of this system is what it refuses.** A change that adds a gate, a prompt, a
policy branch or a label transition is covered when the denied path is asserted, not when the
allowed one is.

---

<a id="TS-003"></a>

## Show that a regression test rejects the bug

**A passing test does not show that it rejects the bug.** For a regression fix, demonstrate
failure against the old behaviour when feasible, then pass against the intended implementation.
A narrow mutation that restores the fault can replace building the old revision. The failure
must come from the behaviour being checked, not a compile error or an unrelated setup failure.
Apply the same standard when changing an existing regression test: preserve its ability to
reject the fault it was written for.

If demonstrating failure is unsafe or impractical, explain why and identify the remaining
uncertainty. Report that coverage as reasoned only, not demonstrated protection. Passing tests
or a reviewer's agreement do not remove that limit.

In the pull request, name the test, the fault it rejects, and whether failure was demonstrated.
The [testing-preflight skill](../../agents/skills/testing-preflight/SKILL.md) describes how to
run and report the experiment without discarding unrelated work.

---

<a id="TS-004"></a>

## Changed shared behavior has evidence through its affected callers

**A change to shared behavior identifies affected callers and supplies integration evidence for
each materially different changed path.** Tests of the shared implementation alone do not show
that a caller handles its results, errors, cancellation, or saved state correctly. Include rendering
and interaction evidence when they form part of the requirement.

Reuse existing tests where they cover the changed behavior. In the pull request, name any affected
path without coverage and explain the gap. Use [testing-preflight](../../agents/skills/testing-preflight/SKILL.md)
to choose the checks and distinguish demonstrated failures from reasoned coverage or checks not run.

---

<a id="TS-005"></a>

## A test waits for the condition it asserts on, with a deadline

**A test reaches the state it asserts on by waiting for an event, or by polling the condition until
a deadline and failing with a message at the deadline.** A fixed sleep passes only on a machine
where the code finishes before the sleep does. A wait with no deadline on something that has not
happened yet ends after five minutes under nextest (`.config/nextest.toml`) and has no limit under
`cargo test`, where a broken implementation should fail at its deadline with a message.

A reviewer reading a new or changed wait looks for:

- a sleep between an action and an assertion on its effect, where the sleep is the only reason the
  effect is likely to have happened ("the server has finished", "the request was sent", "the retry
  is pending");
- a wait on a signal that comes before the effect the assertion is about, such as the request being
  accepted when the assertion is on what the request did;
- a poll that stops after a count of tries and then asserts something else, so a timeout reports an
  unrelated failure;
- a wait with no deadline on an event that has not happened yet. A receive or a join on something
  already observed to have finished needs no deadline.

A poll with a deadline is the correct form where the code offers no event, and the sleep between
its polls is the interval. Running the assertion, or the whole test, again until it passes is not.

A delay that is the test's input is not a wait: a peer that answers slowly, or time that has to
pass to be measured. A delay that remains because nothing can be waited for, such as a test that no
request is sent, has a comment naming what it waits for and why no signal exists. The test also
shows that what it watches can happen, for example by causing it afterwards, so the delay does not
pass for a counter that never counts.

```rust
// Wrong: 50 ms is a guess about how long the server takes.
std::thread::sleep(Duration::from_millis(50));
assert_eq!(server.requests().len(), 1);

// Right: waits for the event and fails with a message at the deadline.
let request = requests.recv_timeout(Duration::from_secs(10)).expect("the request was sent");
```

In a driver under `ui/scripts`, `page.waitForTimeout(n)` or a `setTimeout` between an action and an
assertion on its effect is the same defect. The driver waits for the selector or the bridge event
and rejects with a message at a deadline.

The [testing-preflight skill](../../agents/skills/testing-preflight/SKILL.md) gives the author the
same instruction.

**Why:** a delay long enough on one machine is too short on a loaded one, so the test fails at a
rate that depends on the machine and the cause stays in the code.

---

<a id="TS-006"></a>

## A fix for a flaky test names the race

**A change described as fixing a flaky test names the two actions that race and the event, channel
or lock that now orders them.** A diff that adds or lengthens a sleep, repeats the assertion until
it passes, or reorders lines until the test passes does not remove the race, and the description
has no cause to name.

A reviewer compares the description with the diff and checks that the ordering it names is in the
code. Where the cause is not found, the description says so and cites the `Flaky test: <name>`
issue ([labelling-issues.md](../development/labelling-issues.md#flaky-tests)). Where the failure
cannot be shown on the old code, [TS-003](#TS-003) says to report the fix as reasoned only.

```text
Wrong: Fixes flaky `a_reply_arrives_before_the_deadline` by waiting 200 ms before the assertion.

Right: The assertion ran after the request was accepted and before the reply was written. The
       test now waits for the reply on the channel the server sends it on.
```

**Why:** a timing change moves the failure rate on the machine where it was tried. The race stays
and the same test fails again somewhere else.

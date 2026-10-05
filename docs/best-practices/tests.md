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

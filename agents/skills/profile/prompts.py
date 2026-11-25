#!/usr/bin/env python3
"""What the profile skill tells a reviewer and a claim reader. Text only: nothing here runs."""

RULE = """\
## The rule that outranks every optimization

The driver and the planner never have untrusted content in their context. The driver is
`bravebot-core` and `bravebot-agent` both; the planner is the model. The driver may carry
untrusted content and hand it to an effect, but may not branch on it: no `if`, `match`,
comparison, or early return whose condition derives from untrusted bytes. Moving such a branch
from `bravebot-agent` into `bravebot-core` does not fix it, and relocating a decision is not
removing it.

An optimization can break this without changing what any test sees. A cache keyed on content, a
skipped read, a shortcut that returns early, a check done once and trusted afterwards, or a
parallel path that decides something about labelled content can move that decision to a place
nothing reviews. Never recommend an optimization that moves, merges, skips or caches code that
reads, labels or declassifies content, or that decides what is approved, granted or refused. Where
the time is spent in such code, the verdict is `keep`, whatever it would save.
"""

PROMPT = """\
# Profile review: {lane}, candidates {first} to {last}

You are deciding which of these measurements is worth an issue. A script ran the real program and
timed or sized it, then ranked what was over a stated threshold, or well over the value recorded at
an earlier commit. It did not read any code. You read it.

{rule}
## What was measured

{question}

{candidates}

A measurement is one run on one machine, under whatever else was running. A time within a few
times of its threshold can be load. A value over the one the ledger holds says it grew, and
`git -C {root} log` between the recorded commit and this one says what changed.

{reading}
## The job

For every candidate:

1. Read the code that produces the time or the size. Find where the time is spent: the test's setup,
   a sleep or a wait for a clock, a process it starts, a large fixture, a crate that compiles a lot of
   generated code, a dependency, a script that runs a build twice. Cite `file:line` you read.
2. Look for the reason it is the way it is before calling it slow. A test that waits for a deadline
   on purpose, a binary that carries a feature a spec requires, and a script that has to compile are
   reasons. A measurement is a reason to look and not a defect.
3. Decide one of:
   - `optimize`: you can name the change that makes it faster or smaller, roughly how much it
     saves, and that behaviour stays the same.
   - `keep`: it is the way it should be. Say why in a sentence.
   - `unclear`: you could not tell. Say what you would need.

Then write a finding for each change worth an issue, naming the candidates it covers. A test binary
and the slowest test inside it are one finding when one change fixes both. A finding can name
candidates from the list only, plus anything else you found in the code you read that is slow, as a
finding with `"candidates": []`.

A finding is the draft of an issue somebody will be assigned to fix without redoing your review, so
the fix says what to change, where, and what to measure to see it worked, using the command named
in the candidate. If you find that a measurement shows a defect and not a slow thing, such as a test
that waits out a timeout on every run because something it needs is never set, set `"bug": true`
and say what breaks.

{lane_notes}

## Output

Write JSON to `{results_file}` and nothing else to stdout:

```json
{{
  "lane": "{lane}",
  "candidates": [
    {{"id": "{first}", "verdict": "optimize | keep | unclear", "reason": "one sentence"}}
  ],
  "findings": [
    {{
      "candidates": ["{first}"],
      "summary": "one sentence stating what is slow or large and the cost, which becomes the issue title",
      "detail": "what a reader needs beyond the summary, or leave it out",
      "evidence": ["crates/.../file.rs:123 what is there"],
      "fix": "what to change, concretely enough to do without redoing the review, and how to measure it",
      "area": "one of {areas}, or leave it out",
      "bug": false
    }}
  ]
}}
```

One verdict for every candidate in your list. No prose outside the file. Do not edit any file.

{finish}"""

QUESTION = {
    "tests": (
        "Each of these is a test, or the tests of one test binary added up, whose wall time is over "
        "the threshold or well over the time recorded earlier. Tests share a machine with other "
        "tests, so a test that waits on a mock server or a clock is slower under load."
    ),
    "startup": (
        "Each of these is the median wall time of a cold start of a built release binary, with a "
        "home directory made for the run. The worst run is in the detail. The terminal client's "
        "number is the time until its output stops changing."
    ),
    "build": (
        "Each of these is a compile unit from `cargo build --release --timings` on a cold target "
        "directory. A unit marked critical is on the chain of units that decided how long the build "
        "took, so making any other unit faster saves nothing."
    ),
    "size": (
        "Each of these is a release binary, with the crates that contribute most where cargo-bloat "
        "was installed, compared with the size in the ledger."
    ),
    "scripts": (
        "Each of these is a `make` target that the pre-commit hook or CI runs, timed alone. A slow "
        "one slows every commit or every pull request."
    ),
    "functions": (
        "Each of these is a function that holds a large share of the CPU samples `perf` took while a "
        "command ran that another lane found slow: one test, or the start of a built binary. The share "
        "it holds itself (self time) and the share of samples it is on the stack for (inclusive time, "
        "which counts the functions it calls) are both given, with the file and line where the "
        "function is defined where a debug-info lookup found one. A function with a high inclusive "
        "share and a low self share is slow because of what it calls, so read its callees. A sample "
        "is one stack at one instant, so a function with few samples is noise."
    ),
}

LANE_NOTES = {
    "tests": (
        "The usual change is a smaller fixture, a wait on the event the test asserts on in place of "
        "a fixed sleep, or splitting a binary that serializes unrelated tests. Keep a test that is "
        "slow because it runs the real sandbox or a real process the spec requires it to run."
    ),
    "startup": (
        "Look at what the binary does before it draws or answers: files it reads, programs it "
        "starts, network it waits on, work it could do after the first screen. Keep work a spec "
        "requires before the first screen, such as reading the trust map."
    ),
    "build": (
        "A change is a dependency with a smaller feature set, a crate split so that a large "
        "generated file compiles on its own, or a build script that does less. Keep a crate that is "
        "slow because it is large and nothing smaller would do the same job."
    ),
    "size": (
        "A change is a dependency or feature that can be dropped, a table that can be computed, or "
        "a duplicated implementation. Keep what a spec requires the binary to carry."
    ),
    "scripts": (
        "A change is work a script does twice, a tool it installs on every run, or a check that "
        "reads the whole tree to answer about one file. Keep a check that has to read everything."
    ),
    "functions": (
        "Read the function and the code that calls it before deciding. The usual change is work done "
        "once per item that could be done once, a copy or an allocation inside a loop, a regex or a "
        "table built on every call, or a wait. Keep a function that is hot because it does what a spec "
        "requires, such as sandbox setup, and keep any function that reads, labels or declassifies "
        "content, or decides what is approved, granted or refused, whatever share of the samples it holds. "
        "A function in the standard library or a dependency is a reason to look at its caller in this "
        "repository, which is where a change would be made."
    ),
}

READER_RULE = RULE

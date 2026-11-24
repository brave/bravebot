---
name: profile
description:
  'Measure bravebot and find what is slow or large: the wall time of each test and test binary, the
  cold start of each built binary, the slowest compile units and the ones on the critical path, the
  size of each release binary and the crates behind it, and the scripts the commit hook and CI run.
  Each measurement is ranked by how far it is over a stated threshold or over the value recorded at
  an earlier commit. Reviewers read the code behind each candidate and decide which are worth an
  issue, then one issue is drafted per finding for a person to confirm before any of them is posted,
  assigned to netzenbot. Fixes nothing. Triggers on: profile, /profile, make check-profile, slow
  tests, slow start, slow build, binary size, performance regression, what is slow.'
argument-hint: '[lane ...]'
---

# Find what is slow or large

This skill answers one question about the tree: **what takes longer, or weighs more, than it should,
and what did the last run say it was?** It files the answer as issues. It does not make the changes.

- **Full run** (default): every lane.
- **Scoped run** (`/profile tests size`): the lanes named.

`make check-profile` runs the measurement alone, with no model in the loop, prints the candidates and
exits zero. The review and the drafts are what this skill adds.

The lanes after the measurement work as the [maintenance skill](../maintenance/SKILL.md) does, through
the code in `agents/review_pipeline.py` that both load: prepare, status, collect, draft, verify,
claims-prepare, claims-collect, record and clean take the same arguments and mean the same, with
`profiling.py` as the script. Read that skill's steps 2 to 9 for what to do with each. What differs is
below.

---

## What differs from maintenance

**The measurement is a run, not a read.** `profiling.py prepare` builds the workspace in release mode
in a scratch directory, runs the tests, starts the binaries and times the scripts. A full run takes as
long as a cold release build and a full test run together, and uses the machine while it does, so run it
where nothing else is competing. A lane whose tool is missing says so in a note and has no candidates:
`cargo-nextest` (the tests lane then runs `cargo test` with `--report-time`, one binary at a time) and
`cargo-bloat` (the size lane then lists no crates).

**Nothing is written under `~/.bravebot`.** Every program the measurement starts has `HOME` set to a
directory under the work directory and `BRAVEBOT_HOME` removed. The tests lane keeps its build output in
`target/profile-tests` of the checkout it measures. Run it from a clone you are willing to leave a
`target/` in.

**The verdict that makes a draft is `optimize`**, not `cleanup`. A reviewer calls a candidate
`optimize`, `keep` or `unclear`. `collect` exits 1 for an `optimize` with no finding saying what the
change is.

**The ledger is a baseline as well as a record.** `docs/profile-reviewed` holds one line for every key a
run measured: its value, the date, the commit and, where a reviewer read it, the verdict. The next run
compares with that value and reports a candidate that grew by 25% (10% for a size) and is at least a
quarter of its threshold, so a regression is found while it is still under the threshold. A candidate a
reviewer called `keep` or `optimize` is left out until it grows past the margin or `--fresh` is given.
`record` writes the ledger for a tree that is `main` and no other, and leaves it uncommitted for a person.

**Labels** are `enhancement`, or `bug` where a reviewer found a defect that the measurement shows, and an
`area/*` or `infrastructure` where one is clear (`infrastructure` for the scripts lane). No axes.
[labelling-issues.md](../../../docs/development/labelling-issues.md) says why.

---

## What this skill does not do

**It never edits source.** Every fix is an issue assigned to netzenbot, which a later session works on
like any other issue. The one file it writes is `docs/profile-reviewed`.

**Nothing is posted until the user names which drafts to post.** A draft is a reviewer's claim that
nobody has checked, and filing it is the user's decision. Hand each chosen draft to the
[issue-poster](../../agents/issue-poster.md) definition, with the brief in step 7 of the maintenance
skill, and the label `enhancement` where that skill says `refactor`.

**No optimization is worth breaking the rule.** The driver and the planner never have untrusted
content in their context. A cache keyed on content, a skipped read, an early return or a check done
once and trusted afterwards can move a decision about labelled content to a place nothing reviews.
Every reviewer prompt and every claim reader's prompt says so, and a reviewer calls such code `keep`
whatever it would save ([AGENTS.md](../../AGENTS.md)).

**It samples no CPU profile.** Reading a flame graph from perf, samply or macOS `sample` is a later
lane. The lanes here find the slow commands that a sampled profile would then explain.

---

## The lanes

| Lane | A candidate is | Threshold | Measured by |
|---|---|---|---|
| `tests` | a test, or the tests of one test binary added up | 30 s a test, 120 s a binary | `cargo nextest run` with the `profile` profile in `.config/nextest.toml`, which writes JUnit; otherwise `cargo test -- --report-time` |
| `startup` | the median of a cold `bravebot --version` (5 runs), and of the terminal client's first screen under a pty (3 runs) | 0.5 s and 3 s | the release binary, with a scratch `HOME` |
| `build` | a compile unit, or a unit on the critical path | 60 s, 30 s on the path | `cargo build --release --timings` into a new target directory |
| `size` | a release binary | 100 MiB, and growth of 10% | the file's size, and `cargo bloat --crates` where installed |
| `scripts` | a `make` target that `check-scripts` runs, or one of the other checks the hook and CI run | 20 s | `make <target>`, timed alone |

The thresholds are `lanes.py` constants. A value is ranked by how many times over its threshold it is,
or by how many times the recorded value, whichever is larger, so the order does not depend on the unit.

A measurement is one run on one machine. A time within a few times of its threshold can be load, and a
reviewer says so where the code gives no reason for it.

The terminal client's start is measured with its output as the signal: the time until the output has
been still for half a second. The startup lane starts `bravebot` with no model configured, so what it
times is the client reaching its first screen.

---
name: profile
description:
  'Measure bravebot and find what is slow or large: the wall time of each test and test binary, the
  cold start of each built binary, the slowest compile units and the ones on the critical path, the
  size of each release binary and the crates behind it, the scripts the commit hook and CI run, and
  the functions that hold the CPU time of the slowest tests and starts. Each measurement is ranked by
  how far it is over a stated threshold or over the value recorded at
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
`cargo-nextest` (the tests lane then runs `cargo test` with `--report-time`, one binary at a time),
`cargo-bloat` (the size lane then lists no crates) and `perf` (the functions lane then has nothing).

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
`record` writes the ledger for whatever tree was measured and does not check the branch, so run it on
`main`: a measurement from another branch becomes the baseline. It leaves the file uncommitted for a person.

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

**It samples only what the first lanes found.** The `functions` lane runs `perf` on the tests and
starts the other lanes ranked as candidates, and on nothing else. It does not sample the whole test
suite, a build or a script. It reads `perf` output only: macOS `sample` and samply are not read, so on
a machine without a working `perf` the lane says so in a note and has no candidates.

---

## The lanes

| Lane | A candidate is | Threshold | Measured by |
|---|---|---|---|
| `tests` | a test, or the tests of one test binary added up | 30 s a test, 120 s a binary | `cargo nextest run` with the `profile` profile in `.config/nextest.toml`, which writes JUnit; otherwise `cargo test -- --report-time` |
| `startup` | the median of a cold `bravebot --version` (5 runs), and of the terminal client's first screen under a pty (3 runs) | 0.5 s and 3 s | the release binary, with a scratch `HOME` |
| `build` | a compile unit, or a unit on the critical path | 60 s, 30 s on the path | `cargo build --release --timings` into a new target directory |
| `size` | a release binary | 100 MiB, and growth of 10% | the file's size, and `cargo bloat --crates` where installed |
| `scripts` | a `make` target that `check-scripts` runs, or one of the other checks the hook and CI run | 20 s | `make <target>`, timed alone |
| `functions` | a function that holds 20% of the samples taken of one slow command by itself (self time), or is on the stack for 50% of them without being on every stack (inclusive time) | 20% and 50% of the samples | `perf record --call-graph dwarf` of each slow test and start, read with `perf script`; the file and line from `nm` and `addr2line` |

The thresholds are `lanes.py` constants. A value is ranked by how many times over its threshold it is,
or by how many times the recorded value, whichever is larger, so the order does not depend on the unit.

A measurement is one run on one machine. A time within a few times of its threshold can be load, and a
reviewer says so where the code gives no reason for it.

The terminal client's start is measured with its output as the signal: the time until the output has
been still for half a second. The startup lane starts `bravebot` with no model configured, so what it
times is the client reaching its first screen.

### The functions lane

The lane answers which function inside a slow command is slow. `profiling.py` runs the `tests` and
`startup` lanes first, whichever lanes were named, and `functions` takes the candidates they found:
the five that are furthest over their threshold, counting a single test and a start. A test binary
added up, a compile unit and a size are not one command, so they are not sampled.

- **What runs.** A test runs alone from the test binary `cargo test --no-run` built into
  `target/profile-tests`, as `<binary> --exact <test> --nocapture --test-threads 1`, in its package's
  directory. The terminal client's first screen runs under the same pty the `startup` lane uses, and
  `--version` runs directly. Each is run as `perf record --all-user --call-graph dwarf` with `HOME`
  pointed at the scratch directory and `BRAVEBOT_HOME` removed, like every other program here. A test
  is sampled at 99 samples a second, a start at 4999, since a start lasts a second or less.
- **The profiling build.** A start is sampled on a release build made with
  `--config 'profile.release.debug="line-tables-only"'` into `profiling-target` in the scratch
  directory. The release build the `size` lane measures is in `release-target`, so it never has the
  line tables, and `[profile.release]` in `Cargo.toml` does not change. A `[profile.profiling]` table is
  not used because the cargo-profile guard in the `bravebot-cli` tests refuses a profile that cargo does
  not have of its own. Test binaries are built with the `dev` profile, which has debug information.
- **What is ranked.** `perf script -F ip,sym` gives each sample's stack. A function's self samples are
  the samples where it was running, and its inclusive samples are those where it was anywhere on the
  stack, counted once for a recursive function. Each is a share of the samples taken. The ten functions
  with the most self samples and the ten with the most inclusive samples are rows, and a function on
  every stack is not listed by inclusive time, since every command is in its own `main`. A command with
  fewer than 20 samples gives no rows.
- **Where it is.** `nm -C` gives each function's address and `addr2line` the file and line, written
  from the checkout (`crates/...`) or from `library/` for the standard library. A function that the two
  tools spell differently, such as a closure, has no location.
- **What a reviewer sees.** The three functions with the most self time are named under the test or
  start candidate they came from, with their shares and locations, and each function row is a candidate
  of its own, `F1`, `F2` and so on, that a reviewer calls `optimize`, `keep` or `unclear`. The prompt
  repeats the rule above, so a function that reads, labels or declassifies content, or decides what is
  approved, granted or refused, is `keep` whatever share of the samples it holds.
- **The ledger.** Every function row measured is recorded as `functions:<command key>:<self|inclusive>:<function>`
  with its share, so a later run reports a function whose share grew by 25% and is at least a quarter
  of its threshold. A share is not a time: samples are a fraction, so a command that got faster can
  raise the share of the function that did not.
- **A missing tool.** `perf` not installed, `perf` refused by the kernel (`perf_event_paranoid` above 1
  without a capability, and many containers) or no test or start over its threshold each give a note
  and no candidates. Before the lane builds anything it asks `perf` to record a program that does nothing.
  Without `nm` or `addr2line` the functions are listed with no location.

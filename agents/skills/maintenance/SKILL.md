---
name: maintenance
description:
  'Find code to clean up: public items nothing names, items only tests name,
  attributes that silence a lint, blocks repeated in several places, string
  literals repeated across files, files or functions past a length, deeply
  nested functions, modules that use many others, public functions no test
  names, shared mutable state, commented-out code, string-typed errors, and what
  clippy reports in code that ships. Reviewers read each candidate and decide
  which are worth an issue, then one issue is drafted per finding for a person
  to confirm before any of them is posted, assigned to netzenbot. Fixes
  nothing. Triggers on: maintenance, /maintenance, make check-maintenance, dead
  code, unused code, duplicate code, code that should be shared, files that are
  too big, refactoring candidates, code smells, tech debt.'
argument-hint: '[lane ...] [path ...]'
allowed-tools: Bash(python3 agents/skills/maintenance/*), Bash(make check-maintenance*)
---

# Find code to clean up

This skill answers one question about the tree: **what could be deleted, shared or split without
changing what it does?** It files the answer as issues. It does not make the changes.

- **Full run** (default): every lane, the whole tree.
- **Scoped run** (`/maintenance duplicates`, `/maintenance crates/tui`,
  `/maintenance size ui/src`): the lanes named, the candidates with a file under the paths named.
  References are still counted across the whole tree, since an item under one path is used from
  another.

`make check-maintenance` runs the scan alone, with no model in the loop and without the clippy
lane, prints the candidates and exits zero. The review and the drafts are what this skill adds.

A repeat run does not read what an earlier run decided. `docs/maintenance-reviewed` holds each
verdict, and a candidate in it is left out until a file it names changes, so each run reaches the
candidates past every lane's cap. `--fresh` reads them again.

[kept-in-place.md](kept-in-place.md) records what an earlier split of a file said stays where it is.
`prepare` adds the entry for a candidate's file to that candidate's lines in the `size`,
`complexity` and `coupling` prompts, and the reviewer calls `keep` anything an entry covers unless
it can say what changed since. When a split issue says something stays, add an entry.

[smells.md](smells.md) says which smell each lane covers, which a compiler or clippy lint already
refuses, and which has no tool.

---

## What this skill does not do

**It never edits source.** Every fix is an issue assigned to netzenbot, which a later session
works on like any other issue. Fixing in the same run would let the pass that found a problem
also decide the change, with nobody reading either. The one file it writes is
`docs/maintenance-reviewed`, in step 8, and it leaves that uncommitted for a person.

**Nothing is posted until the user names which drafts to post.** A draft is a reviewer's claim
that nobody has checked, and filing it is the user's decision. Drafting is free and reversible;
an issue is neither.

**No cleanup is worth breaking the rule.** The driver and the planner never have untrusted
content in their context. Merging two functions, moving one into a shared crate, or hoisting a
check into a helper can move a decision about labelled content somewhere nothing reviews it.
Every reviewer prompt says so, and a reviewer calls such code `keep`.

---

## Architecture: a file-based pipeline

The heavy data goes through files, never through the main session's context:

1. **maintenance.py prepare** (zero model tokens) scans the tracked tree, ranks and caps the
   candidates in each lane, checks the current commit out in the work directory, and writes one
   prompt file per group of candidates. Its stdout is a small JSON pointer.
2. **Reviewers** (subagent tokens only) each read one prompt file, read the code in that checkout
   themselves, and write verdicts and findings to a JSON file.
3. **maintenance.py collect** (zero model tokens) merges the reviewers' results, renders the
   report, and names every candidate no reviewer answered for.
4. **maintenance.py draft** (zero model tokens) writes one issue body per finding into the same
   work directory.
5. **Claim readers** (subagent tokens only) each read the drafts of one group at a checkout of
   `HEAD`, and write a verdict on each draft's claim to a JSON file. **maintenance.py claims-collect**
   (zero model tokens) merges them into the table, marking a rejected draft with the reason.
6. **maintenance.py record** (zero model tokens) writes the verdicts to
   `docs/maintenance-reviewed`, after the user has decided which drafts to post.

The main session orchestrates. It never reads a source file, a verdict, or an issue body. Every
one of those is a file path passed between the steps.

---

## The job

### Step 1: prepare (zero model tokens)

```bash
python3 agents/skills/maintenance/maintenance.py [lane ...] [path ...]
```

Stdout is `{"work_dir": ..., "manifest": ...}`. The candidate list goes to stderr; print it.

Parse stdout for `work_dir`.

The `clippy` lane runs `cargo clippy` over the libraries and binaries, into
`target/maintenance-clippy` so it does not recompile what `make check` built. The first run
compiles every dependency and takes minutes; later ones take seconds. Without `cargo` the lane
says so in a note and has no candidates. `--no-clippy` leaves it out, `--fresh` includes the
candidates `docs/maintenance-reviewed` would carry, and `--limit N` sets every lane's cap, which
is otherwise 30, 12 for `size` and 15 for the lanes added after the first six.

A full run with every lane full asks about 23 reviewers. Once an earlier run is recorded, the
carried candidates are not asked about again, so scope a run to a lane or a path when that is too
many.

Where the repository has a commit, `prepare` checks it out at `{work_dir}/tree`, detached and with
hooks off, and the reviewers read that checkout. Other sessions move the working tree while a run is
under way, and a checkout makes every line a reviewer cites a line of one commit. It holds tracked
files only, so a search of it does not walk `target/` or `node_modules/`. The manifest records the
`commit` and the `tree`, and a progress line says when tracked files have uncommitted changes, which
the candidates come from and the reviewers do not read. Without a commit the reviewers read the
working tree.

### Step 2: read the manifest

Read `{work_dir}/manifest.json`. It holds:

- `progress_lines`: print these
- `lanes`: one entry per lane, each with `reviewer_prompts`, each of those with a `prompt_file`
  and a `results_file` path (paths, never prompt text)

If every lane has no reviewer prompts, there is nothing to review: say so and stop.

### Step 3: launch the reviewers

For every entry in every lane's `reviewer_prompts`, launch a subagent
(subagent_type: `general-purpose`) with exactly this prompt:

```
Read your review instructions from: {prompt_file}
Execute them completely. The instructions name the lane, the candidates in your scope, and the specs that govern them.
Write your results JSON to the results file the instructions specify. Do not edit any file.
```

**Launch every reviewer in a single message** so they run concurrently, and launch all of them:
a group with no reviewer becomes a `review-incomplete` gap, which is the correct outcome but a
wasted run.

**A reviewer's result is the file it writes.** Do not message, resume or wait on a reviewer, and do
not go by the agent list. A reviewer that started a command which then ran in the background stays
listed as running after it has finished, its notification can say the result may be interim, and a
message sent to a finished reviewer starts a new turn it has no context for. Each time a reviewer
reports, run:

```bash
python3 agents/skills/maintenance/maintenance.py status --work-dir "$WORK_DIR"
```

It says how many reviewers have written a result that parses and gives a verdict for every
candidate they were asked about, and names the rest. Go to step 4 when it exits 0. If the others
have finished and one has no result, say which, and let the user decide whether to launch it again;
the collector records the gap either way.

**Never write findings yourself.** You have not read the code; the reviewers have. Do not add,
merge, reword, or drop a finding, and do not decide a reviewer was wrong. If a reviewer's result
is missing or unreadable, the collector says so.

### Step 4: collect (zero model tokens)

```bash
python3 agents/skills/maintenance/maintenance.py collect --work-dir "$WORK_DIR"
```

Print its output. Exit code 1 means some candidate has no verdict, or was called `cleanup` with
no finding saying what the change is.

### Step 5: draft an issue per finding (zero model tokens)

```bash
python3 agents/skills/maintenance/maintenance.py draft --work-dir "$WORK_DIR"
```

Print its output. One body per finding is now on disk: what is wrong, the code at the first place
it names, every `file:line` the reviewer cited, the specs a change there has to keep true, and the
fix. Read none of them.

The code a body quotes is read from the reviewers' checkout, and its "Where it is" heading names
that commit. `main` can move between the reading and the posting, so compare the cited lines with
`HEAD`:

```bash
python3 agents/skills/maintenance/maintenance.py verify --work-dir "$WORK_DIR"
```

For each draft it lists the places it cites that moved, with where each is now, that are in code
that has changed since, or whose file is gone. Exit code 1 means some are in changed code or gone,
and a person has to look at them. It does not check a line number written without a path: step 6 does. Keep the
output for step 7.

The table names each draft's labels: `refactor`, or `bug` where the reviewer found a defect
rather than a cleanup, and an `area/*` or `infrastructure` where one is clear. It marks any title
over GitHub's 256-character cap as needing a shorter one, which is a person's job in step 7, and
any draft that overlaps another.

### Step 6: check each draft's claim (one reader per group of drafts)

`verify` checks that cited lines are where they were. Nothing so far checks that the claim is true:
that the code at `HEAD` still has the defect, that no closed issue or spec decided against the
change, that every count and caller the body states is right, and that the fix does not move, merge
or hide code that reads, labels or declassifies content. The reviewers wrote the drafts and the main
session reads none of them, so a wrong claim would otherwise reach the poster.

```bash
python3 agents/skills/maintenance/maintenance.py claims-prepare --work-dir "$WORK_DIR"
```

It checks `HEAD` out at `{work_dir}/head`, splits the drafts into groups of at most five in the order
of the file each cites first, so the drafts about one file go to one reader, and writes a prompt for
each. Stdout is the index: `readers`, each with a `prompt_file` and a `results_file`. The cost is one
reader per group, which is about one for every four to six drafts. A run of twenty drafts launches
four.

Launch one subagent (subagent_type: `general-purpose`) per reader, all in one message, with the
prompt of step 3 and that reader's `prompt_file`. A reader has the checkout and only its drafts, and
writes a verdict for each: `holds`, `rejected` or `unclear`, with a reason and the lines it quotes,
and the place each line number written without a path is at `HEAD`. A reader may run `gh issue list`
and `gh issue view` to read issues, and each call asks first. The same rules as step 3 apply: a
reader's result is the file it writes, and you write no verdict yourself.

```bash
python3 agents/skills/maintenance/maintenance.py claims-collect --work-dir "$WORK_DIR"
```

It prints the table of step 5 again, with `REJECTED by the claim check` and the reason under every
draft a reader rejected, `claim check could not decide` under an `unclear` one, and the corrected
line numbers under any draft that has them. Exit code 1 means some draft has no usable verdict: its
reader wrote no result, or no verdict for it, or rejected or doubted it with no reason. Those drafts
say `claim not checked`, and the user decides whether to launch the reader again. `draft` run again
keeps the marks.

A rejected draft stays in the table so the user sees why it is not recommended. Post it only if the
user, having read the reason, still names it.

### Step 7: the user decides what gets posted

The table from step 6 is what the user chooses from. Ask which drafts to post, post those and no
others, and never post one the backlog already has, open or closed.

Several lanes can find the same code, so two drafts can propose one change. The table marks a draft
`overlaps N` where draft N cites the same file within a few lines of it, or names the same function
in its evidence, and says which of the two cites more lines of the files both cite, which is the
better one to post. `--json` carries the same as `overlaps` on each draft. Before the user chooses,
say which drafts are marked, so that the user is not asked to choose between a draft and its copy.

Where a reader said a draft's claim does not hold or could not be decided, say so and give the reason
before the user chooses it.

Hand each chosen draft to the [issue-poster](../../agents/issue-poster.md) definition, which
searches the backlog for the symbol or path the title names, applies the labels and posts. Give
it the title, the body file, the labels the draft names, the assignee `netzenbot` (or the login
the user names instead), and this brief, with `{commit}` and `{moved}` filled in from the manifest
and from that draft's lines in the output of `verify`, or `none`, and `{written}` from the corrections
the reader gave for that draft in `claims-collect`, or `none`:

```text
Use the title as given. If you think it is inaccurate, post it as given and say so in your report.
Apply no importance, urgency or size: the issue is a finding a reviewer wrote and nobody has read,
and the triage-issues skill judges those.
File exactly this one issue and no other. Where the body names a defect it leaves unfixed, cite an
existing issue for it where one exists. File none for a defect that has no issue: list it in your
report, because the user decides.
The reviewers read commit {commit}. These places the body cites have moved since, or are in code
that changed: {moved}. Correct only those line numbers in the posted body, to the places named for
the moved ones and by reading the code for the others, and list each correction in your report.
Change nothing else in the body.
These line numbers are written without a path, and a reader found where they are at the current
commit: {written}. Replace each in the posted body and list each replacement in your report.
Search open and closed issues for the symbol or path the title names before filing. If an issue
already covers it, cite it and do not file.
```

The poster's own procedure files an issue for each defect a body leaves unfixed unless the task says
to file none, and an issue it files was never a draft, so nobody has chosen it. The brief says to
file none, and keeps the poster from retitling a draft or changing a line number it was not told
about.

Where the table said a title needs shortening, rewrite it before posting rather than posting a
title GitHub will refuse.

Use the existing `refactor` label for cleanup. If a required label is missing, the poster refuses
and prints a creation command for a person; pass it on rather than creating a label or posting
under a different one. Do not create or use a `maintenance` label.

`gh` is deliberately not in this skill's `allowed-tools`, so every call the poster makes asks
first. Do not work around it.

### Step 8: record what was read

```bash
python3 agents/skills/maintenance/maintenance.py record --work-dir "$WORK_DIR"
```

This writes every `keep` and `cleanup` verdict to `docs/maintenance-reviewed`, so the next run
leaves those candidates out until a file they name changes. It does not write `unclear`, nor a
candidate no reviewer answered for. A `cleanup` is recorded whether or not its draft was posted,
since a declined draft that came back every run would be asked about again each time. Delete a
line to have a candidate read again. The file is left uncommitted.

Then remove the checkouts the reviewers and the claim readers read:

```bash
python3 agents/skills/maintenance/maintenance.py clean --work-dir "$WORK_DIR"
```

### Step 9: say what happened

In two or three lines: how many candidates were reviewed, how many were carried from earlier
runs, how many became findings, which one to look at first, and how many drafts are waiting.
Mention the lanes that ran with a note, such as clippy without `cargo`. Nothing else.

---

## The lanes

| Lane | A candidate is | Why no other check finds it |
|---|---|---|
| `unreferenced` | a `pub` Rust item, a module-level Python definition, or a TypeScript export whose name appears nowhere outside comments but its definition | rustc does not warn about a `pub` item a library crate exports, and nothing warns about an unused Python or TypeScript name |
| `test-only` | the same, named only from tests | a test counts as a use, so the item looks alive |
| `allowances` | `allow(dead_code)`, `allow(unused...)`, or an allowance of `too_many_arguments`, `too_many_lines`, `cognitive_complexity`, `type_complexity`, `large_enum_variant` or `needless_pass_by_value`, or `expect` of one, outside tests and not under a platform-only `cfg_attr` | the attribute is what silences the warning |
| `duplicates` | at least 8 matching non-blank lines, comments and whitespace aside, in more than one place outside tests | no check compares files |
| `literals` | a string of 10 or more characters, not a bare word, 3 or more times across 2 or more files outside tests | as above |
| `size` | a file over 2500 non-blank lines outside its tests, or a function over 150 | as above |
| `complexity` | a function with statements 6 blocks deep, 3 boolean parameters, or (outside Rust) 8 parameters | clippy refuses 8 Rust parameters but not nesting or booleans, and nothing checks Python or TypeScript |
| `coupling` | a Rust file naming 12 other modules of its crate or other bravebot crates outside tests | nothing counts what a file depends on |
| `untested` | a Rust file with 5 public functions that code outside tests calls and no test names | coverage tools report lines, not which functions a file offers that nothing checks |
| `shared-state` | a Rust file with 8 uses of `Mutex`, `RwLock`, `RefCell`, a once-initialised static, an atomic, `static mut`, a spawned thread or a channel, outside tests | clippy has no lint for how much shared state a file holds |
| `stale` | a file with 2 consecutive lines of commented-out code, or a `TODO`, `FIXME`, `HACK` or `XXX` that names no issue | nothing reads comments |
| `error-types` | a Rust file with 3 functions returning a `Result` whose error is a `String`, a `&str` or a boxed `dyn Error` | a caller cannot match on one, and no lint objects |
| `clippy` | a lint from `CLIPPY_LINTS` in `maintenance.py`, in one file, in code that ships, ranked by how many sites and how the lint is weighted | these lints are off in `make check`, which keeps only what the tree has none of |

Test code is files under a test directory or with a test name, items under `#[cfg(test)]`, and
every file a `#[cfg(test)] mod name;` reaches. Each lane keeps its highest-ranked candidates:
30 per lane, 12 for `size`, 15 for the lanes after it, and the report says how many more there
were and how many `docs/maintenance-reviewed` carried. Change the caps with `--limit` and
`--size-limit`.

A candidate is keyed by its lane, its file and its item, never a line number, and by a hash of
the text of the files it names. `docs/maintenance-reviewed` holds those, and a candidate whose
files are unchanged is carried. A repeated block or literal is keyed by what it says, so it is
carried until any file holding a copy changes.

A candidate is a count, and the scan reads no code. It calls a name built from parts unused and
two unrelated items with one name uses of each other. The reviewer settles it by reading.

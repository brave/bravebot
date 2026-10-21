---
name: check-spec
description:
  'Check that the implementation matches docs/specs, clause by clause. Runs the
  mechanical pass (clause numbering, renumbering against main, verified-by
  resolution, governs, guards, the README table) and then a conformance review of
  the governed code, then drafts one
  issue per finding for a person to confirm before any of them is posted. A repeat
  run reviews only what changed since each clause was last reviewed, using
  docs/check-spec-reviewed. Triggers
  on: check spec, /check-spec, make check-spec, does the code match the spec, spec
  conformance, spec drift, file issues for spec gaps.'
argument-hint: '[spec-name|spec-id ...] [changed] [strict] [full]'
allowed-tools: Bash(python3 agents/skills/check-spec/*), Bash(make check-spec*)
---

# Check the implementation against the spec

[docs/specs/](../../../docs/specs/README.md) is the source of truth for behaviour. This
skill answers one question about it: **does the code do what the clauses say?**

- **Full run** (default): every spec.
- **Scoped run** (`/check-spec labels routing`, `/check-spec LABEL`, or
  `/check-spec tools/lsp`): named specs only. A spec answers to its id, its file name, or
  its path, and `.md` is optional on either of the last two.
- **Branch run** (`/check-spec changed`): only the specs governing files this branch
  touched. This is the one to run before a commit.
- `strict` makes warnings fail as well as errors.
- `full` reviews every clause in full and ignores what earlier runs recorded. Without it, a
  clause is reviewed again only as far as the code it depends on changed
  ([what a repeat run reviews](#what-a-repeat-run-reviews)).

`make check-spec` runs the mechanical pass alone, with no model in the loop. It is fast,
deterministic, and safe in CI. The conformance review is what this skill adds.

---

## The one direction this check runs in

**When the code and the spec disagree, the code is wrong.** That is the whole point of the
check, and it is the rule every reviewer prompt repeats.

This skill NEVER edits a spec, adds a clause, softens a clause, or reports "the spec is
unrealistic" as a finding. Specs are closely reviewed by humans and change through a
separate, deliberate process. A run that ends by rewording a clause has verified nothing.

This skill also never fixes the code. It reports, and it drafts one issue per finding so
that a run outlives the session it happened in. Fixing is a separate task the user asks for
after reading the findings, and mixing the two means the report is written by the same pass
that decided what to change.

**Nothing is posted until the user names which drafts to post.** Thirty findings are thirty
claims nobody has read yet, and a run that files them itself has taken a judgement that is
not its to take. Drafting is free and reversible; an issue is neither.

---

## Architecture: a file-based pipeline

The heavy data goes through files, never through the main session's context:

1. **check-spec.py** (zero model tokens) runs every mechanical check and writes one prompt
   file per group of clauses. Its stdout is a small JSON pointer.
2. **Reviewers** (subagent tokens only) each read one prompt file, read the governed source
   themselves, and write verdicts to a JSON file.
3. **collect-findings.py** (zero model tokens) merges the mechanical findings with the
   reviewers' verdicts, renders the report, and sets the exit code.
4. **draft-issues.py** (zero model tokens) writes one issue body per finding into the same
   work directory, and names the drafts that are still waiting for a screen.
5. **record-reviewed.py** (zero model tokens) writes what the reviewers decided to
   `docs/check-spec-reviewed`, so the next run starts from it.

The main session orchestrates. It never reads a spec, a source file, a verdict, or an issue
body. Every one of those is a file path passed between the steps.

---

## The job

### Step 1: prepare (zero model tokens)

```bash
python3 agents/skills/check-spec/check-spec.py [spec ...] [--changed] [--strict] [--full]
```

Stdout is `{"work_dir": ..., "manifest": ...}`. The mechanical report goes to stderr; print
it, since those findings are already final and the user should see them first.

Parse stdout for `work_dir`.

### Step 2: read the manifest

Read `{work_dir}/manifest.json`. It holds:

- `progress_lines`: print these. They say how many clauses were kept from the record, how many
  reviewers read a diff, and why the clauses reviewed in full were
- `mechanical_findings`: already decided, already in the report
- `specs`: one entry per spec, each with `reviewer_prompts`, each of those with a
  `prompt_file` and a `results_file` path (paths, never prompt text), and with `kept`, the
  clauses the record answers without a reviewer

If every `reviewer_prompts` list is empty, skip to step 4: the record answered every clause.

### Step 3: launch the reviewers

For every entry in every spec's `reviewer_prompts`, launch a subagent
(subagent_type: `general-purpose`) with exactly this prompt:

```
Read your review instructions from: {prompt_file}
Execute them completely. The instructions name the spec, the clauses in your scope, the governed source, and the guarded symbols.
Write your verdicts JSON to the results file the instructions specify. Do not edit any file.
```

**Launch every reviewer in a single message** so they run concurrently, and launch all of
them: a chunk with no reviewer becomes a `review-incomplete` error, which is the correct
outcome but a wasted run.

Wait for all of them.

A prompt that names a `since-<commit>.diff` file asks the reviewer to say whether an earlier
verdict still holds on the change since that commit, and its answers carry `holds`. The
launch is the same.

**Never write findings yourself.** You have not read the code; the reviewers have. Do not
add, merge, reword, or drop a verdict, and do not decide a reviewer was wrong. If a
reviewer's result is missing or unreadable, the collector says so.

### Step 4: collect (zero model tokens)

```bash
python3 agents/skills/check-spec/collect-findings.py --work-dir "$WORK_DIR" [--strict]
```

Print its output. Exit code 1 means something at severity `error` survived. A finding marked
`recorded at <commit> and still holds` came from the record, not from a reviewer this run.

### Step 5: draft an issue per finding (zero model tokens)

```bash
python3 agents/skills/check-spec/draft-issues.py --work-dir "$WORK_DIR" [--errors]
```

Print its output. One body per finding is now on disk, shaped like a pull request: what is
wrong first, shown rather than described, then the clause it breaks quoted in full, then the
failure walk, the `file:line` evidence, and the fix the reviewer proposed. Read none of them.

A title runs as long as the reviewer's summary does. Nothing cuts one: a summary states the
defect first and its cost second, so the end is where the cost is. The table marks any draft
whose title is over GitHub's 256-character cap as needing a shorter one, which is a person's
job in step 7.

A finding `make check-spec` already fails on gets no draft, because it is red on the branch
that caused it and will be fixed there. So the drafts are the review findings and the clauses
nothing pins, which is what a green CI run leaves unsaid.

A finding carried from the record gets no draft either: the run that decided it drafted it. A
`full` run reviews the clause again and drafts it again, and step 7 never posts a clause that
already has an issue.

### Step 6: capture the screens that were asked for

The table names any draft whose reviewer said the wrong behaviour is something a person can
look at. A bug report that shows the screen is one somebody can act on without reproducing it
first, so fill those in before proposing anything.

This runs the real interface, which needs a build and a backend and writes real sessions. Where
there is no backend to run one against, skip this step and say so: a draft with no screen is
worth more than a draft with an invented one.

Launch one subagent (subagent_type: `general-purpose`) per draft, all in a single message, with
`{...}` filled in from the draft's line in the table and `{repo}` the absolute path of this
checkout:

```
A spec check found a bug that shows up on a screen. Capture what the interface draws, and change nothing.

How to reach it: {screen_wanted}

1. `cargo build` in {repo}.
2. Write a drive_tui script to {session_file}: one step per line, `timeout keys`. `contrib/README.md` says how, and the steps have to answer the trust prompt first.
3. From a disposable directory: `{repo}/contrib/drive_tui.py {session_file} --raw {capture_file} -- {repo}/target/debug/bravebot`
4. `python3 {repo}/contrib/terminal-screenshot.py {capture_file} --strict > {screen_file}`
5. Read {screen_file}. If it does not show the behaviour described above, fix the script and go round again. If you cannot reach it in three tries, delete {screen_file} and report that you could not.

Write only those three files, all of them in the work directory. Do not edit the tree, do not fix the bug, and never write a screen you did not capture.
```

Then run step 5 again. The bodies pick up any screen and session file that now exists, so a
draft that got one shows it and a draft that did not is unchanged.

### Step 7: the user decides what gets posted

The table from step 5 is what the user chooses from. Ask which drafts to post, post those and no
others, and never post one whose clause already has an issue, open or closed.

Hand each chosen draft to the [issue-poster](../../agents/issue-poster.md) definition, which
searches the backlog for the clause id, applies the labels and posts. Give it the title, the body
file, the labels the draft names, and this instruction: the issue is a finding nobody has read, so
apply no `importance`, `urgency` or `size`. Those are the [triage-issues skill](../triage-issues/SKILL.md)'s
to judge, and guessing at them here would put a finding nobody has read into somebody's queue.

Where the table said a title needs shortening, rewrite it before posting rather than posting a
title GitHub will refuse. Keep the defect and the cost, and say the clause id first.

`gh` is deliberately not in this skill's `allowed-tools`, so every call the poster makes asks
first. That is the gate, not a nuisance to work around.

### Step 8: record the verdicts (zero model tokens)

A clause the record keeps is reported but never drafted again, so record only after step 7, once
the user has decided which drafts to post: a draft left unposted is not offered again until the
code its clause depends on changes, or a `full` run. Skip this step for a run on a branch, a run of
`changed`, and a run on a tree that was not main, which the script refuses: a verdict recorded for
another tree would misdate the next run's diff. Otherwise:

```bash
python3 agents/skills/check-spec/record-reviewed.py --work-dir "$WORK_DIR" [--dry-run]
```

Print its output. It writes a line to `docs/check-spec-reviewed` for every clause a reviewer
decided, and takes the line off a clause a reviewer could not decide or that no spec has any
more. It does not commit the file. Tell the user it is written and that committing it on a branch,
as any other change, is what lets the next run on another machine start from it.

### Step 9: say what happened

In two or three lines: how many clauses were checked, what failed, which finding to look at
first, and how many drafts are waiting. Nothing else. The report is the deliverable.

---

## What a repeat run reviews

A full run launches one reviewer per six clauses, and each reads the governed source again.
`docs/check-spec-reviewed` keeps what a run decided so that a repeat run does less. It holds one
line per clause: the verdict, its severity, the main commit it was reached at, a hash and the
reviewer's one-line summary. The hash covers what a reviewer is given besides the code a diff
shows: the clause's words and the tests it names, the paths its spec governs, the symbols it guards
and the uses of each in files nobody watches, the text of the spec outside its clauses, where a
spec lists the exceptions it admits, the exceptions the labels spec lists to the rule every
reviewer is told outranks the clauses, and the reviewer's own instructions. A change to any of
them is a change to what the verdict was read from. The wording of another clause is not in it.

A run decides each clause by the record:

| The record | The clause is |
|---|---|
| has no line for it, or its hash differs | reviewed in full, as with no record |
| none of the code the spec watches has changed since its commit | kept: no reviewer starts, and the verdict is in the report |
| under half of the lines the spec watches have changed | reviewed on the diff: the reviewer is given the earlier verdict and the diff, and says whether the verdict holds |
| half or more have changed | reviewed in full |

- **What a spec watches** is the paths it governs, the files its pinned guards allow a use in (the
  set `changed` selects specs by), and the files holding the tests its clauses name. Lines changed are lines added plus lines deleted, a file
  counted at most once over its length. `--threshold PERCENT` moves the half. Past it the diff is
  as long as reading half of the code, which a reviewer needs beside the diff to understand it.
- **A clause is reviewed in full** whatever the diff where git cannot say what changed: the
  recorded commit is not in this tree's history, or a file git does not track sits in the watched
  paths. A branch cut before the recorded commit reviews in full for that reason.
- **A reviewer of a diff** answers each clause `holds: true` when the earlier verdict is still
  true of the code, and otherwise gives a verdict with everything a full review gives. A reviewer
  that says `holds` and names another verdict has named it: the collector keeps the verdict it
  was given. A clause the reviewer left out is reported as unreviewed and keeps the line it had.
- **`unclear` is never recorded.** It says the review decided nothing, so the clause has no line
  and is reviewed from the start next time. `conforms`, `violation` and `untested` are reused.
- **The record is written for main only**, since a later run takes its diff from the commit named.
  A tree that is not main, that moved since the run was prepared, or that was not a commit when it
  was prepared (an edited governed file, or a file git does not track in what a spec watches) is
  refused, whatever the tree is when the script runs.
- **A diff review reads only what changed.** A verdict held through several diff reviews is as
  sound as the full review that began it, and a fix made in a file the spec does not watch (a
  finding's evidence is often in one) is not seen until a full run. Run `full` on a schedule. Editing the reviewer
  instructions in `check-spec.py` reviews every clause in full on the next run, since they are in
  the hash.
- **A record that cannot be read** is a mechanical error, and until it is fixed every clause is
  reviewed in full. `make check-spec` reads the file, and `full` ignores it.

---

## What each pass decides

| Check | Pass | Fails on |
|---|---|---|
| Clause ids never duplicated, with no gap, in any order down the file | mechanical | error |
| A clause id still names the clause it named on main, unless `agents/renumbered-clauses.txt` says otherwise | mechanical | error |
| Every line of `agents/renumbered-clauses.txt` names a clause this tree has | mechanical | error |
| Every clause carries an anchor so it can be linked to | mechanical | error |
| A withdrawn clause says what replaced it | mechanical | error |
| Every clause carries a `verified-by` line | mechanical | error |
| `verified-by` names a `#[test]` that exists, in the module it says | mechanical | error |
| `by-construction` says what makes the clause hold | mechanical | error |
| `verified-by: none` | mechanical | warning |
| `agents/unverified-clauses.txt` lists exactly the clauses a full run finds uncovered | mechanical | error |
| Every line of `docs/check-spec-reviewed` is a well formed record | mechanical | error |
| `governs` paths exist | mechanical | error |
| `guards` symbols exist, a qualified one under the type or module named | mechanical | error |
| A `guards` entry that pins its sites is used in exactly those files, that many times | mechanical | error |
| Front matter carries no key nothing reads, such as a `sites:` indented out of its entry | mechanical | error |
| No spec cites another spec's clause ids | mechanical | error |
| The README table lists every spec, with the right id and count | mechanical | error |
| No em-dash | mechanical | error |
| The code does what the clause says | review | error |
| The named tests actually pin the clause | review | warning |
| Nothing untrusted reaches the driver or the planner | review | error |

---

## Scope

Only `docs/specs` and the paths those specs list under `governs`. Code under no spec's
`governs` is ordinary code, reviewed as ordinary code, and out of scope here. Adding a spec
is how a topic becomes review-required, and that is a human's decision.

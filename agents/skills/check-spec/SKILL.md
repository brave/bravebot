---
name: check-spec
description:
  'Check that the implementation matches docs/specs, clause by clause. Runs the
  mechanical pass (clause numbering, verified-by resolution, governs, guards, the
  README table) and then a conformance review of the governed code, then drafts one
  issue per finding for a person to confirm before any of them is posted. Triggers
  on: check spec, /check-spec, make check-spec, does the code match the spec, spec
  conformance, spec drift, file issues for spec gaps.'
argument-hint: '[spec-name|spec-id ...] [changed] [strict]'
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

The main session orchestrates. It never reads a spec, a source file, a verdict, or an issue
body. Every one of those is a file path passed between the steps.

---

## The job

### Step 1: prepare (zero model tokens)

```bash
python3 agents/skills/check-spec/check-spec.py [spec ...] [--changed] [--strict]
```

Stdout is `{"work_dir": ..., "manifest": ...}`. The mechanical report goes to stderr; print
it, since those findings are already final and the user should see them first.

Parse stdout for `work_dir`.

### Step 2: read the manifest

Read `{work_dir}/manifest.json`. It holds:

- `progress_lines`: print these
- `mechanical_findings`: already decided, already in the report
- `specs`: one entry per spec, each with `reviewer_prompts`, each of those with a
  `prompt_file` and a `results_file` path (paths, never prompt text)

If `specs` is empty, skip to step 4.

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

**Never write findings yourself.** You have not read the code; the reviewers have. Do not
add, merge, reword, or drop a verdict, and do not decide a reviewer was wrong. If a
reviewer's result is missing or unreadable, the collector says so.

### Step 4: collect (zero model tokens)

```bash
python3 agents/skills/check-spec/collect-findings.py --work-dir "$WORK_DIR" [--strict]
```

Print its output. Exit code 1 means something at severity `error` survived.

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
others, and never post one whose clause already has an open issue:

```bash
gh issue list --repo OWNER/REPO --state open --search "CLAUSE-N in:title"
gh issue create --repo OWNER/REPO --title "TITLE" --label FIRST --label SECOND --body-file BODY_FILE
```

Where the table said a title needs shortening, rewrite it before posting rather than posting a
title GitHub will refuse. Keep the defect and the cost, and say the clause id first;
[labelling-issues.md](../../../docs/development/labelling-issues.md) is what a title carries.

Apply every label the draft names and none it does not. A divergence carries two, because which
clause it breaks and what it is in the code that ships today are different questions: `spec-mismatch`
with `bug` where the code attempts the behaviour and gets it wrong, and `spec-mismatch` with
`enhancement` where nothing attempts it, since a clause nobody built breaks nothing. A clause
nothing pins carries `spec-coverage` alone: the behaviour is right, so it is neither.

`importance`, `urgency` and `size` are the [triage-issues skill](../triage-issues/SKILL.md)'s to
judge, and guessing at them here would put a finding nobody has read into somebody's queue.

`gh` is deliberately not in this skill's `allowed-tools`, so every one of those calls asks
first. That is the gate, not a nuisance to work around.

### Step 8: say what happened

In two or three lines: how many clauses were checked, what failed, which finding to look at
first, and how many drafts are waiting. Nothing else. The report is the deliverable.

---

## What each pass decides

| Check | Pass | Fails on |
|---|---|---|
| Clause ids in order, never renumbered, never duplicated | mechanical | error |
| Every clause carries an anchor so it can be linked to | mechanical | error |
| A withdrawn clause says what replaced it | mechanical | error |
| Every clause carries a `verified-by` line | mechanical | error |
| `verified-by` names a `#[test]` that exists, in the module it says | mechanical | error |
| `by-construction` says what makes the clause hold | mechanical | error |
| `verified-by: none` | mechanical | warning |
| `agents/unverified-clauses.txt` lists exactly the clauses a full run finds uncovered | mechanical | error |
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

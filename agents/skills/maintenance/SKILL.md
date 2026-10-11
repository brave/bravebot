---
name: maintenance
description:
  'Find code to clean up: public items nothing names, items only tests name,
  attributes that silence the dead-code lint, blocks repeated in several places,
  string literals repeated across files, and files or functions past a length.
  Reviewers read each candidate and decide which are worth an issue, then one
  issue is drafted per finding for a person to confirm before any of them is
  posted, assigned to netzenbot. Fixes nothing. Triggers on: maintenance,
  /maintenance, make check-maintenance, dead code, unused code, duplicate code,
  code that should be shared, files that are too big, refactoring candidates.'
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

`make check-maintenance` runs the scan alone, with no model in the loop, prints the candidates
and exits zero. The review and the drafts are what this skill adds.

---

## What this skill does not do

**It never edits the tree.** Every fix is an issue assigned to netzenbot, which a later session
works on like any other issue. Fixing in the same run would let the pass that found a problem
also decide the change, with nobody reading either.

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
   candidates in each lane, and writes one prompt file per group of them. Its stdout is a small
   JSON pointer.
2. **Reviewers** (subagent tokens only) each read one prompt file, read the code themselves, and
   write verdicts and findings to a JSON file.
3. **maintenance.py collect** (zero model tokens) merges the reviewers' results, renders the
   report, and names every candidate no reviewer answered for.
4. **maintenance.py draft** (zero model tokens) writes one issue body per finding into the same
   work directory.

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

Wait for all of them.

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

The table names each draft's labels: `maintenance`, or `bug` where the reviewer found a defect
rather than a cleanup, and an `area/*` or `infrastructure` where one is clear. It marks any title
over GitHub's 256-character cap as needing a shorter one, which is a person's job in step 6.

### Step 6: the user decides what gets posted

The table from step 5 is what the user chooses from. Ask which drafts to post, post those and no
others, and never post one the backlog already has, open or closed.

Hand each chosen draft to the [issue-poster](../../agents/issue-poster.md) definition, which
searches the backlog for the symbol or path the title names, applies the labels and posts. Give
it the title, the body file, the labels the draft names, the assignee `netzenbot` (or the login
the user names instead), and this instruction: the issue is a finding nobody has read, so apply no
`importance`, `urgency` or `size`. Those are the [triage-issues skill](../triage-issues/SKILL.md)'s
to judge.

Where the table said a title needs shortening, rewrite it before posting rather than posting a
title GitHub will refuse.

`maintenance` is a label a person creates, since netzenbot cannot.
[labelling-issues.md](../../../docs/development/labelling-issues.md) has the command. Until it
exists the poster refuses a draft carrying it and prints that command; pass it on rather than
posting under a different label.

`gh` is deliberately not in this skill's `allowed-tools`, so every call the poster makes asks
first. Do not work around it.

### Step 7: say what happened

In two or three lines: how many candidates were reviewed, how many became findings, which one to
look at first, and how many drafts are waiting. Nothing else.

---

## The lanes

| Lane | A candidate is | Why no other check finds it |
|---|---|---|
| `unreferenced` | a `pub` Rust item, a module-level Python definition, or a TypeScript export whose name appears nowhere outside comments but its definition | rustc does not warn about a `pub` item a library crate exports, and nothing warns about an unused Python or TypeScript name |
| `test-only` | the same, named only from tests | a test counts as a use, so the item looks alive |
| `allowances` | `allow(dead_code)`, `allow(unused...)` or `expect` of either, outside tests and not under a platform-only `cfg_attr` | the attribute is what silences the warning |
| `duplicates` | at least 8 matching non-blank lines, comments and whitespace aside, in more than one place outside tests | no check compares files |
| `literals` | a string of 10 or more characters, not a bare word, 3 or more times across 2 or more files outside tests | as above |
| `size` | a file over 2500 non-blank lines outside its tests, or a function over 150 | as above |

Test code is files under a test directory or with a test name, items under `#[cfg(test)]`, and
every file a `#[cfg(test)] mod name;` reaches. Each lane keeps its highest-ranked candidates:
30 per lane, 12 for `size`, and the report says how many more there were. Change the caps with
`--limit` and `--size-limit`.

A candidate is a count, and the scan reads no code. It calls a name built from parts unused and
two unrelated items with one name uses of each other. The reviewer settles it by reading.

---
name: security-audit
description:
  'Audit this repository against the rule that untrusted content never enters the
  driver or the planner. Enumerates the label surface mechanically, then reads the
  code in eight lanes, tries to disprove every candidate, and files one issue per
  finding that survives, skipping anything the tracker already holds.
  Triggers on: security audit, /security-audit, make check-security, find security
  issues, audit the trust boundary, is the rule still holding, prompt injection
  review, file security issues.'
argument-hint: '[lane ...] [changed] [mechanical-only]'
allowed-tools: Bash(python3 agents/skills/security-audit/*), Bash(make check-security*), Bash(make check-spec*), Bash(make check-deps*)
---

# Audit the guarantee

The rule is in [CLAUDE.md](../../../.claude/CLAUDE.md): **the driver and the planner never have
untrusted content in their context.** The driver is `bravebot-core` and `bravebot-agent` both.
The planner is the model. This skill answers one question about that: **does the guarantee
hold?**

- **Full run** (default): all eight lanes.
- **Scoped run** (`/security-audit laundering gates`): named lanes only.
- **Branch run** (`/security-audit changed`): the lanes a branch's diff touches. This is the
  one to run before asking for review on a diff that touches a label.
- **`mechanical-only`**: the deterministic half alone, no model in the loop. This is what
  `make check-security` runs and what belongs in CI.

---

## Why this is not a second check-spec

`make check-spec` asks whether the clauses and their pinned counts line up.
The [check-spec skill](../check-spec/SKILL.md) asks whether the code does what a clause says,
and when the two disagree the code is wrong. Neither can reach three things:

- **The counts pin releases, not decisions.** `docs/specs/labels.md` pins
  `Labelled::declassify` to a count per file, so a new release cannot land silently. It pins
  nothing about how many decisions are then taken from the released bytes, so
  `crates/core/src/policy.rs` has to be read rather than counted.
- **`check-spec` runs in one direction only.** It never faults a clause. So no existing pass
  can return the finding that a clause is satisfied and the guarantee still breaks.
- **`check-spec`'s scope is `docs/specs` plus the paths those specs govern.** Code no spec
  governs is out of its scope and inside the binary.

So this skill can conclude something the others cannot: **the code is wrong, or the clause is,
or nothing reaches it at all.** It never re-implements a mechanical check.
[docs/best_practices.md](../../../docs/best_practices.md) is clear that a rule a tool enforces
or could enforce is a check rather than a review habit, so where a finding could be a check,
the issue asks for the check.

---

## Two things a run must not do

**A run never posts by hand.** It posts, and `post-issues.py` is the only thing in here that
does. Everything keeping that safe is in the script rather than in this file, because a model
cannot be relied on to pace itself, to compare a title against forty existing ones, or to stop
at a cap it read about several thousand tokens ago. So `gh` is deliberately absent from this
skill's `allowed-tools`: a `gh issue create` typed out here asks first, and the script's calls
do not. That is on purpose, and reaching for `gh` to work around the script is the one move that
loses the dedup, the pacing and the cap at once.

**A run never fixes anything.** It reports, it drafts, it files. Fixing is a separate task the
user asks for afterwards, because a pass that decides what to change should not also be the pass
that reports what is wrong.

---

## Architecture: a file-based pipeline

The heavy data goes through files, never through this session's context:

1. **security-audit.py** (zero model tokens) enumerates the audit surface, runs the
   deterministic checks, and writes one prompt file per lane. Its stdout is a small JSON
   pointer.
2. **Lanes** (subagent tokens only) each read one prompt file, read the code themselves, and
   write candidates to a JSON file.
3. **verify-findings.py** (zero model tokens) pairs each candidate with a verifier prompt.
4. **Verifiers** (subagent tokens only) each try to kill one candidate.
5. **collect-findings.py** (zero model tokens) merges, renders the report, sets the exit code.
6. **draft-issues.py** (zero model tokens) writes one issue body per confirmed finding.
7. **post-issues.py** (zero model tokens) files the ones the tracker does not already hold, one
   every ten seconds or so.

This session orchestrates. It reads no source file, no candidate, no verdict and no issue
body. Every one of those is a path passed between steps. `{work_dir}/surface.json` holds every
enumerated site and is for the lanes, not for you: it is a hundred and eighty kilobytes and
reading it would defeat the whole arrangement.

---

## The job

### Step 1: enumerate (zero model tokens)

```bash
python3 agents/skills/security-audit/security-audit.py [lane ...] [--changed BASE] [--mechanical-only]
```

Stdout is `{"work_dir": ..., "manifest": ...}`. The mechanical report goes to stderr; print
it, since those findings are already decided and the user should see them first.

Parse stdout for `work_dir`. If the run was `mechanical-only`, print the report and stop.

### Step 2: read the manifest

Read `{work_dir}/manifest.json`. It holds `progress_lines` (print these),
`mechanical_findings` (already in the report), and `lanes`, one entry each with a
`prompt_file` and a `results_file`. If `lanes` is empty, skip to step 6.

### Step 3: launch the lanes

For every entry in `lanes`, launch a subagent (subagent_type: `general-purpose`) with exactly
this prompt:

```
Read your audit instructions from: {prompt_file}
Execute them completely. The instructions name the rule, your lane's question, the exact sites to start from, and the output contract.
Write your candidates JSON to the results file the instructions specify. Do not edit any file in the tree.
```

**Launch every lane in a single message** so they run concurrently, and launch all of them: a
lane with no subagent becomes a `lane-incomplete` error, which is the correct outcome and a
wasted run.

Wait for all of them.

**Never write a candidate yourself.** You have not read the code; the lanes have. Do not add,
merge, reword or drop one, and do not decide a lane was wrong. A missing or unreadable result
is a finding the collector reports.

### Step 4: pair each candidate with a verifier (zero model tokens)

```bash
python3 agents/skills/security-audit/verify-findings.py --work-dir "$WORK_DIR"
```

Print its output: one line per candidate with its lane, its claim and its prompt file.

### Step 5: launch the verifiers

This step is the quality argument for the whole skill, so it is not optional and it is not
merged into step 3. A lane that found something is a lane that wants to have found something.
A candidate nobody argued against is a candidate nobody checked.

One subagent (subagent_type: `general-purpose`) per candidate, **all in a single message**:

```
Read your verification instructions from: {prompt_file}
Execute them completely. Your job is to kill the claim if it can be killed. Open the files it names and check them yourself.
Write your verdict JSON to the results file the instructions specify. Do not edit any file in the tree.
```

The prompts arm each verifier with this repository's own rebuttal, from the section of
[reviewing-for-the-rule.md](../../../docs/development/reviewing-for-the-rule.md) on inventing a
violation: the rule is about content that could reach the planner or steer a turn in progress,
and it is not a general prohibition on reading bytes that arrived over a network.
`GET /v1/models` carries `Label::untrusted_public()` and is branched on legitimately. A
candidate whose bytes reach neither context is **dropped, not filed**, and that document warns
that a wrong trust argument reads exactly like a safety feature.

Wait for all of them.

### Step 6: collect (zero model tokens)

```bash
python3 agents/skills/security-audit/collect-findings.py --work-dir "$WORK_DIR"
```

Print its output. Exit code 1 means something at severity `error` survived. `high` and
`medium` are errors; `low` is a warning.

### Step 7: draft an issue per finding (zero model tokens)

```bash
python3 agents/skills/security-audit/draft-issues.py --work-dir "$WORK_DIR"
```

Print its output. One body per confirmed finding is now on disk: the user impact, what
happens, the clause quoted, the code it happens in, where the bytes come from and what the
code does with them, how to reproduce it, what it buys an attacker, the argument for its own
kind label, and the fix. Read none of them.

### Step 8: capture the screens that were asked for

The table names any draft whose verifier said the failure is something a person can look at. A
report that shows the screen is one somebody can act on without reproducing it first, so fill
those in before proposing anything.

This runs the real interface, which needs a build and a backend. Where there is no backend to
run one against, skip this step and say so: a draft with no screen is worth more than a draft
with an invented one.

Launch one subagent (subagent_type: `general-purpose`) per draft that wants a screen, all in a
single message, with `{...}` filled in from the draft's line in the table and `{repo}` the
absolute path of this checkout:

```
A security audit found a bug that shows up on a screen. Capture what the interface draws, and change nothing.

How to reach it: {screen_wanted}

1. `cargo build` in {repo}.
2. Write a drive_tui script to {session_file}: one step per line, `timeout keys`. `contrib/README.md` says how, and the steps have to answer the trust prompt first.
3. From a disposable directory: `{repo}/contrib/drive_tui.py {session_file} --raw {capture_file} -- {repo}/target/debug/bravebot`
4. `python3 {repo}/contrib/terminal-screenshot.py {capture_file} --strict > {screen_file}`
5. Read {screen_file}. If it does not show the behaviour described above, fix the script and go round again. If you cannot reach it in three tries, delete {screen_file} and report that you could not.

Write only those three files, all of them in the work directory. Do not edit the tree, do not fix the bug, and never write a screen you did not capture.
```

Then run step 7 again. The bodies pick up any screen and session file that now exists, so a
draft that got one shows it and a draft that did not is unchanged.

### Step 9: file them

```bash
python3 agents/skills/security-audit/post-issues.py --work-dir "$WORK_DIR" [--dry-run]
```

Print its output. One line per draft saying `filed` with the URL, or `skip` with the number of
the issue that says it already. Three things are the script's and not yours to redo:

- **A finding the tracker already holds is skipped**, open or closed. Somebody who read a
  finding and closed it has answered it, and filing it again next week is arguing with them by
  machine.
- **One issue every ten seconds**, plus one to five more so the gap is not a constant. Six
  issues in the same second read as a script having got loose.
- **A cap.** A lane that malfunctions malfunctions at scale, and a shared tracker is the wrong
  place to find that out. What the cap left is printed by title.

The labels are the ones on the draft. Every issue carries `security`, `needs-security-review`,
its kind label, a `severity/*` and an `area/*` where the finding has one. `importance`,
`urgency` and `size` are the [triage-issues skill](../triage-issues/SKILL.md)'s to judge, and
guessing at one here would put a finding nobody has read into somebody's queue.

A label the repository does not have stops the whole step before anything is posted, and the
output names the `gh label create` for each one. Creating a label changes what everybody sees,
so it is the user's to run, not this skill's.

### Step 10: say what happened

In two or three lines: how many lanes ran, how many candidates were dropped by the verifiers,
what to look at first, and how many issues were filed against how many the tracker already had.
Nothing else. The report is the deliverable.

---

## The eight lanes

Each is a prompt file with the sites it starts from already filled in, so no lane is a vague
instruction to go and look at things. The first four are the four shapes a violation takes in a
diff, from [reviewing-for-the-rule.md](../../../docs/development/reviewing-for-the-rule.md).

| Lane | The question | Starts from |
|---|---|---|
| `laundering` | does the label a value is built with dominate every input's label? | every `Labelled::new` and `Labelled::trusted` outside `crates/core`, and every `into_trusted` |
| `decisions-after-release` | is the released value carried and handed to an effect, or does control flow depend on it, and is that effect the one somebody approved? | every `Labelled::declassify` site and its enclosing function |
| `gates` | can a witness be minted outside the four gates, or read as permission to inspect? | every `Declassification::authorise` and every use of the four `Policy` gates |
| `known-costs` | is the enumerated attacker gain still complete against the code as it stands? | each entry under `## Known costs` in `labels.md` |
| `entry-to-planner` | can these bytes reach a model's context or steer a turn already running? | each road in from the `LABEL-8` table |
| `clause-permits-violation` | could an implementation satisfy every clause here and still break the rule? | the normative clauses of the trust specs |
| `unpinned-guarantee` | what holds today that nothing would fail on if it stopped? | `verified-by: none` clauses, the `by-construction` brackets that answered the rest, and label-touching code no spec governs |
| `supply-chain` | what executes with this tree checked out or gets linked into the binary, and who decides what that is? | the workflows, the lock file, and the second network client |

---

## What each pass decides

| Check | Pass | Fails on |
|---|---|---|
| Two documents disagree about how many exceptions are admitted | mechanical | error |
| A document admits exceptions without counting them | mechanical | warning |
| The register and the clause disagree on how many prompts a verdict may answer | mechanical | error |
| Only one of the two counts the prompts a verdict may answer | mechanical | warning |
| A field documented as read in one place that is read in two | mechanical | error |
| A trait `impl` on `Labelled` that reaches its content | mechanical | error |
| A constructor of `Labelled` that no spec pins to a count | mechanical | error |
| A workflow step on a tag or a branch rather than a commit | mechanical | error |
| A container image on a tag rather than a digest | mechanical | error |
| A job holding `id-token: write` or a secret that installs or runs a dependency | mechanical | error |
| A checkout of a bare name a branch and a tag can share, rather than a `refs/` ref | mechanical | error |
| A clause at `verified-by: none` | mechanical | warning |
| A label built with more trust than its inputs had | lane, then verifier | error |
| A decision taken from a released value | lane, then verifier | error |
| A clause a conforming implementation can satisfy and still break the rule | lane, then verifier | error |
| A guarantee that holds and nothing pins | lane, then verifier | warning |
| A lane that returned nothing usable | mechanical | error |

---

## Scope

Everything in the repository, including the specs themselves. The specs being in scope is the
point: a clause that permits a violation is the finding no other pass can return. What is out
of scope is fixing anything, rewording a clause, and creating a label.

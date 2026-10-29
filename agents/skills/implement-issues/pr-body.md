# The body of a batch pull request

The reader is a maintainer who has not seen this code or these issues and decides within half a
minute whether to read the diff. The body answers three questions in that time: can a person using
bravebot tell this landed, how do they see each problem themselves, and what was wrong. Everything
else is optional, and the diff is not narrated.

`pr-fix.py open` checks the shape below. It cannot tell whether a reproduction reproduces or a
problem statement is true.

## Shape

````markdown
Closes brave/bravebot#101
Closes brave/bravebot#102
Part of brave/bravebot#103

> [!WARNING]
> **The commit hook was bypassed for 1 dependency change.** `git commit --no-verify` was used once
> for each commit below, because the hook refuses a staged dependency file.
>
> - `<sha>` <subject>: `<file>` adds `<crate> <version>`, or changes `<crate> <old> to <new>`.
>   Buys: <what it buys>. Instead: <what existing code or writing the behaviour would cost>.
>   Brings: <transitive crates, build scripts, native code and the targets it builds for, or
>   "nothing beyond the crate">. Licence and upkeep: <the licence, and who maintains it>.

User impact: <one line: what a person using bravebot can now see, or "none" and why>

## Decisions to confirm
1. #102: <what was chosen>. Instead: <the alternative>. Why: <reason>. Commit: <subject>.
   To reverse: <what it takes>.

## The problem
**#101 <what it is, in your words>.** <1 to 2 sentences: what goes wrong, who hits it, what they
see.>

**#102 <what it is>.** <1 to 2 sentences.>

## Reproduce
<details>
<summary>#101 <what it is></summary>

1. <step a person takes in the running product>
2. <step>

<What happens, and what should happen.> `cargo test -p <crate> <filter>` covers it and fails on the
parent commit.
</details>

<details>
<summary>#102 <what it is></summary>

...
</details>

## The fix
**#101.** <1 to 2 sentences in plain language: what changed and why that fixes it.>

**#102.** <1 to 2 sentences.>

## Test plan
- [x] `BRAVEBOT_ALLOW_UNCONFIGURED_BUILD=1 make check-affected`: passed on <sha>
- [x] <test>: rejects <the plausible wrong implementation>; demonstrated by <fault>, which fails it
- [ ] CI passes cleanly
````

The four headings `## The problem`, `## Reproduce`, `## The fix` and `## Test plan` appear in that
order, and the last line is exactly `- [ ] CI passes cleanly`. `## Decisions to confirm` sits above
`## The problem`. Each of the four has one block per item, in ledger order, naming the item as
`#101`. Use no subheading inside them: the checkers read each heading as the start of a new
section, which leaves the one above it empty.

The checkers warn at about 110 words in `## The problem`, 400 words and 60 lines of visible prose
before the test plan. A batch passes only if each item costs a sentence or two in the open.
Everything an item needs beyond that goes in a `<details>` block whose summary names it: its
reproduction, a long root cause, its per-behaviour evidence. With one or two items the reproduction
stays in the open, since it is the part a reviewer wants most.

## The closing lines

Every `Closes` line comes first, then the `Part of` lines, each in the `brave/bravebot#<n>` form (a
bare `#<n>` closes nothing in another repository). The first line is the issue number `... open`
is given. An item gets `Closes` when every row of its checklist is `met`, and that is the default.
An item with a `blocked` row gets `Part of`, and its entry under `## The problem` ends with a
sentence naming the follow-up that holds the rest, which is an issue of its own and never the issue
the pull request is for:

```text
Left: <what is left of #103> (#NNNN).
```

An item that is blocked and has no commits gets no closing line and the same sentence.

## The dependency callout

Present only where a commit bypassed the hook, and then the first thing under the closing lines,
before `User impact:`. It is a `> [!WARNING]` block, so GitHub draws it apart from the text, and it
holds one bullet for each commit that touched a dependency file: the commit, the file, the
dependency and its version (old and new where it is changed), and the four statements of
[DEP-001](../../../docs/best-practices/dependencies.md#DEP-001) that the template gives slots to:
what it buys, what existing code would cost, what it brings, and its licence and upkeep. It is
never a `## Decisions to confirm` entry, never inside a `<details>` block, and never left to the
test plan. The same text is posted as a comment on the pull request once it is open, because
editing a body notifies nobody. `... open` refuses a body whose callout does not name each such
commit by its hash. [Dependency changes](SKILL.md#dependency-changes-and-hook-bypasses) says when
the bypass is allowed and how the list is checked.

## User impact

One line above the first heading. For several items it is one sentence naming what changes on
screen or in behaviour. A change nobody can see says why: `none -- tests only` or
`none -- a spec and its tests`. "Improves reliability" names nothing a person can see.

## Decisions to confirm

Always present. It lists every decision SKILL.md defines, numbered, each with its item, the choice,
the alternative, the reason, the commit and what reversing takes. When there are none it reads
`None.`. Never put it inside `<details>`.

## The problem, Reproduce and The fix

- **The problem** gives the symptom before the mechanism. An item that was already fixed says so
  here and points at the test that now pins it.
- **Reproduce** gives numbered steps in the running product, complete from a cold start, in a
  `<details>` block per item when there are more than two: the front end, the launch command, the
  directory and whether its files are trusted, the exact message.
  A stand-in model server is named as one, with the command that starts it and the calls it makes
  in a fenced block apart from the person's steps. A change under `crates/tui/` shows a captured
  screen per state a reader has to be told about, in a fenced block. For a change nobody can see,
  the test is the reproduction. Where nothing can reproduce it here, a line starting
  `Not reproducible locally:` gives the reason and links the evidence.
- **The fix** gives the mechanism after the reader knows why it matters. Depth goes in
  `<details><summary>...</summary>` blocks: a table, a long root-cause chain, a spec reference.

## The test plan

List the commands actually run as checked boxes, with their result. Per behaviour, give the test,
the plausible regression it rejects, and whether the failure was demonstrated (the fault restored,
the failure seen) or the coverage is reasoned only. A passing test count is not evidence.

Also state, where each applies: the commit the gates ran on and the commit the branch was rebased
onto afterwards, with the areas that do not reach the diff; each flaky test, with its isolated
run; a failure that also fails on the parent commit, in a `<details>` block that cites the issue
tracking it; and
`Docker gates skipped: host-only`.

## Never

A preamble ("This PR adds"), self-assessment (`robust`, `comprehensive`, `cleanly handles`), emoji
headings, a file-by-file changelog, a bare clause or ticket id with no words or link, a restatement
of an issue body, an em-dash, an AI attribution, and any account of how the work was done.

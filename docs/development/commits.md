# Commits and pull requests

**One change per commit, with its tests in that same commit.** A commit is the unit somebody reads,
reverts, and bisects on, so it has to stand up alone: the change, the tests that pin it, and any
documentation the change makes wrong if it lands without it. Tests that arrive a commit later say
the behaviour went in unverified, and a bisect that lands between the two hits a revision passing
for the wrong reason.

Keep them small. If the message needs an "and" to describe what the commit does, it is usually two
commits. Every commit must leave the tree building and passing, since that is the whole of what
makes a history worth bisecting.

For behaviour or test changes, the pull request's testing summary must answer: **Which plausible
regression do the new or changed tests reject?** Name the relevant tests and distinguish an
observed failure on broken code from coverage inferred by reading the test. Include checks not
run and any gaps; a passing test count alone does not answer the question. Follow the
[testing-preflight skill](../../agents/skills/testing-preflight/SKILL.md) for the evidence needed.

A spec clause ships with the work it describes, in that same commit. [spec-enforced-development.md](spec-enforced-development.md) says what
that means and what it costs to do otherwise.

**Close a GitHub issue from the commit and the pull request that finish it.** Where the change fully
resolves the issue, use GitHub's closing syntax (`Closes #123`, `Fixes #123`) so that merging closes
it. Where the change is only part of what the issue asks for, name it without the keyword (`Part of
#123`): an issue closed while the rest of it is outstanding is worse than one left open.

## Opening the pull request

The pull request belongs on `brave/bravebot`. A bare `gh pr create` targets whatever repository
`gh` resolves from the clone, so in a clone of a fork it opens the pull request on the fork. Check
which case applies before running it:

```sh
git remote get-url origin
```

- `origin` is `brave/bravebot` (a direct clone): push the branch, then run
  `gh pr create --base main` from it.
- `origin` is a fork: push the branch to the fork, then run
  `gh pr create --repo brave/bravebot --head <fork-owner>:<branch> --base main`, where
  `<fork-owner>` is the account in the `origin` URL.

A worker that was handed a branch to push does not open the pull request unless its brief says to.
It reports the branch, and whoever owns the pull request opens it with the command above.

**A pull request opened by `netzenbot` says which cases it leaves undone, with an issue for each.** The body has a line
`Not covered: none`, or `Not covered:` followed by one bullet per case, each naming the issue that
tracks it as `#N` or `brave/bravebot#N`:

```text
Not covered:
- `aws` keeps bravebot's own PATH (#NNNN)
- hooks named by a relative path (#NNNN)
```

A case is another call site, platform or program with the same weakness as the one the change
fixes, or another defect found while making it. File the issue first, with the
[issue-poster](../../agents/agents/issue-poster.md) definition, so that its number exists when the
body is written. A case that a spec records as accepted under Known costs is cited in the body where
it is described, as [GAP-001](../best-practices/gaps.md#GAP-001) says, and is not a bullet, since an
accepted case has no issue.

`make check-pr-body` fails a pull request opened by `netzenbot` whose body has no such line, or has
a bullet that names no issue. CI runs it when the body is edited as well as when the branch is
pushed. Pull requests by other authors are not held to the line, and
[GAP-001](../best-practices/gaps.md#GAP-001) still applies to what their description says. The check
cannot read whether the list is complete, whether a number is an issue, or whether an issue is about
its case, so the reviewer reads the diff for the cases it leaves.

**No co-attribution markers for Claude Code or other tools**, in a commit message or a pull request
body.

**Never use an em-dash.** Not in documentation, commit messages, the README, code comments, pull
requests, or anywhere else. Reword instead: a comma, a colon, a semicolon, parentheses, or two
sentences will always do the job.

**Never write about your own process.** Not in a commit message, a pull request, a spec, a comment
or a reply. A correction of an earlier claim, or an account of what was guessed, assumed or
checked, tells a later reader nothing about the code, because they did not see the conversation it
came from. State what is true about the code in the present tense, as though for the first time.
Where a correction matters, the corrected fact is all of it: write "an absent store reports
nothing and the turn spends no subscription", and leave out that an earlier claim said otherwise.

`make check-narration` fails a pull request whose commit messages, title, body or added lines
contain one of a fixed list of first-person phrases. It does not catch a paraphrase, so the rule
applies past the list.

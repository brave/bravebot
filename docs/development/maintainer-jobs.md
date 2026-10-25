# Maintainer jobs

Most checks run on every pull request and need nothing from a maintainer. The jobs on this page do
not: each is a skill a maintainer runs, because it needs a model to read code, the tracker or other
projects' documentation. Each one finds something and files issues. None of them edits source.

## What a run looks like

Every job follows the same steps:

1. Run the skill from the main clone, on `main`, with a clean tree.
2. Read the drafts it produces. Each draft is one issue, and nothing is filed yet.
3. Name the drafts to post. A skill files them through the
   [issue-poster](../../agents/agents/issue-poster.md) definition, which searches open and closed
   issues first, so a run does not file what the tracker already holds.
4. Commit the ledger the skill wrote, if it has one, on a branch and open a pull request. A ledger
   records what was reviewed, so the next run, on any machine, reads only what is new.

Filed issues are assigned to netzenbot, and `/pr-fix <issue>` implements one.
[`/implement-issues`](../../agents/skills/implement-issues/SKILL.md) implements a list of them, or
every issue a search returns, in one pull request, and `/pr-fix <pr>` handles its review comments.
Fixing is a separate step from finding, and a run never does both.

## The jobs

The schedules are how often to run each job. Only the peer-advisories schedule is checked by a tool
(see [Before a release](#before-a-release)).

| Job | Question it answers | Schedule | Ledger to commit |
|---|---|---|---|
| [`/peer-advisories`](../../agents/skills/peer-advisories/SKILL.md) | Do security advisories published against other coding agents describe a defect bravebot shares? | Weekly | `docs/peer-advisories-vetted` |
| [`/triage-issues`](../../agents/skills/triage-issues/SKILL.md) | Which open issues are already fixed, stale or still standing, and what are their importance, urgency and size? | Every two weeks, and before a release | none (it edits labels and closes issues) |
| [`/session-review`](../../agents/skills/session-review/SKILL.md) | Where did bravebot fail the person using it in recorded sessions? | Weekly, on a machine that has sessions | none (a marker is written beside each session, locally) |
| [`/security-audit`](../../agents/skills/security-audit/SKILL.md) | Does the rule hold that untrusted content never enters the driver or the planner? | Monthly, and `/security-audit changed` before asking for review on a diff that touches a label | none (it skips what the tracker holds) |
| [`/check-spec`](../../agents/skills/check-spec/SKILL.md) | Does the code do what each spec clause says? | Monthly, and `/check-spec changed` before committing a change to code a spec governs | `docs/check-spec-reviewed` |
| [`/maintenance`](../../agents/skills/maintenance/SKILL.md) | What code could be deleted, shared or split without changing what it does? | Monthly, and after a large run of feature pull requests | `docs/maintenance-reviewed` |
| [`/peer-features`](../../agents/skills/peer-features/SKILL.md) | What do other coding agents document that bravebot lacks, and where could bravebot be about 10% better than the best of them? | Monthly | `docs/peer-features-reviewed` |
| [`/update-docs`](../../agents/skills/update-docs/SKILL.md) | What has changed in bravebot that the documentation site does not say yet? | Before each release, and after a batch of user-visible changes | `docs/docs-updated-to-sha` |

## Notes on each job

**peer-advisories.** The default run vets the eight most severe advisories not yet vetted. When it
has finished, it records the run in a comment on
[#1901](https://github.com/brave/bravebot/issues/1901), which is how the release check knows it ran.
Run it again with an advisory id to revisit a verdict of `absent` after bravebot gains the feature
the advisory is about.

**triage-issues.** It closes an issue only after verifying the commit it cites is on `main`. It
never touches an issue a bot opened, and it never sets `urgency/p1`, which is a person's call.
`/triage-issues label:urgency/p2` lists the queue to work down. What the labels mean is in
[labelling-issues.md](labelling-issues.md).

**session-review.** Sessions are recorded per machine under `~/.bravebot/sessions`, so run it where
you use bravebot. It skips sessions written in the last 30 minutes. A `/note` typed during a session
is read as a lead.

**security-audit.** A full run costs many subagent tokens. The deterministic half is
`make check-security`, which CI runs on every change. The full run is the one to schedule.

**check-spec.** The deterministic half is `make check-spec`, which CI runs on every change. The
conformance review is the job. When the code and a clause disagree, the code is wrong, and the skill
never edits a spec. A repeat run reviews only the clauses whose code changed since their last
review. `/check-spec full` reads every clause again.

**maintenance.** `make check-maintenance` prints the mechanical candidates with no model and no
clippy lane. The skill reads each candidate and drafts issues only for the ones worth fixing, then
has a reader check each draft's claim against `HEAD` (one reader for every four to six drafts) and
marks a rejected draft with the reason. A
cleanup that would move a decision about labelled content is rejected, whatever it saves.

**peer-features.** The default run reviews six units, specs first and then the least recently
reviewed. A reviewed unit is offered again after 90 days. A gap that was declined is not reopened by
a later run. To add a tool to the comparison, add a line to
[agents/skills/peer-features/peers.tsv](../../agents/skills/peer-features/peers.tsv).

**update-docs.** `make docs-changes` lists what has landed since the site was last updated, and
`make docs-updated-to-sha` prints the commit the site is current as of. Neither is a gate. Run the
skill when the list has grown, and commit the documentation changes it makes, and the updated
baseline, together.

## Checks that run without a maintainer

| Check | When | What a maintainer does |
|---|---|---|
| `make check-lint-increase` | Every commit that touches code `make check` runs, and every pull request in CI | Nothing. A pull request that adds sites of a lint the tree has many of fails, and its author removes them or marks a site `#[expect(clippy::lint, reason = "...")]`. Do not raise the base. |
| [`mutation-testing.yml`](../../.github/workflows/mutation-testing.yml) | Sundays | Read the report for `bravebot-core`. Each mutation no test noticed is a test to add. |
| [`test-determinism.yml`](../../.github/workflows/test-determinism.yml) | Mondays | Read the report. Each test that failed on an unchanged tree is a flaky test to fix. |
| [`fuzz.yml`](../../.github/workflows/fuzz.yml) | Wednesdays | Read each failed run. Its log and artifact hold the input that stopped the target. File it as [fuzzing.md](fuzzing.md#when-a-target-finds-something) says. |

No scheduled run gates a pull request. A report nobody reads is the same as no report.

## Before a release

`make bump-version` asks before it continues when any of these hold:

- an open issue carries the `release-blocking` label;
- a translation is missing messages;
- `/peer-advisories` last ran more than seven days ago.

Beyond those, run `/triage-issues` so the `release-blocking` label is accurate, and run
`/update-docs` so the site describes what ships. [releasing.md](releasing.md) has the rest.

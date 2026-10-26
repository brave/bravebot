# Branch protection on main

Branch protection is a repository setting, so no file in a checkout can read it. This page records
what `main` is set to and why, so that a change to the setting is a line in a diff rather than
something a maintainer remembers. The required check contexts are the other half of the setting and
are listed in [../../contrib/required-checks.txt](../../contrib/required-checks.txt).

## Required check contexts

A job named in that file holds a merge only where the setting requires its context, so a maintainer
adds the context of each job the file gains. `Fuzz build and replay` is the job that runs
`make check-fuzz`.

## Pull request reviews

| Setting | Value |
|---|---|
| `required_approving_review_count` | 2 |
| `require_code_owner_reviews` | true |
| `require_last_push_approval` | false |
| `dismiss_stale_reviews` | false |

Read the live values with a maintainer's token:

```sh
gh api repos/brave/bravebot/branches/main/protection/required_pull_request_reviews
```

## Why `dismiss_stale_reviews` is false

With it true, GitHub dismisses every approval when a new commit is pushed. There is no exception
for a trivial push or for a particular reviewer, and nothing in this tree dismisses reviews. A
pull request approved by netzenbot-reviewer lost that approval when netzenbot pushed a one-line
change, and had to be reviewed again from nothing.

Two alternatives keep a stale approval from counting and were not taken:

- A job that re-reviews after a netzenbot push and re-approves when the change is trivial. The
  pull request stays blocked until that job runs, and the review-prs job is not in this repository.
- `require_last_push_approval: true`. It requires an approval given after the last push, so a
  trivial push needs a fresh approval, which is the cost the setting was changed to remove.

## What the setting costs

The setting applies to every pull request and every reviewer, so a human approval also survives
later pushes. A push that changes what a person approved still merges on the old approval. Nothing
requires a re-review after such a push. What exists is the pr-fix skill, which asks
netzenbot-reviewer to review the new head after each push it makes
([../../agents/skills/pr-fix/SKILL.md](../../agents/skills/pr-fix/SKILL.md)). Whoever merges
compares the head they are merging against the head that was approved.

Whether a substantive push after an approval should require a re-review is open. Answering it means
changing the setting or adding a check, and both change this page.

## The npm-publish environment

`publish-npm.yml` runs the file at the ref it is dispatched on, and the npm trusted-publisher record
checks the repository and the workflow filename, not the ref. The `platform` and `publish` jobs, the
two that hold `id-token: write`, therefore name `environment: npm-publish`
([RELEASE-11](../specs/releases.md#RELEASE-11)). The environment is a repository setting, so this
page is where it is recorded:

| Setting | Value |
|---|---|
| Environment name | `npm-publish` |
| Required reviewers | a maintainer who is not the person dispatching |
| Prevent self-review | true |
| Deployment branches and tags | `v*` tags and `main` (narrows the ref; the reviewer is the control) |

On npmjs.com, each of the seven packages' trusted-publisher record names the environment
`npm-publish` beside the repository and `publish-npm.yml`. A dispatch from a ref that is neither a
tag nor `main`, or one nobody approves, then publishes nothing. Read the live values with a
maintainer's token:

```sh
gh api repos/brave/bravebot/environments/npm-publish
```

`make check-security` holds only that each job with the grant names an environment.

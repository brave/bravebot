# Branch protection on main

Branch protection is a repository setting, so no file in a checkout can read it. This page records
what `main` is set to and why, so that a change to the setting is a line in a diff rather than
something a maintainer remembers. The required check contexts are the other half of the setting and
are listed in [../../contrib/required-checks.txt](../../contrib/required-checks.txt).

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

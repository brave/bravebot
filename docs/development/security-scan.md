# The security scan

Pull requests here are scanned by an organization-level workflow that runs
[brave/security-action](https://github.com/brave/security-action) and comments through reviewdog.
Nothing in this repository configures it, so the runners and the rule set live over there.

```sh
make check-reviewdog       # what this branch changed, against its merge base
make check-reviewdog-full  # the whole tree, however old the finding is
```

`contrib/check-reviewdog.sh` clones that repository, pins the tool versions its `action.yml` pins,
and drives the same reviewdog runners against this checkout: **opengrep** (the semgrep fork, on
Brave's rule set), **npm-audit**, **pip-audit**, **safesvg**, and **sveltegrep**. Only the runners
with something to look at are enabled, which here means opengrep and npm-audit. A finding is one
the bot would post, so run `check-reviewdog` before pushing; the full scan reports plenty that
predates any given branch.

The branch is measured against `upstream/main` where a checkout has one, and `origin/main`
otherwise. In a fork checkout `origin` is the fork, whose `main` moves only when somebody
updates it, so a base taken from there is a commit the branch is not based on: the scan then
covers every commit the fork is behind and reports whatever it finds in them against the
branch. `--base` takes any ref, a sha or a tag included.

The first run downloads opengrep, reviewdog and the rules into `~/.cache`; later runs re-use them
and take about half a minute. No model is involved, so both are deterministic.

A runner failure makes the check fail, including a scanner failure hidden by a later
formatter in its pipeline or by reviewdog filtering out all of its findings. Runner stderr
also fails the check so a partial scan cannot report a clean result. Both scan targets first
run `make check-reviewdog-selftest` to test empty scans, findings, and scanner failures.
`make check-scripts` also tests check orchestration. A full scan clears any inherited branch baseline.
Failure to list the files or set the baseline also fails the scan.

Opengrep's baseline mode considers committed changes only. When tracked files have
uncommitted changes, the local check scans without that baseline and lets reviewdog
filter findings to the branch and working-tree diff. Stage new files before scanning.

Branch scans select `sveltegrep` only when the diff includes an HTML or Svelte file
that still exists. Without those inputs, its search for extracted scripts would walk
ignored build output and nested worktrees for no benefit. Full scans and explicit
`--runners` selections retain it. Branch scans print each runner's start and finish.

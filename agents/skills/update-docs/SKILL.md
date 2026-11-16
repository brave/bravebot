---
name: update-docs
description:
  'Bring the documentation site under docs/website/ up to date with bravebot.
  Picks up from the commit recorded in docs/docs-updated-to-sha, or reviews an
  explicit span of commits, folds the user-facing behaviour into the pages it
  belongs on, records how far it got, and builds the site cleanly. Triggers on:
  update docs, /update-docs, sync docs with specs, docs drift.'
argument-hint: '[rev-range] [dry-run]'
allowed-tools: Bash(make -C docs/website *), Bash(python3 agents/skills/update-docs/docs-ref.py *), Bash(git *), Bash(python3 agents/skills/pr-fix/pr-fix.py *), Bash(gh pr create *), Bash(gh pr view *), Bash(grep *), Bash(rg *), Read, Edit, Write
---

# Bring the Documentation Site Up to Date With Brave Bot

This skill documents user-facing behaviour changes across a span of commits or spec clauses, folding them into the appropriate pages under `docs/website/docs/`.

* **Baseline run** (`/update-docs`, no argument): starts from the commit recorded in `docs/docs-updated-to-sha` and works forward. This is the normal mode.
* **Range run** (`/update-docs <rev-range>`): inspects commits in `<rev-range>` (e.g., `HEAD~20..HEAD` or `<tag>..HEAD`), determines what user-facing behaviour or configuration changed, and updates the relevant pages. Use this to walk a large backlog in slices.
* **Unmapped check**: runs `make check-spec` to find normative clauses lacking a page reference or unverified against pages.
* `dry-run`: reports what would change and writes nothing.

---

## Where to Start Reading, and Recording Where You Stopped

`docs/docs-updated-to-sha` holds the commit the site has been brought up to, so a run reads forward from there instead of re-reading the whole history. `docs-ref.py` is the deterministic half of this skill: no model is involved, so it is cheap and its output is reproducible.

```sh
python3 agents/skills/update-docs/docs-ref.py changes --full
```

That prints the span oldest first, with bodies and file lists, and replays anything still deferred ahead of it. On `Up to date.` there is nothing to do and the run stops there.

A run is not finished until it records where it got to, because the next run reads only that:

```sh
# everything up to here is folded in
python3 agents/skills/update-docs/docs-ref.py set <sha>

# reviewed, deliberately not documented yet, and why
python3 agents/skills/update-docs/docs-ref.py defer <sha> <reason>
```

Use `defer` rather than silently skipping. `set` claims everything behind a sha is done and the next span begins after it, so a commit passed over mid-span is invisible to every run that follows. A deferral stays on the list and keeps being offered until `resolve <sha>` drops it.

A deferral needs a reason a later run can act on. "Behaviour is in the code but no settings key reaches it yet" is one; "unclear" is not.

---

## Writing Style Rules

1. **User-facing perspective**: The reader is using Brave Bot, not developing it. Explain what happens, how to use it, and why it is safe. Never mention Rust internal types, internal struct names, or low-level crate architectures unless they are part of user configuration.
2. **Concise and direct**: Add or amend sentences only where a reader would act differently or be misled without the detail. Avoid transcribing internal spec rationale.
3. **Relative links**: All links between documentation pages must be relative and end in `.md`.
4. **Zero em dashes**: Never use em dashes anywhere. Use colons, commas, parentheses, or separate sentences instead.

---

## The Topic Mapping

Each spec already names its pages: `documented-by` in its front matter, which `make check-spec`
holds to pages that exist. Read the spec a commit touched and that is the answer. The table below is
the fallback for a change no spec covers, and for deciding where something new belongs.

When a change passes the gate, map it to the corresponding page in `docs/website/docs/`:

| A bravebot spec about | Belongs on |
|---|---|
| labels, who may read what | `docs/website/docs/security/trust.md` |
| routing, where an effect may land | `docs/website/docs/security/permissions.md` |
| the trust map, vouched paths | `docs/website/docs/security/trust.md`, `docs/website/docs/customize/skills.md` |
| a tool's arguments, refusals, results | `docs/website/docs/reference/tools.md` |
| shell mode, `run` | `docs/website/docs/using/shell-mode.md` |
| terminal input, the transcript | `docs/website/docs/using/interactive-mode.md`, `docs/website/docs/using/transcript.md` |
| sessions, compaction, undo | `docs/website/docs/using/sessions.md`, `docs/website/docs/using/context.md` |
| prompting, permission gates | `docs/website/docs/security/permissions.md` |
| skills, AGENTS.md, instructions | `docs/website/docs/customize/skills.md`, `docs/website/docs/customize/instructions.md` |
| the trace and audit trail | `docs/website/docs/security/audit-trail.md` |
| flags, environment variables, CLI defaults | `docs/website/docs/reference/cli.md`, `docs/website/docs/customize/configuration.md` |
| backends, model rosters, providers | `docs/website/docs/customize/configuration.md` |
| `settings.json`, its keys, precedence | `docs/website/docs/customize/configuration.md` |
| slash commands | `docs/website/docs/reference/commands.md` |
| premium credentials | `docs/website/docs/customize/premium.md` |

Check the span against [coverage.md](coverage.md) before concluding an update.

---

## Verification

After making page updates, verify the site:

```sh
make -C docs/website check
```

This must pass cleanly. `onBrokenLinks` and `onBrokenAnchors` throw errors on any broken link or reference.

---

## Opening the Pull Request

A run that changed files ends by opening a pull request, without being asked. Do it in a new worktree cut from the current `upstream/main`, so the pull request holds only the documentation changes and no other work in the checkout.

1. `git fetch upstream main`, then `git worktree add -b docs/update-through-<number> <path> upstream/main`, where `<number>` is the pull request number in the title of the commit recorded by `docs-ref.py set`.
2. Copy the changed files into the worktree, including `docs/docs-updated-to-sha` and `docs/docs-deferred`. Leave out files the run did not write, such as `docs/maintenance-reviewed`.
3. Commit in the worktree with the person's own git configuration, following [commits.md](../../../docs/development/commits.md). The commit title is `docs: bring the site up to the specs through #<number>`.
4. Push the branch to `origin` with `git push -u origin <branch>`. Never push to `upstream`.
5. Open the pull request with `gh pr create --repo brave/bravebot --base main --title <title> --body-file <file>` from the worktree, so `gh` takes the head from the branch's remote. The body lists the pages changed, the commits deferred with their reasons, and that `make -C docs/website check` passed. It does not describe how the run went.

A change to this skill is a separate pull request from its own worktree, never part of a documentation pull request.

### Waiting for the checks and the review

The pull request is not finished when it is opened. It is finished when its checks have passed and netzenbot-reviewer has reviewed the current head and asks for nothing, or one of the limits below ends the wait. Use [pr-fix](../pr-fix/SKILL.md) for each step, from the worktree. Below, `...` stands for `python3 agents/skills/pr-fix/pr-fix.py`, and the `run` arguments are the ones pr-fix gives for each step.

1. `... review <pr>` asks netzenbot-reviewer for a review of the head.
2. `... ci <pr> --wait` with `deadline_seconds: 600`. Repeat it until no check is pending.
3. Read the verdict with `gh pr view <pr> --repo brave/bravebot --json state,isDraft,headRefOid,reviews,reviewRequests`. Only a review by netzenbot-reviewer made at `headRefOid` counts, since a push makes every earlier review a review of different code. A `PENDING` or `DISMISSED` review is ignored. The verdict is:
   - approved: `APPROVED`, or `COMMENTED` with none of the reviewer's threads open in `... comments <pr>`. The wait is over.
   - changes: `CHANGES_REQUESTED`, or `COMMENTED` with a thread of the reviewer's open. Do a fix round.
   - pending: no counted review at the head. Poll in one `run` call that reads the verdict and sleeps 10 seconds in a loop until a counted review exists or 590 seconds have passed, then read it and call again.
   - closed: the state is not `OPEN`. Stop and report.
4. A fix round follows pr-fix steps 3 to 7 with `... start <pr>`: fix each failing check and make the change each open comment asks for and nothing beside it, commit the fixes as one new commit and never amend, `... push <pr>`, post a reply under each thread with `... reply`, and never resolve a thread. A comment that is mistaken, or that asks for something outside the pull request's changes, gets a reply and no change. Then go back to step 1.

Stop and report what is left when:

- the checks have failed three times in a row;
- the reviewer has asked for changes five times;
- a fix round pushed nothing because every comment was answered with a reply;
- 120 minutes have passed since the last push with no review of the head, in which case report that the review is still pending at that head;
- the pull request was merged or closed.

A failing check that does not touch the diff and also fails on the base is not this pull request's to fix: report it and leave it.

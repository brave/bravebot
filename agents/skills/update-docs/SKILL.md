---
name: update-docs
description:
  'Bring the documentation site under docs/website/ up to date with bravebot.
  Picks up from the commit recorded in docs/docs-updated-to-sha, or reviews an
  explicit span of commits, folds the user-facing behaviour into the pages it
  belongs on, records how far it got, and builds the site cleanly. Triggers on:
  update docs, /update-docs, sync docs with specs, docs drift.'
argument-hint: '[rev-range] [dry-run]'
allowed-tools: Bash(make -C docs/website *), Bash(python3 agents/skills/update-docs/docs-ref.py *), Bash(git *), Bash(gh pr create *), Bash(grep *), Bash(rg *), Read, Edit, Write
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
3. Commit as netzenbot, signed, with the netzenbot name, email and signing key set through environment overrides. The pre-push hook requires the author email `netzenbot@agentmail.to`. The commit title is `docs: bring the site up to the specs through #<number>`.
4. Push to `origin`, which is netzenbot's fork, with `GIT_SSH_COMMAND` set to the netzenbot key. Never push to `upstream`.
5. Open the pull request as netzenbot with `gh pr create --repo brave/bravebot --head netzenbot:<branch> --base main`. The body lists the pages changed, the commits deferred with their reasons, and that `make -C docs/website check` passed. It does not describe how the run went.

A change to this skill is a separate pull request from its own worktree, never part of a documentation pull request.

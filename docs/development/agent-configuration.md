# Agent configuration

`agents/` is the checked-in source of truth for what an agent reads in this repo: `AGENTS.md`,
`settings.json`, the skills under `agents/skills/` and the subagents under `agents/agents/`.
Nothing discovers it there. Claude Code looks under `.claude/`, Codex reads `AGENTS.md` at the
workspace root, skills from `.agents/skills` and subagents from `.codex/agents`, Cursor reads
`AGENTS.md` at the root and skills and subagents under `.cursor/`, and bravebot reads `AGENTS.md`
at the workspace root, skills from `.bravebot/skills`, `.claude/skills` and `.agents/skills`, and
delegate definitions from `.bravebot/agents`, so a fresh clone links the one source into all
four:

```sh
make init
```

That copies the checked-in hooks from `.githooks/` into `bravebot-hooks` in the repository's git
directory, points Git at the copy by absolute path, creates these symlinks, and writes the Codex
files:

```
.claude/skills/<name>       ->  agents/skills/<name>
.agents/skills/<name>       ->  agents/skills/<name>
.bravebot/skills/<name>     ->  agents/skills/<name>
.cursor/skills/<name>       ->  agents/skills/<name>
.claude/agents/<name>.md    ->  agents/agents/<name>.md
.cursor/agents/<name>.md    ->  agents/agents/<name>.md
.bravebot/agents/<name>.md  ->  agents/agents/<name>.md
.claude/CLAUDE.md           ->  agents/AGENTS.md
AGENTS.md                   ->  agents/AGENTS.md
.claude/settings.json       ->  agents/settings.json
.bravebot/settings.json     ->  agents/settings.json
.codex/agents/<name>.toml   <-  agents/agents/<name>.md   (written, not linked)
```

The links, the Codex files and their discovery directories are gitignored, so they are derived
state and a skill is written once rather than copied once per tool. Re-running is idempotent and
silent, a stale link is refreshed, and a real file somebody put in a discovery directory by hand
is left alone rather than replaced.
`python3 agents/setup.py list` shows the current state, and `unlink` removes only the links and
files it owns.

The hooks are a copy because Git resolves a relative `core.hooksPath` against each worktree's own
top level, so in the worktree of a pull request it would run the hooks that pull request tracks,
as whoever runs Git there. No branch tracks a copy in the git directory, and `/pr-fix` and
`/rebase` give Git that directory for every command they run. A clone that has not run `make
hooks` since this changed has no such directory, so no hook runs in those steps. Run `make hooks`
again when `.githooks/` changes. Run it in the main checkout: in a linked worktree it installs
nothing, since the copy would come from that worktree's branch.

Claude Code, Cursor and bravebot all read subagents as Markdown, so `agents/agents/` links into
`.claude/agents`, `.cursor/agents` and `.bravebot/agents`. bravebot calls one a delegate
definition and ignores the frontmatter keys it has no use for
([DELEGATE-19](../specs/delegation.md#DELEGATE-19)), which is what lets one directory serve all
three rather than each tool needing a copy. Slash commands Cursor dropped in favour of skills, so
`agents/commands/` links into `.claude/` alone. Neither directory has to exist: whatever is
present is linked, and a directory added later needs no change to the script.

Codex reads a subagent from a TOML file in `.codex/agents` and does not read Markdown, so a link
would give it nothing. `make init` writes `.codex/agents/<name>.toml` for each definition. The
file takes `name` and `description` from the frontmatter and the text after the frontmatter as
`developer_instructions`, with each relative link rewritten to be relative to the repository root,
the directory a Codex session runs from. The other frontmatter keys have no Codex equivalent and
are dropped: `kind` and `tools` narrow what a bravebot delegate may do, and a Codex agent takes the
permissions of the session that spawns it.

Each generated file begins with a line saying `agents/setup.py` wrote it. `make init` rewrites a
generated file whose definition changed, removes one whose definition is gone, and leaves a file
without that line alone, so run `make init` again after editing a definition. `list` shows a file
that is behind as `STALE generated`. A definition with no name, no description or no text after
the frontmatter fails `make init` and `make check-spec`. `agents/setup.py` reads the frontmatter
the way bravebot does, with a wrapped value joined by spaces and a continuation line holding a
colon not read as a key. It is a second copy of that reading, because `make init` has to run
before anything is built.

Cursor reads `.agents/skills` and `.claude/agents` as compatibility paths, so some of this
already reached it through the Codex and Claude Code links. Its own paths are linked anyway:
they are what Cursor documents, they win a name conflict against the compatibility ones, and
support should not rest on shims Cursor may retire.

`make init` does not grant trust. bravebot loads a workspace skill or delegate definition only
from a path a person vouched for ([TRUST-1](../specs/trust-map.md)), and a script granting that on
your behalf is the inference that clause forbids, so expect to be asked about `.bravebot/skills`
and `.bravebot/agents` the first time you start it in this tree.

## Attribution

`agents/settings.json` is what keeps a co-attribution trailer off the commits and pull requests
made here:

```json
{
  "attribution": {
    "commit": "",
    "pr": ""
  }
}
```

Claude Code and bravebot both read a `settings.json` in this shape, bravebot resolving it
through the three layers of [BACKEND-24](../specs/backends.md), so one file serves both. Empty
means carry nothing. `AGENTS.md` asks for the same thing in prose and goes on doing so, because
Cursor reads the instructions and has no settings file to read instead.

Codex needs a step nothing here can take for you. Its `<git_attribution>` prompt section
requires a `Co-authored-by: Codex` trailer and a `Generated with [Codex]` line, and it says in
as many words to ignore earlier instructions disabling Codex attribution, so `AGENTS.md` loses
to it. The switch is `commit_attribution_enabled`, which lives in the account's user settings
and is fetched over the network: there is no `config.toml` key for it, and
`~/.codex/config.toml` is global with no project-level file beside it, so no checkout can carry
the answer. **Turn commit attribution off in your Codex account settings.**

A hand-written file in a discovery directory is left alone, settings included, and `make init`
says so in one line of output. That is the right answer for a skill and worth knowing about
here: keep your own `.claude/settings.json` and you get none of this, so copy the block above
into it.

## Checking the register when a task ends

`agents/settings.json` also holds a `Stop` hook for Claude Code:

```json
{
  "hooks": {
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "python3 \"$CLAUDE_PROJECT_DIR/agents/skills/security-audit/security-audit.py\" --stop-hook",
            "timeout": 120
          }
        ]
      }
    ]
  }
}
```

When a task ends, the hook lists what the branch has changed since it left `upstream/main`, or
`origin/main` without one: its commits, and the working tree's staged, unstaged and untracked
files. It does nothing unless one is a file where an answer is kept, asked for or registered: the
files `approval-binding` is selected by, the prompting spec and the files it governs, and the test
that applies the register's changes to its answers (`crates/agent/src/kept_answers.rs`). Otherwise it
runs the mechanical half of `make check-security`, about five seconds. A failure blocks the stop and
hands the findings back to the session to fix. A stop that follows a block is let through with the
findings shown to the person, so a session that cannot fix one still ends. CI runs the same check as
the required `Security` context, and is the check on a tree the hook never saw.

The same file is linked as `.bravebot/settings.json`, where the `hooks` key does nothing: bravebot
reads hooks only from `hooks.json` in its state directory, and `bravebot doctor` names the key as
one nothing reads. Codex and Cursor do not read the hook, so CI is the only check on work done in
them.

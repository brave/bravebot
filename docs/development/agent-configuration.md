# Agent configuration

`agents/` is the checked-in source of truth for what an agent reads in this repo: `AGENTS.md`,
`settings.json`, and the skills under `agents/skills/`. Nothing discovers it there. Claude Code
looks under `.claude/`, Codex reads `AGENTS.md` at the workspace root and skills from
`.agents/skills`, Cursor reads `AGENTS.md` at the root and skills and subagents under `.cursor/`,
and bravebot reads `AGENTS.md` at the workspace root and skills from `.bravebot/skills`, so a
fresh clone links the one source into all four:

```sh
make init
```

That points Git at the checked-in pre-commit hook and creates these symlinks:

```
.claude/skills/<name>    ->  agents/skills/<name>
.agents/skills/<name>    ->  agents/skills/<name>
.bravebot/skills/<name>  ->  agents/skills/<name>
.cursor/skills/<name>    ->  agents/skills/<name>
.claude/CLAUDE.md        ->  agents/AGENTS.md
AGENTS.md                ->  agents/AGENTS.md
.claude/settings.json    ->  agents/settings.json
.bravebot/settings.json  ->  agents/settings.json
```

The links and their discovery directories are gitignored, so they are derived state and a skill
is written once rather than copied once per tool. Re-running is idempotent and silent, a stale
link is refreshed, and a real file somebody put in a discovery directory by hand is left alone
rather than replaced.
`python3 agents/setup.py list` shows the current state, and `unlink` removes only the links it
owns.

Cursor also reads subagents, so `agents/agents/` links into `.cursor/agents` as well. Slash
commands it dropped in favour of skills, so `agents/commands/` links into `.claude/` alone.
Neither directory has to exist: whatever is present is linked, and a directory added later needs
no change to the script.

Cursor reads `.agents/skills` and `.claude/agents` as compatibility paths, so some of this
already reached it through the Codex and Claude Code links. Its own paths are linked anyway:
they are what Cursor documents, they win a name conflict against the compatibility ones, and
support should not rest on shims Cursor may retire.

`make init` does not grant trust. bravebot loads a workspace skill only from a path a person
vouched for ([TRUST-1](../specs/trust-map.md)), and a script granting that on your behalf is the
inference that clause forbids, so expect to be asked about `.bravebot/skills` the first time you
start it in this tree.

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

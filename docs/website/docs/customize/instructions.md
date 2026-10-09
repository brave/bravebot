---
sidebar_position: 1
title: Instructions
description: AGENTS.md holds standing instructions for a project or for every project.
---

# Instructions

Put standing instructions in `AGENTS.md` and they apply to every task in that directory.

```markdown
# AGENTS.md

Run `make check` before saying a change is done.
Prefer `edit_file` over rewriting a file.
Commit subjects are imperative; the body explains why, never what.
```

Instructions tell the planner how work is done here. To run a command of your own when something
happens, rather than to ask the planner for it, see [Hooks](hooks.md). For a whole procedure the
planner loads only when the task calls for it, see [Skills](skills.md). For a kind of delegate the
planner can hand a sub-task to, see [Delegate definitions](agents.md).

## The ten sources

| File | Applies to |
|---|---|
| `~/.bravebot/AGENTS.md` | every project |
| `~/.bravebot/skills/<name>/SKILL.md` | every project |
| `~/.bravebot/agents/<name>.md` | every project |
| `<workspace>/AGENTS.md`, else `CLAUDE.md`, else `.claude/CLAUDE.md` | this project |
| `<workspace>/.bravebot/skills/<name>/SKILL.md` | this project |
| `<workspace>/.claude/skills/<name>/SKILL.md` | this project |
| `<workspace>/.agents/skills/<name>/SKILL.md` | this project |
| `<workspace>/.bravebot/agents/<name>.md` | this project |
| `<workspace>/<directory>/AGENTS.md` | this project, once a turn has worked in that directory |
| `<workspace>/<directory>/.bravebot/skills/<name>/SKILL.md` | this project, once a turn has worked in that directory |

And no others. There is **no search of parent directories**. A rule
that walked upwards would pick up instructions from whatever happened to be above a project on this
machine, which is a different set of instructions on the next machine. A file at any other path is an
ordinary file, read only when something asks for it by name, or when the source points at it.

**A project's skills are read from three directories, at the project root only.**
`.agents/skills`, then `.claude/skills`, then `.bravebot/skills`, so a skill in `.bravebot/skills`
wins a name clash with either of the others. Skills you wrote for another agent work here without
being copied or symlinked. Your own `~/.claude/skills` and `~/.agents/skills` are not read, because
`~/.bravebot` is the one user directory tried.

**A directory the session has worked in adds its own.** Once the planner has read a file in
`packages/api/`, or written one, the `AGENTS.md` in `packages/api/` and in each directory above it
down from the project root follows the project's, the deeper last so it wins, and so do the skills
under `.bravebot/skills` there; a skill of the same name replaces the project's. The next turn
picks it up, not the one that read the file. Each goes through the same trust check as the project's
own file, and one that fails it is left out with a notice naming the file. A pointer or an `@path`
import inside a nested file is not followed. Only `.bravebot/skills` is read below the root, not
`.claude/skills` or `.agents/skills`. A directory is counted when a file is read or written in it,
not when it is only listed or searched, and a session remembers up to 64 of them until `/cd` moves
the project.

**The project's file is looked for under three names, and the first that exists is the one.** Not all
three: a repository holding two of them holds one set of instructions under two names, and reading
both would say everything twice in a system prompt that goes out afresh every request.

`~/.bravebot` is `.bravebot` inside the home directory the environment gives, and there is **no
fallback**. When there is no home, or the name is empty, everything kept there is absent. Nothing is
guessed and no other location is tried. Daemons and containers run without a home, and everything
kept there is optional, so absence is a case to do without rather than a reason to refuse to start.

`HOME` is what names it. Stock Windows sets no `HOME`, so there `USERPROFILE` is read after it and
the first of the two that is set wins: that is the platform stating where the profile is, the same
thing `HOME` does on Unix, rather than a guess past an answer. See
[Configuration](configuration.md) for the rest of what lives in that directory.

Note the two roots are spelled differently. Your own skills and delegate definitions sit directly
beneath `~/.bravebot`; a project's sit under a dotted `.bravebot` directory rather than at the root
where `AGENTS.md` sits. A skill is a directory because it has other material to keep beside its
instructions; a delegate definition is one file, so `agents/` is flat.

### A file that only names another is followed

An instructions file under 500 bytes that names another file is followed to that file, and that file
is what reaches the planner. Repositories supporting several agents often keep one real document and
point the other names at it. An `AGENTS.md` holding nothing but

```markdown
Refer to canonical agent instructions in `.claude/CLAUDE.md`.
```

loads `.claude/CLAUDE.md`.

**Length is the whole test.** Anything past 500 bytes is a document that happens to cite other files,
so it is read as itself and its citations are left alone. Following the first name in a real
conventions file would swap your instructions for whatever they mentioned in passing.

Once, not twice: what the named file names in turn is not followed. The pointer is opened by the
same route as any other path, so [confinement](../security/security.md#confinement) and the trust map
decide whether it may be read at all, and a pointer naming something outside the workspace is refused
there.

A file a `deny` [rule](configuration.md#permissions) covers is left out of the turn, whether the
project's instructions file is that file, points at it, or is a link to it. You are told it was left
out.

### Splitting a long file with `@path`

In the project's instructions file, a token that begins with `@` and names a markdown file is
replaced by that file's text:

```markdown
Follow the style rules in @docs/style.md before writing any code.
```

The path is relative to the file the token is written in. Imports inside imported files are
expanded too, to four hops. A token inside backticks or a fenced code block is left alone, and so is
a token naming a file that does not exist.

Each import is read by the same route as a pointer, so confinement, the trust map and `deny` rules
decide. An import that is outside the project, denied, untrusted, part of a cycle or nested too
deeply stays as written, and you are told which file and why. `~/.bravebot/AGENTS.md` is not
expanded.

## Words from the command line

[`--append-system-prompt`](../reference/cli.md#--system-prompt-prompt-and---append-system-prompt-prompt)
adds its words as a ninth source that is not a file. It is read last, after the project's
`AGENTS.md`, so where the two disagree the words win. It is held by the running process, so it is in
every turn of the run and in the stored session of none: resuming without the flag runs without it.

`--system-prompt` replaces the opening of the system prompt and nothing after it. The instructions
for reading a tool's output, the facts about where you are working, the mode and the goal are still
sent, because other guarantees rest on them. Neither flag is read through the trust gate, since
there is no file behind them. `--append-system-prompt "$(cat x)"` therefore gives the file's bytes
your authority.

## What wins

The project has the last word. Sources are read least specific first: your own directory before the
project, **both** `AGENTS.md` files reaching the planner in that order, and a project skill replacing
a global one of the same name. It is the same "most specific wins" rule the trust map uses for paths.
Shadowing by name rather than merging is what lets a project override one skill without restating the
rest.

A directory opened with `/add-dir` during a session adds **no** standing instructions and no skills,
whatever it contains. Opening a directory to read one file out of it should not change how every
later turn behaves.

## Where they end up

What is resolved goes into the **system prompt**, never into the conversation. A session running many
turns carries one copy of its instructions however long it runs, rather than a copy per turn crowding
out the task.

Sources are resolved afresh every turn, so editing `AGENTS.md` mid-session takes effect on the next
thing you send. A source that is not there is not an error. No `AGENTS.md`, no skills directory, no
user directory at all: each is the ordinary case and offers nothing.

## Where you are working

The system prompt also states a handful of facts about your machine, so the planner does not have to
run a command to learn them:

| Line | Value |
|---|---|
| Working directory | The absolute path of the workspace root |
| Is a git repository | Whether this tree or a directory above it holds a `.git` |
| GitHub CLI (gh) on PATH | Whether `gh` is installed, looked up on `$PATH` the way a `run` would |
| Platform | `macos`, `linux`, or whatever this build runs on |
| OS version | The kernel release string on Unix, as `uname` reports it. On Windows, the three numbers a build is named by |
| Shell | `$SHELL`, or `/bin/sh` when that is unset or empty |
| Today's date | The current UTC date, as `YYYY-MM-DD` |
| Scratch directory | The directory this session has to itself, on the sessions that have one |

:::caution[These lines are sent to the model with every request]
The working directory is an absolute path, so on most machines it contains your username, and the
scratch path may too. The OS version names your build. All of it is part of every request this
session sends, including the first one, and there is no setting that withholds any of it.

Nothing else about your machine is added. No environment variables beyond `$SHELL`, no hostname, no
username on its own, no file contents, no directory listing.
:::

They are labelled as facts about the machine rather than as instructions. One of them asks something
of the planner as well, and only because a probe found what the request names: the scratch directory
is stated with what to put in it. Nothing is asked on the account of the rest. What an installed `gh`
is for is said too, just outside this block, and [below](#the-github-cli) is what it says.

The date is stated because a model's sense of it comes from its training and is wrong by however long
ago that was. The rest is stated because discovering any of it otherwise costs a `run`, and a run
costs two approvals rather than one: you approve the command, then its output comes back quarantined
and the planner has to ask to be shown it. A planner that does not know its own working directory
reaches for `pwd` and spends that whole exchange on a value already on your screen at every run
prompt.

This block is composed afresh every turn like the sources are, so [`/cd`](../reference/commands.md)
is followed and the next turn states where the session went.

### The scratch directory

The last line names a directory this session has to itself, created as the session opens in the
system temporary directory rather than anywhere in your project. It is where a file the work needs on
disk but nobody is asking to keep belongs: output to grep through, an archive to look inside. Written
into the project instead, that file is one a build, a test run, a `git add -A` and a reviewer each
have to deal with, and one somebody has to remember to delete.

It is removed when the session ends, with everything written in it, and a session that carries on
from another is given its own. A program started by [`run`](../reference/tools.md#run) reads the same
path from `BRAVEBOT_SCRATCH_DIR`. A session that could not be given a directory says so and runs
without one, and the line is then absent rather than naming somewhere that is not there.

It is stated because no `run` could discover it: nothing names that directory but this program, and a
planner never told of it puts an intermediate file in the project instead. Reaching it grants nothing.
A file there is read and written by its absolute path, but it prompts exactly when a file in the
workspace would, and neither `/add-dir` nor `/cd` will take it. See
[Trusted directories](../security/trust.md).

### The GitHub CLI

Where `gh` is on your `$PATH`, the block says so and adds the commands that read a GitHub URL:
`gh pr view`, `gh pr diff` and `gh issue view`, through [`run`](../reference/tools.md#run), with
`--comments` for what a review said. Paste the URL of a pull request and that is one approval for
exactly the diff, where [`fetch_url`](../reference/tools.md#fetch_url) fetches the page around it.
Both come back quarantined, so either way a processor reads the answer out; the difference is that
one of them carried the page to say it. The `.diff` address of a pull request is worse than that: it
redirects to another host, and an approval for `github.com` does not carry to wherever a fetch went
next, so it is refused unless a
[rule](../customize/configuration.md#permissions) names that host too.

Whether `gh` is installed is all this looks at. Whether you have logged in with it cannot be told
without a request, and none is made before your first turn, so the paragraph names `fetch_url` as
what to fall back to when `gh` fails for that reason.

A [delegate](../reference/tools.md#spawn_agent) is told the fact and not this paragraph. It has no
`fetch_url` at all, and `run` only if its kind reaches programs, so a sentence choosing between the
two would send it after a tool it was not given.

## Trust

`~/.bravebot` is trusted **by provenance**: it is your own directory, on the same footing as the
configuration that picks the model and the endpoint. Putting a file there is the grant, and an empty
directory offers nothing.

A project's own `AGENTS.md` is different. It is workspace content, so it is read through the
[trust map](../security/trust.md) like any other file. It loads when you vouched for the
directory and is left out when you did not:

```
AGENTS.md was not loaded: this directory is not trusted
2 skills in .bravebot/skills were not loaded: this directory is not trusted
1 delegate definition in .bravebot/agents was not loaded: this directory is not trusted
```

A source that fails the trusted-content gate is **dropped entirely, never quarantined**. A reference
to an instruction is no use to anyone: an instruction is either followed or absent, and one from a
directory nobody vouched for has to be absent.

What was skipped is counted, never named. A directory in an untrusted project can be given a name
that reads like an instruction, and that name would otherwise be on your screen as though the agent
had written it.

The notice is said when it is learned, before the first request goes out, rather than when the turn
ends. A turn that fails or is cancelled has already told you what it was working without.

The [environment lines](#where-you-are-working) do not go through this gate, and cannot be refused by
it. There is no file behind any of them: the working directory is where you pointed the session, the
scratch directory is one this program created empty, and the rest comes from the kernel and this
process's own environment, which is the same provenance a command you typed rests on. Nothing read out of the workspace may join that block, which
is the whole reason it can skip the gate.

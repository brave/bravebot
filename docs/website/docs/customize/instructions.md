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

## The four sources

| File | Applies to |
|---|---|
| `~/.bravebot/AGENTS.md` | every project |
| `~/.bravebot/skills/<name>/SKILL.md` | every project |
| `<workspace>/AGENTS.md`, else `CLAUDE.md`, else `.claude/CLAUDE.md` | this project |
| `<workspace>/.bravebot/skills/<name>/SKILL.md` | this project |

And no others. There is **no search of parent directories** and no nested instructions file. A rule
that walked upwards would pick up instructions from whatever happened to be above a project on this
machine, which is a different set of instructions on the next machine. A file at any other path is an
ordinary file, read only when something asks for it by name, or when the source points at it.

**The project's file is looked for under three names, and the first that exists is the one.** Not all
three: a repository holding two of them holds one set of instructions under two names, and reading
both would say everything twice in a system prompt that goes out afresh every request.

`~/.bravebot` is `.bravebot` inside the home directory the environment gives, and there is **no
fallback**. When there is no home, or the name is empty, everything kept there is absent. Nothing is
guessed and no other location is tried. Daemons and containers run without a home, and everything
kept there is optional, so absence is a case to do without rather than a reason to refuse to start.

Note the two roots are spelled differently. Your own skills sit directly beneath `~/.bravebot`; a
project's sit under a dotted `.bravebot` directory rather than at the root where `AGENTS.md` sits.

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

The system prompt also states six facts about your machine, so the planner does not have to run a
command to learn them:

| Line | Value |
|---|---|
| Working directory | The absolute path of the workspace root |
| Is a git repository | Whether this tree or a directory above it holds a `.git` |
| Platform | `macos`, `linux`, or whatever this build runs on |
| OS version | The kernel release string, as `uname` reports it. Unix only |
| Shell | `$SHELL`, or `/bin/sh` when that is unset or empty |
| Today's date | The current UTC date, as `YYYY-MM-DD` |

:::caution[These six lines are sent to the model with every request]
The working directory is an absolute path, so on most machines it contains your username. The kernel
release string names your OS build. Both are part of every request this session sends, including the
first one, and there is no setting that withholds them.

Nothing else about your machine is added. No environment variables beyond `$SHELL`, no hostname, no
username on its own, no file contents, no directory listing.
:::

They are labelled as facts about the machine rather than as instructions, and nothing is asked of the
planner on their account.

The date is stated because a model's sense of it comes from its training and is wrong by however long
ago that was. The rest is stated because discovering any of it otherwise costs a `run`, and a run
costs two approvals rather than one: you approve the command, then its output comes back quarantined
and the planner has to ask to be shown it. A planner that does not know its own working directory
reaches for `pwd` and spends that whole exchange on a value already on your screen at every run
prompt.

This block is composed afresh every turn like the sources are, so [`/cd`](../reference/commands.md)
is followed and the next turn states where the session went.

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
```

A source that fails the trusted-content gate is **dropped entirely, never quarantined**. A reference
to an instruction is no use to anyone: an instruction is either followed or absent, and one from a
directory nobody vouched for has to be absent.

What was skipped is counted, never named. A directory in an untrusted project can be given a name
that reads like an instruction, and that name would otherwise be on your screen as though the agent
had written it.

The notice is said when it is learned, before the first request goes out, rather than when the turn
ends. A turn that fails or is cancelled has already told you what it was working without.

The six [environment lines](#where-you-are-working) do not go through this gate, and cannot be
refused by it. There is no file behind any of them: the working directory is where you pointed the
session, and the rest comes from the kernel and this process's own environment, which is the same
provenance a command you typed rests on. Nothing read out of the workspace may join that block, which
is the whole reason it can skip the gate.

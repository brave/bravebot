---
sidebar_position: 3
title: Delegate definitions
description: Define a kind of delegate in a file, so the standing part of what it is told has somewhere to live.
---

# Delegate definitions

A delegate is a second agent with its own context: it does a sub-task and hands back one report, so
the reading it did stays with it. Three kinds come with bravebot (`reader`, `checker` and `worker`),
and a definition is one more, written down rather than compiled in.

Put one in `~/.bravebot/agents/<name>.md` and it is available in every project; put it in
`<workspace>/.bravebot/agents/<name>.md` and it belongs to that project.

```markdown
---
name: rule-reviewer
description: Checks a diff against the rule in docs/development/reviewing-for-the-rule.md. Use before asking anyone to review a label change.
kind: reader
model: haiku
tools: read_file, list_files
---

Read the diff and the four shapes a violation takes. Report the shape and the file, or say none.
```

The planner can then ask for a `rule-reviewer` the way it asks for a `reader`, and what that
delegate is told about itself is the body of your file. Without one, the standing part of the
instruction, the part that is the same every run, has nowhere to live: it has to be written into
the task afresh every time.

You can also run one yourself, without the planner choosing it:
[`/agent rule-reviewer check this branch`](../reference/commands.md#agent-name-task). That is a turn
of your own under the definition's body, model and narrowing rather than a delegate, so it can still
ask you questions and what it reads stays in the conversation.

## The keys

| Key | Required | Meaning |
|---|---|---|
| `name` | yes | what the planner names to select it |
| `description` | yes | what the planner decides from, so say *when* to use it rather than what it does |
| `kind` | yes | `reader`, `checker` or `worker` |
| `model` | no | the model this delegate runs on (`haiku`, `sonnet`, `opus`, or an explicit model identifier); absent or `inherit` means the spawning turn's |
| `tools` | no | fewer tools than the kind's; absent means the kind's own |
| `skills` | no | the [skills](skills.md) this delegate is offered, out of the ones the turn found; absent means all of them, and an empty line none |
| `mcpServers` | no | the [MCP servers](mcp-servers.md) a `worker` calls, by alias, out of the ones the turn may; absent means all of them unless `tools` is written, and an empty line none |
| `rounds` | no | how many rounds of tools this delegate may take before it has to answer, up to its kind's ceiling; absent or empty means the kind's own |
| `memory` | no | `project` or `local` keeps [a memory](#memory), a file its runs read and write themselves; absent or empty means none |
| body | no | the standing instruction |

Keys other than these are ignored rather than refused, so a definition written for another agent
loads here too. That includes another agent's bound, `maxTurns` or `steps`: a definition ported
with one runs at its kind's own limit until you add a `rounds` line.

**`model` chooses what the delegate runs on.** A tier alias (`haiku`, `sonnet`, `opus`) or an
explicit model identifier resolves through configuration the way any named model does. This lets a
high-volume delegate like a build checker run on a cheap model rather than spending the turn's model
on reading thousands of lines of logs, while a refactoring worker can select a stronger model. Where
`model` is omitted, the delegate inherits the model of the turn that spawned it. Where the named
model needs a sign-in you have not made, the delegate does not run and says so, rather than falling
back to the turn's model, so an intended cost control cannot be silently bypassed. A delegate
answered by a different model than the one named says so, so a misspelt name is not silent.

**`kind` picks what the delegate may do, and your file never describes it.** A `reader` reads,
lists and searches; a `checker` also runs programs and asks a language server; a `worker` also
writes files and calls the tools of the [MCP servers](mcp-servers.md) the turn that spawned it may.
Each still asks you before every write, every command it runs and every call to a server's tool:
delegating saves the agent context, never an approval.

**`tools` can only take things away.** It names a subset of what the kind already reaches, and a
tool the kind does not reach is one the delegate is started without. There is no spelling of it
that adds a capability, including `*`, which is read as a tool name matching nothing rather than as
"all of them". That is the deliberate difference from tools where the same key *is* the permission
list: a file in a repository you cloned cannot hand an agent a shell it was never granted.

**`mcpServers` chooses which servers a `worker` calls.** Name them by the alias you gave each in
`~/.bravebot/mcp.json`, on one line or as a list. The delegate calls those, where the turn that
spawned it may, and no others. A server's tools are not bravebot's, so no `tools` line names one:
a `worker` whose definition has a `tools` line and no `mcpServers` line calls no server, and one
with neither line calls every server the turn may. A `reader` or a `checker` calls none whatever it
names. A name no server goes by picks nothing, and the turn says so:

```
~/.bravebot/agents/forecaster.md names an MCP server this session did not reach, so its delegate runs without it: wether
```

Another agent's definition may describe a server inline under the same key. That starts nothing,
since servers are declared in `~/.bravebot/mcp.json` alone: the definition calls no server, and the
turn says so without repeating the entry, which may hold a secret.

That includes `spawn_agent`, the tool a delegate starts delegates of its own with. A definition
that names its tools and leaves it out, like `rule-reviewer` above, does its work itself and hands
none of it on. Name `spawn_agent` in the list if it should be able to.

Name bravebot's own tools here. A definition ported from another agent usually names that agent's
(`Read`, `Grep`, `Bash(...)`), and none of those is a tool here, so the delegate starts with no
tools at all. It says which names it dropped, so the fix is to rename them.

**`skills` chooses which of your skills the delegate is told about.** A delegate given one job needs
the skill for that job, not the whole list the turn found. Name them the way `tools` names tools, on
one line or as a list. The delegate is listed those and can load no others. A name only picks
from skills the turn already found, so it cannot load a skill from a directory you did not vouch
for. A name no skill goes by picks nothing, and the turn says so:

```
~/.bravebot/agents/rule-reviewer.md names a skill this session did not find, so its delegate is offered without it: rule-reveiw
```

**`rounds` sets how long the delegate may work.** Nobody is watching a delegate, so each kind stops
one after a set number of rounds and makes it answer with what it has: 60 for a `reader`, 80 for a
`checker` and 120 for a `worker`. A definition written for a long job, a staged refactor say, can
ask for more, up to its kind's ceiling: 120 for a `reader`, 160 for a `checker` and 200 for a
`worker`, which is the bound on a one-shot run nobody is watching. Asking for more than that gives
the delegate the ceiling, and the turn says so:

```
~/.bravebot/agents/migrator.md asks for 500 rounds, more than the 200 a worker may make, so its delegate is given 200
```

The value is a whole number above zero. An empty line is the same as none. Anything else, `0` or
`lots` or `1.5`, means the file does not load, and the turn names it. The planner cannot set a
bound when it starts a delegate: only the definition can.

`rounds` bounds the definition only when the planner starts it as a delegate. A turn you run
yourself with `/agent` is yours, and carries no bound, as any turn you are watching does.

A name may not open with `-`, may not contain a colon, which stays reserved for naming things
inside a namespace, and may not be `reader`, `checker` or `worker`: those belong to the kinds, so
that `reader` means the same thing in every project.

## Memory

A definition with `memory: project` or `memory: local` keeps notes from one run to the next, in one
file named after it: `.bravebot/memory/<name>.md` in the directory the session is working in. Each
run under it, a delegate the planner started or a turn you ran with `/agent`, is told where that
file is and reads it itself. Nothing puts the notes in its prompt.

```markdown
---
name: release-notes
description: Drafts the release notes for this branch. Use when asked what changed since the last tag.
kind: worker
memory: project
---

Keep the conventions the maintainers asked for in your memory, and follow them.
```

The run keeps the file up to date with the tools its kind already has, on the same terms as any
other file it writes. `memory` adds no tool, so a `reader` or a `checker` can read a memory
something else wrote and cannot change it.

`project` and `local` are the same file here: whether it is committed is up to you and your ignore
rules, and bravebot writes none. Any other value, `user` included, loads the definition keeping no
memory, and the turn says so:

```
~/.bravebot/agents/release-notes.md keeps no memory: its memory line says user, and only project and local keep one
```

A definition keeping a memory needs a name of lowercase letters and digits in runs joined by single
hyphens, at most 64 characters, since the file is named after it. A session in your home directory
keeps none, because the file would be inside `~/.bravebot`.

**A memory is read on the [trust map](../security/trust.md)'s terms.** In a directory you did not
vouch for, the run is told its memory is withheld, and reading it is a quarantined read like any
other. Where a write leaves the file untrusted, because it passed on something from a page or a file
nobody vouched for, the path is recorded in `~/.bravebot/untrusted`, and later sessions withhold the
memory too. Saying yes when a read of it is quarantined, naming it with `@`, dropping or attaching
it, or a later write that leaves it trusted, takes it out of that record. A write that cannot be
recorded there is refused.

## Which one wins

Your own directory is read first and the project second, so a project definition of the same name
replaces yours, which is the same "most specific wins" the trust map uses for paths. Two files in one
directory resolve by file name, so which is live is the same on every machine.

**It wins about what the definition is for, and never about what it may do.** The project's file
takes over the description, the body, the model, the skills, the rounds and the memory, the rounds
held to the ceiling of the kind it is loaded as. The kind is the narrower of the two, and the `tools` and `mcpServers`
lists are met name by name, so a checkout you vouched for cannot turn a `reader` you wrote into a
`worker`, and cannot hand back a tool or a server your own lines took away.
Vouching for a project is a decision about the project, not one about a name you had already
defined. The same holds for two files of one name in one directory, since which of those is live
is only a matter of file name. Whatever the later file asked for and did not get is said, with the
one that cut it down beside it:

```
.bravebot/agents/rule-reviewer.md does not widen ~/.bravebot/agents/rule-reviewer.md: it names kind worker and is loaded as a reader
```

## Trust

The same rule skills follow, for the same reason and with more riding on it:
`~/.bravebot/agents` is trusted because it is your own directory. A project's `.bravebot/agents` is
workspace content, read through the [trust map](../security/trust.md), so it loads when you vouched
for the directory and is left out when you did not:

```
2 delegate definitions in .bravebot/agents were not loaded: this directory is not trusted
```

Counted and never named, because a file in a project nobody vouched for can be given a name that
reads like an instruction. A definition that fails the gate is dropped entirely rather than
quarantined: an instruction is either followed or absent.

**A definition's body is the whole of what a second agent is told it is.** A skill's body is
guidance a turn may follow; this is more than that. Read one before you install it, the way you
would read the configuration file that picks your model.

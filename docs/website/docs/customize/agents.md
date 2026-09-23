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
tools: read_file, list_files
---

Read the diff and the four shapes a violation takes. Report the shape and the file, or say none.
```

The planner can then ask for a `rule-reviewer` the way it asks for a `reader`, and what that
delegate is told about itself is the body of your file. Without one, the standing part of the
instruction, the part that is the same every run, has nowhere to live: it has to be written into
the task afresh every time.

## The keys

| Key | Required | Meaning |
|---|---|---|
| `name` | yes | what the planner names to select it |
| `description` | yes | what the planner decides from, so say *when* to use it rather than what it does |
| `kind` | yes | `reader`, `checker` or `worker` |
| `tools` | no | fewer tools than the kind's; absent means the kind's own |
| body | no | the standing instruction |

Keys other than these are ignored rather than refused, so a definition written for another agent
loads here too.

**`kind` picks what the delegate may do, and your file never describes it.** A `reader` reads,
lists and searches; a `checker` also runs programs; a `worker` also writes files. Each still asks
you before every write and every command it runs: delegating saves the agent context, never an
approval.

**`tools` can only take things away.** It names a subset of what the kind already reaches, and a
tool the kind does not reach is one the delegate is started without. There is no spelling of it
that adds a capability, including `*`, which is read as a tool name matching nothing rather than as
"all of them". That is the deliberate difference from tools where the same key *is* the permission
list: a file in a repository you cloned cannot hand an agent a shell it was never granted.

Name bravebot's own tools here. A definition ported from another agent usually names that agent's
(`Read`, `Grep`, `Bash(...)`), and none of those is a tool here, so the delegate starts with no
tools at all. It says which names it dropped, so the fix is to rename them.

A name may not open with `-`, may not contain a colon, which stays reserved for naming things
inside a namespace, and may not be `reader`, `checker` or `worker`: those belong to the kinds, so
that `reader` means the same thing in every project.

## Which one wins

Your own directory is read first and the project second, so a project definition of the same name
replaces yours, which is the same "most specific wins" the trust map uses for paths. Two files in one
directory resolve by file name, so which is live is the same on every machine.

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

---
sidebar_position: 2
title: Skills
description: Package a repeatable piece of know-how as a SKILL.md the planner loads when the task calls for it.
---

# Skills

A skill is instructions you wrote for a kind of task. Put one in
`~/.bravebot/skills/<name>/SKILL.md` and it is available in every project; put it in
`<workspace>/.bravebot/skills/<name>/SKILL.md` and it belongs to that project.

```markdown
---
name: commit-style
description: How commit messages are written here. Use before writing one.
---

Write the subject in the imperative. Explain why in the body, never what.
```

## The file

One `SKILL.md`, with `name` and `description` in front matter. Both keys are required, and a file
missing either is skipped with a note saying so. A key nothing here reads stops nothing, so a skill
written for another agent works here; `bravebot doctor` lists such keys, so a line you expected to
do something and that does nothing is somewhere you can find it. A file with no front matter is not
a skill.

A value may wrap over the lines indented beneath it, however the file spells the wrap: folded or
literal with `>` or `|`, quoted and carried over, or plain text continued. A folded value is
joined with spaces; a literal one keeps the newlines it asked for.

## Running a skill on its own model

Two more keys, both optional:

```yaml
name: release-notes
description: Turn a range of commits into release notes. Use when cutting a release.
model: haiku
effort: low
```

`model` is resolved the way the `model` settings key is, `opus`, `sonnet` and `haiku` included.
`effort` is one of `low`, `medium`, `high`, `xhigh` and `max`. Leave either out and the session's own
choice stands.

Both take effect from the moment the skill is loaded, for the rest of that turn, and **they replace
a model you chose for the session**, including one picked with `/model`. The session says so when
it happens, so you see when a skill moves a turn onto a dearer model.

A skill does not replace a model that a definition named. When you address a definition that names
a model, or a delegate's definition names one, a skill loaded in that turn keeps the definition's
model and the session says so. The skill's `effort` still applies.

A value that cannot be used is reported and the skill still loads, on whatever the session was
already running: a level spelled some other way, or a model needing a sign-in this machine has not
made.

## Only the name and description reach the prompt

The body waits until the planner asks for it with `load_skill`, so a directory of long skills does
not crowd out the task. The **description is what the planner decides from**. Write it to say
*when* to use the skill rather than what it contains:

```yaml
description: How commit messages are written here. Use before writing one.
```

not

```yaml
description: Notes about commits.
```

## Naming a skill after a slash

Type `/` and the skills this session has are listed beneath the commands, each with its description
and where it was found: `(project)`, `(user)` or `(built-in)`. After other words, as in
`this is /release-no`, the list holds skills alone. Tab takes the highlighted one and writes
`/release-notes ` into the line.

The line is still a prompt, sent as you typed it. The planner is told that a prompt naming a skill as
`/name` is you asking for it, and it loads the skill with `load_skill` the way it loads any other, so
a skill you named is loaded and recorded exactly as one it picked for itself.

The list is the set the planner is offered and nothing more: a skill in a project you have not trusted
is never listed, just as it is never shown to the planner. No skill is a slash command, so one whose
name a command already claims, such as `loop`, is not listed, and nothing is completed after a
command's own word or in [shell mode](../using/shell-mode.md).

## Loading

`load_skill` takes a name, and the name selects from the set found before the turn started. It is
never a path: a name holding `../` or an absolute path matches nothing and the call is refused, since
there is no lookup for it to reach. A name merely close to a real one is refused too rather than
guessed at, because guessing would load instructions nobody asked for.

## Trust

| Source | Trusted because |
|---|---|
| `~/.bravebot/skills/<name>/SKILL.md` | it is your own directory: provenance, never the trust map |
| `<workspace>/.bravebot/skills/<name>/SKILL.md` | you vouched for the directory |

A workspace `.bravebot/skills` is checked for trust **before it is enumerated at all**, because a
directory name is content too. A source that fails the gate is dropped entirely, and what was skipped
is counted rather than named. See [Instructions](instructions.md#trust).

A project skill replaces a global one of the same name.

A few skills are written into bravebot itself rather than found on disk. They pass no trust gate and
are offered in every session, including one in a directory nobody trusts, because there is no file and
no directory behind them. They are the least specific source, so a skill of your own with the same
name shadows one. The skill that tells a
[`/loop`](../reference/commands.md#loop-interval-prompt) tick how to pace itself is one.

:::caution
A skill downloaded into `~/.bravebot/skills` is trusted exactly as far as a config file you pasted
is. The name, the description and the body all go to the model as instructions, and nothing
downstream second-guesses it, because everything downstream is built to trust what you vouched for.
Read one before installing it.
:::

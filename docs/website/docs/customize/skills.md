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
missing either is skipped with a note saying so. Other keys are ignored, so a skill written for
another agent works here. A file with no front matter is not a skill.

A value may wrap over the lines indented beneath it, however the file spells the wrap: folded or
literal with `>` or `|`, quoted and carried over, or plain text continued. A folded value is
joined with spaces; a literal one keeps the newlines it asked for.

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

## Skills are not slash commands

`/commit-style` is a prompt like any other sentence. Other agents let you type a skill's name after a
slash. This one does not.

A skill is advertised to the planner by name and description, and its body is fetched by the planner
asking for it. Nothing in the input box knows skills exist. The two surfaces stay apart deliberately:
a slash command is a thing *you* decide, and loading a skill is a thing the *planner* decides.

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

---
sidebar_position: 2.5
title: Consulting a bot
description: Tagging a bot with @name to put one question or one task to it, and getting its reply back in the same conversation.
---

# Consulting a bot

Write `@name` in a line, where `name` is a bot, and that line goes to the bot instead of the agent
you are talking to. The bot answers once, its reply is drawn under its own name, and the agent you
were talking to is told what it said. That is a **consult**.

```
> @rule-reviewer is the label change in this branch safe?
```

A **bot** is a [definition](../customize/agents.md) whose name is a slug: lowercase letters and
digits in runs joined by single hyphens, such as `rule-reviewer`. The bots you make in the desktop
app are definitions of that kind. The three built-in kinds, `reader`, `checker` and `worker`, are
not bots. The agent whose conversation you tag a bot in is the **host**: the session's own agent, or
the definition the session's turns are addressed to.

A consult is your own run, as a [`/agent`](../reference/commands.md#agent-name-task) turn is. It can
ask you questions and every write it makes is shown to you. Unlike a `/agent` turn, it works from
what it was sent and not from the whole conversation, and only its reply comes back. What it read,
ran and narrated stays with it, as a delegate's does.

## Typing `@`

The `@` list offers the bots first, then the files and the past sessions. A bot's row is marked as a
bot and says the model it runs on and the most it may spend:

```
rule-reviewer   bot · haiku · up to 60 rounds
```

Taking the row writes `@rule-reviewer ` into the line and runs nothing. The consult starts only when
you send the line.

A bot wins over a file of the same name. To name a file that shares its name with a bot, write
`@./name`, which is always a file. Names with a dot, a slash, a space or a colon, such as
`@README.md`, `@src/lib.rs` and `@session:<id>`, are never bots. Which one a word meant is decided
when the line is sent and written into the session's record, so a bot made later does not change
what an earlier line meant.

Only a line you send starts a consult. A `@name` in a reply, a brief, a file, a command's output, a
tool's result, a `/loop` tick or a watch's fire is text.

## What the bot is given

The agent you are talking to, the **host**, drafts a **brief** from its conversation for the bot,
since it is the one that knows what "this" refers to. This is the same in a plain session and in one
whose turns address a definition. The brief opens with your line as you wrote it and the host's
draft follows.

A brief drafted only from content the host could have held anyway, and holding nothing private, is
sent at once: sending your line was the go-ahead. Any other brief, one you promoted or one drafted
from private content, is put in the input box, where you can read and edit all of it, under a note
naming the bot and its bound:

```
the brief for rule-reviewer (bot · haiku · up to 60 rounds) is in the box. Enter sends it to
rule-reviewer, or Esc puts your line back
```

Enter sends the brief and starts the consult. Esc starts nothing and puts your line back in the box.
The files sent with a brief are the ones your own line named, not ones the host's draft mentions.

A file the host holds as quarantined goes into the brief as a reference, never as its contents. The
bot is handed the same reference, which it may pass to a processor and may not read as trusted.

### When the host has read something untrusted

A host that has read untrusted content, such as a web page or a file you have not trusted, may have
been steered by it, so its draft is quarantined. Before the brief reaches the box, the second opinion
every [promotion prompt](../security/vetting.md) carries is taken over the whole brief, and you are
shown the brief with what the check found. Saying yes promotes that one brief, once, for that one
bot, and puts it in the box. Saying no sends nothing:

```
the brief for rule-reviewer was not sent, and your line is back in the box
```

Auto-vetting never answers this prompt for you.

A brief the host drafted after it was shown private content is private, so the consult starts out
holding private content and nothing it writes leaves as public.

## The model and the bound

A bot runs on the model its definition names, and on the session's model where it names none. The
note and the bot's row name the model the bot runs on and the bound it runs under: its rounds,
and its token limit where its definition sets a `budget`. A bot whose model needs a sign-in this
machine has not made is refused with the model it asked for, and is not consulted. The host's model
is never put in its place.

## What the consult may do

A consult holds what the host's turn holds, narrowed by the bot's kind, tools and servers, the same
way a `/agent` turn is narrowed. A bot of a wider kind than the host gets only the host's reach. It
works in the host's working directory.

A bot that keeps a [memory](../customize/agents.md#memory) keeps it there, with the tools it holds,
and the write is shown to you as any other is. A `reader` writes nothing, and nor does a `worker`
consulted from a host that may not write, so neither changes its memory.

A consult may start the delegates its definition names. It may not consult another bot or propose a
consult: a `@name` in what it writes is text.

## What comes back

The reply is drawn under the bot's name, as a speaker of its own. The session's record keeps which
bot answered, so a resumed session and an exported transcript show the same speaker.

On its next turn, the host holds your line, then a note in bravebot's own words naming the bot, its
definition and the model that answered, and then the reply. The note is never taken from the reply,
so a reply cannot claim to be from another bot. You can then write "fix the first thing it raised"
and the host knows what you mean.

The reply reaches the host labelled by what the consult itself read. A consult that read only
trusted content hands the host its words. One that read something untrusted hands the host a
reference to them, which you can promote through the ordinary promotion prompt.

A consult that fails or is stopped leaves the host as it was.

## A consult lasts one reply

The consult ends when it replies, and your next line goes to the host. To follow up, tag the bot
again. Nothing in the input box changes while a consult runs or after it.

## Several bots in one line

A line that tags several bots consults each of them in turn, in the order the line names them:

```
> @rule-reviewer @docs-checker look over this branch
```

Each reply is drawn and handed to the host as it arrives. Each bot gets a brief of its own, drafted
once the bot before it has replied and offered one at a time. A stop ends the consult that is running and the ones after it.

## When the host proposes a consult

A host that thinks another bot should be asked can propose a consult: it names the bot and writes a
brief. The proposal is shown to you as a typed tag's brief is, with the same check where the host has
read something untrusted and the same model and bound. Sending it runs the consult, and the reply is
what the host's proposal returns. Declining it tells the host it was declined, and its turn goes on.
A name that is not a bot is refused with the names that are.

Nothing runs until you send the proposal. Where nobody can be asked, such as in bypassing mode or a
one-shot run, the host is not offered the proposal at all.

## Keeping a definition out of the list

A definition written for a planner to spawn is noise in the `@` list. Add `consult: off` to its
front matter to stop it being a bot:

```markdown
---
name: build-log-reader
description: Reads a failing build's log and reports the first error.
kind: reader
consult: off
---
```

It is then not offered in the `@` list, `@build-log-reader` names a file, and a proposal naming it is
refused. It can still be addressed with `/agent` and spawned by a planner exactly as before. Any
other value, or no `consult` key, leaves it a bot. Where a later definition has the same name, its
value replaces the earlier one.

In the desktop app, the form for editing a bot shows this as a switch. Turning it off writes
`consult: off` into the definition and turning it on removes the line. Every other line is kept.

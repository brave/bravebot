---
sidebar_position: 2
title: Quickstart
description: Run your first task with Brave Bot and understand the questions it asks.
---

# Quickstart

Install Brave Bot with npm, then run `bravebot` in a project directory to start a session.

```sh
npm install -g @brave/bravebot
cd your-project
bravebot
```

## Install

[Install](install.md) covers the npm package and the install scripts for macOS, Linux and Windows,
staying current, uninstalling, checking a download by hand and checking the configuration.

## The first question: do you trust this directory?

Brave Bot asks whether you trust the working directory before anything else.

- **Trust it** and ordinary work proceeds: files are read as trusted, and edits are not shown to
  you for every path in the tree.
- **Decline** and nothing is trusted. The session still works. Every write is shown to you
  first, and files are read into quarantine rather than into the model's context.

A yes is kept: later sessions started in that directory trust it without asking, until you run
`/forget-trust` ([Remembering the answer](security/trust.md#remembering-the-answer)). A no is not
kept, so the next session asks again, and you can work in the directory either way. `--resume`
restores the answer that session's own user gave.

What that answer means in detail, and every other way a path comes to be trusted, is
[Trusted directories](security/trust.md).

## Ask for something

```
> what does crates/cli/src/main.rs do?
```

Brave Bot reads the file, and answers. A read of a file in a trusted directory reaches the model
directly. A read of a file nobody vouched for is quarantined, and you are offered the chance to
vouch for that one file at the moment it matters:

```
╭ let the model read this file? ────────────────────────────╮
│Trust game.js                                              │
│                                                           │
│  the model cannot read this file, so it is working blind  │
│  on it. Vouching lets it read this file for the rest of   │
│  this session, here and in every later read.              │
│                                                           │
│┃ const SPEED = 100;                                       │
│                                                           │
│  y trust it    n leave it quarantined    ctrl-c stop      │
╰───────────────────────────────────────────────────────────╯
```

## Ask for a change

```
> add a --quiet flag that suppresses the progress line
```

Every write and every edit is put to you before it happens. An edit is shown as a diff, which is
why the agent prefers `edit_file` to rewriting a whole body. Declining is not cancelling: the turn
carries on and can try something else. Ctrl-C refuses and stops.

## Run something

```
> run the tests
```

A `run` prompt shows the compiled plan: every step, the binary each resolved to, the directory, and
every file the line would write. A step whose name reached its binary through a link, such as a
virtualenv's `python`, shows the link and then the binary, as `link -> binary`. The prompt also says
what the command is confined to, which is the directories the session was opened on and what each
program in the line brings beyond them, such as the credentials a `git push` needs. On Linux and
Windows, when a program in the line other than git can write more than the temporary directory, the
heading also warns that it can write a repository's hooks and configuration, which git runs later
with no sandbox. Where the command runs with no sandbox, the prompt says it is not sandboxed instead:

```
  y run it    a always    n don't    ctrl-c stop the turn
```

`a` is a standing permission for that exact command in this session, and it grants two things
together: the command runs again unasked, and what it prints is read as trusted rather than coming
back as a reference. See [the `run` tool](reference/tools.md#run).

Or run it yourself. Type `!` on an empty prompt and the line becomes a command for your own shell:

```
! cargo test
```

Its output goes to the model in full, so the next thing you type can be "fix the first failure".

## One-shot and piping

```sh
bravebot "what does src/main.rs do?"        # one-shot
bravebot "explain this" --file notes.md     # with named context
gh pr diff | bravebot -p "summarise this"   # with piped input
```

A one-shot run refuses effects rather than applying them unseen, because there is nobody to ask.
Piped input is untrusted and private, always. See [Non-interactive use](using/headless.md).

## Where to go next

- [Interactive mode](using/interactive-mode.md): the keys, the box, and what a running turn refuses.
- [Adding context](using/context.md): `@path`, `--file`, pasting and dropping files.
- [Instructions](customize/instructions.md) and [Skills](customize/skills.md): standing rules for a project.
- [Slash commands](reference/commands.md): `/status`, `/model`, `/add-dir`, `/compact`, `/clear`.

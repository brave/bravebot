---
sidebar_position: 2
title: Quickstart
description: Install Brave Bot, run your first task, and understand the questions it asks.
---

# Quickstart

Install Brave Bot with npm, then run `bravebot` in a project directory to start a session.

```sh
npm install -g @brave/bravebot
cd your-project
bravebot
```

## Install

The install downloads the release binary for your platform and verifies its checksum. macOS, Linux
and Windows are supported, on both x86_64 and arm64. To build from source instead, see
[Development](development.md).

On macOS and Linux there is an install script, for a machine with no npm on it:

```sh
curl -fsSL https://raw.githubusercontent.com/brave-experiments/bravebot/main/install.sh | sh
```

It fetches the newest release for your platform, checks it against the published checksum, and
writes nothing if the two differ. It puts the binary in `/usr/local/bin`, asking for sudo only if
that directory is not yours to write to. `INSTALL_DIR` puts it somewhere else:

```sh
curl -fsSL https://raw.githubusercontent.com/brave-experiments/bravebot/main/install.sh \
  | INSTALL_DIR="$HOME/.local/bin" sh
```

Running the line again is how an install made this way is updated. The script records where it put
the binary, so a second run with no directory named replaces that one rather than leaving two on
your `PATH`.

Configuration is baked into the released binary, so there is nothing to set up. Check what it will
actually use:

```sh
bravebot doctor
```

```
configuration OK
  endpoint  https://ai-chat.bsg.brave.com/v1/chat/completions
  premium   https://ai-chat-premium.bsg.brave.com/v1/chat/completions
  key id    …
  model     automatic-brave-bot (default)
  key       … (never transmitted)

confinement …
```

## Staying current

A session that opens on a version something newer has replaced says so, once, under the trust
question, and gives the line that updates the copy you are running: the npm command for an npm
install, the script again for a script install. A build from source is told nothing, since neither
line would update one.

Nothing about it waits. The notice comes from an answer an earlier launch wrote down, and the
request that refreshes it, at most one a day, runs behind the session and is for the next one. So a
first run says nothing, and a release published this morning reaches somebody who last opened a
session last night tomorrow.

Every way this can fail is silence: no network, a registry that will not answer, an answer of an
unexpected shape. A version that is not three numbers is never announced either, release candidates
included. An [incognito session](using/sessions.md#a-session-that-leaves-nothing-behind) still
reads an answer an ordinary session left, and neither records one nor asks.

## The first question: do you trust this directory?

Brave Bot asks whether you trust the working directory before anything else.

- **Trust it** and ordinary work proceeds: files are read as trusted, and edits are not shown to
  you for every path in the tree.
- **Decline** and nothing is trusted. The session still works. Every write is shown to you
  first, and files are read into quarantine rather than into the model's context.

The answer belongs to the session, not to the directory: every fresh session asks again, whatever
you answered last time. `--resume` restores the answer that session's own user gave.

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
every file the line would write. It also says that the command is not sandboxed:

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

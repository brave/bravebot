---
sidebar_position: 13
title: Running Brave Bot in herdr
description: What Brave Bot tells herdr when it runs in a herdr pane, and what herdr does with it after a restart.
---

# Running Brave Bot in herdr

[herdr](https://github.com/herdrdev/herdr) runs coding agents in terminal panes and marks each pane working,
blocked or idle. Start `bravebot` in a herdr pane and it reports to herdr by itself. There is
nothing to install or turn on.

## What herdr is told

- **What the session is doing.** The pane shows `working` while a turn or command runs, `blocked`
  while an approval, a question or a confirmation waits for you, and `idle` at the prompt. The note herdr shows beside a blocked
  pane is a fixed sentence and says nothing about what is being asked.
- **How to reopen the session.** Once the session's record is written, herdr is given
  `bravebot --resume <id>`. After a herdr server restart, herdr runs that command in the pane's
  directory and you are back in the same session. A session that becomes another, as `/clear` and
  `/branch` make it, gives herdr the new id.
- **That it has left.** Quitting tells herdr the pane is free, so herdr clears the agent and its
  resume command at once. Opening an editor or a viewer, stopping with Ctrl+Z, or switching sessions with `/resume` does not, since
  Brave Bot comes back from those.

## What the resume command carries

herdr runs the command without asking you, so it only ever carries what narrows the session:
`--locked`, `--safe`, a `--sandbox` mode tighter than your settings give, `--run-network closed`,
and the paths you denied with `--sandbox-deny-read` or `--sandbox-deny-write`. Anything that
loosens the session is left out, such as `--dangerously-skip-permissions`, `--accept-edits`,
`--auto`, `--sandbox-allow-read`, `--sandbox-allow-write` and `--add-dir`. `--settings` and
`--log-level` are not carried either, so a session that ran under a named settings file reopens
under your usual settings.

## Sessions that send no resume command

- An [incognito](incognito.md) session still reports its state but never gives herdr an id or a
  command, since herdr would keep them after the session is gone.
- A [background session](sessions.md) has no interface in the pane, and `bravebot attach` only
  shows one, so neither reports to herdr.

## Turning it off and where it works

Outside a herdr pane nothing is sent. Brave Bot reports only when herdr's own variables
(`HERDR_ENV`, `HERDR_PANE_ID`, `HERDR_SOCKET_PATH`) are set, and talks only to the socket herdr
names there. To stop herdr reopening sessions after a restart, set herdr's own switch:

```toml
[session]
resume_agents_on_restore = false
```

Reporting works on macOS and Linux. On Windows Brave Bot does not report to herdr yet.

If Brave Bot exits without quitting, after a crash for example, herdr clears the pane by itself
once the shell prompt is back.

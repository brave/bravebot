---
sidebar_position: 6
title: Hooks
description: Run a command of your own when a turn starts, when a tool call finishes, or when a turn is over.
---

# Hooks

A hook is a command of your own that runs when something happens: format after an edit, notify when
a turn is over.

## Where a hook is declared

One file, in one place: `hooks.json` in `~/.bravebot`, the directory holding what is yours.

No other file declares a hook. A checkout carries none, a directory you happen to start a session
in carries none, and there is no layering, no merging and no overriding. On a platform that names no
profile directory there are no hooks at all.

This is deliberately *not* `settings.json`. Settings are read from three places and two of them sit
inside a checkout, so a command named in one would be a command that arrived with a clone. A
[settings file](configuration.md) may name a destination and never a command.

```json
{
  "hooks": [
    { "on": "tool-finished", "tool": "write_file", "run": ["cargo", "fmt"] },
    { "on": "turn-finished", "run": ["/usr/bin/osascript", "-e", "display notification \"done\""] }
  ]
}
```

## The three moments

Each entry names one moment in `on`:

| Moment | Fires |
|---|---|
| `turn-started` | before the turn you asked for sends anything |
| `tool-finished` | after one tool call has finished, whatever came of it |
| `turn-finished` | when the turn you asked for is over, however it ended |

`turn-finished` fires even when the turn was cancelled or failed on its first request, because the
commonest thing to attach to it is telling somebody who walked away, and a turn that died is exactly
when they want telling.

A `tool-finished` entry may add `"tool": "write_file"` to fire for calls of that name alone. An entry
naming no tool fires for every call. A `tool` on either of the turn moments is about a call that does
not happen there, so that entry fires for nothing.

Two entries on one moment are two commands, run one after the other in the order the file lists them.
A word this build does not fire declares no hook and leaves the rest of the file in force.

The turn moments belong to the turn *you* asked for. A delegate is a run inside that turn, so a turn
that spawns three delegates still fires `turn-started` once. A call a delegate made is a call, and
fires `tool-finished` like any other.

## A hook is a program and its arguments

`run` is an array: the program first, then each argument as its own string. Nothing joins them into a
command line and nothing re-parses one, so an argument holding a space, a semicolon or a quote is one
argument and arrives as one.

```json
{ "on": "turn-started", "run": ["echo", "a b; c"] }
```

That runs `echo` with a single argument, `a b; c`. There is no shell anywhere in it.

An entry whose `run` is a single string, whose arguments are not all strings, or which names no
program at all declares no hook. This is a deliberate difference from tools that spell a hook as a
line: copying the spelling means copying the parse, and a line a shell parses is a place where
quoting decides what runs.

## Where it runs, and what it reaches

A hook runs in the directory the turn is working in, so a relative path in one means what you meant.

It is not confined. Its environment is the one Brave Bot was started from, less this agent's own
credentials, which are the same names taken off a program the planner asks to
[run](../reference/tools.md#run). You read the program and the arguments you wrote down, so a secret
travelling beside those is something you were never shown, and a formatter has no use for one.

## What a hook is told

One line of JSON on standard input, naming the moment and nothing else:

```json
{"event":"turn-finished"}
```

No path, no argument a model wrote, no bytes out of a file, and nothing added to its environment. The
`tool` on a `tool-finished` entry selects which entries fire and reaches no process. A hook that needs
to know which tool ran declares one entry per tool.

## A hook decides nothing

Nothing a hook returns or prints changes what happens next. Its output is not read at all, and its
exit status is used for one thing: telling you that your hook is failing. No hook refuses a tool
call, holds one up for an answer, alters an argument, or ends a turn.

A hook runs beside a turn that is reading files, so anything it says is a function of bytes nobody
vouched for. A hook that grepped a file and exited non-zero would be that file deciding whether the
next tool call happens, through a channel no gate can see.

**The cost, stated plainly: a secret scanner that blocks a write is not a hook, and cannot be one.**
A rule about what to refuse is written as a rule, in
[permissions](../security/permissions.md), where the decision comes from something you wrote rather
than from something a program read.

## When one goes wrong

A hook that could not be started, that ended badly, or that was still running after 30 seconds and
was stopped, is reported to you, naming the moment and the program. It reaches a live display as it
happens and the turn's own account of itself as well, so a run with nowhere to draw still says it.

The turn carries on in every case. A hooks file that is missing, unparseable, larger than 64 KB, or
holding an entry this build cannot use, is that much of it not applying and never a session that will
not start: it is your own file, written by hand, and the failure it most often has is a typo.

Nothing a hook prints is read, so this report is the only way you learn your formatter has not run
since you mistyped its path.

## Known costs

- **A hook is told the moment, not the path.** The formatter above runs over the project rather than
  over the file that changed. For a formatter that is a slower run of the same work; for something
  that wanted to act on one file it is the feature not being there.
- **Nothing a hook prints is kept.** Debugging one means having it write a file.
- **A hook runs while the turn waits.** Every hook is time added to the turn, up to 30 seconds each. A
  `tool-finished` hook with no `tool` pays that on every call.
- **There is no moment for a session.** Something that should happen once when the program opens fires
  per turn instead.

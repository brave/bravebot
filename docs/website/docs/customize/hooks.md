---
sidebar_position: 6
title: Hooks
description: Run a command of your own when a session starts or ends, when a turn starts, when a tool call finishes, when a turn is over, when a question is waiting on you, when a checkout is made for a delegate, or when a conversation is compacted.
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
    { "on": "tool-finished", "tool": "write_file", "run": ["cargo", "fmt"], "timeout": 120 },
    { "on": "turn-finished", "run": ["/usr/bin/osascript", "-e", "display notification \"done\""] }
  ]
}
```

## The nine moments

Each entry names one moment in `on`:

| Moment | Fires |
|---|---|
| `turn-started` | before the turn you asked for sends anything |
| `tool-finished` | after one tool call has finished, whatever came of it |
| `turn-finished` | when the turn you asked for is over, however it ended |
| `prompt-shown` | just before a prompt or a question is put to you |
| `checkout-made` | once a checkout has been made for a delegate, before that delegate's first request |
| `compacted` | after a summary has replaced the older part of the conversation in the request |
| `session-started` | when the interface you opened has begun a session, before it sends a turn |
| `session-ended` | when you leave that interface, however it ended |
| `file-changed` | when the file the entry names has changed since the last look |

`turn-finished` fires even when the turn was cancelled or failed on its first request, because the
commonest thing to attach to it is telling somebody who walked away, and a turn that died is exactly
when they want telling.

A `tool-finished` entry may add `"tool": "write_file"` to fire for calls of that name alone. An entry
naming no tool fires for every call. A `tool` on any other moment is about a call that does
not happen there, so that entry fires for nothing.

A `file-changed` entry names the one file to watch in `path`, a literal path that is relative to the
directory you opened bravebot in or absolute, and not a pattern or a directory:

```json
{ "on": "file-changed", "path": ".envrc", "run": ["/usr/bin/notify-send", ".envrc changed"] }
```

The full-screen interface looks at the file every 5 seconds and runs the command when its size or
modification time differs from the last look, or when it appears or is deleted. Several changes in
one window run it once, and it does not run again until 5 seconds after the last run ended. No turn
starts and the model is told nothing. The command is told the moment and not the path, so one
command for two files is two entries. Up to 8 entries are kept, and the looks end when you leave.

Two entries on one moment are two commands, run one after the other in the order the file lists them.
A word this build does not fire declares no hook and leaves the rest of the file in force.

The turn moments belong to the turn *you* asked for. A delegate is a run inside that turn, so a turn
that spawns three delegates still fires `turn-started` once. A call a delegate made is a call, and
fires `tool-finished` like any other.

`prompt-shown` is how a notifier calls you back when you have switched windows: the turn is waiting
on an approval or a question, and `turn-finished` will not fire until you answer. It is told that
the session is waiting and not what for, like every hook. It fires only for a prompt that reaches
you: not in a one-shot run where nobody can be asked, and not for one your permission mode answered
in advance. It cannot answer the prompt, and the hook runs before the prompt is drawn, so a slow one
delays it up to its timeout.

`checkout-made` is where a [checkout](agents.md#a-checkout-of-its-own) gets what a checkout lacks. A
checkout holds the last commit's tracked files, so a delegate working in one has no installed
dependencies and none of the files your repository ignores, such as `.env`. The hook runs once for
each checkout, in the checkout's root, before its delegate sends anything, and it is declared in your
own `hooks.json` like every hook, so a project cannot name the command:

```json
{ "on": "checkout-made", "run": ["/usr/local/bin/set-up-checkout"], "timeout": 300 }
```

Name the program by an absolute path outside the repository, as above: the command runs unconfined
with your access, and a relative path would run whatever file the repository holds at that name.

A delegate that works in your working directory fires nothing, and so does one started inside a
checkout, which works in the checkout it was given. A setup that fails is reported to you and the
delegate runs in the checkout as it stands. A checkout this hook ran in is kept for you to remove with
[`/checkouts remove`](../reference/commands.md#checkouts-apply-n--remove-n), as one a delegate ran a
program in is.

`compacted` lets a command react to the session losing its detailed history: save notes, log the
event, or tell you. It fires once for each summary that was adopted, whether the context budget
forced it in the middle of a turn or you asked for it with `/compact`. It does not fire when there
was nothing worth summarising, when the summary was refused or could not be made, or for a delegate
summarising its own conversation. It is told that a compaction happened and nothing of the summary
or of what it replaced, like every hook, and what it prints does not reach the planner. It cannot
undo the compaction, and one that fails is reported while the conversation stays shortened.

`session-started` and `session-ended` are for a command that should run once for the session rather
than at every turn, such as starting a local service or writing a log line. They belong to the
interface you opened and fire once for it: the full-screen interface, a session in lines
(`--plain`), and a one-shot run (`-p`), which is a session of one turn. A turn never fires them, and
neither does a delegate. `/resume` and `/clear` begin another conversation in the interface you
already have open and fire neither. A session you hand to the background with `/bg` goes on, so
`session-ended` does not fire for it. A run that was refused before it could send anything, or an
interface you left at one of the questions it opens with, began no session, so neither fires.

The hooks file is read when the session begins, and the same hooks fire when it ends. Both run in the
directory the session opened in.

The desktop app, an editor that hosts bravebot over ACP and the local host fire them too, once for
each session they hold. A session there begins with its first turn, so one you opened and sent
nothing to fires neither, and ends when you close it, or when the app or the editor ends and the
process waits up to ten seconds for the hook. `session-ended` runs after the turn that was running
when you closed the session has ended. If a `session-started` hook cannot be started, the first turn
says so. A `session-ended` hook that cannot be started has nowhere to say so, because the session is
already closed.

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
For `checkout-made` that is the checkout's root, which holds the repository's tracked files, so a
relative path there names a file the repository holds.

It is not confined. Its environment is the one Brave Bot was started from, less this agent's own
credentials, which are the same names taken off a program the planner asks to
[run](../reference/tools.md#run), and with a `PATH` cut down as described below. You read the program
and the arguments you wrote down, so a secret travelling beside those is something you were never
shown, and a formatter has no use for one.

A relative path such as `./scripts/format.sh` runs whatever file is at that name in the working
directory when the hook fires, and a program the agent ran there may have written it. Where that
matters, name the hook's program by an absolute path outside the project.

A program named without a path, such as `cargo`, is looked for only in `PATH` directories that a
program the agent runs cannot write. One found only under the session's own directory, such as a
`.venv/bin` or `node_modules/.bin` entry, is reported as a hook that could not be started. A name
with a path in it is yours and is never looked up.

A hook is also started with a `PATH` of those same directories only, in the order written, so a
program it starts by name (the `node` in a `#!/usr/bin/env node` script, or a program a launcher
runs) is not found in a directory a program the agent runs may write. A tool your hook needs that
lives only in such a directory, for example one run from a project's `node_modules/.bin` or an
activated virtualenv's `bin`, is not found by it. A hook that needs one names it by a path in its
`run`. A directory the agent is given writes in later in the session leaves the `PATH` from then on.

## What a hook is told

One line of JSON on standard input, naming the moment and nothing else:

```json
{"event":"turn-finished"}
```

No argument a model wrote, no bytes out of a file, and no path, except for `checkout-made`. That
hook's environment holds two variables, `BRAVEBOT_CHECKOUT`, the checkout it ran for, and
`BRAVEBOT_WORKING_DIRECTORY`, the directory the checkout was made from, so a setup command can copy
a file across:

```sh
cp "$BRAVEBOT_WORKING_DIRECTORY/.env" "$BRAVEBOT_CHECKOUT/.env"
```

Bravebot adds neither to the environment of any other hook, and adds nothing else to anyone's. The `tool` on a
`tool-finished` entry selects which entries fire and reaches no process. A hook that needs to know
which tool ran declares one entry per tool.

## A hook decides nothing

Nothing a hook returns or prints changes what happens next. Its output is not read at all, and its
exit status is used for one thing: telling you that your hook is failing. No hook refuses a tool
call, holds one up for an answer, alters an argument, or ends a turn.

A hook runs beside a turn that is reading files, so anything it says is a function of bytes nobody
vouched for. A hook that grepped a file and exited non-zero would be that file deciding whether the
next tool call happens, through a channel no gate can see.

**The cost, stated plainly: a secret scanner that blocks a write is not a hook, and cannot be one.**
A rule about what to refuse is written as a rule, in
[permissions](../security/permission-rules.md), where the decision comes from something you wrote rather
than from something a program read.

## How long a hook may run

A hook is stopped after 30 seconds, and a `session-ended` hook after 5, since you are leaving. An
entry may set its own limit with `timeout`, in whole seconds from 1 to 600:

```json
{ "on": "turn-finished", "run": ["make", "test"], "timeout": 300 }
```

An entry whose `timeout` is anything else, such as 0, 601, 1.5 or a string, declares no hook. It is not
rounded to the nearest limit, because that would be a limit you did not write. The ceiling is there
because every hook holds the turn open while it runs.

While a hook runs, the status line says so, naming the moment and the program, for example
`Running hook: cargo (tool-finished)`. It disappears when the hook ends and leaves nothing behind in
the transcript. It never shows anything the hook printed.

## When one goes wrong

A hook that could not be started, that ended badly, or that was still running when its time ran out
and was stopped, is reported to you, naming the moment and the program, and the time in seconds for one
that was stopped. It reaches a live display as it
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
- **A hook runs while the turn waits.** Every hook is time added to the turn, up to 30 seconds each, or the `timeout` the entry sets, at most 600. A
  `tool-finished` hook with no `tool` pays that on every call.
- **`checkout-made` holds its delegate back while it runs.** The delegate sends nothing until its
  hook has ended, so a setup that installs dependencies delays the work by as long as it takes. Set
  a `timeout` for one that needs more than 30 seconds. Stopping the turn does not stop a hook that is
  running. Delegates given a checkout by one call run their hooks at the same time, and the status
  line names only one of them.
- **A hook that reaches its time limit is stopped alone.** The program it started is stopped and the
  programs that one started keep running.
- **`compacted` does not say who asked, and there is no moment before a compaction.** A compaction you
  asked for and one the budget forced run the same hooks with the same line. A command that has to
  save something while the full history is still in the request has no moment to attach to.

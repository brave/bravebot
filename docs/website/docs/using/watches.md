---
sidebar_position: 7
title: Watching a file
description: Ask to be told when one file changes, and have a turn begin by itself when it does.
---

# Watching a file

Ask to be told when a file changes and the turn arms a **watch**: a standing look at one path that
outlives the turn that armed it and begins a turn of its own when the file moves, with nobody typing
anything.

```
> tell me when target/release/bravebot is rebuilt
```

The turn calls `watch_file` with one path. Its only argument is that path, so the whole of the
decision you are approving is which file this session may be told about.

## What a watch may be armed on

One file, settled when the watch is armed and never changing afterwards. Not a glob, not a directory,
and not a tree walked for anything underneath it. The file has to exist now: a path with nothing at it
is refused rather than watched for something to appear.

A directory is refused because what changed inside one is a file name the filesystem produced, and a
firing may carry nothing off the filesystem. A path with nothing at it is refused because the first
look would have nothing to compare against, so finding the file would be reported as a write that
never happened.

Asking about one file is what a watch is for. Where what you are waiting on is not a single file, or
where you want your own line sent again on an interval, the answer is a
[repeating prompt](../reference/commands.md) instead.

## What a watch observes

A watch does one thing: it looks at the path, and compares what it sees with what it saw at the look
before. **Size and modification time, and nothing else.** No content, and nothing derived from
content: not a hash, not a first line, not a count of what differs.

The first look is taken when the watch is armed, so a change is a change since you asked about the
file. Every look afterwards is measured against the one before it.

Those two facts stay with Brave Bot itself and are never quoted to the planner. A firing says that the
path looks written to, which is the strongest thing it can say, and it says it without naming a size
or a time. A file the [trust map](../security/trust.md) quarantines is one whose bytes may not reach
the planner, and a watch that derived a bit from those bytes would be releasing exactly that.

It starts no process, writes no file, and sends no request.

## What a firing puts in the conversation

A firing begins a turn by putting one line in the conversation in your own role. That line is Brave
Bot's own sentence, and the only things in it that vary are which watch fired and the path it was
armed on. No file content, no size, no modification time, no directory listing, and no name the
filesystem produced.

The turn then asks for the content if it wants it, and the content arrives labelled, through the same
[read](../reference/tools.md#read_file) every other path into the conversation goes through.

**The path in that sentence is prose and vouches for nothing.** No file is opened by it, and writing
it as `@path` would not make it an endorsement: a keystroke is what makes
[naming a file](context.md) an endorsement, and there is no keystroke behind a sentence the program
wrote.

The role a synthesized prompt lands in is the one the model trusts most, so the only safe content for
that sentence is content that was never untrusted. The path qualifies twice over: it was written by
the turn that armed the watch, out of a context holding no untrusted content, and it cannot have
changed since.

## When a firing happens

A watch that has seen a change fires when the session is idle and nothing you queued is still
waiting, and not before. **It never interrupts.** A filesystem event is not a licence to take the
session off you.

Any number of changes seen while a turn is running, or while a firing is held for one, produce one
firing when the session is free. A firing reports that the path looks written to, which is one fact
however many times it was written.

## What arming asks you

Arming goes through the gate a read of that path goes through, and grants nothing beyond it. Inside
the working directory that is a promotion nobody is asked about. Outside it, that is whatever the
[trust map](../security/trust.md) already answers, and a path a read would be refused is a watch that
is refused.

There is no second prompt of its own: asking to be told when a file changes is asking for less than a
read of it. Where the answer that allowed it stops holding, the watch ends and says so rather than
carrying on looking.

## One of three at a time

A session runs a watch, a repeating loop, or a goal, and never two kinds at once. Several watches may
be live together.

A turn that would arm a watch while a loop is running or a goal is set arms nothing and says why. You
starting a loop or setting a goal while watches are live ends them, saying so. A turn is refused where
you are not, because a turn that silently took your goal off would be ending work you are waiting on
in order to watch a file.

## The bounds

| Bound | Value |
|---|---|
| a watch's age | 7 days, then it ends itself and says so |
| live watches in one session | 8, and arming a ninth is refused rather than dropping one |
| between two firings of the same watch | 5 seconds, measured from the end of the last firing's turn |
| between two looks at a watched path | at most 5 seconds, which is what a firing's latency is |

A change is noticed by looking, so the latency is how often a look happens rather than how fast the
filesystem is. Five seconds is short enough that saving a file feels like the cause of the firing.

## Seeing them, and ending one

`/status` lists every live watch: a number that names it, the path, which turn armed it, and how long
it has left. A session with no live watch says nothing about watches. `/watch` gives the same report.
A firing announces itself, naming the watch it came from.

Numbers are assigned in the order watches were armed, and a number is never reused, so the number
`/status` gives a watch is stable for its whole life.

Seven things end a watch, and each of them says so:

| What | When |
|---|---|
| you end one | `/watch stop <n>`, naming the number the report gives it, which leaves the others |
| you interrupt | Ctrl-C with nothing nearer to stop, which ends every live watch |
| a firing's turn is stopped | the watch that fired ends with it, and the others stand |
| you ask for a loop or a goal | every live watch ends, since a session does one of the three at a time |
| the path stops being readable | the answer that armed it no longer holds |
| the session moves on | `/clear`, and leaving |
| age | 7 days after it was armed |

Ctrl-C means one thing at a time, and the watches are the last rung before leaving: a mode open over
the session, then the turn in flight, then the half-written line, then the loop or the goal, then every
live watch, then leaving.

Stopping a firing's turn ends the watch that fired it, because otherwise the key never reaches a watch
that is firing often: every press would land on a turn, and the next firing arrives seconds later.
Stopping a turn that was not a firing ends no watch.

A watch that ended in silence would be indistinguishable from a watch that is live and has seen
nothing, and the difference between those two is the whole of what you armed it to learn.

`/watch` on its own lists watches and `/watch stop <n>` ends one. Neither arms one: a watch is armed by
asking for it in a prompt.

## Nothing survives the session

A watch is not in the [session record](sessions.md). Resuming a session restores no watch, and nothing
on disk survives a run. The process ending is what reaps it, on the crash path as much as the ordinary
one.

A watch that outlived the session would start sending prompts at somebody who opened a conversation to
read it, with no visible cause and nothing in the transcript to explain it.

## Known costs

- **A firing spends a turn.** Every firing is a whole turn with the conversation re-sent. The bounds
  hold the rate rather than the total.
- **Size and modification time answer a narrower question than the one people ask.** A watch says a
  path looks written to. It does not say what changed, it fires on a write that changed nothing, and a
  filesystem that does not move a modification time hides a change from it entirely. A turn that
  reports a firing as a changed file is claiming more than the firing said.
- **A five-second look is a five-second window.** A file written and written back inside one window is
  a change nothing reports.
- **Nothing survives the session, by choice.** Wanting to know when a file changes overnight is not
  served, and the answer is to leave the session open.
- **Inside the working directory, nobody is asked at all.** A read there is promoted rather than put to
  you, and a watch asks exactly what a read asks, so a week-long channel about a path can be opened
  without a prompt. `/status` and the bounds are the whole of what answers for that.

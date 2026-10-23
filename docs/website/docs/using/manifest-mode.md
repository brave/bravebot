---
sidebar_position: 9
title: Plan first, then execute
description: Have the planner write the whole plan before anything is read, approve it once, and have a driver walk it with no model in the control path.
---

# Plan first, then execute

```sh
bravebot --mode manifest "collect every TODO comment into notes/todos.md"
```

`--mode manifest` decides **everything first**. The planner emits a step list, that list is refused or
frozen, and a driver walks it with no model anywhere in the control path. The default, `--mode turn`,
is what an unqualified `bravebot "task"` has always been: observe, decide, act, and decide again after
each thing it reads.

The gates are the same ones in both modes. What changes is the scope of the precommitment, from one
turn to a whole run.

The two planning calls run on the run's model. Each processing step runs on the model
[`--processor-model`](../reference/cli.md#--processor-model-name) or the
[`processorModel`](../customize/configuration.md#processormodel) setting names, so a plan can be made
on a stronger model and carried out on a cheaper one.

A plan that fails validation **fails the run whole**: nothing is half adopted, no step is patched to
make a plan usable, and nothing re-plans once a step has read something. Where every effect will land
is fixed before the first byte is read, and the driver cannot insert, skip, reorder or invent a step.
A destination has to be text, so a plan that filled one with a number or a list is refused whole
rather than reaching the step that would have used it. The same holds for the `contents` of a
`write_file` step and the `instruction` of a processor step: a number or a list there is refused with
"'contents' must be text" or "'instruction' must be text", where it used to be written as an empty
file. An unknown mode name is refused rather than
guessed, since guessing would run the mode you did not ask for.

A session can start one of these runs too, with `/manifest`. See
[Starting a plan-then-execute run](#starting-a-plan-then-execute-run).

## A plan is put to you first

Between freezing the plan and walking it, the frozen plan is put to you: the task in your own words,
then **every step in order**, each naming its tier, what it would do, and every destination the step
fixed. The routing is part of what you are answering for, so the line carries the directory a search
runs in and the glob it filters by, not the pattern alone.

Declining stops the run, and the declined plan comes back with the error so you can still read it.
Nothing has been read or written by then, so a declined run leaves the workspace exactly as it was.

The question is asked **once**, and has no standing form: nothing between the plan and the run can
reshape the plan, and a plan is written afresh for each run, so remembering an answer would be
approving steps nobody has seen.

**Approving a plan is not approving its writes.** This mode widens the scope of what you commit to in
advance and does not replace the gates inside it, so each write is still put to you as its step
reaches it.

Where nobody can be asked the answer is no, as it is everywhere else. A command typed at a terminal is
somebody: the plan goes out beside the progress and the answer is read back, which is the one question
a one-shot run answers. Piped or redirected there is nobody, so the run stops before its first step
unless permissions were skipped outright. `--dangerously-skip-permissions` approves the plan, and that
is not a standing answer: it covers this run, is recorded nowhere, and the next run asks again unless
the flag is given again.

:::note
At a terminal the plan is printed and a line is read, so a plan longer than the window is scrolled to
in your terminal's own scrollback, and there is no going back to re-read a step once the answer is
typed. A run [started from a session](#starting-a-plan-then-execute-run) does not have this
cost: that prompt is drawn and scrolled.
:::

## What a plan cannot use

- **`edit_file`**, because locating a passage means having read the file, and the planner has read
  nothing.
- **`todo_write`**, because the manifest is already the task list.
- **`run`, and any shell.** A command string is destination and payload at once.
- **Piped stdin**, which is refused rather than dropped: a pipe is observed context, and this mode
  does not observe before it plans. Name a workspace file with `--file` instead.

Everything a step produces is quarantined, whatever its label. There is no planner left to show it
to. The ways out are a later processor, a write back into the workspace, and a release the plan
named in advance.

## What comes back

The goal in plain words, the proposed plan verbatim, the frozen steps, and what each one did, on
success **and on failure**, never behind a flag. A failed plan is printed on stderr even without
`--trace`, and never shares stdout with the reply.

## Starting a plan-then-execute run

```
/manifest collect every TODO comment into notes/todos.md
```

starts one plan-then-execute run and hands the session back afterwards. It is a run, not a mode the
session holds: a session is several turns over one conversation, each deciding what to do next after
the last one read something, so nothing about the session could stay in that mode.

The session is **blocked for the duration.** There is no planner inside a manifest run to hand a line
to, so you can read, scroll and stop, but not send.

**The conversation is neither read nor written.** It does not go in, because the planner that reads
the task may hold the task and nothing else. Nothing comes back out either, because every step's
result is quarantined and there is no planner left to show it to. What the transcript shows is the
goal as the planner understood it, the frozen plan, each step as it runs, and the reply, none of it in
the exchange a later turn resumes. The run leaves the conversation exactly as declining a plan leaves
the workspace.

The run is written down as **its own record** and the session records its name, so the session still
resumes as a conversation while the run stays a run the picker refuses. A run you stopped is not
written at all, since there is nothing in it to read. A run that fails or is stopped after it wrote a
file keeps what it learned about that file: the file stays untrusted, and the saved session resumes with
the same trust.

The plan is [put to you before it runs](#a-plan-is-put-to-you-first), drawn and scrolled
here rather than printed on one line. In [plan mode](../security/permissions.md#answering-in-advance-modes)
a plan with a write in it does not run at all, decided before the plan is put to anybody.

## A manifest run is recorded, but cannot be continued

A plan-then-execute run cannot be resumed. Its conversation is empty, because a session is turns over
one conversation and a manifest run has none. The picker marks the row and refuses Enter. Naming one
on the command line prints what it produced, and still does not continue it. That print is a **report
rather than a refusal**: it goes to stdout and exits successfully, because reading a run back is what
naming one is for.

The record holds its goal, its proposed plan, its frozen steps and what each one did, finished or
not.

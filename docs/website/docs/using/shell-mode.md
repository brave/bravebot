---
sidebar_position: 3
title: Shell mode
description: Type `!` on an empty prompt to run a line in your own shell, and have its output reach the model in full.
---

# Shell mode

Type `!` on an empty prompt and the line becomes a command for your own shell.

```
! cargo test
! git log --oneline -20 | head
! ls build/*.o
```

The line goes to `$SHELL -c`, so globs, `$VAR`, redirection, `&&` and `$(...)` all work exactly as
they do in your terminal. `$SHELL` falls back to a POSIX shell when it is unset. An empty line is not
run.

The `!` is a mode rather than a character: the prompt changes colour, Backspace or Escape leaves it,
and the mode lasts one command.

## Nothing asks

`! rm -rf build` runs, with no approval prompt. The approval prompt exists so that a person endorses
argv the *planner* proposed. Here you are the person it would have asked.

## The output reaches the model in full

The planner reads the whole output, trusted and private, not as a reference. After `! cargo test` you
can say "fix the first failure" and it has already read the errors. Output from a *failing* command
reaches it too, since that is where the explanation is. A cancelled command records nothing.

The label is a first label from provenance, exactly like the label on a program's output or on your
own configuration. It is admissible for the reason a vouched-for command's output is: a person took
responsibility, and nothing inspected anything.

## Only a line a human typed

Shell mode is reachable from one place, a key press in the input box, and nowhere else. Never argv
the planner proposed, never text read from a file, never anything a processor produced, never a line
reconstructed from a transcript.

**The planner gets no shell tool, ever.** Not behind a capability, not behind an approval prompt, not
via MCP. If it could ask for one, everything above is void.

What it gets instead is [`run`](../reference/tools.md#run), which takes a command line and
**compiles** it: bravebot's own grammar reads the line into the programs, arguments and destinations
it names, refuses anything it cannot fully work out, and runs the result. That is shell syntax
without a shell. No line the planner writes is ever handed to an interpreter. Here, by contrast,
`$SHELL` really does read the line, because you typed it.

## The cost, stated plainly

`! cat notes-from-a-stranger.md` puts somebody else's words into the planner's context as though they
were yours. Nothing inspects the bytes to catch that, exactly as nothing inspects a directory that
was vouched for.

It is the same assertion you make by vouching for a command at a run prompt, made once for one
command. **If you would not press `a` for it, ask the agent to `run` it instead** and have the output
quarantined.

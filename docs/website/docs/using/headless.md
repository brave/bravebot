---
sidebar_position: 6
title: Non-interactive use
description: One-shot tasks, piped input, and what changes when there is nobody to ask.
---

# Non-interactive use

```sh
bravebot "what does src/main.rs do?"          # one-shot
bravebot "explain this" --file notes.md       # with named context
bravebot -p "summarise this" < build.log      # with piped input
gh pr diff | bravebot -p "review this"        # the same, from a pipe
```

A one-shot run has nobody to ask, and most of what makes it different follows from that.

## Nothing is approved

**Where nobody can be asked, nothing is approved.** Effects are refused rather than applied unseen,
and the planner's own questions are declined rather than answered on your behalf. The planner is told
that a reply came from a person, so inventing one would be worse than not asking at all.

So a one-shot run is for reading, explaining and summarising. If you want it to change something, run
it interactively, or say so on the command line:

```sh
bravebot --dangerously-skip-permissions -p "fix the failing test"
```

That flag lifts the refusal of effects, because a flag you typed is an instruction rather than a guess
made in your name. It does not lift the second half: the planner's questions are declined in that mode
too, since they are not permissions. It is a mode for a container with no network and nothing in it
worth losing. See [modes](../security/permissions.md#answering-in-advance-modes).

**An `allow` rule in your [settings file](../customize/configuration.md#permissions) decides nothing
here.** A `deny` rule and an `ask` rule still hold, since one refuses before there is anything to
prompt about and the other turns a write that would have gone through silently into one nobody can
approve. An `allow` rule says which prompts to stop raising, which is a decision about a session
somebody is sitting in front of. Read here it would let one line in a file under your home directory
do what the flag above is named and warned about for. That flag is what lets allow rules decide again.
Allow lines are still parsed, so one that cannot be read is still named on stderr.

## Reaching a second checkout

```sh
bravebot --add-dir /srv/other-checkout "how does their error type differ from ours?"
```

`--add-dir` makes a directory outside the working one reachable for the run, and may be given more
than once. It vouches for nothing: files read there are quarantined like any others, because a run
nobody is watching cannot be asked to trust a tree. See
[`--add-dir`](../reference/cli.md#--add-dir-path).

## Piped input is untrusted and private, always

Piped bytes are quarantined, and the planner is given a reference rather than the bytes. Nothing
vouched for what a pipe carries: `gh pr diff` and `cat build-error.txt` both arrive the same way, and
a pipe has no path for the trust map to have an opinion about.

The planner can still pass the reference to a processor, feed it to a program's stdin, or write it to
a file. It just cannot read it. See
[How Brave Bot works](../how-it-works.md#quarantine-and-references).

To give the agent something it can *read*, name a file instead:

```sh
bravebot "summarise this" --file build.log
```

Stdin is read only when it is not a terminal, so an interactive invocation does not sit waiting for
input nobody is sending.

Input over 10 MiB is refused rather than truncated, and says to write it to a file and name that
instead: a silently shortened input is one the planner would answer about having seen part of.

## Stdout carries the reply and nothing else

Progress, errors and the audit trail all go to stderr, so a one-shot run is pipeable:

```sh
bravebot "list the public functions in src/lib.rs" > functions.txt
```

`--trace` puts the audit trail on stderr beside it: which gate checked what, the label every value
carried, and what was released.

```sh
bravebot "what does this do?" --file src/main.rs --trace 2> trail.txt
```

## Planning the whole run first

```sh
bravebot --mode manifest "collect every TODO comment into notes/todos.md"
```

`--mode manifest` decides **everything first**. The planner emits a step list, that list is refused or
frozen, and a driver walks it with no model anywhere in the control path. The default, `--mode turn`,
is what an unqualified `bravebot "task"` has always been: observe, decide, act, and decide again after
each thing it reads.

The gates are the same ones in both modes. What changes is the scope of the precommitment, from one
turn to a whole run.

A plan that fails validation **fails the run whole**: nothing is half adopted, no step is patched to
make a plan usable, and nothing re-plans once a step has read something. Where every effect will land
is fixed before the first byte is read, and the driver cannot insert, skip, reorder or invent a step.

### What a plan cannot use

- **`edit_file`**, because locating a passage means having read the file, and the planner has read
  nothing.
- **`todo_write`**, because the manifest is already the task list.
- **`run`, and any shell.** A command string is destination and payload at once.
- **Piped stdin**, which is refused rather than dropped: a pipe is observed context, and this mode
  does not observe before it plans. Name a workspace file with `--file` instead.

Everything a step produces is quarantined, whatever its label. There is no planner left to show it
to. The ways out are a later processor, a write back into the workspace, and a release the plan
named in advance.

### What comes back

The goal in plain words, the proposed plan verbatim, the frozen steps, and what each one did, on
success **and on failure**, never behind a flag. A failed plan is printed on stderr even without
`--trace`, and never shares stdout with the reply.

## Which model a run asks for

A run with no flag asks for the model a session opening in the same directory would: the choice
`/model` recorded, then the configured one. Picking a model in a terminal is therefore enough to make
your scripts use it.

```sh
bravebot --model opus "review the diff on this branch"
```

`--model` outranks that and everything else, and is the only way two scripts in the same checkout can
ask for different models. Where the server answers with a different model from the one in force, both
names go to stderr, and a run whose `--model` was substituted exits non-zero. See
[`--model`](../reference/cli.md#--model-name).

## A one-shot turn is bounded

A one-shot run carries a limit of 200 rounds of tool calls, where an interactive turn carries none.
On the limiting round the planner is offered no tools and told it has none left, so it answers with
what it has.

## Exit codes

A failure exits non-zero. A configuration error, a refused argument, and a turn that could not run
all fail rather than exiting successfully with an explanation on stdout.

## Flags

| Flag | What it does |
|---|---|
| `--file <path>` | include a workspace file as trusted context; repeatable |
| `--add-dir <path>` | make another directory reachable, trusting nothing in it; repeatable |
| `-p`, `--print` | non-interactive; reads piped stdin as quarantined context |
| `--trace` | print the audit trail to stderr |
| `--model <name>` | the model this run asks for |

The full set is in the [CLI reference](../reference/cli.md).

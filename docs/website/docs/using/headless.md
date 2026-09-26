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

## Reading a run from a program

```sh
bravebot --json -p "fix the failing test" | jq .status
```

`--json` puts **one object, on one line**, in the reply's place on stdout. The prose reply is written
for a person, and a program can recover almost nothing from it: which tools ran, what the turn cost,
and why an effect was refused are either absent or only readable by parsing English that changes with
the reader's language.

The object says how the run ended, its exit status and identifier, the message where there is one, the
reply, the model that answered, how many rounds it took, what it cost in tokens, every tool it called
with what it acted on and whether that call was refused, and every refusal with the principle it
upholds. What a call acted on is the name it was given rather than a resolved path.

It is written on **every** run, including one that failed before the turn began and one that stopped
part way, which still says what it had done by then. A caller never has to tell an empty stdout from a
result.

The object carries a **schema number**. Within one number a field may be added, and never removed,
renamed or given a different meaning, so a caller reading the fields it knows keeps working.

Progress, the message and the trail stay on stderr, exactly as they do without the flag.

## Configuring one run differently from the next

```sh
bravebot --settings ci/bravebot.json -p "review the diff"
```

`--settings` reads one more settings file **above** the three that are found for your home directory,
the checkout and the machine, for the length of the run. It resolves a name at a time, so a file
setting one value leaves the rest of what you and the checkout configured in force, and a CI job can
change one key without restating a whole configuration.

Given twice, the last file wins. A path naming no file, and a blank path, are refused by name and the
run stops before it starts. A named file that did not parse leaves the layers below it in force, and is
visible by its absence from what [`doctor`](../reference/cli.md) lists.

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
A destination has to be text, so a plan that filled one with a number or a list is refused whole
rather than reaching the step that would have used it. An unknown mode name is refused rather than
guessed, since guessing would run the mode you did not ask for.

A session can start one of these runs too, with `/manifest`. See
[Sessions](sessions.md#starting-a-plan-then-execute-run).

### A plan is put to you first

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
typed. A run [started from a session](sessions.md#starting-a-plan-then-execute-run) does not have this
cost: that prompt is drawn and scrolled.
:::

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

A run with no flag asks for the model a session opening in the same directory would: a `model` key
in the checkout's settings, then the choice `/model` recorded, then the configured one. Picking a
model in a terminal is therefore enough to make your scripts use it, in any checkout that does not
name its own.

```sh
bravebot --model opus --effort high "review the diff on this branch"
```

`--model` outranks that and everything else, and is how two scripts in the same checkout ask for
different models without a settings file each. Where the server answers with a different model from the one in force, both
names go to stderr, and a run whose `--model` was substituted exits non-zero. See
[`--model`](../reference/cli.md#--model-name). `--effort` names how hard the model is asked to think
for one run, over the level a checkout or `/effort` named. See
[`--effort`](../reference/cli.md#--effort-level).

## A one-shot turn is bounded

A one-shot run carries a limit of 200 rounds of tool calls, where an interactive turn carries none.
On the limiting round the planner is offered no tools and told it has none left, so it answers with
what it has.

## Exit codes

A failure exits non-zero, and **the status says which failure it was**. A configuration error, a
refused argument, and a turn that could not run all fail rather than exiting successfully with an
explanation on stdout.

| Status | Identifier | The run |
|---|---|---|
| 0 | | did what it was asked |
| 1 | `BB1001` | failed for a reason none of the others name |
| 2 | `BB1002` | refused an argument, so nothing ran |
| 3 | `BB1003` | cannot use the configuration, so nothing ran |
| 4 | `BB1004` | had an effect refused by a gate |
| 5 | `BB1005` | never reached the backend |

A status is never renumbered and never given a second meaning, so a script can branch on one: "the
endpoint was not there, try again", "the configuration is wrong, fail the build" and "a gate refused
the write, this needs a person" are three different things to do about a failed run.

Only the transport's own failures are status 5. A non-success answer *from* the service is the service
answering, so a caller that read a refused credential as a connection worth retrying would retry it
until it gave up.

The identifier is printed in front of the message on stderr, never instead of it, and is the same
whatever language the message is in. A sentence in your own language is the right thing to print and
the wrong thing to search for.

## Flags

| Flag | What it does |
|---|---|
| `--file <path>` | include a workspace file as trusted context; repeatable |
| `--add-dir <path>` | make another directory reachable, trusting nothing in it; repeatable |
| `-p`, `--print` | non-interactive; reads piped stdin as quarantined context |
| `--trace` | print the audit trail to stderr |
| `--model <name>` | the model this run asks for |
| `--json` | one result object on stdout, in the reply's place |
| `--settings <path>` | read a settings file above the ones found, for this run |
| `--mode manifest` | plan the whole run first, then walk the plan |

The full set is in the [CLI reference](../reference/cli.md).

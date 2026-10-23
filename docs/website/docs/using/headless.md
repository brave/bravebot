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

If the run only needs to edit files, `--accept-edits` is the narrower grant: writes go through, and
commands, fetches and a write that would create a credential are still refused.

```sh
bravebot --accept-edits -p "apply the lint fixes"
```

`--dangerously-skip-permissions` lifts the refusal of every effect, because a flag you typed is an instruction rather than a guess
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

## Reading the project's own files

A run starts with nothing trusted, so the planner reads project files only through a processor or a
file named with `--file`. `--trust-workspace` trusts the working directory and everything under it
for this run, as answering yes to the startup question does, and nothing else: directories from
`--add-dir` stay untrusted and no record is written. Trusting the directory opens reads only: a
write is still refused unless the run was given `--accept-edits` or `--dangerously-skip-permissions`.

```sh
bravebot --trust-workspace -p "summarise what src/ does"
```

A run in a directory where a session once kept a yes (`y` at the startup question), or inside the
git worktree whose root it was kept about, opens trusting the directory without the flag, and says so
on stderr with how to take it back. The flag is read from the command line only, so a settings file
or a checkout cannot turn it on.

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

To follow a run as it happens, use `--json-stream`. It writes one event per line for each tool call,
refusal and change in token usage, then the same object as the last line, so a script can show
progress or tell a hung run from a slow one. [The reference](../reference/cli.md#--json-stream)
lists the events.

## Sending several turns to one process

```sh
bravebot --input-format lines --trust-input-lines <<'EOF'
{"type":"prompt","text":"review the diff"}
{"type":"prompt","text":"now fix what you found"}
EOF
```

`--input-format lines` reads JSON lines on stdin for as many turns as it is sent. A `prompt` starts a
turn over the conversation the last one left, a `steer` reaches the turn that is running, an `answer`
replies to a question the agent asked, and a `stop` ends the turn in flight. Each turn ends with its
own result object. Where nobody typed `--trust-input-lines`, the text of a line is a program's words
and the agent is given a reference to it, never the words. [The reference](../reference/cli.md#--input-format-lines)
has the lines and the events.

## Sending a follow-up to an earlier run

```sh
id=$(bravebot --json -p "review the diff" | jq -r '.session')
bravebot -p "now fix what you found" --resume "$id"
```

A one-shot run is written down as a session, and `--json` names it under `session`. The record carries
the turn's timing and the time the run began, and the run's audit trail is kept beside it, so a resumed
session shows both without `--trace`. `--resume <id>`
or `--continue` with a task runs that task as one more turn of it, so a script can chain a review, a
fix and a summary over one conversation instead of restating it each time. Piped input stays a
reference, and `--incognito` writes nothing, so there is no id to pass on.
[The reference](../reference/cli.md#continuing-a-session-from-a-one-shot-run) has the rest.

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

`--mode manifest` decides everything first: the planner emits a step list, and a driver walks it
with no model anywhere in the control path. [Plan first, then execute](manifest-mode.md) covers how
a plan is put to you, what a plan cannot use, and what comes back.

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

[`--max-rounds <n>`](../reference/cli.md#--max-rounds-n-and---max-tokens-n) lowers that limit, and
[`--max-tokens <n>`](../reference/cli.md#--max-rounds-n-and---max-tokens-n) adds one on the tokens a
run may spend. A run that reaches either stops, without the planner's answer, and ends on status 8,
so a job can tell a run that was cut short from one that finished.

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
| 6 | `BB1006` | finished, and its reply is not what `--output-schema` asked for |
| 7 | `BB1007` | worked towards the condition `--goal` gave, and it did not hold |
| 8 | `BB1008` | reached the limit `--max-rounds` or `--max-tokens` gave it, and stopped before going past it |

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
| `--session-ref <id>` | add the newest 8,000 characters of an earlier session of this directory to the task; repeatable |
| `--add-dir <path>` | make another directory reachable, trusting nothing in it; repeatable |
| `--trust-workspace` | trust the working directory for this run, as answering yes to the startup question does |
| `-p`, `--print` | non-interactive; reads piped stdin as quarantined context |
| `--trace` | print the audit trail to stderr |
| `--model <name>` | the model this run asks for |
| `--advisor <name>` | a model the planner may put a question to |
| `--processor-model <name>` | the model processors run on, in place of the `processorModel` setting |
| `--json` | one result object on stdout, in the reply's place |
| `--json-stream` | one event per line on stdout as the run goes, then the result object |
| `--input-format lines` | read turns, steers, answers and stops from stdin, a JSON object to a line; implies `--json-stream` |
| `--trust-input-lines` | treat those lines as your own words and not a program's |
| `--output-schema <path>` | hold the reply to a JSON Schema; a reply off it ends on status 6, one that conforms is also `structured` under `--json` |
| `--goal <condition>` | keep working until a judge says the condition holds; a condition that never does ends on status 7 |
| `--max-rounds <n>` | stop the run when the agent asks for a round of tool calls past this many; ends on status 8 |
| `--max-tokens <n>` | stop the run before a request once this many tokens are spent; ends on status 8 |
| `--settings <path>` | read a settings file above the ones found, for this run |
| `--mode manifest` | plan the whole run first, then walk the plan |

The full set is in the [CLI reference](../reference/cli.md).

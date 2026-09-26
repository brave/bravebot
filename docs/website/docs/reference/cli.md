---
sidebar_position: 1
title: CLI reference
description: Every command, flag and key bravebot takes.
---

# CLI reference

```
bravebot 0.9.0: a general-purpose agent resistant to prompt injection

Usage:
  bravebot                               Start an interactive session
  bravebot --plain                       Start a session in lines, taking nothing from the terminal
  bravebot "<task>" [--file <path>]...   Run a single task
  cat file | bravebot -p "<task>"        ...with piped input, never trusted
  bravebot --resume [id]                 Pick up a session in this directory
  bravebot --continue                    Pick up the most recent session in this directory
  bravebot --fork <id>                   Fork a session and start exploring a different path
  bravebot doctor                        Check configuration and confinement
  bravebot import-leo-creds [channel]    Import a Leo Premium subscription
  bravebot import-providers              Import a model service Claude Code or opencode configured
  bravebot mcp <command>                 Declare, list and approve MCP servers
```

## Commands

| Command | What it does |
|---|---|
| `bravebot` | start an interactive session in the current directory |
| `bravebot --plain` | start a session in lines, drawing nothing ([below](#--plain)) |
| `bravebot "<task>"` | run one task and print the reply |
| `bravebot --resume`, `-r` | choose a session in this directory to pick up |
| `bravebot --resume <id>` | resume that session by id |
| `bravebot --continue`, `-c` | pick up the most recent session in this directory |
| `bravebot --fork <id>`, `-f` | copy a session into one of its own and open that, to try a second approach |
| `bravebot doctor` | report configuration and confinement, changing nothing |
| `bravebot import-leo-creds [channel]` | import a Leo Premium subscription |
| `bravebot import-providers` | import a model service Claude Code or opencode configured, asking first |
| `bravebot mcp <command>` | declare, list, approve and remove MCP servers ([below](#mcp)) |
| `bravebot --version`, `-V` | print the build |
| `bravebot --help`, `-h` | print this |

Anything that is not a recognised flag or subcommand is treated as the task prompt.

## Options

| Option | What it does |
|---|---|
| `--file <path>` | include a workspace file as **trusted** context; repeatable |
| `--add-dir <path>` | make a directory outside the working one reachable for this run; repeatable ([below](#--add-dir-path)) |
| `-p`, `--print` | non-interactive; reads piped stdin as quarantined context |
| `--plain` | a session in lines, taking nothing from the terminal ([below](#--plain)) |
| `--mode <turn\|manifest>` | how a one-shot is run; `turn` (the default) decides step by step, `manifest` plans the whole run first ([below](#--mode-turnmanifest)) |
| `--model <name>` | the model this run asks for; outranks every other way one is named ([below](#--model-name)) |
| `--effort <level>` | how hard this run asks the model to think; outranks every other way one is named ([below](#--effort-level)) |
| `--settings <path>` | read one more settings file, above every layer found ([below](#--settings-path)) |
| `--json` | put one result object on stdout in the reply's place ([below](#--json)) |
| `--trace` | print the audit trail to stderr |
| `--vet` | let a check answer about a quarantined slot, for this run: it releases what it finds nothing in, and where nobody can be asked it keeps back everything else ([below](#--vet)) |
| `--incognito` | write nothing to `~/.bravebot`: no history, no session record, no preference |
| `--dangerously-skip-permissions` | bypass every permission check; recommended only for a sandbox with no internet access |
| `-h`, `--help` | show the help |
| `-V`, `--version` | show the version |

`-p` may lead, as it does for other agents: `bravebot -p "task"`.

Four flags are taken out of the line before anything dispatches on it, so each may go anywhere and
each combines with every way of starting, one another included: `--incognito`,
`--dangerously-skip-permissions`, `--settings` and `--vet`.

`--incognito` writes nothing under `~/.bravebot`. See
[an incognito session](../using/sessions.md#a-session-that-leaves-nothing-behind).

`--dangerously-skip-permissions` is the only way to reach the mode that answers every permission
question, including the ones that decide trust, and the only way a run nobody is watching may write.
See [modes](../security/permissions.md#answering-in-advance-modes) for what it costs.

## `--mode <turn|manifest>`

```sh
bravebot --mode manifest "add a --verbose flag and a test for it"
```

`turn`, the default, observes and decides step by step, which is what a plain `bravebot "task"` has
always been. `manifest` plans the whole run first and then executes it, with nothing re-planned. An
unknown name is refused rather than guessed.

**A `manifest` run asks you to approve the plan before the first step.** The plan is narrated a line
at a time, and the question that follows names the task in your own words, how many steps you are
approving, and what a yes does not cover. It is the one question a one-shot run asks, and it is put
only where both stdin and stderr are a terminal, since that is where whoever typed the command is
still whoever is reading the output. With either end piped or redirected there is nobody to ask, so
the run stops before its first step unless `--dangerously-skip-permissions` was given. A plan that
failed to build is printed on stderr even without `--trace`, and never shares stdout with the reply.

This is a different axis from the [permission mode](../security/permissions.md#answering-in-advance-modes):
`--mode` decides when control flow is settled, the other decides who answers a prompt, and the two
compose. See [Non-interactive use](../using/headless.md) and
[Sessions](../using/sessions.md) for the rest.

## `--model <name>`

```sh
bravebot --model opus "review the diff on this branch"
```

Names the model for one run, and outranks every other way one is named. Where no flag names one, a
run asks for the model a session opening in the same directory would: a
[`model`](../customize/configuration.md#model) key in the checkout's settings or the file
`--settings` names, then the choice [`/model`](../customize/configuration.md#choosing-a-model)
recorded, then the key in `~/.bravebot/settings.json`, then an exported
`BRAVE_AI_CHAT_DEFAULT_MODEL`, then the model the build was made with. So a script uses the model
you picked without your having to write it down twice, a checkout that names its model gets it, and
the flag names a different one for a single run.

`opus`, `sonnet` and `haiku` name a **tier** here, exactly as they do in a settings file, and resolve
the same way. Any other name is sent as you wrote it. `--model` with no name after it, or a blank
one, is refused and the run stops: a script that computed an empty variable asked for a model, and
answering it with whatever was configured is the substitution this flag exists to rule out.

**Where the server answers with a model other than the one in force, both names go to stderr**,
whether the flag, your recorded choice or a settings file named it. Where `--model` named it, the run
also **exits non-zero**, which is the part a script is certain to read. A run that named no model
takes whatever was recorded or configured and does not fail over it. Two cases are neither reported
nor failed: an entry that resolves per request, such as `automatic-bravebot`, and a backend asked by
an opaque handle, which never reports back the name it was given.

## `--effort <level>`

```sh
bravebot --effort high "why does this test fail only on the second run?"
```

Names how hard the model is asked to think for one run, in the words
[`/effort`](commands.md#effort-level) takes (`low`, `medium`, `high`, `xhigh` and `max`, in any
case), and outranks every other way one is named. Where no flag names one, a run asks for the level a
session opening in the same directory would: an [`effort`](../customize/configuration.md#effort) key
in the checkout's settings or the file `--settings` names, then the level `/effort` recorded, then the
key in `~/.bravebot/settings.json`. Nothing is recorded, so the next run is back to those.

`--effort` with no word after it, a blank one, or a word that is no level is refused and the run
stops, naming the levels it takes. A level the model in force reads none of is not sent, as it is
not from any other source.

## `--add-dir <path>`

```sh
bravebot --add-dir /srv/other-checkout "how does their error type differ from ours?"
```

Opens a directory outside the working one for the length of the run, and may be given more than once.
An absolute path outside the working directory is otherwise refused whatever else is true, so without
this a task pointed at one checkout cannot read another at all.

**It makes the directory reachable and vouches for nothing.** A file read there is quarantined on the
same terms as a file nobody vouched for, and a write there is refused as any write in an unattended
run is. A run nobody is watching holds no answer about the directory it works in, so a flag that
trusted the tree beside it would leave that tree better trusted than the project. The interactive
[`/add-dir`](../security/trust.md#add-dir) grants both halves, because a person typed it.

The path must be absolute, must exist, must be a directory, and must not already sit inside the
working one. Anything else is refused by name and the run stops before the turn, rather than failing
further in over a file it was told it could open.

## `--settings <path>`

```sh
bravebot --settings ./ci/bravebot.json -p "run the checks and summarise what failed"
```

Reads one more settings file for the length of the run, above the three
[Configuration](../customize/configuration.md) resolves. It resolves as those do, a name at a time,
so a file that sets one value leaves everything else a person and a checkout configured in force.

The three layers that are found are properties of a person, of a checkout and of a machine, and none
of them is a property of one invocation. This is how a CI job or somebody holding two accounts
configures one run differently from the next without editing the home directory or the checkout.

Given twice, the last file named is the one read, and naming a file that is already one of the three
reads it once. **A path naming no file, and a blank path, are refused by name and the run stops
before it starts**, because a run told to configure itself from a file is a run whose configuration is
that file: carrying on under whatever the directory happened to hold would be the wrong
configuration used in silence. A mistyped path and a variable that expanded to nothing look the same
from here.

What is checked is that the file is there. What is in it is read by the rule every layer gets, where
a file that is oversized or unparseable leaves the others in force, and `bravebot doctor` lists the
layers it read, so a named file that did not parse shows up by its absence from that list.

## `--json`

```sh
bravebot --json -p "what changed on this branch?" | jq -r '.reply'
```

Puts **one object, on one line, on stdout**, in the reply's place. It is written whether the run
finished, failed before the turn began, or was refused something along the way, so a caller never has
to tell an empty stdout from a result.

It holds how the run ended, the [status and identifier](#exit-codes), the message where there is one,
the reply, the model that answered, how many rounds it took, what it cost in tokens, every tool it
called with what it acted on and whether that call was refused, and every refusal with the principle
it upholds. A tool is named as the driver matched it rather than by the word you are shown on screen,
and what a call acted on is the name it was given rather than a resolved path.

Progress, the message and the audit trail stay on stderr, exactly as they do without the flag.

**It carries a schema number.** Within one number a field may be added and never removed, renamed or
given a different meaning, so a caller reading the fields it knows keeps working.

## `--plain`

```sh
bravebot --plain
```

An ordinary session with nothing drawn: the same turns, the same conversation carried between them,
the same trust map and the same questions. None of what the drawing interface takes from your
terminal is taken here, so what the terminal held before is where it stays, the session's own lines
are added to its scrollback, nothing is repainted, and nothing moves on its own.

A line you type is a prompt and Enter sends it. A blank line is not a prompt. **The end of the input
ends the session**, and it is the only way out: no chord is read, because none can be. The keyboard
is the terminal's, so its own interrupt ends the process.

Every question is put in lines: what it is about, then the question, then how to answer it. **Only the
affirmative approves**, and any other line refuses, as does the end of the input. One answer per
question and no second key, so the answers that grant something standing are not offered and nothing
answered here outlives the session.

stdin must be a terminal, and `--plain` is refused where it is not, naming `-p` as the invocation
that reads a pipe. It composes with `--incognito`, `--dangerously-skip-permissions` and `--settings`,
and with nothing else: it starts a session rather than describing one.

**What it does not have**, each being a thing the interface draws or a thing that needs what it
draws: the scroller and its search, the key list, the slash commands, `@` naming a file, a picture on
the clipboard, and the audit trail under a key. No session record is written either, so nothing picks
a session in lines up again and a later `--resume` will not find it.

:::caution
**A line typed before a question was asked is read as the answer to it.** The terminal queues what is
typed and this reads a line at a time, so somebody who pastes several lines at once has typed all of
them before anything asked them anything, and a question raised while those lines are still queued
takes the next one as its answer. Only the affirmative approves, so the line has to be exactly that
word for an effect to follow.
:::

## `--vet`

```sh
bravebot --vet -p "read the linked issue and tell me what it is asking for"
```

Turns auto-vetting on for the length of the run: where a check completes and finds nothing, the
quarantined slot the planner asked to be shown, or the output it asked to read back, is released
without a prompt. Given twice it is given once, which is asking for something already on rather than
an error.

A one-shot run has nobody to ask, so a check on this path would otherwise end in a refusal whatever
it found. The flag is what says in advance that a clean check may answer, and it is a narrower
statement than `--dangerously-skip-permissions`: it answers one question, about one slot at a time,
and only where a check completed and found nothing.

Paired with `--dangerously-skip-permissions` it is also what lets a check refuse. That mode releases
those two slots unshown on its own; with this flag the check's word is what answers instead, so a
verdict that objects, or a check that did not complete, keeps the bytes back and the model is told the
slot was kept from it. That is the way to screen a run nobody is watching. It costs a model call per
release, in the run's own critical path, and the bytes reach the backend to be checked whether or not
they are then released.

It outranks both standing answers, a recorded `off` included, because it is the narrowest in time.
There is no flag the other way: for one run without it, change the file the standing answer is kept
in. See [Vetting](../security/vetting.md).

## `doctor`

```sh
bravebot doctor
```

Answers "what will this actually use", and changes nothing. It reports:

- every backend this build can reach and what identifies it, which names the settings set;
- which settings files are in force, and which of them won a name more than one set;
- any settings file that tries to declare an [MCP server](../customize/mcp-servers.md), which fails
  the report, since only `~/.bravebot/mcp.json` declares one;
- which names a machine-level file pinned, and where that file is;
- how to configure a model service where nothing configured will serve a turn;
- the model in force, and whether it was chosen or defaulted;
- where the state directory is, or that there is none;
- what a TLS handshake is validated against, and what a request is routed through;
- the confinement available on this platform;
- the state of any imported subscription.

**No value from a settings file is ever printed.** Where a credential decides whether a backend
works, what is reported is that one was found, because a settings file holds credentials on some
machines and this is the report people paste into issues. The signing key is named as never
transmitted. The endpoint host and the key id are printed here and left off
[`/status`](commands.md#status).

The state directory is named with the variable that answered for it. Where there is none, the report
says which variables were looked at, what is not kept without one, and that a checkout's own settings,
skills and instructions are read regardless. On a platform where the files under it cannot be
restricted to one account, the report says which of them carry the permissions of the profile
directory instead. That is worth knowing before typing a token into a prompt, since the prompt history
is every path, branch name and pasted fragment somebody has typed.

**A configuration error makes it fail rather than pass with a warning.** Four things it can report are
errors of that kind: a named path that yielded no certificate, a set of trust roots that leaves
nothing trusted, a proxy named in a protocol this build cannot connect through, and a configuration
naming nothing that will serve a turn. A missing state directory is reported rather than failed on,
because a container or a daemon with no profile directory runs as designed.

In a Bravebot source checkout it also reports whether the root `AGENTS.md` resolves to
`agents/AGENTS.md` and whether `direnv` is executable on PATH. These are development advice, do not
change the exit status, and are shown in no ordinary workspace. Nothing is repaired: somebody runs
this to learn what is wrong, and a report that fixed what it found would leave them unable to tell
what was already true.

## `import-leo-creds`

```sh
bravebot import-leo-creds [stable|beta|nightly|development] [--forget]
```

Without a channel, `stable` is what importing means. `--forget` removes what was imported. See
[Leo Premium](../customize/premium.md).

## `import-providers`

```sh
bravebot import-providers
```

Reads what Claude Code and opencode configured in your home directory, shows what it would add to
`~/.bravebot/settings.json`, and asks once for each. It takes no arguments, needs a terminal to ask on,
and refuses in an incognito session. See
[Importing from Claude Code or opencode](../customize/configuration.md#importing-from-claude-code-or-opencode).

## `mcp`

```sh
bravebot mcp add <alias> [--env <name>]... [--dir <path>] --stdio -- <program> [args...]
bravebot mcp add <alias> --http <url>
bravebot mcp get <alias>
bravebot mcp list
bravebot mcp approve <alias>
bravebot mcp remove <alias>
bravebot mcp forget [path]
```

Declares a server in `~/.bravebot/mcp.json`, and approves one. `add` writes the declaration and then
asks whether to use it; `approve` asks again later. The question is put only where stdin and stdout
are both a terminal, and only `y` approves. `approve` with nobody to ask, or answered with anything
else, exits 4. `list` exits 3 where a declaration in the file cannot be used. `list` and `get` name
the machine-level file beside a server it
[refuses](../customize/mcp-servers.md#refused-by-an-administrator), and why. `forget` drops the
standing answers recorded for a project, the current directory unless a path is given, so its
servers and tools are asked about again. `add`, `approve`, `remove` and `forget` are refused in an
incognito session. See [MCP servers](../customize/mcp-servers.md).

## Interactive keys

| Key | What it does |
|---|---|
| Enter | Send |
| Shift-Enter, Ctrl-J | New line without sending |
| Ctrl-G | Compose in `$VISUAL` or `$EDITOR` |
| Ctrl-S | Stash the line, or bring back the stashed one |
| Ctrl-V | Paste, including screenshots |
| Shift-Tab | Choose how much the session asks before it acts |
| Ctrl-T | Toggle the audit trail |
| Ctrl-O | Open the scroller over the transcript |
| Ctrl-L | Watch what a delegate is doing, and read what a command printed |
| Up / Down | Walk back through sent prompts |
| Ctrl-R | Search every prompt you have sent |
| Wheel, PageUp / PageDown | Scroll the transcript |
| Home / End | Jump to the start or the latest |
| Esc | Cancel a running turn, or clear the input |
| Ctrl-C | Stop the nearest thing; leave when there is nothing left |
| Ctrl-D | Leave |
| `?` | List every key, on an empty line |

**Seven of these are defaults you can move**: the external editor, watching a delegate, the scroller,
the history search, the stash, the audit trail and paste. A `keybindings` block in `settings.json`
names an action and the chord it should answer, and the key list on screen reflects what you set. Four
chords are reserved and cannot be taken: Ctrl-C, Ctrl-D, Ctrl-J and Shift-Enter. See
[Configuration](../customize/configuration.md) for the `keybindings` block.

The full behaviour is in [Interactive mode](../using/interactive-mode.md), and the scroller's own keys
are in [Reading the transcript](../using/transcript.md#the-scroller).

## Interactive commands

| Command | What it does |
|---|---|
| `/status` | Report this session, what it may touch, and what it has spent |
| `/cost` | Show what each turn of this session has spent |
| `/model` | Choose which model to think with |
| `/theme [name]` | Choose which theme paints the interface |
| `/effort [level]` | Choose how hard to think before answering |
| `/config` | Choose how the input box edits text |
| `/add-dir <path>` | Open another directory, and trust it for this session |
| `/cd <path>` | Work in another directory from now on, and trust it for this session |
| `/rename <name>` | Call this conversation something else |
| `/compact` | Summarise the conversation so far, keeping the recent part |
| `/btw <question>` | Ask something beside the work, without putting it in the conversation |
| `/clear` | Start a new session here, keeping this one resumable |
| `/loop [interval] <prompt>` | Send a prompt again and again, on your interval or at a pace each turn sets |
| `/goal [<condition> \| clear]` | Keep working until a condition you set is judged met |
| `/watch [stop <n>]` | List the files this session is watching, and stop one by its number |
| `/manifest <task>` | Plan one task in full, show you the plan, then run it with nothing re-planned |
| `/export [path]` | Export the session transcript to a markdown file |
| `/undo` | Rewind one turn and put back the files it wrote |
| `/rewind [turns]` | List the turns a rewind could go back to, or go back that many |
| `/exit` | Leave |
| `@<path>` | Include a workspace file as trusted context |
| `!<line>` | Run a line in your own shell |

See [Slash commands](commands.md).

## The version string

```
bravebot 0.9.0 (f2a6e1a, modified)
```

The commit is what the binary was compiled from, and `modified` means the tree had uncommitted changes
at that point. A build with no git available says `(no git)` rather than naming a commit it cannot see.

Every session record carries the same string. That matters when reading a transcript back: a session
that behaved oddly is usually being read against code that has moved since.

## Exit codes

A failure exits non-zero rather than exiting successfully with an explanation on stdout, and **each
kind of failure has a status of its own**, because "the endpoint was not there, try again", "the
configuration is wrong, fail the build" and "a gate refused the write, this needs a person" are three
different things to do about a failed run.

| Status | Identifier | The run |
|---|---|---|
| 0 | | did what it was asked |
| 1 | `BB1001` | failed for a reason none of the others name |
| 2 | `BB1002` | refused an argument, so nothing ran |
| 3 | `BB1003` | cannot use the configuration, so nothing ran |
| 4 | `BB1004` | had an effect refused by a gate |
| 5 | `BB1005` | never reached the backend |

Only the transport's own failures are status 5. A non-success answer from the service is the service
answering, so a refused credential is a configuration problem rather than a connection to retry.

**A status is never renumbered and never given a second meaning.** A failure kind nothing here names
is 1, and one worth telling apart takes the next number.

The identifier is printed in front of the message on stderr, never instead of it, and is the same
whatever language the message is in. A sentence in the reader's own language is the right thing to
print and the wrong thing to search for: pasted into a bug report it reaches somebody who cannot grep
it.

A run whose [`--model`](#--model-name) was substituted by the server also exits non-zero.

## Streams

Stdout carries the reply and nothing else. Progress, errors and the audit trail go to stderr, so a
one-shot run is pipeable.

## Limits

| | |
|---|---|
| piped input | 10 MiB, refused rather than truncated past that |
| a pasted picture | 10 MB |
| tool rounds in one turn | unbounded interactively; 200 for a one-shot or manifest run, after which the planner answers with what it has |
| context budget | the window your model advertises; 24,000 prompt tokens where it advertises none |

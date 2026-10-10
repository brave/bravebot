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
  bravebot --from-pr <number|url>        Pick up a session linked to a pull request
  bravebot --continue                    Pick up the most recent session in this directory
  bravebot -p "<task>" --resume <id>     Send a one-shot task as the next turn of a session
  bravebot -p "<task>" --continue        Send a one-shot task as the next turn of the most recent session
  bravebot --fork <id>                   Fork a session and start exploring a different path
  bravebot doctor                        Check configuration and confinement
  bravebot doctor --sandbox-check              Run everyday workflows under the sandbox and report which work
  bravebot bug-report                    Write the version, what doctor reports and the newest log's name to a file to attach to a bug report
  bravebot update                        Print the command that updates this copy
  bravebot auth login [way]              Sign in to a model service, listing every way when none is named
  bravebot auth logout <way>             Forget an imported Leo Premium subscription or a stored gateway key
  bravebot auth status [way]             Say whether a sign-in is usable, exiting 0 only if it is
  bravebot import-leo-creds [channel]    Import a Leo Premium subscription
  bravebot import-providers              Import a model service Claude Code or opencode configured
  bravebot mcp <command>                 Declare, list and approve MCP servers
  bravebot permissions check <call>      Report which permission rule decides a tool call
  bravebot completion <bash|zsh|fish>    Print a shell completion script
  bravebot shell-init <bash|zsh|fish>    Print the shell hook that gives @bravebot the commands you ran
```

## Commands

| Command | What it does |
|---|---|
| `bravebot` | start an interactive session in the current directory |
| `bravebot --plain` | start a session in lines, drawing nothing ([below](#--plain)) |
| `bravebot "<task>"` | run one task and print the reply |
| `bravebot --resume`, `-r` | choose a session in this directory to pick up |
| `bravebot --resume <id>` | resume that session by id |
| `bravebot --from-pr <number or url>` | choose among the sessions linked to that pull request |
| `bravebot --continue`, `-c` | pick up the most recent session in this directory |
| `bravebot -p "<task>" --resume <id>` | run the task as one more turn of that session, without opening it ([below](#continuing-a-session-from-a-one-shot-run)) |
| `bravebot -p "<task>" --continue` | the same, for the most recent session in this directory |
| `bravebot --fork <id>`, `-f` | copy a session into one of its own and open that, to try a second approach |
| `bravebot doctor` | report configuration and confinement, changing nothing |
| `bravebot doctor --sandbox-check` | run `git`, `cargo`, `npm` and the other everyday programs under the sandbox and report which work |
| `bravebot bug-report` | write a text file to attach to a bug report ([below](#bug-report)) |
| `bravebot update` | print the command that updates this copy, and run nothing ([below](#update)) |
| `bravebot auth login [way]` | sign in to a model service, listing the ways when none is named ([below](#auth)) |
| `bravebot auth logout leo` | forget an imported Leo Premium subscription |
| `bravebot auth logout gateway [id]` | forget a gateway key `auth login gateway` stored |
| `bravebot auth status [leo\|bedrock\|gateway [id]]` | say whether a sign-in is usable, exiting 0 only if it is ([below](#auth)) |
| `bravebot import-leo-creds [channel]` | import a Leo Premium subscription |
| `bravebot import-providers` | import a model service Claude Code or opencode configured, asking first |
| `bravebot sessions import claude-code` | copy Claude Code's sessions for this directory in as words to read ([Sessions](../using/sessions.md#importing-sessions-from-claude-code)) |
| `bravebot sessions search [workspace:<dir>] [since:<n>h\|d\|w] <text>` | print the ids and titles of past sessions that said it ([Sessions](../using/sessions.md#picking-one-back-up)) |
| `bravebot mcp <command>` | declare, list, approve and remove MCP servers ([below](#mcp)) |
| `bravebot permissions check <call>` | say which permission rule decides a tool call, and the file it is in ([below](#permissions-check)) |
| `bravebot completion <shell>` | print a completion script for `bash`, `zsh` or `fish` ([below](#completion)) |
| `bravebot shell-init <shell>` | print a shell hook that gives `@bravebot` the commands you ran ([below](#shell-init)) |
| `bravebot --version`, `-V` | print the build |
| `bravebot --help`, `-h` | print this |

Anything that is not a recognised flag or subcommand is treated as the task prompt.

## Options

| Option | What it does |
|---|---|
| `--file <path>` | include a workspace file as **trusted** context; repeatable |
| `--session-ref <id>` | add the newest part of an earlier session of this directory to the task; repeatable |
| `--add-dir <path>` | make a directory outside the working one reachable for this run; repeatable ([below](#--add-dir-path)) |
| `--trust-workspace` | trust the working directory for this run, writing no record ([below](#--trust-workspace)) |
| `-p`, `--print` | non-interactive; reads piped stdin as quarantined context |
| `--plain` | a session in lines, taking nothing from the terminal ([below](#--plain)) |
| `--mode <turn\|manifest>` | how a one-shot is run; `turn` (the default) decides step by step, `manifest` plans the whole run first ([below](#--mode-turnmanifest)) |
| `--model <name>` | the model this run asks for; outranks every other way one is named ([below](#--model-name)) |
| `--advisor <name>` | a model the planner may put a question to, through the `advisor` tool ([below](#--advisor-name)) |
| `--effort <level>` | how hard this run asks the model to think; outranks every other way one is named ([below](#--effort-level)) |
| `--settings <path>` | read one more settings file, above every layer found ([below](#--settings-path)) |
| `--output-schema <path>` | hold the reply to the JSON Schema in this file, or end on status 6 ([below](#--output-schema-path)) |
| `--run-network <open\|closed>` | `closed` takes the network from a program the agent runs unless it needs to fetch or reach a remote ([`run.network`](../customize/configuration.md#runnetwork)) |
| `--sandbox-allow-read <path>`, `--sandbox-deny-read <path>`, `--sandbox-allow-write <path>`, `--sandbox-deny-write <path>` | add a path or glob to one of the four lists that move what a program the agent runs reads and writes; repeatable ([`sandbox.filesystem`](../customize/configuration.md#sandboxfilesystem)) |
| `--log-level <error\|info\|debug>` | how much of a failure's shape goes to the [diagnostic log](#--log-level-errorinfodebug) |
| `--agent <name>` | address every turn to one of your definitions, as `/agent` does for one ([below](#--agent-name)) |
| `--system-prompt <prompt>` | replace the opening of the system prompt for this run; the rest of it stays ([below](#--system-prompt-prompt-and---append-system-prompt-prompt)) |
| `--append-system-prompt <prompt>` | add your own words to the system prompt for this run, after the project's `AGENTS.md` ([below](#--system-prompt-prompt-and---append-system-prompt-prompt)) |
| `--json` | put one result object on stdout in the reply's place ([below](#--json)) |
| `--json-stream` | write one event per line as the run goes, then the result object ([below](#--json-stream)) |
| `--trace` | print the audit trail to stderr |
| `--vet` | let a check answer about a quarantined slot, for this run: it releases what it finds nothing in, and where nobody can be asked it keeps back everything else ([below](#--vet)) |
| `--tools <a,b,c>` | offer the agent only the named tools; a tool a setting removed stays removed, and no MCP server's tools are offered ([below](#--tools-abc-and---no-shell)) |
| `--no-shell` | offer no tool that runs a program or reads what one printed: `run`, `read_output` and `job_output` ([below](#--tools-abc-and---no-shell)) |
| `--locked` | as `--safe`, and read no project or local settings file; refuse `--dangerously-skip-permissions` and make the bypass mode unreachable ([below](#--locked)) |
| `--safe` | load none of your hooks, skills, definitions, MCP server requests or `AGENTS.md`, and say so once; sign-in, model and permissions still apply |
| `--incognito` | write nothing to `~/.bravebot`: no history, no session record, no preference |
| `--dangerously-skip-permissions` | bypass every permission check; recommended only for a sandbox with no internet access |
| `-h`, `--help` | show the help |
| `-V`, `--version` | show the version |

`-p` may lead, as it does for other agents: `bravebot -p "task"`.

Eight flags are taken out of the line before anything dispatches on it, so each may go anywhere and
each combines with every way of starting, one another included: `--incognito`, `--safe`, `--locked`,
`--dangerously-skip-permissions`, `--settings`, `--vet`, `--tools` and `--no-shell`. `--agent` is taken out there too, and
combines with a session, `--plain` and a one-shot run, but not with `--resume`, `--continue`,
`--fork` or `--mode manifest`, a task carrying on a session included.
`--system-prompt` and `--append-system-prompt` are taken out there as well, and combine with every
way of starting except `--mode manifest`.

`--incognito` writes nothing under `~/.bravebot`. See
[an incognito session](../using/sessions.md#a-session-that-leaves-nothing-behind).

`--safe` is for finding out whether your own configuration is why a session misbehaves. The run
reads none of your hooks, skills, delegate definitions, `mcp.request` entries or `AGENTS.md` files,
whether they are under `~/.bravebot` or in the project, and says once what it skipped. The built-in
skills and delegate kinds, `--append-system-prompt` and `--settings` are unaffected, and so are
sign-in, the model, permission rules and trust. `--bg` refuses it.

### `--locked`

For a run on a shared machine, or one whose checkout you do not trust. It does what `--safe` does,
and reads settings from only your own file under `~/.bravebot`, the managed file and the one
`--settings` names: the checkout's `.bravebot/settings.json` and `settings.local.json` are not read.
It refuses `--dangerously-skip-permissions`, and the bypass mode cannot be reached from the keyboard
either. A permission question nobody can answer is declined, as it is in any unattended run. `--bg`
refuses it.

### `--tools <a,b,c>` and `--no-shell`

```sh
bravebot -p "summarise this repository" --tools read_file,list_files,search
bravebot -p "fix the failing test" --no-shell
```

`--tools` offers the agent the tools it names and no others, for every turn of the run and for every
delegate it starts. `--no-shell` takes away `run`, `read_output` and `job_output`. A name that is no
tool is refused with the list of those that are. Both only remove: a tool your settings already
removed is not offered because a flag names it, and a call to a tool taken away is refused as an
unknown name even with `--dangerously-skip-permissions`. `--tools` offers no MCP server's tools.
`--mode manifest`, `--bg` and the commands that start no session refuse them.

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
`BRAVEBOT_DEFAULT_MODEL`, then the model the build was made with. So a script uses the model
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

## `--advisor <name>`

```sh
bravebot --advisor opus "plan the migration, then carry it out"
```

Names a second model the planner may consult during the run. The planner is then offered an
[`advisor`](tools.md#advisor) tool, which sends the conversation so far and a question the planner
wrote to that model and returns its reply. Without the flag the model is the one the
[`advisorModel`](../customize/configuration.md#advisormodel) setting names, and with neither there is
no such tool. The flag wins over the setting. The name is read as `--model` reads it, so `opus`,
`sonnet` and `haiku` name a tier.

`--advisor` with no name, a blank one, a model this machine's managed settings refuse, a model
nothing is configured to serve, or `--mode manifest` is refused and the run stops before it starts.
A turn may ask its advisor at most three times. The advisor's tokens are added to the run's total.

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

## `--trust-workspace`

```sh
bravebot --trust-workspace -p "summarise what src/ does"
```

Starts the run from what a yes to the startup question writes: the working directory and everything
beneath it is trusted, so the planner can read project files. Nothing is written to
`~/.bravebot/trusted`, and directories opened with `--add-dir` stay untrusted. A run carrying on an
earlier session keeps that session's trust and adds the working directory to it.

It is a flag and nothing else: no settings file, environment variable or file in the directory turns
it on. A run in a directory about which a session kept an answer (`r`), or inside the git worktree whose
root it was kept about, opens trusting the directory without the flag and says so on stderr.

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

## `--agent <name>`

```sh
bravebot --agent rule-reviewer
bravebot --agent rule-reviewer -p "review the diff on this branch"
```

Addresses every turn of a session, a session in lines or a one-shot run to one of your
[definitions](../customize/agents.md), from the first turn on. [`/agent`](commands.md#agent-name-task)
does the same for a single line. Typed lines, `/loop` ticks and `/goal` rounds are all addressed to
it. A `/agent` line naming another definition addresses that one for one turn, and the next line
goes back to the one you started with. Replies are drawn under its name, and `/status` shows it.

The name is checked before anything is sent. In a session, that happens after you answer the
question about the directory, because the set of definitions depends on the answer. **A one-shot run
reads only your own definitions**, in `~/.bravebot/agents`. It does not ask whether to trust the
checkout, so it counts the definitions in the checkout's `.bravebot/agents` without reading them,
and a name that only the checkout defines is refused with a message saying so. A name no definition
has is refused with the list of names that exist. A definition whose model needs a sign-in this
machine has not made is also refused. Each refusal exits with status 2.

If `--agent` is given twice, the last name is used. A blank name, or one opening with `-`, is
refused.

The session record keeps the name. `--resume`, `--continue`, `--fork`, `--from-pr`, `/resume` and a
`-p` run carrying a session on all work under the definition the session was started under, so
`bravebot --resume <id>` needs no `--agent`. A `--agent` given with one of them names the
definition for that session instead. If the recorded definition no longer exists, the session opens
without it, says which definition is gone and that the narrowing is gone with it, and stops
recording the name. A recorded definition whose model needs a sign-in is refused rather than
replaced by the planner.

To start every session under one definition without typing the flag, set [`agent`](../customize/configuration.md#agent)
in your settings. `--agent` outranks it, and so does the name a resumed session's record carries.

A model the definition names is the one every turn uses. In a session, `/status` shows it as the
definition's, and `/model` is refused. On a one-shot run, `--model` outranks the definition's model,
and the run says so on stderr.

:::note
Under `--agent`, a `/loop` with no interval stops after one tick, because an addressed turn cannot
schedule the next one. Give the loop an interval to keep it running.
:::

## `--system-prompt <prompt>` and `--append-system-prompt <prompt>`

```sh
bravebot --append-system-prompt "Answer in French." -p "summarise the last commit"
bravebot --system-prompt "You are a release-notes editor." -p "draft the notes"
```

Puts your own words in the system prompt of one run. They apply to every turn of it: typed lines,
`/loop` ticks and `/goal` rounds.

`--append-system-prompt` adds the words as the last standing source, after the project's
`AGENTS.md`, so they have the last word ([Instructions](../customize/instructions.md#words-from-the-command-line)).
`--system-prompt` replaces the opening of the system prompt, the paragraph saying what kind of
assistant this is. **It does not replace the rest.** What teaches the planner that a tool's output
is data, the facts about your machine, the mode and the goal are still sent. A delegate is given the
appended words and never the replaced opening. The aside, the summary, the goal check and the other
checks are given neither.

The words grant nothing. A write is still put to you where it would have been, and plan mode still
refuses it. A session record does not store them, so resuming without the flag runs without them.

**The words carry your authority, whatever they came from.** `--append-system-prompt "$(cat x)"`
gives the file's bytes the same standing as something you typed. Content that should not have that
standing is better piped in, where it is quarantined.

A flag with no words after it, a blank one, or one whose words open with `-` and hold no space is
refused with status 2. A sentence opening with `-` is words. If a flag is given twice, the last is
used. Both are refused with `--mode manifest`, and with `doctor`, `bug-report`, `auth`, `mcp`, `permissions`,
`import-leo-creds`, `import-providers`, `completion` and `shell-init`. There is no settings key for them: words a checkout
always wants belong in its `AGENTS.md`.

## `--json`

```sh
bravebot --json -p "what changed on this branch?" | jq -r '.reply'
```

Puts **one object, on one line, on stdout**, in the reply's place. It is written whether the run
finished, failed before the turn began, or was refused something along the way, so a caller never has
to tell an empty stdout from a result.

It holds how the run ended, the [status and identifier](#exit-codes), the message where there is one,
the reply, the model that answered, the definition it was addressed to under `agent` (`null` where
none was), the session it wrote down under `session` (`null` where it wrote none), how many rounds it took, what it cost in tokens, every tool it called with what it acted
on and whether that call was refused, and every refusal with the principle it upholds. A tool is named as the driver matched it rather than by the word you are shown on screen,
and what a call acted on is the name it was given rather than a resolved path.

Progress, the message and the audit trail stay on stderr, exactly as they do without the flag.

**It carries a schema number.** Within one number a field may be added and never removed, renamed or
given a different meaning, so a caller reading the fields it knows keeps working.
`structured` is the reply as a JSON value when the run was given [`--output-schema`](#--output-schema-path),
and `null` otherwise.

## `--output-schema <path>`

```sh
bravebot --json --output-schema ./verdict.json -p "do the tests cover the parser?" | jq '.structured.verdict'
```

Reads a JSON Schema from the file, sends it to the service with the run's requests as the shape the
reply must take, and checks the finished reply against it before anything is written. The file is
yours, trusted as [`--settings`](#--settings-path) is, and the model never sees its path.

A reply that conforms is written as usual, and under `--json` it is also in `structured` as a JSON
value rather than a string. A reply that does not conform ends the run on status 6 (`BB1006`):
nothing goes to stdout, `structured` is `null`, and the message on stderr says where the reply broke
and which constraint, for example `$.verdict` and an enum. The reply is read as one JSON value with
nothing around it, so a reply wrapped in a code fence does not conform.

The check covers `type`, `enum`, `const`, `properties`, `required`, `additionalProperties` as `true`
or `false`, `items` as one schema, `minItems`, `maxItems`, `minLength`, `maxLength`, `minimum` and
`maximum`. `$schema`, `$id`, `title`, `description`, `default` and `examples` are accepted and
constrain nothing. Any other keyword, `$ref`, `oneOf` and `pattern` included, is refused when the
file is loaded rather than ignored.

The flag is refused with status 2 before anything is sent when no path follows it, when the file
cannot be read or is not a usable schema, with `--mode manifest`, and when the model cannot honour a
schema: Bedrock, or a model whose listing states its supported parameters without `structured_outputs`
or `response_format`. A model whose listing states none is asked, and the check after the turn is
what holds the reply there.

## `--json-stream`

```sh
bravebot --json-stream -p "fix the failing test" | jq -c 'select(.event == "refusal")'
```

A `--json` run that also writes **one object per line on stdout as the run goes**, so a script can
show progress, tell a hung run from a slow one, and keep what happened if the process is killed.
Three kinds of event are written, each with `schema` and `event`:

| `event` | written when | other fields |
|---|---|---|
| `call` | a tool call finishes | `tool`, `target`, `refused`, as in the result object's `calls` |
| `refusal` | a gate refuses something | `gate`, `principle`, `reason`, as in the result object's `refusals` |
| `usage` | the run's token usage changes, which is when a model request finishes | `tokens`, the cumulative counts |

The last line is the result object, exactly what `--json` writes for the same run. It has no `event`
field. Nothing else is on stdout, and progress, the message and the audit trail stay on stderr. The
schema rule of `--json` applies to each event: fields and event kinds may be added within one number,
and none is removed or renamed.

## Continuing a session from a one-shot run

```sh
id=$(bravebot --json -p "review the diff" | jq -r '.session')
bravebot -p "now fix what you found" --resume "$id"
bravebot -p "summarise what changed" --continue
```

A one-shot run is written down as a session of one turn, unless `--incognito` is given, and its
[`--json`](#--json) object names the record under `session`. A run that failed writes none, and
`session` is `null`.

`--resume <id>` and `--continue` given with a task run that task as one more turn over the
session's conversation, in either order with `-p`, and write the record back with the turn added. The
run is a one-shot run in every other respect: nothing is asked, a write is refused unless
`--dangerously-skip-permissions` was given, and `--incognito` leaves the record as it was. The paths
you vouched for in the session still count. The commands you vouched for do not, and stay in the
record for the next time you resume it interactively. Content a run could not show the planner, such
as piped input, stays a reference, and the follow-up is told the reference names nothing now.

Given with no task, `--resume` and `--continue` open the session as they always have.

Both are refused with the argument status before anything is sent when the id names no session in
this directory, when `--continue` finds none, when the session is a `--mode manifest` run or is
running in the background, and with `--agent` or `--mode manifest`.

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
answered here outlives the session. A directory you told the drawing interface or the desktop app to
[remember](../security/trust.md#remembering-the-answer) is not asked about here either; the session says it is trusting it
for that reason and names the file that holds the answer: delete the lines about the directory from
it to be asked again.

stdin must be a terminal, and `--plain` is refused where it is not, naming `-p` as the invocation
that reads a pipe. It composes with `--incognito`, `--dangerously-skip-permissions`, `--settings` and
`--agent`, and with nothing else: it starts a session rather than describing one.

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

## `--log-level <error|info|debug>`

Brave Bot keeps a diagnostic log you can attach to a bug report. It records the shape of a failure:
the host a request went to, the status it came back with, how many attempts were made, and whether
a language server or an MCP server started. It never records a prompt, a reply, a file's contents,
a header, a credential or the text of an error.

Each run that has something to write makes its own file in `logs/` in the [state
directory](#doctor), readable by you alone, and the ten most recent are kept. At the default level,
`error`, a run that fails nowhere leaves no file. `info` adds the steps taken, such as a retry or a
server starting, and `debug` adds the detail between them. `doctor` prints the directory.

An [`--incognito`](../using/sessions.md) session writes no log.

## `bug-report`

```sh
bravebot bug-report
```

Writes `bravebot-bug-report.txt` in the current directory, readable by you alone, and prints its
path. A file of that name is never overwritten: the next report is `bravebot-bug-report-1.txt`.

The file holds the version and target, everything `doctor` prints, and the path of the newest
[diagnostic log](#--log-level-errorinfodebug). The log's own lines are not copied in, so attach
that file as well. `doctor` names paths, model services and the names, never the values, of
environment variables, so read the file before posting it. No transcript, trace, settings file,
environment value or file of the directory holds a place in it.

Nothing reads the file back. An [`--incognito`](../using/sessions.md) session, or a machine with no
home directory, writes no report, and the command exits with status 1. Any argument is refused with
status 2.

## `update`

```sh
bravebot update
```

Prints the command that updates this copy, and runs nothing. Two installations have a command it
can name:

| How this copy was installed | What it prints |
|---|---|
| the npm package `@brave/bravebot` | `npm install -g @brave/bravebot@latest` |
| `install.sh` | the same `curl` line that installed it, piped to `sh` |

A copy neither of those put here, a build from source above all, is told there is no update command
for it; that is a success, since nothing on the machine is wrong. The two lines are not
interchangeable: the npm one installs a package manager's copy, which leaves a script install's
binary untouched, and the script's one writes over the path the script recorded.

It asks nothing of any registry, so the command is the same whether or not a newer version has been
published and no version is named. Whether one is out is said once at the top of an interface
session instead. Any argument is refused with status 2.

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
- for each AWS account, whether it is signed in, and `bravebot auth login bedrock` where signing in
  is what is missing;
- which settings files are in force, and which of them won a name more than one set;
- any top-level key in a settings file that this build does not read, by name and file, never its
  value: the file still applies and the report still passes;
- any project or local settings file whose `provider` block or `model` key was ignored, since those two
  are read from your own file and the one `--settings` names;
- whether a gateway key is stored by `bravebot auth login gateway`, without printing it;
- any settings file that tries to declare an [MCP server](../customize/mcp-servers.md), which fails
  the report, since only `~/.bravebot/mcp.json` declares one;
- which names a machine-level file pinned, and where that file is;
- how to configure a model service where nothing configured will serve a turn;
- the model in force, and whether it was chosen or defaulted;
- where the state directory is, or that there is none, and the diagnostic logs in it;
- what a TLS handshake is validated against, and what a request is routed through;
- the confinement available on this platform;
- the state of any imported subscription.

```sh
bravebot doctor --sandbox-check
```

Runs everyday workflows under the sandbox the way a person's shell command would: `git` init, commit
and branch, a script, `make`, `cargo` build and test, `npm`, Python, Go, `gh` offline and an editor.
Each runs on a throwaway account that holds a credential of each kind. It also runs the rows that must stay
refused, a read of an SSH key or AWS credentials and a write beside or outside the session. A workflow that works
without the sandbox and fails with it is reported with its stage, its exit code, a fix and a log path. A
program that is not installed is reported as skipped, by name, and does not fail the report. It exits
non-zero when a workflow fails or a refused row gets through. When `gh` holds a login in your own
home, it also checks that `gh` can still read that login under the sandbox, and the fix for a failure
names `sandbox.filesystem.allowRead`. It takes no further argument.

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
because a container or a daemon with no profile directory runs as designed. The report ends on the
configuration error and exits with status 3, printing `BB1003` in front of it as any
[failed run](#exit-codes) does.

In a Bravebot source checkout it also reports whether the root `AGENTS.md` resolves to
`agents/AGENTS.md` and whether `direnv` is executable on PATH. These are development advice, do not
change the exit status, and are shown in no ordinary workspace. Nothing is repaired: somebody runs
this to learn what is wrong, and a report that fixed what it found would leave them unable to tell
what was already true.

## `auth`

```sh
bravebot auth login [leo [channel] | bedrock | import | gateway [id]]
bravebot auth logout leo
bravebot auth logout gateway [id]
bravebot auth status [leo | bedrock | gateway [id]]
```

With no way named, `auth login` lists the ways to sign in, marks the ones already in use, and asks
which to run. It needs a terminal for that. A script names the way instead:

| Way | What it runs |
|---|---|
| `leo [channel]` | what `import-leo-creds [channel]` runs |
| `bedrock` | the AWS sign-in a session would make on its first turn, for every account the configuration names, then sets `BRAVEBOT_USE_BEDROCK` to `1` in `~/.bravebot/settings.json` once the `AWS_PROFILE` account signs in, unless that file names it, something turns Bedrock off, or the session is incognito |
| `import` | what `import-providers` runs |
| `gateway [id]` | asks for the key of a gateway a provider block names, with nothing shown as it is typed, and keeps it in `~/.bravebot/gateway-keys.json` |

`gateway` needs a terminal even with the id named, since the key is typed rather than given as an
argument. With one gateway configured it asks for no id.

`auth logout leo` runs `import-leo-creds --forget`. `auth logout gateway [id]` forgets a stored key,
and the id can be left off when only one is stored. In an incognito session the `leo`, `import` and
`gateway` ways are refused, and `bedrock`, `auth status` and both forms of `auth logout` are allowed. See
[Signing in](../customize/signing-in.md).

`auth status` prints one line per sign-in and changes nothing:

```text
$ bravebot auth status
leo: signed in: production subscription imported, 12 of 30 credentials unspent
bedrock: not signed in: no AWS account is configured for Bedrock: ...
gateway openrouter: signed in: stored by bravebot auth login gateway (never printed)
```

A line says signed in, not signed in (signing in fixes it), or unusable (it does not: a stored
subscription for another Brave channel, an unreadable file, a missing `aws`), with the reason and
what to run. Only counts and fixed sentences are printed, never a credential. With no way named it
exits 0 if any sign-in is usable. With a way named it exits 0 only if every sign-in under it is
usable. Otherwise it prints a `BB1001` line and exits 1, or exits 3 when the configuration could not
be read. `auth status import` is refused, since an import keeps no sign-in.

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

Reads what Claude Code and opencode configured in your home directory, and asks a running Ollama on
this machine what it serves, shows what it would add to `~/.bravebot/settings.json`, and asks once for
each. It takes no arguments, needs a terminal to ask on, and refuses in an incognito session. See
[Importing from Claude Code, opencode or Ollama](../customize/configuration.md#importing-from-claude-code-opencode-or-ollama).

## `mcp`

```sh
bravebot mcp add <alias> [-s <scope>] [-e|--env <name>[=<value>]...]... [--dir <path>] [--startup-timeout <seconds>] [--tool-timeout <seconds>] [--stdio] -- <program> [args...]
bravebot mcp add <alias> [-s <scope>] [--startup-timeout <seconds>] [--tool-timeout <seconds>] --http <url>
bravebot mcp get <alias>
bravebot mcp list
bravebot mcp approve <alias>
bravebot mcp enable <alias> [-s <scope>]
bravebot mcp disable <alias> [-s <scope>]
bravebot mcp remove <alias>
bravebot mcp forget [path]
```

Declares a server in `~/.bravebot/mcp.json`, approves one, and
[asks for one](../customize/mcp-servers.md#asking-for-one-from-a-checkout) in a settings file.
`add` writes the declaration and then asks whether to use it; `approve` asks again later. `enable`
asks where the server is not approved and adds it to `mcp.request`, and `disable` takes it out. `-s`
names the file for `add`, `enable` and `disable`: `local`, the default, is
`.bravebot/settings.local.json` in the current directory, `project` is `.bravebot/settings.json`
there, and `user` is `~/.bravebot/settings.json`. Without `-s`, `disable` takes the server out of
each of the three. The question is put only where stdin and stdout are both a terminal, and only
`y` approves. `approve` and `enable` with nobody to ask, or answered with anything else, exit 4.
`list` exits 3 where a declaration in the file cannot be used. `list` and `get` name the
machine-level file beside a server it
[refuses](../customize/mcp-servers.md#refused-by-an-administrator), and why. `forget` drops the
standing answers recorded for a project, the current directory unless a path is given, so its
servers and tools are asked about again. `add`, `approve`, `enable`, `disable`, `remove` and
`forget` are refused in an incognito session. See [MCP servers](../customize/mcp-servers.md).

## `permissions check`

```sh
bravebot permissions check Bash git push origin main
bravebot permissions check Read src/secret.env
bravebot permissions check WebFetch https://api.example.com/v1
bravebot permissions check Mcp weather:get_forecast
```

Loads the [`permissions` rules](../customize/configuration.md#permissions) as a session started in
this directory would and says which of deny, ask or allow decides the call, which rule, and which
settings file wrote it. The first of deny, then ask, then allow that matches decides, however
specific a later rule is. `Read` and `Edit` take one path, `Bash` a program and its arguments as
separate words, `WebFetch` a host or a URL (only the host is matched) and `Mcp` a `server:tool`.

```text
decision: ask
rule: Bash(git commit *)
file: /work/app/.bravebot/settings.json
```

When no rule matches, it says so and the ordinary approval gates decide. An `allow` rule from a
checkout's settings is not in force until you grant it at the question a session asks, so it is
never reported as the rule that decides: a line says it would allow the call and is not in force.
It starts nothing and changes nothing. It checks the rules only, not the
[permission modes](../security/permissions.md), and a command line that names no call exits 2.

## `completion`

```sh
bravebot completion <bash|zsh|fish>
```

Prints a script that completes bravebot's subcommands and flags when you press Tab. It makes no
request and reads nothing from `~/.bravebot`, so it completes the names bravebot defines and file
paths, and not session ids.

```sh
# bash: add to ~/.bashrc
eval "$(bravebot completion bash)"

# zsh: add to ~/.zshrc, after compinit
eval "$(bravebot completion zsh)"

# fish
bravebot completion fish > ~/.config/fish/completions/bravebot.fish
```

Then type `bravebot ` and press Tab, or `bravebot --` and Tab. The command takes exactly one shell
name and exits with status 2 for anything else.

## `shell-init`

```sh
bravebot shell-init <bash|zsh|fish>
```

Prints a hook for your shell. It keeps the command lines you run in each terminal and defines
`@bravebot`, which asks one question with the lines you ran since the last question.

```sh
# bash: add to ~/.bashrc
eval "$(bravebot shell-init bash)"

# zsh: add to ~/.zshrc
eval "$(bravebot shell-init zsh)"

# fish: add to ~/.config/fish/config.fish
bravebot shell-init fish | source
```

Then, in that terminal:

```sh
cargo test
@bravebot "why did that fail?"
```

`@bravebot "question"` runs `bravebot -p "question"` with up to the last 200 recorded lines (at most
64 KiB) as piped input, so they are quarantined like any other piped input and the planner is given
a reference to them. Quote the question. The lines are emptied once sent, so the next question
carries only what you ran after it.

- Only the command line is recorded, as you submitted it. What a command printed is not.
- Lines are written to `~/.bravebot/shell/<process id of the shell>`, readable by you alone, and the
  file is removed when the terminal closes. A terminal that is killed leaves its file until another
  shell with the same process id starts.
- A line that is `@bravebot` or begins with `@bravebot` and a space is not recorded. bash records a
  line when the command finishes, and a line your `HISTCONTROL` or `HISTIGNORE` drops is not
  recorded. Anything piped into `@bravebot` is ignored, and the lines are emptied when the question
  is sent, even if the run then fails to start.
- With `BRAVEBOT_INCOGNITO` set to a non-empty value, nothing is recorded and the question runs with
  `--incognito`. It is read at each command, so `export BRAVEBOT_INCOGNITO=1` stops recording from the
  next line.

The command prints a fixed script and reads and writes nothing under `~/.bravebot`. It takes exactly
one shell name and exits with status 2 for anything else.

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
| `/add-dir <path> \| close <path>` | Open another directory and trust it for this session, or close one |
| `/cd <path>` | Work in another directory from now on, and trust it for this session |
| `/rename <name>` | Call this conversation something else |
| `/compact` | Summarise the conversation so far, keeping the recent part |
| `/btw <question>` | Ask something beside the work, without putting it in the conversation |
| `/recap` | Recap where this session stands, without putting it in the conversation |
| `/clear` | Start a new session here, keeping this one resumable |
| `/loop [interval] <prompt>` | Send a prompt again and again, on your interval or at a pace each turn sets |
| `/goal [<condition> \| clear]` | Keep working until a condition you set is judged met |
| `/watch [stop <n>]` | List the files this session is watching, and stop one by its number |
| `/panel` | Show or hide the info panel beside the transcript |
| `/pr [<url> \| clear]` | Say which pull request this session is for, show it, or clear it |
| `/issue [<url> \| clear]` | Say which issue this session is for, show it, or clear it |
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
| 6 | `BB1006` | finished, and its reply is not what `--output-schema` asked for |

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

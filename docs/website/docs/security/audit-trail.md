---
sidebar_position: 3
title: The audit trail
description: What is recorded about every decision the system makes, and how to read it.
---

# The audit trail

Every gate decision is recorded, allowed **or** refused. A trail that logged only what happened
would not answer "why did it not do the thing I asked", which is most of what anyone asks it.

## Reading it

| Where | How |
|---|---|
| in a session, live | **Ctrl-T** toggles the trail |
| a one-shot run | `--trace`, which puts it on stderr, after the error too when the run ends in one |
| after the fact | `~/.bravebot/sessions/<directory>/<id>.audit.jsonl` |

An [incognito session](../using/sessions.md#a-session-that-leaves-nothing-behind) shows its gate
decisions on screen as always and writes no file. The trail holds no content, but it does hold gate
names and paths, which is a record of a session having happened and what it touched.

```sh
bravebot "what does this do?" --file src/main.rs --trace
```

## What a trail looks like

Reading a file in a trusted directory, where the content reaches the model:

```
ok      precommit: routing fields ["task"] fixed before any observation
ok      promote: read_file.path proposed by the model, public and non-destructive
ok      file_read.path [routing] (T,pub)
observe file_read produced (T,priv)
ok      trust: notes.md read as trusted, from a trusted path
ok      render: read_file: content reshaped without being read, still (T,priv)
ok      present: tool_result: notes.md is (T,priv), so the planner may read it
```

The same read where nothing is vouched for, so the content is quarantined instead:

```
observe file_read produced (U,priv)
ok      trust: notes.md read as untrusted
slot    ref:0 at (U,priv)
ok      present: tool_result: notes.md is (U,priv), quarantined as ref:0; the planner
        sees a reference only
```

A read of a file outside the workspace, refused when the path resolves, with what you can do about
it:

```
ok      promote: read_file.path proposed by the model, public and non-destructive
BLOCK   confine: read_file.path: '/etc/hosts' resolves outside the workspace; remedy offered:
        open its directory, or drop the file
```

A `watch_file` on a path outside the workspace is recorded the same way, with the path as typed and
the remedy, and the turn does not end as clean. An absolute path that lands inside the workspace is
recorded as "is inside the workspace but not named relative to its root; remedy offered: name it
relative to the workspace root".

Three pieces of notation appear throughout:

- `(T,pub)` and `(U,priv)` are the **label** on a value: trusted or untrusted on the first axis,
  public or private on the second.
- `ref:N` is a **slot** holding content the planner is not allowed to read, so it is handed the
  reference instead of the bytes.
- `routing` marks the part of a call that **decides where it lands**, as opposed to the part that is
  merely carried.

A *gate* is a check that has to pass before anything consequential happens: content reaching the
model, a file being written, a program being run, a request leaving the process. Each one decides a
single question and refuses rather than warning, so there is no path to a consequence that does not go
through one.

## Compaction is recorded

Every compaction gets a line: how many messages were summarised, how many were kept word for word,
what the summary cost, and which tool-calling round it landed on. A `/compact` you asked for between
rounds reports round zero, having interrupted nothing. One given a focus (`/compact [focus]`) says
so by the focus's length in characters, and never repeats it.

It is recorded *after* the conversation is shortened, so a summary refused on the way back in leaves
no line claiming one was made. Counts and nothing else, so this carries no more content than the
rest of the trail does.

## A reply stopped at the output limit is recorded

Every reply the output limit stopped gets a line: the limit, the tool-calling round, the tool it was
writing a call to and how many bytes of that call had arrived, whether any reasoning arrived, and
what the turn did next: asked the model again, kept what it wrote as the answer, or ended. The tool
is named as bravebot offered it. Nothing the reply wrote is in the line.

## Planning is recorded

A planning call is a gate like any other and gets its own line, refusals included. A run planned in
advance makes two of them, one for reading the goal in plain words and one for fitting that goal to the
tool set, and both appear.

## Messages between sessions are recorded

A [`message_session`](../reference/tools.md#message_session) call that gets as far as a question or a
send adds a `message` line to the sending session's trail. It holds the sender's session id, the
receiver's, the label the message went under, its length in characters, what you answered, and how it
ended: queued, full, too long, stopping, unknown, not reached, or not sent. It holds none of the text.

## The mode and what answered are recorded

Each turn and each manifest run begins with a `permission_mode` line naming the mode it began in:
`ask`, `accept-edits`, `plan` or `bypass`. A manifest run writes it before planning. A command or write that was put to you gets an `approval`
line after it that says either `no mode answered, left to the confirmer` or
`answered by <mode> mode, nobody was asked`, so an entry ending in "asking" is never the only thing
the trail says about who answered. A fetch or a server start that a mode answers carries no such
line. A `run` that asks to start with no sandbox is put to you in every mode, so its line says no mode
answered.

A turn whose prompt opens with a [`/style`](../reference/commands.md#style-name--off) output style adds an
`output_style` line: `the turn began in the concise style (built in)`, `(a file of the person's
own)`, or `(a file of the project's the trust map vouches for)`. It records the name and where the style came from, and no line when `--system-prompt`
replaced the opening.

A write call plan mode refused before the tool ran, and a planned run it refused for a step that
writes, each get a blocked `permission_mode` line naming the tool, or the step and its tool, so the
trail says why nothing was written:

```
BLOCK   permission_mode: write_file: plan mode refuses a write, so nothing was written
```

The [sandbox mode](security.md#confinement) `read-only` is recorded the same way, under `sandbox`:
for a write call it refused, a planned run it refused for a step that writes, and a command line it
refused for redirecting output into a file.

```
BLOCK   sandbox: write_file: the read-only sandbox mode refuses a write, so nothing was written
```

A `run` call that asks for no sandbox (`unconfined`) or for a credential scope (`scopes`) and is
refused gets a blocked `sandbox` line naming the argument and every cause that held. The planner is
told one sentence that names no setting, so this line is where to read which one withheld the
request: the sandbox mode, a workspace you have not trusted, a managed closed network, a delegate,
a turn that confines none of its runs, or for `scopes` a platform with no sandbox or a session with
no profile directory.

```
BLOCK   sandbox: run.unconfined: the request was refused and the line did not run: the strict sandbox mode does not take it
```

A write, an edit, a fetch or a save that was put to you and not approved gets a blocked `approval` line
naming the tool, so the trail says why nothing was written or fetched. Each file an `apply_checkout`
brings back is a write of its own. Answering no, pressing Ctrl-C and a question nobody could be asked
leave the same line, and a write a manifest run planned names its step.
A `download_url` asks about the host and then about the file, and the line says which answer was no.

```
BLOCK   approval: write_file: the write was not approved, so nothing was written
BLOCK   approval: fetch_url: the fetch was not approved, so nothing was fetched
```

A call to an MCP server's tool that was put to you and not approved gets the same line, naming the
call as the server's alias and the tool's name. Answering no, pressing Ctrl-C, and a question nobody
could be asked leave it, with or without words typed beside the answer. A call a `deny` rule refused
was never put to you and has its own `mcp_call` line.

```
BLOCK   approval: weather:get_forecast: the call was not approved, so the server was not called
```

## The hosts a run's programs asked for are recorded

When the sandbox holds a run to [a list of hosts](../customize/configuration.md#sandboxnetwork), the
trail records, once the foreground `run` has stopped, what the list decided for each host its programs
asked for under `hosts`: the host, `allowed` or `refused`, the entry that decided it or `not listed`,
`port not carried` or `not public`, and the stages that were served. Repeats are counted. A program
chooses the name it asks for, so the record carries a name only where it is a host name or an address;
any other request is recorded as a name that is not a host name, without its bytes. At most 256
decisions are kept until they are taken, and the record says how many more there were. A decision taken
for a job left running, or after a line was moved to the background, is recorded with the next
foreground line that shares its proxy. Your answer to a question about an unlisted host is recorded as
`host_grant`, with the hosts.

## Delegates and checkouts are recorded

A [delegate](../how-it-works.md#delegates) gets a line when it starts and another when it ends. The
ending line says how long it ran, how many rounds it made out of the most it may, and one cause: it
answered, it reached its round limit and answered with what it had, it was stopped, it did not finish
(with the kind of failure), or its thread died and returned nothing. A delegate that ran for
twenty minutes and produced nothing can be told apart this way from one that ran out of rounds. The
same cause is in the note you are shown. The planner is told only that the delegate did not finish.

Each [checkout](../customize/agents.md#a-checkout-of-its-own) made for a delegate, and each one
removed, is recorded as well.

## The request a turn sent can be read

The trail records decisions, not the request. `/request` shows the last request built for the model,
each part labelled `typed`, `trusted file <path>`, `tool result (trusted)` or `ref:N`, and it is read
from the request itself, so content the model was not shown appears only as the reference token it
was given. It is kept in memory, written to no file, and is not part of an incognito session's record.

## The trail holds no content

Every field is a gate name, a capability, a label, a path, a destination host or a slot id. A network
decision keeps the host and drops the rest of the URL: no userinfo, no path, no query, no fragment, so
a credential carried in a URL is not written down. That is exactly why the trail can be put on your
screen and written to a file without any release, and it is what makes the record safe to keep for a
workspace nobody vouched for.

## Assertions are recorded as assertions

Vouching for the output of a command you typed, labelling your configuration, and admitting a pasted
picture are each written down, because each is a claim a **human** made rather than something the
system worked out. These are the points where trust enters from outside, and a trail that recorded
only what the system deduced would omit exactly the decisions somebody might later want to account
for.

## On disk

One JSON object per line, appended a turn at a time, so a line-oriented file can be read with whatever
is to hand:

```sh
jq -r 'select(.gate == "present")' ~/.bravebot/sessions/*/…​.audit.jsonl
```

The labels are spelled out in words rather than abbreviated, because a file read months later has no
legend beside it, and each event keeps the time it happened. The compact form suits a terminal, where
the reader has the legend in front of them.

An event a delegate's gate took also names that delegate, in a `delegate` field the turn's own records
do not carry. Without it a resumed session would show one run's decisions where there were three, and
two delegates working at once would be unattributable. On screen the same name goes in front of the
line, so `d1 precommit: routing fields ["task"] fixed` is the first delegate's and an unprefixed line
is the turn's own:

```sh
jq -r 'select(.delegate == "d1")' ~/.bravebot/sessions/*/….audit.jsonl
```

How each delegate ended is the turn's own entry under `delegate`, with the delegate's name first. It
gives how long the delegate ran, how many of its rounds it made out of the most it was allowed, and a
cause from a fixed list: it answered, it reached its round limit, it was stopped, or it did not finish
with the name of the failure, such as `unavailable` or `refused`. The cause never quotes what a
service or a tool said:

```sh
jq -r 'select(.event.gate == "delegate" and (.event.detail | contains(": ended "))) | .event.detail' ~/.bravebot/sessions/*/….audit.jsonl
```

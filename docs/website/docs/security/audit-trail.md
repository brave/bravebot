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
| a one-shot run | `--trace`, which puts it on stderr |
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
ok      promote: read_file.path proposed by the model, confined and non-destructive
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
rounds reports round zero, having interrupted nothing.

It is recorded *after* the conversation is shortened, so a summary refused on the way back in leaves
no line claiming one was made. Counts and nothing else, so this carries no more content than the
rest of the trail does.

## Planning is recorded

A planning call is a gate like any other and gets its own line, refusals included. A run planned in
advance makes two of them, one for reading the goal in plain words and one for fitting that goal to the
tool set, and both appear.

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

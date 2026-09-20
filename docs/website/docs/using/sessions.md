---
sidebar_position: 5
title: Sessions
description: What is kept between runs, how to pick a session back up, and what a resume restores.
---

# Sessions

A session belongs to the directory it ran in. Records live under `~/.bravebot/sessions`, one
directory per working directory, so resuming in one project lists only that project's sessions.

Each session is two files, named after a version 4 UUID:

| File | What it holds |
|---|---|
| `<id>.json` | the record: the conversation, what the picker shows, and what a resume needs |
| `<id>.audit.jsonl` | the trail, appended a turn at a time: one JSON object per line |

**Both are private to you.** On Unix, session directories are created mode 0700 and records, trails
and the temporary files beside them are written mode 0600, with anything already there tightened on
write. A record holds the whole conversation: your prompts, the model's replies, the file snippets
the planner was shown, and the standing permissions you granted.

## Picking one back up

```sh
bravebot --resume          # choose from the sessions in this directory
bravebot --resume <id>     # name one outright
bravebot -r <id>           # the same
bravebot --continue        # carry on with the most recent one, unnamed
bravebot -c                # the same
```

The list is sorted on what each record says it was last written, not by id.

`--continue` takes the session the picker would offer first and picks it up exactly as naming its id
would. It passes over a [manifest run](#a-manifest-run-is-recorded-but-cannot-be-continued) rather
than refusing it. Where this directory holds nothing continuable it says so and fails rather than
starting a fresh session.

**Leaving a session prints the command that resumes it**, after the terminal is handed back, so it
stays on the screen you are left looking at. A session that never wrote a record prints nothing.

### Trying a second approach

```sh
bravebot --fork <id>
bravebot -f <id>
```

Forking copies a session into a new one with an id of its own and opens that. The session you forked
is left exactly as it was. The conversation, the spend history and the audit trail all come with the
copy, since the gates that decided the shared prefix decided the fork's history too. The start time
is reset and the title is marked.

A [manifest run](#a-manifest-run-is-recorded-but-cannot-be-continued) is refused, because there is no
conversation inside one to carry on from. An [incognito](#a-session-that-leaves-nothing-behind)
session writes no copy, so forking in one opens the conversation and records nothing.

### A session that changed directory

[`/cd`](../reference/commands.md#cd-path) takes the record with it, written into the new directory
straight away rather than at the end of the next turn. Until it is saved there, there is nothing
there to find. The turns already written stay where they were written, and are still resumable there.

The line printed on the way out **names the directory** when the session ended somewhere other than
where the shell is standing, because `--resume` looks an id up under the directory it is run in.
Without the name, the same id in the old directory would quietly resume the session as it was before
the move.

The trail does not follow the session either. It is appended beside whichever record was current, so
a move splits it. A resume replays the whole conversation and shows the gate decisions made in the
directory you resumed from.

### A manifest run is recorded, but cannot be continued

A [plan-then-execute run](headless.md#planning-the-whole-run-first) cannot be resumed. Its
conversation is empty, because a session is turns over one conversation and a manifest run has none.
The picker marks the row and refuses Enter. Naming one on the command line prints what it produced,
and still does not continue it.

The record holds its goal, its proposed plan, its frozen steps and what each one did, finished or
not.

## What a resume restores

The conversation, the plan each turn was working to, what the session has spent, the branch it ran
on, and the **standing permissions its user granted**:

- the [trust map](../security/trust.md), including any rule a write recorded, which is what stops a
  resumed turn reading back a file an earlier turn of the same session poisoned;
- the list of [commands you said to stop asking about](../security/permissions.md#vouching-for-a-command);
- every question you asked with [`/btw`](../reference/commands.md#btw-question), and the answers the
  record could keep, which come back into the view Ctrl-L opens and into no conversation.

Nothing else survives. A single-use endorsement is created by one approval, is bound to one value and
is never written down, so a resumed turn cannot replay a write or a run an earlier turn was allowed.
Answers to the planner's own questions live only in the running session, so a resumed session asks
them again.

A resume does **not** ask the startup trust question, because the answer honoured is the one that
session's own user gave. A record from before maps were kept has none, and is asked about. Resuming a
session recorded by a different build says so, beside the note about a changed branch.

## What the record accounts for

The record keeps the model that answered and what **each turn** spent, alongside the total. The name
recorded is the one that **answered**, not the one you asked for, since an endpoint may serve
something other than the name it was given.

Every turn's wall clock is written down **split four ways**: waiting on the model, running tools,
waiting for you to answer a prompt, and whatever is left over. The four are a partition rather than
four separate measures, so an approval drawn from inside a tool call is taken off the tool figure
rather than counted in both. `/status` reports the session total and each part that actually
happened, leaving out a part that did not rather than showing it as zero.

What a [delegate](../how-it-works.md#delegates) spends is counted in the turn's tokens. Its seconds
are not: several delegates and the turn spend the same seconds at once, so adding them would report a
turn as having taken longer than it did.

A turn that failed is recorded like any other. A `/compact` asked for mid-turn is charged to the turn
it interrupted, as its tokens are. A record written before any of this was kept reads as an empty
breakdown, which is not the same as a session that took no time: the durations that were kept are
still there.

## What is never written down

**Nothing untrusted.** Every message in the record has already passed the gate that decides what the
planner may see, so what lands on disk is what the planner was allowed to hold. Quarantined content
is not written at all, and the trail is labels and gate names with no content in it. A record is read
back into a later turn's context, so anything written that the planner could not have held would
enter that context on the next resume.

A pasted picture *is* written down, because it was never quarantined. It is part of your own message,
and a session that turned on a screenshot would be no use resumed without it.

An answer to a [`/btw`](../reference/commands.md#btw-question) asked over an exchange that had met
something untrusted is not written down, for the same reason: it is the planner's own words over such
a context. The question is kept, the row on screen says the answer lasts as long as the window, and a
resume brings back the question alone.

## Naming a session

A session's title is the first line of what you asked, cut rather than mangled if it is long. A
prompt with nothing in it still has a title.

```
/rename dependency audit
```

Renaming rewrites the record immediately, and a chosen name survives the next turn. An empty name is
refused.

## Starting over

```
/clear
```

`/clear` begins a new session in the same directory and keeps the current one resumable. Because it
is a new session it asks the trust question again, restores no standing permissions, and closes any
directory `/add-dir` had opened.

## Taking the last turn back

[`/undo`](../reference/commands.md#undo) rewinds the most recent turn: the files it wrote go back to
what they held, and the conversation, the spend and the standing permissions go back with them. One
turn is as far as it goes, and `/clear`, `/compact`, `/rename`, `/add-dir`, `/cd` and a shell-mode
command each close the window, as does the next turn beginning. It cannot undo what a *program* the
turn ran did, since the workspace never saw those writes.

## Prompt history

Up walks backwards from the most recent prompt and stops at the oldest; Down walks forwards again.
Leaving the newest entry puts back the half-written line you were on. Submitting leaves the mode, and
a prompt arriving while you browse does not shift the view.

History persists across runs under `~/.bravebot/history` and is capped. Consecutive duplicates
collapse into one, and a prompt you cancelled is removed again.

**Ctrl-R searches it.** See
[Interactive mode](interactive-mode.md#searching-the-prompts-you-have-sent). Each prompt is stored
with when it was sent and which workspace it was sent from, both of which that search reads. A
history written before either was kept still reads, as prompts with neither, and is written back out
the way it came in.

## Long conversations

A conversation that grows past its token budget is **compacted**: an older stretch of it is replaced
by a summary, in the request only. The record and the transcript keep the whole thing: the replaced
messages go to an archive that both still read.

```
/compact
```

asks for that work on demand, at any size, without consulting the budget. See
[Configuration](../customize/configuration.md#context-budget) for the budget itself.

Compaction never touches three things: the quarantine, which holds the only copy of what a surviving
reference names; the reference counter, since a slot name handed out twice would collide; and the
context's integrity, since nothing here has un-read what the conversation read. The cut never lands
inside a round, so a call is never separated from its results.

## A session that leaves nothing behind

```sh
bravebot --incognito
```

An incognito session runs like any other and adds nothing to `~/.bravebot`. No prompt reaches the
history, no session record and no title are written, no audit trail is kept, and a model, theme or
effort level chosen inside it applies for that session without being recorded. The sessions directory
is not created either, since an empty one still says that a session ran, in this project, at this
time. Nothing is written, so nothing is resumable. An incognito session does not appear in the
picker, including to itself.

A history and records that were already there are left exactly as they were, and an ordinary session
from before stays resumable. It stops being updated for as long as the incognito one runs.

**Reading is untouched.** The settings, the model and theme you chose, your standing instructions,
your skills and your imported credentials are all read as usual, so the session is the one you
configured rather than a fresh install.

The flag may go anywhere in the command line and combines with `-p`, `--resume`, `--mode` and a
bare invocation alike. It cannot be turned off once the session has started.

### What it does not cover

Three things still reach the filesystem:

- **Your project.** `write_file` and `edit_file` go on editing it. Those edits are the work rather
  than a trace of it.
- **Programs you run.** A program reaches the filesystem with the access your own shell would give
  it, and may write whatever it likes. What confines one is
  [confinement](../security/security.md#confinement), which is a different question.
- **The editor hand-off.** Composing in `$EDITOR` writes a scratch file. It goes to the system
  temporary directory rather than `~/.bravebot`, is readable by nobody else, and does not outlive the
  edit.

[`import-leo-creds`](../customize/premium.md) is refused rather than quietly skipped, since a
credential that did not outlive the session would not be an import. `--forget` still works, because
removing a stored secret leaves less behind rather than more.

## When it cannot be written down

Everything here degrades to doing nothing: a missing home directory, a full disk, a corrupt record, a
stored time in the future. A session that cannot be written down still runs, one that cannot be read
is left out of the list, and a corrupt history reads as no history rather than as an error.

:::note
Two working directories can share a session store. The directory name is derived by mapping every
character outside a small set to `-`, which is lossy, so `/a/b`, `/a-b` and `/a b` all reduce to the
same name. Because a resume restores standing permissions, permissions granted in one of those
directories would be offered in another. This is
[a known bug](https://github.com/brave-experiments/bravebot/issues), not a design decision.
:::

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
write. Every write narrows the directory it lands in, including the one a [fork](#trying-a-second-approach)
writes into. A record holds the whole conversation: your prompts, the model's replies, the file snippets
the planner was shown, and the standing permissions you granted.
[`/export`](../reference/commands.md#export-path) writes the readable form of it out as markdown, so
reading a conversation back does not mean reading JSON out of the state directory.

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
and still does not continue it. That print is a **report rather than a refusal**: it goes to stdout
and exits successfully, because reading a run back is what naming one is for.

The record holds its goal, its proposed plan, its frozen steps and what each one did, finished or
not.

## What a resume restores

The conversation, the plan each turn was working to, what the session has spent, the branch it ran
on, and the **standing permissions its user granted**:

- the [trust map](../security/trust.md), including any rule a write recorded, which is what stops a
  resumed turn reading back a file an earlier turn of the same session poisoned;
- the list of [commands you said to stop asking about](../security/permissions.md#vouching-for-a-command);
- every question you asked with [`/btw`](../reference/commands.md#btw-question), and the answers the
  record could keep, which come back into the view Ctrl-L opens and into no conversation;
- the [turns a rewind can still reach](#a-rewind-survives-a-resume), so `/undo` after a resume reaches
  the same turns it reached before.

Each turn also keeps its number, the prompt as it was shown, what came of it, and its task list,
spend and timing, a turn that failed or was cancelled included. A recorded failure carries the reason
the interface composed and never a message from the backend. A prompt handed back to the editor when
you cancelled stays out of the transcript on resume, though it is still there to recall with Up.

Nothing else survives. A single-use endorsement is created by one approval, is bound to one value and
is never written down, so a resumed turn cannot replay a write or a run an earlier turn was allowed.
Answers to the planner's own questions live only in the running session, so a resumed session asks
them again.

A resume does **not** ask the startup trust question, because the answer honoured is the one that
session's own user gave. A record from before maps were kept has none, and is asked about. Resuming a
session recorded by a different build says so, beside the note about a changed branch, and so does
resuming one the other front end wrote: the terminal and the desktop app keep their sessions in the
same place, and the transcript you are looking at was drawn by whichever of them recorded it.

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
count only where they overlap the stretch the turn actually spent waiting on its delegates, and two
delegates waiting at once are counted once, so a turn is never reported as having taken longer than
it did.

A turn that failed or was stopped is recorded like any other, charging the progress it had made,
completed delegate work included. A successful turn charges its outcome alone, and progress resets
each turn, so a turn that completed no request cannot inherit the previous turn's figures.

A `/compact` asked for mid-turn is charged to the turn it interrupted, as its tokens are. Anything
asked for **before the first turn**, an aside or a command as the first thing a session does, is
charged to a leading entry ahead of that turn rather than to no turn at all. A record written before
any of this was kept reads as an empty breakdown, which is not the same as a session that took no
time: the durations that were kept are still there.

### Asking what it has cost

```
/cost
```

reports the total and, under it, one figure per turn with that turn's share of the total beside it.
What was spent before the first turn is reported too, without a turn number, since no turn did it. A
record holding a total and no breakdown says the breakdown is missing, which is not the answer a
session that has spent nothing gives.

The figures are **tokens, not a bill.** A prompt a service answered out of its own cache is billed at
a fraction of a fresh one, and the breakdown keeps no cache split per turn, so a figure in money would
be composed here rather than measured. What is written stays comparable across turns and across
sessions, which is what the record is read for.

## What is never written down

**Nothing untrusted.** Every message in the record has already passed the gate that decides what the
planner may see, so what lands on disk is what the planner was allowed to hold. Quarantined content
is not written at all, and the trail is labels and gate names with no content in it. A record is read
back into a later turn's context, so anything written that the planner could not have held would
enter that context on the next resume.

A [rewind point](#taking-turns-back) is the one part of a record built from bytes no message carried:
it holds what the files a turn wrote over held, read off the disk rather than out of the conversation.
Those bytes go into the record only where the trust map that stood before the turn vouched for the
path. What a file nobody vouched for held is written down as contents this session did not keep.

:::caution
Vouching for a directory is not saying every file in it is worth copying elsewhere. A turn that
overwrites a file of credentials **inside a project you trusted** puts its previous contents into the
record, so they leave the project and land in the state directory. They go once the point ages out a
few turns later, and mode 0600 covers them while they are there, but a session that ends on such a
turn leaves them in its record until the record is deleted.
:::

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
is a new session it asks the trust question again, unless you said to
[remember the answer](../security/trust.md#remembering-the-answer) there, restores no standing
permissions, and closes any directory `/add-dir` had opened.

## Starting a plan-then-execute run

```
/manifest collect every TODO comment into notes/todos.md
```

starts one [plan-then-execute run](headless.md#planning-the-whole-run-first) and hands the session
back afterwards. It is a run, not a mode the session holds: a session is several turns over one
conversation, each deciding what to do next after the last one read something, so nothing about the
session could stay in that mode.

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
written at all, since there is nothing in it to read.

The plan is [put to you before it runs](headless.md#a-plan-is-put-to-you-first), drawn and scrolled
here rather than printed on one line. In [plan mode](../security/permissions.md#answering-in-advance-modes)
a plan with a write in it does not run at all, decided before the plan is put to anybody.

## Taking turns back

[`/undo`](../reference/commands.md#undo) puts the session back where it stood before the most recent
turn: the files that turn wrote go back to what they held, a file it created is removed, and the
conversation, turn count, spend, timing and remembered command approvals go back with them.
**Saying it again goes back another turn**, as far as the last five.

File trust follows what undo restored or left on disk. Restored files get the lower of their backup's
trust and their trust before the turn; untouched files get the lower of their current trust and their
trust before the turn. Grants made during undone turns are withdrawn. A file you distrusted during
those turns stays distrusted if undo leaves it untouched; you can grant trust again. One failed
restoration leaves that path untrusted without withdrawing unrelated grants.

### Reading a rewind before running one

`/rewind` on its own lists the points the session can go back to, most recent first and numbered
from one. Each row says how many turns back it is, which turn it would land before, what that turn
was asked, and every path that turn wrote over, or that it wrote over none.

`/rewind <n>` then goes back that many turns, which is what saying `/undo` n times does, so it puts
back every row from the first down to the one chosen. A number past what the session remembers
rewinds nothing and says how far back it does go, rather than going as far as it can: reading a tree
two turns younger than you believe it is would be worse than a refusal.

### What will not go back

- **Changes outside file tools.** Commands, hooks, scratch writes, language servers and desktop
  turns keep undo available, but their changes may remain. Undo names the recorded causes in its
  warning. Restoring a backed-up file can overwrite later command changes to that same file;
  undo does not reverse the command itself.
- **An edit you made since.** What goes back is what the path held *before the turn wrote to it*, so
  an edit of your own in between is lost. Nothing compares the file first, and nothing asks.
- **A file past the budget.** What the last five turns wrote over is held to one budget between them
  rather than one each. Past it the path is still remembered and its contents are not, so the rewind
  **names that path** as one that did not go back, and the rest of the rewind still happens.
- **The session's own scratch directory**, which is not part of the project. See
  [Trusted directories](../security/trust.md).

### What gives up every point

These changes outside a turn give up every point: `/clear`, `/compact`, `/btw`, `/rename`,
`/add-dir`, and `/cd`. After one of these, `/undo` says there is nothing left to undo rather than
rewinding to a point that describes a different session. Shell-mode commands keep the points and
add a coverage warning.

A rewind past the first turn removes the record only when restoration is complete, there are no
coverage warnings, and file trust matches the original snapshot. Otherwise the resulting state
stays saved. A name you gave the session before that turn stays with it.

### A rewind survives a resume

The points are kept with the record, so `/undo` and `/rewind` after a resume reach the same turns
they reached before the program was closed. One path is the exception: what a file **nobody vouched
for** held is kept only in memory, never written down, so after a resume that path is reported as one
that did not go back. The file is left alone rather than deleted, and the turn is undone in every
other respect.

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

asks for that work on demand, at any size, without consulting the budget.

The budget is the window the endpoint advertises for **whichever model is in force**, where it
advertises one, so a one-shot run and a session both get the window of the model they are asking for.
The built-in default only stands in, for a model resolved per request and for an entry that advertises
nothing. See [Configuration](../customize/configuration.md#context-budget) for setting it by hand.

A cut has to free more than it keeps, so a conversation with nothing worth giving up is left long
rather than summarised once per round.

Compaction never touches three things: the quarantine, which holds the only copy of what a surviving
reference names; the reference counter, since a slot name handed out twice would collide; and the
context's integrity, since nothing here has un-read what the conversation read. The cut never lands
inside a round, so a call is never separated from its results.

## A session that leaves nothing behind

```sh
bravebot --incognito
```

An incognito session runs like any other and adds nothing to `~/.bravebot`, apart from the one file
[below](#what-it-does-not-cover). No prompt reaches the history, no session record and no title are
written, no audit trail is kept, and a model, theme or effort level chosen inside it applies for
that session without being recorded. The sessions directory is not created either, since an empty
one still says that a session ran, in this project, at this time. No record is written, so nothing
is resumable. An incognito session does not appear in the picker, including to itself.

A history and records that were already there are left exactly as they were, and an ordinary session
from before stays resumable. It stops being updated for as long as the incognito one runs.

**Reading is untouched.** The settings, the model and theme you chose, your standing instructions,
your skills and your imported credentials are all read as usual, so the session is the one you
configured rather than a fresh install. The record of command lines you asked to be remembered past a
session is read on the same terms: a line already in it still stops the asking, and the key that would
add one is not offered. So is the record of directories you told the startup question to
[remember](../security/trust.md#remembering-the-answer): a kept answer still trusts the directory,
the session says so as it opens, `r` is not offered, and `/forget-trust` changes nothing and names
the file so you can remove it yourself.

**One thing is not read: your standing answer about [vetting](../security/vetting.md).** If you have
pressed the key that stops bravebot asking before content nobody vouched for reaches the planner,
an incognito session asks anyway. A model and a theme decide what a session looks like, and that one
decides whether you are asked, which is not a decision to inherit into a session you asked to leave
nothing behind. `--vet` and the `vetting.auto` key in your settings still work here, so you can say
what you want of this run.

The flag may go anywhere in the command line and combines with `-p`, `--resume`, `--mode` and a
bare invocation alike. It cannot be turned off once the session has started.

### What it does not cover

Five things still reach the filesystem:

- **Your project.** `write_file` and `edit_file` go on editing it. Those edits are the work rather
  than a trace of it.
- **Programs you run.** A program reaches the filesystem with the access your own shell would give
  it, and may write whatever it likes. What confines one is
  [confinement](../security/security.md#confinement), which is a different question.
- **The editor hand-off.** Composing in `$EDITOR` writes a scratch file. It goes to the system
  temporary directory rather than `~/.bravebot`, is readable by nobody else, and does not outlive the
  edit.
- **The session's own scratch directory.** Somewhere to put a file that is not part of the project is
  something a turn needs, and this mode does not take it away. It sits in the system temporary
  directory on the same terms as the editor's file and goes with the session. Its name says which
  program made it and nothing about which project or which session, so an empty one records only that
  this program ran at this time.
- **A credential you spent.** With a [Leo Premium](../customize/premium.md) subscription imported,
  the session spends credentials from it as any session does, and records which ones it spent in the
  file the import created under `~/.bravebot`. A credential is single use and presenting it to the
  service spends it there, so one left looking unspent would be offered again in your next session,
  wasting a credential you paid for. If the batch runs out while you are working, a new one is
  fetched for the same subscription and written to the same file, rather than thrown away when you
  exit. What ends up in the file is credentials for a subscription you imported before the session
  and which of them are spent, and nothing about the project or what you asked.

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
[a known bug](https://github.com/brave/bravebot/issues), not a design decision.
:::

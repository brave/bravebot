---
sidebar_position: 5
title: Sessions
description: What is kept between runs, how to pick a session back up, and what a resume restores.
---

# Sessions

A session belongs to the directory it ran in. Records live under `~/.bravebot/sessions`, one
directory per working directory, so resuming in one project lists only that project's sessions.

Each session is named after a version 4 UUID and keeps up to three files:

| File | What it holds |
|---|---|
| `<id>.json` | the record: the conversation, what the Sessions screen shows, and what a resume needs |
| `<id>.audit.jsonl` | the trail, appended a turn at a time: one JSON object per line |
| `<id>.lock` | empty: the lock a process holds while it writes the record |

**Both are private to you.** On Unix, session directories are created mode 0700 and records, trails
and the temporary files beside them are written mode 0600, with anything already there tightened on
write. Every write narrows the directory it lands in, including the one a [fork](#trying-a-second-approach)
writes into. A record holds the whole conversation: your prompts, the model's replies, the file snippets
the planner was shown, and the standing permissions you granted.
[`/export`](../reference/commands.md#export-path) writes the readable form of it out as markdown, so
reading a conversation back does not mean reading JSON out of the state directory.

## Picking one back up

```sh
bravebot --resume          # open the Sessions screen to choose one
bravebot --resume <id>     # name one outright
bravebot -r <id>           # the same
bravebot --continue        # carry on with the most recent one, unnamed
bravebot -c                # the same
```

The list is sorted on what each record says it was last written, not by id.

Typing narrows the list by title, branch and linked issue or pull request, and by what was said in
the session: the prompts you typed, the model's replies, the paths and commands its calls named,
and the [notes](#noting-something-on-a-session) you added. A session found that way shows the first
matching line under its title. Add `since:7d`
(`h` for hours, `d` for days, `w` for weeks) to keep only sessions written within that long. Tool
results, files you attached and [imported sessions](#importing-sessions-from-claude-code) are not
searched. The records are read when you type the first character, so a long list pauses once.

From a script, `bravebot sessions search` does the same and prints a session's id and title on a
line each, newest first:

```sh
bravebot sessions search ledger rounding
bravebot sessions search since:2w workspace:../app flaky test
```

`workspace:` names the directory whose sessions are searched, and defaults to the current one. It
exits 1 and prints nothing when no session matches.

`bravebot sessions list [--json] [--limit <n>]` prints the sessions recorded for this directory, newest
first, one per line: the id, how long ago it was last written, the title, the branch, the front end
that wrote it and whether it can be continued. `--limit` defaults to 20, and a line on stderr says how
many were left out. `--json` prints one object per line with `id`, `title`, `directory`, `updated`,
`branch`, `front` and `continuable`, and nothing else. A limit of zero, one that is not a number, a
repeated flag or any other word is refused. Only sessions whose recorded directory is this one are
listed, so two directories that share a store do not show each other's sessions, which the
Sessions screen does not. A bare `bravebot sessions` lists
[background sessions](#sessions-in-the-background) instead.

`--continue` takes the session the Sessions screen would offer first and picks it up exactly as naming its id
would. It passes over a
[manifest run](manifest-mode.md#a-manifest-run-is-recorded-but-cannot-be-continued) rather than
refusing it. Where this directory holds nothing continuable it says so and fails rather than starting
a fresh session.

**From inside a session**, [`/resume`](../reference/commands.md#resume-id) opens the same list, or
takes an id, and moves you onto that session without leaving the program.

**Leaving a session prints the command that resumes it**, after the terminal is handed back, so it
stays on the screen you are left looking at. A session that never wrote a record prints nothing.

**A session is open in one place at a time.** The process running a session holds its record, so
resuming it from a second terminal or window is refused, and the refusal names the session and says
to copy it with `bravebot --fork <id>`. `--resume`, `--continue`, the `--resume` list, `/resume`, a
one-shot run given a task, the desktop app and `attach` or `reply` on a stopped background session
all refuse in the same way. Two processes that wrote one record would each replace the other's turns,
and with them any file you had not trusted: the file would read as trusted afterwards. The hold ends
when the session does, or when its process does, however it ends. A record that another process wrote
after yours read it is not resumed either, and says to resume it again.

### Trying a second approach

```sh
bravebot --fork <id>
bravebot -f <id>
```

Forking copies a session into a new one with an id of its own and opens that. The session you forked
is left exactly as it was. The conversation, the spend history and the audit trail all come with the
copy, since the gates that decided the shared prefix decided the fork's history too. The start time
is reset and the title is marked.

From inside a session, [`/branch`](../reference/commands.md#branch-name) does the same and moves
you onto the copy, saying the id of the original so `--resume` reaches it.

A [manifest run](manifest-mode.md#a-manifest-run-is-recorded-but-cannot-be-continued) is refused,
because there is no conversation inside one to carry on from. An [incognito](incognito.md) session
writes no copy, so forking in one opens the conversation and records nothing.

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

A plan-then-execute run is recorded as its own record and cannot be resumed. See
[Plan first, then execute](manifest-mode.md#a-manifest-run-is-recorded-but-cannot-be-continued).

## What a resume restores

The conversation, the plan each turn was working to, what the session has spent, the branch it ran
on, and the **standing permissions its user granted**:

- the [trust map](../security/trust.md), including any rule a write recorded, which is what stops a
  resumed turn reading back a file an earlier turn of the same session poisoned;
- the list of [commands you said to stop asking about](../security/permissions.md#vouching-for-a-command);
- every question you asked with [`/btw`](../reference/commands.md#btw-question), and the answers the
  record could keep, which come back into the view Ctrl-L opens and into no conversation;
- the [turns a rewind can still reach](rewind.md#a-rewind-survives-a-resume), so `/undo` after a
  resume reaches the same turns it reached before;
- the model the record names, which stays in force for the first save outside a turn as well, and the
  pull request and issue you gave with `/pr` and `/issue`. `bravebot --fork` copies both, and
  `/clear` begins a session with neither.

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

The record holds the directory as a path and nothing that identifies the checkout standing there. If
you remove a checkout, put a different one at the same path and resume an old session in it, the
trust map and the commands you said to stop asking about come back for the new checkout, and it is
not asked the startup trust question. Start a fresh session there instead, which asks both.

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

### Totalling what sessions spent

```sh
bravebot stats
```

adds the sessions, turns and tokens of every recorded session, with how many of the tokens the model
read and how many it wrote, where `/cost` reports the one in front of you. `--days 7` keeps the
sessions last written in the last seven days, `--models` shows what each model spent, and
`--project` keeps the sessions that ran in this directory, or in the one `--project <dir>` names.
Every option is described under [`stats`](../reference/cli.md#stats).

The same limits apply as for `/cost`: the figures are tokens, and a session is counted whole, under
the day it was last written and the model it ended on. The read and written figures cover the
sessions written by a version that kept them, and the tokens of the rest are in the unsplit column.
An incognito session leaves no record, so it is not in the total. A session you forked, or moved
with `/cd`, is counted once for each record it left, so its spend can appear more than once.

## What is never written down

**Nothing untrusted.** Every message in the record has already passed the gate that decides what the
planner may see, so what lands on disk is what the planner was allowed to hold. Quarantined content
is not written at all, and the trail is labels and gate names with no content in it. A record is read
back into a later turn's context, so anything written that the planner could not have held would
enter that context on the next resume.

A [rewind point](rewind.md) is the one part of a record built from bytes no message carried:
it holds what the files a turn wrote over held, read off the disk rather than out of the conversation.
Those bytes go into the record only where the trust map that stood before the turn vouched for the
path. What a file nobody vouched for held is written down as contents this session did not keep.

:::caution
Vouching for a directory is not saying every file in it is worth copying elsewhere. A turn that
overwrites a file of credentials **inside a project you trusted** puts its previous contents into the
record, so they leave the project and land in the state directory. They go once the point ages out a
few turns later, and mode 0600 covers them while they are there, but a session that ends on such a
turn leaves them in its record until the record is deleted: by you, as
[below](#deleting-a-session), or once nothing has written it for
[`retentionDays`](../customize/configuration.md#retentiondays) days, which is 30 where you state none.
:::

A pasted picture *is* written down, because it was never quarantined. It is part of your own message,
and a session that turned on a screenshot would be no use resumed without it.

An answer to a [`/btw`](../reference/commands.md#btw-question) asked over an exchange that had met
something untrusted is not written down, for the same reason: it is the planner's own words over such
a context. The question is kept, the row on screen says the answer lasts as long as the window, and a
resume brings back the question alone.

## Naming a session

A session's title is the first line of what you asked, cut rather than mangled if it is long. A
prompt with nothing in it still has a title. A `/compact` typed before any prompt writes no record
and does not fix the title; the first prompt that says something names the session.

```
/rename dependency audit
```

Renaming rewrites the record immediately, and a chosen name survives the next turn. An empty name is
refused.

The terminal's title follows the name, as `bravebot · dependency audit`, so a row of tabs running
several sessions can be told apart. It is set once the session has a name and changes when the name
does, and your shell's own title comes back when you leave. While a turn is running the title starts
with `✦`, and while an approval or a question is waiting for you it starts with `[!]`, so the tab
that needs an answer can be found without opening each one. A [`/manifest`](../reference/commands.md#manifest-task) run
shows `[!]` while one of its approvals is open. At the box there is no marker. A terminal that cannot save a title is
left with an empty one rather than this session's name. An incognito session leaves the title alone,
and so does setting [`terminalTitle`](../customize/configuration.md#terminaltitle) to `false`.

## Noting something on a session

```
/note it edited the wrong config file
```

`/note` adds a note to the session while it runs, on the turn the session is on. It is written to the
record at once, with the turn number and the fact that you typed it, in a place of its own. It is not
part of the conversation: the model is never given it, so it changes nothing about the work. Typed
while a turn runs with nothing waiting, it is carried out at once. A resume, a `/branch` and a
continued session keep the notes, and a record from before notes existed has none. The resume list
and `bravebot sessions search` find a session by the words of its notes.
`/note` on its own lists the notes the session holds, one line each with its turn. A session that writes
no record, as an incognito one does, says a note reached none.

A note is for whoever reviews the session later. `/session-review` reads the notes of the sessions it
reviews and treats each as a lead from you: it checks the note against the session's audit trail and
the code, and reports it only when the cause is in bravebot. Nothing in a note is run or followed as
an instruction, and a filed issue carries none of its words. A session with a new note is reviewed
again for that note.

## Telling sessions apart

The title says which session a terminal holds. The info panel says the rest: press Ctrl-X, or type
`/panel`, and a column 36 wide opens on the right of the screen. Drag the border between it and the transcript to give
it more room or less; it keeps at least 24 columns and leaves the transcript at least 64. Kitty,
Ghostty and foot change the pointer to a resize arrow over the border. What a click on its rows does
is in [the mouse](transcript.md#what-a-click-does). It holds

- the session's name, with a GitHub pull request or issue address in it shown as
  `brave/bravebot#1975` and still a link to the address in a terminal that supports links,
  its directory and its branch, and below them its id over two rows, the one
  `bravebot --resume`, `--fork` and `/resume` take, so you can resume or fork a running session from
  another terminal without `/status`; it appears once the session has been saved,
- the model you chose and the effort level in force. A model your configuration names is drawn by the
  name the `/model` picker gives it, with the service after a tier's name, as `Sonnet (Bedrock)`, rather
  than by the inference-profile ARN a Bedrock tier is requested by; `/status` draws it the same way and
  keeps the request name on the line beneath,
- the goal, while one stands,
- the pull request and the issue the session is for, once you give them with `/pr` and `/issue`,
  each cut from the left where it is long so the number at the end stays in view,
- how full the context is (read again after each round of a turn, so the first turn of a session shows
  a figure before it ends), the tokens the session has spent (the figure `/cost` opens with, which
  counts what a running [delegate](../how-it-works.md#delegates) has completed as it completes, nested
  delegates included), the
  cache hit rate, and what the turn read from the cache and wrote
  into it, as two figures. A figure that is zero is not drawn, so a service that reports no cache
  shows neither and a turn that only wrote to the cache shows the write alone. The cache figures update after each round of a turn in flight, including
  the first turn, and a round that reports nothing spent leaves the previous figures in place,
- the language servers the session has started, by program name, and the MCP servers, by alias,
- the files the session has written, as a count with the lines added and removed, once one has been
  written. Clicking the count opens the files view below,
- the plan, which stays after the turn ends. A task still pending or in progress is drawn with a
  filled `■` while a turn is running, and dim with a hollow `□` while none is, as after the turn
  that wrote the list has ended or in a resumed session. Where it runs past the bottom it shows the
  task in progress and the pending tasks first, in the order of the plan, then the finished tasks
  nearest the first one still to do, and a row after them counts what it left out in words of their own,
  as `+3 to do, +40 done, +2 cancelled`. A kind with none left out is not named. A task list with no
  rows leaves the plan as the last list that had rows. `/clear`, a rewind and a new turn clear it.
  [`/todos`](../reference/commands.md#todos), or Alt-A, prints the whole plan in the transcript, every
  task with its status and its text in full, a task still to do drawn filled or hollow as it is here.
  The plan ends with a row that names the key, unless the
  room is under three rows, the plan fills the room to its last row, or the row would take the place
  of a task still to do. The `?` list always names it.

Its last row names the key that hides it. While it is drawn, the context and cache figures leave the
hint line under the box, unless the terminal is too short for the panel to show them; the mode, a
running loop and the job count stay there. It needs a terminal at least 100 columns wide: a press
to open it on a narrower one leaves a note, and narrowing the terminal while the panel is open hides
it until the terminal is wide again. A press to close it works at any width. Whether it was left open is kept in
`~/.bravebot/panel` for the next session, and the width you dragged it to in `~/.bravebot/panel-width`. The panel draws only what you typed, the plan, the
session's own counters, the servers you approved and where the session runs, and never a reply or anything a tool returned.

### The files the session wrote

Type `/diff`, or click the count in the Files section of the panel, and the panel lists every file the session's write calls changed, in the order each was
first written, with the lines added and removed at the right edge. Under the list are all the edits to
the highlighted file, oldest first, each under a header with its place, its turn and its counts, with
line numbers in a gutter and every change drawn, where the transcript draws the first twelve of each
write. The view is half the terminal's columns, between 60 and 100, and needs a terminal at least 100
columns wide.

| Key | Does |
|---|---|
| `↑` `↓`, `k` `j` | highlight the previous or next file |
| `Home` `End`, `g` `G` | highlight the first or last file |
| `PgUp` `PgDn` | scroll the edits |
| `←` `→`, `h` `l` | scroll to the previous or next edit |
| `q`, `Esc`, `Ctrl-C` | leave the view; the panel is as it was before, and a turn running goes on |

A bar under the file's name shows which edit is at the top of the edits, as `edit 2 of 3`, with a
control either side that puts the header of the previous or the next edit at the top. A control with
no edit to go to is dim and does nothing. The edits say how many of their rows are above and below the
ones drawn. The last row of the panel names these keys, and the hint line under the box keeps the
context reading and the cache rate. Where the terminal is too narrow to draw the panel, the hint
line names the keys as well.

With the mouse, clicking a file in the list highlights it and shows its edits from the top, clicking
a control of the bar steps between the edits, and clicking the `x` at the right end of the heading row
leaves the view as `q` does. The wheel scrolls the pane the pointer is over: the list, without moving
the highlight, or the edits, by the number of rows `tui.wheelRows` names, and the transcript where the
pointer is over it. A key that moves the highlight brings it back into view. A drag, a double click or
a triple click still selects text.

Every other key does nothing there, and the line in the box comes back as you left it. Edits that
finish while the view is open are added without moving the highlight or the scroll. The view lists
what the session's own `write_file` and `edit_file` calls and manifest write steps changed. A file a
program changed, a file you edited, and a write a delegate made are not listed. The session record
keeps no write's changes, so a resumed session lists only the edits made since it was resumed, and the
heading says so. `/clear` removes the list, and `/undo` and `/rewind` remove the edits of the turns they
undo.

```
/issue https://github.com/brave/bravebot/issues/1267
/pr https://github.com/brave/bravebot/pull/1
```

Each takes one `http` or `https` link with a host and refuses anything else, a link with a space, a
line break, an escape or a character outside ASCII in it included. Alone, each says what is set, and `/issue clear` or `/pr clear` removes
one. Both are written to the session record at once, a resume brings them back, and `/clear` starts
a session with neither.

## Sessions in the background

`bravebot --bg "<prompt>"` starts a session whose process keeps running after the terminal closes,
and [`/bg`](../reference/commands.md#bg) hands a running session to one. Only a line you typed starts
one: the planner, a delegate, a hook, a loop tick or piped input cannot. `--bg` asks about everything,
as a new session does. `/bg` keeps the permission mode the session was in. Bypass is refused, and a
rule in a settings file that allows something does not answer a question for you there. A
background session states its mode on its opening lines and again when `attach` joins it. The
refusal of bypass says that a session nobody watches does not run in a mode that answers every
question, and names a one-shot run (`-p`) as the way to ask for that. `--dangerously-bypass-approvals-and-sandbox`
is refused the same way. With no model service configured, `--bg` is refused in the terminal you
typed it in, with the configuration status and the five routes (Bedrock, an OpenAI-compatible
gateway, a ChatGPT sign-in, an xAI sign-in, Brave's premium service), and no background process is started.

| Command | What it does |
|---|---|
| `bravebot sessions` | one line per background session of every project: id, name, state, directory, time since it was last written and your last prompt. In a terminal it opens a list grouped by state, in the words and the order the [Sessions screen](./sessions-screen.md) uses (`Ready` where a line says `idle`), where Enter attaches |
| `bravebot sessions --json` | the same fields as JSON |
| `bravebot attach <id>` | draw the session as a foreground one, with a held question put to you. One terminal attaches at a time |
| `bravebot reply <id> "<text>"` | send the text as the session's next prompt. It is refused while the session is working or waiting for an answer, and it never answers a held question |
| `bravebot sessions stop <id>` | stop the turn and the process. The record is kept |

A session is `working`, `needs input`, `idle`, `interrupted` or `stopped`. A question the session would
ask is held until you attach and answer it, however long that takes. That includes the question about
starting an [MCP server](../customize/mcp-servers.md) nobody has approved, which is held as
`needs input (server)` and drawn as a foreground session draws it. The question about sending a
[message to another session](../reference/tools.md#message_session) is held the same way. Closing the terminal, losing the
connection or typing `/detach` leaves the session running, with a turn in progress and a held question
unchanged. `/detach` is not sent to the planner.

A session with nothing running and no terminal attached for an hour is stopped, and the next `attach`
or `reply` starts it again from its record in the mode every session opens in. A process that ends
between turns is started again, up to three times in an hour, idle and in the directory it worked in. It
takes the environment of the terminal that ran the first `--bg`, so a variable exported in a later
terminal does not reach it. One that ends during a turn is marked
`interrupted`, the turn is not run again, and the planner is told so on the next prompt. In a git
repository the session works in a checkout of its own, so two sessions do not edit one tree. A
session running in the background cannot be resumed from another terminal: `--resume` and
`--continue` say to attach. The Sessions screen that `--resume` and `--from-pr` open treats such a record the
same way: Enter on its row says to attach. Attaching and replying are accepted only from a program that can read the
session's state directory, because each start writes a fresh random secret there that the session's
socket requires. A confined stage cannot read that directory.

## Deleting a session

`bravebot sessions delete <id>` removes one session of this directory, taking the id from
`bravebot sessions list`. `bravebot sessions purge` removes all of them; it asks first, naming the
directory and how many, unless you pass `-y`, and `--dry-run` prints what it would remove and
removes nothing. Each deletes the record, its trail, its search index and its lock, and nothing else: your
prompt history is one file for every project and stays, and so do the sessions of other
directories. A session running in the background is left until you stop it with
`bravebot sessions stop`. A session open in another terminal is not seen, and its next save writes
its record back, so close it first. Both work under `--incognito`.

Nothing needs to be asked for to remove an old one: when a session starts, every record nothing has
written for [`retentionDays`](../customize/configuration.md#retentiondays) days is removed, with its
trail and its search index, and the session you are starting is left whatever its age. A record that
is gone cannot be resumed, forked or searched, so run `/export` first to keep the words.

## Importing sessions from Claude Code

```sh
bravebot sessions import claude-code                  # list this directory's Claude Code sessions
bravebot sessions import claude-code 3f2a9c1e         # copy one, by the start of its id
bravebot sessions import claude-code --all --project ~/work/app
```

With nothing named it lists the sessions Claude Code kept for the directory, newest first, and
copies none. Each one named, or all of them with `--all`, becomes a record here called
`claude-code-<its id>`. A session that records no directory is skipped, as is one that belongs to
another directory.

What is copied is what you typed and what the model answered in words. Tool calls and results, the
model's reasoning, pictures and Claude Code's own notes are left behind. The words are in the record
to be read: `bravebot --resume` draws them, and none of them is sent to the model, so a resumed copy
does not know what the old session said. It asks the trust question as a new session does and
carries no permission from the old one. Copying the same session again changes nothing, and
Claude Code's files are not read again afterwards. Because the words are never part of a request, the
desktop app does not fork a session that holds them; `/branch` in the terminal still copies it.

The record comes from the files Claude Code keeps under `$CLAUDE_CONFIG_DIR`, or `~/.claude`.
`bravebot sessions import opencode` is refused: opencode keeps its sessions in a database file this
build cannot read.

## Starting over

```
/clear
```

`/clear` begins a new session in the same directory and keeps the current one resumable. Because it
is a new session it asks the trust question again, unless a yes was
[kept](../security/trust.md#remembering-the-answer) there, restores no standing
permissions, and closes any directory `/add-dir` had opened.

## Starting a plan-then-execute run

[`/manifest`](../reference/commands.md#manifest-task) starts one plan-then-execute run and hands
the session back afterwards. [Plan first, then execute](manifest-mode.md#starting-a-plan-then-execute-run)
has the rest.

## Taking turns back

[`/undo`](../reference/commands.md#undo) puts the session back where it stood before the most recent
turn, and **saying it again goes back another turn**, as far as the last five.
[Taking turns back](rewind.md) covers reading a rewind before running one, putting back one hunk,
what will not go back, what gives up every point, and what a resume keeps.

## Prompt history

Up walks backwards from the most recent prompt and stops at the oldest; Down walks forwards again.
Leaving the newest entry puts back the half-written line you were on. Submitting leaves the mode, and
a prompt arriving while you browse does not shift the view.

History persists across runs under `~/.bravebot/history` and is capped. Consecutive duplicates
collapse into one, and a prompt you cancelled is removed again. Only that one line is removed from the
file, so prompts other sessions saved in the meantime stay.

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

asks for that work on demand, at any size, without consulting the budget. `/compact keep the lexer
benchmarks` says what the summary must keep.

The budget is the window the endpoint advertises for **whichever model is in force**, where it
advertises one, so a one-shot run and a session both get the window of the model they are asking for.
The built-in default only stands in, for a model resolved per request and for an entry that advertises
nothing, including when you switch to such a model from one that advertised a window: the default
replaces the earlier figure rather than leaving it standing. See
[Configuration](../customize/configuration.md#context-budget) for setting it by hand.

A cut has to free more than it keeps, so a conversation with nothing worth giving up is left long
rather than summarised once per round. That is measured twice, in exchanges and in bytes: a cut that
would summarise two short prompts and keep tens of thousands of tokens of file reads is not made, and
the cut by round is tried instead.

A skill the planner loaded before the cut is not left to the summary. Its text is sent again after
the summary, word for word: the newest load of each skill, the first 5,000 tokens of any one skill
(estimated at four bytes a token) and 25,000 tokens for all of them, newest first. A skill that
is cut says so and names `load_skill` for the rest. These count as kept when deciding whether a cut
frees enough. A skill whose result was sent as prose, which is the fallback for a round whose calls
carried no ids, is summarised like any other message.

The planner's task list is kept the same way. When a compaction takes the latest accepted
`todo_write` list out of the conversation, the summary ends with that list word for word, so the
steps and their order survive. A list still in the kept part is not repeated, a refused `todo_write` is
not the latest list, and the copy is carried through later compactions until the planner writes a
newer one.

Compaction never touches three things: the quarantine, which holds the only copy of what a surviving
reference names; the reference counter, since a slot name handed out twice would collide; and the
context's integrity, since nothing here has un-read what the conversation read. The cut never lands
inside a round, so a call is never separated from its results.

## A session that leaves nothing behind

`bravebot --incognito` runs a session that leaves no prompt history, session record, title or
audit trail. [Incognito sessions](incognito.md) covers what it still reads and what it does not
cover.

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

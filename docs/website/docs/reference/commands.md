---
sidebar_position: 2
title: Slash commands
description: The twenty commands the interface acts on itself, and the rules every one of them shares.
---

# Slash commands

A line beginning with `/` is acted on by the interface itself, in place of being sent anywhere.

| Command | Argument | What it does |
|---|---|---|
| `/status` | | Report this session, what it may touch, and what it has spent |
| `/cost` | | Show what each turn of this session has spent |
| `/model` | | Choose which model to think with |
| `/theme` | `[name]` | Choose which theme paints the interface |
| `/effort` | `[level]` | Choose how hard to think before answering |
| `/config` | | Choose how the input box edits text |
| `/add-dir` | `<path>` | Open another directory, and trust it for this session |
| `/cd` | `<path>` | Work in another directory from now on, and trust it for this session |
| `/rename` | `<name>` | Call this conversation something else |
| `/compact` | | Summarise the conversation so far, keeping the recent part |
| `/btw` | `<question>` | Ask something beside the work, without putting it in the conversation |
| `/clear` | | Start a new session here, keeping this one resumable |
| `/loop` | `[interval] <prompt>` | Send a prompt again and again, on your interval or at a pace each turn sets |
| `/goal` | `[<condition> \| clear]` | Keep working until a condition you set is judged met |
| `/watch` | `[stop <n>]` | List the files this session is watching, and stop one by its number |
| `/manifest` | `<task>` | Plan one task in full, show you the plan, then run it with nothing re-planned |
| `/export` | `[path]` | Export the session transcript to a markdown file |
| `/undo` | | Rewind one turn and put back the files it wrote |
| `/rewind` | `[turns]` | List the turns a rewind could go back to, or go back that many |
| `/exit` | | Leave |

Typing `/` offers the list in that order, and Tab completes. The list is one row per command, and a
terminal without the room for all twenty drops the last of them: every command is still typeable in
full, but a short terminal costs you the discovery the list is there for.

## `/status`

Reports everything the session knows about itself:

- the working directory, and anything opened with `/add-dir`;
- the model in force, and whether it was chosen or defaulted. Where the server substituted a
  different one, the model that actually answered is shown beside it;
- the [effort level](#effort-level), and whether this model reads one;
- which deployment the endpoint names, and **which tier the last turn ran on**, rather than which
  tier the build was compiled to reach;
- the confinement available here;
- turns and tokens spent, and **where the time went**: how much was spent waiting on the model,
  running tools, and waiting for you to answer a prompt;
- **every trust rule in force**, listed in full, each marked trusted or untrusted;
- **every command you vouched for**, which now run unasked and whose output is read as trusted;
- what a [`/loop`](#loop-interval-prompt) is repeating and when the next tick is due, where one is
  running, or what a [`/goal`](#goal-condition) is working towards and how many rounds it has
  spent.

The last three are the ones nothing else on your screen tells you. A vouched command is the one that
stops appearing, and what happens next without anybody typing anything cannot be read off the
transcript.

The endpoint host and the key id are left out, though `bravebot doctor` prints both. A status panel
is the thing people paste into an issue or a screenshot.

## `/cost`

Reports what the session has spent as a total, and under it **one figure per turn with that turn's
share of the total beside it**. What was spent before the first turn is reported too, without a turn
number, since no turn did it.

A total cannot tell twenty even turns from one that ran away, and the share is what makes the second
one visible without your dividing each row by the total. It is a word of its own rather than more rows
on [`/status`](#status) because the list grows with the session: a panel that answers "what is this
session" in fifteen rows would answer it in fifty on the fiftieth turn.

:::caution
**The figures are tokens, and they are not a bill.** A prompt a service answered out of its own cache
is charged at a fraction of a fresh one, so two turns recorded at the same figure can differ about
tenfold in money. The breakdown keeps no cache split per turn, so a figure in money would be composed
here rather than measured. What is written stays comparable across turns and across sessions, which is
what it is read for.
:::

## `/model`

Opens a picker on the model in use. The list comes from the endpoint rather than a set compiled in, so
it is whatever the backend offers today. The choice is written to `~/.bravebot`, so it outlives the
session and applies in every directory.

Typing narrows the list rather than walking it, and rows are grouped under the service that answers
them. See [Configuration](../customize/configuration.md#choosing-a-model).

## `/theme [name]`

Opens a picker on the palette in force. With a name, `/theme nord` applies it without opening the
panel.

Up and Down move the cursor, and the theme under it is put in force while it is selected, so you are
comparing themes against your own transcript rather than against a sample. Enter keeps the one on the
cursor and Escape restores the one that was in force when the picker opened.

The choice is written to `~/.bravebot`, so it outlives the session and applies in every directory.
Themes of your own are JSON files under `~/.bravebot/themes/`, and nothing in a workspace is read. See
[Choosing a theme](../customize/configuration.md#choosing-a-theme) and
[Themes](../using/transcript.md#themes).

## `/effort [level]`

Opens a picker of the five levels (`low`, `medium`, `high`, `xhigh` and `max`) above a row for
asking for no level at all, so a first pick is not permanent. With a word, `/effort high` takes it
directly, and a word that names no level changes nothing and says so rather than reaching a request
field.

The choice is written to `~/.bravebot`, so it outlives the session and applies in every directory. See
[Choosing how hard to think](../customize/configuration.md#choosing-how-hard-to-think), which is also
where the two cases worth knowing are: the models that read no level, and the Brave endpoint, which
accepts one and discards it.

## `/config`

Opens a panel over the transcript for a preference about the interface. It lists the choices with
what each one means and marks the one in force. Enter takes the row under the cursor and says so on
the transcript; Escape leaves the setting alone, which is what makes the panel safe to open just to
see what is set.

It holds one choice today: whether the input box edits the ordinary way or
[vi's](../using/interactive-mode.md#editing-the-way-vi-does). The choice reaches the transcript
because it changes what the next keystroke does and the box gives no other sign until a letter has
gone somewhere unexpected. Whichever style you choose, the box comes back taking letters as letters.

The choice is written to `~/.bravebot`, so it outlives the session and applies in every directory.
See [`editorMode`](../customize/configuration.md#editormode) for the settings key that answers for
somebody who has never used this panel.

## `/add-dir <path>`

Makes a directory both reachable and trusted, for this session. `--resume` carries both halves and
`/clear` closes it. A directory already inside the project is refused. See
[Trusted directories](../security/trust.md#add-dir).

An added directory contributes **no** standing instructions and no skills, whatever it contains.

## `/cd <path>`

Moves the working directory. From then on that is what a relative path means, where a program runs,
where `AGENTS.md` and the project's skills are looked for, and what `@` completes against. The path is
taken against where the session is now, so `..` and a name inside the project both work, and the
directory is trusted for the session on the same terms `/add-dir` grants.

The directory you left closes, and so does anything `/add-dir` had opened that holds the new working
directory or sits inside it. Each is said out loud as it happens, with the line that opens it again.
Nothing may overlap the working directory, because a file reachable both relatively and by absolute
path would have a rule in each namespace and so two answers.

Your trust map comes with you rather than being carried over unchanged. See
[Moving the working directory](../security/trust.md#moving-the-working-directory) for what happens to
each rule, and [Sessions](../using/sessions.md#a-session-that-changed-directory) for where the record
goes.

## `/loop [interval] <prompt>`

Sends one prompt again and again until you stop it.

```
/loop 5m check the deploy          # now, and every five minutes
/loop check the deploy every 20m   # the same, written the other way round
/loop watch the build              # now, and each turn says when the next is due
```

The first tick goes at once, so you can see it happen while you are still watching. The gap is
measured from the end of a tick rather than its start, so `every 5m` means five minutes between runs.
A due tick waits for an idle session and never interrupts, and a prompt you type in the middle of a
loop is not a tick of it.

An interval is read off the front of the argument, or off an `every` clause at the end, in that order
and nowhere else. A leading token counts only when it is a number and one of `s`, `m`, `h` or `d`. A
trailing clause counts only when `every` is a word of its own and a time expression is the whole of
what follows it. Given no interval, each turn says when the next tick is due.

| The argument | The interval | The prompt |
|---|---|---|
| `5m check the deploy` | 5 minutes | `check the deploy` |
| `check the deploy every 20m` | 20 minutes | `check the deploy` |
| `check the deploy every 20 minutes` | 20 minutes | `check the deploy` |
| `check every PR` | none, so each turn paces it | `check every PR` |
| `check everything 20m` | none, so each turn paces it | `check everything 20m` |
| `5m check the deploy every 20m` | 5 minutes | `check the deploy every 20m` |

That is what keeps `/loop check every PR` a sentence rather than one with its last two words taken
off, and a word that merely begins with those five letters, such as `everything`, is not the clause at
all.

**The line a loop repeats is the one you typed.** It is settled the moment you press Enter and sent
unchanged for the life of the loop: nothing a turn reads, writes or returns can add to it, edit it or
replace it. A turn that could write its own next prompt would be rewriting its own instructions, and
the point of a loop is that it asks the same question again.

**A tick is a prompt, never a command.** `/loop 5m /status` sends the seven characters `/status` to
the planner every five minutes; it does not run the status command. A command is dispatched from a key
press, and a timer is not one.

| The wait | Shortest | Longest |
|---|---|---|
| an interval you gave | 5 seconds | 7 days |
| a delay a turn asked for | 1 minute | 1 hour |

A number outside those becomes the nearer bound, and you are told what it became rather than left
believing you are watching something ten times more closely than you are. A turn's number is held far
more tightly than yours because a turn that wants longer than an hour can say so in its answer, where
somebody reads it. Where you gave an interval, no turn can change it; a self-paced tick that says
nothing is woken once more twenty minutes later, and a second silence ends the loop.

Each tick is announced with its number, and with how many in a row have reported finding nothing.
That count is the difference between a loop that is working and a loop with nothing to do. Four
things end one, and each says so:

| What | When |
|---|---|
| you interrupt | Ctrl-C, reached after the turn in flight and the half-typed line, and before leaving |
| a turn is stopped | any turn cancelled while a loop runs, tick or not |
| the session moves on | `/clear`, and leaving |
| age | seven days after it started |

**A loop, a [goal](#goal-condition) and a [watch](#watch-stop-n) are never live together**, because a
session does one of the three at a time. Whichever of a loop and a goal was asked for second stands,
and the one it replaced is reported as stopped. Asking for either ends every live watch.

**A loop is never written down.** It is not in the session record, so `--resume` restores none and it
does not outlive the process. A schedule that survived the session that set it would start sending
prompts at somebody who opened a conversation only to read it.

:::caution
**A loop keeps spending.** Every tick is a turn with the whole conversation re-sent, and nothing bounds
the total but the interval and the session's own life. A five-minute loop left open overnight is a
hundred and fifty turns nobody read.
:::

## `/goal <condition>`

Keeps the session working until a condition you wrote is judged met. When a turn ends the condition
is put to a judge, and where it is not met yet the work goes back for another turn with the reason.

```
/goal cargo test exits 0 and the diff is committed
/goal                              # say what the condition is, and how it is going
/goal clear                        # take it off
```

`clear` takes a goal off only when it is the whole argument, so `/goal clear the build directory and
the tests pass` is a condition like any other.

**Setting a goal sends nothing.** A condition is not a prompt, so the session sits idle until you
ask for something; what a goal does is keep that work going. Nothing here writes a first prompt for
you, because there is no line you endorsed to send.

**The condition is the one you typed.** It is settled the moment you press Enter, and nothing a turn
reads, writes or returns can add to it, edit it or replace it. Only another `/goal` changes it. The
condition is what decides when the session is allowed to stop, and a turn that could write its own
would be deciding when it has finished.

The check is one request with no tools over a copy of the conversation, the shape
[`/btw`](#btw-question) uses, so the conversation the next turn resumes is the one that was already
there. Nothing the judge said arrives as your words either: what carries the work on is a sentence
written by this program, naming the condition and quoting the reason inside it.

One answer carries the work on, and everything else ends the goal:

| What came back | What happens |
|---|---|
| the condition is not met yet | another turn, with the reason |
| the condition is met | the goal is over, and the reason is what you are shown |
| the condition can never be met | the goal is over, and the reason says why |
| an answer that is not one of those | the goal is over |
| the check itself failed | the goal is over |

A verdict is the first line of the answer and one of three words. Prose is not a verdict: reading
one out of a sentence nobody constrained would let the judge's wording decide whether your session
keeps working, and a sentence saying the condition is nearly met would read as either answer
depending on which words were searched for.

**Ten rounds and it gives up.** The last reason is kept, so a goal that has given up can still say
what it kept hearing. Four things end one besides a verdict, and each says so:

| What | When |
|---|---|
| you ask | `/goal clear` |
| you interrupt | Ctrl-C, reached after the turn in flight and the half-typed line, and before leaving |
| the session moves on | `/clear`, and leaving |
| the rounds run out | the tenth |

**Stopping a turn leaves the condition set.** Neither a turn you cancelled nor one that failed is
judged: a request that never came back says nothing about whether the work is finished, and an
interrupted turn says only that you did not want that turn. The goal stays set, and what is judged is
the next turn there is something to judge.

That is what makes a goal steerable. Stop the turn, say something else, and the condition is still
there; the press that ends the goal is the one you make with nothing running.

A check already in flight is one request and does not stop, but Escape and Ctrl-C still take the
goal off, and nothing more is sent. A verdict about a goal you have just taken off is neither acted
on nor reported.

**A goal is never written down.** It is not in the session record, so `--resume` restores none and it
does not outlive the process. A condition judged against yesterday's conversation would start
working a session somebody opened only to read.

**A condition only somebody else can satisfy is waited for inside the turn.** Where the work is a
file you have yet to write, the turn sleeps and looks again rather than answering to be sent back:
one round of the ten costs a whole turn plus a judge's reading of the conversation, and a wait costs
a command. A command is killed at its deadline, which is at most ten minutes, so a longer wait is
repeated sleeps, and a condition hours away is not what a goal is for.

:::caution
**The judge reads the transcript, not the world.** It cannot run a command or open a file, so a
condition holds when the conversation shows it being observed: a turn that fixes something and never
checks the fix is sent back for not checking it. A condition no transcript could show, `the code is
clean`, spends all ten rounds and gives up, and nothing warns you in advance. Every round re-sends
the whole conversation, so ten rounds of a long session cost more than ten ordinary turns.
:::

## `/watch [stop <n>]`

Lists the files this session is watching, and ends one by its number.

```
/watch             # what is live, numbered
/watch stop 2      # end that one, leaving the others
```

**It arms none.** A watch is asked for in a prompt, and what a command is needed for is the half you
cannot read off the transcript: which watches are live, and how to end one. See
[Watches](../using/watches.md) for what a watch observes and what a firing puts in the conversation.

| Bound | Value |
|---|---|
| a watch's age | 7 days, after which it ends itself and says so |
| live watches in one session | 8, and arming a ninth is refused rather than dropping one |
| between two fires of the same watch | 5 seconds, measured from the end of the turn the last fire started |
| between two looks at a watched path | at most 5 seconds, which is what a firing's latency is |

Seven things end a watch and each says so: `/watch stop <n>`, Ctrl-C with nothing nearer to stop
(which ends every live watch), a firing's turn being stopped, asking for a
[`/loop`](#loop-interval-prompt) or a [`/goal`](#goal-condition) (a session does one of the three at a
time), the path ceasing to be readable, `/clear` and leaving, and age.

**A watch is never written down**, so `--resume` restores none and none outlives the process.

## `/manifest <task>`

Plans one task in full, shows you the plan, then runs it with nothing re-planned.

```
/manifest add a --verbose flag, wire it through, and add a test
```

**It is a run, not a mode the session holds.** The session starts one run, waits for it, and comes
back to the turn loop. It is blocked for the duration: you can read, edit and stop, but not send.

The conversation is neither read nor written. The task string is what the planner gets, so nothing
from the conversation goes in, and a step's result is quarantined with no planner left to show it to,
so nothing comes back out. What the transcript shows is the goal as the planner understood it, the
frozen plan, each step as it runs, and the reply. The run leaves the conversation exactly as a
declined plan leaves the workspace.

**You approve the plan before the first step**, once, and the approval does not cover the writes. A
run you stopped is not written down, because there is nothing in it to read. The run is recorded as its
own record and the session records its name, so the session still resumes as a conversation.

In [plan mode](../security/permissions.md#answering-in-advance-modes) a plan with a write in it does
not run at all, decided from the frozen plan before the plan is put to anybody. See
[Non-interactive use](../using/headless.md) for the `--mode manifest` form.

## `/rename <name>`

Rewrites the session record immediately, and the chosen name survives the next turn. An empty name is
refused.

## `/compact`

Summarises the conversation so far and keeps the recent part, on demand, at any size, without
consulting the budget. The **request** is shortened, never the record: the replaced messages go to an
archive that the transcript still reads and the session record still stores. See
[Sessions](../using/sessions.md#long-conversations).

## `/btw <question>`

Asks something beside the work. A copy of the conversation goes out with your question on the end of
it, and neither half comes back into the conversation: the planner has read neither your question nor
the answer, and no later turn reads either.

The answer opens in the mode
[Ctrl-L](../using/interactive-mode.md#watching-a-delegate-reading-a-command-and-asking-something-aside)
opens, as a row of its own before the delegates and the commands, which is the one screen it exists
on. Nothing about it is drawn among the turn's own lines, because an exchange drawn in the transcript
is one a reader takes the planner to have had.

One request, and no tools in it. **An aside cannot be asked about**: there is no box to follow up in,
so pressing further means a second `/btw`, over an exchange that still knows nothing of the first.

**The record keeps both halves**, so a resume brings the answer back into that view and into no
conversation. Where the exchange has already met something untrusted the answer is not written down:
the row says so while the words are still on screen to copy, and a resume brings back the question
alone. That is the same rule every message in the record passes, which is that nothing is written
that the planner could not have held.

## `/clear`

Begins a new session in this directory and keeps the current one resumable. Because it is a new
session it asks the trust question again, restores no standing permissions, and closes any directory
`/add-dir` had opened.

## `/export [path]`

Writes the transcript out as a markdown file under the working directory, at the path you name or at
`bravebot-export-<id>.md`. Without this the only way to get a conversation out is to read the session
record's JSON out of the state directory by hand.

The path is typed on the same line as the command, so it gets the confinement any other path from
that line would get: `..`, an absolute path and a drive prefix are refused, and containment is then
tested against the real location of the deepest directory that exists, so a path leading out of the
tree through a symlink is refused as well. Missing parent directories are created.

**Anything already at the path is refused rather than replaced**, a symlink whose target is missing
included. The file is written readable by you alone, as the record it came from is.

## `/undo`

Puts the session back where it stood before the most recent turn, and **saying it again goes back
another**. Every path in the project a rewound turn wrote through a file tool goes back to what it
held first, and a file one created is removed; where two rewound turns wrote the same path, it goes
back to what it held before the first of them.

The conversation returns to the snapshot taken before the earliest rewound turn, and the turn count,
the spend, the timing, the trust map, the commands you vouched for and the transcript go back with it.
Those turns' audit lines are dropped, since they decided about turns that are no longer in the
conversation. A standing permission goes back with the turn that granted it, so a path or a command
vouched for during a rewound turn is vouched for no longer, and one vouched for before them is
untouched. Rewinding past a session's first turn removes its record rather than leaving one with
nothing in it, and a name you gave the session before that turn stays with it.

Disk and conversation move together because either alone leaves the transcript describing a tree that
is not there.

**Five turns back is as far as it goes.** The turn that just ended is the one least likely to need
rewinding, because it is the one still on the screen; what people notice late is a mistake made two or
three prompts ago, after approving several diffs in a row. Depth stops at five because every point
holds a copy of the conversation as well as the bytes, and one is written after every turn whether or
not it is ever read.

**A rewind names any file it could not put back**, and the rest of the rewind still happens. What is
kept is bounded twice over: a session remembers its last five turns, and what those turns wrote over
is held to one budget between them rather than one each. Past it the turns furthest back are dropped
whole, and the most recent is kept whatever it cost. Inside a turn the same budget decides a path:
past it the path is still remembered but its contents are not, and a rewind reports it as a path that
would not go back rather than as a file that was never there.

**The points survive closing the program.** They are written into the session record with the
conversation, so `/undo` and `/rewind` after a `--resume` reach the same turns they reached before.

**Anything that changes the session outside a turn gives up every point at once**: `/clear`,
`/compact`, `/btw`, `/rename`, `/add-dir`, `/cd`, and a shell-mode command, whose writes the workspace
never saw. Every point goes rather than the most recent alone, since such a change lands after the
most recent point and so before none of them. After that `/undo` says there is nothing left to undo
rather than rewinding to a point describing a different session.

## `/rewind [turns]`

Reads the rewind points before acting on one.

```
/rewind          # list the points, most recent first, numbered from one
/rewind 3        # go back three turns
```

Each row says how many turns back it is, which turn it would land before, what that turn was asked,
and **every path that turn wrote over**, or that it wrote over none. `/rewind <n>` then goes back that
many turns, which is what `/undo` said n times does, so what it puts back is every row from the first
down to the one chosen.

A rewind acts the moment it is typed and it overwrites files, including edits you made yourself since
the turn. Deciding to run one is deciding about those files, so they have to be readable first, for
the same reason a write is shown as a diff before it is approved rather than reported after. The paths
are named rather than counted, because a count decides nothing.

`/rewind` given something that is not a number says what it takes. **A number past what the session
remembers rewinds nothing** and says how far back it does go, rather than going as far as it can:
somebody who asked for four turns and got two would be reading a tree two turns younger than they
believe it is.

It is a word of its own rather than an argument to `/undo` because a command that takes no argument is
only ever the bare word, which is what keeps `/undo the last thing I asked for` a prompt. A surface
that takes a number cannot also have that.

:::caution
**A rewind sees file-tool writes and nothing else.** A turn that changed a file by running a program
leaves nothing to put back: those changes stay on disk while the conversation says the turn never
happened. And what goes back is what a path held *before the turn wrote to it*, so an edit you made
yourself in between is lost. Nothing compares the file against what the turn left there, and nothing
asks first.
:::

## The rules every command shares

**Only a line a person typed into the box.** A command is dispatched from a key press and from nowhere
else: never a line the planner produced, never text read out of a file, never anything a processor
returned, never a line reconstructed from a transcript. A model that writes `/clear` has written four
characters, and they reach your screen as four characters.

Every command here decides something a turn is not allowed to decide on its own: which directories are
reachable, what the conversation consists of, which model thinks. The endorsement is the keystroke, so
the keystroke is the only thing that may produce one.

**The whole word, and an argument only after a space.** `/statusline` is not `/status`, and
`what does /add-dir do` is a question. The set of words this program claims is taken out of the
language you can use to talk to the planner, so it is claimed as narrowly as possible. The bare word
with nothing after it is the command with an empty argument, answered by saying what it needs rather
than by doing nothing quietly.

**In shell mode the line is a command line, not a command.** `! /usr/bin/env` runs a program. Nothing
is offered for completion there either, since `/usr/bin/env` is a path, and a turn running changes
none of that: the line waits behind its `!` and is run when the turn ends, rather than being answered
as a command or sent to the model. [Shell mode](../using/shell-mode.md) is the rest of it.

**A command is never sent as a prompt.** A line that is a command is acted on and does not reach the
model. A session asked to shorten itself must not answer by talking about shortening itself.

**The argument is taken verbatim**, spaces and all, with the surrounding whitespace trimmed and
nothing else done to it. A leading `~` is expanded only as a whole first segment, so a directory whose
own name begins with a tilde is not a home-relative path. Nothing shortens it, splits it, or asks the
planner what it meant.

**While a turn runs the word waits.** A command typed mid-turn comes off the box and joins the lines
waiting for the turn to end, exactly as a prompt does: the box clears, the history remembers it, and it
is drawn under the box marked as waiting. It is never offered to the turn in flight, so nothing about
it reaches the planner, and when the queue reaches it, it is carried out rather than sent. The queue
drains in the order you typed, so a command behind a prompt waits for that prompt's turn. Nothing
enters the transcript while it waits, and taking back what is waiting gives the command back to the
box like any other line.

**A command name is written in this program, never read from a directory.** There is no way to add one
by putting a file somewhere.

## Skills are not slash commands

`/commit-style` is a prompt like any other sentence, even where a skill of that name exists. Other
agents let you type a skill's name after a slash. This one does not: a skill is advertised to the
planner by name and description, and its body is fetched by the planner asking for it. Nothing in the
input box knows skills exist. See
[Skills](../customize/skills.md#skills-are-not-slash-commands).

## Not a command, but typed in the same place

| | |
|---|---|
| `@<path>` | include a workspace file as trusted context. [Adding context](../using/context.md) |
| `!<line>` | run a line in your own shell. [Shell mode](../using/shell-mode.md) |

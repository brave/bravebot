---
sidebar_position: 1
title: Interactive mode
description: The input box, every key, and what a running turn refuses.
---

# Interactive mode

```sh
bravebot
```

The input box grows with what you type, up to ten rows, and then scrolls to the caret rather than
growing further. It keeps growing while a turn runs.

## Sending and editing

| Key | What it does |
|---|---|
| Enter | send |
| Shift-Enter, Ctrl-J | start a new line without sending |
| Ctrl-G | compose in `$VISUAL` or `$EDITOR` and take back what you saved |
| Ctrl-S | put the line away, or bring back the one you put away |
| Escape | discard a half-typed prompt, or stop a running turn |
| Ctrl-C | stop the nearest thing there is to stop, and leave when there is nothing left |
| Up / Down | walk back through prompts you have sent |
| Ctrl-R | search every prompt you have sent |
| Tab | complete a slash command or an `@path` |
| Shift-Tab | choose how much the session asks before it acts |
| `?` | on an empty line, list every key |

Enter on an empty line does nothing. Shift-Enter needs a terminal that reports the modifier
(Ghostty, Kitty, WezTerm) or one configured to send a newline; **Ctrl-J is the fallback that always
works**, in every terminal and in shell mode too.

With [vi editing](#editing-the-way-vi-does) chosen, Escape enters NORMAL mode instead of discarding
the line, and the letters do what they do in vi.

## Editing the way vi does

[`/config`](../reference/commands.md#config) chooses between the ordinary box and vi's editing keys.
The choice is written to `~/.bravebot`, so it outlives the session and applies in every directory,
and [`editorMode`](../customize/configuration.md#editormode) in a settings file answers for somebody
who has never made one.

Vi editing has two modes over the same line. **INSERT** is the box everybody has. **NORMAL** takes a
letter as an instruction, and a letter it has no instruction for does nothing at all rather than
being typed. Every session opens in INSERT, whichever style is in force, and the mode is drawn
beneath the box beside the mode that says how much the session asks. The ordinary box is in neither
mode, and nothing about a mode is drawn at it.

**Escape enters NORMAL mode and leaves the line exactly as it was.** Discarding a half-typed line is
still Ctrl-C, and a turn in flight is still stopped first. Ctrl-`[` is the same request from a
terminal that reports the modifier rather than sending the byte Escape already is. In the ordinary
box Escape discards the line as it always has.

### Getting back into INSERT mode

| Key | Where the caret lands |
|---|---|
| `i`, `a` | before or after the character the caret is on |
| `I`, `A` | the first character of the line, the end of the line |
| `o`, `O` | a new line below, a new line above |

Leaving INSERT mode puts the caret on a character rather than past the end of the line, since in
NORMAL mode it sits on the character the next instruction acts on.

`!` and `?` are instructions in NORMAL mode rather than the marks that arm
[shell mode](shell-mode.md) and put the key list up. Both are a press of `i` away. Reading `!` as the
mark would arm a shell from a press asking for something else, and there is no way out of a shell
armed by accident except deleting back past the mark.

### Motions

| Keys | Where the caret goes |
|---|---|
| `h`, `l`, Space | one character left or right |
| `w`, `e`, `b` | the start of the next word, the end of this word or the next, the start of this word or the previous |
| `0`, `$`, `^` | the first column, the last character, the first character that is not a blank |
| `gg`, `G` | the first line of the input, the last |
| `f`, `F`, `t`, `T` then a character | the next or previous occurrence of it on this line, landing on it or stopping one short |
| `;`, `,` | that jump again, and the same jump reversed |

A jump looks only along the line the caret is on, and one that finds nothing leaves the caret where
it was. `w` lands on the first character of the next word, rather than after the word it crossed
where the word keys under Ctrl land.

**`k`, `j` and `/` are the keys they spell rather than motions of their own.** `k` and `j` are Up and
Down: they walk the rows of a paragraph, then your prompt history, then the transcript, exactly as
the arrows do. `/` opens the search Ctrl-R opens, that being the only search here. While a key is
waiting for the character to jump to, every press is that character, so `f/` jumps to a slash and
`fj` to a `j`.

### Operators

`d` takes a stretch out, `c` takes it out and opens INSERT mode where it was, `y` keeps it and leaves
the line alone, and `>` and `<` move the line a step from or towards the margin. Each waits for the
stretch to act on:

| Keys | The stretch |
|---|---|
| a motion | from the caret to wherever that motion would take it |
| the operator's own letter doubled | the whole line |
| `D`, `C`, `x`, `s` | to the end of the line, and the character under the caret |
| `Y`, `S` | the whole line |

So `dw`, `cw` and `yw` are one idea rather than three bindings, and `d$` and `dG` work without being
listed. Whether the character a motion landed on is taken depends on the motion, as it does in vi:
`de` takes the word's last letter where `dw` stops before the next word's first, and `cw` on a
character that is not a blank behaves as `ce`, leaving the space after the word.

`p` and `P` put the register back after and before the caret, and a stretch that was whole lines comes
back as a line of its own. `J` makes this line and the one below into one, with a single space where
the newline was. `u` puts back what the last change took, **one step and no further**. `.` does the
last change again at the caret.

The register is vi's unnamed one and the only one. It is not the system clipboard, which Ctrl-V owns
and which you share with every other window you have open, so a yank here does not travel out of the
box.

### Text objects

After an operator, `i` and `a` say the stretch is a thing rather than a distance, and the next press
says which: `w` a word, `W` a run of anything that is not a blank, and a quote or either half of a
bracket pair for what lies between them. `i` takes what is inside and `a` takes what surrounds it
too. All on the line the caret is on.

`ci(` is what you mean when you want the arguments replaced, and it works with the caret on the name
in front of the bracket, where it usually is: a pair is the one enclosing the caret, or else the next
one along the line. Either half names it, so `di(` and `di)` are the same request.

`w` treats punctuation as a word of its own, so in `src/main.rs` the slashes belong to neither name.
`W` is the same thing with punctuation folded in, which is why `aW` takes the whole path.

### Marking a stretch out first

`v` marks a stretch out character-wise and `V` line-wise, and **the whole of it is drawn while you
choose it**, on every row it crosses. That is the point of having both this and an operator waiting
for a motion: the stretch is on the screen while it is being chosen, and the next key acts on it.

An operator here needs no extent: `x` is `d` and `s` is `c`, `r` replaces every selected character
with one, and `~`, `u` and `U` change the case. Motions move the end the caret is at, `o` puts the
caret at the other end, and a text object becomes the selection.

The key that opened the mode closes it, the other of the two changes which kind is in force, and
Escape abandons the selection. Every operator ends it, so nothing acts on a stretch that is no longer
drawn.

A selection is character-wise or line-wise and never a rectangle. A box ten rows tall holding one
prompt is not where somebody edits columns.

### These keys and a marker

A [marker](#markers) is one thing to every key above. A motion crosses it whole and leaves the caret
nowhere inside it. An operator takes it whole or not at all, and taking it takes the attachment off.
A selection holding one is not replaced character by character, so `r` over such a selection does
nothing: replacing the text either side and leaving the marker standing would be a line nobody could
read.

## Composing in your editor

**Ctrl-G** opens your editor on what is already in the box, and what you save replaces the line. It
is refused while a turn runs, because an editor needs the screen the turn is drawing on.

Every path that does not end in a save leaves the line untouched: quitting without saving, an editor
that failed, an editor that was killed. One trailing newline is dropped, one only. The file holds
your own words, is readable by nobody else, and does not outlive the edit.

Which editor opens: `$VISUAL`, then `$EDITOR`, then the first of `vim`, `vi`, `emacs`, `nano` that is
installed. A variable exported as empty counts as unset. An editor you named that will not start is
reported as such and nothing else is tried, so a fallback never runs an editor you did not ask for.

An editor is started under the name it was asked for, rather than under the file a symbolic link
behind that name points at. This matters for MacVim, which installs `vim`, `vi` and `gvim` as links
to a single program that reads the name it was called by: reached through the resolved path it is
always the detached GUI one, which returns at once and leaves the prompt unedited. The name is kept
only while it still reaches the same program. A link that now points elsewhere is started by resolved
path instead.

A GUI editor that would otherwise return the moment its window opens is told to wait, but only where
you wrote no arguments of your own.

## Looking up the keys

`?` on an empty line puts up every key and what it does. A second `?` takes the list down, as does
Escape, or typing anything at all. It is a mode rather than a character, the way `!` is: nothing
lands in the box, so there is nothing to delete afterwards.

Only on an empty line. A `?` part-way through a sentence is the punctuation you are asking a
question with, and in shell mode it is a glob for your shell to expand.

The list is not a completion. There is nothing in it to choose, so Tab and the arrows go on meaning
what they mean everywhere else while it is up. It folds into as many columns as the width holds, and
no row runs past the edge.

The row beneath the box carries what the session is doing (the mode in force where it is not just
asking, how full the context is, the trail, and the key that opens the delegates and the commands
once the session has anything to open), and then `? for shortcuts`. It names no other binding of its
own.

**The context reading says which of three things the session knows.** A session that has measured a
request says how full the context is, as a percentage of the budget it would be compacted at. One
whose conversation has been shortened underneath that figure says it was *compacted* rather than
giving a percentage, since the number it held describes an exchange that is no longer the one on
screen. A session that has measured nothing says nothing. A resumed session opens with what the last
request of the session it read came to, so the figure is there before this one has sent anything.

**A percentage against a budget nobody advertised is marked as approximate.** Against a window the
endpoint stated, a hundred per cent means shorten the conversation. Against the
[built-in default](../customize/configuration.md#context-budget), it may only mean that default is
too small for the model in force, and the answer is to set the budget rather than to compact.

The trail key is named only **once a turn has left a trail to look at**, since a trail is recorded
when the turn it belongs to ends. The confinement is not on the row at all: it is settled before the
session opens and cannot change while it runs. It is stated once at startup, and
[`/status`](../reference/commands.md#status) answers for it whenever you ask.

## Choosing how much the session asks

**Shift-Tab** cycles the session through asking about everything, accepting edits, plan mode, and
(only where the command line asked for it) bypassing every check. It types nothing, is read before
Tab so it never completes a half-typed line, and works while a turn runs. A turn in flight keeps the
mode it began with, so what you press describes the next one. Both spellings of the chord are
answered, since which one arrives is the terminal's choice rather than yours.

The mode leads the row beneath the box and is the only part of it drawn in a colour. Asking about
everything takes no room at all: what is drawn is a mode somebody chose. When the terminal is too
narrow, the parts are given up whole and in order (the way to the bindings, then the trail key, then
the figures), and the mode is the last to go.

See [modes](../security/permissions.md#answering-in-advance-modes) for what each one answers and what
choosing one costs.

## Stopping and leaving

Escape only ever stops, and never leaves. Ctrl-C is read against what is happening:

| What is happening | What Ctrl-C does |
|---|---|
| a turn in flight, or a command running | stops it, and the session stays where it was |
| nothing running, a line in the box | takes the line, and offers the way out |
| nothing running, an empty box | ends the session |

Taking the line says so, on the row beneath the box, and names the key that ends the session. That
offer lives for exactly one press.

Stopping is silent. A reply still arriving stops arriving, the prompt that was sent comes back to
the box for editing, and that is the whole of the answer. There is nothing to wait through. What
still finishes is a tool call already running, because stopping one part way could leave a file half
written.

The prompt stays in the transcript, marked stopped, rather than coming back to the box, when either
the turn had already done something visible or there are prompts waiting behind it. Both mean there
is an order to keep.

## Sending while a turn runs

**The only thing a running turn refuses is sending.** Typing, editing, pasting, dropping a file,
putting a line away, walking back through earlier prompts, scrolling the transcript, toggling the
audit trail, choosing how much the session asks and asking what the keys are all do exactly what
they do at rest.

Two things follow from that. Nothing is offered to **complete**, because what appears beneath the box
is machinery for finishing a line that is about to be sent; the key list is documentation you asked
for, and is drawn whether or not a turn is running. And **Ctrl-G** is refused, because handing the
terminal to an editor would take the screen from the turn drawing on it.

Enter mid-turn takes the line out of the box and holds it. It is drawn under the box, marked, so you
can see that what you sent went somewhere.

**The turn in flight takes it.** A turn asks between rounds, after the round's tool calls have run
and before the next request goes out. Everything waiting joins the conversation there, in the order
you typed it. So "no, the other file" reaches the planner while the work it is about is still
happening, instead of arriving after the thing it was meant to prevent. A prompt still waiting when
the turn ends becomes a turn of its own.

Between rounds and nowhere else: a round is a set of calls the planner asked for together, so
answering some and abandoning the rest would leave calls hanging, and mid-round the request is already
in flight. Stopping part way is what Escape and Ctrl-C are for.

**An interjection cannot change where effects may land.** Routing is settled by the prompt that began
the turn and stays that way, so what you type mid-turn reaches the planner as words to read, and every
effect it goes on to ask for is gated against the routing the turn started with. It is trusted, on the
footing of the prompt that opened the turn, since a keystroke has no author but the person at the
keyboard. The audit trail records it as your own input, so a turn that changed course halfway through
does not read as one that thought of it unprompted.

**What it carries is text.** Markers resolve to words when the line is sent, because a running turn
fixed the shape of its context before it read anything and has no trusted slot for a file to arrive
in. A dropped file becomes its name, which the planner can go and read through the gate it reads
anything else through, and a pasted picture says it cannot be shown. The box and the transcript keep
the marker, because that is what you are looking at.

A waiting prompt is not in the transcript. It moves there the moment the planner is given it, whether
that is inside the running turn or as a turn of its own. What it names is settled when it is queued, so
a file you took off the line afterwards was never part of it.

Stopping a turn leaves the queue alone: the next waiting prompt starts as it would after any turn,
and the rest go on waiting in order.

### Taking the queue back

**Up** puts everything waiting back into the box in one press, in the order you typed it, one to a
line. Nothing is waiting afterwards, so the rows under the box that said so go with it. A half-typed
line stays below them, where the caret is, and what each prompt named comes back staged with it. A
marker in a recalled line stands for the same file or picture it stood for when it went.

**Only what the planner has not been given.** A prompt the running turn has already taken is in the
conversation, so it cannot come back. Where the turn has taken every waiting prompt, the press leaves
the box exactly as it was.

They stay in the prompt history. From your side they were sent, and taking them back does not unsay
them.

With nothing waiting the key is unchanged: it walks the history, and scrolls once there is nothing
left to walk. Inside a paragraph it moves between rows first, and reaches the queue from the top
row the same way it reaches the history there.

## Searching the prompts you have sent

**Ctrl-R** opens a search over every prompt in your history, drawn over the transcript in place of the
box, newest at the bottom and seeded with whatever single line you had already typed.

| Key | What it does |
|---|---|
| letters | narrow the list; every word you type has to match somewhere in the prompt |
| Up / Down | walk the matches |
| Ctrl-S | swap between every prompt and the ones sent from this workspace |
| Enter | put the prompt under the cursor into the box |
| Escape, Ctrl-C, Backspace past the start | close the search and leave the box as it was |

Each row says how long ago that prompt was sent, and the one under the cursor is drawn in full beside
the list with a word for the lines that did not fit. A terminal too narrow for two columns keeps the
list, and a search matching nothing says so rather than showing an empty panel.

**Enter puts the prompt in the box rather than sending it.** A history file can be edited, on a shared
machine by somebody else, so the keystroke that sends a stored line is your own, after you have read
it. The search answers while a turn is running, on the same footing as the scroller, since searching
sends nothing.

## Putting a line away

**Ctrl-S** is read against the line rather than remembered. A line in the box is put away and the box
emptied. An empty box is where a line you put away earlier comes back, with the caret at its end,
where you carry on typing. There is one place to put a line, so a second one replaces the first, and
a line that has come back is no longer there to come back again: the next press on the empty box it
left has nothing to do, and says nothing.

Nothing is sent, so a running turn refuses none of it. What a line names is settled when it is sent
rather than when it is put away, so a file attached to a stashed line is still staged, and is named
again when the words holding it come back.

**The words travel and the mode does not.** What is put away is what you typed, and `!` is a mode
rather than a character, so it stays where you left it. A prompt comes back into an armed shell as
the command you are writing now, and a command comes back onto an ordinary prompt as words.

A line put away says so, on one row beneath the box (one row however long the line was), and names
the key that returns it. Where the width will not hold both, the words stay and the reminder goes:
which line is waiting is the part only that row can tell you, and the key is in the list `?` puts up
as well.

The caret is not carried back. It belongs to an edit that has finished, and restoring it would put
you back in the middle of a sentence you have not looked at since.

:::note
Ctrl-S is the byte a terminal traditionally freezes its output with. It reaches bravebot because the
session turns that flow control off for as long as it holds the terminal. Behind a `tmux`, `screen`
or `ssh` configured to keep flow control, the key can be taken before it arrives, and then it does
nothing here. Nothing is lost when that happens, because the line stays in the box.
:::

## The rows beneath the box

They run in one order, nearest the box first:

1. what the line in the box carries: attached files and pictures;
2. a line you have put away;
3. prompts waiting for the turn in flight;
4. what a half-typed line could still become: slash-command or `@path` completions.

## Markers

A folded paste, a pasted picture and a dropped file each fold to a marker in the line, like
`[Pasted text #2 +40 lines]` or `[Image #1]`. A marker behaves as one thing:

- Backspace or Delete takes the whole marker in one press, and deleting it takes the attachment off.
- Left or Right crosses it whole; the caret never rests inside one.
- The caret is drawn over every cell of the marker it is on, including the part that wrapped.

Square brackets you typed yourself are still deleted a character at a time.

See [Adding context](context.md) for what each kind of attachment does.

## Slash commands and `@path`

Typing `/` offers the commands, and typing `@` opens a picker over the workspace with directories
first: a prefix narrows it, a slash descends, Tab completes without disturbing the rest of the
sentence, and version-control and build directories are not offered. Sending a prompt that ends in a
half-typed reference completes it rather than sending the fragment.

See [Slash commands](../reference/commands.md) and [Adding context](context.md).

## Reading back

Ctrl-O opens the scroller over the transcript, and Ctrl-T toggles the audit trail. See
[Reading the transcript](transcript.md) and [The audit trail](../security/audit-trail.md).

## Watching a delegate, reading a command, and asking something aside

**Ctrl-L opens the whole of what a delegate did, the whole of what a command printed, and the answer
to anything you asked with [`/btw`](../reference/commands.md#btw-question).** Where there is more
than one row, a list is the way in: a panel over the transcript, a row each. Where there is one, its
own lines open directly. Where there is nothing the key does nothing. The row beneath the box names
the key and how many rows there are, counting them all together, for as long as the session has any.

| Key | What it does |
|---|---|
| Ctrl-L | open the list, and close the mode from either level |
| Enter | open the row under the cursor |
| Up / Down | move through the list |
| n / p | in a delegate, step to the next or previous one without going back to the list |
| q, Escape | back to the list from a delegate, then out |
| Ctrl-C | close the mode, leaving the turn in flight running |

In the transcript itself, each [delegate](../how-it-works.md#delegates) gets a **block of its own**,
where the call that started it happened, holding the last three things it did and a count of the
rest. The turn's own lines stay clear of it, and which block a line goes in is what the driver said
rather than what the line says, since several runs report at once and where a line arrived says
nothing about whose it is. A delegate that has finished collapses to the sentence the turn was told
and the report it answered with; one that could not finish answered nothing, so its block carries the
sentence alone. A report the planner was not allowed to read is drawn in the
[marked block](transcript.md) every quarantined result uses, so you can see which of the two it was.
What a command printed gets the same treatment: the transcript has room for the first lines and a
count.

**Every command the turn ran is a row**, after the delegates and in the order they ran, so a row's
place does not move under you while you are stepping through it. Opening one draws what the command
printed, as far back as is kept, and says so rather than dropping quietly where more was printed than
was kept. Where the planner was kept from the output, every row of it carries the margin every
quarantined block carries. The row is there whether or not the planner read what it printed, and
says which, that being the one thing about the bytes you cannot work out from them.

**A command's row says how the run ended**, with the same three marks a delegate's row carries: one
for a run whose every step exited zero, one for a run a step failed, and the mark of work still going
for a run stopped at [its deadline](../reference/tools.md#a-line-has-a-deadline). Opening
the row says it in words beside the command, naming the step that failed and its code. A stopped run
keeps the working mark because that is what it was doing: a server told to serve a page prints as it
goes and never exits, so a cross beside it would say something about the program that is not true.
The verdict comes from the exit codes and the clock, never from a line the program printed.

**A question asked with `/btw` is a row too**, before the delegates and the commands and in the order
they were asked. Opening one draws the question above the answer, and the row says whether the record
kept the answer. It comes first because it is the only row that survives a resume: put last, every
delegate a later turn spawned would be inserted above it and move its place.

The view opens on an aside the moment it is answered, which is the one thing that opens this mode
without your having pressed the key. You typed the question a moment ago, and an answer left behind a
key nobody told you about is not an answer. Leaving puts the turn's own view back where it was.

The header and the footer name which kind of thing you are looking at: a delegate by its kind and its
number, a command by the line that ran, an aside as an aside.

**The session is the first row of the list**, and choosing it closes the mode and puts the turn's
view back where you left it. Coming back to the list from a delegate puts the cursor on that delegate
rather than on the session, because a press of Enter should not turn into an exit.

The mode takes every key, so nothing you type reaches a box you cannot see. There is no way to talk
to a delegate: it was given one task, has nobody to ask, and takes no line typed mid-turn. What is on
the screen only moves when you ask. A delegate finishing leaves the view on it, and a delegate
starting does not take the screen from an older one you are reading.

A delegate keeps more of its work than its block draws, and drops its oldest once it has made several
hundred calls, so arriving late at a very long run means reading from wherever that bound has reached.

**None of this reaches a model.** The planner that asked is told the report and nothing else, no
delegate is part of the record a session is resumed from, and `/clear` forgets them and what the
commands printed alike, so a resumed session has the reports and none of the work behind them. You
own the directory and may see what your agent did in it. What must not happen is those lines reaching
a planner by any route, of which a record read back into a later turn would be one.

An aside is the one row written down, and it reaches no model either: the record keeps the question
and the answer, a resume puts them back into this view, and there is no path from the record into a
conversation.

## Long turns

**An interactive turn has no round limit.** You can see what it is doing and a stop reaches it
mid-round. A one-shot [`-p` run](headless.md) and a manifest run do carry one, since an unwatched
loop has nothing else to end it.

Where a bound applies, reaching it costs the turn its tools rather than ending it: the planner is
told it has none left and answers with what it has. This is a bound on futility rather than a
safety property. A gate refuses on the thousandth round what it refuses on the first.

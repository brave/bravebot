---
sidebar_position: 4
title: Reading the transcript
description: What is drawn back, the scroller Ctrl-O opens over it, and what the mouse does in it.
---

# Reading the transcript

## What is drawn

A reply is drawn as it arrives, and the round that ends replaces it. The end of a reply is visible
when it arrives, so scrolling back is always deliberate. A tiny terminal still renders.

**A scrollbar stands in the last column of a long transcript** while the mouse is captured. See
[the scrollbar](#the-scrollbar).

**A call the model is still writing is named beside the working indicator**, as "Preparing a call:
Write", by the word its own row will start with. The token count beside it moves as the call's
arguments arrive, so a long file being written reads as work and not as a stall. The name goes once
the call starts, the round ends, or the request has to be sent again.

**While the turn waits for a delegate's report, the working indicator reads "Waiting on d1"**, with the
delegate's number. A planner that has only delegates left ends its round with one line saying what is
outstanding, and that line stays in the transcript. The turn then waits and asks the planner again once
the report arrives.

**In the desktop window, the sidebar's Agents entry counts the agents the open conversation has
started.** While any is working, the entry also says how many. Choosing it opens a list with one row
for each agent: its kind, its number, what it was asked, and whether it is working, finished or did
not finish. Nothing an agent printed is in the list, and Done returns to the conversation. An agent
still working when its turn ends is listed as not finished.

A resumed session redraws what the earlier turns did: each one keeps the prompt you sent and how it
ended, failures and cancellations included, so reading a transcript back does not depend on
remembering the session. A reply that opened with the model's working leaves that block off when it
is redrawn, as it was left off the first time. A task list stays on the turn that made it, and a later turn without one
does not inherit it.

**Untrusted content is shown to you on purpose.** You are the one party allowed to read it, and the
point of quarantine is that the decision comes to you rather than to the model. It is drawn inside a
margin it cannot forge, and never drawn as structure, so untrusted bytes cannot paint themselves as a
heading, a prompt, or a message from the program.

The margin is on every drawn **row**, not every line of the content. A line wider than the box is
broken to the width by the same step that draws the margin, and each row it breaks into carries a bar
of its own.

**Colour codes in output are counted and not drawn.** A program that colours its output, such as a CI
log, writes a code before and after each coloured word. Drawn as pictures, those buried the words.
In the output of a [shell mode](shell-mode.md) command, in a quarantined block's preview, and in the
lines under a call the model read, the codes are left out and a row under the text says how many,
for example `14 colour codes not drawn`. Every other control character and escape sequence, such as
one that moves the cursor or erases a line, is still replaced by a visible picture. A prompt asking
you to approve text, a block's heading, and a file name draw every byte, colour codes included.

**What the session says in its own voice is drawn in an ink of its own**: the trust question, a
confinement that is unavailable, a status report. It is never the ink that marks untrusted content.
The exception is the notes `/add-dir` and `/add-dir close` write, and the notes about the
directories a settings file names: one that says a directory was opened or closed is drawn in the
ink for finished, and one that refuses a path or asks for one in the ink for failed, so a directory
that was added is not read as another error.

Colour is never what makes that marking hold. A colour can be imitated by the content beside it, so
quarantine stands on the margin instead: no ink tells you whether something is quarantined, and a
note drawn in the wrong one would still be outside a block.

Where a result went is drawn only where that is not the ordinary answer. A quarantined read says so
on its call, and an ordinary one does not clutter the transcript saying what always happens. The
marked block holding quarantined content does not repeat it: its heading says `untrusted` and the
margin marks every row. A block whose content is in no model's context at all, such as a
processor's remark, says so in its heading.

**Each call says why it was made.** In the terminal, and in a headless run's progress, the model's own
words are the call's headline and the call itself (its verb and what it is about) is drawn under it,
ahead of what came of it. A reason of several lines is cut to its first. A call given no reason is
headed by the call itself. The reason stays once the call is over, and a resumed session draws it
too. The desktop window draws the reason after the call on the call's own row.

A `run` call shows its command line, so a command that needs no prompt is still visible. A command
of several lines is shown as its first line and `...`. A command line that carries a credential you
declared, on any of its lines, is shown as the verb alone, because `run` refuses such a line before
anyone is shown it.

**A call to an [MCP server's](../customize/mcp-servers.md) tool that ran without a prompt** (an allow
rule, a standing answer or bypass mode) is drawn as `alias:tool` followed by its arguments as compact
JSON on one line. Arguments wider than a previewed line are cut with `...`, and a reference in them is
shown as the planner wrote it. Where any key or string value in them holds a credential you declared,
the arguments are left off. A resumed session draws the same row.

**A call the model read shows a few lines of what it found**, drawn under the call behind a plain
margin. A command shows its last five lines, because that is where a build or a test run says how it
went. A file, a listing or a search shows its first five. A line under them says how many were left
out. A quarantined result shows its marked block instead, never a plain glimpse.

**A call that ran a model inside itself says how long it waited there**, on its own row beside what
came of it. That is a confined check before quarantined content is shown to you, and a processor's own
round. Every other call waited on this machine and says nothing, and a wait too short to state is left
off.

**A fenced block in a reply is drawn as a block.** A reply that writes code between two lines of
three backticks (or tildes) draws a header row saying what the fence named, then each line as it was
written in the colour of inline code and two columns in. Nothing in it is read as markdown, so a `*`
or an `_` in a command stays a character, a `#` is not a heading and a number in it is not linked. A
block nobody closed runs to the end of the reply, as it does while the reply is still arriving. Once the
reply has ended, the header has the word `copy` at its right, which
[copies the block](#what-a-click-does). A block with nothing in it, or nothing but blank lines, has no
word.

**A message from another session is drawn apart from your own prompts.** A session that receives a
[message](../reference/tools.md#message_session) draws it as a quoted block marked as a message from
the sender, and the sender draws each message it sent with the target's name and the whole text. A
resumed session draws both again. Control characters in a name or in the text are replaced by visible
ones.

**A `#<number>` in a reply is a link to that issue or pull request** when the checkout you are in has
an `origin` remote on `github.com`. The number is underlined, and in a terminal that draws links it
opens `https://github.com/{owner}/{repository}/issues/{number}` with the owner and the repository of
the remote, which GitHub sends to the pull request where the number is one. `C#1`, `page#12`, a
number in backticks and a number in a quarantined block are left as text, and so is every number where
`origin` is not on `github.com`. Selecting a linked number copies the number. The desktop window draws
a number as text.

## The end of a turn

A finished turn gets a row of its own: which turn it was, what it cost, and how long it took. The row
lasts until the next turn starts.

This is how you tell a turn that ended from one that is hanging. A reply that asked for no tool ends
the turn, so one ending on `now let me look at the dispatch code` would otherwise leave a promise as
the last thing on screen.

**Succeeding, failing and being cancelled each get their own label**, so a turn you stopped does not
read as one that broke. A turn that failed or one you cancelled carries neither the cost nor the
time, and a failed one says why, below its row and again in its transcript entry: fixed wording,
plus whatever the model service reported and how many requests went out. The reason stays where you
can read it with the audit trail open or shut, after the terminal is resized, and while you are
reading older scrollback, and it wraps rather than being cut. A reason is drawn inert, so control
characters in it cannot paint anything. A turn that succeeds after a failure reports its own outcome
and takes none of the colour of the one before it.

**A turn that changed files and ran nothing says so.** Where a run was possible, files changed and no
program was run, the end of the turn tells you plainly that nothing was built or tested, whether it
answered, failed, or you stopped it: the files are changed either way. A run line refused for a
left-behind credential, and a line stopped with the turn, count as run; any other error does not.
Nothing else
on your screen distinguishes that diff from a checked one. It is not a reproach: plenty of turns have
nothing to build, and one of those says nothing. A file a delegate changed and a program a delegate
ran count as the turn's, except in a [checkout of its own](../customize/agents.md#a-checkout-of-its-own),
which holds none of the turn's changes.

The planner is asked the same question while the turn is still going, a set number of rounds after its
first write, and pointed at a [checker delegate](../reference/tools.md#spawn_agent) where the log
would be long and the answer is one sentence. While a delegate sharing the workspace is still out,
that question waits. Two audiences and two moments: the planner can still
act, and you are about to.

**A refusal the planner got past is not drawn under the turn.** If a gate refused a call and a later
call to the same tool in that turn was not refused, the first refusal is left to the
[audit trail](../security/audit-trail.md) and is not drawn. A refusal still standing at the end is
counted, and the first is named as `gate: reason`. The same tool counts as the same purpose, so a later
`run` of a different line also clears an earlier refused `run`. The audit trail holds every refusal
either way.

## Themes

`/theme` opens a picker on the palette the interface is painted in. Up and Down (or `k` and `j`)
move the cursor, Enter keeps the theme under it, and Escape puts back the one that was in force when
the picker opened. `/theme <name>` applies a theme without opening the panel at all.

The picker is a bordered panel in the middle of the screen rather than a full-screen list. Your
session stays visible behind it and is redrawn every time the cursor moves, so what you are previewing
is your own transcript. On a tiny terminal the panel shrinks to stay inside the frame.

Under the list is a row for whatever a theme does that its name does not say, which today is one
thing: whether its inks were picked for the background sensed at startup. `brave` has it filled, as
does every scheme published in both polarities and any theme of your own that gave a pair. The row is
drawn empty rather than dropped for the themes with nothing to add, so the list does not shift under
the cursor as it moves.

**A scheme published in both polarities is one row.** `catppuccin`, `gruvbox` and `solarized` are each
a single entry, painted from the half matching the background sensed at startup, so which of the two
your terminal wants is not a question you answer here and then answer again on the next machine.

The six fixed halves keep their names and still resolve through `/theme`, since sensing is a guess
wherever a terminal will not say: being guessed wrong about costs one `/theme solarized-light`, and it
stays fixed. They are left out of the list apart from the half already in force, which the picker
needs a row to open on. A scheme published in one polarity only is its own row, and so is a second
dark scheme from the same authors, like `catppuccin-macchiato`.

### What a theme decides

Where a colour is what tells one thing on the screen from another, `brave` uses a shade it mixes
itself rather than one of the sixteen named colours. A named colour is a slot your terminal repaints,
so it is a request rather than a colour: the same code draws something different in every profile. The
named slots are kept only where the meaning is your terminal's own and the slot is one schemes agree
about: green for finished, red for failed, yellow for a call still running. You read those against
whatever palette you chose rather than against each other. A mixed shade that has to stay legible
against the background is picked for the background sensed at startup, and a terminal that will not
say which it has is treated as dark. An aside is one of those: it would otherwise take bright black,
the slot terminals disagree about most, where in most published schemes it is too faint against its
own background to read. So are the inks that tell [shell mode](shell-mode.md) from an ordinary prompt
and a directory from a file, for the same reason.

A theme you choose by name paints every role from its own palette, including the background and the
default text. No named slots are used there, so two roles cannot collapse into one because your
terminal remapped green.

The palette never changes what a marking means. Quarantine stands on the margin, not on a colour, in
every theme.

:::note
The question about your background colour is asked once, before the first frame, and the answer is
read straight off the terminal. Anything typed or pasted into the window before it arrives is read by
that same question and discarded, and a terminal that answers with nothing holds the window open for
its full 80 milliseconds. It happens once a session, before there is a box to type into.
:::

See [Choosing a theme](../customize/theme.md) for where the choice is stored
and how to write one of your own.

## The model picker

[`/model`](../reference/commands.md#model) draws a bordered panel over the session with a search box
above the list. **Typing narrows the list rather than walking it**, matching without regard to case
anywhere in the name shown, in the name a request would carry, and in the service that answers, and
every word you type has to match something. A roster from a gateway runs to hundreds of models, so
arrowing to a row is not the way in.

The cursor stays on the model it was on for as long as that model still matches, whether you are
adding to the search or deleting from it, and falls to the first match only when what it was on stops
matching. A search matching nothing says so, and there is nothing to choose while it does.

**Every row is drawn under the service that will answer it**, one heading per service, in the order
the roster first mentions each. The models Brave's own endpoint serves are a service like any other. A
service the roster mentions in more than one place is still a single section, and is never given two
headings at once. Scrolling through a section holds its heading on the top line, so no row is ever on
screen without the name of what answers it: the same model name is often reachable through more than
one service, billed and credentialled differently, and which one answers is what you are choosing
between.

## When the environment declines colour or motion

Two environment variables are read before the first frame, and each answers to being set rather than
to what it is set to. `NO_COLOR=0` is somebody who set it.

| Variable | What it does |
|---|---|
| `NO_COLOR` | every role is drawn in your terminal's own ink, and none is added |
| `NO_MOTION` | the glyph beside a running turn stands still instead of cycling, and the face on the opening screen stops blinking and glancing |

`NO_COLOR` outranks the theme in force: the background, the mixed shades, the named slots, the
gradient across the wordmark and the pixel face beside it are all given up, and the terminal is not asked about its background
because nothing is drawn against a sensed one. A theme you chose stays recorded and paints again once
the variable is unset, so this is a switch rather than an edit.

Distinctions this interface makes in colour alone are lost, which is what was asked for. Nothing a
colour was carrying goes with them: **the margin down a block the planner may not read is a glyph on
every row**, drawn the same way as always. The row a cursor is on is kept too, in reverse video rather
than a fill, since a list you cannot see your place in cannot be walked.

Under `NO_MOTION` the glyph is still drawn, and the elapsed time and the token counts still change: a
figure that moves when the thing it measures moves is information rather than animation. A row with
nothing in it would be indistinguishable from a program that had hung.

## In Apple's Terminal

Apple's Terminal draws the block characters the wordmark and the face are made of from the font, and
leaves a gap between rows. Where `TERM_PROGRAM` is `Apple_Terminal`, the opening screen draws them from
cell backgrounds instead, so the letters and the face are solid. The terminal is also not asked about
Kitty graphics, which it would print as text at the top of the screen. Inside tmux the variable names
tmux, so neither applies there.

## Scrolling at rest

| Key | Where the view goes |
|---|---|
| the wheel | up and down, stopping at the ends |
| PageUp / PageDown | a screen at a time |
| Home / End | the start, or the latest |

All of these work while a turn is running, and the view does not jump to follow the turn.

While the view is held off the latest, the bottom line says so and counts the rows that are below it,
and names **Ctrl-End**, which returns to the latest. The cue goes when the view is back.

## The scroller

**Ctrl-O** opens the scroller on the view already on the screen. Opening moves nothing: the row you
were looking at when you pressed the key is the row under your eyes afterwards. The scroller draws
lines across the whole width of the screen, so a line that the info panel or the scrollbar made wrap
can take fewer rows there, and the words at the top of the view stay at the top. It is one view with
two sets of keys over it, not a second copy of the transcript.

While it is open the keys are the scroller's, and a key it does not take does nothing at all. A
character does not reach the input box, Enter sends nothing, and a paste or a dropped file does not
reach the line either. The line you were half-way through keeps its text, its caret and whatever is
attached to it, coming back exactly as it was when the scroller closes.

The transcript gets every row of the screen but the last, which is the footer. The input box goes,
the working indicator above it goes, and anything being offered beneath it goes; all of them come
back the moment you close it.

A turn goes on underneath while the scroller is open, and the view does not move to follow it. The
footer says how much has arrived below, and that a turn is still running; `G` reaches it. A queued
turn, a `/loop` tick, a watch, a goal round or an aside starting leaves the scroller where it is too.

### Moving

| Keys | Where the view goes |
|---|---|
| Up / Down, `k` / `j`, `y` / `e`, Ctrl-Y / Ctrl-E, Ctrl-P / Ctrl-N | one line back / on |
| Ctrl-U / Ctrl-D, `u` / `d` | half a screen back / on |
| Space or `f` / `b`, Ctrl-F / Ctrl-B | a whole screen on / back |
| `g` / `G`, `<` / `>`, Home / End | the first row / the last |
| `{` / `}` | the prompt before this one / the prompt after |
| the wheel | as it does at rest |

Both the `less` and the `vi` dialects are there. Ctrl-N is a line on, and `n` alone is the next
match. Each end is a stop rather than a count that keeps going, so a held key comes to rest
somewhere the next press can move away from.

`{` and `}` land on the row a turn begins at, which is a prompt you typed.

A count typed first goes that many times as far: `5j` moves five lines, `3` Ctrl-D three half
screens, `2b` two screens back, `3}` three prompts on and `3n` three matches on. It works on every
key in the first three rows of the table and on `{`, `}`, `n` and `N`, it stops at the ends as a
single press does, and it goes no higher than a thousand, as a count in the box does. The count is
drawn at the start of the footer until the key it is for arrives. Any other key drops it, the wheel
included, and Escape abandons it without closing anything.

### Searching

`/` searches what is drawn. The needle is typed at the foot of the screen, Enter runs it, and it is
matched as a **substring, character for character, never as a pattern**: case-insensitive while the
needle is all lower case, exact from the moment it holds a capital.

A needle wider than the footer row is cut from its start, so the end you are typing stays visible, and
the match count is drawn ahead of any of it, so the way out stays on the row.

Every match is highlighted where it already is, how many there are is drawn, and `n` and `N` walk
them and wrap at the ends. **Two matches on one row are two matches**: the count says so, and the
second is a press of its own, which moves the count on and leaves the view where it is.

What is searched is the text of the rows as they are drawn, so a match is always something you can
see. A search matches untrusted content too, and never lifts it out of its block, and the footer
never quotes what it matched.

### Leaving

`q`, Escape and Ctrl-O each close the scroller, and the view stays where it left it. Escape abandons
a count waiting for its key first, then clears a standing search, since each is the nearer thing to
stop; the press after that closes. The other ways out close it even while a needle is half typed.

The scroller can also be open when a command you queued behind a turn starts to run. The same four
keys close it there, and the press after that reaches the command.

Ctrl-C closes the scroller and does nothing else. A turn in flight goes on running, and the press
that reaches it is the next one. Each press answers the nearest thing there is to stop, and the
screen says which.

A footer stands for as long as the scroller is open, saying so and naming a way out. `?` lists every
key it takes, and names every one of the four that closes it. The list is read instead of the
transcript rather than beside it, so **any key at all puts it away** and that press is spent doing so,
which the list says. A terminal too short for the list loses rows from the middle, never the way out.

### Getting the text out

`x` expands the call nearest the top of the view to every line that was kept of its result, up to
500 lines per result, instead of the few lines the transcript shows by default. Pressing `x` again
collapses it. The extra lines are cleaned the same way the short view is: control characters are
replaced and each line is cut to one row.

`v` opens the transcript in your editor. What goes is the rows as they are drawn, margins and all,
so untrusted content is marked in the file exactly the way it is marked on the screen. The `copy` at
the right of a code block's header is for a click and is left out. It is written
to a temporary file **outside the workspace**, opened with `$VISUAL` or `$EDITOR` the same way a
prompt is, and the file goes when the editor exits. The key does nothing while a turn is running:
an editor needs the screen, and a running turn is drawing it.

Use it for anything past searching a screen: reading two passages side by side, keeping a copy,
grepping the lot.

Ctrl-L opens what the delegates did, as it does at the prompt: the list where there are several and
the one where there is one. From the scroller, the key that closes it returns to the scroller. It does
nothing where the session has started no delegate.

**Nothing comes back.** A transcript is a record of what happened, and a record you can edit back
into the session is not one. No later turn reads that file, and no path in your workspace gains
anything from its having existed.

## The mouse

The terminal interface reads the mouse while [`tui.mouse`](../customize/configuration.md#tuimouse)
is on, which it is unless you turn it off. Every click here is a shortcut, so the session is usable
without a mouse, and the right-hand column of each table gives the key or command that does the same.
Two clicks have none: the mark of a finished task opens words the transcript already holds whole, and
dragging the panel's border sets its width, and neither does anything else. The mouse never answers a
question: an approval, the question of whether to trust a directory, a question the agent asks and the
reading of a credential are answered with keys, and a click, a drag or the wheel anywhere on the
screen leaves each of them waiting.

### Selecting text

Press and drag to select, and let go to copy. The text goes to your clipboard through the platform's
clipboard tool where there is one, and to the terminal as an escape sequence otherwise. A press and a
release on one cell copy nothing. The row at the right of the hints under the box says how many
characters went, `12 chars to clipboard`, and which pane they came from when it was not the
transcript, `12 chars to clipboard, from the panel`. It goes at your next click, your next prompt or
Escape.

- **A selection belongs to the pane the press began in:** the transcript, the info panel, the prompt
  box, the list offered under the box, the status line or the footer. It stops at that pane's edge, so
  a drag down the transcript takes none of the panel's text on the same rows. A press between panes
  starts nothing and clears the selection you hold. The search over earlier prompts takes the mouse
  for itself, so a drag there selects nothing.
- **Double-click selects a word and triple-click a row of the screen:** a line the screen wrapped is
  taken a row at a time. A word is a run of letters, digits and
  underscores, a run of blanks, or a run of anything else. Dragging on from either grows the
  selection by words or rows. Shift-click moves the loose end of the selection you hold, in the pane
  it belongs to.
- **What is copied is what a person would paste.** A paragraph the screen wrapped is copied as one
  line, with a space where it was cut, in the transcript and in the scroller. The info panel copies its
  text without the border or the indent, and a session id drawn over two rows copies as one line. A
  character that takes two columns, as Chinese and Japanese ones do, is copied once.
- **A selection in the transcript stays on the words you put it on** while you scroll, also when they
  scroll out of view, and while a reply arrives below it. It is dropped when the rows would hold other words: when the transcript is drawn
  at another width, as opening or closing the info panel or the scrollbar appearing does, when you open
  the scroller or a delegate's view, when you expand or collapse a call with `x`, when you resize the
  terminal, and when what is above it changes, as when a call above it finishes and takes more rows.
- **A selection can be longer than the screen.** It copies in full, including the rows scrolled out of
  view. Hold the button below the transcript (on the status line, the box, the list under the box or
  the footer) or on or above its first row, and the transcript scrolls toward the pointer while the
  selection grows with it.
- **The words the interface draws for a click are not copied:** `copy` at the right of a code block's
  header and `‹ back` in a delegate's view are no part of the text around them. A search in the
  scroller does not find `copy` there either, only the words a reply says.
- **Escape puts a selection away.** It stays marked after the copy until your next click. Escape
  clears it before it does anything else, and the press after it does what Escape does there, so with
  a turn running the second press stops it.

Without the mouse, the terminal's own selection is the way (see
[giving the mouse back to the terminal](#giving-the-mouse-back-to-the-terminal)), `/copy` copies a
reply, `/copy block` a code block and `/copy text` a reply as drawn, and `v` in the scroller opens the
whole transcript in your editor.

### What a click does

A click is a left press and a release on one cell with nothing swept between them. A drag is a
selection, and so is a double click on text that is no target. A double click on a target acts once:
the second click selects nothing and copies nothing. A click beside a target does nothing. Each table
says what a click does and what does the same from the keyboard.

**In the transcript**

| Click | What it does | The same from the keyboard |
|---|---|---|
| the marker at the start of a reply | copies that reply, as `/copy` does. A reply still arriving has none to click until it ends | `/copy` |
| `copy` at the right of a code block's header | copies that block alone, as the reply wrote it | `/copy block`, or `/copy block 2` for the one before |
| a call that has more to show | at the prompt, including while a turn runs, opens the scroller on that call, expanded. In the scroller it expands the call, and again collapses it | Ctrl-O, then `x` |
| a delegate's block | opens that delegate's view, in the scroller, at the prompt and while a turn runs | Ctrl-L |
| `‹ back` in the header of a delegate's view, or the sentence that closes it | leaves the view and returns to what it was opened over | Ctrl-L |
| a prompt waiting behind a turn | takes it back into the box, with the prompts typed after it | Up, which takes them all |
| a web address in the assistant's words, with ctrl held | opens it in your browser, also while a turn runs | `/open 1`, or `/open` and the address |
| a file path in the assistant's words | shows the file at the end of the transcript, a moment later so a double click still selects the name | `/open 2`, or `/open` and the path |
| a file path in the assistant's words, with ctrl held | opens the file in your editor, at the line the path gave | `/open edit 2`, or `/open edit` and the path |

A web address is underlined. A click without ctrl on one only selects, so a click cannot take you away
from the session by accident. Terminals do not report the Cmd key, so the modifier is ctrl, and a
terminal that keeps ctrl-click for its own menu never sends it. Shift or alt held with ctrl changes
nothing. Only an `http` or `https` address with a plain host opens: one that names a user, as in
`https://name@example.test/`, or holds a backslash or a character outside printable ASCII is left as
text, and so is an address in what you typed, in a note, in a table, in a fenced block or in a
quarantined block. The browser is the one your system opens for an address, started as `open`,
`xdg-open` or `rundll32` is.

A path is a name with an extension, such as `src/main.rs`, `README.md:3` or
`crates/tui/src/app.rs:120:5`, in a reply or in a span of code. It is not underlined, since whether it
names a file is not known until it is opened. It has to name a file in the directory the session works
in or one added with `/add-dir`. A `deny` rule against reading it is kept for the preview, under any
name the file has, and not for the editor, since your editor is the one that reads it. The preview
shows the first lines, or the lines around `:12` when the path ends in a line number, numbered and
drawn behind the bar that marks untrusted content whatever you have told bravebot about trusting the
directory, and no model is given them. The editor is the one `$VISUAL` and `$EDITOR` name, and the line is passed to the ones bravebot
knows how to tell: vim and its relatives, nano, emacs, micro, kakoune, Visual Studio Code and its
forks, Sublime Text, Zed and Helix. Any other editor opens the file at the top. Both take the screen,
so neither is carried out while a turn or a command runs, in a delegate's view or under a picker. A
path that names no file in reach is told so, without repeating the name. The editor a click opens is
started without the credentials bravebot holds for itself in its environment, since a file the
workspace holds may have been written by a checkout.

`/open` is what stands for these three clicks. Alone it lists the web addresses and file paths of the
latest reply that has any, numbered, as they are drawn, and a number, an address or a name typed after it
opens one as the click does, with `edit` before it for the editor. While a turn runs an address opens
in the browser at once, and a file is not shown or opened: the command says so and you type it again
when nothing is running. [The command's page](../reference/commands.md#open-edit-n--address--path) has
the rest.

**In the info panel**

| Click | What it does | The same from the keyboard |
|---|---|---|
| the session's name | puts `/rename` and the name in the box, for you to edit and send. It leaves a line you have begun alone, and says so | `/rename` |
| the model row, the effort row | opens the `/model` or `/effort` picker | `/model`, `/effort` |
| the context reading | says what `/context` says | `/context` |
| a started MCP server | lists the servers, as `/mcp` does | `/mcp` |
| a started language server | says which are running, as `/lsp` does | `/lsp` |
| a task of the plan | moves the transcript to the turn that wrote it, or to its end while that turn runs | `{` and `}` in the scroller go by prompts, and `/` there searches for the words; no key puts the task's row at the top |
| the mark of a finished task | opens the whole of its words, and closes them when clicked again, as does a click on a row of the words below the first | none while the turn runs, when the mark is the one way to read the whole words; once it has ended the transcript holds them whole |
| the pull request or issue row | opens the address you gave `/pr` or `/issue` in your browser, with or without ctrl | `/open` and the address |
| the count under the Files heading | opens the files view, which is what `/diff` draws | `/diff` |
| a file in the list of the files view | highlights it and shows its edits from the top | Up and Down |
| the `x` at the right end of the files view's heading row | leaves the view | `q` or Escape |
| the previous or next control above the edits of the files view | steps between the edits | Left and Right, or `h` and `l` |
| the wheel over the files view | scrolls the list or the edits, whichever the pointer is over, by [`tui.wheelRows`](../customize/configuration.md#tuiwheelrows) rows | PgUp and PgDn scroll the edits, Up and Down move through the files |
| the panel's last row, which names the key that hides it | hides the panel, and the choice is kept for the next start | Ctrl-X, or `/panel` |
| the rest of the border, dragged | gives the panel more room or less | none: Ctrl-X hides it where the width does not suit |

While a turn runs, a click on a row that stands for a command does what the command typed does then:
the pickers open and `/context`, `/lsp` and `/diff` answer at once, and `/mcp`, which waits for the turn to
end, waits in the queue under the box. While a command you typed with `!` runs, every one of them waits in
the queue, and is carried out when the command has ended.

**Under the box**

| Click | What it does | The same from the keyboard |
|---|---|---|
| the permission mode | moves to the next mode, and never into bypassing every check, which only the key reaches | Shift-Tab |
| a hint that names a key: `ctrl-t show trail`, `? for shortcuts` (on an empty line), `ctrl-x info`, `ctrl-l to open`, `alt-l or /outputs to open`, `ctrl-b to background`, `ctrl-end to return`, `esc to cancel` in shell mode, `ctrl-v to paste` | does what the key does, and only where the key does | the key the hint names |
| the text of the prompt box | puts the caret there. Not in vi's style of box | the arrow keys |
| a row of the commands, skills or files offered as you type | the first click moves the cursor to it and the second puts it in the box without running anything | Up, Down and Tab |

Parts of the row that only report, such as the context reading and what a copy took, are not clicks.

**Pickers and lists**

| Click | What it does | The same from the keyboard |
|---|---|---|
| a row of the `/effort`, `/config`, `/theme` and `/model` pickers | the first click moves the cursor to it, and `/theme` shows the theme under it. The second takes it, when it is on the same row | Up, Down and Enter |
| a prompt in the search over earlier prompts | the first click moves the cursor to it and the second puts it in the box, sending nothing | Up, Down and Enter |
| a session in the lists `--resume`, `/resume` and `bravebot sessions` open | the first click moves the cursor to it and the second resumes it, or attaches to it | Up, Down and Enter |
| outside a picker or the search over earlier prompts | closes it, with nothing changed | Escape |
| outside the scroller's key list | puts it away | any key |
| the wheel over a picker, a list or the search | moves the cursor by [`tui.wheelRows`](../customize/configuration.md#tuiwheelrows) rows and stops at the ends | the arrow keys |
| the wheel in an approval whose body is longer than the screen | scrolls it three rows a turn, and answers nothing | the arrow keys |

While a picker is open every press and every turn of the wheel is the picker's wherever the pointer is,
so the wheel does not scroll the transcript behind it and a drag does not select what it covers. The
search over earlier prompts holds the mouse in the same way. A click on the scroller's key list is the
list's. None of these lets a click reach the row of the panel, the permission mode, the call or the text
of the box beneath it. The lists of sessions take the whole screen, so there is no outside to click, and
the question of deleting a record is answered by a key alone. A click on a row moves the cursor and
leaves the rows where they are, scrolled or not, so a double click in place takes the row you point at.
The wheel and the keys move the rows as they did, and a second click counts only on the row the first
one chose.

### The scrollbar

While the mouse is captured and the transcript is longer than the room for it, a scrollbar stands in
the last column of the transcript, in the plain view and not in the scroller, a delegate's view, the
search over earlier prompts or the opening screen. Its thumb shows where the view is in the whole,
and the lines wrap to the columns it leaves. Pressing in the column puts the middle of the thumb where
you pressed, and dragging the thumb scrolls with the pointer, from wherever in the window the pointer
goes: above or below the column is the first row or the last. It answers while a turn runs. It is not
part of the text, so a drag across it copies nothing from it. The wheel, the arrow keys and the page
keys do the same from the keyboard.

### Hover

Where your terminal reports the pointer moving, which most do, the thing under the pointer that a
click acts on is drawn underlined and in bold, so you can see what is clickable before you click. The
row where the hints are drawn says in one line what a click there does, in the room the hints leave
on it, with the address or the file name after it for a link: `click to show this file, ctrl-click
to open it in the editor  ·  src/main.rs:12`. A copy notice, a note or a message that fills the row has
the row to itself, and where the room is too short for the whole sentence none is drawn. Hovering is
not an action: moving the pointer selects nothing, clicks nothing and scrolls nothing, and the mark
goes when you press a key, leave the window, resize it or press a button. A terminal that does not
report the movement never shows it and loses nothing else.

### The right-click menu

A right click on a reply, on a delegate's block or inside a selection you hold opens a small menu at
the pointer. It stays inside the screen and wraps nothing.

| Where | Items | The same from the keyboard |
|---|---|---|
| a reply | `copy` | `/copy` |
| | `copy as text`: the reply as the screen shows it, without the marks of the markdown it was written in, the marker and the margin | `/copy text` |
| | `branch this session`: copies the whole session and carries on in the copy, waiting for the turn to end while one runs | `/branch` |
| a delegate's block | `open this delegate` | Ctrl-L |
| inside a selection you hold | `copy`, which copies the selection | the terminal's own selection |

Up and Down, or `k` and `j`, move the cursor, Enter takes the item, and Escape closes the menu with
nothing done and does not stop a running turn. A click on an item takes it, a click outside the menu
closes it, and the wheel moves the cursor. It never opens over a question, over a picker or the search
over earlier prompts, in the scroller's key list, while a command you typed runs, in the files view or
in a delegate's view. The menu has no key of its own because everything in it has one.
[`tui.contextMenu`](../customize/configuration.md#tuicontextmenu) set to `false` stops it opening,
for a terminal that keeps the right click for a menu of its own or reports a ctrl-click as one, as
macOS Terminal does.

### Giving the mouse back to the terminal

While the session has the mouse captured, a drag is the session's and the wheel scrolls the
transcript. Hold your terminal's bypass modifier, Shift on most terminals and Option on macOS iTerm2
and Terminal.app, and the terminal selects and copies what it shows instead.
[`tui.mouse`](../customize/configuration.md#tuimouse) set to `false` stops the session capturing the
mouse at all: there are then no clicks and no hover, the terminal's selection and wheel need no
modifier, and the keys do everything. The `?` list names the drag, the bypass modifier and the right
click while the mouse is captured, and none of them otherwise.

Behind tmux or GNU screen the multiplexer has to forward the mouse reports to the session, in the
form the session asked for, which [`tui.mouse`](../customize/configuration.md#tuimouse) says more
about. A click past column 223 or row 223 of a wide screen lands on the cell it was made on.

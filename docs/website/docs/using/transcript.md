---
sidebar_position: 4
title: Reading the transcript
description: What is drawn back, and the scroller Ctrl-O opens over it.
---

# Reading the transcript

## What is drawn

A reply is drawn as it arrives, and the round that ends replaces it. The end of a reply is visible
when it arrives, so scrolling back is always deliberate. A resumed session redraws what the earlier
turns did. A tiny terminal still renders.

**Untrusted content is shown to you on purpose.** You are the one party allowed to read it, and the
point of quarantine is that the decision comes to you rather than to the model. It is drawn inside a
margin it cannot forge, and never drawn as structure, so untrusted bytes cannot paint themselves as a
heading, a prompt, or a message from the program.

The margin is on every drawn **row**, not every line of the content. A line wider than the box is
broken to the width by the same step that draws the margin, and each row it breaks into carries a bar
of its own.

**What the session says in its own voice is drawn in an ink of its own**: the trust question, a
confinement that is unavailable, a status report. It is never the ink that marks untrusted content.

Colour is never what makes that marking hold. A colour can be imitated by the content beside it, so
quarantine stands on the margin instead: no ink tells you whether something is quarantined, and a
note drawn in the wrong one would still be outside a block.

Where a result went is drawn only where that is not the ordinary answer. A quarantined read says so,
and an ordinary one does not clutter the transcript saying what always happens.

## The end of a turn

A finished turn gets a row of its own: which turn it was, what it cost, and how long it took. A turn
that failed is reported as stopped, without a cost. The row lasts until the next turn starts.

This is how you tell a turn that ended from one that is hanging. A reply that asked for no tool ends
the turn, so one ending on `now let me look at the dispatch code` would otherwise leave a promise as
the last thing on screen.

**A turn that changed files and ran nothing says so.** Where a run was possible, files changed and no
program was run, the end of the turn tells you plainly that nothing was built or tested. Nothing else
on your screen distinguishes that diff from a checked one. It is not a reproach: plenty of turns have
nothing to build, and one of those says nothing.

The planner is asked the same question while the turn is still going, a set number of rounds after its
first write, and pointed at a [checker delegate](../reference/tools.md#spawn_agent) where the log
would be long and the answer is one sentence. Two audiences and two moments: the planner can still
act, and you are about to.

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
own background to read.

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

See [Configuration](../customize/configuration.md#choosing-a-theme) for where the choice is stored
and how to write one of your own.

## Scrolling at rest

| Key | Where the view goes |
|---|---|
| the wheel | up and down, stopping at the ends |
| PageUp / PageDown | a screen at a time |
| Home / End | the start, or the latest |

All of these work while a turn is running, and the view does not jump to follow the turn.

## The scroller

**Ctrl-O** opens the scroller on the view already on the screen. Opening moves nothing: the row you
were looking at when you pressed the key is the row under your eyes afterwards. It is one view with
two sets of keys over it, not a second copy of the transcript.

While it is open the keys are the scroller's. A character does not reach the input box, Enter sends
nothing, and the line you were half-way through keeps its text, its caret and whatever is attached to
it, coming back exactly as it was when the scroller closes.

The transcript gets every row of the screen but the last, which is the footer. The input box goes,
the working indicator above it goes, and anything being offered beneath it goes; all of them come
back the moment you close it.

A turn goes on underneath while the scroller is open, and the view does not move to follow it. The
footer says how much has arrived below, and that a turn is still running; `G` reaches it.

### Moving

| Keys | Where the view goes |
|---|---|
| Up / Down, `k` / `j` | one line back / on |
| Ctrl-U / Ctrl-D | half a screen back / on |
| Space / `b`, Ctrl-F / Ctrl-B | a whole screen on / back |
| `g` / `G`, Home / End | the first row / the last |
| `{` / `}` | the prompt before this one / the prompt after |
| the wheel | as it does at rest |

Both the `less` and the `vi` dialects are there. Each end is a stop rather than a count that keeps
going, so a held key comes to rest somewhere the next press can move away from.

`{` and `}` land on the row a turn begins at, which is a prompt you typed.

### Searching

`/` searches what is drawn. The needle is typed at the foot of the screen, Enter runs it, and it is
matched as a **substring, character for character, never as a pattern**: case-insensitive while the
needle is all lower case, exact from the moment it holds a capital. `n` and `N` walk the matches.

What is searched is the text of the rows as they are drawn, so a match is always something you can
see. A search matches untrusted content too, and never lifts it out of its block.

### Leaving

`q`, Escape and Ctrl-O each close the scroller, and the view stays where it left it. Escape clears a
standing search first, since that is the nearer thing to stop; the press after that closes.

Ctrl-C closes the scroller and does nothing else. A turn in flight goes on running, and the press
that reaches it is the next one. Each press answers the nearest thing there is to stop, and the
screen says which.

`?` says what the scroller takes, and the scroller says that it is open.

### Getting the text out

`v` opens the transcript in your editor. What goes is the rows as they are drawn, margins and all,
so untrusted content is marked in the file exactly the way it is marked on the screen. It is written
to a temporary file **outside the workspace**, opened with `$VISUAL` or `$EDITOR` the same way a
prompt is, and the file goes when the editor exits. The key does nothing while a turn is running:
an editor needs the screen, and a running turn is drawing it.

Use it for anything past searching a screen: reading two passages side by side, keeping a copy,
grepping the lot.

**Nothing comes back.** A transcript is a record of what happened, and a record you can edit back
into the session is not one. No later turn reads that file, and no path in your workspace gains
anything from its having existed.

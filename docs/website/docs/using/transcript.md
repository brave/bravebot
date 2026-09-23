---
sidebar_position: 4
title: Reading the transcript
description: What is drawn back, and the scroller Ctrl-O opens over it.
---

# Reading the transcript

## What is drawn

A reply is drawn as it arrives, and the round that ends replaces it. The end of a reply is visible
when it arrives, so scrolling back is always deliberate. A tiny terminal still renders.

A resumed session redraws what the earlier turns did: each one keeps the prompt you sent and how it
ended, failures and cancellations included, so reading a transcript back does not depend on
remembering the session. A task list stays on the turn that made it, and a later turn without one
does not inherit it.

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
answered, failed, or you stopped it: the files are changed either way. Nothing else
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

See [Configuration](../customize/configuration.md#choosing-a-theme) for where the choice is stored
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
| `NO_MOTION` | the glyph beside a running turn stands still instead of cycling |

`NO_COLOR` outranks the theme in force: the background, the mixed shades, the named slots and the
gradient across the wordmark are all given up, and the terminal is not asked about its background
because nothing is drawn against a sensed one. A theme you chose stays recorded and paints again once
the variable is unset, so this is a switch rather than an edit.

Distinctions this interface makes in colour alone are lost, which is what was asked for. Nothing a
colour was carrying goes with them: **the margin down a block the planner may not read is a glyph on
every row**, drawn the same way as always. The row a cursor is on is kept too, in reverse video rather
than a fill, since a list you cannot see your place in cannot be walked.

Under `NO_MOTION` the glyph is still drawn, and the elapsed time and the token counts still change: a
figure that moves when the thing it measures moves is information rather than animation. A row with
nothing in it would be indistinguishable from a program that had hung.

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

While it is open the keys are the scroller's, and a key it does not take does nothing at all. A
character does not reach the input box, Enter sends nothing, and a paste or a dropped file does not
reach the line either. The line you were half-way through keeps its text, its caret and whatever is
attached to it, coming back exactly as it was when the scroller closes.

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
needle is all lower case, exact from the moment it holds a capital.

Every match is highlighted where it already is, how many there are is drawn, and `n` and `N` walk
them and wrap at the ends. **Two matches on one row are two matches**: the count says so, and the
second is a press of its own, which moves the count on and leaves the view where it is.

What is searched is the text of the rows as they are drawn, so a match is always something you can
see. A search matches untrusted content too, and never lifts it out of its block, and the footer
never quotes what it matched.

### Leaving

`q`, Escape and Ctrl-O each close the scroller, and the view stays where it left it. Escape clears a
standing search first, since that is the nearer thing to stop; the press after that closes. The other
ways out close it even while a needle is half typed.

Ctrl-C closes the scroller and does nothing else. A turn in flight goes on running, and the press
that reaches it is the next one. Each press answers the nearest thing there is to stop, and the
screen says which.

A footer stands for as long as the scroller is open, saying so and naming a way out. `?` lists every
key it takes, and names every one of the four that closes it. The list is read instead of the
transcript rather than beside it, so **any key at all puts it away** and that press is spent doing so,
which the list says. A terminal too short for the list loses rows from the middle, never the way out.

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

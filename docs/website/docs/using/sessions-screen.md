---
sidebar_position: 12
title: The Sessions screen
description: One list of every session on the machine, with what each is doing, and the keys to open, copy, begin, stop, rename, archive and delete one.
---

# The Sessions screen

The Sessions screen lists every session on your machine in one place: the ones running in the
background, the ones open in other terminals, and the ones saved from earlier days, from every
project. It shows which are waiting on you, which are working, and which are not running, and it is
where you pick one back up, stop one, or tidy the list.

Open it by pressing **Left** with nothing typed in the box, the arrow that has nowhere to go over an
empty line. `/sessions` opens it too, and so do the commands that pick a session up again:
`bravebot --resume`, `bravebot --from-pr <number or url>` and `/resume` with no id. It opens at the
prompt and while a turn is running, and **Escape** or **Ctrl-C** puts it away. The transcript, the box
and a turn that is running are as you left them. From `bravebot --resume`, leaving it with Escape
leaves you in an ordinary new session.

## What is on the screen

The first line says how many sessions are waiting on you and how many are working, or that nothing
is waiting on you. Under it are five tabs with the number of sessions in each: **All**, **Needs you**,
**Working**, **Ready** and **Inactive**. **Tab** and **Shift-Tab** move between them, and the digits
**1** to **5** choose one.

| Tab | Holds |
|---|---|
| Needs you | a background session waiting on an answer, or one whose process ended in the middle of a turn |
| Working | a turn is running |
| Ready | a background session with no turn running, and a session open in another terminal |
| Inactive | a background session that was stopped, and a saved session nothing runs |

The list is one list. The sessions that need you come first, then those working, then those ready,
then those not running, each newest first, and the cursor starts on the first one that needs you. A
row has the session's name, the project it belongs to, its status and how long ago it was last
written. The project is the last two parts of its directory, and a screen narrower than 74 columns
leaves the column out.

Press **`g`** to group the list. It goes through four layouts and round: together, by project, by agent
and by model. Each heading carries the number of sessions under it. By project, the directory you are
in comes first under the heading **Here**, and the others keep the order in which they first appeared,
so a project does not jump about when one of its sessions is written. By agent, a heading names the
agent definition the sessions were started with (`--agent`), and the sessions started with none are
under **No agent**. By model, a heading names the model that last answered. The hint at the top right
says what `g` will do next.

Pinned sessions come first in either layout. Together, a star marks each. By project, they are a
group of their own under **Pinned**.

A list 100 columns wide also has the branch and the size of every session in columns. On a screen
too narrow for the details, a line under the list gives the branch, the size and the start of the id
of the session under the cursor. A manifest run, which can be read and not continued, is marked on its
row.

On a screen wide enough, the details of the session under the cursor stand at the right: its model,
the mode a background session runs in, the turns and tokens it has spent, its directory and branch,
its agent, the issue and pull request you linked, the session it was handed off from, its size, its
whole id and the last prompt you typed to it. A session that needs you says which kind of question it holds.
It does not show what the model said, so a name or a status on this screen is always something you
typed or a fact about a process.

The list is the 40 most recently written sessions. **Show more** at its end reads 40 older ones.

## Keys

| Key | Does |
|---|---|
| Up, Down, `k`, `j` | move the cursor through the sessions and the search box, and go round at the ends |
| Page Up, Page Down | move a screen, and stop at the ends |
| Home, End | go to the first or the last row |
| Enter, Right | open the session under the cursor |
| `1` to `5`, Tab, Shift-Tab | choose a tab |
| `n` | begin a session in the directory of the session under the cursor |
| `f` | copy the session's record and carry on in the copy |
| `r` | rename it |
| `x` | stop the background process running it |
| `a` | archive it: take it off the list until it is used again |
| Backspace | delete it, and everything its record kept |
| `h` | hide it from this list for as long as the session you are in lasts |
| `p` | pin it to the top, or take the pin off |
| `g` | group the list by project, or list it together again |
| `/` or F3 | move the cursor to the search box (below) |
| `?` | list these keys |

## Moving the keys

Thirteen of these keys can be moved with a `sessionsKeys` block in
[`settings.json`](../customize/configuration.md#sessionskeys): `up`, `down`, `new`, `fork`, `rename`,
`stop`, `archive`, `delete`, `hide`, `pin`, `group`, `search` and `help`.

```json
{ "sessionsKeys": { "stop": "ctrl-x", "archive": "z", "group": "f5" } }
```

A key is one character, alone or after `ctrl-` or `alt-`, or a function key from `f1` to `f12`. The
key an action was moved off does nothing, and the footer, the hint beside the title and the list
`?` shows name the keys in force. Enter, the arrows, Tab, Escape, the digits 1 to 5, Ctrl-P, Ctrl-N,
Ctrl-B, Ctrl-F, Ctrl-C and F3 are the screen's own and cannot be moved. A key that cannot be had, or
that another action also asked for, leaves the action where it was, and the screen says so when it
opens. In a search or a name being typed every character is text, whatever it is bound to.

## Searching

The search box is at the top of the list, always. Up from the first session moves into it, Down from
it is the first session, and Up from it is the last, so the box is one more stop in the list. With the
cursor in the box what you type is the search, Enter opens the first match, and Escape clears the text
and puts the cursor back in the list; with the box empty, Escape leaves the screen. `bravebot --resume`,
`--from-pr` and `/resume` open the screen with the cursor in the box, so typing searches at once;
Left and `/sessions` open it with the cursor on a session, where the letters are keys, and `/` or F3
moves the cursor to the box. The search finds text anywhere in a session's name, last prompt, directory, branch, agent
and id, in the issue and pull request you linked, and in what was said in the session: the prompts you
typed, the replies and the calls that were made. A session found by what was said shows the first line
that holds the text under its row. Pasting a pull request address finds the session you gave it. A
`since:7d` word, in hours (`h`), days (`d`) or weeks (`w`), keeps only the sessions written within that
time. A search reads every session, however old, and not only the 40 the list opens with. The records
are read in the background, so the screen keeps taking keys: sessions found by name show at once, and
those found by what was said arrive as their records are read.

`bravebot --from-pr <number or url>` opens the screen already narrowed to the sessions linked to that
pull request. It says nothing matches where there are none.

## Archiving and deleting

Archive and delete ask first, and only `y` answers. A letter or a digit typed into a search or a
rename is part of the line, not a key of the list.

## Opening a session

A session of the directory this terminal is in is carried on right here, as `/resume` does. A session
of another directory is carried on by `bravebot --resume` started in that directory, and a session a
background process is running is joined as `bravebot attach` joins it. A session open in another
terminal cannot be opened, and a manifest run cannot be copied.

While a turn is running the Sessions screen can be looked at, and sessions can be stopped, renamed,
archived and deleted as at rest. Opening a session, copying one and beginning one end the session you
are in, so they ask first: a question over the list names the session and says the turn that is
running will be stopped, as Escape stops it, and the session saved as it stands. **`y`** stops the
turn and goes on to open, copy or begin; **`n`**, **Escape** and **Ctrl-C** withdraw the question and
leave the turn running, and no other key answers it. The session you are already in is opened by
leaving the screen, with no question. A manifest run, a session open in another terminal and a
directory that is gone are refused at once, before the turn is touched.

A session open in this terminal, or in another one, is never archived or deleted from here.

## The mouse

Click a tab to choose it. Click a row to put the cursor on it, and click it again to open it. The
wheel moves the cursor.

## `bravebot sessions`

`bravebot sessions` in a terminal opens a smaller list of the sessions running in the background,
made from the same listing as this screen. It says the same of each session: the same name, the same
status in the same words (`Ready` where `bravebot sessions --json` and a line piped to a script say
`idle`), and the same order, with the sessions waiting on you first. Enter attaches.

## What is kept

Whether the list is grouped by project, the sessions you pinned, and the sessions you archived are kept in three
small files in the [state directory](../customize/configuration.md). A session you archive comes back
to the list by itself the next time it is used. An [incognito](./incognito.md) session writes none of
them.

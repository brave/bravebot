---
sidebar_position: 2
title: Adding context
description: Naming files with @path and --file, pasting text and pictures, and dropping files onto the terminal.
---

# Adding context

There are four ways to put something in front of the agent yourself: name a file, paste, drop a file
on the window, or pipe it in.

## Naming a file: `@path`

Type `@` and a picker opens over the workspace. Directories come first, a prefix narrows the list, a
slash descends into a directory, and Tab completes without disturbing the rest of the sentence.
Version-control and build directories are not offered.

```
> why does @crates/cli/src/main.rs read stdin before checking the prompt?
```

A named file's contents enter the turn as **trusted** input, and the rule recorded is for that file
alone. So:

- `@vendor/lib.js` is trusted even inside a `vendor` directory you marked untrusted, and the rest of
  that directory stays exactly as it was;
- the rule outlives the read, so the file can be edited afterwards;
- naming a file works even in a directory you declined at startup.

`..` and absolute paths are refused rather than resolved, so a named file is always inside the
working directory or a directory opened with `/add-dir`.

A directory cannot be named. Only files can. Neither can prose: an address inside a sentence is not a
reference, and a bare `@` names nothing.

Sending a prompt that ends in a half-typed reference completes it rather than sending the fragment.

:::caution
Content you have not read is content you are vouching for. Be as careful naming a file as answering
yes to a directory: the planner will act on what it says.
:::

## `--file` on the command line

```sh
bravebot "explain this" --file notes.md --file src/main.rs
```

`--file` does exactly what `@path` does, is repeatable, and is trusted for the same reason: you
named it.

## Pasting

Ctrl-V pastes. More than a couple of lines folds to a marker:

```
[Pasted text #2 +40 lines]
```

The words around it are left alone, and the words themselves are put back where the line leaves the
box, so the request, the transcript and your prompt history all hold the paste rather than the
marker. Deleting the marker drops the words. A short paste lands whole, a paste into a command line
is never folded, and a paste ending in a newline does not send.

**The marker goes no further than the box.** It is a handle on text that only the session holding it
can put back. A prompt that comes back for editing after a stop does come back behind its marker,
since that is where a stack trace is worth folding away.

### Pictures

Ctrl-V also pastes a picture: a screenshot, or an image copied from a browser. Command-V does not:
the byte stream over a pty has no encoding for that modifier, so the terminal writes the clipboard's
*text* into the pty instead and the picture silently arrives as nothing. Ctrl-V comes through as a
byte, so Brave Bot goes around the terminal and reads the clipboard itself. On macOS that goes
through `osascript`; on Linux it needs `wl-paste` or `xclip`.

- A picture wins over text when the clipboard holds both, since copying an image in a browser leaves
  the page's URL behind as text, and text has another key.
- The picture is inlined into the request, never linked, so no other machine fetches it.
- The marker is written where the caret is, and the picture goes wherever that text goes. Deleting
  the marker unsends it.
- **A prompt recalled from your history carries no picture and names none.** No durable text stands
  for a screenshot. What this turn sends is untouched: the picture still travels with the prompt that
  named it, and the transcript still shows the line as it was on your screen.
- A picture is refused in shell mode rather than written into the command.
- Anything over 10 MB is refused, and says so with its size.
- A pasted picture is kept with the session record and comes back on resume, because it is part of
  your own message.

## Dropping a file

Drag a file onto the terminal window and it attaches, with its own marker: `[Image #1]`, numbered so
a second drop is distinguishable from the first.

A drop grants two things, for that one file:

- **trust**: its contents can be read and it can be edited for the rest of the session, even inside
  a directory marked untrusted;
- **reach**: a dropped file, and only a dropped file, may name a path outside the working
  directory. Nothing else in the directory it came from becomes trusted or reachable.

What happens depends on the type:

| Dropped | Result |
|---|---|
| an image or a PDF | carried as bytes, so the model looks at it |
| a text file | its contents enter the turn as trusted input |
| anything else | its path is written into the line, as dropping a file always did |

Extensions are recognised whatever their case. Dropping a directory attaches nothing.

**A prompt recalled from your history names the file rather than the marker.** Nothing staged beside
a line outlives the session that staged it. The name is enough: the planner reads it and goes to the
file through the same gate it reads any other file through.

Terminals deliver a drop as text, so it has to be told from typing: a line is a drop only when every
word of it is a path that exists. A plain, quoted, backslash-escaped or `file://` path counts,
several at once count, and a percent sign in a name survives. One word of prose, a path naming
nothing, an unterminated quote or more than one line makes it a paste instead.

:::caution
A screenshot somebody sent you is content you have not read and are vouching for. It goes into the
turn as trusted input on the strength of the gesture alone.
:::

## Piping

```sh
gh pr diff | bravebot -p "which of these changes needs a test?"
```

Piped input is **untrusted and private, always**. It is quarantined and the planner is given a
reference, never the bytes. A pipe has no path, so there is nothing for the trust map to have an
opinion about, and `gh pr diff` and `cat build-error.txt` arrive by exactly the same route.

Input over 10 MiB is refused rather than truncated, and says to write it to a file and name that
instead. See [Non-interactive use](headless.md).

## What each route is worth

| Route | Label | Reach |
|---|---|---|
| `@path`, `--file` | trusted | inside the workspace |
| a dropped file | trusted | anywhere on disk, for that file |
| a pasted picture or text | trusted, as your own message | none |
| `!` shell mode output | trusted, private | your shell's own access |
| a pipe into `-p` | **untrusted**, private | none |
| a file read in a trusted directory | trusted | the workspace |
| a file read anywhere else | **untrusted**, private | quarantined |

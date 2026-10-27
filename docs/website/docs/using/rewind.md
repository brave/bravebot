---
sidebar_position: 11
title: Taking turns back
description: Put the session back to before a turn with /undo, read a rewind before running it with /rewind, and put back one hunk with /hunks.
---

# Taking turns back

[`/undo`](../reference/commands.md#undo) puts the session back where it stood before the most recent
turn: the files that turn wrote go back to what they held, a file it created is removed, and the
conversation, turn count, spend, timing and remembered command approvals go back with them. The prompt
of the earliest turn taken back returns to the input box, as a prompt, when the box is empty and no
turn is running.
**Saying it again goes back another turn**, as far as the last five.

File trust follows what undo restored or left on disk. Restored files get the lower of their backup's
trust and their trust before the turn; untouched files get the lower of their current trust and their
trust before the turn. Grants made during undone turns are withdrawn. A file you distrusted during
those turns stays distrusted if undo leaves it untouched; you can grant trust again. One failed
restoration leaves that path untrusted without withdrawing unrelated grants.

## Reading a rewind before running one

A task continued with `bravebot -p "task" --continue` (or `--resume <id>`) adds a rewind point of its
own behind the record's earlier ones. `/undo` in the session you resume next takes back only that
turn, files and conversation, and warns about a program the turn ran or a language server the
session started, since neither can be put back.

`/rewind` on its own lists the points the session can go back to, most recent first and numbered
from one. Each row says how many turns back it is, which turn it would land before, what that turn
was asked, and every path that turn wrote over, or that it wrote over none.

`/rewind <n>` then goes back that many turns, which is what saying `/undo` n times does, so it puts
back every row from the first down to the one chosen. A number past what the session remembers
rewinds nothing and says how far back it does go, rather than going as far as it can: reading a tree
two turns younger than you believe it is would be worse than a refusal.

## Putting back one hunk

`/hunks` lists the hunks the last turn wrote, numbered from one across its files, and `/hunks 2` puts
hunk 2 back as it stood before the turn while every other line of the file stays as it is. The list
is drawn again afterwards, since the numbers below the hunk have moved, and `/undo` still puts the
whole turn back.

It is for the case where most of a turn is right and one change is not. A file you have edited since
is protected: the file is compared with what the turn kept, and a hunk that has moved or changed is
refused rather than written over. Only a file the session trusts is compared, so any other is named
and left to `/undo`. The [command reference](../reference/commands.md#hunks-number) has the rest.

## What will not go back

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

## What gives up every point

These changes outside a turn give up every point: `/clear`, `/compact`, `/btw`, `/rename`,
`/add-dir`, and `/cd`. After one of these, `/undo` says there is nothing left to undo rather than
rewinding to a point that describes a different session. Shell-mode commands keep the points and
add a coverage warning.

A rewind past the first turn removes the record only when restoration is complete, there are no
coverage warnings, and file trust matches the original snapshot. Otherwise the resulting state
stays saved. A name you gave the session before that turn stays with it.

## A rewind survives a resume

The points are kept with the record, so `/undo` and `/rewind` after a resume reach the same turns
they reached before the program was closed. One path is the exception: what a file **nobody vouched
for** held is kept only in memory, never written down, so after a resume that path is reported as one
that did not go back. The file is left alone rather than deleted, and the turn is undone in every
other respect.
